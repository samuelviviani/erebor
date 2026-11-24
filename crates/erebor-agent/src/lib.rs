//! Erebor agent: the one process that owns the mining engine (and so the miner child
//! processes) for a data folder. The CLI, the TUI and the desktop app talk to it over
//! the local socket (`erebor_core::ipc`).
//!
//! Never starts mining by itself, except with `startOnLaunch` set explicitly in the
//! configuration (and, for real mining, after the fees were accepted), or to resume
//! after restarting on a new version.
//!
//! Self-update (release builds of the `erebor` binary, Unix): checks the signed GitHub
//! releases every hour and installs a newer version. Whenever its executable is
//! replaced (by itself, `erebor update` or a reinstall) it restarts in place on the new
//! one, so service managers keep tracking the same process.

use erebor_core::engine::{self, Engine};
use erebor_core::ipc;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// Present while restarting on a new version with mining active: resume it.
const RESUME_FILE: &str = "resume-mining";

/// Run until SIGINT/SIGTERM or `erebor agent stop`, then stop mining and exit.
pub fn run(data_dir: PathBuf) -> Result<(), String> {
    let (tx, rx) = mpsc::channel();
    let t = tx.clone();
    ctrlc::set_handler(move || {
        let _ = t.send(());
    })
    .map_err(|e| format!("signal handler: {e}"))?;
    let restart = Arc::new(AtomicBool::new(false));
    #[cfg(unix)]
    let exe = self_update::spawn(&data_dir, tx.clone(), restart.clone());
    run_until(data_dir, tx, rx)?;
    #[cfg(unix)]
    if restart.load(Ordering::SeqCst) {
        if let Some(exe) = exe {
            return Err(self_update::exec(&exe));
        }
    }
    let _ = restart;
    Ok(())
}

#[cfg(unix)]
mod self_update {
    use super::*;
    use std::path::Path;
    use std::time::{Duration, Instant, SystemTime};

    const CHECK_EVERY: Duration = Duration::from_secs(3600);
    const FIRST_CHECK: Duration = Duration::from_secs(60);

    fn mtime(p: &Path) -> Option<SystemTime> {
        std::fs::metadata(p).and_then(|m| m.modified()).ok()
    }

    /// Watch the executable and check for releases. Returns the executable it watches,
    /// `None` when self-update does not apply (debug build, `erebor-agent` binary).
    pub fn spawn(dir: &Path, tx: Sender<()>, restart: Arc<AtomicBool>) -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        if cfg!(debug_assertions) || exe.file_name()? != "erebor" {
            return None;
        }
        let (dir, path) = (dir.to_path_buf(), exe.clone());
        std::thread::spawn(move || {
            let start = mtime(&path);
            let mut next_check = Instant::now() + FIRST_CHECK;
            loop {
                std::thread::sleep(Duration::from_secs(10));
                if Instant::now() >= next_check {
                    next_check = Instant::now() + CHECK_EVERY;
                    match erebor_core::update::update(&path) {
                        Ok(Some(v)) => println!("updated to {v}, restarting"),
                        Ok(None) => {}
                        Err(e) => eprintln!("update check failed: {e}"),
                    }
                }
                if mtime(&path) != start {
                    if ipc::snapshot(&dir).is_ok_and(|s| s.is_active()) {
                        let _ = std::fs::write(dir.join(RESUME_FILE), b"");
                    }
                    restart.store(true, Ordering::SeqCst);
                    let _ = tx.send(());
                    return;
                }
            }
        });
        Some(exe)
    }

    /// Replace this process with the new executable, same arguments (and pid).
    pub fn exec(exe: &Path) -> String {
        use std::os::unix::process::CommandExt;
        println!("erebor-agent restarting on the new version");
        let e = std::process::Command::new(exe)
            .args(std::env::args_os().skip(1))
            .exec();
        format!("restart failed: {e}")
    }
}

/// `run` with the stop channel supplied by the caller (tests).
fn run_until(data_dir: PathBuf, tx: Sender<()>, rx: Receiver<()>) -> Result<(), String> {
    // The data-folder lock makes a second agent (or an open desktop app) fail here,
    // before it can touch the miner: one engine, one miner, per computer.
    let engine = Engine::new(data_dir.clone(), engine::default_factory()).map_err(|e| {
        if e == "engine_busy" {
            "engine_busy: another Erebor (agent or desktop app) already owns this data folder"
                .to_string()
        } else {
            e
        }
    })?;
    // After a restart on a new version: pick up where mining was.
    let resume = std::fs::remove_file(data_dir.join(RESUME_FILE)).is_ok();
    let start = engine.config().start_on_launch || resume;
    let engine = Arc::new(Mutex::new(engine));
    engine::spawn_loops(engine.clone());
    let _ipc = ipc::serve(ipc::Host::new(engine.clone(), "agent", Some(tx)))?;
    println!(
        "erebor-agent {} ready · pid {} · socket {}",
        env!("CARGO_PKG_VERSION"),
        std::process::id(),
        ipc::socket_path(&data_dir).display()
    );
    if start {
        match engine
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .start(false)
        {
            Ok(()) if resume => println!("mining resumed after the update"),
            Ok(()) => println!("mining started (startOnLaunch)"),
            Err(e) => eprintln!("mining not started: {e}"),
        }
    }
    let _ = rx.recv();
    engine
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .stop("agent_exit");
    println!("erebor-agent stopped");
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::time::{Duration, Instant};

    fn wait_for(dir: &std::path::Path) {
        let t = Instant::now();
        while ipc::call(dir, "agent_info", &Value::Null).is_err() {
            assert!(t.elapsed() < Duration::from_secs(10), "agent did not start");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn single_instance_demo_lifecycle_and_clean_shutdown() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().to_path_buf();
        // Demo mode: simulated miner, no process, no network.
        std::fs::write(dir.join("config.json"), r#"{"demoMode":true}"#).unwrap();
        let (tx, rx) = mpsc::channel();
        let t = std::thread::spawn({
            let dir = dir.clone();
            move || run_until(dir, tx, rx)
        });
        wait_for(&dir);

        // Duplicate instance refused.
        let (tx2, rx2) = mpsc::channel();
        let err = run_until(dir.clone(), tx2, rx2).unwrap_err();
        assert!(err.starts_with("engine_busy"), "{err}");

        let o = ipc::control(&dir, ipc::Action::Start).unwrap();
        assert_eq!(o.snapshot.state, "running");

        ipc::call(&dir, "shutdown", &Value::Null).unwrap();
        t.join().unwrap().unwrap();
        assert_eq!(
            ipc::call(&dir, "agent_info", &Value::Null),
            Err(json!("agent_unreachable"))
        );
        assert!(!ipc::socket_path(&dir).exists());
    }

    #[test]
    fn resumes_mining_after_restarting_on_a_new_version() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().to_path_buf();
        std::fs::write(dir.join("config.json"), r#"{"demoMode":true}"#).unwrap();
        std::fs::write(dir.join(RESUME_FILE), b"").unwrap();
        let (tx, rx) = mpsc::channel();
        let t = std::thread::spawn({
            let dir = dir.clone();
            move || run_until(dir, tx, rx)
        });
        wait_for(&dir);
        assert_eq!(ipc::snapshot(&dir).unwrap().state, "running");
        assert!(!dir.join(RESUME_FILE).exists(), "resumed once, not forever");
        ipc::call(&dir, "shutdown", &Value::Null).unwrap();
        t.join().unwrap().unwrap();
    }
}

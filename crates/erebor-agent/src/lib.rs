//! Erebor agent: the one process that owns the mining engine (and so the miner child
//! processes) for a data folder. The CLI, the TUI and the desktop app talk to it over
//! the local socket (`erebor_core::ipc`); remote devices over the token-protected HTTP
//! API, only when the user enabled remote access.
//!
//! Never starts mining by itself, except with `startOnLaunch` set explicitly in the
//! configuration (and, for real mining, after the fees were accepted).

use erebor_core::engine::{self, Engine};
use erebor_core::{ipc, server};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// Run until SIGINT/SIGTERM or `erebor agent stop`, then stop mining and exit.
pub fn run(data_dir: PathBuf) -> Result<(), String> {
    let (tx, rx) = mpsc::channel();
    let t = tx.clone();
    ctrlc::set_handler(move || {
        let _ = t.send(());
    })
    .map_err(|e| format!("signal handler: {e}"))?;
    run_until(data_dir, tx, rx)
}

/// `run` with the stop channel supplied by the caller (tests).
pub fn run_until(data_dir: PathBuf, tx: Sender<()>, rx: Receiver<()>) -> Result<(), String> {
    // The data-folder lock makes a second agent (or an open desktop app) fail here,
    // before it can touch the miner: one engine, one miner, per computer.
    let engine = Engine::new(data_dir.clone(), engine::default_factory()).map_err(|e| {
        if e == "engine_busy" {
            "engine_busy: another Erebor (agent, desktop app or node) already owns this data folder"
                .to_string()
        } else {
            e
        }
    })?;
    let remote = engine.config().remote.clone();
    let start = engine.config().start_on_launch;
    let engine = Arc::new(Mutex::new(engine));
    engine::spawn_loops(engine.clone());
    let _ipc = ipc::serve(ipc::Host::new(engine.clone(), "agent", Some(tx)))?;
    let _http = if remote.enabled {
        let started = server::token(&data_dir)
            .and_then(|t| server::start(engine.clone(), remote.port, t, Arc::new(|_: &str| None)));
        match started {
            Ok(s) => {
                println!("remote access on port {} (enabled in settings)", s.port);
                Some(s)
            }
            Err(e) => {
                eprintln!("remote access not started: {e}");
                None
            }
        }
    } else {
        None
    };
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
}

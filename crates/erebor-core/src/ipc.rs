//! Local control channel between the process that owns the engine (agent, desktop app
//! or node) and the CLI/TUI on the same computer.
//!
//! Wire format (protocol v1): one JSON line per connection each way.
//!   request  `{"v":1,"cmd":"get_status","args":{...}}`
//!   reply    `{"ok":<value>}` or `{"err":<error code>}`
//! Commands are the ones of `rpc::call` (same as the GUI), plus `agent_info` and
//! `shutdown`, which only make sense locally.
//!
//! Transport: a Unix domain socket `agent.sock` in the data folder, mode 0600 in a
//! 0700 folder, and every connection's peer UID must equal ours.
//! Windows: a loopback TCP port (127.0.0.1 only) written with a random token to
//! `agent.port` in the data folder, which only this Windows user can read (%APPDATA%);
//! a client sends the token as its first line. Same request/reply lines after that.

use crate::fleet::Snapshot;
use crate::rpc::{self, Shared};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub v: u32,
    pub cmd: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Reply {
    Ok(Value),
    Err(Value),
}

impl From<Result<Value, Value>> for Reply {
    fn from(r: Result<Value, Value>) -> Self {
        match r {
            Ok(v) => Reply::Ok(v),
            Err(e) => Reply::Err(e),
        }
    }
}

/// `<data dir>/agent.sock`; for very long data-folder paths (a socket path holds
/// ~104 bytes) a name derived from the folder in the per-user runtime/temp folder.
pub fn socket_path(data_dir: &Path) -> PathBuf {
    if cfg!(windows) {
        return data_dir.join("agent.port");
    }
    let p = data_dir.join("agent.sock");
    if p.as_os_str().len() < 100 {
        return p;
    }
    use sha2::{Digest, Sha256};
    let h: String = Sha256::digest(data_dir.as_os_str().as_encoded_bytes())[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("erebor-{h}.sock"))
}

/// The process serving the socket.
pub struct Host {
    pub engine: Shared,
    /// "agent", "desktop" or "node".
    pub owner: &'static str,
    /// Set only by the agent: lets `erebor agent stop` end the process.
    pub shutdown: Option<Sender<()>>,
    pub started: i64,
}

impl Host {
    pub fn new(engine: Shared, owner: &'static str, shutdown: Option<Sender<()>>) -> Self {
        Self {
            engine,
            owner,
            shutdown,
            started: crate::now_unix(),
        }
    }
}

/// Transport-independent: one request line in, one reply out.
pub fn handle(host: &Host, line: &str) -> Reply {
    let Ok(req) = serde_json::from_str::<Request>(line) else {
        return Reply::Err("bad_request".into());
    };
    // Answered for any version, so clients can report a mismatch.
    if req.cmd == "agent_info" {
        let dir = host
            .engine
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .data_dir()
            .to_path_buf();
        return Reply::Ok(json!({
            "owner": host.owner,
            "pid": std::process::id(),
            "version": env!("CARGO_PKG_VERSION"),
            "protocol": PROTOCOL_VERSION,
            "startedAt": host.started,
            "dataDir": dir,
        }));
    }
    if req.v != PROTOCOL_VERSION {
        return Reply::Err("protocol_version".into());
    }
    match req.cmd.as_str() {
        "shutdown" => match &host.shutdown {
            Some(tx) => {
                let _ = tx.send(());
                Reply::Ok(Value::Null)
            }
            None => Reply::Err("not_agent".into()),
        },
        cmd => rpc::call(&host.engine, cmd, &req.args).into(),
    }
}

#[cfg(unix)]
pub use unix::{call, serve, Server};

#[cfg(unix)]
mod unix {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::sync::Arc;
    use std::time::Duration;

    const MAX_REQUEST: u64 = 1 << 20;

    /// A listening socket; dropping it removes the socket file.
    pub struct Server {
        path: PathBuf,
    }

    impl Drop for Server {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    #[cfg(target_os = "linux")]
    fn peer_uid(s: &UnixStream) -> Option<u32> {
        use std::os::fd::AsRawFd;
        let mut c = libc::ucred {
            pid: 0,
            uid: 0,
            gid: 0,
        };
        let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        // SAFETY: valid fd, and `c`/`len` describe a writable ucred.
        let r = unsafe {
            libc::getsockopt(
                s.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&mut c as *mut libc::ucred).cast(),
                &mut len,
            )
        };
        (r == 0).then_some(c.uid)
    }

    #[cfg(not(target_os = "linux"))]
    fn peer_uid(s: &UnixStream) -> Option<u32> {
        use std::os::fd::AsRawFd;
        let (mut uid, mut gid) = (0, 0);
        // SAFETY: valid fd and out-pointers.
        let r = unsafe { libc::getpeereid(s.as_raw_fd(), &mut uid, &mut gid) };
        (r == 0).then_some(uid)
    }

    fn my_uid() -> u32 {
        // SAFETY: no arguments, cannot fail.
        unsafe { libc::geteuid() }
    }

    /// Serve the engine on `<data dir>/agent.sock`. Call only while holding the engine
    /// (its data-folder lock), so any socket file left there is stale.
    pub fn serve(host: Host) -> Result<Server, String> {
        let dir = host
            .engine
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .data_dir()
            .to_path_buf();
        let path = socket_path(&dir);
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).map_err(|e| format!("socket: {e}"))?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
        let host = Arc::new(host);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let host = host.clone();
                // ponytail: one thread per request, fine for a CLI and a TUI.
                std::thread::spawn(move || serve_one(&host, stream));
            }
        });
        Ok(Server { path })
    }

    fn serve_one(host: &Host, s: UnixStream) {
        // Same user only: the socket controls mining and the wallet configuration.
        if peer_uid(&s) != Some(my_uid()) {
            return;
        }
        let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
        let mut line = String::new();
        if BufReader::new((&s).take(MAX_REQUEST))
            .read_line(&mut line)
            .is_err()
        {
            return;
        }
        let mut out = serde_json::to_vec(&handle(host, &line)).unwrap_or_default();
        out.push(b'\n');
        let _ = (&s).write_all(&out);
    }

    /// Run `cmd` on the engine owner for `data_dir`.
    pub fn call(data_dir: &Path, cmd: &str, args: &Value) -> Result<Value, Value> {
        let mut s = UnixStream::connect(socket_path(data_dir))
            .map_err(|_| Value::from("agent_unreachable"))?;
        // Both ends check: we only talk to an engine run by our own user.
        if peer_uid(&s) != Some(my_uid()) {
            return Err("agent_untrusted".into());
        }
        // Some commands reach the network (pool checks): allow them time.
        let _ = s.set_read_timeout(Some(Duration::from_secs(60)));
        let req = Request {
            v: PROTOCOL_VERSION,
            cmd: cmd.into(),
            args: args.clone(),
        };
        let mut buf = serde_json::to_vec(&req).map_err(|e| Value::from(e.to_string()))?;
        buf.push(b'\n');
        s.write_all(&buf)
            .map_err(|_| Value::from("agent_unreachable"))?;
        let mut line = String::new();
        BufReader::new(s)
            .read_line(&mut line)
            .map_err(|_| Value::from("agent_bad_response"))?;
        match serde_json::from_str::<Reply>(&line) {
            Ok(Reply::Ok(v)) => Ok(v),
            Ok(Reply::Err(e)) => Err(e),
            Err(_) => Err("agent_bad_response".into()),
        }
    }
}

#[cfg(windows)]
pub use tcp::{call, serve, Server};

/// Windows transport (compiled everywhere in tests, so it is tested on every platform).
#[cfg(any(windows, test))]
mod tcp {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{Ipv4Addr, TcpListener, TcpStream};
    use std::sync::Arc;
    use std::time::Duration;

    const MAX_REQUEST: u64 = 1 << 20;

    /// A listening port; dropping it removes `agent.port`.
    pub struct Server {
        path: PathBuf,
    }

    impl Drop for Server {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    pub(super) fn port_file(data_dir: &Path) -> PathBuf {
        data_dir.join("agent.port")
    }

    fn same(a: &str, b: &str) -> bool {
        a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0, |x, (p, q)| x | (p ^ q)) == 0
    }

    /// Serve the engine on 127.0.0.1 (a free port). Call only while holding the engine.
    pub fn serve(host: Host) -> Result<Server, String> {
        let dir = crate::engine::lock(&host.engine).data_dir().to_path_buf();
        let listener =
            TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| format!("socket: {e}"))?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let token: String = (0..32)
            .map(|_| format!("{:02x}", rand::random::<u8>()))
            .collect();
        let path = port_file(&dir);
        std::fs::write(&path, format!("{port} {token}\n")).map_err(|e| e.to_string())?;
        let host = Arc::new(host);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (host, token) = (host.clone(), token.clone());
                std::thread::spawn(move || serve_one(&host, &token, stream));
            }
        });
        Ok(Server { path })
    }

    fn serve_one(host: &Host, token: &str, s: TcpStream) {
        let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
        let mut r = BufReader::new((&s).take(MAX_REQUEST));
        let (mut auth, mut line) = (String::new(), String::new());
        // Only someone who can read our data folder knows the token.
        if r.read_line(&mut auth).is_err() || !same(auth.trim(), token) {
            return;
        }
        if r.read_line(&mut line).is_err() {
            return;
        }
        let mut out = serde_json::to_vec(&handle(host, &line)).unwrap_or_default();
        out.push(b'\n');
        let _ = (&s).write_all(&out);
    }

    /// Run `cmd` on the engine owner for `data_dir`.
    pub fn call(data_dir: &Path, cmd: &str, args: &Value) -> Result<Value, Value> {
        let unreachable = || Value::from("agent_unreachable");
        let info = std::fs::read_to_string(port_file(data_dir)).map_err(|_| unreachable())?;
        let (port, token) = info.trim().split_once(' ').ok_or_else(unreachable)?;
        let port: u16 = port.parse().map_err(|_| unreachable())?;
        let mut s = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).map_err(|_| unreachable())?;
        let _ = s.set_read_timeout(Some(Duration::from_secs(60)));
        let req = Request {
            v: PROTOCOL_VERSION,
            cmd: cmd.into(),
            args: args.clone(),
        };
        let mut buf = format!("{token}\n").into_bytes();
        buf.extend(serde_json::to_vec(&req).map_err(|e| Value::from(e.to_string()))?);
        buf.push(b'\n');
        s.write_all(&buf).map_err(|_| unreachable())?;
        let mut line = String::new();
        BufReader::new(s)
            .read_line(&mut line)
            .map_err(|_| Value::from("agent_bad_response"))?;
        match serde_json::from_str::<Reply>(&line) {
            Ok(Reply::Ok(v)) => Ok(v),
            Ok(Reply::Err(e)) => Err(e),
            Err(_) if line.is_empty() => Err(unreachable()),
            Err(_) => Err("agent_bad_response".into()),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::engine::{self, Engine};
        use std::sync::Mutex;

        #[test]
        fn loopback_transport_needs_the_token() {
            let d = tempfile::tempdir().unwrap();
            std::fs::write(d.path().join("config.json"), r#"{"demoMode":true}"#).unwrap();
            let e = Engine::new(d.path().into(), engine::default_factory()).unwrap();
            let s = serve(Host::new(Arc::new(Mutex::new(e)), "agent", None)).unwrap();
            assert_eq!(
                call(d.path(), "agent_info", &Value::Null).unwrap()["owner"],
                "agent"
            );
            // A wrong token gets no answer.
            let info = std::fs::read_to_string(port_file(d.path())).unwrap();
            let port = info.split(' ').next().unwrap();
            std::fs::write(port_file(d.path()), format!("{port} wrong\n")).unwrap();
            assert_eq!(
                call(d.path(), "agent_info", &Value::Null),
                Err("agent_unreachable".into())
            );
            drop(s);
            assert!(!port_file(d.path()).exists());
        }
    }
}

// ---------- client helpers shared by the CLI and the TUI ----------

pub fn snapshot(data_dir: &Path) -> Result<Snapshot, Value> {
    let v = call(data_dir, "snapshot", &Value::Null)?;
    serde_json::from_value(v).map_err(|_| "agent_bad_response".into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Start,
    Stop,
    Pause,
    Resume,
    Restart,
}

/// What a mining command did, checked against the engine state afterwards.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// False when the miner already was in the requested state.
    pub changed: bool,
    pub snapshot: Snapshot,
}

/// Run a mining command and verify the state actually changed: success is reported
/// only when the engine ends up in the expected state.
pub fn control(data_dir: &Path, action: Action) -> Result<Outcome, Value> {
    control_via(&|cmd, args| call(data_dir, cmd, args), action)
}

/// `control` over any transport (the local socket, the engine in-process, a remote agent).
pub fn control_via(
    call: &dyn Fn(&str, &Value) -> Result<Value, Value>,
    action: Action,
) -> Result<Outcome, Value> {
    let snapshot = |_: ()| -> Result<Snapshot, Value> {
        serde_json::from_value(call("snapshot", &Value::Null)?)
            .map_err(|_| "agent_bad_response".into())
    };
    let before = snapshot(())?;
    let same = |snapshot| {
        Ok(Outcome {
            changed: false,
            snapshot,
        })
    };
    let (cmd, expect) = match action {
        Action::Start if before.is_active() => return same(before),
        Action::Start => ("start_mining", "running"),
        Action::Stop if before.state == "stopped" && before.restart_in_secs.is_none() => {
            return same(before)
        }
        Action::Stop => ("stop_mining", "stopped"),
        Action::Pause if before.state == "paused" => return same(before),
        Action::Pause if before.state != "running" => return Err("not_running".into()),
        Action::Pause if !before.miner.as_ref().is_some_and(|m| m.supports_pause) => {
            return Err("pause_unsupported".into())
        }
        Action::Pause => ("pause_mining", "paused"),
        Action::Resume if before.state == "running" => return same(before),
        Action::Resume if before.state != "paused" => return Err("not_paused".into()),
        Action::Resume => ("resume_mining", "running"),
        Action::Restart => {
            if before.is_active() {
                call("stop_mining", &Value::Null)?;
            }
            ("start_mining", "running")
        }
    };
    call(cmd, &Value::Null)?;
    let after = snapshot(())?;
    if after.state != expect {
        return Err(after
            .error
            .unwrap_or_else(|| "state_unchanged".into())
            .into());
    }
    Ok(Outcome {
        changed: true,
        snapshot: after,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::engine::{self, Engine};
    use std::sync::{mpsc, Arc, Mutex};

    fn demo_host(dir: &Path, shutdown: Option<Sender<()>>) -> (Shared, Server) {
        let e = Engine::new(dir.into(), engine::default_factory()).unwrap();
        let e: Shared = Arc::new(Mutex::new(e));
        let mut c = e.lock().unwrap().config().clone();
        c.demo_mode = true;
        e.lock().unwrap().update_config(c).unwrap();
        let s = serve(Host::new(e.clone(), "agent", shutdown)).unwrap();
        (e, s)
    }

    #[test]
    fn socket_controls_the_shared_engine_and_verifies_state() {
        let d = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        let (engine, server) = demo_host(d.path(), Some(tx));
        let p = d.path();
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(socket_path(p))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);

        let info = call(p, "agent_info", &Value::Null).unwrap();
        assert_eq!(info["owner"], "agent");
        assert_eq!(info["protocol"], PROTOCOL_VERSION);
        assert_eq!(call(p, "nope", &Value::Null), Err(json!("unknown_command")));

        // Start/pause/resume/stop through the socket, each verified.
        let o = control(p, Action::Start).unwrap();
        assert!(o.changed && o.snapshot.state == "running" && o.snapshot.demo);
        assert!(
            !control(p, Action::Start).unwrap().changed,
            "no second miner"
        );
        assert_eq!(control(p, Action::Pause).unwrap().snapshot.state, "paused");
        assert_eq!(
            control(p, Action::Resume).unwrap().snapshot.state,
            "running"
        );
        assert_eq!(
            control(p, Action::Restart).unwrap().snapshot.state,
            "running"
        );

        // A stop from another frontend (the GUI's in-process call) is seen here too.
        engine.lock().unwrap().stop("user");
        assert_eq!(snapshot(p).unwrap().state, "stopped");
        assert!(!control(p, Action::Stop).unwrap().changed);
        assert_eq!(control(p, Action::Pause), Err(json!("not_running")));
        assert_eq!(control(p, Action::Resume), Err(json!("not_paused")));

        call(p, "shutdown", &Value::Null).unwrap();
        rx.recv().unwrap();
        drop(server);
        assert_eq!(
            call(p, "agent_info", &Value::Null),
            Err(json!("agent_unreachable"))
        );
    }

    #[test]
    fn protocol_errors_and_refusals() {
        let d = tempfile::tempdir().unwrap();
        let (engine, _server) = demo_host(d.path(), None);
        let host = Host::new(engine, "desktop", None);
        assert_eq!(handle(&host, "garbage"), Reply::Err("bad_request".into()));
        assert_eq!(
            handle(&host, r#"{"v":99,"cmd":"get_config"}"#),
            Reply::Err("protocol_version".into())
        );
        let Reply::Ok(info) = handle(&host, r#"{"v":99,"cmd":"agent_info"}"#) else {
            panic!("agent_info answers any version")
        };
        assert_eq!(info["owner"], "desktop");
        // Only the agent can be shut down through the socket, not the desktop app.
        assert_eq!(
            handle(&host, r#"{"v":1,"cmd":"shutdown"}"#),
            Reply::Err("not_agent".into())
        );
        // Real mining without a wallet is refused by the engine itself.
        let mut c = host.engine.lock().unwrap().config().clone();
        c.demo_mode = false;
        host.engine.lock().unwrap().update_config(c).unwrap();
        assert_eq!(
            control(d.path(), Action::Start),
            Err(json!("wallet_missing"))
        );
    }

    #[test]
    fn long_data_dir_paths_get_a_short_socket() {
        let d = tempfile::tempdir().unwrap();
        let long = d.path().join("x".repeat(60)).join("y".repeat(60));
        std::fs::create_dir_all(&long).unwrap();
        let sock = socket_path(&long);
        assert!(sock.as_os_str().len() < 104 && !sock.starts_with(&long));
        let (_e, server) = demo_host(&long, None);
        assert_eq!(
            call(&long, "agent_info", &Value::Null).unwrap()["owner"],
            "agent"
        );
        drop(server);
        assert!(!sock.exists());
    }

    #[test]
    fn unreachable_agent() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(snapshot(d.path()), Err(json!("agent_unreachable")));
        // A stale socket file (crashed agent) is not a live agent.
        std::fs::write(socket_path(d.path()), "").unwrap();
        assert_eq!(
            call(d.path(), "agent_info", &Value::Null),
            Err(json!("agent_unreachable"))
        );
    }
}

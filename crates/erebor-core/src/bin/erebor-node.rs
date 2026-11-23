//! Erebor node: the mining engine without a window, controlled from the Erebor app on
//! another computer or from a phone's browser.
//!
//!   erebor-node [--data-dir DIR] [--port N] [--web DIR]
//!   erebor-node install-service [same options]   # start at login, restart if it stops
//!   erebor-node uninstall-service

use erebor_core::autostart::{self, Kind, NODE_LABEL};
use erebor_core::engine::{self, Engine};
use erebor_core::server;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const USAGE: &str = "usage: erebor-node [install-service|uninstall-service] [--data-dir DIR] [--port N] [--web DIR]";

fn main() {
    if let Err(e) = run() {
        eprintln!("erebor-node: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut action = None;
    let mut data_dir = erebor_core::default_data_dir();
    let mut port: Option<u16> = None;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // Default web UI: `web/` next to the binary (copy of apps/desktop/dist).
    let mut web = exe.parent().map(|d| d.join("web"));
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = || {
            it.next()
                .cloned()
                .ok_or(format!("{a} needs a value\n{USAGE}"))
        };
        match a.as_str() {
            "install-service" | "uninstall-service" => action = Some(a.clone()),
            "--data-dir" => data_dir = PathBuf::from(val()?),
            "--port" => port = Some(val()?.parse().map_err(|_| "invalid port")?),
            "--web" => web = Some(PathBuf::from(val()?)),
            _ => return Err(USAGE.into()),
        }
    }

    match action.as_deref() {
        Some("install-service") => {
            // The service runs this same binary with the same options, by absolute path.
            let mut svc = vec!["--data-dir".into(), abs(&data_dir)];
            if let Some(p) = port {
                svc.extend(["--port".into(), p.to_string()]);
            }
            if let Some(w) = web.as_ref().filter(|w| w.exists()) {
                svc.extend(["--web".into(), abs(w)]);
            }
            autostart::install(NODE_LABEL, &exe, &svc, Kind::Service)?;
            println!("Erebor node installed as a background service ({NODE_LABEL}).");
            return Ok(());
        }
        Some(_) => {
            autostart::uninstall(NODE_LABEL, Kind::Service)?;
            println!("Erebor node service removed.");
            return Ok(());
        }
        None => {}
    }

    let engine = Engine::new(data_dir.clone(), engine::default_factory()).map_err(|e| {
        if e == "engine_busy" {
            "another Erebor (app or node) is already using this data folder".into()
        } else {
            e
        }
    })?;
    let port = port.unwrap_or(engine.config().remote.port);
    let start_mining = engine.config().start_on_launch;
    let engine = Arc::new(Mutex::new(engine));
    engine::spawn_loops(engine.clone());
    if start_mining {
        if let Err(e) = engine.lock().unwrap().start(false) {
            eprintln!("erebor-node: mining not started: {e}");
        }
    }
    // Local CLI/TUI (`erebor status`, `erebor monitor`) talk to the node over its socket.
    let _ipc = erebor_core::ipc::serve(erebor_core::ipc::Host::new(engine.clone(), "node", None))
        .map_err(|e| eprintln!("erebor-node: local control socket unavailable: {e}"))
        .ok();
    let token = server::token(&data_dir)?;
    let assets = match web.filter(|w| w.join("index.html").exists()) {
        Some(w) => server::dir_assets(w),
        None => {
            eprintln!(
                "erebor-node: no web interface found (--web DIR); the Erebor app can still connect"
            );
            Arc::new(|_: &str| None)
        }
    };
    let _server = server::start(engine, port, token.clone(), assets)?;
    let host = server::lan_ip().unwrap_or_else(|| "127.0.0.1".into());
    println!("Erebor node on http://{host}:{port}");
    println!("Access token: {token}");
    println!("Phone/browser link: http://{host}:{port}/#token={token}");
    // ponytail: no signal handling; a killed node leaves the miner running until the
    // next start, which terminates it (orphan pid file).
    loop {
        std::thread::park();
    }
}

fn abs(p: &std::path::Path) -> String {
    std::fs::canonicalize(p)
        .unwrap_or_else(|_| p.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

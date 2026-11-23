// Release builds on Windows: no extra console window (the GUI shows mining status).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Tauri shell. Every GUI command goes through `call` to `erebor_core::rpc`, the same
//! dispatcher the remote server uses. No shell commands, no arbitrary process
//! execution: the only executables ever started are the miners installed from the
//! pinned manifest or explicitly selected by the user (and, on request, the OS tools
//! that register Erebor to start at login).

use erebor_core::autostart::{self, Kind, APP_LABEL};
use erebor_core::engine::{self, Engine};
use erebor_core::ipc;
use erebor_core::rpc::{self, Shared};
use erebor_core::server::{self, Assets};
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{Manager, State};

/// Where "this computer" lives: our own engine, or an agent/node already running here
/// (it holds the data folder lock). The agent is reached over its local socket, the
/// node over loopback HTTP. Either way there is one engine and one source of state.
enum Backend {
    Local(Shared),
    Agent(PathBuf),
    Node { url: String, token: String },
}

struct AppState {
    backend: Backend,
    data_dir: PathBuf,
    assets: Assets,
    server: Mutex<Option<server::Running>>,
    server_error: Mutex<Option<String>>,
    /// While we own the engine, `erebor` (CLI/TUI) controls it through this socket.
    _ipc: Option<ipc::Server>,
}

type R<T> = Result<T, Value>;

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> R<T> + Send + 'static) -> R<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| Value::String(e.to_string()))?
}

impl AppState {
    /// Start or stop the remote access server to match the configuration.
    fn sync_server(&self) {
        let Backend::Local(e) = &self.backend else {
            return;
        };
        let remote = e
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .config()
            .remote
            .clone();
        let mut srv = self.server.lock().unwrap();
        if !remote.enabled || srv.as_ref().is_some_and(|s| s.port != remote.port) {
            *srv = None;
        }
        if remote.enabled && srv.is_none() {
            let started = server::token(&self.data_dir)
                .and_then(|t| server::start(e.clone(), remote.port, t, self.assets.clone()));
            *self.server_error.lock().unwrap() = started.as_ref().err().cloned();
            *srv = started.ok();
        }
    }
}

#[tauri::command]
async fn call(s: State<'_, Arc<AppState>>, cmd: String, args: Option<Value>) -> R<Value> {
    let st = s.inner().clone();
    blocking(move || {
        let args = args.unwrap_or(Value::Null);
        let out = match &st.backend {
            Backend::Local(e) => rpc::call(e, &cmd, &args, false),
            Backend::Agent(dir) => ipc::call(dir, &cmd, &args),
            Backend::Node { url, token } => server::remote_call(url, token, &cmd, &args),
        };
        if out.is_ok() && matches!(cmd.as_str(), "update_config" | "delete_all_data") {
            st.sync_server();
        }
        out
    })
    .await
}

/// Another computer running Erebor (app with remote access, or node).
#[tauri::command]
async fn remote_call(url: String, token: String, cmd: String, args: Option<Value>) -> R<Value> {
    blocking(move || server::remote_call(&url, &token, &cmd, &args.unwrap_or(Value::Null))).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteInfo {
    /// This window talks to a node service on this computer, not to its own engine.
    via_node: bool,
    running: bool,
    port: Option<u16>,
    url: Option<String>,
    token: Option<String>,
    error: Option<String>,
    login_item: bool,
    node_service: bool,
}

#[tauri::command]
fn remote_info(s: State<'_, Arc<AppState>>) -> RemoteInfo {
    let port = s.server.lock().unwrap().as_ref().map(|r| r.port);
    let host = server::lan_ip().unwrap_or_else(|| "127.0.0.1".into());
    RemoteInfo {
        via_node: !matches!(s.backend, Backend::Local(_)),
        running: port.is_some(),
        port,
        url: port.map(|p| format!("http://{host}:{p}")),
        token: port.and_then(|_| server::token(&s.data_dir).ok()),
        error: s.server_error.lock().unwrap().clone(),
        login_item: autostart::is_installed(APP_LABEL, Kind::Login),
        node_service: autostart::is_installed(autostart::NODE_LABEL, Kind::Service),
    }
}

/// Open Erebor at login (this executable).
#[tauri::command]
fn set_login_item(enabled: bool) -> R<()> {
    let r = if enabled {
        let exe = std::env::current_exe().map_err(|e| Value::String(e.to_string()))?;
        autostart::install(APP_LABEL, &exe, &[], Kind::Login)
    } else {
        autostart::uninstall(APP_LABEL, Kind::Login)
    };
    r.map_err(Value::String)
}

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let handle = app.handle().clone();
            let assets: Assets = Arc::new(move |p: &str| {
                handle
                    .asset_resolver()
                    .get(p.into())
                    .map(|a| (a.bytes, a.mime_type))
            });
            let backend = match Engine::new(dir.clone(), engine::default_factory()) {
                Ok(engine) => {
                    let start = engine.config().start_on_launch;
                    let engine = Arc::new(Mutex::new(engine));
                    engine::spawn_loops(engine.clone());
                    if start {
                        let _ = engine.lock().unwrap().start(false);
                    }
                    Backend::Local(engine)
                }
                Err(e)
                    if e == "engine_busy"
                        && ipc::call(&dir, "agent_info", &Value::Null).is_ok() =>
                {
                    Backend::Agent(dir.clone())
                }
                Err(e) if e == "engine_busy" => {
                    let port = erebor_core::config::AppConfig::load(&dir.join("config.json"))
                        .remote
                        .port;
                    Backend::Node {
                        url: format!("http://127.0.0.1:{port}"),
                        token: server::token(&dir)?,
                    }
                }
                Err(e) => return Err(e.into()),
            };
            let ipc = match &backend {
                Backend::Local(e) => ipc::serve(ipc::Host::new(e.clone(), "desktop", None)).ok(),
                _ => None,
            };
            let state = Arc::new(AppState {
                backend,
                _ipc: ipc,
                data_dir: dir,
                assets,
                server: Mutex::new(None),
                server_error: Mutex::new(None),
            });
            state.sync_server();
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            call,
            remote_call,
            remote_info,
            set_login_item
        ])
        .build(tauri::generate_context!())
        .expect("failed to build Erebor");

    app.run(|handle, event| {
        // Mining always stops when the user quits the app (no hidden background mining);
        // the node service is the explicit way to keep mining without a window.
        if let tauri::RunEvent::Exit = event {
            if let Some(s) = handle.try_state::<Arc<AppState>>() {
                if let Backend::Local(e) = &s.backend {
                    e.lock().unwrap_or_else(|p| p.into_inner()).stop("app_exit");
                }
            }
        }
    });
}

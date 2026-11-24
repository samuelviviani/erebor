// Release builds on Windows: no extra console window (the GUI shows mining status).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Tauri shell. Every GUI command goes through `call` to `erebor_core::rpc`, the same
//! dispatcher the `erebor` command line uses. No shell commands, no arbitrary process
//! execution: the only executables ever started are the miners installed from the
//! pinned manifest or explicitly selected by the user (and, on request, the OS tools
//! that register Erebor to start at login).

use erebor_core::autostart::{self, Kind, APP_LABEL};
use erebor_core::engine::{self, Engine};
use erebor_core::ipc;
use erebor_core::rpc::{self, Shared};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{Manager, State};

/// Where the engine lives: our own, or the `erebor` background agent already running
/// here (it holds the data folder lock), reached over its local socket. Either way there
/// is one engine and one source of state.
enum Backend {
    Local(Shared),
    Agent(PathBuf),
}

struct AppState {
    backend: Backend,
    /// While we own the engine, `erebor` (CLI/TUI) controls it through this socket.
    _ipc: Option<ipc::Server>,
}

type R<T> = Result<T, Value>;

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> R<T> + Send + 'static) -> R<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| Value::String(e.to_string()))?
}

#[tauri::command]
async fn call(s: State<'_, Arc<AppState>>, cmd: String, args: Option<Value>) -> R<Value> {
    let st = s.inner().clone();
    blocking(move || {
        let args = args.unwrap_or(Value::Null);
        match &st.backend {
            Backend::Local(e) => rpc::call(e, &cmd, &args),
            Backend::Agent(dir) => ipc::call(dir, &cmd, &args),
        }
    })
    .await
}

/// Whether Erebor opens at login.
#[tauri::command]
fn login_item() -> bool {
    autostart::is_installed(APP_LABEL, Kind::Login)
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
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
                Err(e) => return Err(e.into()),
            };
            let ipc = match &backend {
                Backend::Local(e) => ipc::serve(ipc::Host::new(e.clone(), "desktop", None)).ok(),
                _ => None,
            };
            app.manage(Arc::new(AppState { backend, _ipc: ipc }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![call, login_item, set_login_item])
        .build(tauri::generate_context!())
        .expect("failed to build Erebor");

    app.run(|handle, event| {
        // Mining always stops when the user quits the app (no hidden background mining);
        // `erebor agent` is the explicit way to keep mining without a window.
        if let tauri::RunEvent::Exit = event {
            if let Some(s) = handle.try_state::<Arc<AppState>>() {
                if let Backend::Local(e) = &s.backend {
                    e.lock().unwrap_or_else(|p| p.into_inner()).stop("app_exit");
                }
            }
        }
    });
}

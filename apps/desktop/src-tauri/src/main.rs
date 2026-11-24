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

/// Same dispatch as `call`, for the tray menu (already off the UI thread).
fn rpc_call(st: &AppState, cmd: &str, args: Value) -> R<Value> {
    match &st.backend {
        Backend::Local(e) => rpc::call(e, cmd, &args),
        Backend::Agent(dir) => ipc::call(dir, cmd, &args),
    }
}

/// Bring the window back (tray, second launch, dock icon).
fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Tray menu: start/stop and "stop for a while" without opening the window.
fn tray(app: &tauri::App, st: Arc<AppState>) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::TrayIconBuilder;
    let it = rpc_call(&st, "get_config", Value::Null)
        .ok()
        .and_then(|c| {
            c.get("language")
                .and_then(|l| l.as_str().map(|l| l == "it"))
        })
        .unwrap_or(false);
    let l = |i: &str, e: &str| if it { i.to_string() } else { e.to_string() };
    let item = |id: &str, text: String| MenuItem::with_id(app, id, text, true, None::<&str>);
    let menu = Menu::with_items(
        app,
        &[
            &item(
                "toggle",
                l("Avvia / ferma il mining", "Start / stop mining"),
            )?,
            &item("stop60", l("Ferma per 1 ora", "Stop for 1 hour"))?,
            &item("stop180", l("Ferma per 3 ore", "Stop for 3 hours"))?,
            &PredefinedMenuItem::separator(app)?,
            &item("open", l("Apri Erebor", "Open Erebor"))?,
            &item("quit", l("Esci", "Quit"))?,
        ],
    )?;
    let mut b = TrayIconBuilder::new().menu(&menu).tooltip("Erebor");
    if let Some(icon) = app.default_window_icon() {
        b = b.icon(icon.clone());
    }
    b.on_menu_event(move |app, ev| {
        let id = ev.id().as_ref().to_string();
        match id.as_str() {
            "open" => show_main(app),
            "quit" => app.exit(0),
            _ => {
                let st = st.clone();
                std::thread::spawn(move || {
                    let _ = match id.as_str() {
                        "stop60" => rpc_call(&st, "stop_for", serde_json::json!({"minutes": 60})),
                        "stop180" => rpc_call(&st, "stop_for", serde_json::json!({"minutes": 180})),
                        _ => {
                            let s = rpc_call(&st, "get_status", Value::Null).unwrap_or_default();
                            let on = matches!(s["state"].as_str(), Some("running" | "paused"))
                                || !s["held"].is_null();
                            rpc_call(
                                &st,
                                if on { "stop_mining" } else { "start_mining" },
                                Value::Null,
                            )
                        }
                    };
                });
            }
        }
    })
    .build(app)?;
    Ok(())
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
        // A second launch (Start menu, desktop icon) shows the running app instead.
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show_main(app)
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let backend = match Engine::new(dir.clone(), engine::default_factory()) {
                Ok(engine) => {
                    // Also after an app update that interrupted mining.
                    let start = engine.config().start_on_launch | engine.take_resume_after_update();
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
            let st = Arc::new(AppState { backend, _ipc: ipc });
            app.manage(st.clone());
            // Like a chat app: closing the window keeps Erebor (and mining) running in
            // the tray; "Quit" there exits. Without a tray (Linux without an indicator
            // service) closing still quits, so nothing is left running out of sight.
            match tray(app, st) {
                Ok(()) => {
                    if let Some(w) = app.get_webview_window("main") {
                        let w2 = w.clone();
                        w.on_window_event(move |ev| {
                            if let tauri::WindowEvent::CloseRequested { api, .. } = ev {
                                api.prevent_close();
                                let _ = w2.hide();
                            }
                        });
                    }
                }
                Err(e) => eprintln!("tray unavailable, closing the window quits: {e}"),
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![call, login_item, set_login_item])
        .build(tauri::generate_context!())
        .expect("failed to build Erebor");

    app.run(|handle, event| {
        // macOS: clicking the Dock icon brings the hidden window back.
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = event {
            show_main(handle);
        }
        // Quitting (tray "Quit", Cmd+Q) always stops mining: nothing keeps mining once
        // Erebor is gone; `erebor agent` is the explicit way to mine without the app.
        if let tauri::RunEvent::Exit = event {
            if let Some(s) = handle.try_state::<Arc<AppState>>() {
                if let Backend::Local(e) = &s.backend {
                    e.lock().unwrap_or_else(|p| p.into_inner()).stop("app_exit");
                }
            }
        }
    });
}

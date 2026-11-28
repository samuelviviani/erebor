// Release builds on Windows: no extra console window (the GUI shows mining status).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Tauri shell. Every GUI command goes through `call` to `erebor_core::rpc`, the same
//! dispatcher the `erebor` command line uses. No shell commands, no arbitrary process
//! execution: the only executables ever started are the miners installed from the
//! pinned manifest or explicitly selected by the user (and, on request, the OS tools
//! that register Erebor to start at login).

use erebor_core::autostart::{self, Kind, APP_LABEL};
use erebor_core::controller::LocalCall;
use erebor_core::engine::{self, Engine};
use erebor_core::ipc;
use erebor_core::remote::{self, RemoteConfig};
use erebor_core::rpc::{self, Shared};
use erebor_core::sim::Hub;
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
    /// Devices managed from here (this PC included); in demo mode a simulated set.
    fleet: Hub,
    data_dir: PathBuf,
    remote: Mutex<Option<remote::server::Server>>,
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
    let live = st.clone();
    let mut b = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .tooltip("Erebor");
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
    let Some(tray) = app.tray_by_id("main") else {
        return Ok(());
    };
    // The speed beside the icon in the macOS menu bar (and in the tooltip elsewhere),
    // so a glance is enough to know it is mining.
    std::thread::spawn(move || loop {
        let s = rpc_call(&live, "get_status", Value::Null).unwrap_or_default();
        let h = s["stats"]["hashrate10s"]
            .as_f64()
            .or_else(|| s["stats"]["hashrate60s"].as_f64())
            .filter(|h| *h > 0.0 && s["state"] == "running");
        let label = h.map(hashrate);
        #[cfg(target_os = "macos")]
        let _ = tray.set_title(label.as_deref());
        let _ = tray.set_tooltip(Some(match &label {
            Some(l) => format!("Erebor · {l}"),
            None => "Erebor".to_string(),
        }));
        std::thread::sleep(std::time::Duration::from_secs(5));
    });
    Ok(())
}

/// System notifications for problems (warnings and errors) on this PC and on the other
/// devices, also while the window sits in the tray, plus the monthly summary. While the
/// window is in front the app's own toasts say the same, so nothing is sent. Not in demo
/// mode, nor in the user's quiet hours. Device problems also go to Telegram when it is set
/// up (this PC's own are sent by its engine).
fn notifier(app: tauri::AppHandle, st: Arc<AppState>, data_dir: PathBuf) {
    use erebor_core::config::Notify;
    use erebor_core::{controller::LOCAL_ID, notify};
    use tauri_plugin_notification::NotificationExt;
    std::thread::spawn(move || {
        // The first read sets the baseline: nothing from before the launch is repeated.
        let (mut last_alert, mut last_event) = (None::<u64>, None::<i64>);
        let problem = |v: &Value| matches!(v["level"].as_str(), Some("warning" | "error"));
        loop {
            std::thread::sleep(std::time::Duration::from_secs(5));
            let status = rpc_call(&st, "get_status", Value::Null).unwrap_or_default();
            if status["demo"] == true {
                (last_alert, last_event) = (None, None);
                continue;
            }
            let config = rpc_call(&st, "get_config", Value::Null).unwrap_or_default();
            let lang = config["language"].as_str().unwrap_or("en");
            let n: Notify = serde_json::from_value(config["notify"].clone()).unwrap_or_default();
            let silent = notify::quiet_now(&n);
            let in_front = app.get_webview_window("main").is_some_and(|w| {
                w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false)
            });
            let show = |title: String, body: String| {
                if n.desktop && !silent && !in_front {
                    let _ = app.notification().builder().title(title).body(body).show();
                }
            };

            let alerts = status["alerts"].as_array().cloned().unwrap_or_default();
            let max = alerts.iter().filter_map(|a| a["id"].as_u64()).max();
            if let Some(last) = last_alert {
                for a in alerts
                    .iter()
                    .filter(|a| a["id"].as_u64() > Some(last) && problem(a))
                {
                    let code = a["code"].as_str().unwrap_or_default();
                    show(
                        "Erebor".into(),
                        notify::text(lang, code, a["detail"].as_str()),
                    );
                }
            }
            last_alert = max.max(last_alert).or(Some(0));

            // Not a repeat of anything on screen: shown even with the window in front.
            let summary = |title: String, body: String| {
                if n.desktop {
                    let _ = app.notification().builder().title(title).body(body).show();
                }
            };
            monthly_summary(&st, &data_dir, &config, lang, &n, silent, &summary);

            // Other devices; this PC's own alerts already came from its engine above.
            let since = last_event.unwrap_or(0);
            let Ok(events) = st.fleet.call(
                "fleet_events",
                &serde_json::json!({"sinceId": since, "limit": 50}),
            ) else {
                continue;
            };
            let events = events.as_array().cloned().unwrap_or_default();
            if last_event.is_some() {
                for e in events
                    .iter()
                    .rev()
                    .filter(|e| problem(e) && e["deviceId"] != LOCAL_ID)
                {
                    let name = e["deviceName"].as_str().unwrap_or_default();
                    let code = e["code"].as_str().unwrap_or_default();
                    let (title, body) = (
                        format!("Erebor · {name}"),
                        notify::device_text(lang, code, e["detail"].as_str()),
                    );
                    if notify::has_telegram(&n) && !silent {
                        let (n, title, body) = (n.clone(), title.clone(), body.clone());
                        std::thread::spawn(move || notify::send(&n, &title, &body));
                    }
                    show(title, body);
                }
            }
            last_event = Some(
                events
                    .iter()
                    .filter_map(|e| e["id"].as_i64())
                    .fold(since, i64::max),
            );
        }
    });
}

/// Once a month, the first time Erebor runs in it: what last month mined, what the
/// electricity cost and what is left (system notification and Telegram). Remembered in
/// `<data dir>/summary-month`, so it is sent once even across restarts.
fn monthly_summary(
    st: &AppState,
    data_dir: &std::path::Path,
    config: &Value,
    lang: &str,
    n: &erebor_core::config::Notify,
    silent: bool,
    show: &dyn Fn(String, String),
) {
    use erebor_core::notify;
    if !n.monthly_summary || silent {
        return;
    }
    let (label, name, from, to) = notify::last_month(lang);
    let marker = data_dir.join("summary-month");
    if std::fs::read_to_string(&marker).is_ok_and(|m| m.trim() == label) {
        return;
    }
    let args = serde_json::json!({"rangeSecs": to - from, "to": to});
    let Ok(report) = rpc_call(st, "get_stats", args) else {
        return;
    };
    let _ = std::fs::write(&marker, &label);
    let currency = config["energy"]["currency"].as_str().unwrap_or("EUR");
    if let Some(body) = notify::month_summary(lang, &name, &report["earnings"], currency) {
        if notify::has_telegram(n) {
            let n = n.clone();
            let body = body.clone();
            std::thread::spawn(move || notify::send(&n, "Erebor", &body));
        }
        show("Erebor".into(), body);
    }
}

/// Same format as the app (`hashrate` in src/lib/format.ts).
fn hashrate(h: f64) -> String {
    for (unit, div) in [("TH/s", 1e12), ("GH/s", 1e9), ("MH/s", 1e6), ("kH/s", 1e3)] {
        if h >= div {
            return format!("{:.2} {unit}", h / div);
        }
    }
    format!("{h:.0} H/s")
}

#[tauri::command]
async fn call(s: State<'_, Arc<AppState>>, cmd: String, args: Option<Value>) -> R<Value> {
    let st = s.inner().clone();
    blocking(move || {
        let args = args.unwrap_or(Value::Null);
        if cmd == "desktop_pair" {
            let mut cfg = RemoteConfig::load(&st.data_dir);
            if let Backend::Local(engine) = &st.backend {
                let mut server = st.remote.lock().map_err(|e| Value::String(e.to_string()))?;
                if server.is_none() {
                    let started = remote::server::serve(engine.clone(), &cfg).map_err(Value::String)?;
                    cfg.enabled = true;
                    if let Err(e) = cfg.save(&st.data_dir) {
                        drop(started);
                        return Err(Value::String(e));
                    }
                    *server = Some(started);
                }
            } else if !cfg.enabled {
                return Err(Value::String("remote_agent_disabled".into()));
            }
            let p = remote::start_pairing(&st.data_dir).map_err(Value::String)?;
            return Ok(serde_json::json!({"code": remote::format_code(&p.code), "expiresAt": p.expires_at}));
        }
        if cmd.starts_with("fleet_") {
            return st.fleet.call(&cmd, &args);
        }
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
        // Size and position are remembered between launches.
        .plugin(tauri_plugin_window_state::Builder::default().build())
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
            // This PC in the device list, through the same backend as the rest of the app.
            let local: LocalCall = match &backend {
                Backend::Local(e) => {
                    let e = e.clone();
                    Arc::new(move |c, a| rpc::call(&e, c, a))
                }
                Backend::Agent(d) => {
                    let d = d.clone();
                    Arc::new(move |c, a| ipc::call(&d, c, a))
                }
            };
            let fleet = Hub::new(&dir, local);
            let cfg = RemoteConfig::load(&dir);
            let remote = match &backend {
                Backend::Local(e) if cfg.enabled => match remote::server::serve(e.clone(), &cfg) {
                    Ok(server) => Some(server),
                    Err(e) => {
                        eprintln!("remote control unavailable: {e}");
                        None
                    }
                },
                _ => None,
            };
            let st = Arc::new(AppState {
                backend,
                _ipc: ipc,
                fleet,
                data_dir: dir.clone(),
                remote: Mutex::new(remote),
            });
            app.manage(st.clone());
            notifier(app.handle().clone(), st.clone(), dir.clone());
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

//! Erebor core library. Independent from the GUI so it can be tested headless.

pub mod autostart;
pub mod coins;
pub mod config;
pub mod devfee;
pub mod earnings;
pub mod engine;
pub mod fleet;
pub mod hardware;
pub mod install;
pub mod ipc;
pub mod miner;
pub mod pools;
pub mod rpc;
pub mod server;
pub mod srbminer;
pub mod store;
pub mod update;
pub mod wallet;
pub mod xmrig;

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The desktop app's data folder (Tauri `app_data_dir`): app and node share settings,
/// statistics and installed miners. Only one of them runs the engine at a time.
pub fn default_data_dir() -> std::path::PathBuf {
    use std::path::PathBuf;
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let home = env("HOME").unwrap_or_default();
    let base = if cfg!(windows) {
        env("APPDATA").unwrap_or_default()
    } else if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else {
        env("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share"))
    };
    base.join(autostart::APP_LABEL)
}

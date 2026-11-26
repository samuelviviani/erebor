//! Erebor core library. Independent from the GUI so it can be tested headless.

pub mod asic;
pub mod asic_sim;
pub mod autostart;
pub mod coins;
pub mod config;
pub mod controller;
pub mod cpuminer;
pub mod devfee;
pub mod earnings;
pub mod energy;
pub mod engine;
pub mod fleet;
pub mod hardware;
pub mod install;
pub mod ipc;
pub mod lolminer;
pub mod miner;
pub mod notify;
pub mod pools;
pub mod remote;
pub mod rpc;
pub mod sim;
pub mod srbminer;
pub mod store;
pub mod update;
pub mod wallet;
pub mod xmrig;

/// `Command` for a helper program that never opens a console window on Windows: the GUI
/// polls tools like nvidia-smi every few seconds, and each one would flash a terminal.
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    #[allow(unused_mut)]
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The desktop app's data folder (Tauri `app_data_dir`): app, CLI and agent share settings,
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

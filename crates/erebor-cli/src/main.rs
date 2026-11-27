//! `erebor`: the command-line interface. Every command that reads or changes mining
//! state goes to the process that owns the engine (agent, desktop app or node) over
//! the local socket, so the CLI, the TUI and the GUI always show the same state.
//!
//! Exit codes: 0 ok · 1 command failed · 2 usage error · 3 agent not reachable.

mod agent;
mod args;
mod doctor;
mod fleet;
mod mining;
mod onboard;
mod settings;
mod sim;
mod status;
#[cfg(test)]
mod tests;
mod ui;

use anstream::{eprintln, println};

use self::{agent::*, args::*, mining::*, settings::*, status::*};
use clap::{Parser, Subcommand, ValueEnum};
use erebor_core::config::{AppConfig, PoolConfig, AUTO_COIN};
use erebor_core::hardware::Monitor;
use erebor_core::ipc::{self, Action};
use erebor_core::miner::MinerKind;
use erebor_core::{autostart, install, pools, wallet};
use erebor_tui::fmt::{self, explain};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// A failed command: an error code plus, when known, what to do about it.
#[derive(Debug)]
struct Fail {
    code: String,
}

impl From<Value> for Fail {
    fn from(v: Value) -> Self {
        let code = match v {
            Value::String(s) => s,
            other => other.to_string(),
        };
        Fail { code }
    }
}

impl From<String> for Fail {
    fn from(code: String) -> Self {
        Fail { code }
    }
}

fn fail<T>(code: &str) -> Result<T, Fail> {
    Err(Fail { code: code.into() })
}

fn main() {
    // Release packages install `erebor-agent` as a link to this binary.
    if std::env::args_os()
        .next()
        .and_then(|a| PathBuf::from(a).file_stem().map(|s| s == "erebor-agent"))
        == Some(true)
    {
        return erebor_agent::cli::cli(std::env::args().skip(1));
    }
    let cli = Cli::parse();
    let json = cli.json;
    let dir = cli
        .data_dir
        .clone()
        .unwrap_or_else(erebor_core::default_data_dir);
    if let Err(f) = run(cli, &dir) {
        let (msg, mut hint) = explain(&f.code);
        if f.code == "agent_unreachable" && onboard::needs_setup(&dir) {
            hint = Some("first time? run the guided setup: erebor onboard");
        }
        if json {
            println!("{}", json!({"error": f.code, "message": msg, "hint": hint}));
        } else {
            eprintln!("{} {msg}", ui::paint("✗ error:", ui::ERR));
            if msg != f.code {
                eprintln!("  {}", ui::paint(format!("code: {}", f.code), ui::DIM));
            }
            if let Some(h) = hint {
                eprintln!("  → {}", ui::paint(h, ui::CMD));
            }
        }
        std::process::exit(if f.code == "agent_unreachable" { 3 } else { 1 });
    }
}

fn print_json<T: Serialize>(v: &T) {
    println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
}

fn call(dir: &Path, cmd: &str, args: Value) -> Result<Value, Fail> {
    ipc::call(dir, cmd, &args).map_err(Fail::from)
}

/// Configuration as the engine holds it; the file when no agent runs (read-only).
fn load_config(dir: &Path) -> Result<AppConfig, Fail> {
    match ipc::call(dir, "get_config", &Value::Null) {
        Ok(v) => {
            serde_json::from_value(v).map_err(|_| Fail::from("agent_bad_response".to_string()))
        }
        Err(e) if e == "agent_unreachable" => Ok(AppConfig::load(&dir.join("config.json"))),
        Err(e) => Err(e.into()),
    }
}

/// Changes always go through the engine, which validates and saves them.
fn save_config(dir: &Path, c: &AppConfig) -> Result<AppConfig, Fail> {
    let v = call(dir, "update_config", json!({ "config": c }))?;
    serde_json::from_value(v).map_err(|_| "agent_bad_response".to_string().into())
}

fn row(label: &str, value: impl std::fmt::Display) {
    println!("{} {value}", ui::label(label, 15));
}

fn run(cli: Cli, dir: &Path) -> Result<(), Fail> {
    let json = cli.json;
    let cmd = match cli.cmd {
        Some(c) => c,
        None if onboard::needs_setup(dir) && onboard::interactive() => {
            Cmd::Onboard(onboard::Opts::default())
        }
        None if onboard::needs_setup(dir) => {
            println!(
                "Erebor is not set up yet. Run: {}   (or: {})",
                ui::paint("erebor onboard", ui::CMD),
                ui::paint("erebor --help", ui::CMD)
            );
            return Ok(());
        }
        None if onboard::interactive() && !json => Cmd::Monitor,
        None => Cmd::Status,
    };
    if needs_engine(&cmd) {
        ensure_agent(dir, json)?;
    }
    match cmd {
        Cmd::Start => mining(dir, MiningCmd::Start(Default::default()), json),
        Cmd::Service { state } => service(dir, state, json),
        Cmd::Power { level } => power(dir, level, json),
        Cmd::Update => update(dir, json),
        Cmd::Stop => mining(dir, MiningCmd::Stop(Default::default()), json),
        Cmd::Onboard(o) => onboard::run(dir, o, json),
        Cmd::Status => status(dir, json),
        Cmd::Doctor { offline } => {
            let cfg = load_config(dir)?;
            let checks = doctor::run(dir, &cfg, offline);
            let failed = checks
                .iter()
                .filter(|c| c.status == doctor::Level::Fail)
                .count();
            if json {
                print_json(&json!({"ok": failed == 0, "checks": checks}));
            } else {
                println!(
                    "{} {}\n",
                    ui::paint(format!("Erebor {}", env!("CARGO_PKG_VERSION")), ui::BRAND),
                    ui::paint(format!("· data folder {}", dir.display()), ui::DIM)
                );
                for c in &checks {
                    let mark = match c.status {
                        doctor::Level::Ok => ui::paint("✓", ui::OK),
                        doctor::Level::Warn => ui::paint("!", ui::WARN),
                        doctor::Level::Fail => ui::paint("✗", ui::ERR),
                    };
                    println!("{mark} {} {}", ui::label(c.id, 15), c.message);
                    if let Some(h) = &c.hint {
                        println!("  {:<15} → {}", "", ui::paint(h, ui::CMD));
                    }
                }
                println!(
                    "\n{}",
                    if failed == 0 {
                        ui::ok("No blocking problems.")
                    } else {
                        ui::paint(format!("✗ {failed} problem(s) to fix."), ui::ERR)
                    }
                );
            }
            if failed > 0 {
                std::process::exit(1);
            }
            Ok(())
        }
        Cmd::Mining { action } => mining(dir, action, json),
        Cmd::Hardware { cmd } => hardware(cmd, json),
        Cmd::Wallet { cmd } => wallet_cmd(dir, cmd, json),
        Cmd::Pools { cmd } => pools_cmd(dir, cmd, json),
        Cmd::Monitor => erebor_tui::run(dir.to_path_buf()).map_err(Fail::from),
        Cmd::Config { cmd } => config_cmd(dir, cmd.unwrap_or(ConfigCmd::Show), json),
        Cmd::Coins { cmd } => coins_cmd(dir, cmd.unwrap_or(CoinsCmd::List), json),
        Cmd::Logs { follow } => logs(dir, follow, json),
        Cmd::Export { days } => export(dir, i64::from(days) * 86_400),
        Cmd::Agent { cmd } => agent(dir, cmd, json),
        Cmd::Devices { cmd } => fleet::devices(dir, cmd, json),
        Cmd::Groups { cmd } => fleet::groups(dir, cmd, json),
        Cmd::Profiles { cmd } => fleet::profiles(dir, cmd, json),
        Cmd::Sim {
            pair,
            keep,
            base_port,
        } => sim::run(dir, pair, keep, base_port, json),
        Cmd::Demo { state } => {
            let mut c = load_config_live(dir)?;
            c.demo_mode = state == OnOff::On;
            save_config(dir, &c)?;
            let s = ipc::snapshot(dir)?;
            if json {
                print_json(&s);
            } else if s.demo {
                println!(
                    "{}\nTry: {}",
                    ui::ok("Demo mode on: a simulated miner, no real mining and no network."),
                    ui::paint("erebor start", ui::CMD)
                );
            } else {
                println!("{}", ui::ok("Demo mode off: real mining mode."));
                if let Some(b) = &s.start_blocker {
                    let (m, h) = explain(b);
                    println!(
                        "Not ready yet: {m}{}",
                        h.map(|h| format!("\n  hint: {h}")).unwrap_or_default()
                    );
                }
            }
            Ok(())
        }
        Cmd::Miner {
            cmd: MinerCmd::Install { kind },
        } => miner_install(dir, kind, json),
        Cmd::Device { cmd } => {
            let v = match cmd {
                DeviceCmd::Show => call(dir, "device_info", Value::Null)?,
                DeviceCmd::Name { name } => call(dir, "set_device_name", json!({ "name": name }))?,
            };
            if json {
                print_json(&v);
            } else if v.get("device").is_some() {
                row("Device", v["device"]["name"].as_str().unwrap_or("?"));
                row("ID", v["device"]["id"].as_str().unwrap_or("?"));
                row("Hostname", v["hostname"].as_str().unwrap_or("?"));
                row(
                    "OS",
                    format!(
                        "{} {}",
                        v["hardware"]["osName"].as_str().unwrap_or("?"),
                        v["hardware"]["osVersion"].as_str().unwrap_or("")
                    ),
                );
                row("CPU", v["hardware"]["cpuBrand"].as_str().unwrap_or("?"));
                row(
                    "Mining",
                    fmt::state(v["snapshot"]["state"].as_str().unwrap_or("")),
                );
                row("Schema", v["schema"].clone());
            } else {
                println!("Device name set: {}", v["name"].as_str().unwrap_or("?"));
            }
            Ok(())
        }
    }
}

/// Commands that change or watch the engine: Erebor starts in the background for them.
/// Read-only ones (status, doctor, lists) never start anything.
fn needs_engine(cmd: &Cmd) -> bool {
    // Mining on other devices goes to their agents, not to this computer's engine.
    if let Cmd::Mining {
        action: MiningCmd::Start(t) | MiningCmd::Restart(t),
    } = cmd
    {
        return !t.is_set();
    }
    matches!(
        cmd,
        Cmd::Start
            | Cmd::Monitor
            | Cmd::Power { level: Some(_) }
            | Cmd::Demo { .. }
            | Cmd::Miner { .. }
            | Cmd::Device { .. }
            | Cmd::Coins { .. }
            | Cmd::Config {
                cmd: Some(ConfigCmd::Set { .. })
            }
            | Cmd::Wallet {
                cmd: WalletCmd::Set { .. }
            }
            | Cmd::Pools {
                cmd: PoolsCmd::Set { .. }
            }
    )
}

/// Start the agent in the background when nothing owns the engine yet.
fn ensure_agent(dir: &Path, json: bool) -> Result<(), Fail> {
    match ipc::call(dir, "agent_info", &Value::Null) {
        Err(e) if e == "agent_unreachable" => {
            if !json {
                eprintln!(
                    "{}",
                    ui::paint("Starting Erebor in the background…", ui::DIM)
                );
            }
            spawn_agent(dir).map(|_| ())
        }
        _ => Ok(()),
    }
}

/// Configuration for a change: needs the agent (the engine validates and saves).
fn load_config_live(dir: &Path) -> Result<AppConfig, Fail> {
    let v = call(dir, "get_config", Value::Null)?;
    serde_json::from_value(v).map_err(|_| "agent_bad_response".to_string().into())
}

//! `erebor`: the command-line interface. Every command that reads or changes mining
//! state goes to the process that owns the engine (agent, desktop app or node) over
//! the local socket, so the CLI, the TUI and the GUI always show the same state.
//!
//! Exit codes: 0 ok · 1 command failed · 2 usage error · 3 agent not reachable.

mod doctor;
mod onboard;

use clap::{Parser, Subcommand, ValueEnum};
use erebor_core::config::{AppConfig, PoolConfig, AUTO_COIN};
use erebor_core::hardware::Monitor;
use erebor_core::ipc::{self, Action};
use erebor_core::miner::MinerKind;
use erebor_core::{autostart, coins, devfee, install, pools, wallet, xmrig};
use erebor_tui::fmt::{self, explain};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Parser, Debug)]
#[command(
    name = "erebor",
    version,
    about = "Erebor mining manager — terminal interface",
    after_help = "\
Quick start:
  erebor               live dashboard (first time: guided setup)
  erebor start         start mining (keeps running in the background)
  erebor stop          stop mining
  erebor status        one-shot summary
  erebor demo on       try it with a simulated miner first
  erebor service on    mine always, also after the computer restarts
  erebor update        install the latest version (also automatic in the background)

Exit codes: 0 ok, 1 command failed, 2 usage error, 3 Erebor not running.
Docs: docs/CLI.md"
)]
struct Cli {
    /// Data folder shared with the desktop app (default: the app's data folder).
    #[arg(long, global = true, env = "EREBOR_DATA_DIR", value_name = "DIR")]
    data_dir: Option<PathBuf>,
    /// Machine-readable output (stable, documented in docs/CLI.md).
    #[arg(long, global = true)]
    json: bool,
    /// Without a command: the guided setup on first run, the live dashboard afterwards.
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Start mining. Erebor keeps running in the background after this terminal closes.
    Start,
    /// Stop mining.
    Stop,
    /// Mining state, hashrate, pool, shares, temperature, power.
    Status,
    /// Live dashboard: hashrate chart, earnings, temperatures, controls.
    #[command(visible_alias = "dashboard")]
    Monitor,
    /// Guided setup: wallet, terms, miner, health check (like a first run).
    Onboard(onboard::Opts),
    /// Mine always: start Erebor and mining with the computer (on), or stop that (off).
    Service { state: Option<OnOff> },
    /// Install the latest signed release of this program.
    Update,
    /// Check OS, hardware, permissions, miners, wallet, pool and agent.
    Doctor {
        /// Skip the pool connectivity test.
        #[arg(long)]
        offline: bool,
    },
    /// Start, stop, pause, resume or restart mining (start/stop also as `erebor start`).
    Mining {
        #[command(subcommand)]
        action: MiningCmd,
    },
    /// Detected CPU, GPUs and sensors.
    Hardware {
        #[command(subcommand)]
        cmd: HardwareCmd,
    },
    /// Payout wallet (public address only).
    Wallet {
        #[command(subcommand)]
        cmd: WalletCmd,
    },
    /// Mining pools.
    Pools {
        #[command(subcommand)]
        cmd: PoolsCmd,
    },
    /// The background process that owns mining.
    Agent {
        #[command(subcommand)]
        cmd: AgentCmd,
    },
    /// Demo mode: a simulated miner, no CPU/GPU mining, no network.
    Demo { state: OnOff },
    /// Show the fees and terms; accept or revoke them for real mining.
    Consent {
        #[arg(long, conflicts_with = "revoke")]
        accept: bool,
        #[arg(long)]
        revoke: bool,
    },
    /// Miner programs (pinned releases, SHA-256 verified).
    Miner {
        #[command(subcommand)]
        cmd: MinerCmd,
    },
    /// This device's identity (Erebor Fleet).
    Device {
        #[command(subcommand)]
        cmd: DeviceCmd,
    },
}

#[derive(Subcommand, Debug, Clone, Copy)]
enum MiningCmd {
    /// Start mining.
    Start,
    /// Stop mining.
    Stop,
    /// Pause mining (the miner stays loaded; resume is instant).
    Pause,
    /// Resume paused mining.
    Resume,
    /// Stop and start again.
    Restart,
}

#[derive(Subcommand, Debug)]
enum HardwareCmd {
    /// CPU and GPUs.
    List,
    /// Everything detected, live sensors and capabilities.
    Info,
}

#[derive(Subcommand, Debug)]
enum WalletCmd {
    /// The payout address and per-coin wallets.
    Show,
    /// Set the payout address (validated), or a coin's own wallet with --coin.
    Set {
        address: String,
        #[arg(long, value_name = "COIN")]
        coin: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum PoolsCmd {
    /// The pool catalog (* = in use).
    List,
    /// Test compatibility and connectivity of the configured pool (or all).
    Test {
        #[arg(long)]
        all: bool,
    },
    /// Use a pool from the catalog.
    Set { pool_id: String },
}

#[derive(Subcommand, Debug)]
enum AgentCmd {
    /// Start the agent in the background (or in this terminal with --foreground).
    Start {
        #[arg(long)]
        foreground: bool,
    },
    /// Stop mining and the agent.
    Stop,
    /// Whether the agent runs, its pid, version and uptime.
    Status,
    /// Start the agent at login (systemd user unit / LaunchAgent). Off by default.
    InstallService,
    /// Remove the login service.
    UninstallService,
}

#[derive(Subcommand, Debug)]
enum MinerCmd {
    /// Download and install the pinned miner release.
    Install {
        #[arg(value_enum, default_value = "xmrig")]
        kind: Kind,
    },
}

#[derive(ValueEnum, Debug, Clone, Copy)]
enum Kind {
    Xmrig,
    Srbminer,
}

#[derive(Subcommand, Debug)]
enum DeviceCmd {
    /// Name, id, OS and CPU of this device.
    Show,
    /// Set the name other Erebor instances will show for this device.
    Name { name: String },
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq)]
enum OnOff {
    On,
    Off,
}

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
            eprintln!("error: {msg}");
            if msg != f.code {
                eprintln!("  code: {}", f.code);
            }
            if let Some(h) = hint {
                eprintln!("  hint: {h}");
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
    println!("{label:<15} {value}");
}

fn run(cli: Cli, dir: &Path) -> Result<(), Fail> {
    let json = cli.json;
    let cmd = match cli.cmd {
        Some(c) => c,
        None if onboard::needs_setup(dir) && onboard::interactive() => {
            Cmd::Onboard(onboard::Opts::default())
        }
        None if onboard::needs_setup(dir) => {
            println!("Erebor is not set up yet. Run: erebor onboard   (or: erebor --help)");
            return Ok(());
        }
        None if onboard::interactive() && !json => Cmd::Monitor,
        None => Cmd::Status,
    };
    if needs_engine(&cmd) {
        ensure_agent(dir, json)?;
    }
    match cmd {
        Cmd::Start => mining(dir, MiningCmd::Start, json),
        Cmd::Service { state } => service(dir, state, json),
        Cmd::Update => update(dir, json),
        Cmd::Stop => mining(dir, MiningCmd::Stop, json),
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
                    "Erebor {} · data folder {}\n",
                    env!("CARGO_PKG_VERSION"),
                    dir.display()
                );
                for c in &checks {
                    let mark = match c.status {
                        doctor::Level::Ok => "✓",
                        doctor::Level::Warn => "!",
                        doctor::Level::Fail => "✗",
                    };
                    println!("{mark} {:<15} {}", c.id, c.message);
                    if let Some(h) = &c.hint {
                        println!("  {:<15} → {h}", "");
                    }
                }
                println!(
                    "\n{}",
                    if failed == 0 {
                        "No blocking problems.".to_string()
                    } else {
                        format!("{failed} problem(s) to fix.")
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
        Cmd::Agent { cmd } => agent(dir, cmd, json),
        Cmd::Demo { state } => {
            let mut c = load_config_live(dir)?;
            c.demo_mode = state == OnOff::On;
            save_config(dir, &c)?;
            let s = ipc::snapshot(dir)?;
            if json {
                print_json(&s);
            } else if s.demo {
                println!("Demo mode on: a simulated miner, no real mining and no network.\nTry: erebor start");
            } else {
                println!("Demo mode off: real mining mode.");
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
        Cmd::Consent { accept, revoke } => consent(dir, accept, revoke, json),
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
    matches!(
        cmd,
        Cmd::Start
            | Cmd::Monitor
            | Cmd::Demo { .. }
            | Cmd::Miner { .. }
            | Cmd::Device { .. }
            | Cmd::Wallet {
                cmd: WalletCmd::Set { .. }
            }
            | Cmd::Pools {
                cmd: PoolsCmd::Set { .. }
            }
            | Cmd::Mining {
                action: MiningCmd::Start | MiningCmd::Restart
            }
    ) || matches!(cmd, Cmd::Consent { accept, revoke } if *accept || *revoke)
}

/// Start the agent in the background when nothing owns the engine yet.
fn ensure_agent(dir: &Path, json: bool) -> Result<(), Fail> {
    match ipc::call(dir, "agent_info", &Value::Null) {
        Err(e) if e == "agent_unreachable" => {
            if !json {
                eprintln!("Starting Erebor in the background…");
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

fn status(dir: &Path, json: bool) -> Result<(), Fail> {
    let s = ipc::snapshot(dir)?;
    if json {
        print_json(&s);
        return Ok(());
    }
    let info = ipc::call(dir, "agent_info", &Value::Null).unwrap_or(Value::Null);
    let mut state = format!("● {}", fmt::state(&s.state));
    if s.demo {
        state += "  [DEMO: simulated, nothing is mined]";
    }
    row("State", state);
    if let Some(e) = &s.error {
        row("Error", explain(e).0);
    }
    if let Some(r) = s.restart_in_secs {
        row("Auto-restart", format!("in {r} s"));
    }
    row(
        "Miner",
        s.miner.as_ref().map_or(fmt::NA.to_string(), |m| {
            format!("{} {}", m.name, m.version.clone().unwrap_or_default())
        }),
    );
    row(
        "Coin",
        format!(
            "{} ({}) · {}{}",
            s.coin_name,
            s.coin,
            s.algo,
            if s.auto_coin { " · auto" } else { "" }
        ),
    );
    row(
        "Pool",
        match &s.pool {
            Some(p) => format!(
                "{p} · {}",
                if s.pool_connected {
                    "connected"
                } else {
                    "connecting"
                }
            ),
            None => format!("{} (configured)", s.pool_id),
        },
    );
    row("Hashrate", fmt::hashrate(s.hashrate));
    row(
        "Uptime",
        if s.is_active() {
            fmt::duration(s.uptime_secs)
        } else {
            "-".into()
        },
    );
    row(
        "Shares",
        format!("{} accepted · {} rejected", s.accepted, s.rejected),
    );
    row("Temperature", fmt::temp(s.cpu_temp_c));
    for g in &s.gpus {
        row("", format!("{}: {}", g.name, fmt::temp(g.temp_c)));
    }
    row("Power", fmt::power(s.power_w, &s.power_source));
    row("Earnings", fmt::earnings(&s));
    row(
        "Dev fee",
        if s.dev_fee.enabled {
            format!(
                "{}% of mining time · now mining for: {}",
                s.dev_fee.percent, s.dev_fee.active
            )
        } else {
            "off".into()
        },
    );
    if let (Some(b), false) = (&s.start_blocker, s.is_active()) {
        let (m, h) = explain(b);
        row(
            "Cannot start",
            format!("{m}{}", h.map(|h| format!(" → {h}")).unwrap_or_default()),
        );
    }
    if let Some(o) = info["owner"].as_str() {
        row("Engine", format!("{o} · pid {}", info["pid"]));
    }
    let service = autostart::is_installed(autostart::AGENT_LABEL, autostart::Kind::Service);
    row(
        "At startup",
        if service {
            "starts with the computer (erebor service)"
        } else {
            "manual · mine always: erebor service on"
        },
    );
    Ok(())
}

fn mining(dir: &Path, cmd: MiningCmd, json: bool) -> Result<(), Fail> {
    let action = match cmd {
        MiningCmd::Start => Action::Start,
        MiningCmd::Stop => Action::Stop,
        MiningCmd::Pause => Action::Pause,
        MiningCmd::Resume => Action::Resume,
        MiningCmd::Restart => Action::Restart,
    };
    let o = match ipc::control(dir, action) {
        // Nothing runs in the background, so nothing is mining.
        Err(e) if e == "agent_unreachable" && action == Action::Stop => {
            if json {
                print_json(&json!({"changed": false, "snapshot": null}));
            } else {
                println!("Nothing to do: mining is not running.");
            }
            return Ok(());
        }
        r => r?,
    };
    if json {
        print_json(&json!({"changed": o.changed, "snapshot": o.snapshot}));
        return Ok(());
    }
    let s = &o.snapshot;
    let demo = if s.demo {
        " (demo: simulated miner)"
    } else {
        ""
    };
    if !o.changed {
        println!("Nothing to do: mining is already {}.", s.state);
    } else {
        let verb = match action {
            Action::Start => "started",
            Action::Stop => "stopped",
            Action::Pause => "paused",
            Action::Resume => "resumed",
            Action::Restart => "restarted",
        };
        println!("Mining {verb}{demo}. State: {}.", fmt::state(&s.state));
        if matches!(action, Action::Start | Action::Restart | Action::Resume) {
            println!("Watch it live: erebor   ·   stop: erebor stop");
        }
        if s.dev_fee.enabled && action == Action::Start && !s.demo {
            println!(
                "Dev fee: {}% of mining time, shown in `erebor status`.",
                s.dev_fee.percent
            );
        }
    }
    Ok(())
}

fn hardware(cmd: HardwareCmd, json: bool) -> Result<(), Fail> {
    let mut m = Monitor::new();
    let hw = m.detect();
    match cmd {
        HardwareCmd::List => {
            if json {
                print_json(&hw);
                return Ok(());
            }
            row(
                "CPU",
                format!(
                    "{} · {} cores / {} threads · {}",
                    hw.cpu_brand,
                    hw.physical_cores.map_or("?".into(), |c| c.to_string()),
                    hw.logical_threads,
                    hw.arch
                ),
            );
            if hw.gpus.is_empty() {
                row("GPU", "none detected");
            }
            for (i, g) in hw.gpus.iter().enumerate() {
                let vram = g.vram_mb.map(|v| format!(" · {v} MB")).unwrap_or_default();
                row(&format!("GPU {i}"), format!("{}{vram}", g.name));
            }
        }
        HardwareCmd::Info => {
            let live = m.live();
            let caps = json!({
                "platform": install::platform_key(),
                "temperature": hw.temperature_supported,
                "power": hw.power_supported,
                "gpuMining": hw.gpu_mining_supported,
                "xmrigBuild": install::manifest_for(MinerKind::Xmrig).asset_for_platform().is_ok(),
                "srbminerBuild": install::manifest_for(MinerKind::Srbminer).asset_for_platform().is_ok(),
            });
            if json {
                print_json(&json!({"hardware": hw, "live": live, "capabilities": caps}));
                return Ok(());
            }
            row(
                "OS",
                format!("{} {} · {}", hw.os_name, hw.os_version, hw.arch),
            );
            row("CPU", &hw.cpu_brand);
            row(
                "Cores",
                format!(
                    "{} physical / {} threads",
                    hw.physical_cores.map_or("?".into(), |c| c.to_string()),
                    hw.logical_threads
                ),
            );
            row("RAM", format!("{:.1} GB", hw.ram_total_bytes as f64 / 1e9));
            row("CPU temp", fmt::temp(live.cpu_temp_c));
            row(
                "Power",
                live.power_w
                    .map_or(fmt::NA.into(), |w| format!("{w:.0} W (measured)")),
            );
            for g in &hw.gpus {
                row(
                    "GPU",
                    format!(
                        "{} · vendor {} · driver {} · VRAM {}",
                        g.name,
                        g.vendor.as_deref().unwrap_or(fmt::NA),
                        g.driver.as_deref().unwrap_or(fmt::NA),
                        g.vram_mb.map_or(fmt::NA.into(), |v| format!("{v} MB"))
                    ),
                );
            }
            for g in &live.gpus {
                row(
                    "",
                    format!(
                        "{}: {} · {}",
                        g.name,
                        fmt::temp(g.temp_c),
                        g.power_w.map_or(fmt::NA.into(), |w| format!("{w:.0} W"))
                    ),
                );
            }
            let yn = |b: bool| if b { "yes" } else { "no" };
            row("Platform", install::platform_key());
            row("Temp sensor", yn(hw.temperature_supported));
            row("Power sensor", yn(hw.power_supported));
            row("GPU mining", yn(hw.gpu_mining_supported));
            row("XMRig build", yn(caps["xmrigBuild"] == true));
            row("SRBMiner build", yn(caps["srbminerBuild"] == true));
        }
    }
    Ok(())
}

fn wallet_cmd(dir: &Path, cmd: WalletCmd, json: bool) -> Result<(), Fail> {
    match cmd {
        WalletCmd::Show => {
            let c = load_config(dir)?;
            let kind = wallet::payout_kind(&c.wallet).map(|k| format!("{k:?}"));
            if json {
                print_json(
                    &json!({"wallet": c.wallet, "kind": kind, "coinWallets": c.coin_wallets, "coin": c.coin}),
                );
                return Ok(());
            }
            if c.wallet.is_empty() {
                row("Payout", "not set  → erebor wallet set <ADDRESS>");
            } else {
                row(
                    "Payout",
                    format!(
                        "{} ({})",
                        c.wallet,
                        kind.unwrap_or_else(|| "unrecognized".into())
                    ),
                );
            }
            row(
                "Coin",
                if c.coin == AUTO_COIN {
                    "automatic".to_string()
                } else {
                    c.coin.clone()
                },
            );
            for (coin, w) in &c.coin_wallets {
                row(&format!("Wallet {coin}"), w);
            }
            Ok(())
        }
        WalletCmd::Set { address, coin } => {
            let address = address.trim().to_string();
            let mut c = load_config_live(dir)?;
            match coin {
                Some(id) => {
                    let id = id.to_ascii_uppercase();
                    coins::find(&id).ok_or_else(|| Fail::from("unknown_coin".to_string()))?;
                    wallet::validate_coin_address(&id, &address)
                        .map_err(|e| Fail::from(format!("invalid_address: {e:?}")))?;
                    c.coin_wallets.insert(id, address);
                }
                None => {
                    wallet::validate_payout(&address)
                        .map_err(|e| Fail::from(format!("invalid_address: {e:?}")))?;
                    c.wallet = address;
                }
            }
            let before_coin = c.coin.clone();
            let saved = save_config(dir, &c)?;
            if json {
                print_json(
                    &json!({"wallet": saved.wallet, "coinWallets": saved.coin_wallets, "coin": saved.coin, "pool": saved.pool.id}),
                );
                return Ok(());
            }
            println!("Wallet saved and validated.");
            if saved.coin != before_coin {
                println!("The selected coin cannot be paid to this wallet: coin set to automatic.");
            }
            if saved.pool.id != c.pool.id {
                println!(
                    "Pool changed to {} (required by this payout).",
                    saved.pool.id
                );
            }
            Ok(())
        }
    }
}

fn pools_cmd(dir: &Path, cmd: PoolsCmd, json: bool) -> Result<(), Fail> {
    match cmd {
        PoolsCmd::List => {
            let c = load_config(dir)?;
            if json {
                print_json(&json!({"configured": c.pool.id, "pools": pools::CATALOG}));
                return Ok(());
            }
            for p in pools::CATALOG {
                let mark = if p.id == c.pool.id { "*" } else { " " };
                println!(
                    "{mark} {:<12} {:<14} {}:{}{} · fee {}%",
                    p.id,
                    p.name,
                    p.host,
                    p.port,
                    if p.tls { " (TLS)" } else { "" },
                    p.fee_percent
                );
            }
            if c.pool.id == "custom" {
                println!("* custom       {}:{}", c.pool.host, c.pool.port);
            }
            Ok(())
        }
        PoolsCmd::Test { all } => {
            let c = load_config(dir)?;
            let targets: Vec<(String, String, u16)> = if all {
                pools::CATALOG
                    .iter()
                    .map(|p| (p.id.to_string(), p.host.to_string(), p.port))
                    .collect()
            } else {
                vec![(c.pool.id.clone(), c.pool.host.clone(), c.pool.port)]
            };
            let payout = wallet::payout_kind(&c.wallet);
            let mut results = vec![];
            let mut configured_ok = true;
            for (id, host, port) in targets {
                let reach = pools::check_reachable(&host, port);
                let compat = match payout {
                    Some(wallet::AddressKind::Kryptex) if !pools::is_kryptex_host(&host) => {
                        Some("a Kryptex ID only works on Kryptex")
                    }
                    Some(wallet::AddressKind::Bitcoin) => {
                        Some("BTC payouts mine on unMineable; this pool is not used")
                    }
                    _ => None,
                };
                if id == c.pool.id
                    && (reach.is_err() || compat.is_some_and(|m| m.starts_with("a Kryptex")))
                {
                    configured_ok = false;
                }
                results.push(json!({
                    "id": id, "host": host, "port": port,
                    "reachable": reach.is_ok(),
                    "latencyMs": reach.as_ref().ok(),
                    "error": reach.as_ref().err(),
                    "compatibility": compat,
                }));
                if !json {
                    let r = match &reach {
                        Ok(ms) => format!("✓ reachable ({ms} ms)"),
                        Err(e) => format!("✗ {e}"),
                    };
                    println!(
                        "{id:<12} {host}:{port}  {r}{}",
                        compat.map(|m| format!("  ! {m}")).unwrap_or_default()
                    );
                }
            }
            if json {
                print_json(&json!({"ok": configured_ok, "results": results}));
            }
            if !configured_ok {
                return fail("pool_check_failed");
            }
            Ok(())
        }
        PoolsCmd::Set { pool_id } => {
            let p = pools::find(&pool_id).ok_or_else(|| Fail::from("unknown_pool".to_string()))?;
            let mut c = load_config_live(dir)?;
            c.pool = PoolConfig {
                id: p.id.into(),
                host: p.host.into(),
                port: p.port,
                tls: p.tls,
                worker: c.pool.worker.clone(),
            };
            let saved = save_config(dir, &c)?;
            if json {
                print_json(&saved.pool);
            } else {
                println!(
                    "Pool set: {} ({}:{}). Running mining switches over automatically.",
                    p.name, p.host, p.port
                );
            }
            Ok(())
        }
    }
}

fn agent(dir: &Path, cmd: AgentCmd, json: bool) -> Result<(), Fail> {
    let info = ipc::call(dir, "agent_info", &Value::Null);
    match cmd {
        AgentCmd::Start { .. } if info.is_ok() => {
            let i = info.unwrap_or_default();
            if json {
                print_json(&json!({"started": false, "agent": i}));
            } else {
                println!(
                    "Already running: {} · pid {}. Nothing started.",
                    i["owner"].as_str().unwrap_or("?"),
                    i["pid"]
                );
            }
            Ok(())
        }
        AgentCmd::Start { foreground: true } => {
            erebor_agent::run(dir.to_path_buf()).map_err(Fail::from)
        }
        AgentCmd::Start { foreground: false } => {
            let pid = spawn_agent(dir)?;
            let i = call(dir, "agent_info", Value::Null)?;
            if json {
                print_json(&json!({"started": true, "agent": i}));
            } else {
                println!(
                    "Agent started · pid {pid} · log {}",
                    dir.join("agent.log").display()
                );
                println!("Mining is not started automatically: erebor start");
            }
            Ok(())
        }
        AgentCmd::Stop => {
            info.map_err(Fail::from)?;
            stop_agent(dir)?;
            if json {
                print_json(&json!({"stopped": true}));
            } else {
                println!("Agent stopped (mining stopped with it).");
            }
            Ok(())
        }
        AgentCmd::Status => {
            let service = autostart::is_installed(autostart::AGENT_LABEL, autostart::Kind::Service);
            let i = match info {
                Ok(i) => i,
                Err(e) if e == "agent_unreachable" => {
                    if json {
                        print_json(&json!({"running": false, "service": service}));
                    } else {
                        println!(
                            "Agent not running.{}",
                            if service {
                                " (login service installed)"
                            } else {
                                ""
                            }
                        );
                        println!("It starts by itself when needed (erebor start, erebor), or: erebor agent start");
                    }
                    std::process::exit(3);
                }
                Err(e) => return Err(e.into()),
            };
            if json {
                print_json(&json!({"running": true, "service": service, "agent": i}));
                return Ok(());
            }
            row("Owner", i["owner"].as_str().unwrap_or("?"));
            row("PID", &i["pid"]);
            row("Version", i["version"].as_str().unwrap_or("?"));
            row("Protocol", &i["protocol"]);
            let up = erebor_core::now_unix() - i["startedAt"].as_i64().unwrap_or(0);
            row("Uptime", fmt::duration(up.max(0) as u64));
            row("Socket", ipc::socket_path(dir).display());
            row(
                "Login service",
                if service {
                    "installed"
                } else {
                    "not installed (optional)"
                },
            );
            Ok(())
        }
        AgentCmd::InstallService => {
            if info.is_ok() {
                return fail("stop_agent_first");
            }
            install_service(dir)?;
            println!(
                "Agent installed as a login service ({}) and started.",
                autostart::AGENT_LABEL
            );
            println!("It never starts mining by itself unless startOnLaunch is enabled.");
            Ok(())
        }
        AgentCmd::UninstallService => {
            autostart::uninstall(autostart::AGENT_LABEL, autostart::Kind::Service)?;
            println!("Login service removed.");
            Ok(())
        }
    }
}

/// `erebor service on|off`: Erebor and mining start with the computer, or not.
fn service(dir: &Path, state: Option<OnOff>, json: bool) -> Result<(), Fail> {
    let installed = || autostart::is_installed(autostart::AGENT_LABEL, autostart::Kind::Service);
    match state {
        Some(OnOff::On) => {
            ensure_agent(dir, json)?;
            let mut c = load_config_live(dir)?;
            c.start_on_launch = true;
            save_config(dir, &c)?;
            // The service runs its own agent: hand over from a plain background one.
            if !installed() {
                let info = call(dir, "agent_info", Value::Null)?;
                if info["owner"] != "agent" {
                    return fail("not_agent");
                }
                stop_agent(dir)?;
                install_service(dir)?;
            }
            let linger = enable_linger();
            if json {
                print_json(&json!({"service": true, "startOnLaunch": true, "linger": linger}));
                return Ok(());
            }
            println!("✓ Erebor now starts with the computer and mines automatically.");
            match linger {
                Some(true) | None => {}
                Some(false) => println!(
                    "! On Linux it starts at login. To start right at boot, before anyone logs in:\n    sudo loginctl enable-linger \"$USER\""
                ),
            }
            if let Ok(s) = ipc::snapshot(dir) {
                if let (Some(b), false) = (&s.start_blocker, s.is_active()) {
                    let (m, h) = explain(b);
                    println!("! Mining cannot start yet: {m}");
                    if let Some(h) = h {
                        println!("  → {h}");
                    }
                }
            }
            println!("Turn it off: erebor service off");
            Ok(())
        }
        Some(OnOff::Off) => {
            if let Ok(mut c) = load_config_live(dir) {
                c.start_on_launch = false;
                save_config(dir, &c)?;
            }
            autostart::uninstall(autostart::AGENT_LABEL, autostart::Kind::Service)?;
            if json {
                print_json(&json!({"service": false, "startOnLaunch": false}));
            } else {
                println!("Erebor no longer starts with the computer; background mining stopped.");
                println!("Start by hand any time: erebor start");
            }
            Ok(())
        }
        None => {
            let on = installed();
            let start = load_config(dir)?.start_on_launch;
            if json {
                print_json(&json!({"service": on, "startOnLaunch": start}));
            } else if on && start {
                println!("On: Erebor starts with the computer and mines automatically.\nTurn off: erebor service off");
            } else if on {
                println!("Erebor starts with the computer but does not mine by itself.\nMine always: erebor service on");
            } else {
                println!("Off: Erebor does not start with the computer.\nMine always, also after a restart: erebor service on");
            }
            Ok(())
        }
    }
}

/// Linux: let the user's services run from boot, without a login. `None` elsewhere.
fn enable_linger() -> Option<bool> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let user = std::env::var("USER").unwrap_or_default();
    let ok = std::process::Command::new("loginctl")
        .args(["enable-linger", &user])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    Some(ok)
}

/// `erebor update`: install the latest signed release over this executable.
fn update(dir: &Path, json: bool) -> Result<(), Fail> {
    let exe = std::env::current_exe().map_err(|e| Fail::from(e.to_string()))?;
    if !json {
        eprintln!("Checking for updates…");
    }
    let installed = erebor_core::update::update(&exe)?;
    if json {
        print_json(&json!({"updated": installed, "current": env!("CARGO_PKG_VERSION")}));
        return Ok(());
    }
    match installed {
        None => println!("Erebor is up to date (v{}).", env!("CARGO_PKG_VERSION")),
        Some(v) => {
            println!("✓ Updated to v{v} (signature verified).");
            if ipc::call(dir, "agent_info", &Value::Null).is_ok_and(|i| i["owner"] == "agent") {
                println!("Erebor in the background switches to it within a few seconds; mining continues.");
            }
        }
    }
    Ok(())
}

/// Ask the agent to stop (mining stops with it) and wait until it is gone.
fn stop_agent(dir: &Path) -> Result<(), Fail> {
    call(dir, "shutdown", Value::Null)?;
    let t = Instant::now();
    while ipc::call(dir, "agent_info", &Value::Null).is_ok() {
        if t.elapsed() > Duration::from_secs(15) {
            return fail("agent_stop_timeout");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

/// Register the agent as a per-user login service and start it (no root).
fn install_service(dir: &Path) -> Result<(), Fail> {
    let exe = std::env::current_exe().map_err(|e| Fail::from(e.to_string()))?;
    std::fs::create_dir_all(dir).map_err(|e| Fail::from(e.to_string()))?;
    let abs = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let args = ["agent", "start", "--foreground", "--data-dir"]
        .map(String::from)
        .into_iter()
        .chain([abs.to_string_lossy().into_owned()])
        .collect::<Vec<_>>();
    autostart::install(
        autostart::AGENT_LABEL,
        &exe,
        &args,
        autostart::Kind::Service,
    )?;
    Ok(())
}

/// Start `erebor agent start --foreground` detached from this terminal, log to a file,
/// and wait until it answers on the socket.
fn spawn_agent(dir: &Path) -> Result<u32, Fail> {
    let e = |e: std::io::Error| Fail::from(e.to_string());
    std::fs::create_dir_all(dir).map_err(e)?;
    let log_path = dir.join("agent.log");
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(e)?;
    let log_start = log.metadata().map(|m| m.len() as usize).unwrap_or(0);
    let mut cmd = std::process::Command::new(std::env::current_exe().map_err(e)?);
    cmd.args(["agent", "start", "--foreground", "--data-dir"])
        .arg(dir)
        .stdin(std::process::Stdio::null())
        .stdout(log.try_clone().map_err(e)?)
        .stderr(log);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // New session: closing this terminal (SIGHUP) or Ctrl-C does not reach it.
        // SAFETY: setsid is async-signal-safe.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    let mut child = cmd.spawn().map_err(e)?;
    let t = Instant::now();
    loop {
        if ipc::call(dir, "agent_info", &Value::Null).is_ok() {
            return Ok(child.id());
        }
        if let Ok(Some(_)) = child.try_wait() {
            // The child printed its error (and code) to the log: report that code.
            let log = std::fs::read_to_string(&log_path).unwrap_or_default();
            let new = log.get(log_start..).unwrap_or("");
            let code = new
                .lines()
                .find_map(|l| l.trim().strip_prefix("code: "))
                .or_else(|| new.lines().find_map(|l| l.strip_prefix("error: ")))
                .unwrap_or("agent_start_failed");
            return Err(Fail::from(code.to_string()));
        }
        if t.elapsed() > Duration::from_secs(15) {
            return fail("agent_start_timeout");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn consent(dir: &Path, accept: bool, revoke: bool, json: bool) -> Result<(), Fail> {
    let pct = devfee::DEFAULT_FEE_PERCENT;
    let fee_wallet = devfee::build_wallet().is_some() || devfee::build_btc_wallet().is_some();
    if accept || revoke {
        let mut c = load_config_live(dir)?;
        if revoke {
            let s = ipc::snapshot(dir)?;
            if s.is_active() && !s.demo {
                ipc::control(dir, Action::Stop)?;
            }
        }
        c.onboarding_completed = accept;
        save_config(dir, &c)?;
    }
    let c = load_config(dir)?;
    if json {
        print_json(&json!({
            "accepted": c.onboarding_completed,
            "devFeePercent": pct,
            "devFeeActiveInThisBuild": fee_wallet,
            "xmrigDonatePercent": xmrig::XMRIG_DONATE_LEVEL,
        }));
        return Ok(());
    }
    if !accept && !revoke {
        print_terms();
    }
    println!(
        "Status: {}",
        if c.onboarding_completed {
            "accepted. Revoke with: erebor consent --revoke"
        } else {
            "not accepted. Accept with: erebor consent --accept"
        }
    );
    Ok(())
}

/// Fees and conditions, shown before any real mining (consent and onboarding).
fn print_terms() {
    let pct = devfee::DEFAULT_FEE_PERCENT;
    let fee_wallet = devfee::build_wallet().is_some() || devfee::build_btc_wallet().is_some();
    {
        println!("Erebor — fees and terms for real mining\n");
        println!("  • Erebor dev fee: {pct}% of effective mining time is mined for the Erebor");
        println!(
            "    project, in short slots you can see live (`erebor status`, Dev fee: now ...)."
        );
        if !fee_wallet {
            println!("    This build has no project wallet configured: the fee is inactive.");
        }
        println!(
            "  • XMRig donation: {}% to XMRig's developers (built into the miner).",
            xmrig::XMRIG_DONATE_LEVEL
        );
        println!("  • Pool fee: set by the pool (`erebor pools list`).");
        println!("  • Non-custodial: the pool pays your wallet directly. Erebor never asks for");
        println!("    seed phrases or private keys and never moves funds.");
        println!("  • Mining uses CPU/GPU and electricity. It starts only when you ask, and");
        println!("    stops when you stop it (or the agent).\n");
    }
}

/// Download, verify and install the pinned miner through the engine; wait for it.
fn install_wait(dir: &Path, kind: MinerKind, progress: bool) -> Result<(), Fail> {
    let kind_s = kind.dir_prefix();
    call(dir, "install_miner", json!({ "kind": kind_s }))?;
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let s = ipc::snapshot(dir)?;
        let Some(j) = s.job.filter(|j| j.kind == "install") else {
            continue;
        };
        if progress {
            eprint!(
                "\rInstalling {}: {:>3.0}%",
                kind.label(),
                j.progress * 100.0
            );
        }
        if j.done {
            if progress {
                eprintln!();
            }
            return match j.error {
                Some(e) => Err(e.into()),
                None => Ok(()),
            };
        }
    }
}

fn miner_install(dir: &Path, kind: Kind, json: bool) -> Result<(), Fail> {
    let mk = match kind {
        Kind::Xmrig => MinerKind::Xmrig,
        Kind::Srbminer => MinerKind::Srbminer,
    };
    install_wait(dir, mk, !json)?;
    let c = load_config(dir)?;
    let mc = c.miner_for(mk);
    if json {
        print_json(mc);
    } else {
        println!(
            "Installed {} {} (SHA-256 verified) at {}",
            mk.label(),
            mc.version.clone().unwrap_or_default(),
            mc.path.clone().unwrap_or_default()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn command_line_parses() {
        Cli::command().debug_assert();
        let p =
            |a: &[&str]| Cli::try_parse_from(std::iter::once("erebor").chain(a.iter().copied()));
        assert!(matches!(
            p(&["status", "--json"]).unwrap(),
            Cli {
                json: true,
                cmd: Some(Cmd::Status),
                ..
            }
        ));
        assert!(matches!(
            p(&["--json", "hardware", "list"]).unwrap().cmd.unwrap(),
            Cmd::Hardware {
                cmd: HardwareCmd::List
            }
        ));
        assert!(matches!(
            p(&["mining", "pause"]).unwrap().cmd.unwrap(),
            Cmd::Mining {
                action: MiningCmd::Pause
            }
        ));
        assert!(matches!(
            p(&["agent", "start", "--foreground"]).unwrap().cmd.unwrap(),
            Cmd::Agent {
                cmd: AgentCmd::Start { foreground: true }
            }
        ));
        let c = p(&[
            "wallet",
            "set",
            "krxABC",
            "--coin",
            "prl",
            "--data-dir",
            "/x",
        ])
        .unwrap();
        assert_eq!(c.data_dir, Some(PathBuf::from("/x")));
        assert!(matches!(
            c.cmd.unwrap(),
            Cmd::Wallet {
                cmd: WalletCmd::Set { coin: Some(_), .. }
            }
        ));
        assert!(matches!(
            p(&["miner", "install"]).unwrap().cmd.unwrap(),
            Cmd::Miner {
                cmd: MinerCmd::Install { kind: Kind::Xmrig }
            }
        ));
        assert!(matches!(
            p(&["service", "on"]).unwrap().cmd.unwrap(),
            Cmd::Service {
                state: Some(OnOff::On)
            }
        ));
        assert!(matches!(
            p(&["service"]).unwrap().cmd.unwrap(),
            Cmd::Service { state: None }
        ));
        assert!(matches!(
            p(&["dashboard"]).unwrap().cmd.unwrap(),
            Cmd::Monitor
        ));
        assert!(matches!(p(&["start"]).unwrap().cmd.unwrap(), Cmd::Start));
        assert!(p(&["mining"]).is_err());
        assert!(p(&[]).unwrap().cmd.is_none());
        assert!(p(&["demo", "maybe"]).is_err());
        assert!(p(&["consent", "--accept", "--revoke"]).is_err());
    }

    #[test]
    fn error_codes_have_hints() {
        for code in [
            "agent_unreachable",
            "consent_required",
            "wallet_missing",
            "miner_not_configured",
            "engine_busy: x",
        ] {
            assert!(explain(code).1.is_some(), "{code}");
        }
        assert_eq!(explain("weird").0, "weird");
    }
}

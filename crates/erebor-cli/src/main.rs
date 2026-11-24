//! `erebor`: the command-line interface. Every command that reads or changes mining
//! state goes to the process that owns the engine (agent, desktop app or node) over
//! the local socket, so the CLI, the TUI and the GUI always show the same state.
//!
//! Exit codes: 0 ok · 1 command failed · 2 usage error · 3 agent not reachable.

mod doctor;
mod onboard;
mod ui;

use anstream::{eprint, eprintln, println};

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

#[derive(Parser, Debug)]
#[command(
    name = "erebor",
    version,
    about = "Erebor mining manager — terminal interface",
    styles = ui::HELP,
    after_help = ui::after_help(),
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
    /// Live dashboard: hashrate, earnings, temperatures, controls.
    #[command(visible_alias = "dashboard")]
    Monitor,
    /// How much of the CPU mining uses: eco (25%), balanced (50%, default) or max (100%).
    Power { level: Option<onboard::Perf> },
    /// Payout wallet (public address only).
    Wallet {
        #[command(subcommand)]
        cmd: WalletCmd,
    },
    /// Mine always: start Erebor and mining with the computer (on), or stop that (off).
    Service { state: Option<OnOff> },
    /// Install the latest signed release of this program.
    Update,
    /// Check OS, hardware, miners, GPU, wallet, pool and agent.
    Doctor {
        /// Skip the pool connectivity test.
        #[arg(long)]
        offline: bool,
    },
    /// Guided setup: every option, with defaults; reviewed before anything changes.
    Onboard(onboard::Opts),
    /// Every setting, by key: `config`, `config get <key>`, `config set <key> <value>`.
    Config {
        #[command(subcommand)]
        cmd: Option<ConfigCmd>,
    },
    /// Coins this computer can mine, with hashrate and estimated earnings.
    Coins {
        #[command(subcommand)]
        cmd: Option<CoinsCmd>,
    },
    /// Mining history as CSV on stdout (time, coin, hashrate, estimated amounts, energy and
    /// its cost): `erebor export --days 365 > erebor.csv`.
    Export {
        /// How many days back (default 30).
        #[arg(long, default_value_t = 30)]
        days: u32,
    },
    /// The engine log (the last lines kept in memory).
    Logs {
        /// Keep printing new lines.
        #[arg(short, long)]
        follow: bool,
    },
    /// Start, stop, pause, resume or restart mining.
    Mining {
        #[command(subcommand)]
        action: MiningCmd,
    },
    /// Detected CPU, GPUs and sensors.
    Hardware {
        #[command(subcommand)]
        cmd: HardwareCmd,
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
    /// Miner programs (pinned releases, SHA-256 verified).
    Miner {
        #[command(subcommand)]
        cmd: MinerCmd,
    },
    /// This computer's name and id.
    Device {
        #[command(subcommand)]
        cmd: DeviceCmd,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigCmd {
    /// All settings as `key = value` (the default).
    Show,
    /// One setting (a section prints all its keys).
    Get { key: String },
    /// Change one setting; the engine validates it. VALUE is JSON (`true`, `0.25`,
    /// `null`, `[..]`) or plain text.
    Set { key: String, value: String },
}

#[derive(Subcommand, Debug)]
enum CoinsCmd {
    /// Every coin, its algorithm, hashrate here and estimated earnings (the default).
    List,
    /// Mine this coin (`auto`: the most profitable, switching by itself).
    Set { coin: String },
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
    Lolminer,
    Cpuminer,
}

#[derive(Subcommand, Debug)]
enum DeviceCmd {
    /// Name, id, OS and CPU of this device.
    Show,
    /// Set this computer's name (default: the hostname).
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
        Cmd::Start => mining(dir, MiningCmd::Start, json),
        Cmd::Service { state } => service(dir, state, json),
        Cmd::Power { level } => power(dir, level, json),
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
            | Cmd::Mining {
                action: MiningCmd::Start | MiningCmd::Restart
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

fn status(dir: &Path, json: bool) -> Result<(), Fail> {
    let s = ipc::snapshot(dir)?;
    if json {
        print_json(&s);
        return Ok(());
    }
    let info = ipc::call(dir, "agent_info", &Value::Null).unwrap_or(Value::Null);
    let mut state = ui::paint(format!("● {}", fmt::state(&s.state)), ui::state(&s.state));
    if s.demo {
        state += &ui::paint("  DEMO · simulated, nothing is mined", ui::WARN);
    }
    row("State", state);
    if let Some(e) = &s.error {
        row("Error", ui::paint(explain(e).0, ui::ERR));
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
    row("Hashrate", ui::paint(fmt::hashrate(s.hashrate), ui::BRAND));
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
        format!(
            "{} · {}",
            ui::paint(format!("{} accepted", s.accepted), ui::OK),
            ui::paint(
                format!("{} rejected", s.rejected),
                if s.rejected > 0 { ui::WARN } else { ui::DIM }
            )
        ),
    );
    row("Temperature", fmt::temp(s.cpu_temp_c));
    for g in &s.gpus {
        row("", format!("{}: {}", g.name, fmt::temp(g.temp_c)));
    }
    row("Power", fmt::power(s.power_w, &s.power_source));
    row("Earnings", ui::paint(fmt::earnings(&s), ui::BRAND));
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
            format!(
                "{}{}",
                ui::paint(m, ui::WARN),
                h.map(|h| format!(" → {}", ui::paint(h, ui::CMD)))
                    .unwrap_or_default()
            ),
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
                println!(
                    "{}",
                    ui::paint("Nothing to do: mining is not running.", ui::DIM)
                );
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
        println!(
            "{}",
            ui::paint(
                format!("Nothing to do: mining is already {}.", s.state),
                ui::DIM
            )
        );
    } else {
        let verb = match action {
            Action::Start => "started",
            Action::Stop => "stopped",
            Action::Pause => "paused",
            Action::Resume => "resumed",
            Action::Restart => "restarted",
        };
        println!(
            "{} State: {}.",
            ui::ok(format!("Mining {verb}{demo}.")),
            ui::paint(fmt::state(&s.state), ui::state(&s.state))
        );
        if matches!(action, Action::Start | Action::Restart | Action::Resume) {
            println!(
                "{} {}   {} {}",
                ui::paint("Watch it live:", ui::DIM),
                ui::paint("erebor", ui::CMD),
                ui::paint("· stop:", ui::DIM),
                ui::paint("erebor stop", ui::CMD)
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
                // Own wallet for one coin (pool: `config set coinPools.<COIN>…`; unset: the first listed, Kryptex if there).
                Some(coin) => {
                    let coin = coin.to_ascii_uppercase();
                    erebor_core::coins::find(&coin)
                        .ok_or_else(|| Fail::from("unknown_coin".to_string()))?;
                    wallet::validate_coin_address(&coin, &address)
                        .map_err(|e| Fail::from(format!("invalid_address: {e:?}")))?;
                    c.coin_wallets.insert(coin, address.clone());
                }
                None => {
                    wallet::validate_user_payout(&address)
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
            println!("{}", ui::ok("Wallet saved and validated."));
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
                println!(
                    "Mining is not started automatically: {}",
                    ui::paint("erebor start", ui::CMD)
                );
            }
            Ok(())
        }
        AgentCmd::Stop => {
            info.map_err(Fail::from)?;
            stop_agent(dir)?;
            if json {
                print_json(&json!({"stopped": true}));
            } else {
                println!("{}", ui::ok("Agent stopped (mining stopped with it)."));
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

/// `a.b.c` in a JSON object; `None` when a key does not exist.
fn at<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    key.split('.').try_fold(v, |v, k| v.get(k))
}

/// `key = value` lines for every leaf under `v`.
fn flatten(prefix: &str, v: &Value, out: &mut Vec<(String, String)>) {
    match v {
        Value::Object(m) => {
            for (k, v) in m {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten(&key, v, out);
            }
        }
        Value::String(s) => out.push((prefix.into(), s.clone())),
        v => out.push((prefix.into(), v.to_string())),
    }
}

/// The new value for `key`: JSON when it parses, else text. Text stays text for
/// string settings ("1234" as a worker name).
fn parse_value(current: &Value, raw: &str) -> Value {
    match (current, serde_json::from_str::<Value>(raw)) {
        (Value::String(_), Ok(Value::String(s))) => Value::String(s),
        (Value::String(_), _) => Value::String(raw.into()),
        (_, Ok(v)) => v,
        (_, Err(_)) => Value::String(raw.into()),
    }
}

fn set_key(cfg: &mut Value, key: &str, raw: &str) -> Result<(), Fail> {
    let slot = key
        .split('.')
        .try_fold(cfg, |v, k| v.get_mut(k))
        .ok_or_else(|| Fail::from(format!("unknown_key: {key}")))?;
    if slot.is_object() {
        return Err(format!("not_a_setting: {key} is a section").into());
    }
    *slot = parse_value(slot, raw);
    Ok(())
}

fn config_cmd(dir: &Path, cmd: ConfigCmd, json: bool) -> Result<(), Fail> {
    let to_value = |c: &AppConfig| serde_json::to_value(c).unwrap_or_default();
    let print = |v: &Value, prefix: &str| {
        if json {
            print_json(v);
            return;
        }
        let mut rows = vec![];
        flatten(prefix, v, &mut rows);
        let w = rows.iter().map(|r| r.0.len()).max().unwrap_or(0);
        for (k, v) in rows {
            println!("{}  {v}", ui::label(&k, w));
        }
    };
    match cmd {
        ConfigCmd::Show => print(&to_value(&load_config(dir)?), ""),
        ConfigCmd::Get { key } => {
            let c = to_value(&load_config(dir)?);
            let v = at(&c, &key).ok_or_else(|| Fail::from(format!("unknown_key: {key}")))?;
            match v {
                Value::Object(_) => print(v, &key),
                Value::String(s) if !json => println!("{s}"),
                v if !json => println!("{v}"),
                v => print_json(v),
            }
        }
        ConfigCmd::Set { key, value } => {
            let mut c = to_value(&load_config_live(dir)?);
            set_key(&mut c, &key, &value)?;
            let wanted = at(&c, &key).cloned().unwrap_or_default();
            let new: AppConfig = serde_json::from_value(c)
                .map_err(|e| Fail::from(format!("invalid_value: {key}: {e}")))?;
            let saved = to_value(&save_config(dir, &new)?);
            let got = at(&saved, &key).cloned().unwrap_or_default();
            if json {
                print_json(&json!({ "key": key, "value": got }));
            } else if got == wanted {
                println!("{}", ui::ok(format!("{key} = {}", display(&got))));
            } else {
                // The engine owns some keys (miner paths, version) or adjusts a value.
                println!(
                    "{}",
                    ui::warn(format!("{key} kept by Erebor: {}", display(&got)))
                );
            }
        }
    }
    Ok(())
}

fn display(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        v => v.to_string(),
    }
}

fn coins_cmd(dir: &Path, cmd: CoinsCmd, json: bool) -> Result<(), Fail> {
    if let CoinsCmd::Set { coin } = cmd {
        let id = coin.trim().to_uppercase();
        if id != AUTO_COIN && erebor_core::coins::find(&id).is_none() {
            return fail("unknown_coin");
        }
        let mut c = load_config_live(dir)?;
        c.coin = id;
        let saved = save_config(dir, &c)?;
        if json {
            print_json(&json!({ "coin": saved.coin }));
        } else if saved.coin == AUTO_COIN && c.coin != AUTO_COIN {
            println!(
                "{}",
                ui::warn("This payout cannot mine that coin: kept automatic.")
            );
        } else {
            println!(
                "{}",
                ui::ok(format!(
                    "Coin set: {}. Running mining switches over now.",
                    if saved.coin == AUTO_COIN {
                        "auto"
                    } else {
                        saved.coin.as_str()
                    }
                ))
            );
            if ipc::snapshot(dir).is_ok_and(|s| s.job.is_some_and(|j| !j.done)) {
                println!("{}", ui::paint("Installing its miner first…", ui::DIM));
            }
        }
        return Ok(());
    }
    let opts = call(dir, "coin_options", Value::Null)?;
    let current = load_config(dir)?.coin;
    if json {
        print_json(&json!({ "coin": current, "options": opts }));
        return Ok(());
    }
    let currency = load_config(dir)?.energy.currency;
    println!(
        "{}",
        ui::paint(
            format!(
                "  {:<10}{:<18}{:<14}{:<5}{:>12}{:>12}  NOTE",
                "COIN",
                "NAME",
                "ALGO",
                "DEV",
                "HASHRATE",
                format!("{currency}/DAY")
            ),
            ui::DIM
        )
    );
    // Mineable first, best estimate first.
    let mut opts = opts.as_array().cloned().unwrap_or_default();
    opts.sort_by(|a, b| {
        let v = |o: &Value| o["valuePerDay"].as_f64().unwrap_or(-1.0);
        a["blocker"]
            .is_null()
            .cmp(&b["blocker"].is_null())
            .reverse()
            .then(v(b).total_cmp(&v(a)))
    });
    for o in &opts {
        let c = &o["coin"];
        let id = c["id"].as_str().unwrap_or("?");
        let mark = if id == current { "*" } else { " " };
        let note = match o["blocker"].as_str() {
            Some(b) => explain(b).0,
            None if c["asic"] == true => "ASIC coin: earns little".into(),
            None if o["minerInstalled"] == false => "miner installed on first use".into(),
            None => String::new(),
        };
        let line = format!(
            "{mark} {:<10}{:<18}{:<14}{:<5}{:>12}{:>12}  {note}",
            id,
            c["name"].as_str().unwrap_or(""),
            c["algo"].as_str().unwrap_or(""),
            c["device"].as_str().unwrap_or("").to_uppercase(),
            o["hashrate"]
                .as_f64()
                .map_or("-".into(), |h| fmt::hashrate(Some(h))),
            o["valuePerDay"]
                .as_f64()
                .map_or("-".into(), |v| format!("{v:.3}")),
        );
        if o["blocker"].is_null() {
            println!("{line}");
        } else {
            println!("{}", ui::paint(line, ui::DIM));
        }
    }
    println!(
        "\n{}",
        ui::paint(
            format!(
                "* = selected ({}). Hashrate is measured while mining each coin. Change: erebor coins set <COIN|auto>",
                if current == AUTO_COIN { "auto" } else { current.as_str() }
            ),
            ui::DIM
        )
    );
    Ok(())
}

/// CSV from the running engine, or straight from the database when none runs.
fn export(dir: &Path, range: i64) -> Result<(), Fail> {
    let csv = match ipc::call(dir, "export_csv", &json!({ "rangeSecs": range })) {
        Ok(v) => v.as_str().unwrap_or_default().to_string(),
        Err(e) if e == "agent_unreachable" => {
            let demo = AppConfig::load(&dir.join("config.json")).demo_mode;
            let db = dir.join(if demo { "erebor-demo.db" } else { "erebor.db" });
            let to = erebor_core::now_unix() + 1;
            erebor_core::store::Store::open(&db)
                .and_then(|s| s.export_csv(to - range, to))
                .map_err(|e| Fail::from(e.to_string()))?
        }
        Err(e) => return Err(e.into()),
    };
    print!("{csv}");
    Ok(())
}

fn logs(dir: &Path, follow: bool, json: bool) -> Result<(), Fail> {
    let get = || -> Result<Vec<String>, Fail> {
        Ok(call(dir, "get_logs", Value::Null)?
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect())
    };
    let mut seen = get()?;
    if json && !follow {
        print_json(&seen);
        return Ok(());
    }
    for l in &seen {
        println!("{l}");
    }
    if !follow {
        return Ok(());
    }
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let now = get()?;
        // The log is a ring buffer: print what follows the last line already shown.
        let new = match seen.last().and_then(|l| now.iter().rposition(|x| x == l)) {
            Some(i) => &now[i + 1..],
            None => &now[..],
        };
        for l in new {
            println!("{l}");
        }
        seen = now;
    }
}

/// `erebor power [eco|balanced|max]`: show or change how much CPU mining uses.
fn power(dir: &Path, level: Option<onboard::Perf>, json: bool) -> Result<(), Fail> {
    use erebor_core::config::Preset;
    let describe = |p: &erebor_core::config::Performance| {
        let (threads, _) = p.limits();
        let name = match p.preset {
            Preset::Eco => "eco",
            Preset::Balanced => "balanced",
            Preset::Performance => "max",
            Preset::Custom => "custom",
        };
        (name, threads)
    };
    let c = match level {
        None => load_config(dir)?,
        Some(l) => {
            let mut c = load_config_live(dir)?;
            c.performance.preset = l.into();
            save_config(dir, &c)?
        }
    };
    let (name, threads) = describe(&c.performance);
    if json {
        print_json(&json!({"power": name, "threadsPercent": threads}));
        return Ok(());
    }
    match level {
        None => {
            println!(
                "{} {} {}",
                ui::label("Power", 7),
                ui::paint(name, ui::BRAND),
                ui::paint(format!("({threads}% of CPU threads)"), ui::DIM)
            );
            println!(
                "Change it: {}",
                ui::paint("erebor power eco | balanced | max", ui::CMD)
            );
        }
        Some(_) => {
            println!(
                "{}",
                ui::ok(format!("Power set to {name}: {threads}% of CPU threads."))
            );
            if ipc::snapshot(dir).is_ok_and(|s| s.is_active()) {
                println!("{}", ui::paint("Mining restarted to apply it.", ui::DIM));
            }
            if c.performance.preset == Preset::Performance {
                println!(
                    "{}",
                    ui::warn(format!(
                        "The computer may get slower, louder and hotter. Mining stops by itself above {:.0} °C.",
                        c.safety.max_temp_c
                    ))
                );
            }
        }
    }
    Ok(())
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
            println!(
                "{}",
                ui::ok("Erebor now starts with the computer and mines automatically.")
            );
            match linger {
                Some(true) | None => {}
                Some(false) => println!(
                    "! On Linux it starts at login. To start right at boot, before anyone logs in:\n    sudo loginctl enable-linger \"$USER\""
                ),
            }
            if let Ok(s) = ipc::snapshot(dir) {
                if let (Some(b), false) = (&s.start_blocker, s.is_active()) {
                    let (m, h) = explain(b);
                    println!("{}", ui::warn(format!("Mining cannot start yet: {m}")));
                    if let Some(h) = h {
                        println!("  → {}", ui::paint(h, ui::CMD));
                    }
                }
            }
            println!("Turn it off: {}", ui::paint("erebor service off", ui::CMD));
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
                println!(
                    "{}",
                    ui::ok("Erebor no longer starts with the computer; background mining stopped.")
                );
                println!(
                    "Start by hand any time: {}",
                    ui::paint("erebor start", ui::CMD)
                );
            }
            Ok(())
        }
        None => {
            let on = installed();
            let start = load_config(dir)?.start_on_launch;
            if json {
                print_json(&json!({"service": on, "startOnLaunch": start}));
            } else if on && start {
                println!(
                    "{} Erebor starts with the computer and mines automatically.\nTurn off: {}",
                    ui::paint("● On:", ui::OK),
                    ui::paint("erebor service off", ui::CMD)
                );
            } else if on {
                println!(
                    "{} Erebor starts with the computer but does not mine by itself.\nMine always: {}",
                    ui::paint("◐", ui::WARN),
                    ui::paint("erebor service on", ui::CMD)
                );
            } else {
                println!(
                    "{} Erebor does not start with the computer.\nMine always, also after a restart: {}",
                    ui::paint("○ Off:", ui::DIM),
                    ui::paint("erebor service on", ui::CMD)
                );
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
        eprintln!("{}", ui::paint("Checking for updates…", ui::DIM));
    }
    let installed = erebor_core::update::update(&exe)?;
    if json {
        print_json(&json!({"updated": installed, "current": env!("CARGO_PKG_VERSION")}));
        return Ok(());
    }
    match installed {
        None => println!(
            "{}",
            ui::ok(format!(
                "Erebor is up to date (v{}).",
                env!("CARGO_PKG_VERSION")
            ))
        ),
        Some(v) => {
            println!(
                "{}",
                ui::ok(format!("Updated to v{v} (signature verified)."))
            );
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
                "\r  {} Installing {}  {:>3.0}%",
                ui::paint("◇", ui::BRAND),
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
        Kind::Lolminer => MinerKind::Lolminer,
        Kind::Cpuminer => MinerKind::Cpuminer,
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
        assert!(matches!(
            p(&["power", "max"]).unwrap().cmd.unwrap(),
            Cmd::Power {
                level: Some(onboard::Perf::Performance)
            }
        ));
        assert!(p(&["power", "turbo"]).is_err());
        assert!(p(&["mining"]).is_err());
        assert!(p(&[]).unwrap().cmd.is_none());
        assert!(p(&["demo", "maybe"]).is_err());
    }

    #[test]
    fn config_keys() {
        let mut c = serde_json::to_value(AppConfig::default()).unwrap();
        set_key(&mut c, "pool.worker", "1234").unwrap();
        set_key(&mut c, "energy.pricePerKwh", "0.25").unwrap();
        set_key(&mut c, "safety.autoRestart", "true").unwrap();
        set_key(&mut c, "performance.preset", "custom").unwrap();
        assert_eq!(
            set_key(&mut c, "pool.nope", "1").unwrap_err().code,
            "unknown_key: pool.nope"
        );
        assert!(set_key(&mut c, "pool", "x").is_err());
        let a: AppConfig = serde_json::from_value(c.clone()).unwrap();
        assert_eq!(a.pool.worker, "1234");
        assert_eq!(a.energy.price_per_kwh, Some(0.25));
        assert!(a.safety.auto_restart);
        assert_eq!(a.performance.preset, erebor_core::config::Preset::Custom);
        assert_eq!(at(&c, "pool.worker"), Some(&json!("1234")));
        // Wrong type: refused before reaching the engine.
        set_key(&mut c, "safety.maxTempC", "hot").unwrap();
        assert!(serde_json::from_value::<AppConfig>(c).is_err());
        let mut rows = vec![];
        flatten(
            "",
            &serde_json::to_value(AppConfig::default()).unwrap(),
            &mut rows,
        );
        assert!(rows
            .iter()
            .any(|(k, v)| k == "safety.maxTempC" && v == "85.0"));
    }

    #[test]
    fn error_codes_have_hints() {
        for code in [
            "agent_unreachable",
            "wallet_missing",
            "miner_not_configured",
            "engine_busy: x",
        ] {
            assert!(explain(code).1.is_some(), "{code}");
        }
        assert_eq!(explain("weird").0, "weird");
    }
}

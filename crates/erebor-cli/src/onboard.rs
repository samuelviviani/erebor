//! `erebor onboard`: guided setup. Every option is shown, each with a sensible default
//! (Enter keeps it): whoever installs the terminal version knows what they are doing.
//!
//! Every answer is collected first and reviewed; nothing changes until the user confirms,
//! so leaving half-way (Ctrl-C) changes nothing. Changes go through the engine
//! (`update_config`), with the same validation as the desktop app.

use crate::ui::{self, paint};
use crate::{doctor, Fail};
use anstream::println;
use clap::{Args, ValueEnum};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, Input, Select};
use erebor_core::coins::{self, Device};
use erebor_core::config::{self, AppConfig, Energy, Performance, PoolConfig, Preset, Safety};
use erebor_core::hardware::Monitor;
use erebor_core::ipc::{self, Action};
use erebor_core::miner::MinerKind;
use erebor_core::{autostart, install, pools, wallet};
use serde_json::{json, Value};
use std::io::IsTerminal;
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Args, Debug, Default, Clone)]
pub struct Opts {
    /// No prompts (scripts, provisioning). Fine-tune afterwards with `erebor config set`.
    #[arg(long)]
    pub non_interactive: bool,
    #[arg(long, value_enum)]
    pub mode: Option<Mode>,
    /// Payout: your Kryptex ID (krx…).
    #[arg(long, value_name = "KRYPTEX_ID")]
    pub wallet: Option<String>,
    #[arg(long, value_name = "POOL_ID")]
    pub pool: Option<String>,
    /// Coin id (see `erebor coins`) or `auto`.
    #[arg(long, value_name = "COIN")]
    pub coin: Option<String>,
    /// Worker name shown by the pool (default: this computer's hostname).
    #[arg(long, value_name = "NAME")]
    pub worker: Option<String>,
    #[arg(long, value_enum)]
    pub performance: Option<Perf>,
    /// Do not download the miner now.
    #[arg(long)]
    pub no_install_miner: bool,
    /// Mine always: Erebor and mining start with the computer (systemd user unit /
    /// LaunchAgent, `startOnLaunch`).
    #[arg(long)]
    pub install_service: bool,
    /// Start mining at the end of the setup.
    #[arg(long)]
    pub start: bool,
    /// Do not leave the agent running after the setup.
    #[arg(long)]
    pub no_background: bool,
    /// Skip the final health check (it tests the pool over the network).
    #[arg(long)]
    pub skip_health: bool,
    /// Start from default settings instead of the current ones (statistics are kept).
    #[arg(long)]
    pub reset: bool,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    /// Real mining.
    #[value(alias = "quickstart", alias = "custom")]
    Mining,
    /// Simulated miner: nothing is mined, nothing goes on the network.
    Demo,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq)]
pub enum Perf {
    Eco,
    Balanced,
    #[value(alias = "max")]
    Performance,
}

impl From<Perf> for Preset {
    fn from(p: Perf) -> Self {
        match p {
            Perf::Eco => Preset::Eco,
            Perf::Balanced => Preset::Balanced,
            Perf::Performance => Preset::Performance,
        }
    }
}

/// What the setup will do, decided before anything changes.
#[derive(Debug, Clone, PartialEq, Default)]
struct Plan {
    mode: Option<Mode>,
    reset: bool,
    wallet: Option<String>,
    pool: Option<String>,
    /// "AUTO" or a coin id; set after the miners are installed.
    coin: Option<String>,
    worker: Option<String>,
    performance: Option<Performance>,
    energy: Option<Energy>,
    safety: Option<Safety>,
    install: Vec<MinerKind>,
    background: bool,
    service: bool,
    start: bool,
    monitor: bool,
    health: bool,
}

/// No configuration yet, or neither demo nor terms accepted.
pub fn needs_setup(dir: &Path) -> bool {
    let p = dir.join("config.json");
    if !p.exists() {
        return true;
    }
    let c = AppConfig::load(&p);
    !c.onboarding_completed && !c.demo_mode
}

pub fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

fn installed(c: &AppConfig, k: MinerKind) -> bool {
    c.miner_for(k)
        .path
        .as_deref()
        .is_some_and(|p| Path::new(p).is_file())
}

fn available(k: MinerKind) -> bool {
    install::manifest_for(k).asset_for_platform().is_ok()
}

fn check_wallet(w: &str) -> Result<String, String> {
    match wallet::validate_user_payout(w.trim()) {
        Ok(i) => Ok(format!("{:?}", i.kind)),
        Err(wallet::WalletError::LooksLikeSecret) => Err(
            "this looks like a seed phrase or private key: never share it. Use your public address."
                .into(),
        ),
        Err(_) => Err("enter your Kryptex ID (krx…): for now Erebor pays through Kryptex only".into()),
    }
}

fn check_worker(w: &str) -> Result<(), &'static str> {
    if w.eq_ignore_ascii_case(config::PROJECT_WORKER) {
        Err("EREBOR is reserved for Erebor's 2%")
    } else if config::valid_worker(w) {
        Ok(())
    } else {
        Err("letters, digits, '_' and '-' only, max 32")
    }
}

/// "AUTO" or a known coin id, upper case.
fn check_coin(id: &str) -> Result<String, Fail> {
    let id = id.trim().to_uppercase();
    if id == config::AUTO_COIN || coins::find(&id).is_some() {
        Ok(id)
    } else {
        Err("unknown_coin".to_string().into())
    }
}

/// Coins this computer can mine: CPU coins with a build for this platform, GPU coins
/// with a supported GPU.
fn mineable(c: &AppConfig, gpu: bool) -> Vec<&'static coins::Coin> {
    coins::COINS
        .iter()
        .filter(|k| match k.device {
            Device::Gpu => gpu,
            Device::Cpu => available(k.miner) || installed(c, k.miner),
        })
        .collect()
}

pub fn run(dir: &Path, o: Opts, json: bool) -> Result<(), Fail> {
    let current = crate::load_config(dir)?;
    let plan = if o.non_interactive || !interactive() {
        if !o.non_interactive {
            return Err("onboarding_needs_terminal".to_string().into());
        }
        from_flags(&o, &current)?
    } else {
        match ask(&o, &current)? {
            Some(p) => p,
            None => return summary(dir, None, json),
        }
    };
    apply(dir, &plan, json)
}

fn from_flags(o: &Opts, c: &AppConfig) -> Result<Plan, Fail> {
    let mode = o.mode.unwrap_or(Mode::Mining);
    let mut p = Plan {
        mode: Some(mode),
        reset: o.reset,
        background: !o.no_background || o.start || o.install_service,
        service: o.install_service,
        start: o.start,
        health: !o.skip_health,
        ..Default::default()
    };
    if mode == Mode::Demo {
        return Ok(p);
    }
    let w = o.wallet.clone().unwrap_or_else(|| c.wallet.clone());
    if w.trim().is_empty() {
        return Err("wallet_missing".to_string().into());
    }
    check_wallet(&w).map_err(|_| Fail::from("invalid_address".to_string()))?;
    if let Some(id) = &o.pool {
        pools::find(id).ok_or_else(|| Fail::from("unknown_pool".to_string()))?;
    }
    if let Some(w) = &o.worker {
        check_worker(w).map_err(|_| Fail::from("invalid_worker".to_string()))?;
    }
    p.wallet = Some(w.trim().into());
    p.pool = o.pool.clone();
    p.worker = o.worker.clone();
    p.coin = o.coin.as_deref().map(check_coin).transpose()?;
    p.performance = o.performance.map(|l| Performance {
        preset: l.into(),
        ..c.performance.clone()
    });
    if !o.no_install_miner {
        let gpu = Monitor::new().detect().gpu_mining_supported;
        let mut want = vec![MinerKind::Xmrig];
        if gpu {
            want.extend([MinerKind::Srbminer, MinerKind::Lolminer]);
        }
        if let Some(k) = p.coin.as_deref().and_then(coins::find) {
            want.push(k.miner);
        }
        for k in want {
            if !installed(c, k) && available(k) && !p.install.contains(&k) {
                p.install.push(k);
            }
        }
    }
    Ok(p)
}

fn cancelled(_: dialoguer::Error) -> Fail {
    Fail::from("onboarding_cancelled".to_string())
}

/// Prompts indented like the rest of the setup.
fn theme() -> ColorfulTheme {
    use dialoguer::console::style;
    ColorfulTheme {
        prompt_prefix: style("  ?".to_string()).for_stderr().yellow(),
        success_prefix: style("  ✓".to_string()).for_stderr().green(),
        error_prefix: style("  ✗".to_string()).for_stderr().red(),
        active_item_prefix: style("    ›".to_string()).for_stderr().yellow(),
        inactive_item_prefix: style("     ".to_string()).for_stderr(),
        ..ColorfulTheme::default()
    }
}

/// A section title with a blank line around it, then its notes.
fn section(title: &str, notes: &[&str]) {
    println!("\n\n  {}", paint(title, ui::BRAND));
    for n in notes {
        println!("  {}", paint(n, ui::DIM));
    }
    println!();
}

/// The interactive questions. `None`: keep the current setup.
fn ask(o: &Opts, current: &AppConfig) -> Result<Option<Plan>, Fail> {
    let t = theme();
    let select = |prompt: &str, items: &[String], default: usize| {
        Select::with_theme(&t)
            .with_prompt(prompt)
            .items(items)
            .default(default)
            .max_length(12)
            .interact()
            .map_err(cancelled)
    };
    let confirm = |prompt: &str, default: bool| {
        Confirm::with_theme(&t)
            .with_prompt(prompt)
            .default(default)
            .interact()
            .map_err(cancelled)
    };
    let items = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();

    println!(
        "\n  {}  {}",
        paint("▲ EREBOR", ui::BRAND),
        paint(format!("setup · v{}", env!("CARGO_PKG_VERSION")), ui::DIM)
    );
    println!();
    println!("  Enter keeps the value in brackets. Ctrl-C leaves without changing anything.");
    println!("  You review everything at the end; nothing is mined until you start it.");

    let configured = current.onboarding_completed || current.demo_mode;
    let mut reset = o.reset;
    if configured && !reset {
        let what = if current.demo_mode {
            "demo mode".to_string()
        } else {
            format!(
                "payout {} · pool {}",
                short(&current.wallet),
                current.pool.id
            )
        };
        section(
            "Current setup",
            &[&format!("Erebor is already set up: {what}.")],
        );
        match select(
            "What do you want to do",
            &items(&[
                "Keep it",
                "Change it",
                "Start over from defaults (statistics are kept)",
            ]),
            0,
        )? {
            0 => return Ok(None),
            2 => reset = true,
            _ => {}
        }
    }
    let base = if reset {
        AppConfig::default()
    } else {
        current.clone()
    };

    let mode = match o.mode {
        Some(m) => m,
        None => {
            section("1 · Mode", &[]);
            match select(
                "Mode",
                &items(&[
                    "Real mining",
                    "Demo: simulated miner, nothing is mined, no network",
                ]),
                0,
            )? {
                0 => Mode::Mining,
                _ => Mode::Demo,
            }
        }
    };
    let mut p = Plan {
        mode: Some(mode),
        reset,
        health: !o.skip_health,
        ..Default::default()
    };

    if mode == Mode::Demo {
        section("2 · Demo", &[]);
        p.background = true;
        p.start = confirm("Start the simulated miner now", true)?;
        p.monitor = confirm("Open the dashboard at the end", true)?;
        return review(p, &base).map(Some);
    }

    // Payout.
    section(
        "2 · Payout",
        &[
            "Earnings go to your Kryptex account and are paid in BTC.",
            "Your Kryptex ID (krx…) is on kryptex.com once you sign up.",
        ],
    );
    let mut input = Input::<String>::with_theme(&t).with_prompt("Kryptex ID");
    let prefill = o.wallet.clone().unwrap_or_else(|| base.wallet.clone());
    if !prefill.is_empty() {
        input = input.default(prefill);
    }
    let w = input
        .validate_with(|s: &String| check_wallet(s).map(|_| ()))
        .interact_text()
        .map_err(cancelled)?;
    p.wallet = Some(w.trim().into());

    // What and how to mine.
    let gpu = Monitor::new().detect().gpu_mining_supported;
    section(
        "3 · Mining",
        &[
            "Auto picks the most profitable coin for this machine and switches by itself.",
            if gpu {
                "A supported GPU was found: GPU coins are listed too."
            } else {
                "No supported GPU found: CPU coins only."
            },
        ],
    );
    let list = mineable(&base, gpu);
    let mut coin_items = vec!["Auto (most profitable)".to_string()];
    coin_items.extend(list.iter().map(|c| {
        format!(
            "{:<7} {:<16} {:<13} {}{}",
            c.id,
            c.name,
            c.algo,
            if c.device == Device::Gpu {
                "GPU"
            } else {
                "CPU"
            },
            if c.asic {
                "  (ASIC coin: earns little)"
            } else {
                ""
            }
        )
    }));
    let cur = list
        .iter()
        .position(|c| c.id == base.coin)
        .map_or(0, |i| i + 1);
    let picked = select("Coin", &coin_items, cur)?;
    p.coin = Some(match picked {
        0 => config::AUTO_COIN.to_string(),
        i => list[i - 1].id.to_string(),
    });

    let cur = match base.performance.preset {
        Preset::Eco => 0,
        Preset::Balanced => 1,
        Preset::Performance => 2,
        Preset::Custom => 3,
    };
    let preset = [
        Preset::Eco,
        Preset::Balanced,
        Preset::Performance,
        Preset::Custom,
    ][select(
        "CPU threads",
        &items(&[
            "eco       25%, lowest priority",
            "balanced  50%",
            "max       100% (dedicated machine)",
            "custom    choose the percentage",
        ]),
        cur,
    )?];
    let mut perf = Performance {
        preset,
        ..base.performance.clone()
    };
    if preset == Preset::Custom {
        perf.custom_threads_percent = Input::<u8>::with_theme(&t)
            .with_prompt("CPU threads (%)")
            .default(base.performance.custom_threads_percent)
            .validate_with(|v: &u8| {
                if (1..=100).contains(v) {
                    Ok(())
                } else {
                    Err("between 1 and 100")
                }
            })
            .interact_text()
            .map_err(cancelled)?;
    }
    p.performance = Some(perf);
    p.worker = Some(
        Input::<String>::with_theme(&t)
            .with_prompt("Worker name (shown by the pool)")
            .default(match base.pool.worker.as_str() {
                "" => config::default_worker(),
                w => w.to_string(),
            })
            .validate_with(|s: &String| check_worker(s))
            .interact_text()
            .map_err(cancelled)?,
    );

    // Electricity.
    section(
        "4 · Electricity",
        &["With a price per kWh Erebor shows net earnings and can stop when mining loses money."],
    );
    let mut energy = base.energy.clone();
    let cur = usize::from(energy.currency == "USD");
    energy.currency = ["EUR", "USD"][select("Currency", &items(&["EUR", "USD"]), cur)?].into();
    let price = Input::<String>::with_theme(&t)
        .with_prompt("Price per kWh (empty: skip)")
        .allow_empty(true)
        .with_initial_text(
            energy
                .price_per_kwh
                .map(|v| v.to_string())
                .unwrap_or_default(),
        )
        .validate_with(|s: &String| {
            let s = s.trim().replace(',', ".");
            if s.is_empty() || s.parse::<f64>().is_ok_and(|v| (0.0..=100.0).contains(&v)) {
                Ok(())
            } else {
                Err("a number like 0.25, or empty")
            }
        })
        .interact_text()
        .map_err(cancelled)?;
    energy.price_per_kwh = price.trim().replace(',', ".").parse().ok();
    energy.stop_if_unprofitable = energy.price_per_kwh.is_some()
        && confirm(
            "Stop mining when it costs more than it earns",
            energy.stop_if_unprofitable,
        )?;
    p.energy = Some(energy);

    // Safety.
    section("5 · Safety", &[]);
    let mut safety = base.safety.clone();
    safety.max_temp_c = Input::<f32>::with_theme(&t)
        .with_prompt("Stop above this temperature (°C)")
        .default(safety.max_temp_c)
        .validate_with(|v: &f32| {
            if (40.0..=100.0).contains(v) {
                Ok(())
            } else {
                Err("between 40 and 100")
            }
        })
        .interact_text()
        .map_err(cancelled)?;
    safety.auto_restart = confirm(
        "Restart the miner if it crashes (max 3 times)",
        safety.auto_restart,
    )?;
    p.safety = Some(safety);

    // Miners.
    section(
        "6 · Miners",
        &["Official releases pinned by Erebor, SHA-256 verified, updated automatically."],
    );
    let mut want = vec![MinerKind::Xmrig];
    if gpu {
        want.extend([MinerKind::Srbminer, MinerKind::Lolminer]);
    }
    if let Some(c) = p.coin.as_deref().and_then(coins::find) {
        if !want.contains(&c.miner) {
            want.push(c.miner);
        }
    }
    for k in want {
        let v = install::manifest_for(k).version;
        if installed(current, k) {
            println!("  {}", ui::ok(format!("{} already installed", k.label())));
        } else if !available(k) {
            println!(
                "  {}",
                ui::warn(format!(
                    "{}: no official build for {}",
                    k.label(),
                    install::platform_key()
                ))
            );
        } else if confirm(&format!("Install {} {v}", k.label()), true)? {
            p.install.push(k);
        }
    }

    // Running.
    section(
        "7 · Running",
        &["Mining runs in a background agent: closing the terminal does not stop it."],
    );
    p.background = confirm("Keep Erebor running in the background", true)?;
    if p.background {
        p.service = confirm("Start with the computer and mine always", false)?;
    }
    p.start = confirm("Start mining now", false)?;
    if p.start {
        p.background = true;
    }
    p.monitor = p.start && confirm("Open the dashboard at the end", true)?;
    review(p, &base).map(Some)
}

/// Show what will change and ask to apply it.
fn review(p: Plan, base: &AppConfig) -> Result<Plan, Fail> {
    section("Review", &[]);
    let row = |l: &str, v: String| println!("  {} {v}", ui::label(l, 16));
    let yn = |b: bool| if b { "yes" } else { "no" }.to_string();
    if p.mode == Some(Mode::Demo) {
        row("Mode", "demo (simulated, nothing is mined)".into());
    } else {
        row("Mode", "real mining".into());
        row("Kryptex ID", p.wallet.clone().unwrap_or_default());
        row(
            "Coin",
            match p.coin.as_deref() {
                Some(config::AUTO_COIN) | None => "auto".into(),
                Some(c) => c.into(),
            },
        );
        if let Some(perf) = &p.performance {
            row("CPU threads", erebor_tui::fmt::performance(perf));
        }
        row("Worker", p.worker.clone().unwrap_or_default());
        if let Some(e) = &p.energy {
            row(
                "Electricity",
                match e.price_per_kwh {
                    Some(v) => format!(
                        "{v} {}/kWh · stop when unprofitable: {}",
                        e.currency,
                        yn(e.stop_if_unprofitable)
                    ),
                    None => "not set".into(),
                },
            );
        }
        let s = p.safety.as_ref().unwrap_or(&base.safety);
        row(
            "Safety",
            format!(
                "stop above {:.0} °C · auto-restart: {}",
                s.max_temp_c,
                yn(s.auto_restart)
            ),
        );
        row(
            "Install",
            if p.install.is_empty() {
                "nothing".into()
            } else {
                p.install
                    .iter()
                    .map(|k| k.label())
                    .collect::<Vec<_>>()
                    .join(", ")
            },
        );
    }
    row("Background", yn(p.background));
    row("With computer", yn(p.service));
    row("Start now", yn(p.start));
    println!();
    let ok = Confirm::with_theme(&theme())
        .with_prompt("Apply")
        .default(true)
        .interact()
        .map_err(cancelled)?;
    if ok {
        Ok(p)
    } else {
        Err("onboarding_cancelled".to_string().into())
    }
}

fn short(w: &str) -> String {
    if w.is_empty() {
        "not set".into()
    } else if w.len() > 16 {
        format!("{}…{}", &w[..8], &w[w.len() - 6..])
    } else {
        w.into()
    }
}

fn step(json: bool, msg: &str) {
    if !json {
        println!("  {} {msg}", paint("◇", ui::BRAND));
    }
}

fn apply(dir: &Path, p: &Plan, json: bool) -> Result<(), Fail> {
    if !json {
        section("Applying", &[]);
    }
    let was_running = ipc::call(dir, "agent_info", &Value::Null).is_ok();
    if !was_running {
        step(json, "Starting the Erebor agent");
        crate::spawn_agent(dir)?;
    }
    let owner = ipc::call(dir, "agent_info", &Value::Null)?["owner"]
        .as_str()
        .unwrap_or("")
        .to_string();

    step(json, "Saving the configuration");
    let mut c = if p.reset {
        AppConfig::default()
    } else {
        crate::load_config_live(dir)?
    };
    c.demo_mode = p.mode == Some(Mode::Demo);
    if !c.demo_mode {
        c.onboarding_completed = true; // setup completed
        if let Some(w) = &p.wallet {
            c.wallet = w.clone();
        }
        if let Some(id) = &p.pool {
            let pi = pools::find(id).ok_or_else(|| Fail::from("unknown_pool".to_string()))?;
            c.pool = PoolConfig {
                id: pi.id.into(),
                host: pi.host.into(),
                port: pi.port,
                tls: pi.tls,
                worker: c.pool.worker.clone(),
            };
        }
        if let Some(w) = &p.worker {
            c.pool.worker = w.clone();
        }
        if let Some(x) = &p.performance {
            c.performance = x.clone();
        }
        if let Some(x) = &p.energy {
            c.energy = x.clone();
        }
        if let Some(x) = &p.safety {
            c.safety = x.clone();
        }
    }
    if p.service {
        c.start_on_launch = true;
    }
    crate::save_config(dir, &c)?;

    for k in &p.install {
        crate::install_wait(dir, *k, !json)?;
    }
    // The coin last: picking one whose miner is missing would start its own install.
    if let (Some(coin), false) = (&p.coin, c.demo_mode) {
        let mut c = crate::load_config_live(dir)?;
        c.coin = coin.clone();
        crate::save_config(dir, &c)?;
    }

    let mut problems = vec![];
    if p.health {
        step(json, "Health check");
        let cfg = crate::load_config(dir)?;
        // Demo needs no pool; hardware notes (GPU, sensors) stay in `erebor doctor`.
        let info_only = ["gpu", "temperature", "power"];
        for ch in doctor::run(dir, &cfg, cfg.demo_mode) {
            let note = ch.status == doctor::Level::Warn && info_only.contains(&ch.id);
            if ch.status != doctor::Level::Ok && !note {
                if !json {
                    let mark = if ch.status == doctor::Level::Fail {
                        "✗"
                    } else {
                        "!"
                    };
                    println!(
                        "  {} {}: {}",
                        paint(mark, if mark == "✗" { ui::ERR } else { ui::WARN }),
                        ch.id,
                        ch.message
                    );
                    if let Some(h) = &ch.hint {
                        println!("      → {}", paint(h, ui::CMD));
                    }
                }
                if ch.status == doctor::Level::Fail {
                    problems.push(ch.id);
                }
            }
        }
    }

    if p.service {
        if owner == "agent" {
            step(json, "Setting Erebor to start with the computer");
            // The service starts its own agent: hand over from the one we started.
            crate::call(dir, "shutdown", Value::Null)?;
            wait(dir, false)?;
            crate::install_service(dir)?;
            wait(dir, true)?;
            if crate::enable_linger() == Some(false) && !json {
                println!(
                    "  {} {}",
                    ui::warn("To start right at boot, before anyone logs in:"),
                    paint("sudo loginctl enable-linger \"$USER\"", ui::CMD)
                );
            }
        } else {
            step(
                json,
                "Login service skipped: the desktop app owns the engine right now",
            );
        }
    }

    if p.start {
        if problems.is_empty() {
            step(json, "Starting mining");
            ipc::control(dir, Action::Start)?;
        } else {
            step(json, "Mining not started: fix the problems above first");
        }
    }

    let keep = p.background || p.start || was_running || p.service;
    if !keep && owner == "agent" {
        step(
            json,
            "Stopping the agent (you chose not to keep it running)",
        );
        crate::call(dir, "shutdown", Value::Null)?;
        wait(dir, false)?;
    }
    summary(dir, Some((p, problems)), json)?;
    if p.monitor && !json && ipc::snapshot(dir).is_ok() {
        erebor_tui::run(dir.to_path_buf()).map_err(Fail::from)?;
    }
    Ok(())
}

/// Wait until the agent answers (`up`) or is gone.
fn wait(dir: &Path, up: bool) -> Result<(), Fail> {
    let t = Instant::now();
    while ipc::call(dir, "agent_info", &Value::Null).is_ok() != up {
        if t.elapsed() > Duration::from_secs(15) {
            return Err("agent_start_timeout".to_string().into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

fn summary(dir: &Path, done: Option<(&Plan, Vec<&str>)>, json: bool) -> Result<(), Fail> {
    let c = crate::load_config(dir)?;
    let snap = ipc::snapshot(dir).ok();
    let service = autostart::is_installed(autostart::AGENT_LABEL, autostart::Kind::Service);
    let xmrig = c.miner_for(MinerKind::Xmrig);
    let problems = done.as_ref().map(|d| d.1.clone()).unwrap_or_default();
    if json {
        crate::print_json(&json!({
            "changed": done.is_some(),
            "demo": c.demo_mode,
            "setupCompleted": c.onboarding_completed,
            "wallet": c.wallet,
            "pool": c.pool.id,
            "xmrig": xmrig.path.is_some().then(|| xmrig.version.clone()),
            "agentRunning": snap.is_some(),
            "loginService": service,
            "mining": snap.as_ref().map(|s| s.state.clone()),
            "problems": problems,
        }));
        return Ok(());
    }
    let row = |l: &str, v: String| println!("  {} {v}", ui::label(l, 16));
    let title = match &done {
        Some((_, p)) if !p.is_empty() => paint(
            "Setup saved, with problems to fix (see above)",
            ui::WARN.bold(),
        ),
        Some(_) => paint("Setup complete", ui::OK.bold()),
        None => paint("Current setup kept", ui::OK.bold()),
    };
    println!("\n\n  {title}\n");
    if c.demo_mode {
        row("Mode", "demo (simulated miner, nothing is mined)".into());
    } else {
        row("Kryptex ID", short(&c.wallet));
        row(
            "Coin",
            if c.coin == config::AUTO_COIN {
                "auto".into()
            } else {
                c.coin.clone()
            },
        );
        row(
            "Pool",
            format!(
                "{} · worker {}",
                c.pool.id,
                match c.pool.worker.as_str() {
                    "" => config::default_worker(),
                    w => w.to_string(),
                }
            ),
        );
        let miners: Vec<String> = [
            MinerKind::Xmrig,
            MinerKind::Srbminer,
            MinerKind::Lolminer,
            MinerKind::Cpuminer,
        ]
        .into_iter()
        .filter_map(|k| {
            let m = c.miner_for(k);
            m.path
                .as_ref()
                .map(|_| format!("{} {}", k.label(), m.version.clone().unwrap_or_default()))
        })
        .collect();
        row(
            "Miners",
            if miners.is_empty() {
                "none → erebor miner install".into()
            } else {
                miners.join(", ")
            },
        );
    }
    row(
        "Agent",
        if snap.is_some() {
            "running in the background".into()
        } else {
            "stopped".into()
        },
    );
    row(
        "With computer",
        if service { "yes".into() } else { "no".into() },
    );
    row(
        "Mining",
        snap.map_or("-".into(), |s| erebor_tui::fmt::state(&s.state).into()),
    );
    println!("\n\n  {}\n", paint("Next", ui::BRAND));
    for (cmd, what) in [
        ("erebor", "dashboard (Q leaves mining running)"),
        ("erebor start / stop", "control mining"),
        (
            "erebor config",
            "every setting · erebor config set <key> <value>",
        ),
        ("erebor coins", "coins and estimated earnings"),
        ("erebor --help", "all commands"),
    ] {
        println!("  {}  {what}", paint(format!("{cmd:<20}"), ui::CMD));
    }
    println!();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const XMR: &str = "48edfHu7V9Z84YzzMa6fUueoELZ9ZRXq9VetWzYGzKt52XU5xvqgzYnDK9URnRoJMk1j8nLwEVsaSWJ4fhdUyZijBGUicoD";

    fn opts() -> Opts {
        Opts {
            non_interactive: true,
            ..Default::default()
        }
    }

    #[test]
    fn non_interactive_plans() {
        let c = AppConfig::default();
        // Real mining needs a valid wallet.
        let e = |o: &Opts| from_flags(o, &c).unwrap_err().code;
        let mut o = opts();
        assert_eq!(e(&o), "wallet_missing");
        o.wallet = Some("abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about".into());
        assert_eq!(e(&o), "invalid_address");
        // Kryptex only for now: a Monero address is refused.
        o.wallet = Some(XMR.into());
        assert_eq!(e(&o), "invalid_address");
        o.wallet = Some(" krxABC123 ".into());
        o.pool = Some("nope".into());
        assert_eq!(e(&o), "unknown_pool");
        o.pool = Some("kryptex".into());
        o.no_background = true;
        let p = from_flags(&o, &c).unwrap();
        assert_eq!(p.wallet.as_deref(), Some("krxABC123"));
        assert!(!p.background && !p.start && p.health);
        // Starting mining keeps the agent, whatever --no-background says.
        o.start = true;
        assert!(from_flags(&o, &c).unwrap().background);

        // Demo needs no wallet.
        let d = from_flags(
            &Opts {
                mode: Some(Mode::Demo),
                ..opts()
            },
            &c,
        )
        .unwrap();
        assert_eq!(
            (d.mode, d.background, d.wallet),
            (Some(Mode::Demo), true, None)
        );
    }

    #[test]
    fn secrets_are_refused_with_a_clear_message() {
        assert!(check_wallet("abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about")
            .unwrap_err()
            .contains("seed phrase"));
        assert_eq!(check_wallet("krxABC123").unwrap(), "Kryptex");
        assert_eq!(short(XMR), "48edfHu7…GUicoD");
    }

    #[test]
    fn first_run_detection() {
        let d = tempfile::tempdir().unwrap();
        assert!(needs_setup(d.path()));
        std::fs::write(d.path().join("config.json"), r#"{"demoMode":true}"#).unwrap();
        assert!(!needs_setup(d.path()));
        std::fs::write(d.path().join("config.json"), r#"{"wallet":"krxABC123"}"#).unwrap();
        assert!(needs_setup(d.path()), "setup not completed yet");
    }
}

//! `erebor onboard`: guided first-run setup, modelled on OpenClaw's onboarding wizard
//! (setup mode → payout → miner → background agent → health check → summary).
//!
//! Every answer is collected first; nothing is changed until the last question, so
//! leaving half-way (Ctrl-C) changes nothing. Changes go through
//! the engine (`update_config`), with the same validation as the desktop app.

use crate::ui::{self, paint};
use crate::{doctor, Fail};
use anstream::println;
use clap::{Args, ValueEnum};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, Input, Select};
use erebor_core::config::{self, AppConfig, PoolConfig, Preset};
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
    /// No prompts (scripts, provisioning).
    #[arg(long)]
    pub non_interactive: bool,
    #[arg(long, value_enum)]
    pub mode: Option<Mode>,
    /// Payout: your Kryptex ID (krx…).
    #[arg(long, value_name = "ADDRESS")]
    pub wallet: Option<String>,
    #[arg(long, value_name = "POOL_ID")]
    pub pool: Option<String>,
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
    Quickstart,
    Custom,
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
    preset: Option<Preset>,
    worker: Option<String>,
    device_name: Option<String>,
    max_temp_c: Option<f32>,
    auto_restart: Option<bool>,
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

pub fn run(dir: &Path, o: Opts, json: bool) -> Result<(), Fail> {
    let current = crate::load_config(dir)?;
    let plan = if o.non_interactive || !interactive() {
        if !o.non_interactive {
            return Err("onboarding_needs_terminal".to_string().into());
        }
        from_flags(&o, &current)?
    } else {
        match ask(dir, &o, &current)? {
            Some(p) => p,
            None => return summary(dir, None, json),
        }
    };
    apply(dir, &plan, json)
}

fn from_flags(o: &Opts, c: &AppConfig) -> Result<Plan, Fail> {
    let mode = o.mode.unwrap_or(Mode::Quickstart);
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
    p.wallet = Some(w.trim().into());
    p.pool = o.pool.clone();
    p.preset = o.performance.map(Into::into);
    if !o.no_install_miner && !installed(c, MinerKind::Xmrig) && available(MinerKind::Xmrig) {
        p.install.push(MinerKind::Xmrig);
    }
    if !o.no_install_miner && Monitor::new().detect().gpu_mining_supported {
        for k in [MinerKind::Srbminer, MinerKind::Lolminer] {
            if !installed(c, k) && available(k) {
                p.install.push(k);
            }
        }
    }
    Ok(p)
}

fn cancelled(_: dialoguer::Error) -> Fail {
    Fail::from("onboarding_cancelled".to_string())
}

/// The interactive questions. `None`: keep the current setup.
fn ask(dir: &Path, o: &Opts, current: &AppConfig) -> Result<Option<Plan>, Fail> {
    let t = ColorfulTheme::default();
    let select = |prompt: &str, items: &[&str], default: usize| {
        Select::with_theme(&t)
            .with_prompt(prompt)
            .items(items)
            .default(default)
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

    println!(
        "\n  {}  {}\n",
        paint("▲ EREBOR", ui::BRAND),
        paint("·  mining, under your control", ui::DIM)
    );
    println!(
        "{}\n",
        paint(
            "  This setup takes about a minute. Nothing changes until the last step,\n  and nothing is mined unless you ask for it.",
            ui::DIM
        )
    );

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
        match select(
            &format!("Erebor is already set up ({what})"),
            &[
                "Keep the current setup",
                "Modify it",
                "Start over from defaults (statistics are kept)",
            ],
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
        None => match select(
            "How do you want to set up Erebor?",
            &[
                "Quick start (recommended): your wallet, the official miner, safe defaults",
                "Custom: also choose pool, performance, safety and startup",
                "Demo: try everything with a simulated miner (nothing is mined)",
            ],
            0,
        )? {
            0 => Mode::Quickstart,
            1 => Mode::Custom,
            _ => Mode::Demo,
        },
    };
    let mut p = Plan {
        mode: Some(mode),
        reset,
        health: !o.skip_health,
        ..Default::default()
    };

    if mode == Mode::Demo {
        p.background = true;
        p.start = confirm("Start the simulated miner now?", true)?;
        p.monitor = confirm("Open the live dashboard at the end?", true)?;
        return Ok(Some(p));
    }

    println!("\n  Earnings go to your kryptex.com account (paid in BTC).");
    println!("  Enter your Kryptex ID (krx…), shown on kryptex.com after signing up.\n");
    let mut input = Input::<String>::with_theme(&t).with_prompt("Kryptex ID");
    let prefill = o.wallet.clone().unwrap_or_else(|| base.wallet.clone());
    if !prefill.is_empty() {
        input = input.default(prefill);
    }
    let w = input
        .validate_with(|s: &String| check_wallet(s).map(|_| ()))
        .interact_text()
        .map_err(cancelled)?;
    let kind = wallet::payout_kind(w.trim());
    println!(
        "  {}",
        ui::ok(format!("{} payout", check_wallet(&w).unwrap_or_default()))
    );
    p.wallet = Some(w.trim().into());

    if mode == Mode::Custom {
        match kind {
            Some(wallet::AddressKind::Kryptex) => {
                println!("  Kryptex IDs mine on the Kryptex pool: pool set automatically.")
            }
            Some(wallet::AddressKind::Bitcoin) => {
                println!("  Bitcoin payouts mine on unMineable: no pool to choose.")
            }
            _ => {
                let items: Vec<String> = pools::CATALOG
                    .iter()
                    .map(|p| format!("{} ({}:{}, fee {}%)", p.name, p.host, p.port, p.fee_percent))
                    .collect();
                let refs: Vec<&str> = items.iter().map(String::as_str).collect();
                let cur = pools::CATALOG
                    .iter()
                    .position(|x| x.id == base.pool.id)
                    .unwrap_or(0);
                p.pool = Some(pools::CATALOG[select("Pool", &refs, cur)?].id.into());
            }
        }
        let cur = match base.performance.preset {
            Preset::Eco => 0,
            Preset::Performance => 2,
            _ => 1,
        };
        p.preset = Some(
            [Preset::Eco, Preset::Balanced, Preset::Performance][select(
                "Performance",
                &[
                    "Eco: 25% of CPU threads, lowest priority (PC stays fully usable)",
                    "Balanced: 50% of threads (recommended)",
                    "Performance: all threads (dedicated machine)",
                ],
                cur,
            )?],
        );
        p.worker = Some(
            Input::<String>::with_theme(&t)
                .with_prompt("Name of this PC (shown by the pool)")
                .default(match base.pool.worker.as_str() {
                    "" => config::default_worker(),
                    w => w.to_string(),
                })
                .validate_with(|s: &String| {
                    if s.eq_ignore_ascii_case(config::PROJECT_WORKER) {
                        Err("EREBOR is reserved for Erebor's 2%")
                    } else if config::valid_worker(s) {
                        Ok(())
                    } else {
                        Err("letters, digits, '_' and '-' only, max 32")
                    }
                })
                .interact_text()
                .map_err(cancelled)?,
        );
        let device = erebor_core::fleet::device(dir)
            .map(|d| d.name)
            .unwrap_or_default();
        p.device_name = Some(
            Input::<String>::with_theme(&t)
                .with_prompt("Name of this computer (for Erebor Fleet)")
                .default(device)
                .interact_text()
                .map_err(cancelled)?,
        );
        p.max_temp_c = Some(
            Input::<f32>::with_theme(&t)
                .with_prompt("Stop mining above this CPU temperature (°C)")
                .default(base.safety.max_temp_c)
                .validate_with(|v: &f32| {
                    if (40.0..=100.0).contains(v) {
                        Ok(())
                    } else {
                        Err("between 40 and 100")
                    }
                })
                .interact_text()
                .map_err(cancelled)?,
        );
        p.auto_restart = Some(confirm(
            "Restart the miner automatically if it crashes (max 3 times)?",
            base.safety.auto_restart,
        )?);
    }

    if installed(current, MinerKind::Xmrig) {
        println!("  {}", ui::ok("XMRig already installed"));
    } else if available(MinerKind::Xmrig) {
        let v = install::manifest_for(MinerKind::Xmrig).version;
        if confirm(
            &format!("Install XMRig {v} now (official release, SHA-256 verified, ~5 MB)?"),
            true,
        )? {
            p.install.push(MinerKind::Xmrig);
        } else {
            println!("  Later: erebor miner install xmrig (needed for real mining)");
        }
    } else {
        println!(
            "  ! No pinned XMRig build for {}: select a binary in the desktop app.",
            install::platform_key()
        );
    }
    // A supported GPU earns far more than the CPU: Quick start installs its miner too.
    let gpu_miners: Vec<MinerKind> = [MinerKind::Srbminer, MinerKind::Lolminer]
        .into_iter()
        .filter(|k| !installed(current, *k) && available(*k))
        .collect();
    if !gpu_miners.is_empty() && Monitor::new().detect().gpu_mining_supported {
        let names: Vec<&str> = gpu_miners.iter().map(|k| k.label()).collect();
        let yes = if mode == Mode::Custom {
            confirm(
                &format!("Install {} too, for GPU coins?", names.join(" and ")),
                true,
            )?
        } else {
            println!(
                "  {}",
                ui::ok(format!(
                    "GPU found: {} will be installed for GPU coins.",
                    names.join(" and ")
                ))
            );
            true
        };
        if yes {
            p.install.extend(gpu_miners);
        }
    }

    println!("  The agent runs mining in the background. It never mines on its own.");
    p.background = confirm("Keep the agent running after the setup?", true)?;
    if p.background {
        p.service = confirm(
            "Mine always, also after the computer restarts? (erebor service off to undo)",
            false,
        )?;
    }
    p.start = confirm("Start mining at the end of the setup?", false)?;
    if p.start && !p.background {
        println!("  Mining runs inside the agent: it will stay running while you mine.");
        p.background = true;
    }
    p.monitor = p.start && confirm("Open the live dashboard at the end?", true)?;
    Ok(Some(p))
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
        println!("{} {msg}", paint("◇", ui::BRAND));
    }
}

fn apply(dir: &Path, p: &Plan, json: bool) -> Result<(), Fail> {
    if !json {
        println!();
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
        if let Some(pr) = p.preset {
            c.performance.preset = pr;
        }
        if let Some(t) = p.max_temp_c {
            c.safety.max_temp_c = t;
        }
        if let Some(r) = p.auto_restart {
            c.safety.auto_restart = r;
        }
    }
    if p.service {
        c.start_on_launch = true;
    }
    crate::save_config(dir, &c)?;
    if let Some(n) = &p.device_name {
        crate::call(dir, "set_device_name", json!({ "name": n }))?;
    }

    for k in &p.install {
        step(json, &format!("Installing {}", k.label()));
        crate::install_wait(dir, *k, !json)?;
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
    let row = |l: &str, v: String| println!("  {} {v}", ui::label(l, 15));
    println!();
    println!(
        "{}",
        match &done {
            Some((_, p)) if !p.is_empty() => paint(
                "▲ Setup saved, with problems to fix (see above)",
                ui::WARN.bold()
            ),
            Some(_) => paint("◆ Setup complete", ui::OK.bold()),
            None => paint("◆ Current setup kept", ui::OK.bold()),
        }
    );
    row(
        "Mode",
        if c.demo_mode {
            "demo (simulated miner, nothing is mined)".into()
        } else {
            "real mining".into()
        },
    );
    if !c.demo_mode {
        row(
            "Payout",
            format!(
                "{} ({})",
                short(&c.wallet),
                wallet::payout_kind(&c.wallet)
                    .map(|k| format!("{k:?}"))
                    .unwrap_or_else(|| "not set".into())
            ),
        );
        row("Pool", c.pool.id.clone());
        row(
            "Miner",
            match &xmrig.path {
                Some(_) => format!("XMRig {}", xmrig.version.clone().unwrap_or_default()),
                None => "not installed → erebor miner install".into(),
            },
        );
        let srb = c.miner_for(MinerKind::Srbminer);
        if srb.path.is_some() {
            row(
                "GPU miner",
                format!("SRBMiner-MULTI {}", srb.version.clone().unwrap_or_default()),
            );
        }
    }
    row(
        "Agent",
        if snap.is_some() {
            "running in the background".into()
        } else {
            "stopped → erebor agent start".into()
        },
    );
    row(
        "At startup",
        if service {
            paint("starts with the computer", ui::OK)
        } else {
            "manual (erebor service on to mine always)".into()
        },
    );
    row(
        "Mining",
        snap.map_or("-".into(), |s| erebor_tui::fmt::state(&s.state).into()),
    );
    println!("\n{}", paint("Next steps:", ui::BRAND));
    for (cmd, what) in [
        ("erebor", "live dashboard (Q leaves mining running)"),
        ("erebor start", "start mining · erebor stop to stop"),
        ("erebor power max", "use the whole CPU"),
        ("erebor service on", "mine always, also after a restart"),
        (
            "erebor doctor",
            "diagnostics · erebor onboard to change this setup",
        ),
    ] {
        println!("  {}  {what}", paint(format!("{cmd:<19}"), ui::CMD));
    }
    println!(
        "{}",
        paint("  The Erebor desktop app uses this same setup.", ui::DIM)
    );
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

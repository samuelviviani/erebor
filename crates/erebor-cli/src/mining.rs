//! Mining control: start/stop, power level and miner installation.

use super::*;
use anstream::{eprint, eprintln, println};

pub(super) fn mining(dir: &Path, cmd: MiningCmd, json: bool) -> Result<(), Fail> {
    // Other devices: through the controller, one result per device.
    match &cmd {
        MiningCmd::Start(t) if t.is_set() => return fleet::mining(dir, "start", t, json),
        MiningCmd::Stop(t) if t.is_set() => return fleet::mining(dir, "stop", t, json),
        MiningCmd::Restart(t) if t.is_set() => return fleet::mining(dir, "restart", t, json),
        MiningCmd::Status(t) if t.is_set() => return fleet::mining(dir, "status", t, json),
        MiningCmd::Status(_) => return status(dir, json),
        _ => {}
    }
    let action = match cmd {
        MiningCmd::Start(_) => Action::Start,
        MiningCmd::Stop(_) => Action::Stop,
        MiningCmd::Pause => Action::Pause,
        MiningCmd::Resume => Action::Resume,
        MiningCmd::Restart(_) => Action::Restart,
        MiningCmd::Status(_) => unreachable!(),
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

/// `erebor power [eco|balanced|max]`: show or change how much CPU mining uses.
pub(super) fn power(dir: &Path, level: Option<onboard::Perf>, json: bool) -> Result<(), Fail> {
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

/// Download, verify and install the pinned miner through the engine; wait for it.
pub(super) fn install_wait(dir: &Path, kind: MinerKind, progress: bool) -> Result<(), Fail> {
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

pub(super) fn miner_install(dir: &Path, kind: Kind, json: bool) -> Result<(), Fail> {
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

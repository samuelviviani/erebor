//! Read-only views: `status`, `hardware`, `logs` and the CSV `export`.

use super::*;
use anstream::println;

pub(super) fn status(dir: &Path, json: bool) -> Result<(), Fail> {
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

pub(super) fn hardware(cmd: HardwareCmd, json: bool) -> Result<(), Fail> {
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

/// CSV from the running engine, or straight from the database when none runs.
pub(super) fn export(dir: &Path, range: i64) -> Result<(), Fail> {
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

pub(super) fn logs(dir: &Path, follow: bool, json: bool) -> Result<(), Fail> {
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

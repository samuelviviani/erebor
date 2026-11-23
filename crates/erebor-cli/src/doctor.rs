//! `erebor doctor`: environment checks with plain-language hints. Works without the
//! agent (hardware and files are read locally); asks the agent when it is running.

use erebor_core::config::AppConfig;
use erebor_core::hardware::Monitor;
use erebor_core::miner::MinerKind;
use erebor_core::{install, ipc, pools, wallet, xmrig};
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Serialize)]
pub struct Check {
    pub id: &'static str,
    pub status: Level,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

fn check(id: &'static str, status: Level, message: impl Into<String>, hint: Option<&str>) -> Check {
    Check {
        id,
        status,
        message: message.into(),
        hint: hint.map(Into::into),
    }
}

pub fn run(dir: &Path, cfg: &AppConfig, offline: bool) -> Vec<Check> {
    use Level::*;
    let mut v = vec![];
    let mut m = Monitor::new();
    let hw = m.detect();
    let live = m.live();

    v.push(check(
        "os",
        if cfg!(windows) { Warn } else { Ok },
        format!("{} {} · {}", hw.os_name, hw.os_version, hw.arch),
        cfg!(windows).then_some(
            "the local agent socket is not available on Windows yet: use the desktop app",
        ),
    ));
    v.push(check(
        "cpu",
        Ok,
        format!(
            "{} · {} cores / {} threads",
            hw.cpu_brand,
            hw.physical_cores.map_or("?".to_string(), |c| c.to_string()),
            hw.logical_threads
        ),
        None,
    ));
    v.push(if hw.gpus.is_empty() {
        check("gpu", Ok, "no GPU detected: CPU coins only", None)
    } else {
        let names: Vec<_> = hw.gpus.iter().map(|g| g.name.as_str()).collect();
        let srb = cfg
            .miner_for(erebor_core::miner::MinerKind::Srbminer)
            .path
            .is_some();
        match (hw.gpu_mining_supported, srb) {
            (true, true) => check("gpu", Ok, names.join(", "), None),
            (true, false) => check(
                "gpu",
                Warn,
                format!("{}: not used, its miner is not installed", names.join(", ")),
                Some(
                    "erebor miner install srbminer (GPU coins usually earn much more than the CPU)",
                ),
            ),
            (false, _) => check(
                "gpu",
                Warn,
                names.join(", "),
                Some("GPU mining is not supported for this GPU/OS: CPU coins only"),
            ),
        }
    });
    v.push(match live.cpu_temp_c {
        Some(t) => check("temperature", Ok, format!("CPU {t:.0} °C"), None),
        None => check(
            "temperature",
            Warn,
            "CPU temperature not readable on this system (shown as n/a)",
            Some("on Linux install lm-sensors and check `sensors`; overheat protection needs it"),
        ),
    });
    v.push(match live.power_w {
        Some(w) => check("power", Ok, format!("measured {w:.0} W"), None),
        None => check(
            "power",
            Warn,
            "power draw not measurable here (shown as n/a)",
            Some("set an estimate in the desktop settings (Energy) for cost estimates; on Linux RAPL counters are often root-only"),
        ),
    });

    #[cfg(unix)]
    {
        // SAFETY: no arguments, cannot fail.
        if unsafe { libc::geteuid() } == 0 {
            v.push(check(
                "user",
                Warn,
                "running as root",
                Some("not needed: run Erebor as your normal user"),
            ));
        } else {
            v.push(check("user", Ok, "normal user (no root needed)", None));
        }
    }

    v.push(data_dir_check(dir));

    let agent = ipc::call(dir, "agent_info", &Value::Null);
    v.push(match &agent {
        Result::Ok(i) if i["protocol"] != ipc::PROTOCOL_VERSION => check(
            "agent",
            Fail,
            format!(
                "agent speaks protocol {} (CLI: {})",
                i["protocol"],
                ipc::PROTOCOL_VERSION
            ),
            Some("restart it with this version: erebor agent stop && erebor agent start"),
        ),
        Result::Ok(i) => check(
            "agent",
            Ok,
            format!(
                "{} running · pid {} · v{}",
                i["owner"].as_str().unwrap_or("?"),
                i["pid"],
                i["version"].as_str().unwrap_or("?")
            ),
            None,
        ),
        Err(e) => check(
            "agent",
            Warn,
            format!("not reachable ({})", e.as_str().unwrap_or("error")),
            Some("start it: erebor agent start"),
        ),
    });

    v.push(match cfg.validate() {
        Result::Ok(()) => check("config", Ok, "configuration valid", None),
        Err(e) => check(
            "config",
            Fail,
            format!("configuration invalid: {e}"),
            Some("fix it in the desktop settings or with erebor wallet/pools"),
        ),
    });
    let demo = cfg.demo_mode;
    v.push(check(
        "mode",
        Ok,
        if demo {
            "demo mode: simulated miner, nothing is mined"
        } else {
            "real mining mode"
        },
        None,
    ));
    v.push(match (cfg.wallet.as_str(), demo) {
        ("", true) => check("wallet", Ok, "not set (not needed in demo mode)", None),
        ("", false) => check(
            "wallet",
            Fail,
            "no payout wallet set",
            Some("erebor wallet set <ADDRESS>  (public address only, never a seed phrase)"),
        ),
        (w, _) => match wallet::validate_payout(w) {
            Result::Ok(i) => check(
                "wallet",
                Ok,
                format!("{:?} payout configured", i.kind),
                None,
            ),
            Err(e) => check(
                "wallet",
                Fail,
                format!("payout wallet invalid: {e:?}"),
                Some("erebor wallet set <ADDRESS>"),
            ),
        },
    });

    for kind in MinerKind::ALL {
        let id = match kind {
            MinerKind::Xmrig => "miner.xmrig",
            MinerKind::Srbminer => "miner.srbminer",
        };
        let platform_ok = install::manifest_for(kind).asset_for_platform().is_ok();
        let mc = cfg.miner_for(kind);
        v.push(match &mc.path {
            Some(p) if !Path::new(p).is_file() => check(
                id,
                Fail,
                format!("{} missing at {p}", kind.label()),
                Some("reinstall it: erebor miner install"),
            ),
            Some(p) => match (&mc.sha256, xmrig::sha256_file(Path::new(p))) {
                (Some(want), Result::Ok(got)) if *want != got => check(
                    id,
                    Fail,
                    format!("{} binary changed since it was installed", kind.label()),
                    Some("reinstall it: erebor miner install"),
                ),
                _ => check(
                    id,
                    Ok,
                    format!(
                        "{} {} ({})",
                        kind.label(),
                        mc.version.clone().unwrap_or_default(),
                        if mc.verified {
                            "verified"
                        } else {
                            "unverified, selected by you"
                        }
                    ),
                    None,
                ),
            },
            None if !platform_ok => check(
                id,
                if kind == MinerKind::Xmrig { Warn } else { Ok },
                format!(
                    "{} not installed; no pinned build for {}",
                    kind.label(),
                    install::platform_key()
                ),
                None,
            ),
            None if kind == MinerKind::Xmrig && !demo => check(
                id,
                Warn,
                "XMRig not installed",
                Some("erebor miner install xmrig  (pinned release, SHA-256 verified)"),
            ),
            None => check(
                id,
                Ok,
                format!("{} not installed (optional)", kind.label()),
                None,
            ),
        });
    }

    if offline {
        v.push(check("pool", Ok, "skipped (--offline)", None));
    } else {
        let (h, p) = (&cfg.pool.host, cfg.pool.port);
        v.push(match pools::check_reachable(h, p) {
            Result::Ok(ms) => check("pool", Ok, format!("{h}:{p} reachable ({ms} ms)"), None),
            Err(e) => check(
                "pool",
                if demo { Warn } else { Fail },
                format!("{h}:{p} not reachable: {e}"),
                Some("check network/firewall, or pick another pool: erebor pools list"),
            ),
        });
    }
    v
}

fn data_dir_check(dir: &Path) -> Check {
    let shown = dir.display().to_string();
    if !dir.exists() {
        return check(
            "data_dir",
            Level::Ok,
            format!("{shown} (created on first agent start)"),
            None,
        );
    }
    let probe = dir.join(".erebor-write-test");
    if std::fs::write(&probe, b"").is_err() {
        return check(
            "data_dir",
            Level::Fail,
            format!("{shown} is not writable"),
            Some("fix the folder owner/permissions (it must belong to your user)"),
        );
    }
    let _ = std::fs::remove_file(probe);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir)
            .map(|m| m.permissions().mode())
            .unwrap_or(0);
        if mode & 0o077 != 0 {
            return check(
                "data_dir",
                Level::Warn,
                format!(
                    "{shown} is accessible to other users (mode {:o})",
                    mode & 0o777
                ),
                Some("the agent fixes it on start, or: chmod 700 <folder>"),
            );
        }
    }
    check("data_dir", Level::Ok, shown, None)
}

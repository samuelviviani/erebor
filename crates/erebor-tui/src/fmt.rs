//! Value formatting and error explanations shared with the CLI.

pub const NA: &str = "n/a";

pub fn hashrate(h: Option<f64>) -> String {
    let Some(h) = h else { return NA.into() };
    let units = ["H/s", "kH/s", "MH/s", "GH/s", "TH/s"];
    let mut v = h;
    let mut i = 0;
    while v >= 1000.0 && i < units.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    format!("{v:.2} {}", units[i])
}

pub fn duration(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h > 0 {
        format!("{h}h {m:02}m {s:02}s")
    } else {
        format!("{m}m {s:02}s")
    }
}

pub fn temp(t: Option<f32>) -> String {
    t.map_or(NA.into(), |t| format!("{t:.0} °C"))
}

pub fn power(w: Option<f64>, source: &str) -> String {
    match w {
        Some(w) if source == "estimated" => format!("{w:.0} W (estimated)"),
        Some(w) => format!("{w:.0} W"),
        None => NA.into(),
    }
}

/// Estimated earnings over `days` days, after fees.
pub fn earnings_over(s: &erebor_core::fleet::Snapshot, days: f64) -> Option<String> {
    let e = s.earnings.as_ref()?;
    Some(match e.value_per_day {
        Some(v) => format!("{:.2} {}", v * days, e.currency),
        None => format!("{:.8} BTC", e.btc_per_day * days),
    })
}

pub fn earnings(s: &erebor_core::fleet::Snapshot) -> String {
    earnings_over(s, 1.0).map_or(NA.into(), |v| format!("{v} / day (estimate)"))
}

/// "balanced · 50% of threads".
pub fn performance(p: &erebor_core::config::Performance) -> String {
    use erebor_core::config::Preset;
    let name = match p.preset {
        Preset::Eco => "eco",
        Preset::Balanced => "balanced",
        Preset::Performance => "max",
        Preset::Custom => "custom",
    };
    format!("{name} · {}% of threads", p.limits().0)
}

pub fn state(state: &str) -> &'static str {
    match state {
        "running" => "Running",
        "paused" => "Paused",
        "error" => "Error",
        _ => "Stopped",
    }
}

/// Message and hint for the error codes users meet most.
pub fn explain(code: &str) -> (String, Option<&'static str>) {
    let (m, h) = match code {
        "agent_unreachable" => (
            "Erebor is not running in the background",
            Some("start mining: erebor start · live dashboard: erebor"),
        ),
        "wallet_missing" => (
            "no payout wallet configured",
            Some("erebor wallet set <KRYPTEX_ID>   (or try it first: erebor demo on)"),
        ),
        "miner_not_configured" => (
            "the miner is not installed",
            Some("erebor miner install xmrig"),
        ),
        "miner_binary_changed" => (
            "the miner binary changed since it was installed",
            Some("reinstall it: erebor miner install"),
        ),
        "job_running" => (
            "an install or benchmark is in progress",
            Some("wait for it to finish (erebor status)"),
        ),
        "not_running" => ("mining is not running", Some("erebor start")),
        "not_paused" => ("mining is not paused", None),
        "pause_unsupported" => ("this miner cannot pause", Some("use erebor stop")),
        "stop_mining_first" => ("stop mining first", Some("erebor stop")),
        "kryptex_pool_required" => (
            "a Kryptex ID only works on the Kryptex pool",
            Some("erebor pools set kryptex"),
        ),
        "protocol_version" => (
            "this command and the background Erebor are different versions",
            Some("restart it: erebor agent stop, then run the command again"),
        ),
        "not_agent" => (
            "mining belongs to the Erebor desktop app or node, not to the background agent",
            Some("close the desktop app (it stops mining) or stop the node"),
        ),
        "agent_untrusted" => (
            "the agent socket belongs to another user: refused",
            Some("check who started it (erebor agent status) and the data folder owner"),
        ),
        "pool_check_failed" => (
            "the configured pool is not reachable or not compatible with the wallet",
            Some("check network/DNS/firewall, or pick another: erebor pools set <id>"),
        ),
        "onboarding_cancelled" => (
            "setup cancelled: nothing was changed",
            Some("run erebor onboard again, or try erebor onboard --mode demo"),
        ),
        "onboarding_needs_terminal" => (
            "the guided setup needs an interactive terminal",
            Some("in scripts use: erebor onboard --non-interactive --wallet <ADDRESS>"),
        ),
        "invalid_address" => (
            "not a valid payout address (public Monero address, Kryptex ID or Bitcoin address)",
            None,
        ),
        "state_unchanged" => (
            "the miner did not change state",
            Some("check erebor status and the agent log"),
        ),
        "unknown_pool" => ("no pool with that id", Some("see: erebor pools list")),
        "unknown_coin" => ("no coin with that id", Some("see: erebor coins")),
        "not_in_demo" => ("not available in demo mode", None),
        "no_gpu" => ("needs a supported GPU", None),
        "platform_unsupported" => ("no miner build for this system", None),
        "kryptex_or_btc_required" => ("needs a Kryptex ID payout", None),
        "not_on_unmineable" => ("not minable with this payout", None),
        "own_wallet_required" => (
            "not on Kryptex: needs your own wallet for this coin",
            Some("set one: erebor wallet set <ADDRESS> --coin <COIN>"),
        ),
        "windows_only" => ("Windows only for now", None),
        "invalid_worker" => (
            "worker name: letters, digits, '_' and '-' only, max 32 (EREBOR is reserved)",
            None,
        ),
        _ if code.starts_with("unknown_key") => (
            "no setting with that key",
            Some("see every key: erebor config"),
        ),
        "update_bad_signature" => (
            "the downloaded update is not signed by the Erebor project: not installed",
            Some("nothing was changed; try again later or report it"),
        ),
        _ if code.starts_with("update_check_failed") => (
            "could not check for updates (offline, or no update published yet)",
            Some("try again later: erebor update"),
        ),
        _ if code.starts_with("update_write") => (
            "cannot replace this program (no permission on its folder?)",
            Some("reinstall: sh scripts/install.sh, or run the update as its owner"),
        ),
        "invalid_device_name" => (
            "device name must be 1-64 characters, no control characters",
            None,
        ),
        _ if code.starts_with("engine_busy") => (
            "another Erebor (agent or desktop app) already owns this data folder",
            Some("the CLI talks to it directly: erebor status"),
        ),
        _ => return (code.to_string(), None),
    };
    (m.to_string(), h)
}

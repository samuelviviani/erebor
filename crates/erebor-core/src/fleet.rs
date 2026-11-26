//! This computer's identity and typed, versioned snapshots of its miner, read over the
//! local socket by the CLI and the TUI.
//!
//! Compatibility: `SCHEMA_VERSION` changes only when a field is removed or changes
//! meaning. New optional fields can be added without a bump (readers ignore unknown
//! fields and default missing ones).

use crate::config::AppConfig;
use crate::engine::{Engine, Status};
use crate::hardware::HardwareInfo;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

const SCHEMA_VERSION: u32 = 1;

/// This computer's identity: a random id and a name. Kept in `device.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    /// Random, generated once; never derived from hardware serials or the user.
    pub id: String,
    pub name: String,
}

fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

fn valid_name(n: &str) -> bool {
    !n.is_empty() && n.chars().count() <= 64 && !n.chars().any(char::is_control)
}

fn save(data_dir: &Path, d: &Device) -> Result<(), String> {
    let text = serde_json::to_string_pretty(d).map_err(|e| e.to_string())?;
    std::fs::write(data_dir.join("device.json"), text).map_err(|e| e.to_string())
}

/// The device identity, created on first use.
pub fn device(data_dir: &Path) -> Result<Device, String> {
    let saved = std::fs::read_to_string(data_dir.join("device.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Device>(&s).ok())
        .filter(|d| valid_id(&d.id) && valid_name(&d.name));
    if let Some(d) = saved {
        return Ok(d);
    }
    let host: String = crate::rpc::hostname()
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect();
    let d = Device {
        id: (0..16)
            .map(|_| format!("{:02x}", rand::random::<u8>()))
            .collect(),
        name: if host.is_empty() {
            "Erebor".into()
        } else {
            host
        },
    };
    save(data_dir, &d)?;
    Ok(d)
}

pub fn set_name(data_dir: &Path, name: &str) -> Result<Device, String> {
    let name = name.trim();
    if !valid_name(name) {
        return Err("invalid_device_name".into());
    }
    let mut d = device(data_dir)?;
    d.name = name.into();
    save(data_dir, &d)?;
    Ok(d)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct MinerSummary {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub supports_pause: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct GpuSummary {
    pub name: String,
    pub temp_c: Option<f32>,
    pub power_w: Option<f64>,
    pub util_percent: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct DevFeeSummary {
    pub enabled: bool,
    pub percent: f64,
    /// Who the miner works for right now: "user" or "project".
    pub active: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct EarningsSummary {
    /// Estimated value per day after fees, in `currency`; `None` without a price.
    pub value_per_day: Option<f64>,
    pub currency: String,
    pub btc_per_day: f64,
    /// Based on the live hashrate (false: last measured one).
    pub live: bool,
    /// Electricity cost per day; `None` without a price per kWh or a power reading.
    pub energy_cost_per_day: Option<f64>,
    /// `value_per_day` minus `energy_cost_per_day`.
    pub net_per_day: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct JobSummary {
    pub kind: String,
    pub progress: f64,
    pub done: bool,
    pub error: Option<String>,
}

/// Miner state and metrics. Unavailable values are `null`, never invented.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Snapshot {
    pub schema: u32,
    /// "stopped" | "running" | "paused" | "error"
    pub state: String,
    pub demo: bool,
    pub error: Option<String>,
    /// Why mining cannot start right now (error code).
    pub start_blocker: Option<String>,
    pub miner: Option<MinerSummary>,
    pub coin: String,
    pub coin_name: String,
    pub algo: String,
    pub auto_coin: bool,
    /// Configured pool id from the catalog ("kryptex", "custom", …).
    pub pool_id: String,
    /// Pool the miner is connected to (host:port), when running.
    pub pool: Option<String>,
    pub pool_connected: bool,
    /// Current hashrate in H/s.
    pub hashrate: Option<f64>,
    pub uptime_secs: u64,
    pub accepted: u64,
    pub rejected: u64,
    pub cpu_temp_c: Option<f32>,
    pub gpus: Vec<GpuSummary>,
    pub power_w: Option<f64>,
    /// "measured" | "estimated" | "unavailable"
    pub power_source: String,
    pub dev_fee: DevFeeSummary,
    pub earnings: Option<EarningsSummary>,
    pub restart_in_secs: Option<u64>,
    pub job: Option<JobSummary>,
}

fn tag<T: Serialize>(v: T) -> String {
    match serde_json::to_value(v) {
        Ok(Value::String(s)) => s,
        _ => String::new(),
    }
}

impl Snapshot {
    pub fn new(s: &Status, c: &AppConfig) -> Self {
        let stats = s.stats.as_ref();
        Self {
            schema: SCHEMA_VERSION,
            state: tag(s.state),
            demo: s.demo,
            error: s.error.clone(),
            start_blocker: s.start_blocker.clone(),
            miner: s.miner.as_ref().map(|m| MinerSummary {
                id: m.id.into(),
                name: m.name.into(),
                version: m.version.clone(),
                supports_pause: m.supports_pause,
            }),
            coin: s.coin.id.into(),
            coin_name: s.coin.name.into(),
            algo: s.coin.algo.into(),
            auto_coin: s.auto_coin,
            pool_id: c.pool.id.clone(),
            pool: stats.and_then(|t| t.pool.clone()),
            pool_connected: stats.is_some_and(|t| t.connected),
            hashrate: stats.and_then(|t| t.current_hashrate()),
            uptime_secs: s.session_secs as u64,
            accepted: stats.map_or(0, |t| t.accepted),
            rejected: stats.map_or(0, |t| t.rejected),
            cpu_temp_c: s.hardware.cpu_temp_c,
            gpus: s
                .hardware
                .gpus
                .iter()
                .map(|g| GpuSummary {
                    name: g.name.clone(),
                    temp_c: g.temp_c,
                    power_w: g.power_w,
                    util_percent: g.util_percent,
                })
                .collect(),
            power_w: s.power_w,
            power_source: tag(s.power_source),
            dev_fee: DevFeeSummary {
                enabled: s.devfee.enabled,
                percent: s.devfee.percent,
                active: tag(s.devfee.active),
            },
            earnings: s.projection.as_ref().map(|p| EarningsSummary {
                value_per_day: p.value_per_day,
                currency: c.energy.currency.clone(),
                btc_per_day: p.btc_per_day,
                live: p.live,
                energy_cost_per_day: p.energy_cost_per_day,
                net_per_day: p.net_per_day,
            }),
            restart_in_secs: s.restart_in_secs,
            job: s.job.as_ref().map(|j| JobSummary {
                kind: j.kind.into(),
                progress: j.progress,
                done: j.done,
                error: j.error.clone(),
            }),
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self.state.as_str(), "running" | "paused")
    }
}

/// Everything a fleet view needs to show this device.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceReport {
    pub schema: u32,
    pub device: Device,
    pub hostname: String,
    pub agent_version: &'static str,
    pub hardware: HardwareInfo,
    pub snapshot: Snapshot,
}

pub fn report(e: &mut Engine) -> Result<DeviceReport, String> {
    Ok(DeviceReport {
        schema: SCHEMA_VERSION,
        device: device(e.data_dir())?,
        hostname: crate::rpc::hostname(),
        agent_version: env!("CARGO_PKG_VERSION"),
        hardware: e.detect_hardware(),
        snapshot: Snapshot::new(&e.status(), e.config()),
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct GpuTelemetry {
    pub name: String,
    pub vendor: String,
    pub vram_mb: Option<u64>,
    pub mem_used_mb: Option<u64>,
    pub util_percent: Option<f32>,
    pub temp_c: Option<f32>,
    /// Not read on any platform yet: always `null` (unsupported), never estimated.
    pub hotspot_c: Option<f32>,
    pub fan_percent: Option<f32>,
    pub core_clock_mhz: Option<u32>,
    pub mem_clock_mhz: Option<u32>,
    pub power_w: Option<f64>,
    /// This card's share of the miner's hashrate (H/s).
    pub hashrate: Option<f64>,
}

/// Everything an agent reports about its computer, for the remote protocol
/// (`telemetry`, and the `subscribe` stream). Unavailable values are `null`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Telemetry {
    pub schema: u32,
    pub ts: i64,
    pub device: Device,
    pub hostname: String,
    pub os: String,
    pub arch: String,
    pub agent_version: String,
    pub addresses: Vec<String>,
    pub system_uptime_secs: Option<u64>,
    pub cpu_model: String,
    pub cpu_usage: Option<f32>,
    pub mem_used_bytes: Option<u64>,
    pub mem_total_bytes: Option<u64>,
    pub gpus: Vec<GpuTelemetry>,
    pub snapshot: Snapshot,
    /// Demo miner: simulated values, never real mining.
    pub simulated: bool,
    /// Latest warnings and errors (codes, as in the app's alerts).
    pub recent_errors: Vec<String>,
}

static HW: std::sync::Mutex<Option<(std::time::Instant, HardwareInfo)>> =
    std::sync::Mutex::new(None);

/// Hardware detection runs external tools (nvidia-smi, …): reuse the result for 10 minutes.
pub fn hardware_cached(e: &crate::rpc::Shared, refresh: bool) -> HardwareInfo {
    let mut c = HW.lock().unwrap_or_else(|p| p.into_inner());
    match &*c {
        Some((t, h)) if !refresh && t.elapsed() < std::time::Duration::from_secs(600) => h.clone(),
        _ => {
            let h = e
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .detect_hardware();
            *c = Some((std::time::Instant::now(), h.clone()));
            h
        }
    }
}

/// Telemetry of the engine in this process.
pub fn telemetry(e: &crate::rpc::Shared) -> Telemetry {
    let hw = hardware_cached(e, false);
    let g = e.lock().unwrap_or_else(|p| p.into_inner());
    Telemetry::new(
        &g.status(),
        g.config(),
        device(g.data_dir()).unwrap_or_default(),
        &hw,
    )
}

/// The address other computers reach this one at (the one used for outgoing traffic).
/// No packet is sent: connecting a UDP socket only picks the route.
pub fn primary_address() -> Option<String> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:9").ok()?;
    Some(s.local_addr().ok()?.ip().to_string())
}

impl Telemetry {
    pub fn new(s: &Status, c: &AppConfig, device: Device, hw: &HardwareInfo) -> Self {
        let h = &s.hardware;
        Self {
            schema: SCHEMA_VERSION,
            ts: crate::now_unix(),
            device,
            hostname: crate::rpc::hostname(),
            os: format!("{} {}", hw.os_name, hw.os_version)
                .trim()
                .to_string(),
            arch: hw.arch.clone(),
            agent_version: env!("CARGO_PKG_VERSION").into(),
            addresses: primary_address().into_iter().collect(),
            system_uptime_secs: Some(sysinfo::System::uptime()).filter(|u| *u > 0),
            cpu_model: hw.cpu_brand.clone(),
            cpu_usage: Some(h.cpu_usage),
            mem_used_bytes: Some(h.mem_used_bytes).filter(|m| *m > 0),
            mem_total_bytes: Some(h.mem_total_bytes).filter(|m| *m > 0),
            gpus: h
                .gpus
                .iter()
                .enumerate()
                .map(|(i, g)| GpuTelemetry {
                    name: g.name.clone(),
                    vendor: g.vendor.clone(),
                    vram_mb: g
                        .mem_total_mb
                        .or_else(|| hw.gpus.get(i).and_then(|x| x.vram_mb)),
                    mem_used_mb: g.mem_used_mb,
                    util_percent: g.util_percent,
                    temp_c: g.temp_c,
                    hotspot_c: None,
                    fan_percent: g.fan_percent,
                    core_clock_mhz: None,
                    mem_clock_mhz: None,
                    power_w: g.power_w,
                    hashrate: s.gpus.get(i).and_then(|r| r.hashrate),
                })
                .collect(),
            snapshot: Snapshot::new(s, c),
            simulated: s.demo,
            recent_errors: s
                .alerts
                .iter()
                .rev()
                .filter(|a| a.level != crate::engine::Level::Info)
                .take(5)
                .map(|a| match &a.detail {
                    Some(d) => format!("{}: {d}", a.code),
                    None => a.code.clone(),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_identity_is_stable_and_names_are_validated() {
        let d = tempfile::tempdir().unwrap();
        let a = device(d.path()).unwrap();
        assert!(valid_id(&a.id));
        assert_eq!(device(d.path()).unwrap(), a);
        assert_eq!(set_name(d.path(), "  rig-1 ").unwrap().name, "rig-1");
        assert_eq!(device(d.path()).unwrap().id, a.id);
        assert_eq!(set_name(d.path(), ""), Err("invalid_device_name".into()));
        assert_eq!(
            set_name(d.path(), "a\nb"),
            Err("invalid_device_name".into())
        );
        assert!(set_name(d.path(), &"x".repeat(65)).is_err());
    }

    #[test]
    fn snapshot_json_is_stable() {
        let d = tempfile::tempdir().unwrap();
        let e = Engine::new(d.path().into(), crate::engine::default_factory()).unwrap();
        let s = Snapshot::new(&e.status(), e.config());
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["schema"], 1);
        assert_eq!(v["state"], "stopped");
        assert_eq!(v["hashrate"], Value::Null, "no invented metrics");
        assert_eq!(v["devFee"]["active"], "user");
        // Round trip, and old/new readers tolerate missing or extra fields.
        assert_eq!(serde_json::from_value::<Snapshot>(v).unwrap(), s);
        let partial: Snapshot =
            serde_json::from_str(r#"{"schema":1,"state":"running","future":1}"#).unwrap();
        assert!(partial.is_active());
    }
}

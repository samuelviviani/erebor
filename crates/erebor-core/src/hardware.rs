//! Hardware detection and live readings, with platform-specific adapters.
//! Anything the OS does not expose is reported as `None`, never guessed.

use serde::Serialize;
use std::process::Stdio;
use sysinfo::{Components, System};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    pub name: String,
    pub vendor: Option<String>,
    pub driver: Option<String>,
    pub vram_mb: Option<u64>,
    /// Detection source (nvidia-smi, system_profiler, CIM, sysfs).
    pub source: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareInfo {
    pub cpu_brand: String,
    pub physical_cores: Option<usize>,
    pub logical_threads: usize,
    pub arch: String,
    pub ram_total_bytes: u64,
    pub os_name: String,
    pub os_version: String,
    pub gpus: Vec<GpuInfo>,
    pub temperature_supported: bool,
    pub power_supported: bool,
    /// RandomX is CPU-oriented; GPUs are detected but not used by this release.
    pub gpu_mining_supported: bool,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LiveHardware {
    pub cpu_usage: f32,
    pub mem_used_bytes: u64,
    pub mem_total_bytes: u64,
    pub cpu_temp_c: Option<f32>,
    /// Measured package power (W), when the platform exposes it.
    pub power_w: Option<f64>,
    /// Live readings for each graphics card (NVIDIA via nvidia-smi, AMD via sysfs).
    pub gpus: Vec<GpuLive>,
}

impl LiveHardware {
    /// Hottest known temperature among CPU and GPUs (for the safety stop).
    pub fn max_temp_c(&self) -> Option<f32> {
        self.gpus
            .iter()
            .filter_map(|g| g.temp_c)
            .chain(self.cpu_temp_c)
            .fold(None, |a: Option<f32>, t| Some(a.map_or(t, |a| a.max(t))))
    }
}

/// One graphics card right now. Missing values stay `None`.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GpuLive {
    pub name: String,
    pub vendor: String,
    pub temp_c: Option<f32>,
    pub util_percent: Option<f32>,
    pub power_w: Option<f64>,
    pub fan_percent: Option<f32>,
    pub mem_used_mb: Option<u64>,
    pub mem_total_mb: Option<u64>,
}

/// GPU readings spawn a process (nvidia-smi): poll at most every few seconds and back
/// off for a minute when no source answers.
const GPU_POLL: std::time::Duration = std::time::Duration::from_secs(4);
const GPU_RETRY: std::time::Duration = std::time::Duration::from_secs(60);

pub struct Monitor {
    sys: System,
    components: Components,
    rapl: Rapl,
    gpus: Vec<GpuLive>,
    gpus_at: Option<std::time::Instant>,
}

impl Default for Monitor {
    fn default() -> Self {
        Self::new()
    }
}

impl Monitor {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_cpu();
        sys.refresh_memory();
        Self {
            sys,
            components: Components::new_with_refreshed_list(),
            rapl: Rapl::default(),
            gpus: Vec::new(),
            gpus_at: None,
        }
    }

    fn live_gpus(&mut self) -> Vec<GpuLive> {
        let wait = if self.gpus.is_empty() {
            GPU_RETRY
        } else {
            GPU_POLL
        };
        if self.gpus_at.is_none_or(|t| t.elapsed() >= wait) {
            self.gpus_at = Some(std::time::Instant::now());
            self.gpus = read_gpus_live();
        }
        self.gpus.clone()
    }

    pub fn threads(&self) -> usize {
        self.sys.cpus().len().max(1)
    }

    pub fn detect(&mut self) -> HardwareInfo {
        let gpus = detect_gpus();
        self.sys.refresh_cpu();
        self.sys.refresh_memory();
        let live = self.live();
        HardwareInfo {
            cpu_brand: self
                .sys
                .cpus()
                .first()
                .map(|c| c.brand().trim().to_string())
                .unwrap_or_default(),
            physical_cores: self.sys.physical_core_count(),
            logical_threads: self.sys.cpus().len(),
            arch: std::env::consts::ARCH.to_string(),
            ram_total_bytes: self.sys.total_memory(),
            os_name: System::name().unwrap_or_else(|| std::env::consts::OS.into()),
            os_version: System::os_version().unwrap_or_default(),
            gpu_mining_supported: mining_gpu(&gpus)
                && crate::install::manifest_for(crate::miner::MinerKind::Srbminer)
                    .asset_for_platform()
                    .is_ok(),
            gpus,
            temperature_supported: live.cpu_temp_c.is_some(),
            power_supported: self.rapl.supported(),
        }
    }

    pub fn live(&mut self) -> LiveHardware {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.components.refresh();
        LiveHardware {
            cpu_usage: self.sys.global_cpu_info().cpu_usage(),
            mem_used_bytes: self.sys.used_memory(),
            mem_total_bytes: self.sys.total_memory(),
            cpu_temp_c: cpu_temperature(&self.components),
            power_w: self.rapl.read(),
            gpus: self.live_gpus(),
        }
    }
}

/// An NVIDIA or AMD GPU: the vendors SRBMiner supports for every catalog coin.
pub fn mining_gpu(gpus: &[GpuInfo]) -> bool {
    gpus.iter().any(|g| {
        let s = format!("{} {}", g.vendor.as_deref().unwrap_or(""), g.name).to_ascii_lowercase();
        ["nvidia", "geforce", "amd", "radeon"]
            .iter()
            .any(|k| s.contains(k))
    })
}

fn cpu_temperature(components: &Components) -> Option<f32> {
    const KEYS: [&str; 7] = ["cpu", "package", "tctl", "tdie", "core", "soc", "pmu tdie"];
    components
        .iter()
        .filter(|c| {
            let l = c.label().to_ascii_lowercase();
            KEYS.iter().any(|k| l.contains(k))
        })
        .map(|c| c.temperature())
        .filter(|t| t.is_finite() && *t > 0.0 && *t < 150.0)
        .fold(None, |acc: Option<f32>, t| {
            Some(acc.map_or(t, |a| a.max(t)))
        })
}

/// Linux Intel/AMD RAPL package energy counter (often root-only on recent kernels).
#[derive(Default)]
struct Rapl {
    last: Option<(u64, std::time::Instant)>,
}

impl Rapl {
    const PATH: &'static str = "/sys/class/powercap/intel-rapl:0/energy_uj";

    fn supported(&self) -> bool {
        cfg!(target_os = "linux") && std::fs::read_to_string(Self::PATH).is_ok()
    }

    fn read(&mut self) -> Option<f64> {
        if !cfg!(target_os = "linux") {
            return None;
        }
        let uj: u64 = std::fs::read_to_string(Self::PATH)
            .ok()?
            .trim()
            .parse()
            .ok()?;
        let now = std::time::Instant::now();
        let prev = self.last.replace((uj, now));
        let (puj, pt) = prev?;
        let dt = now.duration_since(pt).as_secs_f64();
        (uj >= puj && dt > 0.5).then(|| (uj - puj) as f64 / 1e6 / dt)
    }
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = crate::command(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn parse_nvidia_smi(out: &str) -> Vec<GpuInfo> {
    out.lines()
        .filter_map(|l| {
            let f: Vec<_> = l.split(',').map(str::trim).collect();
            (f.len() >= 3 && !f[0].is_empty()).then(|| GpuInfo {
                name: f[0].to_string(),
                vendor: Some("NVIDIA".into()),
                driver: Some(f[1].to_string()),
                vram_mb: f[2].parse().ok(),
                source: "nvidia-smi",
            })
        })
        .collect()
}

/// Number from nvidia-smi / sysfs text; "[N/A]", "[Not Supported]" and junk → None.
fn num(s: &str) -> Option<f64> {
    s.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v >= 0.0)
}

/// `nvidia-smi --query-gpu=name,temperature.gpu,utilization.gpu,power.draw,fan.speed,
/// memory.used,memory.total --format=csv,noheader,nounits`
fn parse_nvidia_smi_live(out: &str) -> Vec<GpuLive> {
    out.lines()
        .filter_map(|l| {
            let f: Vec<_> = l.split(',').map(str::trim).collect();
            (f.len() >= 7 && !f[0].is_empty()).then(|| GpuLive {
                name: f[0].to_string(),
                vendor: "NVIDIA".into(),
                temp_c: num(f[1]).map(|v| v as f32),
                util_percent: num(f[2]).map(|v| v as f32),
                power_w: num(f[3]),
                fan_percent: num(f[4]).map(|v| v as f32),
                mem_used_mb: num(f[5]).map(|v| v as u64),
                mem_total_mb: num(f[6]).map(|v| v as u64),
            })
        })
        .collect()
}

fn read_gpus_live() -> Vec<GpuLive> {
    let nvidia = run(
        "nvidia-smi",
        &[
            "--query-gpu=name,temperature.gpu,utilization.gpu,power.draw,fan.speed,memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ],
    )
    .map(|o| parse_nvidia_smi_live(&o))
    .unwrap_or_default();
    let mut all = nvidia;
    all.extend(amd_sysfs_live());
    all
}

/// AMD cards on Linux: amdgpu exposes load, temperature, power, fan and VRAM in sysfs.
fn amd_sysfs_live() -> Vec<GpuLive> {
    if !cfg!(target_os = "linux") {
        return Vec::new();
    }
    let read = |p: &std::path::Path| std::fs::read_to_string(p).ok().and_then(|s| num(&s));
    let Ok(dir) = std::fs::read_dir("/sys/class/drm") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in dir.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let dev = e.path().join("device");
        if std::fs::read_to_string(dev.join("vendor"))
            .unwrap_or_default()
            .trim()
            != "0x1002"
        {
            continue;
        }
        let hwmon = std::fs::read_dir(dev.join("hwmon"))
            .ok()
            .and_then(|mut d| d.next())
            .and_then(|e| e.ok())
            .map(|e| e.path());
        let h = |f: &str| hwmon.as_ref().and_then(|p| read(&p.join(f)));
        out.push(GpuLive {
            name: format!("AMD GPU ({name})"),
            vendor: "AMD".into(),
            temp_c: h("temp1_input").map(|v| (v / 1000.0) as f32),
            util_percent: read(&dev.join("gpu_busy_percent")).map(|v| v as f32),
            power_w: h("power1_average")
                .or_else(|| h("power1_input"))
                .map(|v| v / 1e6),
            fan_percent: h("pwm1").map(|v| (v / 255.0 * 100.0) as f32),
            mem_used_mb: read(&dev.join("mem_info_vram_used")).map(|v| (v / 1_048_576.0) as u64),
            mem_total_mb: read(&dev.join("mem_info_vram_total")).map(|v| (v / 1_048_576.0) as u64),
        });
    }
    out
}

fn detect_gpus() -> Vec<GpuInfo> {
    let mut gpus = run(
        "nvidia-smi",
        &[
            "--query-gpu=name,driver_version,memory.total",
            "--format=csv,noheader,nounits",
        ],
    )
    .map(|o| parse_nvidia_smi(&o))
    .unwrap_or_default();
    if gpus.is_empty() {
        gpus = platform_gpus();
    }
    gpus
}

#[cfg(target_os = "macos")]
fn platform_gpus() -> Vec<GpuInfo> {
    let Some(out) = run("system_profiler", &["SPDisplaysDataType", "-json"]) else {
        return vec![];
    };
    let v: serde_json::Value = serde_json::from_str(&out).unwrap_or_default();
    v["SPDisplaysDataType"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|g| {
                    Some(GpuInfo {
                        name: g["sppci_model"].as_str()?.to_string(),
                        vendor: g["spdisplays_vendor"].as_str().map(String::from),
                        driver: None,
                        vram_mb: None,
                        source: "system_profiler",
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(target_os = "windows")]
fn platform_gpus() -> Vec<GpuInfo> {
    let Some(out) = run(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-CimInstance Win32_VideoController | Select-Object Name,DriverVersion | ConvertTo-Json",
        ],
    ) else {
        return vec![];
    };
    let v: serde_json::Value = serde_json::from_str(&out).unwrap_or_default();
    let items = match v {
        serde_json::Value::Array(a) => a,
        o @ serde_json::Value::Object(_) => vec![o],
        _ => vec![],
    };
    items
        .iter()
        .filter_map(|g| {
            Some(GpuInfo {
                name: g["Name"].as_str()?.to_string(),
                vendor: None,
                driver: g["DriverVersion"].as_str().map(String::from),
                vram_mb: None,
                source: "CIM",
            })
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn platform_gpus() -> Vec<GpuInfo> {
    let Ok(dir) = std::fs::read_dir("/sys/class/drm") else {
        return vec![];
    };
    let mut out = vec![];
    for e in dir.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let vendor = std::fs::read_to_string(e.path().join("device/vendor")).unwrap_or_default();
        let vendor = match vendor.trim() {
            "0x10de" => "NVIDIA",
            "0x1002" => "AMD",
            "0x8086" => "Intel",
            _ => continue,
        };
        out.push(GpuInfo {
            name: format!("{vendor} GPU ({name})"),
            vendor: Some(vendor.into()),
            driver: None,
            vram_mb: None,
            source: "sysfs",
        });
    }
    out
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn platform_gpus() -> Vec<GpuInfo> {
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mining_gpus_are_nvidia_or_amd() {
        let g = |name: &str, vendor: Option<&str>| GpuInfo {
            name: name.into(),
            vendor: vendor.map(String::from),
            driver: None,
            vram_mb: None,
            source: "test",
        };
        // nvidia-smi gives no vendor: the name is enough.
        assert!(mining_gpu(&[g("NVIDIA GeForce RTX 4070", None)]));
        assert!(mining_gpu(&[g("Radeon RX 7800 XT", Some("AMD"))]));
        assert!(!mining_gpu(&[g("Apple M3", Some("sppci_vendor_Apple"))]));
        assert!(!mining_gpu(&[g("Intel UHD Graphics 770", Some("Intel"))]));
        assert!(!mining_gpu(&[]));
    }

    #[test]
    fn parses_nvidia_smi_csv() {
        let g = parse_nvidia_smi("NVIDIA GeForce RTX 3060, 550.54.14, 12288\n\n");
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].vram_mb, Some(12288));
        assert_eq!(g[0].driver.as_deref(), Some("550.54.14"));
    }

    #[test]
    fn detects_this_machine() {
        let mut m = Monitor::new();
        let h = m.detect();
        assert!(h.logical_threads >= 1);
        assert!(h.ram_total_bytes > 0);
        let l = m.live();
        assert!(l.mem_total_bytes > 0);
        if let Some(t) = l.cpu_temp_c {
            assert!(t > 0.0 && t < 150.0);
        }
    }

    #[test]
    fn parses_nvidia_live_readings() {
        let out = "NVIDIA GeForce RTX 4070, 61, 98, 182.35, 55, 7012, 12282\nOld GPU, 45, [N/A], [Not Supported], [N/A], 300, 2048\n";
        let g = parse_nvidia_smi_live(out);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].name, "NVIDIA GeForce RTX 4070");
        assert_eq!(g[0].temp_c, Some(61.0));
        assert_eq!(g[0].util_percent, Some(98.0));
        assert_eq!(g[0].power_w, Some(182.35));
        assert_eq!(g[0].mem_total_mb, Some(12282));
        assert_eq!(
            (g[1].util_percent, g[1].power_w, g[1].fan_percent),
            (None, None, None)
        );
        let live = LiveHardware {
            cpu_temp_c: Some(70.0),
            gpus: g,
            ..Default::default()
        };
        assert_eq!(live.max_temp_c(), Some(70.0));
    }
}

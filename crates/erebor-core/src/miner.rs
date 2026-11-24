//! Miner abstraction: XMRig (CPU, RandomX), SRBMiner-MULTI and lolMiner (GPU) and a demo miner that
//! produces clearly-labelled simulated data and never starts a process.

use serde::{Deserialize, Serialize};

/// External miner programs Erebor can install and drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MinerKind {
    Xmrig,
    Srbminer,
    Lolminer,
    Cpuminer,
}

impl MinerKind {
    pub const ALL: [MinerKind; 4] = [
        MinerKind::Xmrig,
        MinerKind::Srbminer,
        MinerKind::Lolminer,
        MinerKind::Cpuminer,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MinerKind::Xmrig => "XMRig",
            MinerKind::Srbminer => "SRBMiner-MULTI",
            MinerKind::Lolminer => "lolMiner",
            MinerKind::Cpuminer => "cpuminer-opt",
        }
    }

    pub fn binary_name(self) -> &'static str {
        match (self, cfg!(windows)) {
            (MinerKind::Xmrig, false) => "xmrig",
            (MinerKind::Xmrig, true) => "xmrig.exe",
            (MinerKind::Srbminer, false) => "SRBMiner-MULTI",
            (MinerKind::Srbminer, true) => "SRBMiner-MULTI.exe",
            (MinerKind::Lolminer, false) => "lolMiner",
            (MinerKind::Lolminer, true) => "lolMiner.exe",
            (MinerKind::Cpuminer, _) => crate::cpuminer::best_binary(),
        }
    }

    /// Installation folder prefix under `miners/`.
    pub fn dir_prefix(self) -> &'static str {
        match self {
            MinerKind::Xmrig => "xmrig",
            MinerKind::Srbminer => "srbminer",
            MinerKind::Lolminer => "lolminer",
            MinerKind::Cpuminer => "cpuminer",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolTarget {
    /// host:port
    pub url: String,
    pub tls: bool,
    /// Wallet address that receives the work.
    pub user: String,
    /// Worker name / password field.
    pub pass: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    pub pool: PoolTarget,
    /// Algorithm in the miner's own naming ("rx/0", "pearlhash").
    pub algo: String,
    /// Percent of CPU threads (XMRig `cpu.max-threads-hint`).
    pub threads_percent: u8,
    /// 0 (idle) .. 5 (highest).
    pub priority: u8,
    /// Power limit for each GPU (W), restored by the miner when it exits.
    pub gpu_power_limit_w: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MinerInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub version: Option<String>,
    pub algorithms: Vec<&'static str>,
    pub supports_pause: bool,
    pub supports_hot_switch: bool,
    pub supports_benchmark: bool,
    /// Percent of time the miner donates to its own developers (XMRig: `donate-level`).
    pub builtin_donation_percent: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MinerStats {
    /// 10s / 60s / 15m averages (H/s). `None` while the miner warms up.
    pub hashrate_10s: Option<f64>,
    pub hashrate_60s: Option<f64>,
    pub hashrate_15m: Option<f64>,
    pub hashrate_max: Option<f64>,
    pub accepted: u64,
    pub rejected: u64,
    pub pool: Option<String>,
    pub connected: bool,
    pub tls: Option<bool>,
    pub ping_ms: Option<u64>,
    pub algo: Option<String>,
    pub version: Option<String>,
    pub paused: bool,
    pub threads: Option<u32>,
    /// Hashrate of each GPU in the miner's device order (empty when not reported).
    pub gpu_hashrates: Vec<f64>,
}

impl MinerStats {
    pub fn current_hashrate(&self) -> Option<f64> {
        self.hashrate_10s.or(self.hashrate_60s)
    }
}

pub trait MinerAdapter: Send {
    fn info(&self) -> MinerInfo;
    fn start(&mut self, spec: &LaunchSpec) -> Result<(), String>;
    /// Must work even when the miner API is unreachable (kills the process).
    fn stop(&mut self);
    fn set_paused(&mut self, paused: bool) -> Result<(), String>;
    /// Switch pool/wallet without restarting, when supported.
    fn switch_pool(&mut self, pool: &PoolTarget) -> Result<(), String>;
    fn stats(&mut self) -> Result<MinerStats, String>;
    /// `Some(reason)` once if the process terminated without `stop()`.
    fn take_exit(&mut self) -> Option<String>;
    fn logs(&self) -> Vec<String>;
    fn pid(&self) -> Option<u32>;
}

/// Simulated miner for demo mode. No process, no network.
pub struct DemoMiner {
    running: bool,
    paused: bool,
    t: f64,
    accepted: u64,
    pool: Option<String>,
    full: f64,
    base: f64,
    logs: Vec<String>,
}

impl DemoMiner {
    pub fn new(threads: usize) -> Self {
        Self {
            running: false,
            paused: false,
            t: 0.0,
            accepted: 0,
            pool: None,
            full: 600.0 * threads.max(1) as f64,
            base: 0.0,
            logs: vec![],
        }
    }
}

impl MinerAdapter for DemoMiner {
    fn info(&self) -> MinerInfo {
        MinerInfo {
            id: "demo",
            name: "Demo (simulated)",
            version: Some("demo".into()),
            algorithms: vec!["rx/0"],
            supports_pause: true,
            supports_hot_switch: true,
            supports_benchmark: false,
            builtin_donation_percent: 0.0,
        }
    }
    fn start(&mut self, spec: &LaunchSpec) -> Result<(), String> {
        self.running = true;
        self.base = self.full * spec.threads_percent as f64 / 100.0;
        self.pool = Some(spec.pool.url.clone());
        self.logs.push("[DEMO] simulated miner started".into());
        Ok(())
    }
    fn stop(&mut self) {
        self.running = false;
        self.logs.push("[DEMO] simulated miner stopped".into());
    }
    fn set_paused(&mut self, paused: bool) -> Result<(), String> {
        self.paused = paused;
        Ok(())
    }
    fn switch_pool(&mut self, pool: &PoolTarget) -> Result<(), String> {
        self.pool = Some(pool.url.clone());
        Ok(())
    }
    fn stats(&mut self) -> Result<MinerStats, String> {
        if !self.running {
            return Err("not running".into());
        }
        self.t += 1.0;
        let h = if self.paused {
            0.0
        } else {
            self.base * (1.0 + 0.04 * (self.t / 7.0).sin())
        };
        if !self.paused && (self.t as u64).is_multiple_of(15) {
            self.accepted += 1;
        }
        Ok(MinerStats {
            hashrate_10s: Some(h),
            hashrate_60s: Some(self.base),
            hashrate_15m: Some(self.base),
            hashrate_max: Some(self.base * 1.04),
            accepted: self.accepted,
            rejected: 0,
            pool: self.pool.clone(),
            connected: true,
            tls: Some(true),
            ping_ms: Some(42),
            algo: Some("rx/0".into()),
            version: Some("demo".into()),
            paused: self.paused,
            threads: None,
            gpu_hashrates: Vec::new(),
        })
    }
    fn take_exit(&mut self) -> Option<String> {
        None
    }
    fn logs(&self) -> Vec<String> {
        self.logs.clone()
    }
    fn pid(&self) -> Option<u32> {
        None
    }
}

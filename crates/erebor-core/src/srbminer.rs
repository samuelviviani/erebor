//! External miner adapter: SRBMiner-MULTI, lolMiner and cpuminer-opt (see `lolminer`,
//! `cpuminer`). Both are closed-source
//! freeware with their own developer fee per algorithm; Erebor downloads the official
//! release and runs it unmodified, as a separate process, with fixed arguments and no shell.
//!
//! The statistics API (read-only) is polled on a random loopback port. Neither miner has
//! a hot pool switch or a pause: switching target restarts the miner.

use crate::miner::{LaunchSpec, MinerAdapter, MinerInfo, MinerKind, MinerStats, PoolTarget};
use crate::xmrig::{command, free_loopback_port, spawn_error, spawn_log_reader, LogBuffer};
use serde_json::Value;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Command-line arguments for a GPU-only run.
pub fn args(spec: &LaunchSpec, api_port: u16) -> Vec<String> {
    let p = &spec.pool;
    [
        "--disable-cpu",
        "--algorithm",
        &spec.algo,
        "--pool",
        &p.url,
        "--wallet",
        &p.user,
        "--password",
        &p.pass,
        "--tls",
        if p.tls { "true" } else { "false" },
        "--api-enable",
        "--api-port",
        &api_port.to_string(),
        "--api-rig-name",
        "erebor",
        "--retry-time",
        "10",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain(
        gpu_intensity(spec.gpu_percent)
            .into_iter()
            .flat_map(|i| ["--gpu-intensity".to_string(), i.to_string()]),
    )
    .chain(spec.gpu_power_limit_w.into_iter().flat_map(|w| {
        // Absolute watts for every GPU; the reset restores stock power on exit.
        [
            "--gpu-plimit".to_string(),
            w.to_string(),
            "--gpu-reset-oc".into(),
        ]
    }))
    .collect()
}

/// SRBMiner's `--gpu-intensity` (1..=31, `--help` of 3.8.8) for a share below 100%:
/// smaller GPU batches leave room for the desktop. `None` at 100%: the miner's own tuning.
// ponytail: linear share → intensity; tune per algorithm if users report it off.
fn gpu_intensity(percent: u8) -> Option<u8> {
    (percent < 100).then(|| ((percent as u32 * 31 + 50) / 100).clamp(1, 31) as u8)
}

fn num(v: Option<&Value>) -> Option<f64> {
    let v = v?;
    v.as_f64()
        .or_else(|| v.as_str()?.trim().parse().ok())
        .filter(|x: &f64| x.is_finite() && *x >= 0.0)
}

/// Tolerant parser: the API layout is not formally documented, so only values found
/// where expected are reported; anything missing stays `None` (never invented).
pub fn parse_stats(v: &Value) -> MinerStats {
    let algo = v
        .get("algorithms")
        .and_then(|a| a.as_array())
        .and_then(|a| a.first());
    let hr = algo.and_then(|a| a.get("hashrate"));
    let total = hr.and_then(|h| {
        num(h.get("gpu").and_then(|g| g.get("total")))
            .or_else(|| num(h.get("total")))
            .or_else(|| num(h.get("now")))
            .or_else(|| num(Some(h)))
    });
    let one_min = hr
        .and_then(|h| num(h.get("gpu").and_then(|g| g.get("1min"))).or_else(|| num(h.get("1min"))));
    let shares = algo.and_then(|a| a.get("shares"));
    let count = |k: &str| num(shares.and_then(|s| s.get(k))).unwrap_or(0.0) as u64;
    let pool = algo.and_then(|a| a.get("pool"));
    let pool_url = pool
        .and_then(|p| p.get("pool").or_else(|| p.get("url")))
        .and_then(|u| u.as_str())
        .map(String::from);
    let accepted = count("accepted");
    // Per device: `"gpu": {"gpu0": …, "gpu1": …, "total": …}`, in device order.
    let mut per_gpu: Vec<(usize, f64)> = hr
        .and_then(|h| h.get("gpu"))
        .and_then(|g| g.as_object())
        .map(|g| {
            g.iter()
                .filter_map(|(k, v)| Some((k.strip_prefix("gpu")?.parse().ok()?, num(Some(v))?)))
                .collect()
        })
        .unwrap_or_default();
    per_gpu.sort_by_key(|(i, _)| *i);
    MinerStats {
        hashrate_10s: total.filter(|h| *h > 0.0),
        hashrate_60s: one_min.or(total).filter(|h| *h > 0.0),
        hashrate_15m: None,
        hashrate_max: None,
        accepted,
        rejected: count("rejected"),
        connected: pool_url.is_some() || accepted > 0,
        pool: pool_url,
        tls: None,
        ping_ms: num(pool.and_then(|p| p.get("latency").or_else(|| p.get("ping"))))
            .map(|x| x as u64),
        algo: algo
            .and_then(|a| a.get("name"))
            .and_then(|n| n.as_str())
            .map(String::from),
        version: v
            .get("miner_version")
            .and_then(|x| x.as_str())
            .map(String::from),
        paused: false,
        threads: None,
        gpu_hashrates: per_gpu.into_iter().map(|(_, h)| h).collect(),
    }
}

pub struct ExternalMiner {
    kind: MinerKind,
    binary: PathBuf,
    work_dir: PathBuf,
    version: Option<String>,
    child: Option<Child>,
    api_port: u16,
    logs: LogBuffer,
    http: ureq::Agent,
}

impl ExternalMiner {
    pub fn new(
        kind: MinerKind,
        binary: PathBuf,
        work_dir: PathBuf,
        version: Option<String>,
    ) -> Self {
        Self {
            kind,
            binary,
            work_dir,
            version,
            child: None,
            api_port: 0,
            logs: Arc::new(Mutex::new(VecDeque::new())),
            http: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(3))
                .build(),
        }
    }
}

impl MinerAdapter for ExternalMiner {
    fn info(&self) -> MinerInfo {
        MinerInfo {
            id: self.kind.dir_prefix(),
            name: self.kind.label(),
            version: self.version.clone(),
            algorithms: crate::coins::COINS
                .iter()
                .filter(|c| c.miner == self.kind)
                .map(|c| c.algo)
                .collect(),
            supports_pause: false,
            supports_hot_switch: false,
            supports_benchmark: false,
            // Per-algorithm fee: the engine applies the coin's fee to estimates.
            builtin_donation_percent: 0.0,
        }
    }

    fn start(&mut self, spec: &LaunchSpec) -> Result<(), String> {
        if self.child.is_some() {
            return Err("already running".into());
        }
        std::fs::create_dir_all(&self.work_dir).map_err(|e| e.to_string())?;
        self.api_port = free_loopback_port()?;
        // Run from the miner's own folder: SRBMiner stores GPU autotune results there.
        let cwd = self.binary.parent().unwrap_or(&self.work_dir).to_path_buf();
        let args = match self.kind {
            MinerKind::Lolminer => crate::lolminer::args(spec, self.api_port),
            MinerKind::Cpuminer => crate::cpuminer::args(spec, self.api_port),
            _ => args(spec, self.api_port),
        };
        let mut child = command(&self.binary)
            .args(args)
            .current_dir(cwd)
            .spawn()
            .map_err(spawn_error)?;
        if let Some(o) = child.stdout.take() {
            spawn_log_reader(o, self.logs.clone(), None);
        }
        if let Some(e) = child.stderr.take() {
            spawn_log_reader(e, self.logs.clone(), None);
        }
        self.child = Some(child);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    fn set_paused(&mut self, _paused: bool) -> Result<(), String> {
        Err("pause_unsupported".into())
    }

    fn switch_pool(&mut self, _pool: &PoolTarget) -> Result<(), String> {
        Err("hot_switch_unsupported".into())
    }

    fn stats(&mut self) -> Result<MinerStats, String> {
        if self.kind == MinerKind::Cpuminer {
            return crate::cpuminer::query(self.api_port);
        }
        let v: Value = self
            .http
            .get(&format!("http://127.0.0.1:{}/", self.api_port))
            .call()
            .map_err(|e| e.to_string())?
            .into_json()
            .map_err(|e| e.to_string())?;
        Ok(match self.kind {
            MinerKind::Lolminer => crate::lolminer::parse_stats(&v),
            _ => parse_stats(&v),
        })
    }

    fn take_exit(&mut self) -> Option<String> {
        let status = self.child.as_mut()?.try_wait().ok()??;
        self.child = None;
        Some(format!(
            "{} exited unexpectedly ({status})",
            self.kind.label()
        ))
    }

    fn logs(&self) -> Vec<String> {
        self.logs.lock().unwrap().iter().cloned().collect()
    }

    fn pid(&self) -> Option<u32> {
        self.child.as_ref().map(|c| c.id())
    }
}

impl Drop for ExternalMiner {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn args_are_fixed_and_gpu_only() {
        let spec = LaunchSpec {
            pool: PoolTarget {
                url: "prl.kryptex.network:7048".into(),
                tls: false,
                user: "krxABC123/erebor".into(),
                pass: "x".into(),
            },
            algo: "pearlhash".into(),
            threads_percent: 50,
            priority: 1,
            gpu_power_limit_w: None,
            gpu_percent: 100,
        };
        let a = args(&spec, 40123);
        assert_eq!(a[0], "--disable-cpu");
        let at = |k: &str| a[a.iter().position(|x| x == k).unwrap() + 1].clone();
        assert_eq!(at("--algorithm"), "pearlhash");
        assert_eq!(at("--pool"), "prl.kryptex.network:7048");
        assert_eq!(at("--wallet"), "krxABC123/erebor");
        assert_eq!(at("--api-port"), "40123");
        assert!(!a
            .iter()
            .any(|x| x.contains("restart-url") || x.contains("shutdown-url")));
        assert!(!a.iter().any(|x| x.starts_with("--gpu-")));
        let limited = args(
            &LaunchSpec {
                gpu_power_limit_w: Some(150),
                ..spec.clone()
            },
            40123,
        );
        let at = |k: &str| limited[limited.iter().position(|x| x == k).unwrap() + 1].clone();
        assert_eq!(at("--gpu-plimit"), "150");
        assert!(limited.contains(&"--gpu-reset-oc".to_string()));
        // A lower share means a lower intensity; full share leaves the miner's tuning.
        let light = args(
            &LaunchSpec {
                gpu_percent: 25,
                ..spec.clone()
            },
            40123,
        );
        let at = |k: &str| light[light.iter().position(|x| x == k).unwrap() + 1].clone();
        assert_eq!(at("--gpu-intensity"), "8");
        assert_eq!(gpu_intensity(50), Some(16));
        assert_eq!(gpu_intensity(5), Some(2));
        assert_eq!(gpu_intensity(100), None);
    }

    #[test]
    fn parses_stats_tolerantly() {
        let v = json!({"miner_version": "3.8.8", "algorithms": [{
            "name": "pearlhash",
            "hashrate": {"gpu": {"total": 3.1e9, "1min": 3.0e9, "gpu1": 1.0e9, "gpu0": 2.1e9}},
            "shares": {"accepted": 12, "rejected": 1},
            "pool": {"pool": "prl.kryptex.network:7048", "latency": 35}
        }]});
        let s = parse_stats(&v);
        assert_eq!(s.hashrate_10s, Some(3.1e9));
        assert_eq!(s.hashrate_60s, Some(3.0e9));
        assert_eq!(s.gpu_hashrates, vec![2.1e9, 1.0e9]);
        assert_eq!((s.accepted, s.rejected), (12, 1));
        assert!(s.connected);
        assert_eq!(s.ping_ms, Some(35));
        assert_eq!(s.algo.as_deref(), Some("pearlhash"));
        // Unknown layout: nothing invented.
        let empty = parse_stats(&json!({"foo": 1}));
        assert_eq!(empty.current_hashrate(), None);
        assert!(!empty.connected);
    }
}

//! cpuminer-opt specifics: SHA-256d and Scrypt coins (Bitcoin, Litecoin…) on the CPU.
//! Those coins are mined by ASICs: a PC finds very few shares, and the UI says so.
//! Upstream ships Windows builds only, one executable per CPU feature level plus DLLs;
//! Erebor installs the whole archive and runs the best build this CPU supports.
//! The process is driven by `srbminer::ExternalMiner`.

use crate::miner::{LaunchSpec, MinerStats};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// Best build for this CPU, from the most to the least capable (file names of the
/// official Windows archive).
pub fn best_binary() -> &'static str {
    #[cfg(target_arch = "x86_64")]
    {
        use std::arch::is_x86_feature_detected as has;
        if has!("avx512f") && has!("sha") && has!("vaes") {
            return "cpuminer-avx512-sha-vaes.exe";
        }
        if has!("avx512f") {
            return "cpuminer-avx512.exe";
        }
        if has!("avx2") && has!("sha") && has!("vaes") {
            return "cpuminer-avx2-sha-vaes.exe";
        }
        if has!("avx2") && has!("sha") {
            return "cpuminer-avx2-sha.exe";
        }
        if has!("avx2") {
            return "cpuminer-avx2.exe";
        }
        if has!("avx") {
            return "cpuminer-avx.exe";
        }
        if has!("aes") && has!("sse4.2") {
            return "cpuminer-aes-sse42.exe";
        }
    }
    "cpuminer-sse2.exe"
}

/// Command-line arguments. The API listens on loopback only.
pub fn args(spec: &LaunchSpec, api_port: u16) -> Vec<String> {
    let p = &spec.pool;
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let threads = (cores * spec.threads_percent as usize / 100).max(1);
    let scheme = if p.tls { "stratum+ssl" } else { "stratum+tcp" };
    vec![
        "--algo".into(),
        spec.algo.clone(),
        "--url".into(),
        format!("{scheme}://{}", p.url),
        "--user".into(),
        p.user.clone(),
        "--pass".into(),
        p.pass.clone(),
        "--threads".into(),
        threads.to_string(),
        "--cpu-priority".into(),
        spec.priority.min(5).to_string(),
        format!("--api-bind=127.0.0.1:{api_port}"),
    ]
}

/// `summary` over the text API: `NAME=cpuminer-opt;VER=26.1;…;KHS=12.3;ACC=4;REJ=0;…|`.
pub fn query(port: u16) -> Result<MinerStats, String> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(Duration::from_secs(3))).ok();
    s.write_all(b"summary").map_err(|e| e.to_string())?;
    let mut out = String::new();
    s.read_to_string(&mut out).map_err(|e| e.to_string())?;
    Ok(parse_summary(&out))
}

pub fn parse_summary(text: &str) -> MinerStats {
    let get = |k: &str| {
        text.trim_end_matches(['|', '\0', '\n'])
            .split(';')
            .find_map(|kv| kv.split_once('=').filter(|(key, _)| *key == k))
            .map(|(_, v)| v.trim())
    };
    let n = |k: &str| {
        get(k)
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|x| x.is_finite())
    };
    let rate = n("HS")
        .or_else(|| n("KHS").map(|k| k * 1e3))
        .filter(|h| *h > 0.0);
    let accepted = n("ACC").unwrap_or(0.0) as u64;
    MinerStats {
        hashrate_10s: rate,
        hashrate_60s: rate,
        hashrate_15m: None,
        hashrate_max: None,
        accepted,
        rejected: n("REJ").unwrap_or(0.0) as u64,
        connected: get("URL").is_some_and(|u| !u.is_empty()) || accepted > 0,
        pool: get("URL").filter(|u| !u.is_empty()).map(String::from),
        tls: None,
        ping_ms: None,
        algo: get("ALGO").map(String::from),
        version: get("VER").map(|v| format!("cpuminer-opt {v}")),
        paused: false,
        threads: n("CPUS").map(|c| c as u32),
        gpu_hashrates: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::miner::PoolTarget;

    #[test]
    fn args_and_summary() {
        let spec = LaunchSpec {
            pool: PoolTarget {
                url: "btc.kryptex.network:7014".into(),
                tls: false,
                user: "krxABC123/PC".into(),
                pass: "x".into(),
            },
            algo: "sha256d".into(),
            threads_percent: 50,
            priority: 1,
            gpu_power_limit_w: None,
            gpu_percent: 100,
        };
        let a = args(&spec, 40123);
        let at = |k: &str| a[a.iter().position(|x| x == k).unwrap() + 1].clone();
        assert_eq!(at("--url"), "stratum+tcp://btc.kryptex.network:7014");
        assert_eq!(at("--algo"), "sha256d");
        assert!(a.contains(&"--api-bind=127.0.0.1:40123".to_string()));
        let s = parse_summary(
            "NAME=cpuminer-opt;VER=26.1;API=1.0;ALGO=sha256d;CPUS=8;URL=stratum+tcp://btc.kryptex.network:7014;KHS=12500.00;ACC=3;REJ=1;UPTIME=60|",
        );
        assert_eq!(s.hashrate_60s, Some(12_500_000.0));
        assert_eq!((s.accepted, s.rejected, s.threads), (3, 1, Some(8)));
        assert!(s.connected);
        assert_eq!(parse_summary("garbage").current_hashrate(), None);
        assert!(best_binary().starts_with("cpuminer-"));
    }
}

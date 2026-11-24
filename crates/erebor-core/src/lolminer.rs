//! lolMiner specifics (GPU coins SRBMiner does not mine: Alephium, Conflux, Nexa, Kaspa,
//! Tari SHA3x and C29). The process is driven by `srbminer::ExternalMiner`; here: the fixed
//! command line and the statistics parser.

use crate::miner::{LaunchSpec, MinerStats};
use serde_json::Value;

/// Command-line arguments. The API listens on loopback only (lolMiner's default is every
/// interface).
pub fn args(spec: &LaunchSpec, api_port: u16) -> Vec<String> {
    let p = &spec.pool;
    [
        "--algo",
        &spec.algo,
        "--pool",
        &p.url,
        "--user",
        &p.user,
        "--pass",
        &p.pass,
        "--tls",
        if p.tls { "on" } else { "off" },
        "--apihost",
        "127.0.0.1",
        "--apiport",
        &api_port.to_string(),
    ]
    .iter()
    .map(|s| s.to_string())
    // Same value for every NVIDIA card; lolMiner resets it on exit.
    .chain(
        spec.gpu_power_limit_w
            .into_iter()
            .flat_map(|w| ["--pl".to_string(), w.to_string()]),
    )
    .collect()
}

fn num(v: Option<&Value>) -> Option<f64> {
    v?.as_f64().filter(|x| x.is_finite() && *x >= 0.0)
}

/// Unit label ("Mh/s", "g/s") to H/s, when the API gives no explicit factor.
fn unit_factor(unit: &str) -> f64 {
    match unit.trim().chars().next().map(|c| c.to_ascii_lowercase()) {
        Some('k') => 1e3,
        Some('m') => 1e6,
        Some('g') if unit.trim().len() > 3 => 1e9, // "Gh/s", not "g/s" (graphs)
        Some('t') => 1e12,
        Some('p') => 1e15,
        _ => 1.0,
    }
}

/// Tolerant parser for lolMiner's API (`Algorithms[0]`): values not found stay `None`.
pub fn parse_stats(v: &Value) -> MinerStats {
    let a = v
        .get("Algorithms")
        .and_then(|a| a.as_array())
        .and_then(|a| a.first());
    let field = |k: &str| a.and_then(|a| a.get(k));
    let factor = num(field("Performance_Factor"))
        .filter(|f| *f > 0.0)
        .or_else(|| field("Performance_Unit")?.as_str().map(unit_factor))
        .unwrap_or(1.0);
    let rate = num(field("Total_Performance")).map(|h| h * factor);
    let count = |k: &str| num(field(k)).unwrap_or(0.0) as u64;
    let pool = field("Pool").and_then(|p| p.as_str()).map(String::from);
    let accepted = count("Total_Accepted");
    let per_gpu: Vec<f64> = field("Worker_Performance")
        .and_then(|w| w.as_array())
        .map(|w| {
            w.iter()
                .filter_map(|h| num(Some(h)).map(|h| h * factor))
                .collect()
        })
        .unwrap_or_default();
    MinerStats {
        hashrate_10s: rate.filter(|h| *h > 0.0),
        hashrate_60s: rate.filter(|h| *h > 0.0),
        hashrate_15m: None,
        hashrate_max: None,
        accepted,
        rejected: count("Total_Rejected") + count("Total_Stales"),
        connected: pool.is_some() || accepted > 0,
        pool,
        tls: None,
        ping_ms: None,
        algo: field("Algorithm")
            .and_then(|x| x.as_str())
            .map(String::from),
        version: v.get("Software").and_then(|x| x.as_str()).map(String::from),
        paused: false,
        threads: None,
        gpu_hashrates: per_gpu,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::miner::PoolTarget;
    use serde_json::json;

    #[test]
    fn args_bind_the_api_to_loopback() {
        let spec = LaunchSpec {
            pool: PoolTarget {
                url: "alph.kryptex.network:7010".into(),
                tls: false,
                user: "krxABC123/PC".into(),
                pass: "x".into(),
            },
            algo: "ALEPH".into(),
            threads_percent: 50,
            priority: 1,
            gpu_power_limit_w: None,
        };
        let a = args(&spec, 40123);
        let at = |k: &str| a[a.iter().position(|x| x == k).unwrap() + 1].clone();
        assert_eq!(at("--algo"), "ALEPH");
        assert_eq!(at("--user"), "krxABC123/PC");
        assert_eq!(at("--apihost"), "127.0.0.1");
        assert_eq!(at("--apiport"), "40123");
    }

    #[test]
    fn parses_stats_tolerantly() {
        let v = json!({"Software": "lolMiner 1.98a", "Algorithms": [{
            "Algorithm": "Alephium", "Pool": "alph.kryptex.network:7010",
            "Performance_Unit": "Gh/s", "Performance_Factor": 1e9, "Total_Performance": 2.5,
            "Total_Accepted": 7, "Total_Rejected": 1, "Total_Stales": 1,
            "Worker_Performance": [1.5, 1.0]
        }]});
        let s = parse_stats(&v);
        assert_eq!(s.hashrate_60s, Some(2.5e9));
        assert_eq!(s.gpu_hashrates, vec![1.5e9, 1.0e9]);
        assert_eq!((s.accepted, s.rejected), (7, 2));
        assert!(s.connected);
        // Unit only: C29 graphs per second stay as they are.
        let g = json!({"Algorithms": [{"Performance_Unit": "g/s", "Total_Performance": 9.0}]});
        assert_eq!(parse_stats(&g).hashrate_60s, Some(9.0));
        let m = json!({"Algorithms": [{"Performance_Unit": "Mh/s", "Total_Performance": 3.0}]});
        assert_eq!(parse_stats(&m).hashrate_60s, Some(3e6));
        assert_eq!(parse_stats(&json!({})).current_hashrate(), None);
    }
}

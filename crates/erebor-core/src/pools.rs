//! Pool catalog, reachability check and pool/market API adapters.
//!
//! Pools do not all expose the same metrics: every field coming from an API is optional
//! and the UI must show "not available" rather than inventing values.

use serde::{Deserialize, Serialize};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PoolApi {
    /// nodejs-pool compatible HTTP API (`/miner/{addr}/stats`, `/network/stats`).
    NodejsPool,
    /// No public per-miner API: balances are shown on the pool website.
    None,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub host: &'static str,
    pub port: u16,
    pub tls: bool,
    /// Fee in percent as published by the pool. Verify before each release.
    pub fee_percent: f64,
    pub website: &'static str,
    pub api_base: &'static str,
    pub api: PoolApi,
}

/// Curated pools. Values must be re-verified on the pool website before each release
/// (see docs/POOLS.md).
pub const CATALOG: &[PoolInfo] = &[
    // Accepts a Kryptex ID (account mining, credited in BTC on kryptex.com) or an
    // own XMR address (PPS+, min payout 0.01 XMR). TLS port not documented: plain TCP.
    PoolInfo {
        id: "kryptex",
        name: "Kryptex",
        host: "xmr.kryptex.network",
        port: 7029,
        tls: false,
        fee_percent: 1.0,
        website: "https://pool.kryptex.com/xmr",
        api_base: "",
        api: PoolApi::None,
    },
    PoolInfo {
        id: "supportxmr",
        name: "SupportXMR",
        host: "pool.supportxmr.com",
        port: 443,
        tls: true,
        fee_percent: 0.6,
        website: "https://supportxmr.com",
        api_base: "https://www.supportxmr.com/api",
        api: PoolApi::NodejsPool,
    },
];

/// Kryptex account IDs only work on Kryptex hosts.
pub fn is_kryptex_host(host: &str) -> bool {
    host == "kryptex.network" || host.ends_with(".kryptex.network")
}

pub fn find(id: &str) -> Option<&'static PoolInfo> {
    CATALOG.iter().find(|p| p.id == id)
}

/// DNS resolution + TCP connect with timeout. TLS is negotiated by XMRig itself
/// (most pools use self-signed certificates), so this only checks reachability.
pub fn check_reachable(host: &str, port: u16) -> Result<u128, String> {
    let addrs: Vec<_> = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("dns: {e}"))?
        .collect();
    let mut last = String::from("no address");
    for a in addrs {
        let t = std::time::Instant::now();
        match TcpStream::connect_timeout(&a, Duration::from_secs(5)) {
            Ok(_) => return Ok(t.elapsed().as_millis()),
            Err(e) => last = e.to_string(),
        }
    }
    Err(format!("connect: {last}"))
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .user_agent(concat!("Erebor/", env!("CARGO_PKG_VERSION")))
        .build()
}

pub const ATOMIC_PER_XMR: f64 = 1e12;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStats {
    pub difficulty: f64,
    /// Block reward in XMR.
    pub block_reward: f64,
    pub height: Option<u64>,
}

impl NetworkStats {
    /// Expected XMR per hash computed: reward / difficulty.
    pub fn xmr_per_hash(&self) -> Option<f64> {
        (self.difficulty > 0.0 && self.block_reward > 0.0)
            .then(|| self.block_reward / self.difficulty)
    }
}

pub fn parse_network_stats(v: &serde_json::Value) -> Option<NetworkStats> {
    let difficulty = v.get("difficulty")?.as_f64()?;
    let reward = v.get("value")?.as_f64()? / ATOMIC_PER_XMR;
    Some(NetworkStats {
        difficulty,
        block_reward: reward,
        height: v.get("height").and_then(|h| h.as_u64()),
    })
}

pub fn fetch_network_stats(api_base: &str) -> Result<NetworkStats, String> {
    let v: serde_json::Value = agent()
        .get(&format!("{api_base}/network/stats"))
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())?;
    parse_network_stats(&v).ok_or_else(|| "unexpected network stats format".into())
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PoolAccount {
    /// Balance accrued on the pool but not yet paid (XMR).
    pub unpaid_xmr: Option<f64>,
    /// Total confirmed payouts sent by the pool (XMR).
    pub paid_xmr: Option<f64>,
    /// Hashrate as seen by the pool (H/s).
    pub pool_hashrate: Option<f64>,
    pub valid_shares: Option<u64>,
    pub invalid_shares: Option<u64>,
    pub payments: Vec<Payment>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Payment {
    pub ts: i64,
    pub amount_xmr: f64,
    pub tx_hash: Option<String>,
}

pub fn parse_miner_stats(v: &serde_json::Value) -> PoolAccount {
    let num = |k: &str| v.get(k).and_then(|x| x.as_f64());
    PoolAccount {
        unpaid_xmr: num("amtDue").map(|a| a / ATOMIC_PER_XMR),
        paid_xmr: num("amtPaid").map(|a| a / ATOMIC_PER_XMR),
        pool_hashrate: num("hash"),
        valid_shares: v.get("validShares").and_then(|x| x.as_u64()),
        invalid_shares: v.get("invalidShares").and_then(|x| x.as_u64()),
        payments: vec![],
    }
}

pub fn parse_payments(v: &serde_json::Value) -> Vec<Payment> {
    v.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    let mut ts = p.get("ts")?.as_i64()?;
                    if ts > 10_000_000_000 {
                        ts /= 1000; // milliseconds
                    }
                    Some(Payment {
                        ts,
                        amount_xmr: p.get("amount")?.as_f64()? / ATOMIC_PER_XMR,
                        tx_hash: p
                            .get("txnHash")
                            .or_else(|| p.get("hash"))
                            .and_then(|h| h.as_str())
                            .filter(|h| h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()))
                            .map(String::from),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Account data for a (validated) wallet address. Only the public address is sent.
pub fn fetch_account(pool: &PoolInfo, wallet: &str) -> Result<PoolAccount, String> {
    match pool.api {
        PoolApi::NodejsPool => {
            crate::wallet::validate_mainnet_address(wallet).map_err(|e| format!("{e:?}"))?;
            let a = agent();
            let stats: serde_json::Value = a
                .get(&format!("{}/miner/{wallet}/stats", pool.api_base))
                .call()
                .map_err(|e| e.to_string())?
                .into_json()
                .map_err(|e| e.to_string())?;
            let mut acc = parse_miner_stats(&stats);
            if let Ok(r) = a
                .get(&format!("{}/miner/{wallet}/payments", pool.api_base))
                .call()
            {
                if let Ok(p) = r.into_json::<serde_json::Value>() {
                    acc.payments = parse_payments(&p);
                }
            }
            Ok(acc)
        }
        PoolApi::None => Err("no_pool_api".into()),
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Price {
    pub eur: f64,
    pub usd: f64,
    pub source: &'static str,
}

pub fn parse_price(v: &serde_json::Value) -> Option<Price> {
    let m = v.get("monero")?;
    Some(Price {
        eur: m.get("eur")?.as_f64()?,
        usd: m.get("usd")?.as_f64()?,
        source: "CoinGecko",
    })
}

pub fn fetch_price() -> Result<Price, String> {
    let v: serde_json::Value = agent()
        .get("https://api.coingecko.com/api/v3/simple/price?ids=monero&vs_currencies=eur,usd")
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())?;
    parse_price(&v).ok_or_else(|| "unexpected price format".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_network_stats() {
        let n = parse_network_stats(
            &json!({"difficulty": 300e9, "value": 600_000_000_000u64, "height": 3_000_000}),
        )
        .unwrap();
        assert_eq!(n.block_reward, 0.6);
        assert!((n.xmr_per_hash().unwrap() - 0.6 / 300e9).abs() < 1e-24);
        assert!(parse_network_stats(&json!({"difficulty": 1})).is_none());
    }

    #[test]
    fn parses_miner_stats_and_payments() {
        let a = parse_miner_stats(
            &json!({"amtDue": 50_000_000_000u64, "amtPaid": 1_000_000_000_000u64, "hash": 1234.0}),
        );
        assert_eq!(a.unpaid_xmr, Some(0.05));
        assert_eq!(a.paid_xmr, Some(1.0));
        assert_eq!(a.valid_shares, None);
        let p = parse_payments(&json!([
            {"ts": 1_700_000_000_000i64, "amount": 100_000_000_000u64, "txnHash": "ab".repeat(32)},
            {"ts": 1_700_000_000, "amount": 1},
            {"bogus": true}
        ]));
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].ts, 1_700_000_000);
        assert_eq!(p[0].amount_xmr, 0.1);
        assert!(p[0].tx_hash.is_some());
        assert!(parse_payments(&json!({"error": "x"})).is_empty());
    }

    #[test]
    fn parses_price() {
        let p = parse_price(&json!({"monero": {"eur": 150.5, "usd": 160.0}})).unwrap();
        assert_eq!(p.eur, 150.5);
        assert!(parse_price(&json!({})).is_none());
    }

    #[test]
    fn catalog_is_consistent() {
        for p in CATALOG {
            let mut c = crate::config::AppConfig::default();
            c.pool.host = p.host.into();
            c.pool.port = p.port;
            assert!(c.validate().is_ok(), "{}", p.id);
            assert!(p.api == PoolApi::None || p.api_base.starts_with("https://"));
        }
    }

    #[test]
    fn unreachable_port_errors() {
        // Port 1 on loopback is closed in CI environments.
        assert!(check_reachable("127.0.0.1", 1).is_err());
    }
}

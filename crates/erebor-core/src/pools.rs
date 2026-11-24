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

// ---------- per-coin pools (own wallets) ----------

/// How a pool wants the wallet and the worker name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Login {
    /// `address.worker`, password `x` (2Miners, HeroMiners, WoolyPooly, most pools).
    Dot,
    /// `address/worker` (Kryptex).
    Slash,
    /// Address as user, worker as password (MoneroOcean).
    PassWorker,
    /// Worker only: the payout address lives in the user's own node (P2Pool).
    WorkerOnly,
}

impl Login {
    pub fn credentials(self, address: &str, worker: &str) -> (String, String) {
        let w = if worker.is_empty() { "x" } else { worker };
        match self {
            Login::Dot => (format!("{address}.{w}"), "x".into()),
            Login::Slash => (format!("{address}/{w}"), "x".into()),
            Login::PassWorker => (address.to_string(), w.to_string()),
            Login::WorkerOnly => (w.to_string(), "x".into()),
        }
    }
}

/// One pool for one coin. Hosts per region: the first is the default.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoinPool {
    /// "kryptex", "2miners", "herominers", "woolypooly", "moneroocean", "p2pool".
    pub pool: &'static str,
    pub name: &'static str,
    /// (region, host): "eu", "us", "asia", or "local".
    pub hosts: Vec<(&'static str, String)>,
    pub port: u16,
    /// `None`: taken from Kryptex market data (varies by coin).
    pub fee_percent: Option<f64>,
    /// "PPLNS", "PROP", "PPS+", "P2Pool".
    pub scheme: &'static str,
    /// Minimum payout in coins, as published by the pool's API (`None`: see the pool site).
    pub min_payout: Option<f64>,
    pub login: Login,
    /// Address page on the pool website (`{a}` = address), for balance and payouts.
    pub balance_url: Option<String>,
    /// Needs the user's own wallet address (false only for P2Pool, set on the node).
    pub needs_address: bool,
}

fn eu_us_asia(eu: String, us: String, asia: String) -> Vec<(&'static str, String)> {
    vec![("eu", eu), ("us", us), ("asia", asia)]
}

/// Pools for a coin, Kryptex (own address) first when the coin is there. Endpoints checked on 2026-11-24 against
/// each pool's site/API (2Miners pages, HeroMiners `/api/stats`, WoolyPooly coin pages,
/// MoneroOcean `/pool/ports`); re-verify before each release (docs/POOLS.md).
pub fn coin_pools(coin: &crate::coins::Coin) -> Vec<CoinPool> {
    let mut out: Vec<CoinPool> = coin
        .host
        .zip(coin.kryptex_slug())
        .map(|(host, slug)| CoinPool {
            pool: "kryptex",
            name: "Kryptex",
            hosts: vec![("eu", host.to_string())],
            port: coin.port,
            fee_percent: None,
            scheme: "PPS+",
            min_payout: None,
            login: Login::Slash,
            balance_url: Some(format!("https://pool.kryptex.com/{slug}/miner/stats/{{a}}")),
            needs_address: true,
        })
        .into_iter()
        .collect();
    // (coin, subdomain, port, min payout)
    const TWO_MINERS: &[(&str, &str, u16, Option<f64>)] = &[
        ("ETC", "etc", 1010, None),
        ("ETHW", "ethw", 2020, None),
        ("ERG", "erg", 8888, None),
        ("RVN", "rvn", 6060, Some(10.0)),
        ("KAS", "kas", 2020, Some(50.0)),
        ("NEXA", "nexa", 5050, Some(50000.0)),
        ("PRL", "prl", 1818, Some(1.0)),
        ("QUANTUS", "qtc", 3636, Some(0.1)), // 2Miners' "QTC" is Quantus
        ("XNA", "xna", 6060, Some(1000.0)),
        ("ZEPH", "zeph", 2222, None),
    ];
    const HERO: &[(&str, &str, u16, f64, f64)] = &[
        ("KAS", "kaspa", 1206, 0.9, 1.0),
        ("ALPH", "alephium", 1220, 0.9, 0.1),
        ("CFX", "conflux", 1170, 0.9, 1.0),
        ("XEL", "xelis", 1225, 0.9, 0.1),
        ("ERG", "ergo", 1180, 0.9, 0.5),
        ("RVN", "ravencoin", 1140, 0.9, 5.0),
        ("ETC", "etc", 1150, 0.9, 0.01),
        ("QTC", "qubitcoin", 1195, 1.0, 0.1),
        ("PRL", "pearl", 1200, 1.0, 1.0),
        ("BEAM", "beam", 1130, 0.9, 1.0),
        ("ZANO", "zano", 1110, 0.9, 0.2),
    ];
    const WOOLY: &[(&str, &str, u16, f64)] = &[
        ("ETC", "etc", 35000, 0.1),
        ("ETHW", "ethw", 3096, 0.1),
        ("CFX", "cfx", 3094, 1.0),
        ("ERG", "erg", 3100, 1.0),
        ("RVN", "rvn", 55555, 5.0),
        ("ALPH", "alph", 3106, 1.0),
        ("KAS", "kas", 3112, 50.0),
        ("NEXA", "nexa", 3124, 50000.0),
        ("OCTA", "octa", 3130, 2.0),
        ("XEL", "xel", 3150, 0.1),
        ("PRL", "prl", 3154, 1.0),
        ("FIRO", "firo", 3104, 0.1),
        ("ZANO", "zano", 3146, 0.25),
        ("XNA", "xna", 3128, 1000.0),
        ("KLS", "kls", 3132, 500.0),
        ("RXD", "rxd", 3122, 500.0),
    ];
    if let Some(&(_, sub, port, min)) = TWO_MINERS.iter().find(|p| p.0 == coin.id) {
        out.push(CoinPool {
            pool: "2miners",
            name: "2Miners",
            hosts: eu_us_asia(
                format!("{sub}.2miners.com"),
                format!("us-{sub}.2miners.com"),
                format!("asia-{sub}.2miners.com"),
            ),
            port,
            fee_percent: Some(1.0),
            scheme: "PPLNS",
            min_payout: min,
            login: Login::Dot,
            balance_url: Some(format!("https://{sub}.2miners.com/account/{{a}}")),
            needs_address: true,
        });
    }
    if let Some(&(_, sub, port, fee, min)) = HERO.iter().find(|p| p.0 == coin.id) {
        out.push(CoinPool {
            pool: "herominers",
            name: "HeroMiners",
            hosts: eu_us_asia(
                format!("de.{sub}.herominers.com"),
                format!("us.{sub}.herominers.com"),
                format!("hk.{sub}.herominers.com"),
            ),
            port,
            fee_percent: Some(fee),
            scheme: "PROP",
            min_payout: Some(min),
            login: Login::Dot,
            balance_url: Some(format!("https://{sub}.herominers.com/#{{a}}")),
            needs_address: true,
        });
    }
    if let Some(&(_, slug, port, min)) = WOOLY.iter().find(|p| p.0 == coin.id) {
        out.push(CoinPool {
            pool: "woolypooly",
            name: "WoolyPooly",
            hosts: vec![("eu", "pool.woolypooly.com".into())],
            port,
            fee_percent: Some(0.9),
            scheme: "PPLNS",
            min_payout: Some(min),
            login: Login::Dot,
            balance_url: Some(format!(
                "https://woolypooly.com/en/coin/{slug}/wallet/{{a}}"
            )),
            needs_address: true,
        });
    }
    if coin.id == "XMR" {
        out.push(CoinPool {
            pool: "moneroocean",
            name: "MoneroOcean",
            hosts: vec![("eu", "gulf.moneroocean.stream".into())],
            port: 10008,
            fee_percent: Some(0.0),
            scheme: "PPLNS",
            min_payout: Some(0.003),
            login: Login::PassWorker,
            balance_url: Some("https://moneroocean.stream/#/dashboard?addr={a}".into()),
            needs_address: true,
        });
        out.push(CoinPool {
            pool: "p2pool",
            name: "P2Pool",
            hosts: vec![("local", "127.0.0.1".into())],
            port: 3333,
            fee_percent: Some(0.0),
            scheme: "P2Pool",
            min_payout: None,
            login: Login::WorkerOnly,
            balance_url: None,
            needs_address: false,
        });
    }
    out
}

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

const ATOMIC_PER_XMR: f64 = 1e12;

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

fn parse_network_stats(v: &serde_json::Value) -> Option<NetworkStats> {
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

fn parse_miner_stats(v: &serde_json::Value) -> PoolAccount {
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

fn parse_payments(v: &serde_json::Value) -> Vec<Payment> {
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

fn parse_price(v: &serde_json::Value) -> Option<Price> {
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

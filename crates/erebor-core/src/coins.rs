//! What can be mined, with which miner, on which device, and how much it is worth.
//!
//! Coins are mined on Kryptex (account mining with a `krx…` ID) or, for users who want
//! BTC straight to their own wallet, on unMineable (`BTC:address.worker`, auto-converted
//! to BTC, paid out above 0.00075 BTC). Profitability is
//! an *estimate*: measured hashrate on this PC × network reward per hash × price,
//! net of pool and miner fees. Market data comes from WhatToMine (network, price in
//! BTC), Kryptex (pool fee; XMR network) and CoinGecko (BTC and XMR prices).

use crate::miner::MinerKind;
use serde::Serialize;
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Device {
    Cpu,
    Gpu,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Coin {
    /// Ticker, also the id stored in the config ("PRL").
    pub id: &'static str,
    pub name: &'static str,
    /// Algorithm name as the miner expects it.
    pub algo: &'static str,
    pub device: Device,
    pub miner: MinerKind,
    /// Kryptex account-mining endpoint (plain TCP, as documented by Kryptex).
    pub host: &'static str,
    pub port: u16,
    /// Miner developer fee, percent of hashes (from the miner's README).
    pub miner_fee_percent: f64,
    /// Tag in WhatToMine's coins.json, when listed there.
    pub wtm_tag: Option<&'static str>,
    /// unMineable stratum host for this algorithm, if unMineable supports the coin.
    pub unmineable_host: Option<&'static str>,
}

/// unMineable plain-TCP stratum port (SSL: 4444/443), from unmineable.com/coins/BTC.
pub const UNMINEABLE_PORT: u16 = 3333;
/// unMineable fee without referral code. Not confirmed on an official page: estimate.
pub const UNMINEABLE_FEE_PERCENT: f64 = 1.0;

/// Coins mined on Kryptex that the supported miners handle on NVIDIA/AMD GPUs or CPU.
/// Sources: pool.kryptex.com account-mining table, unmineable.com/coins/BTC (hosts),
/// SRBMiner-MULTI README (3.8.8).
pub const COINS: &[Coin] = &[
    gpu(
        "PRL",
        "Pearl",
        "pearlhash",
        "prl.kryptex.network",
        7048,
        2.0,
        Some("pearlpow.unmineable.com"),
    ),
    gpu(
        "ETC",
        "Ethereum Classic",
        "etchash",
        "etc.kryptex.network",
        7033,
        0.65,
        Some("etchash.unmineable.com"),
    ),
    gpu(
        "ETHW",
        "EthereumPoW",
        "ethash",
        "ethw.kryptex.network",
        7034,
        0.65,
        Some("ethash.unmineable.com"),
    ),
    gpu(
        "ERG",
        "Ergo",
        "autolykos2",
        "erg.kryptex.network",
        7021,
        1.0,
        Some("autolykos.unmineable.com"),
    ),
    gpu(
        "RVN",
        "Ravencoin",
        "kawpow",
        "rvn.kryptex.network",
        7031,
        0.85,
        None,
    ),
    gpu(
        "XEL",
        "Xelis",
        "xelishashv3",
        "xel.kryptex.network",
        7019,
        1.5,
        Some("xelishash.unmineable.com"),
    ),
    Coin {
        id: "XMR",
        name: "Monero",
        algo: "rx/0",
        device: Device::Cpu,
        miner: MinerKind::Xmrig,
        host: "xmr.kryptex.network",
        port: 7029,
        miner_fee_percent: 1.0,
        wtm_tag: None,
        unmineable_host: Some("rx.unmineable.com"),
    },
];

#[allow(clippy::too_many_arguments)]
const fn gpu(
    id: &'static str,
    name: &'static str,
    algo: &'static str,
    host: &'static str,
    port: u16,
    miner_fee_percent: f64,
    unmineable_host: Option<&'static str>,
) -> Coin {
    Coin {
        id,
        name,
        algo,
        device: Device::Gpu,
        miner: MinerKind::Srbminer,
        host,
        port,
        miner_fee_percent,
        wtm_tag: Some(id),
        unmineable_host,
    }
}

pub fn find(id: &str) -> Option<&'static Coin> {
    COINS.iter().find(|c| c.id.eq_ignore_ascii_case(id))
}

/// Network and price data for one coin.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoinMarket {
    pub id: String,
    /// Coins earned per hash (block reward / (network hashrate × block time)).
    pub coin_per_hash: f64,
    pub price_btc: f64,
    /// Pool fee, percent.
    pub pool_fee_percent: f64,
}

impl CoinMarket {
    /// Net BTC per hash, after pool and miner fees.
    pub fn btc_per_hash(&self, coin: &Coin) -> f64 {
        self.coin_per_hash
            * self.price_btc
            * (1.0 - self.pool_fee_percent / 100.0)
            * (1.0 - coin.miner_fee_percent / 100.0)
    }
}

/// Expected net BTC per day at `hashrate` H/s.
pub fn btc_per_day(hashrate: f64, m: &CoinMarket, coin: &Coin) -> f64 {
    hashrate * 86_400.0 * m.btc_per_hash(coin)
}

fn per_hash(reward: f64, net_hashrate: f64, block_time: f64) -> Option<f64> {
    (reward > 0.0 && net_hashrate > 0.0 && block_time > 0.0)
        .then(|| reward / (net_hashrate * block_time))
        .filter(|v| v.is_finite())
}

/// Parse WhatToMine `coins.json` into (tag → (coin_per_hash, price_btc)).
pub fn parse_wtm(v: &serde_json::Value) -> HashMap<String, (f64, f64)> {
    let mut out = HashMap::new();
    let Some(coins) = v.get("coins").and_then(|c| c.as_object()) else {
        return out;
    };
    for c in coins.values() {
        let f = |k: &str| {
            c.get(k)
                .and_then(|x| x.as_f64().or_else(|| x.as_str()?.parse().ok()))
        };
        let (Some(tag), Some(reward), Some(nethash), Some(bt), Some(price)) = (
            c.get("tag").and_then(|t| t.as_str()),
            f("block_reward24").or_else(|| f("block_reward")),
            f("nethash"),
            f("block_time"),
            f("exchange_rate24").or_else(|| f("exchange_rate")),
        ) else {
            continue;
        };
        if c.get("lagging").and_then(|l| l.as_bool()) == Some(true) {
            continue; // stale data: better no estimate than a wrong one
        }
        if let Some(cph) = per_hash(reward, nethash, bt) {
            out.insert(tag.to_string(), (cph, price));
        }
    }
    out
}

/// Kryptex `/{ticker}/api/v1/pool/stats` → (pool fee %, coin_per_hash).
pub fn parse_kryptex_stats(v: &serde_json::Value) -> Option<(f64, Option<f64>)> {
    let fee = v.get("fee")?.as_f64()? * 100.0;
    let f = |k: &str| v.get(k).and_then(|x| x.as_f64());
    let cph = match (f("block_reward"), f("net_hashrate"), f("block_time")) {
        (Some(r), Some(n), Some(t)) => per_hash(r, n, t),
        _ => None,
    };
    Some((fee, cph))
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BtcPrice {
    pub eur: f64,
    pub usd: f64,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .user_agent(concat!("Erebor/", env!("CARGO_PKG_VERSION")))
        .build()
}

fn get_json(a: &ureq::Agent, url: &str) -> Result<serde_json::Value, String> {
    a.get(url)
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())
}

/// Fetch market data for every catalog coin. Coins without complete data are left out
/// (their profitability is "not available", never guessed).
pub fn fetch_markets() -> Result<(Vec<CoinMarket>, Option<BtcPrice>), String> {
    let a = agent();
    let wtm = get_json(&a, "https://whattomine.com/coins.json")
        .map(|v| parse_wtm(&v))
        .unwrap_or_default();
    let cg = get_json(
        &a,
        "https://api.coingecko.com/api/v3/simple/price?ids=bitcoin,monero&vs_currencies=eur,usd,btc",
    )
    .ok();
    let num = |coin: &str, cur: &str| -> Option<f64> { cg.as_ref()?.get(coin)?.get(cur)?.as_f64() };
    let btc = match (num("bitcoin", "eur"), num("bitcoin", "usd")) {
        (Some(eur), Some(usd)) => Some(BtcPrice { eur, usd }),
        _ => None,
    };
    let mut out = Vec::new();
    for c in COINS {
        let Ok(stats) = get_json(
            &a,
            &format!(
                "https://pool.kryptex.com/{}/api/v1/pool/stats",
                c.id.to_lowercase()
            ),
        ) else {
            continue;
        };
        let Some((fee, kryptex_cph)) = parse_kryptex_stats(&stats) else {
            continue;
        };
        let data = match c.wtm_tag {
            Some(tag) => wtm.get(tag).copied(),
            None => kryptex_cph.zip(num("monero", "btc")),
        };
        if let Some((coin_per_hash, price_btc)) = data {
            out.push(CoinMarket {
                id: c.id.into(),
                coin_per_hash,
                price_btc,
                pool_fee_percent: fee,
            });
        }
    }
    if out.is_empty() && btc.is_none() {
        return Err("market data unavailable".into());
    }
    Ok((out, btc))
}

/// Pick the coin with the highest expected BTC/day among `candidates` with a measured
/// hashrate. Returns `None` when nothing can be compared.
pub fn best<'a>(
    candidates: &[&'a Coin],
    hashrates: &HashMap<String, f64>,
    markets: &[CoinMarket],
) -> Option<(&'a Coin, f64)> {
    candidates
        .iter()
        .filter_map(|c| {
            let h = *hashrates.get(c.id)?;
            let m = markets.iter().find(|m| m.id == c.id)?;
            Some((*c, btc_per_day(h, m, c)))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn catalog_is_consistent() {
        for c in COINS {
            assert!(c.host.ends_with(".kryptex.network"), "{}", c.id);
            assert!(crate::config::valid_host(c.host));
            assert_eq!(
                c.device == Device::Gpu,
                c.miner == MinerKind::Srbminer,
                "{}",
                c.id
            );
        }
        assert_eq!(find("prl").unwrap().algo, "pearlhash");
    }

    #[test]
    fn parses_wtm_and_skips_lagging() {
        let v = json!({"coins": {
            "Pearl": {"tag": "PRL", "block_reward24": 2000.0, "nethash": 1e20, "block_time": "200.0", "exchange_rate24": 2e-5, "lagging": false},
            "Old": {"tag": "OLD", "block_reward24": 1.0, "nethash": 1.0, "block_time": "1", "exchange_rate24": 1.0, "lagging": true},
            "Broken": {"tag": "BRK", "nethash": 0}
        }});
        let m = parse_wtm(&v);
        let (cph, price) = m["PRL"];
        assert!((cph - 2000.0 / (1e20 * 200.0)).abs() < 1e-30);
        assert_eq!(price, 2e-5);
        assert!(!m.contains_key("OLD") && !m.contains_key("BRK"));
    }

    #[test]
    fn parses_kryptex_stats() {
        let v = json!({"fee": 0.03, "block_reward": 2.0, "net_hashrate": 1000.0, "block_time": 10});
        let (fee, cph) = parse_kryptex_stats(&v).unwrap();
        assert!((fee - 3.0).abs() < 1e-9);
        assert!((cph.unwrap() - 2.0 / 10_000.0).abs() < 1e-12);
    }

    #[test]
    fn picks_the_most_profitable_measured_coin() {
        let prl = find("PRL").unwrap();
        let etc = find("ETC").unwrap();
        let rvn = find("RVN").unwrap();
        let markets = vec![
            CoinMarket {
                id: "PRL".into(),
                coin_per_hash: 1e-14,
                price_btc: 2.5e-5,
                pool_fee_percent: 3.0,
            },
            CoinMarket {
                id: "ETC".into(),
                coin_per_hash: 6e-11,
                price_btc: 9e-5,
                pool_fee_percent: 1.0,
            },
        ];
        let mut h = HashMap::new();
        h.insert("PRL".to_string(), 3e9);
        h.insert("ETC".to_string(), 1e3);
        // RVN has no measured hashrate and no market: never chosen on a guess.
        let (c, v) = best(&[prl, etc, rvn], &h, &markets).unwrap();
        assert_eq!(c.id, "PRL");
        assert!((v - btc_per_day(3e9, &markets[0], prl)).abs() < 1e-18);
        assert!(best(&[rvn], &h, &markets).is_none());
    }
}

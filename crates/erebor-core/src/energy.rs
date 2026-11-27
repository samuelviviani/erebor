//! What a kWh costs right now: the user's own price bands, the share covered by solar
//! surplus (valued at the feed-in price) and the heat credit in the heating months.
//! Only numbers the user entered or a meter measured: nothing is guessed.

use crate::config::{Energy, Solar};
use serde_json::Value;
use std::time::Duration;

/// Bidding zones whose day-ahead prices Energy-Charts publishes (checked by hand).
pub const PRICE_ZONES: &[&str] = &[
    "DE-LU",
    "AT",
    "FR",
    "ES",
    "PL",
    "CZ",
    "DK2",
    "SE4",
    "NO1",
    "IT-North",
    "IT-Centre-North",
    "IT-Centre-South",
    "IT-Sardinia",
];

/// Today's (and, once published, tomorrow's) market prices of `zone`, in €/kWh:
/// `(start unix time, price)`. Free, no key: Energy-Charts (CC BY 4.0, Bundesnetzagentur
/// | SMARD.de). It rate-limits, so the engine asks at most once an hour.
pub fn fetch_spot_prices(zone: &str) -> Result<Vec<(i64, f64)>, String> {
    let url = format!("https://api.energy-charts.info/price?bzn={zone}");
    let v: Value = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .build()
        .get(&url)
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())?;
    let ts = v["unix_seconds"].as_array().ok_or("bad_response")?;
    let pr = v["price"].as_array().ok_or("bad_response")?;
    let per_kwh = if v["unit"].as_str().unwrap_or_default().contains("MWh") {
        1000.0
    } else {
        1.0
    };
    Ok(ts
        .iter()
        .zip(pr)
        .filter_map(|(t, p)| Some((t.as_i64()?, p.as_f64()? / per_kwh)))
        .collect())
}

/// The market price at `now`: the last slot that started at most an hour before.
pub fn spot_at(prices: &[(i64, f64)], now: i64) -> Option<f64> {
    prices
        .iter()
        .rev()
        .find(|(t, _)| *t <= now)
        .filter(|(t, _)| now - t < 3600)
        .map(|(_, p)| *p)
}

/// Grid price at this time: the first band that covers it, else the base price.
pub fn grid_price(e: &Energy, weekday: usize, minute: u16) -> Option<f64> {
    e.bands
        .iter()
        .find(|b| b.covers(weekday, minute))
        .map(|b| b.price)
        .or(e.price_per_kwh)
}

/// Share of the mining draw (0..1) that solar surplus covers.
/// `export_w`: what the meter exports now (negative = importing). While mining the
/// miner's draw is already inside that reading, so it is added back.
pub fn solar_share(export_w: f64, mining_w: Option<f64>, mining: bool) -> f64 {
    match mining_w.filter(|w| *w > 0.0) {
        Some(w) => {
            let available = if mining { export_w + w } else { export_w };
            (available / w).clamp(0.0, 1.0)
        }
        // Draw not known yet: covered while something is exported.
        None => f64::from(export_w > 0.0),
    }
}

/// Price per kWh actually paid for mining energy.
pub fn effective_price(
    grid: Option<f64>,
    solar: Option<(f64, f64)>, // (share covered, feed-in price)
    heat_credit_percent: u8,
) -> Option<f64> {
    let price = match solar {
        Some((share, feed_in)) if share >= 1.0 => feed_in,
        Some((share, feed_in)) => share * feed_in + (1.0 - share) * grid?,
        None => grid?,
    };
    Some(price * (1.0 - f64::from(heat_credit_percent.min(100)) / 100.0))
}

/// Exported watts from the configured meter (positive = exporting).
pub fn read_solar(s: &Solar) -> Result<f64, String> {
    let w = read_number(&s.url, &s.token, &s.field)?;
    Ok(if s.export_positive { w } else { -w })
}

/// One number from a JSON endpoint (meter, thermometer): Home Assistant, Shelly, …
pub fn read_number(url: &str, token: &str, path: &str) -> Result<f64, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(4))
        .build();
    let mut req = agent.get(url);
    if !token.is_empty() {
        req = req.set("Authorization", &format!("Bearer {token}"));
    }
    let v: Value = req
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())?;
    field(&v, path).ok_or_else(|| format!("no number at \"{path}\""))
}

/// Number at a dotted path (`emeters.0.power`); numeric strings count (Home Assistant).
fn field(v: &Value, path: &str) -> Option<f64> {
    let mut cur = v;
    for key in path.split('.').filter(|k| !k.is_empty()) {
        cur = match cur {
            Value::Array(a) => a.get(key.parse::<usize>().ok()?)?,
            _ => cur.get(key)?,
        };
    }
    cur.as_f64()
        .or_else(|| cur.as_str()?.trim().parse().ok())
        .filter(|w: &f64| w.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PriceBand;
    use serde_json::json;

    #[test]
    fn bands_override_the_base_price() {
        let mut e = Energy {
            price_per_kwh: Some(0.30),
            ..Default::default()
        };
        // Night band 23:00–07:00 every day.
        e.bands.push(PriceBand {
            start_minute: 23 * 60,
            end_minute: 7 * 60,
            price: 0.18,
            ..Default::default()
        });
        assert_eq!(grid_price(&e, 0, 2 * 60), Some(0.18));
        assert_eq!(grid_price(&e, 0, 12 * 60), Some(0.30));
        e.price_per_kwh = None;
        assert_eq!(grid_price(&e, 0, 12 * 60), None);
    }

    #[test]
    fn solar_covers_its_share() {
        // Not mining, 150 W exported, miner draws 200 W: 75% covered.
        assert_eq!(solar_share(150.0, Some(200.0), false), 0.75);
        // Mining 200 W and still exporting 50 W: fully covered.
        assert_eq!(solar_share(50.0, Some(200.0), true), 1.0);
        // Mining 200 W and importing 100 W: half covered.
        assert_eq!(solar_share(-100.0, Some(200.0), true), 0.5);
        assert_eq!(solar_share(-10.0, None, false), 0.0);
        assert_eq!(
            effective_price(Some(0.30), Some((0.5, 0.10)), 0),
            Some(0.20)
        );
        assert_eq!(effective_price(None, Some((1.0, 0.10)), 0), Some(0.10));
        assert_eq!(effective_price(None, Some((0.5, 0.10)), 0), None);
    }

    #[test]
    fn heat_credit_lowers_the_price() {
        assert_eq!(effective_price(Some(0.30), None, 100), Some(0.0));
        assert!((effective_price(Some(0.30), None, 50).unwrap() - 0.15).abs() < 1e-12);
    }

    #[test]
    fn reads_meter_fields() {
        let ha = json!({"entity_id": "sensor.grid", "state": "-523.5"});
        assert_eq!(field(&ha, "state"), Some(-523.5));
        let shelly = json!({"emeters": [{"power": 120.0}], "total_act_power": -80});
        assert_eq!(field(&shelly, "emeters.0.power"), Some(120.0));
        assert_eq!(field(&shelly, "total_act_power"), Some(-80.0));
        assert_eq!(field(&ha, "missing"), None);
        assert_eq!(field(&json!({"state": "unavailable"}), "state"), None);
    }

    #[test]
    fn spot_price_is_the_current_slot() {
        let p = [(1000, 0.10), (1900, 0.20), (2800, -0.01)];
        assert_eq!(spot_at(&p, 999), None);
        assert_eq!(spot_at(&p, 1900), Some(0.20));
        assert_eq!(spot_at(&p, 2799), Some(0.20));
        assert_eq!(spot_at(&p, 2800 + 3599), Some(-0.01));
        assert_eq!(
            spot_at(&p, 2800 + 3600),
            None,
            "stale: no data for this hour"
        );
    }
}

//! Reward and cost estimates. Every value is an *estimate* derived from hashrate,
//! network difficulty and block reward; it is never a confirmed payment.
//!
//! Gross → minus XMRig donation → minus pool fee → net (XMR) → × price → minus energy.
//! With several coins, value is tracked per sample in BTC already net of pool and
//! miner fees (`btc_per_hash`), then converted with the current BTC price.
//! The Erebor fee is time-based: project time is excluded from the user's gross
//! (it is not subtracted a second time) and reported separately for transparency.

use serde::Serialize;

/// Aggregated data for a time range (from the sample store).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RangeTotals {
    /// Σ hashrate·secs·xmr_per_hash over samples mined for the user.
    pub user_xmr: f64,
    /// Same for samples mined for the Erebor project wallet.
    pub project_xmr: f64,
    pub user_secs: f64,
    pub project_secs: f64,
    /// Σ watts·secs over samples with a known power value (joules).
    pub energy_joules: f64,
    /// Seconds with no power value.
    pub unknown_power_secs: f64,
    /// True if every power value came from a hardware measurement.
    pub power_all_measured: bool,
    /// Seconds whose samples had no difficulty data (estimate incomplete).
    pub unknown_reward_secs: f64,
    /// Σ hashrate·secs·btc_per_hash for the user (net of pool and miner fees).
    pub user_btc: f64,
    /// User seconds without market value (estimate incomplete).
    pub unknown_value_secs: f64,
    /// User seconds spent on coins other than XMR (not in the XMR amounts).
    pub other_coin_secs: f64,
    /// Electricity cost of the samples that stored their price (currency).
    pub priced_cost: f64,
    /// Joules of samples with a power value but no stored price.
    pub unpriced_joules: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EnergySource {
    Measured,
    Estimated,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Earnings {
    pub gross_xmr: f64,
    pub xmrig_donation_xmr: f64,
    pub pool_fee_xmr: Option<f64>,
    pub net_xmr: f64,
    pub erebor_fee_secs: f64,
    /// What the project time would have earned (informative; already excluded from gross).
    pub erebor_fee_xmr: f64,
    pub mining_secs: f64,
    pub price: Option<f64>,
    /// Net value in BTC across all coins (estimate).
    pub net_btc: f64,
    pub net_value: Option<f64>,
    pub energy_kwh: Option<f64>,
    pub energy_source: EnergySource,
    pub energy_cost: Option<f64>,
    pub net_profit: Option<f64>,
    /// False when difficulty data was missing for part of the range.
    pub complete: bool,
}

pub struct Params {
    pub xmrig_donate_percent: f64,
    pub pool_fee_percent: Option<f64>,
    pub price: Option<f64>,
    /// BTC price in the user's currency.
    pub btc_price: Option<f64>,
    /// Price per kWh for energy without a stored price (projections, older samples).
    pub price_per_kwh: Option<f64>,
}

fn energy_kwh(watts: f64, hours: f64) -> f64 {
    watts / 1000.0 * hours
}

pub fn energy_cost(watts: f64, hours: f64, price_per_kwh: f64) -> f64 {
    energy_kwh(watts, hours) * price_per_kwh
}

pub fn summarize(t: &RangeTotals, p: &Params) -> Earnings {
    let gross = t.user_xmr;
    let donation = gross * p.xmrig_donate_percent / 100.0;
    let pool_fee = p.pool_fee_percent.map(|f| (gross - donation) * f / 100.0);
    let net = gross - donation - pool_fee.unwrap_or(0.0);

    let mining_secs = t.user_secs + t.project_secs;
    let (energy_kwh, source) = if t.unknown_power_secs <= 0.0 {
        if mining_secs > 0.0 {
            let src = if t.power_all_measured {
                EnergySource::Measured
            } else {
                EnergySource::Estimated
            };
            (Some(t.energy_joules / 3.6e6), src)
        } else {
            (Some(0.0), EnergySource::Unavailable)
        }
    } else {
        (None, EnergySource::Unavailable)
    };
    let energy_cost = energy_kwh.and_then(|_| {
        if t.unpriced_joules <= 0.0 && t.priced_cost > 0.0 {
            Some(t.priced_cost)
        } else {
            p.price_per_kwh
                .map(|c| t.priced_cost + t.unpriced_joules / 3.6e6 * c)
        }
    });
    let net_value = match p.btc_price {
        Some(b) if t.user_btc > 0.0 => Some(t.user_btc * b),
        _ if t.other_coin_secs > 0.0 => None,
        _ => p.price.map(|price| net * price),
    };
    Earnings {
        gross_xmr: gross,
        xmrig_donation_xmr: donation,
        pool_fee_xmr: pool_fee,
        net_xmr: net,
        erebor_fee_secs: t.project_secs,
        erebor_fee_xmr: t.project_xmr,
        mining_secs,
        price: p.price,
        net_btc: t.user_btc,
        net_value,
        energy_kwh,
        energy_source: source,
        energy_cost,
        net_profit: net_value.zip(energy_cost).map(|(v, c)| v - c),
        complete: t.unknown_reward_secs <= 0.0
            && (t.other_coin_secs <= 0.0 || t.unknown_value_secs <= 0.0),
    }
}

/// Projected net profit per day at a constant hashrate and power draw.
pub fn daily_profit(
    hashrate: f64,
    xmr_per_hash: f64,
    p: &Params,
    erebor_fee_percent: f64,
    watts: f64,
) -> Option<f64> {
    let day = 86_400.0;
    let user_share = 1.0 - erebor_fee_percent / 100.0;
    let t = RangeTotals {
        user_xmr: hashrate * day * user_share * xmr_per_hash,
        user_secs: day * user_share,
        project_secs: day * (1.0 - user_share),
        energy_joules: watts * day,
        unpriced_joules: watts * day,
        power_all_measured: false,
        ..Default::default()
    };
    summarize(&t, p).net_profit
}

/// Same projection for any coin, from its net BTC per hash.
pub fn daily_profit_btc(
    hashrate: f64,
    btc_per_hash: f64,
    p: &Params,
    erebor_fee_percent: f64,
    watts: f64,
) -> Option<f64> {
    let day = 86_400.0;
    let user_share = 1.0 - erebor_fee_percent / 100.0;
    let t = RangeTotals {
        user_btc: hashrate * day * user_share * btc_per_hash,
        user_secs: day * user_share,
        project_secs: day * (1.0 - user_share),
        other_coin_secs: day * user_share,
        energy_joules: watts * day,
        unpriced_joules: watts * day,
        power_all_measured: false,
        ..Default::default()
    };
    summarize(&t, p).net_profit
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> Params {
        Params {
            xmrig_donate_percent: 1.0,
            pool_fee_percent: Some(0.6),
            price: Some(150.0),
            btc_price: None,
            price_per_kwh: Some(0.30),
        }
    }

    #[test]
    fn other_coins_are_valued_in_btc() {
        let t = RangeTotals {
            user_secs: 3600.0,
            other_coin_secs: 3600.0,
            user_btc: 1e-5,
            ..Default::default()
        };
        let mut p = params();
        assert_eq!(summarize(&t, &p).net_value, None); // no BTC price: no guess
        p.btc_price = Some(60_000.0);
        let e = summarize(&t, &p);
        assert!((e.net_value.unwrap() - 0.6).abs() < 1e-9);
        assert_eq!(e.net_btc, 1e-5);
        assert!(e.complete);
    }

    #[test]
    fn energy_formula() {
        // 100 W for 24 h = 2.4 kWh; at 0.25 €/kWh = 0.60 €.
        assert!((energy_kwh(100.0, 24.0) - 2.4).abs() < 1e-12);
        assert!((energy_cost(100.0, 24.0, 0.25) - 0.6).abs() < 1e-12);
    }

    #[test]
    fn fees_are_not_double_counted() {
        let t = RangeTotals {
            user_xmr: 1.0,
            project_xmr: 0.01,
            user_secs: 3600.0,
            project_secs: 36.0,
            energy_joules: 100.0 * 3636.0,
            unpriced_joules: 100.0 * 3636.0,
            power_all_measured: true,
            ..Default::default()
        };
        let e = summarize(&t, &params());
        assert_eq!(e.gross_xmr, 1.0);
        assert!((e.xmrig_donation_xmr - 0.01).abs() < 1e-12);
        assert!((e.pool_fee_xmr.unwrap() - 0.99 * 0.006).abs() < 1e-12);
        assert!((e.net_xmr - (1.0 - 0.01 - 0.00594)).abs() < 1e-12);
        // Erebor fee is reported but not subtracted from the user's gross.
        assert_eq!(e.erebor_fee_xmr, 0.01);
        assert_eq!(e.energy_source, EnergySource::Measured);
        let kwh = 100.0 * 3636.0 / 3.6e6;
        assert!((e.energy_cost.unwrap() - kwh * 0.30).abs() < 1e-12);
        assert!((e.net_profit.unwrap() - (e.net_xmr * 150.0 - kwh * 0.30)).abs() < 1e-9);
    }

    #[test]
    fn stored_prices_win_over_the_current_one() {
        // 1 kWh at a stored 0.10 plus 1 kWh without a price (current 0.30).
        let mut t = RangeTotals {
            user_secs: 7200.0,
            energy_joules: 7.2e6,
            priced_cost: 0.10,
            unpriced_joules: 3.6e6,
            power_all_measured: true,
            ..Default::default()
        };
        let mut p = params();
        assert!((summarize(&t, &p).energy_cost.unwrap() - 0.40).abs() < 1e-12);
        p.price_per_kwh = None;
        assert_eq!(summarize(&t, &p).energy_cost, None);
        t.unpriced_joules = 0.0;
        assert_eq!(summarize(&t, &p).energy_cost, Some(0.10));
    }

    #[test]
    fn missing_data_is_explicit() {
        let t = RangeTotals {
            user_xmr: 0.5,
            user_secs: 100.0,
            unknown_power_secs: 100.0,
            unknown_reward_secs: 10.0,
            ..Default::default()
        };
        let mut p = params();
        p.price = None;
        p.pool_fee_percent = None;
        let e = summarize(&t, &p);
        assert_eq!(e.energy_kwh, None);
        assert_eq!(e.energy_source, EnergySource::Unavailable);
        assert_eq!(e.net_value, None);
        assert_eq!(e.net_profit, None);
        assert_eq!(e.pool_fee_xmr, None);
        assert!(!e.complete);
    }

    #[test]
    fn daily_profit_sign() {
        // 5 kH/s, 0.6 XMR reward, 300G difficulty, 1% fee, 80 W.
        let xph = 0.6 / 300e9;
        let p = params();
        let profit = daily_profit(5000.0, xph, &p, 1.0, 80.0).unwrap();
        // Revenue ≈ 5000*86400*0.99*2e-12 ≈ 0.000855 XMR (≈0.13 €) vs 1.92 kWh*0.30 = 0.576 €.
        assert!(profit < 0.0);
        assert!(daily_profit(5000.0, xph, &p, 1.0, 0.0).unwrap() > 0.0);
    }
}

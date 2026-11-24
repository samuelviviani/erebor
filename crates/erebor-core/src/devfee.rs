//! Erebor dev fee: a declared share (2%) of *effective* mining time is mined for the
//! project account. Nothing is ever taken from the user's balance. The fee is stated
//! in the app (welcome screen, dashboard) and documented on the website.
//!
//! - Only effective mining time counts (running, not paused, hashrate reported).
//! - The ledger is persisted, so restarts neither reset nor double-charge the fee.
//! - Work for the project happens in blocks of `block_secs` to avoid constant switching.
//! - Disabled when the build has no project wallet.

use serde::{Deserialize, Serialize};

/// Fee declared to users, in percent of effective mining time.
pub const DEFAULT_FEE_PERCENT: f64 = 2.0;
/// Minimum project block (seconds). With 2% this is two minutes every ~100 minutes:
/// fewer, longer blocks waste less time on pool reconnections.
pub const BLOCK_SECS: f64 = 120.0;

/// Project wallet, set at build time (`EREBOR_DEV_FEE_WALLET`, a Kryptex ID or a Monero
/// address). No address is hard-coded.
pub fn build_wallet() -> Option<&'static str> {
    option_env!("EREBOR_DEV_FEE_WALLET")
        .map(str::trim)
        .filter(|w| crate::wallet::validate_payout(w).is_ok())
}

/// Project Bitcoin address (`EREBOR_DEV_FEE_BTC`), used when the user is paid in BTC
/// through unMineable: project time then goes to this address on the same pool.
pub fn build_btc_wallet() -> Option<&'static str> {
    option_env!("EREBOR_DEV_FEE_BTC")
        .map(str::trim)
        .filter(|w| crate::wallet::validate_bitcoin_address(w).is_ok())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    #[default]
    User,
    Project,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Ledger {
    pub user_secs: f64,
    pub project_secs: f64,
}

#[derive(Debug, Clone)]
pub struct DevFeeManager {
    rate: f64,
    enabled: bool,
    ledger: Ledger,
    active: Target,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevFeeStatus {
    pub enabled: bool,
    pub percent: f64,
    pub wallet_configured: bool,
    pub active: Target,
    pub user_secs: f64,
    pub project_secs: f64,
    /// Measured share of effective time mined for the project (percent).
    pub effective_percent: f64,
}

impl DevFeeManager {
    pub fn new(percent: f64, enabled: bool, ledger: Ledger) -> Self {
        let mut m = Self {
            rate: (percent / 100.0).clamp(0.0, 0.05),
            enabled,
            ledger,
            active: Target::User,
        };
        m.active = m.desired(Target::User);
        m
    }

    fn owed(&self) -> f64 {
        self.rate * (self.ledger.user_secs + self.ledger.project_secs) - self.ledger.project_secs
    }

    fn desired(&self, current: Target) -> Target {
        if !self.enabled || self.rate == 0.0 {
            return Target::User;
        }
        let owed = self.owed();
        match current {
            Target::User if owed >= BLOCK_SECS => Target::Project,
            Target::Project if owed <= 0.0 => Target::User,
            t => t,
        }
    }

    /// Account `effective_secs` of real mining for the active target and return the
    /// target that should be active from now on. Pass 0 while paused/idle/warming up.
    /// Time mined while the fee is off (e.g. a build without project wallet) is not
    /// counted: enabling the fee later must not charge it retroactively.
    pub fn tick(&mut self, effective_secs: f64) -> Target {
        if self.enabled && effective_secs > 0.0 && effective_secs.is_finite() {
            match self.active {
                Target::User => self.ledger.user_secs += effective_secs,
                Target::Project => self.ledger.project_secs += effective_secs,
            }
        }
        self.active = self.desired(self.active);
        self.active
    }

    pub fn active(&self) -> Target {
        self.active
    }

    pub fn ledger(&self) -> Ledger {
        self.ledger
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.active = self.desired(self.active);
    }

    pub fn status(&self) -> DevFeeStatus {
        let total = self.ledger.user_secs + self.ledger.project_secs;
        DevFeeStatus {
            enabled: self.enabled,
            percent: self.rate * 100.0,
            wallet_configured: build_wallet().is_some(),
            active: self.active,
            user_secs: self.ledger.user_secs,
            project_secs: self.ledger.project_secs,
            effective_percent: if total > 0.0 {
                self.ledger.project_secs / total * 100.0
            } else {
                0.0
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(m: &mut DevFeeManager, ticks: u32, step: f64) -> u32 {
        let mut switches = 0;
        let mut last = m.active();
        for _ in 0..ticks {
            let t = m.tick(step);
            if t != last {
                switches += 1;
                last = t;
            }
        }
        switches
    }

    #[test]
    fn converges_to_declared_rate() {
        let mut m = DevFeeManager::new(DEFAULT_FEE_PERCENT, true, Ledger::default());
        run(&mut m, 200_000, 2.0); // ~111 hours of 2s ticks
        let s = m.status();
        assert!(
            (s.effective_percent - DEFAULT_FEE_PERCENT).abs() < 0.05,
            "{}",
            s.effective_percent
        );
    }

    #[test]
    fn first_block_after_about_100_minutes() {
        let mut m = DevFeeManager::new(DEFAULT_FEE_PERCENT, true, Ledger::default());
        let mut t = 0.0;
        while m.tick(1.0) == Target::User {
            t += 1.0;
            assert!(t < 10_000.0);
        }
        // owed reaches BLOCK_SECS (120s at 2%) when total ≈ 6000s.
        assert!((5990.0..=6010.0).contains(&t), "{t}");
        // Project block lasts about BLOCK_SECS.
        let mut d = 0.0;
        while m.tick(1.0) == Target::Project {
            d += 1.0;
        }
        assert!((BLOCK_SECS - 1.0..=BLOCK_SECS + 2.0).contains(&d), "{d}");
    }

    #[test]
    fn few_switches() {
        let mut m = DevFeeManager::new(DEFAULT_FEE_PERCENT, true, Ledger::default());
        let switches = run(&mut m, 36_000, 1.0); // 10 hours
                                                 // ~6 project blocks -> ~12 switches, never constant flapping.
        assert!(switches <= 14, "{switches}");
    }

    #[test]
    fn time_with_fee_off_is_not_charged_later() {
        let mut m = DevFeeManager::new(DEFAULT_FEE_PERCENT, false, Ledger::default());
        run(&mut m, 70_000, 1.0); // ~19 hours mined by a build without the fee
        m.set_enabled(true);
        assert_eq!(m.tick(1.0), Target::User);
        assert_eq!(m.ledger().user_secs, 1.0);
    }

    #[test]
    fn no_accrual_when_idle_or_paused() {
        let mut m = DevFeeManager::new(DEFAULT_FEE_PERCENT, true, Ledger::default());
        for _ in 0..100_000 {
            m.tick(0.0);
        }
        assert_eq!(m.ledger(), Ledger::default());
        assert_eq!(m.active(), Target::User);
        m.tick(f64::NAN);
        m.tick(-5.0);
        assert_eq!(m.ledger(), Ledger::default());
    }

    #[test]
    fn disabled_never_mines_for_project() {
        let mut m = DevFeeManager::new(DEFAULT_FEE_PERCENT, false, Ledger::default());
        for _ in 0..100_000 {
            assert_eq!(m.tick(1.0), Target::User);
        }
        assert_eq!(m.ledger().project_secs, 0.0);
    }

    #[test]
    fn restart_restores_debt_without_double_charging() {
        let mut m = DevFeeManager::new(DEFAULT_FEE_PERCENT, true, Ledger::default());
        for _ in 0..5_000 {
            m.tick(1.0);
        }
        let saved = m.ledger();
        // Simulated restart: a new manager from the persisted ledger.
        let mut m2 = DevFeeManager::new(DEFAULT_FEE_PERCENT, true, saved);
        assert_eq!(m2.active(), Target::User);
        let mut user = 0.0;
        while m2.tick(1.0) == Target::User {
            user += 1.0;
        }
        // Only the remaining ~1000s are needed before the first block, not 6000s again.
        assert!(user < 1_100.0, "{user}");
        // Restart in the middle of a project block resumes it (no free pass, no extra).
        let mid = m2.ledger();
        let m3 = DevFeeManager::new(DEFAULT_FEE_PERCENT, true, mid);
        assert_eq!(m3.active(), Target::Project);
    }

    #[test]
    fn rate_is_capped() {
        let m = DevFeeManager::new(50.0, true, Ledger::default());
        assert_eq!(m.status().percent, 5.0);
    }

    #[test]
    fn disabling_switches_back_immediately() {
        let mut m = DevFeeManager::new(
            DEFAULT_FEE_PERCENT,
            true,
            Ledger {
                user_secs: 10_000.0,
                project_secs: 0.0,
            },
        );
        assert_eq!(m.active(), Target::Project);
        m.set_enabled(false);
        assert_eq!(m.active(), Target::User);
    }
}

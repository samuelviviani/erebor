//! Persistent user configuration (JSON file in the app data dir).

use crate::wallet;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const CONFIG_VERSION: u32 = 2;

/// `coin` value meaning "let Erebor choose the most profitable coin for this PC".
pub const AUTO_COIN: &str = "AUTO";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
    Eco,
    #[default]
    Balanced,
    Performance,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PoolConfig {
    /// Catalog id ("kryptex", "supportxmr") or "custom".
    pub id: String,
    pub host: String,
    pub port: u16,
    pub tls: bool,
    pub worker: String,
}

impl Default for PoolConfig {
    fn default() -> Self {
        let p = &crate::pools::CATALOG[0];
        Self {
            id: p.id.into(),
            host: p.host.into(),
            port: p.port,
            tls: p.tls,
            worker: "erebor".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct MinerConfig {
    /// Absolute path of the miner binary.
    pub path: Option<String>,
    pub version: Option<String>,
    /// SHA-256 of the binary recorded at install/selection time.
    pub sha256: Option<String>,
    /// True when installed by Erebor from the pinned, hash-verified manifest.
    pub verified: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Performance {
    pub preset: Preset,
    /// Used only with Preset::Custom (1..=100).
    pub custom_threads_percent: u8,
}

impl Default for Performance {
    fn default() -> Self {
        Self {
            preset: Preset::Balanced,
            custom_threads_percent: 50,
        }
    }
}

impl Performance {
    /// XMRig `cpu.max-threads-hint` (percent of threads) and process priority (0 idle .. 5 highest).
    pub fn limits(&self) -> (u8, u8) {
        match self.preset {
            Preset::Eco => (25, 0),
            Preset::Balanced => (50, 1),
            Preset::Performance => (100, 2),
            Preset::Custom => (self.custom_threads_percent.clamp(1, 100), 1),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Safety {
    pub max_temp_c: f32,
    pub stop_on_overheat: bool,
    /// Restart the miner after a crash (max 3 attempts).
    pub auto_restart: bool,
}

impl Default for Safety {
    fn default() -> Self {
        Self {
            max_temp_c: 85.0,
            stop_on_overheat: true,
            auto_restart: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Energy {
    /// Electricity price per kWh in `currency`.
    pub price_per_kwh: Option<f64>,
    pub currency: String,
    /// User estimate of extra power draw while mining (W). Used when not measurable.
    pub estimated_watts: Option<f64>,
    pub stop_if_unprofitable: bool,
    /// Minimum estimated net profit per day (currency) below which mining stops.
    pub min_profit_per_day: f64,
}

impl Default for Energy {
    fn default() -> Self {
        Self {
            price_per_kwh: None,
            currency: "EUR".into(),
            estimated_watts: None,
            stop_if_unprofitable: false,
            min_profit_per_day: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Schedule {
    pub enabled: bool,
    /// Minutes from local midnight. start > end means the window crosses midnight.
    pub start_minute: u16,
    pub end_minute: u16,
    /// Monday = index 0.
    pub days: [bool; 7],
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            enabled: false,
            start_minute: 22 * 60,
            end_minute: 7 * 60,
            days: [true; 7],
        }
    }
}

impl Schedule {
    /// `weekday` Monday = 0, `minute` minutes since local midnight.
    pub fn is_active(&self, weekday: usize, minute: u16) -> bool {
        if self.start_minute == self.end_minute {
            return self.days[weekday % 7];
        }
        if self.start_minute < self.end_minute {
            self.days[weekday % 7] && minute >= self.start_minute && minute < self.end_minute
        } else if minute >= self.start_minute {
            self.days[weekday % 7]
        } else if minute < self.end_minute {
            // Early-morning part belongs to the window that started the previous day.
            self.days[(weekday + 6) % 7]
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkResult {
    pub hashrate: f64,
    pub threads_percent: u8,
    pub duration_secs: u32,
    pub at: i64,
    pub miner_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppConfig {
    pub version: u32,
    pub onboarding_completed: bool,
    pub demo_mode: bool,
    pub language: String,
    pub theme: String,
    /// "AUTO" or a coin id from `coins::COINS` ("PRL", "XMR", …).
    pub coin: String,
    pub wallet: String,
    pub pool: PoolConfig,
    /// XMRig (CPU).
    pub miner: MinerConfig,
    /// SRBMiner-MULTI (GPU).
    pub srbminer: MinerConfig,
    /// Optional own wallet per coin id ("PRL" → "prl1p…"). When set, that coin is mined
    /// on Kryptex straight to this address (paid in the coin itself), whatever the main
    /// payout is.
    pub coin_wallets: std::collections::BTreeMap<String, String>,
    pub performance: Performance,
    pub safety: Safety,
    pub energy: Energy,
    pub schedule: Schedule,
    pub benchmark: Option<BenchmarkResult>,
    pub check_updates: bool,
    /// Start mining as soon as Erebor (app or node) starts.
    pub start_on_launch: bool,
    /// Let other devices (laptop, phone) control this computer over the network.
    pub remote: Remote,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Remote {
    pub enabled: bool,
    pub port: u16,
}

impl Default for Remote {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 7420,
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            onboarding_completed: false,
            demo_mode: false,
            language: "it".into(),
            theme: "dark".into(),
            coin: AUTO_COIN.into(),
            wallet: String::new(),
            pool: PoolConfig::default(),
            miner: MinerConfig::default(),
            srbminer: MinerConfig::default(),
            coin_wallets: Default::default(),
            performance: Performance::default(),
            safety: Safety::default(),
            energy: Energy::default(),
            schedule: Schedule::default(),
            benchmark: None,
            check_updates: false,
            start_on_launch: false,
            remote: Remote::default(),
        }
    }
}

pub fn valid_host(h: &str) -> bool {
    !h.is_empty()
        && h.len() <= 253
        && !h.starts_with('-')
        && !h.starts_with('.')
        && h.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

pub fn valid_worker(w: &str) -> bool {
    w.len() <= 32
        && w.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

impl AppConfig {
    /// Validate everything that ends up in a process argument, a file or a network request.
    pub fn validate(&self) -> Result<(), String> {
        if !self.wallet.is_empty() {
            wallet::validate_payout(&self.wallet).map_err(|e| format!("wallet: {e:?}"))?;
            if wallet::is_kryptex_id(&self.wallet)
                && !crate::pools::is_kryptex_host(&self.pool.host)
            {
                return Err("kryptex_pool_required".into());
            }
        }
        if self.remote.port < 1024 {
            return Err("invalid_port".into());
        }
        if !valid_host(&self.pool.host) {
            return Err("pool.host: invalid hostname".into());
        }
        if self.pool.port == 0 {
            return Err("pool.port: invalid".into());
        }
        if !valid_worker(&self.pool.worker) {
            return Err("pool.worker: only letters, digits, '_' and '-' (max 32)".into());
        }
        if !(40.0..=100.0).contains(&self.safety.max_temp_c) {
            return Err("safety.maxTempC: must be between 40 and 100".into());
        }
        if !(1..=100).contains(&self.performance.custom_threads_percent) {
            return Err("performance.customThreadsPercent: 1..100".into());
        }
        for (name, v) in [
            ("energy.pricePerKwh", self.energy.price_per_kwh),
            ("energy.estimatedWatts", self.energy.estimated_watts),
        ] {
            if let Some(v) = v {
                if !v.is_finite() || !(0.0..=100_000.0).contains(&v) {
                    return Err(format!("{name}: invalid value"));
                }
            }
        }
        if !self.energy.min_profit_per_day.is_finite() {
            return Err("energy.minProfitPerDay: invalid".into());
        }
        if !["EUR", "USD"].contains(&self.energy.currency.as_str()) {
            return Err("energy.currency: EUR or USD".into());
        }
        if !["it", "en"].contains(&self.language.as_str()) {
            return Err("language: it or en".into());
        }
        if self.schedule.start_minute >= 1440 || self.schedule.end_minute >= 1440 {
            return Err("schedule: minutes must be < 1440".into());
        }
        for (id, addr) in &self.coin_wallets {
            crate::coins::find(id).ok_or("coin_wallets: unknown coin")?;
            wallet::validate_coin_address(id, addr)
                .map_err(|e| format!("coin_wallets.{id}: {e:?}"))?;
        }
        if self.coin != AUTO_COIN && !self.coin_wallets.contains_key(&self.coin) {
            let c = crate::coins::find(&self.coin).ok_or("coin: unknown")?;
            let payout = wallet::payout_kind(&self.wallet);
            // GPU coins are mined on Kryptex or unMineable: Kryptex ID or BTC address.
            if c.device == crate::coins::Device::Gpu
                && !self.wallet.is_empty()
                && !matches!(
                    payout,
                    Some(wallet::AddressKind::Kryptex | wallet::AddressKind::Bitcoin)
                )
            {
                return Err("kryptex_or_btc_required".into());
            }
            if payout == Some(wallet::AddressKind::Bitcoin) && c.unmineable_host.is_none() {
                return Err("not_on_unmineable".into());
            }
        }
        Ok(())
    }

    pub fn miner_for(&self, kind: crate::miner::MinerKind) -> &MinerConfig {
        match kind {
            crate::miner::MinerKind::Xmrig => &self.miner,
            crate::miner::MinerKind::Srbminer => &self.srbminer,
        }
    }

    pub fn miner_for_mut(&mut self, kind: crate::miner::MinerKind) -> &mut MinerConfig {
        match kind {
            crate::miner::MinerKind::Xmrig => &mut self.miner,
            crate::miner::MinerKind::Srbminer => &mut self.srbminer,
        }
    }

    pub fn load(path: &Path) -> Self {
        let mut c: Self = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        // v1 always mined XMR without asking: move it to automatic choice.
        if c.version < 2 && c.coin == "XMR" {
            c.coin = AUTO_COIN.into();
        }
        c
    }

    /// Atomic write: temp file + rename.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let tmp = path.with_extension("json.tmp");
        let data = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, data).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.json");
        assert_eq!(AppConfig::load(&p), AppConfig::default());
        let mut c = AppConfig {
            onboarding_completed: true,
            ..Default::default()
        };
        c.energy.price_per_kwh = Some(0.25);
        c.save(&p).unwrap();
        assert_eq!(AppConfig::load(&p), c);
    }

    #[test]
    fn partial_json_fills_defaults() {
        let c: AppConfig = serde_json::from_str(r#"{"language":"en"}"#).unwrap();
        assert_eq!(c.language, "en");
        assert_eq!(c.pool, PoolConfig::default());
    }

    #[test]
    fn rejects_injection_attempts() {
        let mut c = AppConfig::default();
        c.pool.host = "pool.example.com; rm -rf /".into();
        assert!(c.validate().is_err());
        let mut c = AppConfig::default();
        c.pool.worker = "w\" --malicious".into();
        assert!(c.validate().is_err());
        let c = AppConfig {
            wallet: "not-a-wallet".into(),
            ..Default::default()
        };
        assert!(c.validate().is_err());
        assert!(AppConfig::default().validate().is_ok());
    }

    #[test]
    fn presets_map_to_real_limits() {
        let mut p = Performance::default();
        assert_eq!(p.limits().0, 50);
        p.preset = Preset::Custom;
        p.custom_threads_percent = 0;
        assert_eq!(p.limits().0, 1);
    }

    #[test]
    fn schedule_windows() {
        let s = Schedule::default(); // 22:00 -> 07:00 every day
        assert!(s.is_active(0, 23 * 60));
        assert!(s.is_active(0, 6 * 60));
        assert!(!s.is_active(0, 12 * 60));
        let mut s = Schedule {
            start_minute: 9 * 60,
            end_minute: 17 * 60,
            ..Default::default()
        };
        s.days = [true, false, false, false, false, false, false];
        assert!(s.is_active(0, 10 * 60));
        assert!(!s.is_active(1, 10 * 60));
        // Overnight window started Monday is still active Tuesday 03:00 even if Tuesday is off.
        let n = Schedule {
            days: [true, false, false, false, false, false, false],
            ..Default::default()
        };
        assert!(n.is_active(1, 3 * 60));
        assert!(!n.is_active(1, 23 * 60));
    }
}

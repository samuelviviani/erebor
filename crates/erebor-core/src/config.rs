//! Persistent user configuration (JSON file in the app data dir).

use crate::wallet;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const CONFIG_VERSION: u32 = 3;

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
            worker: String::new(),
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
    /// Mine only after this many minutes without keyboard or mouse input; any input
    /// makes mining wait at once. `None`: mine whether or not the PC is in use.
    pub idle_minutes: Option<u16>,
    /// Power limit for each NVIDIA GPU while mining (W), passed to the miner. It needs
    /// administrator rights; the driver keeps it within the card's own range.
    pub gpu_power_limit_w: Option<u32>,
}

impl Default for Performance {
    fn default() -> Self {
        Self {
            preset: Preset::Balanced,
            custom_threads_percent: 50,
            idle_minutes: None,
            gpu_power_limit_w: None,
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
    /// Prices by time of day from the user's own bill. The first band that covers the
    /// current time wins; outside every band `price_per_kwh` applies.
    pub bands: Vec<PriceBand>,
    /// Months (January = 0) when the mining heat replaces other heating.
    pub heating_months: [bool; 12],
    /// Share of the electricity cost paid back by that heat in those months (0–100).
    pub heating_credit_percent: u8,
    pub solar: Solar,
    pub thermostat: Thermostat,
    /// Mining waits while it loses money at the current price and resumes when it pays.
    pub stop_if_unprofitable: bool,
    /// Minimum estimated net profit per day (currency) below which mining waits.
    pub min_profit_per_day: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PriceBand {
    /// Minutes from local midnight; start > end crosses midnight, start == end is all day.
    pub start_minute: u16,
    pub end_minute: u16,
    /// Monday = index 0.
    pub days: [bool; 7],
    pub price: f64,
}

impl Default for PriceBand {
    fn default() -> Self {
        Self {
            start_minute: 0,
            end_minute: 0,
            days: [true; 7],
            price: 0.0,
        }
    }
}

/// A room thermometer read over HTTP: mining heats the room up to `off_above_c`, waits,
/// and starts again once it cools to `on_below_c`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Thermostat {
    pub enabled: bool,
    pub url: String,
    pub token: String,
    pub field: String,
    pub on_below_c: f64,
    pub off_above_c: f64,
}

impl Default for Thermostat {
    fn default() -> Self {
        Self {
            enabled: false,
            url: String::new(),
            token: String::new(),
            field: "state".into(),
            on_below_c: 20.0,
            off_above_c: 21.0,
        }
    }
}

/// A power meter read over HTTP (Home Assistant, Shelly, an inverter's local API…).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Solar {
    pub enabled: bool,
    /// Answers JSON, e.g. `http://homeassistant.local:8123/api/states/sensor.grid_power`.
    pub url: String,
    /// Sent as `Authorization: Bearer …` when not empty (Home Assistant).
    pub token: String,
    /// Dotted path to the watts in the JSON: `state`, `total_act_power`, `emeters.0.power`.
    pub field: String,
    /// The value is positive while exporting. Off: meter convention, negative = exporting.
    pub export_positive: bool,
    /// What exported energy earns (per kWh): the cost of using it instead.
    pub feed_in_price: f64,
}

impl Default for Energy {
    fn default() -> Self {
        Self {
            price_per_kwh: None,
            currency: "EUR".into(),
            bands: Vec::new(),
            heating_months: [false; 12],
            heating_credit_percent: 0,
            solar: Solar::default(),
            thermostat: Thermostat::default(),
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
        in_window(
            &self.days,
            self.start_minute,
            self.end_minute,
            weekday,
            minute,
        )
    }
}

impl PriceBand {
    pub fn covers(&self, weekday: usize, minute: u16) -> bool {
        in_window(
            &self.days,
            self.start_minute,
            self.end_minute,
            weekday,
            minute,
        )
    }
}

/// Weekly time window. `weekday` Monday = 0, `minute` minutes since local midnight.
fn in_window(days: &[bool; 7], start: u16, end: u16, weekday: usize, minute: u16) -> bool {
    if start == end {
        return days[weekday % 7];
    }
    if start < end {
        days[weekday % 7] && minute >= start && minute < end
    } else if minute >= start {
        days[weekday % 7]
    } else if minute < end {
        // Early-morning part belongs to the window that started the previous day.
        days[(weekday + 6) % 7]
    } else {
        false
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
    /// lolMiner (GPU coins SRBMiner does not mine).
    pub lolminer: MinerConfig,
    /// cpuminer-opt (SHA-256d and Scrypt coins on the CPU).
    pub cpuminer: MinerConfig,
    /// Optional own wallet per coin id ("PRL" → "prl1p…"). When set, that coin is mined
    /// on Kryptex straight to this address (paid in the coin itself), whatever the main
    /// payout is.
    pub coin_wallets: std::collections::BTreeMap<String, String>,
    /// Pool for each coin with an own wallet (missing: the first listed, Kryptex if there).
    pub coin_pools: std::collections::BTreeMap<String, CoinPoolChoice>,
    /// Coins the automatic choice must not use, whatever they pay.
    pub auto_exclude: std::collections::BTreeSet<String>,
    pub performance: Performance,
    pub safety: Safety,
    pub energy: Energy,
    pub schedule: Schedule,
    pub benchmark: Option<BenchmarkResult>,
    pub check_updates: bool,
    /// Update the miners Erebor installed as soon as a newer verified build is pinned.
    /// Off: a notification says an update is available, the user installs it.
    pub auto_update_miners: bool,
    /// Start mining as soon as Erebor (app or agent) starts.
    pub start_on_launch: bool,
    /// Windows: the user agreed to exclude the miners folder from Defender.
    pub defender_allowed: bool,
    pub notify: Notify,
    /// Extra share of mining time given to the project on top of the fee (0 to
    /// `devfee::MAX_DONATION_PERCENT`). Off by default, only the user turns it on.
    pub donation_percent: f64,
}

/// Where a coin with an own wallet is mined.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct CoinPoolChoice {
    /// A pool of `pools::coin_pools` ("2miners", …) or "custom"; empty means Kryptex.
    pub pool: String,
    /// "eu", "us" or "asia" where the pool has regional servers.
    pub region: String,
    /// Custom stratum only.
    pub host: String,
    pub port: u16,
    pub tls: bool,
    /// Catalog pool used while the main one is unreachable; empty: none.
    pub backup: String,
}

/// Where problems are pushed. Empty fields: that channel is off.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Notify {
    pub telegram_token: String,
    pub telegram_chat: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            onboarding_completed: false,
            demo_mode: false,
            language: "en".into(),
            theme: "light".into(),
            coin: AUTO_COIN.into(),
            wallet: String::new(),
            pool: PoolConfig::default(),
            miner: MinerConfig::default(),
            srbminer: MinerConfig::default(),
            lolminer: MinerConfig::default(),
            cpuminer: MinerConfig::default(),
            coin_wallets: Default::default(),
            coin_pools: Default::default(),
            auto_exclude: Default::default(),
            performance: Performance::default(),
            safety: Safety::default(),
            energy: Energy::default(),
            schedule: Schedule::default(),
            benchmark: None,
            check_updates: false,
            start_on_launch: false,
            defender_allowed: false,
            auto_update_miners: true,
            notify: Notify::default(),
            donation_percent: 0.0,
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

/// Worker name of the project's 2% on the pool: reserved, so the owner of that account
/// can tell it apart from the PCs that mine for themselves.
pub const PROJECT_WORKER: &str = "EREBOR";

/// Name of this PC on the pool when the user did not choose one: the hostname, letters and
/// digits only (what Kryptex accepts).
pub fn default_worker() -> String {
    let w: String = crate::rpc::hostname()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(32)
        .collect();
    if w.is_empty() || w.eq_ignore_ascii_case(PROJECT_WORKER) {
        "PC".into()
    } else {
        w
    }
}

pub fn valid_worker(w: &str) -> bool {
    w.len() <= 32
        && w.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

impl AppConfig {
    /// Validate everything that ends up in a process argument, a file or a network request.
    pub fn validate(&self) -> Result<(), String> {
        if !(0.0..=crate::devfee::MAX_DONATION_PERCENT).contains(&self.donation_percent) {
            return Err("donationPercent: invalid value".into());
        }
        if !self.wallet.is_empty() {
            wallet::validate_payout(&self.wallet).map_err(|e| format!("wallet: {e:?}"))?;
            if wallet::is_kryptex_id(&self.wallet)
                && !crate::pools::is_kryptex_host(&self.pool.host)
            {
                return Err("kryptex_pool_required".into());
            }
        }
        if !valid_host(&self.pool.host) {
            return Err("pool.host: invalid hostname".into());
        }
        if self.pool.port == 0 {
            return Err("pool.port: invalid".into());
        }
        if self.pool.worker.eq_ignore_ascii_case(PROJECT_WORKER) {
            return Err("worker_reserved".into());
        }
        if !valid_worker(&self.pool.worker) {
            return Err("pool.worker: only letters, digits, '_' and '-' (max 32)".into());
        }
        if !(40.0..=100.0).contains(&self.safety.max_temp_c) {
            return Err("safety.maxTempC: must be between 40 and 100".into());
        }
        if self
            .performance
            .idle_minutes
            .is_some_and(|m| !(1..=240).contains(&m))
        {
            return Err("performance.idleMinutes: 1..240".into());
        }
        if self
            .performance
            .gpu_power_limit_w
            .is_some_and(|w| !(30..=1000).contains(&w))
        {
            return Err("performance.gpuPowerLimitW: 30..1000".into());
        }
        if !(1..=100).contains(&self.performance.custom_threads_percent) {
            return Err("performance.customThreadsPercent: 1..100".into());
        }
        let price_ok = |v: f64| v.is_finite() && (0.0..=100_000.0).contains(&v);
        if self.energy.price_per_kwh.is_some_and(|v| !price_ok(v)) {
            return Err("energy.pricePerKwh: invalid value".into());
        }
        for b in &self.energy.bands {
            if !price_ok(b.price) || b.start_minute >= 1440 || b.end_minute >= 1440 {
                return Err("energy.bands: invalid band".into());
            }
        }
        if self.energy.heating_credit_percent > 100 {
            return Err("energy.heatingCreditPercent: 0..100".into());
        }
        let n = &self.notify;
        if !n
            .telegram_token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == ':' || c == '_' || c == '-')
        {
            return Err("notify.telegramToken: invalid".into());
        }
        let th = &self.energy.thermostat;
        if th.enabled {
            if !(th.url.starts_with("http://") || th.url.starts_with("https://")) {
                return Err("energy.thermostat.url: must start with http:// or https://".into());
            }
            if !(th.on_below_c.is_finite()
                && th.off_above_c.is_finite()
                && th.on_below_c < th.off_above_c)
            {
                return Err("energy.thermostat: on below must be lower than off above".into());
            }
        }
        let s = &self.energy.solar;
        if !price_ok(s.feed_in_price) {
            return Err("energy.solar.feedInPrice: invalid value".into());
        }
        if s.enabled && !(s.url.starts_with("http://") || s.url.starts_with("https://")) {
            return Err("energy.solar.url: must start with http:// or https://".into());
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
        for (id, choice) in &self.coin_pools {
            let coin = crate::coins::find(id).ok_or("coin_pools: unknown coin")?;
            let known = crate::pools::coin_pools(coin);
            let listed = |p: &str| p.is_empty() || known.iter().any(|k| k.pool == p);
            if choice.pool == "custom" {
                if !valid_host(&choice.host) || choice.port == 0 {
                    return Err(format!("coin_pools.{id}: invalid custom pool"));
                }
            } else if !listed(&choice.pool) {
                return Err(format!("coin_pools.{id}: unknown pool"));
            }
            if !listed(&choice.backup)
                || (!choice.backup.is_empty() && choice.backup == choice.pool)
            {
                return Err(format!("coin_pools.{id}: invalid backup pool"));
            }
        }
        if self.coin != AUTO_COIN && !self.coin_wallets.contains_key(&self.coin) {
            let c = crate::coins::find(&self.coin).ok_or("coin: unknown")?;
            let payout = wallet::payout_kind(&self.wallet);
            let unmineable = wallet::via_unmineable(payout);
            // GPU coins are mined on Kryptex or unMineable: Kryptex ID or unMineable payout.
            if c.device == crate::coins::Device::Gpu
                && !self.wallet.is_empty()
                && payout != Some(wallet::AddressKind::Kryptex)
                && !unmineable
            {
                return Err("kryptex_or_btc_required".into());
            }
            if c.host.is_none() && !(unmineable && c.unmineable_host.is_some()) {
                return Err("own_wallet_required".into());
            }
            if unmineable && c.unmineable_host.is_none() {
                return Err("not_on_unmineable".into());
            }
        }
        Ok(())
    }

    pub fn miner_for(&self, kind: crate::miner::MinerKind) -> &MinerConfig {
        match kind {
            crate::miner::MinerKind::Xmrig => &self.miner,
            crate::miner::MinerKind::Srbminer => &self.srbminer,
            crate::miner::MinerKind::Lolminer => &self.lolminer,
            crate::miner::MinerKind::Cpuminer => &self.cpuminer,
        }
    }

    pub fn miner_for_mut(&mut self, kind: crate::miner::MinerKind) -> &mut MinerConfig {
        match kind {
            crate::miner::MinerKind::Xmrig => &mut self.miner,
            crate::miner::MinerKind::Srbminer => &mut self.srbminer,
            crate::miner::MinerKind::Lolminer => &mut self.lolminer,
            crate::miner::MinerKind::Cpuminer => &mut self.cpuminer,
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
        // v2's "QTC" went to Kryptex's and 2Miners' QTC pools, which mine Quantus: those
        // routes are Quantus now; "QTC" is Qubitcoin (qhash, HeroMiners).
        if c.version < 3 {
            let quantus = c
                .coin_pools
                .get("QTC")
                .is_none_or(|p| p.pool.is_empty() || p.pool == "2miners");
            if quantus {
                if let Some(w) = c.coin_wallets.remove("QTC") {
                    c.coin_wallets.insert("QUANTUS".into(), w);
                }
                if let Some(mut p) = c.coin_pools.remove("QTC") {
                    p.backup.clear(); // HeroMiners' QTC is the other coin
                    c.coin_pools.insert("QUANTUS".into(), p);
                }
                if c.coin == "QTC" {
                    c.coin = "QUANTUS".into();
                }
                if c.auto_exclude.remove("QTC") {
                    c.auto_exclude.insert("QUANTUS".into());
                }
            }
            c.version = CONFIG_VERSION;
        }
        // The old default name was the project's own: use this PC's name instead.
        if c.pool.worker.eq_ignore_ascii_case(PROJECT_WORKER) {
            c.pool.worker.clear();
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
    fn v2_qtc_on_kryptex_becomes_quantus_but_herominers_stays_qubitcoin() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.json");
        let mut c = AppConfig {
            version: 2,
            coin: "QTC".into(),
            ..Default::default()
        };
        c.coin_wallets.insert("QTC".into(), "addr".into());
        c.auto_exclude.insert("QTC".into());
        c.save(&p).unwrap();
        let m = AppConfig::load(&p);
        assert_eq!((m.coin.as_str(), m.version), ("QUANTUS", CONFIG_VERSION));
        assert!(m.coin_wallets.contains_key("QUANTUS") && !m.coin_wallets.contains_key("QTC"));
        assert!(m.auto_exclude.contains("QUANTUS"));

        c.coin_pools.insert(
            "QTC".into(),
            CoinPoolChoice {
                pool: "herominers".into(),
                ..Default::default()
            },
        );
        c.save(&p).unwrap();
        let m = AppConfig::load(&p);
        assert_eq!(m.coin, "QTC");
        assert!(m.coin_wallets.contains_key("QTC"));
        // Already v3: a Qubitcoin route on the first listed pool is left alone.
        let mut v3 = m.clone();
        v3.coin_pools.clear();
        v3.save(&p).unwrap();
        assert_eq!(AppConfig::load(&p).coin, "QTC");
    }

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

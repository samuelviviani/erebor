//! Orchestrates miner lifecycle, dev fee switching, sampling, safety rules and jobs.
//! The GUI calls into an `Arc<Mutex<Engine>>`; a background loop calls `tick()` every
//! `TICK` and a slower loop refreshes market data outside the lock.

use crate::coins::{self, BtcPrice, Coin, CoinMarket, Device};
use crate::config::{AppConfig, AUTO_COIN};
use crate::devfee::{self, DevFeeManager, DevFeeStatus, Ledger, Target};
use crate::earnings::{self, Earnings, EnergySource, Params};
use crate::hardware::{HardwareInfo, LiveHardware, Monitor};
use crate::miner::{
    DemoMiner, LaunchSpec, MinerAdapter, MinerInfo, MinerKind, MinerStats, PoolTarget,
};
use crate::pools::{self, NetworkStats, Price};
use crate::store::{HashStats, Sample, SeriesPoint, Session, Store};
use crate::wallet::AddressKind;
use crate::{install, now_unix, wallet, xmrig};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const TICK: Duration = Duration::from_secs(2);
/// Larger gaps (sleep, suspended process) are not counted as mining time.
const MAX_TICK_GAP: f64 = 10.0;
const SAMPLE_SECS: f64 = 10.0;
const HOT_TICKS_TO_STOP: u32 = 3;
const MAX_AUTO_RESTARTS: u32 = 3;
const RETENTION_DAYS: i64 = 90;
pub const BENCHMARK_SECS: u64 = 90;
const PROJECT_WORKER: &str = "erebor";
/// Automatic choice: time on an unmeasured coin before trying the next one.
const EXPLORE_SECS: u64 = 180;
/// Automatic choice: how often to re-check profitability, and the gain needed to switch.
const REEVAL_SECS: u64 = 1800;
const SWITCH_GAIN: f64 = 1.10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RunState {
    Stopped,
    Running,
    Paused,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info,
    Warning,
    Error,
}

/// UI notification. `code` is translated by the GUI; `detail` is shown as-is.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    pub id: u64,
    pub ts: i64,
    pub level: Level,
    pub code: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Market {
    pub network: Option<NetworkStats>,
    pub price: Option<Price>,
    /// Per-coin network/price data (all catalog coins with complete data).
    pub coins: Vec<CoinMarket>,
    pub btc: Option<BtcPrice>,
    pub updated: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStatus {
    pub kind: &'static str,
    pub progress: f64,
    pub done: bool,
    pub error: Option<String>,
    /// Benchmark: current hashrate; install: bytes downloaded.
    pub value: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub demo: bool,
    pub state: RunState,
    pub error: Option<String>,
    pub target: Target,
    pub miner: Option<MinerInfo>,
    pub stats: Option<MinerStats>,
    pub session_started: Option<i64>,
    pub session_secs: f64,
    pub devfee: DevFeeStatus,
    pub xmrig_donate_percent: f64,
    pub pool_fee_percent: Option<f64>,
    pub hardware: LiveHardware,
    pub power_source: EnergySource,
    pub power_w: Option<f64>,
    pub earnings_24h: Earnings,
    pub market: Market,
    pub job: Option<JobStatus>,
    pub alerts: Vec<Alert>,
    pub restart_in_secs: Option<u64>,
    /// Why mining cannot start right now (error code), if anything.
    pub start_blocker: Option<String>,
    /// Coin being mined (or that would be mined on start).
    pub coin: Coin,
    pub auto_coin: bool,
    /// Expected earnings at the current (or last measured) speed; `None` without data.
    pub projection: Option<Projection>,
    /// Coins mined so far for the user, per coin (estimate from the work done).
    pub mined: Vec<crate::store::CoinAmount>,
    /// `mined` valued in BTC at today's prices (what Kryptex/unMineable pay out);
    /// coins without a market price are left out.
    pub mined_btc: Option<f64>,
    pub advice: Advice,
}

/// What earns the most BTC on this PC: which coin, and how to be paid.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Advice {
    /// GPU when a usable graphics card is present, otherwise CPU.
    pub device: Device,
    /// Most profitable coin measured on this PC; `None` until speeds are measured.
    pub coin: Option<Coin>,
    /// Cheapest way to be paid in BTC for that coin: "kryptex" or "bitcoin" (unMineable).
    pub payout: &'static str,
    pub btc_per_day: Option<f64>,
    /// BTC/day of the current setup (chosen coin and payout), when measured.
    pub current_btc_per_day: Option<f64>,
}

/// Forward-looking estimate per day (×30 for a month in the UI), after pool, miner and
/// Erebor fees. Energy cost only when both power and electricity price are known.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Projection {
    pub hashrate: f64,
    /// True when based on the live hashrate, false when on the last measurement.
    pub live: bool,
    pub btc_per_day: f64,
    pub value_per_day: Option<f64>,
    pub energy_cost_per_day: Option<f64>,
    pub net_per_day: Option<f64>,
}

/// One entry of the coin picker.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoinOption {
    pub coin: Coin,
    /// Why it cannot be mined here, if so ("no_gpu", "kryptex_or_btc_required", …).
    pub blocker: Option<&'static str>,
    pub miner_installed: bool,
    /// Measured on this PC while mining it.
    pub hashrate: Option<f64>,
    pub btc_per_day: Option<f64>,
    pub value_per_day: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsReport {
    pub from: i64,
    pub to: i64,
    pub hash: HashStats,
    pub earnings: Earnings,
    pub series: Vec<SeriesPoint>,
    pub sessions: Vec<Session>,
}

pub type Factory =
    Box<dyn Fn(&AppConfig, &Coin, &Path, usize) -> Result<Box<dyn MinerAdapter>, String> + Send>;

pub fn default_factory() -> Factory {
    Box::new(
        |cfg: &AppConfig,
         coin: &Coin,
         data_dir: &Path,
         threads: usize|
         -> Result<Box<dyn MinerAdapter>, String> {
            if cfg.demo_mode {
                return Ok(Box::new(DemoMiner::new(threads)));
            }
            let mc = cfg.miner_for(coin.miner);
            let p = mc.path.as_deref().ok_or("miner_not_configured")?;
            let path = xmrig::validate_binary_path_prefix(p, coin.miner.dir_prefix())?;
            if let Some(expected) = &mc.sha256 {
                if xmrig::sha256_file(&path)? != *expected {
                    return Err("miner_binary_changed".into());
                }
            }
            let run = data_dir.join("run");
            Ok(match coin.miner {
                MinerKind::Xmrig => Box::new(xmrig::XmrigMiner::new(path, run, mc.version.clone())),
                MinerKind::Srbminer => Box::new(crate::srbminer::SrbMiner::new(
                    path,
                    run,
                    mc.version.clone(),
                )),
            })
        },
    )
}

fn xmr() -> &'static Coin {
    coins::find("XMR").expect("XMR in catalog")
}

fn has_mining_gpu(hw: &HardwareInfo) -> bool {
    crate::hardware::mining_gpu(&hw.gpus)
}

#[derive(Default)]
struct Acc {
    secs: f64,
    hash_secs: f64,
    hash_known_secs: f64,
    power_j: f64,
    power_secs: f64,
    temp_sum: f64,
    temp_n: u32,
    accepted: u64,
    rejected: u64,
}

pub struct Engine {
    data_dir: PathBuf,
    config: AppConfig,
    store: Store,
    monitor: Monitor,
    factory: Factory,
    miner: Option<Box<dyn MinerAdapter>>,
    state: RunState,
    error: Option<String>,
    session: Option<(i64, i64, Instant)>,
    devfee: DevFeeManager,
    last_tick: Option<Instant>,
    acc: Acc,
    acc_target: Target,
    last_shares: (u64, u64),
    stats: Option<MinerStats>,
    api_failures: u32,
    live: LiveHardware,
    market: Market,
    hot_ticks: u32,
    restart_attempts: u32,
    restart_at: Option<Instant>,
    started_by_schedule: bool,
    schedule_suppressed: bool,
    alerts: VecDeque<Alert>,
    alert_seq: u64,
    job: Arc<Mutex<Option<JobStatus>>>,
    cancel_job: Arc<AtomicBool>,
    /// Coin of the current (or last) run.
    coin: &'static Coin,
    coin_since: Option<Instant>,
    last_eval: Option<Instant>,
    /// Hashrate measured on this PC per coin id (H/s), persisted in the store.
    hashrates: HashMap<String, f64>,
    hashrates_saved: Option<Instant>,
    /// All-time coins mined for the user, refreshed every 30 s (cheap status reads).
    mined: Vec<crate::store::CoinAmount>,
    mined_at: Option<Instant>,
    has_gpu: bool,
    /// A pinned GPU miner build exists for this OS/CPU.
    gpu_platform: bool,
    /// Exclusive lock on the data dir: one engine (app or node) per computer.
    _lock: std::fs::File,
}

fn open_store(data_dir: &Path, demo: bool) -> Result<Store, String> {
    let name = if demo { "erebor-demo.db" } else { "erebor.db" };
    Store::open(&data_dir.join(name)).map_err(|e| e.to_string())
}

fn load_hashrates(store: &Store) -> HashMap<String, f64> {
    store
        .kv_get("hashrates")
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn load_ledger(store: &Store) -> Ledger {
    store
        .kv_get("devfee_ledger")
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

impl Engine {
    pub fn new(data_dir: PathBuf, factory: Factory) -> Result<Self, String> {
        std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
        // Before touching the miner pid: another engine (node service or app) may own it.
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(data_dir.join("engine.lock"))
            .map_err(|e| e.to_string())?;
        lock.try_lock().map_err(|_| "engine_busy".to_string())?;
        kill_orphan(&data_dir);
        let config = AppConfig::load(&data_dir.join("config.json"));
        let store = open_store(&data_dir, config.demo_mode)?;
        let _ = store.close_dangling_sessions(now_unix());
        let _ = store.prune(now_unix() - RETENTION_DAYS * 86_400);
        let devfee = DevFeeManager::new(
            devfee::DEFAULT_FEE_PERCENT,
            fee_enabled(&config),
            load_ledger(&store),
        );
        let mut monitor = Monitor::new();
        let live = monitor.live();
        let has_gpu = has_mining_gpu(&monitor.detect());
        let hashrates = load_hashrates(&store);
        Ok(Self {
            data_dir,
            config,
            store,
            monitor,
            factory,
            miner: None,
            state: RunState::Stopped,
            error: None,
            session: None,
            acc_target: devfee.active(),
            devfee,
            last_tick: None,
            acc: Acc::default(),
            last_shares: (0, 0),
            stats: None,
            api_failures: 0,
            live,
            market: Market::default(),
            hot_ticks: 0,
            restart_attempts: 0,
            restart_at: None,
            started_by_schedule: false,
            schedule_suppressed: false,
            alerts: VecDeque::new(),
            alert_seq: 0,
            job: Arc::new(Mutex::new(None)),
            cancel_job: Arc::new(AtomicBool::new(false)),
            coin: xmr(),
            coin_since: None,
            last_eval: None,
            hashrates,
            hashrates_saved: None,
            mined: Vec::new(),
            mined_at: None,
            has_gpu,
            gpu_platform: install::manifest_for(MinerKind::Srbminer)
                .asset_for_platform()
                .is_ok(),
            _lock: lock,
        })
    }

    // ---------- coin choice ----------

    /// Why `coin` cannot be mined here, if so.
    fn payout(&self) -> Option<AddressKind> {
        wallet::payout_kind(&self.config.wallet)
    }

    /// The user's own wallet for this coin, if set.
    fn coin_wallet(&self, coin: &Coin) -> Option<&str> {
        self.config
            .coin_wallets
            .get(coin.id)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
    }

    fn coin_blocker(&self, coin: &Coin) -> Option<&'static str> {
        let own = self.coin_wallet(coin).is_some();
        let payout = self.payout();
        if !own && payout == Some(AddressKind::Bitcoin) && coin.unmineable_host.is_none() {
            return Some("not_on_unmineable");
        }
        if let Some(b) = self.hardware_blocker(coin) {
            return Some(b);
        }
        if coin.device == Device::Gpu
            && !own
            && !matches!(payout, Some(AddressKind::Kryptex | AddressKind::Bitcoin))
        {
            return Some("kryptex_or_btc_required");
        }
        None
    }

    /// Why this PC cannot mine `coin`, whatever the payout.
    fn hardware_blocker(&self, coin: &Coin) -> Option<&'static str> {
        if coin.device != Device::Gpu {
            None
        } else if self.config.demo_mode {
            Some("not_in_demo")
        } else if !self.has_gpu {
            Some("no_gpu")
        } else if !self.gpu_platform {
            Some("platform_unsupported")
        } else {
            None
        }
    }

    /// Best (coin, payout) for BTC on this PC, among measured coins, each valued with
    /// that payout's pool fee. Ties go to Kryptex (it lists every coin).
    pub fn advice(&self) -> Advice {
        let gpu = coins::COINS
            .iter()
            .any(|c| c.device == Device::Gpu && self.hardware_blocker(c).is_none());
        let device = if gpu { Device::Gpu } else { Device::Cpu };
        let mut best: Option<(&Coin, &'static str, f64)> = None;
        for c in coins::COINS.iter().filter(|c| c.device == device) {
            let h = self.hashrates.get(c.id);
            let m = self.market.coins.iter().find(|m| m.id == c.id);
            let (Some(h), Some(m)) = (h, m) else { continue };
            let mut ways = vec![("kryptex", m.clone())];
            if c.unmineable_host.is_some() {
                ways.push((
                    "bitcoin",
                    CoinMarket {
                        pool_fee_percent: coins::UNMINEABLE_FEE_PERCENT,
                        ..m.clone()
                    },
                ));
            }
            for (way, m) in ways {
                let v = coins::btc_per_day(*h, &m, c);
                if best.is_none_or(|b| v > b.2) {
                    best = Some((c, way, v));
                }
            }
        }
        let now = self.wanted_coin();
        Advice {
            device,
            coin: best.map(|b| b.0.clone()),
            payout: best.map_or("kryptex", |b| b.1),
            btc_per_day: best.map(|b| b.2),
            current_btc_per_day: self
                .hashrates
                .get(now.id)
                .zip(self.coin_market(now.id))
                .map(|(h, m)| coins::btc_per_day(*h, &m, now)),
        }
    }

    /// Coins the automatic choice may use. A usable GPU takes priority over the CPU.
    fn candidates(&self) -> Vec<&'static Coin> {
        let gpu: Vec<_> = coins::COINS
            .iter()
            .filter(|c| c.device == Device::Gpu && self.coin_blocker(c).is_none())
            .filter(|c| self.config.miner_for(c.miner).path.is_some())
            .collect();
        if gpu.is_empty() {
            vec![xmr()]
        } else {
            gpu
        }
    }

    /// Unmeasured candidates first (to learn this PC's hashrate), then the most profitable.
    fn pick_auto(&self) -> &'static Coin {
        let c = self.candidates();
        if let Some(u) = c.iter().find(|c| !self.hashrates.contains_key(c.id)) {
            return u;
        }
        coins::best(&c, &self.hashrates, &self.markets())
            .map(|(c, _)| c)
            .unwrap_or(c[0])
    }

    fn wanted_coin(&self) -> &'static Coin {
        if self.config.coin == AUTO_COIN {
            self.pick_auto()
        } else {
            coins::find(&self.config.coin).unwrap_or_else(xmr)
        }
    }

    /// Market data as seen by this user: unMineable's fee replaces Kryptex's when the
    /// user is paid in BTC through unMineable.
    fn markets(&self) -> Vec<CoinMarket> {
        let btc = self.payout() == Some(AddressKind::Bitcoin);
        self.market
            .coins
            .iter()
            .cloned()
            .map(|mut m| {
                let own = self.config.coin_wallets.contains_key(&m.id);
                if btc && !own {
                    m.pool_fee_percent = coins::UNMINEABLE_FEE_PERCENT;
                }
                m
            })
            .collect()
    }

    fn coin_market(&self, id: &str) -> Option<CoinMarket> {
        self.markets().into_iter().find(|m| m.id == id)
    }

    fn btc_price(&self) -> Option<f64> {
        self.market.btc.as_ref().map(|b| {
            if self.config.energy.currency == "USD" {
                b.usd
            } else {
                b.eur
            }
        })
    }

    fn projection(&self) -> Option<Projection> {
        let coin = if self.is_active() {
            self.coin
        } else {
            self.wanted_coin()
        };
        let live = self
            .is_active()
            .then(|| self.stats.as_ref().and_then(|s| s.hashrate_60s))
            .flatten()
            .filter(|h| *h > 0.0);
        let hashrate = live.or_else(|| self.hashrates.get(coin.id).copied())?;
        let market = self.coin_market(coin.id)?;
        let user_share = 1.0 - self.devfee.status().percent / 100.0;
        let btc_per_day = coins::btc_per_day(hashrate, &market, coin) * user_share;
        let value_per_day = self.btc_price().map(|p| btc_per_day * p);
        let energy_cost_per_day = self
            .power()
            .0
            .zip(self.config.energy.price_per_kwh)
            .map(|(w, price)| earnings::energy_cost(w, 24.0, price));
        Some(Projection {
            hashrate,
            live: live.is_some(),
            btc_per_day,
            value_per_day,
            energy_cost_per_day,
            net_per_day: value_per_day.zip(energy_cost_per_day).map(|(v, c)| v - c),
        })
    }

    pub fn coin_options(&self) -> Vec<CoinOption> {
        coins::COINS
            .iter()
            .map(|c| {
                let hashrate = self.hashrates.get(c.id).copied();
                let btc_per_day = hashrate
                    .zip(self.coin_market(c.id))
                    .map(|(h, m)| coins::btc_per_day(h, &m, c));
                CoinOption {
                    coin: c.clone(),
                    blocker: self.coin_blocker(c),
                    miner_installed: self.config.demo_mode
                        || self.config.miner_for(c.miner).path.is_some(),
                    hashrate,
                    btc_per_day,
                    value_per_day: btc_per_day.zip(self.btc_price()).map(|(b, p)| b * p),
                }
            })
            .collect()
    }

    /// Learn the hashrate of the coin being mined (user time only, after warm-up).
    fn learn_hashrate(&mut self) {
        if self.state != RunState::Running || self.devfee.active() != Target::User {
            return;
        }
        let warm = self
            .coin_since
            .is_some_and(|t| t.elapsed() >= Duration::from_secs(60));
        let Some(h) = self.stats.as_ref().and_then(|s| s.hashrate_60s) else {
            return;
        };
        if !warm || h <= 0.0 {
            return;
        }
        let v = self.hashrates.entry(self.coin.id.to_string()).or_insert(h);
        *v = *v * 0.9 + h * 0.1;
        if self
            .hashrates_saved
            .is_none_or(|t| t.elapsed() >= Duration::from_secs(60))
        {
            self.hashrates_saved = Some(Instant::now());
            if let Ok(s) = serde_json::to_string(&self.hashrates) {
                let _ = self.store.kv_set("hashrates", &s);
            }
        }
    }

    /// Automatic choice: try each unmeasured coin for EXPLORE_SECS, then move to the most
    /// profitable one when it beats the current one by SWITCH_GAIN.
    fn auto_switch(&mut self) {
        if self.config.coin != AUTO_COIN
            || self.state != RunState::Running
            || self.devfee.active() != Target::User
        {
            return;
        }
        let Some(since) = self.coin_since else { return };
        let want = self.pick_auto();
        if want.id == self.coin.id {
            return;
        }
        let exploring = !self.hashrates.contains_key(want.id);
        let go = if exploring {
            self.hashrates.contains_key(self.coin.id)
                && since.elapsed() >= Duration::from_secs(EXPLORE_SECS)
        } else {
            if self
                .last_eval
                .is_some_and(|t| t.elapsed() < Duration::from_secs(REEVAL_SECS))
            {
                return;
            }
            self.last_eval = Some(Instant::now());
            let value = |c: &Coin| {
                self.hashrates
                    .get(c.id)
                    .zip(self.coin_market(c.id))
                    .map(|(h, m)| coins::btc_per_day(*h, &m, c))
            };
            match (value(want), value(self.coin)) {
                (Some(w), Some(cur)) => w > cur * SWITCH_GAIN,
                (Some(_), None) => true,
                _ => false,
            }
        };
        if go {
            self.alert(Level::Info, "coin_switched", Some(want.name.into()));
            let by_schedule = self.started_by_schedule;
            self.end_session("coin_switch");
            self.state = RunState::Stopped;
            if let Err(e) = self.start(by_schedule) {
                self.state = RunState::Error;
                self.error = Some(e);
            }
        }
    }

    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    fn save_config(&self) -> Result<(), String> {
        self.config.save(&self.data_dir.join("config.json"))
    }

    fn alert(&mut self, level: Level, code: &str, detail: Option<String>) {
        self.alert_seq += 1;
        if self.alerts.len() >= 20 {
            self.alerts.pop_front();
        }
        self.alerts.push_back(Alert {
            id: self.alert_seq,
            ts: now_unix(),
            level,
            code: code.into(),
            detail,
        });
    }

    pub fn is_active(&self) -> bool {
        matches!(self.state, RunState::Running | RunState::Paused)
    }

    fn job_running(&self) -> bool {
        self.job.lock().unwrap().as_ref().is_some_and(|j| !j.done)
    }

    // ---------- configuration ----------

    /// Apply settings from the GUI. Protected fields (miner binary, benchmark) can only
    /// change through their dedicated commands.
    pub fn update_config(&mut self, mut new: AppConfig) -> Result<(), String> {
        new.miner = self.config.miner.clone();
        new.srbminer = self.config.srbminer.clone();
        new.benchmark = self.config.benchmark.clone();
        new.version = crate::config::CONFIG_VERSION;
        new.wallet = new.wallet.trim().to_string();
        // A new payout identifier brings its pool along: Kryptex IDs only work on
        // Kryptex. A coin picked by hand that the new payout can't mine falls back to the
        // automatic choice instead of blocking the save.
        if wallet::is_kryptex_id(&new.wallet) && !pools::is_kryptex_host(&new.pool.host) {
            new.pool = crate::config::PoolConfig {
                worker: new.pool.worker.clone(),
                ..Default::default()
            };
        }
        if new.coin != AUTO_COIN
            && new
                .validate()
                .is_err_and(|e| e == "kryptex_or_btc_required" || e == "not_on_unmineable")
        {
            new.coin = AUTO_COIN.into();
        }
        new.validate()?;
        let relaunch = self.is_active()
            && new.demo_mode == self.config.demo_mode
            && (new.wallet != self.config.wallet
                || new.coin != self.config.coin
                || new.pool != self.config.pool
                || new.performance != self.config.performance);
        if new.demo_mode != self.config.demo_mode {
            self.stop("mode_changed");
            self.persist_ledger();
            self.store = open_store(&self.data_dir, new.demo_mode)?;
            self.devfee =
                DevFeeManager::new(devfee::DEFAULT_FEE_PERCENT, false, load_ledger(&self.store));
        }
        self.config = new;
        self.devfee.set_enabled(fee_enabled(&self.config));
        self.save_config()?;
        if relaunch {
            self.stop("config_changed");
            self.start(false)?;
        }
        Ok(())
    }

    pub fn select_miner(&mut self, path: &str) -> Result<(), String> {
        self.select_miner_kind(MinerKind::Xmrig, path)
    }

    pub fn select_miner_kind(&mut self, kind: MinerKind, path: &str) -> Result<(), String> {
        if self.is_active() {
            return Err("stop_mining_first".into());
        }
        let p = xmrig::validate_binary_path_prefix(path, kind.dir_prefix())?;
        let version = match kind {
            MinerKind::Xmrig => Some(xmrig::detect_version(&p)?),
            MinerKind::Srbminer => None,
        };
        let sha = xmrig::sha256_file(&p)?;
        let current = self.config.miner_for(kind);
        // Re-selecting the binary Erebor installed keeps it verified.
        let verified = current.sha256.as_deref() == Some(sha.as_str()) && current.verified;
        *self.config.miner_for_mut(kind) = crate::config::MinerConfig {
            path: Some(p.to_string_lossy().into_owned()),
            version: version.or_else(|| verified.then(|| current.version.clone()).flatten()),
            sha256: Some(sha),
            verified,
        };
        self.save_config()
    }

    pub fn export_config(&self) -> String {
        serde_json::to_string_pretty(&self.config).unwrap_or_default()
    }

    /// Stops mining and removes statistics, configuration and installed miners.
    pub fn delete_all_data(&mut self) -> Result<(), String> {
        self.stop("data_deleted");
        // Re-opening the production store closes the demo database if it was open.
        self.store = open_store(&self.data_dir, false)?;
        self.store.clear().map_err(|e| e.to_string())?;
        for f in [
            "erebor-demo.db",
            "erebor-demo.db-wal",
            "erebor-demo.db-shm",
            "config.json",
        ] {
            let _ = std::fs::remove_file(self.data_dir.join(f));
        }
        let _ = std::fs::remove_dir_all(self.data_dir.join("miners"));
        self.config = AppConfig::default();
        self.devfee = DevFeeManager::new(devfee::DEFAULT_FEE_PERCENT, false, Ledger::default());
        self.alerts.clear();
        Ok(())
    }

    // ---------- lifecycle ----------

    pub fn can_start(&self) -> Result<(), String> {
        let coin = self.wanted_coin();
        if !self.config.demo_mode {
            if self.config.wallet.is_empty() {
                return Err("wallet_missing".into());
            }
            if let Some(b) = self.coin_blocker(coin) {
                return Err(b.into());
            }
            if self.config.miner_for(coin.miner).path.is_none() {
                return Err("miner_not_configured".into());
            }
        }
        if self.job_running() {
            return Err("job_running".into());
        }
        self.config.validate()
    }

    fn pool_target(&self, target: Target) -> PoolTarget {
        let c = &self.config;
        let coin = self.coin;
        // Own wallet for this coin: mined on Kryptex as `address/worker`, paid in the
        // coin itself. Project time on that coin goes to the project's Kryptex account.
        if let Some(addr) = self.coin_wallet(coin) {
            let (user, pass) = match target {
                Target::User => {
                    let w: String = c
                        .pool
                        .worker
                        .chars()
                        .filter(char::is_ascii_alphanumeric)
                        .collect();
                    let w = if w.is_empty() {
                        PROJECT_WORKER.to_string()
                    } else {
                        w
                    };
                    (format!("{addr}/{w}"), "x".to_string())
                }
                Target::Project => login(
                    devfee::build_wallet().unwrap_or("DEMO-PROJECT-WALLET"),
                    PROJECT_WORKER,
                ),
            };
            return PoolTarget {
                url: format!("{}:{}", coin.host, coin.port),
                tls: false,
                user,
                pass,
            };
        }
        let (wallet, worker) = match target {
            Target::User => (
                if c.demo_mode && c.wallet.is_empty() {
                    "DEMO-WALLET"
                } else {
                    c.wallet.as_str()
                },
                c.pool.worker.as_str(),
            ),
            // Every user's project time shows up as one worker: no user rig names leak.
            // Users paid in BTC (unMineable) give project time to a BTC address on the
            // same pool; everyone else to the project's Kryptex account.
            Target::Project => (
                if self.payout() == Some(AddressKind::Bitcoin) {
                    devfee::build_btc_wallet().unwrap_or("DEMO-PROJECT-WALLET")
                } else {
                    devfee::build_wallet().unwrap_or("DEMO-PROJECT-WALLET")
                },
                PROJECT_WORKER,
            ),
        };
        // Where the work goes depends on how that wallet is paid:
        // BTC address → unMineable; Kryptex ID → Kryptex (GPU coin endpoint, or the
        // configured pool for XMR when it is a Kryptex host); Monero → configured pool.
        let (host, port, tls) = match wallet::payout_kind(wallet) {
            Some(AddressKind::Bitcoin) => (
                coin.unmineable_host.unwrap_or(coin.host),
                coins::UNMINEABLE_PORT,
                false,
            ),
            _ if coin.device == Device::Gpu => (coin.host, coin.port, false),
            Some(AddressKind::Kryptex) if !pools::is_kryptex_host(&c.pool.host) => {
                match pools::find("kryptex") {
                    Some(k) => (k.host, k.port, k.tls),
                    None => (coin.host, coin.port, false),
                }
            }
            _ => (c.pool.host.as_str(), c.pool.port, c.pool.tls),
        };
        let (user, pass) = login(wallet, worker);
        PoolTarget {
            url: format!("{host}:{port}"),
            tls,
            user,
            pass,
        }
    }

    fn launch_spec(&self, target: Target) -> LaunchSpec {
        let (threads_percent, priority) = self.config.performance.limits();
        LaunchSpec {
            pool: self.pool_target(target),
            algo: self.coin.algo.into(),
            threads_percent,
            priority,
        }
    }

    /// Start mining. `by_schedule` marks schedule-initiated runs.
    pub fn start(&mut self, by_schedule: bool) -> Result<(), String> {
        if self.is_active() {
            return Ok(());
        }
        self.can_start()?;
        let coin = self.wanted_coin();
        let mut miner = (self.factory)(&self.config, coin, &self.data_dir, self.monitor.threads())?;
        self.coin = coin;
        // Project time on a GPU coin needs a Kryptex project account.
        // Project time needs a project wallet that works on the user's pool.
        let fee_ok = self.config.demo_mode
            || if self.coin_wallet(coin).is_some() {
                devfee::build_wallet().is_some_and(wallet::is_kryptex_id)
            } else {
                match self.payout() {
                    Some(AddressKind::Bitcoin) => devfee::build_btc_wallet().is_some(),
                    _ if coin.device == Device::Gpu => {
                        devfee::build_wallet().is_some_and(wallet::is_kryptex_id)
                    }
                    _ => devfee::build_wallet().is_some(),
                }
            };
        self.devfee.set_enabled(fee_ok);
        let target = self.devfee.active();
        let spec = self.launch_spec(target);
        miner.start(&spec)?;
        if let Some(pid) = miner.pid() {
            let _ = std::fs::write(self.data_dir.join("miner.pid"), pid.to_string());
        }
        let info = miner.info();
        let label = format!(
            "{} {} · {}",
            info.name,
            info.version.clone().unwrap_or_default(),
            coin.id
        );
        let now = now_unix();
        let id = self
            .store
            .start_session(now, label.trim(), &spec.pool.url)
            .unwrap_or(-1);
        self.session = Some((id, now, Instant::now()));
        self.miner = Some(miner);
        self.state = RunState::Running;
        self.error = None;
        self.last_tick = Some(Instant::now());
        self.acc = Acc::default();
        self.acc_target = target;
        self.last_shares = (0, 0);
        self.stats = None;
        self.api_failures = 0;
        self.hot_ticks = 0;
        self.restart_at = None;
        self.started_by_schedule = by_schedule;
        self.coin_since = Some(Instant::now());
        self.alert(Level::Info, "mining_started", Some(coin.name.into()));
        Ok(())
    }

    fn end_session(&mut self, reason: &str) {
        self.flush_sample();
        if let Some(mut m) = self.miner.take() {
            m.stop();
        }
        let _ = std::fs::remove_file(self.data_dir.join("miner.pid"));
        if let Some((id, _, _)) = self.session.take() {
            let _ = self.store.end_session(id, now_unix(), reason);
        }
        self.persist_ledger();
        self.stats = None;
        self.last_tick = None;
    }

    /// Stop mining. Works even if the miner API is unreachable (process is killed).
    pub fn stop(&mut self, reason: &str) {
        let was_active = self.is_active();
        self.end_session(reason);
        self.restart_at = None;
        if reason == "user" && self.config.schedule.enabled {
            self.schedule_suppressed = true;
        }
        if self.state != RunState::Error || reason == "user" {
            self.state = RunState::Stopped;
            self.error = None;
        }
        if was_active {
            self.alert(Level::Info, "mining_stopped", Some(reason.into()));
        }
    }

    pub fn pause(&mut self) -> Result<(), String> {
        if self.state != RunState::Running {
            return Err("not_running".into());
        }
        self.miner.as_mut().ok_or("not_running")?.set_paused(true)?;
        self.flush_sample();
        self.state = RunState::Paused;
        Ok(())
    }

    pub fn resume(&mut self) -> Result<(), String> {
        if self.state != RunState::Paused {
            return Err("not_paused".into());
        }
        self.miner
            .as_mut()
            .ok_or("not_running")?
            .set_paused(false)?;
        self.state = RunState::Running;
        Ok(())
    }

    fn on_crash(&mut self, reason: String) {
        self.end_session("crash");
        self.state = RunState::Error;
        self.error = Some(reason.clone());
        self.alert(Level::Error, "miner_crashed", Some(reason));
        if self.config.safety.auto_restart && self.restart_attempts < MAX_AUTO_RESTARTS {
            self.restart_attempts += 1;
            self.restart_at =
                Some(Instant::now() + Duration::from_secs(10 * self.restart_attempts as u64));
        }
    }

    // ---------- periodic work ----------

    pub fn tick(&mut self) {
        let now = Instant::now();
        let dt = self
            .last_tick
            .map(|t| now.duration_since(t).as_secs_f64())
            .unwrap_or(0.0);
        let dt = if dt > MAX_TICK_GAP { 0.0 } else { dt };
        if self.miner.is_some() {
            self.last_tick = Some(now);
        }
        self.live = self.monitor.live();
        self.refresh_mined();

        if let Some(at) = self.restart_at {
            if now >= at {
                self.restart_at = None;
                if let Err(e) = self.start(false) {
                    self.error = Some(e.clone());
                    self.alert(Level::Error, "restart_failed", Some(e));
                } else {
                    self.alert(Level::Info, "miner_restarted", None);
                }
            }
        }
        self.apply_schedule();

        let Some(miner) = self.miner.as_mut() else {
            return;
        };
        if let Some(reason) = miner.take_exit() {
            self.on_crash(reason);
            return;
        }
        match miner.stats() {
            Ok(s) => {
                self.api_failures = 0;
                self.stats = Some(s);
            }
            Err(e) => {
                self.api_failures += 1;
                if self.api_failures >= 3 {
                    self.stats = None;
                }
                if self.api_failures == 15 {
                    self.alert(Level::Warning, "miner_api_unreachable", Some(e));
                }
            }
        }

        let hashing = self.state == RunState::Running
            && self
                .stats
                .as_ref()
                .is_some_and(|s| !s.paused && s.current_hashrate().is_some_and(|h| h > 0.0));
        let eff = if hashing { dt } else { 0.0 };
        self.accumulate(eff);

        let prev = self.devfee.active();
        let next = self.devfee.tick(eff);
        if next != prev {
            self.switch_target(next);
        }

        if let Some((_, _, started)) = self.session {
            if started.elapsed() > Duration::from_secs(600) {
                self.restart_attempts = 0;
            }
        }
        self.check_temperature();
        self.check_economics();
        self.learn_hashrate();
        self.auto_switch();
        self.refresh_mined();
    }

    fn mined_btc(&self) -> Option<f64> {
        let mut total = None;
        for m in &self.mined {
            if let Some(c) = self.market.coins.iter().find(|c| c.id == m.coin) {
                *total.get_or_insert(0.0) += m.amount * c.price_btc;
            }
        }
        total
    }

    fn refresh_mined(&mut self) {
        if self
            .mined_at
            .is_some_and(|t| t.elapsed() < Duration::from_secs(30))
        {
            return;
        }
        self.mined_at = Some(Instant::now());
        self.mined = self
            .store
            .coin_totals(0, now_unix() + 1)
            .unwrap_or_default();
    }

    fn switch_target(&mut self, next: Target) {
        self.flush_sample();
        self.acc_target = next;
        let pool = self.pool_target(next);
        let res = match self.miner.as_mut() {
            Some(m) => m.switch_pool(&pool),
            None => return,
        };
        let code = match next {
            Target::Project => "devfee_started",
            Target::User => "devfee_ended",
        };
        self.alert(Level::Info, code, None);
        if let Err(e) = res {
            // Hot switch failed: restart the miner on the right target.
            self.alert(Level::Warning, "switch_failed_restarting", Some(e));
            let by_schedule = self.started_by_schedule;
            self.end_session("devfee_switch");
            self.state = RunState::Stopped;
            if let Err(e) = self.start(by_schedule) {
                self.state = RunState::Error;
                self.error = Some(e);
            }
        }
    }

    fn accumulate(&mut self, eff: f64) {
        let Some(s) = &self.stats else { return };
        let (a, r) = (s.accepted, s.rejected);
        let delta = |cur: u64, last: u64| if cur >= last { cur - last } else { cur };
        self.acc.accepted += delta(a, self.last_shares.0);
        self.acc.rejected += delta(r, self.last_shares.1);
        self.last_shares = (a, r);
        if eff <= 0.0 {
            return;
        }
        self.acc.secs += eff;
        if let Some(h) = s.current_hashrate() {
            self.acc.hash_secs += h * eff;
            self.acc.hash_known_secs += eff;
        }
        if let Some(p) = self.live.power_w {
            self.acc.power_j += p * eff;
            self.acc.power_secs += eff;
        }
        if let Some(t) = self.live.cpu_temp_c {
            self.acc.temp_sum += t as f64;
            self.acc.temp_n += 1;
        }
        if self.acc.secs >= SAMPLE_SECS {
            self.flush_sample();
        }
    }

    fn flush_sample(&mut self) {
        let a = std::mem::take(&mut self.acc);
        if a.secs <= 0.0 {
            return;
        }
        let sample = Sample {
            ts: now_unix(),
            secs: a.secs,
            hashrate: (a.hash_known_secs > 0.0).then(|| a.hash_secs / a.hash_known_secs),
            target: self.acc_target,
            accepted: a.accepted,
            rejected: a.rejected,
            // Coin-per-hash of the coin being mined (column name kept from v1).
            xmr_per_hash: if self.coin.id == "XMR" {
                self.market.network.as_ref().and_then(|n| n.xmr_per_hash())
            } else {
                self.coin_market(self.coin.id).map(|m| m.coin_per_hash)
            },
            // Only full-window measurements are stored; estimates are applied at query time.
            power_w: (a.power_secs >= a.secs * 0.99).then(|| a.power_j / a.power_secs),
            power_measured: true,
            temp_c: (a.temp_n > 0).then(|| a.temp_sum / a.temp_n as f64),
            coin: self.coin.id.into(),
            btc_per_hash: self
                .coin_market(self.coin.id)
                .map(|m| m.btc_per_hash(self.coin)),
        };
        if let Err(e) = self.store.insert_sample(&sample) {
            self.alert(Level::Warning, "storage_error", Some(e.to_string()));
        }
        self.persist_ledger();
    }

    fn persist_ledger(&self) {
        if let Ok(s) = serde_json::to_string(&self.devfee.ledger()) {
            let _ = self.store.kv_set("devfee_ledger", &s);
        }
    }

    fn check_temperature(&mut self) {
        let Some(t) = self.live.max_temp_c() else {
            self.hot_ticks = 0;
            return;
        };
        if t < self.config.safety.max_temp_c {
            self.hot_ticks = 0;
            return;
        }
        self.hot_ticks += 1;
        if self.hot_ticks == 1 {
            self.alert(
                Level::Warning,
                "temperature_high",
                Some(format!("{t:.0} °C")),
            );
        }
        if self.config.safety.stop_on_overheat && self.hot_ticks >= HOT_TICKS_TO_STOP {
            self.stop("overheat");
            self.alert(Level::Error, "stopped_overheat", Some(format!("{t:.0} °C")));
        }
    }

    fn params(&self) -> Params {
        let price = self.market.price.as_ref().map(|p| {
            if self.config.energy.currency == "USD" {
                p.usd
            } else {
                p.eur
            }
        });
        Params {
            xmrig_donate_percent: self
                .miner
                .as_ref()
                .map(|m| m.info().builtin_donation_percent)
                .unwrap_or(if self.config.demo_mode {
                    0.0
                } else {
                    xmrig::XMRIG_DONATE_LEVEL as f64
                }),
            pool_fee_percent: if self.coin.device == Device::Gpu
                || self.payout() == Some(AddressKind::Bitcoin)
            {
                self.coin_market(self.coin.id).map(|m| m.pool_fee_percent)
            } else {
                pools::find(&self.config.pool.id).map(|p| p.fee_percent)
            },
            price,
            btc_price: self.btc_price(),
            price_per_kwh: self.config.energy.price_per_kwh,
            estimated_watts: self.config.energy.estimated_watts,
        }
    }

    fn power(&self) -> (Option<f64>, EnergySource) {
        match (self.live.power_w, self.config.energy.estimated_watts) {
            (Some(p), _) => (Some(p), EnergySource::Measured),
            (None, Some(w)) => (Some(w), EnergySource::Estimated),
            _ => (None, EnergySource::Unavailable),
        }
    }

    fn check_economics(&mut self) {
        let e = &self.config.energy;
        if !e.stop_if_unprofitable || self.state != RunState::Running {
            return;
        }
        let Some((_, _, started)) = self.session else {
            return;
        };
        if started.elapsed() < Duration::from_secs(600) {
            return;
        }
        let min = e.min_profit_per_day;
        let per_hash = if self.coin.id == "XMR" {
            self.market.network.as_ref().and_then(|n| n.xmr_per_hash())
        } else {
            self.coin_market(self.coin.id)
                .map(|m| m.btc_per_hash(self.coin))
        };
        let (Some(h), Some(ph), (Some(w), _)) = (
            self.stats.as_ref().and_then(|s| s.hashrate_60s),
            per_hash,
            self.power(),
        ) else {
            return; // Not enough reliable data: never stop on a guess.
        };
        let params = self.params();
        let fee = self.devfee.status().percent;
        let projected = if self.coin.id == "XMR" {
            earnings::daily_profit(h, ph, &params, fee, w)
        } else {
            earnings::daily_profit_btc(h, ph, &params, fee, w)
        };
        if let Some(p) = projected {
            if p < min {
                self.stop("unprofitable");
                self.alert(
                    Level::Warning,
                    "stopped_unprofitable",
                    Some(format!("{p:.2}")),
                );
            }
        }
    }

    fn apply_schedule(&mut self) {
        if !self.config.schedule.enabled {
            return;
        }
        use chrono::{Datelike, Local, Timelike};
        let now = Local::now();
        let active = self.config.schedule.is_active(
            now.weekday().num_days_from_monday() as usize,
            (now.hour() * 60 + now.minute()) as u16,
        );
        if active {
            if self.state == RunState::Stopped
                && !self.schedule_suppressed
                && self.restart_at.is_none()
                && self.can_start().is_ok()
            {
                if let Err(e) = self.start(true) {
                    self.alert(Level::Error, "schedule_start_failed", Some(e));
                    self.schedule_suppressed = true;
                }
            }
        } else {
            self.schedule_suppressed = false;
            if self.is_active() && self.started_by_schedule {
                self.stop("schedule");
            }
        }
    }

    // ---------- market data ----------

    /// Network difficulty is global: use the current pool's API, else any pool that has one.
    pub fn market_api_base(&self) -> &'static str {
        pools::find(&self.config.pool.id)
            .filter(|p| p.api == pools::PoolApi::NodejsPool)
            .or_else(|| {
                pools::CATALOG
                    .iter()
                    .find(|p| p.api == pools::PoolApi::NodejsPool)
            })
            .map(|p| p.api_base)
            .unwrap_or("")
    }

    pub fn set_market(
        &mut self,
        network: Result<NetworkStats, String>,
        price: Result<Price, String>,
        coin_markets: Result<(Vec<CoinMarket>, Option<BtcPrice>), String>,
    ) {
        let mut errors = vec![];
        match coin_markets {
            Ok((c, b)) => {
                self.market.coins = c;
                self.market.btc = b.or(self.market.btc.take());
            }
            Err(e) => errors.push(format!("coins: {e}")),
        }
        match network {
            Ok(n) => self.market.network = Some(n),
            Err(e) => errors.push(format!("network: {e}")),
        }
        match price {
            Ok(p) => self.market.price = Some(p),
            Err(e) => errors.push(format!("price: {e}")),
        }
        self.market.updated = Some(now_unix());
        self.market.error = (!errors.is_empty()).then(|| errors.join("; "));
    }

    // ---------- views ----------

    pub fn status(&self) -> Status {
        let now = now_unix();
        let totals = self.store.totals(now - 86_400, now + 1).unwrap_or_default();
        let (power_w, power_source) = self.power();
        // Clone in its own statement: a guard inside the struct literal would
        // live until the end of it and deadlock with can_start() below.
        let job = self.job.lock().unwrap().clone();
        Status {
            demo: self.config.demo_mode,
            state: self.state,
            error: self.error.clone(),
            target: self.devfee.active(),
            miner: self.miner.as_ref().map(|m| m.info()),
            stats: self.stats.clone(),
            session_started: self.session.map(|(_, ts, _)| ts),
            session_secs: self
                .session
                .map(|(_, _, i)| i.elapsed().as_secs_f64())
                .unwrap_or(0.0),
            devfee: self.devfee.status(),
            xmrig_donate_percent: self.params().xmrig_donate_percent,
            pool_fee_percent: pools::find(&self.config.pool.id).map(|p| p.fee_percent),
            hardware: self.live.clone(),
            power_source,
            power_w,
            earnings_24h: earnings::summarize(&totals, &self.params()),
            market: self.market.clone(),
            job,
            alerts: self.alerts.iter().cloned().collect(),
            restart_in_secs: self
                .restart_at
                .map(|t| t.saturating_duration_since(Instant::now()).as_secs()),
            start_blocker: self.can_start().err(),
            coin: if self.is_active() {
                self.coin.clone()
            } else {
                self.wanted_coin().clone()
            },
            auto_coin: self.config.coin == AUTO_COIN,
            projection: self.projection(),
            mined: self.mined.clone(),
            mined_btc: self.mined_btc(),
            advice: self.advice(),
        }
    }

    pub fn stats_report(&self, range_secs: i64) -> Result<StatsReport, String> {
        let to = now_unix() + 1;
        let from = to - range_secs.clamp(60, 366 * 86_400);
        let e = |e: rusqlite::Error| e.to_string();
        let totals = self.store.totals(from, to).map_err(e)?;
        Ok(StatsReport {
            from,
            to,
            hash: self.store.hash_stats(from, to).map_err(e)?,
            earnings: earnings::summarize(&totals, &self.params()),
            series: self.store.series(from, to, (to - from) / 60).map_err(e)?,
            sessions: self.store.sessions(50).map_err(e)?,
        })
    }

    pub fn logs(&self) -> Vec<String> {
        self.miner.as_ref().map(|m| m.logs()).unwrap_or_default()
    }

    pub fn detect_hardware(&mut self) -> HardwareInfo {
        self.monitor.detect()
    }

    pub fn dismiss_alert(&mut self, id: u64) {
        self.alerts.retain(|a| a.id != id);
    }

    pub fn cancel_job(&self) {
        self.cancel_job.store(true, Ordering::Relaxed);
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop("app_exit");
    }
}

/// Stratum login. Kryptex account mining: `krxID/worker` (Latin letters and digits only).
/// Monero address pools: the address as user, the worker as password.
fn login(wallet: &str, worker: &str) -> (String, String) {
    if wallet::payout_kind(wallet) == Some(AddressKind::Bitcoin) {
        // unMineable: payout coin, address and worker in the user field.
        let w: String = worker.chars().filter(char::is_ascii_alphanumeric).collect();
        let w = if w.is_empty() {
            "erebor".to_string()
        } else {
            w
        };
        return (format!("BTC:{wallet}.{w}"), "x".into());
    }
    if wallet::is_kryptex_id(wallet) {
        let w: String = worker.chars().filter(char::is_ascii_alphanumeric).collect();
        let user = if w.is_empty() {
            wallet.to_string()
        } else {
            format!("{wallet}/{w}")
        };
        (user, "x".into())
    } else {
        let pass = if worker.is_empty() { "x" } else { worker };
        (wallet.to_string(), pass.to_string())
    }
}

fn fee_enabled(c: &AppConfig) -> bool {
    c.demo_mode || devfee::build_wallet().is_some() || devfee::build_btc_wallet().is_some()
}

/// If Erebor itself crashed, a miner it started could survive. Kill it on next launch,
/// so mining never continues without the UI.
fn kill_orphan(data_dir: &Path) {
    let pid_file = data_dir.join("miner.pid");
    let Some(pid) = std::fs::read_to_string(&pid_file)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
    else {
        return;
    };
    let mut sys = sysinfo::System::new();
    sys.refresh_processes();
    if let Some(p) = sys.process(sysinfo::Pid::from_u32(pid)) {
        let name = p.name().to_ascii_lowercase();
        if name.contains("xmrig") || name.contains("srbminer") {
            p.kill();
        }
    }
    let _ = std::fs::remove_file(pid_file);
}

// ---------- background jobs (run without holding the engine lock) ----------

fn set_job(job: &Arc<Mutex<Option<JobStatus>>>, s: JobStatus) {
    *job.lock().unwrap() = Some(s);
}

/// Download, verify and install the pinned XMRig release.
pub fn spawn_install(engine: Arc<Mutex<Engine>>) -> Result<(), String> {
    spawn_install_kind(engine, MinerKind::Xmrig)
}

/// Download, verify and install the pinned release of `kind`.
pub fn spawn_install_kind(engine: Arc<Mutex<Engine>>, kind: MinerKind) -> Result<(), String> {
    let (job, miners_dir) = {
        let e = engine.lock().unwrap();
        if e.job_running() {
            return Err("job_running".into());
        }
        if e.is_active() {
            return Err("stop_mining_first".into());
        }
        install::manifest_for(kind).asset_for_platform()?;
        e.cancel_job.store(false, Ordering::Relaxed);
        (e.job.clone(), e.data_dir.join("miners"))
    };
    let mk = |progress: f64, value: Option<f64>| JobStatus {
        kind: "install",
        progress,
        done: false,
        error: None,
        value,
    };
    set_job(&job, mk(0.0, None));
    std::thread::spawn(move || {
        let j = job.clone();
        let res = install::install_pinned_kind(kind, &miners_dir, &move |got, total| {
            let p = total.map(|t| got as f64 / t.max(1) as f64).unwrap_or(0.0);
            set_job(&j, mk(p.min(0.99), Some(got as f64)));
        });
        let mut e = engine.lock().unwrap();
        let mut status = mk(1.0, None);
        status.done = true;
        match res {
            Ok(inst) => {
                *e.config.miner_for_mut(kind) = crate::config::MinerConfig {
                    path: Some(inst.path.to_string_lossy().into_owned()),
                    version: Some(inst.version),
                    sha256: Some(inst.sha256),
                    verified: true,
                };
                if let Err(err) = e.save_config() {
                    status.error = Some(err);
                }
                e.alert(Level::Info, "miner_installed", None);
            }
            Err(err) => status.error = Some(err),
        }
        set_job(&job, status);
    });
    Ok(())
}

/// Offline benchmark with declared duration (BENCHMARK_SECS) and thread share.
pub fn spawn_benchmark(engine: Arc<Mutex<Engine>>) -> Result<(), String> {
    let (job, cancel, binary, threads, version) = {
        let e = engine.lock().unwrap();
        if e.job_running() {
            return Err("job_running".into());
        }
        if e.is_active() {
            return Err("stop_mining_first".into());
        }
        if e.config.demo_mode {
            return Err("not_in_demo".into());
        }
        let path = e.config.miner.path.clone().ok_or("miner_not_configured")?;
        let binary = xmrig::validate_binary_path(&path)?;
        e.cancel_job.store(false, Ordering::Relaxed);
        (
            e.job.clone(),
            e.cancel_job.clone(),
            binary,
            e.config.performance.limits().0,
            e.config.miner.version.clone(),
        )
    };
    let mk = |progress: f64, value: Option<f64>| JobStatus {
        kind: "benchmark",
        progress,
        done: false,
        error: None,
        value,
    };
    set_job(&job, mk(0.0, None));
    std::thread::spawn(move || {
        let j = job.clone();
        let res = xmrig::run_benchmark(
            &binary,
            threads,
            Duration::from_secs(BENCHMARK_SECS),
            &cancel,
            &move |p, h| set_job(&j, mk(p.min(0.99), h)),
        );
        let mut status = mk(1.0, None);
        status.done = true;
        match res {
            Ok(h) => {
                status.value = Some(h);
                let mut e = engine.lock().unwrap();
                e.config.benchmark = Some(crate::config::BenchmarkResult {
                    hashrate: h,
                    threads_percent: threads,
                    duration_secs: BENCHMARK_SECS as u32,
                    at: now_unix(),
                    miner_version: version,
                });
                let _ = e.save_config();
            }
            Err(err) => status.error = Some(err),
        }
        set_job(&job, status);
    });
    Ok(())
}

/// Background loops: engine tick every TICK, market data every 10 minutes.
pub fn spawn_loops(engine: Arc<Mutex<Engine>>) {
    let e2 = engine.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(TICK);
        e2.lock().unwrap().tick();
    });
    std::thread::spawn(move || loop {
        let (base, demo) = {
            let e = engine.lock().unwrap();
            (e.market_api_base(), e.config.demo_mode)
        };
        if demo {
            // Demo mode: fixed, clearly simulated market values; no network calls.
            engine.lock().unwrap().set_market(
                Ok(NetworkStats {
                    difficulty: 300e9,
                    block_reward: 0.6,
                    height: None,
                }),
                Ok(Price {
                    eur: 150.0,
                    usd: 160.0,
                    source: "demo",
                }),
                Ok((
                    vec![CoinMarket {
                        id: "XMR".into(),
                        coin_per_hash: 0.6 / 300e9,
                        price_btc: 0.0026,
                        pool_fee_percent: 1.0,
                    }],
                    Some(BtcPrice {
                        eur: 58_000.0,
                        usd: 62_000.0,
                    }),
                )),
            );
        } else {
            let n = pools::fetch_network_stats(base);
            let p = pools::fetch_price();
            let c = coins::fetch_markets();
            engine.lock().unwrap().set_market(n, p, c);
        }
        std::thread::sleep(Duration::from_secs(600));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    #[derive(Default)]
    struct Shared {
        starts: AtomicU32,
        stops: AtomicU32,
        switches: Mutex<Vec<String>>,
        coins: Mutex<Vec<String>>,
        crash: AtomicBool,
        fail_switch: AtomicBool,
    }

    struct MockMiner {
        s: Arc<Shared>,
        running: bool,
    }

    impl MinerAdapter for MockMiner {
        fn info(&self) -> MinerInfo {
            MinerInfo {
                id: "mock",
                name: "Mock",
                version: Some("1".into()),
                algorithms: vec!["rx/0"],
                supports_pause: true,
                supports_hot_switch: true,
                supports_benchmark: false,
                builtin_donation_percent: 1.0,
            }
        }
        fn start(&mut self, _: &LaunchSpec) -> Result<(), String> {
            self.s.starts.fetch_add(1, Ordering::SeqCst);
            self.running = true;
            Ok(())
        }
        fn stop(&mut self) {
            if self.running {
                self.s.stops.fetch_add(1, Ordering::SeqCst);
            }
            self.running = false;
        }
        fn set_paused(&mut self, _: bool) -> Result<(), String> {
            Ok(())
        }
        fn switch_pool(&mut self, p: &PoolTarget) -> Result<(), String> {
            self.s.switches.lock().unwrap().push(p.user.clone());
            if self.s.fail_switch.load(Ordering::SeqCst) {
                return Err("api down".into());
            }
            Ok(())
        }
        fn stats(&mut self) -> Result<MinerStats, String> {
            Ok(MinerStats {
                hashrate_10s: Some(1000.0),
                hashrate_60s: Some(1000.0),
                accepted: 3,
                connected: true,
                ..Default::default()
            })
        }
        fn take_exit(&mut self) -> Option<String> {
            (self.running && self.s.crash.swap(false, Ordering::SeqCst)).then(|| {
                self.running = false;
                "boom".to_string()
            })
        }
        fn logs(&self) -> Vec<String> {
            vec![]
        }
        fn pid(&self) -> Option<u32> {
            None
        }
    }

    const WALLET: &str = "48edfHu7V9Z84YzzMa6fUueoELZ9ZRXq9VetWzYGzKt52XU5xvqgzYnDK9URnRoJMk1j8nLwEVsaSWJ4fhdUyZijBGUicoD";

    fn engine(dir: &Path, demo: bool) -> (Engine, Arc<Shared>) {
        let s = Arc::new(Shared::default());
        let s2 = s.clone();
        let factory: Factory = Box::new(
            move |_: &AppConfig,
                  coin: &Coin,
                  _: &Path,
                  _: usize|
                  -> Result<Box<dyn MinerAdapter>, String> {
                s2.coins.lock().unwrap().push(coin.id.to_string());
                Ok(Box::new(MockMiner {
                    s: s2.clone(),
                    running: false,
                }))
            },
        );
        let mut e = Engine::new(dir.to_path_buf(), factory).unwrap();
        let mut c = e.config().clone();
        c.demo_mode = demo;
        c.onboarding_completed = true;
        c.wallet = WALLET.into();
        e.update_config(c).unwrap();
        e.config.miner.path = Some("/mock/xmrig".into());
        (e, s)
    }

    #[test]
    fn starts_without_fee_prompt_but_needs_a_wallet() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, s) = engine(d.path(), false);
        let mut c = e.config().clone();
        c.wallet.clear();
        e.update_config(c).unwrap();
        assert_eq!(e.start(false), Err("wallet_missing".into()));
        assert_eq!(s.starts.load(Ordering::SeqCst), 0);
        let mut c = e.config().clone();
        c.wallet = WALLET.into();
        e.update_config(c).unwrap();
        e.start(false).unwrap();
        assert_eq!(e.status().state, RunState::Running);
    }

    #[test]
    fn kryptex_login_and_pool_rules() {
        assert_eq!(
            login("krxABC123", "my-rig_1"),
            ("krxABC123/myrig1".into(), "x".into())
        );
        assert_eq!(login("krxABC123", ""), ("krxABC123".into(), "x".into()));
        assert_eq!(login(WALLET, "rig"), (WALLET.into(), "rig".into()));
        let d = tempfile::tempdir().unwrap();
        let (mut e, _) = engine(d.path(), false);
        let mut c = e.config().clone();
        c.wallet = "krxABC123".into();
        c.pool.host = "pool.supportxmr.com".into();
        c.pool.id = "supportxmr".into();
        // Saving a Kryptex ID moves the pool to Kryptex instead of failing.
        e.update_config(c).unwrap();
        assert_eq!(e.config().pool.id, "kryptex");
        let t = e.pool_target(Target::User);
        assert_eq!(t.url, "xmr.kryptex.network:7029");
        assert_eq!(t.user, "krxABC123/erebor");
    }

    #[test]
    fn start_stop_and_session_history() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, s) = engine(d.path(), false);
        e.start(false).unwrap();
        e.start(false).unwrap(); // idempotent: no second process
        assert_eq!(s.starts.load(Ordering::SeqCst), 1);
        e.pause().unwrap();
        assert_eq!(e.status().state, RunState::Paused);
        e.resume().unwrap();
        e.stop("user");
        assert_eq!(e.status().state, RunState::Stopped);
        let sessions = e.store.sessions(10).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].end_reason.as_deref(), Some("user"));
    }

    #[test]
    fn crash_is_detected_and_restarted_when_enabled() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, s) = engine(d.path(), false);
        let mut c = e.config().clone();
        c.safety.auto_restart = true;
        e.update_config(c).unwrap();
        e.start(false).unwrap();
        s.crash.store(true, Ordering::SeqCst);
        e.tick();
        assert_eq!(e.status().state, RunState::Error);
        assert!(e.restart_at.is_some());
        e.restart_at = Some(Instant::now()); // fast-forward
        e.tick();
        assert_eq!(e.status().state, RunState::Running);
        assert_eq!(s.starts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn crash_without_auto_restart_stays_stopped() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, s) = engine(d.path(), false);
        e.start(false).unwrap();
        s.crash.store(true, Ordering::SeqCst);
        e.tick();
        assert_eq!(e.status().state, RunState::Error);
        assert!(e.restart_at.is_none());
        e.stop("user");
        assert_eq!(e.status().state, RunState::Stopped);
    }

    #[test]
    fn effective_time_is_sampled_and_ledger_persisted() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, _) = engine(d.path(), true); // demo: dev fee simulated
        e.start(false).unwrap();
        // Simulate 2s ticks by moving last_tick back.
        for _ in 0..20 {
            e.last_tick = Some(Instant::now() - Duration::from_secs(2));
            e.tick();
        }
        e.stop("user");
        let t = e.store.totals(0, now_unix() + 10).unwrap();
        assert!((t.user_secs - 40.0).abs() < 1.0, "{}", t.user_secs);
        let ledger = load_ledger(&e.store);
        assert!((ledger.user_secs - 40.0).abs() < 1.0);
    }

    #[test]
    fn devfee_switches_and_falls_back_to_restart() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, s) = engine(d.path(), true);
        // Pretend 100 minutes were already mined: the next tick owes a project block.
        e.devfee = DevFeeManager::new(
            devfee::DEFAULT_FEE_PERCENT,
            true,
            Ledger {
                user_secs: 5_995.0,
                project_secs: 0.0,
            },
        );
        e.start(false).unwrap();
        e.last_tick = Some(Instant::now() - Duration::from_secs(9));
        e.tick();
        assert_eq!(e.status().target, Target::Project);
        assert_eq!(
            s.switches.lock().unwrap().as_slice(),
            ["DEMO-PROJECT-WALLET"]
        );
        // Hot switch failure → miner restarted on the user target.
        s.fail_switch.store(true, Ordering::SeqCst);
        for _ in 0..70 {
            e.last_tick = Some(Instant::now() - Duration::from_secs(2));
            e.tick();
        }
        assert_eq!(e.status().target, Target::User);
        assert!(s.starts.load(Ordering::SeqCst) >= 2);
        assert_eq!(e.status().state, RunState::Running);
    }

    #[test]
    fn sleep_gaps_are_not_counted() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, _) = engine(d.path(), true);
        e.start(false).unwrap();
        e.last_tick = Some(Instant::now() - Duration::from_secs(3600));
        e.tick();
        e.stop("user");
        assert_eq!(e.devfee.ledger().user_secs, 0.0);
    }

    #[test]
    fn demo_and_production_data_are_separate() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, _) = engine(d.path(), true);
        e.start(false).unwrap();
        for _ in 0..10 {
            e.last_tick = Some(Instant::now() - Duration::from_secs(2));
            e.tick();
        }
        let mut c = e.config().clone();
        c.demo_mode = false;
        e.update_config(c).unwrap();
        assert_eq!(e.status().state, RunState::Stopped);
        let t = e.store.totals(0, now_unix() + 10).unwrap();
        assert_eq!(t.user_secs, 0.0);
        assert!(d.path().join("erebor-demo.db").exists());
    }

    #[test]
    fn config_is_restored_after_restart() {
        let d = tempfile::tempdir().unwrap();
        {
            let _ = engine(d.path(), false);
        }
        let e = Engine::new(d.path().to_path_buf(), default_factory()).unwrap();
        assert_eq!(e.config().wallet, WALLET);
    }

    #[test]
    fn protected_fields_cannot_be_set_via_update() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, _) = engine(d.path(), false);
        let mut c = e.config().clone();
        c.miner.path = Some("/tmp/evil".into());
        e.update_config(c).unwrap();
        assert_eq!(e.config().miner.path.as_deref(), Some("/mock/xmrig"));
    }

    #[test]
    fn delete_all_data_resets() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, _) = engine(d.path(), false);
        e.start(false).unwrap();
        e.delete_all_data().unwrap();
        assert_eq!(e.status().state, RunState::Stopped);
        assert_eq!(e.config(), &AppConfig::default());
        assert!(!d.path().join("config.json").exists());
    }

    fn market(id: &str, coin_per_hash: f64) -> CoinMarket {
        CoinMarket {
            id: id.into(),
            coin_per_hash,
            price_btc: 1e-5,
            pool_fee_percent: 1.0,
        }
    }

    #[test]
    fn without_a_gpu_auto_mines_monero_on_the_cpu() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, s) = engine(d.path(), false);
        e.has_gpu = false;
        assert_eq!(e.config().coin, AUTO_COIN);
        e.start(false).unwrap();
        assert_eq!(s.coins.lock().unwrap().as_slice(), ["XMR"]);
        assert_eq!(e.status().coin.id, "XMR");
    }

    #[test]
    fn gpu_coins_need_a_kryptex_id_and_a_gpu() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, _) = engine(d.path(), false);
        e.has_gpu = true;
        e.gpu_platform = true;
        let prl = coins::find("PRL").unwrap();
        assert_eq!(e.coin_blocker(prl), Some("kryptex_or_btc_required"));
        let mut c = e.config().clone();
        c.coin = "PRL".into();
        // Not mineable with a Monero address: back to the automatic choice.
        e.update_config(c).unwrap();
        assert_eq!(e.config().coin, AUTO_COIN);
        e.has_gpu = false;
        let mut c = e.config().clone();
        c.wallet = "krxABC123".into();
        e.update_config(c).unwrap();
        assert_eq!(e.coin_blocker(prl), Some("no_gpu"));
    }

    #[test]
    fn auto_explores_gpu_coins_then_keeps_the_most_profitable() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, s) = engine(d.path(), false);
        e.has_gpu = true;
        e.gpu_platform = true;
        e.config.srbminer.path = Some("/mock/SRBMiner-MULTI".into());
        let mut c = e.config().clone();
        c.wallet = "krxABC123".into();
        e.update_config(c).unwrap();
        // GPU first: the first unmeasured GPU coin is tried.
        e.start(false).unwrap();
        assert_eq!(e.coin.id, "PRL");
        assert_eq!(e.pool_target(Target::User).url, "prl.kryptex.network:7048");
        assert_eq!(e.launch_spec(Target::User).algo, "pearlhash");
        // After warm-up the hashrate is learned; after EXPLORE_SECS the next coin is tried.
        e.coin_since = Some(Instant::now() - Duration::from_secs(EXPLORE_SECS + 5));
        e.tick();
        assert!(e.hashrates.contains_key("PRL"));
        assert_eq!(e.coin.id, "ETC");
        assert_eq!(s.coins.lock().unwrap().as_slice(), ["PRL", "ETC"]);
        // Everything measured: PRL pays 3x more than the others, so Erebor goes back to it.
        for c in coins::COINS.iter().filter(|c| c.device == Device::Gpu) {
            e.hashrates.insert(c.id.into(), 1000.0);
        }
        e.market.coins = coins::COINS
            .iter()
            .map(|c| market(c.id, if c.id == "PRL" { 3e-6 } else { 1e-6 }))
            .collect();
        e.tick();
        assert_eq!(e.coin.id, "PRL");
        // Within REEVAL_SECS nothing changes, even if another coin becomes better.
        e.market
            .coins
            .iter_mut()
            .find(|m| m.id == "ETC")
            .unwrap()
            .coin_per_hash = 9e-6;
        e.tick();
        assert_eq!(e.coin.id, "PRL");
    }

    #[test]
    fn advice_names_the_best_coin_and_payout_for_btc() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, _) = engine(d.path(), false);
        e.has_gpu = true;
        e.gpu_platform = true;
        // Monero address: mines XMR on the CPU, yet the GPU is the way to go.
        let a = e.advice();
        assert_eq!(a.device, Device::Gpu);
        assert!(a.coin.is_none());
        assert_eq!(a.payout, "kryptex");
        for c in coins::COINS {
            e.hashrates.insert(c.id.into(), 1000.0);
        }
        e.market.coins = coins::COINS
            .iter()
            .map(|c| market(c.id, if c.id == "PRL" { 3e-6 } else { 1e-6 }))
            .collect();
        // Kryptex fee higher than unMineable's: BTC wallet on unMineable pays more.
        e.market.coins[0].pool_fee_percent = 3.0;
        let a = e.advice();
        assert_eq!(a.coin.unwrap().id, "PRL");
        assert_eq!(a.payout, "bitcoin");
        assert!(a.btc_per_day.unwrap() > a.current_btc_per_day.unwrap() * 2.0);
        // RVN is Kryptex-only.
        e.market.coins = vec![market("RVN", 9e-6)];
        assert_eq!(e.advice().payout, "kryptex");
        e.has_gpu = false;
        assert_eq!(e.advice().device, Device::Cpu);
    }

    #[test]
    fn manual_coin_choice_is_respected() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, s) = engine(d.path(), false);
        e.has_gpu = true;
        e.gpu_platform = true;
        e.config.srbminer.path = Some("/mock/SRBMiner-MULTI".into());
        let mut c = e.config().clone();
        c.wallet = "krxABC123".into();
        c.coin = "RVN".into();
        e.update_config(c).unwrap();
        e.start(false).unwrap();
        e.coin_since = Some(Instant::now() - Duration::from_secs(EXPLORE_SECS + 5));
        e.tick();
        assert_eq!(s.coins.lock().unwrap().as_slice(), ["RVN"]);
        assert_eq!(e.status().coin.id, "RVN");
        assert!(!e.status().auto_coin);
    }

    #[test]
    fn projection_uses_live_speed_and_deducts_fees() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, _) = engine(d.path(), false);
        e.has_gpu = false;
        assert!(e.status().projection.is_none()); // nothing measured, no market: no guess
        e.market.coins = vec![market("XMR", 1e-6)];
        e.market.btc = Some(BtcPrice {
            eur: 50_000.0,
            usd: 55_000.0,
        });
        e.start(false).unwrap();
        e.stats = Some(MinerStats {
            hashrate_60s: Some(1000.0),
            ..Default::default()
        });
        let p = e.status().projection.unwrap();
        assert!(p.live);
        let xmr = coins::find("XMR").unwrap();
        let expected = coins::btc_per_day(1000.0, &e.market.coins[0], xmr)
            * (1.0 - e.devfee.status().percent / 100.0);
        assert!((p.btc_per_day - expected).abs() < 1e-15);
        assert!((p.value_per_day.unwrap() - expected * 50_000.0).abs() < 1e-9);
        assert!(p.energy_cost_per_day.is_none());
    }

    #[test]
    fn bitcoin_payout_mines_on_unmineable() {
        const BTC: &str = "bc1qqqqsyqcyq5rqwzqfpg9scrgwpugpzysn4v0345";
        let d = tempfile::tempdir().unwrap();
        let (mut e, s) = engine(d.path(), false);
        e.has_gpu = true;
        e.gpu_platform = true;
        e.config.srbminer.path = Some("/mock/SRBMiner-MULTI".into());
        let mut c = e.config().clone();
        c.wallet = BTC.into();
        e.update_config(c.clone()).unwrap();
        // Ravencoin is not on unMineable: never chosen, and refused if picked by hand.
        let rvn = coins::find("RVN").unwrap();
        assert_eq!(e.coin_blocker(rvn), Some("not_on_unmineable"));
        c.coin = "RVN".into();
        e.update_config(c).unwrap();
        assert_eq!(e.config().coin, AUTO_COIN);
        e.start(false).unwrap();
        assert_eq!(s.coins.lock().unwrap().as_slice(), ["PRL"]);
        let t = e.pool_target(Target::User);
        assert_eq!(t.url, "pearlpow.unmineable.com:3333");
        assert_eq!(t.user, format!("BTC:{BTC}.erebor"));
        assert_eq!(t.pass, "x");
        // unMineable's fee replaces Kryptex's in estimates.
        e.market.coins = vec![market("PRL", 1e-6)];
        assert_eq!(
            e.coin_market("PRL").unwrap().pool_fee_percent,
            coins::UNMINEABLE_FEE_PERCENT
        );
        // CPU fallback for BTC payouts is RandomX on unMineable too.
        e.stop("user");
        e.has_gpu = false;
        e.start(false).unwrap();
        assert_eq!(e.pool_target(Target::User).url, "rx.unmineable.com:3333");
    }

    #[test]
    fn coin_wallet_overrides_the_main_payout_for_that_coin() {
        let prl_addr = format!("prl1p{}", "q".repeat(58));
        let d = tempfile::tempdir().unwrap();
        let (mut e, _) = engine(d.path(), false);
        e.has_gpu = true;
        e.gpu_platform = true;
        e.config.srbminer.path = Some("/mock/SRBMiner-MULTI".into());
        // Main payout is a Monero address: GPU coins are blocked…
        let prl = coins::find("PRL").unwrap();
        assert_eq!(e.coin_blocker(prl), Some("kryptex_or_btc_required"));
        // …until the user adds an own Pearl wallet.
        let mut c = e.config().clone();
        c.coin_wallets.insert("PRL".into(), prl_addr.clone());
        c.coin = "PRL".into();
        e.update_config(c).unwrap();
        assert_eq!(e.coin_blocker(prl), None);
        e.start(false).unwrap();
        let t = e.pool_target(Target::User);
        assert_eq!(t.url, "prl.kryptex.network:7048");
        assert_eq!(t.user, format!("{prl_addr}/erebor"));
        // Bad addresses are refused.
        let mut c = e.config().clone();
        c.coin_wallets.insert("ETC".into(), "0xnope".into());
        assert!(e.update_config(c).is_err());
    }

    #[test]
    fn mined_coins_are_totalled_per_coin() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, _) = engine(d.path(), true);
        e.market.coins = vec![market("XMR", 1e-6)];
        e.market.network = Some(NetworkStats {
            difficulty: 1e6,
            block_reward: 1.0,
            height: None,
        });
        e.start(false).unwrap();
        for _ in 0..10 {
            e.last_tick = Some(Instant::now() - Duration::from_secs(2));
            e.tick();
        }
        e.stop("user");
        e.mined_at = None;
        e.refresh_mined();
        let m = &e.status().mined;
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].coin, "XMR");
        // 1000 H/s (mock) × ~20 s × 1e-6 XMR/hash.
        assert!(
            (m[0].amount - 1000.0 * m[0].secs * 1e-6).abs() < 1e-9,
            "{:?}",
            m[0]
        );
    }
}

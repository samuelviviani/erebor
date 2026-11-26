//! Orchestrates miner lifecycle, dev fee switching, sampling, safety rules and jobs.
//! The GUI calls into an `Arc<Mutex<Engine>>`; a background loop calls `tick()` every
//! `TICK` and a slower loop refreshes market data outside the lock.

use crate::coins::{self, BtcPrice, Coin, CoinMarket, Device};
use crate::config::{AppConfig, AUTO_COIN, PROJECT_WORKER};
use crate::devfee::{self, DevFeeManager, DevFeeStatus, Ledger, Target};
use crate::earnings::{self, Earnings, EnergySource, Params};
use crate::energy;
use crate::hardware::{self, HardwareInfo, LiveHardware, Monitor};
use crate::miner::{
    DemoMiner, LaunchSpec, MinerAdapter, MinerInfo, MinerKind, MinerStats, PoolTarget,
};
use crate::notify;
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

const TICK: Duration = Duration::from_secs(2);
/// Blockers an own wallet for the coin removes (the others are about this PC).
const PAYOUT_BLOCKERS: [&str; 4] = [
    "wallet_missing",
    "own_wallet_required",
    "not_on_unmineable",
    "kryptex_or_btc_required",
];
/// Larger gaps (sleep, suspended process) are not counted as mining time.
const MAX_TICK_GAP: f64 = 10.0;
const SAMPLE_SECS: f64 = 10.0;
const HOT_TICKS_TO_STOP: u32 = 3;
const MAX_AUTO_RESTARTS: u32 = 3;
const RETENTION_DAYS: i64 = 90;
const BENCHMARK_SECS: u64 = 90;
/// Automatic choice: how often to re-check profitability, and the gain needed to switch.
const REEVAL_SECS: u64 = 1800;
const SWITCH_GAIN: f64 = 1.10;
/// A profitability change must hold this long before mining waits or resumes.
const ECON_STEADY: Duration = Duration::from_secs(180);
const IDLE_POLL: Duration = Duration::from_secs(5);
/// GPU hashrate under this share of normal, for this long, means a stuck card.
const DROP_SHARE: f64 = 0.7;
const DROP_STEADY: Duration = Duration::from_secs(300);
/// No pool connection for this long: switch to the backup pool.
const FAILOVER_AFTER: Duration = Duration::from_secs(180);
const PRIMARY_RETRY: Duration = Duration::from_secs(900);
const SENSOR_STALE: Duration = Duration::from_secs(120);

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
    /// Mining is on but waiting: "in_use" (idle-only mode), "warm" (thermostat) or
    /// "unprofitable".
    pub held: Option<&'static str>,
    /// What a kWh costs for mining right now (bands, solar share, heat credit).
    pub energy_price: Option<f64>,
    /// Watts the meter exports now (negative = importing), when solar is set up.
    pub solar_export_w: Option<f64>,
    pub solar_error: Option<String>,
    /// One entry per `hardware.gpus` card, same order.
    pub gpus: Vec<GpuRow>,
    /// Room temperature from the thermostat sensor, when set up.
    pub room_temp_c: Option<f64>,
    pub room_error: Option<String>,
    /// Seconds without keyboard/mouse input; `None` when the system does not say.
    pub idle_secs: Option<u64>,
    /// Stopped for a while: seconds until mining starts again by itself.
    pub resume_in_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuRow {
    /// This card's share of the miner's hashrate (H/s).
    pub hashrate: Option<f64>,
    /// Typical hashrate of this model on the current coin (public GPU table).
    pub expected: Option<f64>,
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

/// One thing mining a coin takes. `code` names it for the UI ("gpu", "no_gpu",
/// "windows_only", "miner_missing", "own_wallet_required", …).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Requirement {
    /// "hardware", "system", "miner" or "payout".
    pub kind: &'static str,
    /// "ok", "missing" (blocks mining) or "warn" (works, with a catch).
    pub status: &'static str,
    pub code: &'static str,
}

/// One entry of the coin picker.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoinOption {
    pub coin: Coin,
    /// Why it cannot be mined here, if so ("no_gpu", "kryptex_or_btc_required", …).
    pub blocker: Option<&'static str>,
    /// Everything mining it takes on this PC, met or not (hardware, system, miner, payout).
    pub requirements: Vec<Requirement>,
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
            if cfg!(windows) && !Path::new(p).exists() {
                return Err("blocked_by_antivirus".into()); // quarantined after install
            }
            let path = xmrig::validate_binary_path(p, coin.miner.dir_prefix())?;
            if let Some(expected) = &mc.sha256 {
                if xmrig::sha256_file(&path)? != *expected {
                    return Err("miner_binary_changed".into());
                }
            }
            let run = data_dir.join("run");
            Ok(match coin.miner {
                MinerKind::Xmrig => Box::new(xmrig::XmrigMiner::new(path, run, mc.version.clone())),
                kind => Box::new(crate::srbminer::ExternalMiner::new(
                    kind,
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
    /// Expected hashrate of this PC's GPUs per coin id, from the GPU table (H/s).
    estimates: HashMap<String, f64>,
    gpu_names: Vec<String>,
    /// All-time coins mined for the user, refreshed every 30 s (cheap status reads).
    mined: Vec<crate::store::CoinAmount>,
    mined_at: Option<Instant>,
    has_gpu: bool,
    /// Miner updates already announced ("srbminer-3.9.0"), so the notice is not repeated.
    update_notified: std::collections::HashSet<String>,
    /// A pinned GPU miner build exists for this OS/CPU.
    gpu_platform: bool,
    /// Mining is on but waiting for a condition to clear (see `hold_reason`).
    held: Option<&'static str>,
    /// Debounced "mining loses money at the current price", and since when the raw
    /// reading disagrees with it.
    econ_bad: bool,
    econ_flip: Option<Instant>,
    /// Draw measured while mining each coin (W), for decisions while not mining.
    watts: HashMap<String, f64>,
    /// Meter (exported W) and room thermometer (°C), read by `spawn_loops`.
    solar: Reading,
    room: Reading,
    idle: Option<u64>,
    idle_at: Option<Instant>,
    /// Failover: mining on the coin's backup pool, since when, and since when the pool
    /// connection is missing.
    on_backup: bool,
    backup_since: Option<Instant>,
    offline_since: Option<Instant>,
    /// Power-limit search in progress (see `start_tune`).
    tune: Option<Tune>,
    /// Expected hashrate per GPU model and coin (bundled, refreshed with market data).
    gpu_table: coins::GpuTable,
    /// Last push per alert code (rate limit).
    pushed: HashMap<String, Instant>,
    /// "Stop for a while": when to start again, and whether the schedule had started it.
    resume_at: Option<(Instant, bool)>,
    /// Since when the GPU hashrate is below normal, and the last restart it caused.
    low_since: Option<Instant>,
    drop_restart_at: Option<Instant>,
    /// Seconds without input (replaced in tests).
    idle_source: fn() -> Option<u64>,
    /// Exclusive lock on the data dir: one engine (app or node) per computer.
    _lock: std::fs::File,
}

/// Efficiency search: stock power first, then lower limits, each measured on the real
/// miner; the limit with the best net result wins.
struct Tune {
    /// Limit before the search, restored if it is cancelled.
    original: Option<u32>,
    /// Limits still to try after the current one (filled after the stock step).
    queue: Vec<u32>,
    current: Option<u32>,
    step_started: Instant,
    hash_sum: f64,
    watt_sum: f64,
    n: u32,
    /// (limit, hashrate, watts) per finished step.
    results: Vec<(Option<u32>, f64, f64)>,
}

/// Lower limits tried, as a share of the stock draw.
const TUNE_SHARES: [f64; 4] = [0.9, 0.8, 0.7, 0.6];
const TUNE_WARM: Duration = Duration::from_secs(60);
const TUNE_MEASURE: Duration = Duration::from_secs(120);

/// Last value from an HTTP sensor and the last error; stale after `SENSOR_STALE`.
#[derive(Default)]
struct Reading {
    last: Option<(f64, Instant)>,
    error: Option<String>,
}

impl Reading {
    fn set(&mut self, r: Result<f64, String>) {
        match r {
            Ok(v) => {
                self.last = Some((v, Instant::now()));
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn fresh(&self) -> Option<f64> {
        self.last
            .filter(|(_, at)| at.elapsed() < SENSOR_STALE)
            .map(|(v, _)| v)
    }
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
        // Before touching the miner pid: another engine (agent or app) may own it.
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
            fee_percent(&config),
            fee_enabled(&config),
            load_ledger(&store),
        );
        let mut monitor = Monitor::new();
        let live = monitor.live();
        let hw = monitor.detect();
        let has_gpu = has_mining_gpu(&hw);
        let gpu_names: Vec<String> = hw.gpus.into_iter().map(|g| g.name).collect();
        let names: Vec<&str> = gpu_names.iter().map(String::as_str).collect();
        let gpu_table = coins::bundled_gpu_table();
        let estimates = coins::estimate(&names, &gpu_table);
        let hashrates = load_hashrates(&store);
        let watts = store
            .kv_get("watts")
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
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
            estimates,
            gpu_names,
            mined: Vec::new(),
            mined_at: None,
            has_gpu,
            update_notified: Default::default(),
            gpu_platform: install::manifest_for(MinerKind::Srbminer)
                .asset_for_platform()
                .is_ok(),
            held: None,
            econ_bad: false,
            econ_flip: None,
            watts,
            solar: Reading::default(),
            room: Reading::default(),
            idle: None,
            idle_at: None,
            idle_source: hardware::idle_secs,
            low_since: None,
            drop_restart_at: None,
            resume_at: None,
            pushed: HashMap::new(),
            gpu_table,
            tune: None,
            on_backup: false,
            backup_since: None,
            offline_since: None,
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
        // Order matters: payout reasons first (an own wallet fixes them), then hardware.
        let own = self.coin_wallet(coin).is_some();
        let payout = self.payout();
        if !own && self.config.wallet.is_empty() && !self.config.demo_mode {
            return Some("wallet_missing");
        }
        let unmineable = wallet::via_unmineable(payout);
        let via_unmineable = unmineable && coin.unmineable_host.is_some();
        // Not on Kryptex: only an own wallet (or unMineable with a BTC payout) reaches it.
        if !own && coin.host.is_none() && !via_unmineable {
            return Some("own_wallet_required");
        }
        if !own && unmineable && coin.unmineable_host.is_none() {
            return Some("not_on_unmineable");
        }
        if let Some(b) = self.hardware_blocker(coin) {
            return Some(b);
        }
        if coin.own_endpoint() && !own && payout != Some(AddressKind::Kryptex) && !unmineable {
            return Some("kryptex_or_btc_required");
        }
        None
    }

    /// Every requirement of `coin` on this PC, so the user sees all that is missing (the
    /// blocker is only the first). Same rules as `coin_blocker` and `hardware_blocker`.
    fn requirements(&self, coin: &Coin) -> Vec<Requirement> {
        let req = |kind, ok: bool, code_ok, code_missing| Requirement {
            kind,
            status: if ok { "ok" } else { "missing" },
            code: if ok { code_ok } else { code_missing },
        };
        let demo = self.config.demo_mode;
        let mut out = Vec::new();
        if coin.device == Device::Gpu {
            out.push(if demo {
                req("hardware", false, "gpu", "not_in_demo")
            } else {
                req("hardware", self.has_gpu, "gpu", "no_gpu")
            });
            out.push(req(
                "system",
                demo || self.gpu_platform,
                "system",
                "platform_unsupported",
            ));
        } else {
            out.push(Requirement {
                kind: "hardware",
                status: if coin.asic { "warn" } else { "ok" },
                code: if coin.asic { "asic" } else { "cpu" },
            });
            let built = install::manifest_for(coin.miner)
                .asset_for_platform()
                .is_ok()
                || self.config.miner_for(coin.miner).path.is_some();
            let missing = if coin.miner == MinerKind::Cpuminer {
                "windows_only"
            } else {
                "platform_unsupported"
            };
            out.push(req("system", demo || built, "system", missing));
        }
        let installed = demo || self.config.miner_for(coin.miner).path.is_some();
        out.push(Requirement {
            kind: "miner",
            // Installed in one click from the coin's sheet: not a real blocker.
            status: if installed { "ok" } else { "warn" },
            code: if installed {
                "miner_installed"
            } else {
                "miner_missing"
            },
        });
        let payout = match self.coin_blocker(coin) {
            Some(b) if PAYOUT_BLOCKERS.contains(&b) => req("payout", false, "", b),
            _ if self.coin_wallet(coin).is_some() => req("payout", true, "own_wallet", ""),
            _ => req("payout", true, "main_payout", ""),
        };
        out.push(payout);
        out
    }

    /// Why this PC cannot mine `coin`, whatever the payout.
    fn hardware_blocker(&self, coin: &Coin) -> Option<&'static str> {
        if coin.device != Device::Gpu {
            // No official build here (cpuminer-opt outside Windows) and none picked by hand.
            let no_build = install::manifest_for(coin.miner)
                .asset_for_platform()
                .is_err()
                && self.config.miner_for(coin.miner).path.is_none();
            (no_build && !self.config.demo_mode).then_some("platform_unsupported")
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
    fn advice(&self) -> Advice {
        let gpu = coins::COINS
            .iter()
            .any(|c| c.device == Device::Gpu && self.hardware_blocker(c).is_none());
        let device = if gpu { Device::Gpu } else { Device::Cpu };
        let mut best: Option<(&Coin, &'static str, f64)> = None;
        for c in coins::COINS.iter().filter(|c| c.device == device) {
            let h = self.hashrate(c.id);
            let m = self.market.coins.iter().find(|m| m.id == c.id);
            let (Some(h), Some(m)) = (h, m) else { continue };
            // unMineable first: on a tie the account-free BTC payout wins.
            let um = c.unmineable_host.map(|_| CoinMarket {
                pool_fee_percent: coins::UNMINEABLE_FEE_PERCENT,
                ..m.clone()
            });
            let ways = um
                .map(|m| ("bitcoin", m))
                .into_iter()
                .chain([("kryptex", m.clone())]);
            for (way, m) in ways {
                let v = coins::btc_per_day(h, &m, c);
                if best.is_none_or(|b| v > b.2) {
                    best = Some((c, way, v));
                }
            }
        }
        let now = self.wanted_coin();
        Advice {
            device,
            coin: best.map(|b| b.0.clone()),
            payout: best.map_or("bitcoin", |b| b.1),
            btc_per_day: best.map(|b| b.2),
            current_btc_per_day: self
                .hashrate(now.id)
                .zip(self.coin_market(now.id))
                .map(|(h, m)| coins::btc_per_day(h, &m, now)),
        }
    }

    /// Coins the automatic choice may use. A usable GPU takes priority over the CPU.
    fn candidates(&self) -> Vec<&'static Coin> {
        let usable = |d: Device| -> Vec<&'static Coin> {
            coins::COINS
                .iter()
                // ASIC coins earn almost nothing on a PC: never picked automatically.
                .filter(|c| c.device == d && !c.asic && self.coin_blocker(c).is_none())
                .filter(|c| self.config.miner_for(c.miner).path.is_some())
                .filter(|c| !self.config.auto_exclude.contains(c.id))
                .collect()
        };
        let gpu = usable(Device::Gpu);
        if !gpu.is_empty() {
            return gpu;
        }
        match usable(Device::Cpu) {
            cpu if cpu.is_empty() => vec![xmr()],
            cpu => cpu,
        }
    }

    /// Hashrate of `coin` on this PC: measured while mining it, else the GPU table's.
    /// A speed measured on another coin with the same algorithm counts too (RandomX coins,
    /// KawPow coins…): same work, same speed.
    fn hashrate(&self, id: &str) -> Option<f64> {
        let same_algo = || {
            let c = coins::find(id)?;
            coins::COINS
                .iter()
                .filter(|o| o.algo == c.algo && o.device == c.device)
                .find_map(|o| self.hashrates.get(o.id))
        };
        self.hashrates
            .get(id)
            .or_else(same_algo)
            .or(self.estimates.get(id))
            .copied()
    }

    /// Most profitable candidate by today's market data and this PC's (expected)
    /// hashrate; the first candidate when nothing can be compared.
    fn pick_auto(&self) -> &'static Coin {
        let c = self.candidates();
        let speeds = c
            .iter()
            .filter_map(|c| Some((c.id.to_string(), self.hashrate(c.id)?)))
            .collect();
        coins::best(&c, &speeds, &self.markets())
            .map(|(c, _)| c)
            .unwrap_or(c[0])
    }

    /// New GPU table downloaded from the repository.
    fn set_gpu_table(&mut self, table: &coins::GpuTable) {
        let names: Vec<&str> = self.gpu_names.iter().map(String::as_str).collect();
        self.estimates = coins::estimate(&names, table);
        self.gpu_table = table.clone();
    }

    /// Per card while mining a GPU coin: measured hashrate (when the miner's device list
    /// lines up with the detected cards) and what that model usually does on this coin.
    fn gpu_rows(&self) -> Vec<GpuRow> {
        let coin = self.coin;
        let mining = self.is_active() && coin.device == Device::Gpu;
        let measured = self
            .stats
            .as_ref()
            .map(|s| s.gpu_hashrates.as_slice())
            .filter(|h| mining && h.len() == self.live.gpus.len())
            .unwrap_or_default();
        self.live
            .gpus
            .iter()
            .enumerate()
            .map(|(i, g)| GpuRow {
                hashrate: measured.get(i).copied(),
                expected: coins::estimate(&[g.name.as_str()], &self.gpu_table)
                    .get(coin.id)
                    .copied()
                    .filter(|_| coin.device == Device::Gpu),
            })
            .collect()
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
        let btc = wallet::via_unmineable(self.payout());
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
        let hashrate = live.or_else(|| self.hashrate(coin.id))?;
        let market = self.coin_market(coin.id)?;
        let user_share = 1.0 - self.devfee.status().percent / 100.0;
        let btc_per_day = coins::btc_per_day(hashrate, &market, coin) * user_share;
        let value_per_day = self.btc_price().map(|p| btc_per_day * p);
        // Idle readings say nothing about mining draw: live power only while mining,
        // otherwise what this coin drew last time.
        let mining = live.is_some();
        let watts = self.mining_watts(coin, mining);
        let energy_cost_per_day = watts
            .zip(self.energy_price(coin, mining))
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
                let hashrate = self.hashrate(c.id);
                let btc_per_day = hashrate
                    .zip(self.coin_market(c.id))
                    .map(|(h, m)| coins::btc_per_day(h, &m, c));
                CoinOption {
                    coin: c.clone(),
                    blocker: self.coin_blocker(c),
                    requirements: self.requirements(c),
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
    /// GPU watchdog: a card that hangs often keeps the miner alive at a fraction of its
    /// speed. Below `DROP_SHARE` of the learned hashrate for `DROP_STEADY`, the miner is
    /// restarted once; if the speed is still low afterwards it becomes the new normal.
    /// Returns true while the reading counts as a drop.
    fn hashrate_dropped(&mut self, h: f64) -> bool {
        let normal = self.hashrates.get(self.coin.id).copied();
        let low = self.coin.device == Device::Gpu && normal.is_some_and(|n| h < n * DROP_SHARE);
        if !low {
            self.low_since = None;
            return false;
        }
        let since = *self.low_since.get_or_insert_with(Instant::now);
        if since.elapsed() < DROP_STEADY {
            return true;
        }
        self.low_since = None;
        let detail = format!("{:.0}% of normal", h / normal.unwrap_or(h) * 100.0);
        if self
            .drop_restart_at
            .is_some_and(|t| t.elapsed() < Duration::from_secs(1800))
        {
            // Restarting did not help: the hardware or settings changed. Accept it.
            self.hashrates.insert(self.coin.id.to_string(), h);
            self.alert(Level::Warning, "hashrate_low", Some(detail));
            return false;
        }
        self.drop_restart_at = Some(Instant::now());
        self.alert(Level::Warning, "hashrate_dropped", Some(detail));
        let by_schedule = self.started_by_schedule;
        self.end_session("hashrate_drop");
        self.state = RunState::Stopped;
        if let Err(e) = self.start(by_schedule) {
            self.state = RunState::Error;
            self.error = Some(e.clone());
            self.alert(Level::Error, "restart_failed", Some(e));
        }
        true
    }

    fn learn_hashrate(&mut self) {
        // While the efficiency search runs, low limits are on purpose: neither learn
        // them nor treat them as a stuck card.
        if self.tune.is_some()
            || self.state != RunState::Running
            || self.devfee.active() != Target::User
        {
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
        if self.hashrate_dropped(h) {
            return; // a stuck card must not become the new normal
        }
        let v = self.hashrates.entry(self.coin.id.to_string()).or_insert(h);
        *v = *v * 0.9 + h * 0.1;
        if let Some(w) = self.power(self.coin.device) {
            let v = self.watts.entry(self.coin.id.to_string()).or_insert(w);
            *v = *v * 0.9 + w * 0.1;
        }
        if self
            .hashrates_saved
            .is_none_or(|t| t.elapsed() >= Duration::from_secs(60))
        {
            self.hashrates_saved = Some(Instant::now());
            if let Ok(s) = serde_json::to_string(&self.hashrates) {
                let _ = self.store.kv_set("hashrates", &s);
            }
            if let Ok(s) = serde_json::to_string(&self.watts) {
                let _ = self.store.kv_set("watts", &s);
            }
        }
    }

    /// Automatic choice: every REEVAL_SECS, move to the most profitable coin when it beats
    /// the current one by SWITCH_GAIN (prices and difficulty change during the day).
    fn auto_switch(&mut self) {
        if self.config.coin != AUTO_COIN
            || self.state != RunState::Running
            || self.devfee.active() != Target::User
            || self.coin_since.is_none()
        {
            return;
        }
        let want = self.pick_auto();
        if want.id == self.coin.id
            || self
                .last_eval
                .is_some_and(|t| t.elapsed() < Duration::from_secs(REEVAL_SECS))
        {
            return;
        }
        self.last_eval = Some(Instant::now());
        let value = |c: &Coin| {
            self.hashrate(c.id)
                .zip(self.coin_market(c.id))
                .map(|(h, m)| coins::btc_per_day(h, &m, c))
        };
        let go = match (value(want), value(self.coin)) {
            (Some(w), Some(cur)) => w > cur * SWITCH_GAIN,
            (Some(_), None) => true,
            _ => false,
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
        if level != Level::Info && !self.config.demo_mode {
            self.push(code, detail.as_deref());
        }
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

    /// Push a problem to the user's phone, at most once per code every 10 minutes, on a
    /// thread of its own (a slow server never stalls the engine).
    fn push(&mut self, code: &str, detail: Option<&str>) {
        let n = &self.config.notify;
        if n.telegram_token.is_empty() || n.telegram_chat.is_empty() {
            return;
        }
        if self
            .pushed
            .get(code)
            .is_some_and(|t| t.elapsed() < Duration::from_secs(600))
        {
            return;
        }
        self.pushed.insert(code.to_string(), Instant::now());
        let (n, title) = (n.clone(), format!("Erebor · {}", self.worker_label()));
        let body = notify::text(&self.config.language, code, detail);
        std::thread::spawn(move || notify::send(&n, &title, &body));
    }

    /// This PC as the pool sees it.
    fn worker_label(&self) -> String {
        if self.config.pool.worker.is_empty() {
            crate::config::default_worker()
        } else {
            self.config.pool.worker.clone()
        }
    }

    /// Channels, title and body of a test message; sent by the caller outside the lock.
    pub fn test_notify(&self) -> Result<(crate::config::Notify, String, String), String> {
        let n = &self.config.notify;
        if n.telegram_token.is_empty() || n.telegram_chat.is_empty() {
            return Err("notify_not_configured".into());
        }
        let title = format!("Erebor · {}", self.worker_label());
        Ok((
            n.clone(),
            title,
            notify::text(&self.config.language, "test", None),
        ))
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
        new.lolminer = self.config.lolminer.clone();
        new.cpuminer = self.config.cpuminer.clone();
        new.defender_allowed = self.config.defender_allowed;
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
            self.devfee = DevFeeManager::new(fee_percent(&new), false, load_ledger(&self.store));
        }
        self.config = new;
        self.devfee.set_enabled(fee_enabled(&self.config));
        self.devfee.set_percent(fee_percent(&self.config));
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
        let p = xmrig::validate_binary_path(path, kind.dir_prefix())?;
        let version = match kind {
            MinerKind::Xmrig => Some(xmrig::detect_version(&p)?),
            _ => None,
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

    /// Samples of the last `range_secs` as CSV.
    pub fn export_csv(&self, range_secs: i64) -> Result<String, String> {
        let to = now_unix() + 1;
        let from = to - range_secs.clamp(60, 366 * 86_400);
        self.store.export_csv(from, to).map_err(|e| e.to_string())
    }

    /// Same CSV written to the user's Downloads folder (data folder as fallback).
    pub fn save_csv(&self, range_secs: i64) -> Result<String, String> {
        let csv = self.export_csv(range_secs)?;
        let home =
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
        let dir = home
            .map(|h| h.join("Downloads"))
            .filter(|d| d.is_dir())
            .unwrap_or_else(|| self.data_dir.clone());
        let name = format!(
            "erebor-{}.csv",
            chrono::Local::now().format("%Y-%m-%d-%H%M")
        );
        let path = dir.join(name);
        std::fs::write(&path, csv).map_err(|e| e.to_string())?;
        Ok(path.to_string_lossy().into_owned())
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

    fn can_start(&self) -> Result<(), String> {
        let coin = self.wanted_coin();
        if !self.config.demo_mode {
            if self.config.wallet.is_empty() && self.config.coin_wallets.is_empty() {
                return Err("wallet_missing".into());
            }
            if let Some(b) = self.coin_blocker(coin) {
                return Err(b.into());
            }
            if self.config.miner_for(coin.miner).path.is_none() {
                return Err("miner_not_configured".into());
            }
        }
        // The efficiency search restarts the miner itself: only other jobs block a start.
        let other_job = self
            .job
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|j| !j.done && j.kind != "tune");
        if other_job {
            return Err("job_running".into());
        }
        self.config.validate()
    }

    /// Name of this PC on the pool: the user's choice, else the hostname.
    fn worker(&self) -> String {
        match self.config.pool.worker.trim() {
            "" => crate::config::default_worker(),
            w => w.to_string(),
        }
    }

    /// Endpoint of a coin mined to the user's own wallet: the chosen pool and region, or
    /// its backup while failing over. Default: Kryptex with that address.
    fn own_route(&self, coin: &Coin) -> (String, u16, bool, pools::Login) {
        let choice = self
            .config
            .coin_pools
            .get(coin.id)
            .cloned()
            .unwrap_or_default();
        let id = if self.on_backup && !choice.backup.is_empty() {
            choice.backup.as_str()
        } else {
            choice.pool.as_str()
        };
        if id == "custom" {
            return (
                choice.host.clone(),
                choice.port,
                choice.tls,
                pools::Login::Dot,
            );
        }
        // "" (or an unknown id): the first listed pool, Kryptex when the coin is there.
        let list = pools::coin_pools(coin);
        let Some(p) = list.iter().find(|p| p.pool == id).or(list.first()) else {
            return (String::new(), 0, false, pools::Login::Dot);
        };
        let host = p
            .hosts
            .iter()
            .find(|(r, _)| *r == choice.region)
            .or(p.hosts.first())
            .map(|(_, h)| h.clone())
            .unwrap_or_default();
        (host, p.port, false, p.login)
    }

    /// Switch to the backup pool after `FAILOVER_AFTER` without a pool connection, and
    /// try the main pool again every `PRIMARY_RETRY` while on the backup.
    fn check_pool_failover(&mut self) {
        let has_backup = self.coin_wallet(self.coin).is_some()
            && self
                .config
                .coin_pools
                .get(self.coin.id)
                .is_some_and(|c| !c.backup.is_empty());
        if !has_backup || self.state != RunState::Running || self.devfee.active() != Target::User {
            self.offline_since = None;
            return;
        }
        if self.on_backup {
            if self
                .backup_since
                .is_some_and(|t| t.elapsed() >= PRIMARY_RETRY)
            {
                self.on_backup = false;
                self.alert(Level::Info, "pool_primary_retry", None);
                self.restart_miner("pool_primary_retry");
            }
            return;
        }
        if self.stats.as_ref().is_some_and(|s| s.connected) {
            self.offline_since = None;
            return;
        }
        let since = *self.offline_since.get_or_insert_with(Instant::now);
        if since.elapsed() >= FAILOVER_AFTER {
            self.offline_since = None;
            self.on_backup = true;
            self.backup_since = Some(Instant::now());
            let backup = self.config.coin_pools[self.coin.id].backup.clone();
            self.alert(Level::Warning, "pool_failover", Some(backup));
            self.restart_miner("pool_failover");
        }
    }

    /// Stop and start the miner with the current settings (pool, limits).
    fn restart_miner(&mut self, reason: &str) {
        let by_schedule = self.started_by_schedule;
        self.end_session(reason);
        self.state = RunState::Stopped;
        if let Err(e) = self.start(by_schedule) {
            self.state = RunState::Error;
            self.error = Some(e.clone());
            self.alert(Level::Error, "restart_failed", Some(e));
        }
    }

    /// Where Erebor's share goes, whatever the user's payout: the project's BTC address on
    /// unMineable when the build has one and unMineable mines this coin, else the
    /// project's Kryptex account, else unMineable through another coin of the same
    /// algorithm (the miner keeps running the same algorithm, only the pool changes).
    /// `None`: no project destination for this coin (no fee).
    fn project_target(&self, coin: &Coin) -> Option<PoolTarget> {
        let demo = self.config.demo_mode;
        let btc = devfee::build_btc_wallet().or(demo.then_some("DEMO-PROJECT-BTC"));
        let unmineable = |host: &str, addr: &str| {
            let (user, pass) = login_unmineable(&format!("BTC:{addr}"), PROJECT_WORKER, None);
            PoolTarget {
                url: format!("{host}:{}", coins::UNMINEABLE_PORT),
                tls: false,
                user,
                pass,
            }
        };
        if let (Some(addr), Some(host)) = (btc, coin.unmineable_host) {
            return Some(unmineable(host, addr));
        }
        let krx = devfee::build_wallet()
            .filter(|w| wallet::is_kryptex_id(w))
            .or(demo.then_some("DEMO-PROJECT-WALLET"));
        if let (Some(krx), Some(host)) = (krx, coin.host) {
            let (user, pass) = login(krx, PROJECT_WORKER);
            return Some(PoolTarget {
                url: format!("{host}:{}", coin.port),
                tls: false,
                user,
                pass,
            });
        }
        let host = coins::unmineable_host_for_algo(coin.algo)?;
        Some(unmineable(host, btc?))
    }

    fn pool_target(&self, target: Target) -> PoolTarget {
        let c = &self.config;
        let coin = self.coin;
        if target == Target::Project {
            if let Some(t) = self.project_target(coin) {
                return t;
            }
        }
        // Own wallet for this coin: the chosen pool (Kryptex by default), paid in the coin.
        if let Some(addr) = self.coin_wallet(coin) {
            let w: String = self
                .worker()
                .chars()
                .filter(char::is_ascii_alphanumeric)
                .collect();
            let (host, port, tls, style) = self.own_route(coin);
            let (user, pass) = style.credentials(addr, &w);
            return PoolTarget {
                url: format!("{host}:{port}"),
                tls,
                user,
                pass,
            };
        }
        // The user's main payout (project time was routed above by `project_target`).
        let user_worker = self.worker();
        let worker = user_worker.as_str();
        let wallet = if c.demo_mode && c.wallet.is_empty() {
            "DEMO-WALLET"
        } else {
            c.wallet.as_str()
        };
        // Where the work goes depends on how that wallet is paid:
        // BTC address → unMineable; Kryptex ID → Kryptex (GPU coin endpoint, or the
        // configured pool for XMR when it is a Kryptex host); Monero → configured pool.
        let (host, port, tls) = match wallet::payout_kind(wallet) {
            // `coin_blocker` keeps coins with no route here from starting.
            Some(AddressKind::Bitcoin | AddressKind::Converted) => (
                coin.unmineable_host.or(coin.host).unwrap_or_default(),
                coins::UNMINEABLE_PORT,
                false,
            ),
            _ if coin.own_endpoint() => (coin.host.unwrap_or_default(), coin.port, false),
            Some(AddressKind::Kryptex) if !pools::is_kryptex_host(&c.pool.host) => {
                match pools::find("kryptex") {
                    Some(k) => (k.host, k.port, k.tls),
                    None => (coin.host.unwrap_or_default(), coin.port, false),
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
            gpu_power_limit_w: self.config.performance.gpu_power_limit_w,
            gpu_percent: self.config.performance.gpu_percent(),
        }
    }

    /// Start mining. `by_schedule` marks schedule-initiated runs.
    pub fn start(&mut self, by_schedule: bool) -> Result<(), String> {
        if self.is_active() {
            return Ok(());
        }
        self.can_start()?;
        self.resume_at = None;
        self.update_holds(true);
        if let Some(r) = self.hold_reason() {
            // On, but waiting: `apply_holds` starts the miner when the reason clears.
            self.held = Some(r);
            self.started_by_schedule = by_schedule;
            return Ok(());
        }
        self.held = None;
        let coin = self.wanted_coin();
        let mut miner = (self.factory)(&self.config, coin, &self.data_dir, self.monitor.threads())?;
        self.coin = coin;
        // Project time on a GPU coin needs a Kryptex project account.
        // Project time needs a project wallet that works on the user's pool.
        let fee_ok = self.project_target(coin).is_some();
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
        let was_active = self.is_active() || self.held.is_some();
        self.held = None;
        self.resume_at = None;
        self.on_backup = false;
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

    /// Start the efficiency search while mining a GPU coin. Takes about 3 minutes per step
    /// (5 steps); mining goes on throughout, at the limit being measured.
    pub fn start_tune(&mut self) -> Result<(), String> {
        if self.job_running() {
            return Err("job_running".into());
        }
        if self.state != RunState::Running || self.coin.device != Device::Gpu {
            return Err("tune_needs_gpu_mining".into());
        }
        if self.power(Device::Gpu).is_none() {
            return Err("tune_needs_power_reading".into());
        }
        self.cancel_job.store(false, Ordering::Relaxed);
        let original = self.config.performance.gpu_power_limit_w;
        self.tune = Some(Tune {
            original,
            queue: Vec::new(),
            current: None,
            step_started: Instant::now(),
            hash_sum: 0.0,
            watt_sum: 0.0,
            n: 0,
            results: Vec::new(),
        });
        self.set_tune_job(0.0, None, None);
        if original.is_some() {
            self.apply_power_limit(None); // the first step measures stock power
        }
        Ok(())
    }

    fn set_tune_job(&self, progress: f64, value: Option<f64>, error: Option<String>) {
        set_job(
            &self.job,
            JobStatus {
                kind: "tune",
                progress,
                done: progress >= 1.0 || error.is_some(),
                error,
                value,
            },
        );
    }

    /// New limit, applied by restarting the miner (limits are launch arguments).
    fn apply_power_limit(&mut self, w: Option<u32>) {
        self.config.performance.gpu_power_limit_w = w;
        self.restart_miner("power_limit");
    }

    fn tune_tick(&mut self) {
        let Some(t) = self.tune.as_mut() else { return };
        let total = 1 + TUNE_SHARES.len();
        let cancelled = self.cancel_job.load(Ordering::Relaxed);
        if cancelled || self.state != RunState::Running || self.coin.device != Device::Gpu {
            let original = t.original;
            self.tune = None;
            self.config.performance.gpu_power_limit_w = original;
            let _ = self.save_config();
            self.set_tune_job(0.0, None, Some("tune_interrupted".into()));
            return;
        }
        let elapsed = t.step_started.elapsed();
        if elapsed < TUNE_WARM {
            return;
        }
        let h = self.stats.as_ref().and_then(|s| s.hashrate_10s);
        let w = self.power(Device::Gpu);
        let Some(t) = self.tune.as_mut() else { return };
        if let (Some(h), Some(w)) = (h, w) {
            t.hash_sum += h;
            t.watt_sum += w;
            t.n += 1;
        }
        if t.n == 0 && elapsed > TUNE_WARM + 2 * TUNE_MEASURE {
            let original = t.original;
            self.tune = None;
            self.apply_power_limit(original);
            self.set_tune_job(0.0, None, Some("tune_needs_power_reading".into()));
            return;
        }
        if elapsed < TUNE_WARM + TUNE_MEASURE || t.n == 0 {
            return;
        }
        let (h, w) = (t.hash_sum / t.n as f64, t.watt_sum / t.n as f64);
        let limit = t.current;
        // A limit the driver ignored (no admin rights): stop rather than guess.
        if limit.is_some_and(|l| w > l as f64 * 1.15) {
            let original = t.original;
            self.tune = None;
            self.apply_power_limit(original);
            let _ = self.save_config();
            self.set_tune_job(0.0, None, Some("power_limit_not_applied".into()));
            return;
        }
        t.results.push((limit, h, w));
        if limit.is_none() {
            t.queue = TUNE_SHARES.iter().map(|s| (w * s).round() as u32).collect();
        }
        let done = t.results.len();
        if let Some(next) = (!t.queue.is_empty()).then(|| t.queue.remove(0)) {
            t.current = Some(next);
            t.step_started = Instant::now();
            (t.hash_sum, t.watt_sum, t.n) = (0.0, 0.0, 0);
            self.set_tune_job(done as f64 / total as f64, Some(next as f64), None);
            self.apply_power_limit(Some(next));
            return;
        }
        let results = std::mem::take(&mut t.results);
        self.tune = None;
        let best = self.best_limit(&results);
        self.apply_power_limit(best);
        let _ = self.save_config();
        let detail = best.map_or("stock".to_string(), |w| format!("{w} W"));
        self.alert(Level::Info, "tune_done", Some(detail));
        self.set_tune_job(1.0, best.map(f64::from), None);
    }

    /// Highest net profit per day at today's price; without a price, the best hashrate
    /// per watt among limits that keep at least 90% of the stock hashrate.
    fn best_limit(&self, results: &[(Option<u32>, f64, f64)]) -> Option<u32> {
        let coin = self.coin;
        let price = self.energy_price(coin, true);
        let per_hash = self.coin_market(coin.id).map(|m| m.btc_per_hash(coin));
        let fee = self.devfee.status().percent;
        let score = |&(_, h, w): &(Option<u32>, f64, f64)| -> Option<f64> {
            let mut p = self.params();
            p.price_per_kwh = Some(price?);
            earnings::daily_profit_btc(h, per_hash?, &p, fee, w)
        };
        if results.iter().all(|r| score(r).is_some()) {
            return results
                .iter()
                .max_by(|a, b| {
                    score(a)
                        .partial_cmp(&score(b))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .and_then(|r| r.0);
        }
        let stock = results.first().map_or(0.0, |r| r.1);
        results
            .iter()
            .filter(|r| r.1 >= stock * 0.9 && r.2 > 0.0)
            .max_by(|a, b| {
                (a.1 / a.2)
                    .partial_cmp(&(b.1 / b.2))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .and_then(|r| r.0)
    }

    /// Before an app update restarts the process: remember whether mining was on, so the
    /// new version picks it up (`take_resume_after_update`).
    pub fn prepare_update(&mut self) {
        if self.is_active() || self.held.is_some() {
            let _ = self.store.kv_set("resume_after_update", "1");
        }
    }

    /// True once after an update that interrupted mining.
    pub fn take_resume_after_update(&self) -> bool {
        let on = self
            .store
            .kv_get("resume_after_update")
            .ok()
            .flatten()
            .as_deref()
            == Some("1");
        if on {
            let _ = self.store.kv_set("resume_after_update", "0");
        }
        on
    }

    /// Stop now and start again by itself after `secs` (gaming, a call, a render…).
    pub fn stop_for(&mut self, secs: u64) {
        if !self.is_active() && self.held.is_none() {
            return; // nothing to come back to
        }
        let by_schedule = self.started_by_schedule;
        self.stop("user");
        self.resume_at = Some((Instant::now() + Duration::from_secs(secs), by_schedule));
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
        if self.resume_at.is_some_and(|(t, _)| now >= t) {
            let (_, by_schedule) = self.resume_at.take().unwrap();
            if let Err(e) = self.start(by_schedule) {
                self.alert(Level::Error, "restart_failed", Some(e));
            }
        }
        self.apply_schedule();
        self.apply_holds();

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
        self.check_pool_failover();
        self.tune_tick();
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
        if let Some(p) = self.power(self.coin.device) {
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
            price_kwh: self.energy_price(self.coin, true),
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
            pool_fee_percent: if self.coin.own_endpoint() || wallet::via_unmineable(self.payout()) {
                self.coin_market(self.coin.id).map(|m| m.pool_fee_percent)
            } else {
                pools::find(&self.config.pool.id).map(|p| p.fee_percent)
            },
            price,
            btc_price: self.btc_price(),
            price_per_kwh: self.config.energy.price_per_kwh,
        }
    }

    /// Measured draw of the hardware doing the mining: the GPUs (summed) for GPU
    /// coins, the CPU package for CPU coins. Never a user guess.
    fn power(&self, device: Device) -> Option<f64> {
        match device {
            Device::Gpu => self
                .live
                .gpus
                .iter()
                .filter_map(|g| g.power_w)
                .fold(None, |a, w| Some(a.unwrap_or(0.0) + w)),
            Device::Cpu => self.live.power_w,
        }
    }

    /// Draw of `coin`'s device: measured live while mining, else the last learned value.
    fn mining_watts(&self, coin: &Coin, mining: bool) -> Option<f64> {
        if mining {
            self.power(coin.device)
        } else {
            self.watts.get(coin.id).copied()
        }
    }

    /// What a kWh costs for mining `coin` right now: the user's band (or base) price,
    /// the share covered by solar surplus at the feed-in price, minus the heat credit.
    fn energy_price(&self, coin: &Coin, mining: bool) -> Option<f64> {
        use chrono::{Datelike, Local, Timelike};
        let e = &self.config.energy;
        let now = Local::now();
        let grid = energy::grid_price(
            e,
            now.weekday().num_days_from_monday() as usize,
            (now.hour() * 60 + now.minute()) as u16,
        );
        let solar = self.solar_export().map(|w| {
            (
                energy::solar_share(w, self.mining_watts(coin, mining), mining),
                e.solar.feed_in_price,
            )
        });
        let credit = if e.heating_months[now.month0() as usize] {
            e.heating_credit_percent
        } else {
            0
        };
        energy::effective_price(grid, solar, credit)
    }

    /// Fresh meter reading (exported W), when solar is set up.
    fn solar_export(&self) -> Option<f64> {
        self.config
            .energy
            .solar
            .enabled
            .then(|| self.solar.fresh())
            .flatten()
    }

    /// Fresh room temperature (°C), when the thermostat is set up.
    fn room_temp(&self) -> Option<f64> {
        self.config
            .energy
            .thermostat
            .enabled
            .then(|| self.room.fresh())
            .flatten()
    }

    pub fn set_solar(&mut self, reading: Result<f64, String>) {
        self.solar.set(reading);
    }

    pub fn set_room(&mut self, reading: Result<f64, String>) {
        self.room.set(reading);
    }

    /// Would mining the coin lose money at today's price? `None`: not enough data,
    /// and nothing changes on a guess.
    fn unprofitable_now(&self) -> Option<bool> {
        let e = &self.config.energy;
        if !e.stop_if_unprofitable {
            return Some(false);
        }
        let mining = self.is_active();
        let coin = if mining {
            self.coin
        } else {
            self.wanted_coin()
        };
        let h = self
            .hashrate(coin.id)
            .or_else(|| self.stats.as_ref().and_then(|s| s.hashrate_60s))
            .filter(|h| *h > 0.0)?;
        let w = self.mining_watts(coin, mining)?;
        let mut params = self.params();
        params.price_per_kwh = Some(self.energy_price(coin, mining)?);
        let fee = self.devfee.status().percent;
        let profit = if coin.id == "XMR" {
            let ph = self
                .market
                .network
                .as_ref()
                .and_then(|n| n.xmr_per_hash())?;
            earnings::daily_profit(h, ph, &params, fee, w)
        } else {
            let ph = self.coin_market(coin.id).map(|m| m.btc_per_hash(coin))?;
            earnings::daily_profit_btc(h, ph, &params, fee, w)
        }?;
        Some(profit < e.min_profit_per_day)
    }

    /// Refresh the inputs of the hold decision: idle time and profitability, debounced
    /// unless `fresh` (a start decides on the current reading).
    fn update_holds(&mut self, fresh: bool) {
        if self.config.performance.idle_minutes.is_some() {
            if fresh || self.idle_at.is_none_or(|t| t.elapsed() >= IDLE_POLL) {
                self.idle_at = Some(Instant::now());
                self.idle = (self.idle_source)();
            }
        } else {
            self.idle = None;
        }
        match self.unprofitable_now() {
            Some(bad) if bad != self.econ_bad => {
                let since = *self.econ_flip.get_or_insert_with(Instant::now);
                // Prices move by the hour and solar by the minute: act on a steady change.
                if fresh
                    || !self.config.energy.stop_if_unprofitable
                    || since.elapsed() >= ECON_STEADY
                {
                    self.econ_bad = bad;
                    self.econ_flip = None;
                }
            }
            _ => self.econ_flip = None,
        }
    }

    /// Why mining should wait right now, if anything.
    fn hold_reason(&self) -> Option<&'static str> {
        let in_use = self
            .config
            .performance
            .idle_minutes
            .zip(self.idle)
            .is_some_and(|(m, idle)| idle < u64::from(m) * 60);
        // Thermostat with hysteresis: wait at `off_above`, start again below `on_below`.
        // Without a fresh reading nothing changes.
        let th = &self.config.energy.thermostat;
        let was_warm = self.held == Some("warm");
        let warm = match self.room_temp() {
            Some(t) if was_warm => t > th.on_below_c,
            Some(t) => t >= th.off_above_c,
            None => was_warm && th.enabled,
        };
        if in_use {
            Some("in_use")
        } else if warm {
            Some("warm")
        } else if self.econ_bad {
            Some("unprofitable")
        } else {
            None
        }
    }

    /// Put mining on hold when a reason appears, resume it when every reason clears.
    fn apply_holds(&mut self) {
        if !self.is_active() && self.held.is_none() {
            return;
        }
        self.update_holds(false);
        match (self.held, self.hold_reason()) {
            (None, Some(r)) if self.state == RunState::Running => {
                self.end_session(r);
                self.state = RunState::Stopped;
                self.held = Some(r);
                let code = match r {
                    "in_use" => "held_in_use",
                    "warm" => "held_warm",
                    _ => "held_unprofitable",
                };
                self.alert(Level::Info, code, None);
            }
            (Some(_), Some(r)) => self.held = Some(r),
            (Some(_), None) => {
                self.held = None;
                match self.start(self.started_by_schedule) {
                    Ok(()) => self.alert(Level::Info, "mining_resumed", None),
                    Err(e) => {
                        self.state = RunState::Error;
                        self.error = Some(e.clone());
                        self.alert(Level::Error, "restart_failed", Some(e));
                    }
                }
            }
            _ => {}
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
                && self.held.is_none()
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
            if (self.is_active() || self.held.is_some()) && self.started_by_schedule {
                self.stop("schedule");
            }
        }
    }

    // ---------- market data ----------

    /// Network difficulty is global: use the current pool's API, else any pool that has one.
    fn market_api_base(&self) -> &'static str {
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

    fn set_market(
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
        let device = if self.is_active() {
            self.coin.device
        } else {
            self.wanted_coin().device
        };
        let power_w = self.power(device);
        let power_source = if power_w.is_some() {
            EnergySource::Measured
        } else {
            EnergySource::Unavailable
        };
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
            held: self.held,
            energy_price: {
                let coin = if self.is_active() {
                    self.coin
                } else {
                    self.wanted_coin()
                };
                self.energy_price(coin, self.is_active())
            },
            solar_export_w: self.solar_export(),
            solar_error: self
                .config
                .energy
                .solar
                .enabled
                .then(|| self.solar.error.clone())
                .flatten(),
            gpus: self.gpu_rows(),
            room_temp_c: self.room_temp(),
            room_error: self
                .config
                .energy
                .thermostat
                .enabled
                .then(|| self.room.error.clone())
                .flatten(),
            idle_secs: self.idle,
            resume_in_secs: self
                .resume_at
                .map(|(t, _)| t.saturating_duration_since(Instant::now()).as_secs()),
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

    /// Exclude the miners folder from Windows Defender, after the user's consent in the UI.
    /// Called without the engine lock held: it waits for the UAC prompt.
    pub fn allow_in_defender(engine: &Arc<Mutex<Engine>>) -> Result<(), String> {
        let dir = engine.lock().unwrap().data_dir.join("miners");
        install::allow_in_defender(&dir)?;
        let mut e = engine.lock().unwrap();
        e.config.defender_allowed = true;
        e.save_config()
    }

    pub fn detect_hardware(&mut self) -> HardwareInfo {
        self.monitor.detect()
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
/// unMineable: payout coin, address and worker in the user field.
/// unMineable: `COIN:address.worker`, then `#referral` when given.
fn login_unmineable(coin_addr: &str, worker: &str, referral: Option<&str>) -> (String, String) {
    let w: String = worker.chars().filter(char::is_ascii_alphanumeric).collect();
    let w = if w.is_empty() {
        "erebor".to_string()
    } else {
        w
    };
    let r = referral.map(|r| format!("#{r}")).unwrap_or_default();
    (format!("{coin_addr}.{w}{r}"), "x".into())
}

fn login(wallet: &str, worker: &str) -> (String, String) {
    // The user's unMineable payout carries Erebor's referral (lower fee for the user).
    let referral = Some(devfee::UNMINEABLE_REFERRAL);
    match wallet::payout_kind(wallet) {
        Some(AddressKind::Bitcoin) => {
            return login_unmineable(&format!("BTC:{wallet}"), worker, referral)
        }
        Some(AddressKind::Converted) => {
            let (coin, addr) = wallet.split_once(':').unwrap_or_default();
            let coin_addr = format!("{}:{addr}", coin.to_ascii_uppercase());
            return login_unmineable(&coin_addr, worker, referral);
        }
        _ => {}
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

/// Erebor's fee plus the donation the user chose (0 unless they turned it on).
fn fee_percent(c: &AppConfig) -> f64 {
    devfee::DEFAULT_FEE_PERCENT + c.donation_percent.clamp(0.0, devfee::MAX_DONATION_PERCENT)
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
        if MinerKind::ALL.iter().any(|k| name.contains(k.dir_prefix())) {
            p.kill();
        }
    }
    let _ = std::fs::remove_file(pid_file);
}

// ---------- background jobs (run without holding the engine lock) ----------

fn set_job(job: &Arc<Mutex<Option<JobStatus>>>, s: JobStatus) {
    *job.lock().unwrap() = Some(s);
}

/// Miners Erebor installed that are older than the build pinned in this app version:
/// update them in the background (or only announce it, when automatic updates are off).
/// The new build goes to its own folder, so a running miner is untouched; it is used from
/// the next start. Binaries the user picked by hand are never replaced.
fn update_miners(engine: &Arc<Mutex<Engine>>) {
    for kind in MinerKind::ALL {
        let pinned = install::manifest_for(kind);
        let dir = {
            let mut e = engine.lock().unwrap();
            let m = e.config.miner_for(kind);
            let outdated = m.verified
                && m.path.is_some()
                && m.version.as_deref() != Some(pinned.version.as_str());
            if !outdated || e.config.demo_mode || pinned.asset_for_platform().is_err() {
                continue;
            }
            let key = format!("{}-{}", kind.dir_prefix(), pinned.version);
            if !e.config.auto_update_miners {
                if e.update_notified.insert(key) {
                    let what = format!("{} {}", kind.label(), pinned.version);
                    e.alert(Level::Info, "miner_update_available", Some(what));
                }
                continue;
            }
            e.data_dir.join("miners")
        };
        let res = install::install_pinned(kind, &dir, &|_, _| {});
        let mut e = engine.lock().unwrap();
        match res {
            Ok(inst) => {
                *e.config.miner_for_mut(kind) = crate::config::MinerConfig {
                    path: Some(inst.path.to_string_lossy().into_owned()),
                    version: Some(inst.version.clone()),
                    sha256: Some(inst.sha256),
                    verified: true,
                };
                let saved = e.save_config();
                let what = format!("{} {}", kind.label(), inst.version);
                match saved {
                    Ok(()) => e.alert(Level::Info, "miner_updated", Some(what)),
                    Err(err) => e.alert(Level::Warning, "miner_update_failed", Some(err)),
                }
            }
            Err(err) => {
                // Retried at the next market refresh; said once per version.
                let key = format!("failed-{}-{}", kind.dir_prefix(), pinned.version);
                if e.update_notified.insert(key) {
                    e.alert(Level::Warning, "miner_update_failed", Some(err));
                }
            }
        }
    }
}

/// Download, verify and install the pinned release of `kind`.
pub fn spawn_install(engine: Arc<Mutex<Engine>>, kind: MinerKind) -> Result<(), String> {
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
        let res = install::install_pinned(kind, &miners_dir, &move |got, total| {
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
        let binary = xmrig::validate_binary_path(&path, "xmrig")?;
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
    // The meter is read outside the engine lock: a slow device never stalls the UI.
    let e3 = engine.clone();
    std::thread::spawn(move || loop {
        let (solar, room) = {
            let e = e3.lock().unwrap();
            let on = !e.config.demo_mode;
            let (s, t) = (&e.config.energy.solar, &e.config.energy.thermostat);
            (
                (on && s.enabled).then(|| s.clone()),
                (on && t.enabled).then(|| t.clone()),
            )
        };
        if let Some(s) = solar {
            let r = energy::read_solar(&s);
            e3.lock().unwrap().set_solar(r);
        }
        if let Some(t) = room {
            let r = energy::read_number(&t.url, &t.token, &t.field);
            e3.lock().unwrap().set_room(r);
        }
        std::thread::sleep(Duration::from_secs(15));
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
            let t = coins::fetch_gpu_table();
            {
                let mut e = engine.lock().unwrap();
                e.set_market(n, p, c);
                if let Some(t) = t {
                    e.set_gpu_table(&t);
                }
            }
            update_miners(&engine);
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

    const KRX: &str = "krxABC123";
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
        c.wallet = KRX.into();
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
        c.wallet = KRX.into();
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
        // No name chosen: this PC's hostname. The project's 2% is always "EREBOR".
        let pc = crate::config::default_worker();
        assert_eq!(t.user, format!("krxABC123/{pc}"));
        // No project destination in a test build: no fee (the demo test covers "EREBOR").
        assert!(e.project_target(e.coin).is_none());
        let mut c = e.config().clone();
        c.pool.worker = "Ufficio".into();
        e.update_config(c).unwrap();
        assert_eq!(e.pool_target(Target::User).user, "krxABC123/Ufficio");
        let mut c = e.config().clone();
        c.pool.worker = "erebor".into();
        assert_eq!(e.update_config(c), Err("worker_reserved".into()));
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
        // Project time goes to the project's BTC address on unMineable.
        assert_eq!(
            s.switches.lock().unwrap().as_slice(),
            ["BTC:DEMO-PROJECT-BTC.EREBOR"]
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
        assert_eq!(e.config().wallet, KRX);
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
    fn idle_only_mining_waits_while_the_pc_is_in_use() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, s) = engine(d.path(), false);
        e.config.performance.idle_minutes = Some(5);
        e.idle_source = || Some(10); // someone is typing
        e.start(false).unwrap();
        assert_eq!(e.status().held, Some("in_use"));
        assert_eq!(s.starts.load(Ordering::SeqCst), 0);
        e.idle_source = || Some(600); // away for 10 minutes
        e.idle_at = None; // skip the 5 s polling interval
        e.apply_holds();
        assert_eq!(e.held, None);
        assert_eq!(e.state, RunState::Running);
        assert_eq!(s.starts.load(Ordering::SeqCst), 1);
        e.idle_source = || Some(0); // back at the keyboard: waits at once
        e.idle_at = None;
        e.apply_holds();
        assert_eq!((e.state, e.held), (RunState::Stopped, Some("in_use")));
        assert_eq!(s.stops.load(Ordering::SeqCst), 1);
        e.stop("user"); // stopping by hand forgets the wait
        assert_eq!(e.held, None);
        e.idle_source = || Some(600);
        e.idle_at = None;
        e.apply_holds();
        assert_eq!(e.state, RunState::Stopped);
    }

    #[test]
    fn unprofitable_mining_waits_and_resumes() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, s) = engine(d.path(), false);
        e.config.energy.stop_if_unprofitable = true; // no market data: never flips on its own
        e.econ_bad = true;
        e.start(false).unwrap();
        assert_eq!(e.held, Some("unprofitable"));
        assert_eq!(s.starts.load(Ordering::SeqCst), 0);
        e.econ_bad = false;
        e.apply_holds();
        assert_eq!((e.state, e.held), (RunState::Running, None));
        e.econ_bad = true;
        e.apply_holds();
        assert_eq!((e.state, e.held), (RunState::Stopped, Some("unprofitable")));
    }

    #[test]
    fn a_stuck_gpu_is_restarted_once_then_accepted() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, s) = engine(d.path(), false);
        e.has_gpu = true;
        e.gpu_platform = true;
        e.config.srbminer.path = Some("/mock/srbminer".into());
        let mut c = e.config().clone();
        c.coin = "PRL".into();
        e.update_config(c).unwrap();
        e.start(false).unwrap();
        assert_eq!((e.state, e.coin.id), (RunState::Running, "PRL"));
        e.hashrates.insert("PRL".into(), 1000.0);
        let long_ago = || Instant::now().checked_sub(DROP_STEADY + Duration::from_secs(1));
        assert!(!e.hashrate_dropped(950.0)); // normal
        assert!(e.hashrate_dropped(500.0)); // low, not for long yet
        assert_eq!(s.starts.load(Ordering::SeqCst), 1);
        e.low_since = long_ago();
        assert!(e.hashrate_dropped(500.0)); // low for 5 minutes: restart
        assert_eq!(s.starts.load(Ordering::SeqCst), 2);
        assert_eq!(e.state, RunState::Running);
        e.hashrate_dropped(500.0);
        e.low_since = long_ago();
        assert!(!e.hashrate_dropped(500.0)); // still low after the restart: new normal
        assert_eq!(e.hashrates["PRL"], 500.0);
        assert_eq!(s.starts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn stop_for_a_while_starts_again_by_itself() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, s) = engine(d.path(), false);
        e.stop_for(3600); // not mining: nothing to come back to
        assert_eq!(e.status().resume_in_secs, None);
        e.start(false).unwrap();
        e.stop_for(3600);
        assert_eq!(e.state, RunState::Stopped);
        assert!(e.status().resume_in_secs.is_some_and(|s| s > 3500));
        e.resume_at = Some((Instant::now(), false));
        e.tick();
        assert_eq!(e.state, RunState::Running);
        assert_eq!(s.starts.load(Ordering::SeqCst), 2);
        e.stop_for(3600);
        e.stop("user"); // a real stop cancels the comeback
        assert_eq!(e.resume_at, None);
    }

    #[test]
    fn thermostat_heats_between_its_two_temperatures() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, _) = engine(d.path(), false);
        e.config.energy.thermostat.enabled = true; // 20 °C on, 21 °C off
        e.config.energy.thermostat.url = "http://ha.local/api/states/sensor.room".into();
        let at = |e: &mut Engine, t: f64| {
            e.set_room(Ok(t));
            e.apply_holds();
            (e.state, e.held)
        };
        e.set_room(Ok(21.5));
        e.start(false).unwrap();
        assert_eq!(e.held, Some("warm"));
        assert_eq!(at(&mut e, 20.5), (RunState::Stopped, Some("warm")));
        assert_eq!(at(&mut e, 19.9), (RunState::Running, None));
        assert_eq!(at(&mut e, 20.5), (RunState::Running, None));
        assert_eq!(at(&mut e, 21.0), (RunState::Stopped, Some("warm")));
        // Sensor down: it keeps waiting instead of guessing.
        e.room = Reading::default();
        e.apply_holds();
        assert_eq!(e.held, Some("warm"));
    }

    #[test]
    fn mining_comes_back_after_an_app_update() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, _) = engine(d.path(), false);
        e.prepare_update(); // not mining: nothing to remember
        assert!(!e.take_resume_after_update());
        e.start(false).unwrap();
        e.prepare_update();
        assert!(e.take_resume_after_update());
        assert!(!e.take_resume_after_update()); // only once
    }

    #[test]
    fn efficiency_search_keeps_the_best_limit() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, s) = engine(d.path(), false);
        e.has_gpu = true;
        e.gpu_platform = true;
        e.config.srbminer.path = Some("/mock/srbminer".into());
        let mut c = e.config().clone();
        c.coin = "PRL".into();
        e.update_config(c).unwrap();
        e.start(false).unwrap();
        e.live.gpus = vec![crate::hardware::GpuLive {
            power_w: Some(200.0),
            ..Default::default()
        }];
        e.start_tune().unwrap();
        // Same hashrate at every limit (mock miner), no market data: least watts wins.
        for _ in 0..5 {
            assert!(e.tune.is_some(), "{:?}", e.error);
            e.tune.as_mut().unwrap().step_started = Instant::now()
                .checked_sub(TUNE_WARM + TUNE_MEASURE)
                .unwrap();
            let limit = e.config.performance.gpu_power_limit_w;
            e.live.gpus[0].power_w = Some(limit.map_or(200.0, f64::from));
            e.stats = Some(MinerStats {
                hashrate_10s: Some(1000.0),
                ..Default::default()
            });
            e.tune_tick();
        }
        assert!(e.tune.is_none());
        assert_eq!(e.config().performance.gpu_power_limit_w, Some(120));
        assert_eq!(s.starts.load(Ordering::SeqCst), 6); // start + one per limit tried + final
        assert!(e
            .job
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|j| j.done && j.error.is_none()));
    }

    #[test]
    fn own_wallets_go_to_the_chosen_pool_and_fail_over() {
        let kas = "kaspa:qqkqkzjvr7zwxxmjxjkmxxdwju9kjs6e9u82uh59z07vgaks6gg62v8707g73";
        let d = tempfile::tempdir().unwrap();
        let (mut e, s) = engine(d.path(), false);
        e.has_gpu = true;
        e.gpu_platform = true;
        e.config.lolminer.path = Some("/mock/lolMiner".into());
        // No main payout at all: an own wallet is enough to mine that coin.
        let mut c = e.config().clone();
        c.wallet.clear();
        c.coin = "KAS".into();
        c.coin_wallets.insert("KAS".into(), kas.into());
        c.coin_pools.insert(
            "KAS".into(),
            crate::config::CoinPoolChoice {
                pool: "2miners".into(),
                region: "asia".into(),
                backup: "woolypooly".into(),
                ..Default::default()
            },
        );
        e.update_config(c).unwrap();
        e.start(false).unwrap();
        let t = e.pool_target(Target::User);
        assert_eq!(t.url, "asia-kas.2miners.com:2020");
        assert!(t.user.starts_with(&format!("{kas}.")));
        // This test build has no project destination: no fee, never the user's wallet.
        assert!(e.project_target(e.coin).is_none());
        // Three minutes without a pool connection: the backup takes over.
        e.stats = Some(MinerStats::default());
        e.offline_since = Instant::now().checked_sub(FAILOVER_AFTER);
        e.check_pool_failover();
        assert!(e.on_backup);
        assert_eq!(e.pool_target(Target::User).url, "pool.woolypooly.com:3112");
        assert_eq!(s.starts.load(Ordering::SeqCst), 2);
        // Later it tries the main pool again.
        e.backup_since = Instant::now().checked_sub(PRIMARY_RETRY);
        e.check_pool_failover();
        assert!(!e.on_backup);
        // Coins without a destination are not mined; excluded ones are not picked.
        let etc = coins::find("ETC").unwrap();
        assert_eq!(e.coin_blocker(etc), Some("wallet_missing"));
        e.stop("user");
        let mut c = e.config().clone();
        c.coin = AUTO_COIN.into();
        c.auto_exclude.insert("KAS".into());
        e.update_config(c).unwrap();
        assert!(!e.candidates().iter().any(|c| c.id == "KAS"));
        // Unknown pools and bad custom hosts are refused.
        let mut c = e.config().clone();
        c.coin_pools.get_mut("KAS").unwrap().pool = "nope".into();
        assert!(e.update_config(c).is_err());
        let mut c = e.config().clone();
        let k = c.coin_pools.get_mut("KAS").unwrap();
        (k.pool, k.host, k.port) = ("custom".into(), "pool.example.com; rm".into(), 3333);
        assert!(e.update_config(c).is_err());
    }

    #[test]
    fn power_is_the_measured_draw_of_the_mining_device() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, _) = engine(d.path(), false);
        let gpu = |w| crate::hardware::GpuLive {
            power_w: w,
            ..Default::default()
        };
        e.live.power_w = Some(40.0);
        e.live.gpus = vec![gpu(Some(150.0)), gpu(None), gpu(Some(100.0))];
        assert_eq!(e.power(Device::Cpu), Some(40.0));
        assert_eq!(e.power(Device::Gpu), Some(250.0));
        e.live.gpus = vec![gpu(None)];
        assert_eq!(e.power(Device::Gpu), None);
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
    fn coins_off_kryptex_need_an_own_wallet_and_say_everything_missing() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, _) = engine(d.path(), false);
        e.has_gpu = false;
        let mut c = e.config().clone();
        c.wallet = KRX.into();
        e.update_config(c.clone()).unwrap();
        let firo = coins::find("FIRO").unwrap();
        assert_eq!(e.coin_blocker(firo), Some("own_wallet_required"));
        let reqs: Vec<_> = e
            .requirements(firo)
            .iter()
            .map(|r| (r.kind, r.status, r.code))
            .collect();
        assert!(reqs.contains(&("hardware", "missing", "no_gpu")));
        assert!(reqs.contains(&("miner", "warn", "miner_missing")));
        assert!(reqs.contains(&("payout", "missing", "own_wallet_required")));
        // An own wallet clears the payout requirement; the missing GPU remains.
        c.coin_wallets
            .insert("FIRO".into(), "aFiroPublicAddress1234567890xyz".into());
        e.update_config(c).unwrap();
        assert_eq!(e.coin_blocker(firo), Some("no_gpu"));
        assert!(e
            .requirements(firo)
            .iter()
            .any(|r| r.code == "own_wallet" && r.status == "ok"));
        // ASIC coins show up, but the automatic choice never takes them.
        assert!(e.requirements(coins::find("BTC").unwrap())[0].status == "warn");
        assert!(e.candidates().iter().all(|c| !c.asic));
    }

    #[test]
    fn gpu_coins_need_a_kryptex_id_and_a_gpu() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, _) = engine(d.path(), false);
        e.has_gpu = true;
        e.gpu_platform = true;
        let mut c = e.config().clone();
        c.wallet = WALLET.into();
        e.update_config(c).unwrap();
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
    fn auto_picks_the_most_profitable_coin_without_trying_each() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, s) = engine(d.path(), false);
        e.has_gpu = true;
        e.gpu_platform = true;
        e.config.srbminer.path = Some("/mock/SRBMiner-MULTI".into());
        e.hashrates.clear();
        e.gpu_names = vec!["NVIDIA GeForce RTX 3060".into()];
        e.set_gpu_table(&coins::bundled_gpu_table());
        let mut c = e.config().clone();
        c.wallet = "krxABC123".into();
        e.update_config(c).unwrap();
        // Same reward per hash everywhere: the coin with the highest expected BTC/day for
        // an RTX 3060 wins straight away, from the table alone.
        e.market.coins = coins::COINS.iter().map(|c| market(c.id, 1e-6)).collect();
        let want = coins::best(&e.candidates(), &e.estimates, &e.markets())
            .unwrap()
            .0
            .id;
        e.start(false).unwrap();
        assert_eq!(e.coin.id, want);
        // Another coin becomes 10x better: Erebor moves at the next evaluation.
        let other = if want == "ERG" { "RVN" } else { "ERG" };
        let ratio = e.estimates[want] / e.estimates[other];
        let m = e.market.coins.iter_mut().find(|m| m.id == other).unwrap();
        m.coin_per_hash = 1e-5 * ratio;
        e.tick();
        assert_eq!(e.coin.id, other);
        assert_eq!(s.coins.lock().unwrap().as_slice(), [want, other]);
        // Within REEVAL_SECS nothing changes, even if the first coin becomes better again.
        e.market
            .coins
            .iter_mut()
            .find(|m| m.id == want)
            .unwrap()
            .coin_per_hash = 1.0;
        e.tick();
        assert_eq!(e.coin.id, other);
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
        assert_eq!(a.payout, "bitcoin");
        for c in coins::COINS {
            e.hashrates.insert(c.id.into(), 1000.0);
        }
        e.market.coins = coins::COINS
            .iter()
            .map(|c| market(c.id, if c.id == "PRL" { 3e-6 } else { 1e-6 }))
            .collect();
        // Pearl costs 3% on Kryptex, 1% on unMineable: BTC through unMineable wins.
        e.market.coins[0].pool_fee_percent = 3.0;
        let a = e.advice();
        assert_eq!(a.coin.unwrap().id, "PRL");
        assert_eq!(a.payout, "bitcoin");
        // RVN is Kryptex-only.
        e.market.coins = vec![market("RVN", 9e-6)];
        assert_eq!(e.advice().payout, "kryptex");
        e.has_gpu = false;
        assert_eq!(e.advice().device, Device::Cpu);
    }

    #[test]
    fn outdated_miners_are_announced_once_when_auto_update_is_off() {
        let d = tempfile::tempdir().unwrap();
        let (mut e, _) = engine(d.path(), false);
        e.config.auto_update_miners = false;
        e.config.miner = crate::config::MinerConfig {
            path: Some("/old/xmrig".into()),
            version: Some("0.0.1".into()),
            sha256: None,
            verified: true,
        };
        // Picked by hand (not verified): never touched, never announced.
        e.config.srbminer = crate::config::MinerConfig {
            path: Some("/mine/SRBMiner-MULTI".into()),
            version: Some("0.0.1".into()),
            sha256: None,
            verified: false,
        };
        let e = Arc::new(Mutex::new(e));
        update_miners(&e);
        update_miners(&e);
        let g = e.lock().unwrap();
        let notes: Vec<_> = g
            .alerts
            .iter()
            .filter(|a| a.code == "miner_update_available")
            .collect();
        if install::manifest().asset_for_platform().is_ok() {
            assert_eq!(notes.len(), 1);
            assert!(notes[0].detail.as_deref().unwrap().starts_with("XMRig "));
        }
        assert_eq!(g.config.miner.path.as_deref(), Some("/old/xmrig"));
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
        // PRL pays far more, but the user picked RVN.
        e.hashrates.insert("PRL".into(), 1000.0);
        e.market.coins = vec![market("PRL", 1.0), market("RVN", 1e-9)];
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
    fn converted_payout_mines_on_unmineable_with_referral() {
        const LTC: &str = "LTC:LQ3B36Yv2rBTxdgAdYpU2UcEZsaNwXeATk";
        let d = tempfile::tempdir().unwrap();
        let (mut e, _) = engine(d.path(), false);
        let mut c = e.config().clone();
        c.wallet = LTC.into();
        c.pool.worker = "rig".into();
        e.update_config(c).unwrap();
        e.start(false).unwrap();
        let t = e.pool_target(Target::User);
        assert_eq!(t.url, "rx.unmineable.com:3333");
        assert_eq!(t.user, format!("{LTC}.rig#v3ni-3jdo"));
        // A donation raises the project's share, never past the cap.
        let mut c = e.config().clone();
        c.donation_percent = 1.0;
        e.update_config(c.clone()).unwrap();
        assert_eq!(e.devfee.status().percent, 3.0);
        c.donation_percent = 9.0;
        assert!(e.update_config(c).is_err());
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
        assert_eq!(
            t.user,
            format!("BTC:{BTC}.{}#v3ni-3jdo", crate::config::default_worker())
        );
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
    #[ignore = "Monero/BTC payouts are off for now: Kryptex only"]
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
        assert_eq!(
            t.user,
            format!("{prl_addr}/{}", crate::config::default_worker())
        );
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

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
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

mod coin_choice;
mod jobs;
mod lifecycle;
mod settings;
#[cfg(test)]
mod tests;
mod tick;
mod views;

pub use jobs::{spawn_benchmark, spawn_install, spawn_loops};

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
    /// Mining is on but waiting: "in_use" (idle-only mode), "app" (a listed program
    /// runs), "warm" (thermostat) or "unprofitable".
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
    /// Watts the smart plug in front of this PC reads, when set up.
    pub plug_w: Option<f64>,
    pub plug_error: Option<String>,
    /// Dynamic tariff: today's prices per kWh (market + surcharge), `(start, price)`.
    pub spot_prices: Vec<(i64, f64)>,
    pub spot_error: Option<String>,
    /// The program that makes mining wait (`held == "app"`).
    pub running_app: Option<String>,
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
    plug: Reading,
    /// Market prices (€/kWh, without surcharge) and when they were fetched.
    spot: Vec<(i64, f64)>,
    spot_error: Option<String>,
    spot_at: Option<Instant>,
    /// A program from `performance.pause_apps` that is running, polled like idle time.
    running_app: Option<String>,
    apps_at: Option<Instant>,
    apps_source: fn(&[String]) -> Option<String>,
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
            plug: Reading::default(),
            spot: Vec::new(),
            spot_error: None,
            spot_at: None,
            running_app: None,
            apps_at: None,
            apps_source: hardware::running_app,
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
        if !notify::has_telegram(n) || notify::quiet_now(n) {
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
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop("app_exit");
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

/// The engine lock, also after a panic elsewhere poisoned it: reading or stopping the
/// engine must never depend on an unrelated bug.
pub fn lock(e: &Mutex<Engine>) -> MutexGuard<'_, Engine> {
    e.lock().unwrap_or_else(|p| p.into_inner())
}

fn set_job(job: &Arc<Mutex<Option<JobStatus>>>, s: JobStatus) {
    *job.lock().unwrap() = Some(s);
}

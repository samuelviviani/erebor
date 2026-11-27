//! The controller: every device this installation manages (this PC, paired agents, ASICs
//! reached directly or through a gateway agent), in one model.
//!
//! - Registry, groups, tags, profiles, samples and events in SQLite
//!   (`<data dir>/controller.db`, versioned migrations). Secrets aren't stored there: the
//!   only credential is this installation's TLS key (`remote/key.der`, 0600) and the
//!   pinned fingerprint of each agent's certificate.
//! - Live state: one persistent connection per agent (`subscribe` stream), reconnecting
//!   with exponential backoff and jitter; direct ASICs polled every 15 s; this PC read
//!   in-process. Mining never depends on these connections.
//! - Alerts with thresholds, deduplication and a cooldown; bulk operations with a
//!   concurrency limit and a result per device; profiles checked for compatibility before
//!   they are applied, rolled back when the miner won't start with them.

use crate::asic::{self, AsicAction, AsicEntry, AsicStatus, Capability};
use crate::fleet::Telemetry;
use crate::remote::client::{self, Link};
use crate::remote::{self, Identity};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

mod commands;
mod history;
mod live;
mod ops;
mod registry;
mod rows;
#[cfg(test)]
mod tests;

pub use commands::{call, discover};

pub type LocalCall = Arc<dyn Fn(&str, &Value) -> Result<Value, Value> + Send + Sync>;
pub const LOCAL_ID: &str = "local";
const CONCURRENCY: usize = 8;
const SAMPLE_EVERY: i64 = 60;
/// Samples older than this are averaged into 15-minute rows.
const COMPACT_AFTER: i64 = 2 * 86_400;
const COMPACT_SPAN: i64 = 900;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    LocalPc,
    RemotePc,
    Server,
    GpuRig,
    CpuRig,
    Asic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Conn {
    Online,
    #[default]
    Offline,
    Connecting,
    Unreachable,
    Unauthorized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mining {
    #[default]
    Idle,
    Starting,
    Mining,
    Stopping,
    Error,
    Maintenance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRec {
    pub id: String,
    pub kind: DeviceKind,
    pub name: String,
    pub address: String,
    pub fingerprint: Option<String>,
    /// ASICs reached through an agent: that agent's device id.
    pub gateway: Option<String>,
    pub group: Option<i64>,
    pub tags: Vec<String>,
    /// Price per kWh for this device (overrides the group's and the global one).
    pub energy_price: Option<f64>,
    pub added_at: i64,
    pub last_seen: Option<i64>,
}

#[derive(Debug, Clone, Default)]
struct Live {
    conn: Conn,
    error: Option<String>,
    telemetry: Option<Telemetry>,
    asic: Option<AsicStatus>,
    gateway_asics: Vec<AsicStatus>,
    updated: i64,
    pending: Option<(Mining, i64)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Actions {
    pub start: Capability,
    pub stop: Capability,
    pub restart: Capability,
    pub configure: Capability,
    pub set_pool: Capability,
    pub remove: Capability,
    pub rename: Capability,
}

/// One device as every view shows it. Unknown values are `null`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRow {
    pub id: String,
    pub kind: DeviceKind,
    pub name: String,
    pub address: String,
    pub group: Option<i64>,
    pub group_name: Option<String>,
    pub tags: Vec<String>,
    pub conn: Conn,
    pub mining: Mining,
    pub error: Option<String>,
    pub simulated: bool,
    pub hardware: String,
    pub os: Option<String>,
    pub version: Option<String>,
    pub algo: Option<String>,
    pub coin: Option<String>,
    pub pool: Option<String>,
    pub hashrate: Option<f64>,
    pub temp_c: Option<f32>,
    pub power_w: Option<f64>,
    pub power_measured: bool,
    pub revenue_day: Option<f64>,
    pub cost_day: Option<f64>,
    pub net_day: Option<f64>,
    pub currency: String,
    pub accepted: u64,
    pub rejected: u64,
    pub last_seen: Option<i64>,
    pub gateway: Option<String>,
    pub actions: Actions,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub currency: String,
    /// Price per kWh for every device without its own or its group's. `None`: each
    /// computer's own setting (what its agent reports).
    pub energy_price: Option<f64>,
    pub max_temp_c: f32,
    pub asic_max_temp_c: f32,
    /// Alert when the hashrate falls this far below its recent average (percent).
    pub hashrate_drop_percent: f64,
    pub power_rise_percent: f64,
    pub rejected_percent: f64,
    pub cooldown_minutes: i64,
    pub retention_days: i64,
    pub discovery: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            currency: "EUR".into(),
            energy_price: None,
            max_temp_c: 85.0,
            asic_max_temp_c: 90.0,
            hashrate_drop_percent: 30.0,
            power_rise_percent: 50.0,
            rejected_percent: 5.0,
            cooldown_minutes: 30,
            retention_days: 30,
            discovery: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Group {
    pub id: i64,
    pub name: String,
    pub energy_price: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct AsicPoolSpec {
    /// stratum+tcp://host:port
    pub url: String,
    /// `wallet.worker` or `account.worker`; `{device}` becomes the device name.
    pub user: String,
    pub password: String,
}

/// A reusable mining setup. `config` is a partial Erebor configuration (same keys as
/// `config.json`) merged into each computer's; `asic` is the pool for ASICs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Profile {
    pub id: i64,
    pub name: String,
    pub config: Value,
    pub asic: Option<AsicPoolSpec>,
    /// ASICs: apply only to models of this algorithm ("sha256d", …).
    pub algo: Option<String>,
    /// Start (or restart) mining once applied.
    pub start: bool,
}

/// Configuration keys a profile may set on a computer. Paths, notification channels,
/// demo mode and the like stay local to each computer.
const PROFILE_KEYS: &[&str] = &[
    "coin",
    "wallet",
    "pool",
    "coinWallets",
    "coinPools",
    "autoExclude",
    "performance",
    "safety",
    "schedule",
    "startOnLaunch",
    "autoUpdateMiners",
    "donationPercent",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpResult {
    pub id: String,
    pub name: String,
    /// "completed" | "failed" | "skipped" | "rolled_back"
    pub state: String,
    pub error: Option<String>,
    pub detail: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: i64,
    pub ts: i64,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub level: String,
    pub code: String,
    pub detail: Option<String>,
}

const MIGRATIONS: &[&str] = &[
    "CREATE TABLE groups (
        id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE COLLATE NOCASE, energy_price REAL);
     CREATE TABLE devices (
        id TEXT PRIMARY KEY, kind TEXT NOT NULL, name TEXT NOT NULL, address TEXT NOT NULL,
        fingerprint TEXT, gateway TEXT REFERENCES devices(id) ON DELETE CASCADE,
        group_id INTEGER REFERENCES groups(id) ON DELETE SET NULL,
        energy_price REAL, added_at INTEGER NOT NULL, last_seen INTEGER);
     CREATE TABLE device_tags (
        device_id TEXT NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
        tag TEXT NOT NULL, PRIMARY KEY (device_id, tag));
     CREATE TABLE profiles (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE COLLATE NOCASE, body TEXT NOT NULL);
     CREATE TABLE samples (
        device_id TEXT NOT NULL REFERENCES devices(id) ON DELETE CASCADE, ts INTEGER NOT NULL,
        span INTEGER NOT NULL, algo TEXT, hashrate REAL, power_w REAL, power_measured INTEGER,
        temp_c REAL, revenue_day REAL, cost_day REAL, currency TEXT, mining INTEGER);
     CREATE INDEX samples_device_ts ON samples(device_id, ts);
     CREATE INDEX samples_ts ON samples(ts);
     CREATE TABLE events (
        id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, device_id TEXT, level TEXT NOT NULL,
        code TEXT NOT NULL, detail TEXT);
     CREATE INDEX events_ts ON events(ts);
     CREATE TABLE settings (k TEXT PRIMARY KEY, v TEXT NOT NULL);",
];

fn open_db(path: &Path) -> rusqlite::Result<Connection> {
    let mut c = Connection::open(path)?;
    c.pragma_update_and_check(None, "journal_mode", "WAL", |_| Ok(()))?;
    c.pragma_update(None, "foreign_keys", "ON")?;
    c.busy_timeout(Duration::from_secs(5))?;
    let v: i64 = c.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(v as usize) {
        let tx = c.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(c)
}

fn db_err(e: rusqlite::Error) -> String {
    format!("database: {e}")
}

fn kind_str(k: DeviceKind) -> String {
    serde_json::to_value(k)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn valid_name(n: &str) -> bool {
    !n.trim().is_empty() && n.chars().count() <= 64 && !n.chars().any(char::is_control)
}

fn valid_tag(t: &str) -> bool {
    !t.is_empty() && t.len() <= 32 && t.chars().all(|c| c.is_alphanumeric() || "-_ .".contains(c))
}

/// Deep merge: objects merge key by key, anything else replaces.
fn merge(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            for (k, v) in p {
                merge(b.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (b, p) => *b = p.clone(),
    }
}

/// `{device}` in worker names and ASIC users becomes the device name (letters and digits).
fn templated(v: &Value, device: &str) -> Value {
    let clean: String = device
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(32)
        .collect();
    match v {
        Value::String(s) => Value::String(s.replace("{device}", &clean)),
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, v)| (k.clone(), templated(v, device)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(|v| templated(v, device)).collect()),
        v => v.clone(),
    }
}

fn newer(a: &str, b: &str) -> bool {
    let p = |s: &str| -> Vec<u64> { s.split('.').map(|x| x.parse().unwrap_or(0)).collect() };
    p(a) > p(b)
}

pub struct Fleet {
    pub data_dir: PathBuf,
    db: Mutex<Connection>,
    pub identity: Identity,
    local: Option<LocalCall>,
    live: Mutex<HashMap<String, Live>>,
    watchers: Mutex<HashMap<String, Arc<AtomicBool>>>,
    market: Mutex<Option<(Instant, Value)>>,
    alerted: Mutex<HashMap<(String, String), i64>>,
    prev: Mutex<HashMap<String, (Conn, Mining, f64, f64)>>,
    stopped: AtomicBool,
    /// This computer is set up for mining: only then is it listed among the devices
    /// (someone who only manages other machines doesn't want an idle "this PC" row).
    local_mining: AtomicBool,
}

impl Fleet {
    /// Open the registry. `local`: this computer's engine, shown as the "local" device.
    pub fn open(data_dir: &Path, local: Option<LocalCall>) -> Result<Arc<Self>, String> {
        std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
        let db = open_db(&data_dir.join("controller.db")).map_err(db_err)?;
        let f = Arc::new(Self {
            data_dir: data_dir.to_path_buf(),
            identity: Identity::load_or_create(data_dir)?,
            db: Mutex::new(db),
            local,
            live: Mutex::new(HashMap::new()),
            watchers: Mutex::new(HashMap::new()),
            market: Mutex::new(None),
            alerted: Mutex::new(HashMap::new()),
            prev: Mutex::new(HashMap::new()),
            stopped: AtomicBool::new(false),
            local_mining: AtomicBool::new(false),
        });
        if f.local.is_some() {
            let name = crate::fleet::device(data_dir)
                .map(|d| d.name)
                .unwrap_or_else(|_| "This computer".into());
            f.db()
                .execute(
                    "INSERT OR IGNORE INTO devices (id, kind, name, address, added_at) VALUES (?1, 'local_pc', ?2, 'local', ?3)",
                    params![LOCAL_ID, name, crate::now_unix()],
                )
                .map_err(db_err)?;
        }
        Ok(f)
    }

    fn db(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn live(&self) -> MutexGuard<'_, HashMap<String, Live>> {
        self.live.lock().unwrap_or_else(|p| p.into_inner())
    }

    // ---------- settings ----------

    pub fn settings(&self) -> Settings {
        self.db()
            .query_row("SELECT v FROM settings WHERE k = 'settings'", [], |r| {
                r.get::<_, String>(0)
            })
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn set_settings(&self, s: &Settings) -> Result<(), String> {
        if s.retention_days < 1
            || s.cooldown_minutes < 0
            || s.energy_price.is_some_and(|p| !(0.0..100.0).contains(&p))
        {
            return Err("invalid_settings".into());
        }
        let v = serde_json::to_string(s).map_err(|e| e.to_string())?;
        self.db()
            .execute(
                "INSERT OR REPLACE INTO settings (k, v) VALUES ('settings', ?1)",
                params![v],
            )
            .map(|_| ())
            .map_err(db_err)
    }
}

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

    // ---------- registry ----------

    pub fn devices(&self) -> Vec<DeviceRec> {
        let db = self.db();
        let mut tags: HashMap<String, Vec<String>> = HashMap::new();
        if let Ok(mut st) = db.prepare("SELECT device_id, tag FROM device_tags ORDER BY tag") {
            let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)));
            for (d, t) in rows.into_iter().flatten().flatten() {
                tags.entry(d).or_default().push(t);
            }
        }
        let Ok(mut st) = db.prepare(
            "SELECT id, kind, name, address, fingerprint, gateway, group_id, energy_price, added_at, last_seen
             FROM devices ORDER BY CASE kind WHEN 'local_pc' THEN 0 ELSE 1 END, name COLLATE NOCASE",
        ) else {
            return vec![];
        };
        let rows = st.query_map([], |r| {
            let kind: String = r.get(1)?;
            Ok(DeviceRec {
                id: r.get(0)?,
                kind: serde_json::from_value(json!(kind)).unwrap_or(DeviceKind::RemotePc),
                name: r.get(2)?,
                address: r.get(3)?,
                fingerprint: r.get(4)?,
                gateway: r.get(5)?,
                group: r.get(6)?,
                energy_price: r.get(7)?,
                added_at: r.get(8)?,
                last_seen: r.get(9)?,
                tags: vec![],
            })
        });
        rows.into_iter()
            .flatten()
            .flatten()
            .map(|mut d| {
                d.tags = tags.remove(&d.id).unwrap_or_default();
                d
            })
            .collect()
    }

    /// By id, else by name (refused when several devices share it).
    pub fn device(&self, id: &str) -> Result<DeviceRec, String> {
        let devs = self.devices();
        if let Some(d) = devs.iter().find(|d| d.id == id) {
            return Ok(d.clone());
        }
        let mut named = devs.into_iter().filter(|d| d.name.eq_ignore_ascii_case(id));
        match (named.next(), named.next()) {
            (Some(d), None) => Ok(d),
            (Some(_), Some(_)) => Err("device_ambiguous".into()),
            _ => Err("device_not_found".into()),
        }
    }

    fn insert(&self, d: &DeviceRec) -> Result<(), String> {
        let n = self
            .db()
            .execute(
                "INSERT OR IGNORE INTO devices (id, kind, name, address, fingerprint, gateway, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![d.id, kind_str(d.kind), d.name, d.address, d.fingerprint, d.gateway, d.added_at],
            )
            .map_err(db_err)?;
        if n == 0 {
            return Err("device_exists".into());
        }
        self.event(Some(&d.id), "info", "device_added", None);
        Ok(())
    }

    /// Pair with an agent and add it. `kind`: how it is shown (remote PC, server, rig).
    pub fn add_agent(
        &self,
        address: &str,
        code: &str,
        kind: Option<DeviceKind>,
    ) -> Result<DeviceRec, String> {
        let me = crate::fleet::device(&self.data_dir)?.name;
        let p = client::pair(&self.identity, address, code, &me)?;
        let d = DeviceRec {
            id: format!("agent:{}", p.device.id),
            kind: kind
                .filter(|k| !matches!(k, DeviceKind::LocalPc | DeviceKind::Asic))
                .unwrap_or(DeviceKind::RemotePc),
            name: p.device.name,
            address: address.trim().into(),
            fingerprint: Some(p.fingerprint),
            gateway: None,
            group: None,
            tags: vec![],
            energy_price: None,
            added_at: crate::now_unix(),
            last_seen: Some(crate::now_unix()),
        };
        // Paired again (new code, new address): keep the record, refresh the trust.
        let updated = self
            .db()
            .execute(
                "UPDATE devices SET address = ?2, fingerprint = ?3 WHERE id = ?1",
                params![d.id, d.address, d.fingerprint],
            )
            .map_err(db_err)?;
        if updated == 0 {
            self.insert(&d)?;
        } else {
            self.event(Some(&d.id), "info", "device_repaired", None);
        }
        self.device(&d.id)
    }

    /// Add an ASIC by address, read directly or through a paired gateway agent. The same
    /// ASIC (same address) is never added twice, whichever path reaches it.
    pub fn add_asic(
        &self,
        host: &str,
        port: u16,
        name: &str,
        via: Option<&str>,
    ) -> Result<DeviceRec, String> {
        let e = AsicEntry {
            host: host.trim().into(),
            port: if port == 0 { asic::API_PORT } else { port },
            name: name.trim().into(),
        };
        asic::validate_entry(&e)?;
        if self.devices().iter().any(|d| d.id == e.id()) {
            return Err("device_exists".into());
        }
        let st = match via {
            Some(g) => {
                let gw = self.device(g)?;
                if gw.kind == DeviceKind::Asic || gw.id == LOCAL_ID {
                    return Err("not_a_gateway".into());
                }
                let v = self
                    .agent_call(&gw, "asics.add", &json!({"asic": e}))
                    .or_else(|err| {
                        if err == "asic_exists" {
                            Ok(Value::Null)
                        } else {
                            Err(err)
                        }
                    })?;
                serde_json::from_value::<AsicStatus>(v).ok()
            }
            None => {
                let st = asic::read(&e);
                if !st.online {
                    return Err(st.error.unwrap_or_else(|| "asic_unreachable".into()));
                }
                Some(st)
            }
        };
        let name = if e.name.is_empty() {
            st.as_ref()
                .and_then(|s| s.model.clone())
                .unwrap_or_else(|| e.host.clone())
        } else {
            e.name.clone()
        };
        let d = DeviceRec {
            id: e.id(),
            kind: DeviceKind::Asic,
            name,
            address: format!("{}:{}", e.host, e.port),
            fingerprint: None,
            gateway: via.map(|g| self.device(g).map(|d| d.id)).transpose()?,
            group: None,
            tags: vec![],
            energy_price: None,
            added_at: crate::now_unix(),
            last_seen: st.as_ref().filter(|s| s.online).map(|s| s.checked_at),
        };
        self.insert(&d)?;
        if let Some(st) = st {
            self.live().insert(
                d.id.clone(),
                Live {
                    conn: Conn::Online,
                    asic: Some(st),
                    updated: crate::now_unix(),
                    ..Default::default()
                },
            );
        }
        Ok(d)
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let d = self.device(id)?;
        if d.id == LOCAL_ID {
            return Err("cannot_remove_local".into());
        }
        // Best effort: tell the agent to forget us; tell a gateway to drop the ASIC.
        if d.kind != DeviceKind::Asic {
            let _ = self.agent_call(&d, "unpair", &Value::Null);
        } else if let Some(g) = d.gateway.as_deref().and_then(|g| self.device(g).ok()) {
            let _ = self.agent_call(&g, "asics.remove", &json!({"id": d.id}));
        }
        self.db()
            .execute("DELETE FROM devices WHERE id = ?1", params![d.id])
            .map_err(db_err)?;
        self.live().remove(&d.id);
        if let Some(stop) = self
            .watchers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&d.id)
        {
            stop.store(true, Ordering::SeqCst);
        }
        self.event(None, "info", "device_removed", Some(&d.name));
        Ok(())
    }

    pub fn rename(&self, id: &str, name: &str) -> Result<(), String> {
        if !valid_name(name) {
            return Err("invalid_device_name".into());
        }
        let d = self.device(id)?;
        // The agent keeps its own name too, so other controllers see the same.
        if d.id == LOCAL_ID {
            let _ = self.local_call("set_device_name", &json!({"name": name.trim()}));
        } else if d.kind != DeviceKind::Asic {
            let _ = self.agent_call(&d, "device.rename", &json!({"name": name.trim()}));
        }
        self.db()
            .execute(
                "UPDATE devices SET name = ?2 WHERE id = ?1",
                params![d.id, name.trim()],
            )
            .map(|_| ())
            .map_err(db_err)
    }

    pub fn set_kind(&self, id: &str, kind: DeviceKind) -> Result<(), String> {
        let d = self.device(id)?;
        if matches!(kind, DeviceKind::LocalPc | DeviceKind::Asic)
            || matches!(d.kind, DeviceKind::LocalPc | DeviceKind::Asic)
        {
            return Err("invalid_kind".into());
        }
        self.db()
            .execute(
                "UPDATE devices SET kind = ?2 WHERE id = ?1",
                params![d.id, kind_str(kind)],
            )
            .map(|_| ())
            .map_err(db_err)
    }

    pub fn set_energy_price(&self, id: &str, price: Option<f64>) -> Result<(), String> {
        if price.is_some_and(|p| !(0.0..100.0).contains(&p)) {
            return Err("invalid_price".into());
        }
        let d = self.device(id)?;
        self.db()
            .execute(
                "UPDATE devices SET energy_price = ?2 WHERE id = ?1",
                params![d.id, price],
            )
            .map(|_| ())
            .map_err(db_err)
    }

    // ---------- groups and tags ----------

    pub fn groups(&self) -> Vec<Group> {
        let db = self.db();
        let Ok(mut st) =
            db.prepare("SELECT id, name, energy_price FROM groups ORDER BY name COLLATE NOCASE")
        else {
            return vec![];
        };
        st.query_map([], |r| {
            Ok(Group {
                id: r.get(0)?,
                name: r.get(1)?,
                energy_price: r.get(2)?,
            })
        })
        .into_iter()
        .flatten()
        .flatten()
        .collect()
    }

    pub fn group_id(&self, name_or_id: &str) -> Result<i64, String> {
        self.groups()
            .into_iter()
            .find(|g| g.name.eq_ignore_ascii_case(name_or_id) || g.id.to_string() == name_or_id)
            .map(|g| g.id)
            .ok_or_else(|| "group_not_found".into())
    }

    pub fn save_group(&self, g: &Group) -> Result<Group, String> {
        if !valid_name(&g.name) || g.energy_price.is_some_and(|p| !(0.0..100.0).contains(&p)) {
            return Err("invalid_group".into());
        }
        let db = self.db();
        let r = if g.id == 0 {
            db.execute(
                "INSERT INTO groups (name, energy_price) VALUES (?1, ?2)",
                params![g.name.trim(), g.energy_price],
            )
            .map(|_| db.last_insert_rowid())
        } else {
            db.execute(
                "UPDATE groups SET name = ?2, energy_price = ?3 WHERE id = ?1",
                params![g.id, g.name.trim(), g.energy_price],
            )
            .map(|_| g.id)
        };
        match r {
            Ok(id) => Ok(Group {
                id,
                name: g.name.trim().into(),
                energy_price: g.energy_price,
            }),
            Err(e) if e.to_string().contains("UNIQUE") => Err("group_exists".into()),
            Err(e) => Err(db_err(e)),
        }
    }

    pub fn delete_group(&self, id: i64) -> Result<(), String> {
        let n = self
            .db()
            .execute("DELETE FROM groups WHERE id = ?1", params![id])
            .map_err(db_err)?;
        if n == 0 {
            return Err("group_not_found".into());
        }
        Ok(())
    }

    pub fn assign_group(&self, ids: &[String], group: Option<i64>) -> Result<usize, String> {
        let db = self.db();
        let mut n = 0;
        for id in ids {
            n += db
                .execute(
                    "UPDATE devices SET group_id = ?2 WHERE id = ?1",
                    params![id, group],
                )
                .map_err(|e| {
                    if e.to_string().contains("FOREIGN KEY") {
                        "group_not_found".into()
                    } else {
                        db_err(e)
                    }
                })?;
        }
        Ok(n)
    }

    /// Add (`add`) or remove tags on devices.
    pub fn tag(&self, ids: &[String], tags: &[String], add: bool) -> Result<(), String> {
        if !tags.iter().all(|t| valid_tag(t.trim())) {
            return Err("invalid_tag".into());
        }
        let db = self.db();
        for id in ids {
            for t in tags {
                let sql = if add {
                    "INSERT OR IGNORE INTO device_tags (device_id, tag) VALUES (?1, ?2)"
                } else {
                    "DELETE FROM device_tags WHERE device_id = ?1 AND tag = ?2"
                };
                db.execute(sql, params![id, t.trim()]).map_err(db_err)?;
            }
        }
        Ok(())
    }

    // ---------- profiles ----------

    pub fn profiles(&self) -> Vec<Profile> {
        let db = self.db();
        let Ok(mut st) =
            db.prepare("SELECT id, name, body FROM profiles ORDER BY name COLLATE NOCASE")
        else {
            return vec![];
        };
        st.query_map([], |r| {
            let body: String = r.get(2)?;
            let mut p: Profile = serde_json::from_str(&body).unwrap_or_default();
            p.id = r.get(0)?;
            p.name = r.get(1)?;
            Ok(p)
        })
        .into_iter()
        .flatten()
        .flatten()
        .collect()
    }

    pub fn profile(&self, name_or_id: &str) -> Result<Profile, String> {
        self.profiles()
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case(name_or_id) || p.id.to_string() == name_or_id)
            .ok_or_else(|| "profile_not_found".into())
    }

    pub fn save_profile(&self, p: &Profile) -> Result<Profile, String> {
        if !valid_name(&p.name) {
            return Err("invalid_profile".into());
        }
        let config = match &p.config {
            Value::Null => json!({}),
            Value::Object(m) => {
                if let Some(k) = m.keys().find(|k| !PROFILE_KEYS.contains(&k.as_str())) {
                    return Err(format!("profile_key_not_allowed:{k}"));
                }
                Value::Object(m.clone())
            }
            _ => return Err("invalid_profile".into()),
        };
        if let Some(a) = &p.asic {
            if !a.url.starts_with("stratum") || a.user.trim().is_empty() {
                return Err("invalid_pool".into());
            }
        }
        let p = Profile {
            config,
            name: p.name.trim().into(),
            ..p.clone()
        };
        let body = serde_json::to_string(&p).map_err(|e| e.to_string())?;
        let db = self.db();
        let r = if p.id == 0 {
            db.execute(
                "INSERT INTO profiles (name, body) VALUES (?1, ?2)",
                params![p.name, body],
            )
            .map(|_| db.last_insert_rowid())
        } else {
            db.execute(
                "UPDATE profiles SET name = ?2, body = ?3 WHERE id = ?1",
                params![p.id, p.name, body],
            )
            .map(|_| p.id)
        };
        match r {
            Ok(id) => Ok(Profile { id, ..p }),
            Err(e) if e.to_string().contains("UNIQUE") => Err("profile_exists".into()),
            Err(e) => Err(db_err(e)),
        }
    }

    pub fn delete_profile(&self, id: i64) -> Result<(), String> {
        let n = self
            .db()
            .execute("DELETE FROM profiles WHERE id = ?1", params![id])
            .map_err(db_err)?;
        if n == 0 {
            return Err("profile_not_found".into());
        }
        Ok(())
    }

    // ---------- transport ----------

    fn local_call(&self, cmd: &str, args: &Value) -> Result<Value, String> {
        let f = self.local.as_ref().ok_or("local_unavailable")?;
        f(cmd, args).map_err(code)
    }

    fn agent_call(&self, d: &DeviceRec, cmd: &str, args: &Value) -> Result<Value, String> {
        let mut l = Link::connect(
            &self.identity,
            &d.address,
            Some(d.fingerprint.as_deref().ok_or("not_paired")?),
        )?;
        if l.hello["paired"] == false {
            return Err("unauthorized".into());
        }
        l.call(cmd, args).map_err(code)
    }

    /// A command for a computer (agent or this PC), by its remote name.
    fn computer(&self, d: &DeviceRec, cmd: &str, args: &Value) -> Result<Value, String> {
        if d.id != LOCAL_ID {
            return self.agent_call(d, cmd, args);
        }
        let action = |a| {
            let f = self.local.as_ref().ok_or("local_unavailable")?;
            crate::ipc::control_via(&|c, x| f(c, x), a)
                .map(
                    |o| json!({"state": "completed", "changed": o.changed, "snapshot": o.snapshot}),
                )
                .map_err(code)
        };
        match cmd {
            "mining.start" => action(crate::ipc::Action::Start),
            "mining.stop" => action(crate::ipc::Action::Stop),
            "mining.restart" => action(crate::ipc::Action::Restart),
            "mining.config" => self.local_call("get_config", args),
            "mining.config.set" => self
                .local_call("update_config", args)
                .map(|c| json!({"state": "completed", "config": c})),
            "telemetry" => self.local_call("telemetry", args),
            "logs" => self.local_call("get_logs", args),
            "coins" => self.local_call("coin_options", args),
            "stats" => self.local_call("get_stats", args),
            "miners" => {
                let c: crate::config::AppConfig =
                    serde_json::from_value(self.local_call("get_config", &Value::Null)?)
                        .map_err(|_| "bad_config")?;
                Ok(json!(crate::miner::MinerKind::ALL
                    .iter()
                    .map(|k| {
                        let pin = crate::install::manifest_for(*k);
                        json!({"kind": k, "installed": c.miner_for(*k).path.is_some(),
                            "availableHere": pin.asset_for_platform().is_ok()})
                    })
                    .collect::<Vec<_>>()))
            }
            _ => Err("unknown_command".into()),
        }
    }

    /// A gateway's ASIC (through the agent) or a direct one.
    fn asic_action(&self, d: &DeviceRec, a: &AsicAction) -> Result<Value, String> {
        match d.gateway.as_deref() {
            Some(g) => {
                let gw = self.device(g)?;
                self.agent_call(&gw, "asics.action", &json!({"id": d.id, "action": a}))
            }
            None => {
                let (host, port) = d.address.rsplit_once(':').ok_or("bad_address")?;
                let e = AsicEntry {
                    host: host.into(),
                    port: port.parse().unwrap_or(asic::API_PORT),
                    name: d.name.clone(),
                };
                asic::act(&e, a)
            }
        }
    }

    // ---------- live state ----------

    fn set_live(&self, id: &str, f: impl FnOnce(&mut Live)) {
        let mut l = self.live();
        let e = l.entry(id.to_string()).or_default();
        f(e);
        e.updated = crate::now_unix();
    }

    fn update_from_telemetry(&self, id: &str, t: Telemetry) {
        self.set_live(id, |l| {
            l.conn = Conn::Online;
            l.error = None;
            l.telemetry = Some(t);
        });
    }

    /// Read every device once, in parallel (CLI, first load).
    pub fn refresh_once(&self) {
        let devs = self.devices();
        self.for_each(&devs, |d| {
            self.poll(d);
            Ok(Value::Null)
        });
    }

    fn poll(&self, d: &DeviceRec) {
        match d.kind {
            _ if d.id == LOCAL_ID => {
                let set_up = self.local_call("get_config", &Value::Null).is_ok_and(|c| {
                    c["onboardingCompleted"] == true
                        || c["demoMode"] == true
                        || c["wallet"].as_str().is_some_and(|w| !w.is_empty())
                });
                self.local_mining.store(set_up, Ordering::SeqCst);
                if !set_up {
                    return;
                }
                match self.local_call("telemetry", &Value::Null) {
                    Ok(v) => match serde_json::from_value::<Telemetry>(v) {
                        Ok(t) => self.update_from_telemetry(&d.id, t),
                        Err(_) => self.set_live(&d.id, |l| l.conn = Conn::Unreachable),
                    },
                    Err(e) => self.set_live(&d.id, |l| {
                        l.conn = Conn::Offline;
                        l.error = Some(e);
                    }),
                }
            }
            DeviceKind::Asic if d.gateway.is_some() => {} // from the gateway's stream
            DeviceKind::Asic => {
                let (host, port) = d.address.rsplit_once(':').unwrap_or((&d.address, "4028"));
                let st = asic::read(&AsicEntry {
                    host: host.into(),
                    port: port.parse().unwrap_or(asic::API_PORT),
                    name: d.name.clone(),
                });
                self.set_live(&d.id, |l| {
                    l.conn = if st.online {
                        Conn::Online
                    } else {
                        Conn::Offline
                    };
                    l.error = st.error.clone();
                    l.asic = Some(st);
                });
            }
            _ => {
                let r = (|| {
                    let mut l =
                        Link::connect(&self.identity, &d.address, d.fingerprint.as_deref())?;
                    if l.hello["paired"] == false {
                        return Err("unauthorized".to_string());
                    }
                    let t = l.call("telemetry", &Value::Null).map_err(code)?;
                    let a = l.call("asics", &Value::Null).unwrap_or(Value::Null);
                    Ok((t, a))
                })();
                match r {
                    Ok((t, a)) => {
                        if let Ok(t) = serde_json::from_value(t) {
                            self.update_from_telemetry(&d.id, t);
                        }
                        let a: Vec<AsicStatus> = serde_json::from_value(a).unwrap_or_default();
                        self.set_live(&d.id, |l| l.gateway_asics = a);
                    }
                    Err(e) => self.set_live(&d.id, |l| {
                        l.conn = conn_for(&e);
                        l.error = Some(e);
                    }),
                }
            }
        }
    }

    /// Keep one event stream per paired agent, reconnecting with backoff and jitter.
    fn watch(self: Arc<Self>, id: String, stop: Arc<AtomicBool>) {
        let mut backoff = 1.0f64;
        while !stop.load(Ordering::SeqCst) && !self.stopped.load(Ordering::SeqCst) {
            let Ok(d) = self.device(&id) else { return };
            self.set_live(&id, |l| {
                if l.conn != Conn::Online {
                    l.conn = Conn::Connecting;
                }
            });
            let r = (|| -> Result<(), String> {
                let mut l = Link::connect(&self.identity, &d.address, d.fingerprint.as_deref())?;
                if l.hello["paired"] == false {
                    return Err("unauthorized".into());
                }
                l.subscribe(3).map_err(code)?;
                backoff = 1.0;
                loop {
                    if stop.load(Ordering::SeqCst) || self.stopped.load(Ordering::SeqCst) {
                        return Ok(());
                    }
                    let ev = l.next_event()?;
                    match ev["event"].as_str() {
                        Some("telemetry") => {
                            if let Ok(t) = serde_json::from_value(ev["data"].clone()) {
                                self.update_from_telemetry(&id, t);
                            }
                        }
                        Some("asics") => {
                            let a: Vec<AsicStatus> =
                                serde_json::from_value(ev["data"].clone()).unwrap_or_default();
                            self.set_live(&id, |l| l.gateway_asics = a);
                        }
                        Some("alert") => {
                            let a = &ev["data"];
                            self.event(
                                Some(&id),
                                a["level"].as_str().unwrap_or("warning"),
                                a["code"].as_str().unwrap_or("agent_alert"),
                                a["detail"].as_str(),
                            );
                        }
                        Some("revoked") => return Err("unauthorized".into()),
                        _ => {}
                    }
                }
            })();
            if let Err(e) = r {
                self.set_live(&id, |l| {
                    l.conn = conn_for(&e);
                    l.error = Some(e.clone());
                });
                if e == "unauthorized" || e == "fingerprint_mismatch" {
                    backoff = 60.0;
                }
            }
            // Exponential backoff with ±50% jitter, at most a minute.
            let wait = backoff * (0.5 + rand::random::<f64>());
            backoff = (backoff * 2.0).min(60.0);
            let t = Instant::now();
            while t.elapsed().as_secs_f64() < wait {
                if stop.load(Ordering::SeqCst) || self.stopped.load(Ordering::SeqCst) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }

    /// Background work for a long-running controller (the desktop app): streams, polls,
    /// samples, alerts, retention.
    pub fn start(self: &Arc<Self>) {
        let f = self.clone();
        std::thread::spawn(move || {
            let mut last_sample = 0i64;
            let mut last_asic = Instant::now() - Duration::from_secs(60);
            let mut last_compact = 0i64;
            while !f.stopped.load(Ordering::SeqCst) {
                let devs = f.devices();
                f.sync_watchers(&devs);
                if let Some(d) = devs.iter().find(|d| d.id == LOCAL_ID) {
                    f.poll(d);
                }
                if last_asic.elapsed() >= Duration::from_secs(15) {
                    last_asic = Instant::now();
                    let direct: Vec<DeviceRec> = devs
                        .iter()
                        .filter(|d| d.kind == DeviceKind::Asic && d.gateway.is_none())
                        .cloned()
                        .collect();
                    let f2 = f.clone();
                    std::thread::spawn(move || {
                        f2.for_each(&direct, |d| {
                            f2.poll(d);
                            Ok(Value::Null)
                        });
                    });
                }
                f.refresh_market();
                let rows = f.rows();
                f.check_alerts(&rows);
                let now = crate::now_unix();
                if now - last_sample >= SAMPLE_EVERY {
                    last_sample = now;
                    f.sample(&rows);
                }
                if now - last_compact >= 3600 {
                    last_compact = now;
                    let _ = f.compact();
                }
                std::thread::sleep(Duration::from_secs(3));
            }
        });
    }

    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
    }

    fn sync_watchers(self: &Arc<Self>, devs: &[DeviceRec]) {
        let mut w = self.watchers.lock().unwrap_or_else(|p| p.into_inner());
        let agents: Vec<&DeviceRec> = devs
            .iter()
            .filter(|d| d.kind != DeviceKind::Asic && d.id != LOCAL_ID)
            .collect();
        w.retain(|id, stop| {
            let keep = agents.iter().any(|d| &d.id == id);
            if !keep {
                stop.store(true, Ordering::SeqCst);
            }
            keep
        });
        for d in agents {
            if !w.contains_key(&d.id) {
                let stop = Arc::new(AtomicBool::new(false));
                w.insert(d.id.clone(), stop.clone());
                let (f, id) = (self.clone(), d.id.clone());
                let _ = std::thread::Builder::new()
                    .stack_size(256 * 1024)
                    .spawn(move || f.watch(id, stop));
            }
        }
    }

    /// Market data from this computer's engine, to value ASIC hashrate (10-minute cache).
    fn refresh_market(&self) {
        let stale = self
            .market
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .is_none_or(|(t, _)| t.elapsed() > Duration::from_secs(600));
        if stale {
            if let Ok(m) = self.local_call("market", &Value::Null) {
                *self.market.lock().unwrap_or_else(|p| p.into_inner()) = Some((Instant::now(), m));
            }
        }
    }

    /// Estimated revenue per day of an ASIC, in `currency`, from public market data.
    fn asic_revenue(&self, algo: &str, hashrate: f64, currency: &str) -> Option<f64> {
        let coin = crate::coins::find(match algo {
            "sha256d" => "BTC",
            "scrypt" => "LTC",
            _ => return None,
        })?;
        let m = self
            .market
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()?
            .1
            .clone();
        let c = m["coins"].as_array()?.iter().find(|c| c["id"] == coin.id)?;
        let cm = crate::coins::CoinMarket {
            id: coin.id.into(),
            coin_per_hash: c["coinPerHash"].as_f64()?,
            price_btc: c["priceBtc"].as_f64()?,
            pool_fee_percent: c["poolFeePercent"].as_f64().unwrap_or(0.0),
        };
        let btc_day = crate::coins::btc_per_day(hashrate, &cm, coin);
        let btc_price = m["btc"][currency.to_ascii_lowercase()].as_f64()?;
        Some(btc_day * btc_price)
    }

    /// Every device with its live state, ready to show.
    pub fn rows(&self) -> Vec<DeviceRow> {
        let devs = self.devices();
        let groups: HashMap<i64, Group> = self.groups().into_iter().map(|g| (g.id, g)).collect();
        let settings = self.settings();
        let live = self.live().clone();
        let now = crate::now_unix();
        let local_mining = self.local_mining.load(Ordering::SeqCst);
        devs.iter()
            .filter(|d| d.id != LOCAL_ID || local_mining)
            .map(|d| {
                let l = live.get(&d.id).cloned().unwrap_or_default();
                let group = d.group.and_then(|g| groups.get(&g));
                let price = d
                    .energy_price
                    .or(group.and_then(|g| g.energy_price))
                    .or(settings.energy_price);
                let mut row = DeviceRow {
                    id: d.id.clone(),
                    kind: d.kind,
                    name: d.name.clone(),
                    address: d.address.clone(),
                    group: d.group,
                    group_name: group.map(|g| g.name.clone()),
                    tags: d.tags.clone(),
                    conn: l.conn,
                    mining: Mining::Idle,
                    error: l.error.clone(),
                    simulated: false,
                    hardware: String::new(),
                    os: None,
                    version: None,
                    algo: None,
                    coin: None,
                    pool: None,
                    hashrate: None,
                    temp_c: None,
                    power_w: None,
                    power_measured: false,
                    revenue_day: None,
                    cost_day: None,
                    net_day: None,
                    currency: settings.currency.clone(),
                    accepted: 0,
                    rejected: 0,
                    last_seen: if l.conn == Conn::Online {
                        Some(l.updated)
                    } else {
                        d.last_seen
                    },
                    gateway: d.gateway.clone(),
                    actions: Actions::default(),
                };
                if d.kind == DeviceKind::Asic {
                    let st = match &d.gateway {
                        Some(g) => {
                            let gl = live.get(g);
                            let st = gl.and_then(|gl| {
                                gl.gateway_asics.iter().find(|a| a.id == d.id).cloned()
                            });
                            row.conn = match (gl.map(|g| g.conn), &st) {
                                (Some(Conn::Online), Some(s)) if s.online => Conn::Online,
                                (Some(Conn::Online), Some(_)) => Conn::Offline,
                                (Some(Conn::Online), None) => Conn::Connecting,
                                _ => Conn::Offline,
                            };
                            if row.conn == Conn::Offline
                                && gl.is_none_or(|g| g.conn != Conn::Online)
                            {
                                row.error = Some("gateway_offline".into());
                            } else {
                                row.error = st.as_ref().and_then(|s| s.error.clone());
                            }
                            st
                        }
                        None => l.asic.clone(),
                    };
                    if let Some(s) = st {
                        row.simulated = s.simulated;
                        row.hardware = s.model.clone().unwrap_or_else(|| "ASIC".into());
                        row.version = s.firmware.clone();
                        row.algo = s.algo.clone();
                        row.hashrate = s.hashrate;
                        row.temp_c = s.temp_max_c;
                        row.power_w = s.power_w;
                        row.power_measured = s.power_w.is_some();
                        row.accepted = s.accepted;
                        row.rejected = s.rejected;
                        row.pool = s.pools.iter().find(|p| p.active).map(|p| p.url.clone());
                        row.mining = if !s.online {
                            if s.error.is_some() && row.conn == Conn::Online {
                                Mining::Error
                            } else {
                                Mining::Idle
                            }
                        } else if s.hashrate.unwrap_or(0.0) > 0.0 {
                            Mining::Mining
                        } else {
                            Mining::Idle
                        };
                        if s.checked_at > 0 && row.conn == Conn::Online {
                            row.last_seen = Some(s.checked_at);
                        }
                        if let (Some(a), Some(h)) = (&s.algo, s.hashrate) {
                            row.revenue_day = self.asic_revenue(a, h, &settings.currency);
                        }
                        let off = |c: &Capability| {
                            if row.conn != Conn::Online {
                                Capability {
                                    supported: false,
                                    reason: Some("device_offline".into()),
                                }
                            } else {
                                c.clone()
                            }
                        };
                        row.actions = Actions {
                            start: off(&s.capabilities.start_stop),
                            stop: off(&s.capabilities.start_stop),
                            restart: off(&s.capabilities.restart),
                            configure: Capability {
                                supported: false,
                                reason: Some("asic_configure_via_pool".into()),
                            },
                            set_pool: off(&s.capabilities.set_pool),
                            remove: Capability {
                                supported: true,
                                reason: None,
                            },
                            rename: Capability {
                                supported: true,
                                reason: None,
                            },
                        };
                    } else {
                        let off = Capability {
                            supported: false,
                            reason: Some("device_offline".into()),
                        };
                        row.actions = Actions {
                            start: off.clone(),
                            stop: off.clone(),
                            restart: off.clone(),
                            configure: off.clone(),
                            set_pool: off,
                            remove: Capability {
                                supported: true,
                                reason: None,
                            },
                            rename: Capability {
                                supported: true,
                                reason: None,
                            },
                        };
                    }
                } else if let Some(t) = &l.telemetry {
                    let s = &t.snapshot;
                    row.simulated = t.simulated;
                    row.os = Some(t.os.clone());
                    row.version = Some(t.agent_version.clone());
                    row.hardware = std::iter::once(t.cpu_model.clone())
                        .chain(t.gpus.iter().map(|g| g.name.clone()))
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join(" · ");
                    row.algo = Some(s.algo.clone()).filter(|a| !a.is_empty());
                    row.coin = Some(s.coin.clone()).filter(|c| !c.is_empty());
                    row.pool = s
                        .pool
                        .clone()
                        .or_else(|| Some(s.pool_id.clone()).filter(|p| !p.is_empty()));
                    row.hashrate = s.hashrate.filter(|_| s.is_active());
                    row.temp_c = t
                        .gpus
                        .iter()
                        .filter_map(|g| g.temp_c)
                        .chain(s.cpu_temp_c)
                        .reduce(f32::max);
                    row.power_w = s.power_w;
                    row.power_measured = s.power_source == "measured";
                    row.accepted = s.accepted;
                    row.rejected = s.rejected;
                    row.mining = match s.state.as_str() {
                        "running" => Mining::Mining,
                        "error" => Mining::Error,
                        _ if s.job.as_ref().is_some_and(|j| !j.done) => Mining::Maintenance,
                        _ => Mining::Idle,
                    };
                    if let Some(e) = &s.earnings {
                        row.currency = e.currency.clone();
                        row.revenue_day = e.value_per_day.filter(|_| s.is_active());
                        row.cost_day = e.energy_cost_per_day.filter(|_| s.is_active());
                    }
                    let online = row.conn == Conn::Online;
                    let cap = |ok: bool, reason: &str| Capability {
                        supported: ok && online,
                        reason: (!online)
                            .then(|| "device_offline".to_string())
                            .or_else(|| (!ok).then(|| reason.to_string())),
                    };
                    row.actions = Actions {
                        start: cap(
                            s.start_blocker.is_none() || s.is_active(),
                            s.start_blocker.as_deref().unwrap_or(""),
                        ),
                        stop: cap(true, ""),
                        restart: cap(
                            s.start_blocker.is_none(),
                            s.start_blocker.as_deref().unwrap_or(""),
                        ),
                        configure: cap(true, ""),
                        set_pool: cap(true, ""),
                        remove: Capability {
                            supported: d.id != LOCAL_ID,
                            reason: (d.id == LOCAL_ID).then(|| "cannot_remove_local".into()),
                        },
                        rename: Capability {
                            supported: true,
                            reason: None,
                        },
                    };
                } else {
                    let off = Capability {
                        supported: false,
                        reason: Some(
                            match l.conn {
                                Conn::Unauthorized => "unauthorized",
                                _ => "device_offline",
                            }
                            .into(),
                        ),
                    };
                    row.actions = Actions {
                        start: off.clone(),
                        stop: off.clone(),
                        restart: off.clone(),
                        configure: off.clone(),
                        set_pool: off,
                        remove: Capability {
                            supported: d.id != LOCAL_ID,
                            reason: (d.id == LOCAL_ID).then(|| "cannot_remove_local".into()),
                        },
                        rename: Capability {
                            supported: true,
                            reason: None,
                        },
                    };
                }
                // A price set in Erebor (device, group or global) wins over the agent's own.
                if let (Some(p), Some(w)) = (price, row.power_w) {
                    if row.mining == Mining::Mining {
                        row.cost_day = Some(w * 24.0 / 1000.0 * p);
                        row.currency = settings.currency.clone();
                    }
                }
                row.net_day = row
                    .revenue_day
                    .zip(row.cost_day)
                    .map(|(r, c)| r - c)
                    .or(row.revenue_day.filter(|_| row.power_w.is_none()));
                if let Some((m, at)) = l.pending {
                    if now - at < 60 {
                        row.mining = m;
                    }
                }
                row
            })
            .collect()
    }

    /// Totals for the top of the dashboard. Hashrate is summed per algorithm only.
    pub fn overview(&self, rows: &[DeviceRow]) -> Value {
        let mut algos: BTreeMap<String, (f64, usize)> = BTreeMap::new();
        let mut money: BTreeMap<String, (f64, f64, bool, bool)> = BTreeMap::new();
        for r in rows {
            if let (Some(a), Some(h)) = (&r.algo, r.hashrate) {
                let e = algos.entry(a.clone()).or_default();
                e.0 += h;
                e.1 += 1;
            }
            if r.revenue_day.is_some() || r.cost_day.is_some() {
                let m = money.entry(r.currency.clone()).or_default();
                m.0 += r.revenue_day.unwrap_or(0.0);
                m.1 += r.cost_day.unwrap_or(0.0);
                m.2 |= r.revenue_day.is_some();
                m.3 |= r.cost_day.is_some();
            }
        }
        let count = |f: &dyn Fn(&DeviceRow) -> bool| rows.iter().filter(|r| f(r)).count();
        let power: Vec<&DeviceRow> = rows
            .iter()
            .filter(|r| r.power_w.is_some() && r.conn == Conn::Online)
            .collect();
        json!({
            "total": rows.len(),
            "online": count(&|r| r.conn == Conn::Online),
            "offline": count(&|r| r.conn != Conn::Online),
            "mining": count(&|r| r.mining == Mining::Mining),
            "simulated": count(&|r| r.simulated),
            "powerW": power.iter().filter_map(|r| r.power_w).sum::<f64>() + 0.0,
            "powerMeasuredW": power.iter().filter(|r| r.power_measured).filter_map(|r| r.power_w).sum::<f64>() + 0.0,
            "powerDevices": power.len(),
            "hashrate": algos.iter().map(|(a, (h, n))| json!({"algo": a, "hashrate": h, "devices": n})).collect::<Vec<_>>(),
            "money": money.iter().map(|(c, (r, k, hr, hc))| json!({
                "currency": c,
                "revenueDay": hr.then_some(*r),
                "costDay": hc.then_some(*k),
                "netDay": (*hr && *hc).then(|| r - k),
            })).collect::<Vec<_>>(),
        })
    }

    // ---------- operations ----------

    /// Run `f` on each device, at most `CONCURRENCY` at a time; one failure never stops
    /// the others.
    fn for_each(
        &self,
        devs: &[DeviceRec],
        f: impl Fn(&DeviceRec) -> Result<Value, String> + Sync,
    ) -> Vec<OpResult> {
        let next = AtomicUsize::new(0);
        let out = Mutex::new(Vec::new());
        std::thread::scope(|s| {
            for _ in 0..CONCURRENCY.min(devs.len()) {
                s.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(d) = devs.get(i) else { return };
                    let r = f(d);
                    let (state, error, detail) = match r {
                        Ok(v) => (
                            v["state"]
                                .as_str()
                                .filter(|s| *s == "skipped" || *s == "rolled_back")
                                .unwrap_or("completed")
                                .to_string(),
                            v["error"].as_str().map(str::to_string),
                            v,
                        ),
                        Err(e) => ("failed".to_string(), Some(e), Value::Null),
                    };
                    out.lock().unwrap_or_else(|p| p.into_inner()).push((
                        i,
                        OpResult {
                            id: d.id.clone(),
                            name: d.name.clone(),
                            state,
                            error,
                            detail,
                        },
                    ));
                });
            }
        });
        let mut v = out.into_inner().unwrap_or_default();
        v.sort_by_key(|(i, _)| *i);
        v.into_iter().map(|(_, r)| r).collect()
    }

    /// Devices by id or name, by group, or all.
    pub fn select(
        &self,
        ids: &[String],
        group: Option<&str>,
        all: bool,
    ) -> Result<Vec<DeviceRec>, String> {
        let devs = self.devices();
        if all {
            return Ok(devs);
        }
        if let Some(g) = group {
            let gid = self.group_id(g)?;
            return Ok(devs.into_iter().filter(|d| d.group == Some(gid)).collect());
        }
        drop(devs);
        ids.iter()
            .map(|i| self.device(i).map_err(|e| format!("{e}:{i}")))
            .collect()
    }

    /// "start" | "stop" | "restart" on many devices.
    pub fn mining(&self, devs: &[DeviceRec], action: &str) -> Vec<OpResult> {
        let (pending, cmd) = match action {
            "start" => (Mining::Starting, "mining.start"),
            "stop" => (Mining::Stopping, "mining.stop"),
            "restart" => (Mining::Starting, "mining.restart"),
            _ => return vec![],
        };
        let res = self.for_each(devs, |d| {
            let now = crate::now_unix();
            self.set_live(&d.id, |l| l.pending = Some((pending, now)));
            let r = if d.kind == DeviceKind::Asic {
                match action {
                    "restart" => self.asic_action(d, &AsicAction::Restart),
                    _ => Err("asic_no_stop_command".into()),
                }
            } else {
                self.computer(d, cmd, &Value::Null)
            };
            self.set_live(&d.id, |l| l.pending = None);
            if let Ok(v) = &r {
                if let Ok(s) =
                    serde_json::from_value::<crate::fleet::Snapshot>(v["snapshot"].clone())
                {
                    self.set_live(&d.id, |l| {
                        if let Some(t) = &mut l.telemetry {
                            t.snapshot = s;
                        }
                    });
                }
            }
            r
        });
        for r in &res {
            self.event(
                Some(&r.id),
                if r.error.is_some() { "warning" } else { "info" },
                &format!("command_{action}"),
                r.error.as_deref(),
            );
        }
        res
    }

    /// Why a profile can't go on a device, if it can't.
    fn incompatible(&self, d: &DeviceRec, p: &Profile, row: Option<&DeviceRow>) -> Option<String> {
        if d.kind == DeviceKind::Asic {
            let Some(_) = &p.asic else {
                return Some("profile_has_no_asic_pool".into());
            };
            let row = row?;
            if let (Some(want), Some(have)) = (&p.algo, &row.algo) {
                if want != have {
                    return Some(format!("algo_mismatch:{have}"));
                }
            }
            return (!row.actions.set_pool.supported).then(|| {
                row.actions
                    .set_pool
                    .reason
                    .clone()
                    .unwrap_or_else(|| "unsupported".into())
            });
        }
        if p.config.as_object().is_none_or(|m| m.is_empty()) {
            return Some("profile_has_no_computer_config".into());
        }
        if let Some(coin) = p.config["coin"]
            .as_str()
            .filter(|c| *c != crate::config::AUTO_COIN)
        {
            let Some(c) = crate::coins::find(coin) else {
                return Some("unknown_coin".into());
            };
            let tel = self.live().get(&d.id).and_then(|l| l.telemetry.clone());
            if c.device == crate::coins::Device::Gpu
                && tel.as_ref().is_some_and(|t| t.gpus.is_empty())
            {
                return Some("no_gpu".into());
            }
            // The miner for that coin must exist for the device's OS and CPU.
            if let Ok(m) = self.computer(d, "miners", &Value::Null) {
                let ok = m
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|x| x["kind"] == json!(c.miner) && x["availableHere"] != false);
                if !ok {
                    return Some("miner_unavailable_for_platform".into());
                }
            }
        }
        None
    }

    /// Apply a profile (or an ad-hoc one: change pool, wallet…) to devices. Each computer
    /// validates the result itself; if mining won't start with it, its previous
    /// configuration is put back.
    pub fn apply(&self, p: &Profile, devs: &[DeviceRec]) -> Vec<OpResult> {
        let rows: HashMap<String, DeviceRow> =
            self.rows().into_iter().map(|r| (r.id.clone(), r)).collect();
        let res = self.for_each(devs, |d| {
            if let Some(why) = self.incompatible(d, p, rows.get(&d.id)) {
                return Ok(json!({"state": "skipped", "error": why}));
            }
            if d.kind == DeviceKind::Asic {
                let a = p.asic.as_ref().ok_or("profile_has_no_asic_pool")?;
                let spec = templated(&json!(a), &d.name);
                let a: AsicPoolSpec = serde_json::from_value(spec).map_err(|e| e.to_string())?;
                return self.asic_action(
                    d,
                    &AsicAction::SetPool {
                        url: a.url,
                        user: a.user,
                        password: a.password,
                    },
                );
            }
            let before = self.computer(d, "mining.config", &Value::Null)?;
            let mut next = before.clone();
            merge(&mut next, &templated(&p.config, &d.name));
            self.computer(d, "mining.config.set", &json!({"config": next}))?;
            if p.start {
                if let Err(e) = self.computer(d, "mining.restart", &Value::Null) {
                    // Last known good configuration back, then report.
                    let _ = self.computer(d, "mining.config.set", &json!({"config": before}));
                    return Ok(json!({"state": "rolled_back", "error": e}));
                }
            }
            Ok(json!({"state": "completed"}))
        });
        for r in &res {
            let level = if r.state == "completed" {
                "info"
            } else {
                "warning"
            };
            self.event(
                Some(&r.id),
                level,
                &format!("profile_{}", r.state),
                Some(&format!(
                    "{}{}",
                    p.name,
                    r.error
                        .as_ref()
                        .map(|e| format!(": {e}"))
                        .unwrap_or_default()
                )),
            );
        }
        res
    }

    pub fn config(&self, id: &str) -> Result<Value, String> {
        let d = self.device(id)?;
        if d.kind == DeviceKind::Asic {
            return Err("asic_configure_via_pool".into());
        }
        self.computer(&d, "mining.config", &Value::Null)
    }

    pub fn set_config(&self, id: &str, config: &Value) -> Result<Value, String> {
        let d = self.device(id)?;
        let r = self.computer(&d, "mining.config.set", &json!({"config": config}));
        self.event(
            Some(&d.id),
            if r.is_ok() { "info" } else { "warning" },
            "config_changed",
            r.as_ref().err().map(String::as_str),
        );
        r
    }

    /// Read-only commands for one device: logs, coins, stats, miners, telemetry.
    pub fn query(&self, id: &str, cmd: &str, args: &Value) -> Result<Value, String> {
        let d = self.device(id)?;
        if d.kind == DeviceKind::Asic {
            return match cmd {
                "telemetry" => self
                    .rows()
                    .into_iter()
                    .find(|r| r.id == d.id)
                    .map(|r| json!(r))
                    .ok_or("device_not_found".into()),
                "asic" => Ok(json!(self.asic_status(&d))),
                _ => Err("unsupported_for_asic".into()),
            };
        }
        match cmd {
            "logs" | "coins" | "stats" | "miners" | "telemetry" | "controllers" | "audit"
            | "hardware" => {
                if d.id == LOCAL_ID && matches!(cmd, "controllers" | "audit") {
                    return Ok(json!(crate::remote::audit_tail(&self.data_dir, 200)));
                }
                if d.id == LOCAL_ID && cmd == "hardware" {
                    return self.local_call("detect_hardware", args);
                }
                self.computer(&d, cmd, args)
            }
            _ => Err("unknown_command".into()),
        }
    }

    fn asic_status(&self, d: &DeviceRec) -> Option<AsicStatus> {
        let l = self.live();
        match &d.gateway {
            Some(g) => l
                .get(g)?
                .gateway_asics
                .iter()
                .find(|a| a.id == d.id)
                .cloned(),
            None => l.get(&d.id)?.asic.clone(),
        }
    }

    // ---------- samples, history, events ----------

    fn sample(&self, rows: &[DeviceRow]) {
        let now = crate::now_unix();
        let mut db = self.db();
        let Ok(tx) = db.transaction() else { return };
        for r in rows.iter().filter(|r| r.conn == Conn::Online) {
            let _ = tx.execute(
                "INSERT INTO samples (device_id, ts, span, algo, hashrate, power_w, power_measured, temp_c, revenue_day, cost_day, currency, mining)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![r.id, now, SAMPLE_EVERY, r.algo, r.hashrate, r.power_w, r.power_measured, r.temp_c, r.revenue_day, r.cost_day, r.currency, r.mining == Mining::Mining],
            );
            let _ = tx.execute(
                "UPDATE devices SET last_seen = ?2 WHERE id = ?1",
                params![r.id, now],
            );
        }
        let _ = tx.commit();
    }

    /// Average old samples into 15-minute rows and drop what is past the retention.
    pub fn compact(&self) -> Result<(), String> {
        let now = crate::now_unix();
        let keep = self.settings().retention_days * 86_400;
        let cut = (now - COMPACT_AFTER) / COMPACT_SPAN * COMPACT_SPAN;
        let mut db = self.db();
        let tx = db.transaction().map_err(db_err)?;
        tx.execute(
            "INSERT INTO samples (device_id, ts, span, algo, hashrate, power_w, power_measured, temp_c, revenue_day, cost_day, currency, mining)
             SELECT device_id, (ts / ?2) * ?2, ?2, MAX(algo), AVG(hashrate), AVG(power_w), MAX(power_measured), MAX(temp_c),
                    AVG(revenue_day), AVG(cost_day), MAX(currency), MAX(mining)
             FROM samples WHERE ts < ?1 AND span < ?2 GROUP BY device_id, ts / ?2",
            params![cut, COMPACT_SPAN],
        )
        .map_err(db_err)?;
        tx.execute(
            "DELETE FROM samples WHERE ts < ?1 AND span < ?2",
            params![cut, COMPACT_SPAN],
        )
        .map_err(db_err)?;
        tx.execute("DELETE FROM samples WHERE ts < ?1", params![now - keep])
            .map_err(db_err)?;
        tx.execute("DELETE FROM events WHERE ts < ?1", params![now - keep])
            .map_err(db_err)?;
        tx.commit().map_err(db_err)
    }

    /// Time series for one device or the whole fleet: power, money and temperature, and
    /// hashrate per algorithm. Buckets fit the range (1 h → 1 min … 30 d → 6 h).
    pub fn history(&self, device: Option<&str>, range_secs: i64) -> Result<Value, String> {
        let bucket = if range_secs <= 3_600 {
            60
        } else if range_secs <= 86_400 {
            900
        } else if range_secs <= 604_800 {
            7_200
        } else {
            21_600
        };
        let from = crate::now_unix() - range_secs;
        let dev = device.map(|d| self.device(d).map(|d| d.id)).transpose()?;
        let db = self.db();
        // Per device and bucket first (averages), then summed across devices.
        let inner = "SELECT device_id, ts / ?2 * ?2 AS b, algo, AVG(hashrate) h, AVG(power_w) p, MAX(temp_c) t,
                            AVG(revenue_day) r, AVG(cost_day) c
                     FROM samples WHERE ts >= ?1 AND (?3 IS NULL OR device_id = ?3) GROUP BY device_id, b";
        let mut st = db
            .prepare(&format!(
                "SELECT b, SUM(p), MAX(t), SUM(r), SUM(c) FROM ({inner}) GROUP BY b ORDER BY b"
            ))
            .map_err(db_err)?;
        let points: Vec<Value> = st
            .query_map(params![from, bucket, dev], |r| {
                Ok(json!({"ts": r.get::<_, i64>(0)?, "powerW": r.get::<_, Option<f64>>(1)?, "tempMax": r.get::<_, Option<f64>>(2)?,
                    "revenueDay": r.get::<_, Option<f64>>(3)?, "costDay": r.get::<_, Option<f64>>(4)?}))
            })
            .map_err(db_err)?
            .flatten()
            .collect();
        let mut st = db
            .prepare(&format!("SELECT algo, b, SUM(h) FROM ({inner}) WHERE algo IS NOT NULL AND h IS NOT NULL GROUP BY algo, b ORDER BY b"))
            .map_err(db_err)?;
        let mut by_algo: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for (a, b, h) in st
            .query_map(params![from, bucket, dev], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, f64>(2)?,
                ))
            })
            .map_err(db_err)?
            .flatten()
        {
            by_algo
                .entry(a)
                .or_default()
                .push(json!({"ts": b, "hashrate": h}));
        }
        Ok(json!({"bucketSecs": bucket, "points": points, "hashrate": by_algo}))
    }

    /// Demo registries only: `days` of made-up 15-minute samples around each device's
    /// current values, so the demo's charts aren't empty. Never called on a real registry.
    pub fn seed_demo_history(&self, days: i64) -> Result<(), String> {
        let rows = self.rows();
        let now = crate::now_unix() / COMPACT_SPAN * COMPACT_SPAN;
        let mut db = self.db();
        let tx = db.transaction().map_err(db_err)?;
        for (n, r) in rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.simulated || r.id == LOCAL_ID)
        {
            let (h, p, t) = match r.kind {
                DeviceKind::Asic => (
                    r.hashrate.unwrap_or(95e12),
                    r.power_w.unwrap_or(3250.0),
                    r.temp_c.unwrap_or(75.0) as f64,
                ),
                DeviceKind::GpuRig => (r.hashrate.unwrap_or(2400.0), 650.0, 62.0),
                _ => (
                    r.hashrate.unwrap_or(2400.0),
                    r.power_w.unwrap_or(140.0),
                    58.0,
                ),
            };
            let algo = r.algo.clone().unwrap_or_else(|| {
                if r.kind == DeviceKind::Asic {
                    "sha256d".into()
                } else {
                    "rx/0".into()
                }
            });
            let rev = r
                .revenue_day
                .unwrap_or(if r.kind == DeviceKind::Asic { 4.0 } else { 0.3 });
            for i in 1..=(days * 86_400 / COMPACT_SPAN) {
                let ts = now - i * COMPACT_SPAN;
                // Daily rhythm plus a per-device phase; one device rests at night.
                let x = (ts as f64 / 86_400.0 * std::f64::consts::TAU) + n as f64;
                let w = 1.0 + 0.04 * x.sin();
                let resting = n % 4 == 1 && (ts / 3600) % 24 < 7;
                let price = 0.25 * 24.0 / 1000.0;
                tx.execute(
                    "INSERT INTO samples (device_id, ts, span, algo, hashrate, power_w, power_measured, temp_c, revenue_day, cost_day, currency, mining)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'EUR', ?11)",
                    params![
                        r.id, ts, COMPACT_SPAN, algo,
                        (!resting).then_some(h * w),
                        (!resting).then_some(p * w),
                        r.kind == DeviceKind::Asic,
                        t + 3.0 * x.sin() - if resting { 15.0 } else { 0.0 },
                        (!resting).then_some(rev * w),
                        (!resting).then_some(p * w * price),
                        !resting
                    ],
                )
                .map_err(db_err)?;
            }
        }
        tx.commit().map_err(db_err)
    }

    pub fn event(&self, device: Option<&str>, level: &str, code: &str, detail: Option<&str>) {
        let _ = self.db().execute(
            "INSERT INTO events (ts, device_id, level, code, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![crate::now_unix(), device, level, code, detail],
        );
    }

    pub fn events(&self, since_id: i64, limit: i64) -> Vec<Event> {
        let db = self.db();
        let Ok(mut st) = db.prepare(
            "SELECT e.id, e.ts, e.device_id, d.name, e.level, e.code, e.detail FROM events e
             LEFT JOIN devices d ON d.id = e.device_id WHERE e.id > ?1 ORDER BY e.id DESC LIMIT ?2",
        ) else {
            return vec![];
        };
        st.query_map(params![since_id, limit.clamp(1, 1000)], |r| {
            Ok(Event {
                id: r.get(0)?,
                ts: r.get(1)?,
                device_id: r.get(2)?,
                device_name: r.get(3)?,
                level: r.get(4)?,
                code: r.get(5)?,
                detail: r.get(6)?,
            })
        })
        .into_iter()
        .flatten()
        .flatten()
        .collect()
    }

    /// Raise an alert unless the same one fired for this device within the cooldown.
    fn alert(&self, id: &str, level: &str, code: &str, detail: Option<String>, cooldown: i64) {
        let now = crate::now_unix();
        let key = (id.to_string(), code.to_string());
        let mut a = self.alerted.lock().unwrap_or_else(|p| p.into_inner());
        if a.get(&key).is_some_and(|t| now - t < cooldown) {
            return;
        }
        a.insert(key, now);
        drop(a);
        self.event(Some(id), level, code, detail.as_deref());
    }

    fn check_alerts(&self, rows: &[DeviceRow]) {
        let s = self.settings();
        let cd = s.cooldown_minutes * 60;
        let mine = env!("CARGO_PKG_VERSION");
        let mut prev = self.prev.lock().unwrap_or_else(|p| p.into_inner());
        for r in rows {
            let (pc, pm, ph, pp) = prev
                .get(&r.id)
                .copied()
                .unwrap_or((r.conn, r.mining, 0.0, 0.0));
            if pc == Conn::Online && r.conn != Conn::Online && r.conn != Conn::Connecting {
                self.alert(&r.id, "warning", "device_offline", r.error.clone(), cd);
            }
            if pc != Conn::Online
                && pc != Conn::Connecting
                && r.conn == Conn::Online
                && prev.contains_key(&r.id)
            {
                self.alert(&r.id, "info", "device_reconnected", None, 0);
            }
            if r.conn == Conn::Unauthorized {
                self.alert(&r.id, "error", "auth_failed", r.error.clone(), cd);
            }
            if pm == Mining::Mining && r.mining == Mining::Error {
                self.alert(&r.id, "error", "miner_crashed", r.error.clone(), cd);
            } else if pm == Mining::Mining && r.mining == Mining::Idle && r.conn == Conn::Online {
                self.alert(&r.id, "warning", "miner_stopped", None, cd);
            }
            let limit = if r.kind == DeviceKind::Asic {
                s.asic_max_temp_c
            } else {
                s.max_temp_c
            };
            if let Some(t) = r.temp_c.filter(|t| *t >= limit) {
                self.alert(
                    &r.id,
                    "warning",
                    "temperature_high",
                    Some(format!("{t:.0} °C")),
                    cd,
                );
            }
            // Baselines: slow moving averages while mining.
            let (mut bh, mut bp) = (ph, pp);
            if r.mining == Mining::Mining {
                if let Some(h) = r.hashrate {
                    if bh > 0.0 && h < bh * (1.0 - s.hashrate_drop_percent / 100.0) {
                        self.alert(
                            &r.id,
                            "warning",
                            "hashrate_low",
                            Some(format!("{:.0}%", h / bh * 100.0)),
                            cd,
                        );
                    }
                    bh = if bh == 0.0 { h } else { bh * 0.95 + h * 0.05 };
                }
                if let Some(p) = r.power_w {
                    if bp > 0.0 && p > bp * (1.0 + s.power_rise_percent / 100.0) {
                        self.alert(
                            &r.id,
                            "warning",
                            "power_high",
                            Some(format!("{p:.0} W")),
                            cd,
                        );
                    }
                    bp = if bp == 0.0 { p } else { bp * 0.95 + p * 0.05 };
                }
                let shares = r.accepted + r.rejected;
                if shares >= 20 && r.rejected as f64 / shares as f64 * 100.0 > s.rejected_percent {
                    self.alert(
                        &r.id,
                        "warning",
                        "rejected_high",
                        Some(format!("{}/{}", r.rejected, shares)),
                        cd,
                    );
                }
                if r.kind != DeviceKind::Asic && r.pool.is_none() {
                    self.alert(&r.id, "warning", "pool_unreachable", None, cd);
                }
            }
            if let Some(v) = &r.version {
                if r.kind != DeviceKind::Asic && newer(mine, v) {
                    self.alert(
                        &r.id,
                        "info",
                        "update_available",
                        Some(format!("{v} → {mine}")),
                        7 * 86_400,
                    );
                }
            }
            prev.insert(r.id.clone(), (r.conn, r.mining, bh, bp));
        }
    }

    // ---------- export / import ----------

    /// Groups, profiles, devices and settings as JSON. No private key is ever exported;
    /// ASIC pool passwords only with `secrets`.
    pub fn export(&self, secrets: bool) -> Value {
        let groups = self.groups();
        let gname = |id: Option<i64>| {
            id.and_then(|i| groups.iter().find(|g| g.id == i))
                .map(|g| g.name.clone())
        };
        let profiles: Vec<Profile> = self
            .profiles()
            .into_iter()
            .map(|mut p| {
                if !secrets {
                    if let Some(a) = &mut p.asic {
                        a.password.clear();
                    }
                }
                p
            })
            .collect();
        json!({
            "erebor": "controller-export", "version": 1, "exportedAt": crate::now_unix(),
            "settings": self.settings(),
            "groups": groups,
            "profiles": profiles,
            "devices": self.devices().iter().filter(|d| d.id != LOCAL_ID).map(|d| json!({
                "id": d.id, "kind": d.kind, "name": d.name, "address": d.address,
                "fingerprint": d.fingerprint, "gateway": d.gateway, "group": gname(d.group),
                "tags": d.tags, "energyPrice": d.energy_price,
            })).collect::<Vec<_>>(),
        })
    }

    /// Merge an export into this registry (existing names and ids are kept). Paired agents
    /// trust the certificate of the installation that paired them: on another computer
    /// they show as unauthorized until paired again.
    pub fn import(&self, v: &Value) -> Result<Value, String> {
        if v["erebor"] != "controller-export" {
            return Err("not_an_export".into());
        }
        if let Ok(s) = serde_json::from_value::<Settings>(v["settings"].clone()) {
            self.set_settings(&s)?;
        }
        let mut added = 0;
        for g in v["groups"].as_array().into_iter().flatten() {
            if let Ok(g) = serde_json::from_value::<Group>(g.clone()) {
                if self.save_group(&Group { id: 0, ..g }).is_ok() {
                    added += 1;
                }
            }
        }
        for p in v["profiles"].as_array().into_iter().flatten() {
            if let Ok(p) = serde_json::from_value::<Profile>(p.clone()) {
                if self.save_profile(&Profile { id: 0, ..p }).is_ok() {
                    added += 1;
                }
            }
        }
        // Gateways before the ASICs behind them.
        let mut devs: Vec<&Value> = v["devices"].as_array().into_iter().flatten().collect();
        devs.sort_by_key(|d| d["gateway"].is_string());
        for d in devs {
            let kind: DeviceKind =
                serde_json::from_value(d["kind"].clone()).map_err(|_| "bad_device")?;
            let rec = DeviceRec {
                id: d["id"].as_str().ok_or("bad_device")?.into(),
                kind,
                name: d["name"]
                    .as_str()
                    .filter(|n| valid_name(n))
                    .ok_or("bad_device")?
                    .into(),
                address: d["address"].as_str().ok_or("bad_device")?.into(),
                fingerprint: d["fingerprint"].as_str().map(str::to_string),
                gateway: d["gateway"].as_str().map(str::to_string),
                group: None,
                tags: vec![],
                energy_price: d["energyPrice"].as_f64(),
                added_at: crate::now_unix(),
                last_seen: None,
            };
            if kind == DeviceKind::LocalPc || self.insert(&rec).is_err() {
                continue;
            }
            added += 1;
            if let Some(g) = d["group"].as_str().and_then(|g| self.group_id(g).ok()) {
                let _ = self.assign_group(std::slice::from_ref(&rec.id), Some(g));
            }
            let tags: Vec<String> = serde_json::from_value(d["tags"].clone()).unwrap_or_default();
            let _ = self.tag(std::slice::from_ref(&rec.id), &tags, true);
            let _ = self.set_energy_price(&rec.id, rec.energy_price);
        }
        Ok(json!({"added": added}))
    }
}

fn code(v: Value) -> String {
    match v {
        Value::String(s) => s,
        v => v.to_string(),
    }
}

fn conn_for(e: &str) -> Conn {
    match e {
        "unauthorized" | "fingerprint_mismatch" | "not_paired" => Conn::Unauthorized,
        "address_not_found" | "tls_failed" | "protocol_incompatible" | "bad_response" => {
            Conn::Unreachable
        }
        _ => Conn::Offline,
    }
}

/// LAN discovery, with whether each agent is already in the registry.
pub fn discover(f: &Fleet) -> Result<Value, String> {
    if !f.settings().discovery {
        return Err("discovery_disabled".into());
    }
    let known: Vec<String> = f.devices().into_iter().map(|d| d.id).collect();
    let found = client::discover(remote::PORT, Duration::from_millis(1500))?;
    Ok(json!(found
        .into_iter()
        .map(|x| {
            let paired = known.contains(&format!("agent:{}", x.id));
            json!({"found": x, "paired": paired})
        })
        .collect::<Vec<_>>()))
}

fn arg<T: for<'de> Deserialize<'de>>(args: &Value, k: &str) -> Result<T, Value> {
    serde_json::from_value(args.get(k).cloned().unwrap_or(Value::Null))
        .map_err(|_| Value::from(format!("bad_argument:{k}")))
}

/// The GUI's fleet commands (`fleet_*`), same shape as `rpc::call`.
pub fn call(f: &Fleet, cmd: &str, args: &Value) -> Result<Value, Value> {
    let ids = || -> Result<Vec<DeviceRec>, Value> {
        let ids: Vec<String> = arg(args, "ids")?;
        f.select(&ids, None, false).map_err(Value::from)
    };
    let r: Result<Value, String> = match cmd {
        "fleet_list" => {
            let rows = f.rows();
            Ok(
                json!({"overview": f.overview(&rows), "devices": rows, "groups": f.groups(), "identity": remote::short_fp(&f.identity.fingerprint)}),
            )
        }
        "fleet_refresh" => {
            f.refresh_once();
            Ok(Value::Null)
        }
        "fleet_discover" => discover(f),
        "fleet_add_agent" => f
            .add_agent(
                &arg::<String>(args, "address")?,
                &arg::<String>(args, "code")?,
                arg(args, "kind").ok(),
            )
            .map(|d| json!(d)),
        "fleet_add_asic" => f
            .add_asic(
                &arg::<String>(args, "host")?,
                arg::<Option<u16>>(args, "port")?.unwrap_or(0),
                &arg::<Option<String>>(args, "name")?.unwrap_or_default(),
                arg::<Option<String>>(args, "via")?.as_deref(),
            )
            .map(|d| json!(d)),
        "fleet_scan" => {
            let cidr: String = arg(args, "cidr")?;
            match arg::<Option<String>>(args, "via")? {
                Some(g) => f
                    .device(&g)
                    .and_then(|gw| f.agent_call(&gw, "asics.scan", &json!({"cidr": cidr}))),
                None => asic::scan(&cidr, 0).map(|v| json!(v)),
            }
        }
        "fleet_remove" => f.remove(&arg::<String>(args, "id")?).map(|_| Value::Null),
        "fleet_rename" => f
            .rename(&arg::<String>(args, "id")?, &arg::<String>(args, "name")?)
            .map(|_| Value::Null),
        "fleet_set_kind" => f
            .set_kind(&arg::<String>(args, "id")?, arg(args, "kind")?)
            .map(|_| Value::Null),
        "fleet_set_price" => f
            .set_energy_price(&arg::<String>(args, "id")?, arg(args, "price")?)
            .map(|_| Value::Null),
        "fleet_mining" => Ok(json!(f.mining(&ids()?, &arg::<String>(args, "action")?))),
        "fleet_apply" => {
            let p = match args.get("profile") {
                Some(Value::Object(_)) => arg::<Profile>(args, "profile")?,
                _ => f
                    .profile(&arg::<String>(args, "profileId")?)
                    .map_err(Value::from)?,
            };
            Ok(json!(f.apply(&p, &ids()?)))
        }
        "fleet_assign_group" => f
            .assign_group(&arg::<Vec<String>>(args, "ids")?, arg(args, "group")?)
            .map(|n| json!(n)),
        "fleet_tag" => f
            .tag(
                &arg::<Vec<String>>(args, "ids")?,
                &arg::<Vec<String>>(args, "tags")?,
                arg::<Option<bool>>(args, "remove")? != Some(true),
            )
            .map(|_| Value::Null),
        "fleet_groups" => Ok(json!(f.groups())),
        "fleet_save_group" => f.save_group(&arg(args, "group")?).map(|g| json!(g)),
        "fleet_delete_group" => f.delete_group(arg(args, "id")?).map(|_| Value::Null),
        "fleet_profiles" => Ok(json!(f.profiles())),
        "fleet_save_profile" => f.save_profile(&arg(args, "profile")?).map(|p| json!(p)),
        "fleet_delete_profile" => f.delete_profile(arg(args, "id")?).map(|_| Value::Null),
        "fleet_config" => f.config(&arg::<String>(args, "id")?),
        "fleet_set_config" => {
            f.set_config(&arg::<String>(args, "id")?, &arg::<Value>(args, "config")?)
        }
        "fleet_query" => f.query(
            &arg::<String>(args, "id")?,
            &arg::<String>(args, "cmd")?,
            args.get("args").unwrap_or(&Value::Null),
        ),
        "fleet_history" => f.history(
            arg::<Option<String>>(args, "id")?.as_deref(),
            arg(args, "rangeSecs")?,
        ),
        "fleet_events" => Ok(json!(f.events(
            arg::<Option<i64>>(args, "sinceId")?.unwrap_or(0),
            arg::<Option<i64>>(args, "limit")?.unwrap_or(200)
        ))),
        "fleet_settings" => Ok(json!(f.settings())),
        "fleet_set_settings" => f
            .set_settings(&arg(args, "settings")?)
            .map(|_| json!(f.settings())),
        "fleet_export" => Ok(f.export(arg::<Option<bool>>(args, "secrets")?.unwrap_or(false))),
        "fleet_import" => f.import(&arg::<Value>(args, "data")?),
        _ => return Err("unknown_command".into()),
    };
    r.map_err(Value::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asic_sim::{SimAsic, SimKind};
    use crate::engine::{self, Engine};
    use crate::remote::server::{self, Server};
    use crate::remote::RemoteConfig;
    use std::time::Instant;

    fn agent(dir: &Path, demo: bool) -> (Server, String) {
        std::fs::write(dir.join("config.json"), format!(r#"{{"demoMode":{demo}}}"#)).unwrap();
        let e = Engine::new(dir.into(), engine::default_factory()).unwrap();
        let cfg = RemoteConfig {
            enabled: true,
            listen: "127.0.0.1:0".into(),
            discovery: false,
            asics: vec![],
        };
        let s = server::serve(Arc::new(Mutex::new(e)), &cfg).unwrap();
        let code = remote::start_pairing(dir).unwrap().code;
        (s, code)
    }

    fn controller(dir: &Path) -> Arc<Fleet> {
        std::fs::write(
            dir.join("config.json"),
            r#"{"demoMode":true,"onboardingCompleted":true}"#,
        )
        .unwrap();
        let e: crate::rpc::Shared = Arc::new(Mutex::new(
            Engine::new(dir.into(), engine::default_factory()).unwrap(),
        ));
        let local: LocalCall = Arc::new(move |c, a| crate::rpc::call(&e, c, a));
        Fleet::open(dir, Some(local)).unwrap()
    }

    #[test]
    fn manages_agents_asics_and_this_pc_together() {
        let (c, a1, a2, a3) = (
            tempfile::tempdir().unwrap(),
            tempfile::tempdir().unwrap(),
            tempfile::tempdir().unwrap(),
            tempfile::tempdir().unwrap(),
        );
        let f = controller(c.path());
        let (s1, code1) = agent(a1.path(), true);
        let (s2, code2) = agent(a2.path(), true);
        let (gw, code3) = agent(a3.path(), true);

        let d1 = f
            .add_agent(&s1.addr.to_string(), &code1, Some(DeviceKind::GpuRig))
            .unwrap();
        assert_eq!(d1.kind, DeviceKind::GpuRig);
        let d2 = f.add_agent(&s2.addr.to_string(), &code2, None).unwrap();
        let g = f
            .add_agent(&gw.addr.to_string(), &code3, Some(DeviceKind::Server))
            .unwrap();
        assert_eq!(
            f.add_agent(&s1.addr.to_string(), &code1, None).err(),
            Some("pairing_not_active".into())
        );

        // ASICs: one direct, one through the gateway; the same ASIC can't be added twice.
        let direct = SimAsic::start(SimKind::Antminer { privileged: true }).unwrap();
        let behind = SimAsic::start(SimKind::Whatsminer).unwrap();
        let a = f.add_asic("127.0.0.1", direct.port, "s19", None).unwrap();
        assert_eq!(
            f.add_asic("127.0.0.1", direct.port, "again", Some(&g.id))
                .err(),
            Some("device_exists".into())
        );
        let w = f
            .add_asic("127.0.0.1", behind.port, "", Some(&g.id))
            .unwrap();
        assert!(w.name.contains("M30S+"), "{}", w.name);
        assert_eq!(
            f.add_asic("8.8.8.8", 4028, "x", None).err(),
            Some("asic_address_not_private".into())
        );

        f.refresh_once();
        let rows = f.rows();
        assert_eq!(rows.len(), 6, "local + 3 agents + 2 ASICs");
        assert_eq!(rows[0].id, LOCAL_ID);
        assert!(rows.iter().all(|r| r.conn == Conn::Online), "{rows:#?}");
        assert!(
            rows.iter().all(|r| r.simulated),
            "demo agents and simulators are flagged"
        );
        let wr = rows.iter().find(|r| r.id == w.id).unwrap();
        assert_eq!(wr.gateway.as_deref(), Some(g.id.as_str()));
        assert!(
            !wr.actions.restart.supported
                && wr.actions.restart.reason.as_deref()
                    == Some("whatsminer_write_api_not_implemented")
        );

        // Mining on a selection, verified per device; ASICs can't start/stop.
        let res = f.mining(
            &f.select(&[d1.id.clone(), d2.id.clone(), a.id.clone()], None, false)
                .unwrap(),
            "start",
        );
        assert_eq!(
            res.iter().map(|r| r.state.as_str()).collect::<Vec<_>>(),
            ["completed", "completed", "failed"]
        );
        assert_eq!(res[2].error.as_deref(), Some("asic_no_stop_command"));
        f.refresh_once();
        let rows = f.rows();
        let ov = f.overview(&rows);
        assert_eq!(ov["mining"], 4, "2 rigs + 2 ASICs");
        // Hashrate is never summed across algorithms.
        let mut cpu = rows[0].clone();
        (cpu.algo, cpu.hashrate) = (Some("rx/0".into()), Some(5_000.0));
        let mut mixed = rows.clone();
        mixed.push(cpu);
        let ov2 = f.overview(&mixed);
        let by: HashMap<&str, f64> = ov2["hashrate"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| (h["algo"].as_str().unwrap(), h["hashrate"].as_f64().unwrap()))
            .collect();
        let asics: f64 = rows
            .iter()
            .filter(|r| r.kind == DeviceKind::Asic)
            .filter_map(|r| r.hashrate)
            .sum();
        assert_eq!((by["sha256d"], by["rx/0"]), (asics, 5_000.0), "{by:?}");

        // One device down: the others still answer, the failure is reported for it alone.
        f.check_alerts(&rows);
        drop(s2);
        std::thread::sleep(Duration::from_millis(300));
        let res = f.mining(
            &f.select(&[], None, true)
                .unwrap()
                .into_iter()
                .filter(|d| d.kind != DeviceKind::Asic)
                .collect::<Vec<_>>(),
            "stop",
        );
        let failed: Vec<&str> = res
            .iter()
            .filter(|r| r.state == "failed")
            .map(|r| r.id.as_str())
            .collect();
        assert_eq!(failed, [d2.id.as_str()]);
        f.refresh_once();
        f.check_alerts(&f.rows());
        assert!(f
            .events(0, 100)
            .iter()
            .any(|e| e.code == "device_offline" && e.device_id.as_deref() == Some(d2.id.as_str())));

        // Groups, tags, profiles applied to a group (computers and ASICs).
        let grp = f
            .save_group(&Group {
                id: 0,
                name: "Home mining".into(),
                energy_price: Some(0.30),
            })
            .unwrap();
        assert_eq!(
            f.save_group(&Group {
                id: 0,
                name: "home MINING".into(),
                energy_price: None
            })
            .err(),
            Some("group_exists".into())
        );
        f.assign_group(&[d1.id.clone(), a.id.clone()], Some(grp.id))
            .unwrap();
        f.tag(
            std::slice::from_ref(&d1.id),
            &["gpu".into(), "office".into()],
            true,
        )
        .unwrap();
        assert_eq!(f.device(&d1.id).unwrap().tags, ["gpu", "office"]);
        assert!(f
            .tag(std::slice::from_ref(&d1.id), &["bad|tag".into()], true)
            .is_err());
        let p = f
            .save_profile(&Profile {
                name: "btc-pool".into(),
                config: json!({"pool": {"worker": "{device}"}, "performance": {"preset": "eco"}}),
                asic: Some(AsicPoolSpec {
                    url: "stratum+tcp://btc.example:3333".into(),
                    user: "bc1qexample.{device}".into(),
                    password: String::new(),
                }),
                ..Default::default()
            })
            .unwrap();
        assert!(f
            .save_profile(&Profile {
                name: "x".into(),
                config: json!({"miner": {"path": "/bin/sh"}}),
                ..Default::default()
            })
            .unwrap_err()
            .starts_with("profile_key_not_allowed"));
        let res = f.apply(&p, &f.select(&[], Some("home mining"), false).unwrap());
        assert!(res.iter().all(|r| r.state == "completed"), "{res:#?}");
        let cfg = f.config(&d1.id).unwrap();
        assert_eq!(cfg["performance"]["preset"], "eco");
        assert!(!cfg["pool"]["worker"].as_str().unwrap().is_empty());
        let st = asic::read(&direct.entry(""));
        assert!(st
            .pools
            .iter()
            .any(|p| p.active && p.user == "bc1qexample.s19"));

        // Energy price from the group wins over the agent's own.
        f.mining(
            &f.select(std::slice::from_ref(&d1.id), None, false).unwrap(),
            "start",
        );
        f.refresh_once();
        let r1 = f.rows().into_iter().find(|r| r.id == d1.id).unwrap();
        if let Some(w) = r1.power_w {
            assert!((r1.cost_day.unwrap() - w * 24.0 / 1000.0 * 0.30).abs() < 1e-9);
        }

        // History and retention.
        f.sample(&f.rows());
        let h = f.history(None, 3600).unwrap();
        assert!(!h["points"].as_array().unwrap().is_empty());
        f.compact().unwrap();

        // Export without secrets, import elsewhere.
        let ex = f.export(false);
        assert_eq!(ex["devices"].as_array().unwrap().len(), 5);
        let other = tempfile::tempdir().unwrap();
        let f2 = Fleet::open(other.path(), None).unwrap();
        assert_eq!(
            f2.import(&ex).unwrap()["added"],
            7,
            "1 group, 1 profile, 5 devices"
        );
        assert_eq!(
            f2.device(&w.id).unwrap().gateway.as_deref(),
            Some(g.id.as_str())
        );

        // Removing a device revokes this controller on the agent.
        f.remove(&d1.id).unwrap();
        assert!(remote::controllers(a1.path()).is_empty());
        assert_eq!(f.remove(LOCAL_ID).err(), Some("cannot_remove_local".into()));
    }

    #[test]
    fn profile_rolls_back_when_mining_wont_start() {
        let (c, a) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let f = controller(c.path());
        // Real mode without a payout: the config saves, mining refuses to start.
        let (s, code) = agent(a.path(), false);
        let d = f.add_agent(&s.addr.to_string(), &code, None).unwrap();
        let before = f.config(&d.id).unwrap();
        let p = Profile {
            name: "eco".into(),
            config: json!({"performance": {"preset": "eco"}}),
            start: true,
            ..Default::default()
        };
        let r = f.apply(&p, std::slice::from_ref(&d));
        assert_eq!(r[0].state, "rolled_back");
        assert_eq!(r[0].error.as_deref(), Some("wallet_missing"));
        assert_eq!(
            f.config(&d.id).unwrap()["performance"],
            before["performance"]
        );
        // An invalid configuration is refused by the agent and changes nothing.
        let bad = Profile {
            name: "bad".into(),
            config: json!({"wallet": "not-a-wallet"}),
            ..Default::default()
        };
        assert_eq!(f.apply(&bad, std::slice::from_ref(&d))[0].state, "failed");
    }

    #[test]
    fn revoked_controller_shows_unauthorized() {
        let (c, a) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let f = controller(c.path());
        let (s, code) = agent(a.path(), true);
        let d = f.add_agent(&s.addr.to_string(), &code, None).unwrap();
        remote::revoke(a.path(), &f.identity.fingerprint).unwrap();
        f.refresh_once();
        // This computer is listed only once it is set up for mining.
        assert!(f.rows().iter().any(|r| r.id == LOCAL_ID));
        let mut c: crate::config::AppConfig =
            serde_json::from_value(f.local_call("get_config", &Value::Null).unwrap()).unwrap();
        (c.demo_mode, c.onboarding_completed) = (false, false);
        f.local_call("update_config", &json!({"config": c}))
            .unwrap();
        f.refresh_once();
        assert!(!f.rows().iter().any(|r| r.id == LOCAL_ID));
        let r = f.rows().into_iter().find(|r| r.id == d.id).unwrap();
        assert_eq!(r.conn, Conn::Unauthorized);
        assert_eq!(r.actions.start.reason.as_deref(), Some("unauthorized"));
    }

    #[test]
    fn migrations_are_versioned_and_reopen() {
        let d = tempfile::tempdir().unwrap();
        drop(Fleet::open(d.path(), None).unwrap());
        let f = Fleet::open(d.path(), None).unwrap();
        let v: i64 = f
            .db()
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v as usize, MIGRATIONS.len());
        // Referential integrity: deleting a gateway deletes the ASICs behind it.
        f.insert(&DeviceRec {
            id: "agent:g".into(),
            kind: DeviceKind::Server,
            name: "g".into(),
            address: "x".into(),
            fingerprint: None,
            gateway: None,
            group: None,
            tags: vec![],
            energy_price: None,
            added_at: 0,
            last_seen: None,
        })
        .unwrap();
        f.insert(&DeviceRec {
            id: "asic:10.0.0.1:4028".into(),
            kind: DeviceKind::Asic,
            name: "a".into(),
            address: "10.0.0.1:4028".into(),
            fingerprint: None,
            gateway: Some("agent:g".into()),
            group: None,
            tags: vec![],
            energy_price: None,
            added_at: 0,
            last_seen: None,
        })
        .unwrap();
        f.db()
            .execute("DELETE FROM devices WHERE id = 'agent:g'", [])
            .unwrap();
        assert!(f.devices().is_empty());
    }

    /// Scale check, run on demand: `cargo test -p erebor-core --release -- --ignored load --nocapture`.
    /// 200 simulated ASICs polled for real over TCP, then 500 devices' worth of samples.
    #[test]
    #[ignore]
    fn load_many_devices() {
        let d = tempfile::tempdir().unwrap();
        let f = Fleet::open(d.path(), None).unwrap();
        let sims: Vec<SimAsic> = (0..200)
            .map(|_| SimAsic::start(SimKind::Antminer { privileged: true }).unwrap())
            .collect();
        let t = Instant::now();
        for (i, s) in sims.iter().enumerate() {
            f.insert(&DeviceRec {
                id: s.entry("").id(),
                kind: DeviceKind::Asic,
                name: format!("s19-{i}"),
                address: format!("127.0.0.1:{}", s.port),
                fingerprint: None,
                gateway: None,
                group: None,
                tags: vec![],
                energy_price: None,
                added_at: 0,
                last_seen: None,
            })
            .unwrap();
        }
        let t_insert = t.elapsed();
        let t = Instant::now();
        f.refresh_once();
        let t_poll = t.elapsed();
        let t = Instant::now();
        let rows = f.rows();
        let ov = f.overview(&rows);
        let t_rows = t.elapsed();
        assert_eq!(ov["online"], 200);
        // 500 devices × 24 h of one-minute samples = 720 000 rows.
        let t = Instant::now();
        {
            let mut db = f.db();
            let tx = db.transaction().unwrap();
            for dev in 0..500 {
                let id = rows[dev % rows.len()].id.clone();
                for m in 0..1440i64 {
                    tx.execute("INSERT INTO samples (device_id, ts, span, algo, hashrate, power_w, power_measured, temp_c, revenue_day, cost_day, currency, mining) VALUES (?1, ?2, 60, 'sha256d', 9.5e13, 3250, 1, 70, 4.0, 2.0, 'EUR', 1)",
                        params![id, crate::now_unix() - 3 * 86_400 + m * 60 + dev as i64]).unwrap();
                }
            }
            tx.commit().unwrap();
        }
        let t_samples = t.elapsed();
        let t = Instant::now();
        let h = f.history(None, 30 * 86_400).unwrap();
        let t_hist = t.elapsed();
        let t = Instant::now();
        f.compact().unwrap();
        let t_compact = t.elapsed();
        let rows_after: i64 = f
            .db()
            .query_row("SELECT COUNT(*) FROM samples", [], |r| r.get(0))
            .unwrap();
        f.db().execute_batch("VACUUM").unwrap();
        let size: i64 = f
            .db()
            .query_row(
                "SELECT page_count * page_size FROM pragma_page_count(), pragma_page_size()",
                [],
                |r| r.get(0),
            )
            .unwrap();
        println!("insert 200 devices {t_insert:?} · poll 200 ASICs {t_poll:?} · rows+overview {t_rows:?}");
        println!("720k samples {t_samples:?} · 30-day history {t_hist:?} ({} points) · compact {t_compact:?} → {rows_after} rows, {:.1} MB", h["points"].as_array().unwrap().len(), size as f64 / 1e6);
        assert!(rows_after < 720_000 / 10, "compaction to 15-minute rows");
    }

    #[test]
    fn templating_merge_and_versions() {
        let mut b = json!({"pool": {"host": "a", "worker": "w"}, "coin": "AUTO"});
        merge(
            &mut b,
            &templated(
                &json!({"pool": {"worker": "{device}"}, "coin": "XMR"}),
                "rig 01!",
            ),
        );
        assert_eq!(
            b,
            json!({"pool": {"host": "a", "worker": "rig01"}, "coin": "XMR"})
        );
        assert!(newer("0.2.0", "0.1.9") && !newer("0.1.7", "0.1.7") && newer("0.1.10", "0.1.9"));
    }
}

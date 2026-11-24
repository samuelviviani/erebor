//! Local SQLite storage: 10-second samples, sessions and small key/value state.
//! Demo mode uses a separate database file, so simulated and real data never mix.

use crate::devfee::Target;
use crate::earnings::RangeTotals;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;

const MIGRATIONS: &[&str] = &[
    // v1
    "CREATE TABLE samples (
        ts INTEGER NOT NULL,
        secs REAL NOT NULL,
        hashrate REAL,
        target TEXT NOT NULL,
        accepted INTEGER NOT NULL DEFAULT 0,
        rejected INTEGER NOT NULL DEFAULT 0,
        xmr_per_hash REAL,
        power_w REAL,
        power_measured INTEGER NOT NULL DEFAULT 0,
        temp_c REAL
     );
     CREATE INDEX samples_ts ON samples(ts);
     CREATE TABLE sessions (
        id INTEGER PRIMARY KEY,
        started INTEGER NOT NULL,
        ended INTEGER,
        end_reason TEXT,
        miner TEXT NOT NULL,
        pool TEXT NOT NULL
     );
     CREATE TABLE kv (k TEXT PRIMARY KEY, v TEXT NOT NULL);",
    // v2: several coins. Value is tracked in BTC (net of pool and miner fees).
    "ALTER TABLE samples ADD COLUMN coin TEXT NOT NULL DEFAULT 'XMR';
     ALTER TABLE samples ADD COLUMN btc_per_hash REAL;",
    // v3: what a kWh cost when the sample was taken (bands, solar, heat credit).
    "ALTER TABLE samples ADD COLUMN price_kwh REAL;",
];

#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub ts: i64,
    pub secs: f64,
    pub hashrate: Option<f64>,
    pub target: Target,
    pub accepted: u64,
    pub rejected: u64,
    pub xmr_per_hash: Option<f64>,
    pub power_w: Option<f64>,
    pub power_measured: bool,
    pub temp_c: Option<f64>,
    pub coin: String,
    /// Net BTC per hash at sampling time (pool and miner fees already deducted).
    pub btc_per_hash: Option<f64>,
    /// Effective electricity price per kWh at sampling time, if the user set one.
    pub price_kwh: Option<f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SeriesPoint {
    pub ts: i64,
    pub hashrate: Option<f64>,
    pub user_xmr: f64,
    pub user_btc: f64,
    pub energy_wh: Option<f64>,
    pub temp_c: Option<f64>,
}

/// Coins mined for the user (estimated from the work done, before pool/miner fees).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CoinAmount {
    pub coin: String,
    pub amount: f64,
    pub secs: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct HashStats {
    pub avg: Option<f64>,
    pub max: Option<f64>,
    pub accepted: u64,
    pub rejected: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: i64,
    pub started: i64,
    pub ended: Option<i64>,
    pub end_reason: Option<String>,
    pub miner: String,
    pub pool: String,
}

pub struct Store {
    conn: Connection,
}

fn target_str(t: Target) -> &'static str {
    match t {
        Target::User => "user",
        Target::Project => "project",
    }
}

impl Store {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        Self::init(Connection::open(path)?)
    }

    #[cfg(test)]
    fn in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Self> {
        // journal_mode returns a row, hence the `_and_check` variant.
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |_| Ok(()))?;
        let mut s = Self { conn };
        s.migrate()?;
        Ok(s)
    }

    fn schema_version(&self) -> rusqlite::Result<usize> {
        self.conn
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .map(|v| v as usize)
    }

    fn migrate(&mut self) -> rusqlite::Result<()> {
        let current = self.schema_version()?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn insert_sample(&self, s: &Sample) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO samples (ts, secs, hashrate, target, accepted, rejected, xmr_per_hash, power_w, power_measured, temp_c, coin, btc_per_hash, price_kwh)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                s.ts,
                s.secs,
                s.hashrate,
                target_str(s.target),
                s.accepted as i64,
                s.rejected as i64,
                s.xmr_per_hash,
                s.power_w,
                s.power_measured,
                s.temp_c,
                s.coin,
                s.btc_per_hash,
                s.price_kwh
            ],
        )?;
        Ok(())
    }

    pub fn totals(&self, from: i64, to: i64) -> rusqlite::Result<RangeTotals> {
        self.conn.query_row(
            "SELECT
               COALESCE(SUM(CASE WHEN target='user' AND coin='XMR' THEN hashrate*secs*xmr_per_hash END), 0),
               COALESCE(SUM(CASE WHEN target='project' AND coin='XMR' THEN hashrate*secs*xmr_per_hash END), 0),
               COALESCE(SUM(CASE WHEN target='user' THEN secs END), 0),
               COALESCE(SUM(CASE WHEN target='project' THEN secs END), 0),
               COALESCE(SUM(power_w*secs), 0),
               COALESCE(SUM(CASE WHEN power_w IS NULL THEN secs END), 0),
               COALESCE(MIN(CASE WHEN power_w IS NOT NULL THEN power_measured END), 0),
               COALESCE(SUM(CASE WHEN coin='XMR' AND xmr_per_hash IS NULL AND hashrate > 0 THEN secs END), 0),
               COALESCE(SUM(CASE WHEN target='user' THEN hashrate*secs*btc_per_hash END), 0),
               COALESCE(SUM(CASE WHEN target='user' AND btc_per_hash IS NULL AND hashrate > 0 THEN secs END), 0),
               COALESCE(SUM(CASE WHEN target='user' AND coin <> 'XMR' THEN secs END), 0),
               COALESCE(SUM(power_w*secs*price_kwh), 0) / 3.6e6,
               COALESCE(SUM(CASE WHEN price_kwh IS NULL THEN power_w*secs END), 0)
             FROM samples WHERE ts >= ?1 AND ts < ?2",
            params![from, to],
            |r| {
                Ok(RangeTotals {
                    user_xmr: r.get(0)?,
                    project_xmr: r.get(1)?,
                    user_secs: r.get(2)?,
                    project_secs: r.get(3)?,
                    energy_joules: r.get(4)?,
                    unknown_power_secs: r.get(5)?,
                    power_all_measured: r.get::<_, i64>(6)? == 1,
                    unknown_reward_secs: r.get(7)?,
                    user_btc: r.get(8)?,
                    unknown_value_secs: r.get(9)?,
                    other_coin_secs: r.get(10)?,
                    priced_cost: r.get(11)?,
                    unpriced_joules: r.get(12)?,
                })
            },
        )
    }

    /// Per-coin amount mined for the user. `xmr_per_hash` holds the coin-per-hash rate
    /// of the sample's own coin (historical column name).
    pub fn coin_totals(&self, from: i64, to: i64) -> rusqlite::Result<Vec<CoinAmount>> {
        let mut st = self.conn.prepare(
            "SELECT coin, SUM(hashrate*secs*xmr_per_hash), SUM(secs) FROM samples
             WHERE ts >= ?1 AND ts < ?2 AND target='user' AND xmr_per_hash IS NOT NULL AND hashrate > 0
             GROUP BY coin ORDER BY SUM(secs) DESC",
        )?;
        let rows = st.query_map(params![from, to], |r| {
            Ok(CoinAmount {
                coin: r.get(0)?,
                amount: r.get(1)?,
                secs: r.get(2)?,
            })
        })?;
        rows.collect()
    }

    pub fn hash_stats(&self, from: i64, to: i64) -> rusqlite::Result<HashStats> {
        self.conn.query_row(
            "SELECT SUM(hashrate*secs) / NULLIF(SUM(CASE WHEN hashrate IS NOT NULL THEN secs END), 0),
                    MAX(hashrate), COALESCE(SUM(accepted), 0), COALESCE(SUM(rejected), 0)
             FROM samples WHERE ts >= ?1 AND ts < ?2",
            params![from, to],
            |r| {
                Ok(HashStats {
                    avg: r.get(0)?,
                    max: r.get(1)?,
                    accepted: r.get::<_, i64>(2)? as u64,
                    rejected: r.get::<_, i64>(3)? as u64,
                })
            },
        )
    }

    pub fn series(&self, from: i64, to: i64, bucket: i64) -> rusqlite::Result<Vec<SeriesPoint>> {
        let bucket = bucket.max(1);
        let mut st = self.conn.prepare(
            "SELECT (ts / ?3) * ?3 AS b,
                    SUM(hashrate*secs) / NULLIF(SUM(CASE WHEN hashrate IS NOT NULL THEN secs END), 0),
                    COALESCE(SUM(CASE WHEN target='user' AND coin='XMR' THEN hashrate*secs*xmr_per_hash END), 0),
                    COALESCE(SUM(CASE WHEN target='user' THEN hashrate*secs*btc_per_hash END), 0),
                    SUM(power_w*secs) / 3600.0,
                    AVG(temp_c)
             FROM samples WHERE ts >= ?1 AND ts < ?2 GROUP BY b ORDER BY b",
        )?;
        let rows = st.query_map(params![from, to, bucket], |r| {
            Ok(SeriesPoint {
                ts: r.get(0)?,
                hashrate: r.get(1)?,
                user_xmr: r.get(2)?,
                user_btc: r.get(3)?,
                energy_wh: r.get(4)?,
                temp_c: r.get(5)?,
            })
        })?;
        rows.collect()
    }

    /// Every sample in the range as CSV (local time), for spreadsheets and tax records.
    /// Amounts are estimates from hashrate and network data, like everywhere in the app.
    pub fn export_csv(&self, from: i64, to: i64) -> rusqlite::Result<String> {
        use chrono::TimeZone;
        let mut st = self.conn.prepare(
            "SELECT ts, secs, coin, target, hashrate, hashrate*secs*xmr_per_hash,
                    hashrate*secs*btc_per_hash, power_w*secs/3.6e6, price_kwh,
                    power_w*secs*price_kwh/3.6e6, accepted, rejected, temp_c
             FROM samples WHERE ts >= ?1 AND ts < ?2 ORDER BY ts",
        )?;
        let mut out = String::from(
            "time,seconds,coin,mined_for,hashrate_hs,coin_amount,btc_value,energy_kwh,price_per_kwh,energy_cost,accepted,rejected,temp_c\n",
        );
        let opt = |v: Option<f64>| v.map(|v| v.to_string()).unwrap_or_default();
        let mut rows = st.query(params![from, to])?;
        while let Some(r) = rows.next()? {
            let ts: i64 = r.get(0)?;
            let time = chrono::Local
                .timestamp_opt(ts, 0)
                .single()
                .map(|t| t.format("%Y-%m-%dT%H:%M:%S%:z").to_string())
                .unwrap_or_default();
            let coin: String = r.get(2)?;
            let target: String = r.get(3)?;
            out += &format!(
                "{time},{},{},{},{},{},{},{},{},{},{},{},{}\n",
                r.get::<_, f64>(1)?,
                coin.replace(',', ""),
                target,
                opt(r.get(4)?),
                opt(r.get(5)?),
                opt(r.get(6)?),
                opt(r.get(7)?),
                opt(r.get(8)?),
                opt(r.get(9)?),
                r.get::<_, i64>(10)?,
                r.get::<_, i64>(11)?,
                opt(r.get(12)?),
            );
        }
        Ok(out)
    }

    pub fn start_session(&self, ts: i64, miner: &str, pool: &str) -> rusqlite::Result<i64> {
        self.conn.execute(
            "INSERT INTO sessions (started, miner, pool) VALUES (?1, ?2, ?3)",
            params![ts, miner, pool],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn end_session(&self, id: i64, ts: i64, reason: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE sessions SET ended = ?2, end_reason = ?3 WHERE id = ?1 AND ended IS NULL",
            params![id, ts, reason],
        )?;
        Ok(())
    }

    /// Sessions left open by a crash of Erebor itself.
    pub fn close_dangling_sessions(&self, ts: i64) -> rusqlite::Result<usize> {
        self.conn.execute(
            "UPDATE sessions SET ended = ?1, end_reason = 'app_terminated' WHERE ended IS NULL",
            params![ts],
        )
    }

    pub fn sessions(&self, limit: u32) -> rusqlite::Result<Vec<Session>> {
        let mut st = self.conn.prepare(
            "SELECT id, started, ended, end_reason, miner, pool FROM sessions ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = st.query_map(params![limit], |r| {
            Ok(Session {
                id: r.get(0)?,
                started: r.get(1)?,
                ended: r.get(2)?,
                end_reason: r.get(3)?,
                miner: r.get(4)?,
                pool: r.get(5)?,
            })
        })?;
        rows.collect()
    }

    pub fn kv_get(&self, k: &str) -> rusqlite::Result<Option<String>> {
        self.conn
            .query_row("SELECT v FROM kv WHERE k = ?1", params![k], |r| r.get(0))
            .optional()
    }

    pub fn kv_set(&self, k: &str, v: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO kv (k, v) VALUES (?1, ?2) ON CONFLICT(k) DO UPDATE SET v = excluded.v",
            params![k, v],
        )?;
        Ok(())
    }

    pub fn prune(&self, before: i64) -> rusqlite::Result<usize> {
        self.conn
            .execute("DELETE FROM samples WHERE ts < ?1", params![before])
    }

    pub fn clear(&self) -> rusqlite::Result<()> {
        self.conn
            .execute_batch("DELETE FROM samples; DELETE FROM sessions; DELETE FROM kv;")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(ts: i64, target: Target, h: Option<f64>, power: Option<f64>) -> Sample {
        Sample {
            ts,
            secs: 10.0,
            hashrate: h,
            target,
            accepted: 1,
            rejected: 0,
            xmr_per_hash: Some(2e-12),
            power_w: power,
            power_measured: true,
            temp_c: Some(60.0),
            coin: "XMR".into(),
            btc_per_hash: Some(2e-12 * 0.0077),
            price_kwh: None,
        }
    }

    #[test]
    fn exports_samples_as_csv() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("t.db")).unwrap();
        let mut a = sample(1000, Target::User, Some(5000.0), Some(100.0));
        a.price_kwh = Some(0.36);
        s.insert_sample(&a).unwrap();
        s.insert_sample(&sample(1010, Target::Project, None, None))
            .unwrap();
        let csv = s.export_csv(0, 2000).unwrap();
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("time,seconds,coin,mined_for,"));
        let f: Vec<&str> = lines[1].split(',').collect();
        assert_eq!(f.len(), 13);
        assert_eq!((f[2], f[3], f[4]), ("XMR", "user", "5000"));
        // 100 W for 10 s at 0.36/kWh.
        assert!((f[9].parse::<f64>().unwrap() - 1000.0 * 0.36 / 3.6e6).abs() < 1e-15);
        let g: Vec<&str> = lines[2].split(',').collect();
        assert_eq!((g[3], g[4], g[9]), ("project", "", ""));
    }

    #[test]
    fn migrates_once_and_reopens() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.db");
        let s = Store::open(&p).unwrap();
        assert_eq!(s.schema_version().unwrap(), MIGRATIONS.len());
        s.kv_set("a", "1").unwrap();
        drop(s);
        let s = Store::open(&p).unwrap();
        assert_eq!(s.kv_get("a").unwrap().as_deref(), Some("1"));
        s.kv_set("a", "2").unwrap();
        assert_eq!(s.kv_get("a").unwrap().as_deref(), Some("2"));
        assert_eq!(s.kv_get("missing").unwrap(), None);
    }

    #[test]
    fn totals_split_user_and_project() {
        let s = Store::in_memory().unwrap();
        s.insert_sample(&sample(100, Target::User, Some(1000.0), Some(50.0)))
            .unwrap();
        s.insert_sample(&sample(110, Target::Project, Some(1000.0), Some(50.0)))
            .unwrap();
        s.insert_sample(&sample(120, Target::User, None, None))
            .unwrap();
        let t = s.totals(0, 1000).unwrap();
        assert!((t.user_xmr - 1000.0 * 10.0 * 2e-12).abs() < 1e-20);
        assert!((t.project_xmr - 1000.0 * 10.0 * 2e-12).abs() < 1e-20);
        assert_eq!(t.user_secs, 20.0);
        assert_eq!(t.project_secs, 10.0);
        assert_eq!(t.energy_joules, 1000.0);
        assert_eq!(t.unknown_power_secs, 10.0);
        assert!(t.power_all_measured);
        let empty = s.totals(5000, 6000).unwrap();
        assert_eq!(empty, RangeTotals::default());
    }

    #[test]
    fn stats_and_series() {
        let s = Store::in_memory().unwrap();
        for i in 0..6 {
            s.insert_sample(&sample(
                i * 10,
                Target::User,
                Some(100.0 * (i + 1) as f64),
                Some(60.0),
            ))
            .unwrap();
        }
        let h = s.hash_stats(0, 1000).unwrap();
        assert_eq!(h.max, Some(600.0));
        assert_eq!(h.avg, Some(350.0));
        assert_eq!(h.accepted, 6);
        let series = s.series(0, 1000, 30).unwrap();
        assert_eq!(series.len(), 2);
        assert_eq!(series[0].ts, 0);
        assert_eq!(series[0].hashrate, Some(200.0));
        assert_eq!(series[1].energy_wh, Some(60.0 * 30.0 / 3600.0));
    }

    #[test]
    fn sessions_lifecycle() {
        let s = Store::in_memory().unwrap();
        let id = s.start_session(1, "xmrig 6.22.2", "pool:443").unwrap();
        let id2 = s.start_session(2, "xmrig", "pool:443").unwrap();
        s.end_session(id, 5, "user").unwrap();
        assert_eq!(s.close_dangling_sessions(9).unwrap(), 1);
        let list = s.sessions(10).unwrap();
        assert_eq!(list[0].id, id2);
        assert_eq!(list[0].end_reason.as_deref(), Some("app_terminated"));
        assert_eq!(list[1].end_reason.as_deref(), Some("user"));
        s.clear().unwrap();
        assert!(s.sessions(10).unwrap().is_empty());
    }

    #[test]
    fn prune_old_samples() {
        let s = Store::in_memory().unwrap();
        s.insert_sample(&sample(1, Target::User, Some(1.0), None))
            .unwrap();
        s.insert_sample(&sample(100, Target::User, Some(1.0), None))
            .unwrap();
        assert_eq!(s.prune(50).unwrap(), 1);
    }
}

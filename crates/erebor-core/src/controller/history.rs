//! Samples, history, events and the alerts raised from live state.

use super::*;

impl Fleet {
    // ---------- samples, history, events ----------

    pub(super) fn sample(&self, rows: &[DeviceRow]) {
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
        // Whole buckets only: the current one is still filling, and summed across devices
        // it would count only those that already reported (a false drop at the end).
        let to = crate::now_unix() / bucket * bucket;
        let from = to - range_secs;
        let dev = device.map(|d| self.device(d).map(|d| d.id)).transpose()?;
        let db = self.db();
        // Per device and bucket first (averages), then summed across devices.
        let inner = "SELECT device_id, ts / ?2 * ?2 AS b, algo, AVG(hashrate) h, AVG(power_w) p, MAX(temp_c) t,
                            AVG(revenue_day) r, AVG(cost_day) c
                     FROM samples WHERE ts >= ?1 AND ts < ?4 AND (?3 IS NULL OR device_id = ?3) GROUP BY device_id, b";
        let mut st = db
            .prepare(&format!(
                "SELECT b, SUM(p), MAX(t), SUM(r), SUM(c) FROM ({inner}) GROUP BY b ORDER BY b"
            ))
            .map_err(db_err)?;
        let points: Vec<Value> = st
            .query_map(params![from, bucket, dev, to], |r| {
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
            .query_map(params![from, bucket, dev, to], |r| {
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

    pub(super) fn check_alerts(&self, rows: &[DeviceRow]) {
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
}

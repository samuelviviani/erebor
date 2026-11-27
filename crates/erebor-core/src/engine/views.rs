//! Market data and the read-only views the frontends poll (status, stats, logs).

use super::*;

impl Engine {
    /// Network difficulty is global: use the current pool's API, else any pool that has one.
    pub(super) fn market_api_base(&self) -> &'static str {
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

    pub(super) fn set_market(
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
            plug_w: self.plug_watts(),
            plug_error: self
                .config
                .energy
                .plug
                .enabled
                .then(|| self.plug.error.clone())
                .flatten(),
            spot_prices: if self.config.energy.dynamic.enabled {
                let s = self.config.energy.dynamic.surcharge;
                self.spot
                    .iter()
                    .map(|(t, p)| (*t, (p + s).max(0.0)))
                    .collect()
            } else {
                Vec::new()
            },
            spot_error: self
                .config
                .energy
                .dynamic
                .enabled
                .then(|| self.spot_error.clone())
                .flatten(),
            running_app: self.running_app.clone(),
            idle_secs: self.idle,
            resume_in_secs: self
                .resume_at
                .map(|(t, _)| t.saturating_duration_since(Instant::now()).as_secs()),
        }
    }

    /// Statistics for the `range_secs` before `to` (default: now), e.g. a calendar month.
    pub fn stats_report(&self, range_secs: i64, to: Option<i64>) -> Result<StatsReport, String> {
        let to = to.unwrap_or_else(|| now_unix() + 1);
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

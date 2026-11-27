//! The periodic work `tick()` does every `TICK`: sampling, dev fee, safety rules,
//! holds (idle, price, solar, room temperature) and the schedule.

use super::*;

impl Engine {
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

    pub(super) fn mined_btc(&self) -> Option<f64> {
        let mut total = None;
        for m in &self.mined {
            if let Some(c) = self.market.coins.iter().find(|c| c.id == m.coin) {
                *total.get_or_insert(0.0) += m.amount * c.price_btc;
            }
        }
        total
    }

    pub(super) fn refresh_mined(&mut self) {
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

    pub(super) fn flush_sample(&mut self) {
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

    pub(super) fn persist_ledger(&self) {
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

    pub(super) fn params(&self) -> Params {
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
    pub(super) fn power(&self, device: Device) -> Option<f64> {
        // A smart plug measures the whole computer: the honest figure, never lower.
        if let Some(w) = self.plug_watts() {
            return Some(w);
        }
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
    pub(super) fn mining_watts(&self, coin: &Coin, mining: bool) -> Option<f64> {
        if mining {
            self.power(coin.device)
        } else {
            self.watts.get(coin.id).copied()
        }
    }

    /// What a kWh costs for mining `coin` right now: the user's band (or base) price,
    /// the share covered by solar surplus at the feed-in price, minus the heat credit.
    pub(super) fn energy_price(&self, coin: &Coin, mining: bool) -> Option<f64> {
        use chrono::{Datelike, Local, Timelike};
        let e = &self.config.energy;
        let now = Local::now();
        let grid = self.spot_price().or_else(|| {
            energy::grid_price(
                e,
                now.weekday().num_days_from_monday() as usize,
                (now.hour() * 60 + now.minute()) as u16,
            )
        });
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
    pub(super) fn solar_export(&self) -> Option<f64> {
        self.config
            .energy
            .solar
            .enabled
            .then(|| self.solar.fresh())
            .flatten()
    }

    /// Fresh room temperature (°C), when the thermostat is set up.
    pub(super) fn room_temp(&self) -> Option<f64> {
        self.config
            .energy
            .thermostat
            .enabled
            .then(|| self.room.fresh())
            .flatten()
    }

    /// Fresh smart-plug reading (W), when the plug is set up.
    pub(super) fn plug_watts(&self) -> Option<f64> {
        self.config
            .energy
            .plug
            .enabled
            .then(|| self.plug.fresh())
            .flatten()
    }

    /// Dynamic tariff price per kWh now (market + surcharge, never below 0), when set up
    /// and known for this hour; otherwise the bands and base price apply.
    pub(super) fn spot_price(&self) -> Option<f64> {
        let d = &self.config.energy.dynamic;
        d.enabled
            .then(|| energy::spot_at(&self.spot, now_unix()))
            .flatten()
            .map(|p| (p + d.surcharge).max(0.0))
    }

    /// Prices to fetch now (zone), when the dynamic tariff is on and the last fetch is
    /// older than an hour (or failed more than 10 minutes ago).
    pub(super) fn spot_due(&self) -> Option<String> {
        let d = &self.config.energy.dynamic;
        let every = if self.spot_error.is_some() { 600 } else { 3600 };
        (d.enabled
            && !self.config.demo_mode
            && self.spot_at.is_none_or(|t| t.elapsed().as_secs() >= every))
        .then(|| d.zone.clone())
    }

    pub fn set_spot(&mut self, r: Result<Vec<(i64, f64)>, String>) {
        self.spot_at = Some(Instant::now());
        match r {
            Ok(p) if !p.is_empty() => {
                self.spot = p;
                self.spot_error = None;
            }
            Ok(_) => self.spot_error = Some("no_prices".into()),
            Err(e) => self.spot_error = Some(e),
        }
    }

    pub fn set_plug(&mut self, reading: Result<f64, String>) {
        self.plug.set(reading);
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
    pub(super) fn update_holds(&mut self, fresh: bool) {
        if self.config.performance.idle_minutes.is_some() {
            if fresh || self.idle_at.is_none_or(|t| t.elapsed() >= IDLE_POLL) {
                self.idle_at = Some(Instant::now());
                self.idle = (self.idle_source)();
            }
        } else {
            self.idle = None;
        }
        let apps = &self.config.performance.pause_apps;
        if apps.is_empty() {
            self.running_app = None;
        } else if fresh || self.apps_at.is_none_or(|t| t.elapsed() >= IDLE_POLL) {
            self.apps_at = Some(Instant::now());
            self.running_app = (self.apps_source)(apps);
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
    pub(super) fn hold_reason(&self) -> Option<&'static str> {
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
        } else if self.running_app.is_some() {
            Some("app")
        } else if warm {
            Some("warm")
        } else if self.econ_bad {
            Some("unprofitable")
        } else {
            None
        }
    }

    /// Put mining on hold when a reason appears, resume it when every reason clears.
    pub(super) fn apply_holds(&mut self) {
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
                    "app" => "held_app",
                    "warm" => "held_warm",
                    _ => "held_unprofitable",
                };
                let detail = (r == "app").then(|| self.running_app.clone()).flatten();
                self.alert(Level::Info, code, detail);
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
}

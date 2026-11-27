//! Start, stop, pause and crash recovery; pool routes and failover; the power-limit
//! search; stratum logins.

use super::*;

impl Engine {
    pub(super) fn can_start(&self) -> Result<(), String> {
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
    pub(super) fn check_pool_failover(&mut self) {
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
    pub(super) fn project_target(&self, coin: &Coin) -> Option<PoolTarget> {
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

    pub(super) fn pool_target(&self, target: Target) -> PoolTarget {
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

    pub(super) fn end_session(&mut self, reason: &str) {
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

    pub(super) fn tune_tick(&mut self) {
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

    pub(super) fn on_crash(&mut self, reason: String) {
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

pub(super) fn login(wallet: &str, worker: &str) -> (String, String) {
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

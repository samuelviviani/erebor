//! Which coin to mine here: requirements, ranking, measured hashrates, automatic
//! switching and earnings projections.

use super::*;

impl Engine {
    /// Why `coin` cannot be mined here, if so.
    pub(super) fn payout(&self) -> Option<AddressKind> {
        wallet::payout_kind(&self.config.wallet)
    }

    /// The user's own wallet for this coin, if set.
    pub(super) fn coin_wallet(&self, coin: &Coin) -> Option<&str> {
        self.config
            .coin_wallets
            .get(coin.id)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
    }

    pub(super) fn coin_blocker(&self, coin: &Coin) -> Option<&'static str> {
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
    pub(super) fn requirements(&self, coin: &Coin) -> Vec<Requirement> {
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
    pub(super) fn advice(&self) -> Advice {
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
    pub(super) fn candidates(&self) -> Vec<&'static Coin> {
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
    pub(super) fn hashrate(&self, id: &str) -> Option<f64> {
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
    pub(super) fn set_gpu_table(&mut self, table: &coins::GpuTable) {
        let names: Vec<&str> = self.gpu_names.iter().map(String::as_str).collect();
        self.estimates = coins::estimate(&names, table);
        self.gpu_table = table.clone();
    }

    /// Per card while mining a GPU coin: measured hashrate (when the miner's device list
    /// lines up with the detected cards) and what that model usually does on this coin.
    pub(super) fn gpu_rows(&self) -> Vec<GpuRow> {
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

    pub(super) fn wanted_coin(&self) -> &'static Coin {
        if self.config.coin == AUTO_COIN {
            self.pick_auto()
        } else {
            coins::find(&self.config.coin).unwrap_or_else(xmr)
        }
    }

    /// Market data as seen by this user: unMineable's fee replaces Kryptex's when the
    /// user is paid in BTC through unMineable.
    pub(super) fn markets(&self) -> Vec<CoinMarket> {
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

    pub(super) fn coin_market(&self, id: &str) -> Option<CoinMarket> {
        self.markets().into_iter().find(|m| m.id == id)
    }

    pub(super) fn btc_price(&self) -> Option<f64> {
        self.market.btc.as_ref().map(|b| {
            if self.config.energy.currency == "USD" {
                b.usd
            } else {
                b.eur
            }
        })
    }

    pub(super) fn projection(&self) -> Option<Projection> {
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
    pub(super) fn hashrate_dropped(&mut self, h: f64) -> bool {
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

    pub(super) fn learn_hashrate(&mut self) {
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
    pub(super) fn auto_switch(&mut self) {
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
}

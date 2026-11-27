//! Talking to devices (this PC, agents, ASICs) and keeping their live state: one
//! stream per agent with backoff, ASIC polling, market data for ASIC revenue.

use super::*;

impl Fleet {
    // ---------- transport ----------

    pub(super) fn local_call(&self, cmd: &str, args: &Value) -> Result<Value, String> {
        let f = self.local.as_ref().ok_or("local_unavailable")?;
        f(cmd, args).map_err(code)
    }

    pub(super) fn agent_call(
        &self,
        d: &DeviceRec,
        cmd: &str,
        args: &Value,
    ) -> Result<Value, String> {
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
    pub(super) fn computer(&self, d: &DeviceRec, cmd: &str, args: &Value) -> Result<Value, String> {
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
    pub(super) fn asic_action(&self, d: &DeviceRec, a: &AsicAction) -> Result<Value, String> {
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

    pub(super) fn set_live(&self, id: &str, f: impl FnOnce(&mut Live)) {
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
    pub(super) fn asic_revenue(&self, algo: &str, hashrate: f64, currency: &str) -> Option<f64> {
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

//! Bulk operations: start/stop/restart, profiles (with compatibility check and
//! rollback), per-device config and queries.

use super::*;

impl Fleet {
    // ---------- operations ----------

    /// Run `f` on each device, at most `CONCURRENCY` at a time; one failure never stops
    /// the others.
    pub(super) fn for_each(
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
}

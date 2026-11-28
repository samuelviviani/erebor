//! The device registry: devices, groups, tags, profiles, export and import.

use super::*;

impl Fleet {
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

    pub(super) fn insert(&self, d: &DeviceRec) -> Result<(), String> {
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
        let mut d = DeviceRec {
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
        if kind.is_none() {
            if let Ok(info) = self.agent_call(&d, "device", &Value::Null) {
                let hw = &info["hardware"];
                let gpu_count = hw["gpus"].as_array().map_or(0, Vec::len);
                let os = hw["osName"].as_str().unwrap_or("").to_lowercase();
                d.kind = if gpu_count >= 2 {
                    DeviceKind::GpuRig
                } else if os.contains("server") {
                    DeviceKind::Server
                } else {
                    DeviceKind::RemotePc
                };
            }
        }
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

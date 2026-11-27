//! The device table and the totals above it.

use super::*;

impl Fleet {
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
}

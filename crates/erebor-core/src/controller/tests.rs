use super::*;
use crate::asic_sim::{SimAsic, SimKind};
use crate::engine::{self, Engine};
use crate::remote::server::{self, Server};
use crate::remote::RemoteConfig;
use std::time::Instant;

fn agent(dir: &Path, demo: bool) -> (Server, String) {
    std::fs::write(dir.join("config.json"), format!(r#"{{"demoMode":{demo}}}"#)).unwrap();
    let e = Engine::new(dir.into(), engine::default_factory()).unwrap();
    let cfg = RemoteConfig {
        enabled: true,
        listen: "127.0.0.1:0".into(),
        discovery: false,
        asics: vec![],
    };
    let s = server::serve(Arc::new(Mutex::new(e)), &cfg).unwrap();
    let code = remote::start_pairing(dir).unwrap().code;
    (s, code)
}

fn controller(dir: &Path) -> Arc<Fleet> {
    std::fs::write(
        dir.join("config.json"),
        r#"{"demoMode":true,"onboardingCompleted":true}"#,
    )
    .unwrap();
    let e: crate::rpc::Shared = Arc::new(Mutex::new(
        Engine::new(dir.into(), engine::default_factory()).unwrap(),
    ));
    let local: LocalCall = Arc::new(move |c, a| crate::rpc::call(&e, c, a));
    Fleet::open(dir, Some(local)).unwrap()
}

#[test]
fn manages_agents_asics_and_this_pc_together() {
    let (c, a1, a2, a3) = (
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
    );
    let f = controller(c.path());
    let (s1, code1) = agent(a1.path(), true);
    let (s2, code2) = agent(a2.path(), true);
    let (gw, code3) = agent(a3.path(), true);

    let d1 = f
        .add_agent(&s1.addr.to_string(), &code1, Some(DeviceKind::GpuRig))
        .unwrap();
    assert_eq!(d1.kind, DeviceKind::GpuRig);
    let d2 = f.add_agent(&s2.addr.to_string(), &code2, None).unwrap();
    let g = f
        .add_agent(&gw.addr.to_string(), &code3, Some(DeviceKind::Server))
        .unwrap();
    assert_eq!(
        f.add_agent(&s1.addr.to_string(), &code1, None).err(),
        Some("pairing_not_active".into())
    );

    // ASICs: one direct, one through the gateway; the same ASIC can't be added twice.
    let direct = SimAsic::start(SimKind::Antminer { privileged: true }).unwrap();
    let behind = SimAsic::start(SimKind::Whatsminer).unwrap();
    let a = f.add_asic("127.0.0.1", direct.port, "s19", None).unwrap();
    assert_eq!(
        f.add_asic("127.0.0.1", direct.port, "again", Some(&g.id))
            .err(),
        Some("device_exists".into())
    );
    let w = f
        .add_asic("127.0.0.1", behind.port, "", Some(&g.id))
        .unwrap();
    assert!(w.name.contains("M30S+"), "{}", w.name);
    assert_eq!(
        f.add_asic("8.8.8.8", 4028, "x", None).err(),
        Some("asic_address_not_private".into())
    );

    f.refresh_once();
    let rows = f.rows();
    assert_eq!(rows.len(), 6, "local + 3 agents + 2 ASICs");
    assert_eq!(rows[0].id, LOCAL_ID);
    assert!(rows.iter().all(|r| r.conn == Conn::Online), "{rows:#?}");
    assert!(
        rows.iter().all(|r| r.simulated),
        "demo agents and simulators are flagged"
    );
    let wr = rows.iter().find(|r| r.id == w.id).unwrap();
    assert_eq!(wr.gateway.as_deref(), Some(g.id.as_str()));
    assert!(
        !wr.actions.restart.supported
            && wr.actions.restart.reason.as_deref() == Some("whatsminer_write_api_not_implemented")
    );

    // Mining on a selection, verified per device; ASICs can't start/stop.
    let res = f.mining(
        &f.select(&[d1.id.clone(), d2.id.clone(), a.id.clone()], None, false)
            .unwrap(),
        "start",
    );
    assert_eq!(
        res.iter().map(|r| r.state.as_str()).collect::<Vec<_>>(),
        ["completed", "completed", "failed"]
    );
    assert_eq!(res[2].error.as_deref(), Some("asic_no_stop_command"));
    f.refresh_once();
    let rows = f.rows();
    let ov = f.overview(&rows);
    assert_eq!(ov["mining"], 4, "2 rigs + 2 ASICs");
    // Hashrate is never summed across algorithms.
    let mut cpu = rows[0].clone();
    (cpu.algo, cpu.hashrate) = (Some("rx/0".into()), Some(5_000.0));
    let mut mixed = rows.clone();
    mixed.push(cpu);
    let ov2 = f.overview(&mixed);
    let by: HashMap<&str, f64> = ov2["hashrate"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| (h["algo"].as_str().unwrap(), h["hashrate"].as_f64().unwrap()))
        .collect();
    let asics: f64 = rows
        .iter()
        .filter(|r| r.kind == DeviceKind::Asic)
        .filter_map(|r| r.hashrate)
        .sum();
    assert_eq!((by["sha256d"], by["rx/0"]), (asics, 5_000.0), "{by:?}");

    // One device down: the others still answer, the failure is reported for it alone.
    f.check_alerts(&rows);
    drop(s2);
    std::thread::sleep(Duration::from_millis(300));
    let res = f.mining(
        &f.select(&[], None, true)
            .unwrap()
            .into_iter()
            .filter(|d| d.kind != DeviceKind::Asic)
            .collect::<Vec<_>>(),
        "stop",
    );
    let failed: Vec<&str> = res
        .iter()
        .filter(|r| r.state == "failed")
        .map(|r| r.id.as_str())
        .collect();
    assert_eq!(failed, [d2.id.as_str()]);
    f.refresh_once();
    f.check_alerts(&f.rows());
    assert!(f
        .events(0, 100)
        .iter()
        .any(|e| e.code == "device_offline" && e.device_id.as_deref() == Some(d2.id.as_str())));

    // Groups, tags, profiles applied to a group (computers and ASICs).
    let grp = f
        .save_group(&Group {
            id: 0,
            name: "Home mining".into(),
            energy_price: Some(0.30),
        })
        .unwrap();
    assert_eq!(
        f.save_group(&Group {
            id: 0,
            name: "home MINING".into(),
            energy_price: None
        })
        .err(),
        Some("group_exists".into())
    );
    f.assign_group(&[d1.id.clone(), a.id.clone()], Some(grp.id))
        .unwrap();
    f.tag(
        std::slice::from_ref(&d1.id),
        &["gpu".into(), "office".into()],
        true,
    )
    .unwrap();
    assert_eq!(f.device(&d1.id).unwrap().tags, ["gpu", "office"]);
    assert!(f
        .tag(std::slice::from_ref(&d1.id), &["bad|tag".into()], true)
        .is_err());
    let p = f
        .save_profile(&Profile {
            name: "btc-pool".into(),
            config: json!({"pool": {"worker": "{device}"}, "performance": {"preset": "eco"}}),
            asic: Some(AsicPoolSpec {
                url: "stratum+tcp://btc.example:3333".into(),
                user: "bc1qexample.{device}".into(),
                password: String::new(),
            }),
            ..Default::default()
        })
        .unwrap();
    assert!(f
        .save_profile(&Profile {
            name: "x".into(),
            config: json!({"miner": {"path": "/bin/sh"}}),
            ..Default::default()
        })
        .unwrap_err()
        .starts_with("profile_key_not_allowed"));
    let res = f.apply(&p, &f.select(&[], Some("home mining"), false).unwrap());
    assert!(res.iter().all(|r| r.state == "completed"), "{res:#?}");
    let cfg = f.config(&d1.id).unwrap();
    assert_eq!(cfg["performance"]["preset"], "eco");
    assert!(!cfg["pool"]["worker"].as_str().unwrap().is_empty());
    let st = asic::read(&direct.entry(""));
    assert!(st
        .pools
        .iter()
        .any(|p| p.active && p.user == "bc1qexample.s19"));

    // Energy price from the group wins over the agent's own.
    f.mining(
        &f.select(std::slice::from_ref(&d1.id), None, false).unwrap(),
        "start",
    );
    f.refresh_once();
    let r1 = f.rows().into_iter().find(|r| r.id == d1.id).unwrap();
    if let Some(w) = r1.power_w {
        assert!((r1.cost_day.unwrap() - w * 24.0 / 1000.0 * 0.30).abs() < 1e-9);
    }

    // History and retention.
    f.sample(&f.rows());
    // The current minute is still filling: left out until it is whole.
    let h = f.history(None, 3600).unwrap();
    assert!(h["points"].as_array().unwrap().is_empty());
    f.db()
        .execute("UPDATE samples SET ts = ts - 120", [])
        .unwrap();
    let h = f.history(None, 3600).unwrap();
    assert!(!h["points"].as_array().unwrap().is_empty());
    f.compact().unwrap();

    // Export without secrets, import elsewhere.
    let ex = f.export(false);
    assert_eq!(ex["devices"].as_array().unwrap().len(), 5);
    let other = tempfile::tempdir().unwrap();
    let f2 = Fleet::open(other.path(), None).unwrap();
    assert_eq!(
        f2.import(&ex).unwrap()["added"],
        7,
        "1 group, 1 profile, 5 devices"
    );
    assert_eq!(
        f2.device(&w.id).unwrap().gateway.as_deref(),
        Some(g.id.as_str())
    );

    // Removing a device revokes this controller on the agent.
    f.remove(&d1.id).unwrap();
    assert!(remote::controllers(a1.path()).is_empty());
    assert_eq!(f.remove(LOCAL_ID).err(), Some("cannot_remove_local".into()));
}

#[test]
fn profile_rolls_back_when_mining_wont_start() {
    let (c, a) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let f = controller(c.path());
    // Real mode without a payout: the config saves, mining refuses to start.
    let (s, code) = agent(a.path(), false);
    let d = f.add_agent(&s.addr.to_string(), &code, None).unwrap();
    let before = f.config(&d.id).unwrap();
    let p = Profile {
        name: "eco".into(),
        config: json!({"performance": {"preset": "eco"}}),
        start: true,
        ..Default::default()
    };
    let r = f.apply(&p, std::slice::from_ref(&d));
    assert_eq!(r[0].state, "rolled_back");
    assert_eq!(r[0].error.as_deref(), Some("wallet_missing"));
    assert_eq!(
        f.config(&d.id).unwrap()["performance"],
        before["performance"]
    );
    // An invalid configuration is refused by the agent and changes nothing.
    let bad = Profile {
        name: "bad".into(),
        config: json!({"wallet": "not-a-wallet"}),
        ..Default::default()
    };
    assert_eq!(f.apply(&bad, std::slice::from_ref(&d))[0].state, "failed");
}

#[test]
fn revoked_controller_shows_unauthorized() {
    let (c, a) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let f = controller(c.path());
    let (s, code) = agent(a.path(), true);
    let d = f.add_agent(&s.addr.to_string(), &code, None).unwrap();
    remote::revoke(a.path(), &f.identity.fingerprint).unwrap();
    f.refresh_once();
    // This computer is listed only once it is set up for mining.
    assert!(f.rows().iter().any(|r| r.id == LOCAL_ID));
    let mut c: crate::config::AppConfig =
        serde_json::from_value(f.local_call("get_config", &Value::Null).unwrap()).unwrap();
    (c.demo_mode, c.onboarding_completed) = (false, false);
    f.local_call("update_config", &json!({"config": c}))
        .unwrap();
    f.refresh_once();
    assert!(!f.rows().iter().any(|r| r.id == LOCAL_ID));
    let r = f.rows().into_iter().find(|r| r.id == d.id).unwrap();
    assert_eq!(r.conn, Conn::Unauthorized);
    assert_eq!(r.actions.start.reason.as_deref(), Some("unauthorized"));
}

#[test]
fn migrations_are_versioned_and_reopen() {
    let d = tempfile::tempdir().unwrap();
    drop(Fleet::open(d.path(), None).unwrap());
    let f = Fleet::open(d.path(), None).unwrap();
    let v: i64 = f
        .db()
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(v as usize, MIGRATIONS.len());
    // Referential integrity: deleting a gateway deletes the ASICs behind it.
    f.insert(&DeviceRec {
        id: "agent:g".into(),
        kind: DeviceKind::Server,
        name: "g".into(),
        address: "x".into(),
        fingerprint: None,
        gateway: None,
        group: None,
        tags: vec![],
        energy_price: None,
        added_at: 0,
        last_seen: None,
    })
    .unwrap();
    f.insert(&DeviceRec {
        id: "asic:10.0.0.1:4028".into(),
        kind: DeviceKind::Asic,
        name: "a".into(),
        address: "10.0.0.1:4028".into(),
        fingerprint: None,
        gateway: Some("agent:g".into()),
        group: None,
        tags: vec![],
        energy_price: None,
        added_at: 0,
        last_seen: None,
    })
    .unwrap();
    f.db()
        .execute("DELETE FROM devices WHERE id = 'agent:g'", [])
        .unwrap();
    assert!(f.devices().is_empty());
}

/// Scale check, run on demand: `cargo test -p erebor-core --release -- --ignored load --nocapture`.
/// 200 simulated ASICs polled for real over TCP, then 500 devices' worth of samples.
#[test]
#[ignore]
fn load_many_devices() {
    let d = tempfile::tempdir().unwrap();
    let f = Fleet::open(d.path(), None).unwrap();
    let sims: Vec<SimAsic> = (0..200)
        .map(|_| SimAsic::start(SimKind::Antminer { privileged: true }).unwrap())
        .collect();
    let t = Instant::now();
    for (i, s) in sims.iter().enumerate() {
        f.insert(&DeviceRec {
            id: s.entry("").id(),
            kind: DeviceKind::Asic,
            name: format!("s19-{i}"),
            address: format!("127.0.0.1:{}", s.port),
            fingerprint: None,
            gateway: None,
            group: None,
            tags: vec![],
            energy_price: None,
            added_at: 0,
            last_seen: None,
        })
        .unwrap();
    }
    let t_insert = t.elapsed();
    let t = Instant::now();
    f.refresh_once();
    let t_poll = t.elapsed();
    let t = Instant::now();
    let rows = f.rows();
    let ov = f.overview(&rows);
    let t_rows = t.elapsed();
    assert_eq!(ov["online"], 200);
    // 500 devices × 24 h of one-minute samples = 720 000 rows.
    let t = Instant::now();
    {
        let mut db = f.db();
        let tx = db.transaction().unwrap();
        for dev in 0..500 {
            let id = rows[dev % rows.len()].id.clone();
            for m in 0..1440i64 {
                tx.execute("INSERT INTO samples (device_id, ts, span, algo, hashrate, power_w, power_measured, temp_c, revenue_day, cost_day, currency, mining) VALUES (?1, ?2, 60, 'sha256d', 9.5e13, 3250, 1, 70, 4.0, 2.0, 'EUR', 1)",
                    params![id, crate::now_unix() - 3 * 86_400 + m * 60 + dev as i64]).unwrap();
            }
        }
        tx.commit().unwrap();
    }
    let t_samples = t.elapsed();
    let t = Instant::now();
    let h = f.history(None, 30 * 86_400).unwrap();
    let t_hist = t.elapsed();
    let t = Instant::now();
    f.compact().unwrap();
    let t_compact = t.elapsed();
    let rows_after: i64 = f
        .db()
        .query_row("SELECT COUNT(*) FROM samples", [], |r| r.get(0))
        .unwrap();
    f.db().execute_batch("VACUUM").unwrap();
    let size: i64 = f
        .db()
        .query_row(
            "SELECT page_count * page_size FROM pragma_page_count(), pragma_page_size()",
            [],
            |r| r.get(0),
        )
        .unwrap();
    println!(
        "insert 200 devices {t_insert:?} · poll 200 ASICs {t_poll:?} · rows+overview {t_rows:?}"
    );
    println!("720k samples {t_samples:?} · 30-day history {t_hist:?} ({} points) · compact {t_compact:?} → {rows_after} rows, {:.1} MB", h["points"].as_array().unwrap().len(), size as f64 / 1e6);
    assert!(rows_after < 720_000 / 10, "compaction to 15-minute rows");
}

#[test]
fn templating_merge_and_versions() {
    let mut b = json!({"pool": {"host": "a", "worker": "w"}, "coin": "AUTO"});
    merge(
        &mut b,
        &templated(
            &json!({"pool": {"worker": "{device}"}, "coin": "XMR"}),
            "rig 01!",
        ),
    );
    assert_eq!(
        b,
        json!({"pool": {"host": "a", "worker": "rig01"}, "coin": "XMR"})
    );
    assert!(newer("0.2.0", "0.1.9") && !newer("0.1.7", "0.1.7") && newer("0.1.10", "0.1.9"));
}

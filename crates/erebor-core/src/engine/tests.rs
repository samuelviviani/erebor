use super::*;
use super::{jobs::every, jobs::update_miners, lifecycle::login};
use std::sync::atomic::AtomicU32;

#[derive(Default)]
struct Shared {
    starts: AtomicU32,
    stops: AtomicU32,
    switches: Mutex<Vec<String>>,
    coins: Mutex<Vec<String>>,
    crash: AtomicBool,
    fail_switch: AtomicBool,
}

struct MockMiner {
    s: Arc<Shared>,
    running: bool,
}

impl MinerAdapter for MockMiner {
    fn info(&self) -> MinerInfo {
        MinerInfo {
            id: "mock",
            name: "Mock",
            version: Some("1".into()),
            algorithms: vec!["rx/0"],
            supports_pause: true,
            supports_hot_switch: true,
            supports_benchmark: false,
            builtin_donation_percent: 1.0,
        }
    }
    fn start(&mut self, _: &LaunchSpec) -> Result<(), String> {
        self.s.starts.fetch_add(1, Ordering::SeqCst);
        self.running = true;
        Ok(())
    }
    fn stop(&mut self) {
        if self.running {
            self.s.stops.fetch_add(1, Ordering::SeqCst);
        }
        self.running = false;
    }
    fn set_paused(&mut self, _: bool) -> Result<(), String> {
        Ok(())
    }
    fn switch_pool(&mut self, p: &PoolTarget) -> Result<(), String> {
        self.s.switches.lock().unwrap().push(p.user.clone());
        if self.s.fail_switch.load(Ordering::SeqCst) {
            return Err("api down".into());
        }
        Ok(())
    }
    fn stats(&mut self) -> Result<MinerStats, String> {
        Ok(MinerStats {
            hashrate_10s: Some(1000.0),
            hashrate_60s: Some(1000.0),
            accepted: 3,
            connected: true,
            ..Default::default()
        })
    }
    fn take_exit(&mut self) -> Option<String> {
        (self.running && self.s.crash.swap(false, Ordering::SeqCst)).then(|| {
            self.running = false;
            "boom".to_string()
        })
    }
    fn logs(&self) -> Vec<String> {
        vec![]
    }
    fn pid(&self) -> Option<u32> {
        None
    }
}

const KRX: &str = "krxABC123";
const WALLET: &str = "48edfHu7V9Z84YzzMa6fUueoELZ9ZRXq9VetWzYGzKt52XU5xvqgzYnDK9URnRoJMk1j8nLwEVsaSWJ4fhdUyZijBGUicoD";

fn engine(dir: &Path, demo: bool) -> (Engine, Arc<Shared>) {
    let s = Arc::new(Shared::default());
    let s2 = s.clone();
    let factory: Factory = Box::new(
        move |_: &AppConfig,
              coin: &Coin,
              _: &Path,
              _: usize|
              -> Result<Box<dyn MinerAdapter>, String> {
            s2.coins.lock().unwrap().push(coin.id.to_string());
            Ok(Box::new(MockMiner {
                s: s2.clone(),
                running: false,
            }))
        },
    );
    let mut e = Engine::new(dir.to_path_buf(), factory).unwrap();
    let mut c = e.config().clone();
    c.demo_mode = demo;
    c.onboarding_completed = true;
    c.wallet = KRX.into();
    e.update_config(c).unwrap();
    e.config.miner.path = Some("/mock/xmrig".into());
    (e, s)
}

#[test]
fn starts_without_fee_prompt_but_needs_a_wallet() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, s) = engine(d.path(), false);
    let mut c = e.config().clone();
    c.wallet.clear();
    e.update_config(c).unwrap();
    assert_eq!(e.start(false), Err("wallet_missing".into()));
    assert_eq!(s.starts.load(Ordering::SeqCst), 0);
    let mut c = e.config().clone();
    c.wallet = KRX.into();
    e.update_config(c).unwrap();
    e.start(false).unwrap();
    assert_eq!(e.status().state, RunState::Running);
}

#[test]
fn kryptex_login_and_pool_rules() {
    assert_eq!(
        login("krxABC123", "my-rig_1"),
        ("krxABC123/myrig1".into(), "x".into())
    );
    assert_eq!(login("krxABC123", ""), ("krxABC123".into(), "x".into()));
    assert_eq!(login(WALLET, "rig"), (WALLET.into(), "rig".into()));
    let d = tempfile::tempdir().unwrap();
    let (mut e, _) = engine(d.path(), false);
    let mut c = e.config().clone();
    c.wallet = "krxABC123".into();
    c.pool.host = "pool.supportxmr.com".into();
    c.pool.id = "supportxmr".into();
    // Saving a Kryptex ID moves the pool to Kryptex instead of failing.
    e.update_config(c).unwrap();
    assert_eq!(e.config().pool.id, "kryptex");
    let t = e.pool_target(Target::User);
    assert_eq!(t.url, "xmr.kryptex.network:7029");
    // No name chosen: this PC's hostname. The project's 2% is always "EREBOR".
    let pc = crate::config::default_worker();
    assert_eq!(t.user, format!("krxABC123/{pc}"));
    // No project destination in a test build: no fee (the demo test covers "EREBOR").
    assert!(e.project_target(e.coin).is_none());
    let mut c = e.config().clone();
    c.pool.worker = "Ufficio".into();
    e.update_config(c).unwrap();
    assert_eq!(e.pool_target(Target::User).user, "krxABC123/Ufficio");
    let mut c = e.config().clone();
    c.pool.worker = "erebor".into();
    assert_eq!(e.update_config(c), Err("worker_reserved".into()));
}

#[test]
fn start_stop_and_session_history() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, s) = engine(d.path(), false);
    e.start(false).unwrap();
    e.start(false).unwrap(); // idempotent: no second process
    assert_eq!(s.starts.load(Ordering::SeqCst), 1);
    e.pause().unwrap();
    assert_eq!(e.status().state, RunState::Paused);
    e.resume().unwrap();
    e.stop("user");
    assert_eq!(e.status().state, RunState::Stopped);
    let sessions = e.store.sessions(10).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].end_reason.as_deref(), Some("user"));
}

#[test]
fn crash_is_detected_and_restarted_when_enabled() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, s) = engine(d.path(), false);
    let mut c = e.config().clone();
    c.safety.auto_restart = true;
    e.update_config(c).unwrap();
    e.start(false).unwrap();
    s.crash.store(true, Ordering::SeqCst);
    e.tick();
    assert_eq!(e.status().state, RunState::Error);
    assert!(e.restart_at.is_some());
    e.restart_at = Some(Instant::now()); // fast-forward
    e.tick();
    assert_eq!(e.status().state, RunState::Running);
    assert_eq!(s.starts.load(Ordering::SeqCst), 2);
}

#[test]
fn crash_without_auto_restart_stays_stopped() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, s) = engine(d.path(), false);
    e.start(false).unwrap();
    s.crash.store(true, Ordering::SeqCst);
    e.tick();
    assert_eq!(e.status().state, RunState::Error);
    assert!(e.restart_at.is_none());
    e.stop("user");
    assert_eq!(e.status().state, RunState::Stopped);
}

#[test]
fn effective_time_is_sampled_and_ledger_persisted() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, _) = engine(d.path(), true); // demo: dev fee simulated
    e.start(false).unwrap();
    // Simulate 2s ticks by moving last_tick back.
    for _ in 0..20 {
        e.last_tick = Some(Instant::now() - Duration::from_secs(2));
        e.tick();
    }
    e.stop("user");
    let t = e.store.totals(0, now_unix() + 10).unwrap();
    assert!((t.user_secs - 40.0).abs() < 1.0, "{}", t.user_secs);
    let ledger = load_ledger(&e.store);
    assert!((ledger.user_secs - 40.0).abs() < 1.0);
}

#[test]
fn devfee_switches_and_falls_back_to_restart() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, s) = engine(d.path(), true);
    // Pretend 100 minutes were already mined: the next tick owes a project block.
    e.devfee = DevFeeManager::new(
        devfee::DEFAULT_FEE_PERCENT,
        true,
        Ledger {
            user_secs: 5_995.0,
            project_secs: 0.0,
        },
    );
    e.start(false).unwrap();
    e.last_tick = Some(Instant::now() - Duration::from_secs(9));
    e.tick();
    assert_eq!(e.status().target, Target::Project);
    // Project time goes to the project's BTC address on unMineable.
    assert_eq!(
        s.switches.lock().unwrap().as_slice(),
        ["BTC:DEMO-PROJECT-BTC.EREBOR"]
    );
    // Hot switch failure → miner restarted on the user target.
    s.fail_switch.store(true, Ordering::SeqCst);
    for _ in 0..70 {
        e.last_tick = Some(Instant::now() - Duration::from_secs(2));
        e.tick();
    }
    assert_eq!(e.status().target, Target::User);
    assert!(s.starts.load(Ordering::SeqCst) >= 2);
    assert_eq!(e.status().state, RunState::Running);
}

#[test]
fn sleep_gaps_are_not_counted() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, _) = engine(d.path(), true);
    e.start(false).unwrap();
    e.last_tick = Some(Instant::now() - Duration::from_secs(3600));
    e.tick();
    e.stop("user");
    assert_eq!(e.devfee.ledger().user_secs, 0.0);
}

#[test]
fn demo_and_production_data_are_separate() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, _) = engine(d.path(), true);
    e.start(false).unwrap();
    for _ in 0..10 {
        e.last_tick = Some(Instant::now() - Duration::from_secs(2));
        e.tick();
    }
    let mut c = e.config().clone();
    c.demo_mode = false;
    e.update_config(c).unwrap();
    assert_eq!(e.status().state, RunState::Stopped);
    let t = e.store.totals(0, now_unix() + 10).unwrap();
    assert_eq!(t.user_secs, 0.0);
    assert!(d.path().join("erebor-demo.db").exists());
}

#[test]
fn config_is_restored_after_restart() {
    let d = tempfile::tempdir().unwrap();
    {
        let _ = engine(d.path(), false);
    }
    let e = Engine::new(d.path().to_path_buf(), default_factory()).unwrap();
    assert_eq!(e.config().wallet, KRX);
}

#[test]
fn protected_fields_cannot_be_set_via_update() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, _) = engine(d.path(), false);
    let mut c = e.config().clone();
    c.miner.path = Some("/tmp/evil".into());
    e.update_config(c).unwrap();
    assert_eq!(e.config().miner.path.as_deref(), Some("/mock/xmrig"));
}

#[test]
fn delete_all_data_resets() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, _) = engine(d.path(), false);
    e.start(false).unwrap();
    e.delete_all_data().unwrap();
    assert_eq!(e.status().state, RunState::Stopped);
    assert_eq!(e.config(), &AppConfig::default());
    assert!(!d.path().join("config.json").exists());
}

fn market(id: &str, coin_per_hash: f64) -> CoinMarket {
    CoinMarket {
        id: id.into(),
        coin_per_hash,
        price_btc: 1e-5,
        pool_fee_percent: 1.0,
    }
}

#[test]
fn idle_only_mining_waits_while_the_pc_is_in_use() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, s) = engine(d.path(), false);
    e.config.performance.idle_minutes = Some(5);
    e.idle_source = || Some(10); // someone is typing
    e.start(false).unwrap();
    assert_eq!(e.status().held, Some("in_use"));
    assert_eq!(s.starts.load(Ordering::SeqCst), 0);
    e.idle_source = || Some(600); // away for 10 minutes
    e.idle_at = None; // skip the 5 s polling interval
    e.apply_holds();
    assert_eq!(e.held, None);
    assert_eq!(e.state, RunState::Running);
    assert_eq!(s.starts.load(Ordering::SeqCst), 1);
    e.idle_source = || Some(0); // back at the keyboard: waits at once
    e.idle_at = None;
    e.apply_holds();
    assert_eq!((e.state, e.held), (RunState::Stopped, Some("in_use")));
    assert_eq!(s.stops.load(Ordering::SeqCst), 1);
    e.stop("user"); // stopping by hand forgets the wait
    assert_eq!(e.held, None);
    e.idle_source = || Some(600);
    e.idle_at = None;
    e.apply_holds();
    assert_eq!(e.state, RunState::Stopped);
}

#[test]
fn unprofitable_mining_waits_and_resumes() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, s) = engine(d.path(), false);
    e.config.energy.stop_if_unprofitable = true; // no market data: never flips on its own
    e.econ_bad = true;
    e.start(false).unwrap();
    assert_eq!(e.held, Some("unprofitable"));
    assert_eq!(s.starts.load(Ordering::SeqCst), 0);
    e.econ_bad = false;
    e.apply_holds();
    assert_eq!((e.state, e.held), (RunState::Running, None));
    e.econ_bad = true;
    e.apply_holds();
    assert_eq!((e.state, e.held), (RunState::Stopped, Some("unprofitable")));
}

#[test]
fn a_stuck_gpu_is_restarted_once_then_accepted() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, s) = engine(d.path(), false);
    e.has_gpu = true;
    e.gpu_platform = true;
    e.config.srbminer.path = Some("/mock/srbminer".into());
    let mut c = e.config().clone();
    c.coin = "PRL".into();
    e.update_config(c).unwrap();
    e.start(false).unwrap();
    assert_eq!((e.state, e.coin.id), (RunState::Running, "PRL"));
    e.hashrates.insert("PRL".into(), 1000.0);
    let long_ago = || Instant::now().checked_sub(DROP_STEADY + Duration::from_secs(1));
    assert!(!e.hashrate_dropped(950.0)); // normal
    assert!(e.hashrate_dropped(500.0)); // low, not for long yet
    assert_eq!(s.starts.load(Ordering::SeqCst), 1);
    e.low_since = long_ago();
    assert!(e.hashrate_dropped(500.0)); // low for 5 minutes: restart
    assert_eq!(s.starts.load(Ordering::SeqCst), 2);
    assert_eq!(e.state, RunState::Running);
    e.hashrate_dropped(500.0);
    e.low_since = long_ago();
    assert!(!e.hashrate_dropped(500.0)); // still low after the restart: new normal
    assert_eq!(e.hashrates["PRL"], 500.0);
    assert_eq!(s.starts.load(Ordering::SeqCst), 2);
}

#[test]
fn stop_for_a_while_starts_again_by_itself() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, s) = engine(d.path(), false);
    e.stop_for(3600); // not mining: nothing to come back to
    assert_eq!(e.status().resume_in_secs, None);
    e.start(false).unwrap();
    e.stop_for(3600);
    assert_eq!(e.state, RunState::Stopped);
    assert!(e.status().resume_in_secs.is_some_and(|s| s > 3500));
    e.resume_at = Some((Instant::now(), false));
    e.tick();
    assert_eq!(e.state, RunState::Running);
    assert_eq!(s.starts.load(Ordering::SeqCst), 2);
    e.stop_for(3600);
    e.stop("user"); // a real stop cancels the comeback
    assert_eq!(e.resume_at, None);
}

#[test]
fn thermostat_heats_between_its_two_temperatures() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, _) = engine(d.path(), false);
    e.config.energy.thermostat.enabled = true; // 20 °C on, 21 °C off
    e.config.energy.thermostat.url = "http://ha.local/api/states/sensor.room".into();
    let at = |e: &mut Engine, t: f64| {
        e.set_room(Ok(t));
        e.apply_holds();
        (e.state, e.held)
    };
    e.set_room(Ok(21.5));
    e.start(false).unwrap();
    assert_eq!(e.held, Some("warm"));
    assert_eq!(at(&mut e, 20.5), (RunState::Stopped, Some("warm")));
    assert_eq!(at(&mut e, 19.9), (RunState::Running, None));
    assert_eq!(at(&mut e, 20.5), (RunState::Running, None));
    assert_eq!(at(&mut e, 21.0), (RunState::Stopped, Some("warm")));
    // Sensor down: it keeps waiting instead of guessing.
    e.room = Reading::default();
    e.apply_holds();
    assert_eq!(e.held, Some("warm"));
}

#[test]
fn mining_comes_back_after_an_app_update() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, _) = engine(d.path(), false);
    e.prepare_update(); // not mining: nothing to remember
    assert!(!e.take_resume_after_update());
    e.start(false).unwrap();
    e.prepare_update();
    assert!(e.take_resume_after_update());
    assert!(!e.take_resume_after_update()); // only once
}

#[test]
fn efficiency_search_keeps_the_best_limit() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, s) = engine(d.path(), false);
    e.has_gpu = true;
    e.gpu_platform = true;
    e.config.srbminer.path = Some("/mock/srbminer".into());
    let mut c = e.config().clone();
    c.coin = "PRL".into();
    e.update_config(c).unwrap();
    e.start(false).unwrap();
    e.live.gpus = vec![crate::hardware::GpuLive {
        power_w: Some(200.0),
        ..Default::default()
    }];
    e.start_tune().unwrap();
    // Same hashrate at every limit (mock miner), no market data: least watts wins.
    for _ in 0..5 {
        assert!(e.tune.is_some(), "{:?}", e.error);
        e.tune.as_mut().unwrap().step_started = Instant::now()
            .checked_sub(TUNE_WARM + TUNE_MEASURE)
            .unwrap();
        let limit = e.config.performance.gpu_power_limit_w;
        e.live.gpus[0].power_w = Some(limit.map_or(200.0, f64::from));
        e.stats = Some(MinerStats {
            hashrate_10s: Some(1000.0),
            ..Default::default()
        });
        e.tune_tick();
    }
    assert!(e.tune.is_none());
    assert_eq!(e.config().performance.gpu_power_limit_w, Some(120));
    assert_eq!(s.starts.load(Ordering::SeqCst), 6); // start + one per limit tried + final
    assert!(e
        .job
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|j| j.done && j.error.is_none()));
}

#[test]
fn own_wallets_go_to_the_chosen_pool_and_fail_over() {
    let kas = "kaspa:qqkqkzjvr7zwxxmjxjkmxxdwju9kjs6e9u82uh59z07vgaks6gg62v8707g73";
    let d = tempfile::tempdir().unwrap();
    let (mut e, s) = engine(d.path(), false);
    e.has_gpu = true;
    e.gpu_platform = true;
    e.config.lolminer.path = Some("/mock/lolMiner".into());
    // No main payout at all: an own wallet is enough to mine that coin.
    let mut c = e.config().clone();
    c.wallet.clear();
    c.coin = "KAS".into();
    c.coin_wallets.insert("KAS".into(), kas.into());
    c.coin_pools.insert(
        "KAS".into(),
        crate::config::CoinPoolChoice {
            pool: "2miners".into(),
            region: "asia".into(),
            backup: "woolypooly".into(),
            ..Default::default()
        },
    );
    e.update_config(c).unwrap();
    e.start(false).unwrap();
    let t = e.pool_target(Target::User);
    assert_eq!(t.url, "asia-kas.2miners.com:2020");
    assert!(t.user.starts_with(&format!("{kas}.")));
    // This test build has no project destination: no fee, never the user's wallet.
    assert!(e.project_target(e.coin).is_none());
    // Three minutes without a pool connection: the backup takes over.
    e.stats = Some(MinerStats::default());
    e.offline_since = Instant::now().checked_sub(FAILOVER_AFTER);
    e.check_pool_failover();
    assert!(e.on_backup);
    assert_eq!(e.pool_target(Target::User).url, "pool.woolypooly.com:3112");
    assert_eq!(s.starts.load(Ordering::SeqCst), 2);
    // Later it tries the main pool again.
    e.backup_since = Instant::now().checked_sub(PRIMARY_RETRY);
    e.check_pool_failover();
    assert!(!e.on_backup);
    // Coins without a destination are not mined; excluded ones are not picked.
    let etc = coins::find("ETC").unwrap();
    assert_eq!(e.coin_blocker(etc), Some("wallet_missing"));
    e.stop("user");
    let mut c = e.config().clone();
    c.coin = AUTO_COIN.into();
    c.auto_exclude.insert("KAS".into());
    e.update_config(c).unwrap();
    assert!(!e.candidates().iter().any(|c| c.id == "KAS"));
    // Unknown pools and bad custom hosts are refused.
    let mut c = e.config().clone();
    c.coin_pools.get_mut("KAS").unwrap().pool = "nope".into();
    assert!(e.update_config(c).is_err());
    let mut c = e.config().clone();
    let k = c.coin_pools.get_mut("KAS").unwrap();
    (k.pool, k.host, k.port) = ("custom".into(), "pool.example.com; rm".into(), 3333);
    assert!(e.update_config(c).is_err());
}

#[test]
fn power_is_the_measured_draw_of_the_mining_device() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, _) = engine(d.path(), false);
    let gpu = |w| crate::hardware::GpuLive {
        power_w: w,
        ..Default::default()
    };
    e.live.power_w = Some(40.0);
    e.live.gpus = vec![gpu(Some(150.0)), gpu(None), gpu(Some(100.0))];
    assert_eq!(e.power(Device::Cpu), Some(40.0));
    assert_eq!(e.power(Device::Gpu), Some(250.0));
    e.live.gpus = vec![gpu(None)];
    assert_eq!(e.power(Device::Gpu), None);
    // A smart plug measures the whole PC and wins over the parts.
    e.config.energy.plug.enabled = true;
    e.set_plug(Ok(310.0));
    assert_eq!(e.power(Device::Gpu), Some(310.0));
    assert_eq!(e.power(Device::Cpu), Some(310.0));
    e.config.energy.plug.enabled = false;
    assert_eq!(e.power(Device::Cpu), Some(40.0));
}

#[test]
fn mining_waits_while_a_listed_program_runs() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, s) = engine(d.path(), false);
    e.config.performance.pause_apps = vec!["game.exe".into()];
    e.apps_source = |_| Some("game.exe".into());
    e.start(false).unwrap();
    assert_eq!(e.status().held, Some("app"));
    assert_eq!(e.status().running_app.as_deref(), Some("game.exe"));
    assert_eq!(s.starts.load(Ordering::SeqCst), 0);
    e.apps_source = |_| None; // the game was closed
    e.apps_at = None;
    e.apply_holds();
    assert_eq!((e.state, e.held), (RunState::Running, None));
}

#[test]
fn dynamic_tariff_price_replaces_the_bands_while_known() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, _) = engine(d.path(), false);
    e.config.energy.price_per_kwh = Some(0.30);
    let coin = e.coin;
    assert_eq!(e.energy_price(coin, false), Some(0.30));
    e.config.energy.dynamic.enabled = true;
    e.config.energy.dynamic.surcharge = 0.12;
    assert!(e.spot_due().is_some());
    e.set_spot(Ok(vec![(now_unix() - 60, 0.05)]));
    assert!(e.spot_due().is_none(), "fetched at most once an hour");
    let p = e.energy_price(coin, false).unwrap();
    assert!(
        (p - 0.17).abs() < 1e-9,
        "market 0.05 + surcharge 0.12, got {p}"
    );
    // Negative market prices: mining never counts as paid to run.
    e.config.energy.dynamic.surcharge = 0.0;
    e.set_spot(Ok(vec![(now_unix() - 60, -0.20)]));
    assert_eq!(e.energy_price(coin, false), Some(0.0));
    // No price for this hour: back to the user's own price.
    e.set_spot(Ok(vec![(now_unix() - 7200, 0.05)]));
    assert_eq!(e.energy_price(coin, false), Some(0.30));
}

#[test]
fn without_a_gpu_auto_mines_monero_on_the_cpu() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, s) = engine(d.path(), false);
    e.has_gpu = false;
    assert_eq!(e.config().coin, AUTO_COIN);
    e.start(false).unwrap();
    assert_eq!(s.coins.lock().unwrap().as_slice(), ["XMR"]);
    assert_eq!(e.status().coin.id, "XMR");
}

#[test]
fn coins_off_kryptex_need_an_own_wallet_and_say_everything_missing() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, _) = engine(d.path(), false);
    e.has_gpu = false;
    let mut c = e.config().clone();
    c.wallet = KRX.into();
    e.update_config(c.clone()).unwrap();
    let firo = coins::find("FIRO").unwrap();
    assert_eq!(e.coin_blocker(firo), Some("own_wallet_required"));
    let reqs: Vec<_> = e
        .requirements(firo)
        .iter()
        .map(|r| (r.kind, r.status, r.code))
        .collect();
    assert!(reqs.contains(&("hardware", "missing", "no_gpu")));
    assert!(reqs.contains(&("miner", "warn", "miner_missing")));
    assert!(reqs.contains(&("payout", "missing", "own_wallet_required")));
    // An own wallet clears the payout requirement; the missing GPU remains.
    c.coin_wallets
        .insert("FIRO".into(), "aFiroPublicAddress1234567890xyz".into());
    e.update_config(c).unwrap();
    assert_eq!(e.coin_blocker(firo), Some("no_gpu"));
    assert!(e
        .requirements(firo)
        .iter()
        .any(|r| r.code == "own_wallet" && r.status == "ok"));
    // ASIC coins show up, but the automatic choice never takes them.
    assert!(e.requirements(coins::find("BTC").unwrap())[0].status == "warn");
    assert!(e.candidates().iter().all(|c| !c.asic));
}

#[test]
fn gpu_coins_need_a_kryptex_id_and_a_gpu() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, _) = engine(d.path(), false);
    e.has_gpu = true;
    e.gpu_platform = true;
    let mut c = e.config().clone();
    c.wallet = WALLET.into();
    e.update_config(c).unwrap();
    let prl = coins::find("PRL").unwrap();
    assert_eq!(e.coin_blocker(prl), Some("kryptex_or_btc_required"));
    let mut c = e.config().clone();
    c.coin = "PRL".into();
    // Not mineable with a Monero address: back to the automatic choice.
    e.update_config(c).unwrap();
    assert_eq!(e.config().coin, AUTO_COIN);
    e.has_gpu = false;
    let mut c = e.config().clone();
    c.wallet = "krxABC123".into();
    e.update_config(c).unwrap();
    assert_eq!(e.coin_blocker(prl), Some("no_gpu"));
}

#[test]
fn auto_picks_the_most_profitable_coin_without_trying_each() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, s) = engine(d.path(), false);
    e.has_gpu = true;
    e.gpu_platform = true;
    e.config.srbminer.path = Some("/mock/SRBMiner-MULTI".into());
    e.hashrates.clear();
    e.gpu_names = vec!["NVIDIA GeForce RTX 3060".into()];
    e.set_gpu_table(&coins::bundled_gpu_table());
    let mut c = e.config().clone();
    c.wallet = "krxABC123".into();
    e.update_config(c).unwrap();
    // Same reward per hash everywhere: the coin with the highest expected BTC/day for
    // an RTX 3060 wins straight away, from the table alone.
    e.market.coins = coins::COINS.iter().map(|c| market(c.id, 1e-6)).collect();
    let want = coins::best(&e.candidates(), &e.estimates, &e.markets())
        .unwrap()
        .0
        .id;
    e.start(false).unwrap();
    assert_eq!(e.coin.id, want);
    // Another coin becomes 10x better: Erebor moves at the next evaluation.
    let other = if want == "ERG" { "RVN" } else { "ERG" };
    let ratio = e.estimates[want] / e.estimates[other];
    let m = e.market.coins.iter_mut().find(|m| m.id == other).unwrap();
    m.coin_per_hash = 1e-5 * ratio;
    e.tick();
    assert_eq!(e.coin.id, other);
    assert_eq!(s.coins.lock().unwrap().as_slice(), [want, other]);
    // Within REEVAL_SECS nothing changes, even if the first coin becomes better again.
    e.market
        .coins
        .iter_mut()
        .find(|m| m.id == want)
        .unwrap()
        .coin_per_hash = 1.0;
    e.tick();
    assert_eq!(e.coin.id, other);
}

#[test]
fn advice_names_the_best_coin_and_payout_for_btc() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, _) = engine(d.path(), false);
    e.has_gpu = true;
    e.gpu_platform = true;
    // Monero address: mines XMR on the CPU, yet the GPU is the way to go.
    let a = e.advice();
    assert_eq!(a.device, Device::Gpu);
    assert!(a.coin.is_none());
    assert_eq!(a.payout, "bitcoin");
    for c in coins::COINS {
        e.hashrates.insert(c.id.into(), 1000.0);
    }
    e.market.coins = coins::COINS
        .iter()
        .map(|c| market(c.id, if c.id == "PRL" { 3e-6 } else { 1e-6 }))
        .collect();
    // Pearl costs 3% on Kryptex, 1% on unMineable: BTC through unMineable wins.
    e.market.coins[0].pool_fee_percent = 3.0;
    let a = e.advice();
    assert_eq!(a.coin.unwrap().id, "PRL");
    assert_eq!(a.payout, "bitcoin");
    // RVN is Kryptex-only.
    e.market.coins = vec![market("RVN", 9e-6)];
    assert_eq!(e.advice().payout, "kryptex");
    e.has_gpu = false;
    assert_eq!(e.advice().device, Device::Cpu);
}

#[test]
fn outdated_miners_are_announced_once_when_auto_update_is_off() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, _) = engine(d.path(), false);
    e.config.auto_update_miners = false;
    e.config.miner = crate::config::MinerConfig {
        path: Some("/old/xmrig".into()),
        version: Some("0.0.1".into()),
        sha256: None,
        verified: true,
    };
    // Picked by hand (not verified): never touched, never announced.
    e.config.srbminer = crate::config::MinerConfig {
        path: Some("/mine/SRBMiner-MULTI".into()),
        version: Some("0.0.1".into()),
        sha256: None,
        verified: false,
    };
    let e = Arc::new(Mutex::new(e));
    update_miners(&e);
    update_miners(&e);
    let g = e.lock().unwrap();
    let notes: Vec<_> = g
        .alerts
        .iter()
        .filter(|a| a.code == "miner_update_available")
        .collect();
    if install::manifest().asset_for_platform().is_ok() {
        assert_eq!(notes.len(), 1);
        assert!(notes[0].detail.as_deref().unwrap().starts_with("XMRig "));
    }
    assert_eq!(g.config.miner.path.as_deref(), Some("/old/xmrig"));
}

#[test]
fn manual_coin_choice_is_respected() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, s) = engine(d.path(), false);
    e.has_gpu = true;
    e.gpu_platform = true;
    e.config.srbminer.path = Some("/mock/SRBMiner-MULTI".into());
    let mut c = e.config().clone();
    c.wallet = "krxABC123".into();
    c.coin = "RVN".into();
    e.update_config(c).unwrap();
    e.start(false).unwrap();
    // PRL pays far more, but the user picked RVN.
    e.hashrates.insert("PRL".into(), 1000.0);
    e.market.coins = vec![market("PRL", 1.0), market("RVN", 1e-9)];
    e.tick();
    assert_eq!(s.coins.lock().unwrap().as_slice(), ["RVN"]);
    assert_eq!(e.status().coin.id, "RVN");
    assert!(!e.status().auto_coin);
}

#[test]
fn projection_uses_live_speed_and_deducts_fees() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, _) = engine(d.path(), false);
    e.has_gpu = false;
    assert!(e.status().projection.is_none()); // nothing measured, no market: no guess
    e.market.coins = vec![market("XMR", 1e-6)];
    e.market.btc = Some(BtcPrice {
        eur: 50_000.0,
        usd: 55_000.0,
    });
    e.start(false).unwrap();
    e.stats = Some(MinerStats {
        hashrate_60s: Some(1000.0),
        ..Default::default()
    });
    let p = e.status().projection.unwrap();
    assert!(p.live);
    let xmr = coins::find("XMR").unwrap();
    let expected = coins::btc_per_day(1000.0, &e.market.coins[0], xmr)
        * (1.0 - e.devfee.status().percent / 100.0);
    assert!((p.btc_per_day - expected).abs() < 1e-15);
    assert!((p.value_per_day.unwrap() - expected * 50_000.0).abs() < 1e-9);
    assert!(p.energy_cost_per_day.is_none());
}

#[test]
fn converted_payout_mines_on_unmineable_with_referral() {
    const LTC: &str = "LTC:LQ3B36Yv2rBTxdgAdYpU2UcEZsaNwXeATk";
    let d = tempfile::tempdir().unwrap();
    let (mut e, _) = engine(d.path(), false);
    let mut c = e.config().clone();
    c.wallet = LTC.into();
    c.pool.worker = "rig".into();
    e.update_config(c).unwrap();
    e.start(false).unwrap();
    let t = e.pool_target(Target::User);
    assert_eq!(t.url, "rx.unmineable.com:3333");
    assert_eq!(t.user, format!("{LTC}.rig#v3ni-3jdo"));
    // A donation raises the project's share, never past the cap.
    let mut c = e.config().clone();
    c.donation_percent = 1.0;
    e.update_config(c.clone()).unwrap();
    assert_eq!(e.devfee.status().percent, 3.0);
    c.donation_percent = 9.0;
    assert!(e.update_config(c).is_err());
}

#[test]
fn bitcoin_payout_mines_on_unmineable() {
    const BTC: &str = "bc1qqqqsyqcyq5rqwzqfpg9scrgwpugpzysn4v0345";
    let d = tempfile::tempdir().unwrap();
    let (mut e, s) = engine(d.path(), false);
    e.has_gpu = true;
    e.gpu_platform = true;
    e.config.srbminer.path = Some("/mock/SRBMiner-MULTI".into());
    let mut c = e.config().clone();
    c.wallet = BTC.into();
    e.update_config(c.clone()).unwrap();
    // Ravencoin is not on unMineable: never chosen, and refused if picked by hand.
    let rvn = coins::find("RVN").unwrap();
    assert_eq!(e.coin_blocker(rvn), Some("not_on_unmineable"));
    c.coin = "RVN".into();
    e.update_config(c).unwrap();
    assert_eq!(e.config().coin, AUTO_COIN);
    e.start(false).unwrap();
    assert_eq!(s.coins.lock().unwrap().as_slice(), ["PRL"]);
    let t = e.pool_target(Target::User);
    assert_eq!(t.url, "pearlpow.unmineable.com:3333");
    assert_eq!(
        t.user,
        format!("BTC:{BTC}.{}#v3ni-3jdo", crate::config::default_worker())
    );
    assert_eq!(t.pass, "x");
    // unMineable's fee replaces Kryptex's in estimates.
    e.market.coins = vec![market("PRL", 1e-6)];
    assert_eq!(
        e.coin_market("PRL").unwrap().pool_fee_percent,
        coins::UNMINEABLE_FEE_PERCENT
    );
    // CPU fallback for BTC payouts is RandomX on unMineable too.
    e.stop("user");
    e.has_gpu = false;
    e.start(false).unwrap();
    assert_eq!(e.pool_target(Target::User).url, "rx.unmineable.com:3333");
}

#[test]
#[ignore = "Monero/BTC payouts are off for now: Kryptex only"]
fn coin_wallet_overrides_the_main_payout_for_that_coin() {
    let prl_addr = format!("prl1p{}", "q".repeat(58));
    let d = tempfile::tempdir().unwrap();
    let (mut e, _) = engine(d.path(), false);
    e.has_gpu = true;
    e.gpu_platform = true;
    e.config.srbminer.path = Some("/mock/SRBMiner-MULTI".into());
    // Main payout is a Monero address: GPU coins are blocked…
    let prl = coins::find("PRL").unwrap();
    assert_eq!(e.coin_blocker(prl), Some("kryptex_or_btc_required"));
    // …until the user adds an own Pearl wallet.
    let mut c = e.config().clone();
    c.coin_wallets.insert("PRL".into(), prl_addr.clone());
    c.coin = "PRL".into();
    e.update_config(c).unwrap();
    assert_eq!(e.coin_blocker(prl), None);
    e.start(false).unwrap();
    let t = e.pool_target(Target::User);
    assert_eq!(t.url, "prl.kryptex.network:7048");
    assert_eq!(
        t.user,
        format!("{prl_addr}/{}", crate::config::default_worker())
    );
    // Bad addresses are refused.
    let mut c = e.config().clone();
    c.coin_wallets.insert("ETC".into(), "0xnope".into());
    assert!(e.update_config(c).is_err());
}

#[test]
fn mined_coins_are_totalled_per_coin() {
    let d = tempfile::tempdir().unwrap();
    let (mut e, _) = engine(d.path(), true);
    e.market.coins = vec![market("XMR", 1e-6)];
    e.market.network = Some(NetworkStats {
        difficulty: 1e6,
        block_reward: 1.0,
        height: None,
    });
    e.start(false).unwrap();
    for _ in 0..10 {
        e.last_tick = Some(Instant::now() - Duration::from_secs(2));
        e.tick();
    }
    e.stop("user");
    e.mined_at = None;
    e.refresh_mined();
    let m = &e.status().mined;
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].coin, "XMR");
    // 1000 H/s (mock) × ~20 s × 1e-6 XMR/hash.
    assert!(
        (m[0].amount - 1000.0 * m[0].secs * 1e-6).abs() < 1e-9,
        "{:?}",
        m[0]
    );
}

#[test]
fn engine_loops_survive_a_panicking_round() {
    let n = Arc::new(AtomicU32::new(0));
    let n2 = n.clone();
    every(Duration::from_millis(5), "test", move || {
        if n2.fetch_add(1, Ordering::SeqCst) == 0 {
            panic!("first round fails");
        }
    });
    let t = Instant::now();
    while n.load(Ordering::SeqCst) < 3 && t.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        n.load(Ordering::SeqCst) >= 3,
        "the loop kept going after the panic"
    );
}

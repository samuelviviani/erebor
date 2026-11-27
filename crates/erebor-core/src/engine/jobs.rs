//! Background jobs that run without holding the engine lock: miner install and update,
//! benchmark, and the engine loops.

use super::*;

/// Miners Erebor installed that are older than the build pinned in this app version:
/// update them in the background (or only announce it, when automatic updates are off).
/// The new build goes to its own folder, so a running miner is untouched; it is used from
/// the next start. Binaries the user picked by hand are never replaced.
pub(super) fn update_miners(engine: &Arc<Mutex<Engine>>) {
    for kind in MinerKind::ALL {
        let pinned = install::manifest_for(kind);
        let dir = {
            let mut e = lock(engine);
            let m = e.config.miner_for(kind);
            let outdated = m.verified
                && m.path.is_some()
                && m.version.as_deref() != Some(pinned.version.as_str());
            if !outdated || e.config.demo_mode || pinned.asset_for_platform().is_err() {
                continue;
            }
            let key = format!("{}-{}", kind.dir_prefix(), pinned.version);
            if !e.config.auto_update_miners {
                if e.update_notified.insert(key) {
                    let what = format!("{} {}", kind.label(), pinned.version);
                    e.alert(Level::Info, "miner_update_available", Some(what));
                }
                continue;
            }
            e.data_dir.join("miners")
        };
        let res = install::install_pinned(kind, &dir, &|_, _| {});
        let mut e = lock(engine);
        match res {
            Ok(inst) => {
                *e.config.miner_for_mut(kind) = crate::config::MinerConfig {
                    path: Some(inst.path.to_string_lossy().into_owned()),
                    version: Some(inst.version.clone()),
                    sha256: Some(inst.sha256),
                    verified: true,
                };
                let saved = e.save_config();
                let what = format!("{} {}", kind.label(), inst.version);
                match saved {
                    Ok(()) => e.alert(Level::Info, "miner_updated", Some(what)),
                    Err(err) => e.alert(Level::Warning, "miner_update_failed", Some(err)),
                }
            }
            Err(err) => {
                // Retried at the next market refresh; said once per version.
                let key = format!("failed-{}-{}", kind.dir_prefix(), pinned.version);
                if e.update_notified.insert(key) {
                    e.alert(Level::Warning, "miner_update_failed", Some(err));
                }
            }
        }
    }
}

/// Download, verify and install the pinned release of `kind`.
pub fn spawn_install(engine: Arc<Mutex<Engine>>, kind: MinerKind) -> Result<(), String> {
    let (job, miners_dir) = {
        let e = lock(&engine);
        if e.job_running() {
            return Err("job_running".into());
        }
        if e.is_active() {
            return Err("stop_mining_first".into());
        }
        install::manifest_for(kind).asset_for_platform()?;
        e.cancel_job.store(false, Ordering::Relaxed);
        (e.job.clone(), e.data_dir.join("miners"))
    };
    let mk = |progress: f64, value: Option<f64>| JobStatus {
        kind: "install",
        progress,
        done: false,
        error: None,
        value,
    };
    set_job(&job, mk(0.0, None));
    std::thread::spawn(move || {
        let j = job.clone();
        let res = install::install_pinned(kind, &miners_dir, &move |got, total| {
            let p = total.map(|t| got as f64 / t.max(1) as f64).unwrap_or(0.0);
            set_job(&j, mk(p.min(0.99), Some(got as f64)));
        });
        let mut e = lock(&engine);
        let mut status = mk(1.0, None);
        status.done = true;
        match res {
            Ok(inst) => {
                *e.config.miner_for_mut(kind) = crate::config::MinerConfig {
                    path: Some(inst.path.to_string_lossy().into_owned()),
                    version: Some(inst.version),
                    sha256: Some(inst.sha256),
                    verified: true,
                };
                if let Err(err) = e.save_config() {
                    status.error = Some(err);
                }
                e.alert(Level::Info, "miner_installed", None);
            }
            Err(err) => status.error = Some(err),
        }
        set_job(&job, status);
    });
    Ok(())
}

/// Offline benchmark with declared duration (BENCHMARK_SECS) and thread share.
pub fn spawn_benchmark(engine: Arc<Mutex<Engine>>) -> Result<(), String> {
    let (job, cancel, binary, threads, version) = {
        let e = lock(&engine);
        if e.job_running() {
            return Err("job_running".into());
        }
        if e.is_active() {
            return Err("stop_mining_first".into());
        }
        if e.config.demo_mode {
            return Err("not_in_demo".into());
        }
        let path = e.config.miner.path.clone().ok_or("miner_not_configured")?;
        let binary = xmrig::validate_binary_path(&path, "xmrig")?;
        e.cancel_job.store(false, Ordering::Relaxed);
        (
            e.job.clone(),
            e.cancel_job.clone(),
            binary,
            e.config.performance.limits().0,
            e.config.miner.version.clone(),
        )
    };
    let mk = |progress: f64, value: Option<f64>| JobStatus {
        kind: "benchmark",
        progress,
        done: false,
        error: None,
        value,
    };
    set_job(&job, mk(0.0, None));
    std::thread::spawn(move || {
        let j = job.clone();
        let res = xmrig::run_benchmark(
            &binary,
            threads,
            Duration::from_secs(BENCHMARK_SECS),
            &cancel,
            &move |p, h| set_job(&j, mk(p.min(0.99), h)),
        );
        let mut status = mk(1.0, None);
        status.done = true;
        match res {
            Ok(h) => {
                status.value = Some(h);
                let mut e = lock(&engine);
                e.config.benchmark = Some(crate::config::BenchmarkResult {
                    hashrate: h,
                    threads_percent: threads,
                    duration_secs: BENCHMARK_SECS as u32,
                    at: now_unix(),
                    miner_version: version,
                });
                let _ = e.save_config();
            }
            Err(err) => status.error = Some(err),
        }
        set_job(&job, status);
    });
    Ok(())
}

/// Background loops: engine tick every TICK, market data every 10 minutes.
pub fn spawn_loops(engine: Arc<Mutex<Engine>>) {
    let e2 = engine.clone();
    every(TICK, "tick", move || lock(&e2).tick());
    // The meter is read outside the engine lock: a slow device never stalls the UI.
    let e3 = engine.clone();
    every(Duration::from_secs(15), "sensors", move || {
        let (solar, room, plug, zone) = {
            let e = lock(&e3);
            let on = !e.config.demo_mode;
            let en = &e.config.energy;
            (
                (on && en.solar.enabled).then(|| en.solar.clone()),
                (on && en.thermostat.enabled).then(|| en.thermostat.clone()),
                (on && en.plug.enabled).then(|| en.plug.clone()),
                e.spot_due(),
            )
        };
        if let Some(p) = plug {
            let r = energy::read_number(&p.url, &p.token, &p.field);
            lock(&e3).set_plug(r);
        }
        if let Some(z) = zone {
            let r = energy::fetch_spot_prices(&z);
            lock(&e3).set_spot(r);
        }
        if let Some(s) = solar {
            let r = energy::read_solar(&s);
            lock(&e3).set_solar(r);
        }
        if let Some(t) = room {
            let r = energy::read_number(&t.url, &t.token, &t.field);
            lock(&e3).set_room(r);
        }
    });
    every(Duration::from_secs(600), "market", move || {
        let (base, demo) = {
            let e = lock(&engine);
            (e.market_api_base(), e.config.demo_mode)
        };
        if demo {
            // Demo mode: fixed, clearly simulated market values; no network calls.
            lock(&engine).set_market(
                Ok(NetworkStats {
                    difficulty: 300e9,
                    block_reward: 0.6,
                    height: None,
                }),
                Ok(Price {
                    eur: 150.0,
                    usd: 160.0,
                    source: "demo",
                }),
                Ok((
                    vec![CoinMarket {
                        id: "XMR".into(),
                        coin_per_hash: 0.6 / 300e9,
                        price_btc: 0.0026,
                        pool_fee_percent: 1.0,
                    }],
                    Some(BtcPrice {
                        eur: 58_000.0,
                        usd: 62_000.0,
                    }),
                )),
            );
        } else {
            let n = pools::fetch_network_stats(base);
            let p = pools::fetch_price();
            let c = coins::fetch_markets();
            let t = coins::fetch_gpu_table();
            {
                let mut e = lock(&engine);
                e.set_market(n, p, c);
                if let Some(t) = t {
                    e.set_gpu_table(&t);
                }
            }
            update_miners(&engine);
        }
    });
}

/// Run `f` now and then every `period`, for good: a round that panics is logged and the
/// next one runs anyway (the tick is what stops an overheating or crashed miner).
pub(super) fn every(period: Duration, name: &'static str, f: impl Fn() + Send + 'static) {
    std::thread::spawn(move || loop {
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(&f)).is_err() {
            eprintln!("engine {name} loop: a round panicked; carrying on");
        }
        std::thread::sleep(period);
    });
}

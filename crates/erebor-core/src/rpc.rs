//! Every command the GUI can send, by name, with JSON arguments. One dispatcher for
//! both transports: Tauri IPC (desktop app) and the local socket (`erebor` CLI, see `ipc`).

use crate::config::{self, AppConfig};
use crate::engine::{self, Engine};
use crate::miner::MinerKind;
use crate::{devfee, fleet, install, pools, wallet, xmrig};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex, MutexGuard};

pub type Shared = Arc<Mutex<Engine>>;
type R = Result<Value, Value>;

fn lock(e: &Shared) -> MutexGuard<'_, Engine> {
    e.lock().unwrap_or_else(|p| p.into_inner())
}

fn ok<T: Serialize>(v: T) -> R {
    serde_json::to_value(v).map_err(|e| Value::String(e.to_string()))
}

fn err<E: Serialize>(e: E) -> Value {
    serde_json::to_value(e).unwrap_or(Value::Null)
}

fn arg<T: DeserializeOwned>(args: &Value, name: &str) -> Result<T, Value> {
    serde_json::from_value(args.get(name).cloned().unwrap_or(Value::Null))
        .map_err(|_| Value::String(format!("bad_argument:{name}")))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: &'static str,
    platform: String,
    dev_fee_percent: f64,
    dev_fee_wallet_configured: bool,
    xmrig_pinned_version: String,
    xmrig_pinned_available: bool,
    xmrig_donate_percent: f64,
    srbminer_pinned_version: String,
    srbminer_pinned_available: bool,
    lolminer_pinned_version: String,
    lolminer_pinned_available: bool,
    cpuminer_pinned_version: String,
    cpuminer_pinned_available: bool,
    repo_url: Option<&'static str>,
    hostname: String,
}

pub fn hostname() -> String {
    sysinfo::System::host_name().unwrap_or_else(|| "Erebor".into())
}

fn app_info() -> AppInfo {
    let m = install::manifest();
    let srb = install::manifest_for(MinerKind::Srbminer);
    let lol = install::manifest_for(MinerKind::Lolminer);
    let cpm = install::manifest_for(MinerKind::Cpuminer);
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        platform: install::platform_key(),
        dev_fee_percent: devfee::DEFAULT_FEE_PERCENT,
        dev_fee_wallet_configured: devfee::build_wallet().is_some()
            || devfee::build_btc_wallet().is_some(),
        xmrig_pinned_available: m.asset_for_platform().is_ok(),
        xmrig_pinned_version: m.version,
        xmrig_donate_percent: xmrig::XMRIG_DONATE_LEVEL as f64,
        srbminer_pinned_available: srb.asset_for_platform().is_ok(),
        srbminer_pinned_version: srb.version,
        lolminer_pinned_available: lol.asset_for_platform().is_ok(),
        lolminer_pinned_version: lol.version,
        cpuminer_pinned_available: cpm.asset_for_platform().is_ok(),
        cpuminer_pinned_version: cpm.version,
        repo_url: option_env!("EREBOR_REPO_URL"),
        hostname: hostname(),
    }
}

/// Run `cmd`. Blocking (some commands reach the network): call it off the UI thread.
pub fn call(e: &Shared, cmd: &str, args: &Value) -> R {
    match cmd {
        "app_info" => ok(app_info()),
        "get_config" => ok(lock(e).config()),
        "update_config" => {
            let c: AppConfig = arg(args, "config")?;
            let missing = {
                let mut g = lock(e);
                g.update_config(c).map_err(err)?;
                // A coin picked by hand whose miner is not installed yet: install the
                // verified build right away (best effort: the UI shows the job).
                crate::coins::find(&g.config().coin)
                    .map(|c| c.miner)
                    .filter(|k| !g.config().demo_mode && g.config().miner_for(*k).path.is_none())
            };
            if let Some(kind) = missing {
                let _ = engine::spawn_install(e.clone(), kind);
            }
            ok(lock(e).config())
        }
        "validate_wallet" => {
            let a: String = arg(args, "address")?;
            wallet::validate_user_payout(&a)
                .map(|v| json!(v))
                .map_err(err)
        }
        "validate_coin_wallet" => {
            let (c, a): (String, String) = (arg(args, "coin")?, arg(args, "address")?);
            wallet::validate_coin_address(&c, &a)
                .map(|_| Value::Null)
                .map_err(err)
        }
        "pool_catalog" => ok(pools::CATALOG),
        // Pools each coin can be mined on with an own wallet (Kryptex first).
        "coin_pools" => ok(crate::coins::COINS
            .iter()
            .map(|c| (c.id, pools::coin_pools(c)))
            .collect::<std::collections::BTreeMap<_, _>>()),
        "check_pool" => {
            let (host, port): (String, u16) = (arg(args, "host")?, arg(args, "port")?);
            if !config::valid_host(&host) || port == 0 {
                return Err(err("invalid_host"));
            }
            pools::check_reachable(&host, port)
                .map(|v| json!(v))
                .map_err(err)
        }
        "pool_account" => {
            let (id, w, demo) = {
                let g = lock(e);
                let c = g.config();
                (c.pool.id.clone(), c.wallet.clone(), c.demo_mode)
            };
            if demo {
                return Err(err("not_in_demo"));
            }
            let pool = pools::find(&id).ok_or_else(|| err("pool_api_unavailable"))?;
            pools::fetch_account(pool, &w)
                .map(|v| json!(v))
                .map_err(err)
        }
        "detect_hardware" => ok(lock(e).detect_hardware()),
        "install_miner" => {
            let kind: Option<MinerKind> = arg(args, "kind")?;
            engine::spawn_install(e.clone(), kind.unwrap_or(MinerKind::Xmrig))
                .map(|_| Value::Null)
                .map_err(err)
        }
        "select_miner" => {
            let path: String = arg(args, "path")?;
            let kind: Option<MinerKind> = arg(args, "kind")?;
            let mut g = lock(e);
            g.select_miner_kind(kind.unwrap_or(MinerKind::Xmrig), &path)
                .map_err(err)?;
            ok(g.config())
        }
        "allow_in_defender" => {
            Engine::allow_in_defender(e).map_err(err)?;
            ok(lock(e).config())
        }
        "coin_options" => ok(lock(e).coin_options()),
        "run_benchmark" => engine::spawn_benchmark(e.clone())
            .map(|_| Value::Null)
            .map_err(err),
        "cancel_job" => {
            lock(e).cancel_job();
            Ok(Value::Null)
        }
        "start_mining" => lock(e).start(false).map(|_| Value::Null).map_err(err),
        "stop_mining" => {
            lock(e).stop("user");
            Ok(Value::Null)
        }
        "tune_power" => lock(e).start_tune().map(|_| Value::Null).map_err(err),
        "prepare_update" => {
            lock(e).prepare_update();
            Ok(Value::Null)
        }
        "test_notify" => {
            let (n, title, body) = lock(e).test_notify().map_err(err)?;
            crate::notify::send(&n, &title, &body)
                .map(|_| Value::Null)
                .map_err(err)
        }
        "stop_for" => {
            let minutes: u64 = arg(args, "minutes")?;
            lock(e).stop_for(minutes.clamp(1, 24 * 60) * 60);
            Ok(Value::Null)
        }
        "pause_mining" => lock(e).pause().map(|_| Value::Null).map_err(err),
        "resume_mining" => lock(e).resume().map(|_| Value::Null).map_err(err),
        "get_status" => ok(lock(e).status()),
        // Stable, versioned subset of the status for scripts, the TUI and fleet views.
        "snapshot" => {
            let g = lock(e);
            ok(fleet::Snapshot::new(&g.status(), g.config()))
        }
        "device_info" => fleet::report(&mut lock(e)).map(|r| json!(r)).map_err(err),
        "set_device_name" => {
            let name: String = arg(args, "name")?;
            let dir = lock(e).data_dir().to_path_buf();
            fleet::set_name(&dir, &name).map(|d| json!(d)).map_err(err)
        }
        "get_logs" => ok(lock(e).logs()),
        "get_stats" => {
            let range: i64 = arg(args, "rangeSecs")?;
            lock(e).stats_report(range).map(|v| json!(v)).map_err(err)
        }
        "export_config" => ok(lock(e).export_config()),
        "export_csv" => {
            let range: i64 = arg(args, "rangeSecs")?;
            lock(e).export_csv(range).map(|v| json!(v)).map_err(err)
        }
        "save_csv" => {
            let range: i64 = arg(args, "rangeSecs")?;
            lock(e).save_csv(range).map(|v| json!(v)).map_err(err)
        }
        "delete_all_data" => {
            let mut g = lock(e);
            g.delete_all_data().map_err(err)?;
            ok(g.config())
        }
        "data_dir" => ok(lock(e).data_dir().to_string_lossy()),
        _ => Err(err("unknown_command")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatches_commands() {
        let dir = tempfile::tempdir().unwrap();
        let e: Shared = Arc::new(Mutex::new(
            Engine::new(dir.path().into(), engine::default_factory()).unwrap(),
        ));
        let c = call(&e, "get_config", &Value::Null).unwrap();
        assert_eq!(c["coin"], "AUTO");
        assert_eq!(
            call(&e, "nope", &Value::Null),
            Err(json!("unknown_command"))
        );
        assert_eq!(
            call(&e, "get_stats", &json!({})),
            Err(json!("bad_argument:rangeSecs"))
        );
        // One engine per data dir.
        assert_eq!(
            Engine::new(dir.path().into(), engine::default_factory()).err(),
            Some("engine_busy".into())
        );
    }
}

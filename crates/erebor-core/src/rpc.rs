//! Every command the GUI can send, by name, with JSON arguments. One dispatcher for
//! both transports: Tauri IPC (local app) and HTTP (remote node, see `server`).

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
    repo_url: Option<&'static str>,
    hostname: String,
}

pub fn hostname() -> String {
    sysinfo::System::host_name().unwrap_or_else(|| "Erebor".into())
}

fn app_info() -> AppInfo {
    let m = install::manifest();
    let srb = install::manifest_for(MinerKind::Srbminer);
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
        repo_url: option_env!("EREBOR_REPO_URL"),
        hostname: hostname(),
    }
}

/// Run `cmd`. Blocking (some commands reach the network): call it off the UI thread.
/// `remote`: the caller is another device; commands that pick local files are refused.
pub fn call(e: &Shared, cmd: &str, args: &Value, remote: bool) -> R {
    match cmd {
        "app_info" => ok(app_info()),
        "get_config" => ok(lock(e).config()),
        "update_config" => {
            let c: AppConfig = arg(args, "config")?;
            let mut g = lock(e);
            g.update_config(c).map_err(err)?;
            ok(g.config())
        }
        "validate_wallet" => {
            let a: String = arg(args, "address")?;
            wallet::validate_payout(&a).map(|v| json!(v)).map_err(err)
        }
        "validate_coin_wallet" => {
            let (c, a): (String, String) = (arg(args, "coin")?, arg(args, "address")?);
            wallet::validate_coin_address(&c, &a)
                .map(|_| Value::Null)
                .map_err(err)
        }
        "pool_catalog" => ok(pools::CATALOG),
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
            engine::spawn_install_kind(e.clone(), kind.unwrap_or(MinerKind::Xmrig))
                .map(|_| Value::Null)
                .map_err(err)
        }
        "select_miner" => {
            if remote {
                return Err(err("local_only"));
            }
            let path: String = arg(args, "path")?;
            let kind: Option<MinerKind> = arg(args, "kind")?;
            let mut g = lock(e);
            g.select_miner_kind(kind.unwrap_or(MinerKind::Xmrig), &path)
                .map_err(err)?;
            ok(g.config())
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
        "delete_all_data" => {
            let mut g = lock(e);
            g.delete_all_data().map_err(err)?;
            ok(g.config())
        }
        "dismiss_alert" => {
            let id: u64 = arg(args, "id")?;
            lock(e).dismiss_alert(id);
            Ok(Value::Null)
        }
        "data_dir" => ok(lock(e).data_dir().to_string_lossy()),
        _ => Err(err("unknown_command")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatches_and_refuses_local_only_commands_remotely() {
        let dir = tempfile::tempdir().unwrap();
        let e: Shared = Arc::new(Mutex::new(
            Engine::new(dir.path().into(), engine::default_factory()).unwrap(),
        ));
        let c = call(&e, "get_config", &Value::Null, true).unwrap();
        assert_eq!(c["coin"], "AUTO");
        assert_eq!(
            call(&e, "select_miner", &json!({"path": "/bin/sh"}), true),
            Err(json!("local_only"))
        );
        assert_eq!(
            call(&e, "nope", &Value::Null, false),
            Err(json!("unknown_command"))
        );
        assert_eq!(
            call(&e, "get_stats", &json!({}), false),
            Err(json!("bad_argument:rangeSecs"))
        );
        // One engine per data dir.
        assert_eq!(
            Engine::new(dir.path().into(), engine::default_factory()).err(),
            Some("engine_busy".into())
        );
    }
}

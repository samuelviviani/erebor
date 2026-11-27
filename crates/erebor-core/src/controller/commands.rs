//! The GUI's `fleet_*` commands and LAN discovery.

use super::*;

/// LAN discovery, with whether each agent is already in the registry.
pub fn discover(f: &Fleet) -> Result<Value, String> {
    if !f.settings().discovery {
        return Err("discovery_disabled".into());
    }
    let known: Vec<String> = f.devices().into_iter().map(|d| d.id).collect();
    let found = client::discover(remote::PORT, Duration::from_millis(1500))?;
    Ok(json!(found
        .into_iter()
        .map(|x| {
            let paired = known.contains(&format!("agent:{}", x.id));
            json!({"found": x, "paired": paired})
        })
        .collect::<Vec<_>>()))
}

fn arg<T: for<'de> Deserialize<'de>>(args: &Value, k: &str) -> Result<T, Value> {
    serde_json::from_value(args.get(k).cloned().unwrap_or(Value::Null))
        .map_err(|_| Value::from(format!("bad_argument:{k}")))
}

/// The GUI's fleet commands (`fleet_*`), same shape as `rpc::call`.
pub fn call(f: &Fleet, cmd: &str, args: &Value) -> Result<Value, Value> {
    let ids = || -> Result<Vec<DeviceRec>, Value> {
        let ids: Vec<String> = arg(args, "ids")?;
        f.select(&ids, None, false).map_err(Value::from)
    };
    let r: Result<Value, String> = match cmd {
        "fleet_list" => {
            let rows = f.rows();
            Ok(
                json!({"overview": f.overview(&rows), "devices": rows, "groups": f.groups(), "identity": remote::short_fp(&f.identity.fingerprint)}),
            )
        }
        "fleet_refresh" => {
            f.refresh_once();
            Ok(Value::Null)
        }
        "fleet_discover" => discover(f),
        "fleet_add_agent" => f
            .add_agent(
                &arg::<String>(args, "address")?,
                &arg::<String>(args, "code")?,
                arg(args, "kind").ok(),
            )
            .map(|d| json!(d)),
        "fleet_add_asic" => f
            .add_asic(
                &arg::<String>(args, "host")?,
                arg::<Option<u16>>(args, "port")?.unwrap_or(0),
                &arg::<Option<String>>(args, "name")?.unwrap_or_default(),
                arg::<Option<String>>(args, "via")?.as_deref(),
            )
            .map(|d| json!(d)),
        "fleet_scan" => {
            let cidr: String = arg(args, "cidr")?;
            match arg::<Option<String>>(args, "via")? {
                Some(g) => f
                    .device(&g)
                    .and_then(|gw| f.agent_call(&gw, "asics.scan", &json!({"cidr": cidr}))),
                None => asic::scan(&cidr, 0).map(|v| json!(v)),
            }
        }
        "fleet_remove" => f.remove(&arg::<String>(args, "id")?).map(|_| Value::Null),
        "fleet_rename" => f
            .rename(&arg::<String>(args, "id")?, &arg::<String>(args, "name")?)
            .map(|_| Value::Null),
        "fleet_set_kind" => f
            .set_kind(&arg::<String>(args, "id")?, arg(args, "kind")?)
            .map(|_| Value::Null),
        "fleet_set_price" => f
            .set_energy_price(&arg::<String>(args, "id")?, arg(args, "price")?)
            .map(|_| Value::Null),
        "fleet_mining" => Ok(json!(f.mining(&ids()?, &arg::<String>(args, "action")?))),
        "fleet_apply" => {
            let p = match args.get("profile") {
                Some(Value::Object(_)) => arg::<Profile>(args, "profile")?,
                _ => f
                    .profile(&arg::<String>(args, "profileId")?)
                    .map_err(Value::from)?,
            };
            Ok(json!(f.apply(&p, &ids()?)))
        }
        "fleet_assign_group" => f
            .assign_group(&arg::<Vec<String>>(args, "ids")?, arg(args, "group")?)
            .map(|n| json!(n)),
        "fleet_tag" => f
            .tag(
                &arg::<Vec<String>>(args, "ids")?,
                &arg::<Vec<String>>(args, "tags")?,
                arg::<Option<bool>>(args, "remove")? != Some(true),
            )
            .map(|_| Value::Null),
        "fleet_groups" => Ok(json!(f.groups())),
        "fleet_save_group" => f.save_group(&arg(args, "group")?).map(|g| json!(g)),
        "fleet_delete_group" => f.delete_group(arg(args, "id")?).map(|_| Value::Null),
        "fleet_profiles" => Ok(json!(f.profiles())),
        "fleet_save_profile" => f.save_profile(&arg(args, "profile")?).map(|p| json!(p)),
        "fleet_delete_profile" => f.delete_profile(arg(args, "id")?).map(|_| Value::Null),
        "fleet_config" => f.config(&arg::<String>(args, "id")?),
        "fleet_set_config" => {
            f.set_config(&arg::<String>(args, "id")?, &arg::<Value>(args, "config")?)
        }
        "fleet_query" => f.query(
            &arg::<String>(args, "id")?,
            &arg::<String>(args, "cmd")?,
            args.get("args").unwrap_or(&Value::Null),
        ),
        "fleet_history" => f.history(
            arg::<Option<String>>(args, "id")?.as_deref(),
            arg(args, "rangeSecs")?,
        ),
        "fleet_events" => Ok(json!(f.events(
            arg::<Option<i64>>(args, "sinceId")?.unwrap_or(0),
            arg::<Option<i64>>(args, "limit")?.unwrap_or(200)
        ))),
        "fleet_settings" => Ok(json!(f.settings())),
        "fleet_set_settings" => f
            .set_settings(&arg(args, "settings")?)
            .map(|_| json!(f.settings())),
        "fleet_export" => Ok(f.export(arg::<Option<bool>>(args, "secrets")?.unwrap_or(false))),
        "fleet_import" => f.import(&arg::<Value>(args, "data")?),
        _ => return Err("unknown_command".into()),
    };
    r.map_err(Value::from)
}

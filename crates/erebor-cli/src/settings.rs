//! Setup commands: wallet, pools, coins and `config get/set`.

use super::*;
use anstream::println;

pub(super) fn wallet_cmd(dir: &Path, cmd: WalletCmd, json: bool) -> Result<(), Fail> {
    match cmd {
        WalletCmd::Show => {
            let c = load_config(dir)?;
            let kind = wallet::payout_kind(&c.wallet).map(|k| format!("{k:?}"));
            if json {
                print_json(
                    &json!({"wallet": c.wallet, "kind": kind, "coinWallets": c.coin_wallets, "coin": c.coin}),
                );
                return Ok(());
            }
            if c.wallet.is_empty() {
                row("Payout", "not set  → erebor wallet set <ADDRESS>");
            } else {
                row(
                    "Payout",
                    format!(
                        "{} ({})",
                        c.wallet,
                        kind.unwrap_or_else(|| "unrecognized".into())
                    ),
                );
            }
            row(
                "Coin",
                if c.coin == AUTO_COIN {
                    "automatic".to_string()
                } else {
                    c.coin.clone()
                },
            );
            for (coin, w) in &c.coin_wallets {
                row(&format!("Wallet {coin}"), w);
            }
            Ok(())
        }
        WalletCmd::Set { address, coin } => {
            let address = address.trim().to_string();
            let mut c = load_config_live(dir)?;
            match coin {
                // Own wallet for one coin (pool: `config set coinPools.<COIN>…`; unset: the first listed, Kryptex if there).
                Some(coin) => {
                    let coin = coin.to_ascii_uppercase();
                    erebor_core::coins::find(&coin)
                        .ok_or_else(|| Fail::from("unknown_coin".to_string()))?;
                    wallet::validate_coin_address(&coin, &address)
                        .map_err(|e| Fail::from(format!("invalid_address: {e:?}")))?;
                    c.coin_wallets.insert(coin, address.clone());
                }
                None => {
                    wallet::validate_user_payout(&address)
                        .map_err(|e| Fail::from(format!("invalid_address: {e:?}")))?;
                    c.wallet = address;
                }
            }
            let before_coin = c.coin.clone();
            let saved = save_config(dir, &c)?;
            if json {
                print_json(
                    &json!({"wallet": saved.wallet, "coinWallets": saved.coin_wallets, "coin": saved.coin, "pool": saved.pool.id}),
                );
                return Ok(());
            }
            println!("{}", ui::ok("Wallet saved and validated."));
            if saved.coin != before_coin {
                println!("The selected coin cannot be paid to this wallet: coin set to automatic.");
            }
            if saved.pool.id != c.pool.id {
                println!(
                    "Pool changed to {} (required by this payout).",
                    saved.pool.id
                );
            }
            Ok(())
        }
    }
}

pub(super) fn pools_cmd(dir: &Path, cmd: PoolsCmd, json: bool) -> Result<(), Fail> {
    match cmd {
        PoolsCmd::List => {
            let c = load_config(dir)?;
            if json {
                print_json(&json!({"configured": c.pool.id, "pools": pools::CATALOG}));
                return Ok(());
            }
            for p in pools::CATALOG {
                let mark = if p.id == c.pool.id { "*" } else { " " };
                println!(
                    "{mark} {:<12} {:<14} {}:{}{} · fee {}%",
                    p.id,
                    p.name,
                    p.host,
                    p.port,
                    if p.tls { " (TLS)" } else { "" },
                    p.fee_percent
                );
            }
            if c.pool.id == "custom" {
                println!("* custom       {}:{}", c.pool.host, c.pool.port);
            }
            Ok(())
        }
        PoolsCmd::Test { all } => {
            let c = load_config(dir)?;
            let targets: Vec<(String, String, u16)> = if all {
                pools::CATALOG
                    .iter()
                    .map(|p| (p.id.to_string(), p.host.to_string(), p.port))
                    .collect()
            } else {
                vec![(c.pool.id.clone(), c.pool.host.clone(), c.pool.port)]
            };
            let payout = wallet::payout_kind(&c.wallet);
            let mut results = vec![];
            let mut configured_ok = true;
            for (id, host, port) in targets {
                let reach = pools::check_reachable(&host, port);
                let compat = match payout {
                    Some(wallet::AddressKind::Kryptex) if !pools::is_kryptex_host(&host) => {
                        Some("a Kryptex ID only works on Kryptex")
                    }
                    Some(wallet::AddressKind::Bitcoin | wallet::AddressKind::Converted) => {
                        Some("unMineable payouts mine on unMineable; this pool is not used")
                    }
                    _ => None,
                };
                if id == c.pool.id
                    && (reach.is_err() || compat.is_some_and(|m| m.starts_with("a Kryptex")))
                {
                    configured_ok = false;
                }
                results.push(json!({
                    "id": id, "host": host, "port": port,
                    "reachable": reach.is_ok(),
                    "latencyMs": reach.as_ref().ok(),
                    "error": reach.as_ref().err(),
                    "compatibility": compat,
                }));
                if !json {
                    let r = match &reach {
                        Ok(ms) => format!("✓ reachable ({ms} ms)"),
                        Err(e) => format!("✗ {e}"),
                    };
                    println!(
                        "{id:<12} {host}:{port}  {r}{}",
                        compat.map(|m| format!("  ! {m}")).unwrap_or_default()
                    );
                }
            }
            if json {
                print_json(&json!({"ok": configured_ok, "results": results}));
            }
            if !configured_ok {
                return fail("pool_check_failed");
            }
            Ok(())
        }
        PoolsCmd::Set { pool_id } => {
            let p = pools::find(&pool_id).ok_or_else(|| Fail::from("unknown_pool".to_string()))?;
            let mut c = load_config_live(dir)?;
            c.pool = PoolConfig {
                id: p.id.into(),
                host: p.host.into(),
                port: p.port,
                tls: p.tls,
                worker: c.pool.worker.clone(),
            };
            let saved = save_config(dir, &c)?;
            if json {
                print_json(&saved.pool);
            } else {
                println!(
                    "Pool set: {} ({}:{}). Running mining switches over automatically.",
                    p.name, p.host, p.port
                );
            }
            Ok(())
        }
    }
}

/// `a.b.c` in a JSON object; `None` when a key does not exist.
pub(super) fn at<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    key.split('.').try_fold(v, |v, k| v.get(k))
}

/// `key = value` lines for every leaf under `v`.
pub(super) fn flatten(prefix: &str, v: &Value, out: &mut Vec<(String, String)>) {
    match v {
        Value::Object(m) => {
            for (k, v) in m {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten(&key, v, out);
            }
        }
        Value::String(s) => out.push((prefix.into(), s.clone())),
        v => out.push((prefix.into(), v.to_string())),
    }
}

/// The new value for `key`: JSON when it parses, else text. Text stays text for
/// string settings ("1234" as a worker name).
fn parse_value(current: &Value, raw: &str) -> Value {
    match (current, serde_json::from_str::<Value>(raw)) {
        (Value::String(_), Ok(Value::String(s))) => Value::String(s),
        (Value::String(_), _) => Value::String(raw.into()),
        (_, Ok(v)) => v,
        (_, Err(_)) => Value::String(raw.into()),
    }
}

pub(super) fn set_key(cfg: &mut Value, key: &str, raw: &str) -> Result<(), Fail> {
    let slot = key
        .split('.')
        .try_fold(cfg, |v, k| v.get_mut(k))
        .ok_or_else(|| Fail::from(format!("unknown_key: {key}")))?;
    if slot.is_object() {
        return Err(format!("not_a_setting: {key} is a section").into());
    }
    *slot = parse_value(slot, raw);
    Ok(())
}

pub(super) fn config_cmd(dir: &Path, cmd: ConfigCmd, json: bool) -> Result<(), Fail> {
    let to_value = |c: &AppConfig| serde_json::to_value(c).unwrap_or_default();
    let print = |v: &Value, prefix: &str| {
        if json {
            print_json(v);
            return;
        }
        let mut rows = vec![];
        flatten(prefix, v, &mut rows);
        let w = rows.iter().map(|r| r.0.len()).max().unwrap_or(0);
        for (k, v) in rows {
            println!("{}  {v}", ui::label(&k, w));
        }
    };
    match cmd {
        ConfigCmd::Show => print(&to_value(&load_config(dir)?), ""),
        ConfigCmd::Get { key } => {
            let c = to_value(&load_config(dir)?);
            let v = at(&c, &key).ok_or_else(|| Fail::from(format!("unknown_key: {key}")))?;
            match v {
                Value::Object(_) => print(v, &key),
                Value::String(s) if !json => println!("{s}"),
                v if !json => println!("{v}"),
                v => print_json(v),
            }
        }
        ConfigCmd::Set { key, value } => {
            let mut c = to_value(&load_config_live(dir)?);
            set_key(&mut c, &key, &value)?;
            let wanted = at(&c, &key).cloned().unwrap_or_default();
            let new: AppConfig = serde_json::from_value(c)
                .map_err(|e| Fail::from(format!("invalid_value: {key}: {e}")))?;
            let saved = to_value(&save_config(dir, &new)?);
            let got = at(&saved, &key).cloned().unwrap_or_default();
            if json {
                print_json(&json!({ "key": key, "value": got }));
            } else if got == wanted {
                println!("{}", ui::ok(format!("{key} = {}", display(&got))));
            } else {
                // The engine owns some keys (miner paths, version) or adjusts a value.
                println!(
                    "{}",
                    ui::warn(format!("{key} kept by Erebor: {}", display(&got)))
                );
            }
        }
    }
    Ok(())
}

fn display(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        v => v.to_string(),
    }
}

pub(super) fn coins_cmd(dir: &Path, cmd: CoinsCmd, json: bool) -> Result<(), Fail> {
    if let CoinsCmd::Set { coin } = cmd {
        let id = coin.trim().to_uppercase();
        if id != AUTO_COIN && erebor_core::coins::find(&id).is_none() {
            return fail("unknown_coin");
        }
        let mut c = load_config_live(dir)?;
        c.coin = id;
        let saved = save_config(dir, &c)?;
        if json {
            print_json(&json!({ "coin": saved.coin }));
        } else if saved.coin == AUTO_COIN && c.coin != AUTO_COIN {
            println!(
                "{}",
                ui::warn("This payout cannot mine that coin: kept automatic.")
            );
        } else {
            println!(
                "{}",
                ui::ok(format!(
                    "Coin set: {}. Running mining switches over now.",
                    if saved.coin == AUTO_COIN {
                        "auto"
                    } else {
                        saved.coin.as_str()
                    }
                ))
            );
            if ipc::snapshot(dir).is_ok_and(|s| s.job.is_some_and(|j| !j.done)) {
                println!("{}", ui::paint("Installing its miner first…", ui::DIM));
            }
        }
        return Ok(());
    }
    let opts = call(dir, "coin_options", Value::Null)?;
    let current = load_config(dir)?.coin;
    if json {
        print_json(&json!({ "coin": current, "options": opts }));
        return Ok(());
    }
    let currency = load_config(dir)?.energy.currency;
    println!(
        "{}",
        ui::paint(
            format!(
                "  {:<10}{:<18}{:<14}{:<5}{:>12}{:>12}  NOTE",
                "COIN",
                "NAME",
                "ALGO",
                "DEV",
                "HASHRATE",
                format!("{currency}/DAY")
            ),
            ui::DIM
        )
    );
    // Mineable first, best estimate first.
    let mut opts = opts.as_array().cloned().unwrap_or_default();
    opts.sort_by(|a, b| {
        let v = |o: &Value| o["valuePerDay"].as_f64().unwrap_or(-1.0);
        a["blocker"]
            .is_null()
            .cmp(&b["blocker"].is_null())
            .reverse()
            .then(v(b).total_cmp(&v(a)))
    });
    for o in &opts {
        let c = &o["coin"];
        let id = c["id"].as_str().unwrap_or("?");
        let mark = if id == current { "*" } else { " " };
        let note = match o["blocker"].as_str() {
            Some(b) => explain(b).0,
            None if c["asic"] == true => "ASIC coin: earns little".into(),
            None if o["minerInstalled"] == false => "miner installed on first use".into(),
            None => String::new(),
        };
        let line = format!(
            "{mark} {:<10}{:<18}{:<14}{:<5}{:>12}{:>12}  {note}",
            id,
            c["name"].as_str().unwrap_or(""),
            c["algo"].as_str().unwrap_or(""),
            c["device"].as_str().unwrap_or("").to_uppercase(),
            o["hashrate"]
                .as_f64()
                .map_or("-".into(), |h| fmt::hashrate(Some(h))),
            o["valuePerDay"]
                .as_f64()
                .map_or("-".into(), |v| format!("{v:.3}")),
        );
        if o["blocker"].is_null() {
            println!("{line}");
        } else {
            println!("{}", ui::paint(line, ui::DIM));
        }
    }
    println!(
        "\n{}",
        ui::paint(
            format!(
                "* = selected ({}). Hashrate is measured while mining each coin. Change: erebor coins set <COIN|auto>",
                if current == AUTO_COIN { "auto" } else { current.as_str() }
            ),
            ui::DIM
        )
    );
    Ok(())
}

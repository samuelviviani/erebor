use super::*;
use clap::CommandFactory;

#[test]
fn command_line_parses() {
    Cli::command().debug_assert();
    let p = |a: &[&str]| Cli::try_parse_from(std::iter::once("erebor").chain(a.iter().copied()));
    assert!(matches!(
        p(&["status", "--json"]).unwrap(),
        Cli {
            json: true,
            cmd: Some(Cmd::Status),
            ..
        }
    ));
    assert!(matches!(
        p(&["--json", "hardware", "list"]).unwrap().cmd.unwrap(),
        Cmd::Hardware {
            cmd: HardwareCmd::List
        }
    ));
    assert!(matches!(
        p(&["mining", "pause"]).unwrap().cmd.unwrap(),
        Cmd::Mining {
            action: MiningCmd::Pause
        }
    ));
    assert!(matches!(
        p(&["agent", "start", "--foreground"]).unwrap().cmd.unwrap(),
        Cmd::Agent {
            cmd: AgentCmd::Start { foreground: true }
        }
    ));
    let c = p(&[
        "wallet",
        "set",
        "krxABC",
        "--coin",
        "prl",
        "--data-dir",
        "/x",
    ])
    .unwrap();
    assert_eq!(c.data_dir, Some(PathBuf::from("/x")));
    assert!(matches!(
        c.cmd.unwrap(),
        Cmd::Wallet {
            cmd: WalletCmd::Set { coin: Some(_), .. }
        }
    ));
    assert!(matches!(
        p(&["miner", "install"]).unwrap().cmd.unwrap(),
        Cmd::Miner {
            cmd: MinerCmd::Install { kind: Kind::Xmrig }
        }
    ));
    assert!(matches!(
        p(&["service", "on"]).unwrap().cmd.unwrap(),
        Cmd::Service {
            state: Some(OnOff::On)
        }
    ));
    assert!(matches!(
        p(&["service"]).unwrap().cmd.unwrap(),
        Cmd::Service { state: None }
    ));
    assert!(matches!(
        p(&["dashboard"]).unwrap().cmd.unwrap(),
        Cmd::Monitor
    ));
    assert!(matches!(p(&["start"]).unwrap().cmd.unwrap(), Cmd::Start));
    assert!(matches!(
        p(&["power", "max"]).unwrap().cmd.unwrap(),
        Cmd::Power {
            level: Some(onboard::Perf::Performance)
        }
    ));
    assert!(p(&["power", "turbo"]).is_err());
    assert!(p(&["mining"]).is_err());
    assert!(p(&[]).unwrap().cmd.is_none());
    assert!(p(&["demo", "maybe"]).is_err());
}

#[test]
fn config_keys() {
    let mut c = serde_json::to_value(AppConfig::default()).unwrap();
    set_key(&mut c, "pool.worker", "1234").unwrap();
    set_key(&mut c, "energy.pricePerKwh", "0.25").unwrap();
    set_key(&mut c, "safety.autoRestart", "true").unwrap();
    set_key(&mut c, "performance.preset", "custom").unwrap();
    assert_eq!(
        set_key(&mut c, "pool.nope", "1").unwrap_err().code,
        "unknown_key: pool.nope"
    );
    assert!(set_key(&mut c, "pool", "x").is_err());
    let a: AppConfig = serde_json::from_value(c.clone()).unwrap();
    assert_eq!(a.pool.worker, "1234");
    assert_eq!(a.energy.price_per_kwh, Some(0.25));
    assert!(a.safety.auto_restart);
    assert_eq!(a.performance.preset, erebor_core::config::Preset::Custom);
    assert_eq!(at(&c, "pool.worker"), Some(&json!("1234")));
    // Wrong type: refused before reaching the engine.
    set_key(&mut c, "safety.maxTempC", "hot").unwrap();
    assert!(serde_json::from_value::<AppConfig>(c).is_err());
    let mut rows = vec![];
    flatten(
        "",
        &serde_json::to_value(AppConfig::default()).unwrap(),
        &mut rows,
    );
    assert!(rows
        .iter()
        .any(|(k, v)| k == "safety.maxTempC" && v == "85.0"));
}

#[test]
fn error_codes_have_hints() {
    for code in [
        "agent_unreachable",
        "wallet_missing",
        "miner_not_configured",
        "engine_busy: x",
    ] {
        assert!(explain(code).1.is_some(), "{code}");
    }
    assert_eq!(explain("weird").0, "weird");
}

//! End-to-end: the real `erebor` binary against a real agent, in demo mode only
//! (simulated miner: no mining process, no CPU load, no network).
#![cfg(unix)]

use serde_json::Value;
use std::path::Path;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

fn erebor(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_erebor"))
        .args(args)
        .env("EREBOR_DATA_DIR", dir)
        .output()
        .unwrap()
}

fn json(dir: &Path, args: &[&str]) -> (i32, Value) {
    let mut a = args.to_vec();
    a.push("--json");
    let o = erebor(dir, &a);
    let v = serde_json::from_slice(&o.stdout).unwrap_or_else(|_| {
        panic!(
            "{args:?}: not JSON: {}\n{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        )
    });
    (o.status.code().unwrap_or(-1), v)
}

/// Kills the agent if the test fails half-way.
struct Guard(std::process::Child);

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn cli_agent_demo_lifecycle() {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path();
    std::fs::write(dir.join("config.json"), r#"{"demoMode":true}"#).unwrap();

    // No agent yet: exit code 3 and a JSON error.
    let (code, v) = json(dir, &["status"]);
    assert_eq!((code, v["error"].as_str()), (3, Some("agent_unreachable")));
    assert!(
        String::from_utf8_lossy(&erebor(dir, &["status"]).stderr).contains("erebor agent start")
    );

    // Agent in the foreground, as a child we control.
    let mut agent = Guard(
        Command::new(env!("CARGO_BIN_EXE_erebor"))
            .args(["agent", "start", "--foreground"])
            .env("EREBOR_DATA_DIR", dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let t = Instant::now();
    while erebor(dir, &["agent", "status"]).status.code() != Some(0) {
        assert!(t.elapsed() < Duration::from_secs(20), "agent did not start");
        std::thread::sleep(Duration::from_millis(100));
    }

    // A second agent is never started (the agent crate tests the engine lock itself).
    let second = erebor(dir, &["agent", "start", "--foreground"]);
    assert_eq!(second.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&second.stdout).contains("Already running"));
    let (_, v) = json(dir, &["agent", "start"]);
    assert_eq!(v["started"], false);

    let (code, v) = json(dir, &["status"]);
    assert_eq!(code, 0);
    assert_eq!(
        (
            v["schema"].as_u64(),
            v["state"].as_str(),
            v["demo"].as_bool()
        ),
        (Some(1), Some("stopped"), Some(true))
    );

    let (code, v) = json(dir, &["mining", "start"]);
    assert_eq!((code, v["changed"].as_bool()), (0, Some(true)));
    assert_eq!(v["snapshot"]["state"], "running");
    let (_, v) = json(dir, &["mining", "start"]);
    assert_eq!(v["changed"], false, "already running: no second miner");
    assert_eq!(
        json(dir, &["mining", "pause"]).1["snapshot"]["state"],
        "paused"
    );
    assert_eq!(
        json(dir, &["mining", "resume"]).1["snapshot"]["state"],
        "running"
    );
    assert_eq!(json(dir, &["status"]).1["state"], "running");

    // Human output.
    let out = String::from_utf8_lossy(&erebor(dir, &["status"]).stdout).to_string();
    assert!(out.contains("Running") && out.contains("DEMO"), "{out}");

    // Wallet validation goes through the engine; secrets are refused.
    let bad = erebor(dir, &["wallet", "set", "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"]);
    assert_eq!(bad.status.code(), Some(1));
    let (code, v) = json(dir, &["wallet", "set", "krxABC123"]);
    assert_eq!((code, v["pool"].as_str()), (0, Some("kryptex")));
    assert_eq!(json(dir, &["wallet", "show"]).1["wallet"], "krxABC123");
    assert_eq!(json(dir, &["pools", "set", "nope"]).0, 1);

    // Real mode without accepted fees: start is refused by the engine, state verified.
    assert_eq!(
        json(dir, &["mining", "stop"]).1["snapshot"]["state"],
        "stopped"
    );
    json(dir, &["demo", "off"]);
    let (code, v) = json(dir, &["mining", "start"]);
    assert_eq!((code, v["error"].as_str()), (1, Some("consent_required")));
    assert_eq!(json(dir, &["status"]).1["state"], "stopped");
    json(dir, &["demo", "on"]);

    let (code, v) = json(dir, &["device", "name", "rig-test"]);
    assert_eq!((code, v["name"].as_str()), (0, Some("rig-test")));
    assert_eq!(
        json(dir, &["doctor", "--offline"]).1["checks"]
            .as_array()
            .map(|c| !c.is_empty()),
        Some(true)
    );
    assert_eq!(json(dir, &["hardware", "list"]).0, 0);

    // Stop the agent: mining stops with it and the socket goes away.
    json(dir, &["mining", "start"]);
    let (code, _) = json(dir, &["agent", "stop"]);
    assert_eq!(code, 0);
    assert!(agent.0.wait().unwrap().success());
    assert_eq!(json(dir, &["status"]).0, 3);
    assert!(!dir.join("agent.sock").exists());
}

/// Stops a background agent if the test fails half-way.
struct Stop(std::path::PathBuf);

impl Drop for Stop {
    fn drop(&mut self) {
        let _ = erebor(&self.0, &["agent", "stop"]);
    }
}

#[test]
fn background_agent_start_and_stop() {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path();
    std::fs::write(dir.join("config.json"), r#"{"demoMode":true}"#).unwrap();
    let (code, v) = json(dir, &["agent", "start"]);
    assert_eq!((code, v["started"].as_bool()), (0, Some(true)), "{v}");
    let _cleanup = Stop(dir.to_path_buf());
    assert_eq!(json(dir, &["agent", "status"]).1["agent"]["owner"], "agent");
    assert_eq!(json(dir, &["agent", "stop"]).0, 0);
    let (code, v) = json(dir, &["agent", "status"]);
    assert_eq!((code, v["running"].as_bool()), (3, Some(false)));
}

#[test]
fn onboard_non_interactive() {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path();
    let _cleanup = Stop(dir.to_path_buf());

    // Real mining: refused without explicit acceptance; nothing is created.
    let (code, v) = json(
        dir,
        &["onboard", "--non-interactive", "--wallet", "krxABC123"],
    );
    assert_eq!(
        (code, v["error"].as_str()),
        (1, Some("accept_risk_required"))
    );
    assert!(!dir.join("config.json").exists());

    // Accepted, without download or network; the agent it started is stopped again.
    let (code, v) = json(
        dir,
        &[
            "onboard",
            "--non-interactive",
            "--accept-risk",
            "--wallet",
            "krxABC123",
            "--no-install-miner",
            "--skip-health",
            "--no-background",
        ],
    );
    assert_eq!(code, 0, "{v}");
    assert_eq!(
        (v["termsAccepted"].as_bool(), v["pool"].as_str()),
        (Some(true), Some("kryptex"))
    );
    assert_eq!(v["agentRunning"], false);
    let cfg: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("config.json")).unwrap()).unwrap();
    assert_eq!(
        (cfg["onboardingCompleted"].as_bool(), cfg["wallet"].as_str()),
        (Some(true), Some("krxABC123"))
    );

    // Demo: agent kept running, simulated miner started on request.
    let (code, v) = json(
        dir,
        &[
            "onboard",
            "--non-interactive",
            "--mode",
            "demo",
            "--start",
            "--skip-health",
        ],
    );
    assert_eq!(code, 0, "{v}");
    assert_eq!(
        (
            v["demo"].as_bool(),
            v["agentRunning"].as_bool(),
            v["mining"].as_str()
        ),
        (Some(true), Some(true), Some("running"))
    );
    // No command, already set up: plain status.
    let o = erebor(dir, &[]);
    assert!(String::from_utf8_lossy(&o.stdout).contains("Running"));
}

#[test]
fn first_run_hint() {
    let d = tempfile::tempdir().unwrap();
    // Not a terminal: no wizard, just the pointer to it.
    let o = erebor(d.path(), &[]);
    assert_eq!(o.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&o.stdout).contains("erebor onboard"));
    let o = erebor(d.path(), &["status"]);
    assert!(String::from_utf8_lossy(&o.stderr).contains("erebor onboard"));
}

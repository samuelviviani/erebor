//! The background agent: `agent`, `service on|off`, self-update, and starting the agent
//! when a command needs the engine.

use super::*;
use anstream::{eprintln, println};

pub(super) fn agent(dir: &Path, cmd: AgentCmd, json: bool) -> Result<(), Fail> {
    let info = ipc::call(dir, "agent_info", &Value::Null);
    match cmd {
        AgentCmd::Start { .. } if info.is_ok() => {
            let i = info.unwrap_or_default();
            if json {
                print_json(&json!({"started": false, "agent": i}));
            } else {
                println!(
                    "Already running: {} · pid {}. Nothing started.",
                    i["owner"].as_str().unwrap_or("?"),
                    i["pid"]
                );
            }
            Ok(())
        }
        AgentCmd::Start { foreground: true } => {
            erebor_agent::run(dir.to_path_buf()).map_err(Fail::from)
        }
        AgentCmd::Start { foreground: false } => {
            let pid = spawn_agent(dir)?;
            let i = call(dir, "agent_info", Value::Null)?;
            if json {
                print_json(&json!({"started": true, "agent": i}));
            } else {
                println!(
                    "Agent started · pid {pid} · log {}",
                    dir.join("agent.log").display()
                );
                println!(
                    "Mining is not started automatically: {}",
                    ui::paint("erebor start", ui::CMD)
                );
            }
            Ok(())
        }
        AgentCmd::Stop => {
            info.map_err(Fail::from)?;
            stop_agent(dir)?;
            if json {
                print_json(&json!({"stopped": true}));
            } else {
                println!("{}", ui::ok("Agent stopped (mining stopped with it)."));
            }
            Ok(())
        }
        AgentCmd::Status => {
            let service = autostart::is_installed(autostart::AGENT_LABEL, autostart::Kind::Service);
            let i = match info {
                Ok(i) => i,
                Err(e) if e == "agent_unreachable" => {
                    if json {
                        print_json(&json!({"running": false, "service": service}));
                    } else {
                        println!(
                            "Agent not running.{}",
                            if service {
                                " (login service installed)"
                            } else {
                                ""
                            }
                        );
                        println!("It starts by itself when needed (erebor start, erebor), or: erebor agent start");
                    }
                    std::process::exit(3);
                }
                Err(e) => return Err(e.into()),
            };
            if json {
                print_json(&json!({"running": true, "service": service, "agent": i}));
                return Ok(());
            }
            row("Owner", i["owner"].as_str().unwrap_or("?"));
            row("PID", &i["pid"]);
            row("Version", i["version"].as_str().unwrap_or("?"));
            row("Protocol", &i["protocol"]);
            let up = erebor_core::now_unix() - i["startedAt"].as_i64().unwrap_or(0);
            row("Uptime", fmt::duration(up.max(0) as u64));
            row("Socket", ipc::socket_path(dir).display());
            row(
                "Login service",
                if service {
                    "installed"
                } else {
                    "not installed (optional)"
                },
            );
            Ok(())
        }
        AgentCmd::InstallService => {
            if info.is_ok() {
                return fail("stop_agent_first");
            }
            install_service(dir)?;
            println!(
                "Agent installed as a login service ({}) and started.",
                autostart::AGENT_LABEL
            );
            println!("It never starts mining by itself unless startOnLaunch is enabled.");
            Ok(())
        }
        AgentCmd::UninstallService => {
            autostart::uninstall(autostart::AGENT_LABEL, autostart::Kind::Service)?;
            println!("Login service removed.");
            Ok(())
        }
        AgentCmd::Setup => {
            ensure_agent(dir, json)?;
            let mut v = call(dir, "remote_setup", Value::Null);
            if v.as_ref()
                .is_err_and(|e| e.code == "unknown_command" || e.code == "remote_setup_unavailable")
            {
                let old_version = info
                    .as_ref()
                    .ok()
                    .and_then(|i| i["version"].as_str())
                    .unwrap_or("");
                if old_version != env!("CARGO_PKG_VERSION") {
                    if !json {
                        eprintln!("Waiting for the updated agent to restart...");
                    }
                    let deadline = Instant::now() + Duration::from_secs(15);
                    while Instant::now() < deadline {
                        std::thread::sleep(Duration::from_millis(500));
                        v = call(dir, "remote_setup", Value::Null);
                        if v.is_ok()
                            || v.as_ref().is_err_and(|e| {
                                !matches!(
                                    e.code.as_str(),
                                    "unknown_command"
                                        | "remote_setup_unavailable"
                                        | "agent_unreachable"
                                )
                            })
                        {
                            break;
                        }
                    }
                }
            }
            let v = v.map_err(|e| {
                if matches!(
                    e.code.as_str(),
                    "unknown_command" | "remote_setup_unavailable" | "agent_unreachable"
                ) {
                    Fail::from("remote_setup_update_required".to_string())
                } else {
                    e
                }
            })?;
            let mut v = v;
            let firewall = crate::firewall::configure(v["listen"].as_str(), json);
            v["firewall"] = json!(firewall);
            if json {
                print_json(&v);
            } else {
                println!("{}", ui::ok("Ready to connect this computer."));
                println!(
                    "Pairing code: {}",
                    ui::paint(v["code"].as_str().unwrap_or(""), ui::BRAND)
                );
                println!("Valid once for 10 minutes. In Erebor: Devices > Add device.");
                let addresses = v["addresses"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ");
                if !addresses.is_empty() {
                    println!("Address: {addresses}");
                }
                println!("Firewall: {firewall}");
            }
            Ok(())
        }
        AgentCmd::Init {
            listen,
            no_discovery,
        } => {
            let v = erebor_agent::init(dir, listen, !no_discovery)?;
            if json {
                print_json(&v);
            } else {
                println!(
                    "{}",
                    ui::ok(format!(
                        "Remote control on: {} (TLS).",
                        v["listen"].as_str().unwrap_or("")
                    ))
                );
                println!("Fingerprint: {}", v["fingerprint"].as_str().unwrap_or(""));
                println!(
                    "Restart the agent to apply it, then: {}",
                    ui::paint("erebor agent pair", ui::CMD)
                );
            }
            Ok(())
        }
        AgentCmd::Pair => {
            let v = erebor_agent::pair(dir)?;
            if json {
                print_json(&v);
            } else {
                println!(
                    "Pairing code: {}",
                    ui::paint(v["code"].as_str().unwrap_or(""), ui::BRAND)
                );
                println!(
                    "Valid 10 minutes, once. Addresses: {}",
                    v["addresses"]
                        .as_array()
                        .map(|a| a
                            .iter()
                            .filter_map(|x| x.as_str())
                            .collect::<Vec<_>>()
                            .join(", "))
                        .unwrap_or_default()
                );
                println!("Fingerprint: {}", v["fingerprint"].as_str().unwrap_or(""));
            }
            Ok(())
        }
        AgentCmd::Controllers => {
            let v = erebor_agent::controllers(dir);
            if json {
                print_json(&v);
            } else {
                for c in v.as_array().into_iter().flatten() {
                    println!(
                        "{}  {}",
                        c["fingerprint"].as_str().unwrap_or(""),
                        c["name"].as_str().unwrap_or("")
                    );
                }
            }
            Ok(())
        }
        AgentCmd::Revoke { who } => {
            let v = erebor_agent::revoke(dir, &who)?;
            if json {
                print_json(&v);
            } else {
                println!(
                    "{}",
                    ui::ok(format!(
                        "Access removed: {}",
                        v["revoked"].as_str().unwrap_or("")
                    ))
                );
            }
            Ok(())
        }
        AgentCmd::Disable => {
            erebor_agent::disable(dir)?;
            println!("Remote control off from the next agent start.");
            Ok(())
        }
    }
}

/// `erebor service on|off`: Erebor and mining start with the computer, or not.
pub(super) fn service(dir: &Path, state: Option<OnOff>, json: bool) -> Result<(), Fail> {
    let installed = || autostart::is_installed(autostart::AGENT_LABEL, autostart::Kind::Service);
    match state {
        Some(OnOff::On) => {
            ensure_agent(dir, json)?;
            let mut c = load_config_live(dir)?;
            c.start_on_launch = true;
            save_config(dir, &c)?;
            // The service runs its own agent: hand over from a plain background one.
            if !installed() {
                let info = call(dir, "agent_info", Value::Null)?;
                if info["owner"] != "agent" {
                    return fail("not_agent");
                }
                stop_agent(dir)?;
                install_service(dir)?;
            }
            let linger = enable_linger();
            if json {
                print_json(&json!({"service": true, "startOnLaunch": true, "linger": linger}));
                return Ok(());
            }
            println!(
                "{}",
                ui::ok("Erebor now starts with the computer and mines automatically.")
            );
            match linger {
                Some(true) | None => {}
                Some(false) => println!(
                    "! On Linux it starts at login. To start right at boot, before anyone logs in:\n    sudo loginctl enable-linger \"$USER\""
                ),
            }
            if let Ok(s) = ipc::snapshot(dir) {
                if let (Some(b), false) = (&s.start_blocker, s.is_active()) {
                    let (m, h) = explain(b);
                    println!("{}", ui::warn(format!("Mining cannot start yet: {m}")));
                    if let Some(h) = h {
                        println!("  → {}", ui::paint(h, ui::CMD));
                    }
                }
            }
            println!("Turn it off: {}", ui::paint("erebor service off", ui::CMD));
            Ok(())
        }
        Some(OnOff::Off) => {
            if let Ok(mut c) = load_config_live(dir) {
                c.start_on_launch = false;
                save_config(dir, &c)?;
            }
            autostart::uninstall(autostart::AGENT_LABEL, autostart::Kind::Service)?;
            if json {
                print_json(&json!({"service": false, "startOnLaunch": false}));
            } else {
                println!(
                    "{}",
                    ui::ok("Erebor no longer starts with the computer; background mining stopped.")
                );
                println!(
                    "Start by hand any time: {}",
                    ui::paint("erebor start", ui::CMD)
                );
            }
            Ok(())
        }
        None => {
            let on = installed();
            let start = load_config(dir)?.start_on_launch;
            if json {
                print_json(&json!({"service": on, "startOnLaunch": start}));
            } else if on && start {
                println!(
                    "{} Erebor starts with the computer and mines automatically.\nTurn off: {}",
                    ui::paint("● On:", ui::OK),
                    ui::paint("erebor service off", ui::CMD)
                );
            } else if on {
                println!(
                    "{} Erebor starts with the computer but does not mine by itself.\nMine always: {}",
                    ui::paint("◐", ui::WARN),
                    ui::paint("erebor service on", ui::CMD)
                );
            } else {
                println!(
                    "{} Erebor does not start with the computer.\nMine always, also after a restart: {}",
                    ui::paint("○ Off:", ui::DIM),
                    ui::paint("erebor service on", ui::CMD)
                );
            }
            Ok(())
        }
    }
}

/// Linux: let the user's services run from boot, without a login. `None` elsewhere.
pub(super) fn enable_linger() -> Option<bool> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let user = std::env::var("USER").unwrap_or_default();
    let ok = std::process::Command::new("loginctl")
        .args(["enable-linger", &user])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    Some(ok)
}

/// `erebor update`: install the latest signed release over this executable.
pub(super) fn update(dir: &Path, json: bool) -> Result<(), Fail> {
    let exe = std::env::current_exe().map_err(|e| Fail::from(e.to_string()))?;
    if !json {
        eprintln!("{}", ui::paint("Checking for updates…", ui::DIM));
    }
    let installed = erebor_core::update::update(&exe)?;
    if json {
        print_json(&json!({"updated": installed, "current": env!("CARGO_PKG_VERSION")}));
        return Ok(());
    }
    match installed {
        None => println!(
            "{}",
            ui::ok(format!(
                "Erebor is up to date (v{}).",
                env!("CARGO_PKG_VERSION")
            ))
        ),
        Some(v) => {
            println!(
                "{}",
                ui::ok(format!("Updated to v{v} (signature verified)."))
            );
            if ipc::call(dir, "agent_info", &Value::Null).is_ok_and(|i| i["owner"] == "agent") {
                println!("Erebor in the background switches to it within a few seconds; mining continues.");
            }
        }
    }
    Ok(())
}

/// Ask the agent to stop (mining stops with it) and wait until it is gone.
fn stop_agent(dir: &Path) -> Result<(), Fail> {
    call(dir, "shutdown", Value::Null)?;
    let t = Instant::now();
    while ipc::call(dir, "agent_info", &Value::Null).is_ok() {
        if t.elapsed() > Duration::from_secs(15) {
            return fail("agent_stop_timeout");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

/// Register the agent as a per-user login service and start it (no root).
pub(super) fn install_service(dir: &Path) -> Result<(), Fail> {
    let exe = std::env::current_exe().map_err(|e| Fail::from(e.to_string()))?;
    std::fs::create_dir_all(dir).map_err(|e| Fail::from(e.to_string()))?;
    let abs = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let args = ["agent", "start", "--foreground", "--data-dir"]
        .map(String::from)
        .into_iter()
        .chain([abs.to_string_lossy().into_owned()])
        .collect::<Vec<_>>();
    autostart::install(
        autostart::AGENT_LABEL,
        &exe,
        &args,
        autostart::Kind::Service,
    )?;
    Ok(())
}

/// Start `erebor agent start --foreground` detached from this terminal, log to a file,
/// and wait until it answers on the socket.
pub(super) fn spawn_agent(dir: &Path) -> Result<u32, Fail> {
    let e = |e: std::io::Error| Fail::from(e.to_string());
    std::fs::create_dir_all(dir).map_err(e)?;
    let log_path = dir.join("agent.log");
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(e)?;
    let log_start = log.metadata().map(|m| m.len() as usize).unwrap_or(0);
    let mut cmd = std::process::Command::new(std::env::current_exe().map_err(e)?);
    cmd.args(["agent", "start", "--foreground", "--data-dir"])
        .arg(dir)
        .stdin(std::process::Stdio::null())
        .stdout(log.try_clone().map_err(e)?)
        .stderr(log);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // New session: closing this terminal (SIGHUP) or Ctrl-C does not reach it.
        // SAFETY: setsid is async-signal-safe.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    let mut child = cmd.spawn().map_err(e)?;
    let t = Instant::now();
    loop {
        if ipc::call(dir, "agent_info", &Value::Null).is_ok() {
            return Ok(child.id());
        }
        if let Ok(Some(_)) = child.try_wait() {
            // The child printed its error (and code) to the log: report that code.
            let log = std::fs::read_to_string(&log_path).unwrap_or_default();
            let new = log.get(log_start..).unwrap_or("");
            let code = new
                .lines()
                .find_map(|l| l.trim().strip_prefix("code: "))
                .or_else(|| new.lines().find_map(|l| l.strip_prefix("error: ")))
                .unwrap_or("agent_start_failed");
            return Err(Fail::from(code.to_string()));
        }
        if t.elapsed() > Duration::from_secs(15) {
            return fail("agent_start_timeout");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

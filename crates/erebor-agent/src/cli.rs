//! `erebor-agent` subcommands; usage text in main.rs.

use crate as agent;
use serde_json::Value;
use std::path::PathBuf;

const USAGE: &str = "usage: erebor-agent [start|setup|init|pair|status|controllers|revoke|rotate-key|disable|asic] [--data-dir DIR] [--json]\n       erebor-agent --help";

/// The `erebor-agent` command line (also `erebor` when started through an
/// `erebor-agent` symlink, as the release packages install it).
pub fn cli(args: impl Iterator<Item = String>) {
    let mut dir = std::env::var_os("EREBOR_DATA_DIR").map(PathBuf::from);
    let (mut json, mut pos, mut opts) = (false, Vec::new(), Vec::<(String, String)>::new());
    let mut args = args;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--data-dir" => {
                dir = Some(
                    args.next()
                        .map(PathBuf::from)
                        .unwrap_or_else(|| exit_usage()),
                )
            }
            "--json" => json = true,
            "--no-discovery" => opts.push(("no-discovery".into(), String::new())),
            "--listen" | "--port" | "--name" => {
                let v = args.next().unwrap_or_else(|| exit_usage());
                opts.push((a.trim_start_matches("--").into(), v));
            }
            "--version" | "-V" => return println!("erebor-agent {}", env!("CARGO_PKG_VERSION")),
            "--help" | "-h" => {
                return print!(
                    "{}",
                    include_str!("main.rs")
                        .lines()
                        .take_while(|l| l.starts_with("//!"))
                        .map(|l| format!(
                            "{}\n",
                            l.trim_start_matches("//!").strip_prefix(' ').unwrap_or("")
                        ))
                        .collect::<String>()
                )
            }
            _ if a.starts_with('-') => exit_usage(),
            _ => pos.push(a),
        }
    }
    #[cfg(target_os = "linux")]
    if pos.as_slice() == ["setup"] && dir.is_none() && std::path::Path::new(SYSTEM_DIR).is_dir() {
        use std::os::unix::fs::MetadataExt;
        let owner = std::fs::metadata(SYSTEM_DIR).map(|m| m.uid()).ok();
        if owner != Some(unsafe { libc::geteuid() }) {
            let exe = std::env::args_os()
                .next()
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    let e = "missing executable";
                    eprintln!("{e}");
                    std::process::exit(1)
                });
            let status = std::process::Command::new("sudo")
                .args(["-u", "erebor-agent"])
                .arg(&exe)
                .args(["status", "--json"])
                .output();
            let info: Value = status
                .ok()
                .filter(|s| s.status.success())
                .and_then(|s| serde_json::from_slice(&s.stdout).ok())
                .unwrap_or_else(|| {
                    eprintln!("erebor-agent: cannot read service status");
                    std::process::exit(1)
                });
            if info["enabled"] != true {
                let ok = std::process::Command::new("sudo")
                    .args(["-u", "erebor-agent"])
                    .arg(&exe)
                    .arg("init")
                    .status()
                    .is_ok_and(|s| s.success());
                if !ok {
                    eprintln!("erebor-agent: setup failed");
                    std::process::exit(1);
                }
            }
            if info["enabled"] != true || info["agentRunning"] != true {
                let ok = std::process::Command::new("sudo")
                    .args(["systemctl", "restart", "erebor-agent"])
                    .status()
                    .is_ok_and(|s| s.success());
                if !ok {
                    eprintln!("erebor-agent: service restart failed");
                    std::process::exit(1);
                }
            }
            let status = std::process::Command::new("sudo")
                .args(["-u", "erebor-agent"])
                .arg(exe)
                .arg("pair")
                .args(if json { vec!["--json"] } else { vec![] })
                .status();
            std::process::exit(status.ok().and_then(|s| s.code()).unwrap_or(1));
        }
    }
    let dir = dir.unwrap_or_else(default_dir);
    let opt = |k: &str| opts.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    let pos: Vec<&str> = pos.iter().map(String::as_str).collect();
    let r: Result<Value, String> = match pos.as_slice() {
        [] | ["start"] => {
            if let Err(e) = agent::run(dir) {
                eprintln!("erebor-agent: {e}");
                std::process::exit(1);
            }
            return;
        }
        ["setup"] => setup(&dir),
        ["init"] => agent::init(&dir, opt("listen"), opt("no-discovery").is_none()),
        ["pair"] => agent::pair(&dir),
        ["status"] => Ok(agent::remote_status(&dir)),
        ["controllers"] => Ok(agent::controllers(&dir)),
        ["revoke", who] => agent::revoke(&dir, who),
        ["rotate-key"] => erebor_core::remote::Identity::rotate(&dir).map(
            |i| serde_json::json!({"fingerprint": erebor_core::remote::short_fp(&i.fingerprint)}),
        ),
        ["disable"] => agent::disable(&dir),
        ["asic", "add", ip] => agent::asic_add(
            &dir,
            ip,
            opt("port").and_then(|p| p.parse().ok()).unwrap_or(0),
            &opt("name").unwrap_or_default(),
        ),
        ["asic", "remove", ip] => agent::asic_remove(&dir, ip),
        ["asic", "list"] => Ok(agent::remote_status(&dir)["asics"].clone()),
        _ => exit_usage(),
    };
    match r {
        Ok(v) if json => println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default()),
        Ok(v) => print_human(pos[0], &v),
        Err(e) => {
            if json {
                println!("{}", serde_json::json!({"error": e}));
            } else {
                eprintln!("erebor-agent: {e}");
            }
            std::process::exit(1);
        }
    }
}

fn setup(dir: &std::path::Path) -> Result<Value, String> {
    erebor_core::ipc::call(dir, "remote_setup", &Value::Null).map_err(|e| {
        let code = e.as_str().unwrap_or("remote_setup_failed");
        match code {
            "agent_unreachable" => "Start the agent first: erebor-agent start".into(),
            "unknown_command" | "remote_setup_unavailable" => {
                "Update and restart the running agent, then retry setup".into()
            }
            _ => code.into(),
        }
    })
}

fn exit_usage() -> ! {
    eprintln!("{USAGE}");
    std::process::exit(2);
}

fn print_human(cmd: &str, v: &Value) {
    let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
    match cmd {
        "init" => {
            println!(
                "Remote control on: {} (TLS), discovery {}.",
                s("listen"),
                if v["discovery"] == true { "on" } else { "off" }
            );
            println!("This agent's fingerprint: {}", s("fingerprint"));
            println!("Next: restart the agent (sudo systemctl restart erebor-agent), then: erebor-agent pair");
        }
        "setup" | "pair" => {
            println!("Pairing code: {}", s("code"));
            println!("Valid for 10 minutes, once. In Erebor Desktop: Devices → Add device.");
            for a in v["addresses"].as_array().into_iter().flatten() {
                println!("Address:      {}", a.as_str().unwrap_or(""));
            }
            println!(
                "Fingerprint:  {} (the app shows the same after pairing)",
                s("fingerprint")
            );
            println!(
                "From a terminal: erebor devices add <address> --code {}",
                s("code")
            );
        }
        "status" => {
            println!(
                "Agent running:  {}",
                if v["agentRunning"] == true {
                    "yes"
                } else {
                    "no"
                }
            );
            println!(
                "Remote control: {}",
                if v["enabled"] == true {
                    format!("on, {}", s("listen"))
                } else {
                    "off (erebor-agent init)".into()
                }
            );
            println!(
                "Discovery:      {}",
                if v["discovery"] == true { "on" } else { "off" }
            );
            println!("Fingerprint:    {}", s("fingerprint"));
            println!(
                "Pairing code:   {}",
                if v["pairingActive"] == true {
                    "active"
                } else {
                    "none"
                }
            );
            let c = v["controllers"].as_array().cloned().unwrap_or_default();
            println!("Controllers:    {}", c.len());
            for c in c {
                println!(
                    "  {}  {}",
                    c["fingerprint"].as_str().unwrap_or(""),
                    c["name"].as_str().unwrap_or("")
                );
            }
            let a = v["asics"].as_array().map_or(0, |a| a.len());
            if a > 0 {
                println!("ASICs (gateway): {a}");
            }
        }
        "controllers" => {
            let c = v.as_array().cloned().unwrap_or_default();
            if c.is_empty() {
                println!("No controller paired. Pair one with: erebor-agent pair");
            }
            for c in c {
                println!(
                    "{}  {}",
                    c["fingerprint"].as_str().unwrap_or(""),
                    c["name"].as_str().unwrap_or("")
                );
            }
        }
        "revoke" => println!(
            "Access removed for {} ({}).",
            s("revoked"),
            s("fingerprint")
        ),
        "rotate-key" => println!(
            "New fingerprint {}. Every controller has to pair again.",
            s("fingerprint")
        ),
        "disable" => println!("Remote control off from the next agent start."),
        _ => println!("{}", serde_json::to_string_pretty(v).unwrap_or_default()),
    }
}

/// The system service's folder when this is that service's user (installed by
/// scripts/install-agent.sh), else the per-user folder shared with the app.
fn default_dir() -> PathBuf {
    #[cfg(unix)]
    if let Some(system) = Some(PathBuf::from(SYSTEM_DIR)).filter(|p| p.is_dir()) {
        // SAFETY: no arguments.
        let me = unsafe { libc::geteuid() };
        use std::os::unix::fs::MetadataExt;
        let owner = std::fs::metadata(&system).map(|m| m.uid()).ok();
        if owner == Some(me) {
            return system;
        }
        if me == 0 {
            // Files written as root would be unreadable by the service.
            eprintln!(
                "erebor-agent: run it as the service user: sudo -u erebor-agent erebor-agent {}",
                std::env::args().skip(1).collect::<Vec<_>>().join(" ")
            );
            std::process::exit(2);
        }
    }
    erebor_core::default_data_dir()
}

#[cfg(unix)]
const SYSTEM_DIR: &str = "/var/lib/erebor-agent";

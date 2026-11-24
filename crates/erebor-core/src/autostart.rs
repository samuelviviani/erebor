//! Start Erebor with the system, per user, without admin rights:
//! - `Kind::Login`: the desktop app opens at login (LaunchAgent, XDG autostart, Run key).
//! - `Kind::Service`: the headless node runs in the background and restarts if it
//!   crashes (LaunchAgent with KeepAlive, systemd user unit, scheduled task at logon).

use std::path::{Path, PathBuf};

pub const APP_LABEL: &str = "org.erebor.desktop";
pub const AGENT_LABEL: &str = "org.erebor.agent";

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Login,
    Service,
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn plist(label: &str, exe: &Path, args: &[String], kind: Kind) -> String {
    let argv: String = std::iter::once(exe.to_string_lossy().into_owned())
        .chain(args.iter().cloned())
        .map(|a| format!("    <string>{}</string>\n", xml(&a)))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n<dict>\n  <key>Label</key><string>{}</string>\n\
  <key>ProgramArguments</key>\n  <array>\n{argv}  </array>\n\
  <key>RunAtLoad</key><true/>\n  <key>KeepAlive</key>{}\n</dict>\n</plist>\n",
        xml(label),
        // Service: restarted after a crash, not after a clean stop (`erebor agent stop`).
        if kind == Kind::Service {
            "<dict><key>SuccessfulExit</key><false/></dict>"
        } else {
            "<false/>"
        }
    )
}

/// Quote for systemd `ExecStart=` / desktop `Exec=` lines.
fn quoted(exe: &Path, args: &[String]) -> String {
    std::iter::once(exe.to_string_lossy().into_owned())
        .chain(args.iter().cloned())
        .map(|a| format!("\"{}\"", a.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

fn file(label: &str, kind: Kind) -> PathBuf {
    if cfg!(target_os = "macos") {
        home().join(format!("Library/LaunchAgents/{label}.plist"))
    } else {
        let cfg = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join(".config"));
        match kind {
            Kind::Login => cfg.join(format!("autostart/{label}.desktop")),
            Kind::Service => cfg.join(format!("systemd/user/{label}.service")),
        }
    }
}

fn run(cmd: &str, args: &[&str]) -> Result<(), String> {
    let s = crate::command(cmd)
        .args(args)
        .status()
        .map_err(|e| format!("{cmd}: {e}"))?;
    s.success()
        .then_some(())
        .ok_or_else(|| format!("{cmd} {}: {s}", args.join(" ")))
}

fn write(p: &Path, text: &str) -> Result<(), String> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(p, text).map_err(|e| e.to_string())
}

pub fn install(label: &str, exe: &Path, args: &[String], kind: Kind) -> Result<(), String> {
    if cfg!(windows) {
        let line = quoted(exe, args);
        return match kind {
            Kind::Login => run(
                "reg",
                &[
                    "add",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                    "/v",
                    label,
                    "/d",
                    &line,
                    "/f",
                ],
            ),
            Kind::Service => {
                run(
                    "schtasks",
                    &[
                        "/Create", "/TN", label, "/TR", &line, "/SC", "ONLOGON", "/RL", "LIMITED",
                        "/F",
                    ],
                )?;
                run("schtasks", &["/Run", "/TN", label])
            }
        };
    }
    let p = file(label, kind);
    if cfg!(target_os = "macos") {
        write(&p, &plist(label, exe, args, kind))?;
        if kind == Kind::Service {
            let _ = run("launchctl", &["unload", &p.to_string_lossy()]);
            run("launchctl", &["load", "-w", &p.to_string_lossy()])?;
        }
        return Ok(());
    }
    match kind {
        Kind::Login => write(
            &p,
            &format!("[Desktop Entry]\nType=Application\nName=Erebor\nExec={}\nX-GNOME-Autostart-enabled=true\n", quoted(exe, args)),
        ),
        Kind::Service => {
            write(
                &p,
                &format!("[Unit]\nDescription=Erebor background service\nAfter=network-online.target\n\n[Service]\nExecStart={}\nRestart=on-failure\nRestartSec=10\n\n[Install]\nWantedBy=default.target\n", quoted(exe, args)),
            )?;
            run("systemctl", &["--user", "daemon-reload"])?;
            run("systemctl", &["--user", "enable", "--now", &format!("{label}.service")])
        }
    }
}

pub fn uninstall(label: &str, kind: Kind) -> Result<(), String> {
    if cfg!(windows) {
        return match kind {
            Kind::Login => run(
                "reg",
                &[
                    "delete",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                    "/v",
                    label,
                    "/f",
                ],
            ),
            Kind::Service => {
                let _ = run("schtasks", &["/End", "/TN", label]);
                run("schtasks", &["/Delete", "/TN", label, "/F"])
            }
        };
    }
    let p = file(label, kind);
    if kind == Kind::Service {
        let _ = if cfg!(target_os = "macos") {
            run("launchctl", &["unload", "-w", &p.to_string_lossy()])
        } else {
            run(
                "systemctl",
                &["--user", "disable", "--now", &format!("{label}.service")],
            )
        };
    }
    match std::fs::remove_file(&p) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
        _ => Ok(()),
    }
}

pub fn is_installed(label: &str, kind: Kind) -> bool {
    if cfg!(windows) {
        let q = match kind {
            Kind::Login => crate::command("reg")
                .args([
                    "query",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                    "/v",
                    label,
                ])
                .output(),
            Kind::Service => crate::command("schtasks")
                .args(["/Query", "/TN", label])
                .output(),
        };
        return q.is_ok_and(|o| o.status.success());
    }
    file(label, kind).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_files_quote_paths() {
        let p = plist(
            "org.x",
            Path::new("/A & B/erebor"),
            &["--port".into(), "7420".into()],
            Kind::Service,
        );
        assert!(p.contains("<string>/A &amp; B/erebor</string>"));
        assert!(p.contains("<string>7420</string>"));
        assert!(p.contains("<key>SuccessfulExit</key><false/>"));
        assert!(plist("x", Path::new("/e"), &[], Kind::Login).contains("<false/>"));
        assert_eq!(
            quoted(Path::new("/a b/e"), &["x\"y".into()]),
            r#""/a b/e" "x\"y""#
        );
    }
}

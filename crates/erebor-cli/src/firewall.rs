//! Configure an active Linux UFW for the private networks on this computer.

#[cfg(target_os = "linux")]
fn private_interfaces(output: &str, listen: std::net::SocketAddr) -> Vec<(String, String)> {
    use std::net::{IpAddr, Ipv4Addr};
    let mut networks = Vec::new();
    for line in output.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        let Some(index) = fields.iter().position(|f| *f == "inet") else {
            continue;
        };
        let Some((address, prefix)) = fields.get(index + 1).and_then(|v| v.split_once('/')) else {
            continue;
        };
        let (Ok(address), Ok(prefix)) = (address.parse::<Ipv4Addr>(), prefix.parse::<u32>()) else {
            continue;
        };
        let Some(interface) = fields.get(1).map(|s| s.split('@').next().unwrap_or("")) else {
            continue;
        };
        if !address.is_private()
            || !(16..=30).contains(&prefix)
            || interface.starts_with("docker")
            || interface.starts_with("veth")
            || interface.starts_with("virbr")
            || interface.starts_with("br-")
            || interface.is_empty()
            || !interface
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
            || (!listen.ip().is_unspecified() && listen.ip() != IpAddr::V4(address))
        {
            continue;
        }
        let mask = u32::MAX << (32 - prefix);
        let cidr = format!("{}/{prefix}", Ipv4Addr::from(u32::from(address) & mask));
        let entry = (interface.to_string(), cidr);
        if !networks.contains(&entry) {
            networks.push(entry);
        }
    }
    networks
}

#[cfg(target_os = "linux")]
pub fn configure(listen: Option<&str>, json: bool) -> String {
    use std::io::IsTerminal;
    use std::process::{Command, Stdio};
    let Some(listen) = listen.and_then(|s| s.parse::<std::net::SocketAddr>().ok()) else {
        return "could not determine the listening address; check the firewall manually".into();
    };
    if listen.ip().is_loopback() {
        return "loopback only; no firewall changes".into();
    }
    let config = std::fs::read_to_string("/etc/ufw/ufw.conf").unwrap_or_default();
    if !config.lines().any(|l| l.trim() == "ENABLED=yes") {
        return "UFW is not enabled; check other firewalls if the device is unreachable".into();
    }
    let Ok(output) = Command::new("ip")
        .args(["-o", "-4", "addr", "show", "scope", "global"])
        .output()
    else {
        return "could not detect local networks; allow the listening port manually".into();
    };
    let networks = private_interfaces(&String::from_utf8_lossy(&output.stdout), listen);
    if networks.is_empty() {
        return "no eligible private network found; check firewall rules manually".into();
    }
    let interactive = !json && std::io::stdin().is_terminal();
    if interactive {
        eprintln!(
            "Configuring UFW for the local network (administrator password may be requested)..."
        );
    }
    let mut failures = 0;
    for (interface, network) in &networks {
        for protocol in ["tcp", "udp"] {
            let mut command = if unsafe { libc::geteuid() } == 0 {
                Command::new("/usr/sbin/ufw")
            } else {
                let mut sudo = Command::new("sudo");
                if !interactive {
                    sudo.arg("-n");
                }
                sudo.arg("/usr/sbin/ufw");
                sudo
            };
            let ok = command
                .args([
                    "allow",
                    "in",
                    "on",
                    interface,
                    "from",
                    network,
                    "to",
                    "any",
                    "port",
                    &listen.port().to_string(),
                    "proto",
                    protocol,
                ])
                .stdin(if interactive {
                    Stdio::inherit()
                } else {
                    Stdio::null()
                })
                .stdout(Stdio::null())
                .stderr(if interactive {
                    Stdio::inherit()
                } else {
                    Stdio::null()
                })
                .status()
                .is_ok_and(|s| s.success());
            if !ok {
                failures += 1;
            }
        }
    }
    if failures == 0 {
        format!(
            "UFW allows TCP and UDP {} from {}",
            listen.port(),
            networks
                .iter()
                .map(|(_, n)| n.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        format!(
            "could not add all UFW rules; allow TCP and UDP {} from your trusted local network",
            listen.port()
        )
    }
}

#[cfg(not(target_os = "linux"))]
pub fn configure(_listen: Option<&str>, _json: bool) -> String {
    "check the system firewall if another computer cannot connect".into()
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    #[test]
    fn limits_rules_to_private_local_interfaces() {
        let input = "2: eth0 inet 192.168.1.37/24 brd 192.168.1.255 scope global eth0\n3: docker0 inet 172.17.0.1/16 scope global docker0\n4: eth1 inet 8.8.8.8/24 scope global eth1\n5: tun0 inet 10.0.0.1/8 scope global tun0\n";
        assert_eq!(
            private_interfaces(input, "0.0.0.0:7420".parse().unwrap()),
            vec![("eth0".into(), "192.168.1.0/24".into())]
        );
        assert!(private_interfaces(input, "192.168.2.1:7420".parse().unwrap()).is_empty());
    }
}

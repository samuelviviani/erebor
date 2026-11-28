//! The controller side: connect to a paired agent (certificate pinned), pair with a code,
//! find agents on the LAN.

use super::*;
use rustls::{ClientConnection, StreamOwned};
use serde_json::json;
use std::io::BufReader;
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Some commands (pool checks, ASIC scans) take a while on the agent.
const REPLY_TIMEOUT: Duration = Duration::from_secs(60);

/// "host", "host:port", "1.2.3.4", "[fd00::1]:7420", "fd00::1".
pub fn resolve(addr: &str) -> Result<SocketAddr, String> {
    let addr = addr.trim();
    if let Ok(a) = addr.parse::<SocketAddr>() {
        return Ok(a);
    }
    if let Ok(ip) = addr
        .trim_matches(|c| c == '[' || c == ']')
        .parse::<IpAddr>()
    {
        return Ok(SocketAddr::new(ip, PORT));
    }
    let with_port = match addr.rsplit_once(':') {
        Some((_, p)) if p.parse::<u16>().is_ok() => addr.to_string(),
        _ => format!("{addr}:{PORT}"),
    };
    with_port
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.next())
        .ok_or_else(|| "address_not_found".to_string())
}

pub struct Link {
    r: BufReader<StreamOwned<ClientConnection, TcpStream>>,
    /// Fingerprint of the agent's certificate (checked against the pin, when given).
    pub agent_fp: String,
    pub hello: Value,
    v: u32,
}

impl Link {
    /// Connect and negotiate the protocol version. With `pin`, any other certificate is
    /// refused (`fingerprint_mismatch`).
    pub fn connect(id: &Identity, addr: &str, pin: Option<&str>) -> Result<Self, String> {
        let sa = resolve(addr)?;
        let mut tcp = TcpStream::connect_timeout(&sa, CONNECT_TIMEOUT)
            .map_err(|_| "unreachable".to_string())?;
        let _ = tcp.set_read_timeout(Some(CONNECT_TIMEOUT));
        let _ = tcp.set_write_timeout(Some(CONNECT_TIMEOUT));
        let _ = tcp.set_nodelay(true);
        let (cfg, seen) = client_config(id, pin)?;
        let name = ServerName::try_from("erebor-agent").map_err(|e| e.to_string())?;
        let mut conn = ClientConnection::new(cfg, name).map_err(|e| e.to_string())?;
        while conn.is_handshaking() {
            conn.complete_io(&mut tcp).map_err(|e| {
                if e.to_string().contains("fingerprint_mismatch") {
                    "fingerprint_mismatch".to_string()
                } else {
                    "tls_failed".to_string()
                }
            })?;
        }
        let agent_fp = seen
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
            .ok_or("tls_failed")?;
        let _ = tcp.set_read_timeout(Some(REPLY_TIMEOUT));
        let mut link = Self {
            r: BufReader::new(StreamOwned::new(conn, tcp)),
            agent_fp,
            hello: Value::Null,
            v: PROTOCOL,
        };
        let hello = link.call("hello", &Value::Null).map_err(err_code)?;
        let (theirs, their_min) = (
            hello["protocol"].as_u64().unwrap_or(0) as u32,
            hello["minProtocol"].as_u64().unwrap_or(0) as u32,
        );
        if theirs < MIN_PROTOCOL || their_min > PROTOCOL {
            return Err("protocol_incompatible".into());
        }
        link.v = theirs.min(PROTOCOL);
        link.hello = hello;
        Ok(link)
    }

    pub fn call(&mut self, cmd: &str, args: &Value) -> Result<Value, Value> {
        let id = request_id();
        let req = Request {
            v: self.v,
            id: id.clone(),
            ts: crate::now_unix(),
            cmd: cmd.into(),
            args: args.clone(),
        };
        let req = serde_json::to_value(&req).map_err(|e| Value::from(e.to_string()))?;
        write_line(self.r.get_mut(), &req).map_err(|_| Value::from("connection_lost"))?;
        loop {
            let line = read_line(&mut self.r)
                .map_err(|e| {
                    Value::from(
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::TimedOut
                        {
                            "timeout"
                        } else {
                            "connection_lost"
                        },
                    )
                })?
                .ok_or_else(|| Value::from("connection_lost"))?;
            let v: Value = serde_json::from_str(&line).map_err(|_| Value::from("bad_response"))?;
            if v["id"] != id.as_str() {
                continue; // an event, or a reply to someone else's request
            }
            return match v.get("err") {
                Some(e) => Err(e.clone()),
                None => Ok(v.get("ok").cloned().unwrap_or(Value::Null)),
            };
        }
    }

    /// Turn this connection into an event stream (`next_event`).
    pub fn subscribe(&mut self, interval_secs: u64) -> Result<(), Value> {
        self.call("subscribe", &json!({"intervalSecs": interval_secs}))
            .map(|_| ())
    }

    /// Block for the next event; `Err` when the connection ends or stays silent too long.
    pub fn next_event(&mut self) -> Result<Value, String> {
        let line = read_line(&mut self.r)
            .map_err(|_| "connection_lost".to_string())?
            .ok_or("connection_lost")?;
        serde_json::from_str(&line).map_err(|_| "bad_response".into())
    }
}

#[cfg(test)]
impl Link {
    pub(crate) fn r_mut(&mut self) -> &mut StreamOwned<ClientConnection, TcpStream> {
        self.r.get_mut()
    }
    pub(crate) fn reader(&mut self) -> &mut BufReader<StreamOwned<ClientConnection, TcpStream>> {
        &mut self.r
    }
}

fn err_code(v: Value) -> String {
    match v {
        Value::String(s) => s,
        v => v.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Paired {
    pub fingerprint: String,
    pub device: crate::fleet::Device,
    pub agent_version: String,
}

/// Pair with the agent at `addr` using the code it shows. On success the agent trusts our
/// certificate and we know (and pin) its fingerprint.
pub fn pair(
    id: &Identity,
    addr: &str,
    code: &str,
    controller_name: &str,
) -> Result<Paired, String> {
    let code = normalize_code(code);
    if code.len() != 12 {
        return Err("pairing_code_invalid".into());
    }
    let mut link = Link::connect(id, addr, None)?;
    let mine = proof(&code, "controller", &link.agent_fp, &id.fingerprint);
    let r = link
        .call("pair", &json!({"name": controller_name, "proof": mine}))
        .map_err(err_code)?;
    let theirs = r["proof"].as_str().unwrap_or("");
    if !same_proof(
        theirs,
        &proof(&code, "agent", &link.agent_fp, &id.fingerprint),
    ) {
        return Err("pairing_agent_proof_invalid".into());
    }
    Ok(Paired {
        fingerprint: link.agent_fp.clone(),
        device: serde_json::from_value(r["device"].clone()).map_err(|_| "bad_response")?,
        agent_version: link.hello["agentVersion"].as_str().unwrap_or("").into(),
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub address: String,
    pub id: String,
    pub name: String,
    pub version: String,
    pub fingerprint: String,
    pub pairing: bool,
}

/// Limit active discovery to private addresses within the actual interface subnet.
#[cfg(any(unix, test))]
fn subnet_targets(ip: std::net::Ipv4Addr, netmask: std::net::Ipv4Addr) -> Vec<std::net::Ipv4Addr> {
    if !ip.is_private() {
        return Vec::new();
    }
    let mask = u32::from(netmask);
    let prefix = mask.leading_ones();
    if !(16..=30).contains(&prefix) || mask != u32::MAX << (32 - prefix) {
        return Vec::new();
    }
    let prefix = prefix.max(24);
    let mask = u32::MAX << (32 - prefix);
    let first = u32::from(ip) & mask;
    (first + 1..first + !mask)
        .filter(|n| *n != u32::from(ip))
        .map(std::net::Ipv4Addr::from)
        .collect()
}

#[cfg(unix)]
fn local_discovery_targets() -> Vec<std::net::Ipv4Addr> {
    let mut interfaces = std::ptr::null_mut();
    // SAFETY: getifaddrs initializes the list on success; the list is freed once below.
    if unsafe { libc::getifaddrs(&mut interfaces) } != 0 {
        return Vec::new();
    }
    let mut targets = Vec::new();
    let mut current = interfaces;
    while !current.is_null() && targets.len() < 1024 {
        // SAFETY: current is an entry in the list returned by getifaddrs.
        let entry = unsafe { &*current };
        if !entry.ifa_addr.is_null()
            && !entry.ifa_netmask.is_null()
            && entry.ifa_flags & libc::IFF_UP as u32 != 0
            && entry.ifa_flags & libc::IFF_LOOPBACK as u32 == 0
            && unsafe { (*entry.ifa_addr).sa_family as i32 } == libc::AF_INET
        {
            // SAFETY: AF_INET entries have sockaddr_in address and mask structures.
            let (ip, mask) = unsafe {
                let ip = &*(entry.ifa_addr as *const libc::sockaddr_in);
                let mask = &*(entry.ifa_netmask as *const libc::sockaddr_in);
                (
                    std::net::Ipv4Addr::from(ip.sin_addr.s_addr.to_ne_bytes()),
                    std::net::Ipv4Addr::from(mask.sin_addr.s_addr.to_ne_bytes()),
                )
            };
            for target in subnet_targets(ip, mask) {
                if !targets.contains(&target) {
                    targets.push(target);
                }
                if targets.len() >= 1024 {
                    break;
                }
            }
        }
        current = entry.ifa_next;
    }
    // SAFETY: interfaces came from a successful getifaddrs call.
    unsafe { libc::freeifaddrs(interfaces) };
    targets
}

#[cfg(not(unix))]
fn local_discovery_targets() -> Vec<std::net::Ipv4Addr> {
    Vec::new()
}

/// Ask agents on this network to identify themselves, including private subnet probes
/// on Unix when firewalls discard broadcast. Other subnets still need an address.
pub fn discover(port: u16, wait: Duration) -> Result<Vec<Found>, String> {
    let s = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    s.set_broadcast(true).map_err(|e| e.to_string())?;
    let _ = s.set_read_timeout(Some(Duration::from_millis(200)));
    for target in [
        format!("255.255.255.255:{port}"),
        format!("127.0.0.1:{port}"),
    ] {
        let _ = s.send_to(server::DISCOVER_PROBE, &target);
    }
    for target in local_discovery_targets() {
        let _ = s.send_to(server::DISCOVER_PROBE, (target, port));
    }
    let t = Instant::now();
    let mut out: Vec<Found> = Vec::new();
    let mut buf = [0u8; 1024];
    while t.elapsed() < wait {
        let Ok((n, from)) = s.recv_from(&mut buf) else {
            continue;
        };
        let Ok(v) = serde_json::from_slice::<Value>(&buf[..n]) else {
            continue;
        };
        let f = Found {
            address: SocketAddr::new(from.ip(), v["port"].as_u64().unwrap_or(PORT as u64) as u16)
                .to_string(),
            id: v["id"].as_str().unwrap_or("").into(),
            name: v["name"].as_str().unwrap_or("").into(),
            version: v["version"].as_str().unwrap_or("").into(),
            fingerprint: v["fingerprint"].as_str().unwrap_or("").into(),
            pairing: v["pairing"].as_bool().unwrap_or(false),
        };
        // Same agent seen on several interfaces (loopback and LAN): keep one.
        if !f.id.is_empty() && !out.iter().any(|o| o.id == f.id) {
            out.push(f);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod discovery_tests {
    use super::*;
    #[test]
    fn targets_stay_inside_private_interface_subnet() {
        let targets = subnet_targets(
            "192.168.1.37".parse().unwrap(),
            "255.255.255.128".parse().unwrap(),
        );
        assert_eq!(targets.len(), 125);
        assert!(targets.iter().all(|ip| ip.octets()[3] < 127
            && *ip != "192.168.1.37".parse::<std::net::Ipv4Addr>().unwrap()));
        assert!(
            subnet_targets("8.8.8.8".parse().unwrap(), "255.255.255.0".parse().unwrap()).is_empty()
        );
        assert!(
            subnet_targets("10.1.1.2".parse().unwrap(), "255.0.0.0".parse().unwrap()).is_empty()
        );
    }
}

//! ASIC miners, read and controlled through their own firmware's API: nothing is
//! installed on the ASIC.
//!
//! One protocol covers most devices: the CGMiner API (TCP 4028, `{"command":"…"}` in, one
//! JSON reply, connection closed). Bitmain's BMMiner and MicroBT's btminer speak dialects of
//! it. Each vendor adapter maps the reply to `AsicStatus` and declares what it really
//! supports (`Capabilities`), with the reason when it doesn't.
//!
//! Verified only against the documented formats and `asic_sim`; no physical ASIC was
//! tested (see docs/ASIC.md). Never flashes firmware, never logs in to web interfaces,
//! never touches voltages or frequencies.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

pub const API_PORT: u16 = 4028;
const TIMEOUT: Duration = Duration::from_secs(4);

/// An ASIC the user added (by address).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct AsicEntry {
    pub host: String,
    pub port: u16,
    pub name: String,
}

impl AsicEntry {
    pub fn id(&self) -> String {
        asic_id(&self.host, self.port)
    }
}

/// Stable identity of an ASIC: the same address reached directly or through a gateway is
/// the same device.
pub fn asic_id(host: &str, port: u16) -> String {
    format!(
        "asic:{}:{}",
        host.to_ascii_lowercase(),
        if port == 0 { API_PORT } else { port }
    )
}

/// Only addresses of networks people administer themselves: private IPv4, CGNAT (Tailscale,
/// Meshnet), link-local, loopback and IPv6 ULA. Public addresses are refused, so a gateway
/// can't be turned into a scanner of the Internet.
pub fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            v.is_private()
                || v.is_loopback()
                || v.is_link_local()
                || (o[0] == 100 && (64..128).contains(&o[1]))
        }
        IpAddr::V6(v) => {
            v.is_loopback()
                || (v.segments()[0] & 0xfe00) == 0xfc00
                || (v.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

pub fn validate_entry(e: &AsicEntry) -> Result<(), String> {
    let ip: IpAddr = e
        .host
        .parse()
        .map_err(|_| "asic_address_must_be_ip".to_string())?;
    if !is_private(ip) {
        return Err("asic_address_not_private".into());
    }
    if e.name.chars().count() > 64 || e.name.chars().any(char::is_control) {
        return Err("invalid_device_name".into());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Vendor {
    Antminer,
    Whatsminer,
    /// Anything else answering the CGMiner API (Avalon, Innosilicon, Braiins/LuxOS
    /// compatibility layers, …).
    #[default]
    Cgminer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Capability {
    pub supported: bool,
    /// Why not, as an error code the UI explains.
    pub reason: Option<String>,
}

fn yes() -> Capability {
    Capability {
        supported: true,
        reason: None,
    }
}

fn no(reason: &str) -> Capability {
    Capability {
        supported: false,
        reason: Some(reason.into()),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub monitor: Capability,
    pub restart: Capability,
    /// Add a pool and switch to it (wallet and worker go in the pool's user field).
    pub set_pool: Capability,
    pub start_stop: Capability,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Hashboard {
    pub index: u32,
    pub hashrate: Option<f64>,
    pub temp_c: Option<f32>,
    pub chip_temp_c: Option<f32>,
    pub chips: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AsicPool {
    pub index: u32,
    pub url: String,
    pub user: String,
    pub status: String,
    pub active: bool,
    pub priority: i64,
    pub accepted: u64,
    pub rejected: u64,
}

/// Everything read from one ASIC. Unknown values are `None`, never guessed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct AsicStatus {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub name: String,
    pub vendor: Vendor,
    pub model: Option<String>,
    pub firmware: Option<String>,
    pub api_version: Option<String>,
    /// A simulator (`asic_sim`), never real hardware.
    pub simulated: bool,
    pub online: bool,
    pub error: Option<String>,
    /// Hashing algorithm, from the model family when known ("sha256d", "scrypt", …).
    pub algo: Option<String>,
    /// H/s, last few seconds and average since start.
    pub hashrate: Option<f64>,
    pub hashrate_avg: Option<f64>,
    pub accepted: u64,
    pub rejected: u64,
    pub hardware_errors: u64,
    pub uptime_secs: Option<u64>,
    pub boards: Vec<Hashboard>,
    pub fans_rpm: Vec<u32>,
    pub temp_max_c: Option<f32>,
    pub power_w: Option<f64>,
    pub pools: Vec<AsicPool>,
    pub capabilities: Capabilities,
    pub checked_at: i64,
}

// ---------- CGMiner API transport ----------

fn addr(host: &str, port: u16) -> Result<SocketAddr, String> {
    (host, if port == 0 { API_PORT } else { port })
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.next())
        .ok_or_else(|| "asic_unreachable".to_string())
}

/// Send one command, return the parsed reply. Tolerates the firmware quirks seen in the
/// wild: a trailing NUL byte and `}{` between objects in `stats`.
pub fn command(host: &str, port: u16, cmd: &str, param: Option<&str>) -> Result<Value, String> {
    let mut s = TcpStream::connect_timeout(&addr(host, port)?, TIMEOUT)
        .map_err(|_| "asic_unreachable".to_string())?;
    let _ = s.set_read_timeout(Some(TIMEOUT));
    let _ = s.set_write_timeout(Some(TIMEOUT));
    let req = match param {
        Some(p) => json!({"command": cmd, "parameter": p}),
        None => json!({"command": cmd}),
    };
    s.write_all(req.to_string().as_bytes())
        .map_err(|_| "asic_unreachable".to_string())?;
    let mut buf = Vec::new();
    s.take(1 << 20)
        .read_to_end(&mut buf)
        .map_err(|_| "asic_timeout".to_string())?;
    parse_reply(&buf)
}

pub fn parse_reply(buf: &[u8]) -> Result<Value, String> {
    let text = String::from_utf8_lossy(buf);
    let text = text.trim_matches(|c: char| c == '\0' || c.is_whitespace());
    let v: Value = serde_json::from_str(text)
        .or_else(|_| serde_json::from_str(&text.replace("}{", "},{")))
        .map_err(|_| "asic_bad_response".to_string())?;
    // CGMiner: STATUS is a list; btminer sometimes a plain object.
    let st = v.get("STATUS").and_then(|s| s.get(0).or(Some(s)));
    match st.and_then(|s| s.get("STATUS")).and_then(Value::as_str) {
        Some("E") | Some("F") => Err(format!(
            "asic_error:{}",
            st.and_then(|s| s.get("Msg"))
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        )),
        _ => Ok(v),
    }
}

fn num(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().trim_end_matches(" W").parse().ok(),
        _ => None,
    }
    .filter(|x: &f64| x.is_finite())
}

fn first<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.get(key).and_then(|a| a.get(0))
}

/// Hashrate in H/s from whichever unit the firmware used.
fn hashrate(s: &Value, window: &str) -> Option<f64> {
    [("THS", 1e12), ("GHS", 1e9), ("MHS", 1e6), ("KHS", 1e3)]
        .iter()
        .find_map(|(u, m)| num(s.get(format!("{u} {window}"))).map(|h| h * m))
}

/// The algorithm a model family mines. Only families whose algorithm is unambiguous.
pub fn algo_for_model(model: &str) -> Option<&'static str> {
    let m = model.to_ascii_lowercase();
    let m = m
        .trim_start_matches("antminer ")
        .trim_start_matches("whatsminer ");
    let fam = m.chars().next()?;
    if model.to_ascii_lowercase().contains("whatsminer")
        || (fam == 'm' && m[1..].starts_with(|c: char| c.is_ascii_digit()))
    {
        return Some("sha256d");
    }
    match (fam, m.get(..2)) {
        (_, Some("ks")) => Some("kheavyhash"),
        (_, Some("ka")) => Some("kadena"),
        ('s' | 't', _) => Some("sha256d"),
        ('l', _) => Some("scrypt"),
        ('e', _) => Some("etchash"),
        ('d', _) => Some("x11"),
        _ => None,
    }
}

fn detect(version: &Value) -> (Vendor, Option<String>, Option<String>, Option<String>) {
    let v = first(version, "VERSION")
        .or_else(|| version.get("Msg"))
        .cloned()
        .unwrap_or(Value::Null);
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let model = s("Type").or(s("Model")).or(s("platform"));
    let fw = s("BMMiner")
        .map(|b| format!("BMMiner {b}"))
        .or(s("CompileTime"))
        .or(s("fw_ver"))
        .or(s("Firmware"))
        .or(s("CGMiner").map(|c| format!("CGMiner {c}")));
    let api = s("API").or(s("api_ver"));
    let lower = format!("{} {}", model.clone().unwrap_or_default(), v).to_ascii_lowercase();
    let vendor = if lower.contains("antminer") || v.get("BMMiner").is_some() {
        Vendor::Antminer
    } else if lower.contains("whatsminer")
        || v.get("api_ver").is_some()
        || lower.contains("btminer")
    {
        Vendor::Whatsminer
    } else {
        Vendor::Cgminer
    };
    (vendor, model, fw, api)
}

/// Read everything an ASIC tells. Errors leave `online: false` and the reason.
pub fn read(entry: &AsicEntry) -> AsicStatus {
    let port = if entry.port == 0 {
        API_PORT
    } else {
        entry.port
    };
    let mut st = AsicStatus {
        id: asic_id(&entry.host, port),
        host: entry.host.clone(),
        port,
        name: if entry.name.is_empty() {
            entry.host.clone()
        } else {
            entry.name.clone()
        },
        checked_at: crate::now_unix(),
        ..Default::default()
    };
    let version = command(&entry.host, port, "version", None)
        .or_else(|_| command(&entry.host, port, "get_version", None));
    let version = match version {
        Ok(v) => v,
        Err(e) => {
            st.error = Some(e);
            st.capabilities = Capabilities {
                monitor: no("asic_unreachable"),
                restart: no("asic_unreachable"),
                set_pool: no("asic_unreachable"),
                start_stop: no("asic_unreachable"),
            };
            return st;
        }
    };
    let (vendor, mut model, fw, api) = detect(&version);
    if vendor == Vendor::Whatsminer {
        // get_version names the control board, not the miner: the model is in devdetails.
        model = command(&entry.host, port, "devdetails", None)
            .ok()
            .and_then(|v| {
                first(&v, "DEVDETAILS")?
                    .get("Model")?
                    .as_str()
                    .map(|m| format!("WhatsMiner {m}"))
            })
            .or(model);
    }
    st.vendor = vendor;
    st.simulated = model
        .as_deref()
        .is_some_and(|m| m.to_ascii_lowercase().contains("simulator"));
    st.algo = model
        .as_deref()
        .and_then(algo_for_model)
        .map(str::to_string);
    st.model = model;
    st.firmware = fw;
    st.api_version = api;
    st.online = true;

    match command(&entry.host, port, "summary", None) {
        Ok(v) => {
            let s = first(&v, "SUMMARY").cloned().unwrap_or(Value::Null);
            st.hashrate = hashrate(&s, "5s").or_else(|| hashrate(&s, "RT"));
            st.hashrate_avg = hashrate(&s, "av");
            st.accepted = num(s.get("Accepted")).unwrap_or(0.0) as u64;
            st.rejected = num(s.get("Rejected")).unwrap_or(0.0) as u64;
            st.hardware_errors = num(s.get("Hardware Errors")).unwrap_or(0.0) as u64;
            st.uptime_secs = num(s.get("Elapsed")).map(|e| e as u64);
            st.power_w = num(s.get("Power")).or(num(s.get("Power_RT")));
            if let Some(t) = num(s.get("Temperature")) {
                st.temp_max_c = Some(t as f32);
            }
            for k in ["Fan Speed In", "Fan Speed Out"] {
                if let Some(f) = num(s.get(k)) {
                    st.fans_rpm.push(f as u32);
                }
            }
        }
        Err(e) => st.error = Some(e),
    }

    if vendor == Vendor::Whatsminer {
        if let Ok(v) = command(&entry.host, port, "devs", None) {
            for (i, d) in v
                .get("DEVS")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .enumerate()
            {
                st.boards.push(Hashboard {
                    index: num(d.get("Slot")).map_or(i as u32, |s| s as u32),
                    hashrate: hashrate(d, "5s").or_else(|| hashrate(d, "av")),
                    temp_c: num(d.get("Temperature")).map(|t| t as f32),
                    chip_temp_c: num(d.get("Chip Temp Max")).map(|t| t as f32),
                    chips: num(d.get("Effective Chips")).map(|c| c as u32),
                });
            }
        }
    } else if let Ok(v) = command(&entry.host, port, "stats", None) {
        // BMMiner: STATS[0] identifies the miner, STATS[1] has chains, temps and fans.
        let s = v
            .get("STATS")
            .and_then(Value::as_array)
            .and_then(|a| {
                a.iter()
                    .find(|x| x.get("chain_rate1").is_some() || x.get("fan1").is_some())
            })
            .cloned()
            .unwrap_or(Value::Null);
        for n in 1..=16u32 {
            let rate = num(s.get(format!("chain_rate{n}")));
            let chips = num(s.get(format!("chain_acn{n}"))).map(|c| c as u32);
            if rate.is_none() && chips.unwrap_or(0) == 0 {
                continue;
            }
            // Chip temps: one number (`temp2_N`) or "a-b-c-d" (`temp_chipN`): the hottest.
            let chip = num(s.get(format!("temp2_{n}"))).or_else(|| {
                s.get(format!("temp_chip{n}"))
                    .and_then(Value::as_str)
                    .and_then(|t| {
                        t.split('-')
                            .filter_map(|x| x.parse::<f64>().ok())
                            .reduce(f64::max)
                    })
            });
            st.boards.push(Hashboard {
                index: n,
                hashrate: rate.map(|r| r * 1e9),
                temp_c: num(s.get(format!("temp{n}")))
                    .or_else(|| {
                        s.get(format!("temp_pcb{n}"))
                            .and_then(Value::as_str)
                            .and_then(|t| {
                                t.split('-')
                                    .filter_map(|x| x.parse::<f64>().ok())
                                    .reduce(f64::max)
                            })
                    })
                    .map(|t| t as f32),
                chip_temp_c: chip.map(|t| t as f32),
                chips,
            });
        }
        for n in 1..=8 {
            if let Some(f) = num(s.get(format!("fan{n}"))).filter(|f| *f > 0.0) {
                st.fans_rpm.push(f as u32);
            }
        }
        if st.power_w.is_none() {
            st.power_w = num(s.get("Power")).or(num(s.get("chain_power")));
        }
    }
    let hottest = st
        .boards
        .iter()
        .flat_map(|b| [b.temp_c, b.chip_temp_c])
        .flatten()
        .reduce(f32::max);
    st.temp_max_c = match (st.temp_max_c, hottest) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };

    if let Ok(v) = command(&entry.host, port, "pools", None) {
        st.pools = v
            .get("POOLS")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|p| AsicPool {
                index: num(p.get("POOL")).unwrap_or(0.0) as u32,
                url: p.get("URL").and_then(Value::as_str).unwrap_or("").into(),
                user: p.get("User").and_then(Value::as_str).unwrap_or("").into(),
                status: p.get("Status").and_then(Value::as_str).unwrap_or("").into(),
                active: p
                    .get("Stratum Active")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                priority: num(p.get("Priority")).unwrap_or(0.0) as i64,
                accepted: num(p.get("Accepted")).unwrap_or(0.0) as u64,
                rejected: num(p.get("Rejected")).unwrap_or(0.0) as u64,
            })
            .collect();
    }

    st.capabilities = capabilities(vendor, &entry.host, port);
    st
}

/// What this device allows. Write commands of the CGMiner API need "privileged" access,
/// which most stock firmware grants only to localhost.
fn capabilities(vendor: Vendor, host: &str, port: u16) -> Capabilities {
    let privileged = command(host, port, "privileged", None).is_ok();
    match vendor {
        Vendor::Whatsminer => Capabilities {
            monitor: yes(),
            // btminer API 2.x: writes need a token and AES with the admin password.
            restart: no("whatsminer_write_api_not_implemented"),
            set_pool: no("whatsminer_write_api_not_implemented"),
            start_stop: no("whatsminer_write_api_not_implemented"),
        },
        _ => Capabilities {
            monitor: yes(),
            restart: if privileged {
                yes()
            } else {
                no("asic_api_read_only")
            },
            set_pool: if privileged {
                yes()
            } else {
                no("asic_api_read_only")
            },
            // The CGMiner API has no stop that the firmware won't undo (`quit` gets the
            // miner restarted by the watchdog).
            start_stop: no("asic_no_stop_command"),
        },
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum AsicAction {
    Restart,
    /// Add `url` (stratum) with `user` (wallet.worker or account.worker) and switch to it;
    /// the previous pools stay as fallbacks.
    SetPool {
        url: String,
        user: String,
        password: String,
    },
    SwitchPool {
        index: u32,
    },
}

fn valid_pool_field(s: &str, max: usize) -> bool {
    // The CGMiner API separates parameters with ',' and commands with '|'.
    !s.is_empty()
        && s.len() <= max
        && !s
            .chars()
            .any(|c| c == ',' || c == '|' || c.is_control() || c == '"')
}

/// Run an action, refusing what the device doesn't support. On a failed switch the pool
/// just added is removed again (the previous configuration stays).
pub fn act(entry: &AsicEntry, action: &AsicAction) -> Result<Value, String> {
    let (h, p) = (
        entry.host.as_str(),
        if entry.port == 0 {
            API_PORT
        } else {
            entry.port
        },
    );
    let st = read(entry);
    if !st.online {
        return Err(st.error.unwrap_or_else(|| "asic_unreachable".into()));
    }
    let need = |c: &Capability| -> Result<(), String> {
        if c.supported {
            Ok(())
        } else {
            Err(c.reason.clone().unwrap_or_else(|| "unsupported".into()))
        }
    };
    match action {
        AsicAction::Restart => {
            need(&st.capabilities.restart)?;
            command(h, p, "restart", None)?;
            Ok(json!({"state": "accepted"}))
        }
        AsicAction::SwitchPool { index } => {
            need(&st.capabilities.set_pool)?;
            command(h, p, "switchpool", Some(&index.to_string()))?;
            Ok(json!({"state": "completed"}))
        }
        AsicAction::SetPool {
            url,
            user,
            password,
        } => {
            need(&st.capabilities.set_pool)?;
            if !(url.starts_with("stratum+tcp://")
                || url.starts_with("stratum+ssl://")
                || url.starts_with("stratum2+tcp://"))
                || !valid_pool_field(url, 200)
                || !valid_pool_field(user, 200)
                || !(password.is_empty() || valid_pool_field(password, 64))
            {
                return Err("invalid_pool".into());
            }
            let pass = if password.is_empty() { "x" } else { password };
            if let Some(existing) = st.pools.iter().find(|x| x.url == *url && x.user == *user) {
                command(h, p, "switchpool", Some(&existing.index.to_string()))?;
                return Ok(json!({"state": "completed", "pool": existing.index}));
            }
            command(h, p, "addpool", Some(&format!("{url},{user},{pass}")))?;
            let after = read(entry);
            let added = after
                .pools
                .iter()
                .filter(|x| x.url == *url && x.user == *user)
                .map(|x| x.index)
                .max()
                .ok_or("asic_pool_not_added")?;
            if let Err(e) = command(h, p, "switchpool", Some(&added.to_string())) {
                let _ = command(h, p, "removepool", Some(&added.to_string()));
                return Err(e);
            }
            Ok(json!({"state": "completed", "pool": added}))
        }
    }
}

/// Look for CGMiner-compatible APIs in one private subnet (CIDR, at most /22 = 1024
/// addresses). Only explicit, user-started scans; nothing is added by itself.
pub fn scan(cidr: &str, port: u16) -> Result<Vec<AsicEntry>, String> {
    let (base, bits) = cidr.split_once('/').ok_or("invalid_cidr")?;
    let base: std::net::Ipv4Addr = base.parse().map_err(|_| "invalid_cidr")?;
    let bits: u32 = bits.parse().map_err(|_| "invalid_cidr")?;
    if !(22..=32).contains(&bits) {
        return Err("scan_range_too_large".into());
    }
    if !is_private(IpAddr::V4(base)) {
        return Err("asic_address_not_private".into());
    }
    let mask = if bits == 32 {
        u32::MAX
    } else {
        !(u32::MAX >> bits)
    };
    let net = u32::from(base) & mask;
    let hosts: Vec<std::net::Ipv4Addr> = (0..=(!mask))
        .map(|i| std::net::Ipv4Addr::from(net | i))
        .filter(|ip| bits >= 31 || (u32::from(*ip) != net && u32::from(*ip) != (net | !mask)))
        .collect();
    let port = if port == 0 { API_PORT } else { port };
    let found = std::sync::Mutex::new(Vec::new());
    // 64 probes at a time, 400 ms each: a /24 takes about 2 s.
    std::thread::scope(|s| {
        for chunk in hosts.chunks(hosts.len().div_ceil(64).max(1)) {
            let found = &found;
            s.spawn(move || {
                for ip in chunk {
                    let a = SocketAddr::new(IpAddr::V4(*ip), port);
                    if TcpStream::connect_timeout(&a, Duration::from_millis(400)).is_ok()
                        && command(&ip.to_string(), port, "version", None).is_ok()
                    {
                        found.lock().unwrap().push(AsicEntry {
                            host: ip.to_string(),
                            port,
                            name: String::new(),
                        });
                    }
                }
            });
        }
    });
    let mut v = found.into_inner().unwrap_or_default();
    v.sort_by_key(|e| {
        e.host
            .parse::<std::net::Ipv4Addr>()
            .map(u32::from)
            .unwrap_or(0)
    });
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asic_sim::{SimAsic, SimKind};

    #[test]
    fn parses_firmware_quirks_and_errors() {
        assert!(parse_reply(b"{\"STATUS\":[{\"STATUS\":\"S\"}]}\0").is_ok());
        let v = parse_reply(br#"{"STATS":[{"a":1}{"b":2}]}"#).unwrap();
        assert_eq!(v["STATS"][1]["b"], 2);
        assert_eq!(
            parse_reply(
                br#"{"STATUS":[{"STATUS":"E","Msg":"Access denied to 'privileged' command"}]}"#
            ),
            Err("asic_error:Access denied to 'privileged' command".into())
        );
        assert_eq!(parse_reply(b"<html>"), Err("asic_bad_response".into()));
    }

    #[test]
    fn model_families_and_private_ranges() {
        assert_eq!(algo_for_model("Antminer S19j Pro"), Some("sha256d"));
        assert_eq!(algo_for_model("Antminer L7"), Some("scrypt"));
        assert_eq!(algo_for_model("Antminer KS3"), Some("kheavyhash"));
        assert_eq!(algo_for_model("WhatsMiner M30S+"), Some("sha256d"));
        assert_eq!(algo_for_model("Mystery 9000"), None);
        for ok in [
            "192.168.1.10",
            "10.0.0.1",
            "172.16.5.5",
            "100.101.1.2",
            "127.0.0.1",
            "fd00::1",
        ] {
            assert!(is_private(ok.parse().unwrap()), "{ok}");
        }
        for bad in ["8.8.8.8", "100.200.1.1", "2001:db8::1"] {
            assert!(!is_private(bad.parse().unwrap()), "{bad}");
        }
        assert_eq!(
            scan("8.8.8.0/24", 0),
            Err("asic_address_not_private".into())
        );
        assert_eq!(scan("10.0.0.0/16", 0), Err("scan_range_too_large".into()));
        let e = AsicEntry {
            host: "example.com".into(),
            port: 4028,
            name: String::new(),
        };
        assert_eq!(validate_entry(&e), Err("asic_address_must_be_ip".into()));
    }

    #[test]
    fn reads_and_controls_simulated_antminer() {
        let sim = SimAsic::start(SimKind::Antminer { privileged: true }).unwrap();
        let e = sim.entry("rack-1");
        let st = read(&e);
        assert!(st.online && st.simulated, "{st:?}");
        assert_eq!(st.vendor, Vendor::Antminer);
        assert_eq!(st.algo.as_deref(), Some("sha256d"));
        assert_eq!(st.boards.len(), 3);
        assert!(st.hashrate.unwrap() > 1e13, "TH/s range");
        assert_eq!(st.fans_rpm.len(), 4);
        assert!(st.temp_max_c.is_some());
        assert!(st.capabilities.restart.supported && st.capabilities.set_pool.supported);
        assert!(!st.capabilities.start_stop.supported);

        let r = act(
            &e,
            &AsicAction::SetPool {
                url: "stratum+tcp://btc.example:3333".into(),
                user: "bc1qexample.rack1".into(),
                password: String::new(),
            },
        )
        .unwrap();
        let after = read(&e);
        let active: Vec<_> = after.pools.iter().filter(|p| p.active).collect();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].index, r["pool"].as_u64().unwrap() as u32);
        assert_eq!(after.pools.len(), 2, "old pool kept as fallback");
        assert_eq!(
            act(
                &e,
                &AsicAction::SetPool {
                    url: "http://x".into(),
                    user: "u".into(),
                    password: String::new()
                }
            ),
            Err("invalid_pool".into())
        );
        assert_eq!(act(&e, &AsicAction::Restart).unwrap()["state"], "accepted");
    }

    #[test]
    fn read_only_whatsminer_broken_and_offline_devices() {
        let w = SimAsic::start(SimKind::Whatsminer).unwrap();
        let st = read(&w.entry(""));
        assert_eq!(st.vendor, Vendor::Whatsminer);
        assert!(st.power_w.is_some() && st.boards.len() == 3);
        assert_eq!(
            act(&w.entry(""), &AsicAction::Restart),
            Err("whatsminer_write_api_not_implemented".into())
        );

        let ro = SimAsic::start(SimKind::Antminer { privileged: false }).unwrap();
        assert_eq!(
            act(&ro.entry(""), &AsicAction::Restart),
            Err("asic_api_read_only".into())
        );

        let bad = SimAsic::start(SimKind::Broken).unwrap();
        let st = read(&bad.entry(""));
        assert!(!st.online);
        assert_eq!(st.error.as_deref(), Some("asic_bad_response"));

        let port = bad.port;
        drop(bad);
        let gone = read(&AsicEntry {
            host: "127.0.0.1".into(),
            port,
            name: String::new(),
        });
        assert_eq!(gone.error.as_deref(), Some("asic_unreachable"));
    }
}

//! Simulated ASICs speaking the CGMiner API dialects `asic` reads (BMMiner/Antminer and
//! btminer/WhatsMiner), for tests and `erebor sim`. Every model name carries
//! "(Erebor simulator)", so `asic::read` flags it `simulated` and it can't pass for real
//! hardware. Values move a little over time; they are invented and never stored as real.

use crate::asic::AsicEntry;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SimKind {
    /// Antminer S19 (3 hashboards, ~95 TH/s). `privileged`: write commands allowed.
    Antminer { privileged: bool },
    /// Like Antminer, but running hot (chips near 96 °C).
    HotAntminer,
    /// WhatsMiner M30S+ (read-only API, reports power).
    Whatsminer,
    /// Answers with invalid JSON.
    Broken,
}

struct State {
    pools: Vec<(String, String, bool)>,
    started: Instant,
    accepted: u64,
}

pub struct SimAsic {
    pub port: u16,
    pub kind: SimKind,
    stop: Arc<AtomicBool>,
}

impl SimAsic {
    pub fn start(kind: SimKind) -> Result<Self, String> {
        Self::start_on("127.0.0.1:0", kind)
    }

    pub fn start_on(bind: &str, kind: SimKind) -> Result<Self, String> {
        let l = TcpListener::bind(bind).map_err(|e| format!("bind {bind}: {e}"))?;
        let port = l.local_addr().map_err(|e| e.to_string())?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(State {
            pools: vec![(
                "stratum+tcp://sim-pool.invalid:3333".into(),
                "sim.worker".into(),
                true,
            )],
            started: Instant::now(),
            accepted: 1200,
        }));
        let s2 = stop.clone();
        std::thread::spawn(move || {
            for c in l.incoming() {
                if s2.load(Ordering::SeqCst) {
                    return;
                }
                if let Ok(c) = c {
                    let st = state.clone();
                    std::thread::spawn(move || serve(c, kind, &st));
                }
            }
        });
        Ok(Self { port, kind, stop })
    }

    pub fn entry(&self, name: &str) -> AsicEntry {
        AsicEntry {
            host: "127.0.0.1".into(),
            port: self.port,
            name: name.into(),
        }
    }
}

impl Drop for SimAsic {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

fn serve(mut c: TcpStream, kind: SimKind, st: &Mutex<State>) {
    let _ = c.set_read_timeout(Some(Duration::from_secs(3)));
    let mut buf = [0u8; 4096];
    let n = c.read(&mut buf).unwrap_or(0);
    let req: Value = serde_json::from_slice(&buf[..n]).unwrap_or(Value::Null);
    let cmd = req["command"].as_str().unwrap_or("");
    let param = req["parameter"].as_str().unwrap_or("");
    let body = if kind == SimKind::Broken {
        "<html>busy{".to_string()
    } else {
        reply(
            kind,
            cmd,
            param,
            &mut st.lock().unwrap_or_else(|p| p.into_inner()),
        )
    };
    // Real firmware ends replies with a NUL byte.
    let _ = c.write_all(body.as_bytes());
    let _ = c.write_all(b"\0");
}

fn status(ok: bool, msg: &str) -> Value {
    json!([{"STATUS": if ok {"S"} else {"E"}, "When": crate::now_unix(), "Code": if ok {11} else {45}, "Msg": msg, "Description": "Erebor simulator"}])
}

fn reply(kind: SimKind, cmd: &str, param: &str, st: &mut State) -> String {
    let t = st.started.elapsed().as_secs_f64();
    let wobble = 1.0 + 0.02 * (t / 7.0).sin();
    st.accepted += 1;
    let whats = kind == SimKind::Whatsminer;
    let privileged = matches!(
        kind,
        SimKind::Antminer { privileged: true } | SimKind::HotAntminer
    );
    let hot = if kind == SimKind::HotAntminer {
        21.0
    } else {
        0.0
    };
    let model = if whats {
        "M30S+ (Erebor simulator)"
    } else {
        "Antminer S19 (Erebor simulator)"
    };
    let denied = || {
        json!({"STATUS": status(false, &format!("Access denied to '{cmd}' command"))}).to_string()
    };
    let v = match cmd {
        "version" if whats => json!({"STATUS": status(false, "Invalid command")}),
        "version" => json!({"STATUS": status(true, "BMMiner versions"), "VERSION": [{
            "BMMiner": "2.0.0", "API": "3.1", "Miner": "uart_trans.1.3",
            "CompileTime": "Erebor simulator", "Type": model}]}),
        "get_version" if whats => json!({"STATUS": "S", "When": crate::now_unix(), "Code": 131,
            "Msg": {"api_ver": "2.0.5", "fw_ver": "20230101.00.REL (Erebor simulator)", "platform": "H6OS", "chip": "K60"}}),
        "devdetails" if whats => {
            json!({"STATUS": status(true, "Device Details"), "DEVDETAILS": [{"DEVDETAILS": 0, "Name": "SM", "Model": model}]})
        }
        "summary" if whats => json!({"STATUS": status(true, "Summary"), "SUMMARY": [{
            "Elapsed": t as u64, "MHS av": 100_000_000.0, "MHS 5s": 100_000_000.0 * wobble,
            "Accepted": st.accepted, "Rejected": 4, "Temperature": 71.5, "Power": 3_400,
            "Fan Speed In": 4_500, "Fan Speed Out": 4_620}]}),
        "summary" => json!({"STATUS": status(true, "Summary"), "SUMMARY": [{
            "Elapsed": t as u64, "GHS 5s": format!("{:.2}", 95_000.0 * wobble), "GHS av": 94_800.0,
            "Accepted": st.accepted, "Rejected": 7, "Hardware Errors": 3}]}),
        "devs" if whats => {
            json!({"STATUS": status(true, "3 ASC(s)"), "DEVS": (0..3).map(|i| json!({
            "Slot": i, "MHS av": 33_000_000.0, "MHS 5s": 33_300_000.0 * wobble,
            "Temperature": 68.0 + i as f64, "Chip Temp Max": 82.0 + i as f64, "Effective Chips": 148})).collect::<Vec<_>>()})
        }
        "stats" if !whats => {
            let mut chains = serde_json::Map::new();
            for n in 1..=3 {
                chains.insert(
                    format!("chain_rate{n}"),
                    json!(format!("{:.2}", 31_666.7 * wobble)),
                );
                chains.insert(format!("chain_acn{n}"), json!(76));
                chains.insert(format!("temp{n}"), json!(60.0 + n as f64 + hot));
                chains.insert(format!("temp2_{n}"), json!(74.0 + n as f64 + hot));
            }
            for n in 1..=4 {
                chains.insert(format!("fan{n}"), json!(5_400 + n * 30));
            }
            chains.insert("Elapsed".into(), json!(t as u64));
            // Old BMMiner joins the two objects without a comma; the reader copes.
            let head = json!({"BMMiner": "2.0.0", "Type": model});
            return format!(
                "{{\"STATUS\":{},\"STATS\":[{}{}],\"id\":1}}",
                status(true, "BMMiner stats"),
                head,
                Value::Object(chains)
            );
        }
        "pools" => {
            json!({"STATUS": status(true, "pools"), "POOLS": st.pools.iter().enumerate().map(|(i, (u, w, a))| json!({
            "POOL": i, "URL": u, "User": w, "Status": "Alive", "Priority": i,
            "Stratum Active": a, "Accepted": if *a { st.accepted } else { 0 }, "Rejected": 0})).collect::<Vec<_>>()})
        }
        "privileged" if privileged => json!({"STATUS": status(true, "Privileged access OK")}),
        "privileged" => return denied(),
        "addpool" | "switchpool" | "removepool" | "restart" if !privileged => return denied(),
        "addpool" => {
            let p: Vec<&str> = param.splitn(3, ',').collect();
            if p.len() < 2 {
                json!({"STATUS": status(false, "Missing addpool details")})
            } else {
                st.pools.push((p[0].into(), p[1].into(), false));
                json!({"STATUS": status(true, &format!("Added pool {}", st.pools.len() - 1))})
            }
        }
        "switchpool" => match param.parse::<usize>() {
            Ok(i) if i < st.pools.len() => {
                for (j, p) in st.pools.iter_mut().enumerate() {
                    p.2 = j == i;
                }
                json!({"STATUS": status(true, &format!("Switching to pool {i}"))})
            }
            _ => json!({"STATUS": status(false, "Invalid pool id")}),
        },
        "removepool" => match param.parse::<usize>() {
            Ok(i) if i < st.pools.len() && !st.pools[i].2 => {
                st.pools.remove(i);
                json!({"STATUS": status(true, "Removed pool")})
            }
            _ => json!({"STATUS": status(false, "Cannot remove pool")}),
        },
        "restart" => {
            st.started = Instant::now();
            json!({"STATUS": status(true, "Restarting")})
        }
        _ => json!({"STATUS": status(false, "Invalid command")}),
    };
    v.to_string()
}

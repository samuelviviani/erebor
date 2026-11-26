//! The agent side: TLS listener, pairing, the allowed commands, the event stream, the
//! ASIC gateway and LAN discovery. Mining itself stays in the engine: controllers only
//! watch and send commands, so mining goes on when they disconnect.

use super::*;
use crate::asic::{self, AsicAction, AsicEntry, AsicStatus};
use crate::fleet::{self, Telemetry};
use crate::hardware::HardwareInfo;
use crate::ipc::{self, Action};
use crate::miner::MinerKind;
use crate::rpc::{self, Shared};
use serde_json::json;
use std::collections::HashMap;
use std::io::BufReader;
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const MAX_CONNECTIONS: usize = 32;
const IDLE: Duration = Duration::from_secs(300);
const ASIC_POLL: Duration = Duration::from_secs(15);
/// After this many failed pairings or unauthorized requests from one address in
/// `LOCKOUT`, that address is ignored until the window ends.
const MAX_FAILURES: u32 = 10;
const LOCKOUT: Duration = Duration::from_secs(600);
pub const DISCOVER_PROBE: &[u8] = b"EREBOR-DISCOVER/1";

pub struct Ctx {
    pub engine: Shared,
    pub data_dir: PathBuf,
    pub identity: Identity,
    started: i64,
    asics: Mutex<Vec<AsicStatus>>,
    pairing: Mutex<()>,
    failures: Mutex<HashMap<IpAddr, (u32, Instant)>>,
    conns: AtomicUsize,
}

impl Ctx {
    pub fn new(engine: Shared) -> Result<Arc<Self>, String> {
        let data_dir = engine
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .data_dir()
            .to_path_buf();
        Ok(Arc::new(Self {
            identity: Identity::load_or_create(&data_dir)?,
            engine,
            data_dir,
            started: crate::now_unix(),
            asics: Mutex::new(Vec::new()),
            pairing: Mutex::new(()),
            failures: Mutex::new(HashMap::new()),
            conns: AtomicUsize::new(0),
        }))
    }

    fn rpc(&self, cmd: &str, args: &Value) -> Result<Value, Value> {
        rpc::call(&self.engine, cmd, args)
    }

    fn hardware(&self, refresh: bool) -> HardwareInfo {
        fleet::hardware_cached(&self.engine, refresh)
    }

    pub fn telemetry(&self) -> Telemetry {
        fleet::telemetry(&self.engine)
    }

    fn trusted(&self, fp: &str) -> Option<TrustedController> {
        controllers(&self.data_dir)
            .into_iter()
            .find(|c| c.fingerprint == fp)
    }

    fn locked_out(&self, ip: IpAddr) -> bool {
        let mut f = self.failures.lock().unwrap_or_else(|p| p.into_inner());
        f.retain(|_, (_, t)| t.elapsed() < LOCKOUT);
        f.get(&ip).is_some_and(|(n, _)| *n >= MAX_FAILURES)
    }

    fn failed(&self, ip: IpAddr) {
        let mut f = self.failures.lock().unwrap_or_else(|p| p.into_inner());
        f.entry(ip).or_insert((0, Instant::now())).0 += 1;
    }
}

/// The listening agent; dropping it stops accepting connections.
pub struct Server {
    pub addr: SocketAddr,
    pub ctx: Arc<Ctx>,
    stop: Arc<AtomicBool>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_millis(200));
    }
}

/// Listen on `cfg.listen` (TLS), answer discovery when `cfg.discovery`, and watch the
/// configured ASICs.
pub fn serve(engine: Shared, cfg: &RemoteConfig) -> Result<Server, String> {
    let ctx = Ctx::new(engine)?;
    let tls = server_config(&ctx.identity)?;
    let l = TcpListener::bind(&cfg.listen).map_err(|e| format!("listen {}: {e}", cfg.listen))?;
    let addr = l.local_addr().map_err(|e| e.to_string())?;
    let stop = Arc::new(AtomicBool::new(false));
    let (c, s) = (ctx.clone(), stop.clone());
    std::thread::spawn(move || {
        for sock in l.incoming() {
            if s.load(Ordering::SeqCst) {
                return;
            }
            let Ok(sock) = sock else { continue };
            if c.conns.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
                continue;
            }
            let (c, tls) = (c.clone(), tls.clone());
            c.conns.fetch_add(1, Ordering::SeqCst);
            std::thread::spawn(move || {
                handle(&c, tls, sock);
                c.conns.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
    if cfg.discovery {
        let (c, s) = (ctx.clone(), stop.clone());
        let port = addr.port();
        std::thread::spawn(move || discovery(&c, port, &s));
    }
    let (c, s) = (ctx.clone(), stop.clone());
    std::thread::spawn(move || loop {
        let list = RemoteConfig::load(&c.data_dir).asics;
        let read = read_asics(&list);
        *c.asics.lock().unwrap_or_else(|p| p.into_inner()) = read;
        let t = Instant::now();
        while t.elapsed() < ASIC_POLL {
            if s.load(Ordering::SeqCst) {
                return;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
    Ok(Server { addr, ctx, stop })
}

fn read_asics(list: &[AsicEntry]) -> Vec<AsicStatus> {
    std::thread::scope(|s| {
        let hs: Vec<_> = list
            .iter()
            .map(|e| s.spawn(move || asic::read(e)))
            .collect();
        hs.into_iter().filter_map(|h| h.join().ok()).collect()
    })
}

fn discovery(ctx: &Ctx, port: u16, stop: &AtomicBool) {
    let Ok(sock) = UdpSocket::bind(("0.0.0.0", port)) else {
        eprintln!("discovery: UDP port {port} busy, LAN discovery off");
        return;
    };
    let _ = sock.set_read_timeout(Some(Duration::from_secs(1)));
    let mut buf = [0u8; 64];
    while !stop.load(Ordering::SeqCst) {
        let Ok((n, from)) = sock.recv_from(&mut buf) else {
            continue;
        };
        // Only probes from the local/private networks: no amplification towards the Internet.
        if &buf[..n] != DISCOVER_PROBE || !asic::is_private(from.ip()) {
            continue;
        }
        let dev = fleet::device(&ctx.data_dir).unwrap_or_default();
        let r = json!({
            "erebor": PROTOCOL, "id": dev.id, "name": dev.name, "port": port,
            "version": env!("CARGO_PKG_VERSION"), "fingerprint": ctx.identity.fingerprint,
            "pairing": pending_pairing(&ctx.data_dir).is_some(),
        });
        let _ = sock.send_to(r.to_string().as_bytes(), from);
    }
}

fn reply(w: &mut impl Write, id: &str, r: Result<Value, Value>) -> std::io::Result<()> {
    write_line(
        w,
        &match r {
            Ok(v) => json!({"id": id, "ok": v}),
            Err(e) => json!({"id": id, "err": e}),
        },
    )
}

fn handle(ctx: &Ctx, tls: Arc<rustls::ServerConfig>, mut tcp: TcpStream) {
    let Ok(peer) = tcp.peer_addr() else { return };
    if ctx.locked_out(peer.ip()) {
        return;
    }
    let _ = tcp.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = tcp.set_write_timeout(Some(Duration::from_secs(30)));
    let _ = tcp.set_nodelay(true);
    let Ok(mut conn) = rustls::ServerConnection::new(tls) else {
        return;
    };
    while conn.is_handshaking() {
        if conn.complete_io(&mut tcp).is_err() {
            return;
        }
    }
    let Some(fp) = conn
        .peer_certificates()
        .and_then(|c| c.first())
        .map(|c| fingerprint(c))
    else {
        return;
    };
    let _ = tcp.set_read_timeout(Some(IDLE));
    let mut r = BufReader::new(rustls::StreamOwned::new(conn, tcp));
    let peer_s = peer.ip().to_string();
    while let Ok(Some(line)) = read_line(&mut r) {
        let Ok(req) = serde_json::from_str::<Request>(&line) else {
            let _ = reply(r.get_mut(), "", Err("bad_request".into()));
            return;
        };
        let w = r.get_mut();
        let trusted = ctx.trusted(&fp);
        let res = match req.cmd.as_str() {
            // Answered for any version and any peer, so clients can negotiate.
            "hello" => Ok(json!({
                "protocol": PROTOCOL, "minProtocol": MIN_PROTOCOL,
                "agentVersion": env!("CARGO_PKG_VERSION"),
                "deviceId": fleet::device(&ctx.data_dir).unwrap_or_default().id,
                "paired": trusted.is_some(),
                "pairing": pending_pairing(&ctx.data_dir).is_some(),
                "time": crate::now_unix(),
            })),
            _ if !(MIN_PROTOCOL..=PROTOCOL).contains(&req.v) => Err("protocol_version".into()),
            "pair" => {
                let r = pair(ctx, &fp, &req.args);
                audit(
                    &ctx.data_dir,
                    req.args["name"].as_str().unwrap_or("?"),
                    &peer_s,
                    "pair",
                    r.is_ok(),
                );
                if r.is_err() {
                    ctx.failed(peer.ip());
                }
                r
            }
            _ if trusted.is_none() => {
                ctx.failed(peer.ip());
                audit(
                    &ctx.data_dir,
                    "-",
                    &peer_s,
                    &format!("unauthorized:{}", req.cmd),
                    false,
                );
                let _ = reply(w, &req.id, Err("unauthorized".into()));
                return;
            }
            _ if (crate::now_unix() - req.ts).abs() > MAX_CLOCK_SKEW => Err("clock_skew".into()),
            "subscribe" => {
                if reply(w, &req.id, Ok(Value::Null)).is_ok() {
                    let every = req.args["intervalSecs"].as_u64().unwrap_or(3).clamp(1, 60);
                    stream(ctx, w, &fp, Duration::from_secs(every));
                }
                return;
            }
            cmd => {
                let who = trusted.map(|t| t.name).unwrap_or_default();
                let r = dispatch(ctx, &fp, cmd, &req.args);
                if MUTATING.contains(&cmd) {
                    audit(&ctx.data_dir, &who, &peer_s, cmd, r.is_ok());
                }
                r
            }
        };
        if reply(w, &req.id, res).is_err() {
            return;
        }
    }
}

const MUTATING: &[&str] = &[
    "mining.start",
    "mining.stop",
    "mining.restart",
    "mining.config.set",
    "miners.install",
    "device.rename",
    "asics.add",
    "asics.remove",
    "asics.action",
    "asics.scan",
    "unpair",
];

fn pair(ctx: &Ctx, controller_fp: &str, args: &Value) -> Result<Value, Value> {
    let _g = ctx.pairing.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut p) = pending_pairing(&ctx.data_dir) else {
        return Err("pairing_not_active".into());
    };
    let given = args["proof"].as_str().unwrap_or("");
    let name: String = args["name"]
        .as_str()
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect();
    let agent_fp = &ctx.identity.fingerprint;
    if !same_proof(
        &proof(&p.code, "controller", agent_fp, controller_fp),
        given,
    ) {
        p.attempts += 1;
        let path = dir(&ctx.data_dir).join("pairing.json");
        if p.attempts >= PAIRING_MAX_ATTEMPTS {
            cancel_pairing(&ctx.data_dir);
        } else {
            let _ = write_json(&path, &p);
        }
        return Err("pairing_failed".into());
    }
    trust(
        &ctx.data_dir,
        TrustedController {
            fingerprint: controller_fp.into(),
            name: if name.is_empty() {
                "controller".into()
            } else {
                name
            },
            paired_at: crate::now_unix(),
        },
    )
    .map_err(Value::from)?;
    // Single use: the code is gone, from now on only the certificates count.
    cancel_pairing(&ctx.data_dir);
    Ok(json!({
        "proof": proof(&p.code, "agent", agent_fp, controller_fp),
        "device": fleet::device(&ctx.data_dir).unwrap_or_default(),
    }))
}

fn arg<T: for<'de> Deserialize<'de>>(args: &Value, k: &str) -> Result<T, Value> {
    serde_json::from_value(args.get(k).cloned().unwrap_or(Value::Null))
        .map_err(|_| Value::from(format!("bad_argument:{k}")))
}

fn mining(ctx: &Ctx, action: Action) -> Result<Value, Value> {
    let o = ipc::control_via(&|c, a| ctx.rpc(c, a), action)?;
    Ok(json!({"state": "completed", "changed": o.changed, "snapshot": o.snapshot}))
}

/// Every command a paired controller may send. Anything else is `unknown_command`.
pub fn dispatch(ctx: &Ctx, controller_fp: &str, cmd: &str, args: &Value) -> Result<Value, Value> {
    match cmd {
        "device" => Ok(json!({
            "device": fleet::device(&ctx.data_dir).map_err(Value::from)?,
            "hostname": rpc::hostname(),
            "agentVersion": env!("CARGO_PKG_VERSION"),
            "protocol": PROTOCOL,
            "fingerprint": ctx.identity.fingerprint,
            "hardware": ctx.hardware(false),
            "gateway": !RemoteConfig::load(&ctx.data_dir).asics.is_empty(),
        })),
        "health" => Ok(json!({
            "status": "ok",
            "agentVersion": env!("CARGO_PKG_VERSION"),
            "protocol": PROTOCOL,
            "uptimeSecs": crate::now_unix() - ctx.started,
        })),
        "hardware" => Ok(json!(
            ctx.hardware(args["refresh"].as_bool().unwrap_or(false))
        )),
        "telemetry" => Ok(json!(ctx.telemetry())),
        "mining.status" => ctx.rpc("snapshot", &Value::Null),
        "mining.config" => ctx.rpc("get_config", &Value::Null),
        // The engine keeps miner paths and other local-only fields (`update_config`).
        "mining.config.set" => {
            let config = ctx.rpc("update_config", &json!({"config": args.get("config")}))?;
            Ok(json!({"state": "completed", "config": config}))
        }
        "mining.start" => mining(ctx, Action::Start),
        "mining.stop" => mining(ctx, Action::Stop),
        "mining.restart" => mining(ctx, Action::Restart),
        "miners" => {
            let c = ctx.rpc("get_config", &Value::Null)?;
            let cfg: crate::config::AppConfig =
                serde_json::from_value(c).map_err(|_| Value::from("bad_config"))?;
            Ok(json!(MinerKind::ALL
                .iter()
                .map(|k| {
                    let m = cfg.miner_for(*k);
                    let pin = crate::install::manifest_for(*k);
                    json!({
                        "kind": k, "installed": m.path.is_some(), "version": m.version,
                        "verified": m.verified, "pinnedVersion": pin.version,
                        "availableHere": pin.asset_for_platform().is_ok(),
                    })
                })
                .collect::<Vec<_>>()))
        }
        "miners.install" => {
            let kind: MinerKind = arg(args, "kind")?;
            ctx.rpc("install_miner", &json!({"kind": kind}))?;
            Ok(json!({"state": "accepted"}))
        }
        "coins" => ctx.rpc("coin_options", &Value::Null),
        "logs" => ctx.rpc("get_logs", &Value::Null),
        "stats" => ctx.rpc(
            "get_stats",
            &json!({"rangeSecs": arg::<i64>(args, "rangeSecs")?}),
        ),
        "device.rename" => {
            let name: String = arg(args, "name")?;
            fleet::set_name(&ctx.data_dir, &name)
                .map(|d| json!(d))
                .map_err(Value::from)
        }
        "asics" => Ok(json!(*ctx.asics.lock().unwrap_or_else(|p| p.into_inner()))),
        "asics.add" => {
            let e: AsicEntry = arg(args, "asic")?;
            let e = AsicEntry {
                port: if e.port == 0 { asic::API_PORT } else { e.port },
                ..e
            };
            asic::validate_entry(&e).map_err(Value::from)?;
            let mut cfg = RemoteConfig::load(&ctx.data_dir);
            if cfg.asics.iter().any(|x| x.id() == e.id()) {
                return Err("asic_exists".into());
            }
            cfg.asics.push(e.clone());
            cfg.save(&ctx.data_dir).map_err(Value::from)?;
            let st = asic::read(&e);
            ctx.asics
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .push(st.clone());
            Ok(json!(st))
        }
        "asics.remove" => {
            let id: String = arg(args, "id")?;
            let mut cfg = RemoteConfig::load(&ctx.data_dir);
            let n = cfg.asics.len();
            cfg.asics.retain(|x| x.id() != id);
            if cfg.asics.len() == n {
                return Err("asic_not_found".into());
            }
            cfg.save(&ctx.data_dir).map_err(Value::from)?;
            ctx.asics
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .retain(|s| s.id != id);
            Ok(Value::Null)
        }
        // Only ASICs configured on this gateway can be reached through it.
        "asics.action" => {
            let id: String = arg(args, "id")?;
            let action: AsicAction = arg(args, "action")?;
            let e = RemoteConfig::load(&ctx.data_dir)
                .asics
                .into_iter()
                .find(|x| x.id() == id)
                .ok_or_else(|| Value::from("asic_not_found"))?;
            asic::act(&e, &action).map_err(Value::from)
        }
        "asics.scan" => {
            let cidr: String = arg(args, "cidr")?;
            asic::scan(&cidr, args["port"].as_u64().unwrap_or(0) as u16)
                .map(|v| json!(v))
                .map_err(Value::from)
        }
        "controllers" => Ok(json!(controllers(&ctx.data_dir)
            .iter()
            .map(
                |c| json!({"name": c.name, "fingerprint": short_fp(&c.fingerprint),
                "pairedAt": c.paired_at, "you": c.fingerprint == controller_fp})
            )
            .collect::<Vec<_>>())),
        "audit" => Ok(json!(audit_tail(&ctx.data_dir, 200))),
        "unpair" => revoke(&ctx.data_dir, controller_fp)
            .map(|_| Value::Null)
            .map_err(Value::from),
        _ => Err("unknown_command".into()),
    }
}

/// Telemetry every `every`, state changes and new alerts as they happen, ASIC readings
/// when this agent is a gateway. Ends when the client goes away or is revoked.
fn stream(ctx: &Ctx, w: &mut impl Write, fp: &str, every: Duration) {
    let mut last_state = String::new();
    let mut last_alert = u64::MAX;
    let mut last_asics = Instant::now() - ASIC_POLL;
    loop {
        if ctx.trusted(fp).is_none() {
            let _ = write_line(w, &json!({"event": "revoked"}));
            return;
        }
        let t = ctx.telemetry();
        let ts = t.ts;
        let device = t.device.id.clone();
        let mut events = Vec::new();
        if t.snapshot.state != last_state {
            if !last_state.is_empty() {
                events.push(json!({"event": "state", "ts": ts, "device": device,
                    "data": {"from": last_state, "to": t.snapshot.state}}));
            }
            last_state = t.snapshot.state.clone();
        }
        let alerts = ctx
            .engine
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .status()
            .alerts;
        let max = alerts.iter().map(|a| a.id).max().unwrap_or(0);
        if last_alert != u64::MAX {
            for a in alerts.iter().filter(|a| a.id > last_alert) {
                events.push(json!({"event": "alert", "ts": ts, "device": device, "data": a}));
            }
        }
        last_alert = max;
        events.push(json!({"event": "telemetry", "ts": ts, "device": device, "data": t}));
        if last_asics.elapsed() >= ASIC_POLL {
            last_asics = Instant::now();
            let a = ctx.asics.lock().unwrap_or_else(|p| p.into_inner()).clone();
            if !a.is_empty() {
                events.push(json!({"event": "asics", "ts": ts, "device": device, "data": a}));
            }
        }
        for e in &events {
            if write_line(w, e).is_err() {
                return;
            }
        }
        std::thread::sleep(every);
    }
}

#[cfg(test)]
mod tests {
    use super::client::{self, Link};
    use super::*;
    use crate::asic_sim::{SimAsic, SimKind};
    use crate::engine::{self, Engine};

    fn demo_agent(dir: &Path) -> Server {
        std::fs::write(dir.join("config.json"), r#"{"demoMode":true}"#).unwrap();
        let e = Engine::new(dir.into(), engine::default_factory()).unwrap();
        let cfg = RemoteConfig {
            enabled: true,
            listen: "127.0.0.1:0".into(),
            discovery: false,
            asics: vec![],
        };
        serve(Arc::new(Mutex::new(e)), &cfg).unwrap()
    }

    #[test]
    fn pairing_pinning_control_and_revocation() {
        let (a, c) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let srv = demo_agent(a.path());
        let addr = srv.addr.to_string();
        let me = Identity::load_or_create(c.path()).unwrap();

        // Not paired: only hello and pair.
        let mut l = Link::connect(&me, &addr, None).unwrap();
        assert_eq!(l.hello["paired"], false);
        assert_eq!(
            l.call("mining.status", &Value::Null),
            Err("unauthorized".into())
        );
        assert_eq!(
            client::pair(&me, &addr, "ABCD-EFGH-JKMN", "ctl"),
            Err("pairing_not_active".into())
        );

        // Wrong code: refused and counted; the right one works once.
        let code = start_pairing(a.path()).unwrap().code;
        assert_eq!(
            client::pair(&me, &addr, "0000-0000-0000", "ctl"),
            Err("pairing_failed".into())
        );
        let p = client::pair(&me, &addr, &format_code(&code).to_lowercase(), "laptop").unwrap();
        assert_eq!(p.fingerprint, srv.ctx.identity.fingerprint);
        assert!(pending_pairing(a.path()).is_none(), "single use");
        assert_eq!(controllers(a.path())[0].name, "laptop");

        // Pinned: the agent's certificate must match.
        let other = Identity::load_or_create(tempfile::tempdir().unwrap().path()).unwrap();
        assert_eq!(
            Link::connect(&me, &addr, Some(&other.fingerprint)).err(),
            Some("fingerprint_mismatch".into())
        );
        let mut l = Link::connect(&me, &addr, Some(&p.fingerprint)).unwrap();
        assert_eq!(l.hello["paired"], true);
        assert_eq!(l.call("health", &Value::Null).unwrap()["status"], "ok");
        let r = l.call("mining.start", &Value::Null).unwrap();
        assert_eq!(
            (r["state"].as_str(), r["snapshot"]["state"].as_str()),
            (Some("completed"), Some("running"))
        );
        let t = l.call("telemetry", &Value::Null).unwrap();
        assert_eq!(t["simulated"], true);
        assert_eq!(
            t["gpus"]
                .as_array()
                .map(|g| g.iter().all(|g| g["hotspotC"].is_null())),
            Some(true)
        );
        assert_eq!(
            l.call("rm -rf /", &Value::Null),
            Err("unknown_command".into())
        );
        assert_eq!(
            l.call("mining.stop", &Value::Null).unwrap()["snapshot"]["state"],
            "stopped"
        );

        // Config edits go through the engine's validation and keep local-only fields.
        let mut cfg = l.call("mining.config", &Value::Null).unwrap();
        cfg["miner"]["path"] = json!("/tmp/evil");
        cfg["pool"]["worker"] = json!("rig01");
        let saved = l
            .call("mining.config.set", &json!({"config": cfg}))
            .unwrap();
        assert_eq!(saved["config"]["pool"]["worker"], "rig01");
        assert!(
            saved["config"]["miner"]["path"].is_null(),
            "remote can't choose an executable"
        );

        // A stale request (recorded and replayed later) is refused.
        let old = json!({"v": 1, "id": "x", "ts": 1, "cmd": "mining.start", "args": {}});
        write_line(l.r_mut(), &old).unwrap();
        let reply: Value = serde_json::from_str(&read_line(l.reader()).unwrap().unwrap()).unwrap();
        assert_eq!(reply["err"], "clock_skew");

        // Event stream: telemetry flows; revocation ends it and later requests.
        let mut s = Link::connect(&me, &addr, Some(&p.fingerprint)).unwrap();
        s.subscribe(1).unwrap();
        assert_eq!(s.next_event().unwrap()["event"], "telemetry");
        revoke(a.path(), &me.fingerprint[..16]).unwrap();
        let mut saw_revoked = false;
        for _ in 0..5 {
            match s.next_event() {
                Ok(e) if e["event"] == "revoked" => {
                    saw_revoked = true;
                    break;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        assert!(saw_revoked);
        assert_eq!(l.call("health", &Value::Null), Err("unauthorized".into()));
        let audit = audit_tail(a.path(), 50);
        assert!(audit
            .iter()
            .any(|e| e["cmd"] == "mining.start" && e["ok"] == true));
        assert!(audit.iter().any(|e| e["cmd"] == "pair" && e["ok"] == false));
    }

    #[test]
    fn repeated_failures_lock_the_address_out() {
        let (a, c) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let srv = demo_agent(a.path());
        let me = Identity::load_or_create(c.path()).unwrap();
        let addr = srv.addr.to_string();
        start_pairing(a.path()).unwrap();
        for _ in 0..PAIRING_MAX_ATTEMPTS {
            let _ = client::pair(&me, &addr, "0000-0000-0000", "x");
        }
        assert!(
            pending_pairing(a.path()).is_none(),
            "code burnt after 5 tries"
        );
        for _ in 0..MAX_FAILURES {
            if let Ok(mut l) = Link::connect(&me, &addr, None) {
                let _ = l.call("health", &Value::Null);
            }
        }
        assert!(Link::connect(&me, &addr, None).is_err(), "locked out");
    }

    #[test]
    fn gateway_reads_and_controls_only_configured_asics() {
        let (a, c) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let srv = demo_agent(a.path());
        let me = Identity::load_or_create(c.path()).unwrap();
        let addr = srv.addr.to_string();
        let code = start_pairing(a.path()).unwrap().code;
        let p = client::pair(&me, &addr, &code, "ctl").unwrap();
        let mut l = Link::connect(&me, &addr, Some(&p.fingerprint)).unwrap();

        let sim = SimAsic::start(SimKind::Antminer { privileged: true }).unwrap();
        let st = l
            .call("asics.add", &json!({"asic": sim.entry("s19-a")}))
            .unwrap();
        assert_eq!(
            (st["online"].as_bool(), st["simulated"].as_bool()),
            (Some(true), Some(true))
        );
        assert_eq!(
            l.call("asics.add", &json!({"asic": sim.entry("dup")})),
            Err("asic_exists".into())
        );
        assert_eq!(
            l.call(
                "asics.add",
                &json!({"asic": {"host": "8.8.8.8", "port": 4028}})
            ),
            Err("asic_address_not_private".into())
        );
        assert_eq!(
            l.call("asics", &Value::Null)
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let id = st["id"].as_str().unwrap();
        let r = l
            .call(
                "asics.action",
                &json!({"id": id, "action": {"action": "restart"}}),
            )
            .unwrap();
        assert_eq!(r["state"], "accepted");
        assert_eq!(
            l.call(
                "asics.action",
                &json!({"id": "asic:10.9.9.9:4028", "action": {"action": "restart"}})
            ),
            Err("asic_not_found".into())
        );
        l.call("asics.remove", &json!({"id": id})).unwrap();
        assert!(RemoteConfig::load(a.path()).asics.is_empty());
    }
}

//! A simulated infrastructure in this process: demo agents behind the real remote
//! protocol (demo miner: no process, no network, `simulated`) and ASIC simulators (model
//! "… (Erebor simulator)"). Used by `erebor sim` and by the app's demo mode, where it
//! lives in its own registry (`demo-fleet/`) and never mixes with real devices.
//!
//! Layout: 3 PCs, 2 GPU rigs, a gateway server with 3 ASICs behind it (Antminer,
//! WhatsMiner, one answering garbage), 2 direct ASICs (one hot, one read-only), and one
//! agent paired and then shut down (offline).

use crate::asic_sim::{SimAsic, SimKind};
use crate::controller::{DeviceKind, Fleet, LocalCall};
use crate::engine::{self, Engine};
use crate::remote::{self, server, RemoteConfig};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct SimAgent {
    pub name: &'static str,
    pub kind: DeviceKind,
    pub addr: String,
    pub code: String,
    pub dir: PathBuf,
    server: Option<server::Server>,
}

pub struct SimAsicDev {
    pub name: &'static str,
    /// Reached through the gateway agent rather than directly.
    pub behind_gateway: bool,
    pub sim: SimAsic,
}

/// Everything simulated; dropping it stops listening (demo engines stay idle in memory).
pub struct Lab {
    pub agents: Vec<SimAgent>,
    pub asics: Vec<SimAsicDev>,
}

const AGENTS: [(&str, DeviceKind, bool); 7] = [
    ("sim-pc-1", DeviceKind::RemotePc, true),
    ("sim-pc-2", DeviceKind::RemotePc, false),
    ("sim-pc-3", DeviceKind::RemotePc, true),
    ("sim-rig-1", DeviceKind::GpuRig, true),
    ("sim-rig-2", DeviceKind::GpuRig, true),
    ("sim-gateway", DeviceKind::Server, false),
    ("sim-offline", DeviceKind::Server, false),
];

fn agent(
    root: &Path,
    name: &'static str,
    kind: DeviceKind,
    listen: String,
    mine: bool,
) -> Result<SimAgent, String> {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(
        dir.join("config.json"),
        r#"{"demoMode":true,"onboardingCompleted":true}"#,
    )
    .map_err(|e| e.to_string())?;
    crate::fleet::set_name(&dir, name)?;
    let e = Arc::new(Mutex::new(Engine::new(
        dir.clone(),
        engine::default_factory(),
    )?));
    engine::spawn_loops(e.clone());
    if mine {
        e.lock().unwrap_or_else(|p| p.into_inner()).start(false)?;
    }
    let cfg = RemoteConfig {
        enabled: true,
        listen,
        discovery: false,
        asics: vec![],
    };
    let s = server::serve(e, &cfg)?;
    Ok(SimAgent {
        name,
        kind,
        addr: s.addr.to_string(),
        code: remote::start_pairing(&dir)?.code,
        dir,
        server: Some(s),
    })
}

/// Start the lab under `root` on 127.0.0.1: fixed ports from `base_port`, or free ones.
pub fn start(root: &Path, base_port: Option<u16>) -> Result<Lab, String> {
    let addr = |i: u16| format!("127.0.0.1:{}", base_port.map_or(0, |b| b + i));
    let mut agents = vec![];
    for (i, (name, kind, mine)) in AGENTS.into_iter().enumerate() {
        agents.push(agent(root, name, kind, addr(1 + i as u16), mine)?);
    }
    let mut asics = vec![];
    for (i, (name, kind, behind)) in [
        ("sim-s19-a", SimKind::Antminer { privileged: true }, true),
        ("sim-m30s", SimKind::Whatsminer, true),
        ("sim-broken", SimKind::Broken, true),
        ("sim-s19-hot", SimKind::HotAntminer, false),
        ("sim-s19-ro", SimKind::Antminer { privileged: false }, false),
    ]
    .into_iter()
    .enumerate()
    {
        asics.push(SimAsicDev {
            name,
            behind_gateway: behind,
            sim: SimAsic::start_on(&addr(101 + i as u16), kind)?,
        });
    }
    Ok(Lab { agents, asics })
}

impl Lab {
    /// Pair everything into `f`; the "offline" agent is then shut down. Returns the ids.
    pub fn pair_into(&mut self, f: &Fleet) -> Result<Vec<String>, String> {
        let mut ids = vec![];
        for a in &self.agents {
            ids.push(f.add_agent(&a.addr, &a.code, Some(a.kind))?.id);
        }
        let gw = self
            .agents
            .iter()
            .find(|a| a.name == "sim-gateway")
            .ok_or("no gateway")?;
        let gw = format!("agent:{}", crate::fleet::device(&gw.dir)?.id);
        for a in &self.asics {
            ids.push(
                f.add_asic(
                    "127.0.0.1",
                    a.sim.port,
                    a.name,
                    a.behind_gateway.then_some(gw.as_str()),
                )?
                .id,
            );
        }
        if let Some(a) = self.agents.iter_mut().find(|a| a.name == "sim-offline") {
            a.server = None;
        }
        Ok(ids)
    }
}

/// The app's demo mode: a fresh simulated infrastructure in `<data dir>/demo-fleet/`, its
/// own registry, a week of made-up history, and this computer's (demo) engine as `local`.
pub struct Demo {
    pub fleet: Arc<Fleet>,
    _lab: Lab,
}

pub fn demo(data_dir: &Path, local: LocalCall) -> Result<Demo, String> {
    let root = data_dir.join("demo-fleet");
    // Fresh each time the app starts: old ports and registries are meaningless.
    let _ = std::fs::remove_dir_all(&root);
    let lab_dir = root.join("agents");
    let mut lab = start(&lab_dir, None)?;
    let fleet = Fleet::open(&root, Some(local))?;
    lab.pair_into(&fleet)?;
    fleet.refresh_once();
    fleet.seed_demo_history(7)?;
    fleet.start();
    Ok(Demo { fleet, _lab: lab })
}

/// The controller the app talks to: the real registry, or, while this computer's engine
/// is in demo mode, the demo one (created on first use, kept until the app quits).
pub struct Hub {
    real: Option<Arc<Fleet>>,
    demo: Mutex<Option<Result<Demo, String>>>,
    local: LocalCall,
    data_dir: PathBuf,
}

impl Hub {
    pub fn new(data_dir: &Path, local: LocalCall) -> Self {
        let real = match Fleet::open(data_dir, Some(local.clone())) {
            Ok(f) => {
                f.start();
                Some(f)
            }
            Err(e) => {
                eprintln!("device management unavailable: {e}");
                None
            }
        };
        Self {
            real,
            demo: Mutex::new(None),
            local,
            data_dir: data_dir.to_path_buf(),
        }
    }

    fn active(&self) -> Result<Arc<Fleet>, String> {
        let demo_mode = (self.local)("get_config", &serde_json::Value::Null)
            .is_ok_and(|c| c["demoMode"] == true);
        if !demo_mode {
            return self.real.clone().ok_or_else(|| "fleet_unavailable".into());
        }
        let mut d = self.demo.lock().unwrap_or_else(|p| p.into_inner());
        if d.is_none() {
            *d = Some(demo(&self.data_dir, self.local.clone()));
        }
        match d.as_ref() {
            Some(Ok(demo)) => Ok(demo.fleet.clone()),
            Some(Err(e)) => Err(e.clone()),
            None => Err("fleet_unavailable".into()),
        }
    }

    pub fn call(
        &self,
        cmd: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, serde_json::Value> {
        let f = self.active().map_err(serde_json::Value::from)?;
        crate::controller::call(&f, cmd, args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::Conn;
    use serde_json::{json, Value};

    #[test]
    fn demo_mode_shows_a_simulated_infrastructure_apart_from_real_devices() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("config.json"), r#"{"demoMode":false}"#).unwrap();
        let e: crate::rpc::Shared = Arc::new(Mutex::new(
            Engine::new(d.path().into(), engine::default_factory()).unwrap(),
        ));
        let e2 = e.clone();
        let local: LocalCall = Arc::new(move |c, a| crate::rpc::call(&e2, c, a));
        let hub = Hub::new(d.path(), local);

        // Real mode: an empty registry (this PC isn't set up for mining).
        let real = hub.call("fleet_list", &Value::Null).unwrap();
        assert_eq!(real["devices"].as_array().unwrap().len(), 0);

        // Demo mode: simulated devices, data in the charts, alerts from the hot ASIC.
        let mut c: crate::config::AppConfig =
            serde_json::from_value(crate::rpc::call(&e, "get_config", &Value::Null).unwrap())
                .unwrap();
        c.demo_mode = true;
        crate::rpc::call(&e, "update_config", &json!({"config": c})).unwrap();
        let demo = hub.call("fleet_list", &Value::Null).unwrap();
        let devs: Vec<crate::controller::DeviceRow> =
            serde_json::from_value(demo["devices"].clone()).unwrap();
        assert_eq!(devs.len(), 13, "12 simulated + this PC in demo");
        assert!(devs
            .iter()
            .filter(|r| r.id != crate::controller::LOCAL_ID)
            .all(|r| r.simulated || r.conn != Conn::Online));
        assert!(devs
            .iter()
            .any(|r| r.name == "sim-offline" && r.conn != Conn::Online));
        let h = hub
            .call("fleet_history", &json!({"rangeSecs": 7 * 86_400}))
            .unwrap();
        assert!(h["points"].as_array().unwrap().len() > 50);
        assert!(h["hashrate"]["sha256d"].is_array());

        // Back to real mode: the real registry, untouched.
        c.demo_mode = false;
        crate::rpc::call(&e, "update_config", &json!({"config": c})).unwrap();
        hub.call("fleet_refresh", &Value::Null).unwrap(); // the app re-reads this PC every 3 s
        assert_eq!(
            hub.call("fleet_list", &Value::Null).unwrap()["devices"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }
}

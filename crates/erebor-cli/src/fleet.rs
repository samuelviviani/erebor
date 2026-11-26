//! `erebor devices`, `erebor groups`, `erebor profiles` and mining on other devices
//! (`erebor mining start --device rig-01`). Same registry and certificate as the desktop
//! app (`<data dir>/controller.db`, `remote/`): devices paired in one show in the other.
//! Each command connects, does its work and exits; nothing has to keep running.

use crate::{print_json, ui, Fail};
use anstream::println;
use clap::{Args, Subcommand, ValueEnum};
use erebor_core::controller::{
    self, AsicPoolSpec, Conn, DeviceKind, DeviceRow, Fleet, Group, LocalCall, Mining, OpResult,
    Profile,
};
use erebor_core::ipc;
use erebor_tui::fmt;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;

/// Which devices a command acts on. None of them: this computer.
#[derive(Args, Debug, Clone, Default)]
pub struct Target {
    /// Device name or id (repeat for several).
    #[arg(long = "device", short = 'd', value_name = "DEVICE")]
    pub devices: Vec<String>,
    /// Every device in this group.
    #[arg(long, value_name = "GROUP")]
    pub group: Option<String>,
    /// Every device.
    #[arg(long)]
    pub all: bool,
}

impl Target {
    pub fn is_set(&self) -> bool {
        !self.devices.is_empty() || self.group.is_some() || self.all
    }
}

#[derive(ValueEnum, Debug, Clone, Copy)]
pub enum Kind {
    RemotePc,
    Server,
    GpuRig,
    CpuRig,
}

impl From<Kind> for DeviceKind {
    fn from(k: Kind) -> Self {
        match k {
            Kind::RemotePc => DeviceKind::RemotePc,
            Kind::Server => DeviceKind::Server,
            Kind::GpuRig => DeviceKind::GpuRig,
            Kind::CpuRig => DeviceKind::CpuRig,
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum DevicesCmd {
    /// Every device with its state (the default).
    List,
    /// Agents on this network (and, with --asic, ASIC APIs in a subnet you own).
    Discover {
        /// Scan this private subnet for ASICs (CIDR, at most /22), e.g. 192.168.1.0/24.
        #[arg(long, value_name = "CIDR")]
        asic: Option<String>,
        /// Scan from this gateway agent instead of this computer.
        #[arg(long, value_name = "GATEWAY")]
        via: Option<String>,
    },
    /// Pair an agent (address + the code `erebor-agent pair` shows) or add an ASIC (--asic).
    Add {
        /// host, host:port or IP of the agent, or of the ASIC with --asic.
        address: String,
        /// Pairing code shown by the agent.
        #[arg(long, required_unless_present = "asic")]
        code: Option<String>,
        /// How the device is shown.
        #[arg(long, value_enum)]
        kind: Option<Kind>,
        /// The address is an ASIC (CGMiner-compatible API, port 4028).
        #[arg(long)]
        asic: bool,
        /// ASIC API port (default 4028).
        #[arg(long)]
        port: Option<u16>,
        #[arg(long)]
        name: Option<String>,
        /// Reach the ASIC through this gateway agent.
        #[arg(long, value_name = "GATEWAY")]
        via: Option<String>,
    },
    /// Remove a device (an agent also forgets this computer).
    Remove { device: String },
    /// State of the selected devices (default: all).
    Status {
        #[command(flatten)]
        target: Target,
    },
    /// Everything known about one device: state, hardware, configuration, capabilities.
    Inspect { device: String },
    /// The miner log of a computer.
    Logs { device: String },
    /// Rename a device.
    Rename { device: String, name: String },
    /// Add or remove tags.
    Tag {
        device: Vec<String>,
        #[arg(long, value_delimiter = ',', required = true)]
        tags: Vec<String>,
        #[arg(long)]
        remove: bool,
    },
    /// Export groups, profiles and devices as JSON (no private key; ASIC pool passwords
    /// only with --secrets).
    Export {
        #[arg(long)]
        secrets: bool,
    },
    /// Import an export (merged with what is here).
    Import { file: std::path::PathBuf },
    /// Alerts and operations, newest first.
    Events {
        #[arg(long, default_value_t = 50)]
        limit: i64,
    },
}

#[derive(Subcommand, Debug)]
pub enum GroupsCmd {
    /// Groups and how many devices each has.
    List,
    /// New group, optionally with its own electricity price per kWh.
    Create {
        name: String,
        #[arg(long)]
        energy_price: Option<f64>,
    },
    /// Delete a group (its devices stay, ungrouped).
    Delete { name: String },
    /// Put devices in a group.
    Assign { group: String, devices: Vec<String> },
}

#[derive(Subcommand, Debug)]
pub enum ProfilesCmd {
    /// Saved profiles.
    List,
    /// One profile, as JSON.
    Show {
        name: String,
    },
    /// New profile. Computers: any of --coin/--wallet/--worker/--pool-*/--preset/--max-temp
    /// (or --config JSON). ASICs: --asic-url and --asic-user. `{device}` in a worker or
    /// user becomes the device name.
    Create {
        name: String,
        #[arg(long)]
        coin: Option<String>,
        /// Public payout address (never a seed phrase or private key).
        #[arg(long)]
        wallet: Option<String>,
        #[arg(long)]
        worker: Option<String>,
        /// Pool from the catalog (erebor pools list).
        #[arg(long)]
        pool: Option<String>,
        #[arg(long, value_parser = ["eco", "balanced", "performance"])]
        preset: Option<String>,
        #[arg(long)]
        max_temp: Option<f32>,
        #[arg(long)]
        auto_restart: Option<bool>,
        /// Partial configuration as JSON, merged with the options above.
        #[arg(long)]
        config: Option<String>,
        #[arg(long, value_name = "STRATUM_URL")]
        asic_url: Option<String>,
        #[arg(long, value_name = "WALLET.WORKER")]
        asic_user: Option<String>,
        #[arg(long, default_value = "")]
        asic_password: String,
        /// ASICs: apply only to this algorithm (sha256d, scrypt, …).
        #[arg(long)]
        algo: Option<String>,
        /// Start (or restart) mining once applied.
        #[arg(long)]
        start: bool,
    },
    /// Apply a profile; compatibility is checked per device first.
    Apply {
        name: String,
        #[command(flatten)]
        target: Target,
    },
    Delete {
        name: String,
    },
}

pub fn open(dir: &Path) -> Result<Arc<Fleet>, Fail> {
    let d = dir.to_path_buf();
    let local: LocalCall = Arc::new(move |c, a| ipc::call(&d, c, a));
    Fleet::open(dir, Some(local)).map_err(Fail::from)
}

fn conn(c: Conn) -> String {
    let (s, st) = match c {
        Conn::Online => ("online", ui::OK),
        Conn::Connecting => ("connecting", ui::WARN),
        Conn::Unauthorized => ("unauthorized", ui::ERR),
        Conn::Unreachable => ("unreachable", ui::ERR),
        Conn::Offline => ("offline", ui::DIM),
    };
    ui::paint(format!("{s:<12}"), st)
}

fn mining_s(m: Mining) -> String {
    let (s, st) = match m {
        Mining::Mining => ("mining", ui::OK.bold()),
        Mining::Error => ("error", ui::ERR),
        Mining::Starting => ("starting", ui::WARN),
        Mining::Stopping => ("stopping", ui::WARN),
        Mining::Maintenance => ("maintenance", ui::WARN),
        Mining::Idle => ("idle", ui::DIM),
    };
    ui::paint(format!("{s:<11}"), st)
}

fn kind_s(k: DeviceKind) -> &'static str {
    match k {
        DeviceKind::LocalPc => "this PC",
        DeviceKind::RemotePc => "PC",
        DeviceKind::Server => "server",
        DeviceKind::GpuRig => "GPU rig",
        DeviceKind::CpuRig => "CPU rig",
        DeviceKind::Asic => "ASIC",
    }
}

fn cut(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        format!("{s:<n$}")
    } else {
        format!("{}…", s.chars().take(n - 1).collect::<String>())
    }
}

fn table(rows: &[DeviceRow]) {
    println!(
        "{}",
        ui::paint(
            format!(
                "{:<20} {:<8} {:<12} {:<11} {:>12} {:<10} {:>6} {:>7} {:<12}",
                "NAME",
                "KIND",
                "CONNECTION",
                "MINING",
                "HASHRATE",
                "ALGO",
                "TEMP",
                "POWER",
                "GROUP"
            ),
            ui::DIM
        )
    );
    for r in rows {
        println!(
            "{} {:<8} {} {} {:>12} {:<10} {:>6} {:>7} {}{}",
            cut(&r.name, 20),
            kind_s(r.kind),
            conn(r.conn),
            mining_s(r.mining),
            r.hashrate.map_or("—".into(), |h| fmt::hashrate(Some(h))),
            cut(r.algo.as_deref().unwrap_or("—"), 10),
            r.temp_c.map_or("—".into(), |t| format!("{t:.0}°C")),
            r.power_w.map_or("—".into(), |w| format!("{w:.0} W")),
            cut(r.group_name.as_deref().unwrap_or(""), 12),
            if r.simulated {
                ui::paint(" (simulated)", ui::WARN)
            } else {
                String::new()
            },
        );
    }
}

fn overview(f: &Fleet, rows: &[DeviceRow]) {
    let o = f.overview(rows);
    println!(
        "{} devices · {} online · {} offline · {} mining · {:.0} W",
        o["total"],
        o["online"],
        o["offline"],
        o["mining"],
        o["powerW"].as_f64().unwrap_or(0.0)
    );
    for h in o["hashrate"].as_array().into_iter().flatten() {
        println!(
            "  {:<10} {} on {} device(s)",
            h["algo"].as_str().unwrap_or(""),
            fmt::hashrate(h["hashrate"].as_f64()),
            h["devices"]
        );
    }
    for m in o["money"].as_array().into_iter().flatten() {
        let v = |k: &str| m[k].as_f64().map_or("n/a".into(), |x| format!("{x:.2}"));
        println!(
            "  per day (estimate, {}): revenue {} · electricity {} · net {}",
            m["currency"].as_str().unwrap_or(""),
            v("revenueDay"),
            v("costDay"),
            v("netDay")
        );
    }
    if o["simulated"].as_u64().unwrap_or(0) > 0 {
        println!(
            "{}",
            ui::warn(format!(
                "{} simulated device(s): their values are not real.",
                o["simulated"]
            ))
        );
    }
}

/// Per-device results; exit code 4 when some failed.
fn results(res: &[OpResult], json: bool) -> Result<(), Fail> {
    if json {
        print_json(&res);
    } else {
        for r in res {
            let mark = match r.state.as_str() {
                "completed" => ui::paint("✓", ui::OK),
                "skipped" => ui::paint("–", ui::DIM),
                _ => ui::paint("✗", ui::ERR),
            };
            println!(
                "{mark} {:<20} {:<12} {}",
                cut(&r.name, 20),
                r.state,
                r.error.as_deref().unwrap_or("")
            );
        }
    }
    if res
        .iter()
        .any(|r| r.state != "completed" && r.state != "skipped")
    {
        std::process::exit(4);
    }
    Ok(())
}

fn select(f: &Fleet, t: &Target) -> Result<Vec<controller::DeviceRec>, Fail> {
    let all = t.all || !t.is_set();
    Ok(f.select(&t.devices, t.group.as_deref(), all)?)
}

/// `erebor mining start|stop|restart|status` with --device, --group or --all.
pub fn mining(dir: &Path, action: &str, t: &Target, json: bool) -> Result<(), Fail> {
    let f = open(dir)?;
    let devs = select(&f, t)?;
    if action == "status" {
        f.refresh_once();
        let ids: Vec<&str> = devs.iter().map(|d| d.id.as_str()).collect();
        let rows: Vec<DeviceRow> = f
            .rows()
            .into_iter()
            .filter(|r| ids.contains(&r.id.as_str()))
            .collect();
        if json {
            print_json(&rows);
        } else {
            table(&rows);
        }
        return Ok(());
    }
    results(&f.mining(&devs, action), json)
}

pub fn devices(dir: &Path, cmd: Option<DevicesCmd>, json: bool) -> Result<(), Fail> {
    let f = open(dir)?;
    match cmd.unwrap_or(DevicesCmd::List) {
        DevicesCmd::List => {
            f.refresh_once();
            let rows = f.rows();
            if json {
                print_json(&json!({"overview": f.overview(&rows), "devices": rows}));
            } else {
                overview(&f, &rows);
                println!();
                table(&rows);
            }
        }
        DevicesCmd::Status { target } => return mining(dir, "status", &target, json),
        DevicesCmd::Discover {
            asic: Some(cidr),
            via,
        } => {
            let found = match via {
                Some(g) => {
                    let gw = f.device(&g)?;
                    let _ = gw;
                    controller::call(&f, "fleet_scan", &json!({"cidr": cidr, "via": g}))?
                }
                None => controller::call(&f, "fleet_scan", &json!({"cidr": cidr}))?,
            };
            if json {
                print_json(&found);
            } else {
                let list = found.as_array().cloned().unwrap_or_default();
                println!("{} ASIC API(s) found in {cidr}.", list.len());
                for a in list {
                    println!(
                        "  {}:{}   add: erebor devices add {} --asic",
                        a["host"].as_str().unwrap_or(""),
                        a["port"],
                        a["host"].as_str().unwrap_or("")
                    );
                }
            }
        }
        DevicesCmd::Discover { asic: None, .. } => {
            let found = controller::discover(&f)?;
            if json {
                print_json(&found);
            } else {
                let list = found.as_array().cloned().unwrap_or_default();
                if list.is_empty() {
                    println!("No agent answered. Agents on other networks (VPN) don't answer broadcasts: add them by address.");
                }
                for x in list {
                    let a = &x["found"];
                    println!(
                        "{:<22} {:<20} v{:<8} {}",
                        a["address"].as_str().unwrap_or(""),
                        cut(a["name"].as_str().unwrap_or(""), 20),
                        a["version"].as_str().unwrap_or(""),
                        if x["paired"] == true {
                            "paired"
                        } else if a["pairing"] == true {
                            "waiting for a code"
                        } else {
                            ""
                        }
                    );
                }
            }
        }
        DevicesCmd::Add {
            address,
            code,
            kind,
            asic: false,
            ..
        } => {
            let d = f.add_agent(
                &address,
                code.as_deref().unwrap_or(""),
                kind.map(Into::into),
            )?;
            if json {
                print_json(&d);
            } else {
                println!("{}", ui::ok(format!("Paired: {} ({})", d.name, d.address)));
                println!(
                    "Fingerprint {} pinned. Mining on it keeps going when this computer is off.",
                    erebor_core::remote::short_fp(d.fingerprint.as_deref().unwrap_or(""))
                );
            }
        }
        DevicesCmd::Add {
            address,
            port,
            name,
            via,
            asic: true,
            ..
        } => {
            let d = f.add_asic(
                &address,
                port.unwrap_or(0),
                name.as_deref().unwrap_or(""),
                via.as_deref(),
            )?;
            if json {
                print_json(&d);
            } else {
                println!(
                    "{}",
                    ui::ok(format!(
                        "ASIC added: {} ({}){}",
                        d.name,
                        d.address,
                        via.map(|g| format!(" via {g}")).unwrap_or_default()
                    ))
                );
            }
        }
        DevicesCmd::Remove { device } => {
            f.remove(&device)?;
            if json {
                print_json(&json!({"removed": device}));
            } else {
                println!("{}", ui::ok(format!("Removed {device}.")));
            }
        }
        DevicesCmd::Inspect { device } => {
            f.refresh_once();
            let d = f.device(&device)?;
            let row = f.rows().into_iter().find(|r| r.id == d.id);
            let detail = if d.kind == DeviceKind::Asic {
                json!({"asic": f.query(&d.id, "asic", &Value::Null).ok()})
            } else {
                json!({"config": f.config(&d.id).ok(), "telemetry": f.query(&d.id, "telemetry", &Value::Null).ok()})
            };
            print_json(&json!({"device": d, "state": row, "detail": detail}));
        }
        DevicesCmd::Logs { device } => {
            let v = f.query(&device, "logs", &Value::Null)?;
            if json {
                print_json(&v);
            } else {
                for l in v.as_array().into_iter().flatten() {
                    println!("{}", l.as_str().unwrap_or(""));
                }
            }
        }
        DevicesCmd::Rename { device, name } => {
            f.rename(&device, &name)?;
            println!("{}", ui::ok(format!("Renamed to {name}.")));
        }
        DevicesCmd::Tag {
            device,
            tags,
            remove,
        } => {
            let ids: Vec<String> = f
                .select(&device, None, false)?
                .into_iter()
                .map(|d| d.id)
                .collect();
            f.tag(&ids, &tags, !remove)?;
            println!("{}", ui::ok("Tags updated."));
        }
        DevicesCmd::Export { secrets } => print_json(&f.export(secrets)),
        DevicesCmd::Import { file } => {
            let text = std::fs::read_to_string(&file).map_err(|e| Fail::from(e.to_string()))?;
            let v: Value =
                serde_json::from_str(&text).map_err(|_| Fail::from("not_an_export".to_string()))?;
            let r = f.import(&v)?;
            if json {
                print_json(&r);
            } else {
                println!("{}", ui::ok(format!("Imported {} item(s).", r["added"])));
            }
        }
        DevicesCmd::Events { limit } => {
            let ev = f.events(0, limit);
            if json {
                print_json(&ev);
            } else {
                for e in ev {
                    let when = chrono_time(e.ts);
                    println!(
                        "{when}  {:<7} {:<20} {} {}",
                        e.level,
                        cut(e.device_name.as_deref().unwrap_or("—"), 20),
                        e.code,
                        e.detail.unwrap_or_default()
                    );
                }
            }
        }
    }
    Ok(())
}

fn chrono_time(ts: i64) -> String {
    let age = erebor_core::now_unix() - ts;
    if age < 60 {
        format!("{age:>3}s ago")
    } else if age < 3600 {
        format!("{:>3}m ago", age / 60)
    } else if age < 86_400 {
        format!("{:>3}h ago", age / 3600)
    } else {
        format!("{:>3}d ago", age / 86_400)
    }
}

pub fn groups(dir: &Path, cmd: Option<GroupsCmd>, json: bool) -> Result<(), Fail> {
    let f = open(dir)?;
    match cmd.unwrap_or(GroupsCmd::List) {
        GroupsCmd::List => {
            let devs = f.devices();
            let list: Vec<Value> = f
                .groups()
                .into_iter()
                .map(|g| json!({"id": g.id, "name": g.name, "energyPrice": g.energy_price, "devices": devs.iter().filter(|d| d.group == Some(g.id)).count()}))
                .collect();
            if json {
                print_json(&list);
            } else if list.is_empty() {
                println!("No groups yet: erebor groups create \"GPU rigs\"");
            } else {
                for g in list {
                    println!(
                        "{:<24} {} device(s){}",
                        g["name"].as_str().unwrap_or(""),
                        g["devices"],
                        g["energyPrice"]
                            .as_f64()
                            .map(|p| format!(" · {p}/kWh"))
                            .unwrap_or_default()
                    );
                }
            }
        }
        GroupsCmd::Create { name, energy_price } => {
            let g = f.save_group(&Group {
                id: 0,
                name,
                energy_price,
            })?;
            if json {
                print_json(&g);
            } else {
                println!("{}", ui::ok(format!("Group {} created.", g.name)));
            }
        }
        GroupsCmd::Delete { name } => {
            f.delete_group(f.group_id(&name)?)?;
            println!("{}", ui::ok("Group deleted; its devices stay, ungrouped."));
        }
        GroupsCmd::Assign { group, devices } => {
            let gid = f.group_id(&group)?;
            let ids: Vec<String> = f
                .select(&devices, None, false)?
                .into_iter()
                .map(|d| d.id)
                .collect();
            let n = f.assign_group(&ids, Some(gid))?;
            println!("{}", ui::ok(format!("{n} device(s) in {group}.")));
        }
    }
    Ok(())
}

pub fn profiles(dir: &Path, cmd: Option<ProfilesCmd>, json: bool) -> Result<(), Fail> {
    let f = open(dir)?;
    match cmd.unwrap_or(ProfilesCmd::List) {
        ProfilesCmd::List => {
            let list = f.profiles();
            if json {
                print_json(&list);
            } else if list.is_empty() {
                println!("No profiles yet: erebor profiles create --help");
            } else {
                for p in list {
                    let what = [
                        (!p.config.as_object().is_none_or(|m| m.is_empty())).then_some("computers"),
                        p.asic.is_some().then_some("ASICs"),
                    ]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" + ");
                    println!(
                        "{:<24} {}{}",
                        p.name,
                        what,
                        if p.start { " · starts mining" } else { "" }
                    );
                }
            }
        }
        ProfilesCmd::Show { name } => print_json(&f.profile(&name)?),
        ProfilesCmd::Create {
            name,
            coin,
            wallet,
            worker,
            pool,
            preset,
            max_temp,
            auto_restart,
            config,
            asic_url,
            asic_user,
            asic_password,
            algo,
            start,
        } => {
            let mut c = match config {
                Some(s) => {
                    serde_json::from_str(&s).map_err(|_| Fail::from("invalid_json".to_string()))?
                }
                None => json!({}),
            };
            let mut set = |path: &[&str], v: Value| {
                let mut cur = &mut c;
                for k in &path[..path.len() - 1] {
                    cur = cur
                        .as_object_mut()
                        .unwrap()
                        .entry(k.to_string())
                        .or_insert(json!({}));
                }
                cur[path[path.len() - 1]] = v;
            };
            if let Some(x) = coin {
                set(&["coin"], json!(x.to_ascii_uppercase()));
            }
            if let Some(x) = wallet {
                set(&["wallet"], json!(x));
            }
            if let Some(x) = worker {
                set(&["pool", "worker"], json!(x));
            }
            if let Some(id) = pool {
                let p = erebor_core::pools::find(&id)
                    .ok_or_else(|| Fail::from("unknown_pool".to_string()))?;
                set(&["pool", "id"], json!(p.id));
                set(&["pool", "host"], json!(p.host));
                set(&["pool", "port"], json!(p.port));
                set(&["pool", "tls"], json!(p.tls));
            }
            if let Some(x) = preset {
                set(&["performance", "preset"], json!(x));
            }
            if let Some(x) = max_temp {
                set(&["safety", "maxTempC"], json!(x));
            }
            if let Some(x) = auto_restart {
                set(&["safety", "autoRestart"], json!(x));
            }
            let asic = match (asic_url, asic_user) {
                (Some(url), Some(user)) => Some(AsicPoolSpec {
                    url,
                    user,
                    password: asic_password,
                }),
                (None, None) => None,
                _ => return Err(Fail::from("asic_url_and_user_together".to_string())),
            };
            let p = f.save_profile(&Profile {
                id: 0,
                name,
                config: c,
                asic,
                algo,
                start,
            })?;
            if json {
                print_json(&p);
            } else {
                println!(
                    "{}",
                    ui::ok(format!(
                        "Profile {} saved. Apply it: erebor profiles apply \"{}\" --group <GROUP>",
                        p.name, p.name
                    ))
                );
            }
        }
        ProfilesCmd::Apply { name, target } => {
            let p = f.profile(&name)?;
            if !target.is_set() {
                return Err(Fail::from(
                    "choose_devices: --device, --group or --all".to_string(),
                ));
            }
            f.refresh_once();
            let devs = select(&f, &target)?;
            return results(&f.apply(&p, &devs), json);
        }
        ProfilesCmd::Delete { name } => {
            f.delete_profile(f.profile(&name)?.id)?;
            println!("{}", ui::ok("Profile deleted."));
        }
    }
    Ok(())
}

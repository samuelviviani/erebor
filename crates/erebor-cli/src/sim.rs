//! `erebor sim`: the simulated infrastructure of `erebor_core::sim` in this process, to
//! try the multi-device features without hardware. Nothing here mines.

use crate::{print_json, ui, Fail};
use anstream::println;
use erebor_core::remote;
use serde_json::json;
use std::path::Path;

pub fn run(dir: &Path, pair: bool, keep: bool, base_port: u16, json: bool) -> Result<(), Fail> {
    let root = std::env::temp_dir().join(format!("erebor-sim-{}", std::process::id()));
    let mut lab = erebor_core::sim::start(&root, Some(base_port)).map_err(Fail::from)?;
    let mut added = vec![];
    if pair {
        added = lab.pair_into(&*crate::fleet::open(dir)?)?;
    }
    if json {
        print_json(&json!({
            "agents": lab.agents.iter().map(|a| json!({"name": a.name, "address": a.addr, "code": (!pair).then(|| remote::format_code(&a.code))})).collect::<Vec<_>>(),
            "asics": lab.asics.iter().map(|a| json!({"name": a.name, "address": format!("127.0.0.1:{}", a.sim.port), "behindGateway": a.behind_gateway})).collect::<Vec<_>>(),
            "paired": pair,
        }));
    } else {
        println!(
            "{}",
            ui::warn("Simulated infrastructure: demo miners and ASIC simulators, nothing mines.")
        );
        for a in &lab.agents {
            let code = if pair {
                String::new()
            } else {
                format!("code {}", remote::format_code(&a.code))
            };
            println!("  agent {:<12} {:<18} {code}", a.name, a.addr);
        }
        for a in &lab.asics {
            let via = if a.behind_gateway {
                "  (add it through sim-gateway)"
            } else {
                ""
            };
            println!("  ASIC  {:<12} 127.0.0.1:{}{via}", a.name, a.sim.port);
        }
        if pair {
            println!("Paired into this computer's Erebor: open the app (Devices) or run `erebor devices`.");
        } else {
            println!(
                "Pair one: erebor devices add <address> --code <code>   (or rerun with --pair)"
            );
        }
        println!("{}", ui::paint("Ctrl-C stops the simulation.", ui::DIM));
    }

    let (tx, rx) = std::sync::mpsc::channel();
    ctrlc::set_handler(move || {
        let _ = tx.send(());
    })
    .map_err(|e| Fail::from(e.to_string()))?;
    let _ = rx.recv();
    if pair && !keep {
        let f = crate::fleet::open(dir)?;
        for id in added.iter().rev() {
            let _ = f.remove(id);
        }
    }
    drop(lab);
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

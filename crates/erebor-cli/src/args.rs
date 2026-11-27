//! The command line, as clap parses it: every command and option.

use super::*;

#[derive(Parser, Debug)]
#[command(
    name = "erebor",
    version,
    about = "Erebor mining manager — terminal interface",
    styles = ui::HELP,
    after_help = ui::after_help(),
)]
pub(crate) struct Cli {
    /// Data folder shared with the desktop app (default: the app's data folder).
    #[arg(long, global = true, env = "EREBOR_DATA_DIR", value_name = "DIR")]
    pub(crate) data_dir: Option<PathBuf>,
    /// Machine-readable output (stable, documented in docs/CLI.md).
    #[arg(long, global = true)]
    pub(crate) json: bool,
    /// Without a command: the guided setup on first run, the live dashboard afterwards.
    #[command(subcommand)]
    pub(crate) cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Cmd {
    /// Start mining. Erebor keeps running in the background after this terminal closes.
    Start,
    /// Stop mining.
    Stop,
    /// Mining state, hashrate, pool, shares, temperature, power.
    Status,
    /// Live dashboard: hashrate, earnings, temperatures, controls.
    #[command(visible_alias = "dashboard")]
    Monitor,
    /// How much of the CPU mining uses: eco (25%), balanced (50%, default) or max (100%).
    Power { level: Option<onboard::Perf> },
    /// Payout wallet (public address only).
    Wallet {
        #[command(subcommand)]
        cmd: WalletCmd,
    },
    /// Mine always: start Erebor and mining with the computer (on), or stop that (off).
    Service { state: Option<OnOff> },
    /// Install the latest signed release of this program.
    Update,
    /// Check OS, hardware, miners, GPU, wallet, pool and agent.
    Doctor {
        /// Skip the pool connectivity test.
        #[arg(long)]
        offline: bool,
    },
    /// Guided setup: every option, with defaults; reviewed before anything changes.
    Onboard(onboard::Opts),
    /// Every setting, by key: `config`, `config get <key>`, `config set <key> <value>`.
    Config {
        #[command(subcommand)]
        cmd: Option<ConfigCmd>,
    },
    /// Coins this computer can mine, with hashrate and estimated earnings.
    Coins {
        #[command(subcommand)]
        cmd: Option<CoinsCmd>,
    },
    /// Mining history as CSV on stdout (time, coin, hashrate, estimated amounts, energy and
    /// its cost): `erebor export --days 365 > erebor.csv`.
    Export {
        /// How many days back (default 30).
        #[arg(long, default_value_t = 30)]
        days: u32,
    },
    /// The engine log (the last lines kept in memory).
    Logs {
        /// Keep printing new lines.
        #[arg(short, long)]
        follow: bool,
    },
    /// Start, stop, pause, resume or restart mining.
    Mining {
        #[command(subcommand)]
        action: MiningCmd,
    },
    /// Detected CPU, GPUs and sensors.
    Hardware {
        #[command(subcommand)]
        cmd: HardwareCmd,
    },
    /// Mining pools.
    Pools {
        #[command(subcommand)]
        cmd: PoolsCmd,
    },
    /// The background process that owns mining.
    Agent {
        #[command(subcommand)]
        cmd: AgentCmd,
    },
    /// Demo mode: a simulated miner, no CPU/GPU mining, no network.
    Demo { state: OnOff },
    /// Miner programs (pinned releases, SHA-256 verified).
    Miner {
        #[command(subcommand)]
        cmd: MinerCmd,
    },
    /// This computer's name and id.
    Device {
        #[command(subcommand)]
        cmd: DeviceCmd,
    },
    /// Other computers, servers, rigs and ASICs managed from here (pairing, state,
    /// logs, export). Without a subcommand: the list.
    Devices {
        #[command(subcommand)]
        cmd: Option<fleet::DevicesCmd>,
    },
    /// Device groups ("GPU rigs", "ASIC farm", …).
    Groups {
        #[command(subcommand)]
        cmd: Option<fleet::GroupsCmd>,
    },
    /// Reusable mining setups applied to many devices at once.
    Profiles {
        #[command(subcommand)]
        cmd: Option<fleet::ProfilesCmd>,
    },
    /// Run a simulated infrastructure (demo agents, ASIC simulators) to try device
    /// management without hardware. Nothing mines. Ctrl-C stops it.
    Sim {
        /// Pair everything into this computer's device list (removed again on exit).
        #[arg(long)]
        pair: bool,
        /// Keep the simulated devices in the list after exit.
        #[arg(long)]
        keep: bool,
        /// First of the local ports used (agents +1…, ASICs +101…).
        #[arg(long, default_value_t = 17420)]
        base_port: u16,
    },
}

#[derive(Subcommand, Debug)]
pub(crate) enum ConfigCmd {
    /// All settings as `key = value` (the default).
    Show,
    /// One setting (a section prints all its keys).
    Get { key: String },
    /// Change one setting; the engine validates it. VALUE is JSON (`true`, `0.25`,
    /// `null`, `[..]`) or plain text.
    Set { key: String, value: String },
}

#[derive(Subcommand, Debug)]
pub(crate) enum CoinsCmd {
    /// Every coin, its algorithm, hashrate here and estimated earnings (the default).
    List,
    /// Mine this coin (`auto`: the most profitable, switching by itself).
    Set { coin: String },
}

#[derive(Subcommand, Debug, Clone)]
pub(crate) enum MiningCmd {
    /// Start mining (this computer, or --device/--group/--all).
    Start(#[command(flatten)] fleet::Target),
    /// Stop mining.
    Stop(#[command(flatten)] fleet::Target),
    /// Pause mining (the miner stays loaded; resume is instant). This computer only.
    Pause,
    /// Resume paused mining. This computer only.
    Resume,
    /// Stop and start again.
    Restart(#[command(flatten)] fleet::Target),
    /// Mining state of this computer, or of --device/--group/--all.
    Status(#[command(flatten)] fleet::Target),
}

#[derive(Subcommand, Debug)]
pub(crate) enum HardwareCmd {
    /// CPU and GPUs.
    List,
    /// Everything detected, live sensors and capabilities.
    Info,
}

#[derive(Subcommand, Debug)]
pub(crate) enum WalletCmd {
    /// The payout address and per-coin wallets.
    Show,
    /// Set the payout address (validated), or a coin's own wallet with --coin.
    Set {
        address: String,
        #[arg(long, value_name = "COIN")]
        coin: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub(crate) enum PoolsCmd {
    /// The pool catalog (* = in use).
    List,
    /// Test compatibility and connectivity of the configured pool (or all).
    Test {
        #[arg(long)]
        all: bool,
    },
    /// Use a pool from the catalog.
    Set { pool_id: String },
}

#[derive(Subcommand, Debug)]
pub(crate) enum AgentCmd {
    /// Start the agent in the background (or in this terminal with --foreground).
    Start {
        #[arg(long)]
        foreground: bool,
    },
    /// Stop mining and the agent.
    Stop,
    /// Whether the agent runs, its pid, version and uptime.
    Status,
    /// Start the agent at login (systemd user unit / LaunchAgent). Off by default.
    InstallService,
    /// Remove the login service.
    UninstallService,
    /// Accept paired controllers over the network (TLS, port 7420). Applies at next start.
    Init {
        /// Address to listen on (default 0.0.0.0:7420; a VPN address limits it to the VPN).
        #[arg(long)]
        listen: Option<String>,
        /// Don't answer LAN discovery.
        #[arg(long)]
        no_discovery: bool,
    },
    /// Show a one-time pairing code for a controller (10 minutes).
    Pair,
    /// Controllers allowed to manage this computer.
    Controllers,
    /// Remove a controller's access (name or fingerprint).
    Revoke { who: String },
    /// Stop accepting remote controllers (at next start).
    Disable,
}

#[derive(Subcommand, Debug)]
pub(crate) enum MinerCmd {
    /// Download and install the pinned miner release.
    Install {
        #[arg(value_enum, default_value = "xmrig")]
        kind: Kind,
    },
}

#[derive(ValueEnum, Debug, Clone, Copy)]
pub(crate) enum Kind {
    Xmrig,
    Srbminer,
    Lolminer,
    Cpuminer,
}

#[derive(Subcommand, Debug)]
pub(crate) enum DeviceCmd {
    /// Name, id, OS and CPU of this device.
    Show,
    /// Set this computer's name (default: the hostname).
    Name { name: String },
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq)]
pub(crate) enum OnOff {
    On,
    Off,
}

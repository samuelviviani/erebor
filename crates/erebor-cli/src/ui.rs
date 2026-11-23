//! Terminal colors. Every module prints through `anstream`, which drops the colors when
//! the output is not a terminal or `NO_COLOR` is set: scripts and `--json` stay plain.

use anstyle::{AnsiColor, Style};
use std::fmt::Display;

pub const BRAND: Style = AnsiColor::Yellow.on_default().bold();
pub const OK: Style = AnsiColor::Green.on_default();
pub const WARN: Style = AnsiColor::Yellow.on_default();
pub const ERR: Style = AnsiColor::Red.on_default().bold();
pub const DIM: Style = Style::new().dimmed();
/// Commands the user can type.
pub const CMD: Style = AnsiColor::Cyan.on_default();

pub fn paint(s: impl Display, style: Style) -> String {
    format!("{style}{s}{style:#}")
}

/// `✓ msg` in green.
pub fn ok(msg: impl Display) -> String {
    paint(format!("✓ {msg}"), OK)
}

/// `! msg` in yellow.
pub fn warn(msg: impl Display) -> String {
    paint(format!("! {msg}"), WARN)
}

/// A label padded to `width`, dimmed (padding first: escape codes have no width).
pub fn label(s: &str, width: usize) -> String {
    paint(format!("{s:<width$}"), DIM)
}

/// Color for a mining state, as in the dashboard.
pub fn state(state: &str) -> Style {
    match state {
        "running" => OK.bold(),
        "paused" => WARN.bold(),
        "error" => ERR,
        _ => DIM,
    }
}

/// `clap` help colors: gold headings, cyan commands.
pub const HELP: clap::builder::Styles = clap::builder::Styles::styled()
    .header(BRAND)
    .usage(BRAND)
    .literal(AnsiColor::Cyan.on_default().bold())
    .placeholder(CMD)
    .error(ERR)
    .valid(OK)
    .invalid(WARN);

/// The text after `erebor --help`: quick start and the less common commands.
pub fn after_help() -> String {
    let h = |s: &str| paint(s, BRAND);
    let c = |cmd: &str, what: &str| format!("  {}  {what}\n", paint(format!("{cmd:<19}"), CMD));
    let mut s = h("Quick start:\n");
    for (cmd, what) in [
        ("erebor", "live dashboard (first time: guided setup)"),
        (
            "erebor start",
            "start mining (keeps running in the background)",
        ),
        ("erebor stop", "stop mining"),
        (
            "erebor power max",
            "use the whole CPU (eco · balanced · max)",
        ),
        (
            "erebor service on",
            "mine always, also after the computer restarts",
        ),
        ("erebor demo on", "try it with a simulated miner first"),
    ] {
        s += &c(cmd, what);
    }
    s += &format!("\n{}\n", h("More commands:"));
    for (cmd, what) in [
        ("mining", "pause, resume, restart"),
        ("miner install", "install XMRig (CPU) or SRBMiner (GPU)"),
        ("pools", "list, test or change the pool"),
        ("hardware", "detected CPU, GPUs and sensors"),
        ("agent", "the background process (start, stop, status)"),
        ("demo", "simulated miner on/off"),
        ("device", "this computer's name and id"),
    ] {
        s += &c(cmd, what);
    }
    s += &format!(
        "\n{}\n{}",
        paint(
            "Details on any command: erebor help <command> · docs/CLI.md",
            DIM
        ),
        paint(
            "Exit codes: 0 ok, 1 command failed, 2 usage error, 3 Erebor not running.",
            DIM
        )
    );
    s
}

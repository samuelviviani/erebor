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

/// The text after `erebor --help`: examples, then exit codes.
pub fn after_help() -> String {
    let mut s = paint("Examples:\n", BRAND);
    for (cmd, what) in [
        ("erebor", "dashboard (first time: guided setup)"),
        (
            "erebor coins set XMR",
            "mine one coin (auto: the most profitable)",
        ),
        ("erebor config set performance.preset custom", ""),
        (
            "erebor config set performance.customThreadsPercent 75",
            "custom share of CPU threads",
        ),
        (
            "erebor config set energy.pricePerKwh 0.25",
            "net earnings, stop if unprofitable",
        ),
        (
            "erebor config set pool.worker rig01",
            "worker name on the pool",
        ),
        ("erebor service on", "mine always, also after a restart"),
        ("erebor logs -f", "follow the engine log"),
        ("erebor status --json", "for scripts and monitoring"),
    ] {
        s += &format!("  {}", paint(format!("{cmd:<55}"), CMD));
        if !what.is_empty() {
            s += &paint(what, DIM);
        }
        s += "\n";
    }
    s += &format!(
        "\n{}\n{}",
        paint(
            "Every setting: erebor config · details: erebor help <command> · docs/CLI.md",
            DIM
        ),
        paint(
            "Exit codes: 0 ok, 1 command failed, 2 usage error, 3 Erebor not running.",
            DIM
        )
    );
    s
}

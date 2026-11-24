//! Erebor terminal dashboard. Reads and controls the engine through the local socket
//! (`erebor_core::ipc`): the same state the desktop app and the CLI see.
//!
//! Keys: S start · P pause/resume · X stop · R restart · ↑↓/PgUp/PgDn log · ? help ·
//! Q/Esc quit. Quitting never stops mining; stopping is always an explicit X.

use erebor_core::config::AppConfig;
use erebor_core::fleet::Snapshot;
use erebor_core::ipc::{self, Action};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, Clear, Paragraph, Row, Table, Wrap};
use ratatui::Frame;
use serde_json::Value;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const REFRESH: Duration = Duration::from_secs(1);
const BG: Color = Color::Black;
const FG: Color = Color::Gray;
const DIM: Color = Color::DarkGray;
const ACCENT: Color = Color::Rgb(230, 169, 60); // Erebor gold
const OK: Color = Color::Green;
/// The logo, shown only at launch.
const SPLASH: Duration = Duration::from_millis(1200);

/// The Erebor mark: a mountain with a mine entrance and circuit traces.
const MOUNTAIN: [&str; 7] = [
    r"             /\",
    r"            /::\",
    r"    /\     /::o:\      /\",
    r"   /::\   /:::|::\    /::\",
    r"  /::::\ /:o--':::\  /:o::\",
    r" /:o::::/:::.--.:::\/::|:::\",
    r"/::|::::::::|  |:::::::|::::\",
];
const WORDMARK: [&str; 5] = [
    "███████ ██████  ███████ ██████   ██████  ██████",
    "██      ██   ██ ██      ██   ██ ██    ██ ██   ██",
    "█████   ██████  █████   ██████  ██    ██ ██████",
    "██      ██   ██ ██      ██   ██ ██    ██ ██   ██",
    "███████ ██   ██ ███████ ██████   ██████  ██   ██",
];
const TAGLINE: &str = "Mining, under your control.";

/// Value formatting and error explanations shared with the CLI.
pub mod fmt {
    pub const NA: &str = "n/a";

    pub fn hashrate(h: Option<f64>) -> String {
        let Some(h) = h else { return NA.into() };
        let units = ["H/s", "kH/s", "MH/s", "GH/s", "TH/s"];
        let mut v = h;
        let mut i = 0;
        while v >= 1000.0 && i < units.len() - 1 {
            v /= 1000.0;
            i += 1;
        }
        format!("{v:.2} {}", units[i])
    }

    pub fn duration(secs: u64) -> String {
        let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
        if h > 0 {
            format!("{h}h {m:02}m {s:02}s")
        } else {
            format!("{m}m {s:02}s")
        }
    }

    pub fn temp(t: Option<f32>) -> String {
        t.map_or(NA.into(), |t| format!("{t:.0} °C"))
    }

    pub fn power(w: Option<f64>, source: &str) -> String {
        match w {
            Some(w) if source == "estimated" => format!("{w:.0} W (estimated)"),
            Some(w) => format!("{w:.0} W"),
            None => NA.into(),
        }
    }

    /// Estimated earnings over `days` days, after fees.
    pub fn earnings_over(s: &erebor_core::fleet::Snapshot, days: f64) -> Option<String> {
        let e = s.earnings.as_ref()?;
        Some(match e.value_per_day {
            Some(v) => format!("{:.2} {}", v * days, e.currency),
            None => format!("{:.8} BTC", e.btc_per_day * days),
        })
    }

    pub fn earnings(s: &erebor_core::fleet::Snapshot) -> String {
        earnings_over(s, 1.0).map_or(NA.into(), |v| format!("{v} / day (estimate)"))
    }

    /// "balanced · 50% of threads".
    pub fn performance(p: &erebor_core::config::Performance) -> String {
        use erebor_core::config::Preset;
        let name = match p.preset {
            Preset::Eco => "eco",
            Preset::Balanced => "balanced",
            Preset::Performance => "max",
            Preset::Custom => "custom",
        };
        format!("{name} · {}% of threads", p.limits().0)
    }

    pub fn state(state: &str) -> &'static str {
        match state {
            "running" => "Running",
            "paused" => "Paused",
            "error" => "Error",
            _ => "Stopped",
        }
    }

    /// Message and hint for the error codes users meet most.
    pub fn explain(code: &str) -> (String, Option<&'static str>) {
        let (m, h) = match code {
            "agent_unreachable" => (
                "Erebor is not running in the background",
                Some("start mining: erebor start · live dashboard: erebor"),
            ),
            "wallet_missing" => (
                "no payout wallet configured",
                Some("erebor wallet set <KRYPTEX_ID>   (or try it first: erebor demo on)"),
            ),
            "miner_not_configured" => (
                "the miner is not installed",
                Some("erebor miner install xmrig"),
            ),
            "miner_binary_changed" => (
                "the miner binary changed since it was installed",
                Some("reinstall it: erebor miner install"),
            ),
            "job_running" => (
                "an install or benchmark is in progress",
                Some("wait for it to finish (erebor status)"),
            ),
            "not_running" => ("mining is not running", Some("erebor start")),
            "not_paused" => ("mining is not paused", None),
            "pause_unsupported" => ("this miner cannot pause", Some("use erebor stop")),
            "stop_mining_first" => ("stop mining first", Some("erebor stop")),
            "kryptex_pool_required" => (
                "a Kryptex ID only works on the Kryptex pool",
                Some("erebor pools set kryptex"),
            ),
            "protocol_version" => (
                "this command and the background Erebor are different versions",
                Some("restart it: erebor agent stop, then run the command again"),
            ),
            "not_agent" => (
                "mining belongs to the Erebor desktop app or node, not to the background agent",
                Some("close the desktop app (it stops mining) or stop the node"),
            ),
            "ipc_unsupported_platform" => (
                "the terminal interface is not available on this OS yet",
                Some("use the desktop app"),
            ),
            "agent_untrusted" => (
                "the agent socket belongs to another user: refused",
                Some("check who started it (erebor agent status) and the data folder owner"),
            ),
            "pool_check_failed" => (
                "the configured pool is not reachable or not compatible with the wallet",
                Some("check network/DNS/firewall, or pick another: erebor pools set <id>"),
            ),
            "onboarding_cancelled" => (
                "setup cancelled: nothing was changed",
                Some("run erebor onboard again, or try erebor onboard --mode demo"),
            ),
            "onboarding_needs_terminal" => (
                "the guided setup needs an interactive terminal",
                Some("in scripts use: erebor onboard --non-interactive --wallet <ADDRESS>"),
            ),
            "invalid_address" => (
                "not a valid payout address (public Monero address, Kryptex ID or Bitcoin address)",
                None,
            ),
            "state_unchanged" => (
                "the miner did not change state",
                Some("check erebor status and the agent log"),
            ),
            "unknown_pool" => ("no pool with that id", Some("see: erebor pools list")),
            "unknown_coin" => ("no coin with that id", Some("see: erebor coins")),
            "not_in_demo" => ("not available in demo mode", None),
            "no_gpu" => ("needs a supported GPU", None),
            "platform_unsupported" => ("no miner build for this system", None),
            "kryptex_or_btc_required" => ("needs a Kryptex ID payout", None),
            "not_on_unmineable" => ("not minable with this payout", None),
            "invalid_worker" => (
                "worker name: letters, digits, '_' and '-' only, max 32 (EREBOR is reserved)",
                None,
            ),
            _ if code.starts_with("unknown_key") => (
                "no setting with that key",
                Some("see every key: erebor config"),
            ),
            "update_bad_signature" => (
                "the downloaded update is not signed by the Erebor project: not installed",
                Some("nothing was changed; try again later or report it"),
            ),
            _ if code.starts_with("update_check_failed") => (
                "could not check for updates (offline, or no update published yet)",
                Some("try again later: erebor update"),
            ),
            _ if code.starts_with("update_write") => (
                "cannot replace this program (no permission on its folder?)",
                Some("reinstall: sh scripts/install.sh, or run the update as its owner"),
            ),
            "invalid_device_name" => (
                "device name must be 1-64 characters, no control characters",
                None,
            ),
            _ if code.starts_with("engine_busy") => (
                "another Erebor (agent or desktop app) already owns this data folder",
                Some("the CLI talks to it directly: erebor status"),
            ),
            _ => return (code.to_string(), None),
        };
        (m.to_string(), h)
    }
}

pub struct App {
    dir: PathBuf,
    snap: Option<Snapshot>,
    cfg: Option<AppConfig>,
    /// Why the agent could not be read (error code).
    unreachable: Option<String>,
    logs: Vec<String>,
    /// Lines scrolled up from the newest log line.
    scroll: usize,
    msg: Option<(String, bool)>,
    help: bool,
}

enum Flow {
    Continue,
    Quit,
}

impl App {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            snap: None,
            cfg: None,
            unreachable: None,
            logs: vec![],
            scroll: 0,
            msg: None,
            help: false,
        }
    }

    fn refresh(&mut self) {
        match ipc::snapshot(&self.dir) {
            Ok(s) => {
                self.snap = Some(s);
                self.unreachable = None;
                if let Ok(Value::Array(l)) = ipc::call(&self.dir, "get_logs", &Value::Null) {
                    self.logs = l
                        .into_iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect();
                }
                self.cfg = ipc::call(&self.dir, "get_config", &Value::Null)
                    .ok()
                    .and_then(|v| serde_json::from_value(v).ok());
            }
            Err(e) => {
                self.snap = None;
                self.unreachable = Some(e.as_str().unwrap_or("agent_error").to_string());
            }
        }
    }

    fn act(&mut self, a: Action) {
        let (done, already, verb) = match a {
            Action::Start => ("Mining started", "running", "start"),
            Action::Stop => ("Mining stopped", "stopped", "stop"),
            Action::Pause => ("Mining paused", "paused", "pause"),
            Action::Resume => ("Mining resumed", "running", "resume"),
            Action::Restart => ("Mining restarted", "running", "restart"),
        };
        self.msg = Some(match ipc::control(&self.dir, a) {
            Ok(o) if o.changed => (done.to_string(), false),
            Ok(_) => (format!("Mining is already {already}"), false),
            Err(e) => {
                let (m, h) = fmt::explain(e.as_str().unwrap_or("error"));
                let h = h.map(|h| format!(" → {h}")).unwrap_or_default();
                (format!("Could not {verb}: {m}{h}"), true)
            }
        });
        self.refresh();
    }

    fn on_key(&mut self, code: KeyCode, mods: KeyModifiers) -> Flow {
        if self.help && code != KeyCode::Char('q') {
            self.help = false;
            return Flow::Continue;
        }
        match code {
            KeyCode::Char('q' | 'Q') | KeyCode::Esc => return Flow::Quit,
            KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => return Flow::Quit,
            KeyCode::Char('s' | 'S') => self.act(Action::Start),
            KeyCode::Char('x' | 'X') => self.act(Action::Stop),
            KeyCode::Char('r' | 'R') => self.act(Action::Restart),
            KeyCode::Char('p' | 'P') => {
                let paused = self.snap.as_ref().is_some_and(|s| s.state == "paused");
                self.act(if paused {
                    Action::Resume
                } else {
                    Action::Pause
                })
            }
            KeyCode::Char('?' | 'h') => self.help = true,
            KeyCode::Up => self.scroll = self.scroll.saturating_add(1),
            KeyCode::Down => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_add(10),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_sub(10),
            KeyCode::Home => self.scroll = self.logs.len(),
            KeyCode::End => self.scroll = 0,
            _ => {}
        }
        Flow::Continue
    }
}

/// Open the dashboard; returns when the user quits (mining keeps its state).
pub fn run(dir: PathBuf) -> Result<(), String> {
    let mut term = ratatui::try_init().map_err(|e| format!("terminal: {e}"))?;
    let mut app = App::new(dir);
    let res = (|| -> std::io::Result<()> {
        // Splash: the logo while the first state loads; any key skips it.
        term.draw(draw_splash)?;
        let t = Instant::now();
        app.refresh();
        while event::poll(SPLASH.saturating_sub(t.elapsed()))? {
            if matches!(event::read()?, Event::Key(_)) {
                break;
            }
        }
        let mut last = Instant::now();
        loop {
            term.draw(|f| draw(f, &app))?;
            if event::poll(Duration::from_millis(200))? {
                // Resize events need nothing else: the next draw uses the new size.
                if let Event::Key(k) = event::read()? {
                    if k.kind == KeyEventKind::Press {
                        if let Flow::Quit = app.on_key(k.code, k.modifiers) {
                            return Ok(());
                        }
                        last = Instant::now();
                    }
                }
            }
            if last.elapsed() >= REFRESH {
                app.refresh();
                last = Instant::now();
            }
        }
    })();
    ratatui::restore();
    res.map_err(|e| e.to_string())
}

/// One logo row: the mountain's fill dim, its edges and circuits gold.
fn mountain_line(row: &str) -> Vec<Span<'static>> {
    row.chars()
        .map(|c| {
            let color = if c == ':' { DIM } else { ACCENT };
            Span::styled(c.to_string(), Style::new().fg(color))
        })
        .collect()
}

/// Launch screen: mountain above the wordmark, centered.
fn draw_splash(f: &mut Frame) {
    let area = f.area();
    f.render_widget(Block::new().style(Style::new().bg(BG).fg(FG)), area);
    let word = Style::new()
        .fg(Color::Rgb(226, 232, 240))
        .add_modifier(Modifier::BOLD);
    let mut lines: Vec<Line> = MOUNTAIN
        .iter()
        .map(|m| Line::from(mountain_line(&format!("{m:<31}"))).centered())
        .collect();
    lines.push(Line::raw(""));
    lines.extend(
        WORDMARK
            .iter()
            .map(|w| Line::styled(format!("{w:<48}"), word).centered()),
    );
    lines.push(Line::raw(""));
    lines.push(Line::styled(TAGLINE, Style::new().fg(DIM)).centered());
    let top = area.height.saturating_sub(lines.len() as u16) / 2;
    let [_, mid] = Layout::vertical([Constraint::Length(top), Constraint::Min(0)]).areas(area);
    f.render_widget(Paragraph::new(lines), mid);
}

fn dim(s: impl Into<String>) -> Span<'static> {
    Span::styled(s.into(), Style::new().fg(DIM))
}

fn plain(s: impl Into<String>, color: Color) -> Span<'static> {
    Span::styled(s.into(), Style::new().fg(color))
}

/// A section rule: `── Title ─────`.
fn rule(title: &str, width: u16) -> Line<'static> {
    let head = if title.is_empty() {
        String::new()
    } else {
        format!("── {title} ")
    };
    let rest = (width as usize).saturating_sub(head.chars().count());
    Line::styled(format!("{head}{}", "─".repeat(rest)), Style::new().fg(DIM))
}

fn kv(label: &str, value: Vec<Span<'static>>) -> Line<'static> {
    let mut v = vec![dim(format!(" {label:<10}"))];
    v.extend(value);
    Line::from(v)
}

fn state_color(state: &str) -> Color {
    match state {
        "running" => OK,
        "paused" => Color::Yellow,
        "error" => Color::Red,
        _ => DIM,
    }
}

fn temp_span(t: Option<f32>, limit: f32) -> Span<'static> {
    let color = match t {
        None => DIM,
        Some(t) if t >= limit - 5.0 => Color::Red,
        Some(t) if t >= limit - 15.0 => Color::Yellow,
        Some(_) => OK,
    };
    plain(fmt::temp(t), color)
}

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    f.render_widget(Block::new().style(Style::new().bg(BG).fg(FG)), area);
    let [header, body, notice, keys] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_header(f, header, app);
    match &app.snap {
        Some(s) => draw_body(f, body, s, app),
        None => {
            let code = app.unreachable.as_deref().unwrap_or("connecting");
            let (m, h) = fmt::explain(code);
            let lines = vec![
                rule("", area.width),
                Line::raw(""),
                Line::styled(format!(" {m}."), Style::new().fg(Color::Red)),
                Line::raw(""),
                Line::styled(
                    format!(" {}", h.unwrap_or("check: erebor doctor")),
                    Style::new().fg(FG),
                ),
                Line::styled(" Retrying every second…", Style::new().fg(DIM)),
            ];
            f.render_widget(Paragraph::new(lines), body);
        }
    }

    // Notice line: the last action's result, else what is wrong, else nothing.
    let s = app.snap.as_ref();
    let hint = |h: Option<&str>| h.map(|h| format!(" → {h}")).unwrap_or_default();
    let line = if let Some((m, err)) = &app.msg {
        Line::styled(
            format!(" {} {m}", if *err { "✗" } else { "✓" }),
            Style::new().fg(if *err { Color::Red } else { OK }),
        )
    } else if let Some(e) = s.and_then(|s| s.error.as_deref()) {
        let (m, h) = fmt::explain(e);
        Line::styled(format!(" ✗ {m}{}", hint(h)), Style::new().fg(Color::Red))
    } else if let Some(r) = s.and_then(|s| s.restart_in_secs) {
        Line::styled(
            format!(" ! Miner crashed: restarting in {r} s"),
            Style::new().fg(Color::Yellow),
        )
    } else if let Some(b) = s
        .filter(|s| !s.is_active())
        .and_then(|s| s.start_blocker.as_deref())
    {
        let (m, h) = fmt::explain(b);
        Line::styled(
            format!(" ! Cannot start: {m}{}", hint(h)),
            Style::new().fg(Color::Yellow),
        )
    } else if let Some(j) = s.and_then(|s| s.job.as_ref()).filter(|j| !j.done) {
        Line::styled(
            format!(" … {} {:.0}%", j.kind, j.progress * 100.0),
            Style::new().fg(FG),
        )
    } else {
        Line::default()
    };
    f.render_widget(Paragraph::new(line), notice);
    draw_keys(f, keys, app);

    if app.help {
        draw_help(f, area);
    }
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let mut left = vec![Span::raw(" ")];
    match &app.snap {
        Some(s) => {
            left.push(Span::styled(
                format!("● {}", fmt::state(&s.state)),
                Style::new()
                    .fg(state_color(&s.state))
                    .add_modifier(Modifier::BOLD),
            ));
            if s.is_active() {
                left.push(plain(format!("  {}", fmt::duration(s.uptime_secs)), FG));
            }
            left.push(plain(format!("   {} ({})", s.coin_name, s.coin), FG));
            left.push(dim(format!(
                " · {}{}",
                s.algo,
                if s.auto_coin { " · auto" } else { "" }
            )));
            if s.demo {
                left.push(plain("   DEMO (simulated)", Color::Yellow));
            }
        }
        None => left.push(Span::styled(
            "● Offline",
            Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
    }
    let left = Line::from(left);
    let version = format!("erebor {} ", env!("CARGO_PKG_VERSION"));
    if left.width() + version.len() + 2 <= area.width as usize {
        f.render_widget(
            Paragraph::new(Line::from(dim(version)).right_aligned()),
            area,
        );
    }
    f.render_widget(Paragraph::new(left), area);
}

fn draw_keys(f: &mut Frame, area: Rect, app: &App) {
    let state = app.snap.as_ref().map_or("offline", |s| s.state.as_str());
    let pause = app
        .snap
        .as_ref()
        .and_then(|s| s.miner.as_ref())
        .is_some_and(|m| m.supports_pause);
    let mut keys: Vec<(&str, &str)> = match state {
        "running" if pause => vec![("P", "pause"), ("X", "stop"), ("R", "restart")],
        "running" => vec![("X", "stop"), ("R", "restart")],
        "paused" => vec![("P", "resume"), ("X", "stop")],
        "offline" => vec![],
        _ => vec![("S", "start")],
    };
    keys.extend([("↑↓", "log"), ("?", "help"), ("Q", "quit")]);
    let mut spans = vec![];
    for (k, l) in keys {
        spans.push(Span::styled(
            format!(" {k}"),
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
        spans.push(dim(format!(" {l}  ")));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_body(f: &mut Frame, area: Rect, s: &Snapshot, app: &App) {
    let cfg = app.cfg.clone().unwrap_or_default();
    let active = s.is_active();

    // Mining (left) and money/limits (right).
    let total = s.accepted + s.rejected;
    let mut shares = vec![
        plain(format!("{} ✓", s.accepted), OK),
        plain(
            format!("  {} ✗", s.rejected),
            if s.rejected > 0 { Color::Yellow } else { DIM },
        ),
    ];
    if total > 0 {
        shares.push(dim(format!(
            "  {:.1}% rejected",
            s.rejected as f64 * 100.0 / total as f64
        )));
    }
    let pool = match (&s.pool, s.pool_connected) {
        (Some(p), true) => vec![plain("● ", OK), plain(p.clone(), FG)],
        (Some(p), false) => vec![plain("◌ ", Color::Yellow), plain(p.clone(), FG)],
        (None, _) => vec![plain(format!("{}:{}", cfg.pool.host, cfg.pool.port), FG)],
    };
    let worker = match cfg.pool.worker.as_str() {
        "" => erebor_core::config::default_worker(),
        w => w.to_string(),
    };
    let miner = s.miner.as_ref().map_or(fmt::NA.to_string(), |m| {
        format!("{} {}", m.name, m.version.clone().unwrap_or_default())
            .trim()
            .to_string()
    });
    let left = vec![
        kv(
            "Hashrate",
            vec![Span::styled(
                fmt::hashrate(s.hashrate.filter(|_| active)),
                Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
            )],
        ),
        kv("Shares", shares),
        kv("Pool", pool),
        kv("Worker", vec![plain(worker, FG)]),
        kv("Miner", vec![plain(miner, FG)]),
    ];

    let e = s.earnings.as_ref();
    let money = |v: f64| format!("{v:.2} {}", e.map_or("", |e| e.currency.as_str()));
    let earnings = match fmt::earnings_over(s, 1.0) {
        Some(d) => vec![
            plain(format!("{d}/day"), ACCENT),
            dim(format!(
                "  {}/month",
                fmt::earnings_over(s, 30.0).unwrap_or_default()
            )),
        ],
        None => vec![dim(fmt::NA)],
    };
    let net = match e.and_then(|e| e.net_per_day.zip(e.energy_cost_per_day)) {
        Some((n, c)) => vec![
            plain(
                format!("{}/day", money(n)),
                if n < 0.0 { Color::Red } else { OK },
            ),
            dim(format!("  power {}/day", money(c))),
        ],
        None if cfg.energy.price_per_kwh.is_none() => vec![dim("no kWh price set")],
        None => vec![dim(fmt::NA)],
    };
    let fee = if s.dev_fee.enabled {
        let now = if s.dev_fee.active == "project" {
            "now for Erebor"
        } else {
            "now for you"
        };
        format!("{}% of time · {now}", s.dev_fee.percent)
    } else {
        "off".into()
    };
    let right = vec![
        kv("Earnings", earnings),
        kv("Net", net),
        kv(
            "Power",
            vec![plain(fmt::power(s.power_w, &s.power_source), FG)],
        ),
        kv(
            "CPU use",
            vec![plain(fmt::performance(&cfg.performance), FG)],
        ),
        kv("Dev fee", vec![dim(fee)]),
    ];

    // Devices: one row each.
    let limit = cfg.safety.max_temp_c;
    let mut devices = vec![Row::new(vec![
        Cell::from(" CPU"),
        Cell::from(temp_span(s.cpu_temp_c, limit)),
        Cell::from(dim("")),
        Cell::from(dim("")),
    ])];
    for (i, g) in s.gpus.iter().enumerate() {
        devices.push(Row::new(vec![
            Cell::from(format!(" GPU {i}  {}", g.name)),
            Cell::from(temp_span(g.temp_c, limit)),
            Cell::from(plain(
                g.power_w.map_or(fmt::NA.into(), |w| format!("{w:.0} W")),
                FG,
            )),
            Cell::from(plain(
                g.util_percent
                    .map_or(fmt::NA.into(), |u| format!("{u:.0}%")),
                FG,
            )),
        ]));
    }

    let two_cols = area.width >= 76;
    let stats_h = if two_cols { 5 } else { 10 };
    let [r1, stats, _, r2, table, _, r3, logs] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(stats_h),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1 + devices.len() as u16),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(area);

    f.render_widget(Paragraph::new(rule("Mining", area.width)), r1);
    if two_cols {
        let [l, r] = Layout::horizontal([Constraint::Percentage(50); 2]).areas(stats);
        f.render_widget(Paragraph::new(left), l);
        f.render_widget(Paragraph::new(right), r);
    } else {
        f.render_widget(Paragraph::new([left, right].concat()), stats);
    }

    f.render_widget(
        Paragraph::new(rule(
            &format!("Devices · stop above {limit:.0} °C"),
            area.width,
        )),
        r2,
    );
    let head = Row::new(["", "TEMP", "POWER", "LOAD"]).style(Style::new().fg(DIM));
    f.render_widget(
        Table::new(
            devices,
            [
                Constraint::Length(30),
                Constraint::Length(8),
                Constraint::Length(8),
                Constraint::Length(6),
                Constraint::Min(0),
            ],
        )
        .header(head),
        table,
    );

    let h = logs.height as usize;
    let max_scroll = app.logs.len().saturating_sub(h);
    let end = app.logs.len() - app.scroll.min(max_scroll);
    let lines: Vec<Line> = app.logs[end.saturating_sub(h)..end]
        .iter()
        .map(|l| Line::styled(format!(" {l}"), Style::new().fg(DIM)))
        .collect();
    let title = if app.scroll > 0 { "Log ↑" } else { "Log" };
    f.render_widget(Paragraph::new(rule(title, area.width)), r3);
    f.render_widget(Paragraph::new(lines), logs);
}

fn draw_help(f: &mut Frame, area: Rect) {
    let w = area.width.min(64);
    let h = area.height.min(18);
    let r = Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    );
    let text = [
        ("S", "start mining"),
        ("P", "pause / resume (if the miner supports it)"),
        ("X", "stop mining"),
        ("R", "restart mining"),
        ("↑ ↓ PgUp PgDn", "scroll the log (Home/End: oldest/newest)"),
        ("Q / Esc", "close the dashboard: mining keeps running"),
    ];
    let mut lines: Vec<Line> = text
        .iter()
        .map(|(k, v)| {
            Line::from(vec![
                Span::styled(format!(" {k:<15}"), Style::new().fg(ACCENT)),
                Span::styled(*v, Style::new().fg(FG)),
            ])
        })
        .collect();
    lines.extend([
        Line::raw(""),
        Line::styled(" From the shell:", Style::new().fg(DIM)),
        Line::styled(" erebor config      every setting", Style::new().fg(FG)),
        Line::styled(
            " erebor coins       coins and estimates",
            Style::new().fg(FG),
        ),
        Line::styled(" erebor logs -f     follow the log", Style::new().fg(FG)),
        Line::raw(""),
        Line::styled(" Any key closes this help.", Style::new().fg(DIM)),
    ]);
    f.render_widget(Clear, r);
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .style(Style::new().bg(BG))
            .block(
                Block::bordered()
                    .border_type(ratatui::widgets::BorderType::Rounded)
                    .border_style(Style::new().fg(DIM))
                    .title(" Help "),
            ),
        r,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use erebor_core::fleet::{EarningsSummary, GpuSummary, MinerSummary};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render(app: &App, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| draw(f, app)).unwrap();
        let buf = t.backend().buffer().clone();
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
                    + "\n"
            })
            .collect()
    }

    fn app_with(s: Snapshot) -> App {
        let mut a = App::new("/nonexistent".into());
        a.snap = Some(s);
        a.logs = (0..50).map(|i| format!("log line {i}")).collect();
        a
    }

    fn running() -> Snapshot {
        Snapshot {
            schema: 1,
            state: "running".into(),
            demo: true,
            miner: Some(MinerSummary {
                name: "XMRig".into(),
                version: Some("6.26.0".into()),
                supports_pause: true,
                ..Default::default()
            }),
            coin: "XMR".into(),
            coin_name: "Monero".into(),
            algo: "rx/0".into(),
            pool: Some("xmr.kryptex.network:7029".into()),
            pool_connected: true,
            hashrate: Some(4250.0),
            uptime_secs: 3723,
            accepted: 148,
            rejected: 2,
            power_source: "unavailable".into(),
            earnings: Some(EarningsSummary {
                value_per_day: Some(0.82),
                currency: "EUR".into(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn renders_live_metrics_and_unavailable_values() {
        for (w, h) in [(80, 24), (120, 30), (60, 30)] {
            let out = render(&app_with(running()), w, h);
            for s in [
                "● Running",
                "Monero (XMR)",
                "DEMO",
                "4.25 kH/s",
                "0.82 EUR/day",
                "24.60 EUR/month",
                "1h 02m 03s",
                "XMRig 6.26.0",
                "148 ✓",
                "1.3% rejected",
                "log line 49",
                "P pause",
                "Q quit",
            ] {
                assert!(out.contains(s), "{w}x{h}: missing {s:?}\n{out}");
            }
            // No sensor: shown as unavailable, never a made-up number.
            assert!(out.contains(" CPU") && out.contains("n/a"), "{out}");
            assert!(out.contains("Power     n/a"), "{out}");
            assert!(!out.contains("S start"), "{out}");
        }
    }

    #[test]
    fn stopped_shows_start_and_explains_the_blocker() {
        let mut s = running();
        s.state = "stopped".into();
        s.start_blocker = Some("wallet_missing".into());
        s.cpu_temp_c = Some(85.0);
        s.gpus = vec![GpuSummary {
            name: "RTX".into(),
            temp_c: Some(55.0),
            power_w: Some(120.0),
            util_percent: Some(98.0),
        }];
        s.earnings.as_mut().unwrap().energy_cost_per_day = Some(0.32);
        s.earnings.as_mut().unwrap().net_per_day = Some(0.5);
        let out = render(&app_with(s), 100, 30);
        for want in [
            "● Stopped",
            "S start",
            "no payout wallet configured → erebor wallet set",
            "85 °C",
            "GPU 0  RTX",
            "120 W",
            "98%",
            "0.50 EUR/day",
            "power 0.32 EUR/day",
        ] {
            assert!(out.contains(want), "missing {want:?}\n{out}");
        }
    }

    #[test]
    fn logo_only_on_the_splash() {
        let tall = render(&app_with(running()), 100, 50);
        assert!(!tall.contains("███") && !tall.contains(TAGLINE), "{tall}");
        for (w, h) in [(80, 24), (20, 5), (1, 1)] {
            let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
            t.draw(draw_splash).unwrap();
        }
    }

    #[test]
    fn offline_tiny_and_help_screens_do_not_panic() {
        let mut a = App::new("/nonexistent".into());
        a.unreachable = Some("agent_unreachable".into());
        let out = render(&a, 80, 24);
        assert!(out.contains("erebor start"), "{out}");
        assert!(out.contains("Offline"), "{out}");
        render(&a, 10, 4);
        render(&a, 1, 1);
        let mut a = app_with(running());
        a.help = true;
        a.scroll = 1000;
        assert!(render(&a, 80, 24).contains("mining keeps running"));
        render(&a, 20, 6);
        render(&a, 1, 1);
    }

    #[test]
    fn keys_quit_without_stopping_and_scroll_logs() {
        let mut a = app_with(running());
        a.on_key(KeyCode::Up, KeyModifiers::NONE);
        a.on_key(KeyCode::PageUp, KeyModifiers::NONE);
        assert_eq!(a.scroll, 11);
        a.on_key(KeyCode::End, KeyModifiers::NONE);
        assert_eq!(a.scroll, 0);
        // Quitting sends nothing to the agent (the dir does not even exist).
        assert!(matches!(
            a.on_key(KeyCode::Char('q'), KeyModifiers::NONE),
            Flow::Quit
        ));
        assert!(a.msg.is_none());
    }

    #[test]
    fn formats() {
        assert_eq!(fmt::hashrate(Some(950.0)), "950.00 H/s");
        assert_eq!(fmt::hashrate(Some(1.5e6)), "1.50 MH/s");
        assert_eq!(fmt::hashrate(None), "n/a");
        assert_eq!(fmt::duration(59), "0m 59s");
        assert_eq!(fmt::power(Some(85.4), "measured"), "85 W");
        assert_eq!(fmt::explain("weird").0, "weird");
    }
}

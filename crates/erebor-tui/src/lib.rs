//! Erebor terminal dashboard. Reads and controls the engine through the local socket
//! (`erebor_core::ipc`): the same state the desktop app and the CLI see.
//!
//! Keys: S start · P pause/resume · X stop · R restart · ↑↓/PgUp/PgDn logs · ? help ·
//! Q/Esc quit. Quitting never stops mining; stopping is always an explicit X.

use erebor_core::fleet::Snapshot;
use erebor_core::ipc::{self, Action};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;
use serde_json::Value;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const REFRESH: Duration = Duration::from_secs(1);
const BG: Color = Color::Black;
const FG: Color = Color::Gray;
const DIM: Color = Color::DarkGray;
const ACCENT: Color = Color::Green;

/// Value formatting shared with the CLI.
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

    pub fn earnings(s: &erebor_core::fleet::Snapshot) -> String {
        match &s.earnings {
            Some(e) => match e.value_per_day {
                Some(v) => format!("{v:.2} {} / day (estimate)", e.currency),
                None => format!("{:.8} BTC / day (estimate)", e.btc_per_day),
            },
            None => NA.into(),
        }
    }

    pub fn state(state: &str) -> &'static str {
        match state {
            "running" => "Running",
            "paused" => "Paused",
            "error" => "Error",
            _ => "Stopped",
        }
    }
}

pub struct App {
    dir: PathBuf,
    snap: Option<Snapshot>,
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
            }
            Err(e) => {
                self.snap = None;
                self.unreachable = Some(e.as_str().unwrap_or("agent_error").to_string());
            }
        }
    }

    fn act(&mut self, a: Action) {
        let name = format!("{a:?}").to_lowercase();
        self.msg = Some(match ipc::control(&self.dir, a) {
            Ok(o) if o.changed => (format!("{name}: done"), false),
            Ok(o) => (format!("{name}: already {}", o.snapshot.state), false),
            Err(e) => (
                format!("{name} failed: {}", e.as_str().unwrap_or("error")),
                true,
            ),
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
    app.refresh();
    let mut last = Instant::now();
    let res = (|| -> std::io::Result<()> {
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

fn block(title: &str) -> Block<'static> {
    Block::bordered()
        .border_style(Style::new().fg(DIM))
        .title(Span::styled(format!(" {title} "), Style::new().fg(DIM)))
}

fn row(label: &str, value: String, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!(" {label:<17}"), Style::new().fg(DIM)),
        Span::styled(value, Style::new().fg(color)),
    ])
}

fn state_color(state: &str) -> Color {
    match state {
        "running" => ACCENT,
        "paused" => Color::Yellow,
        "error" => Color::Red,
        _ => DIM,
    }
}

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    f.render_widget(Block::new().style(Style::new().bg(BG).fg(FG)), area);
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(4),
    ])
    .areas(area);

    // Header: name, mode, state.
    let (label, color, demo) = match &app.snap {
        Some(s) => (fmt::state(&s.state), state_color(&s.state), s.demo),
        None => ("Agent offline", Color::Red, false),
    };
    let mut right = vec![Span::styled(
        format!("● {label} "),
        Style::new().fg(color).add_modifier(Modifier::BOLD),
    )];
    if demo {
        right.insert(0, Span::styled("DEMO  ", Style::new().fg(Color::Yellow)));
    }
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " EREBOR",
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
        )))
        .block(
            Block::bordered()
                .border_style(Style::new().fg(DIM))
                .title_top(Line::from(right).right_aligned()),
        ),
        header,
    );

    match &app.snap {
        Some(s) => draw_body(f, body, s, app),
        None => {
            let code = app.unreachable.as_deref().unwrap_or("connecting");
            let text = vec![
                Line::raw(""),
                row("Agent", format!("not reachable ({code})"), Color::Red),
                Line::raw(""),
                Line::styled(
                    "  Start it in another terminal:  erebor agent start",
                    Style::new().fg(FG),
                ),
                Line::styled("  Retrying every second…", Style::new().fg(DIM)),
            ];
            f.render_widget(Paragraph::new(text).block(block("Status")), body);
        }
    }

    // Footer: keys and the result of the last action.
    let key = |k: &str, l: &str| {
        vec![
            Span::styled(format!("[{k}]"), Style::new().fg(ACCENT)),
            Span::styled(format!(" {l}  "), Style::new().fg(FG)),
        ]
    };
    let pause = if app.snap.as_ref().is_some_and(|s| s.state == "paused") {
        "Resume"
    } else {
        "Pause"
    };
    let mut keys: Vec<Span> = [
        ("S", "Start"),
        ("P", pause),
        ("X", "Stop"),
        ("R", "Restart"),
        ("?", "Help"),
        ("Q", "Quit"),
    ]
    .iter()
    .flat_map(|(k, l)| key(k, l))
    .collect();
    keys.insert(0, Span::raw(" "));
    let mut lines = vec![Line::from(keys)];
    if let Some((m, err)) = &app.msg {
        lines.insert(
            0,
            Line::styled(
                format!(" {m}"),
                Style::new().fg(if *err { Color::Red } else { ACCENT }),
            ),
        );
    }
    let footer_block = Block::bordered().border_style(Style::new().fg(DIM));
    let inner = footer_block.inner(footer);
    f.render_widget(footer_block, footer);
    // Last line(s) that fit: the message, if any, above the keys.
    let skip = lines.len().saturating_sub(inner.height as usize);
    f.render_widget(Paragraph::new(lines.split_off(skip)), inner);

    if app.help {
        draw_help(f, area);
    }
}

fn draw_body(f: &mut Frame, area: Rect, s: &Snapshot, app: &App) {
    let wide = area.width >= 100;
    let panels_h = if wide { 9 } else { 16 };
    let [panels, logs] =
        Layout::vertical([Constraint::Length(panels_h), Constraint::Min(0)]).areas(area);
    let [left, right] = if wide {
        Layout::horizontal([Constraint::Percentage(50); 2]).areas(panels)
    } else {
        Layout::vertical([Constraint::Length(7), Constraint::Length(9)]).areas(panels)
    };

    let hot = s.cpu_temp_c.is_some_and(|t| t >= 80.0);
    let mut metrics = vec![
        row("Hashrate", fmt::hashrate(s.hashrate), ACCENT),
        row("Earnings", fmt::earnings(s), FG),
        row(
            "CPU temperature",
            fmt::temp(s.cpu_temp_c),
            if hot { Color::Red } else { FG },
        ),
        row("Power", fmt::power(s.power_w, &s.power_source), FG),
        row("Uptime", fmt::duration(s.uptime_secs), FG),
    ];
    if let Some(g) = s.gpus.iter().find(|g| g.temp_c.is_some()) {
        metrics.push(row("GPU temperature", fmt::temp(g.temp_c), FG));
    }
    f.render_widget(Paragraph::new(metrics).block(block("Metrics")), left);

    let miner = s.miner.as_ref().map_or(fmt::NA.to_string(), |m| {
        format!("{} {}", m.name, m.version.clone().unwrap_or_default())
            .trim()
            .to_string()
    });
    let pool = match (&s.pool, s.pool_connected) {
        (Some(p), true) => (p.clone(), "Connected", ACCENT),
        (Some(p), false) => (p.clone(), "Connecting", Color::Yellow),
        (None, _) => (s.pool_id.clone(), "Idle", DIM),
    };
    let fee = if s.dev_fee.enabled {
        format!("{}% · now: {}", s.dev_fee.percent, s.dev_fee.active)
    } else {
        "off".into()
    };
    let mut info = vec![
        row("Miner", miner, FG),
        row("Coin", format!("{} · {}", s.coin_name, s.algo), FG),
        row("Pool", pool.1.into(), pool.2),
        row("Pool address", pool.0, FG),
        row("Accepted shares", s.accepted.to_string(), FG),
        row(
            "Rejected shares",
            s.rejected.to_string(),
            if s.rejected > 0 { Color::Yellow } else { FG },
        ),
        row("Dev fee", fee, FG),
    ];
    if let Some(e) = &s.error {
        info.push(row("Error", e.clone(), Color::Red));
    } else if let (Some(b), false) = (&s.start_blocker, s.is_active()) {
        info.push(row("Cannot start", b.clone(), Color::Yellow));
    }
    f.render_widget(Paragraph::new(info).block(block("Miner")), right);

    if logs.height > 2 {
        let h = (logs.height - 2) as usize;
        let max_scroll = app.logs.len().saturating_sub(h);
        let end = app.logs.len() - app.scroll.min(max_scroll);
        let lines: Vec<Line> = app.logs[end.saturating_sub(h)..end]
            .iter()
            .map(|l| Line::styled(format!(" {l}"), Style::new().fg(FG)))
            .collect();
        let title = if app.scroll > 0 { "Logs ↑" } else { "Logs" };
        f.render_widget(Paragraph::new(lines).block(block(title)), logs);
    }
}

fn draw_help(f: &mut Frame, area: Rect) {
    let w = area.width.min(58);
    let h = area.height.min(14);
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
        ("↑ ↓ PgUp PgDn", "scroll logs (Home/End: oldest/newest)"),
        ("Q / Esc", "quit — mining keeps running"),
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
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        " Any key closes this help.",
        Style::new().fg(DIM),
    ));
    f.render_widget(Clear, r);
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .style(Style::new().bg(BG))
            .block(block("Help")),
        r,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use erebor_core::fleet::{EarningsSummary, MinerSummary};
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
        for (w, h) in [(80, 30), (120, 30)] {
            let out = render(&app_with(running()), w, h);
            for s in [
                "EREBOR",
                "● Running",
                "DEMO",
                "4.25 kH/s",
                "0.82 EUR / day",
                "1h 02m 03s",
                "XMRig 6.26.0",
                "Connected",
                "148",
                "log line 49",
                "[Q] Quit",
            ] {
                assert!(out.contains(s), "{w}x{h}: missing {s:?}\n{out}");
            }
            // No sensor: shown as unavailable, never a made-up number.
            assert!(out.contains("CPU temperature  n/a"), "{out}");
            assert!(out.contains("Power            n/a"), "{out}");
        }
    }

    #[test]
    fn offline_tiny_and_help_screens_do_not_panic() {
        let mut a = App::new("/nonexistent".into());
        a.unreachable = Some("agent_unreachable".into());
        assert!(render(&a, 80, 24).contains("erebor agent start"));
        render(&a, 10, 4);
        render(&a, 1, 1);
        let mut a = app_with(running());
        a.help = true;
        a.scroll = 1000;
        assert!(render(&a, 80, 24).contains("mining keeps running"));
        render(&a, 20, 6);
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
    }
}

//! XMRig integration: config generation, child process control, HTTP API (loopback only).
//!
//! The process is started with a fixed argument list (`--config=<file>`): no shell, no
//! string concatenation of user input into a command line. All user values are validated
//! before they reach the JSON config file.

use crate::miner::{LaunchSpec, MinerAdapter, MinerInfo, MinerStats, PoolTarget};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// XMRig's built-in donation (`donate-level`). Official builds enforce a minimum of 1%
/// (`kMinimumDonateLevel` in src/donate.h). Erebor keeps the minimum and shows it to the user.
pub const XMRIG_DONATE_LEVEL: u8 = 1;
const MAX_LOG_LINES: usize = 500;

/// A user-selected miner must be an existing absolute file whose name starts with `prefix`
/// ("xmrig", "srbminer", …).
pub fn validate_binary_path(p: &str, prefix: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(p);
    if !path.is_absolute() {
        return Err("path must be absolute".into());
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !name.starts_with(prefix) {
        return Err(format!("file name must start with '{prefix}'"));
    }
    if !path.is_file() {
        return Err("file not found".into());
    }
    Ok(path)
}

/// Parse `XMRig 6.22.2` from `xmrig --version` output.
fn parse_version(output: &str) -> Option<String> {
    output.lines().find_map(|l| {
        let rest = l.trim().strip_prefix("XMRig ")?;
        let v = rest.split_whitespace().next()?;
        v.chars()
            .all(|c| c.is_ascii_digit() || c == '.')
            .then(|| v.to_string())
    })
}

pub fn detect_version(path: &Path) -> Result<String, String> {
    let out = crate::command(path)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    parse_version(&String::from_utf8_lossy(&out.stdout))
        .ok_or_else(|| "not an XMRig binary (version not recognised)".into())
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

fn pool_json(pool: &PoolTarget) -> Value {
    json!({
        "algo": "rx/0",
        "coin": "monero",
        "url": pool.url,
        "user": pool.user,
        "pass": pool.pass,
        "rig-id": pool.pass,
        "keepalive": true,
        "enabled": true,
        "tls": pool.tls,
        "tls-fingerprint": null,
        "nicehash": false,
        "daemon": false
    })
}

/// Full XMRig JSON config. The HTTP API listens on loopback only with a random token.
fn build_config(spec: &LaunchSpec, api_port: u16, token: &str) -> Value {
    json!({
        "autosave": false,
        "background": false,
        "colors": false,
        "donate-level": XMRIG_DONATE_LEVEL,
        "donate-over-proxy": XMRIG_DONATE_LEVEL,
        "log-file": null,
        "print-time": 30,
        "health-print-time": 60,
        "retries": 5,
        "retry-pause": 5,
        "syslog": false,
        "watch": false,
        "pause-on-battery": false,
        "pause-on-active": false,
        "http": {
            "enabled": true,
            "host": "127.0.0.1",
            "port": api_port,
            "access-token": token,
            "restricted": false
        },
        "cpu": {
            "enabled": true,
            "huge-pages": true,
            "yield": true,
            "priority": spec.priority.min(5),
            "max-threads-hint": spec.threads_percent.clamp(1, 100)
        },
        "opencl": { "enabled": false },
        "cuda": { "enabled": false },
        "pools": [pool_json(&spec.pool)]
    })
}

pub fn parse_summary(v: &Value) -> MinerStats {
    let hr = |i: usize| {
        v.pointer(&format!("/hashrate/total/{i}"))
            .and_then(|x| x.as_f64())
    };
    let conn = v.get("connection");
    let pool = conn
        .and_then(|c| c.get("pool"))
        .and_then(|p| p.as_str())
        .filter(|p| !p.is_empty())
        .map(String::from);
    let uptime = conn
        .and_then(|c| c.get("uptime"))
        .and_then(|u| u.as_u64())
        .unwrap_or(0);
    MinerStats {
        hashrate_10s: hr(0),
        hashrate_60s: hr(1),
        hashrate_15m: hr(2),
        hashrate_max: v.pointer("/hashrate/highest").and_then(|x| x.as_f64()),
        accepted: conn
            .and_then(|c| c.get("accepted"))
            .and_then(|x| x.as_u64())
            .or_else(|| v.pointer("/results/shares_good").and_then(|x| x.as_u64()))
            .unwrap_or(0),
        rejected: conn
            .and_then(|c| c.get("rejected"))
            .and_then(|x| x.as_u64())
            .unwrap_or(0),
        connected: pool.is_some() && uptime > 0,
        pool,
        tls: conn
            .and_then(|c| c.get("tls"))
            .map(|t| t.is_string() || t.as_bool() == Some(true)),
        ping_ms: conn.and_then(|c| c.get("ping")).and_then(|x| x.as_u64()),
        algo: v.get("algo").and_then(|x| x.as_str()).map(String::from),
        version: v.get("version").and_then(|x| x.as_str()).map(String::from),
        paused: v.get("paused").and_then(|x| x.as_bool()).unwrap_or(false),
        threads: v
            .pointer("/hashrate/threads")
            .and_then(|t| t.as_array())
            .map(|a| a.len() as u32),
    }
}

/// Parse the 10s/60s speed from a stdout line like
/// `miner    speed 10s/60s/15m 4123.1 4101.9 n/a H/s max 4200.2 H/s`.
fn parse_speed_line(line: &str) -> Option<(Option<f64>, Option<f64>)> {
    let idx = line.find("speed 10s/60s/15m")?;
    let mut it = line[idx + "speed 10s/60s/15m".len()..].split_whitespace();
    let a = it.next()?.parse().ok();
    let b = it.next()?.parse().ok();
    Some((a, b))
}

pub fn free_loopback_port() -> Result<u16, String> {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|e| e.to_string())
}

fn random_token() -> String {
    use rand::Rng;
    let bytes: [u8; 24] = rand::thread_rng().gen();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub type LogBuffer = Arc<Mutex<VecDeque<String>>>;
pub type LineHook = Arc<dyn Fn(&str) + Send + Sync>;

pub fn spawn_log_reader<R: std::io::Read + Send + 'static>(
    src: R,
    buf: LogBuffer,
    on_line: Option<LineHook>,
) {
    std::thread::spawn(move || {
        for line in BufReader::new(src).lines().map_while(Result::ok) {
            if let Some(f) = &on_line {
                f(&line);
            }
            let mut b = buf.lock().unwrap();
            if b.len() >= MAX_LOG_LINES {
                b.pop_front();
            }
            b.push_back(line);
        }
    });
}

/// Windows refuses to run a file its antivirus flagged (ERROR_VIRUS_INFECTED/_DELETED).
pub fn spawn_error(e: std::io::Error) -> String {
    if matches!(e.raw_os_error(), Some(225 | 226)) && cfg!(windows) {
        "blocked_by_antivirus".into()
    } else {
        format!("spawn failed: {e}")
    }
}

pub fn command(binary: &Path) -> Command {
    let mut cmd = crate::command(binary);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

pub struct XmrigMiner {
    binary: PathBuf,
    work_dir: PathBuf,
    version: Option<String>,
    child: Option<Child>,
    api_port: u16,
    token: String,
    spec: Option<LaunchSpec>,
    logs: LogBuffer,
    http: ureq::Agent,
}

impl XmrigMiner {
    pub fn new(binary: PathBuf, work_dir: PathBuf, version: Option<String>) -> Self {
        Self {
            binary,
            work_dir,
            version,
            child: None,
            api_port: 0,
            token: random_token(),
            spec: None,
            logs: Arc::new(Mutex::new(VecDeque::new())),
            http: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(3))
                .build(),
        }
    }

    fn api(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.api_port)
    }

    fn auth(&self) -> String {
        format!("Bearer {}", self.token)
    }

    fn config_path(&self) -> PathBuf {
        self.work_dir.join("xmrig-config.json")
    }

    fn rpc(&self, method: &str) -> Result<(), String> {
        self.http
            .post(&self.api("/json_rpc"))
            .set("Authorization", &self.auth())
            .send_json(json!({"id": 1, "jsonrpc": "2.0", "method": method}))
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

impl MinerAdapter for XmrigMiner {
    fn info(&self) -> MinerInfo {
        MinerInfo {
            id: "xmrig",
            name: "XMRig",
            version: self.version.clone(),
            algorithms: vec!["rx/0"],
            supports_pause: true,
            supports_hot_switch: true,
            supports_benchmark: true,
            builtin_donation_percent: XMRIG_DONATE_LEVEL as f64,
        }
    }

    fn start(&mut self, spec: &LaunchSpec) -> Result<(), String> {
        if self.child.is_some() {
            return Err("already running".into());
        }
        std::fs::create_dir_all(&self.work_dir).map_err(|e| e.to_string())?;
        self.api_port = free_loopback_port()?;
        self.token = random_token();
        let cfg = build_config(spec, self.api_port, &self.token);
        let path = self.config_path();
        std::fs::write(&path, serde_json::to_vec_pretty(&cfg).unwrap())
            .map_err(|e| e.to_string())?;
        restrict_permissions(&path);
        let mut child = command(&self.binary)
            .arg(format!("--config={}", path.display()))
            .current_dir(&self.work_dir)
            .spawn()
            .map_err(spawn_error)?;
        if let Some(o) = child.stdout.take() {
            spawn_log_reader(o, self.logs.clone(), None);
        }
        if let Some(e) = child.stderr.take() {
            spawn_log_reader(e, self.logs.clone(), None);
        }
        self.child = Some(child);
        self.spec = Some(spec.clone());
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        let _ = std::fs::remove_file(self.config_path());
    }

    fn set_paused(&mut self, paused: bool) -> Result<(), String> {
        self.rpc(if paused { "pause" } else { "resume" })
    }

    fn switch_pool(&mut self, pool: &PoolTarget) -> Result<(), String> {
        let mut spec = self.spec.clone().ok_or("not running")?;
        spec.pool = pool.clone();
        let cfg = build_config(&spec, self.api_port, &self.token);
        self.http
            .put(&self.api("/1/config"))
            .set("Authorization", &self.auth())
            .send_json(cfg)
            .map_err(|e| e.to_string())?;
        self.spec = Some(spec);
        Ok(())
    }

    fn stats(&mut self) -> Result<MinerStats, String> {
        let v: Value = self
            .http
            .get(&self.api("/2/summary"))
            .set("Authorization", &self.auth())
            .call()
            .map_err(|e| e.to_string())?
            .into_json()
            .map_err(|e| e.to_string())?;
        Ok(parse_summary(&v))
    }

    fn take_exit(&mut self) -> Option<String> {
        let status = self.child.as_mut()?.try_wait().ok()??;
        self.child = None;
        Some(format!("XMRig exited unexpectedly ({status})"))
    }

    fn logs(&self) -> Vec<String> {
        self.logs.lock().unwrap().iter().cloned().collect()
    }

    fn pid(&self) -> Option<u32> {
        self.child.as_ref().map(|c| c.id())
    }
}

impl Drop for XmrigMiner {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Offline benchmark (`--bench`): no pool connection, no wallet. Runs for `duration`
/// at the given thread percentage and returns the last 60s (or 10s) hashrate printed.
pub fn run_benchmark(
    binary: &Path,
    threads_percent: u8,
    duration: Duration,
    cancel: &std::sync::atomic::AtomicBool,
    progress: &dyn Fn(f64, Option<f64>),
) -> Result<f64, String> {
    let last: Arc<Mutex<Option<f64>>> = Arc::new(Mutex::new(None));
    let sink = last.clone();
    let on_line: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(move |l: &str| {
        if let Some((h10, h60)) = parse_speed_line(l) {
            if let Some(h) = h60.or(h10) {
                *sink.lock().unwrap() = Some(h);
            }
        }
    });
    let logs: LogBuffer = Arc::new(Mutex::new(VecDeque::new()));
    let mut child = command(binary)
        .args([
            "--bench=10M".to_string(),
            "--print-time=5".to_string(),
            "--no-color".to_string(),
            format!("--cpu-max-threads-hint={}", threads_percent.clamp(1, 100)),
        ])
        .spawn()
        .map_err(spawn_error)?;
    if let Some(o) = child.stdout.take() {
        spawn_log_reader(o, logs.clone(), Some(on_line));
    }
    if let Some(e) = child.stderr.take() {
        spawn_log_reader(e, logs, None);
    }
    let start = std::time::Instant::now();
    let result = loop {
        std::thread::sleep(Duration::from_millis(500));
        let elapsed = start.elapsed();
        progress(
            (elapsed.as_secs_f64() / duration.as_secs_f64()).min(1.0),
            *last.lock().unwrap(),
        );
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            break Err("cancelled".to_string());
        }
        if let Ok(Some(st)) = child.try_wait() {
            break Err(format!("XMRig exited during benchmark ({st})"));
        }
        if elapsed >= duration {
            break last
                .lock()
                .unwrap()
                .ok_or_else(|| "no hashrate reported".to_string());
        }
    };
    let _ = child.kill();
    let _ = child.wait();
    result
}

/// The config file contains the API token: keep it private to the user.
fn restrict_permissions(_path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(_path, std::fs::Permissions::from_mode(0o600));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> LaunchSpec {
        LaunchSpec {
            pool: PoolTarget {
                url: "pool.supportxmr.com:443".into(),
                tls: true,
                user: "WALLET".into(),
                pass: "rig1".into(),
            },
            algo: "rx/0".into(),
            threads_percent: 50,
            priority: 1,
        }
    }

    #[test]
    fn config_is_loopback_and_safe() {
        let c = build_config(&spec(), 45678, "tok");
        assert_eq!(c["http"]["host"], "127.0.0.1");
        assert_eq!(c["http"]["port"], 45678);
        assert_eq!(c["http"]["access-token"], "tok");
        assert_eq!(c["autosave"], false);
        assert_eq!(c["background"], false);
        assert_eq!(c["donate-level"], 1);
        assert_eq!(c["cpu"]["max-threads-hint"], 50);
        assert_eq!(c["pools"][0]["url"], "pool.supportxmr.com:443");
        assert_eq!(c["pools"][0]["tls"], true);
        assert_eq!(c["pools"][0]["user"], "WALLET");
        assert_eq!(c["opencl"]["enabled"], false);
    }

    #[test]
    fn clamps_limits() {
        let mut s = spec();
        s.threads_percent = 0;
        s.priority = 9;
        let c = build_config(&s, 1, "t");
        assert_eq!(c["cpu"]["max-threads-hint"], 1);
        assert_eq!(c["cpu"]["priority"], 5);
    }

    #[test]
    fn parses_summary() {
        let v = json!({
            "version": "6.22.2", "algo": "rx/0", "paused": false,
            "hashrate": {"total": [4100.5, 4090.0, null], "highest": 4200.0, "threads": [[1],[2],[3],[4]]},
            "connection": {"pool": "pool.supportxmr.com:443", "uptime": 120, "ping": 35,
                           "accepted": 10, "rejected": 1, "tls": "TLSv1.3"},
            "results": {"shares_good": 10}
        });
        let s = parse_summary(&v);
        assert_eq!(s.hashrate_10s, Some(4100.5));
        assert_eq!(s.hashrate_15m, None);
        assert_eq!(s.accepted, 10);
        assert_eq!(s.rejected, 1);
        assert!(s.connected);
        assert_eq!(s.tls, Some(true));
        assert_eq!(s.threads, Some(4));
        assert_eq!(s.current_hashrate(), Some(4100.5));
    }

    #[test]
    fn parses_summary_while_warming_up() {
        let s = parse_summary(
            &json!({"hashrate": {"total": [null, null, null]}, "connection": {"pool": "", "uptime": 0}}),
        );
        assert_eq!(s.current_hashrate(), None);
        assert!(!s.connected);
        assert_eq!(s.pool, None);
    }

    #[test]
    fn parses_version_and_speed() {
        assert_eq!(
            parse_version(" XMRig 6.22.2\n built on Jan 1 2025 with GCC"),
            Some("6.22.2".into())
        );
        assert_eq!(parse_version("something else"), None);
        let l = "[2025-01-01 10:00:00.000]  miner    speed 10s/60s/15m 4123.1 4101.9 n/a H/s max 4200.2 H/s";
        assert_eq!(parse_speed_line(l), Some((Some(4123.1), Some(4101.9))));
        let l2 = "miner    speed 10s/60s/15m n/a n/a n/a H/s max n/a H/s";
        assert_eq!(parse_speed_line(l2), Some((None, None)));
    }

    #[test]
    fn rejects_bad_binary_paths() {
        assert!(validate_binary_path("relative/xmrig", "xmrig").is_err());
        assert!(validate_binary_path("/bin/sh", "xmrig").is_err());
        let dir = tempfile::tempdir().unwrap();
        let p = dir
            .path()
            .join(crate::miner::MinerKind::Xmrig.binary_name());
        std::fs::write(&p, b"x").unwrap();
        assert!(validate_binary_path(p.to_str().unwrap(), "xmrig").is_ok());
    }

    #[test]
    fn token_is_random_hex() {
        let a = random_token();
        assert_eq!(a.len(), 48);
        assert_ne!(a, random_token());
    }
}

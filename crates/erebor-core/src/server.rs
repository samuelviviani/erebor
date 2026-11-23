//! Remote control over HTTP: other devices (a laptop, a phone's browser) drive this
//! computer's engine with the same commands as the local GUI.
//!
//! `POST /rpc/<command>` with `Authorization: Bearer <token>` and JSON arguments;
//! any other `GET` serves the web interface. Plain HTTP with a random 256-bit token:
//! meant for the home network or a private VPN (Tailscale, WireGuard), not for exposing
//! a port to the internet.

use crate::rpc::{self, Shared};
use serde_json::Value;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tiny_http::{Header, Request, Response, Server};

/// Resolves a web asset path ("index.html", "assets/x.js") to (bytes, mime type).
pub type Assets = Arc<dyn Fn(&str) -> Option<(Vec<u8>, String)> + Send + Sync>;

const MAX_BODY: u64 = 1 << 20;

/// A listening server; dropping it stops listening.
pub struct Running {
    server: Arc<Server>,
    pub port: u16,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.server.unblock();
    }
}

pub fn start(engine: Shared, port: u16, token: String, assets: Assets) -> Result<Running, String> {
    let server = Arc::new(Server::http(("0.0.0.0", port)).map_err(|e| format!("listen: {e}"))?);
    let s = server.clone();
    let token = Arc::new(token);
    std::thread::spawn(move || {
        for req in s.incoming_requests() {
            let (engine, token, assets) = (engine.clone(), token.clone(), assets.clone());
            // ponytail: one thread per request, fine for a handful of devices.
            std::thread::spawn(move || handle(req, &engine, &token, &assets));
        }
    });
    Ok(Running { server, port })
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("static header")
}

fn json(status: u16, v: &Value) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_data(serde_json::to_vec(v).unwrap_or_default())
        .with_status_code(status)
        .with_header(header("Content-Type", "application/json"))
        .with_header(header("Cache-Control", "no-store"))
}

/// Constant-time comparison: the token must not leak through response timing.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

fn handle(mut req: Request, engine: &Shared, token: &str, assets: &Assets) {
    let url = req.url().split('?').next().unwrap_or("/").to_string();
    let res = if let Some(cmd) = url.strip_prefix("/rpc/") {
        let auth = req
            .headers()
            .iter()
            .find(|h| h.field.equiv("Authorization"))
            .map(|h| h.value.as_str().trim_start_matches("Bearer ").to_string());
        if *req.method() != tiny_http::Method::Post {
            json(405, &Value::from("method_not_allowed"))
        } else if !auth.is_some_and(|a| same(&a, token)) {
            std::thread::sleep(Duration::from_millis(500)); // slows down guessing
            json(401, &Value::from("remote_unauthorized"))
        } else {
            let mut body = Vec::new();
            let _ = req.as_reader().take(MAX_BODY).read_to_end(&mut body);
            let args: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
            match rpc::call(engine, cmd, &args, true) {
                Ok(v) => json(200, &v),
                Err(e) => json(400, &e),
            }
        }
    } else {
        let path = url.trim_start_matches('/');
        // Single-page app: unknown paths get the index.
        match assets(if path.is_empty() { "index.html" } else { path })
            .or_else(|| assets("index.html"))
        {
            Some((bytes, mime)) => Response::from_data(bytes)
                .with_header(header("Content-Type", &mime))
                .with_header(header("X-Content-Type-Options", "nosniff")),
            None => json(404, &Value::from("not_found")),
        }
    };
    let _ = req.respond(res);
}

/// Web interface from a folder (the desktop app's `dist`), refusing paths outside it.
pub fn dir_assets(dir: PathBuf) -> Assets {
    Arc::new(move |path: &str| {
        let rel = Path::new(path);
        if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
            return None;
        }
        let bytes = std::fs::read(dir.join(rel)).ok()?;
        let mime = match rel.extension().and_then(|e| e.to_str()).unwrap_or("") {
            "html" => "text/html; charset=utf-8",
            "js" => "text/javascript",
            "css" => "text/css",
            "svg" => "image/svg+xml",
            "png" => "image/png",
            "json" | "webmanifest" => "application/json",
            "woff2" => "font/woff2",
            _ => "application/octet-stream",
        };
        Some((bytes, mime.into()))
    })
}

/// The access token, created on first use and kept in the data dir (owner-only on Unix).
pub fn token(data_dir: &Path) -> Result<String, String> {
    let p = data_dir.join("remote-token");
    if let Ok(t) = std::fs::read_to_string(&p) {
        if t.trim().len() >= 32 {
            return Ok(t.trim().into());
        }
    }
    let t: String = (0..32)
        .map(|_| format!("{:02x}", rand::random::<u8>()))
        .collect();
    std::fs::write(&p, &t).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    }
    Ok(t)
}

/// This computer's address on the local network (no packet is sent).
pub fn lan_ip() -> Option<String> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:80").ok()?;
    Some(s.local_addr().ok()?.ip().to_string())
}

/// Client side: run `cmd` on a remote Erebor at `url` ("http://host:port").
pub fn remote_call(url: &str, token: &str, cmd: &str, args: &Value) -> Result<Value, Value> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("invalid_url".into());
    }
    let res = ureq::post(&format!("{}/rpc/{cmd}", url.trim_end_matches('/')))
        .set("Authorization", &format!("Bearer {token}"))
        .timeout(Duration::from_secs(20))
        .send_json(args);
    match res {
        Ok(r) => r.into_json().map_err(|_| "remote_bad_response".into()),
        Err(ureq::Error::Status(401, _)) => Err("remote_unauthorized".into()),
        Err(ureq::Error::Status(_, r)) => Err(r.into_json().unwrap_or("remote_error".into())),
        Err(_) => Err("remote_unreachable".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{self, Engine};
    use serde_json::json;
    use std::sync::Mutex;

    #[test]
    fn remote_commands_need_the_token_and_assets_stay_in_their_folder() {
        let dir = tempfile::tempdir().unwrap();
        let web = tempfile::tempdir().unwrap();
        std::fs::write(web.path().join("index.html"), "<html>").unwrap();
        let e: Shared = Arc::new(Mutex::new(
            Engine::new(dir.path().into(), engine::default_factory()).unwrap(),
        ));
        let tok = token(dir.path()).unwrap();
        assert_eq!(tok, token(dir.path()).unwrap(), "token is stable");
        let s = start(e, 0, tok.clone(), dir_assets(web.path().into())).unwrap();
        let port = s.server.server_addr().to_ip().unwrap().port();
        let url = format!("http://127.0.0.1:{port}");

        let c = remote_call(&url, &tok, "get_config", &Value::Null).unwrap();
        assert_eq!(c["coin"], "AUTO");
        assert_eq!(
            remote_call(&url, "wrong", "get_config", &Value::Null),
            Err(json!("remote_unauthorized"))
        );
        assert_eq!(
            remote_call(&url, &tok, "select_miner", &json!({"path": "/bin/sh"})),
            Err(json!("local_only"))
        );
        let page = ureq::get(&format!("{url}/some/route")).call().unwrap();
        assert_eq!(page.into_string().unwrap(), "<html>");
        assert!(dir_assets(web.path().into())("../secret").is_none());
    }
}

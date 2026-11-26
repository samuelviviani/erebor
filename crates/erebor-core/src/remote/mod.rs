//! Remote control between a controller (desktop app, CLI) and `erebor-agent` on another
//! computer. Protocol v1, documented in docs/PROTOCOL.md.
//!
//! - Transport: TCP + TLS 1.3, port 7420 by default. Both sides present a self-signed
//!   certificate generated on first use (`Identity`); trust is the SHA-256 fingerprint of
//!   that certificate, pinned at pairing time. No CA, no account, no central server.
//! - Pairing: the agent shows a one-time code (60 bits, 10 minutes, 5 attempts). Both sides
//!   prove they know it with an HMAC bound to both certificate fingerprints, so a machine
//!   in the middle can't splice itself in. Afterwards only the certificates are used.
//! - Framing: one JSON object per line, many requests per connection. Requests carry an
//!   id and a timestamp; replies echo the id. `subscribe` turns the connection into a
//!   stream of events (telemetry every few seconds).
//! - Commands: an explicit list (`server::dispatch`). No shell, no file paths.

pub mod client;
pub mod server;

use hmac::{Hmac, Mac};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, DistinguishedName, SignatureScheme};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const PORT: u16 = 7420;
/// Protocol spoken by this build, and the oldest one it still accepts.
pub const PROTOCOL: u32 = 1;
pub const MIN_PROTOCOL: u32 = 1;
/// Requests older or newer than this (seconds) are refused: a recorded request can't be
/// replayed later. TLS already prevents replay inside a connection.
pub const MAX_CLOCK_SKEW: i64 = 600;
pub const MAX_LINE: u64 = 1 << 20;
pub const PAIRING_TTL_SECS: i64 = 600;
pub const PAIRING_MAX_ATTEMPTS: u32 = 5;

pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("remote")
}

/// Write a file only the owner can read, atomically.
pub fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700));
        }
    }
    let tmp = path.with_extension("tmp");
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    let mut f = o.open(&tmp).map_err(|e| e.to_string())?;
    f.write_all(bytes).map_err(|e| e.to_string())?;
    drop(f);
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(p: &Path) -> T {
    std::fs::read_to_string(p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_json<T: Serialize>(p: &Path, v: &T) -> Result<(), String> {
    write_private(
        p,
        serde_json::to_string_pretty(v)
            .map_err(|e| e.to_string())?
            .as_bytes(),
    )
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn fingerprint(cert: &[u8]) -> String {
    hex(&Sha256::digest(cert))
}

/// First 16 hex digits in groups of four: what people compare by eye.
pub fn short_fp(fp: &str) -> String {
    fp.as_bytes()
        .chunks(4)
        .take(4)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join(":")
}

// ---------- identity ----------

/// This installation's TLS certificate and key (`remote/cert.der`, `remote/key.der`, 0600).
/// The same identity serves as agent (server) and as controller (client).
#[derive(Clone)]
pub struct Identity {
    pub cert: CertificateDer<'static>,
    key: Arc<Vec<u8>>,
    pub fingerprint: String,
}

impl Identity {
    pub fn load_or_create(data_dir: &Path) -> Result<Self, String> {
        let d = dir(data_dir);
        let (cp, kp) = (d.join("cert.der"), d.join("key.der"));
        if let (Ok(cert), Ok(key)) = (std::fs::read(&cp), std::fs::read(&kp)) {
            return Ok(Self::from_der(cert, key));
        }
        let ck = rcgen::generate_simple_self_signed(vec!["erebor-agent".into()])
            .map_err(|e| format!("certificate: {e}"))?;
        let (cert, key) = (ck.cert.der().to_vec(), ck.signing_key.serialize_der());
        write_private(&kp, &key)?;
        write_private(&cp, &cert)?;
        Ok(Self::from_der(cert, key))
    }

    fn from_der(cert: Vec<u8>, key: Vec<u8>) -> Self {
        Self {
            fingerprint: fingerprint(&cert),
            cert: CertificateDer::from(cert),
            key: Arc::new(key),
        }
    }

    fn key(&self) -> PrivateKeyDer<'static> {
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key.to_vec()))
    }

    /// New key and certificate: every controller has to pair again.
    pub fn rotate(data_dir: &Path) -> Result<Self, String> {
        let d = dir(data_dir);
        let _ = std::fs::remove_file(d.join("cert.der"));
        let _ = std::fs::remove_file(d.join("key.der"));
        Self::load_or_create(data_dir)
    }
}

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// Accepts any client certificate: the server then looks its fingerprint up in the list
/// of paired controllers (or treats the peer as a pairing candidate).
#[derive(Debug)]
struct AnyClientCert(Arc<CryptoProvider>);

impl ClientCertVerifier for AnyClientCert {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }
    fn verify_client_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        Ok(ClientCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        d: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(m, c, d, &self.0.signature_verification_algorithms)
    }
    fn verify_tls13_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        d: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(m, c, d, &self.0.signature_verification_algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// The agent's certificate must have the pinned fingerprint (`None`: pairing, accept any
/// and record what was seen; the pairing proofs then bind it).
#[derive(Debug)]
struct PinnedServer {
    pin: Option<String>,
    seen: Arc<Mutex<Option<String>>>,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for PinnedServer {
    fn verify_server_cert(
        &self,
        cert: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let fp = fingerprint(cert);
        *self.seen.lock().unwrap_or_else(|p| p.into_inner()) = Some(fp.clone());
        match &self.pin {
            Some(p) if *p != fp => Err(rustls::Error::General("fingerprint_mismatch".into())),
            _ => Ok(ServerCertVerified::assertion()),
        }
    }
    fn verify_tls12_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        d: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(m, c, d, &self.provider.signature_verification_algorithms)
    }
    fn verify_tls13_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        d: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(m, c, d, &self.provider.signature_verification_algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

fn server_config(id: &Identity) -> Result<Arc<rustls::ServerConfig>, String> {
    let p = provider();
    rustls::ServerConfig::builder_with_provider(p.clone())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| e.to_string())?
        .with_client_cert_verifier(Arc::new(AnyClientCert(p)))
        .with_single_cert(vec![id.cert.clone()], id.key())
        .map(Arc::new)
        .map_err(|e| e.to_string())
}

/// The fingerprint the server presented, filled in during the handshake.
type Seen = Arc<Mutex<Option<String>>>;

fn client_config(
    id: &Identity,
    pin: Option<&str>,
) -> Result<(Arc<rustls::ClientConfig>, Seen), String> {
    let p = provider();
    let seen = Arc::new(Mutex::new(None));
    let v = PinnedServer {
        pin: pin.map(str::to_string),
        seen: seen.clone(),
        provider: p.clone(),
    };
    let cfg = rustls::ClientConfig::builder_with_provider(p)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(v))
        .with_client_auth_cert(vec![id.cert.clone()], id.key())
        .map_err(|e| e.to_string())?;
    Ok((Arc::new(cfg), seen))
}

// ---------- framing ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub v: u32,
    pub id: String,
    pub ts: i64,
    pub cmd: String,
    #[serde(default)]
    pub args: Value,
}

/// One line, at most `MAX_LINE` bytes. `Ok(None)` at end of stream.
pub fn read_line(r: &mut impl BufRead) -> std::io::Result<Option<String>> {
    let mut buf = Vec::new();
    let n = r.by_ref().take(MAX_LINE).read_until(b'\n', &mut buf)?;
    if n == 0 {
        return Ok(None);
    }
    if buf.last() != Some(&b'\n') {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "line_too_long",
        ));
    }
    String::from_utf8(buf)
        .map(Some)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "not_utf8"))
}

pub fn write_line(w: &mut impl Write, v: &Value) -> std::io::Result<()> {
    let mut b = serde_json::to_vec(v)?;
    b.push(b'\n');
    w.write_all(&b)?;
    w.flush()
}

pub fn request_id() -> String {
    hex(&rand::random::<[u8; 8]>())
}

// ---------- pairing ----------

const CODE_ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Pairing {
    pub code: String,
    pub expires_at: i64,
    pub attempts: u32,
}

/// Canonical form of a code typed by a person: no separators, upper case, and the
/// letters Crockford's alphabet leaves out read as the digits they look like.
pub fn normalize_code(c: &str) -> String {
    c.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            c => c,
        })
        .collect()
}

pub fn format_code(c: &str) -> String {
    c.as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join("-")
}

/// A new one-time code (12 symbols = 60 bits), valid `PAIRING_TTL_SECS`. Replaces any
/// previous one.
pub fn start_pairing(data_dir: &Path) -> Result<Pairing, String> {
    use rand::Rng;
    let mut rng = rand::rngs::OsRng;
    let code: String = (0..12)
        .map(|_| CODE_ALPHABET[rng.gen_range(0..CODE_ALPHABET.len())] as char)
        .collect();
    let p = Pairing {
        code,
        expires_at: crate::now_unix() + PAIRING_TTL_SECS,
        attempts: 0,
    };
    write_json(&dir(data_dir).join("pairing.json"), &p)?;
    Ok(p)
}

pub fn cancel_pairing(data_dir: &Path) {
    let _ = std::fs::remove_file(dir(data_dir).join("pairing.json"));
}

pub fn pending_pairing(data_dir: &Path) -> Option<Pairing> {
    let p: Pairing = read_json(&dir(data_dir).join("pairing.json"));
    (!p.code.is_empty() && p.expires_at > crate::now_unix() && p.attempts < PAIRING_MAX_ATTEMPTS)
        .then_some(p)
}

/// HMAC proof that `role` ("controller" or "agent") knows the code, bound to both
/// certificates of this connection.
pub fn proof(code: &str, role: &str, agent_fp: &str, controller_fp: &str) -> String {
    let mut m = Hmac::<Sha256>::new_from_slice(normalize_code(code).as_bytes())
        .expect("hmac takes any key length");
    m.update(format!("erebor-pair-v1|{role}|{agent_fp}|{controller_fp}").as_bytes());
    hex(&m.finalize().into_bytes())
}

/// Constant-time comparison of two proofs.
pub fn same_proof(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

// ---------- agent-side settings and trust ----------

/// `remote/config.json`: whether this agent accepts remote controllers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RemoteConfig {
    /// Off until `erebor-agent init`: nothing listens on the network by default.
    pub enabled: bool,
    pub listen: String,
    /// Answer discovery broadcasts on the local network (UDP, same port).
    pub discovery: bool,
    /// ASICs this agent watches and controls as a gateway, configured by the user.
    pub asics: Vec<crate::asic::AsicEntry>,
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listen: format!("0.0.0.0:{PORT}"),
            discovery: true,
            asics: Vec::new(),
        }
    }
}

impl RemoteConfig {
    pub fn load(data_dir: &Path) -> Self {
        read_json(&dir(data_dir).join("config.json"))
    }
    pub fn save(&self, data_dir: &Path) -> Result<(), String> {
        self.listen
            .parse::<std::net::SocketAddr>()
            .map_err(|_| "invalid_listen_address".to_string())?;
        write_json(&dir(data_dir).join("config.json"), self)
    }
}

/// A controller this agent trusts (`remote/controllers.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct TrustedController {
    pub fingerprint: String,
    pub name: String,
    pub paired_at: i64,
}

pub fn controllers(data_dir: &Path) -> Vec<TrustedController> {
    read_json(&dir(data_dir).join("controllers.json"))
}

fn save_controllers(data_dir: &Path, list: &[TrustedController]) -> Result<(), String> {
    write_json(&dir(data_dir).join("controllers.json"), &list)
}

pub fn trust(data_dir: &Path, c: TrustedController) -> Result<(), String> {
    let mut list = controllers(data_dir);
    list.retain(|x| x.fingerprint != c.fingerprint);
    list.push(c);
    save_controllers(data_dir, &list)
}

/// Remove a controller by fingerprint or its prefix (at least 8 digits) or by name.
/// Its open connections are closed at their next request.
pub fn revoke(data_dir: &Path, who: &str) -> Result<TrustedController, String> {
    let mut list = controllers(data_dir);
    let who_fp = who.replace(':', "").to_ascii_lowercase();
    let hits: Vec<usize> = list
        .iter()
        .enumerate()
        .filter(|(_, c)| (who_fp.len() >= 8 && c.fingerprint.starts_with(&who_fp)) || c.name == who)
        .map(|(i, _)| i)
        .collect();
    match hits.as_slice() {
        [i] => {
            let c = list.remove(*i);
            save_controllers(data_dir, &list)?;
            audit(data_dir, &c.name, "-", "revoke", true);
            Ok(c)
        }
        [] => Err("controller_not_found".into()),
        _ => Err("controller_ambiguous".into()),
    }
}

/// Append to `remote/audit.log` (JSON lines; rotated at 1 MiB, one old file kept).
pub fn audit(data_dir: &Path, who: &str, peer: &str, cmd: &str, ok: bool) {
    let p = dir(data_dir).join("audit.log");
    if std::fs::metadata(&p).is_ok_and(|m| m.len() > 1 << 20) {
        let _ = std::fs::rename(&p, p.with_extension("log.1"));
    }
    let line = serde_json::json!({
        "ts": crate::now_unix(), "controller": who, "peer": peer, "cmd": cmd, "ok": ok,
    });
    let _ = std::fs::create_dir_all(dir(data_dir));
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)
    {
        let _ = writeln!(f, "{line}");
    }
}

pub fn audit_tail(data_dir: &Path, n: usize) -> Vec<Value> {
    let s = std::fs::read_to_string(dir(data_dir).join("audit.log")).unwrap_or_default();
    let lines: Vec<&str> = s.lines().collect();
    lines[lines.len().saturating_sub(n)..]
        .iter()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_normalize_and_proofs_bind_both_certificates() {
        assert_eq!(normalize_code("abcd-oil1 x"), "ABCD0111X");
        assert_eq!(format_code("ABCDEFGHJKMN"), "ABCD-EFGH-JKMN");
        let p = proof("ABCD-EFGH-JKMN", "controller", "aa", "bb");
        assert_eq!(p, proof("abcdefghjkmn", "controller", "aa", "bb"));
        assert_ne!(p, proof("ABCDEFGHJKMN", "agent", "aa", "bb"));
        assert_ne!(p, proof("ABCDEFGHJKMN", "controller", "aa", "cc"));
        assert!(same_proof(&p, &p.clone()) && !same_proof(&p, "x"));
    }

    #[test]
    fn identity_persists_and_trust_list_revokes() {
        let d = tempfile::tempdir().unwrap();
        let a = Identity::load_or_create(d.path()).unwrap();
        assert_eq!(
            Identity::load_or_create(d.path()).unwrap().fingerprint,
            a.fingerprint
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let m = std::fs::metadata(dir(d.path()).join("key.der")).unwrap();
            assert_eq!(m.permissions().mode() & 0o777, 0o600);
        }
        assert_ne!(
            Identity::rotate(d.path()).unwrap().fingerprint,
            a.fingerprint
        );

        let p = start_pairing(d.path()).unwrap();
        assert_eq!(p.code.len(), 12);
        assert!(pending_pairing(d.path()).is_some());
        cancel_pairing(d.path());
        assert!(pending_pairing(d.path()).is_none());

        for (fp, name) in [("aaaa1111bbbb", "laptop"), ("cccc2222dddd", "office")] {
            trust(
                d.path(),
                TrustedController {
                    fingerprint: fp.into(),
                    name: name.into(),
                    paired_at: 1,
                },
            )
            .unwrap();
        }
        assert_eq!(revoke(d.path(), "zzz"), Err("controller_not_found".into()));
        assert_eq!(revoke(d.path(), "aaaa:1111").unwrap().name, "laptop");
        assert_eq!(revoke(d.path(), "office").unwrap().name, "office");
        assert!(controllers(d.path()).is_empty());
        assert_eq!(audit_tail(d.path(), 10).len(), 2);
    }
}

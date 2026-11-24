//! Installation of official miner releases (XMRig, SRBMiner-MULTI) from pinned manifests.
//!
//! Only URLs under each project's official GitHub release path are accepted, every
//! archive is verified against the SHA-256 recorded in the manifest and only the miner
//! executable is extracted.
//! - `xmrig-manifest.json`: hashes from upstream `SHA256SUMS` (`scripts/update-xmrig-manifest.mjs`).
//! - `srbminer-manifest.json`: upstream publishes MD5 only; the archive was downloaded from
//!   the official release, its MD5 checked against the published one, and its SHA-256
//!   pinned (`scripts/update-srbminer-manifest.mjs`).
//! - `lolminer-manifest.json`: SHA-256 published by GitHub for each release asset, checked
//!   on download (`scripts/update-github-manifest.mjs`).
//! - `cpuminer-manifest.json`: same, for cpuminer-opt (Windows builds only upstream). Its
//!   archive holds one build per CPU feature level plus DLLs: all of it is extracted.

use crate::miner::MinerKind;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

fn official_prefix(kind: MinerKind) -> &'static str {
    match kind {
        MinerKind::Xmrig => "https://github.com/xmrig/xmrig/releases/download/",
        MinerKind::Srbminer => "https://github.com/doktor83/SRBMiner-Multi/releases/download/",
        MinerKind::Lolminer => "https://github.com/Lolliedieb/lolMiner-releases/releases/download/",
        MinerKind::Cpuminer => "https://github.com/JayDDee/cpuminer-opt/releases/download/",
    }
}
const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub url: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    #[serde(skip, default = "default_kind")]
    pub kind: MinerKind,
    pub version: String,
    pub assets: HashMap<String, Asset>,
}

fn default_kind() -> MinerKind {
    MinerKind::Xmrig
}

/// XMRig manifest (kept for existing callers).
pub fn manifest() -> Manifest {
    manifest_for(MinerKind::Xmrig)
}

pub fn manifest_for(kind: MinerKind) -> Manifest {
    let raw = match kind {
        MinerKind::Xmrig => include_str!("../xmrig-manifest.json"),
        MinerKind::Srbminer => include_str!("../srbminer-manifest.json"),
        MinerKind::Lolminer => include_str!("../lolminer-manifest.json"),
        MinerKind::Cpuminer => include_str!("../cpuminer-manifest.json"),
    };
    let mut m: Manifest = serde_json::from_str(raw).expect("valid embedded manifest");
    m.kind = kind;
    m
}

pub fn platform_key() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

impl Manifest {
    pub fn asset_for_platform(&self) -> Result<&Asset, String> {
        let a = self.assets.get(&platform_key()).ok_or_else(|| {
            format!(
                "no pinned {} {} build for {} in this release of Erebor",
                self.kind.label(),
                self.version,
                platform_key()
            )
        })?;
        if !a.url.starts_with(official_prefix(self.kind)) {
            return Err(format!(
                "manifest URL is not an official {} release",
                self.kind.label()
            ));
        }
        if a.sha256.len() != 64 || !a.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("manifest hash is invalid".into());
        }
        Ok(a)
    }
}

pub fn verify_sha256(data: &[u8], expected: &str) -> Result<(), String> {
    let got = format!("{:x}", Sha256::digest(data));
    if got.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(format!("SHA-256 mismatch: expected {expected}, got {got}"))
    }
}

pub fn download(url: &str, progress: &dyn Fn(u64, Option<u64>)) -> Result<Vec<u8>, String> {
    let resp = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .get(url)
        .call()
        .map_err(|e| e.to_string())?;
    let total = resp
        .header("Content-Length")
        .and_then(|l| l.parse::<u64>().ok());
    let mut reader = resp.into_reader().take(MAX_ARCHIVE_BYTES);
    let mut data = Vec::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        progress(data.len() as u64, total);
    }
    Ok(data)
}

fn is_binary_entry(path: &str, name: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n == name)
}

/// Extract only the XMRig executable from a .tar.gz or .zip archive.
pub fn extract_binary(archive: &[u8], url: &str, dest_dir: &Path) -> Result<PathBuf, String> {
    extract_binary_named(archive, url, dest_dir, MinerKind::Xmrig.binary_name())
}

/// Extract only the executable called `name` from a .tar.gz or .zip archive.
pub fn extract_binary_named(
    archive: &[u8],
    url: &str,
    dest_dir: &Path,
    name: &str,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    let dest = dest_dir.join(name);
    let mut bytes = Vec::new();
    if url.ends_with(".zip") {
        let mut z =
            zip::ZipArchive::new(std::io::Cursor::new(archive)).map_err(|e| e.to_string())?;
        for i in 0..z.len() {
            let mut f = z.by_index(i).map_err(|e| e.to_string())?;
            let entry = f.name().map_err(|e| e.to_string())?.into_owned();
            if f.is_file() && is_binary_entry(&entry, name) {
                f.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
                break;
            }
        }
    } else {
        let gz = flate2::read::GzDecoder::new(archive);
        let mut tar = tar::Archive::new(gz);
        for entry in tar.entries().map_err(|e| e.to_string())? {
            let mut e = entry.map_err(|e| e.to_string())?;
            let p = e
                .path()
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .into_owned();
            if e.header().entry_type().is_file() && is_binary_entry(&p, name) {
                e.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
                break;
            }
        }
    }
    if bytes.is_empty() {
        return Err(format!("{name} not found in archive"));
    }
    write_miner(&dest, &bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    Ok(dest)
}

/// Write an extracted miner file. On Windows an antivirus that blocks it answers
/// "access denied" (os error 5) or "virus detected" (225): say so instead.
fn write_miner(dest: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(dest, bytes).map_err(|e| {
        let blocked = e.kind() == std::io::ErrorKind::PermissionDenied
            || e.raw_os_error() == Some(225);
        if cfg!(windows) && blocked {
            "blocked_by_antivirus".to_string()
        } else {
            e.to_string()
        }
    })
}

/// Extract every regular file of a .zip into `dest_dir`, flat (names only, never paths:
/// nothing can land outside the folder).
fn extract_zip_files(archive: &[u8], dest_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(archive)).map_err(|e| e.to_string())?;
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|e| e.to_string())?;
        let entry = f.name().map_err(|e| e.to_string())?.into_owned();
        let Some(name) = Path::new(&entry).file_name().filter(|_| f.is_file()) else {
            continue;
        };
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        write_miner(&dest_dir.join(name), &bytes)?;
    }
    Ok(())
}

pub struct Installed {
    pub path: PathBuf,
    pub version: String,
    pub sha256: String,
}

/// Download, verify and extract the pinned XMRig build into `miners_dir/xmrig-<version>/`.
pub fn install_pinned(
    miners_dir: &Path,
    progress: &dyn Fn(u64, Option<u64>),
) -> Result<Installed, String> {
    install_pinned_kind(MinerKind::Xmrig, miners_dir, progress)
}

/// Same for any miner: `miners_dir/<prefix>-<version>/<binary>`.
pub fn install_pinned_kind(
    kind: MinerKind,
    miners_dir: &Path,
    progress: &dyn Fn(u64, Option<u64>),
) -> Result<Installed, String> {
    let m = manifest_for(kind);
    let asset = m.asset_for_platform()?;
    let data = download(&asset.url, progress)?;
    verify_sha256(&data, &asset.sha256)?;
    let dir = miners_dir.join(format!("{}-{}", kind.dir_prefix(), m.version));
    let path = if kind == MinerKind::Cpuminer {
        extract_zip_files(&data, &dir)?;
        dir.join(kind.binary_name())
    } else {
        extract_binary_named(&data, &asset.url, &dir, kind.binary_name())?
    };
    // Defender quarantines a miner as soon as it is written or read.
    let sha256 = crate::xmrig::sha256_file(&path).map_err(|e| {
        if cfg!(windows) {
            "blocked_by_antivirus".to_string()
        } else {
            e
        }
    })?;
    Ok(Installed {
        path,
        version: m.version,
        sha256,
    })
}

/// Windows Defender reports every crypto miner (XMRig, SRBMiner) as "CoinMiner" and
/// quarantines it. Only after the user's explicit consent in the app: exclude Erebor's
/// miners folder from scanning. Windows asks for administrator approval (UAC).
pub fn allow_in_defender(miners_dir: &Path) -> Result<(), String> {
    use base64::Engine as _;
    if !cfg!(windows) {
        return Err("windows_only".into());
    }
    std::fs::create_dir_all(miners_dir).map_err(|e| e.to_string())?;
    let dir = miners_dir.to_string_lossy().replace('\'', "''");
    let inner = format!("Add-MpPreference -ExclusionPath '{dir}'");
    let utf16: Vec<u8> = inner.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let enc = base64::engine::general_purpose::STANDARD.encode(utf16);
    let outer = format!(
        "$p = Start-Process powershell -Verb RunAs -Wait -PassThru -WindowStyle Hidden \
         -ArgumentList '-NoProfile','-EncodedCommand','{enc}'; exit $p.ExitCode"
    );
    let out = crate::command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &outer])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    Err(if err.contains("cancel") {
        "cancelled".into()
    } else {
        format!("defender_failed: {}", err.trim())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_manifest_parses_and_is_official() {
        for kind in MinerKind::ALL {
            let m = manifest_for(kind);
            for (k, a) in &m.assets {
                assert!(a.url.starts_with(official_prefix(kind)), "{k}");
                assert_eq!(a.sha256.len(), 64, "{k}");
                let v = m.version.replace('.', "-");
                assert!(a.url.contains(&m.version) || a.url.contains(&v), "{k}");
            }
        }
        assert!(manifest_for(MinerKind::Srbminer)
            .assets
            .contains_key("linux-x86_64"));
    }

    #[test]
    fn rejects_unofficial_urls() {
        let mut m = Manifest {
            kind: MinerKind::Xmrig,
            version: "1.0.0".into(),
            assets: HashMap::new(),
        };
        m.assets.insert(
            platform_key(),
            Asset {
                url: "https://evil.example/xmrig.tar.gz".into(),
                sha256: "0".repeat(64),
            },
        );
        assert!(m.asset_for_platform().is_err());
    }

    #[test]
    fn sha256_verification() {
        // sha256("abc")
        let h = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(verify_sha256(b"abc", h).is_ok());
        assert!(verify_sha256(b"abd", h).is_err());
    }

    #[test]
    fn extracts_only_the_binary_from_tar_gz() {
        let mut tar_bytes = Vec::new();
        {
            let gz = flate2::write::GzEncoder::new(&mut tar_bytes, flate2::Compression::fast());
            let mut b = tar::Builder::new(gz);
            for (name, body) in [
                ("xmrig-6.0.0/config.json", &b"{}"[..]),
                (
                    &format!("xmrig-6.0.0/{}", crate::xmrig::binary_name())[..],
                    &b"BINARY"[..],
                ),
            ] {
                let mut h = tar::Header::new_gnu();
                h.set_size(body.len() as u64);
                h.set_mode(0o644);
                h.set_cksum();
                b.append_data(&mut h, name, body).unwrap();
            }
            b.into_inner().unwrap().finish().unwrap();
        }
        let dir = tempfile::tempdir().unwrap();
        let p = extract_binary(&tar_bytes, "x.tar.gz", dir.path()).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"BINARY");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn extracts_only_the_binary_from_zip() {
        use std::io::Write;
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default();
            z.start_file("xmrig-6.0.0/WinRing0x64.sys", o).unwrap();
            z.write_all(b"driver").unwrap();
            z.start_file(format!("xmrig-6.0.0/{}", crate::xmrig::binary_name()), o)
                .unwrap();
            z.write_all(b"EXE").unwrap();
            z.finish().unwrap();
        }
        let dir = tempfile::tempdir().unwrap();
        let p = extract_binary(buf.get_ref(), "x.zip", dir.path()).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"EXE");
    }
}

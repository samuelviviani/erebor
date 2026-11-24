//! Self-update of the `erebor` terminal binary from signed GitHub releases.
//!
//! Same source and key as the desktop app's updater: the release's `latest.json` says
//! which version is current, the binary for this OS/CPU is downloaded next to it and
//! installed only if its minisign signature matches the project's public key.

use base64::Engine as _;
use minisign_verify::{PublicKey, Signature};
use std::path::Path;

const REPO: &str = "samuelviviani/erebor";
/// The updater public key of `apps/desktop/src-tauri/tauri.conf.json`, decoded.
const PUBLIC_KEY: &str = "RWSdKo7ff9UNAS3NRoFf+5WIdVPynFW4bsFMMyym+L5bWGEHF1BACK8u";

/// Release asset for this platform, e.g. `erebor-linux-x86_64`.
fn asset_name() -> String {
    format!("erebor-{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// `true` when `latest` (e.g. "0.2.0" or "v0.2.0") is newer than `current`.
fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.trim_start_matches('v')
            .split(['.', '-', '+'])
            .take(3)
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    parse(latest) > parse(current)
}

/// Version of the latest published release (not drafts or pre-releases).
fn latest_version() -> Result<String, String> {
    let url = format!("https://github.com/{REPO}/releases/latest/download/latest.json");
    let body = crate::install::download(&url, &|_, _| {})
        .map_err(|e| format!("update_check_failed: {e}"))?;
    let v: serde_json::Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    v["version"]
        .as_str()
        .map(|s| s.trim_start_matches('v').to_string())
        .ok_or_else(|| "latest.json without version".into())
}

/// Check the signature of `bin` against a Tauri-style `.sig` file (base64 of the
/// minisign signature).
pub fn verify(bin: &[u8], sig_file: &[u8]) -> Result<(), String> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let text = b64
        .decode(String::from_utf8_lossy(sig_file).trim())
        .map_err(|_| "update_bad_signature")?;
    let sig =
        Signature::decode(&String::from_utf8_lossy(&text)).map_err(|_| "update_bad_signature")?;
    PublicKey::from_base64(PUBLIC_KEY)
        .and_then(|k| k.verify(bin, &sig, false))
        .map_err(|_| "update_bad_signature".to_string())
}

/// Download `version`, verify it and atomically replace the executable at `exe`.
pub fn install(version: &str, exe: &Path) -> Result<(), String> {
    let base = format!(
        "https://github.com/{REPO}/releases/download/v{version}/{}",
        asset_name()
    );
    let bin = crate::install::download(&base, &|_, _| {})?;
    // Since 0.1.6 the signatures are in the release's latest.json (`cli`), so the
    // release does not need a `.sig` file per binary; older releases only have those.
    let listed = crate::install::download(
        &format!("https://github.com/{REPO}/releases/download/v{version}/latest.json"),
        &|_, _| {},
    )
    .ok()
    .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
    .and_then(|v| {
        v["cli"][asset_name()]
            .as_str()
            .map(|s| s.as_bytes().to_vec())
    });
    let sig = match listed {
        Some(s) => s,
        None => crate::install::download(&format!("{base}.sig"), &|_, _| {})?,
    };
    verify(&bin, &sig)?;
    // Same folder, then rename: the old file is never half-written, and a running
    // process keeps executing the old inode until it restarts.
    let tmp = exe.with_extension("update");
    std::fs::write(&tmp, &bin).map_err(|e| format!("update_write: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, exe).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("update_write: {e}")
    })
}

/// Update `exe` when a newer release exists. `Ok(Some(version))` when it was installed.
pub fn update(exe: &Path) -> Result<Option<String>, String> {
    let latest = latest_version()?;
    if !is_newer(&latest, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }
    install(&latest, exe)?;
    Ok(Some(latest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(is_newer("v0.2.0", "0.1.9"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
        assert!(!is_newer("garbage", "0.1.0"));
    }

    #[test]
    fn key_matches_the_desktop_updater() {
        let conf: serde_json::Value = serde_json::from_str(include_str!(
            "../../../apps/desktop/src-tauri/tauri.conf.json"
        ))
        .unwrap();
        let b64 = conf["plugins"]["updater"]["pubkey"].as_str().unwrap();
        let text = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap();
        assert!(String::from_utf8(text).unwrap().contains(PUBLIC_KEY));
    }

    #[test]
    fn signature_is_checked() {
        // Signed with the project key: `tauri signer sign` of "erebor test binary\n".
        let sig = b"dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVTZEtvN2ZmOVVOQWNSOFRNTGdkMU5ZaXpwa0JlUnc5VHpMdy9ieHBySVZJTllkM3J1dE11SnMrQnpYM21YMFBueHVzdXViUUkvb0d6ZUdZY1pqekYvWEJxcFFTWTJLUWdFPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzk1NDQ0NTYzCWZpbGU6YmxvYgpqbVVEcGdvMlRNbHFpRHJKWGl6OGJuMWJLY0VFWHFzYUdLSWhBaDQ3QjJQMzZsWVRlb0VpNkRlTUtpZnc1d3VoM0ljSDJoMXo5MXlQbjNZR3FwUERCUT09Cg==";
        assert_eq!(verify(b"erebor test binary\n", sig), Ok(()));
        assert!(verify(b"erebor test binary!\n", sig).is_err());
        assert!(verify(b"erebor test binary\n", b"not a signature").is_err());
    }
}

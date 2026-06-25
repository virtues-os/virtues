//! Cross-platform OS keystore for the PairingBundle and the WG private key.
//!
//! Uses the `keyring` crate which talks to:
//!
//! - **macOS**   — Keychain (`SecKeychain`)
//! - **Linux**   — Secret Service / `libsecret` (KWallet / GNOME Keyring)
//! - **Windows** — Credential Manager
//!
//! Two entries are stored under service `virtues-client`:
//!
//! - account `default-box` — the [`virtues_protocol::PairingBundle`] as JSON
//! - account `default-box-wg-private` — the WG static private key, base64
//!
//! Splitting them means tooling that only needs to read the bundle (e.g. a
//! future "status" inspector) can do so without unlocking the private key.

use anyhow::{Context, Result};
use virtues_protocol::PairingBundle;

const SERVICE: &str = "virtues-client";
/// Single-box-per-machine for v0.2. When multi-box support lands this becomes
/// per-box-keyed so multiple bundles can coexist on one device.
const ACCOUNT_BUNDLE: &str = "default-box";
const ACCOUNT_WG_PRIVATE: &str = "default-box-wg-private";
/// Legacy entry written by an earlier build (stored the device id, which the
/// revoke endpoint doesn't accept). No longer written; kept only so
/// [`delete_bundle`] can clean it up on machines that still have it.
const ACCOUNT_DEVICE_ID: &str = "default-box-device-id";
/// The server credential row id. This — NOT the device id — is what
/// `DELETE /api/credentials/:id` matches on, so revoke needs it.
const ACCOUNT_CREDENTIAL_ID: &str = "default-box-credential-id";
/// The box's SPKI fingerprint (`sha256-<base64>`) pinned at first pair. TOFU:
/// a subsequent pair against a different server key is rejected (the box was
/// swapped, or a MITM). Mirrors the iOS `loadServerPin`/`saveServerPin`.
const ACCOUNT_SERVER_PIN: &str = "default-box-server-pin";

fn entry(account: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, account).context("open keyring entry")
}

/// `~/.virtues` — the on-disk home for the bundle + WG-key file fallbacks.
fn virtues_dir() -> Result<std::path::PathBuf> {
    let home = std::env::var("HOME").context("$HOME not set")?;
    Ok(std::path::PathBuf::from(home).join(".virtues"))
}

/// `~/.virtues/wg-private.key` — the authoritative WG private key store.
fn wg_private_file() -> Result<std::path::PathBuf> {
    Ok(virtues_dir()?.join("wg-private.key"))
}

/// Atomic 0600 write (temp file → rename), mirroring `pair::write_daemon_bundle`.
fn write_secret_file(path: &std::path::Path, contents: &str) -> Result<()> {
    let dir = path.parent().context("secret file has no parent dir")?;
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let tmp = path.with_extension("tmp");
    {
        use std::io::Write as _;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts
            .open(&tmp)
            .with_context(|| format!("create {}", tmp.display()))?;
        f.write_all(contents.as_bytes())
            .with_context(|| format!("write {}", tmp.display()))?;
        f.sync_all().ok();
    }
    std::fs::rename(&tmp, path)
        .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

// ────────────────────────────────────────────────────────────────────────
// Pairing bundle
// ────────────────────────────────────────────────────────────────────────

pub fn save_bundle(bundle: &PairingBundle) -> Result<()> {
    let json = serde_json::to_string(bundle).context("serialize bundle")?;
    entry(ACCOUNT_BUNDLE)?
        .set_password(&json)
        .context("write bundle to keyring")?;
    Ok(())
}

pub fn load_bundle() -> Result<Option<PairingBundle>> {
    match entry(ACCOUNT_BUNDLE)?.get_password() {
        Ok(json) => {
            let bundle: PairingBundle =
                serde_json::from_str(&json).context("decode stored bundle")?;
            Ok(Some(bundle))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(anyhow::Error::new(e).context("read bundle from keyring")),
    }
}

// ────────────────────────────────────────────────────────────────────────
// WG private key
// ────────────────────────────────────────────────────────────────────────

/// Persist the WG private key. The authoritative store is a 0600 file
/// (`~/.virtues/wg-private.key`); the keychain write is best-effort on top.
///
/// Why a file at all: `keyring` v3 on macOS targets the data-protection
/// keychain, which silently NO-OPS for a Developer-ID binary without a
/// `keychain-access-groups` entitlement — `set_password` returns Ok but the key
/// never persists, so the tunnel can never read it and falls back to a direct
/// upstream forever. The bundle already uses this exact file fallback, and the
/// bearer it holds is equally sensitive, so the key on disk at 0600 doesn't
/// widen the threat model. (Cross-platform bonus: Linux/Windows hit the same
/// keyring quirks; the file works everywhere.) Keychain stays a best-effort
/// primary so an entitled build would still prefer it on read.
pub fn save_wg_private(b64: &str) -> Result<()> {
    write_secret_file(&wg_private_file()?, b64).context("write WG private file")?;
    let _ = entry(ACCOUNT_WG_PRIVATE).and_then(|e| {
        e.set_password(b64)
            .map_err(|e| anyhow::Error::new(e).context("write WG private to keyring"))
    });
    Ok(())
}

pub fn load_wg_private() -> Result<Option<String>> {
    // Keychain primary (if entitled / working), else the 0600 file fallback.
    // Any keychain hiccup falls through to the file rather than erroring.
    if let Ok(entry) = entry(ACCOUNT_WG_PRIVATE) {
        if let Ok(s) = entry.get_password() {
            return Ok(Some(s));
        }
    }
    match std::fs::read_to_string(wg_private_file()?) {
        Ok(s) => Ok(Some(s.trim().to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow::Error::new(e).context("read WG private file")),
    }
}

// ────────────────────────────────────────────────────────────────────────
// Credential ID (the revocable credential row id, for revoke)
// ────────────────────────────────────────────────────────────────────────

pub fn save_credential_id(id: &str) -> Result<()> {
    entry(ACCOUNT_CREDENTIAL_ID)?
        .set_password(id)
        .context("write credential ID to keyring")?;
    Ok(())
}

pub fn load_credential_id() -> Result<Option<String>> {
    match entry(ACCOUNT_CREDENTIAL_ID)?.get_password() {
        Ok(s) => Ok(Some(s)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(anyhow::Error::new(e).context("read credential ID from keyring")),
    }
}

// ────────────────────────────────────────────────────────────────────────
// Server pin (TOFU SPKI fingerprint)
// ────────────────────────────────────────────────────────────────────────

pub fn save_server_pin(fpr: &str) -> Result<()> {
    entry(ACCOUNT_SERVER_PIN)?
        .set_password(fpr)
        .context("write server pin to keyring")?;
    Ok(())
}

pub fn load_server_pin() -> Result<Option<String>> {
    match entry(ACCOUNT_SERVER_PIN)?.get_password() {
        Ok(s) => Ok(Some(s)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(anyhow::Error::new(e).context("read server pin from keyring")),
    }
}

// ────────────────────────────────────────────────────────────────────────
// Delete (revoke)
// ────────────────────────────────────────────────────────────────────────

pub fn delete_bundle() -> Result<()> {
    // All best-effort — a revoke/reset must produce a clean slate even if the
    // keychain is flaky (the file fallbacks below are what actually held state).
    let _ = delete_entry(ACCOUNT_BUNDLE);
    let _ = delete_entry(ACCOUNT_WG_PRIVATE);
    let _ = delete_entry(ACCOUNT_DEVICE_ID);
    let _ = delete_entry(ACCOUNT_CREDENTIAL_ID);
    let _ = delete_entry(ACCOUNT_SERVER_PIN);
    // Also remove the on-disk fallbacks so reset is a TRUE clean slate, not just
    // the keychain. Without this, a stale bundle.json / wg-private.key would let
    // the next `up` keep using revoked creds.
    if let Ok(dir) = virtues_dir() {
        let _ = std::fs::remove_file(dir.join("bundle.json"));
        let _ = std::fs::remove_file(wg_private_file()?);
    }
    Ok(())
}

fn delete_entry(account: &str) -> Result<()> {
    match entry(account)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(anyhow::Error::new(e).context("delete from keyring")),
    }
}

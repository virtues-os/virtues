//! Cross-platform OS keystore for the paired-box record.
//!
//! Uses the `keyring` crate which talks to:
//!
//! - **macOS**   — Keychain (`SecKeychain`)
//! - **Linux**   — Secret Service / `libsecret` (KWallet / GNOME Keyring)
//! - **Windows** — Credential Manager
//!
//! In the relay model a vanilla browser reaches the box directly at its HTTPS
//! URL (the relay terminates nothing — the box holds a browser-trusted cert), so
//! the client no longer brings up a tunnel and holds no WG keys or SPKI pin. All
//! it persists is a small [`PairedBox`]: where the box is and the bearer that
//! authorizes it. Stored as JSON under service `virtues-client`, account
//! `default-box`, with a `~/.virtues/box.json` (0600) fallback for keychain
//! setups that silently no-op (macOS data-protection keychain without an
//! entitlement, headless Linux, etc.).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const SERVICE: &str = "virtues-client";
/// Single-box-per-machine for v1. When multi-box support lands this becomes
/// per-box-keyed so multiple records can coexist on one device.
const ACCOUNT_BOX: &str = "default-box";

/// Legacy accounts written by the WireGuard-era client. No longer written; kept
/// only so [`delete_box`] can clean them up on machines that still have them.
const LEGACY_ACCOUNTS: &[&str] = &[
    "default-box-wg-private",
    "default-box-device-id",
    "default-box-credential-id",
    "default-box-server-pin",
];

/// The persisted paired-box record. Everything the thin client needs to reach
/// and authorize the box.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedBox {
    /// Canonical browser-reachable URL, e.g. `https://a1b2c3.boxes.virtues.com`
    /// (the box's relay URL when configured, else the origin we paired against).
    pub box_url: String,
    /// Server-issued bearer that authorizes this device's API calls.
    pub bearer: String,
    /// The revocable credential row id — what `revoke` sends to
    /// `DELETE /api/credentials/:id`. Absent for pairings made before the box
    /// returned one.
    #[serde(default)]
    pub credential_id: Option<String>,
}

fn entry(account: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, account).context("open keyring entry")
}

/// `~/.virtues` — on-disk home for the record fallback.
fn virtues_dir() -> Result<std::path::PathBuf> {
    let home = std::env::var("HOME").context("$HOME not set")?;
    Ok(std::path::PathBuf::from(home).join(".virtues"))
}

/// `~/.virtues/box.json` — the file fallback for the paired-box record.
fn box_file() -> Result<std::path::PathBuf> {
    Ok(virtues_dir()?.join("box.json"))
}

/// Atomic 0600 write (temp file → rename) — no world-readable window.
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

/// Persist the paired-box record. The 0600 file is authoritative (it works
/// everywhere); the keychain write is best-effort on top, preferred on read by
/// an entitled build.
pub fn save_box(rec: &PairedBox) -> Result<()> {
    let json = serde_json::to_string(rec).context("serialize paired box")?;
    write_secret_file(&box_file()?, &json).context("write box.json")?;
    let _ = entry(ACCOUNT_BOX).and_then(|e| {
        e.set_password(&json)
            .map_err(|e| anyhow::Error::new(e).context("write box to keyring"))
    });
    Ok(())
}

/// Load the paired-box record: keychain primary (if entitled/working), else the
/// 0600 file fallback. Any keychain hiccup falls through to the file.
pub fn load_box() -> Result<Option<PairedBox>> {
    if let Ok(e) = entry(ACCOUNT_BOX) {
        if let Ok(json) = e.get_password() {
            if let Ok(rec) = serde_json::from_str::<PairedBox>(&json) {
                return Ok(Some(rec));
            }
        }
    }
    match std::fs::read_to_string(box_file()?) {
        Ok(json) => Ok(Some(
            serde_json::from_str(&json).context("decode box.json")?,
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow::Error::new(e).context("read box.json")),
    }
}

/// Clear all local creds — keychain entries (incl. legacy WG-era ones) and the
/// on-disk fallbacks — so revoke/reset is a true clean slate.
pub fn delete_box() -> Result<()> {
    let _ = delete_entry(ACCOUNT_BOX);
    for acct in LEGACY_ACCOUNTS {
        let _ = delete_entry(acct);
    }
    if let Ok(dir) = virtues_dir() {
        let _ = std::fs::remove_file(dir.join("box.json"));
        // Remove WG-era fallbacks too, so an old machine resets fully.
        let _ = std::fs::remove_file(dir.join("bundle.json"));
        let _ = std::fs::remove_file(dir.join("wg-private.key"));
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

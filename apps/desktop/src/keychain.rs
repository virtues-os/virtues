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
/// `box-<publish_id>` so multiple bundles can coexist on one device.
const ACCOUNT_BUNDLE: &str = "default-box";
const ACCOUNT_WG_PRIVATE: &str = "default-box-wg-private";
const ACCOUNT_DEVICE_ID: &str = "default-box-device-id";

fn entry(account: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, account).context("open keyring entry")
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

pub fn save_wg_private(b64: &str) -> Result<()> {
    entry(ACCOUNT_WG_PRIVATE)?
        .set_password(b64)
        .context("write WG private to keyring")?;
    Ok(())
}

pub fn load_wg_private() -> Result<Option<String>> {
    match entry(ACCOUNT_WG_PRIVATE)?.get_password() {
        Ok(s) => Ok(Some(s)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(anyhow::Error::new(e).context("read WG private from keyring")),
    }
}

// ────────────────────────────────────────────────────────────────────────
// Device ID (from server's credential row, used for revoke)
// ────────────────────────────────────────────────────────────────────────

pub fn save_device_id(id: &str) -> Result<()> {
    entry(ACCOUNT_DEVICE_ID)?
        .set_password(id)
        .context("write device ID to keyring")?;
    Ok(())
}

pub fn load_device_id() -> Result<Option<String>> {
    match entry(ACCOUNT_DEVICE_ID)?.get_password() {
        Ok(s) => Ok(Some(s)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(anyhow::Error::new(e).context("read device ID from keyring")),
    }
}

// ────────────────────────────────────────────────────────────────────────
// Delete (revoke)
// ────────────────────────────────────────────────────────────────────────

pub fn delete_bundle() -> Result<()> {
    delete_entry(ACCOUNT_BUNDLE)?;
    // Best-effort: also drop the WG private key and device ID. Don't error if absent.
    let _ = delete_entry(ACCOUNT_WG_PRIVATE);
    let _ = delete_entry(ACCOUNT_DEVICE_ID);
    Ok(())
}

fn delete_entry(account: &str) -> Result<()> {
    match entry(account)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(anyhow::Error::new(e).context("delete from keyring")),
    }
}

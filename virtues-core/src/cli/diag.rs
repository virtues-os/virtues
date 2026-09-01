//! Shared diagnostic helpers for `virtues report-crash` and `virtues status
//! --json`.
//!
//! (It used to say "and the install-time beacon" too. There isn't one: atlas
//! routes `/diag/install`, but nothing on the box has ever posted to it — the
//! installer contains no beacon code and no `VIRTUES_DIAG` handling at all.)
//!
//! Two responsibilities:
//!
//! 1. **Opt-out gate.** `VIRTUES_DIAG=off` in the env file systemd's
//!    `EnvironmentFile=` loads — `/var/lib/virtues/virtues.env` on boxes the
//!    installer built, NOT `/etc/virtues/env`, which it has never written —
//!    or the process env, whichever is set. Disables every cloud beacon. The
//!    `enabled()` helper returns false; callers exit cleanly without
//!    sending anything. Default is on for v1; the install step prints a
//!    one-line notice so users see what's happening before they ever
//!    open the docs.
//!
//! 2. **POST to atlas.** A `send(path, payload)` helper with a tight
//!    timeout and best-effort semantics — diagnostic posts never error.
//!    If the network is down or atlas is unreachable, we log at info and
//!    move on. The crash beacon should NEVER make a crash worse.

use std::time::Duration;

use serde_json::Value;

/// Default atlas base URL. Honors `VIRTUES_ATLAS_URL` if set, falls back
/// to the production atlas. Localhost shows up in dev / when staging is
/// configured.
pub fn atlas_url() -> String {
    crate::virtues_api::atlas_url()
}

/// True when the user has not opted out of cloud diagnostics. We read
/// from the process env (which systemd's `EnvironmentFile=` populates).
/// "off", "false", "0", "no", and "disabled" all opt out — case-
/// insensitive — so the user can write whichever feels natural.
pub fn enabled() -> bool {
    match std::env::var("VIRTUES_DIAG") {
        Ok(v) => {
            let lower = v.trim().to_ascii_lowercase();
            !matches!(lower.as_str(), "off" | "false" | "0" | "no" | "disabled")
        }
        Err(_) => true,
    }
}

/// POST `payload` to `<atlas>/<path>` with a 5-second timeout, no retry.
/// Returns `Ok(())` on 2xx, `Err(_)` for any other outcome — but every
/// caller should swallow the error: a failed diagnostic post is never a
/// reason to disrupt the user's workflow or amplify a crash.
pub async fn send(path: &str, payload: &Value) -> Result<(), String> {
    if !enabled() {
        return Ok(());
    }
    let url = format!("{}{}", atlas_url().trim_end_matches('/'), path);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .user_agent(concat!("virtues-diag/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("build client: {e}"))?;
    let resp = client
        .post(&url)
        .json(payload)
        .send()
        .await
        .map_err(|e| format!("POST {url}: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("POST {url} returned {}", resp.status()));
    }
    Ok(())
}

/// Read a stable, anonymized box id from the env.
///
/// `VIRTUES_BOX_ID` wins if set — but **nothing sets it**: no installer, no
/// unit, no firstboot script writes it, so in practice every box falls through
/// to the hostname hash below. Left in place as the override hook it always
/// was; do not document it as the normal path.
///
/// Either shape is fine for diag — it's a per-box correlation key, not an
/// identity claim.
pub fn box_id() -> String {
    if let Ok(id) = std::env::var("VIRTUES_BOX_ID") {
        if !id.is_empty() {
            return id;
        }
    }
    // Stable fallback: SHA-256 of hostname, hex-encoded prefix. Avoids
    // emitting the raw hostname to atlas.
    use sha2::{Digest, Sha256};
    let host = std::env::var("HOSTNAME")
        .or_else(|_| {
            std::fs::read_to_string("/etc/hostname")
                .map(|s| s.trim().to_string())
                .map_err(|e| e.to_string())
        })
        .unwrap_or_else(|_| "unknown".to_string());
    let mut h = Sha256::new();
    h.update(host.as_bytes());
    format!("h:{}", &hex::encode(h.finalize())[..16])
}

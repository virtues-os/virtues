//! Authenticated network management — Settings → Box → Network.
//!
//! The feature a claimed box was missing, discovered the hard way on
//! 2026-08-11: an office box was onboarded onto a captive guest network, and
//! the moment a device paired, every setup surface correctly closed (the BLE
//! service stops, `/api/provision/*` 404s — setup is a phase, not a feature).
//! Which left NO way to move the box to a better network. It was marooned by
//! its own security posture, one network-quality tier below usable, with the
//! owner standing right there holding a paired phone.
//!
//! So: the same three verbs as provisioning — status, scan, join — behind the
//! normal auth middleware instead of the AP-subnet gate. A paired device is a
//! strictly stronger credential than "joined the setup network", so nothing
//! about the trust model shifts; the surface just stops evaporating at claim
//! time. The join plumbing IS `provision::perform_join_full` — one
//! implementation of the switchover, everywhere.
//!
//! **A successful join can still sever the connection it was requested over**
//! (single radio; and the new network may not even reach the old one's LAN).
//! Clients treat a dead socket exactly like the BLE flow does: not an error,
//! an instruction to go and look. Over the relay this doesn't apply — the box
//! re-registers and the same reach ticket keeps working — which is precisely
//! why post-pair wifi management composes with the reach architecture instead
//! of fighting it.

use axum::{extract::State, response::IntoResponse, Json};
use serde::Deserialize;

use crate::server::AppState;

/// `GET /api/network/status` — where the box stands, honestly.
///
/// `connectivity` is NetworkManager's word (`full` | `portal` | `limited` |
/// `none` | `unknown`), not an IP-presence guess — `portal` is exactly the
/// captive-network state that started all this, and the UI names it.
pub async fn status_handler(State(_state): State<AppState>) -> impl IntoResponse {
    // The shared active check ("check", not the passive read): someone is on
    // this screen precisely because they doubt the network, so a cached
    // verdict is the wrong one to show them.
    let connectivity = crate::cli::link::connectivity();

    Json(serde_json::json!({
        "connectivity": connectivity,
        "ssid": crate::api::provision::active_client_ssid().await,
        "ip": crate::cli::link::primary_ip().map(|i| i.to_string()),
    }))
}

/// `GET /api/network/scan` — what the box can see, same shape as setup's.
pub async fn scan_handler(State(_state): State<AppState>) -> impl IntoResponse {
    match crate::api::provision::scan_or_cached().await {
        Ok(nets) => Json(serde_json::json!({ "networks": nets })).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct JoinBody {
    pub ssid: String,
    #[serde(default)]
    pub psk: Option<String>,
    /// Present = 802.1X; `psk` is then the account password.
    #[serde(default)]
    pub identity: Option<String>,
}

/// `POST /api/network/join` — move the box to another network.
pub async fn join_handler(
    State(_state): State<AppState>,
    Json(body): Json<JoinBody>,
) -> impl IntoResponse {
    if body.ssid.trim().is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "ok": false, "detail": "ssid is required" })),
        )
            .into_response();
    }
    match crate::api::provision::perform_join_full(
        &body.ssid,
        body.psk.as_deref().filter(|p| !p.is_empty()),
        body.identity.as_deref().filter(|i| !i.is_empty()),
    )
    .await
    {
        None => Json(serde_json::json!({ "ok": true })).into_response(),
        Some(detail) => {
            // 200 with ok:false — same contract as provisioning: the failure
            // detail is NetworkManager's own words, and the HTTP layer worked.
            Json(serde_json::json!({ "ok": false, "detail": detail })).into_response()
        }
    }
}

/// `GET /api/network/relay` — the box's rendezvous, named out loud.
///
/// The relay is a baked default (see `relay::DEFAULT_RELAY_URL` for why that
/// is the normal shape and what it does/doesn't reveal), so the honest UI is
/// this reading plus a real off switch — disclosure is what separates a
/// default from a secret.
///
/// `relay_url` is what resolution currently decides (None = relay-less);
/// `homed` is whether the box is reachable as configured — bound, and actually
/// connected to the relay when one is configured. It goes false on its own if
/// the relay leg drops, so this is a live reading rather than a record that a
/// bind once succeeded; `enabled`
/// is the switch state (false only when the stored config carries the off
/// word — the env override is operator territory and reads as its value).
pub async fn relay_status_handler(State(state): State<AppState>) -> impl IntoResponse {
    let stored = crate::virtues_api::relay::load(state.db.pool())
        .await
        .ok()
        .flatten()
        .map(|c| c.relay_url);
    let disabled = stored.as_deref().is_some_and(|s| {
        matches!(s.to_ascii_lowercase().as_str(), "off" | "none" | "disabled")
    });
    Json(serde_json::json!({
        "enabled": !disabled,
        "relay_url": crate::relay::box_relay_url(),
        "default_url": crate::relay::DEFAULT_RELAY_URL,
        "homed": crate::relay::is_relay_registered(),
    }))
}

#[derive(Deserialize)]
pub struct RelayToggleBody {
    pub enabled: bool,
}

/// `PUT /api/network/relay` — the off switch.
///
/// Off stores the literal word "off" in the relay config slot (so the choice
/// survives upgrades and re-links — a stored value beats env and default in
/// resolution). On deletes the override: a linked box then re-fetches its
/// config from atlas, an unlinked one falls to the baked default. Either way
/// the reach loop rebinds immediately rather than at the next restart.
pub async fn relay_toggle_handler(
    State(state): State<AppState>,
    Json(body): Json<RelayToggleBody>,
) -> impl IntoResponse {
    let res = if body.enabled {
        crate::box_secrets::delete(state.db.pool(), crate::virtues_api::relay::BOX_SECRET_KEY).await
    } else {
        crate::virtues_api::relay::store(
            state.db.pool(),
            &crate::virtues_api::relay::RelayConfig { relay_url: "off".to_string() },
        )
        .await
    };
    if let Err(e) = res {
        tracing::warn!("relay toggle failed: {e:#}");
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "could not save the relay setting" })),
        )
            .into_response();
    }
    crate::relay::request_rebind();
    Json(serde_json::json!({ "ok": true, "enabled": body.enabled })).into_response()
}

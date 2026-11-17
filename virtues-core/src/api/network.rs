//! The relay switch (Settings → Box → Network): whether this server keeps a
//! rendezvous on the Virtues relay so devices can reach it away from home.

use axum::{extract::State, response::IntoResponse, Json};
use serde::Deserialize;

use crate::server::AppState;

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
/// word). `VIRTUES_RELAY_URL` is NOT reflected here and cannot be: resolution
/// consults it only when no stored config exists, so on any linked box the env
/// var is inert — see `relay::resolve_relay_url`.
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
        crate::secrets::delete(state.db.pool(), crate::secrets::Kind::Box, crate::virtues_api::relay::BOX_SECRET_KEY).await
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
    crate::api::settings_changes::record(
        state.db.pool(),
        "reach",
        "relay_enabled",
        &serde_json::json!(!body.enabled),
        &serde_json::json!(body.enabled),
    )
    .await;
    Json(serde_json::json!({ "ok": true, "enabled": body.enabled })).into_response()
}

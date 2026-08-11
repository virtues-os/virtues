//! `GET /api/box/identity` — who is this box, publicly.
//!
//! Exists for the two-boxes-on-one-LAN problem, seen live 2026-08-10: a prod
//! and a test box in one house rendered as two identical "Virtues box" chips
//! in the app, distinguishable only by IP. The subnet scan probes an endpoint
//! that proves *a* box exists without learning anything about it; this is the
//! endpoint it asks next.
//!
//! Deliberately public (LAN-readable, pre-auth) and deliberately tiny. The
//! name is a label the box already broadcasts in its AP SSID and BLE
//! advertisement — a stranger on the LAN learns nothing the airwaves don't
//! already say. `claimed` is already public via `/api/setup/state`. Version
//! and everything else stay off this surface; discovery needs a name and a
//! state, not a fingerprint.

use axum::{extract::State, response::IntoResponse, Json};

use crate::server::AppState;

pub async fn identity_handler(State(state): State<AppState>) -> impl IntoResponse {
    let name = crate::codename::box_codename();
    Json(serde_json::json!({
        // Kebab for machines ("quaint-tern"), label for humans ("Quaint Tern").
        "name": name,
        "label": crate::codename::pretty(&name),
        "claimed": crate::api::pair::paired_device_count(state.db.pool()).await > 0,
    }))
}

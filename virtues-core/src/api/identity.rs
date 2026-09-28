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
//!
//! `linked` is the same kind of fact and earns its place the same way: the
//! app's setup flow has to know whether the box still needs step 2, and the
//! alternative is asking the person to read their own hardware and report
//! back. It is one bit — "does an account key exist" — never the key, never
//! the account, never the code. A stranger learns that a box they can already
//! see is or isn't paying for relay reach, which changes nothing they could
//! do about it.

use axum::{extract::State, response::IntoResponse, Json};

use crate::server::AppState;

/// What people call this box, everywhere it is named for a person: its
/// screen, the app's list, Settings. "Virtues 4812" until it has an owner;
/// after, its assistant's: "Ari's server" (setup-plan.md, "one name": the
/// name belongs to the assistant, the hardware takes the possessive).
///
/// The radio name stays machine-shaped (`Virtues-4812`, `setup_ap::ap_ssid`):
/// a claimed box only advertises while it is offline, and an apostrophe has
/// no business in an SSID.
pub async fn box_label(pool: &sqlx::PgPool) -> String {
    if crate::api::pair::is_unclaimed(pool).await {
        return format!("Virtues {}", crate::codename::box_number());
    }
    match crate::api::assistant_profile::get_assistant_name(pool).await {
        Ok(name) if !name.trim().is_empty() => format!("{}'s server", name.trim()),
        _ => "Your server".into(),
    }
}

pub async fn identity_handler(State(state): State<AppState>) -> impl IntoResponse {
    let pool = state.db.pool();
    Json(serde_json::json!({
        // Machine-shaped ("virtues-4812") and for people ("Virtues 4812",
        // then "Ari's server"). Older apps read `label` as it is.
        "name": format!("virtues-{}", crate::codename::box_number()),
        "label": box_label(pool).await,
        // Fails CLOSED — a DB blip must not tell the LAN this box is unclaimed.
        "claimed": !crate::api::pair::is_unclaimed(pool).await,
        // Setup's step 2, as one bit — see the module docs.
        "linked": crate::virtues_api::renew::read_api_key(pool)
            .await
            .ok()
            .flatten()
            .is_some(),
        "online": crate::cli::link::has_internet(),
    }))
}

//! The box asking about itself: `POST /account/entitlement { api_key }`
//! → `{ linked: true, subscribed: bool }`.
//!
//! atlas has always known this — `is_entitled` is exact and `key_owner` already
//! separates "valid key, never paid" from "unknown key". What was missing was a
//! door the BOX could knock on: every existing answer was either session-authed
//! (`/account/session`, for the app's email OTP) or a side effect away
//! (minting a Stripe portal session to discover you have no subscription).
//!
//! Without it the box inferred entitlement from the one bit it had — "do I hold
//! an api_key" — which stopped meaning "subscribed" on 2026-08-31 when linking
//! became identity rather than billing (0017). A free account then read as
//! Active on its owner's screen, and the only button that would have told them
//! otherwise answered "try again".
//!
//! Read-only and Stripe-free: two indexed lookups, no external call, safe to
//! poll. Privacy: same wall as the other api_key doors — we resolve
//! api_key → account and answer one boolean about it, and learn nothing about
//! the box.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::routes::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/account/entitlement", post(entitlement))
}

#[derive(Debug, Deserialize)]
struct EntitlementBody {
    api_key: String,
}

async fn entitlement(
    State(state): State<AppState>,
    Json(body): Json<EntitlementBody>,
) -> axum::response::Response {
    let token_hash = sha256(body.api_key.as_bytes());

    let customer_id = match super::claim::key_owner(&state.pool, &token_hash[..]).await {
        Ok(super::claim::KeyOwner::Customer(cid)) => Some(cid),
        // Valid key, no Stripe customer behind it. This is the answer the door
        // exists for, so it is a 200 with `subscribed: false` — not the 402
        // the billing doors use, which the box would have to read as an error.
        Ok(super::claim::KeyOwner::FreeAccount) => None,
        Ok(super::claim::KeyOwner::Unknown) => {
            return err(
                StatusCode::UNAUTHORIZED,
                "invalid_api_key",
                "unknown api key",
            );
        }
        Err(e) => {
            tracing::warn!("entitlement key lookup failed: {e:#}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "lookup failed");
        }
    };

    // A read error is a 500, never `subscribed: false`. The box renders this
    // answer as the owner's standing, so a blipped query must not read as
    // "your subscription is gone" — the box holds its last known answer
    // instead (virtues-core/src/api/subscription.rs).
    let subscribed = match super::account::is_entitled(&state, customer_id.as_deref()).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("entitlement check failed: {e:#}");
            return err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "entitlement check failed",
            );
        }
    };

    (
        StatusCode::OK,
        Json(json!({ "linked": true, "subscribed": subscribed })),
    )
        .into_response()
}

fn err(status: StatusCode, code: &str, message: &str) -> axum::response::Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": message } })),
    )
        .into_response()
}

fn sha256(data: &[u8]) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().to_vec()
}

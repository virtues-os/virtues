//! The box asking about itself: `POST /account/entitlement { api_key }`
//! → `{ linked: true, subscribed: bool, email: string | null,
//!     renews_at: timestamp | null }`.
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
//! Read-only and Stripe-free: a few indexed lookups, no external call, safe to
//! poll. Privacy: same wall as the other api_key doors — we resolve
//! api_key → account and answer about it, and learn nothing about the box.
//! The email goes back to the key's holder so Billing can say which account
//! the subscription is on; checkout already pre-fills it for the same holder.

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

    // A valid key with no subscription behind it is the answer this door
    // exists for, so it is a 200 with `subscribed: false`, not the 402 the
    // billing doors use, which the box would have to read as an error.
    let account_id = match super::claim::account_id_by_key_hash(&state.pool, &token_hash[..]).await {
        Ok(Some(id)) => id,
        Ok(None) => {
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
    // instead (virtues-core/src/api/subscription.rs). Entitlement is read
    // per account, the same way checkout reads it.
    let subscribed = match super::account::account_is_entitled(&state.pool, &account_id).await {
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

    // A failed email read is logged and sent as null. The email is a label,
    // and the subscription answer above must still reach the box.
    let email: Option<String> = match sqlx::query_scalar("SELECT email FROM accounts WHERE account_id = $1")
        .bind(&account_id)
        .fetch_optional(&state.pool)
        .await
    {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("entitlement email lookup failed: {e:#}");
            None
        }
    };

    // When Stripe bills next, which is also when the monthly allotment resets.
    // A label like the email: a failed read is logged and sent as null.
    let renews_at = match super::account::account_renews_at(&state.pool, &account_id).await {
        Ok(at) => at,
        Err(e) => {
            tracing::warn!("entitlement renewal date lookup failed: {e:#}");
            None
        }
    };

    (
        StatusCode::OK,
        Json(json!({
            "linked": true,
            "subscribed": subscribed,
            "email": email,
            "renews_at": renews_at,
        })),
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

//! The box buying a subscription for the account it is already linked to:
//! `POST /billing/checkout/sessions { api_key }` → `{ url }`.
//!
//! The sibling of `billing_portal.rs`, for the state 0017 created and no door
//! served: a linked FREE account. Its owner had two ways to reach Stripe and
//! both were wrong — `/account/checkout` wants an OTP session the box does
//! not hold, and starting a fresh device link would mint a SECOND account
//! rather than subscribe this one. So Settings showed "Active" to someone with
//! no subscription and, once that was fixed, had nothing to offer them.
//!
//! This mints a Checkout session with the account's email pre-filled and
//! sends the browser through the existing `/account/checkout/done`, which
//! attaches by the email Stripe reports — the same finalizer the airlock's
//! checkout uses, so there is one definition of "paid" (`claim::verify_and_
//! claim_session`) and one attach.
//!
//! Privacy: same wall as the other api_key doors. We resolve api_key → account
//! to pre-fill an email the account already gave us, and learn nothing about
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
    Router::new().route("/billing/checkout/sessions", post(create_checkout_session))
}

#[derive(Debug, Deserialize)]
struct CheckoutBody {
    api_key: String,
}

async fn create_checkout_session(
    State(state): State<AppState>,
    Json(body): Json<CheckoutBody>,
) -> axum::response::Response {
    let token_hash = sha256(body.api_key.as_bytes());

    // Who is buying. A Customer may already be entitled (nothing to sell —
    // say so, never double-charge); a FreeAccount is the expected caller.
    let customer_id = match super::claim::key_owner(&state.pool, &token_hash[..]).await {
        Ok(super::claim::KeyOwner::Customer(cid)) => Some(cid),
        Ok(super::claim::KeyOwner::FreeAccount) => None,
        Ok(super::claim::KeyOwner::Unknown) => {
            return err(StatusCode::UNAUTHORIZED, "invalid_api_key", "unknown api key");
        }
        Err(e) => {
            tracing::warn!("checkout key lookup failed: {e:#}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "lookup failed");
        }
    };
    match super::account::is_entitled(&state, customer_id.as_deref()).await {
        Ok(true) => {
            return (StatusCode::OK, Json(json!({ "entitled": true }))).into_response();
        }
        Ok(false) => {}
        // Not "treat as unentitled": that sends a paying owner to pay again.
        Err(e) => {
            tracing::warn!("checkout entitlement check failed: {e:#}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "entitlement check failed");
        }
    }

    // The email the account signed in with, to pre-fill. Its absence is a
    // contract break (every key hangs off an account since 0017), not a
    // reason to open an anonymous checkout that would fork the account.
    let email: Option<String> = match super::claim::account_id_by_key_hash(&state.pool, &token_hash[..]).await {
        Ok(Some(account_id)) => {
            match sqlx::query_scalar::<_, String>("SELECT email FROM accounts WHERE account_id = $1")
                .bind(&account_id)
                .fetch_optional(&state.pool)
                .await
            {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("checkout account email lookup failed: {e:#}");
                    return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "account lookup failed");
                }
            }
        }
        Ok(None) => None,
        Err(e) => {
            tracing::warn!("checkout account lookup failed: {e:#}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "account lookup failed");
        }
    };
    let Some(email) = email else {
        tracing::warn!("checkout: valid key with no account email");
        return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "account has no email");
    };

    if !state.stripe.is_configured() || state.stripe_price_id.is_empty() {
        return err(StatusCode::SERVICE_UNAVAILABLE, "stripe_not_configured", "billing isn't configured");
    }
    let base = state.public_url.trim_end_matches('/');
    let success_url = format!("{base}/account/checkout/done?session_id={{CHECKOUT_SESSION_ID}}");
    let cancel_url = format!("{base}/account/checkout/done?session_id=cancelled");
    match state
        .stripe
        .create_checkout_session_for(
            &state.stripe_price_id,
            &success_url,
            &cancel_url,
            // No user_code: this checkout belongs to an ACCOUNT, not a link.
            "",
            state.allow_promotion_codes,
            Some(&email),
        )
        .await
    {
        Ok(session) => (StatusCode::OK, Json(json!({ "url": session.url }))).into_response(),
        Err(e) => {
            tracing::warn!("api_key checkout create failed: {e:#}");
            err(StatusCode::BAD_GATEWAY, "stripe_error", "couldn't open checkout")
        }
    }
}

fn err(status: StatusCode, code: &str, message: &str) -> axum::response::Response {
    (status, Json(json!({ "error": { "code": code, "message": message } }))).into_response()
}

fn sha256(data: &[u8]) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().to_vec()
}

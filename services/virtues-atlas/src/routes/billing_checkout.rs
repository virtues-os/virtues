//! The box buying a subscription for the account it is already linked to:
//! `POST /billing/checkout/sessions { api_key }` → `{ url }`.
//!
//! The sibling of `billing_portal.rs`, for the state 0017 created and no door
//! served: a linked FREE account. Its owner had two ways to reach Stripe and
//! both were wrong — the app's checkout wanted an OTP session the box does
//! not hold, and starting a fresh device link would mint a SECOND account
//! rather than subscribe this one. So Settings showed "Active" to someone with
//! no subscription and, once that was fixed, had nothing to offer them.
//!
//! This mints a Checkout session bound to the key's account: the account id
//! goes into the session's metadata, and the account's Stripe customer is
//! reused when it has one (otherwise its email is pre-filled). The browser
//! then goes through `/account/checkout/done`, which attaches the
//! subscription to that stamped account. Nothing the payer types at Stripe
//! chooses the account.
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

    // An unknown key buys nothing. Entitlement is read per account below.
    match super::claim::key_owner(&state.pool, &token_hash[..]).await {
        Ok(super::claim::KeyOwner::Customer(_) | super::claim::KeyOwner::FreeAccount) => {}
        Ok(super::claim::KeyOwner::Unknown) => {
            return err(StatusCode::UNAUTHORIZED, "invalid_api_key", "unknown api key");
        }
        Err(e) => {
            tracing::warn!("checkout key lookup failed: {e:#}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "lookup failed");
        }
    }

    // The account behind the key, which this checkout is bound to. Its
    // absence is a contract break (every key hangs off an account since
    // 0017), not a reason to open a checkout that is bound to nobody.
    let account_id = match super::claim::account_id_by_key_hash(&state.pool, &token_hash[..]).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            tracing::warn!("checkout: valid key with no account");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "key has no account");
        }
        Err(e) => {
            tracing::warn!("checkout account lookup failed: {e:#}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "account lookup failed");
        }
    };
    match open_account_checkout(&state, &account_id).await {
        Ok(AccountCheckout::Entitled) => {
            (StatusCode::OK, Json(json!({ "entitled": true }))).into_response()
        }
        Ok(AccountCheckout::Url(url)) => (StatusCode::OK, Json(json!({ "url": url }))).into_response(),
        Err(e) => err(e.status, e.code, e.message),
    }
}

/// What opening a checkout for an account found.
pub(crate) enum AccountCheckout {
    /// The account already has an active subscription: nothing to sell.
    Entitled,
    /// A Stripe Checkout URL bound to the account.
    Url(String),
}

pub(crate) struct CheckoutErr {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: &'static str,
}

/// Open a subscription checkout for an account the caller has already
/// authenticated (a box key, or a just-confirmed emailed link). The one
/// place a subscription checkout is created, so every door binds the
/// session to its account the same way and checks entitlement the same way.
pub(crate) async fn open_account_checkout(
    state: &AppState,
    account_id: &str,
) -> Result<AccountCheckout, CheckoutErr> {
    let internal = |message| CheckoutErr { status: StatusCode::INTERNAL_SERVER_ERROR, code: "internal", message };
    // Entitlement is any active subscription on any of the account's
    // customers. Reading one customer only would sell a second subscription
    // to an owner who re-subscribed under a new Stripe customer.
    match super::account::account_is_entitled(&state.pool, account_id).await {
        Ok(true) => return Ok(AccountCheckout::Entitled),
        Ok(false) => {}
        // Not "treat as unentitled": that sends a paying owner to pay again.
        Err(e) => {
            tracing::warn!("checkout entitlement check failed: {e:#}");
            return Err(internal("entitlement check failed"));
        }
    }
    let account: Option<(String, Option<String>)> = match sqlx::query_as(
        "SELECT email, stripe_customer_id FROM accounts WHERE account_id = $1",
    )
    .bind(account_id)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("checkout account row lookup failed: {e:#}");
            return Err(internal("account lookup failed"));
        }
    };
    let Some((email, stripe_customer_id)) = account else {
        tracing::warn!(account_id = %account_id, "checkout: account has no row");
        return Err(internal("account has no email"));
    };
    let customer = match stripe_customer_id.as_deref() {
        Some(cid) => crate::stripe_api::CheckoutCustomer::Existing(cid),
        None => crate::stripe_api::CheckoutCustomer::Email(&email),
    };

    if !state.stripe.is_configured() || state.stripe_price_id.is_empty() {
        return Err(CheckoutErr {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "stripe_not_configured",
            message: "billing isn't configured",
        });
    }
    let base = state.public_url.trim_end_matches('/');
    let success_url = format!("{base}/account/checkout/done?session_id={{CHECKOUT_SESSION_ID}}");
    let cancel_url = format!("{base}/account/checkout/done?session_id=cancelled");
    match state
        .stripe
        .create_checkout_session(
            &state.stripe_price_id,
            &success_url,
            &cancel_url,
            account_id,
            customer,
            state.allow_promotion_codes,
        )
        .await
    {
        Ok(session) => Ok(AccountCheckout::Url(session.url)),
        Err(e) => {
            tracing::warn!("account checkout create failed: {e:#}");
            Err(CheckoutErr { status: StatusCode::BAD_GATEWAY, code: "stripe_error", message: "couldn't open checkout" })
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

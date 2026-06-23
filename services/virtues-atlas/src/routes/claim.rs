//! Billing-token claim (customer-facing, once per signup).
//!
//! `POST /claim { session_id }`
//!
//! After Stripe Checkout, the browser is redirected to
//! `success_url?session_id=cs_xxx`. The home server posts that session
//! here. Atlas verifies the payment, creates the customer + subscription,
//! mints a stable **billing token**, and returns it. The home server
//! stores the billing token and uses it monthly to fetch vouchers.
//!
//! The billing token is the identity-side credential — it proves "I'm a
//! paying customer." It never reaches virtues-api and carries no usage
//! data. Re-claiming (e.g., a lost token) issues a fresh one, invalidating
//! the old; recovery is a billing-side concern, which is allowed.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use anyhow::Context as _;
use chrono::{TimeZone, Utc};
use rand::RngCore;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::routes::AppState;
use crate::virtues_api_client::{Credit, RegisterDevice};

pub fn router() -> Router<AppState> {
    Router::new().route("/claim", post(claim))
}

#[derive(Debug, Deserialize)]
struct ClaimBody {
    session_id: String,
}

async fn claim(State(state): State<AppState>, Json(body): Json<ClaimBody>) -> axum::response::Response {
    match finalize_paid_session(&state, &body.session_id).await {
        Ok(f) => (
            StatusCode::CREATED,
            Json(json!({
                "api_key": f.api_key,
                "current_period_end": f.period_end,
            })),
        )
            .into_response(),
        Err(e) => err(e.status, e.code, &e.message),
    }
}

/// The minted credential for a verified paid session.
pub(crate) struct Finalized {
    /// The device api_key the box stores + sends to the proxy.
    pub api_key: String,
    pub period_end: chrono::DateTime<Utc>,
    /// The session id we just consumed — for `/link/done` to match against the
    /// `device_link.stripe_session_id` it stamped, so a session for code A
    /// can't finalize the row for code B.
    pub session_id: String,
    /// The `metadata[user_code]` we stamped at create time, returned so
    /// `/link/done` can verify it matches the URL code (binding C2 fix).
    pub metadata_user_code: Option<String>,
}

/// A finalize failure, carrying the HTTP shape the caller should surface.
pub(crate) struct FinalizeErr {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

/// Verify a paid Stripe Checkout session, mint a fresh billing token, and
/// upsert the customer + subscription. Shared by `POST /claim` (success-URL
/// post-back) and the device-link completion handler — one place that turns a
/// paid session into a billing token, so the two paths can't drift.
pub(crate) async fn finalize_paid_session(
    state: &AppState,
    session_id: &str,
) -> Result<Finalized, FinalizeErr> {
    if !state.stripe.is_configured() {
        return Err(FinalizeErr {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "stripe_not_configured",
            message: "STRIPE_SECRET_KEY not set".to_string(),
        });
    }

    // ── Anti-replay (C1) ──
    // A `cs_*` id can be observed in browser URLs / logs / referrers. Without
    // this guard, every replay would mint a new billing_token AND rotate the
    // real owner's token via the customers UPSERT (silent account DoS). Claim
    // each session at most once; subsequent attempts return 409.
    let claimed = sqlx::query(
        "INSERT INTO claimed_sessions (stripe_session_id) VALUES ($1) \
         ON CONFLICT (stripe_session_id) DO NOTHING",
    )
    .bind(session_id)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("claimed_sessions insert failed: {e:#}");
        FinalizeErr {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal",
            message: "could not record claim".to_string(),
        }
    })?;
    if claimed.rows_affected() == 0 {
        return Err(FinalizeErr {
            status: StatusCode::CONFLICT,
            code: "session_already_claimed",
            message: "this checkout session was already used".to_string(),
        });
    }

    let session = state
        .stripe
        .retrieve_checkout_session(session_id)
        .await
        .map_err(|e| {
            tracing::warn!("stripe session retrieve failed: {e:#}");
            FinalizeErr {
                status: StatusCode::BAD_GATEWAY,
                code: "stripe_error",
                message: e.to_string(),
            }
        })?;

    // Prod: only `paid` settles. Staging (`ATLAS_ALLOW_PROMOTION_CODES=true`)
    // also accepts `no_payment_required` so a 100%-off coupon completes the
    // claim without a card charge. Gating on the same flag that exposes the
    // coupon field keeps the two halves consistent.
    let payment_ok = session.payment_status == "paid"
        || (state.allow_promotion_codes && session.payment_status == "no_payment_required");
    if !payment_ok {
        return Err(FinalizeErr {
            status: StatusCode::PAYMENT_REQUIRED,
            code: "payment_not_complete",
            message: format!("checkout payment_status = {}", session.payment_status),
        });
    }
    // Stripe says the session must be a *completed subscription* for OUR price.
    // Without these, a one-off `mode=payment` session, an `expired` session, or
    // a cheap-price-on-the-same-account session would all pass `paid` and yield
    // a full billing token. (C1 hardening.)
    if session.mode != "subscription" {
        return Err(FinalizeErr {
            status: StatusCode::BAD_REQUEST,
            code: "wrong_mode",
            message: format!("session.mode = {} (want subscription)", session.mode),
        });
    }
    if session.status != "complete" {
        return Err(FinalizeErr {
            status: StatusCode::BAD_REQUEST,
            code: "session_not_complete",
            message: format!("session.status = {}", session.status),
        });
    }
    if !state.stripe_price_id.is_empty() {
        let price_ok = session
            .line_items
            .as_ref()
            .map(|li| {
                li.data
                    .iter()
                    .any(|item| item.price.as_ref().map(|p| p.id == state.stripe_price_id).unwrap_or(false))
            })
            .unwrap_or(false);
        if !price_ok {
            return Err(FinalizeErr {
                status: StatusCode::BAD_REQUEST,
                code: "price_mismatch",
                message: "session was not for the configured price".to_string(),
            });
        }
    }

    let Some(stripe_customer_id) = session.customer.clone() else {
        return Err(FinalizeErr {
            status: StatusCode::BAD_REQUEST,
            code: "no_customer",
            message: "session has no customer".to_string(),
        });
    };
    let stripe_subscription_id = session.subscription.clone().unwrap_or_default();
    let email = session
        .customer_details
        .as_ref()
        .and_then(|d| d.email.clone())
        .unwrap_or_else(|| "unknown@unknown".to_string());

    // Period end from metadata if present, else 30d out (webhooks correct it).
    let period_end = session
        .metadata
        .get("current_period_end")
        .and_then(|v| v.as_str().and_then(|s| s.parse::<i64>().ok()).or_else(|| v.as_i64()))
        .and_then(|ts| Utc.timestamp_opt(ts, 0).single())
        .unwrap_or_else(|| Utc::now() + chrono::Duration::days(30));

    // Mint a fresh device api_key (the box's single credential).
    let api_key = random_token();
    let api_key_hash = sha256(api_key.as_bytes());

    let internal = |what: &str| FinalizeErr {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        code: "internal",
        message: what.to_string(),
    };

    // Upsert customer with the new api_key hash (rotate on re-claim). The
    // opaque `account_id` is assigned once and kept on conflict, so re-claiming
    // re-points the device to the SAME account — the wallet is preserved.
    let candidate_account_id = new_account_id();
    let (account_id, daily_cap_micros): (String, i64) = sqlx::query_as(
        r#"
        INSERT INTO customers (stripe_customer_id, email, api_key_hash, account_id)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (stripe_customer_id)
        DO UPDATE SET api_key_hash = $3, email = $2
        RETURNING account_id, daily_cap_micros
        "#,
    )
    .bind(&stripe_customer_id)
    .bind(&email)
    .bind(&api_key_hash[..])
    .bind(&candidate_account_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("claim upsert customer failed: {e:#}");
        internal("customer upsert failed")
    })?;

    // Upsert subscription.
    sqlx::query(
        r#"
        INSERT INTO subscriptions (stripe_subscription_id, stripe_customer_id, status, current_period_end)
        VALUES ($1, $2, 'active', $3)
        ON CONFLICT (stripe_subscription_id)
        DO UPDATE SET status = 'active', current_period_end = $3
        "#,
    )
    .bind(&stripe_subscription_id)
    .bind(&stripe_customer_id)
    .bind(period_end)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("claim upsert subscription failed: {e:#}");
        internal("subscription upsert failed")
    })?;

    // Register the device key with virtues-api and fund this period's wallet.
    // A fresh paid checkout funds the monthly allotment immediately ($15);
    // invoice.paid keeps it fresh monthly.
    //
    // CRITICAL: these are the last steps, and they sit downstream of the
    // already-committed anti-replay `claimed_sessions` row. If either fails
    // (transient virtues-api blip) we must RELEASE the claim — otherwise the
    // box gets a 500, never received the api_key, and its retry would hit
    // `session_already_claimed` forever (bricked checkout). Both calls are
    // idempotent (register replaces the account's key; credit `set` overwrites),
    // so re-running the whole finalize on the box's retry is safe.
    let provision = async {
        state
            .virtues_api
            .register_device(&RegisterDevice {
                api_key_hash: hex::encode(&api_key_hash),
                account_id: account_id.clone(),
                daily_cap_micros,
            })
            .await
            .context("register_device")?;
        state
            .virtues_api
            .credit(&Credit {
                account_id: account_id.clone(),
                amount_micros: state.voucher.renewal_micros,
                mode: "set",
                daily_cap_micros,
                reference: Some(format!("checkout:{session_id}")),
            })
            .await
            .context("initial credit")?;
        anyhow::Ok(())
    }
    .await;
    if let Err(e) = provision {
        tracing::warn!("provisioning failed, releasing claim for retry: {e:#}");
        let _ = sqlx::query("DELETE FROM claimed_sessions WHERE stripe_session_id = $1")
            .bind(session_id)
            .execute(&state.pool)
            .await;
        return Err(internal("provisioning failed — please retry"));
    }

    let metadata_user_code = session
        .metadata
        .get("user_code")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(Finalized {
        api_key,
        period_end,
        session_id: session_id.to_string(),
        metadata_user_code,
    })
}

/// A random 32-byte hex token (api_key / device_code shape).
pub(crate) fn random_token() -> String {
    let mut b = [0u8; 32];
    rand::rng().fill_bytes(&mut b);
    hex::encode(b)
}

/// A fresh opaque account id (`acct_<hex>`).
pub(crate) fn new_account_id() -> String {
    let mut b = [0u8; 16];
    rand::rng().fill_bytes(&mut b);
    format!("acct_{}", hex::encode(b))
}

pub(crate) fn sha256(data: &[u8]) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().to_vec()
}

fn err(status: StatusCode, code: &str, message: &str) -> axum::response::Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": message } })),
    )
        .into_response()
}

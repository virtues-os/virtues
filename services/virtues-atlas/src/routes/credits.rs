//! Credit top-up routes (linked prepaid model).
//!
//! Two device-facing endpoints, both authed via the device `api_key`:
//!
//! - `POST /credits/auto-topup` — box hit a 402 with `wallet_empty`. We
//!   charge the saved card $10 off-session, then credit the account's wallet
//!   in virtues-api (`/internal/credit` mode `add`).
//! - `POST /credits/topup { amount_micros }` — user explicitly tapped
//!   "Add credit" with a chosen amount in `$10–$50`. Same flow.
//!
//! Both routes enforce:
//!   1. Active subscription (no auto-charge if sub canceled/past_due).
//!   2. Monthly cap (`customers.monthly_cap_micros`): refuse if this month's
//!      total charges + new amount would exceed it.
//!   3. Amount band ($10 ≤ amount ≤ $50 for manual; fixed $10 for auto).
//!
//! Order matters: we charge the card, THEN credit the wallet keyed by the
//! opaque `account_id`. On a credit failure after a successful charge we 500
//! for manual reconciliation (the Stripe PaymentIntent id is the ledger ref).

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::routes::AppState;
use crate::stripe_api::OffSessionChargeError;
use crate::virtues_api_client::Credit;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/credits/auto-topup", post(auto_topup))
        .route("/credits/topup", post(manual_topup))
}

#[derive(Debug, Deserialize)]
struct AutoTopupBody {
    api_key: String,
}

#[derive(Debug, Deserialize)]
struct ManualTopupBody {
    api_key: String,
    amount_micros: i64,
}

/// Box-triggered auto-top-up. Fired when virtues-api returned 402 with
/// `wallet_empty`. Fixed amount (`state.credit.auto_topup_micros`,
/// default $10). On success, credits the wallet and returns `{ok}`.
async fn auto_topup(
    State(state): State<AppState>,
    Json(body): Json<AutoTopupBody>,
) -> axum::response::Response {
    let amount = state.credit.auto_topup_micros;
    let (customer_id, account_id) = match resolve_active_customer(&state, &body.api_key).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    if let Err(resp) = enforce_monthly_cap(&state, &customer_id, amount).await {
        return resp;
    }

    do_topup(&state, &customer_id, &account_id, amount, "Virtues auto top-up", true).await
}

/// User-initiated top-up from iOS. Amount must lie in
/// [`state.credit.topup_min_micros`, `state.credit.topup_max_micros`]
/// (defaults $10–$50). Same Stripe flow as auto-top-up.
async fn manual_topup(
    State(state): State<AppState>,
    Json(body): Json<ManualTopupBody>,
) -> axum::response::Response {
    let min = state.credit.topup_min_micros;
    let max = state.credit.topup_max_micros;
    if body.amount_micros < min || body.amount_micros > max {
        return err(
            StatusCode::BAD_REQUEST,
            "amount_out_of_band",
            &format!("amount_micros must be between {min} and {max}"),
        );
    }

    let (customer_id, account_id) = match resolve_active_customer(&state, &body.api_key).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    if let Err(resp) = enforce_monthly_cap(&state, &customer_id, body.amount_micros).await {
        return resp;
    }

    do_topup(&state, &customer_id, &account_id, body.amount_micros, "Virtues credit", false).await
}

/// Shared body: charge saved card off-session → on success, credit the
/// account's wallet in virtues-api + record month-spend.
async fn do_topup(
    state: &AppState,
    customer_id: &str,
    account_id: &str,
    amount_micros: i64,
    description: &str,
    is_auto: bool,
) -> axum::response::Response {
    let pi_id = match state
        .stripe
        .charge_off_session(customer_id, amount_micros, description)
        .await
    {
        Ok(pi) => pi,
        Err(OffSessionChargeError::StripeDeclined { code, message }) => {
            tracing::warn!(customer = customer_id, %code, "off-session charge declined");
            return (
                StatusCode::PAYMENT_REQUIRED,
                Json(json!({
                    "error": {
                        "code": "card_declined",
                        "stripe_code": code,
                        "message": message,
                    }
                })),
            )
                .into_response();
        }
        Err(OffSessionChargeError::AuthenticationRequired(pi)) => {
            // 3DS required — surface so iOS can prompt the user.
            return (
                StatusCode::PAYMENT_REQUIRED,
                Json(json!({
                    "error": {
                        "code": "authentication_required",
                        "payment_intent": pi,
                        "message": "card requires authentication; confirm in app",
                    }
                })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::warn!(error = %e, "off-session charge failed");
            return err(StatusCode::BAD_GATEWAY, "stripe_error", &e.to_string());
        }
    };

    // Credit the account's wallet (ADD). On failure after a successful charge,
    // log + 500 for manual reconciliation (money is in Stripe, not yet credited).
    if let Err(e) = state
        .virtues_api
        .credit(&Credit {
            account_id: account_id.to_string(),
            amount_micros,
            mode: "add",
            reference: Some(format!("topup:{pi_id}")),
        })
        .await
    {
        tracing::error!(
            customer = customer_id,
            payment_intent = pi_id,
            error = %e,
            "credit with virtues-api FAILED after Stripe charge — manual reconciliation needed"
        );
        return err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "credit_failed",
            "credit failed after charge; support has been notified",
        );
    }

    // Record the spend toward monthly cap + bump auto-topup timestamp.
    let _ = sqlx::query(
        r#"
        UPDATE customers
        SET monthly_charges_micros = monthly_charges_micros + $2,
            last_auto_topup_at = CASE WHEN $3 THEN now() ELSE last_auto_topup_at END
        WHERE stripe_customer_id = $1
        "#,
    )
    .bind(customer_id)
    .bind(amount_micros)
    .bind(is_auto)
    .execute(&state.pool)
    .await;

    (
        StatusCode::OK,
        Json(json!({ "ok": true, "amount_micros": amount_micros })),
    )
        .into_response()
}

/// Resolve an api_key → (active customer id, account id). Errors on unknown
/// key or inactive subscription. Shared with `routes::relay` (token minting).
pub(crate) async fn resolve_active_customer(
    state: &AppState,
    api_key: &str,
) -> Result<(String, String), axum::response::Response> {
    let key_hash = sha256(api_key.as_bytes());

    // Per-box keys first (box_key), legacy customers.api_key_hash as
    // fallback — the ONE shared lookup (claim.rs), so every key-authed door
    // moves together.
    let cid = super::claim::customer_id_by_key_hash(&state.pool, &key_hash[..])
        .await
        .map_err(|e| {
            tracing::warn!("key lookup failed: {e:#}");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "customer lookup failed",
            )
        })?;
    let Some(cid) = cid else {
        return Err(err(
            StatusCode::UNAUTHORIZED,
            "invalid_api_key",
            "unknown api key",
        ));
    };

    let row: Option<(String, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT c.stripe_customer_id, c.account_id,
               (SELECT s.status FROM subscriptions s
                WHERE s.stripe_customer_id = c.stripe_customer_id
                ORDER BY s.current_period_end DESC NULLS LAST
                LIMIT 1) AS sub_status
        FROM customers c
        WHERE c.stripe_customer_id = $1
        "#,
    )
    .bind(&cid)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("customer lookup failed: {e:#}");
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "customer lookup failed",
        )
    })?;

    let Some((customer_id, account_id, sub_status)) = row else {
        return Err(err(
            StatusCode::UNAUTHORIZED,
            "invalid_api_key",
            "unknown api key",
        ));
    };

    if sub_status.as_deref() != Some("active") {
        return Err(err(
            StatusCode::PAYMENT_REQUIRED,
            "subscription_inactive",
            "subscription is not active",
        ));
    }

    Ok((customer_id, account_id))
}

/// Enforce `customers.monthly_cap_micros`. Resets `monthly_charges_micros`
/// lazily at the cohort-aligned 1st-of-month UTC. Returns Err with a 402
/// response if the new amount would exceed the cap.
async fn enforce_monthly_cap(
    state: &AppState,
    customer_id: &str,
    amount_micros: i64,
) -> Result<(), axum::response::Response> {
    let now = Utc::now();

    // Lazy reset: if we're past `month_reset_at`, zero the rolling count
    // and advance the reset date to the next 1st-of-month.
    let _ = sqlx::query(
        r#"
        UPDATE customers
        SET monthly_charges_micros = 0,
            month_reset_at = date_trunc('month', $2::timestamptz + interval '1 month')
        WHERE stripe_customer_id = $1
          AND month_reset_at <= $2
        "#,
    )
    .bind(customer_id)
    .bind(now)
    .execute(&state.pool)
    .await;

    let row: Option<(i64, i64)> = sqlx::query_as(
        "SELECT monthly_cap_micros, monthly_charges_micros FROM customers WHERE stripe_customer_id = $1",
    )
    .bind(customer_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("monthly cap read failed: {e:#}");
        err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "cap check failed")
    })?;

    let Some((cap, spent)) = row else {
        // Customer disappeared between lookup and cap check — shouldn't
        // happen but be defensive.
        return Err(err(
            StatusCode::UNAUTHORIZED,
            "customer_not_found",
            "customer not found",
        ));
    };

    if spent.saturating_add(amount_micros) > cap {
        return Err((
            StatusCode::PAYMENT_REQUIRED,
            Json(json!({
                "error": {
                    "code": "monthly_cap_reached",
                    "monthly_cap_micros": cap,
                    "monthly_charges_micros": spent,
                    "message": "monthly spend cap reached — raise it in Settings to continue",
                }
            })),
        )
            .into_response());
    }

    Ok(())
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

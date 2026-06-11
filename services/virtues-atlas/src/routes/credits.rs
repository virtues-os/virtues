//! Credit top-up routes (v3, locked 2026-06-05).
//!
//! Two device-facing endpoints, both bearer-authed via `billing_token`:
//!
//! - `POST /credits/auto-topup` — box hit a 402 with `wallet_empty`. We
//!   charge the saved card $10 off-session, mint a top-up voucher,
//!   return the voucher_code so the box can redeem.
//! - `POST /credits/topup { amount_micros }` — user explicitly tapped
//!   "Add credit" in iOS Settings with a chosen amount in `$10–$50`.
//!   Same flow, just user-chosen amount.
//!
//! Both routes enforce:
//!   1. Active subscription (no auto-charge if sub canceled/past_due).
//!   2. Monthly cap (`customers.monthly_cap_micros`): refuse if this
//!      month's total charges + new amount would exceed it.
//!   3. Amount band ($10 ≤ amount ≤ $50 for manual; fixed $10 for auto).
//!
//! Privacy: same wall as sub renewal. We learn the customer paid $N at
//! time T. We do NOT learn what the box's wallet was used for. The
//! voucher hash bridges atlas's "charge customer" to virtues-api's
//! "credit bearer" — then is discarded by both sides (24h sweeper).

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use chrono::{Duration, Utc};
use rand::RngCore;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::routes::AppState;
use crate::stripe_api::OffSessionChargeError;
use crate::virtues_api_client::RegisterVoucher;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/credits/auto-topup", post(auto_topup))
        .route("/credits/topup", post(manual_topup))
}

#[derive(Debug, Deserialize)]
struct AutoTopupBody {
    billing_token: String,
}

#[derive(Debug, Deserialize)]
struct ManualTopupBody {
    billing_token: String,
    amount_micros: i64,
}

/// Box-triggered auto-top-up. Fired when virtues-api returned 402 with
/// `wallet_empty`. Fixed amount (`state.voucher.auto_topup_micros`,
/// default $10). On success, returns `{voucher_code}` for the box to
/// redeem.
async fn auto_topup(
    State(state): State<AppState>,
    Json(body): Json<AutoTopupBody>,
) -> axum::response::Response {
    let amount = state.voucher.auto_topup_micros;
    let (customer_id, _email) = match resolve_active_customer(&state, &body.billing_token).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    if let Err(resp) = enforce_monthly_cap(&state, &customer_id, amount).await {
        return resp;
    }

    do_topup(&state, &customer_id, amount, "Virtues auto top-up", true).await
}

/// User-initiated top-up from iOS. Amount must lie in
/// [`state.voucher.topup_min_micros`, `state.voucher.topup_max_micros`]
/// (defaults $10–$50). Same Stripe flow as auto-top-up.
async fn manual_topup(
    State(state): State<AppState>,
    Json(body): Json<ManualTopupBody>,
) -> axum::response::Response {
    let min = state.voucher.topup_min_micros;
    let max = state.voucher.topup_max_micros;
    if body.amount_micros < min || body.amount_micros > max {
        return err(
            StatusCode::BAD_REQUEST,
            "amount_out_of_band",
            &format!("amount_micros must be between {min} and {max}"),
        );
    }

    let (customer_id, _email) = match resolve_active_customer(&state, &body.billing_token).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    if let Err(resp) = enforce_monthly_cap(&state, &customer_id, body.amount_micros).await {
        return resp;
    }

    do_topup(&state, &customer_id, body.amount_micros, "Virtues credit", false).await
}

/// Shared body: charge saved card off-session → on success, mint voucher
/// + register with virtues-api + record month-spend + return voucher_code.
async fn do_topup(
    state: &AppState,
    customer_id: &str,
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

    // Mint voucher + register with virtues-api. Use the same code shape as
    // sub renewal: 32 random bytes hex-encoded, SHA-256 hash registered.
    let mut code_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut code_bytes);
    let voucher_code = hex::encode(code_bytes);
    let code_hash_hex = hex::encode(sha256(voucher_code.as_bytes()));
    let voucher_expires_at = Utc::now() + Duration::days(state.voucher.unredeemed_days);

    if let Err(e) = state
        .virtues_api
        .register_voucher(&RegisterVoucher {
            voucher_code_hash: code_hash_hex,
            amount_micros,
            is_renewal: false, // top-ups ADD to wallet, not overwrite
            voucher_expires_at,
        })
        .await
    {
        // Refund the Stripe charge? Risk window. For v1, log + 500 so
        // the operator can investigate. The voucher is unmilled — money
        // is in your Stripe balance and not yet credited to the user.
        // Manual refund + retry by support.
        tracing::error!(
            customer = customer_id,
            payment_intent = pi_id,
            error = %e,
            "voucher registration with virtues-api FAILED after Stripe charge — manual reconciliation needed"
        );
        return err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "voucher_register_failed",
            "voucher registration failed after charge; support has been notified",
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
        StatusCode::CREATED,
        Json(json!({
            "voucher_code": voucher_code,
            "amount_micros": amount_micros,
            "voucher_expires_at": voucher_expires_at,
        })),
    )
        .into_response()
}

/// Resolve a billing_token → active customer. Errors on unknown token or
/// inactive subscription.
async fn resolve_active_customer(
    state: &AppState,
    billing_token: &str,
) -> Result<(String, String), axum::response::Response> {
    let token_hash = sha256(billing_token.as_bytes());

    let row: Option<(String, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT c.stripe_customer_id, c.email,
               (SELECT s.status FROM subscriptions s
                WHERE s.stripe_customer_id = c.stripe_customer_id
                ORDER BY s.current_period_end DESC NULLS LAST
                LIMIT 1) AS sub_status
        FROM customers c
        WHERE c.billing_token_hash = $1
        "#,
    )
    .bind(&token_hash[..])
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

    let Some((customer_id, email, sub_status)) = row else {
        return Err(err(
            StatusCode::UNAUTHORIZED,
            "invalid_billing_token",
            "unknown billing token",
        ));
    };

    if sub_status.as_deref() != Some("active") {
        return Err(err(
            StatusCode::PAYMENT_REQUIRED,
            "subscription_inactive",
            "subscription is not active",
        ));
    }

    Ok((customer_id, email))
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

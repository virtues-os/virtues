//! Voucher minting (customer-facing, monthly).
//!
//! `POST /voucher { billing_token }`
//!
//! The home server presents its billing token. Atlas verifies the
//! subscription is active, mints a one-time voucher code, registers the
//! code's hash with virtues-api (carrying only the value — no customer,
//! no bearer), and returns the raw code. The home server then redeems it
//! at virtues-api onto its monthly bearer.
//!
//! Atlas stores NOTHING linking this voucher to the customer. It only
//! records `last_voucher_issued_at` to rate-limit minting. That timestamp
//! is customer-side state with no bearer link.

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
use crate::virtues_api_client::RegisterVoucher;

pub fn router() -> Router<AppState> {
    Router::new().route("/voucher", post(mint_voucher))
}

#[derive(Debug, Deserialize)]
struct VoucherBody {
    billing_token: String,
}

async fn mint_voucher(
    State(state): State<AppState>,
    Json(body): Json<VoucherBody>,
) -> axum::response::Response {
    let token_hash = sha256(body.billing_token.as_bytes());

    // Look up the customer by billing token + their active subscription.
    let row: Option<(String, Option<chrono::DateTime<Utc>>, Option<chrono::DateTime<Utc>>, Option<String>)> =
        sqlx::query_as(
            r#"
            SELECT c.stripe_customer_id, c.last_voucher_issued_at,
                   s.current_period_end, s.status
            FROM customers c
            LEFT JOIN subscriptions s ON s.stripe_customer_id = c.stripe_customer_id
            WHERE c.billing_token_hash = $1
            ORDER BY s.current_period_end DESC NULLS LAST
            LIMIT 1
            "#,
        )
        .bind(&token_hash[..])
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

    let Some((customer_id, last_issued, period_end, status)) = row else {
        return err(StatusCode::UNAUTHORIZED, "invalid_billing_token", "unknown billing token");
    };

    // Subscription must be active and not lapsed.
    let now = Utc::now();
    let active = status.as_deref() == Some("active")
        && period_end.map(|e| e > now).unwrap_or(false);
    if !active {
        return err(
            StatusCode::PAYMENT_REQUIRED,
            "subscription_inactive",
            "no active subscription for this billing token",
        );
    }

    // Anti-stacking: refuse if a voucher was issued too recently.
    if let Some(last) = last_issued {
        if now - last < Duration::days(state.voucher.min_interval_days) {
            return err(
                StatusCode::TOO_MANY_REQUESTS,
                "voucher_too_soon",
                "a voucher was issued recently; wait until near expiry",
            );
        }
    }

    // Mint a one-time voucher code.
    let mut code_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut code_bytes);
    let voucher_code = hex::encode(code_bytes);
    let code_hash_hex = hex::encode(sha256(voucher_code.as_bytes()));
    let voucher_expires_at = now + Duration::days(state.voucher.unredeemed_days);

    // Register with virtues-api (value only — no customer, no bearer).
    if let Err(e) = state
        .virtues_api
        .register_voucher(&RegisterVoucher {
            voucher_code_hash: code_hash_hex,
            amount_micros: state.voucher.renewal_micros,
            is_renewal: true,
            voucher_expires_at,
        })
        .await
    {
        tracing::warn!("voucher registration with virtues-api failed: {e:#}");
        return err(StatusCode::BAD_GATEWAY, "register_failed", &e.to_string());
    }

    // Record issuance time (rate-limit state; no bearer link).
    let _ = sqlx::query("UPDATE customers SET last_voucher_issued_at = now() WHERE stripe_customer_id = $1")
        .bind(&customer_id)
        .execute(&state.pool)
        .await;

    (
        StatusCode::CREATED,
        Json(json!({
            "voucher_code": voucher_code,
            "amount_micros": state.voucher.renewal_micros,
            "voucher_expires_at": voucher_expires_at,
        })),
    )
        .into_response()
}

fn sha256(data: &[u8]) -> Vec<u8> {
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

//! User-tunable settings (v3, locked 2026-06-05).
//!
//! `GET /settings`  → current caps + auto-topup toggle
//! `PUT /settings`  → update caps + auto-topup toggle
//!
//! Both authed via the box's `api_key`. iOS Settings is the. iOS Settings is the
//! primary consumer — pulls current state on open, writes back on change.
//!
//! The monthly cap is atlas-side because:
//!   1. atlas owns the customer record + Stripe relationship
//!   2. monthly cap enforcement requires the customer ledger which lives here
//!      (it bounds top-ups, the Cursor-style spend ceiling — there is no
//!      per-day wall)
//!
//! ## Bounds
//!
//! - `monthly_cap_micros`: $100 (100_000_000) ≤ x ≤ $1000 (1_000_000_000),
//!   default $200
//! - `auto_topup_enabled`: bool

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, put},
    Router,
};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::routes::AppState;

const MONTHLY_CAP_MIN: i64 = 100_000_000; // $100
const MONTHLY_CAP_MAX: i64 = 1_000_000_000; // $1000

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/settings", get(get_settings))
        .route("/settings", put(put_settings))
}

#[derive(Debug, Deserialize)]
struct AuthBody {
    api_key: String,
}

#[derive(Debug, Deserialize)]
struct SettingsUpdate {
    api_key: String,
    monthly_cap_micros: Option<i64>,
    auto_topup_enabled: Option<bool>,
}

async fn get_settings(
    State(state): State<AppState>,
    Json(body): Json<AuthBody>,
) -> axum::response::Response {
    let token_hash = sha256(body.api_key.as_bytes());

    // Per-box keys first, legacy fallback — via the shared lookup (claim.rs).
    // A DB error is 500, never 401: telling a box its key is dead because a
    // query blipped is the wrong lie (review finding, 2026-08-24).
    let cid = match super::claim::customer_id_by_key_hash(&state.pool, &token_hash[..]).await {
        Ok(Some(cid)) => cid,
        Ok(None) => {
            return err(
                StatusCode::UNAUTHORIZED,
                "invalid_api_key",
                "unknown api key",
            );
        }
        Err(e) => {
            tracing::warn!("key lookup failed: {e:#}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "customer lookup failed");
        }
    };

    let row: Option<(i64, bool, i64, i64)> = sqlx::query_as(
        r#"
        SELECT monthly_cap_micros, auto_topup_enabled,
               monthly_charges_micros, COALESCE(EXTRACT(EPOCH FROM month_reset_at)::bigint, 0)
        FROM customers
        WHERE stripe_customer_id = $1
        "#,
    )
    .bind(&cid)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();

    let Some((monthly_cap, auto_topup, charges, reset_epoch)) = row else {
        return err(
            StatusCode::UNAUTHORIZED,
            "invalid_api_key",
            "unknown api key",
        );
    };

    (
        StatusCode::OK,
        Json(json!({
            "monthly_cap_micros": monthly_cap,
            "auto_topup_enabled": auto_topup,
            "monthly_charges_micros": charges,
            "month_reset_epoch": reset_epoch,
        })),
    )
        .into_response()
}

async fn put_settings(
    State(state): State<AppState>,
    Json(body): Json<SettingsUpdate>,
) -> axum::response::Response {
    // Validate any provided values.
    if let Some(mc) = body.monthly_cap_micros {
        if !(MONTHLY_CAP_MIN..=MONTHLY_CAP_MAX).contains(&mc) {
            return err(
                StatusCode::BAD_REQUEST,
                "monthly_cap_out_of_range",
                &format!("monthly_cap_micros must be {MONTHLY_CAP_MIN}..={MONTHLY_CAP_MAX}"),
            );
        }
    }
    let token_hash = sha256(body.api_key.as_bytes());

    // Per-box keys first, legacy fallback — via the shared lookup (claim.rs).
    // A DB error is 500, never 401: telling a box its key is dead because a
    // query blipped is the wrong lie (review finding, 2026-08-24).
    let cid = match super::claim::customer_id_by_key_hash(&state.pool, &token_hash[..]).await {
        Ok(Some(cid)) => cid,
        Ok(None) => {
            return err(
                StatusCode::UNAUTHORIZED,
                "invalid_api_key",
                "unknown api key",
            );
        }
        Err(e) => {
            tracing::warn!("key lookup failed: {e:#}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "customer lookup failed");
        }
    };

    // Partial update: only touch fields the client sent.
    let result = sqlx::query(
        r#"
        UPDATE customers
        SET monthly_cap_micros   = COALESCE($2, monthly_cap_micros),
            auto_topup_enabled   = COALESCE($3, auto_topup_enabled)
        WHERE stripe_customer_id = $1
        "#,
    )
    .bind(&cid)
    .bind(body.monthly_cap_micros)
    .bind(body.auto_topup_enabled)
    .execute(&state.pool)
    .await;

    match result {
        Ok(r) if r.rows_affected() == 0 => err(
            StatusCode::UNAUTHORIZED,
            "invalid_api_key",
            "unknown api key",
        ),
        Ok(_) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Err(e) => {
            tracing::warn!("settings update failed: {e:#}");
            err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "update failed")
        }
    }
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

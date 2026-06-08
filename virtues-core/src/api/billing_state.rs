//! `GET /api/billing/state` — local billing-related state for the Billing UI.
//!
//! Returns the box's view of the money flow: whether auto-top-up is enabled,
//! whether the circuit breaker has tripped, whether a BYO provider key is
//! configured (and which provider). Does NOT include wallet balance, monthly
//! cap, or daily-cap progress — those live in virtues-api and are read via
//! its own endpoints. The UI assembles the full Billing view from both.
//!
//! Also exposes `POST /api/billing/auto-topup` for the user to flip auto-
//! top-up on/off (not sudo-gated; it's a normal preference toggle). Enabling
//! also clears the breaker counter — by re-enabling, the user is saying
//! "I've fixed whatever was wrong with my card."

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;

use crate::middleware::auth::AuthUser;

#[derive(Debug, Serialize)]
pub struct BillingState {
    pub auto_topup: AutoTopupState,
    pub byo: ByoState,
}

#[derive(Debug, Serialize)]
pub struct AutoTopupState {
    pub enabled: bool,
    /// Consecutive failed top-ups in the last 24h. Reaches 3 → breaker
    /// trips → `enabled` becomes false until the user re-enables.
    pub failures_24h: i32,
    /// When the breaker last tripped, if ever. UI shows "your card kept
    /// failing — top up manually or update your payment method on Stripe".
    pub disabled_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ByoState {
    pub configured: bool,
    pub provider: Option<String>,
    pub default_model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AutoTopupRequest {
    pub enabled: bool,
}

pub async fn state_handler(
    State(pool): State<PgPool>,
    _user: AuthUser,
) -> impl IntoResponse {
    let row: Option<(bool, i32, Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as(
        "SELECT auto_topup_enabled, auto_topup_failures_24h, auto_topup_disabled_at \
         FROM app_user_profile \
         WHERE id = '00000000-0000-0000-0000-000000000001'",
    )
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten();
    let (enabled, failures, disabled_at) = row.unwrap_or((true, 0, None));

    // BYO status — same metadata fields the settings endpoint returns,
    // but inline so the UI doesn't need a second round-trip.
    let byo_row: Option<(serde_json::Value,)> = sqlx::query_as(
        "SELECT metadata FROM credentials \
         WHERE source_id = $1 AND status = 'active' \
         LIMIT 1",
    )
    .bind(crate::api::settings_byo::BYO_SOURCE_ID)
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten();
    let byo = match byo_row {
        Some((meta,)) => ByoState {
            configured: true,
            provider: meta.get("provider").and_then(|v| v.as_str()).map(String::from),
            default_model: meta
                .get("default_model")
                .and_then(|v| v.as_str())
                .map(String::from),
        },
        None => ByoState {
            configured: false,
            provider: None,
            default_model: None,
        },
    };

    (
        StatusCode::OK,
        Json(BillingState {
            auto_topup: AutoTopupState {
                enabled,
                failures_24h: failures,
                disabled_at: disabled_at.map(|d| d.to_rfc3339()),
            },
            byo,
        }),
    )
        .into_response()
}

pub async fn set_auto_topup_handler(
    State(pool): State<PgPool>,
    _user: AuthUser,
    Json(req): Json<AutoTopupRequest>,
) -> impl IntoResponse {
    // Enabling auto-top-up also clears the breaker counter — by flipping
    // it back on, the user is asserting they've fixed the underlying
    // issue (declined card, etc). Disabling leaves the counter alone.
    let q = if req.enabled {
        "UPDATE app_user_profile \
         SET auto_topup_enabled = TRUE, \
             auto_topup_failures_24h = 0, \
             auto_topup_disabled_at = NULL \
         WHERE id = '00000000-0000-0000-0000-000000000001'"
    } else {
        "UPDATE app_user_profile \
         SET auto_topup_enabled = FALSE \
         WHERE id = '00000000-0000-0000-0000-000000000001'"
    };
    let _ = sqlx::query(q).execute(&pool).await;
    (StatusCode::OK, Json(json!({"ok": true, "enabled": req.enabled})))
}

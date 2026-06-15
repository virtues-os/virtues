//! Google Places via bearer-auth + entitlement::charge().
//!
//! Charge model: charge before upstream → refund if upstream fails. The
//! charge window is short enough that race-vs-cancel is negligible at
//! our scale; the refund keeps customers honest when upstream errors
//! out (timeout, 5xx, etc.).

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::bearer_auth::BearerAuth;
use crate::entitlement::{self, ChargeError};
use crate::AppState;

/// Google Places: ~$0.003 per autocomplete or details request.
/// Tracked as 3,000 micros.
const PLACES_COST_MICROS: i64 = 3_000;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/places/autocomplete", post(places_autocomplete))
        .route("/v1/places/:place_id", get(places_details))
}

#[derive(Debug, Deserialize, Serialize)]
struct AutocompleteRequest {
    input: String,
    #[serde(flatten)]
    other: Value,
}

async fn places_autocomplete(
    State(state): State<Arc<AppState>>,
    BearerAuth(ent): BearerAuth,
    headers: HeaderMap,
    Json(request): Json<AutocompleteRequest>,
) -> axum::response::Response {
    let Some(api_key) = state.config.google_api_key.as_ref() else {
        return error_resp(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_not_configured",
            "Google Places API key not set",
        );
    };
    let pool = &state.db;

    let _ = &headers; // X-Virtues-Purpose accepted (v3 no-op telemetry)
    let charged = match entitlement::charge(pool, &ent.bearer_hash, PLACES_COST_MICROS).await {
        Ok(c) => c,
        Err(e) => return charge_error_resp(e),
    };

    // Forward the request body as-is — `#[serde(flatten)] other: Value`
    // ensures any extra fields flow through to Google without name munging.
    let upstream = state
        .http_client
        .post("https://places.googleapis.com/v1/places:autocomplete")
        .header("X-Goog-Api-Key", api_key)
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await;

    finish_charged(pool, &ent.bearer_hash, charged.billed_micros, upstream).await
}

async fn places_details(
    State(state): State<Arc<AppState>>,
    BearerAuth(ent): BearerAuth,
    headers: HeaderMap,
    Path(place_id): Path<String>,
) -> axum::response::Response {
    let Some(api_key) = state.config.google_api_key.as_ref() else {
        return error_resp(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_not_configured",
            "Google Places API key not set",
        );
    };
    let pool = &state.db;

    let _ = &headers; // X-Virtues-Purpose accepted (v3 no-op telemetry)
    let charged = match entitlement::charge(pool, &ent.bearer_hash, PLACES_COST_MICROS).await {
        Ok(c) => c,
        Err(e) => return charge_error_resp(e),
    };

    let upstream = state
        .http_client
        .get(format!(
            "https://places.googleapis.com/v1/places/{}",
            place_id
        ))
        .header("X-Goog-Api-Key", api_key)
        .header(
            "X-Goog-FieldMask",
            "id,displayName,formattedAddress,location",
        )
        .send()
        .await;

    finish_charged(pool, &ent.bearer_hash, charged.billed_micros, upstream).await
}

/// Shared tail: pass upstream response through, refund on any upstream
/// failure (transport error OR non-2xx). `billed_micros` is what was
/// actually decremented (post-markup) and is what we refund.
async fn finish_charged(
    pool: &sqlx::PgPool,
    bearer_hash: &[u8],
    billed_micros: i64,
    upstream: Result<reqwest::Response, reqwest::Error>,
) -> axum::response::Response {
    match upstream {
        Ok(resp) => {
            let status = resp.status();
            let body: Value = resp.json().await.unwrap_or_else(|_| json!({}));

            if !status.is_success() {
                if let Err(re) = entitlement::refund(pool, bearer_hash, billed_micros).await {
                    tracing::warn!("places refund failed after upstream non-2xx: {re:#}");
                }
            } else {
                tracing::info!(
                    billed_micros,
                    "places upstream success — charge retained"
                );
            }

            (
                StatusCode::from_u16(status.as_u16())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                Json(body),
            )
                .into_response()
        }
        Err(e) => {
            if let Err(re) = entitlement::refund(pool, bearer_hash, billed_micros).await {
                tracing::warn!("places refund failed after upstream transport error: {re:#}");
            }
            error_resp(StatusCode::BAD_GATEWAY, "upstream_error", &e.to_string())
        }
    }
}

fn charge_error_resp(e: ChargeError) -> axum::response::Response {
    let (status, code, message) = match e {
        ChargeError::Expired => (
            StatusCode::PAYMENT_REQUIRED,
            "bearer_expired",
            "bearer expired — redeem a fresh voucher".to_string(),
        ),
        ChargeError::InsufficientBudget => (
            StatusCode::PAYMENT_REQUIRED,
            "insufficient_budget",
            "today's budget exhausted".to_string(),
        ),
        ChargeError::NotFound => (
            StatusCode::UNAUTHORIZED,
            "unknown_bearer",
            "bearer not recognized".to_string(),
        ),
        ChargeError::InvalidCost => (
            StatusCode::BAD_REQUEST,
            "invalid_cost",
            "cost_micros must be > 0".to_string(),
        ),
        ChargeError::CallTooExpensive => (
            StatusCode::BAD_REQUEST,
            "call_too_expensive",
            "single call exceeds per-call cap".to_string(),
        ),
        ChargeError::DailyCapReached => (
            StatusCode::PAYMENT_REQUIRED,
            "daily_cap_reached",
            "daily spend ceiling reached".to_string(),
        ),
        ChargeError::Db(err) => {
            tracing::warn!("places charge db error: {err:#}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "charge failed".to_string(),
            )
        }
    };
    error_resp(status, code, &message)
}

fn error_resp(status: StatusCode, code: &str, message: &str) -> axum::response::Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": message } })),
    )
        .into_response()
}

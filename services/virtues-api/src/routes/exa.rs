//! Exa (web search) via bearer-auth + entitlement::charge().

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::bearer_auth::BearerAuth;
use crate::entitlement::{self, ChargeError};
use crate::AppState;

/// Exa: ~$0.003 per search or contents request. Tracked as 3,000 micros.
const EXA_COST_MICROS: i64 = 3_000;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/exa/search", post(exa_search))
        .route("/v1/exa/contents", post(exa_contents))
}

#[derive(Debug, Deserialize, Serialize)]
struct SearchRequest {
    query: String,
    #[serde(flatten)]
    other: Value,
}

async fn exa_search(
    State(state): State<Arc<AppState>>,
    BearerAuth(ent): BearerAuth,
    headers: HeaderMap,
    Json(request): Json<SearchRequest>,
) -> axum::response::Response {
    let Some(api_key) = state.config.exa_api_key.as_ref() else {
        return err(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_not_configured",
            "Exa API key not set",
        );
    };
    let pool = &state.db;

    let _ = &headers;
    let charged = match entitlement::charge(pool, &ent.account_id, EXA_COST_MICROS).await {
        Ok(c) => c,
        Err(e) => return charge_err(e),
    };

    let upstream = state
        .http_client
        .post("https://api.exa.ai/search")
        .header("x-api-key", api_key)
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await;

    finish_charged(pool, &ent.account_id, charged.billed_micros, upstream).await
}

async fn exa_contents(
    State(state): State<Arc<AppState>>,
    BearerAuth(ent): BearerAuth,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> axum::response::Response {
    let Some(api_key) = state.config.exa_api_key.as_ref() else {
        return err(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_not_configured",
            "Exa API key not set",
        );
    };
    let pool = &state.db;

    let _ = &headers;
    let charged = match entitlement::charge(pool, &ent.account_id, EXA_COST_MICROS).await {
        Ok(c) => c,
        Err(e) => return charge_err(e),
    };

    let upstream = state
        .http_client
        .post("https://api.exa.ai/contents")
        .header("x-api-key", api_key)
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await;

    finish_charged(pool, &ent.account_id, charged.billed_micros, upstream).await
}

async fn finish_charged(
    pool: &sqlx::PgPool,
    account_id: &str,
    billed_micros: i64,
    upstream: Result<reqwest::Response, reqwest::Error>,
) -> axum::response::Response {
    match upstream {
        Ok(resp) => {
            let status = resp.status();
            let body: Value = resp.json().await.unwrap_or_else(|_| json!({}));
            if !status.is_success() {
                if let Err(re) = entitlement::refund(pool, account_id, billed_micros).await {
                    tracing::warn!("exa refund failed after non-2xx: {re:#}");
                }
            } else {
                tracing::info!(billed_micros, "exa upstream success — charge retained");
            }
            (
                StatusCode::from_u16(status.as_u16())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                Json(body),
            )
                .into_response()
        }
        Err(e) => {
            if let Err(re) = entitlement::refund(pool, account_id, billed_micros).await {
                tracing::warn!("exa refund failed after transport error: {re:#}");
            }
            err(StatusCode::BAD_GATEWAY, "upstream_error", &e.to_string())
        }
    }
}

fn charge_err(e: ChargeError) -> axum::response::Response {
    let (status, code, message) = match e {
        ChargeError::Expired => (
            StatusCode::PAYMENT_REQUIRED,
            "wallet_expired",
            "subscription wallet expired — reconnect".to_string(),
        ),
        ChargeError::InsufficientBudget => (
            StatusCode::PAYMENT_REQUIRED,
            "insufficient_budget",
            "today's budget exhausted".to_string(),
        ),
        ChargeError::NotFound => (
            StatusCode::UNAUTHORIZED,
            "unknown_key",
            "api key not recognized — reconnect".to_string(),
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
        ChargeError::Db(e) => {
            tracing::warn!("exa charge db error: {e:#}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "charge failed".to_string(),
            )
        }
    };
    err(status, code, &message)
}

fn err(status: StatusCode, code: &str, message: &str) -> axum::response::Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": message } })),
    )
        .into_response()
}

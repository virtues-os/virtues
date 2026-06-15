//! AI proxies via bearer-auth + entitlement::charge() (WS-6b).
//!
//! Migrated paths for Vercel AI Gateway upstream:
//! - POST /v1/ai/chat/completions   (streaming + non-streaming)
//! - POST /v1/ai/completions         (text completion)
//! - POST /v1/ai/embeddings          (embeddings)
//! - GET  /v1/ai/models              (model catalog)
//!
//! Streaming chat (`stream: true`) hands off to `routes/streaming.rs` with
//! an `entitlement::charge()` callback fired once the upstream emits
//! `[DONE]`. The home server's agent loop (core `BearerClient::stream`)
//! drives this path. This is the only LLM proxy — there is no legacy
//! `/v1/chat/*` route.
//!
//! Cost model: authoritative cost is Vercel AI Gateway's `usage.cost`
//! (see `extract_cost_micros`); when absent we fall back to the per-model
//! pricing in `crates/virtues-registry`. USD → micros → charge after success.
//! Charge race window (between successful response and DB UPDATE) is
//! tolerated; failed charges are logged but do not propagate back to
//! the customer.
//!
//! PRIVACY: we do not log prompts or completions. Only usage counts and
//! resolved cost. Lint 8 enforces no events tables in this schema.

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::bearer_auth::BearerAuth;
use crate::entitlement::{self, ChargeError};
use crate::providers::{calculate_cost, get_embeddings_config, get_provider_config};
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/ai/chat/completions", post(chat_completions))
        .route("/v1/ai/completions", post(completions))
        .route("/v1/ai/embeddings", post(embeddings))
        .route("/v1/ai/models", get(list_models))
}

#[derive(Debug, Deserialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Value>,
    #[serde(default)]
    max_tokens: Option<u32>,
    #[serde(default)]
    temperature: Option<f32>,
    #[serde(default)]
    stream: Option<bool>,
    #[serde(default)]
    tools: Option<Vec<Value>>,
    #[serde(default)]
    tool_choice: Option<Value>,
}

async fn chat_completions(
    State(state): State<Arc<AppState>>,
    BearerAuth(ent): BearerAuth,
    headers: HeaderMap,
    Json(request): Json<ChatRequest>,
) -> Response {
    let pool = &state.db;

    // A valid (non-expired) bearer can use AI — single $39/mo plan, no
    // tier check. BearerAuth already enforced expiry; budget is enforced
    // by charge() after the response.

    let _ = &headers; // X-Virtues-Purpose accepted but ignored (v3 no-op)

    // Streaming: hand off to streaming.rs with a charge callback that
    // applies the resolved cost via entitlement::charge() once the
    // upstream stream emits [DONE].
    if request.stream == Some(true) {
        let streaming_req = crate::routes::streaming::StreamingRequest {
            model: request.model.clone(),
            messages: request.messages.clone(),
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            tools: request.tools.clone(),
            tool_choice: request.tool_choice.clone(),
        };
        let pool_clone = pool.clone();
        let bearer_hash = ent.bearer_hash.clone();
        let result = crate::routes::streaming::create_streaming_response(
            &state.http_client,
            &state.config,
            streaming_req,
            move |cost_micros| async move {
                if let Err(e) =
                    entitlement::charge(&pool_clone, &bearer_hash, cost_micros).await
                {
                    tracing::warn!("ai stream charge failed: {e}");
                }
            },
        )
        .await;
        return match result {
            Ok(resp) => resp,
            // ProxyError implements IntoResponse — let it render itself.
            Err(e) => e.into_response(),
        };
    }

    let provider = get_provider_config(&request.model, &state.config);
    let model = request.model.clone();

    let mut body = json!({
        "model": provider.model_name,
        "messages": request.messages,
        "max_tokens": request.max_tokens.unwrap_or(4096),
        "temperature": request.temperature.unwrap_or(0.7),
    });
    if let Some(ref tools) = request.tools {
        if !tools.is_empty() {
            body["tools"] = json!(tools);
            if let Some(ref choice) = request.tool_choice {
                body["tool_choice"] = choice.clone();
            }
        }
    }

    let upstream = state
        .http_client
        .post(&provider.endpoint)
        .header("Authorization", format!("Bearer {}", provider.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await;

    let resp = match upstream {
        Ok(r) => r,
        Err(e) => return err(StatusCode::BAD_GATEWAY, "upstream_error", &e.to_string()),
    };

    let status = resp.status();
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return err(StatusCode::BAD_GATEWAY, "upstream_parse_error", &e.to_string()),
    };

    if status.is_success() {
        let cost_micros = extract_cost_micros(&body, &model);
        if cost_micros > 0 {
            match entitlement::charge(pool, &ent.bearer_hash, cost_micros).await {
                Ok(ok) => {
                    tracing::debug!(
                        model = %model,
                        real_micros = ok.real_micros,
                        billed_micros = ok.billed_micros,
                        "ai chat charged"
                    );
                }
                Err(e) => {
                    // Don't fail the response; the customer already got it. Log
                    // and let the next call be rejected by budget check.
                    tracing::warn!("ai chat charge failed (response already returned): {e}");
                }
            }
        }
    }

    (
        StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        Json(body),
    )
        .into_response()
}

async fn completions(
    State(state): State<Arc<AppState>>,
    BearerAuth(ent): BearerAuth,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Response {
    let pool = &state.db;

    let model = request
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let provider = get_provider_config(&model, &state.config);
    let _ = &headers;

    let upstream = state
        .http_client
        .post(provider.endpoint.replace("/chat/completions", "/completions"))
        .header("Authorization", format!("Bearer {}", provider.api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await;

    forward_then_charge(pool, &ent.bearer_hash, &model, upstream).await
}

async fn embeddings(
    State(state): State<Arc<AppState>>,
    BearerAuth(ent): BearerAuth,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Response {
    let pool = &state.db;

    let provider = get_embeddings_config(&state.config);
    let _ = &headers;

    let upstream = state
        .http_client
        .post(&provider.endpoint)
        .header("Authorization", format!("Bearer {}", provider.api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await;

    let resp = match upstream {
        Ok(r) => r,
        Err(e) => return err(StatusCode::BAD_GATEWAY, "upstream_error", &e.to_string()),
    };

    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or_else(|_| json!({}));

    if status.is_success() {
        // Embedding cost: $0.0001 per 1K tokens (flat across models for now).
        let total_tokens = body
            .get("usage")
            .and_then(|u| u.get("total_tokens"))
            .and_then(|t| t.as_u64())
            .unwrap_or(0);
        let cost_usd = (total_tokens as f64 / 1000.0) * 0.0001;
        let cost_micros = usd_to_micros(cost_usd);
        if cost_micros > 0 {
            if let Err(e) = entitlement::charge(pool, &ent.bearer_hash, cost_micros).await {
                tracing::warn!("ai embeddings charge failed: {e}");
            }
        }
    }

    (
        StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        Json(body),
    )
        .into_response()
}

async fn list_models(BearerAuth(_): BearerAuth) -> Response {
    // Open the catalog to any authenticated bearer (free OR paid). No charge.
    let models = virtues_registry::default_models();
    let payload = json!({
        "object": "list",
        "data": models.iter().map(|m| {
            json!({
                "id": m.model_id,
                "display_name": m.display_name,
                "provider": m.provider,
                "object": "model",
                "context_window": m.context_window,
                "max_output_tokens": m.max_output_tokens,
                "supports_tools": m.supports_tools,
                "input_cost_per_1k": m.input_cost_per_1k,
                "output_cost_per_1k": m.output_cost_per_1k,
            })
        }).collect::<Vec<_>>(),
    });
    Json(payload).into_response()
}

/// Shared post-upstream tail for paid AI calls that extract usage from
/// the response.
async fn forward_then_charge(
    pool: &sqlx::PgPool,
    bearer_hash: &[u8],
    model: &str,
    upstream: Result<reqwest::Response, reqwest::Error>,
) -> Response {
    let resp = match upstream {
        Ok(r) => r,
        Err(e) => return err(StatusCode::BAD_GATEWAY, "upstream_error", &e.to_string()),
    };
    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or_else(|_| json!({}));

    if status.is_success() {
        let cost_micros = extract_cost_micros(&body, model);
        if cost_micros > 0 {
            if let Err(e) = entitlement::charge(pool, bearer_hash, cost_micros).await {
                tracing::warn!("ai charge failed: {e}");
            }
        }
    }

    (
        StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        Json(body),
    )
        .into_response()
}

/// Resolve cost in micros from an AI Gateway response.
///
/// **Authoritative source: Vercel AI Gateway's `usage.cost` field**
/// (USD float, returned in every chat/completions response). This avoids
/// the staleness bug class that comes from maintaining our own per-model
/// pricing table — provider price changes flow through automatically.
///
/// Fallback: if the field is missing (older endpoints, non-Vercel
/// upstreams, embeddings) we compute from `prompt_tokens` +
/// `completion_tokens` using the registry pricing in
/// `crates/virtues-registry`.
fn extract_cost_micros(body: &Value, model: &str) -> i64 {
    // Authoritative: gateway-reported cost.
    if let Some(cost) = body
        .get("usage")
        .and_then(|u| u.get("cost"))
        .and_then(|c| c.as_f64())
    {
        return usd_to_micros(cost);
    }

    // Fallback: registry pricing × token usage.
    let (prompt, completion) = body
        .get("usage")
        .map(|u| {
            (
                u.get("prompt_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32,
                u.get("completion_tokens")
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0) as u32,
            )
        })
        .unwrap_or((0, 0));
    let cost_usd = calculate_cost(model, prompt, completion);
    usd_to_micros(cost_usd)
}

fn usd_to_micros(usd: f64) -> i64 {
    (usd * 1_000_000.0).round() as i64
}

#[allow(dead_code)]
fn charge_err(e: ChargeError) -> Response {
    // Currently unused: paid AI calls don't reject on charge failure (charge
    // happens after a successful response). Kept so future migrations of
    // pre-flight-charge AI flows can reuse it.
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
        ChargeError::Db(e) => {
            tracing::warn!("ai charge db error: {e:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "internal", "charge failed".to_string())
        }
    };
    err(status, code, &message)
}

fn err(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": message } })),
    )
        .into_response()
}

//! AI proxies via bearer-auth + entitlement::charge() (WS-6b).
//!
//! Migrated paths for Vercel AI Gateway upstream:
//! - POST /v1/ai/chat/completions   (streaming + non-streaming)
//! - POST /v1/ai/completions         (text completion)
//! - GET  /v1/ai/models              (model catalog)
//!
//! There is no embeddings route. Boxes embed locally (the embedding sidecar);
//! the old `/v1/ai/embeddings` proxy sat here with zero callers after that
//! pivot and was removed 2026-08-24.
//!
//! Streaming chat (`stream: true`) hands off to `routes/streaming.rs` with
//! an `entitlement::charge()` callback fired once the upstream emits
//! `[DONE]`. The home server's agent loop (core `BearerClient::stream`)
//! drives this path. This is the only LLM proxy — there is no legacy
//! `/v1/chat/*` route.
//!
//! Cost model: authoritative cost is Vercel AI Gateway's `usage.cost`
//! (see `extract_cost_micros`), present on every response on both the
//! streaming and non-streaming paths; when absent we fall back to token counts
//! × the live gateway catalog price (`catalog.rs`), and if THAT is cold we
//! serve the call **unbilled** rather than invent a rate. USD → micros →
//! charge after success.
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
use crate::entitlement::{self, Account};
use crate::providers::{calculate_cost, get_provider_config};
use crate::AppState;

/// Pre-flight budget gate. AI cost is only known after the response, so we
/// charge post-success — but we must still refuse to *start* a call when the
/// wallet can't plausibly cover it, otherwise a $0 account chats for free
/// (charges just get logged-and-dropped). BearerAuth already enforced expiry;
/// here we gate empty balance so the box surfaces wallet_empty ("Add credits")
/// before burning upstream spend. There is no per-day wall — the only ceiling
/// is the monthly top-up cap enforced atlas-side.
fn budget_gate(acct: &Account) -> Option<Response> {
    if acct.balance_micros <= 0 {
        return Some(err(StatusCode::PAYMENT_REQUIRED, "wallet_empty", "wallet empty — add credits"));
    }
    None
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/ai/chat/completions", post(chat_completions))
        .route("/v1/ai/completions", post(completions))
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
    /// Optional reasoning budget hint ("low" | "medium" | "high") forwarded to
    /// the gateway. Lets callers (e.g. transcription) trim thinking-token cost.
    #[serde(default)]
    reasoning_effort: Option<String>,
}

async fn chat_completions(
    State(state): State<Arc<AppState>>,
    BearerAuth(ent): BearerAuth,
    headers: HeaderMap,
    Json(request): Json<ChatRequest>,
) -> Response {
    let pool = &state.db;

    // Pre-flight budget gate (empty wallet / daily cap). The actual charge
    // happens after the response, since AI cost is only known then.
    if let Some(resp) = budget_gate(&ent) {
        return resp;
    }

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
            reasoning_effort: request.reasoning_effort.clone(),
        };
        let pool_clone = pool.clone();
        let account_id = ent.account_id.clone();
        let result = crate::routes::streaming::create_streaming_response(
            &state.http_client,
            &state.config,
            &state.catalog,
            streaming_req,
            move |cost_micros| async move {
                if let Err(e) =
                    entitlement::settle(&pool_clone, &account_id, cost_micros).await
                {
                    tracing::warn!("ai stream settle failed: {e}");
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
    if let Some(ref effort) = request.reasoning_effort {
        body["reasoning_effort"] = json!(effort);
    }
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
        let cost_micros = extract_cost_micros(&state.catalog, &body, &model);
        if cost_micros > 0 {
            // Post-paid settle: debit the true cost (the response already went
            // out). The pre-flight gate refuses the next call if this puts the
            // wallet in the red.
            match entitlement::settle(pool, &ent.account_id, cost_micros).await {
                Ok(balance) => tracing::debug!(model = %model, balance, "ai chat settled"),
                Err(e) => tracing::warn!("ai chat settle failed (response already returned): {e}"),
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

    if let Some(resp) = budget_gate(&ent) {
        return resp;
    }

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

    forward_then_charge(pool, &state.catalog, &ent.account_id, &model, upstream).await
}

/// The models a box should offer, and which one fills each slot.
///
/// `data` is the picker: every priced language model the gateway carries, its
/// facts derived from the gateway for all of them equally. `recommended:true`
/// marks the five slot models — the whole of what we vouch for. Everything
/// else is the BYO path: selectable, with the provider's own capability claims.
/// The box sections on the flag.
///
/// `slots` is the live slot map. Boxes resolve a slot as:
///
///   1. the user's `app_assistant_profile` override — their choice always wins
///   2. this map — so swapping the Lite model is a cloud change, not a release
///   3. the box's compiled `default_model_for_slot` — the offline floor
///
/// That middle layer is the point: model ids churn faster than we ship boxes.
///
/// Open to ALL callers — no bearer. The list is public data (the gateway
/// serves the same catalog unauthenticated) plus our slot picks; requiring a
/// key here left every unlinked box on its 2-model compiled floor, because the
/// box-side fetch 401'd forever. Completions stay gated; this never charges.
async fn list_models(State(state): State<Arc<AppState>>) -> Response {
    use virtues_registry::models::{default_model_for_slot, ModelSlot};

    let payload = json!({
        "object": "list",
        "data": state.catalog.picker(),
        "slots": {
            "chat":   default_model_for_slot(ModelSlot::Chat),
            "lite":   default_model_for_slot(ModelSlot::Lite),
            "coding": default_model_for_slot(ModelSlot::Coding),
            "image":  default_model_for_slot(ModelSlot::Image),
        },
        // Tells the box whether `input_cost_per_1k` is real or absent, so it can
        // say "pricing unavailable" rather than render a confident zero.
        "catalog_cold": state.catalog.is_cold(),
    });
    Json(payload).into_response()
}

/// Shared post-upstream tail for paid AI calls that extract usage from
/// the response.
async fn forward_then_charge(
    pool: &sqlx::PgPool,
    catalog: &crate::catalog::Catalog,
    account_id: &str,
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
        let cost_micros = extract_cost_micros(catalog, &body, model);
        if cost_micros > 0 {
            if let Err(e) = entitlement::settle(pool, account_id, cost_micros).await {
                tracing::warn!("ai settle failed: {e}");
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
fn extract_cost_micros(catalog: &crate::catalog::Catalog, body: &Value, model: &str) -> i64 {
    // Authoritative: gateway-reported cost.
    if let Some(cost) = body
        .get("usage")
        .and_then(|u| u.get("cost"))
        .and_then(|c| c.as_f64())
    {
        return entitlement::usd_to_micros(cost);
    }

    // Fallback: live catalog pricing × token usage.
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
    // `None` = we don't know the rate. Charge nothing rather than invent one;
    // `calculate_cost` has already logged the blind spot.
    match calculate_cost(catalog, model, prompt, completion) {
        // Shared formula — no local duplicate (see entitlement::usd_to_micros).
        Some(cost_usd) => entitlement::usd_to_micros(cost_usd),
        None => 0,
    }
}

fn err(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": message } })),
    )
        .into_response()
}

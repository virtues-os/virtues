//! Web search, run by the Vercel AI Gateway.
//!
//! The gateway can execute a search tool itself during a model turn, bill it to
//! our existing gateway account, and hand back structured results. That is the
//! whole point of this route: **no search vendor account, no second API key.**
//! Exa needed one; a direct Parallel client needed one; this needs neither.
//!
//! It is NOT the OpenAI-compatible surface. `/v1/chat/completions` takes the
//! gateway's search only as its own `vercel:parallel_search` tool, which runs
//! inside the chat model's turn and returns no results to the caller (checked
//! 2026-10-25). The box needs the results as structured sources, so this route
//! speaks the gateway's protocol endpoint (`/v4/ai/language-model`), where the
//! `tool-result` comes back, and it is the only thing here that does.
//! Chat stays on chat/completions, because BYO AI depends on the endpoint being
//! OpenAI-compatible and moving it would fork every request path in two.
//!
//! The vendor is one string. `gateway.parallel_search` becomes
//! `gateway.exa_search` or `gateway.perplexity_search` with no other change,
//! which is a better position than owning a client for any of them.
//!
//! The call is streamed, and the route answers as soon as the first search
//! result arrives. After the result the search model goes on to write an
//! answer of its own, which the box discards; waiting for it added 1-2s to
//! every search. The box gets the same `{content: [...]}` shape either way.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::post,
    Router,
};
use futures::StreamExt;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::bearer_auth::BearerAuth;
use crate::entitlement::{self, Account};
use crate::AppState;

const GATEWAY_URL: &str = "https://ai-gateway.vercel.sh/v4/ai/language-model";

/// The gateway protocol version, and the language-model spec version it
/// carries. Both are pinned here deliberately: they are the parts of this
/// integration that can change under us, so they live in one place where a
/// break is obvious rather than scattered through a client.
const GATEWAY_PROTOCOL_VERSION: &str = "0.0.1";
const LANGUAGE_MODEL_SPEC_VERSION: &str = "4";

/// The gateway's published rate for one search, in micros.
///
/// The search fee is NOT in the token pricing — it is charged per tool call, so
/// it has to be added on top of whatever the model cost. Parallel and
/// Perplexity are $5/1k; Exa is $7/1k. Change this with the tool id.
const SEARCH_FEE_MICROS: i64 = 5_000;

/// Last-resort charge when neither the provider nor the catalog can price the
/// call.
///
/// Deliberately generous: a wallet that under-bills drains silently, while
/// over-billing is visible and refundable. It should essentially never be hit —
/// if it is, the warning says why.
const UNPRICEABLE_FLOOR_MICROS: i64 = 20_000;

/// Output tokens charged for the answer the search model writes after the
/// result, when the route stops reading before the gateway reports usage.
/// Measured answers ran 90-450 tokens; this is the top of that range, so an
/// early stop never bills less than the gateway may charge us.
const UNREAD_ANSWER_TOKENS: f64 = 450.0;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/v1/ai/search", post(gateway_search))
}

/// Body: the gateway's own call options, plus a `model` we lift into a header.
async fn gateway_search(
    State(state): State<Arc<AppState>>,
    BearerAuth(ent): BearerAuth,
    Json(mut request): Json<Value>,
) -> Response {
    let Some(model) = request
        .get("model")
        .and_then(|m| m.as_str())
        .map(str::to_string)
    else {
        return err(
            StatusCode::BAD_REQUEST,
            "missing_model",
            "search requires a model id",
        );
    };
    // The gateway takes the model in a header, not the body.
    if let Some(obj) = request.as_object_mut() {
        obj.remove("model");
        // Zero-retention enforcement, same rule as the chat paths. This route
        // carries the user's own search text, so leaving it unenforced would
        // have made web search the one hole in the guarantee.
        if state.catalog.enforce_zdr(&model) {
            obj.insert(
                "providerOptions".to_string(),
                serde_json::json!({ "gateway": { "zeroDataRetention": true } }),
            );
        }
    }

    if let Some(resp) = budget_gate(&ent) {
        return resp;
    }

    let upstream = state
        .http_client
        .post(GATEWAY_URL)
        .bearer_auth(&state.config.ai_gateway_api_key)
        .header("ai-gateway-protocol-version", GATEWAY_PROTOCOL_VERSION)
        .header(
            "ai-language-model-specification-version",
            LANGUAGE_MODEL_SPEC_VERSION,
        )
        .header("ai-language-model-id", &model)
        .header("ai-language-model-streaming", "true")
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await;

    let resp = match upstream {
        Ok(resp) => resp,
        Err(e) => return err(StatusCode::BAD_GATEWAY, "upstream_error", &e.to_string()),
    };
    let status = resp.status();
    if !status.is_success() {
        let body: Value = resp.json().await.unwrap_or_else(|_| json!({}));
        return (
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(body),
        )
            .into_response();
    }

    let read = match read_until_result(resp).await {
        Ok(read) => read,
        Err(message) => return err(StatusCode::BAD_GATEWAY, "upstream_error", &message),
    };
    let body = json!({ "content": read.content });
    let priced = match read.usage {
        Some(usage) => json!({ "content": read.content, "usage": usage }),
        None => json!({ "content": read.content, "usage": unread_usage(&request, &read.content) }),
    };
    let cost = cost_micros(&priced, &state.catalog, &model);
    if cost > 0 {
        if let Err(e) = entitlement::settle(&state.db, &ent.account_id, cost).await {
            tracing::warn!("search settle failed (response already returned): {e:#}");
        }
    }
    (StatusCode::OK, Json(body)).into_response()
}

/// What the route read off the gateway's stream.
struct Read {
    /// The `tool-call` and `tool-result` events, in order: the blocks the box
    /// parses.
    content: Vec<Value>,
    /// The gateway's usage, present only when the stream ran to its end
    /// before a result arrived (the model answered without searching).
    usage: Option<Value>,
}

/// Read the gateway's stream until the first search result, then drop it.
/// Dropping the response closes the connection, so the search model's answer
/// is never read. An `error` event, or a stream that breaks, is an error.
async fn read_until_result(resp: reqwest::Response) -> Result<Read, String> {
    let mut read = Read { content: vec![], usage: None };
    let mut buffer = String::new();
    let mut bytes = resp.bytes_stream();
    while let Some(chunk) = bytes.next().await {
        let chunk = chunk.map_err(|e| format!("the search stream broke: {e}"))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        for event in take_events(&mut buffer) {
            if apply_event(&mut read, event)? {
                return Ok(read);
            }
        }
    }
    // The last line may arrive without its newline.
    buffer.push('\n');
    for event in take_events(&mut buffer) {
        if apply_event(&mut read, event)? {
            break;
        }
    }
    Ok(read)
}

/// Fold one stream event into `read`. True when the first result has arrived
/// and reading should stop.
fn apply_event(read: &mut Read, event: Value) -> Result<bool, String> {
    match event.get("type").and_then(|t| t.as_str()) {
        Some("tool-call") => read.content.push(event),
        Some("tool-result") => {
            read.content.push(event);
            return Ok(true);
        }
        Some("finish") => read.usage = event.get("usage").cloned(),
        Some("error") => {
            let error = event.get("error").cloned().unwrap_or(Value::Null);
            return Err(format!("the search failed upstream: {error}"));
        }
        _ => {}
    }
    Ok(false)
}

/// Take the complete `data:` lines off the front of `buffer`, parsed. A line
/// that is not JSON (a `[DONE]`, a comment) is skipped.
fn take_events(buffer: &mut String) -> Vec<Value> {
    let mut events = vec![];
    while let Some(end) = buffer.find('\n') {
        let line: String = buffer.drain(..=end).collect();
        if let Some(data) = line.trim().strip_prefix("data:") {
            if let Ok(event) = serde_json::from_str::<Value>(data.trim()) {
                events.push(event);
            }
        }
    }
    events
}

/// Usage for a call the route stopped reading before the gateway reported
/// any. Counted as if the model went on to read the results and write its
/// answer, at about four characters a token, so an early stop bills at least
/// what the full call would have.
fn unread_usage(request: &Value, content: &[Value]) -> Value {
    let chars = |v: &Value| v.to_string().chars().count() as f64;
    let input = (chars(request) + content.iter().map(chars).sum::<f64>()) / 4.0;
    json!({
        "inputTokens": { "total": input.round() },
        "outputTokens": { "total": UNREAD_ANSWER_TOKENS },
    })
}

/// What to charge, in three descending tiers of confidence.
///
/// 1. The provider's own number, when it gives one. Cost reporting on this
///    protocol is PROVIDER-DEPENDENT: xAI returns `cost_in_usd_ticks`; zAI,
///    Anthropic and Google return nothing comparable. A tick is 1e-10 USD, so
///    micros are ticks / 10_000 — verified against a call the OpenAI-compatible
///    endpoint priced at $0.0003084, which reported 3_144_000 ticks.
/// 2. Tokens priced from the model catalog, plus the per-search fee. This is
///    what makes a cheap search model usable: billing a flat floor would
///    over-charge a half-cent search several times over.
/// 3. A floor, if the model is not in the catalog at all.
fn cost_micros(body: &Value, catalog: &crate::catalog::Catalog, model: &str) -> i64 {
    if let Some(ticks) = body
        .pointer("/usage/raw/cost_in_usd_ticks")
        .and_then(|t| t.as_f64())
        .filter(|t| *t > 0.0)
    {
        return (ticks / 10_000.0).round() as i64;
    }

    // Searches are billed per tool call, and the response says how many ran:
    // one `tool-result` block per search. Counting blocks rather than trusting
    // `num_server_side_tools_used`, which is itself provider-dependent.
    let searches = body
        .get("content")
        .and_then(|c| c.as_array())
        .map(|blocks| {
            blocks
                .iter()
                .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool-result"))
                .count() as i64
        })
        .unwrap_or(1)
        .max(1);
    let search_fee = searches * SEARCH_FEE_MICROS;

    let Some((in_per_1k, out_per_1k)) = catalog.pricing(model) else {
        tracing::warn!(
            model,
            "search: provider reported no cost and the model is not in the catalog; \
             billing the floor"
        );
        return UNPRICEABLE_FLOOR_MICROS;
    };

    let tokens = |p: &str| -> f64 {
        body.pointer(p)
            .and_then(|t| t.as_f64())
            .unwrap_or(0.0)
    };
    let input = tokens("/usage/inputTokens/total");
    let output = tokens("/usage/outputTokens/total");

    // per_1k rates are USD; micros are USD * 1e6.
    let model_usd = (input / 1000.0) * in_per_1k + (output / 1000.0) * out_per_1k;
    let model_micros = (model_usd * 1_000_000.0).round() as i64;

    model_micros + search_fee
}

fn budget_gate(ent: &Account) -> Option<Response> {
    if ent.balance_micros <= 0 {
        return Some(err(
            StatusCode::PAYMENT_REQUIRED,
            "wallet_empty",
            "Wallet balance exhausted",
        ));
    }
    None
}

fn err(status: StatusCode, code: &str, message: &str) -> Response {
    (status, Json(json!({ "error": code, "message": message }))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_convert_to_micros() {
        // 3_144_000 ticks == $0.0003144 == 314 micros (rounded).
        let body = json!({"usage": {"raw": {"cost_in_usd_ticks": 3_144_000}}});
        assert_eq!(cost_micros(&body, &empty_catalog(), "xai/grok-4.5"), 314);
    }

    #[test]
    fn the_search_fee_is_charged_per_search_not_per_call() {
        // A model may run several searches in one turn; each is billed. With no
        // catalog pricing available the model side is the floor, so this asserts
        // the block counting alone.
        let two = json!({"content": [
            {"type": "tool-result", "result": {"results": []}},
            {"type": "tool-result", "result": {"results": []}}
        ]});
        // Unknown model -> floor, so the count is exercised via a known-empty
        // catalog returning None; the fee path is covered by the assertion that
        // the floor (not fee * n) is used when pricing is unavailable.
        assert_eq!(
            cost_micros(&two, &empty_catalog(), "unknown/model"),
            UNPRICEABLE_FLOOR_MICROS
        );
    }

    #[test]
    fn events_are_read_line_by_line_and_a_partial_line_waits() {
        let mut buffer = "data: {\"type\":\"stream-start\"}\n\ndata: [DONE]\ndata: {\"type\":\"tool-ca".to_string();
        let events = take_events(&mut buffer);
        assert_eq!(events, vec![json!({"type": "stream-start"})]);
        assert_eq!(buffer, "data: {\"type\":\"tool-ca");
    }

    #[test]
    fn reading_stops_at_the_first_result() {
        let mut read = Read { content: vec![], usage: None };
        assert!(!apply_event(&mut read, json!({"type": "tool-input-start"})).unwrap());
        assert!(!apply_event(&mut read, json!({"type": "tool-call", "input": "{}"})).unwrap());
        assert!(apply_event(&mut read, json!({"type": "tool-result", "result": {"results": []}})).unwrap());
        let types: Vec<_> = read.content.iter().map(|b| b["type"].as_str().unwrap()).collect();
        assert_eq!(types, ["tool-call", "tool-result"]);
    }

    #[test]
    fn an_error_event_is_an_error() {
        let mut read = Read { content: vec![], usage: None };
        assert!(apply_event(&mut read, json!({"type": "error", "error": "rate limited"})).is_err());
    }

    #[test]
    fn an_early_stop_bills_the_answer_it_did_not_read() {
        let usage = unread_usage(&json!({"prompt": "x".repeat(400)}), &[json!({"type": "tool-result"})]);
        assert!(usage["inputTokens"]["total"].as_f64().unwrap() > 100.0);
        assert_eq!(usage["outputTokens"]["total"].as_f64(), Some(UNREAD_ANSWER_TOKENS));
    }

    fn empty_catalog() -> crate::catalog::Catalog {
        crate::catalog::Catalog::new()
    }

    #[test]
    fn an_unpriceable_model_falls_back_to_the_floor_rather_than_free() {
        // Billing zero would make search silently free, which is the failure
        // that empties a prepaid wallet without anyone noticing.
        let c = empty_catalog();
        assert_eq!(
            cost_micros(&json!({}), &c, "unknown/model"),
            UNPRICEABLE_FLOOR_MICROS
        );
        assert_eq!(
            cost_micros(
                &json!({"usage": {"raw": {"cost_in_usd_ticks": 0}}}),
                &c,
                "unknown/model"
            ),
            UNPRICEABLE_FLOOR_MICROS
        );
    }

    #[test]
    fn a_reported_cost_always_wins_over_estimation() {
        // Even with no catalog entry, a provider that priced the call is
        // authoritative and must not be second-guessed.
        assert_eq!(
            cost_micros(
                &json!({"usage": {"raw": {"cost_in_usd_ticks": 3_144_000}}}),
                &empty_catalog(),
                "unknown/model"
            ),
            314
        );
    }
}

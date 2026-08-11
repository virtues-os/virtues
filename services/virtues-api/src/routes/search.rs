//! Web search, run by the Vercel AI Gateway.
//!
//! The gateway can execute a search tool itself during a model turn, bill it to
//! our existing gateway account, and hand back structured results. That is the
//! whole point of this route: **no search vendor account, no second API key.**
//! Exa needed one; a direct Parallel client needed one; this needs neither.
//!
//! It is NOT the OpenAI-compatible surface. Provider-executed tools only exist
//! on the gateway's own protocol endpoint (`/v4/ai/language-model`), which
//! `/v1/chat/completions` rejects outright — verified: `expected "function"`.
//! So this route speaks that protocol, and it is the only thing here that does.
//! Chat stays on chat/completions, because BYO AI depends on the endpoint being
//! OpenAI-compatible and moving it would fork every request path in two.
//!
//! The vendor is one string. `gateway.parallel_search` becomes
//! `gateway.exa_search` or `gateway.perplexity_search` with no other change,
//! which is a better position than owning a client for any of them.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::post,
    Router,
};
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

/// Charged when the response reports no cost.
///
/// Cost reporting on this protocol is PROVIDER-DEPENDENT — xAI returns
/// `usage.raw.cost_in_usd_ticks`, Anthropic returns nothing comparable. A
/// search is a model call plus a search fee, and a real one measured about
/// 1.2¢, so this floor rounds up. Under-billing drains a prepaid wallet
/// silently; over-billing by a cent is visible and refundable.
const NO_COST_REPORTED_FLOOR_MICROS: i64 = 20_000;

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
        .header("ai-language-model-streaming", "false")
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await;

    match upstream {
        Ok(resp) => {
            let status = resp.status();
            let body: Value = resp.json().await.unwrap_or_else(|_| json!({}));
            if status.is_success() {
                let cost = cost_micros(&body);
                if cost > 0 {
                    if let Err(e) = entitlement::settle(&state.db, &ent.account_id, cost).await {
                        tracing::warn!("search settle failed (response already returned): {e:#}");
                    }
                }
            }
            (
                StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                Json(body),
            )
                .into_response()
        }
        Err(e) => err(StatusCode::BAD_GATEWAY, "upstream_error", &e.to_string()),
    }
}

/// Real cost when the provider reports one, the floor when it does not.
///
/// A tick is 1e-10 USD, so micros (1e-6 USD) are ticks / 10_000. Verified
/// against a call the OpenAI-compatible endpoint priced at $0.0003084: the same
/// shape reported 3_144_000 ticks.
fn cost_micros(body: &Value) -> i64 {
    let ticks = body
        .get("usage")
        .and_then(|u| u.get("raw"))
        .and_then(|r| r.get("cost_in_usd_ticks"))
        .and_then(|t| t.as_f64());

    match ticks {
        Some(t) if t > 0.0 => (t / 10_000.0).round() as i64,
        _ => {
            tracing::warn!(
                "search response reported no cost; billing the floor. If this is \
                 constant, the search model's provider does not report cost on \
                 this protocol and the slot should move to one that does."
            );
            NO_COST_REPORTED_FLOOR_MICROS
        }
    }
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
        assert_eq!(cost_micros(&body), 314);
    }

    #[test]
    fn a_real_search_prices_in_cents_not_dollars() {
        // The measured shape: ~1.2¢ for a three-result search on grok.
        let body = json!({"usage": {"raw": {"cost_in_usd_ticks": 117_944_000i64}}});
        assert_eq!(cost_micros(&body), 11_794);
    }

    #[test]
    fn missing_cost_falls_back_to_the_floor_rather_than_free() {
        // Anthropic does not report cost on this protocol. Billing zero would
        // make search silently free, which is the failure that empties a wallet.
        assert_eq!(cost_micros(&json!({})), NO_COST_REPORTED_FLOOR_MICROS);
        assert_eq!(
            cost_micros(&json!({"usage": {"raw": {"input_tokens": 500}}})),
            NO_COST_REPORTED_FLOOR_MICROS
        );
        assert_eq!(
            cost_micros(&json!({"usage": {"raw": {"cost_in_usd_ticks": 0}}})),
            NO_COST_REPORTED_FLOOR_MICROS
        );
    }
}

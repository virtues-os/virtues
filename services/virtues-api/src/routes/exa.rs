//! Exa (web search) via bearer-auth + post-paid settlement.
//!
//! Cost model: Exa returns the authoritative `costDollars.total` (USD float) in
//! every 2.0 response. We mirror the AI path (`routes/ai.rs`): a read-only
//! pre-flight budget gate, fire the call, then `entitlement::settle()` the real
//! cost. This auto-scales billing across search types — a default `/search`
//! (~$0.005) and a `type: "deep"` search (~$0.015) settle whatever Exa reports,
//! with no per-type constant to maintain. Deep is a body param on `/search`, so
//! it needs no separate route.
//!
//! Bodies are passed through verbatim (`Json<Value>`), so caller-set fields like
//! `type`, `maxAgeHours`, and `contents` reach Exa untouched.

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

/// Fallback real-cost floors (micros), used only if Exa omits `costDollars`
/// (it shouldn't on the 2.0 API). Chosen to over- rather than under-bill: a
/// search-with-contents is ~$0.007, a deep search ~$0.015, a contents call
/// ~$0.0015. `settle()` applies the markup on top.
const SEARCH_FLOOR_MICROS: i64 = 7_000;
const DEEP_FLOOR_MICROS: i64 = 15_000;
const CONTENTS_FLOOR_MICROS: i64 = 1_500;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/exa/search", post(exa_search))
        .route("/v1/exa/contents", post(exa_contents))
}

async fn exa_search(
    State(state): State<Arc<AppState>>,
    BearerAuth(ent): BearerAuth,
    Json(request): Json<Value>,
) -> Response {
    // A `type: "deep"` (or "deep-reasoning") search costs more; pick the floor to
    // match in case Exa ever omits `costDollars`.
    let floor = match request.get("type").and_then(|t| t.as_str()) {
        Some("deep") | Some("deep-reasoning") => DEEP_FLOOR_MICROS,
        _ => SEARCH_FLOOR_MICROS,
    };
    proxy_and_settle(&state, &ent, "https://api.exa.ai/search", &request, floor).await
}

async fn exa_contents(
    State(state): State<Arc<AppState>>,
    BearerAuth(ent): BearerAuth,
    Json(request): Json<Value>,
) -> Response {
    proxy_and_settle(
        &state,
        &ent,
        "https://api.exa.ai/contents",
        &request,
        CONTENTS_FLOOR_MICROS,
    )
    .await
}

/// Shared proxy tail: gate, forward to Exa, settle the real cost on success.
async fn proxy_and_settle(
    state: &AppState,
    ent: &Account,
    url: &str,
    request: &Value,
    floor_micros: i64,
) -> Response {
    let Some(api_key) = state.config.exa_api_key.as_ref() else {
        return err(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_not_configured",
            "Exa API key not set",
        );
    };

    // Pre-flight gate. Like AI, cost is only known after the response, so we
    // refuse to *start* a call the wallet can't plausibly cover; the actual
    // debit happens post-success via settle().
    if let Some(resp) = budget_gate(ent) {
        return resp;
    }

    let upstream = state
        .http_client
        .post(url)
        .header("x-api-key", api_key)
        .header("Content-Type", "application/json")
        .json(request)
        .send()
        .await;

    match upstream {
        Ok(resp) => {
            let status = resp.status();
            let body: Value = resp.json().await.unwrap_or_else(|_| json!({}));
            if status.is_success() {
                let cost = extract_exa_cost_micros(&body, floor_micros);
                if cost > 0 {
                    // Post-paid: the response already went out, so debit
                    // unconditionally. The pre-flight gate refuses the next
                    // call if this puts the wallet in the red.
                    if let Err(e) = entitlement::settle(&state.db, &ent.account_id, cost).await {
                        tracing::warn!("exa settle failed (response already returned): {e:#}");
                    }
                }
            }
            // Non-2xx: nothing was charged, so nothing to refund — pass the
            // upstream error body straight back.
            (
                StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                Json(body),
            )
                .into_response()
        }
        Err(e) => err(StatusCode::BAD_GATEWAY, "upstream_error", &e.to_string()),
    }
}

/// Resolve real cost from Exa's authoritative `costDollars.total` (USD float).
/// Falls back to the per-endpoint floor if the field is absent.
fn extract_exa_cost_micros(body: &Value, floor_micros: i64) -> i64 {
    if let Some(total) = body
        .get("costDollars")
        .and_then(|c| c.get("total"))
        .and_then(|t| t.as_f64())
    {
        let micros = entitlement::usd_to_micros(total);
        if micros > 0 {
            return micros;
        }
    }
    floor_micros
}

/// Read-only pre-flight gate, mirroring `routes/ai.rs::budget_gate`. BearerAuth
/// already enforced expiry; here we surface empty wallet / daily cap before
/// burning upstream spend.
fn budget_gate(acct: &Account) -> Option<Response> {
    if acct.balance_micros <= 0 {
        return Some(err(
            StatusCode::PAYMENT_REQUIRED,
            "wallet_empty",
            "wallet empty — add credits",
        ));
    }
    if acct.today_spent_micros >= acct.daily_cap_micros {
        return Some(err(
            StatusCode::PAYMENT_REQUIRED,
            "daily_cap_reached",
            "daily spend ceiling reached",
        ));
    }
    None
}

fn err(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": message } })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_from_cost_dollars() {
        let body = json!({ "costDollars": { "total": 0.0123 }, "results": [] });
        assert_eq!(extract_exa_cost_micros(&body, SEARCH_FLOOR_MICROS), 12_300);
    }

    #[test]
    fn cost_falls_back_to_floor_when_missing() {
        let body = json!({ "results": [] });
        assert_eq!(
            extract_exa_cost_micros(&body, DEEP_FLOOR_MICROS),
            DEEP_FLOOR_MICROS
        );
    }

    #[test]
    fn cost_falls_back_when_zero() {
        let body = json!({ "costDollars": { "total": 0.0 } });
        assert_eq!(
            extract_exa_cost_micros(&body, SEARCH_FLOOR_MICROS),
            SEARCH_FLOOR_MICROS
        );
    }
}

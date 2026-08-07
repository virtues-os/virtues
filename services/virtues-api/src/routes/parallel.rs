//! Parallel (web search) via bearer-auth + post-paid settlement.
//!
//! Replaces the Exa route. Same shape — read-only pre-flight gate, fire the
//! call, settle after — with one material difference the wallet has to live
//! with:
//!
//! **Parallel does not report what a call cost.** Exa returns an authoritative
//! `costDollars.total` in every response and we settled exactly that, so
//! billing was measured. Parallel's response carries `usage: [{name, count}]`
//! and no price, so settlement here is an ESTIMATE from published rates
//! ($0.005 a search, +$0.001 per result past the first ten) picked by mode.
//!
//! That is a real loss of accuracy, taken deliberately for one fewer vendor
//! account. The estimates round UP: over-billing a fraction of a cent is
//! recoverable and visible, while under-billing silently drains a prepaid
//! wallet nobody is watching. If Parallel ever returns a price, prefer it and
//! delete the table below.

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

/// Published base rates, in micros (USD × 1e6), by `mode`.
///
/// `advanced` is multi-step and costs more; `turbo` is the cheap tier. These
/// are the numbers to revisit when Parallel changes pricing — they are the
/// only place the cost model lives now that the upstream does not report one.
const TURBO_FLOOR_MICROS: i64 = 3_000;
const BASIC_FLOOR_MICROS: i64 = 5_000;
const ADVANCED_FLOOR_MICROS: i64 = 15_000;

/// Results included in the base rate; each one past this adds a thousandth.
const RESULTS_INCLUDED: usize = 10;
const PER_EXTRA_RESULT_MICROS: i64 = 1_000;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/v1/parallel/search", post(parallel_search))
}

async fn parallel_search(
    State(state): State<Arc<AppState>>,
    BearerAuth(ent): BearerAuth,
    Json(request): Json<Value>,
) -> Response {
    let base = match request.get("mode").and_then(|m| m.as_str()) {
        Some("turbo") => TURBO_FLOOR_MICROS,
        Some("advanced") => ADVANCED_FLOOR_MICROS,
        // Absent or "basic" — Parallel's own default.
        _ => BASIC_FLOOR_MICROS,
    };

    let Some(api_key) = state.config.parallel_api_key.as_ref() else {
        return err(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_not_configured",
            "Parallel API key not set",
        );
    };

    // Pre-flight gate: cost is only knowable after the call, so refuse to
    // START one the wallet cannot plausibly cover. The debit happens after.
    if let Some(resp) = budget_gate(&ent) {
        return resp;
    }

    let upstream = state
        .http_client
        .post("https://api.parallel.ai/v1/search")
        .header("x-api-key", api_key)
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await;

    match upstream {
        Ok(resp) => {
            let status = resp.status();
            let body: Value = resp.json().await.unwrap_or_else(|_| json!({}));
            if status.is_success() {
                let cost = estimate_cost_micros(&body, base);
                if cost > 0 {
                    // Post-paid: the response has already gone out, so debit
                    // unconditionally and let the gate refuse the next call.
                    if let Err(e) = entitlement::settle(&state.db, &ent.account_id, cost).await {
                        tracing::warn!("parallel settle failed (response already returned): {e:#}");
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

/// Base rate plus the per-result overage, counted from what actually came back.
///
/// Counting returned results rather than the requested `max_results` means a
/// search that asked for 20 and found 3 is billed for 3 — the honest direction,
/// and the one that cannot over-bill a thin query.
fn estimate_cost_micros(body: &Value, base_micros: i64) -> i64 {
    let returned = body
        .get("results")
        .and_then(|r| r.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let extra = returned.saturating_sub(RESULTS_INCLUDED) as i64;
    base_micros + extra * PER_EXTRA_RESULT_MICROS
}

/// Read-only budget check. Mirrors the AI path.
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
    fn base_rate_when_results_fit_the_allowance() {
        let body = json!({ "results": (0..8).map(|_| json!({})).collect::<Vec<_>>() });
        assert_eq!(estimate_cost_micros(&body, BASIC_FLOOR_MICROS), 5_000);
    }

    #[test]
    fn overage_counts_only_results_past_the_allowance() {
        let body = json!({ "results": (0..13).map(|_| json!({})).collect::<Vec<_>>() });
        assert_eq!(
            estimate_cost_micros(&body, BASIC_FLOOR_MICROS),
            5_000 + 3 * PER_EXTRA_RESULT_MICROS
        );
    }

    #[test]
    fn a_search_that_found_nothing_still_costs_the_base() {
        // The upstream did the work either way; only the overage is
        // result-dependent.
        assert_eq!(
            estimate_cost_micros(&json!({ "results": [] }), ADVANCED_FLOOR_MICROS),
            15_000
        );
        assert_eq!(estimate_cost_micros(&json!({}), BASIC_FLOOR_MICROS), 5_000);
    }
}

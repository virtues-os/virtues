//! Plaid bank-data proxy via bearer-auth + post-paid settlement.
//!
//! WHY THIS EXISTS: Plaid's API requires `client_id` + `secret` on *every* data
//! call (there is no scoped-token mode — confirmed against Plaid docs). If the
//! home box called Plaid directly it would have to carry the MASTER Plaid
//! secret, which can read every linked account across the entire user base. So
//! instead the box sends only its per-user `access_token` here, and this proxy
//! injects `client_id`+`secret` server-side. The master secret never leaves
//! virtues-api. This mirrors how `oauth.rs` already injects them for the OAuth
//! link/exchange calls, and how the box already keeps per-user tokens for
//! Gmail/Notion.
//!
//! Structure copies `routes/parallel.rs`: `BearerAuth(ent)` → per-account auth,
//! read-only `budget_gate`, verbatim `Json<Value>` body passthrough, fire the
//! call, then `entitlement::settle()`.
//!
//! Bodies are strict pass-through — never logged, never stored. Bank data
//! transits in-flight only, same posture as the AI proxy.
//!
//! Cost model: Plaid bills ~$0.30 per connected account per month, not per API
//! call, so there is no per-response cost field to read. We settle a small fixed
//! per-call amount as a deliberate approximation (see the plan's "Metering
//! note"). `settle()` applies the wallet markup on top.

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

// Fixed per-call cost (micros, pre-markup). Small on purpose: `transactions/sync`
// paginates (several calls per run) and the true cost is monthly-per-account, so
// per-call must not over-bill a backfill. Tune here.
const TRANSACTIONS_MICROS: i64 = 500;
const ACCOUNTS_MICROS: i64 = 500;
const HOLDINGS_MICROS: i64 = 1_000;
const LIABILITIES_MICROS: i64 = 1_000;

pub fn router() -> Router<Arc<AppState>> {
    // Allowlisted, named endpoints only — never a generic passthrough. Security
    // is the whole point of this module, so the box can reach exactly these
    // four Plaid data endpoints and nothing else.
    Router::new()
        .route("/v1/services/plaid/transactions/sync", post(transactions_sync))
        .route("/v1/services/plaid/accounts/get", post(accounts_get))
        .route(
            "/v1/services/plaid/investments/holdings/get",
            post(investments_holdings_get),
        )
        .route("/v1/services/plaid/liabilities/get", post(liabilities_get))
}

async fn transactions_sync(
    State(state): State<Arc<AppState>>,
    BearerAuth(ent): BearerAuth,
    Json(body): Json<Value>,
) -> Response {
    proxy_and_settle(&state, &ent, "transactions/sync", &body, TRANSACTIONS_MICROS).await
}

async fn accounts_get(
    State(state): State<Arc<AppState>>,
    BearerAuth(ent): BearerAuth,
    Json(body): Json<Value>,
) -> Response {
    proxy_and_settle(&state, &ent, "accounts/get", &body, ACCOUNTS_MICROS).await
}

async fn investments_holdings_get(
    State(state): State<Arc<AppState>>,
    BearerAuth(ent): BearerAuth,
    Json(body): Json<Value>,
) -> Response {
    proxy_and_settle(
        &state,
        &ent,
        "investments/holdings/get",
        &body,
        HOLDINGS_MICROS,
    )
    .await
}

async fn liabilities_get(
    State(state): State<Arc<AppState>>,
    BearerAuth(ent): BearerAuth,
    Json(body): Json<Value>,
) -> Response {
    proxy_and_settle(&state, &ent, "liabilities/get", &body, LIABILITIES_MICROS).await
}

/// Shared proxy tail: gate, inject the master secret, forward to Plaid, settle.
async fn proxy_and_settle(
    state: &AppState,
    ent: &Account,
    plaid_path: &str,
    body: &Value,
    cost_micros: i64,
) -> Response {
    let (Some(client_id), Some(secret)) =
        (&state.config.plaid_client_id, &state.config.plaid_secret)
    else {
        return err(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_not_configured",
            "Plaid credentials not set",
        );
    };

    // Pre-flight gate: refuse to start a call the wallet can't cover.
    if let Some(resp) = budget_gate(ent) {
        return resp;
    }

    // Inject the master credentials server-side. The box sent only the per-user
    // `access_token` (+ cursor/params); we add `client_id`+`secret` here so they
    // never leave virtues-api.
    let outbound = with_credentials(body, client_id, secret);
    let url = format!("{}/{}", state.config.plaid_base_url, plaid_path);

    let upstream = state
        .http_client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&outbound)
        .send()
        .await;

    match upstream {
        Ok(resp) => {
            let status = resp.status();
            let resp_body: Value = resp.json().await.unwrap_or_else(|_| json!({}));
            if status.is_success() {
                // Post-paid: the data already went out, so debit unconditionally.
                // The pre-flight gate refuses the next call if this reds the wallet.
                if let Err(e) = entitlement::settle(&state.db, &ent.account_id, cost_micros).await {
                    tracing::warn!("plaid settle failed (response already returned): {e:#}");
                }
            }
            // Non-2xx: nothing charged; pass Plaid's error body straight back.
            (
                StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                Json(resp_body),
            )
                .into_response()
        }
        Err(e) => err(StatusCode::BAD_GATEWAY, "upstream_error", &e.to_string()),
    }
}

/// Return a copy of the request body with `client_id` + `secret` set. If the box
/// sent a non-object body (shouldn't happen), wrap it so we always send a valid
/// Plaid request object.
fn with_credentials(body: &Value, client_id: &str, secret: &str) -> Value {
    let mut obj = match body {
        Value::Object(map) => map.clone(),
        _ => serde_json::Map::new(),
    };
    obj.insert("client_id".to_string(), json!(client_id));
    obj.insert("secret".to_string(), json!(secret));
    Value::Object(obj)
}

/// Read-only pre-flight gate, mirroring `routes/parallel.rs::budget_gate`.
fn budget_gate(acct: &Account) -> Option<Response> {
    if acct.balance_micros <= 0 {
        return Some(err(
            StatusCode::PAYMENT_REQUIRED,
            "wallet_empty",
            "wallet empty — add credits",
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
    fn injects_credentials_into_object_body() {
        let body = json!({ "access_token": "access-sandbox-123", "cursor": "abc" });
        let out = with_credentials(&body, "cid", "sec");
        assert_eq!(out["client_id"], json!("cid"));
        assert_eq!(out["secret"], json!("sec"));
        // caller fields preserved
        assert_eq!(out["access_token"], json!("access-sandbox-123"));
        assert_eq!(out["cursor"], json!("abc"));
    }

    #[test]
    fn overwrites_any_client_supplied_credentials() {
        // A box must never be able to override the server's secret.
        let body = json!({ "access_token": "t", "client_id": "spoof", "secret": "spoof" });
        let out = with_credentials(&body, "real_id", "real_secret");
        assert_eq!(out["client_id"], json!("real_id"));
        assert_eq!(out["secret"], json!("real_secret"));
    }

    #[test]
    fn wraps_non_object_body() {
        let out = with_credentials(&json!("garbage"), "cid", "sec");
        assert!(out.is_object());
        assert_eq!(out["client_id"], json!("cid"));
    }
}

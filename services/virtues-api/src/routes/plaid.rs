//! Plaid bank-data proxy for subscribers.
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
//! Bodies are strict pass-through — never logged, never stored. Bank data
//! transits in-flight only, same posture as the AI proxy.
//!
//! Cost model: bank data is included in the subscription, so these calls never
//! touch the wallet. A data call needs a live subscription (`BearerAuth`) and
//! keeps working when the month's AI credit is spent. Plaid bills a monthly fee
//! per linked bank, not per call, so the limit is on banks per account
//! (`crate::banks`), checked when a bank is linked.
//!
//! [`remove_item`] is reached through the OAuth proxy's revoke route
//! (`oauth.rs`), which needs only a box key we issued, so a box whose
//! subscription lapsed can still remove its banks and stop the monthly fee.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::post,
    Router,
};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::banks;
use crate::bearer_auth::BearerAuth;
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    // Allowlisted, named endpoints only — never a generic passthrough. Security
    // is the whole point of this module, so the box can reach exactly these
    // Plaid endpoints and nothing else.
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
    BearerAuth(_ent): BearerAuth,
    Json(body): Json<Value>,
) -> Response {
    proxy(&state, "transactions/sync", &body).await
}

async fn accounts_get(
    State(state): State<Arc<AppState>>,
    BearerAuth(_ent): BearerAuth,
    Json(body): Json<Value>,
) -> Response {
    proxy(&state, "accounts/get", &body).await
}

async fn investments_holdings_get(
    State(state): State<Arc<AppState>>,
    BearerAuth(_ent): BearerAuth,
    Json(body): Json<Value>,
) -> Response {
    proxy(&state, "investments/holdings/get", &body).await
}

async fn liabilities_get(
    State(state): State<Arc<AppState>>,
    BearerAuth(_ent): BearerAuth,
    Json(body): Json<Value>,
) -> Response {
    proxy(&state, "liabilities/get", &body).await
}

/// Remove a linked bank from Plaid, which ends its monthly fee, and free its
/// place under the account's bank limit. Called when the owner disconnects
/// the bank, or when the box lets go of its banks after a lapsed
/// subscription. Plaid's `item/remove` does not return the item id, so this
/// asks `item/get` first to know which place to free.
pub(crate) async fn remove_item(state: &AppState, account_id: &str, access_token: &str) -> Response {
    if access_token.is_empty() {
        return err(StatusCode::BAD_REQUEST, "missing_access_token", "access_token is required");
    }
    let only_token = json!({ "access_token": access_token });

    let item_id = match call(state, "item/get", &only_token).await {
        Ok((status, v)) if status.is_success() => v
            .pointer("/item/item_id")
            .and_then(|x| x.as_str())
            .map(String::from),
        // An item Plaid no longer knows is already removed: nothing to free
        // here and nothing more to stop.
        Ok((_, v)) if plaid_error_code(&v) == Some("ITEM_NOT_FOUND") => return removed(),
        Ok((status, v)) => return passthrough(status, v),
        Err(resp) => return resp,
    };

    match call(state, "item/remove", &only_token).await {
        Ok((status, _)) if status.is_success() => {}
        Ok((_, v)) if plaid_error_code(&v) == Some("ITEM_NOT_FOUND") => {}
        Ok((status, v)) => return passthrough(status, v),
        Err(resp) => return resp,
    }
    if let Some(item_id) = item_id {
        if let Err(e) = banks::forget(&state.db, account_id, &item_id).await {
            // Plaid has removed the bank, so the fee has stopped. The place under
            // the limit stays taken until the next removal attempt, which a
            // repeated call makes because ITEM_NOT_FOUND is answered as removed.
            tracing::error!(error = %e, "plaid item removed, but freeing its place failed");
        }
    }
    removed()
}

/// Remove a bank from Plaid by its access token, for a bank refused at link
/// time. Returns whether Plaid confirmed it. Nothing is recorded under the
/// limit, because a refused bank never took a place.
pub(crate) async fn remove_refused(state: &AppState, access_token: &str) -> bool {
    match call(state, "item/remove", &json!({ "access_token": access_token })).await {
        Ok((status, _)) => status.is_success(),
        Err(_) => false,
    }
}

fn removed() -> Response {
    (StatusCode::OK, Json(json!({ "removed": true }))).into_response()
}

fn plaid_error_code(v: &Value) -> Option<&str> {
    v.get("error_code").and_then(|c| c.as_str())
}

fn passthrough(status: reqwest::StatusCode, body: Value) -> Response {
    (
        StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        Json(body),
    )
        .into_response()
}

/// Forward one call to Plaid with the master credentials injected. `Err` is a
/// response to return as is (not configured, or Plaid unreachable).
async fn call(state: &AppState, plaid_path: &str, body: &Value) -> Result<(reqwest::StatusCode, Value), Response> {
    let (Some(client_id), Some(secret)) =
        (&state.config.plaid_client_id, &state.config.plaid_secret)
    else {
        return Err(err(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_not_configured",
            "Plaid credentials not set",
        ));
    };
    // The box sent only the per-user `access_token` (+ cursor/params); we add
    // `client_id`+`secret` here so they never leave virtues-api.
    let outbound = with_credentials(body, client_id, secret);
    let url = format!("{}/{}", state.config.plaid_base_url, plaid_path);
    let resp = state
        .http_client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&outbound)
        .send()
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, "upstream_error", &e.without_url().to_string()))?;
    let status = resp.status();
    let v: Value = resp.json().await.unwrap_or_else(|_| json!({}));
    Ok((status, v))
}

/// A data call: forward it and return Plaid's answer, success or error, as is.
async fn proxy(state: &AppState, plaid_path: &str, body: &Value) -> Response {
    match call(state, plaid_path, body).await {
        Ok((status, v)) => passthrough(status, v),
        Err(resp) => resp,
    }
}

/// Return a copy of the request body with `client_id` + `secret` set. If the box
/// sent a non-object body (shouldn't happen), wrap it so we always send a valid
/// Plaid request object.
pub(crate) fn with_credentials(body: &Value, client_id: &str, secret: &str) -> Value {
    let mut obj = match body {
        Value::Object(map) => map.clone(),
        _ => serde_json::Map::new(),
    };
    obj.insert("client_id".to_string(), json!(client_id));
    obj.insert("secret".to_string(), json!(secret));
    Value::Object(obj)
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

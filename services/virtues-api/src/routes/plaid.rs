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
//! Any subscriber's key reaches these routes, so a key alone does not say
//! whose bank a token is. Each call, a removal included, is checked against
//! the banks recorded for the caller's account before Plaid acts on it.
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
    BearerAuth(wallet): BearerAuth,
    Json(body): Json<Value>,
) -> Response {
    proxy(&state, &wallet.account_id, "transactions/sync", &body).await
}

async fn accounts_get(
    State(state): State<Arc<AppState>>,
    BearerAuth(wallet): BearerAuth,
    Json(body): Json<Value>,
) -> Response {
    proxy(&state, &wallet.account_id, "accounts/get", &body).await
}

async fn investments_holdings_get(
    State(state): State<Arc<AppState>>,
    BearerAuth(wallet): BearerAuth,
    Json(body): Json<Value>,
) -> Response {
    proxy(&state, &wallet.account_id, "investments/holdings/get", &body).await
}

async fn liabilities_get(
    State(state): State<Arc<AppState>>,
    BearerAuth(wallet): BearerAuth,
    Json(body): Json<Value>,
) -> Response {
    proxy(&state, &wallet.account_id, "liabilities/get", &body).await
}

/// Remove a linked bank from Plaid, which ends its monthly fee, and free its
/// place under the account's bank limit. Called when the owner disconnects
/// the bank, or when the box lets go of its banks after a lapsed
/// subscription. Plaid's `item/remove` does not return the item id, so this
/// asks `item/get` first to know which place to free.
///
/// Only the account a bank is recorded for may remove it. A removal cannot be
/// undone at Plaid, so a leaked token must not be enough. Servers from the
/// release that carries virtues-core `credentials::pending_release` keep the
/// token until this answers that the bank is gone, so a refusal loses
/// nothing. Earlier servers erase it first, so for them a refusal leaves a
/// bank only support can remove. Ship the server release first.
pub(crate) async fn remove_item(state: &AppState, account_id: &str, access_token: &str) -> Response {
    if access_token.is_empty() {
        return err(StatusCode::BAD_REQUEST, "missing_access_token", "access_token is required");
    }
    let only_token = json!({ "access_token": access_token });
    let owner = match banks::token_owner(&state.db, account_id, access_token).await {
        Ok(banks::TokenOwner::Another) => return not_your_bank(),
        Ok(owner) => owner,
        Err(e) => return lookup_failed(e),
    };

    let item_id = match call(state, "item/get", &only_token).await {
        Ok((status, v)) if status.is_success() => v
            .pointer("/item/item_id")
            .and_then(|x| x.as_str())
            .map(String::from),
        // An item Plaid no longer knows is already removed. Plaid cannot name
        // it any more, so its place is freed by the token alone.
        Ok((_, v)) if plaid_error_code(&v) == Some("ITEM_NOT_FOUND") => {
            free_place(state, account_id, None, access_token).await;
            return removed();
        }
        Ok((status, v)) => return passthrough(status, v),
        Err(resp) => return resp,
    };
    // A token the table does not know: the bank must be the caller's, or no
    // account's, before it is removed.
    if owner == banks::TokenOwner::Unknown {
        if let Err(refusal) = claim_bank(state, account_id, item_id.as_deref(), access_token).await {
            return refusal;
        }
    }

    match call(state, "item/remove", &only_token).await {
        Ok((status, _)) if status.is_success() => {}
        Ok((_, v)) if plaid_error_code(&v) == Some("ITEM_NOT_FOUND") => {}
        Ok((status, v)) => return passthrough(status, v),
        Err(resp) => return resp,
    }
    free_place(state, account_id, item_id.as_deref(), access_token).await;
    removed()
}

/// Forget a bank Plaid has removed. The fee has stopped either way, so a
/// failure is only logged and the place under the limit stays taken. A row
/// from before tokens were recorded cannot be freed once Plaid no longer
/// names the bank.
async fn free_place(state: &AppState, account_id: &str, item_id: Option<&str>, access_token: &str) {
    if let Err(e) = banks::forget(&state.db, account_id, item_id, access_token).await {
        tracing::error!(error = %e, "plaid item removed, but freeing its place failed");
    }
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

/// Refuse a bank call whose access token is not one of the caller's banks.
/// `Err` is the response to return.
///
/// A token the table does not know costs one extra Plaid call, `item/get`,
/// to learn which bank it is; the answer is recorded, so later calls with
/// that token are a single lookup here.
async fn caller_owns_bank(state: &AppState, account_id: &str, access_token: &str) -> Result<(), Response> {
    if access_token.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "missing_access_token", "access_token is required"));
    }
    match banks::token_owner(&state.db, account_id, access_token).await {
        Ok(banks::TokenOwner::Caller) => return Ok(()),
        Ok(banks::TokenOwner::Another) => return Err(not_your_bank()),
        Ok(banks::TokenOwner::Unknown) => {}
        Err(e) => return Err(lookup_failed(e)),
    }
    let item_id = match call(state, "item/get", &json!({ "access_token": access_token })).await? {
        (status, v) if status.is_success() => v.pointer("/item/item_id").and_then(|x| x.as_str()).map(String::from),
        // Plaid's own refusal (a dead or malformed token) goes back as the
        // data call's would have.
        (status, v) => return Err(passthrough(status, v)),
    };
    claim_bank(state, account_id, item_id.as_deref(), access_token).await
}

/// Record a bank Plaid just named as the caller's, unless another account
/// has it (`banks::claim`). `Err` is the response to return.
async fn claim_bank(
    state: &AppState,
    account_id: &str,
    item_id: Option<&str>,
    access_token: &str,
) -> Result<(), Response> {
    let Some(item_id) = item_id else {
        tracing::error!("plaid item/get succeeded without an item_id");
        return Err(err(StatusCode::BAD_GATEWAY, "upstream_error", "Plaid did not name the bank"));
    };
    match banks::claim(&state.db, account_id, item_id, access_token).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(not_your_bank()),
        Err(e) => Err(lookup_failed(e)),
    }
}

fn not_your_bank() -> Response {
    tracing::warn!(code = "bank_not_yours", "plaid call refused: the bank is recorded for another account");
    err(
        StatusCode::FORBIDDEN,
        "bank_not_yours",
        "This bank was connected under a different Virtues account. Connect it again from this server, or link this server to that account.",
    )
}

/// The ownership lookup failed. The call is refused: serving it would skip
/// the check.
fn lookup_failed(e: anyhow::Error) -> Response {
    tracing::error!(error = %e, "linked bank lookup failed; plaid call refused");
    err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "Could not check this bank. Try again.")
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
async fn proxy(state: &AppState, account_id: &str, plaid_path: &str, body: &Value) -> Response {
    let token = body.get("access_token").and_then(|v| v.as_str()).unwrap_or_default();
    if let Err(refusal) = caller_owns_bank(state, account_id, token).await {
        return refusal;
    }
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

/// The bank routes against a stand-in Plaid.
#[cfg(test)]
mod route_tests {
    use super::*;
    use axum::http::Request;
    use sqlx::PgPool;
    use std::sync::Mutex;
    use tower::Service;

    /// A stand-in Plaid: token `tok-x` is bank `item-x`, and `tok-gone` is a
    /// bank Plaid no longer knows. Returns its address and the calls it got.
    async fn plaid() -> (String, Arc<Mutex<Vec<String>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let seen = calls.clone();
        let app = Router::new().route(
            "/*path",
            post(move |axum::extract::Path(path): axum::extract::Path<String>, Json(body): Json<Value>| {
                let token = body["access_token"].as_str().unwrap_or_default().to_string();
                seen.lock().unwrap().push(format!("{path} {token}"));
                async move {
                    if token == "tok-gone" {
                        return (StatusCode::BAD_REQUEST, Json(json!({ "error_code": "ITEM_NOT_FOUND" })));
                    }
                    let item = token.replacen("tok-", "item-", 1);
                    (StatusCode::OK, Json(json!({ "item": { "item_id": item }, "added": [] })))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (url, calls)
    }

    async fn app(pool: &PgPool, plaid_url: String) -> Router {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder().no_proxy().build().unwrap();
        let state = Arc::new(AppState {
            config: Arc::new(crate::config::Config {
                port: 0,
                internal_secret: "s".repeat(32),
                ai_gateway_api_key: String::new(),
                ai_gateway_url: String::new(),
                google_api_key: None,
                unsplash_access_key: None,
                plaid_client_id: Some("cid".into()),
                plaid_secret: Some("sec".into()),
                truto_api_token: None,
                truto_api_url: String::new(),
                plaid_base_url: plaid_url,
                maps_dir: std::path::PathBuf::new(),
                maps_bucket: None,
            }),
            http_client: http.clone(),
            stream_client: http,
            db: pool.clone(),
            blocklist: crate::blocklist::Blocklist::from_env(),
            catalog: crate::catalog::Catalog::new(),
        });
        for (key, account) in [("key-a", "acct-a"), ("key-b", "acct-b")] {
            let hash = crate::bearer_auth::key_hash(key);
            crate::entitlement::register_device(pool, &hash, account, None).await.unwrap();
            crate::entitlement::credit(pool, account, 1_000_000, crate::entitlement::CreditMode::Set, None)
                .await
                .unwrap();
        }
        router().merge(super::super::oauth::router()).with_state(state)
    }

    async fn call_as(app: &Router, key: &str, path: &str, body: Value) -> (StatusCode, Value) {
        // The data routes read the key from `Authorization`, the revoke route
        // from `x-virtues-api-key`.
        let req = Request::post(path)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {key}"))
            .header("x-virtues-api-key", key);
        let res = app
            .clone()
            .call(req.body(axum::body::Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let raw = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
        (status, serde_json::from_slice(&raw).unwrap_or(Value::Null))
    }

    const DATA: [&str; 4] = [
        "/v1/services/plaid/transactions/sync",
        "/v1/services/plaid/accounts/get",
        "/v1/services/plaid/investments/holdings/get",
        "/v1/services/plaid/liabilities/get",
    ];

    async fn data(app: &Router, key: &str, path: &str, token: &str) -> (StatusCode, Value) {
        call_as(app, key, path, json!({ "access_token": token })).await
    }

    async fn remove(app: &Router, key: &str, token: &str) -> StatusCode {
        call_as(app, key, "/plaid/revoke", json!({ "secrets": { "access_token": token } })).await.0
    }

    /// Another account's token is refused on every data route, before Plaid
    /// sees it.
    #[sqlx::test]
    async fn a_data_call_is_served_only_for_the_callers_own_bank(pool: PgPool) {
        let (url, calls) = plaid().await;
        let app = app(&pool, url).await;
        banks::admit(&pool, "acct-a", "item-a", "tok-a", 10).await.unwrap();
        for path in DATA {
            assert_eq!(data(&app, "key-a", path, "tok-a").await.0, StatusCode::OK, "{path}");
            calls.lock().unwrap().clear();
            let (status, body) = data(&app, "key-b", path, "tok-a").await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
            assert_eq!(body["error"]["code"], "bank_not_yours", "{path}");
            assert!(calls.lock().unwrap().is_empty(), "{path} reached Plaid");
            assert_eq!(data(&app, "key-a", path, "").await.0, StatusCode::BAD_REQUEST, "{path}");
        }
    }

    /// A bank recorded without a token costs one lookup, then none.
    #[sqlx::test]
    async fn an_unrecorded_token_is_looked_up_once(pool: PgPool) {
        let (url, calls) = plaid().await;
        let app = app(&pool, url).await;
        assert_eq!(data(&app, "key-a", DATA[1], "tok-a").await.0, StatusCode::OK);
        assert_eq!(data(&app, "key-a", DATA[1], "tok-a").await.0, StatusCode::OK);
        assert_eq!(*calls.lock().unwrap(), ["item/get tok-a", "accounts/get tok-a", "accounts/get tok-a"]);
        assert_eq!(data(&app, "key-b", DATA[1], "tok-a").await.0, StatusCode::FORBIDDEN);
    }

    async fn rows(pool: &PgPool) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM linked_banks").fetch_one(pool).await.unwrap()
    }

    /// A leaked token is not enough to disconnect someone's bank: not when
    /// the token is recorded, and not when only the bank is.
    #[sqlx::test]
    async fn only_the_account_that_has_a_bank_can_remove_it(pool: PgPool) {
        let (url, calls) = plaid().await;
        let app = app(&pool, url).await;
        banks::admit(&pool, "acct-a", "item-a", "tok-a", 10).await.unwrap();
        banks::admit(&pool, "acct-a", "item-old", "tok-old", 10).await.unwrap();
        sqlx::query("UPDATE linked_banks SET token_hash = NULL WHERE token_hash = $1")
            .bind(<sha2::Sha256 as sha2::Digest>::digest(b"tok-old").to_vec())
            .execute(&pool)
            .await
            .unwrap();
        for token in ["tok-a", "tok-old"] {
            assert_eq!(remove(&app, "key-b", token).await, StatusCode::FORBIDDEN, "{token}");
        }
        assert!(!calls.lock().unwrap().iter().any(|c| c.starts_with("item/remove")));
        assert_eq!(rows(&pool).await, 2);
        for token in ["tok-a", "tok-old"] {
            assert_eq!(remove(&app, "key-a", token).await, StatusCode::OK, "{token}");
        }
        assert_eq!(rows(&pool).await, 0);
    }

    /// A bank no account has recorded can be removed by the server holding
    /// its token, so one linked before the table existed is not stuck.
    #[sqlx::test]
    async fn an_unrecorded_bank_can_be_removed(pool: PgPool) {
        let (url, calls) = plaid().await;
        let app = app(&pool, url).await;
        assert_eq!(remove(&app, "key-a", "tok-x").await, StatusCode::OK);
        assert!(calls.lock().unwrap().contains(&"item/remove tok-x".to_string()));
        assert_eq!(rows(&pool).await, 0);
    }

    #[sqlx::test]
    async fn removal_needs_a_key_we_issued(pool: PgPool) {
        let (url, calls) = plaid().await;
        let app = app(&pool, url).await;
        assert_eq!(remove(&app, "key-nobody", "tok-a").await, StatusCode::UNAUTHORIZED);
        assert!(calls.lock().unwrap().is_empty());
    }

    /// Plaid already removed it: the place is freed by the token alone.
    #[sqlx::test]
    async fn a_bank_plaid_no_longer_knows_frees_its_place(pool: PgPool) {
        let (url, _) = plaid().await;
        let app = app(&pool, url).await;
        banks::admit(&pool, "acct-a", "item-gone", "tok-gone", 10).await.unwrap();
        assert_eq!(remove(&app, "key-a", "tok-gone").await, StatusCode::OK);
        let left: i64 = sqlx::query_scalar("SELECT count(*) FROM linked_banks").fetch_one(&pool).await.unwrap();
        assert_eq!(left, 0);
    }
}

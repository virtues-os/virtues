//! Source-connect HTTP handlers.
//!
//! Five thin axum routes that drive the source catalog connect flows. Each
//! handler is ~30–50 lines: validate, call `virtues_helpers::auth::*`,
//! return JSON or 302. No subprocess spawn, no run-row writes.
//!
//! Routes (mounted in `core/src/server/mod.rs`):
//!
//! ```text
//! POST /api/pairing/initiate                  pair_initiate
//! POST /api/pairing/complete/:credential_id   pair_complete
//! POST /api/connect/:source_id/start          oauth_start
//! GET  /oauth/callback                        oauth_callback
//! POST /api/connect/:source_id/complete       apikey_complete
//! ```
//!
//! These coexist with the legacy `/api/devices/pairing/...` routes during
//! the dual-path window (Phase 6 deletes the legacy path).

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use virtues_helpers::auth::{
    fanout_action_ids, finalize_apikey_credential, finalize_credential,
    finalize_self_issued_bearer, mint_pending_credential, proxy_exchange, sign_oauth_state,
    verify_oauth_state, AuthError,
};

use crate::action_templates::{lookup_source, reconcile_templates, SourceAuth};
use crate::server::webhook::AppState;

// ─────────────────────────────────────────────────────────────────────────────
// Error mapping
// ─────────────────────────────────────────────────────────────────────────────

fn auth_error_response(err: AuthError) -> Response {
    let status = StatusCode::from_u16(err.http_status())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    tracing::warn!(error = %err, status = err.http_status(), "auth handler error");
    (status, Json(serde_json::json!({ "error": err.to_string() }))).into_response()
}

fn bad_request(msg: impl Into<String>) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": msg.into() })),
    )
        .into_response()
}

fn not_found(msg: impl Into<String>) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": msg.into() })),
    )
        .into_response()
}

// ─────────────────────────────────────────────────────────────────────────────
// pair_initiate — POST /api/pairing/initiate
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PairInitiateRequest {
    pub source_id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct PairInitiateResponse {
    pub credential_id: String,
    /// Opaque payload the device renders as a QR code. Today: just the
    /// credential_id; future iterations may include a server URL.
    pub qr_payload: String,
}

pub async fn pair_initiate_handler(
    State(state): State<AppState>,
    Json(req): Json<PairInitiateRequest>,
) -> Response {
    let Some(source) = lookup_source(&req.source_id) else {
        return not_found(format!("unknown source: {}", req.source_id));
    };

    if !matches!(source.auth, SourceAuth::SelfIssuedBearer) {
        return bad_request(format!(
            "source '{}' uses auth.kind = {} — pair flow only applies to self_issued_bearer",
            req.source_id,
            source.auth.kind_str()
        ));
    }

    match mint_pending_credential(state.db.pool(), &req.source_id, &req.name).await {
        Ok(credential_id) => {
            let qr_payload = credential_id.clone();
            (
                StatusCode::CREATED,
                Json(PairInitiateResponse {
                    credential_id,
                    qr_payload,
                }),
            )
                .into_response()
        }
        Err(e) => auth_error_response(e),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// pair_complete — POST /api/pairing/complete/:credential_id
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PairCompleteRequest {
    /// The plaintext bearer token the device generated. Encrypted server-side
    /// and the HMAC stored as `secret_lookup_hash` for O(1) webhook auth.
    pub token: String,
    /// Plaintext non-secret context (device name, model, OS version, etc.)
    /// stored in `credentials.metadata`.
    #[serde(default)]
    pub device_info: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct PairCompleteResponse {
    pub credential_id: String,
    /// Map of `function_name → app_actions.id` for the per-credential fan-out.
    /// The device stores these and routes each stream flush to
    /// `POST /webhook/{action_id}`.
    pub action_ids: HashMap<String, String>,
}

pub async fn pair_complete_handler(
    State(state): State<AppState>,
    Path(credential_id): Path<String>,
    Json(req): Json<PairCompleteRequest>,
) -> Response {
    let pool = state.db.pool();

    if let Err(e) =
        finalize_self_issued_bearer(pool, &credential_id, &req.token, &req.device_info).await
    {
        return auth_error_response(e);
    }

    // Trigger reconcile so per-credential action rows get fanned out before
    // we look them up.
    if let Err(e) = reconcile_templates(pool).await {
        tracing::error!(error = %e, "reconcile after pair_complete failed");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("reconcile failed: {e}") })),
        )
            .into_response();
    }

    let action_ids = match fanout_action_ids(pool, &credential_id).await {
        Ok(map) => map,
        Err(e) => return auth_error_response(e),
    };

    (
        StatusCode::OK,
        Json(PairCompleteResponse {
            credential_id,
            action_ids,
        }),
    )
        .into_response()
}

// ─────────────────────────────────────────────────────────────────────────────
// oauth_start — POST /api/connect/:source_id/start
// ─────────────────────────────────────────────────────────────────────────────

const PROXY_URL: &str = "https://auth.virtues.com";

#[derive(Debug, Deserialize, Default)]
pub struct OauthStartRequest {
    /// Set when re-authing an existing credential (token revoked, new scopes
    /// needed). `None` for fresh connects — the callback mints a new row.
    #[serde(default)]
    pub existing_credential_id: Option<String>,
    /// Where the proxy should send the user's browser after the dance.
    /// Defaults to the canonical `<self>/oauth/callback`.
    #[serde(default)]
    pub return_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OauthStartResponse {
    pub redirect_url: String,
}

pub async fn oauth_start_handler(
    State(_state): State<AppState>,
    Path(source_id): Path<String>,
    Json(req): Json<OauthStartRequest>,
) -> Response {
    let Some(source) = lookup_source(&source_id) else {
        return not_found(format!("unknown source: {source_id}"));
    };

    let start_path = match &source.auth {
        SourceAuth::ViaProxy { start_path } => start_path.as_str(),
        other => {
            return bad_request(format!(
                "source '{source_id}' uses auth.kind = {} — oauth_start only applies to via_proxy",
                other.kind_str()
            ));
        }
    };

    let signed_state =
        match sign_oauth_state(&source_id, req.existing_credential_id.as_deref()) {
            Ok(s) => s,
            Err(e) => return auth_error_response(e),
        };

    // Default return_url: the caller's instance hits its own /oauth/callback.
    // The proxy round-trips it; we don't enforce it server-side because the
    // signed state is what authenticates the callback. For self-hosted
    // override (Phase 8+) we'd consult VIRTUES_OAUTH_PROXY_URL env.
    let return_url = req
        .return_url
        .unwrap_or_else(|| "/oauth/callback".to_string());

    let redirect_url = format!(
        "{proxy}{start}?return_url={ret}&state={state}",
        proxy = PROXY_URL,
        start = start_path,
        ret = urlencoding::encode(&return_url),
        state = urlencoding::encode(&signed_state),
    );

    (StatusCode::OK, Json(OauthStartResponse { redirect_url })).into_response()
}

// ─────────────────────────────────────────────────────────────────────────────
// oauth_callback — GET /oauth/callback?state=...&exchange_token=...
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct OauthCallbackQuery {
    pub state: String,
    pub exchange_token: String,
}

pub async fn oauth_callback_handler(
    State(state): State<AppState>,
    Query(q): Query<OauthCallbackQuery>,
) -> Response {
    let pool = state.db.pool();

    // 1. Verify the signed state (HMAC + expiry).
    let claims = match verify_oauth_state(&q.state) {
        Ok(c) => c,
        Err(e) => return auth_error_response(e),
    };

    // 2. POST the exchange token to the proxy; receive normalized
    //    {secrets, metadata, expires_in, scopes}.
    let resp = match proxy_exchange(&claims.source_id, &q.exchange_token).await {
        Ok(r) => r,
        Err(e) => return auth_error_response(e),
    };

    // 3. Mint a pending row if this is a fresh connect, otherwise reuse the
    //    existing credential id from the state claims (reauth flow).
    let credential_id = match claims.existing_credential_id {
        Some(id) => id,
        None => {
            // Default name = the user's email if metadata carries one,
            // otherwise the source display_name + "account".
            let default_name = resp
                .metadata
                .get("email")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_else(|| {
                    lookup_source(&claims.source_id)
                        .map(|s| format!("{} account", s.display_name))
                        .unwrap_or_else(|| "Connected account".to_string())
                });
            match mint_pending_credential(pool, &claims.source_id, &default_name).await {
                Ok(id) => id,
                Err(e) => return auth_error_response(e),
            }
        }
    };

    // 4. Encrypt secrets, write the row (UPDATE WHERE status='pending' for
    //    idempotent dedup of double-callbacks).
    if let Err(e) = finalize_credential(
        pool,
        &credential_id,
        &resp.secrets,
        &resp.metadata,
        resp.expires_in,
        resp.scopes.as_deref(),
    )
    .await
    {
        return auth_error_response(e);
    }

    // 5. Reconcile so per-credential fan-out picks up.
    if let Err(e) = reconcile_templates(pool).await {
        tracing::error!(error = %e, "reconcile after oauth_callback failed");
        // Don't fail the user's redirect — the credential is active; the next
        // reconcile (startup or another callback) will catch up.
    }

    // 6. Redirect the browser to the Sources tab with a success marker.
    let location = format!("/sources?connected={}", claims.source_id);
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::LOCATION,
        HeaderValue::from_str(&location).unwrap_or(HeaderValue::from_static("/")),
    );
    (StatusCode::FOUND, headers).into_response()
}

// ─────────────────────────────────────────────────────────────────────────────
// apikey_complete — POST /api/connect/:source_id/complete
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ApiKeyCompleteRequest {
    pub name: String,
    /// Field values the form collected. `{"token": "..."}` for single-field;
    /// `{"key1": "...", "key2": "..."}` for multi-field connectors.
    pub fields: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct ApiKeyCompleteResponse {
    pub credential_id: String,
}

pub async fn apikey_complete_handler(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Json(req): Json<ApiKeyCompleteRequest>,
) -> Response {
    let pool = state.db.pool();

    let Some(source) = lookup_source(&source_id) else {
        return not_found(format!("unknown source: {source_id}"));
    };

    let expected_fields = match &source.auth {
        SourceAuth::ApiKey { fields } => fields,
        other => {
            return bad_request(format!(
                "source '{source_id}' uses auth.kind = {} — apikey_complete only applies to api_key",
                other.kind_str()
            ));
        }
    };

    // Validate that every declared field has a non-empty string value.
    let Some(obj) = req.fields.as_object() else {
        return bad_request("`fields` must be a JSON object");
    };
    for field in expected_fields {
        match obj.get(field).and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => {}
            _ => {
                return bad_request(format!("missing or empty field: {field}"));
            }
        }
    }

    let credential_id =
        match finalize_apikey_credential(pool, &source_id, &req.name, &req.fields).await {
            Ok(id) => id,
            Err(e) => return auth_error_response(e),
        };

    // Reconcile so per-credential fan-out picks up (e.g. MCP server actions).
    if let Err(e) = reconcile_templates(pool).await {
        tracing::error!(error = %e, "reconcile after apikey_complete failed");
    }

    (
        StatusCode::CREATED,
        Json(ApiKeyCompleteResponse { credential_id }),
    )
        .into_response()
}

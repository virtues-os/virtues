//! Source-connect HTTP handlers.
//!
//! Three thin axum routes that drive the source catalog connect flows. Each
//! handler is ~30–50 lines: validate, call `virtues_helpers::auth::*`,
//! return JSON or 302. No subprocess spawn, no run-row writes.
//!
//! Routes (mounted in `core/src/server/mod.rs`):
//!
//! ```text
//! POST /api/connect/:source_id/start          oauth_start
//! GET  /oauth/callback                        oauth_callback
//! POST /api/connect/:source_id/complete       apikey_complete
//! ```
//!
//! Device pairing (iOS / Mac / sensor) lives at `/api/pair/*` — the unified
//! pair-only flow in `crate::api::pair`. The legacy `/api/pairing/initiate`
//! and `/api/pairing/complete/:id` endpoints were removed in v1; iOS now
//! pairs via `/api/pair/consume` with `kind = "mobile_app"`.

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

use virtues_helpers::auth::{
    finalize_apikey_credential, finalize_credential, mint_pending_credential, proxy_exchange,
    proxy_url, sign_oauth_state, verify_oauth_state, AuthError,
};

use crate::applet_templates::{lookup_source, reconcile_templates, SourceAuth};
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
// oauth_start — POST /api/connect/:source_id/start
// ─────────────────────────────────────────────────────────────────────────────

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
    // signed state is what authenticates the callback.
    let return_url = req
        .return_url
        .unwrap_or_else(|| "/oauth/callback".to_string());

    let redirect_url = format!(
        "{proxy}{start}?return_url={ret}&state={state}",
        proxy = proxy_url(),
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
    /// Absent whenever the proxy bounced back an `error` instead — a required
    /// field here used to turn every such bounce into an opaque 422 from the
    /// query extractor, before the handler could say what went wrong. Plaid's
    /// Hosted Link makes that a routine path (Plaid redirects on user-cancel
    /// exactly as it does on success), so the error leg has to be first-class.
    #[serde(default)]
    pub exchange_token: Option<String>,
    /// Proxy-side failure code: `connect_cancelled` when the user backed out of
    /// the provider's flow, `token_exchange_failed` / `provider_error` when the
    /// provider itself refused.
    #[serde(default)]
    pub error: Option<String>,
    /// `"native"` when the connect was started from a Tauri shell (Mac/iOS),
    /// where OAuth ran in the system browser. A 302 to `/sources` would strand
    /// the user on a second copy of the app in a browser tab, so we render a
    /// terminal "return to Virtues" page instead. Absent for browser connects.
    #[serde(default)]
    pub shell: Option<String>,
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

    // 1b. The provider leg didn't produce a token. Cancelling is a normal thing
    //     to do, so hand the user back the same way a success does — no
    //     credential is written and nothing is logged as a fault.
    let exchange_token = match (&q.exchange_token, &q.error) {
        (Some(t), _) if !t.is_empty() => t.clone(),
        (_, reason) => {
            let code = reason.as_deref().unwrap_or("no_exchange_token");
            tracing::info!(
                source_id = %claims.source_id,
                reason = %code,
                "oauth callback returned without an exchange token"
            );
            return oauth_incomplete_response(&claims.source_id, code, q.shell.as_deref());
        }
    };

    // 2. POST the exchange token to the proxy; receive normalized
    //    {secrets, metadata, expires_in, scopes}.
    let resp = match proxy_exchange(&claims.source_id, &exchange_token).await {
        Ok(r) => r,
        Err(e) => return auth_error_response(e),
    };

    // 3. Mint a pending row if this is a fresh connect, otherwise reuse the
    //    existing credential id from the state claims (reauth flow).
    let credential_id = match claims.existing_credential_id {
        Some(id) => id,
        None => {
            // Default name = whatever the provider gave us that identifies this
            // particular connection: the account's email, or the proxy's
            // `display_name` (Plaid puts the bank's name there — "Plaid account"
            // tells the user nothing once they've connected three of them).
            // Both keys are generic on purpose; core stays provider-agnostic.
            let default_name = resp
                .metadata
                .get("email")
                .or_else(|| resp.metadata.get("display_name"))
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

    // 6. Hand the browser back. A native (Tauri) connect ran OAuth in the system
    //    browser, so a 302 into `/sources` would leave the user staring at a second
    //    copy of the app in a browser tab with no way back. Render a terminal
    //    success page instead; the app's own focus handler refreshes the source
    //    list when the user switches back to it. Browser connects keep the 302.
    if claims_shell_is_native(q.shell.as_deref()) {
        return oauth_return_page(&claims.source_id).into_response();
    }
    let location = format!("/sources?connected={}", claims.source_id);
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::LOCATION,
        HeaderValue::from_str(&location).unwrap_or(HeaderValue::from_static("/")),
    );
    (StatusCode::FOUND, headers).into_response()
}

fn claims_shell_is_native(shell: Option<&str>) -> bool {
    shell == Some("native")
}

/// The connect ended without a credential. Mirrors the success path's two
/// shells: a terminal page for native (the system browser has nowhere to go),
/// a 302 back into the app for browser connects. Never an HTTP error — the
/// user didn't do anything wrong, and most of the time they simply cancelled.
fn oauth_incomplete_response(source_id: &str, reason: &str, shell: Option<&str>) -> Response {
    if claims_shell_is_native(shell) {
        let source = source_display_name(source_id);
        let (heading, detail) = if reason == "connect_cancelled" {
            (
                format!("{source} wasn't connected"),
                "You closed the connection flow before it finished. You can close this tab and try again from Virtues.",
            )
        } else {
            (
                format!("Couldn't finish connecting {source}"),
                "Nothing was connected. You can close this tab and try again from Virtues.",
            )
        };
        return terminal_page("Not connected — Virtues", "—", &heading, detail);
    }
    let location = format!(
        "/sources?source={}&error={}",
        urlencoding::encode(source_id),
        urlencoding::encode(reason)
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::LOCATION,
        HeaderValue::from_str(&location).unwrap_or(HeaderValue::from_static("/sources")),
    );
    (StatusCode::FOUND, headers).into_response()
}

/// Terminal page shown after a native-shell OAuth connect finishes in the system
/// browser. Self-contained (no asset deps, CSP-safe) so it renders anywhere.
fn oauth_return_page(source_id: &str) -> Response {
    let heading = format!("{} connected", source_display_name(source_id));
    terminal_page(
        "Connected — Virtues",
        "✓",
        &heading,
        "You can close this tab and return to Virtues.",
    )
}

fn source_display_name(source_id: &str) -> String {
    lookup_source(source_id)
        .map(|s| s.display_name.to_string())
        .unwrap_or_else(|| "Your account".to_string())
}

fn terminal_page(title: &str, mark: &str, heading: &str, detail: &str) -> Response {
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let (title, heading, detail) = (esc(title), esc(heading), esc(detail));
    let body = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <meta name="referrer" content="no-referrer">
  <title>{title}</title>
  <style>
    body {{ font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto,
            "Helvetica Neue", Arial, sans-serif; max-width: 480px; margin: 0 auto;
            padding: 64px 24px; color: #1f2937; background: #f9fafb; line-height: 1.5;
            text-align: center; }}
    .mark {{ font-size: 40px; line-height: 1; margin-bottom: 16px; }}
    h1 {{ font-size: 22px; margin: 0 0 8px; }}
    p  {{ font-size: 15px; color: #4b5563; margin: 0; }}
  </style>
</head>
<body>
  <div class="mark">{mark}</div>
  <h1>{heading}</h1>
  <p>{detail}</p>
</body>
</html>"#,
    );
    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body,
    )
        .into_response()
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

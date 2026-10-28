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
    response::{IntoResponse, Redirect, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use virtues_helpers::auth::{
    finalize_apikey_credential, finalize_credential, mint_pending_credential, proxy_exchange,
    proxy_url, sign_oauth_state, vault, verify_oauth_state, AuthError, ProxyExchangeResponse,
};
use virtues_helpers::crypto::TokenEncryptor;

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

/// Check that `credential_id` is a sign-in of `source_id` that a reconnect
/// can repair. A disconnected (`revoked`) row cannot be repaired. The owner
/// removed it, the Sources list no longer shows it, and connecting the source
/// again adds a new sign-in. PATCH /api/credentials/:id refuses to re-activate
/// a revoked row by the same rule.
async fn reconnect_target(
    pool: &PgPool,
    credential_id: &str,
    source_id: &str,
) -> std::result::Result<(), Response> {
    let row: Option<(String, String)> = match sqlx::query_as(
        "SELECT key, status FROM credentials WHERE id = $1 AND kind = 'source'",
    )
    .bind(credential_id)
    .fetch_optional(pool)
    .await
    {
        Ok(row) => row,
        Err(e) => {
            tracing::error!(error = %e, "credential lookup failed");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "credential lookup failed" })),
            )
                .into_response());
        }
    };
    match row {
        Some((key, status)) if key == source_id => {
            if status == "revoked" {
                return Err((
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({
                        "error": format!(
                            "This sign-in was disconnected. Connect {} again to add a new one.",
                            source_display_name(source_id)
                        )
                    })),
                )
                    .into_response());
            }
            Ok(())
        }
        _ => Err(not_found(format!("no {source_id} credential {credential_id}"))),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// oauth_start — POST /api/connect/:source_id/start
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Default)]
pub struct OauthStartRequest {
    /// Reconnecting: the callback repairs this credential in place instead
    /// of adding a second one, so its applets, sync cursors and name carry on.
    /// `None` for a fresh connect, which adds a new row.
    ///
    /// Named like `credential_id` on `/complete`. A released box reads only
    /// `existing_credential_id` and cannot finish a reconnect with it: its
    /// callback updates only a pending row, so it answers 409 for a broken row
    /// once the provider's code is spent, and drops the new tokens for an
    /// active one. The app sends this name instead, which such a box ignores,
    /// and that box adds a new row.
    #[serde(default)]
    pub credential_id: Option<String>,
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
    State(state): State<AppState>,
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

    // Check the row before sending the owner to the provider, so a reconnect
    // that cannot repair it fails here and not after they sign in.
    if let Some(id) = &req.credential_id {
        if let Err(response) = reconnect_target(state.db.pool(), id, &source_id).await {
            return response;
        }
    }

    let signed_state = match sign_oauth_state(&source_id, req.credential_id.as_deref()) {
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
    /// `"1"` when the iOS app opened the sign-in in its sign-in sheet
    /// (`SignInSheet.swift`). The box then sends the sheet to
    /// [`CLOSE_SHEET`], which closes it, instead of showing a page in it.
    #[serde(default)]
    pub sheet: Option<String>,
    /// Where the browser leg's 302 should land instead of the default
    /// `/sources` — a connect started from onboarding must return to the
    /// onboarding screen it left, not dump the person into the app. Set by the
    /// SPA on the return_url; round-trips through the proxy like `shell`.
    /// Local paths only — anything else falls back to `/sources`.
    #[serde(default)]
    pub next: Option<String>,
}

/// Accept only a same-origin absolute path: starts with exactly one `/`
/// (`//host` is scheme-relative and escapes the origin), no scheme, no
/// backslashes. Anything else gets the default.
fn safe_next(next: Option<&str>) -> Option<&str> {
    next.filter(|n| {
        n.starts_with('/') && !n.starts_with("//") && !n.contains('\\') && !n.contains("://")
    })
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
            if q.sheet.as_deref() == Some("1") {
                return Redirect::to(CLOSE_SHEET).into_response();
            }
            return oauth_incomplete_response(
                &claims.source_id,
                code,
                q.shell.as_deref(),
                q.next.as_deref(),
            );
        }
    };

    // 2. POST the exchange token to the proxy; receive normalized
    //    {secrets, metadata, expires_in, scopes}. The box identifies itself
    //    with its Virtues key when it has one — an unlinked box is a normal
    //    caller here (connecting a source needs no account), so absence is
    //    fine; a vault that cannot be read is logged, not treated as absence.
    let api_key = match virtues_helpers::auth::read_box_api_key(pool).await {
        Ok(k) => k,
        Err(e) => {
            tracing::warn!(error = %e, "could not read box api_key for proxy exchange");
            None
        }
    };
    let resp = match proxy_exchange(&claims.source_id, &exchange_token, api_key.as_deref()).await {
        Ok(r) => r,
        Err(e) => return auth_error_response(e),
    };

    // 3. Seal the grant: into the row being reconnected, or a new one.
    if let Err(e) = store_grant(
        pool,
        &claims.source_id,
        claims.existing_credential_id.as_deref(),
        &resp,
    )
    .await
    {
        return auth_error_response(e);
    }

    // 4. Reconcile so per-credential fan-out picks up.
    if let Err(e) = reconcile_templates(pool).await {
        tracing::error!(error = %e, "reconcile after oauth_callback failed");
        // Don't fail the user's redirect — the credential is active; the next
        // reconcile (startup or another callback) will catch up.
    }

    // 5. Hand the browser back. A native (Tauri) connect ran OAuth in the system
    //    browser, so a 302 into `/sources` would leave the user staring at a second
    //    copy of the app in a browser tab with no way back. Render a terminal
    //    success page instead; the app's own focus handler refreshes the source
    //    list when the user switches back to it. Browser connects keep the 302.
    if q.sheet.as_deref() == Some("1") {
        return Redirect::to(CLOSE_SHEET).into_response();
    }
    if claims_shell_is_native(q.shell.as_deref()) {
        return oauth_return_page(&claims.source_id).into_response();
    }
    let base = safe_next(q.next.as_deref()).unwrap_or("/sources");
    let sep = if base.contains('?') { '&' } else { '?' };
    let location = format!("{base}{sep}connected={}", claims.source_id);
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::LOCATION,
        HeaderValue::from_str(&location).unwrap_or(HeaderValue::from_static("/")),
    );
    (StatusCode::FOUND, headers).into_response()
}

/// Store the grant the proxy exchanged and return its credential id. A
/// reconnect repairs the row it was started from. A fresh connect adds a new
/// row. So does a reconnect whose row was deleted or disconnected while the
/// owner was at the provider, so the sign-in they just finished is kept, and
/// one where the owner signed in to a different account than the row's.
async fn store_grant(
    pool: &PgPool,
    source_id: &str,
    reconnecting: Option<&str>,
    grant: &ProxyExchangeResponse,
) -> virtues_helpers::auth::Result<String> {
    if let Some(id) = reconnecting {
        if !same_account(pool, id, grant).await? {
            tracing::info!(
                credential_id = id,
                source_id,
                "the reconnect signed in to a different account; adding a new credential"
            );
        } else if vault::reauth_credential(
            pool,
            id,
            source_id,
            &grant.secrets,
            Some(&grant.metadata),
            grant.expires_in,
        )
        .await?
        {
            return Ok(id.to_string());
        } else {
            tracing::info!(
                credential_id = id,
                source_id,
                "the credential being reconnected is gone or disconnected; adding a new one"
            );
        }
    }

    // Default name = whatever the provider gave us that identifies this
    // particular connection: the account's email, or the proxy's
    // `display_name` (Plaid puts the bank's name there — "Plaid account"
    // tells the user nothing once they've connected three of them).
    // Both keys are generic on purpose; core stays provider-agnostic.
    let default_name = grant
        .metadata
        .get("email")
        .or_else(|| grant.metadata.get("display_name"))
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| {
            lookup_source(source_id)
                .map(|s| format!("{} account", s.display_name))
                .unwrap_or_else(|| "Connected account".to_string())
        });
    let id = mint_pending_credential(pool, source_id, &default_name).await?;
    // UPDATE … WHERE status = 'pending', so a second callback is a no-op.
    finalize_credential(pool, &id, &grant.secrets, &grant.metadata, grant.expires_in).await?;
    Ok(id)
}

/// The provider account a grant was made for, where the proxy reports one:
/// a Strava athlete, a Plaid item, a Notion workspace. A Google grant names
/// none, because the proxy returns only its scopes.
fn grant_account(secrets: &serde_json::Value, metadata: &serde_json::Value) -> Option<String> {
    [
        metadata.pointer("/athlete/id"),
        metadata.pointer("/item_id"),
        secrets.pointer("/workspace_id"),
    ]
    .into_iter()
    .flatten()
    .find(|v| !v.is_null())
    .map(|v| v.to_string())
}

/// Whether a reconnect's grant is for the account the row already holds.
/// Repairing the row with another account's grant would sync that account's
/// data under the row's name and from its cursors, so that grant becomes a
/// sign-in of its own. When the grant or the row names no account, the grant
/// repairs the row.
async fn same_account(
    pool: &PgPool,
    credential_id: &str,
    grant: &ProxyExchangeResponse,
) -> virtues_helpers::auth::Result<bool> {
    let Some(new) = grant_account(&grant.secrets, &grant.metadata) else {
        return Ok(true);
    };
    let row: Option<(serde_json::Value, String)> = sqlx::query_as(
        "SELECT metadata, secrets_ciphertext FROM credentials WHERE id = $1 AND kind = 'source'",
    )
    .bind(credential_id)
    .fetch_optional(pool)
    .await?;
    // A row that is gone fails the repair, which then adds a new row.
    let Some((metadata, ciphertext)) = row else {
        return Ok(true);
    };
    // Decrypted here rather than through `read_credential_secrets`, because
    // this comparison is not a use and must not set `last_seen_at` on a row
    // that may stay broken.
    let opened: virtues_helpers::auth::Result<serde_json::Value> = TokenEncryptor::from_env()?
        .decrypt(&ciphertext)
        .map_err(AuthError::from)
        .and_then(|plaintext| Ok(serde_json::from_str(&plaintext)?));
    let secrets = match opened {
        Ok(secrets) => secrets,
        // Secrets that no longer open, such as ones sealed under another key
        // before a restore, are one thing a reconnect repairs. The metadata
        // decides alone.
        Err(e) => {
            tracing::warn!(credential_id, error = %e, "could not open the credential being reconnected");
            serde_json::Value::Null
        }
    };
    Ok(grant_account(&secrets, &metadata).map_or(true, |old| old == new))
}

/// Sending the iOS app's sign-in sheet here closes it. Only the sheet
/// listens for this scheme, so it carries nothing; the app reloads its
/// sources when the sheet closes.
const CLOSE_SHEET: &str = "virtues-signin://done";

fn claims_shell_is_native(shell: Option<&str>) -> bool {
    shell == Some("native")
}

/// The connect ended without a credential. Mirrors the success path's two
/// shells: a terminal page for native (the system browser has nowhere to go),
/// a 302 back into the app for browser connects. Never an HTTP error — the
/// user didn't do anything wrong, and most of the time they simply cancelled.
fn oauth_incomplete_response(
    source_id: &str,
    reason: &str,
    shell: Option<&str>,
    next: Option<&str>,
) -> Response {
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
    let base = safe_next(next).unwrap_or("/sources");
    let sep = if base.contains('?') { '&' } else { '?' };
    let location = format!(
        "{base}{sep}source={}&error={}",
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
    /// Reconnecting: replace this credential's secrets rather than add a
    /// second one, so its applets, history and name carry on and the broken
    /// row does not linger beside a new one.
    #[serde(default)]
    pub credential_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ApiKeyCompleteResponse {
    pub credential_id: String,
}

pub async fn apikey_complete_handler(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Json(mut req): Json<ApiKeyCompleteRequest>,
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

    // A sign-in through the app also sends `jar`, the whole session.
    crate::credentials::jar::clean(&mut req.fields);
    let expires_at = source
        .login
        .as_ref()
        .and_then(|l| crate::credentials::jar::login_expiry(&req.fields, &l.cookies));

    let credential_id = match &req.credential_id {
        Some(existing) => {
            if let Err(response) = reconnect_target(pool, existing, &source_id).await {
                return response;
            }
            match vault::reauth_credential(pool, existing, &source_id, &req.fields, None, None).await {
                Ok(true) => existing.clone(),
                // Disconnected or removed since the check above.
                Ok(false) => {
                    return auth_error_response(AuthError::Conflict(format!(
                        "credential {existing} was disconnected"
                    )))
                }
                Err(e) => return auth_error_response(e),
            }
        }
        None => match finalize_apikey_credential(pool, &source_id, &req.name, &req.fields).await {
            Ok(id) => id,
            Err(e) => return auth_error_response(e),
        },
    };

    // Set or clear, so a reconnect without a date does not keep the old one.
    if let Err(e) = vault::set_credential_expiry(pool, &credential_id, expires_at).await {
        return auth_error_response(e);
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::ensure_test_key;
    use virtues_helpers::auth::{mark_credential_status, read_credential_secrets, CredentialStatus};

    fn grant(access_token: &str, expires_in: Option<i64>) -> ProxyExchangeResponse {
        ProxyExchangeResponse {
            secrets: serde_json::json!({ "access_token": access_token, "refresh_token": "rt-example" }),
            metadata: serde_json::json!({ "granted_scopes": "calendar.readonly" }),
            expires_in,
            scopes: None,
        }
    }

    /// A Strava grant, which names its athlete.
    fn athlete_grant(access_token: &str, athlete_id: i64) -> ProxyExchangeResponse {
        ProxyExchangeResponse {
            secrets: serde_json::json!({ "access_token": access_token, "refresh_token": "rt-example" }),
            metadata: serde_json::json!({ "athlete": { "id": athlete_id, "firstname": "Nick" } }),
            expires_in: Some(21600),
            scopes: None,
        }
    }

    /// A Notion grant, which names its workspace among the secrets.
    fn workspace_grant(access_token: &str, workspace_id: &str) -> ProxyExchangeResponse {
        ProxyExchangeResponse {
            secrets: serde_json::json!({ "access_token": access_token, "workspace_id": workspace_id }),
            metadata: serde_json::json!({ "workspace_name": "Notes" }),
            expires_in: None,
            scopes: None,
        }
    }

    async fn last_seen_recently(pool: &PgPool, id: &str) -> bool {
        sqlx::query_scalar(
            "SELECT coalesce(last_seen_at > now() - interval '1 minute', false) FROM credentials WHERE id = $1",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn row(pool: &PgPool, id: &str) -> (String, Option<String>) {
        sqlx::query_as("SELECT status, status_reason FROM credentials WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn source_rows(pool: &PgPool, source_id: &str) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM credentials WHERE kind = 'source' AND key = $1")
            .bind(source_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn disconnect(pool: &PgPool, id: &str) {
        sqlx::query("UPDATE credentials SET status = 'revoked', status_reason = 'user_revoked' WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_reconnect_repairs_the_row_it_was_started_from(pool: PgPool) {
        ensure_test_key();
        let id = store_grant(&pool, "google", None, &grant("at-first", Some(3600))).await.unwrap();
        mark_credential_status(&pool, &id, CredentialStatus::ReauthRequired, Some("token_rejected_by_provider"))
            .await
            .unwrap();
        sqlx::query("UPDATE credentials SET expires_at = now() - interval '1 day' WHERE id = $1")
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();

        let repaired = store_grant(&pool, "google", Some(&id), &grant("at-second", Some(3600)))
            .await
            .unwrap();

        assert_eq!(repaired, id, "the reconnect keeps the row's id");
        assert_eq!(source_rows(&pool, "google").await, 1, "no second row");
        assert_eq!(row(&pool, &id).await, ("active".to_string(), None));
        let secrets = read_credential_secrets(&pool, &id).await.unwrap();
        assert_eq!(secrets["access_token"], "at-second");
        let fresh: bool = sqlx::query_scalar(
            "SELECT expires_at > now() AND next_refresh_at > now() FROM credentials WHERE id = $1",
        )
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(fresh, "the expiry fields describe the new grant");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_disconnected_row_stays_disconnected(pool: PgPool) {
        ensure_test_key();
        let id = store_grant(&pool, "google", None, &grant("at-first", Some(3600))).await.unwrap();
        disconnect(&pool, &id).await;

        // Starting a reconnect, or pasting new fields, is refused up front.
        let refused = reconnect_target(&pool, &id, "google").await.unwrap_err();
        assert_eq!(refused.status(), StatusCode::CONFLICT);

        // A write that was already under way cannot bring it back, as active
        // or as needing a reconnect.
        let new_fields = serde_json::json!({ "token": "tok-example" });
        let repaired = vault::reauth_credential(&pool, &id, "google", &new_fields, None, None)
            .await
            .unwrap();
        assert!(!repaired);
        assert!(matches!(
            mark_credential_status(&pool, &id, CredentialStatus::ReauthRequired, None).await,
            Err(AuthError::Conflict(_))
        ));
        assert_eq!(row(&pool, &id).await.0, "revoked");

        // An OAuth reconnect that finishes after the disconnect keeps the
        // owner's new sign-in as a new row.
        let added = store_grant(&pool, "google", Some(&id), &grant("at-second", Some(3600)))
            .await
            .unwrap();
        assert_ne!(added, id);
        assert_eq!(row(&pool, &id).await.0, "revoked");
        assert_eq!(row(&pool, &added).await.0, "active");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_reconnect_stays_within_its_source(pool: PgPool) {
        ensure_test_key();
        let strava = store_grant(&pool, "strava", None, &grant("at-strava", Some(3600))).await.unwrap();

        let refused = reconnect_target(&pool, &strava, "google").await.unwrap_err();
        assert_eq!(refused.status(), StatusCode::NOT_FOUND);
        let refused = reconnect_target(&pool, "cred_missing", "google").await.unwrap_err();
        assert_eq!(refused.status(), StatusCode::NOT_FOUND);
        assert!(reconnect_target(&pool, &strava, "strava").await.is_ok());

        let google = store_grant(&pool, "google", Some(&strava), &grant("at-google", Some(3600)))
            .await
            .unwrap();
        assert_ne!(google, strava);
        let secrets = read_credential_secrets(&pool, &strava).await.unwrap();
        assert_eq!(secrets["access_token"], "at-strava", "the other source's row is untouched");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn connecting_and_new_secrets_count_as_a_use(pool: PgPool) {
        ensure_test_key();
        let oauth = store_grant(&pool, "google", None, &grant("at-first", Some(3600))).await.unwrap();
        assert!(last_seen_recently(&pool, &oauth).await, "an OAuth connect");
        let pasted = finalize_apikey_credential(
            &pool,
            "github",
            "GitHub",
            &serde_json::json!({ "token": "tok-example" }),
        )
        .await
        .unwrap();
        assert!(last_seen_recently(&pool, &pasted).await, "a pasted key");

        sqlx::query("UPDATE credentials SET last_seen_at = now() - interval '1 day'")
            .execute(&pool)
            .await
            .unwrap();
        vault::update_credential_secrets(&pool, &oauth, &grant("at-second", None).secrets, Some(3600))
            .await
            .unwrap();
        let fields = serde_json::json!({ "token": "tok-second" });
        assert!(vault::reauth_credential(&pool, &pasted, "github", &fields, None, None)
            .await
            .unwrap());
        assert!(last_seen_recently(&pool, &oauth).await, "new secrets");
        assert!(last_seen_recently(&pool, &pasted).await, "a pasted reconnect");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_reconnect_to_another_account_adds_a_row(pool: PgPool) {
        ensure_test_key();
        let strava = store_grant(&pool, "strava", None, &athlete_grant("at-first", 101)).await.unwrap();
        mark_credential_status(&pool, &strava, CredentialStatus::ReauthRequired, None)
            .await
            .unwrap();
        sqlx::query("UPDATE credentials SET last_seen_at = now() - interval '1 day' WHERE id = $1")
            .bind(&strava)
            .execute(&pool)
            .await
            .unwrap();

        let other = store_grant(&pool, "strava", Some(&strava), &athlete_grant("at-other", 202))
            .await
            .unwrap();
        assert_ne!(other, strava, "another athlete is a sign-in of its own");
        assert_eq!(row(&pool, &strava).await.0, "reauth_required");
        assert!(!last_seen_recently(&pool, &strava).await, "comparing accounts is not a use");
        let secrets = read_credential_secrets(&pool, &strava).await.unwrap();
        assert_eq!(secrets["access_token"], "at-first", "the old row keeps its own grant");

        let same = store_grant(&pool, "strava", Some(&strava), &athlete_grant("at-second", 101))
            .await
            .unwrap();
        assert_eq!(same, strava, "the same athlete repairs the row");
        assert_eq!(row(&pool, &strava).await.0, "active");

        // Notion names its workspace among the secrets.
        let notion = store_grant(&pool, "notion", None, &workspace_grant("at-first", "ws-1")).await.unwrap();
        let other = store_grant(&pool, "notion", Some(&notion), &workspace_grant("at-other", "ws-2"))
            .await
            .unwrap();
        assert_ne!(other, notion);
        let same = store_grant(&pool, "notion", Some(&notion), &workspace_grant("at-second", "ws-1"))
            .await
            .unwrap();
        assert_eq!(same, notion);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relinking_works_on_a_box_key_an_older_release_revoked(pool: PgPool) {
        ensure_test_key();
        let id = finalize_apikey_credential(
            &pool,
            "virtues_api",
            "Virtues API",
            &serde_json::json!({ "api_key": "vk-first" }),
        )
        .await
        .unwrap();
        disconnect(&pool, &id).await;

        let relinked = serde_json::json!({ "api_key": "vk-second" });
        vault::update_credential_secrets(&pool, &id, &relinked, None).await.unwrap();

        assert_eq!(row(&pool, &id).await, ("active".to_string(), None));
        assert_eq!(
            virtues_helpers::auth::read_box_api_key(&pool).await.unwrap().as_deref(),
            Some("vk-second")
        );
    }
}

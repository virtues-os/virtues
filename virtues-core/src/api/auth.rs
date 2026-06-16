//! Session endpoints — read + sign-out.
//!
//! All actual *authentication* (creating a new session) happens in
//! `api::pair` via the pair-token consume flow. There is no signin endpoint
//! because there is no password/email auth.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use serde::Serialize;
use sqlx::PgPool;

use crate::middleware::auth::{
    delete_session, peek_session, read_bearer, validate_bearer, SESSION_COOKIE_NAME,
    SESSION_COOKIE_NAME_SECURE,
};

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub user: Option<SessionUser>,
    /// Hard session expiry (RFC 3339). The idle ceiling is enforced separately
    /// in the middleware and isn't exposed here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionUser {
    pub id: String,
    pub device_id: String,
    pub device_label: String,
}

/// `GET /auth/session` — "who am I?" probe used by the web UI to decide
/// whether to render the app shell or redirect to `/pair`.
///
/// Honors EITHER credential the box accepts:
///   1. a browser **session cookie** (the web `/pair` flow), or
///   2. a device **`Authorization: Bearer`** — the desktop app's local proxy
///      injects this on every request, so a paired device authenticating the
///      local browser counts as "paired" too. Without this, an app-paired
///      device (which has a bearer, not a cookie) would be wrongly bounced to
///      `/pair` even though it's fully paired.
///
/// The cookie path uses `peek_session` (no `last_used_at` touch) so a polling
/// status check doesn't keep an idle browser session alive.
pub async fn session_handler(
    State(pool): State<PgPool>,
    headers: axum::http::HeaderMap,
    jar: CookieJar,
) -> impl IntoResponse {
    // 1. Browser session cookie.
    if let Some(session_token) = read_cookie(&jar) {
        if let Some(user) = peek_session(&pool, &session_token).await {
            let expires: Option<String> = sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>(
                "SELECT expires_at FROM app_auth_session WHERE session_token = $1",
            )
            .bind(&session_token)
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten()
            .map(|dt| dt.to_rfc3339());
            return (StatusCode::OK, Json(SessionResponse { user: Some(session_user(user)), expires }));
        }
    }

    // 2. Device bearer (no session-expiry concept for device credentials).
    if let Some(token) = read_bearer(&headers) {
        if let Some(user) = validate_bearer(&pool, &token).await {
            return (StatusCode::OK, Json(SessionResponse { user: Some(session_user(user)), expires: None }));
        }
    }

    // 3. Neither → not paired.
    (StatusCode::OK, Json(SessionResponse { user: None, expires: None }))
}

fn session_user(u: crate::middleware::auth::AuthUser) -> SessionUser {
    SessionUser {
        id: u.id,
        device_id: u.device_id,
        device_label: u.device_label,
    }
}

/// `POST /auth/signout` — delete the session row + clear the cookie. The
/// device row is *not* revoked here — sign-out is "log me out of this tab"
/// not "this device is no longer trusted." For the latter, the user revokes
/// the device from `/settings/devices`.
pub async fn signout_handler(
    State(pool): State<PgPool>,
    jar: CookieJar,
) -> impl IntoResponse {
    if let Some(token) = read_cookie(&jar) {
        if let Err(e) = delete_session(&pool, &token).await {
            tracing::warn!("signout: delete_session failed: {e}");
        }
    }
    let jar = jar
        .remove(Cookie::from(SESSION_COOKIE_NAME))
        .remove(Cookie::from(SESSION_COOKIE_NAME_SECURE));
    (StatusCode::OK, jar, Json(serde_json::json!({"ok": true})))
}

fn read_cookie(jar: &CookieJar) -> Option<String> {
    jar.get(SESSION_COOKIE_NAME_SECURE)
        .or_else(|| jar.get(SESSION_COOKIE_NAME))
        .map(|c| c.value().to_string())
}

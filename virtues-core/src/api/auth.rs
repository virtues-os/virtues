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
    delete_session, peek_session, SESSION_COOKIE_NAME, SESSION_COOKIE_NAME_SECURE,
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
/// whether to render the app shell or redirect to `/pair`. Does NOT touch
/// `last_used_at` (uses the peek path), so a polling status check doesn't
/// keep an idle session alive.
pub async fn session_handler(
    State(pool): State<PgPool>,
    jar: CookieJar,
) -> impl IntoResponse {
    let session_token = read_cookie(&jar);
    let session_token = match session_token {
        Some(t) => t,
        None => {
            return (
                StatusCode::OK,
                Json(SessionResponse {
                    user: None,
                    expires: None,
                }),
            )
        }
    };

    let user = match peek_session(&pool, &session_token).await {
        Some(u) => u,
        None => {
            return (
                StatusCode::OK,
                Json(SessionResponse {
                    user: None,
                    expires: None,
                }),
            )
        }
    };

    let expires: Option<String> = sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>(
        "SELECT expires_at FROM app_auth_session WHERE session_token = $1",
    )
    .bind(&session_token)
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten()
    .map(|dt| dt.to_rfc3339());

    (
        StatusCode::OK,
        Json(SessionResponse {
            user: Some(SessionUser {
                id: user.id,
                device_id: user.device_id,
                device_label: user.device_label,
            }),
            expires,
        }),
    )
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

//! Authentication middleware.
//!
//! Validates a session cookie against the three-table model:
//!   - `app_auth_session` holds the cookie token
//!   - `app_device`       is the canonical paired-device record
//!   - `app_auth_user`    is the single-tenant owner
//!
//! A request is authenticated iff all three are true:
//!   1. Cookie matches an `app_auth_session` row whose `expires_at > now()`.
//!   2. `app_device.revoked_at IS NULL` for the linked device.
//!   3. `app_auth_session.last_used_at > now() - IDLE_TIMEOUT` (8h default).
//!
//! Each authenticated request bumps `last_used_at` and `app_device.last_seen_at`.
//! Idle timeout means a forgotten browser tab can't be used to re-enter the box
//! after the user has walked away — they have to re-pair.

use axum::{
    async_trait,
    extract::{FromRef, FromRequestParts},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;

/// Idle timeout — sessions go stale after this many hours of inactivity even
/// if their hard `expires_at` is still in the future.
const IDLE_TIMEOUT_HOURS: i64 = 8;

/// Authenticated principal extracted from the cookie.
///
/// `id` is the owner-user id (always the singleton in v1). `device_id` is the
/// specific paired device the cookie belongs to — handlers that need to scope
/// to "this device" (e.g. minting a pair token on behalf of the minting
/// device, or revoking your own session vs another) read from here.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: String,
    pub device_id: String,
    pub device_label: String,
}

pub const SESSION_COOKIE_NAME: &str = "virtues.session-token";
pub const SESSION_COOKIE_NAME_SECURE: &str = "__Secure-virtues.session-token";

#[derive(Debug, Serialize)]
pub struct AuthError {
    pub error: String,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        (StatusCode::UNAUTHORIZED, Json(self)).into_response()
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    PgPool: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let session_token = read_session_cookie(&jar).ok_or_else(unauthorized)?;
        let pool = PgPool::from_ref(state);
        validate_and_touch(&pool, &session_token).await.ok_or_else(unauthorized)
    }
}

fn unauthorized() -> AuthError {
    AuthError {
        error: "Unauthorized".to_string(),
    }
}

fn read_session_cookie(jar: &CookieJar) -> Option<String> {
    jar.get(SESSION_COOKIE_NAME_SECURE)
        .or_else(|| jar.get(SESSION_COOKIE_NAME))
        .map(|c| c.value().to_string())
}

/// Look up the session, enforce expiry + revoke + idle, then touch `last_used_at`
/// + `app_device.last_seen_at`. Returns `None` on any auth failure.
async fn validate_and_touch(pool: &PgPool, session_token: &str) -> Option<AuthUser> {
    // Join session → device → user. We pull `last_used_at` so we can enforce
    // the idle ceiling in the same query and in the same row-lock.
    let row: Option<(String, String, String, DateTime<Utc>, DateTime<Utc>)> = sqlx::query_as(
        "SELECT u.id, d.id, d.label, s.last_used_at, s.expires_at \
         FROM app_auth_session s \
         JOIN app_device d ON d.id = s.device_id \
         JOIN app_auth_user u ON u.id = d.user_id \
         WHERE s.session_token = $1 \
           AND s.expires_at > now() \
           AND d.revoked_at IS NULL",
    )
    .bind(session_token)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let (user_id, device_id, device_label, last_used_at, _expires_at) = row?;

    // Idle check — separate from hard expiry so the failure mode is distinct
    // (we could log "idle_logout" vs "hard_expiry" if we wanted).
    let idle_cutoff = Utc::now() - chrono::Duration::hours(IDLE_TIMEOUT_HOURS);
    if last_used_at < idle_cutoff {
        // Treat as logged out. We don't delete the row here — the periodic
        // sweeper handles cleanup; the user just has to re-pair.
        return None;
    }

    // Touch both timestamps. We tolerate failure here (rare) — auth still
    // succeeds; we just don't get a fresh last-seen.
    let _ = sqlx::query(
        "UPDATE app_auth_session SET last_used_at = now() WHERE session_token = $1",
    )
    .bind(session_token)
    .execute(pool)
    .await;
    let _ = sqlx::query("UPDATE app_device SET last_seen_at = now() WHERE id = $1")
        .bind(&device_id)
        .execute(pool)
        .await;

    Some(AuthUser {
        id: user_id,
        device_id,
        device_label,
    })
}

/// Public helper used by `/auth/session` and `/auth/signout`. Mirrors the
/// extractor's read path but doesn't touch timestamps — useful for "just tell
/// me if this cookie is alive" queries that shouldn't reset the idle clock.
///
/// Enforces the same predicates as the extractor (hard expiry + soft revoke +
/// idle ceiling) so that `/auth/session` never reports `user = Some` for a
/// session the very next state-changing request would reject.
pub async fn peek_session(pool: &PgPool, session_token: &str) -> Option<AuthUser> {
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT u.id, d.id, d.label \
         FROM app_auth_session s \
         JOIN app_device d ON d.id = s.device_id \
         JOIN app_auth_user u ON u.id = d.user_id \
         WHERE s.session_token = $1 \
           AND s.expires_at > now() \
           AND s.last_used_at > now() - make_interval(hours => $2::int) \
           AND d.revoked_at IS NULL",
    )
    .bind(session_token)
    .bind(IDLE_TIMEOUT_HOURS as i32)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    row.map(|(id, device_id, device_label)| AuthUser {
        id,
        device_id,
        device_label,
    })
}

/// Delete a session row by cookie value. Used by `/auth/signout`.
pub async fn delete_session(pool: &PgPool, session_token: &str) -> crate::Result<()> {
    sqlx::query("DELETE FROM app_auth_session WHERE session_token = $1")
        .bind(session_token)
        .execute(pool)
        .await
        .map_err(|e| crate::Error::Database(format!("delete session: {e}")))?;
    Ok(())
}

/// Periodic cleanup — drop sessions whose hard expiry has passed, and pair
/// tokens whose TTL elapsed in a non-active state.
pub async fn cleanup_expired(pool: &PgPool) -> crate::Result<(u64, u64)> {
    let sessions = sqlx::query("DELETE FROM app_auth_session WHERE expires_at < now()")
        .execute(pool)
        .await
        .map_err(|e| crate::Error::Database(format!("cleanup sessions: {e}")))?
        .rows_affected();

    // Past expires_at, every terminal state is safe to delete — the token
    // can no longer be claimed, confirmed, or re-consumed. (`consumed`
    // tokens stay useful for ~60s as a re-claim window inside the consume
    // path; the WS-4 sweeper enforces that grace window separately.)
    let tokens = sqlx::query(
        "DELETE FROM app_pair_token \
         WHERE expires_at < now() \
           AND status IN ('pending', 'authorized', 'expired', 'denied', 'consumed')",
    )
    .execute(pool)
    .await
    .map_err(|e| crate::Error::Database(format!("cleanup pair tokens: {e}")))?
    .rows_affected();

    Ok((sessions, tokens))
}

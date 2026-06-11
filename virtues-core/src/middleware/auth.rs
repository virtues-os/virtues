//! Authentication middleware.
//!
//! **Auth lives at the app layer; the network is a dumb transport.** The box
//! authenticates every request from its own credentials — NOT from the path it
//! arrived over — so the same auth works whether a request comes over Virtues'
//! WireGuard, a user's BYO overlay (Tailscale/Headscale/VPS), or plain LAN.
//! See `[[project_networking_doctrine]]`.
//!
//! Three accepted credentials, checked in order:
//!   1. **Device bearer** (`Authorization: Bearer <token>`) — a paired
//!      non-browser device (iOS/Mac/custom). HMAC-lookup against `credentials`,
//!      joined to its `app_device` (enforcing `revoked_at IS NULL`). Pure
//!      capability, works over any transport.
//!   2. **Loopback console** — a process on the box itself connecting directly
//!      to `127.0.0.1`/`::1`. Gated: refused when a forwarding header is
//!      present (a reverse proxy also connects from loopback — see below).
//!   3. **Session cookie** — a browser, validated against the three-table model
//!      (`app_auth_session` → `app_device` → `app_auth_user`) with hard expiry,
//!      soft revoke, and an 8h idle ceiling.
//!
//! Each authenticated request bumps `last_used_at` / `last_seen_at`. Idle
//! timeout means a forgotten browser tab can't re-enter the box after the user
//! walks away — they have to re-pair.

use axum::{
    async_trait,
    extract::{ConnectInfo, FromRef, FromRequestParts},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use std::net::SocketAddr;

/// Idle timeout — sessions go stale after this many hours of inactivity even
/// if their hard `expires_at` is still in the future.
const IDLE_TIMEOUT_HOURS: i64 = 8;

/// Synthetic device id for the "I'm sitting at the box's monitor + keyboard"
/// session. The auth extractor returns an `AuthUser` with this id when the
/// request's socket peer is loopback (`127.0.0.1` / `::1`) AND no forwarding
/// header is present, bypassing the cookie + pair-token requirement.
///
/// Safe because the threat model is "physical access = you" — a process on
/// the box can already read `/var/lib/virtues/lake/` and `/etc/virtues/env`,
/// so trusting a loopback-only HTTP request adds no attack surface. LAN
/// peers are NOT trusted (`is_loopback()` is strict, not RFC1918).
///
/// CRITICAL: a reverse proxy (Caddy/an HTTPS sidecar) terminating an external
/// connection ALSO forwards to the box from loopback — so a naive
/// "loopback == owner" rule would hand owner auth to anyone who reached the
/// proxy. We therefore refuse the bypass whenever an `X-Forwarded-For` /
/// `Forwarded` header is present: a genuinely local process connects directly
/// and sets no such header; a proxied request always carries one.
pub const CONSOLE_DEVICE_ID: &str = "local-console";
pub const CONSOLE_DEVICE_LABEL: &str = "Local console";

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
        let pool = PgPool::from_ref(state);

        // 1. Device bearer — transport-agnostic app-layer credential. A paired
        //    non-browser device presents `Authorization: Bearer <token>`. The
        //    bearer IS the credential, so this authenticates over ANY transport
        //    (Virtues WG, a BYO overlay, plain LAN) — the network is never the
        //    trust boundary. An explicit bearer means "I am a device": a bad
        //    one fails closed rather than falling through to cookie/loopback.
        if let Some(token) = read_bearer(&parts.headers) {
            return validate_bearer(&pool, &token).await.ok_or_else(unauthorized);
        }

        // 2. Loopback console — a process on the box itself, connecting
        //    directly to 127.0.0.1 / ::1. Physical access wins the threat
        //    model. Refused when a forwarding header is present, because a
        //    reverse proxy in front of the box also connects from loopback
        //    while forwarding a REMOTE client (see CONSOLE_DEVICE_ID docs).
        let is_loopback = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0.ip().is_loopback())
            .unwrap_or(false);
        let is_proxied = parts.headers.contains_key("x-forwarded-for")
            || parts.headers.contains_key("forwarded");
        if is_loopback && !is_proxied {
            return Ok(AuthUser {
                id: crate::middleware::http::OWNER_USER_ID.to_string(),
                device_id: CONSOLE_DEVICE_ID.to_string(),
                device_label: CONSOLE_DEVICE_LABEL.to_string(),
            });
        }

        // 3. Session cookie — a browser.
        let jar = CookieJar::from_headers(&parts.headers);
        let session_token = read_session_cookie(&jar).ok_or_else(unauthorized)?;
        validate_and_touch(&pool, &session_token).await.ok_or_else(unauthorized)
    }
}

/// Extract a token from an `Authorization: Bearer <token>` header.
fn read_bearer(headers: &axum::http::HeaderMap) -> Option<String> {
    let raw = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let token = raw
        .strip_prefix("Bearer ")
        .or_else(|| raw.strip_prefix("bearer "))?
        .trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

/// Validate a device bearer into an `AuthUser`. HMAC-looks-up the credential,
/// joins it to its paired device + owner, and enforces the device-list ACL
/// (credential `active`, device `revoked_at IS NULL`). Touches last-seen on
/// success. Transport-independent — this is what makes the box reachable over
/// a BYO overlay with no special-casing.
async fn validate_bearer(pool: &PgPool, token: &str) -> Option<AuthUser> {
    let credential_id = crate::api::credentials::validate_device_token(pool, token)
        .await
        .ok()?;
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT u.id, d.id, d.label \
         FROM credentials c \
         JOIN app_device d ON d.id = c.device_id \
         JOIN app_auth_user u ON u.id = d.user_id \
         WHERE c.id = $1 AND c.status = 'active' AND d.revoked_at IS NULL",
    )
    .bind(&credential_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let (user_id, device_id, device_label) = row?;

    // Best-effort last-seen touch on both the credential and the device row.
    let _ = crate::api::credentials::update_last_seen(pool, &credential_id).await;
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

/// Idempotent upsert for the synthetic console device row. Called at server
/// startup so the FK in `app_auth_session` (and any future row that references
/// the console session) stays valid. The row is created revoked=NULL,
/// last_seen_at=NULL — `last_seen_at` gets touched on real use via the same
/// validate_and_touch path (loopback bypass updates it best-effort, see below).
pub async fn ensure_console_device(pool: &PgPool) -> crate::Result<()> {
    sqlx::query(
        "INSERT INTO app_device (id, user_id, kind, label, paired_from_ip) \
         VALUES ($1, $2, 'cli', $3, '127.0.0.1') \
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(CONSOLE_DEVICE_ID)
    .bind(crate::middleware::http::OWNER_USER_ID)
    .bind(CONSOLE_DEVICE_LABEL)
    .execute(pool)
    .await
    .map_err(|e| crate::Error::Database(format!("ensure_console_device: {e}")))?;
    Ok(())
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

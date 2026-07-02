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
use serde::Serialize;
use sqlx::PgPool;
use std::net::SocketAddr;

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

        // 1. iroh transport identity — the primary credential for app/iroh
        //    clients. The QUIC raw-public-key handshake PROVED the peer's
        //    EndpointId, and `serve()` only forwards allowlisted (paired) peers,
        //    stamping the proven id as `ProvenPeer`. Map it to the device: the
        //    allowlisted key IS the credential. Unspoofable — a typed extension
        //    set post-handshake, never a header, and only the iroh serve path
        //    sets it (the plain :8000 listener never does). A proven id that
        //    isn't a known device falls through to the paths below.
        if let Some(peer) = parts.extensions.get::<virtues_iroh::ProvenPeer>() {
            let node_id = peer.0.to_string();
            if let Some(user) = validate_iroh_peer(&pool, &node_id).await {
                return Ok(user);
            }
        }

        // 2. Loopback console — a process on the box itself (on-box CLI),
        //    connecting
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

        // 3. Dev fallback — `ENVIRONMENT=dev` is a developer's local stack (core
        //    isn't exposed; the request reaches us through the vite proxy, which
        //    defeats the loopback bypass above). Authenticate as the console
        //    owner so `make dev` lands straight in the app with no pairing — the
        //    complement to VIRTUES_DEV_SKIP_SETUP. A real box NEVER sets
        //    ENVIRONMENT=dev, so this is inert in production. Last resort: the
        //    proven iroh key / loopback still win above.
        if is_dev() {
            return Ok(AuthUser {
                id: crate::middleware::http::OWNER_USER_ID.to_string(),
                device_id: CONSOLE_DEVICE_ID.to_string(),
                device_label: CONSOLE_DEVICE_LABEL.to_string(),
            });
        }

        Err(unauthorized())
    }
}

/// True only on a developer's local stack (`ENVIRONMENT=dev`). Gates the dev
/// auto-login + skip-pairing conveniences; never set on a real appliance.
pub fn is_dev() -> bool {
    std::env::var("ENVIRONMENT").map(|v| v == "dev").unwrap_or(false)
}

/// Authenticate a device by its proven, allowlisted iroh EndpointId (hex).
/// Mirrors [`validate_bearer`]'s join but keyed on `app_device.node_id` — no
/// bearer/credential row involved. The caller has already established (via the
/// QUIC handshake + `serve()`'s allowlist gate) that the peer holds this key, so
/// a live device row owning it is sufficient to authenticate. Touches last-seen.
pub(crate) async fn validate_iroh_peer(pool: &PgPool, node_id: &str) -> Option<AuthUser> {
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT u.id, d.id, d.label \
         FROM app_device d \
         JOIN app_auth_user u ON u.id = d.user_id \
         WHERE d.node_id = $1 AND d.revoked_at IS NULL",
    )
    .bind(node_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let (user_id, device_id, device_label) = row?;

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

/// Periodic cleanup — drop pair tokens whose TTL elapsed in a non-active state.
pub async fn cleanup_expired(pool: &PgPool) -> crate::Result<u64> {
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

    Ok(tokens)
}

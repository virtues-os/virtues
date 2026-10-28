//! Authentication middleware.
//!
//! **The allowlisted iroh key is the credential.** A paired device holds an
//! iroh EndpointId that the box has allowlisted; the QUIC raw-public-key
//! handshake proves the key on every connection, so authentication is just
//! "is the proven key a live device?" — no bearer, no cookie, no second secret.
//!
//! Three accepted credentials, checked in order:
//!   1. **Proven iroh peer** — the primary path for every interactive client
//!      (iOS, desktop + its webview via the daemon, CLI-over-iroh). `serve()`
//!      only forwards allowlisted peers and stamps the proven EndpointId as a
//!      `ProvenPeer` extension; we map it to its `app_device` row (enforcing
//!      `revoked_at IS NULL`). Unspoofable — a typed extension, never a header.
//!   2. **Loopback console** — a process on the box itself connecting directly
//!      to `127.0.0.1`/`::1`. Gated: refused when a forwarding header is
//!      present (a reverse proxy also connects from loopback — see below).
//!   3. **Dev fallback** — `ENVIRONMENT=dev` only, and only for a peer on
//!      loopback: a loopback request that carries a forwarding header, which
//!      the console refuses. Inert on a real appliance.
//!
//! The first two, and the third on a developer's stack, are authority that
//! comes from how a request arrived. [`owner_by_transport`] names exactly
//! those requests, and `middleware::security` refuses one whose `Host` the
//! box does not serve, which is what keeps a DNS-rebinding page off them.
//!
//! Each authenticated request bumps `last_seen_at`.
//!
//! There is no fourth class. This paragraph used to say webhook and OAuth
//! bearers were "a separate, surviving token class (see `validate_device_token`)"
//! — no function of that name has ever existed, and neither half of the claim
//! is true. Webhook ingestion is gated by the same proven iroh key as every
//! other route (`server::webhook`), with no long-lived bearer anywhere. The
//! secrets in the `credentials` table point the other way: they are what the
//! box presents to Google or Plaid, never something a caller presents to us,
//! so nothing checks them here or ever should.
//!
//! Other tokens DO exist, and are presented by callers and checked outside
//! this extractor: a pair token (redeemed once at `/api/pair/consume`), a
//! signed OAuth state and exchange token (one round-trip each), and a face
//! token (`server::faces`, in-memory, minted behind this extractor). **None of
//! them is a way to authenticate as a device.** Each opens exactly one thing —
//! one pairing, one OAuth completion, one face load — and not the API. Shared
//! links are served by the door, a separate process that presents nothing to
//! this extractor (`crate::door`).
//!
//! (An earlier revision of this paragraph, correcting a false claim, made its
//! own: it said faces held the only other token in the process. Stated here
//! because a security doc that has been wrong twice is read carefully or not
//! at all, and the second error was the kind that reassures.)

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
/// header is present, bypassing the iroh-key requirement.
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

/// Authenticated principal for a request.
///
/// `id` is the owner-user id (always the singleton in v1). `device_id` is the
/// specific paired device the proven iroh key resolved to — handlers that need
/// to scope to "this device" (e.g. minting a pair token on behalf of the
/// minting device, or revoking one device vs another) read from here.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: String,
    pub device_id: String,
    pub device_label: String,
}

#[derive(Debug, Serialize)]
pub struct AuthError {
    pub error: String,
    /// 401 for a credential that does not authenticate; 503 when the device
    /// table could not be read at all. The second used to fall through to the
    /// first, so a dead pool told every paired device it was a stranger.
    #[serde(skip)]
    status: StatusCode,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let status = self.status;
        (status, Json(self)).into_response()
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
        authenticate(parts, &PgPool::from_ref(state), is_dev()).await
    }
}

/// The extractor's body, with `dev` ([`is_dev`]) passed in so a test can
/// set it without touching the process environment.
async fn authenticate(parts: &mut Parts, pool: &PgPool, dev: bool) -> Result<AuthUser, AuthError> {
    // 1. iroh transport identity — the primary credential for app/iroh
    //    clients. The QUIC raw-public-key handshake PROVED the peer's
    //    EndpointId, and `serve()` only forwards allowlisted (paired) peers,
    //    stamping the proven id as `ProvenPeer`. Map it to the device: the
    //    allowlisted key IS the credential. Unspoofable — a typed extension
    //    set post-handshake, never a header, and only the iroh serve path
    //    sets it (the plain :8000 listener never does). A proven id that
    //    isn't a known device falls through to the paths below.
    if let Some(peer) = parts.extensions.get::<virtues_iroh::ProvenPeer>() {
        let endpoint_id = peer.0.to_string();
        let user = match validate_iroh_peer(pool, &endpoint_id).await {
            Ok(user) => user,
            Err(e) => return Err(unavailable(e)),
        };
        if let Some((user, kind)) = user {
            // An agent device (an MCP bridge on the owner's machine,
            // agents/plan/mcp-plan.md) reaches the MCP endpoint and no
            // other route, and is refused here rather than handed to the
            // paths below.
            if kind == AGENT_KIND && parts.uri.path() != crate::server::api::mcp::PATH {
                return Err(agent_refused());
            }
            // Best-effort: refresh the device's reported build identity from
            // the X-Virtues-Client header (update-manifold Phase 1). Never
            // affects auth — a missing/malformed header is simply skipped.
            if let Some(cb) = parse_client_header(&parts.headers) {
                record_client_build(pool, &user.device_id, &cb).await;
            }
            return Ok(user);
        }
    }

    // 2. Loopback console — a process on the box itself (on-box CLI),
    //    connecting directly to 127.0.0.1 / ::1. Physical access wins the
    //    threat model. Refused when a forwarding header is present, because a
    //    reverse proxy in front of the box also connects from loopback while
    //    forwarding a REMOTE client (see CONSOLE_DEVICE_ID docs).
    let is_loopback = peer_is_loopback(&parts.extensions);
    if is_loopback && !is_forwarded(&parts.headers) {
        // The console row used to be startup fiction — inserted once with
        // last_seen NULL and never touched again, so Devices said "Never"
        // no matter how much the box's own screen or CLI was used. Record
        // reality instead. Throttled to minute resolution because the
        // kiosk polls over loopback at 1s and legibility does not need a
        // write per frame. Best-effort, never blocks auth.
        let _ = sqlx::query(
            "UPDATE app_device SET last_seen_at = now() \
             WHERE id = $1 AND (last_seen_at IS NULL \
                OR last_seen_at < now() - interval '60 seconds')",
        )
        .bind(CONSOLE_DEVICE_ID)
        .execute(pool)
        .await;
        if let Some(cb) = parse_client_header(&parts.headers) {
            record_client_build(pool, CONSOLE_DEVICE_ID, &cb).await;
        }
        return Ok(console_owner());
    }

    // 3. Dev fallback — `ENVIRONMENT=dev`, a developer's local stack, and a
    //    peer on loopback. `make dev` binds every interface, and the loopback
    //    rule keeps the rest of the developer's network from reaching core
    //    directly as the owner. The dev server's proxy
    //    (`apps/web/vite.config.ts`) connects from loopback and sets no
    //    forwarding header, so its requests are the console above and log in
    //    there. That includes a client elsewhere on the network that opens
    //    the dev server, which `vite dev --host` serves on every interface
    //    (`apps/web/package.json`). What reaches this branch is a
    //    loopback request that carries a forwarding header: a proxy the
    //    developer runs in front of the stack. The Host check
    //    (`middleware::security`) still refuses a name the box does not
    //    serve, such as a tunnel's public hostname. A real box NEVER sets
    //    ENVIRONMENT=dev, so this is inert in production.
    if dev && is_loopback {
        return Ok(console_owner());
    }

    Err(unauthorized())
}

/// The owner, as the box's own console.
fn console_owner() -> AuthUser {
    AuthUser {
        id: crate::middleware::http::OWNER_USER_ID.to_string(),
        device_id: CONSOLE_DEVICE_ID.to_string(),
        device_label: CONSOLE_DEVICE_LABEL.to_string(),
    }
}

/// Whether the request came straight from a socket on this machine. The
/// address is canonicalized first: the server binds `*:8000` (dual-stack),
/// so an on-box client dialing `127.0.0.1` arrives as `::ffff:127.0.0.1`,
/// which `is_loopback()` does not match. See `crate::peer_addr`. An iroh
/// request has no socket peer here and is never loopback.
fn peer_is_loopback(extensions: &axum::http::Extensions) -> bool {
    extensions
        .get::<ConnectInfo<SocketAddr>>()
        .is_some_and(|ci| crate::peer_addr::canonical_peer(&ci.0).is_loopback())
}

/// Whether a forwarding header says a proxy relayed the request for a client
/// somewhere else.
fn is_forwarded(headers: &axum::http::HeaderMap) -> bool {
    headers.contains_key("x-forwarded-for") || headers.contains_key("forwarded")
}

/// Whether the box takes this request as the owner because of how it
/// arrived, before any device row is read: a proven iroh peer, a direct
/// loopback connection, and on a developer's stack (`dev`) any loopback
/// connection. These are the requests the Host check holds to the names the
/// box serves (`middleware::security`). Every other request gets only what
/// the box gives a stranger on its network.
pub(crate) fn owner_by_transport(
    extensions: &axum::http::Extensions,
    headers: &axum::http::HeaderMap,
    dev: bool,
) -> bool {
    extensions.get::<virtues_iroh::ProvenPeer>().is_some()
        || (peer_is_loopback(extensions) && (dev || !is_forwarded(headers)))
}

/// True only on a developer's local stack (`ENVIRONMENT=dev`). Gates the dev
/// auto-login + skip-pairing conveniences; never set on a real appliance.
pub fn is_dev() -> bool {
    std::env::var("ENVIRONMENT").map(|v| v == "dev").unwrap_or(false)
}

/// Authenticate a device by its proven, allowlisted iroh EndpointId (hex).
/// Joins `app_device` to its owner keyed on `app_device.endpoint_id` — no
/// bearer/credential row involved. The caller has already established (via the
/// QUIC handshake + `serve()`'s allowlist gate) that the peer holds this key, so
/// a live device row owning it is sufficient to authenticate. Touches last-seen.
///
/// Returns the device's kind beside the user, for the agent check above.
///
/// `Ok(None)` is "no live device holds this key"; `Err` is "could not look",
/// and the caller must not read the second as the first.
pub(crate) async fn validate_iroh_peer(
    pool: &PgPool,
    endpoint_id: &str,
) -> Result<Option<(AuthUser, String)>, sqlx::Error> {
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT u.id, d.id, d.label, d.kind \
         FROM app_device d \
         JOIN app_auth_user u ON u.id = d.user_id \
         WHERE d.endpoint_id = $1 AND d.revoked_at IS NULL",
    )
    .bind(endpoint_id)
    .fetch_optional(pool)
    .await?;
    let Some((user_id, device_id, device_label, kind)) = row else {
        return Ok(None);
    };

    let _ = sqlx::query("UPDATE app_device SET last_seen_at = now() WHERE id = $1")
        .bind(&device_id)
        .execute(pool)
        .await;

    Ok(Some((
        AuthUser {
            id: user_id,
            device_id,
            device_label,
        },
        kind,
    )))
}

/// The build identity a client reports via
/// `X-Virtues-Client: version=…; sha=…; channel=…[; app=…]`. All best-effort —
/// a malformed header is ignored, never a request failure.
#[derive(Debug, Default)]
pub(crate) struct ClientBuild {
    pub version: String,
    pub sha: String,
    pub channel: String,
    /// The NATIVE shell's own version (`1.0.23`), when the client runs inside
    /// one. `version`/`sha`/`channel` describe the UI bundle — which, for a
    /// paired desktop, is the box-served SPA and therefore mirrors the box.
    /// This field is the only one that answers "which app binary is that
    /// device on", so absence must never erase a previously reported value
    /// (the SPA sends it only after the shell bridge resolves).
    pub app: Option<String>,
}

/// Parse the `X-Virtues-Client` header. Returns `None` unless a version is
/// present (so we never write an empty build blob) and the values are within
/// sane length bounds (hygiene against a hostile paired client).
pub(crate) fn parse_client_header(headers: &axum::http::HeaderMap) -> Option<ClientBuild> {
    let raw = headers.get("x-virtues-client")?.to_str().ok()?;
    let mut cb = ClientBuild::default();
    for part in raw.split(';') {
        let mut kv = part.splitn(2, '=');
        match (kv.next().map(str::trim), kv.next().map(str::trim)) {
            (Some("version"), Some(v)) => cb.version = v.to_string(),
            (Some("sha"), Some(v)) => cb.sha = v.to_string(),
            (Some("channel"), Some(v)) => cb.channel = v.to_string(),
            (Some("app"), Some(v)) if !v.is_empty() => cb.app = Some(v.to_string()),
            _ => {}
        }
    }
    if cb.version.is_empty() || cb.version.len() > 64 || cb.sha.len() > 64 || cb.channel.len() > 32
        || cb.app.as_deref().is_some_and(|a| a.len() > 64)
    {
        return None;
    }
    Some(cb)
}

/// Merge a client's reported build into `device_info.build`. Best-effort — a
/// failure never blocks the request. The shallow jsonb `||` replaces just the
/// `build` key. The `IS DISTINCT FROM` guard makes this a no-op write on the
/// common path (build unchanged between upgrades), so it doesn't churn the row
/// on every request — it only writes when the reported sha actually changes.
pub(crate) async fn record_client_build(pool: &PgPool, device_id: &str, cb: &ClientBuild) {
    // Merged INTO the existing build object (not replacing it): early requests
    // from a desktop SPA arrive before the shell bridge resolves and carry no
    // `app` — a whole-object replace would erase the app version the previous
    // request reported. `jsonb_strip_nulls` drops the absent key instead. The
    // guard fires on either identity moving: the UI bundle (sha) or the native
    // shell (app) — an app update under an unchanged bundle must still write.
    let _ = sqlx::query(
        "UPDATE app_device \
         SET device_info = jsonb_set(device_info, '{build}', \
             COALESCE(device_info->'build', '{}'::jsonb) || jsonb_strip_nulls(\
                 jsonb_build_object('version', $2::text, 'sha', $3::text, \
                                    'channel', $4::text, 'app', $5::text))) \
         WHERE id = $1 AND ((device_info->'build'->>'sha') IS DISTINCT FROM $3::text \
             OR ($5::text IS NOT NULL \
                 AND (device_info->'build'->>'app') IS DISTINCT FROM $5::text))",
    )
    .bind(device_id)
    .bind(&cb.version)
    .bind(&cb.sha)
    .bind(&cb.channel)
    .bind(cb.app.as_deref())
    .execute(pool)
    .await;
}

/// Idempotent upsert for the synthetic console device row. Called at server
/// startup so any row that references the console device (e.g. a pair token
/// minted from the on-box CLI) has a valid FK target. The row is created
/// revoked=NULL, last_seen_at=NULL.
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

/// The device kind of an MCP bridge (`api::pair`, migration 0064).
pub const AGENT_KIND: &str = "agent";

/// An agent device asked for a route other than the MCP endpoint.
fn agent_refused() -> AuthError {
    AuthError {
        error: "This device is an agent and can reach only the MCP endpoint.".to_string(),
        status: StatusCode::FORBIDDEN,
    }
}

fn unauthorized() -> AuthError {
    AuthError {
        error: "Unauthorized".to_string(),
        status: StatusCode::UNAUTHORIZED,
    }
}

/// The device lookup itself failed. Not a 401: the credential was never
/// examined, so the honest status is "try again", and the log carries why.
fn unavailable(e: sqlx::Error) -> AuthError {
    tracing::error!(error = %e, "device lookup failed during authentication");
    AuthError {
        error: "Authentication store unavailable".to_string(),
        status: StatusCode::SERVICE_UNAVAILABLE,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// An agent device reaches `/api/mcp` and is refused everywhere else; a
    /// desktop app's key is not limited the same way.
    #[sqlx::test(migrations = "./migrations")]
    async fn an_agent_device_reaches_only_the_mcp_endpoint(pool: PgPool) {
        let key = |seed: u8| virtues_iroh::SecretKey::from_bytes(&[seed; 32]).public();
        for (id, kind, seed) in [("dev_agent", AGENT_KIND, 7u8), ("dev_mac", "desktop_app", 8u8)] {
            sqlx::query(
                "INSERT INTO app_device (id, user_id, kind, label, endpoint_id) VALUES ($1, $2, $3, 'test', $4)",
            )
            .bind(id)
            .bind(crate::middleware::http::OWNER_USER_ID)
            .bind(kind)
            .bind(key(seed).to_string())
            .execute(&pool)
            .await
            .unwrap();
        }
        let ask = |seed: u8, path: &str| {
            let (mut parts, _) = axum::http::Request::builder().uri(path).body(()).unwrap().into_parts();
            parts.extensions.insert(virtues_iroh::ProvenPeer(key(seed)));
            parts
        };

        let mut mcp = ask(7, crate::server::api::mcp::PATH);
        let user = AuthUser::from_request_parts(&mut mcp, &pool).await.unwrap();
        assert_eq!(user.device_id, "dev_agent");

        let mut pages = ask(7, "/api/pages");
        let refused = AuthUser::from_request_parts(&mut pages, &pool).await.unwrap_err();
        assert_eq!(refused.status, StatusCode::FORBIDDEN);

        let mut mac = ask(8, "/api/pages");
        assert_eq!(AuthUser::from_request_parts(&mut mac, &pool).await.unwrap().device_id, "dev_mac");
    }

    /// A request from `peer` (none for one that came over iroh), with an
    /// `X-Forwarded-For` when `forwarded`.
    fn arriving(peer: Option<&str>, forwarded: bool) -> Parts {
        let mut req = axum::http::Request::builder().uri("/api/pages");
        if forwarded {
            req = req.header("x-forwarded-for", "203.0.113.9");
        }
        let (mut parts, _) = req.body(()).unwrap().into_parts();
        if let Some(peer) = peer {
            parts.extensions.insert(ConnectInfo(peer.parse::<SocketAddr>().unwrap()));
        }
        parts
    }

    /// `make dev` binds every interface. The dev fallback admits a loopback
    /// peer and nobody else, so a phone or laptop on the developer's network
    /// gets a 401 like it would from any box. None of these cases reads the
    /// database, so the pool never connects.
    #[tokio::test]
    async fn the_dev_fallback_admits_a_loopback_peer_alone() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://nobody@127.0.0.1:1/none")
            .unwrap();
        for (peer, forwarded, dev) in [
            (Some("127.0.0.1:50000"), true, true),
            (Some("[::ffff:127.0.0.1]:50000"), true, true),
            (Some("[::1]:50000"), true, true),
        ] {
            let mut parts = arriving(peer, forwarded);
            let user = authenticate(&mut parts, &pool, dev).await.unwrap();
            assert_eq!(user.device_id, CONSOLE_DEVICE_ID, "{peer:?}");
        }
        for (peer, forwarded, dev) in [
            // Someone else on the developer's network.
            (Some("192.168.1.20:50000"), false, true),
            (Some("[::ffff:192.168.1.20]:50000"), true, true),
            // No socket peer: what an iroh request from a key with no
            // device row falls through as. A real `ProvenPeer` would read
            // the database, which this test avoids.
            (None, false, true),
            // A proxied loopback request off a developer's stack.
            (Some("127.0.0.1:50000"), true, false),
        ] {
            let mut parts = arriving(peer, forwarded);
            let refused = authenticate(&mut parts, &pool, dev).await.unwrap_err();
            assert_eq!(refused.status, StatusCode::UNAUTHORIZED, "{peer:?} forwarded={forwarded} dev={dev}");
        }
    }

    /// The requests the Host check applies to: exactly the ones the extractor
    /// above admits without reading a device row.
    #[test]
    fn owner_by_transport_names_the_requests_authority_comes_with() {
        let mut proven = arriving(None, true);
        proven
            .extensions
            .insert(virtues_iroh::ProvenPeer(virtues_iroh::SecretKey::from_bytes(&[9; 32]).public()));
        assert!(owner_by_transport(&proven.extensions, &proven.headers, false));

        for (peer, forwarded, dev, expected) in [
            (Some("127.0.0.1:50000"), false, false, true),
            (Some("[::ffff:127.0.0.1]:50000"), false, false, true),
            // Caddy in front of a box: loopback, forwarded, no authority.
            (Some("127.0.0.1:50000"), true, false, false),
            (Some("127.0.0.1:50000"), true, true, true),
            (Some("192.168.1.20:50000"), false, true, false),
            (None, false, true, false),
        ] {
            let parts = arriving(peer, forwarded);
            assert_eq!(
                owner_by_transport(&parts.extensions, &parts.headers, dev),
                expected,
                "{peer:?} forwarded={forwarded} dev={dev}"
            );
        }
    }

    fn headers(v: &str) -> axum::http::HeaderMap {
        let mut h = axum::http::HeaderMap::new();
        h.insert("x-virtues-client", v.parse().unwrap());
        h
    }

    #[test]
    fn client_header_parses_with_and_without_app() {
        let cb = parse_client_header(&headers("version=0.1.5; sha=abc1234; channel=staging")).unwrap();
        assert_eq!(cb.version, "0.1.5");
        assert_eq!(cb.app, None);

        let cb = parse_client_header(&headers(
            "version=0.1.5; sha=abc1234; channel=staging; app=1.0.23",
        ))
        .unwrap();
        assert_eq!(cb.app.as_deref(), Some("1.0.23"));

        // Hostile lengths are rejected wholesale, same as the other fields.
        let long = format!("version=0.1.5; sha=a; channel=c; app={}", "x".repeat(65));
        assert!(parse_client_header(&headers(&long)).is_none());
    }

    /// The merge semantics the Devices page depends on: an app-less write
    /// (early SPA request before the shell bridge resolves) must never erase a
    /// previously reported app version, and an app change under an unchanged
    /// UI bundle must still write (the old sha-only guard would have skipped
    /// exactly the write that records a native app update).
    #[sqlx::test(migrations = "./migrations")]
    async fn record_client_build_merges_and_guards(pool: PgPool) {
        sqlx::query(
            "INSERT INTO app_device (id, user_id, kind, label) \
             VALUES ('dev_test', $1, 'desktop_app', 'test')",
        )
        .bind(crate::middleware::http::OWNER_USER_ID)
        .execute(&pool)
        .await
        .unwrap();

        async fn build(pool: &PgPool) -> serde_json::Value {
            sqlx::query_scalar::<_, serde_json::Value>(
                "SELECT device_info->'build' FROM app_device WHERE id='dev_test'",
            )
            .fetch_one(pool)
            .await
            .unwrap()
        }

        // First report carries the app.
        let mut cb = ClientBuild {
            version: "0.1.5".into(),
            sha: "aaa1111".into(),
            channel: "staging".into(),
            app: Some("1.0.22".into()),
        };
        record_client_build(&pool, "dev_test", &cb).await;
        assert_eq!(build(&pool).await["app"], "1.0.22");

        // App-less write with a NEW bundle sha: bundle fields move, app stays.
        cb.sha = "bbb2222".into();
        cb.app = None;
        record_client_build(&pool, "dev_test", &cb).await;
        let b = build(&pool).await;
        assert_eq!(b["sha"], "bbb2222");
        assert_eq!(b["app"], "1.0.22");

        // App update under an UNCHANGED bundle must still write.
        cb.app = Some("1.0.23".into());
        record_client_build(&pool, "dev_test", &cb).await;
        assert_eq!(build(&pool).await["app"], "1.0.23");
    }
}

//! Pair-only auth bootstrap.
//!
//! One primitive — a short-lived 24-byte random token in `app_pair_token` —
//! grants the right to enroll a device. The token never carries any
//! long-lived secret; the redeeming device generates its own keypair (for
//! WG-capable devices) and submits the pubkey when consuming.
//!
//! Mint paths
//!   - CLI (`virtues link`): minted on the box itself; status starts
//!     `authorized` because physical access proves intent. Used for first-pair
//!     and recovery.
//!   - Web ("+ Add Device" from a paired browser): status starts `pending`;
//!     the minting device must POST `/api/pair/confirm/:id` before the new
//!     device can redeem. Defeats shoulder-surf-the-QR attacks.
//!
//! Consume path (`POST /api/pair/consume`)
//!   - Accepts `{token, kind, label, device_info, wg_public_key?}`.
//!   - For `kind = browser`: creates a device row + session cookie.
//!   - For `kind = mobile_app | desktop_app | sensor`: creates a device row +
//!     a `credentials` row with a server-issued bearer (HMAC-lookup, encrypted
//!     at rest) + optional WG bundle if `wg_public_key` was supplied.
//!
//! Status path (`GET /api/pair/status/:id`) — RFC 8628-shaped polling that the
//! "+ Add Device" modal hits to know when the new device has finished.
//!
//! Token storage: SHA-256(token) is persisted; the raw token only exists in
//! the response that goes back to the client + the QR URL. Atomic
//! single-use semantics via `UPDATE … WHERE status = 'authorized' RETURNING`.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::{Duration, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::middleware::auth::{AuthUser, SESSION_COOKIE_NAME, SESSION_COOKIE_NAME_SECURE};
use crate::middleware::{client_ip, is_secure_environment, rate_limit_ip, OWNER_USER_ID};

// ─── Constants ──────────────────────────────────────────────────────────────

/// Window during which a `pending` web-minted token can be confirmed by the
/// minting device. After this elapses the row is `expired`.
const PENDING_CONFIRM_TTL_MIN: i64 = 10;

/// Window during which an `authorized` token can be consumed.
const AUTHORIZED_REDEEM_TTL_MIN: i64 = 5;

/// CLI-minted tokens get a longer window — they're typed into the desktop app,
/// and the user may need a few minutes to download it the first time.
const CLI_REDEEM_TTL_MIN: i64 = 30;

/// Session cookie hard expiry (idle expiry is shorter and enforced in
/// middleware via `last_used_at`).
const SESSION_TTL_DAYS: i64 = 30;

// ─── Token helpers ──────────────────────────────────────────────────────────

/// Generate a short human-typeable pair code: 6 chars from an unambiguous
/// uppercase alphabet (A-Z minus I and O, which are indistinguishable from
/// 1 and 0 in many fonts). 24^6 ≈ 191M combinations; combined with rate
/// limiting on /api/pair/consume this is unbrutable in the 30-min window.
/// Displayed as "ABC DEF" in the CLI handoff, entered that way in the app.
fn random_pair_code() -> String {
    const ALPHA: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ"; // 24 chars
    let mut code = String::with_capacity(6);
    let mut buf = [0u8; 1];
    let mut rng = rand::rng();
    while code.len() < 6 {
        rng.fill_bytes(&mut buf);
        // 240 = 10 * 24; reject 240–255 so every char maps with equal probability.
        if buf[0] < 240 {
            code.push(ALPHA[(buf[0] % 24) as usize] as char);
        }
    }
    code
}

fn random_32_session_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn random_32_bearer() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn hash_token(token: &str) -> String {
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    hex::encode(h.finalize())
}

// ─── Public mint (used by CLI `virtues link` and by web "+ Add Device") ────

/// Mint a pair token. `minted_by_device` is `None` for CLI mints (status
/// starts `authorized`) and `Some(device_id)` for web mints (status starts
/// `pending`, awaits confirm from the minting device).
///
/// Returns the raw token string — the caller is responsible for delivering it
/// to the user (CLI prints the URL; web returns it in the modal payload).
/// Only SHA-256(token) is persisted.
pub async fn mint_pair_token(
    pool: &PgPool,
    minted_by_device: Option<&str>,
    intended_kind: Option<&str>,
) -> crate::Result<MintedToken> {
    let token = random_pair_code();
    let token_hash = hash_token(&token);
    let id = crate::ids::generate_id(
        crate::ids::PAIR_TOKEN_PREFIX,
        &[&token_hash[..16]],
    );

    let (status, minted_via, ttl_min) = match minted_by_device {
        None => ("authorized", "cli", CLI_REDEEM_TTL_MIN),
        Some(_) => ("pending", "web", PENDING_CONFIRM_TTL_MIN),
    };
    let expires_at = Utc::now() + Duration::minutes(ttl_min);
    let authorized_at = if status == "authorized" { Some(Utc::now()) } else { None };

    sqlx::query(
        "INSERT INTO app_pair_token \
         (id, token_hash, minted_by_device, minted_via, intended_kind, \
          status, authorized_at, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&id)
    .bind(&token_hash)
    .bind(minted_by_device)
    .bind(minted_via)
    .bind(intended_kind)
    .bind(status)
    .bind(authorized_at)
    .bind(expires_at)
    .execute(pool)
    .await
    .map_err(|e| crate::Error::Database(format!("insert pair token: {e}")))?;

    Ok(MintedToken {
        id,
        token,
        expires_at,
        status: status.to_string(),
    })
}

pub struct MintedToken {
    pub id: String,
    pub token: String,
    pub expires_at: chrono::DateTime<Utc>,
    pub status: String,
}

impl MintedToken {
    /// Human-readable display of the pair code, grouped as "ABC DEF".
    /// The raw token (no space) is used in URL fragments and API calls;
    /// the display form is what the user sees in the CLI and types in the app.
    pub fn display_code(&self) -> String {
        let t = &self.token;
        if t.len() == 6 {
            format!("{} {}", &t[..3], &t[3..])
        } else {
            t.clone()
        }
    }
}

// ─── HTTP handlers ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct MintRequest {
    /// Optional hint about what kind of device the QR is for — surfaces in
    /// the modal label so the user knows which token is which.
    pub intended_kind: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MintResponse {
    pub id: String,
    pub token: String,
    pub pair_url: String,
    /// SVG QR encoding `pair_url`. Generated server-side specifically to keep
    /// the token off third-party services — the data: URL never leaves the
    /// box, the user's browser, and the new device's camera.
    pub qr_svg: String,
    pub expires_at: String,
}

/// `POST /api/pair/mint` — auth'd. Paired user creates a token to add a new
/// device. Status starts `pending` and the minting device must call
/// `/api/pair/confirm/:id` to authorize.
pub async fn mint_handler(
    State(pool): State<PgPool>,
    user: AuthUser,
    Json(req): Json<MintRequest>,
) -> impl IntoResponse {
    let kind = req.intended_kind.as_deref();
    let minted = match mint_pair_token(&pool, Some(&user.device_id), kind).await {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("pair mint failed: {e:#}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "mint_failed"})),
            )
                .into_response();
        }
    };

    let _ = log_event(
        &pool,
        Some(&user.device_id),
        "pair_token_minted",
        json!({"intended_kind": kind, "token_id": &minted.id}),
        None,
        None,
    )
    .await;

    // Embed the box's SPKI fingerprint in the QR so the scanning device can
    // verify the WG server key it gets back in the bundle was not substituted
    // by a LAN MITM. The QR travels over an out-of-band channel (the screen),
    // not the spoofable HTTP response, so it's a trustworthy carrier.
    let fpr = box_spki_fpr(&pool).await;
    let pair_url = format_pair_url(&minted.token, fpr.as_deref());
    let qr_svg = render_qr_svg(&pair_url);
    (
        StatusCode::OK,
        Json(MintResponse {
            id: minted.id,
            token: minted.token,
            pair_url,
            qr_svg,
            expires_at: minted.expires_at.to_rfc3339(),
        }),
    )
        .into_response()
}

#[derive(Debug, Serialize)]
pub struct MintCollectorResponse {
    pub token: String,
    pub expires_at: String,
}

/// `POST /api/pair/mint-collector` — auth'd. Mints a one-time pair token for
/// installing the local data collector (`virtues-collector`) and authorizes it
/// in the SAME call. Unlike `/api/pair/mint` (which starts `pending` and needs
/// a separate `/confirm` from a second device), the collector runs on the same
/// machine as the already-authenticated owner session, so the confirm
/// round-trip is friction with no security gain — the caller IS the owner on
/// this host. The raw token is handed straight to `installCollector(token)` via
/// the Tauri bridge; the collector redeems it at `/api/pair/consume` declaring
/// `source="mac"` to receive its `mac_activity` action fan-out.
pub async fn mint_collector_handler(
    State(pool): State<PgPool>,
    user: AuthUser,
) -> impl IntoResponse {
    let minted = match mint_pair_token(&pool, Some(&user.device_id), Some("desktop_app")).await {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("collector mint failed: {e:#}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "mint_failed"})),
            )
                .into_response();
        }
    };
    // Self-authorize (same-host owner; no second-device confirm). Mirrors the
    // confirm_handler transition but folded in for the local-install case.
    let authorized = sqlx::query(
        "UPDATE app_pair_token \
         SET status = 'authorized', authorized_at = now(), \
             expires_at = now() + make_interval(mins => $3::int) \
         WHERE id = $1 \
           AND minted_by_device = $2 \
           AND minted_via = 'web' \
           AND status = 'pending'",
    )
    .bind(&minted.id)
    .bind(&user.device_id)
    .bind(AUTHORIZED_REDEEM_TTL_MIN as i32)
    .execute(&pool)
    .await;
    match authorized {
        Ok(r) if r.rows_affected() == 1 => {}
        _ => {
            tracing::warn!("collector token self-authorize failed: {}", minted.id);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "authorize_failed"})),
            )
                .into_response();
        }
    }
    let _ = log_event(
        &pool,
        Some(&user.device_id),
        "collector_token_minted",
        json!({"token_id": &minted.id}),
        None,
        None,
    )
    .await;
    let expires_at = (Utc::now() + Duration::minutes(AUTHORIZED_REDEEM_TTL_MIN)).to_rfc3339();
    (
        StatusCode::OK,
        Json(MintCollectorResponse {
            token: minted.token,
            expires_at,
        }),
    )
        .into_response()
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub status: String,
    pub consumed_by_device: Option<String>,
    pub consumed_by_label: Option<String>,
}

/// `GET /api/pair/status/:id` — auth'd. Polled by the "+ Add Device" modal
/// to know when the new device has redeemed (or the token expired/denied).
pub async fn status_handler(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Limit visibility to the device that minted the token — prevents one
    // paired device from learning about another's pending pair.
    let row: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT t.status, t.consumed_by_device, d.label \
         FROM app_pair_token t \
         LEFT JOIN app_device d ON d.id = t.consumed_by_device \
         WHERE t.id = $1 AND t.minted_by_device = $2",
    )
    .bind(&id)
    .bind(&user.device_id)
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten();

    match row {
        Some((status, by_id, by_label)) => (
            StatusCode::OK,
            Json(StatusResponse {
                status,
                consumed_by_device: by_id,
                consumed_by_label: by_label,
            }),
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({"error": "not_found"}))).into_response(),
    }
}

/// `POST /api/pair/confirm/:id` — auth'd. The minting device approves a
/// pending pair, transitioning the token to `authorized`. Web-minted tokens
/// cannot be consumed before this fires.
pub async fn confirm_handler(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // `minted_via = 'web'` is defense-in-depth: today CLI mints land directly
    // in `authorized`, so the `status = 'pending'` filter already excludes
    // them, but if that ever changes we don't want a CLI-origin token to be
    // re-confirmable by a paired browser session.
    let result = sqlx::query(
        "UPDATE app_pair_token \
         SET status = 'authorized', authorized_at = now(), \
             expires_at = now() + make_interval(mins => $3::int) \
         WHERE id = $1 \
           AND minted_by_device = $2 \
           AND minted_via = 'web' \
           AND status = 'pending' \
           AND expires_at > now()",
    )
    .bind(&id)
    .bind(&user.device_id)
    .bind(AUTHORIZED_REDEEM_TTL_MIN as i32)
    .execute(&pool)
    .await;

    match result {
        Ok(r) if r.rows_affected() == 1 => {
            let _ = log_event(
                &pool,
                Some(&user.device_id),
                "pair_token_authorized",
                json!({"token_id": &id}),
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({"ok": true}))).into_response()
        }
        Ok(_) => (
            StatusCode::CONFLICT,
            Json(json!({"error": "not_pending"})),
        )
            .into_response(),
        Err(e) => {
            tracing::warn!("pair confirm db error: {e:#}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal"})),
            )
                .into_response()
        }
    }
}

/// `POST /api/pair/deny/:id` — auth'd. The minting device explicitly denies
/// a pending pair (e.g. "this wasn't me"). Transitions to `denied`.
pub async fn deny_handler(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let _ = sqlx::query(
        "UPDATE app_pair_token \
         SET status = 'denied' \
         WHERE id = $1 AND minted_by_device = $2 AND status = 'pending'",
    )
    .bind(&id)
    .bind(&user.device_id)
    .execute(&pool)
    .await;

    let _ = log_event(
        &pool,
        Some(&user.device_id),
        "pair_token_denied",
        json!({"token_id": &id}),
        None,
        None,
    )
    .await;
    (StatusCode::OK, Json(json!({"ok": true}))).into_response()
}

// ─── Consume ────────────────────────────────────────────────────────────────

/// Resolve the credential `source_id` from a pair-consume `kind` + optional
/// explicit `source` (see [`ConsumeRequest::source`]).
///
/// - Explicit `source` present + non-blank: must resolve via the sources
///   catalog (`lookup_source`), else `Err(())` (caller → 400 `invalid_source`).
/// - Absent + `kind = "mobile_app"`: `"ios"` (mobile is unambiguously iOS today,
///   so collectors needn't change).
/// - Otherwise: `"__device__"` — a sentinel that matches no template, so NO
///   action fan-out. Correct for the WG desktop daemon (`kind=desktop_app`, no
///   source — NOT a collector) and for `sensor` (no source defined yet).
///
/// This is the fix for the `__device__`-matches-nothing bug: a collector that
/// declares its real source now gets `reconcile_templates` to create its
/// per-credential webhook actions.
fn resolve_source_id(kind: &str, source: Option<&str>) -> Result<String, ()> {
    match source.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => {
            if crate::action_templates::lookup_source(s).is_none() {
                Err(())
            } else {
                Ok(s.to_string())
            }
        }
        None if kind == "mobile_app" => Ok("ios".to_string()),
        None => Ok("__device__".to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct ConsumeRequest {
    pub token: String,
    pub kind: String,                                 // 'browser' | 'mobile_app' | 'desktop_app' | 'sensor'
    pub label: Option<String>,                        // auto-generated if absent
    pub device_info: Option<Value>,                   // arbitrary JSON describing the device
    pub wg_public_key: Option<String>,                // WG-capable devices only
    /// The data source this collector represents (`"mac"`, `"ios"`, …, from
    /// `actions/sources.toml`). REQUIRED for a collector to receive its
    /// per-credential action fan-out — the credential's `source_id` is set
    /// from this so `reconcile_templates` matches the source's webhook
    /// templates. `kind="desktop_app"` is ambiguous (the WG daemon AND
    /// mac-source both use it), so collectors MUST declare `source` explicitly;
    /// `mobile_app` defaults to `"ios"`. Absent/`"__device__"` → no fan-out
    /// (correct for the WG desktop daemon, which is not a collector).
    pub source: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ConsumeResponse {
    pub device_id: String,
    /// The credential row id (`AUTH_TOKEN_PREFIX`). This is the id the device
    /// must send to `DELETE /api/credentials/:id` to revoke itself — distinct
    /// from `device_id` (the `app_device.id`). Absent for browser pairings,
    /// whose credential is a session cookie with no revocable row id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<String>,
    pub redirect: String,
    /// Server-issued bearer — returned ONCE for non-browser devices. Browsers
    /// don't see this; their credential is a session cookie set on this response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bearer: Option<String>,
    /// Map of `binary-name → app_actions.id` for the per-credential action
    /// fan-out. Returned for `kind = mobile_app | desktop_app | sensor` so
    /// the device knows which webhook id to POST each stream flush to. Empty
    /// for browser pairings (browsers don't run actions).
    #[serde(skip_serializing_if = "std::collections::HashMap::is_empty", default)]
    pub action_ids: std::collections::HashMap<String, String>,
    /// WireGuard provisioning bundle. Present when the device supplied a
    /// `wg_public_key` AND the box's WG engine is operational (Linux only,
    /// `assemble_bundle` succeeded). Absent on macOS dev hosts and on
    /// devices that didn't request a tunnel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle: Option<crate::wireguard::bundle::PairingBundle>,
}

/// `POST /api/pair/consume` — anonymous, but valid token required.
pub async fn consume_handler(
    State(pool): State<PgPool>,
    headers: axum::http::HeaderMap,
    jar: CookieJar,
    Json(body): Json<ConsumeRequest>,
) -> axum::response::Response {
    let token = body.token.trim();
    if token.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "missing_token"})))
            .into_response();
    }

    // Per-IP rate limit: 10 attempts per 30-minute window. Defends the 6-char
    // code space against LAN enumeration. We key on the proxy-appended
    // (right-most) XFF entry so a client can't earn a fresh budget by spoofing
    // the header. A request with NO XFF didn't transit our proxy (direct
    // loopback / dev) and isn't remotely reachable, so it's exempt rather than
    // sharing one bucket with every other header-less caller.
    if let Some(ip_key) = rate_limit_ip(&headers) {
        if !crate::middleware::rate_limit::pair_limiter().check_and_record(&ip_key) {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({"error": "too_many_attempts", "retry_after_secs": 1800})),
            )
                .into_response();
        }
    }

    let kind = match body.kind.as_str() {
        "browser" | "mobile_app" | "desktop_app" | "sensor" => body.kind.as_str(),
        _ => {
            return (StatusCode::BAD_REQUEST, Json(json!({"error": "invalid_kind"})))
                .into_response()
        }
    };

    // Resolve the credential's source (see `resolve_source_id`). A bad explicit
    // source is a loud 400, not a silent no-fan-out.
    let source_id = match resolve_source_id(kind, body.source.as_deref()) {
        Ok(s) => s,
        Err(()) => {
            return (StatusCode::BAD_REQUEST, Json(json!({"error": "invalid_source"})))
                .into_response()
        }
    };

    let token_hash = hash_token(token);
    let ip = client_ip(&headers);
    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Build everything we need OUTSIDE the transaction: the device id, label,
    // and (for non-browser devices) the encrypted bearer. This keeps the tx
    // short — it only does atomic DB writes, no crypto, no JSON munging.
    let device_id = crate::ids::generate_id(
        crate::ids::DEVICE_PREFIX,
        &[&token_hash[..16], &Utc::now().to_rfc3339()],
    );
    let label = body
        .label
        .as_deref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_label_for(kind, user_agent.as_deref(), &body.device_info));
    let device_info = body
        .device_info
        .clone()
        .unwrap_or_else(|| json!({}));

    // For non-browser devices, pre-encrypt the bearer so a slow KMS call
    // doesn't hold the DB transaction open.
    let bearer_pack = if kind == "browser" {
        None
    } else {
        match build_bearer_pack(kind, &label, &body.device_info, body.wg_public_key.as_deref()) {
            Ok(p) => Some(p),
            Err(e) => return e.into_response(),
        }
    };

    // Browser session token is also pre-generated (cheap), so the tx is
    // purely DB writes.
    let session_pack = if kind == "browser" {
        Some(SessionPack {
            id: crate::ids::generate_id(
                crate::ids::AUTH_SESSION_PREFIX,
                &[&device_id, &Utc::now().to_rfc3339()],
            ),
            token: random_32_session_token(),
            expires_at: Utc::now() + Duration::days(SESSION_TTL_DAYS),
        })
    } else {
        None
    };

    // ─── Single transaction: claim token + create device + create credential
    //     (or session) + back-link the token. Any failure rolls everything back
    //     including the token claim — caller can retry with the same token. ──
    let mut tx = match pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("pair consume: tx begin failed: {e:#}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal"})),
            )
                .into_response();
        }
    };

    // Atomic claim. UPDATE-RETURNING inside the tx means a concurrent claim
    // sees `status = 'consumed'` once we commit; before commit, the row is
    // locked. `consumed_by_device` is left NULL on this claim because the
    // device row doesn't exist yet — we back-fill it after the INSERT below.
    // (The FK constraint on consumed_by_device would otherwise reject the
    // UPDATE.) On error we surface the DB message so a real bug doesn't
    // masquerade as `invalid_or_expired_token`.
    let claimed: Option<(String,)> = match sqlx::query_as(
        "UPDATE app_pair_token \
         SET status = 'consumed', consumed_at = now() \
         WHERE token_hash = $1 \
           AND status = 'authorized' \
           AND expires_at > now() \
         RETURNING id",
    )
    .bind(&token_hash)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(row) => row,
        Err(e) => {
            tracing::warn!("pair consume: token claim failed: {e:#}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal"})),
            )
                .into_response();
        }
    };

    let token_id = match claimed {
        Some((id,)) => id,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "invalid_or_expired_token"})),
            )
                .into_response();
        }
    };

    if let Err(e) = sqlx::query(
        "INSERT INTO app_device \
         (id, user_id, kind, label, device_info, paired_from_ip, last_seen_at) \
         VALUES ($1, $2, $3, $4, $5, $6, now())",
    )
    .bind(&device_id)
    .bind(OWNER_USER_ID)
    .bind(kind)
    .bind(&label)
    .bind(&device_info)
    .bind(&ip)
    .execute(&mut *tx)
    .await
    {
        tracing::warn!("pair consume: device insert failed: {e:#}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "device_insert_failed"})),
        )
            .into_response();
    }

    // Back-fill the token → device link now that the device row exists.
    if let Err(e) = sqlx::query(
        "UPDATE app_pair_token SET consumed_by_device = $1 WHERE id = $2",
    )
    .bind(&device_id)
    .bind(&token_id)
    .execute(&mut *tx)
    .await
    {
        tracing::warn!("pair consume: token backlink failed: {e:#}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "internal"})),
        )
            .into_response();
    }

    if kind == "browser" {
        let sp = session_pack.as_ref().expect("session_pack for browser");
        if let Err(e) = sqlx::query(
            "INSERT INTO app_auth_session \
             (id, session_token, device_id, expires_at, last_used_at) \
             VALUES ($1, $2, $3, $4, now())",
        )
        .bind(&sp.id)
        .bind(&sp.token)
        .bind(&device_id)
        .bind(sp.expires_at)
        .execute(&mut *tx)
        .await
        {
            tracing::warn!("pair consume: session insert failed: {e:#}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "session_insert_failed"})),
            )
                .into_response();
        }
    } else {
        let bp = bearer_pack.as_ref().expect("bearer_pack for non-browser");
        if let Err(e) = sqlx::query(
            "INSERT INTO credentials \
             (id, source_id, name, device_id, status, secrets_ciphertext, \
              secret_lookup_hash, metadata, last_seen_at) \
             VALUES ($1, $2, $3, $4, 'active', $5, $6, $7, now())",
        )
        .bind(&bp.credential_id)
        .bind(&source_id)
        .bind(&label)
        .bind(&device_id)
        .bind(&bp.ciphertext)
        .bind(&bp.lookup_hash)
        .bind(&bp.metadata)
        .execute(&mut *tx)
        .await
        {
            tracing::warn!("pair consume: credential insert failed: {e:#}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "credential_insert_failed"})),
            )
                .into_response();
        }
    }

    if let Err(e) = tx.commit().await {
        tracing::warn!("pair consume: tx commit failed: {e:#}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "internal"})),
        )
            .into_response();
    }

    // Post-commit: log the pairing event (best-effort) and assemble the
    // response (cookie for browsers, bearer for everything else).
    let _ = log_event(
        &pool,
        Some(&device_id),
        "paired",
        json!({
            "kind": kind,
            "label": &label,
            "has_wg": body.wg_public_key.is_some(),
            "token_id": &token_id,
        }),
        ip,
        user_agent,
    )
    .await;

    if let Some(sp) = session_pack {
        let is_secure = is_secure_environment();
        let cookie_name = if is_secure {
            SESSION_COOKIE_NAME_SECURE
        } else {
            SESSION_COOKIE_NAME
        };
        let cookie = Cookie::build((cookie_name, sp.token))
            .path("/")
            .http_only(true)
            .secure(is_secure)
            .same_site(SameSite::Lax)
            .max_age(time::Duration::days(SESSION_TTL_DAYS))
            .build();
        let jar = jar.add(cookie);
        return (
            jar,
            (
                StatusCode::OK,
                Json(ConsumeResponse {
                    device_id,
                    credential_id: None,
                    redirect: "/".to_string(),
                    bearer: None,
                    action_ids: std::collections::HashMap::new(),
                    bundle: None,
                }),
            ),
        )
            .into_response();
    }

    // App / sensor pairing: assemble the per-device action fan-out so the
    // device knows which `app_actions.id` to POST each stream flush to, and
    // (when a WG pubkey was supplied) the WG provisioning bundle. Both are
    // post-commit best-effort: a failure here doesn't undo the pairing, the
    // device just shows up paired but with no per-credential actions until
    // a manual `virtues reconcile` (or the next legacy flow sync). The
    // device handler can call `/api/devices/<id>/reconcile` to retry. For
    // v1, we log loudly and let the user re-pair if they hit this.
    let bp = bearer_pack.expect("bearer_pack present for non-browser");

    let action_ids = match assemble_action_fanout(&pool, &bp.credential_id).await {
        Ok(map) => map,
        Err(e) => {
            tracing::warn!(
                "pair consume: action fanout failed for credential {}: {e:#}; \
                 device paired but actions not wired",
                bp.credential_id
            );
            std::collections::HashMap::new()
        }
    };

    let bundle = match body.wg_public_key.as_deref() {
        Some(pubkey) if !pubkey.is_empty() => {
            match assemble_wg_bundle(&pool, &bp.credential_id, &bp.bearer, pubkey).await {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!(
                        "pair consume: WG bundle assembly failed for credential {}: {e:#}; \
                         device paired without tunnel",
                        bp.credential_id
                    );
                    None
                }
            }
        }
        _ => None,
    };

    (
        StatusCode::OK,
        Json(ConsumeResponse {
            device_id,
            credential_id: Some(bp.credential_id),
            redirect: "/".to_string(),
            bearer: Some(bp.bearer),
            action_ids,
            bundle,
        }),
    )
        .into_response()
}

// ─── Post-commit fan-out + WG bundle assembly ──────────────────────────────
//
// Both run AFTER the consume transaction commits — failure logs but doesn't
// undo the pairing. They're shaped as their own helpers so the consume
// handler stays the easy-to-read top-level flow.

/// Reconcile action templates (so per-credential `app_actions` rows are
/// fanned out) and read back the binary-name → action-id map the device
/// uses to route stream flushes to `POST /webhook/<action_id>`. Lifted
/// out of the legacy `pair_complete_handler` so the unified pair flow
/// produces identical device-side behavior.
async fn assemble_action_fanout(
    pool: &PgPool,
    credential_id: &str,
) -> Result<std::collections::HashMap<String, String>, crate::Error> {
    crate::action_templates::reconcile_templates(pool).await?;
    virtues_helpers::auth::fanout_action_ids(pool, credential_id)
        .await
        .map_err(|e| crate::Error::Other(format!("fanout_action_ids: {e}")))
}

/// Assemble the WG provisioning bundle on Linux; no-op on the macOS dev
/// host (the WG engine is Linux-only). When the device supplied a
/// `wg_public_key`, the box installs them as a peer and returns the bundle
/// of (server pubkey, allowed IPs, endpoint, CA root, rendezvous capability)
/// the device needs to dial the tunnel later.
#[cfg(target_os = "linux")]
async fn assemble_wg_bundle(
    pool: &PgPool,
    credential_id: &str,
    bearer: &str,
    pubkey: &str,
) -> Result<Option<crate::wireguard::bundle::PairingBundle>, crate::Error> {
    crate::wireguard::pairing::assemble_bundle(pool, credential_id, bearer, pubkey)
        .await
        .map(Some)
        .map_err(|e| crate::Error::Other(format!("assemble_bundle: {e}")))
}

#[cfg(not(target_os = "linux"))]
async fn assemble_wg_bundle(
    _pool: &PgPool,
    _credential_id: &str,
    _bearer: &str,
    _pubkey: &str,
) -> Result<Option<crate::wireguard::bundle::PairingBundle>, crate::Error> {
    // On the macOS dev host there's no kernel WG and no rendezvous publisher.
    // The pubkey is recorded in the credential's metadata (in `consume_handler`
    // above); the iOS app will simply have no bundle to dial from this dev box.
    tracing::debug!("WG bundle assembly skipped (non-Linux host)");
    Ok(None)
}

// ─── Bearer pack builder (extracted from consume_handler for tx hygiene) ────

struct BearerPack {
    credential_id: String,
    bearer: String,
    ciphertext: String,
    lookup_hash: String,
    metadata: Value,
}

struct SessionPack {
    id: String,
    token: String,
    expires_at: chrono::DateTime<Utc>,
}

/// Failure modes from the bearer-pack builder. Kept domain-flavored (no
/// HTTP types) so the helper stays testable; the caller maps to a response
/// at the boundary.
#[derive(Debug)]
enum BearerPackError {
    EncryptionUnavailable,
    EncryptionFailed,
    LookupHashFailed,
}

impl BearerPackError {
    fn into_response(self) -> axum::response::Response {
        let code = match self {
            BearerPackError::EncryptionUnavailable => "encryption_unavailable",
            BearerPackError::EncryptionFailed => "encryption_failed",
            BearerPackError::LookupHashFailed => "lookup_hash_failed",
        };
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": code}))).into_response()
    }
}

/// Mint a bearer + its encrypted form + lookup hash + metadata blob. Anything
/// CPU- or IO-bound (KMS, hashing) happens here, BEFORE we open a DB
/// transaction in `consume_handler`.
fn build_bearer_pack(
    kind: &str,
    label: &str,
    device_info: &Option<Value>,
    wg_public_key: Option<&str>,
) -> Result<BearerPack, BearerPackError> {
    let bearer = random_32_bearer();
    let credential_id = crate::ids::generate_id(
        crate::ids::AUTH_TOKEN_PREFIX,
        &[&bearer[..16], &Utc::now().to_rfc3339()],
    );
    let encryptor = crate::crypto::TokenEncryptor::from_env().map_err(|e| {
        tracing::warn!("encryptor init failed: {e:#}");
        BearerPackError::EncryptionUnavailable
    })?;
    let ciphertext = encryptor
        .encrypt(&json!({"token": bearer}).to_string())
        .map_err(|e| {
            tracing::warn!("bearer encrypt failed: {e:#}");
            BearerPackError::EncryptionFailed
        })?;
    let lookup_hash = encryptor.lookup_hash(&bearer).map_err(|e| {
        tracing::warn!("bearer lookup_hash failed: {e:#}");
        BearerPackError::LookupHashFailed
    })?;
    let mut metadata = json!({"label": label, "kind": kind});
    if let Some(Value::Object(map)) = device_info {
        if let Value::Object(meta) = &mut metadata {
            for (k, v) in map {
                meta.insert(k.clone(), v.clone());
            }
        }
    }
    if let Some(pubkey) = wg_public_key {
        if let Value::Object(meta) = &mut metadata {
            meta.insert("wg_public_key".to_string(), Value::String(pubkey.to_string()));
        }
    }
    Ok(BearerPack {
        credential_id,
        bearer,
        ciphertext,
        lookup_hash,
        metadata,
    })
}

// ─── Internal helpers ───────────────────────────────────────────────────────

/// Render the pair URL as an SVG QR code, in-process. Kept inline (no network,
/// no third-party service) so the token never leaves the box's process
/// boundary on its way to the user's browser.
fn render_qr_svg(data: &str) -> String {
    use qrcode::{render::svg, QrCode};
    match QrCode::new(data.as_bytes()) {
        Ok(code) => code
            .render::<svg::Color<'_>>()
            .min_dimensions(240, 240)
            .quiet_zone(true)
            .dark_color(svg::Color("#000000"))
            .light_color(svg::Color("#ffffff"))
            .build(),
        Err(e) => {
            tracing::warn!("qr render failed: {e}");
            // Fall back to an empty SVG; the modal also shows the URL as text.
            "<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>".to_string()
        }
    }
}

/// The box's SPKI fingerprint (`sha256-<b64nopad>` of its WG server public key),
/// for out-of-band identity verification at pairing. `None` on a host with no WG
/// engine (macOS dev) — such a box returns no tunnel bundle either, so there's
/// nothing to verify.
#[cfg(target_os = "linux")]
async fn box_spki_fpr(pool: &PgPool) -> Option<String> {
    let kp = crate::wireguard::reconcile::ensure_server_keypair(pool)
        .await
        .ok()?;
    let raw = base64::engine::general_purpose::STANDARD
        .decode(kp.public_key.trim())
        .ok()?;
    let arr: [u8; 32] = raw.try_into().ok()?;
    Some(virtues_protocol::spki_fingerprint(&arr).to_string())
}

#[cfg(not(target_os = "linux"))]
async fn box_spki_fpr(_pool: &PgPool) -> Option<String> {
    None
}

fn format_pair_url(token: &str, fpr: Option<&str>) -> String {
    // URL fragment (not query): fragments never leave the browser, so the
    // token doesn't end up in proxy logs or referer headers.
    //
    // `VIRTUES_PUBLIC_URL` is the canonical override for setups where the
    // box is reachable at a different mDNS/DNS name than `virtues.local`. In
    // its absence we fall back to the documented default. We *warn* (not
    // error) so an misconfigured box still mints a URL — it just may not be
    // the right one for the network.
    let base = match std::env::var("VIRTUES_PUBLIC_URL") {
        Ok(url) => url,
        Err(_) if is_secure_environment() => {
            // The box has no TLS surface; pair URLs land on plain HTTP at the
            // canonical port. On the box itself (loopback) this is auto-authed;
            // from other devices the URL is consumed by the Virtues client
            // daemon (v0.2). If your network reaches the box at a different
            // hostname, set VIRTUES_PUBLIC_URL in /etc/virtues/env.
            format!("http://localhost:{}", crate::wireguard::INTERNAL_PORT)
        }
        Err(_) => {
            let port = std::env::var("VIRTUES_WEB_PORT").unwrap_or_else(|_| "5173".to_string());
            format!("http://localhost:{port}")
        }
    };
    match fpr {
        Some(fpr) => format!("{base}/pair#t={token}&fpr={fpr}"),
        None => format!("{base}/pair#t={token}"),
    }
}

fn default_label_for(kind: &str, ua: Option<&str>, info: &Option<Value>) -> String {
    // Apps usually send a structured device_info — prefer those fields.
    if let Some(Value::Object(map)) = info {
        let name = map.get("device_name").and_then(|v| v.as_str());
        let model = map.get("device_model").and_then(|v| v.as_str());
        match (name, model) {
            (Some(n), _) if !n.is_empty() => return n.to_string(),
            (_, Some(m)) if !m.is_empty() => return m.to_string(),
            _ => {}
        }
    }
    match (kind, ua) {
        ("browser", Some(ua)) => parse_browser_label(ua),
        ("browser", None) => "Browser".to_string(),
        ("mobile_app", _) => "Mobile app".to_string(),
        ("desktop_app", _) => "Desktop app".to_string(),
        ("sensor", _) => "Sensor".to_string(),
        _ => "Device".to_string(),
    }
}

fn parse_browser_label(ua: &str) -> String {
    // Cheap UA classifier — not exhaustive, just enough to give the user
    // something recognizable in the device list. Hardware first.
    let hardware = if ua.contains("iPhone") {
        "iPhone"
    } else if ua.contains("iPad") {
        "iPad"
    } else if ua.contains("Android") {
        "Android"
    } else if ua.contains("Macintosh") {
        "Mac"
    } else if ua.contains("Windows") {
        "Windows"
    } else if ua.contains("Linux") {
        "Linux"
    } else {
        "Browser"
    };
    let browser = if ua.contains("Edg/") {
        " · Edge"
    } else if ua.contains("Chrome/") && !ua.contains("Chromium") {
        " · Chrome"
    } else if ua.contains("Firefox/") {
        " · Firefox"
    } else if ua.contains("Safari/") {
        " · Safari"
    } else {
        ""
    };
    format!("{hardware}{browser}")
}

async fn log_event(
    pool: &PgPool,
    device_id: Option<&str>,
    event_type: &str,
    detail: Value,
    ip: Option<String>,
    user_agent: Option<String>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO app_auth_event \
         (user_id, device_id, event_type, detail, ip, user_agent) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(OWNER_USER_ID)
    .bind(device_id)
    .bind(event_type)
    .bind(&detail)
    .bind(ip)
    .bind(user_agent)
    .execute(pool)
    .await
    .map(|_| ())
}

#[cfg(test)]
mod source_resolution_tests {
    use super::resolve_source_id;

    // The sources catalog is compiled in (`include_str!(sources.toml)`), so
    // `lookup_source` resolves real ids here without a DB.

    #[test]
    fn mobile_app_defaults_to_ios() {
        assert_eq!(resolve_source_id("mobile_app", None).unwrap(), "ios");
        // explicit ios works too
        assert_eq!(resolve_source_id("mobile_app", Some("ios")).unwrap(), "ios");
    }

    #[test]
    fn desktop_app_without_source_is_sentinel_no_fanout() {
        // The WG desktop daemon pairs as desktop_app with NO source — it must
        // get "__device__" so mac_activity never fans out to it.
        assert_eq!(resolve_source_id("desktop_app", None).unwrap(), "__device__");
    }

    #[test]
    fn collector_declares_explicit_source() {
        // mac-source sends source="mac" → its credential matches the
        // mac_activity template's source and fans out.
        assert_eq!(resolve_source_id("desktop_app", Some("mac")).unwrap(), "mac");
    }

    #[test]
    fn sensor_without_source_is_sentinel() {
        assert_eq!(resolve_source_id("sensor", None).unwrap(), "__device__");
    }

    #[test]
    fn invalid_source_is_rejected_not_silently_downgraded() {
        // A typo must be a loud 400 (Err), not a silent no-fan-out.
        assert!(resolve_source_id("desktop_app", Some("Mac")).is_err()); // wrong case
        assert!(resolve_source_id("desktop_app", Some("nope")).is_err());
    }

    #[test]
    fn blank_source_falls_through_to_kind_default() {
        assert_eq!(resolve_source_id("desktop_app", Some("   ")).unwrap(), "__device__");
        assert_eq!(resolve_source_id("mobile_app", Some("")).unwrap(), "ios");
    }
}

//! Pair-only auth bootstrap.
//!
//! One primitive — a short-lived 24-byte random token in `app_pair_token` —
//! grants the right to enroll a device. The token never carries any
//! long-lived secret; the redeeming device generates its own keypair (for
//! WG-capable devices) and submits the pubkey when consuming.
//!
//! Mint paths — one model: an authenticated mint is `authorized`.
//!   - CLI (`virtues pair`): minted on the box itself; trusted by physical
//!     access. Used for first-pair and recovery.
//!   - Web ("+ Add Device" from a paired browser): minted by the
//!     already-authenticated owner, so it's authorized in the same call (same
//!     justification as the collector). The old `pending` → `/confirm`
//!     round-trip was friction with no security gain for a QR the owner scans
//!     on their own screen, so it was removed. Cancel an outstanding token via
//!     `POST /api/pair/deny/:id` (the modal fires it on close).
//!
//! Consume path (`POST /api/pair/consume`)
//!   - Accepts `{token, kind, label, device_info, device_node_id?}`.
//!   - For `kind = mobile_app | desktop_app | sensor | cli`: creates a device
//!     row (recording the device's iroh `node_id` for the allowlist) + a
//!     `credentials` row with a server-issued bearer (HMAC-lookup, encrypted at
//!     rest). Reach is over iroh — no WG bundle.
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
use chrono::{Duration, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::middleware::auth::AuthUser;
use crate::middleware::{client_ip, is_secure_environment, rate_limit_ip, OWNER_USER_ID};

// ─── Constants ──────────────────────────────────────────────────────────────

/// Window during which an `authorized` token can be consumed.
const AUTHORIZED_REDEEM_TTL_MIN: i64 = 5;

/// CLI-minted tokens get a longer window — they're typed into the desktop app,
/// and the user may need a few minutes to download it the first time.
const CLI_REDEEM_TTL_MIN: i64 = 30;

/// Claim deadline for a desktop-relayed `provision` credential. Unlike the
/// consume path (the credential is minted only when the device redeems a
/// token), provision mints the credential live *before* the new device scans
/// the QR — so an unclaimed one carries this deadline and lapses if the device
/// never comes online. Generous vs the QR's on-screen TTL (~2 min) so a code
/// scanned at the last second still completes its tunnel bring-up + first call.
/// Cleared to NULL on first authenticated use (`credentials::update_last_seen`).
const PROVISION_CLAIM_TTL_MIN: i64 = 15;

// ─── Token helpers ──────────────────────────────────────────────────────────

/// Generate a short human-typeable pair code: 6 digits. Digits (not letters)
/// because the primary surface is someone reading a code off a screen and
/// typing it on a numeric pad — far less error-prone than a 24-letter
/// alphabet. 10^6 = 1M combinations: smaller than the old letter space, so the
/// /api/pair/consume per-IP rate limit is load-bearing, especially for the
/// multi-use standing code. It holds because the standing code also ROTATES
/// (~every 20 min, see `STANDING_TTL_MIN`): 10 guesses/30-min/IP against a
/// target that changes every 20 min makes enumeration of a 1M space
/// infeasible. (A box-wide lockout was considered and rejected — it hands an
/// attacker a trivial DoS against the real owner.) Displayed as "123 456".
fn random_pair_code() -> String {
    const DIGITS: &[u8] = b"0123456789";
    let mut code = String::with_capacity(6);
    let mut buf = [0u8; 1];
    let mut rng = rand::rng();
    while code.len() < 6 {
        rng.fill_bytes(&mut buf);
        // 250 = 10 * 25; reject 250–255 so every digit maps with equal probability.
        if buf[0] < 250 {
            code.push(DIGITS[(buf[0] % 10) as usize] as char);
        }
    }
    code
}

fn random_32_bearer() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// A device-link code: 10 chars from an unambiguous alphabet (~50 bits) —
/// human-typeable but far stronger than the 6-digit pair code, since a link
/// code's hash sits on atlas and a weak code would be offline-brute-forceable.
/// Grouped `XXXXX-XXXXX` for entry. See docs/reach-enrollment.md.
pub(crate) fn random_link_code() -> String {
    const ALPHABET: &[u8] = b"23456789ABCDEFGHJKLMNPQRSTVWXYZ"; // no 0/1/O/I/U
    let n = ALPHABET.len() as u8; // 31
    let cutoff = 256 - (256 % ALPHABET.len()); // reject bias
    let mut out = String::with_capacity(11);
    let mut buf = [0u8; 1];
    let mut rng = rand::rng();
    while out.chars().filter(|c| *c != '-').count() < 10 {
        rng.fill_bytes(&mut buf);
        if (buf[0] as usize) < cutoff {
            out.push(ALPHABET[(buf[0] % n) as usize] as char);
            if out.chars().filter(|c| *c != '-').count() == 5 && !out.contains('-') {
                out.push('-');
            }
        }
    }
    out
}

pub(crate) fn hash_token(token: &str) -> String {
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

    // One model: an authenticated mint is authorized. The CLI mint is trusted by
    // physical access to the box; a web mint is trusted because the caller is the
    // already-authenticated owner (same justification as mint-collector). The old
    // web `pending` → `/confirm` round-trip added friction with no security gain
    // for a QR the owner scans on their own screen, so it's collapsed away.
    let (status, minted_via, ttl_min) = match minted_by_device {
        None => ("authorized", "cli", CLI_REDEEM_TTL_MIN),
        Some(_) => ("authorized", "web", AUTHORIZED_REDEEM_TTL_MIN),
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
    /// Human-readable display of the pair code, grouped as "123 456".
    /// The raw token (no space) is used in URL fragments and API calls;
    /// the display form is what the user sees in the CLI and types in the app.
    pub fn display_code(&self) -> String {
        group_code(&self.token)
    }
}

/// Format a 6-char code as "123 456" for display (CLI / panel). The raw,
/// ungrouped code is what's typed and matched.
pub fn group_code(code: &str) -> String {
    if code.len() == 6 {
        format!("{} {}", &code[..3], &code[3..])
    } else {
        code.to_string()
    }
}

// ─── Standing rotating code (the universal box code) ─────────────────────────
//
// One code pairs everything (phone setup, desktop app, CLI). Unlike a one-off
// token it is MULTI-use within its window and rotated on a timer (see
// maintenance::pair_rotator) with an overlap window. The raw value is stored
// encrypted (display_secret) so box-local surfaces can DISPLAY it; it is never
// served over the LAN.

/// How often the rotator mints a fresh standing code.
pub const STANDING_ROTATE_INTERVAL_MIN: i64 = 15;
/// A rotated-out code stays valid this long after a newer one appears, so a
/// code read mid-rotation never dies under the user.
pub const STANDING_GRACE_MIN: i64 = 5;
/// Total validity of a standing code = interval + grace (~20 min).
const STANDING_TTL_MIN: i64 = STANDING_ROTATE_INTERVAL_MIN + STANDING_GRACE_MIN;

/// Mint a fresh standing code. Stores SHA-256(code) for matching AND the raw
/// code encrypted (for box-local display). Multi-use; expires by time. Returns
/// the row as a `MintedToken` (raw code in `.token`).
pub async fn mint_standing_code(pool: &PgPool) -> crate::Result<MintedToken> {
    let token = random_pair_code();
    let token_hash = hash_token(&token);
    let id = crate::ids::generate_id(crate::ids::PAIR_TOKEN_PREFIX, &[&token_hash[..16]]);
    let display_secret = {
        let enc = crate::crypto::TokenEncryptor::from_env()
            .map_err(|e| crate::Error::Other(format!("encryptor: {e}")))?;
        enc.encrypt(&token)
            .map_err(|e| crate::Error::Other(format!("encrypt standing code: {e}")))?
    };
    let expires_at = Utc::now() + Duration::minutes(STANDING_TTL_MIN);
    sqlx::query(
        "INSERT INTO app_pair_token \
         (id, token_hash, minted_via, status, kind, display_secret, authorized_at, expires_at) \
         VALUES ($1, $2, 'cli', 'authorized', 'standing', $3, now(), $4)",
    )
    .bind(&id)
    .bind(&token_hash)
    .bind(&display_secret)
    .bind(expires_at)
    .execute(pool)
    .await
    .map_err(|e| crate::Error::Database(format!("insert standing code: {e}")))?;
    Ok(MintedToken {
        id,
        token,
        expires_at,
        status: "authorized".to_string(),
    })
}

/// The current valid standing code as a `MintedToken` (raw code decrypted), if
/// one exists. BOX-LOCAL ONLY — never expose the raw code over the network.
pub async fn current_standing(pool: &PgPool) -> crate::Result<Option<MintedToken>> {
    let row: Option<(String, String, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, display_secret, expires_at FROM app_pair_token \
         WHERE kind = 'standing' AND status = 'authorized' AND expires_at > now() \
           AND display_secret IS NOT NULL \
         ORDER BY expires_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| crate::Error::Database(format!("read standing code: {e}")))?;
    match row {
        Some((id, ciphertext, expires_at)) => {
            let enc = crate::crypto::TokenEncryptor::from_env()
                .map_err(|e| crate::Error::Other(format!("encryptor: {e}")))?;
            let token = enc
                .decrypt(&ciphertext)
                .map_err(|e| crate::Error::Other(format!("decrypt standing code: {e}")))?;
            Ok(Some(MintedToken {
                id,
                token,
                expires_at,
                status: "authorized".to_string(),
            }))
        }
        None => Ok(None),
    }
}

/// Return the current standing code (minting one if none is valid). Used by the
/// CLI and at rotator startup so a fresh box always has a code to show.
/// (Expired standing rows are pruned by `maintenance::sweeper`.)
pub async fn ensure_standing(pool: &PgPool) -> crate::Result<MintedToken> {
    if let Some(m) = current_standing(pool).await? {
        return Ok(m);
    }
    mint_standing_code(pool).await
}

/// Thin wrapper: the current standing code as a raw string, minting if needed.
/// For surfaces that only need the digits (e.g. the panel render).
pub async fn ensure_standing_code(pool: &PgPool) -> crate::Result<String> {
    Ok(ensure_standing(pool).await?.token)
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
/// device. The token is minted `authorized` (the caller is the authenticated
/// owner), so the QR is immediately redeemable; closing the modal cancels it
/// via `/api/pair/deny/:id`.
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

    // No SPKI fingerprint in the relay model — device trust is the relay/box
    // cert (or app pinning), not a WG server key.
    let fpr: Option<String> = None;
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
/// `source="mac"` to receive its `mac_ingest` action fan-out.
pub async fn mint_collector_handler(
    State(pool): State<PgPool>,
    user: AuthUser,
) -> impl IntoResponse {
    // `mint_pair_token` now mints authenticated tokens as `authorized` directly
    // (one model — see its body), so the collector token is immediately
    // redeemable with no separate self-authorize step.
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

#[derive(Debug, Serialize)]
pub struct ProvisionStatusResponse {
    /// True once the provisioned device's tunnel is live — it has made an
    /// authenticated call, bumping `last_seen_at` past `paired_at`.
    pub online: bool,
}

/// `GET /api/pair/provision-status/:device_id` — auth'd. Polled by the relay UI
/// after `provision` to know when the new device's tunnel is ACTUALLY live (the
/// phone scanned the bundle QR + brought up WG), not merely when the box
/// accepted the provision. The signal is `last_seen_at > paired_at` (with a
/// small guard against the provision-time `now()` jitter); `last_seen_at` is
/// touched on every authenticated request.
pub async fn provision_status_handler(
    State(pool): State<PgPool>,
    _user: AuthUser,
    Path(device_id): Path<String>,
) -> impl IntoResponse {
    let row: Option<(bool,)> = sqlx::query_as(
        "SELECT (last_seen_at IS NOT NULL \
                 AND last_seen_at > paired_at + interval '5 seconds') \
         FROM app_device WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(&device_id)
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten();
    match row {
        Some((online,)) => {
            (StatusCode::OK, Json(ProvisionStatusResponse { online })).into_response()
        }
        None => (StatusCode::NOT_FOUND, Json(json!({"error": "not_found"}))).into_response(),
    }
}

/// `POST /api/pair/deny/:id` — auth'd. The minting device cancels an
/// outstanding (unconsumed) pair token — e.g. the user closes the Add-Device
/// modal before a device scanned the QR. Tokens are minted `authorized` now
/// (the `pending`/`confirm` step was collapsed away), so this cancels an
/// `authorized` token; `pending` is kept in the filter for forward-safety. A
/// no-op once the token has been consumed/expired.
pub async fn deny_handler(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let _ = sqlx::query(
        "UPDATE app_pair_token \
         SET status = 'denied' \
         WHERE id = $1 AND minted_by_device = $2 \
           AND status IN ('pending', 'authorized')",
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
pub(crate) fn resolve_source_id(kind: &str, source: Option<&str>) -> Result<String, ()> {
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
    /// The data source this collector represents (`"mac"`, `"ios"`, …, from
    /// `actions/sources.toml`). REQUIRED for a collector to receive its
    /// per-credential action fan-out — the credential's `source_id` is set
    /// from this so `reconcile_templates` matches the source's webhook
    /// templates. `kind="desktop_app"` is ambiguous (the WG daemon AND
    /// mac-source both use it), so collectors MUST declare `source` explicitly;
    /// `mobile_app` defaults to `"ios"`. Absent/`"__device__"` → no fan-out
    /// (correct for the WG desktop daemon, which is not a collector).
    pub source: Option<String>,
    /// The enrolling device's own iroh **EndpointId** (hex). The device
    /// generates its keypair locally and submits its EndpointId here; the box
    /// records it on `app_device` and allowlists it on its iroh transport so the
    /// device can dial the box by the box's EndpointId. Absent for the box's own
    /// browser / non-iroh clients.
    pub device_node_id: Option<String>,
    /// Client-generated idempotency key (persisted per pairing attempt). If a
    /// consume response is lost and the client retries with the same key, the box
    /// re-returns the SAME bearer instead of failing on the already-consumed
    /// token. Non-browser only. Absent → no idempotency (legacy behavior).
    pub idempotency_key: Option<String>,
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
    /// The box's iroh **EndpointId** (hex) — the device dials this to reach the
    /// box. Present once the box's iroh endpoint is up; `None` otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub box_node_id: Option<String>,
    /// The relay URL to reach the box's EndpointId through (`https://relay…`).
    /// Paired with `box_node_id` as the reach ticket; `None` on a dev/LAN box.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay_url: Option<String>,
}

/// The box's iroh reach ticket: `(EndpointId, relay_url)`. A device dials the
/// box's EndpointId through the relay (then upgrades to hole-punched direct).
/// `None` until the box's iroh endpoint is up; the client can pick it up later
/// from `box/status` or `GET /api/devices/self/reach`.
pub(crate) fn box_reach() -> Option<(String, String)> {
    if !crate::relay::is_relay_registered() {
        return None;
    }
    let node_id = crate::relay::box_endpoint_id()?;
    let relay_url = crate::relay::box_relay_url()?;
    Some((node_id, relay_url))
}

/// `POST /api/pair/consume` — anonymous, but valid token required.
pub async fn consume_handler(
    State(pool): State<PgPool>,
    headers: axum::http::HeaderMap,
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
        "mobile_app" | "desktop_app" | "sensor" | "cli" => body.kind.as_str(),
        _ => {
            return (StatusCode::BAD_REQUEST, Json(json!({"error": "invalid_kind"})))
                .into_response()
        }
    };

    // Idempotency replay: if this key already produced a bearer (a prior consume
    // whose response the client lost), re-return the SAME result without touching
    // the (now-consumed) token.
    let idem_key = body
        .idempotency_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(key) = idem_key {
        if let Some(resp) = replay_consume_idem(&pool, key).await {
            return resp;
        }
    }

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

    // Pre-encrypt the bearer so a slow KMS call doesn't hold the DB transaction
    // open. (This bearer serves programmatic/webhook callers; interactive
    // clients authenticate over iroh by their allowlisted key.)
    let bearer_pack = match build_bearer_pack(kind, &label, &body.device_info) {
        Ok(p) => p,
        Err(e) => return e.into_response(),
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

    // Atomic claim with kind-aware consumption:
    //   • `matched` locks the valid token row (FOR UPDATE) so concurrent
    //     redeems of the same one-off token serialize — the second sees no
    //     'authorized' row after the first commits.
    //   • `consumed` marks it 'consumed' ONLY for 'oneoff' tokens (single-use).
    //     'standing' codes are multi-use within their window, so they are
    //     validated but NOT consumed — they pair many devices over their life.
    // We RETURN the id from `matched` either way (a standing match is still a
    // successful pair). `consumed_by_device` is back-filled after the device
    // INSERT below (the FK would reject it here). On error we surface the DB
    // message so a real bug doesn't masquerade as `invalid_or_expired_token`.
    let token_id = match claim_pair_token(&mut tx, &token_hash).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "invalid_or_expired_token"})),
            )
                .into_response();
        }
        Err(e) => {
            tracing::warn!("pair consume: token claim failed: {e:#}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal"})),
            )
                .into_response();
        }
    };

    if let Err(e) = insert_device_row(
        &mut tx,
        &device_id,
        kind,
        &label,
        &device_info,
        ip.as_deref(),
        body.device_node_id.as_deref(),
    )
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

    // No claim deadline on the consume path — the credential is minted only
    // when the device itself redeems the token, so it's permanent.
    if let Err(e) =
        insert_credential_row(&mut tx, &bearer_pack, &source_id, &label, &device_id, None).await
    {
        tracing::warn!("pair consume: credential insert failed: {e:#}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "credential_insert_failed"})),
        )
            .into_response();
    }

    if let Err(e) = tx.commit().await {
        tracing::warn!("pair consume: tx commit failed: {e:#}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "internal"})),
        )
            .into_response();
    }

    // Post-commit cross-check for home_timezone (the box's location). The box
    // normally seeds this from its own system clock, but a datacenter box reads
    // "UTC", which is wrong — so when the current value is unset or UTC, fall back
    // to the pairing device's reported zone. A real appliance configured at home
    // keeps its server-detected zone. See docs/timezone-model.md.
    if let Some(dev_tz) = device_info
        .get("timezone")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "UTC")
    {
        // Seed home_timezone from the box's own system clock first — so a
        // correctly-configured appliance wins over a (possibly traveling) pairing
        // device; only a UTC/unset result defers to the device.
        let _ = crate::api::profile::ensure_home_timezone(&pool).await;
        let current = crate::api::profile::get_timezone(&pool)
            .await
            .ok()
            .flatten();
        if current.as_deref().map(|c| c == "UTC").unwrap_or(true) {
            let _ = crate::api::profile::update_profile(
                &pool,
                crate::api::profile::UpdateProfileRequest {
                    home_timezone: Some(dev_tz.to_string()),
                    ..Default::default()
                },
            )
            .await;
        }
    }

    // Post-commit: log the pairing event (best-effort) and assemble the bearer
    // response.
    let _ = log_event(
        &pool,
        Some(&device_id),
        "paired",
        json!({
            "kind": kind,
            "label": &label,
            "token_id": &token_id,
        }),
        ip,
        user_agent,
    )
    .await;

    // Assemble the per-device action fan-out so the device knows which
    // `app_actions.id` to POST each stream flush to. Post-commit best-effort: a
    // failure here doesn't undo the pairing — the device shows up paired but with
    // no per-credential actions until a `/api/devices/<id>/reconcile` retry.
    let bp = bearer_pack;

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

    // Newly-paired device carries its iroh EndpointId → allowlist it now.
    crate::relay::after_pairing_change(pool.clone());

    // Record for idempotent replay (best-effort) so a lost response is recoverable.
    if let Some(key) = idem_key {
        store_consume_idem(&pool, key, &device_id, &bp, &action_ids).await;
    }

    // Compute the reach ticket once so both halves are consistent.
    let (box_node_id, relay_url) = match box_reach() {
        Some((n, r)) => (Some(n), Some(r)),
        None => (None, None),
    };
    (
        StatusCode::OK,
        Json(ConsumeResponse {
            device_id,
            credential_id: Some(bp.credential_id),
            redirect: "/".to_string(),
            bearer: Some(bp.bearer),
            action_ids,
            box_node_id,
            relay_url,
        }),
    )
        .into_response()
}

// ─── Link redeem (fully-remote enrollment — the new device pulls its bearer) ─

#[derive(Debug, Deserialize)]
pub struct LinkRedeemRequest {
    pub code: String,
}

/// `POST /api/pair/link-redeem` — the new device, now allowlisted (its EndpointId
/// was approved by a voucher via `link/approve`), dials the box over iroh and
/// redeems the one-time linking code for the bearer stashed at approve.
/// Anonymous (bearer-less): gated by the code over the already-allowlisted +
/// encrypted iroh channel, and one-time (the row flips to `redeemed`). The bearer
/// never transited atlas. See docs/reach-enrollment.md.
pub async fn link_redeem_handler(
    State(pool): State<PgPool>,
    Json(body): Json<LinkRedeemRequest>,
) -> axum::response::Response {
    let code = body.code.trim();
    if code.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "missing_code"}))).into_response();
    }
    let code_hash = hash_token(code);
    // Atomically claim: approved + unexpired → redeemed (one-time).
    let row: Option<(Option<String>, Option<String>, Value)> = sqlx::query_as(
        "UPDATE app_link_session SET status = 'redeemed' \
         WHERE code_hash = $1 AND status = 'approved' AND expires_at > now() \
         RETURNING bearer_ciphertext, credential_id, action_ids",
    )
    .bind(&code_hash)
    .fetch_optional(&pool)
    .await
    .unwrap_or(None);
    let (ciphertext, credential_id, action_ids_json) = match row {
        Some((Some(ct), cred, ids)) => (ct, cred, ids),
        _ => {
            return (StatusCode::NOT_FOUND, Json(json!({"error": "not_approved_or_expired"}))).into_response();
        }
    };
    let bearer = match crate::crypto::TokenEncryptor::from_env()
        .ok()
        .and_then(|enc| enc.decrypt(&ciphertext).ok())
        .and_then(|pt| serde_json::from_str::<Value>(&pt).ok())
        .and_then(|v| v.get("token").and_then(|t| t.as_str()).map(String::from))
    {
        Some(b) => b,
        None => {
            tracing::warn!("link_redeem: bearer decrypt failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal"}))).into_response();
        }
    };
    let action_ids: std::collections::HashMap<String, String> =
        serde_json::from_value(action_ids_json).unwrap_or_default();
    let (box_node_id, relay_url) = match box_reach() {
        Some((n, r)) => (Some(n), Some(r)),
        None => (None, None),
    };
    (
        StatusCode::OK,
        Json(json!({
            "bearer": bearer,
            "credential_id": credential_id,
            "action_ids": action_ids,
            "box_node_id": box_node_id,
            "relay_url": relay_url,
        })),
    )
        .into_response()
}

// ─── Provision (desktop-relayed off-LAN pairing) ───────────────────────────

#[derive(Debug, Deserialize)]
pub struct ProvisionRequest {
    /// Defaults to `mobile_app` (the iOS off-LAN case this exists for).
    pub kind: Option<String>,
    pub label: Option<String>,
    pub device_info: Option<Value>,
    /// The data source this device represents (see [`ConsumeRequest::source`]).
    /// Absent + `mobile_app` → `"ios"`.
    pub source: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProvisionResponse {
    pub device_id: String,
    pub credential_id: String,
    pub bearer: String,
    #[serde(skip_serializing_if = "std::collections::HashMap::is_empty", default)]
    pub action_ids: std::collections::HashMap<String, String>,
    /// SVG QR for the new device to scan (carries the box reach ticket + bearer).
    /// Empty when the box isn't reachable yet.
    pub qr_svg: String,
    /// The box's iroh reach ticket, so the provisioned device can dial it off-LAN.
    /// `None` when the box's iroh endpoint isn't up yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub box_node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay_url: Option<String>,
}

/// `POST /api/pair/provision` — AUTHENTICATED. An already-paired device (reached
/// here over its WG tunnel, e.g. the desktop relay) provisions a brand-new
/// device on its behalf and gets back a COMPLETE bundle to hand off out-of-band
/// (one QR, Mac → phone). No pair token: the caller is already a trusted owner
/// device, so the mint→consume token dance (which exists to authorize an
/// *untrusted* device) is unnecessary. The box generates the new device's WG
/// keypair (returned once in the bundle, never persisted) so the new device
/// never has to speak to the box first — see `pairing::assemble_bundle_generated`.
pub async fn provision_handler(
    State(pool): State<PgPool>,
    user: AuthUser,
    headers: axum::http::HeaderMap,
    Json(body): Json<ProvisionRequest>,
) -> axum::response::Response {
    let kind = match body.kind.as_deref().unwrap_or("mobile_app") {
        // `browser` is excluded: bare browsers can't pair (no session path).
        k @ ("mobile_app" | "desktop_app" | "sensor") => k,
        _ => {
            return (StatusCode::BAD_REQUEST, Json(json!({"error": "invalid_kind"})))
                .into_response()
        }
    };

    let source_id = match resolve_source_id(kind, body.source.as_deref()) {
        Ok(s) => s,
        Err(()) => {
            return (StatusCode::BAD_REQUEST, Json(json!({"error": "invalid_source"})))
                .into_response()
        }
    };

    let ip = client_ip(&headers);
    let device_id = crate::ids::generate_id(
        crate::ids::DEVICE_PREFIX,
        &[&user.device_id, &Utc::now().to_rfc3339()],
    );
    let label = body
        .label
        .as_deref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_label_for(kind, None, &body.device_info));
    let device_info = body.device_info.clone().unwrap_or_else(|| json!({}));

    // The box generates the device's iroh keypair (relay path); its EndpointId
    // is recorded for the allowlist by the bundle assembly below.
    let bp = match build_bearer_pack(kind, &label, &body.device_info) {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };

    let mut tx = match pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("pair provision: tx begin failed: {e:#}");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal"})))
                .into_response();
        }
    };
    // node_id is None here: the provisioned device isn't present to submit its
    // EndpointId; it reports it on first authenticated contact (follow-up).
    if let Err(e) = insert_device_row(&mut tx, &device_id, kind, &label, &device_info, ip.as_deref(), None).await {
        tracing::warn!("pair provision: device insert failed: {e:#}");
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "device_insert_failed"})))
            .into_response();
    }
    // Live-minted before the new device scans the QR → carry a claim deadline
    // so an unclaimed credential (abandoned QR, browser closed) lapses on its
    // own. Cleared to NULL on the device's first authenticated call.
    let claim_deadline = Utc::now() + Duration::minutes(PROVISION_CLAIM_TTL_MIN);
    if let Err(e) = insert_credential_row(&mut tx, &bp, &source_id, &label, &device_id, Some(claim_deadline)).await {
        tracing::warn!("pair provision: credential insert failed: {e:#}");
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "credential_insert_failed"})))
            .into_response();
    }
    if let Err(e) = tx.commit().await {
        tracing::warn!("pair provision: tx commit failed: {e:#}");
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal"})))
            .into_response();
    }

    let _ = log_event(
        &pool,
        Some(&device_id),
        "provisioned_via_relay",
        json!({
            "kind": kind,
            "label": &label,
            "relayed_by_device": &user.device_id,
            "credential_id": &bp.credential_id,
        }),
        ip,
        None,
    )
    .await;

    let action_ids = match assemble_action_fanout(&pool, &bp.credential_id).await {
        Ok(map) => map,
        Err(e) => {
            tracing::warn!(
                "pair provision: action fanout failed for credential {}: {e:#}; \
                 device provisioned but actions not wired",
                bp.credential_id
            );
            std::collections::HashMap::new()
        }
    };

    // iroh model: the QR hands the phone everything it needs to reach the box
    // off-LAN — the box's iroh reach ticket (EndpointId + relay URL) plus this
    // device's bearer. Only meaningful once the box's iroh endpoint is up;
    // otherwise there's no address to convey, so the QR is left empty. Payload
    // contract (the iOS scanner must match) is in apps/ios/RELAY_MIGRATION.md.
    let reach = box_reach();
    let qr_svg = match &reach {
        Some((node_id, relay_url)) => render_qr_svg(
            &serde_json::json!({
                "v": 2,
                "box_node_id": node_id,
                "relay_url": relay_url,
                "bearer": &bp.bearer,
                "credential_id": &bp.credential_id,
                "device_id": &device_id,
            })
            .to_string(),
        ),
        None => String::new(),
    };

    (
        StatusCode::OK,
        Json(ProvisionResponse {
            device_id,
            credential_id: bp.credential_id,
            bearer: bp.bearer,
            action_ids,
            qr_svg,
            box_node_id: reach.as_ref().map(|(n, _)| n.clone()),
            relay_url: reach.as_ref().map(|(_, r)| r.clone()),
        }),
    )
        .into_response()
}

// ─── Post-commit fan-out ────────────────────────────────────────────────────
//
// Runs AFTER the consume transaction commits — failure logs but doesn't undo
// the pairing. Shaped as its own helper so the consume handler stays the
// easy-to-read top-level flow.

/// Reconcile action templates (so per-credential `app_actions` rows are
/// fanned out) and read back the binary-name → action-id map the device
/// uses to route stream flushes to `POST /webhook/<action_id>`. Lifted
/// out of the legacy `pair_complete_handler` so the unified pair flow
/// produces identical device-side behavior.
pub(crate) async fn assemble_action_fanout(
    pool: &PgPool,
    credential_id: &str,
) -> Result<std::collections::HashMap<String, String>, crate::Error> {
    crate::action_templates::reconcile_templates(pool).await?;
    virtues_helpers::auth::fanout_action_ids(pool, credential_id)
        .await
        .map_err(|e| crate::Error::Other(format!("fanout_action_ids: {e}")))
}

/// Atomically claim a pair token by its hash: locks the valid 'authorized' row
/// `FOR UPDATE` (so concurrent redeems of a one-off serialize), marks a `oneoff`
/// token 'consumed' (a `standing` code is validated but left multi-use), and
/// returns the token id. `Ok(None)` = no valid/unexpired token. Shared by the
/// HTTP consume handler and any other enrollment path.
pub(crate) async fn claim_pair_token(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    token_hash: &str,
) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(String,)> = sqlx::query_as(
        "WITH matched AS ( \
             SELECT id, kind FROM app_pair_token \
             WHERE token_hash = $1 \
               AND status = 'authorized' \
               AND expires_at > now() \
             FOR UPDATE \
         ), \
         consumed AS ( \
             UPDATE app_pair_token t \
             SET status = 'consumed', consumed_at = now() \
             FROM matched m \
             WHERE t.id = m.id AND m.kind = 'oneoff' \
             RETURNING t.id \
         ) \
         SELECT id FROM matched",
    )
    .bind(token_hash)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(|(id,)| id))
}

/// Idempotency replay for consume: if `key` already produced a bearer, decrypt +
/// re-return the identical `ConsumeResponse`. `None` = no prior result (fall
/// through to a normal consume). Never fails the request on its own error.
async fn replay_consume_idem(pool: &PgPool, key: &str) -> Option<axum::response::Response> {
    let row: Option<(String, String, String, Value)> = sqlx::query_as(
        "SELECT device_id, credential_id, bearer_ciphertext, action_ids \
         FROM app_pair_consume_idem WHERE idempotency_key = $1",
    )
    .bind(key)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let (device_id, credential_id, ciphertext, action_ids_json) = row?;

    // Decrypt the stored bearer ({"token": <bearer>}) — same form as credentials.
    let encryptor = crate::crypto::TokenEncryptor::from_env().ok()?;
    let plaintext = encryptor.decrypt(&ciphertext).ok()?;
    let bearer = serde_json::from_str::<Value>(&plaintext)
        .ok()?
        .get("token")?
        .as_str()?
        .to_string();
    let action_ids: std::collections::HashMap<String, String> =
        serde_json::from_value(action_ids_json).unwrap_or_default();
    let (box_node_id, relay_url) = match box_reach() {
        Some((n, r)) => (Some(n), Some(r)),
        None => (None, None),
    };
    Some(
        (
            StatusCode::OK,
            Json(ConsumeResponse {
                device_id,
                credential_id: Some(credential_id),
                redirect: "/".to_string(),
                bearer: Some(bearer),
                action_ids,
                box_node_id,
                relay_url,
            }),
        )
            .into_response(),
    )
}

/// Persist a consume result for idempotent replay (best-effort) + sweep rows
/// older than an hour. The bearer is stored only as ciphertext.
async fn store_consume_idem(
    pool: &PgPool,
    key: &str,
    device_id: &str,
    bp: &BearerPack,
    action_ids: &std::collections::HashMap<String, String>,
) {
    let action_ids_json = serde_json::to_value(action_ids).unwrap_or_else(|_| json!({}));
    if let Err(e) = sqlx::query(
        "INSERT INTO app_pair_consume_idem \
         (idempotency_key, device_id, credential_id, bearer_ciphertext, action_ids) \
         VALUES ($1, $2, $3, $4, $5) ON CONFLICT (idempotency_key) DO NOTHING",
    )
    .bind(key)
    .bind(device_id)
    .bind(&bp.credential_id)
    .bind(&bp.ciphertext)
    .bind(&action_ids_json)
    .execute(pool)
    .await
    {
        tracing::warn!("pair consume: idempotency store failed: {e:#}");
    }
    // Opportunistic sweep — these are only needed for a brief retry window.
    let _ = sqlx::query("DELETE FROM app_pair_consume_idem WHERE created_at < now() - interval '1 hour'")
        .execute(pool)
        .await;
}

/// Insert the `app_device` row for a freshly-paired/provisioned device. Shared
/// by `consume_handler`, `provision_handler`, and `enroll_peer`.
pub(crate) async fn insert_device_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    device_id: &str,
    kind: &str,
    label: &str,
    device_info: &Value,
    ip: Option<&str>,
    node_id: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO app_device \
         (id, user_id, kind, label, device_info, paired_from_ip, node_id, last_seen_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, now())",
    )
    .bind(device_id)
    .bind(OWNER_USER_ID)
    .bind(kind)
    .bind(label)
    .bind(device_info)
    .bind(ip)
    .bind(node_id)
    .execute(&mut **tx)
    .await
    .map(|_| ())
}

/// Insert the `credentials` row (encrypted bearer) for a non-browser device.
/// Shared by `consume_handler` and `provision_handler`.
///
/// `expires_at` is the claim deadline: `None` for the consume path (the
/// credential is minted at claim time and is permanent), `Some(deadline)` for
/// the provision path (minted live before the device scans, so it must lapse if
/// never claimed — see `credentials::validate_device_token` / `update_last_seen`).
pub(crate) async fn insert_credential_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    bp: &BearerPack,
    source_id: &str,
    label: &str,
    device_id: &str,
    expires_at: Option<chrono::DateTime<Utc>>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO credentials \
         (id, source_id, name, device_id, status, secrets_ciphertext, \
          secret_lookup_hash, metadata, last_seen_at, expires_at) \
         VALUES ($1, $2, $3, $4, 'active', $5, $6, $7, now(), $8)",
    )
    .bind(&bp.credential_id)
    .bind(source_id)
    .bind(label)
    .bind(device_id)
    .bind(&bp.ciphertext)
    .bind(&bp.lookup_hash)
    .bind(&bp.metadata)
    .bind(expires_at)
    .execute(&mut **tx)
    .await
    .map(|_| ())
}

// ─── Bearer pack builder (extracted from consume_handler for tx hygiene) ────

pub(crate) struct BearerPack {
    pub(crate) credential_id: String,
    pub(crate) bearer: String,
    pub(crate) ciphertext: String,
    pub(crate) lookup_hash: String,
    pub(crate) metadata: Value,
}

/// Failure modes from the bearer-pack builder. Kept domain-flavored (no
/// HTTP types) so the helper stays testable; the caller maps to a response
/// at the boundary.
#[derive(Debug)]
pub(crate) enum BearerPackError {
    EncryptionUnavailable,
    EncryptionFailed,
    LookupHashFailed,
}

impl BearerPackError {
    pub(crate) fn into_response(self) -> axum::response::Response {
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
pub(crate) fn build_bearer_pack(
    kind: &str,
    label: &str,
    device_info: &Option<Value>,
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
            // canonical port. Use the box's LAN address (not `localhost`) — this
            // URL is rendered into the QR a *phone* scans, and `localhost` on a
            // phone points at the phone itself. `forward_host()` is the same
            // LAN-IP-preferring helper the `virtues pair` CLI QR uses (it falls
            // back to the mDNS name only when no address is discoverable). If
            // your network reaches the box at a different hostname, set
            // VIRTUES_PUBLIC_URL in /etc/virtues/env.
            format!(
                "http://{}:{}",
                crate::cli::link::forward_host(),
                crate::cli::link::INTERNAL_PORT
            )
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
    // Bare browsers can't pair (no session cookie path anymore); kinds are
    // app/sensor/cli only.
    let _ = ua;
    match kind {
        "mobile_app" => "Mobile app".to_string(),
        "desktop_app" => "Desktop app".to_string(),
        "sensor" => "Sensor".to_string(),
        _ => "Device".to_string(),
    }
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
        // get "__device__" so mac_ingest never fans out to it.
        assert_eq!(resolve_source_id("desktop_app", None).unwrap(), "__device__");
    }

    #[test]
    fn collector_declares_explicit_source() {
        // mac-source sends source="mac" → its credential matches the
        // mac_ingest template's source and fans out.
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

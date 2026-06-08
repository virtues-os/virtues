//! Blind rendezvous (WS-2) — `publish_id → opaque ciphertext`.
//!
//! The ONLY Virtues-cloud touchpoint in the otherwise-direct WireGuard remote
//! access path. It lets a paired phone relearn the home box's current public
//! endpoint after the ISP rotates the prefix — without virtues-api ever being
//! able to read that endpoint or tie it to anyone.
//!
//! What it holds: `publish_id` (an opaque, unguessable 128-bit capability) → an
//! opaque ciphertext blob the box encrypted under a per-box key `K` that lives
//! ONLY on the box + its paired devices. virtues-api never holds K, so the
//! stored value is meaningless here. There is NO customer column, NO bearer
//! column, NO join key (Lint-10 stays green).
//!
//! Asymmetric auth, by design:
//!   - PUT (write) is gated by the anonymous entitlement bearer — only a live
//!     paying box may publish. The bearer is verified and **discarded**, never
//!     stored beside the publish_id. A box knows only its own publish_id, so it
//!     can only ever overwrite its own row.
//!   - GET (read) is gated only by possession of the `publish_id`. The phone
//!     has no bearer; requiring one would force a new per-device token = a join
//!     key. Capability-as-URL is the read auth; 128 random bits make it
//!     unguessable, so enumeration is infeasible.
//!
//! Deliberately a self-contained "fourth party": its own table, no FK to
//! entitlements, verify-and-discard. Runs in-process today, built to extract
//! into its own service (+ Tor-fronting) later — see
//! `docs/wireguard-pairing.md` §6.

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::put,
    Json, Router,
};
use serde_json::json;
use std::sync::Arc;

use crate::bearer_auth::BearerAuth;
use crate::AppState;

/// Max ciphertext accepted on PUT. An endpoint blob (`{v,ip,port,wg_pub,ts}`)
/// is ~340 plaintext bytes; with the 12-byte nonce + 16-byte GCM tag it stays
/// well under 1 KiB. Anything larger is malformed/abuse → 413.
const MAX_BLOB_BYTES: usize = 1024;

/// How long a published endpoint stays resolvable before the sweeper reaps it.
/// Refreshed on every PUT, so a live box never expires; a dead one ages out.
const RENDEZVOUS_TTL_DAYS: i64 = 30;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route(
        "/v1/rendezvous/:publish_id",
        put(put_rendezvous).get(get_rendezvous),
    )
}

fn err(status: StatusCode, code: &str, message: &str) -> axum::response::Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": message } })),
    )
        .into_response()
}

/// `publish_id` is a base64url capability (16 random bytes → 22 chars). Bound
/// the length and charset so a junk key can't bloat the PK or the URL.
fn valid_publish_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// PUT /v1/rendezvous/:publish_id — bearer-authed write of the opaque blob.
/// Body = raw ciphertext bytes. The bearer proves "a live entitlement made
/// this"; it is NOT metered and NOT stored. UPSERT, refreshing the TTL.
async fn put_rendezvous(
    State(state): State<Arc<AppState>>,
    _auth: BearerAuth,
    Path(publish_id): Path<String>,
    body: Bytes,
) -> axum::response::Response {
    let Some(pool) = state.db.as_ref() else {
        return err(
            StatusCode::SERVICE_UNAVAILABLE,
            "db_unavailable",
            "database not configured",
        );
    };
    if !valid_publish_id(&publish_id) {
        return err(
            StatusCode::BAD_REQUEST,
            "invalid_publish_id",
            "publish_id must be base64url, <= 64 chars",
        );
    }
    if body.is_empty() {
        return err(
            StatusCode::BAD_REQUEST,
            "empty_body",
            "ciphertext body required",
        );
    }
    if body.len() > MAX_BLOB_BYTES {
        return err(
            StatusCode::PAYLOAD_TOO_LARGE,
            "blob_too_large",
            "ciphertext exceeds limit",
        );
    }

    let expires_at = chrono::Utc::now() + chrono::Duration::days(RENDEZVOUS_TTL_DAYS);
    let res = sqlx::query(
        "INSERT INTO rendezvous (publish_id, ciphertext, updated_at, expires_at)
         VALUES ($1, $2, now(), $3)
         ON CONFLICT (publish_id) DO UPDATE
           SET ciphertext = EXCLUDED.ciphertext,
               updated_at = now(),
               expires_at = EXCLUDED.expires_at",
    )
    .bind(&publish_id)
    .bind(body.as_ref())
    .bind(expires_at)
    .execute(pool)
    .await;

    match res {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::warn!("rendezvous put failed: {e:#}");
            err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "write failed")
        }
    }
}

/// GET /v1/rendezvous/:publish_id — unauthed capability read. Returns the raw
/// ciphertext bytes (the phone decrypts with K). Missing and expired both
/// return an identical 404 — the only thing leaked is the capability oracle.
async fn get_rendezvous(
    State(state): State<Arc<AppState>>,
    Path(publish_id): Path<String>,
) -> axum::response::Response {
    let Some(pool) = state.db.as_ref() else {
        return err(
            StatusCode::SERVICE_UNAVAILABLE,
            "db_unavailable",
            "database not configured",
        );
    };
    if !valid_publish_id(&publish_id) {
        return err(
            StatusCode::BAD_REQUEST,
            "invalid_publish_id",
            "publish_id must be base64url, <= 64 chars",
        );
    }

    let row: Result<Option<(Vec<u8>,)>, sqlx::Error> = sqlx::query_as(
        "SELECT ciphertext FROM rendezvous WHERE publish_id = $1 AND expires_at > now()",
    )
    .bind(&publish_id)
    .fetch_optional(pool)
    .await;

    match row {
        Ok(Some((ciphertext,))) => (
            StatusCode::OK,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            )],
            ciphertext,
        )
            .into_response(),
        Ok(None) => err(StatusCode::NOT_FOUND, "not_found", "no current endpoint"),
        Err(e) => {
            tracing::warn!("rendezvous get failed: {e:#}");
            err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "read failed")
        }
    }
}

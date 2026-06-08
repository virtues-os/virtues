//! Devices — the unified "+ paired things" surface.
//!
//! Lists every active paired device (browsers, mobile apps, sensors, the CLI)
//! and supports revocation. Revoke is the single operation that ends all of
//! a device's authority in one step:
//!
//!   1. Mark `app_device.revoked_at = now()` — the middleware sees this on the
//!      next request and refuses both cookies and bearers linked to the row.
//!   2. Move any attached `credentials.status` to `'revoked'` and clear the
//!      `secret_lookup_hash` so the bearer can no longer be matched O(1) at
//!      `validate_device_token` time.
//!   3. If the credential metadata carries a `wg_public_key`, call
//!      `virtues_wg::manager::remove_peer(pubkey)` so the tunnel drops on the
//!      kernel side immediately (Linux-only; no-op on the macOS dev host).
//!   4. Append an `app_auth_event` row tagged `revoked`.
//!
//! Steps 1–2 happen in a single transaction; the WG step + event log are
//! best-effort and logged on failure but don't block the revocation.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::middleware::auth::AuthUser;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DeviceListItem {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub paired_at: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub paired_from_ip: Option<String>,
    /// True if this is the device currently making the request.
    pub is_current: bool,
}

/// `GET /api/devices` — list all active paired devices for the current user.
pub async fn list_handler(State(pool): State<PgPool>, user: AuthUser) -> impl IntoResponse {
    let rows: Result<Vec<(String, String, String, DateTime<Utc>, Option<DateTime<Utc>>, Option<String>)>, _> =
        sqlx::query_as(
            "SELECT id, kind, label, paired_at, last_seen_at, paired_from_ip \
             FROM app_device \
             WHERE user_id = $1 AND revoked_at IS NULL \
             ORDER BY last_seen_at DESC NULLS LAST, paired_at DESC",
        )
        .bind(&user.id)
        .fetch_all(&pool)
        .await;

    match rows {
        Ok(rows) => {
            let items: Vec<DeviceListItem> = rows
                .into_iter()
                .map(|(id, kind, label, paired_at, last_seen_at, ip)| {
                    let is_current = id == user.device_id;
                    DeviceListItem {
                        id,
                        kind,
                        label,
                        paired_at,
                        last_seen_at,
                        paired_from_ip: ip,
                        is_current,
                    }
                })
                .collect();
            (StatusCode::OK, Json(json!({"devices": items}))).into_response()
        }
        Err(e) => {
            tracing::warn!("devices list failed: {e:#}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "list_failed"})),
            )
                .into_response()
        }
    }
}

/// `DELETE /api/devices/:id` — revoke a paired device. Refuses if it's the
/// last active device (would lock the user out). To delete the last device,
/// use `virtues sudo` from the box CLI.
pub async fn revoke_handler(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(device_id): Path<String>,
) -> impl IntoResponse {
    // Whole flow runs in one transaction. The order matters:
    //
    //   1. `SELECT … FOR UPDATE` the target device row — takes the row lock so
    //      concurrent revokes can't interleave under the count guard.
    //   2. `SELECT COUNT(*) … FOR UPDATE` the other active devices — also locked
    //      so a parallel revoke can't drop the active count between our count
    //      and our update.
    //   3. If we'd be revoking the only active device → bail with 409.
    //   4. Capture the WG pubkey from credentials before the credential row's
    //      lookup_hash gets cleared.
    //   5. Update device, revoke credentials, delete sessions.
    //   6. Commit. Eviction + audit log happen after commit (best-effort).
    let mut tx = match pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("devices revoke: tx begin failed: {e:#}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal"})),
            )
                .into_response();
        }
    };

    // Lock the target row. NOT FOUND if the device doesn't exist (or is already
    // revoked) for this user.
    let target: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM app_device \
         WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL \
         FOR UPDATE",
    )
    .bind(&device_id)
    .bind(&user.id)
    .fetch_optional(&mut *tx)
    .await
    .ok()
    .flatten();
    if target.is_none() {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "not_found"})))
            .into_response();
    }

    // Lock + count the *other* active devices. Holding the lock prevents a
    // concurrent revoke from passing its own count guard while we're still
    // in this transaction.
    // Bubble DB errors as 500 — eating them would conflate "database down"
    // with "actually the only device" and surface a spurious last_device 409
    // to the user.
    let other_active: Result<Option<(i64,)>, sqlx::Error> = sqlx::query_as(
        "SELECT COUNT(*) FROM app_device \
         WHERE user_id = $1 AND revoked_at IS NULL AND id <> $2 \
         FOR UPDATE",
    )
    .bind(&user.id)
    .bind(&device_id)
    .fetch_optional(&mut *tx)
    .await;
    let other_count = match other_active {
        Ok(opt) => opt.map(|(n,)| n).unwrap_or(0),
        Err(e) => {
            tracing::warn!("devices revoke: count query failed: {e:#}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal"})),
            )
                .into_response();
        }
    };
    if other_count == 0 {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": "last_device",
                "message": "Refusing to revoke the only active device. Run `virtues sudo` to confirm."
            })),
        )
            .into_response();
    }

    // Capture the WG pubkey before clearing the credential's lookup hash.
    let wg_pubkey: Option<String> = sqlx::query_scalar(
        "SELECT metadata->>'wg_public_key' \
         FROM credentials \
         WHERE device_id = $1 AND status = 'active' \
         LIMIT 1",
    )
    .bind(&device_id)
    .fetch_optional(&mut *tx)
    .await
    .ok()
    .flatten();

    // Apply the revoke.
    if let Err(e) = sqlx::query(
        "UPDATE app_device SET revoked_at = now() WHERE id = $1",
    )
    .bind(&device_id)
    .execute(&mut *tx)
    .await
    {
        tracing::warn!("devices revoke: device update failed: {e:#}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "internal"})),
        )
            .into_response();
    }

    // Revoke credential rows belonging to this device (apps/sensors). Browser
    // sessions are deleted instead — they're cookies, not long-lived secrets.
    if let Err(e) = sqlx::query(
        "UPDATE credentials SET status = 'revoked', secret_lookup_hash = NULL, \
                                status_reason = 'device_revoked', updated_at = now() \
         WHERE device_id = $1",
    )
    .bind(&device_id)
    .execute(&mut *tx)
    .await
    {
        tracing::warn!("devices revoke: credential revoke failed: {e:#}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "internal"})),
        )
            .into_response();
    }
    if let Err(e) = sqlx::query("DELETE FROM app_auth_session WHERE device_id = $1")
        .bind(&device_id)
        .execute(&mut *tx)
        .await
    {
        tracing::warn!("devices revoke: session delete failed: {e:#}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "internal"})),
        )
            .into_response();
    }

    if let Err(e) = tx.commit().await {
        tracing::warn!("devices revoke: tx commit failed: {e:#}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "internal"})),
        )
            .into_response();
    }

    // WG eviction — best-effort, after commit so DB state is authoritative.
    if let Some(pubkey) = wg_pubkey.as_deref() {
        evict_wg_peer(pubkey);
    }

    // Event log.
    let _ = sqlx::query(
        "INSERT INTO app_auth_event (user_id, device_id, event_type, detail) \
         VALUES ($1, $2, 'revoked', $3)",
    )
    .bind(&user.id)
    .bind(&device_id)
    .bind(json!({"revoked_by_device": &user.device_id, "had_wg_peer": wg_pubkey.is_some()}))
    .execute(&pool)
    .await;

    (StatusCode::OK, Json(json!({"ok": true}))).into_response()
}

// ─── WG peer eviction ───────────────────────────────────────────────────────
// Linux-only. The macOS dev host has no kernel WG; revocation is still
// authoritative in the DB and the WG daemon will simply have no peer to
// reconcile when it next syncs.

#[cfg(target_os = "linux")]
fn evict_wg_peer(public_key: &str) {
    match virtues_wg::manager::remove_peer(public_key) {
        Ok(()) => tracing::info!(public_key, "evicted wg peer on revoke"),
        Err(e) => tracing::warn!("wg peer eviction failed (DB state is authoritative): {e:#}"),
    }
}

#[cfg(not(target_os = "linux"))]
fn evict_wg_peer(_public_key: &str) {
    tracing::debug!("wg peer eviction skipped (non-Linux host)");
}

// Silence unused `Value` import if the file evolves.
#[allow(dead_code)]
fn _value_ref(_: &Value) {}

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

    // Capture the WG pubkey before clearing the credential's lookup hash. Two
    // storage shapes: the consume path records it top-level as `wg_public_key`
    // (the device supplied its own pubkey), while the relay/provision path only
    // has the peer record at `wg.device_public_key` (the box generated the
    // keypair). COALESCE both so a provisioned device's revoke also triggers an
    // immediate reconcile + logs `had_wg_peer` accurately — otherwise its peer
    // lingers until the daemon's next backstop poll.
    let wg_pubkey: Option<String> = sqlx::query_scalar(
        "SELECT COALESCE(metadata->>'wg_public_key', metadata->'wg'->>'device_public_key') \
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

    // Drop the revoked device's EndpointId from the iroh transport allowlist
    // (and re-report the shrunk set to atlas) so it can no longer reach the box.
    crate::relay::after_pairing_change(pool.clone());

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

#[derive(serde::Deserialize)]
pub struct SelfNodeIdRequest {
    /// The calling device's iroh EndpointId (hex).
    pub node_id: String,
}

/// `POST /api/devices/self/node-id { node_id }` — the calling device (authed by
/// its own bearer) reports its iroh EndpointId so the box allowlists it on its
/// iroh transport. This is how a device provisioned off-LAN (QR/iOS), which
/// wasn't present at consume time to submit `device_node_id`, becomes reachable.
pub async fn set_self_node_id(
    State(pool): State<PgPool>,
    user: AuthUser,
    Json(body): Json<SelfNodeIdRequest>,
) -> impl IntoResponse {
    let node_id = body.node_id.trim();
    if node_id.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "missing_node_id"}))).into_response();
    }
    match sqlx::query("UPDATE app_device SET node_id = $1 WHERE id = $2 AND revoked_at IS NULL")
        .bind(node_id)
        .bind(&user.device_id)
        .execute(&pool)
        .await
    {
        Ok(_) => {
            // Hot-swap the allowlist + re-report to atlas so the device can reach
            // the box immediately.
            crate::relay::after_pairing_change(pool.clone());
            (StatusCode::OK, Json(json!({"ok": true}))).into_response()
        }
        Err(e) => {
            // Most likely a unique violation — another active device already
            // holds this EndpointId.
            tracing::warn!(error = %e, "set_self_node_id failed");
            (StatusCode::CONFLICT, Json(json!({"error": "node_id_conflict"}))).into_response()
        }
    }
}

/// `GET /api/devices/self/reach` — the calling device (authed by its own bearer)
/// re-reads the box's *current* iroh reach ticket `{box_node_id, relay_url}`.
///
/// Devices freeze the ticket at pair time; this lets them refresh it (on launch
/// or after a dial failure) instead of being stuck if the box had no relay reach
/// when they paired, or the relay URL later changed. Read-only; no state change.
pub async fn get_self_reach(State(_pool): State<PgPool>, _user: AuthUser) -> impl IntoResponse {
    let (box_node_id, relay_url) = match crate::api::pair::box_reach() {
        Some((n, r)) => (Some(n), Some(r)),
        None => (None, None),
    };
    (
        StatusCode::OK,
        Json(json!({ "box_node_id": box_node_id, "relay_url": relay_url })),
    )
        .into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct EnrollPeerRequest {
    /// The new device's iroh EndpointId (hex), generated on that device and
    /// handed to this (already-paired) device out-of-band.
    pub peer_node_id: String,
    /// The new device's kind: `mobile_app` | `desktop_app` | `sensor`.
    pub kind: String,
    /// Optional human label for the Devices page.
    pub label: Option<String>,
    /// Optional device metadata (name/model/os) for the Devices page.
    pub device_info: Option<Value>,
}

/// `POST /api/devices/enroll-peer` — peer-vouched enrollment. An **already-paired**
/// device (authed by its own bearer) vouches for a NEW device by its EndpointId:
/// the box mints the new device's credential + allowlists its EndpointId, so the
/// new device can reach the box over the relay *once it's registered* (no relay
/// grace-pass, no chicken-egg). The returned bearer is relayed back to the new
/// device by the vouching device over a trusted out-of-band channel.
///
/// This is the iroh-native replacement for the off-LAN "provision" QR: the new
/// device generates its own key first, so its EndpointId is known at enrollment.
pub async fn enroll_peer(
    State(pool): State<PgPool>,
    _user: AuthUser,
    Json(body): Json<EnrollPeerRequest>,
) -> impl IntoResponse {
    let peer_node_id = body.peer_node_id.trim();
    if peer_node_id.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "missing_peer_node_id"}))).into_response();
    }
    let kind = body.kind.trim();
    if !matches!(kind, "mobile_app" | "desktop_app" | "sensor") {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "invalid_kind"}))).into_response();
    }
    let source_id = match crate::api::pair::resolve_source_id(kind, None) {
        Ok(s) => s,
        Err(()) => return (StatusCode::BAD_REQUEST, Json(json!({"error": "unknown_source"}))).into_response(),
    };
    let label = body
        .label
        .as_deref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| kind.to_string());

    // Mint the encrypted bearer OUTSIDE the tx (crypto/KMS).
    let bp = match crate::api::pair::build_bearer_pack(kind, &label, &body.device_info) {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    let device_id = crate::ids::generate_id(
        crate::ids::DEVICE_PREFIX,
        &[peer_node_id, &Utc::now().to_rfc3339()],
    );
    let device_info = body.device_info.clone().unwrap_or_else(|| json!({}));

    let mut tx = match pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(error = %e, "enroll_peer: begin tx failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal"}))).into_response();
        }
    };
    if let Err(e) = crate::api::pair::insert_device_row(
        &mut tx, &device_id, kind, &label, &device_info, None, Some(peer_node_id),
    )
    .await
    {
        // Unique-index violation → this EndpointId is already an active device.
        tracing::warn!(error = %e, "enroll_peer: device insert failed");
        return (StatusCode::CONFLICT, Json(json!({"error": "peer_already_enrolled"}))).into_response();
    }
    if let Err(e) = crate::api::pair::insert_credential_row(&mut tx, &bp, &source_id, &label, &device_id, None).await {
        tracing::warn!(error = %e, "enroll_peer: credential insert failed");
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "credential_insert_failed"}))).into_response();
    }
    if let Err(e) = tx.commit().await {
        tracing::warn!(error = %e, "enroll_peer: commit failed");
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal"}))).into_response();
    }

    // Fan out per-credential actions, then allowlist the new EndpointId + register
    // it with atlas so the relay admits it.
    let action_ids = crate::api::pair::assemble_action_fanout(&pool, &bp.credential_id)
        .await
        .unwrap_or_default();
    crate::relay::after_pairing_change(pool.clone());

    let (box_node_id, relay_url) = match crate::api::pair::box_reach() {
        Some((n, r)) => (Some(n), Some(r)),
        None => (None, None),
    };
    (
        StatusCode::OK,
        Json(json!({
            "device_id": device_id,
            "credential_id": bp.credential_id,
            "bearer": bp.bearer,
            "action_ids": action_ids,
            "box_node_id": box_node_id,
            "relay_url": relay_url,
        })),
    )
        .into_response()
}

// WG peer eviction is no longer done inline here. Kernel `wg0` state has a
// single writer — the `virtues-wireguard` daemon — which reconciles from the
// active peer set (see virtues_wg::reconcile + signal). Revoke marks the row
// and fires `NOTIFY wg_reconcile`; the daemon drops the peer. This removes the
// previous dual-writer (a direct `remove_peer` here racing the daemon's poll).

// Silence unused `Value` import if the file evolves.
#[allow(dead_code)]
fn _value_ref(_: &Value) {}

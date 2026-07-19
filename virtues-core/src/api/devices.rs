//! Devices — the unified "+ paired things" surface.
//!
//! Lists every active paired device (mobile apps, sensors, the CLI) and supports
//! revocation. A device's only credential is its allowlisted iroh key, so revoke
//! is simply:
//!
//!   1. Mark `app_device.revoked_at = now()` — the reconciler drops the device's
//!      iroh key from the allowlist, so its next dial is refused at the QUIC
//!      handshake (existing connections aren't force-evicted; iroh has no
//!      per-conn revoke — next dial is the boundary). The same reconcile GCs the
//!      device's device-anchored ingest actions.
//!   2. Append an `app_auth_event` row tagged `revoked`.
//!
//! The allowlist refresh + event log are best-effort and logged on failure but
//! don't block the revocation.

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
    /// The client's reported build identity (from the `X-Virtues-Client`
    /// header, stored under `device_info.build`). Null until the device has
    /// made a request on a build that sends it.
    pub version: Option<String>,
    pub sha: Option<String>,
    pub channel: Option<String>,
    /// True if this is the device currently making the request.
    pub is_current: bool,
}

/// `GET /api/devices` — list all active paired devices for the current user.
pub async fn list_handler(State(pool): State<PgPool>, user: AuthUser) -> impl IntoResponse {
    #[allow(clippy::type_complexity)]
    let rows: Result<Vec<(String, String, String, DateTime<Utc>, Option<DateTime<Utc>>, Option<String>, Option<String>, Option<String>, Option<String>)>, _> =
        sqlx::query_as(
            "SELECT id, kind, label, paired_at, last_seen_at, paired_from_ip, \
                    device_info->'build'->>'version' AS version, \
                    device_info->'build'->>'sha'     AS sha, \
                    device_info->'build'->>'channel' AS channel \
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
                .map(|(id, kind, label, paired_at, last_seen_at, ip, version, sha, channel)| {
                    let is_current = id == user.device_id;
                    DeviceListItem {
                        id,
                        kind,
                        label,
                        paired_at,
                        last_seen_at,
                        paired_from_ip: ip,
                        version,
                        sha,
                        channel,
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

/// Bare-pool device list for the `virtues device ls` CLI. No `AuthUser` — the
/// on-box operator is the owner (physical access = you). Non-revoked devices,
/// newest-active first. Returns `(id, kind, label, node_id, last_seen_at)`.
pub async fn list_devices_cli(
    pool: &PgPool,
) -> Result<Vec<(String, String, String, Option<String>, Option<DateTime<Utc>>)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, kind, label, node_id, last_seen_at \
         FROM app_device \
         WHERE revoked_at IS NULL \
         ORDER BY last_seen_at DESC NULLS LAST, paired_at DESC",
    )
    .fetch_all(pool)
    .await
}

/// Bare-pool device revoke for the `virtues device rm` CLI. Mirrors the HTTP
/// revoke's core in one transaction (mark the device revoked + revoke its
/// credential rows), then kicks `after_pairing_change` so the de-allowlist +
/// atlas re-report happen immediately. `Ok(false)` if no such active device.
pub async fn revoke_device_cli(pool: &PgPool, device_id: &str) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let affected = sqlx::query(
        "UPDATE app_device SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(device_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if affected == 0 {
        tx.rollback().await?;
        return Ok(false);
    }
    tx.commit().await?;
    // De-allowlist the device's iroh key (+ GC its device-anchored ingest actions
    // on the next reconcile). Devices hold no credential row to revoke.
    crate::relay::after_pairing_change(pool.clone());
    Ok(true)
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

    // The device's iroh key is de-allowlisted by the `app_device.revoked_at`
    // update above (the reconciler drops it from the allowlist), so the next dial
    // is refused at the handshake. Devices hold no credential row to revoke, and
    // their device-anchored ingest actions are GC'd by the next reconcile.
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
    .bind(json!({"revoked_by_device": &user.device_id}))
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

/// `GET /api/devices/self/reach` — read the box's *current* iroh reach ticket
/// `{box_node_id, relay_url}`. **Anonymous**: the reach ticket is the box's
/// public address (its EndpointId + relay URL) — not a secret — and a device
/// needs it precisely to bootstrap its first iroh dial (before which it has no
/// key-authenticated channel). Connecting still requires an allowlisted key, so
/// exposing the address grants nothing. Read-only; no state change.
pub async fn get_self_reach(State(_pool): State<PgPool>) -> impl IntoResponse {
    let (box_node_id, relay_url, box_direct_addrs) = crate::api::pair::box_reach_fields();
    (
        StatusCode::OK,
        Json(json!({
            "box_node_id": box_node_id,
            "relay_url": relay_url,
            "box_direct_addrs": box_direct_addrs,
        })),
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
    match enroll_peer_core(
        &pool,
        &body.peer_node_id,
        &body.kind,
        body.label.as_deref(),
        body.device_info.as_ref(),
    )
    .await
    {
        Ok(p) => {
            let (box_node_id, relay_url, box_direct_addrs) =
                crate::api::pair::box_reach_fields();
            (
                StatusCode::OK,
                Json(json!({
                    "device_id": p.device_id,
                    "action_ids": p.action_ids,
                    "box_node_id": box_node_id,
                    "relay_url": relay_url,
                    "box_direct_addrs": box_direct_addrs,
                })),
            )
                .into_response()
        }
        Err(e) => e.into_response(),
    }
}

/// The result of enrolling a peer device: the device row + its ingest action
/// map. No bearer — the peer authenticates by its allowlisted iroh key.
pub(crate) struct EnrolledPeer {
    pub device_id: String,
    pub action_ids: std::collections::HashMap<String, String>,
}

pub(crate) enum EnrollError {
    MissingPeer,
    InvalidKind,
    UnknownSource,
    /// The EndpointId is already an active device (unique-index violation).
    Conflict,
    Internal,
}

impl EnrollError {
    pub(crate) fn into_response(self) -> axum::response::Response {
        match self {
            EnrollError::MissingPeer => {
                (StatusCode::BAD_REQUEST, Json(json!({"error": "missing_peer_node_id"}))).into_response()
            }
            EnrollError::InvalidKind => {
                (StatusCode::BAD_REQUEST, Json(json!({"error": "invalid_kind"}))).into_response()
            }
            EnrollError::UnknownSource => {
                (StatusCode::BAD_REQUEST, Json(json!({"error": "unknown_source"}))).into_response()
            }
            EnrollError::Conflict => {
                (StatusCode::CONFLICT, Json(json!({"error": "peer_already_enrolled"}))).into_response()
            }
            EnrollError::Internal => {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal"}))).into_response()
            }
        }
    }
}

/// Core peer enrollment shared by `enroll_peer` (HTTP) and the link-a-device
/// approve step: insert `app_device{node_id=peer, source_id}`, allowlist +
/// register the EndpointId with atlas, then fan out the device's ingest actions.
/// No bearer changes hands — the peer's proven key is its credential.
pub(crate) async fn enroll_peer_core(
    pool: &PgPool,
    peer_node_id: &str,
    kind: &str,
    label: Option<&str>,
    device_info: Option<&Value>,
) -> Result<EnrolledPeer, EnrollError> {
    let peer_node_id = peer_node_id.trim();
    if peer_node_id.is_empty() {
        return Err(EnrollError::MissingPeer);
    }
    let kind = kind.trim();
    if !matches!(kind, "mobile_app" | "desktop_app" | "sensor") {
        return Err(EnrollError::InvalidKind);
    }
    let source_id = crate::api::pair::resolve_source_id(kind, None).map_err(|()| EnrollError::UnknownSource)?;
    let label = label
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| kind.to_string());

    let device_info_val = device_info.cloned().unwrap_or_else(|| json!({}));
    let device_id = crate::ids::generate_id(
        crate::ids::DEVICE_PREFIX,
        &[peer_node_id, &Utc::now().to_rfc3339()],
    );

    let mut tx = pool.begin().await.map_err(|e| {
        tracing::warn!(error = %e, "enroll_peer_core: begin tx failed");
        EnrollError::Internal
    })?;
    // Idempotent on node_id: re-enrolling the same peer UPDATEs its row and
    // returns the existing id (rather than 500-ing on the unique node_id).
    let device_id = crate::api::pair::insert_device_row(
        &mut tx,
        &device_id,
        kind,
        &label,
        &device_info_val,
        None,
        Some(peer_node_id),
        Some(source_id.as_str()),
    )
    .await
    .map_err(|e| {
        tracing::warn!(error = %e, "enroll_peer_core: device insert failed");
        EnrollError::Internal
    })?;
    tx.commit().await.map_err(|e| {
        tracing::warn!(error = %e, "enroll_peer_core: commit failed");
        EnrollError::Internal
    })?;

    // Allowlist the EndpointId + register it with atlas so the relay admits it,
    // THEN fan out the device's ingest actions (anchored on device_id).
    crate::relay::after_pairing_change(pool.clone());
    let action_ids = crate::api::pair::assemble_action_fanout(pool, &device_id)
        .await
        .unwrap_or_default();

    Ok(EnrolledPeer {
        device_id,
        action_ids,
    })
}

// WG peer eviction is no longer done inline here. Kernel `wg0` state has a
// single writer — the `virtues-wireguard` daemon — which reconciles from the
// active peer set (see virtues_wg::reconcile + signal). Revoke marks the row
// and fires `NOTIFY wg_reconcile`; the daemon drops the peer. This removes the
// previous dual-writer (a direct `remove_peer` here racing the daemon's poll).

//! Devices — the unified "+ paired things" surface.
//!
//! Lists every active paired device (browsers, mobile apps, sensors, the CLI)
//! and supports revocation. Revoke is the single operation that ends all of
//! a device's authority in one step:
//!
//!   1. Mark `app_device.revoked_at = now()` — the reconciler drops the device's
//!      iroh key from the allowlist, so its next dial is refused at the QUIC
//!      handshake (existing connections aren't force-evicted; iroh has no
//!      per-conn revoke — next dial is the boundary).
//!   2. Move any attached `credentials.status` to `'revoked'` and clear the
//!      `secret_lookup_hash` so any webhook/OAuth token it owns can no longer be
//!      matched O(1) at `validate_device_token` time.
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
    sqlx::query(
        "UPDATE credentials SET status = 'revoked', secret_lookup_hash = NULL, \
                                status_reason = 'device_revoked', updated_at = now() \
         WHERE device_id = $1",
    )
    .bind(device_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
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

    // Revoke credential rows belonging to this device (webhook/OAuth tokens).
    // The device's iroh key is de-allowlisted by the `app_device.revoked_at`
    // update above (the reconciler drops it from the allowlist), so the next
    // dial is refused at the handshake — there is no session row to delete.
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

/// `GET /api/devices/self/reach` — read the box's *current* iroh reach ticket
/// `{box_node_id, relay_url}`. **Anonymous**: the reach ticket is the box's
/// public address (its EndpointId + relay URL) — not a secret — and a device
/// needs it precisely to bootstrap its first iroh dial (before which it has no
/// key-authenticated channel). Connecting still requires an allowlisted key, so
/// exposing the address grants nothing. Read-only; no state change.
pub async fn get_self_reach(State(_pool): State<PgPool>) -> impl IntoResponse {
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
            let (box_node_id, relay_url) = match crate::api::pair::box_reach() {
                Some((n, r)) => (Some(n), Some(r)),
                None => (None, None),
            };
            (
                StatusCode::OK,
                Json(json!({
                    "device_id": p.device_id,
                    "credential_id": p.credential_id,
                    "bearer": p.bearer,
                    "action_ids": p.action_ids,
                    "box_node_id": box_node_id,
                    "relay_url": relay_url,
                })),
            )
                .into_response()
        }
        Err(e) => e.into_response(),
    }
}

/// The result of enrolling a peer device — the caller decides how to deliver the
/// bearer (HTTP response for `enroll-peer`, or stashed for later iroh redeem in
/// the link-a-device flow).
pub(crate) struct EnrolledPeer {
    pub device_id: String,
    pub credential_id: String,
    pub bearer: String,
    /// Encrypted `{"token": bearer}` (same form as `credentials.secrets_ciphertext`)
    /// — for stashing the bearer at rest until a later redeem.
    pub bearer_ciphertext: String,
    pub action_ids: std::collections::HashMap<String, String>,
}

pub(crate) enum EnrollError {
    MissingPeer,
    InvalidKind,
    UnknownSource,
    /// The EndpointId is already an active device (unique-index violation).
    Conflict,
    Bearer(crate::api::pair::BearerPackError),
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
            EnrollError::Bearer(e) => e.into_response(),
            EnrollError::Internal => {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal"}))).into_response()
            }
        }
    }
}

/// Core peer enrollment shared by `enroll_peer` (HTTP) and the link-a-device
/// approve step: insert `app_device{node_id=peer}`, mint the credential, fan out
/// actions, and allowlist + register the EndpointId with atlas. Returns the
/// minted bearer (+ its ciphertext) for the caller to deliver.
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

    let di = device_info.cloned();
    let device_info_val = di.clone().unwrap_or_else(|| json!({}));
    // Mint the encrypted bearer OUTSIDE the tx (crypto/KMS).
    let bp = crate::api::pair::build_bearer_pack(kind, &label, &di).map_err(EnrollError::Bearer)?;
    let device_id = crate::ids::generate_id(
        crate::ids::DEVICE_PREFIX,
        &[peer_node_id, &Utc::now().to_rfc3339()],
    );

    let mut tx = pool.begin().await.map_err(|e| {
        tracing::warn!(error = %e, "enroll_peer_core: begin tx failed");
        EnrollError::Internal
    })?;
    crate::api::pair::insert_device_row(&mut tx, &device_id, kind, &label, &device_info_val, None, Some(peer_node_id))
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "enroll_peer_core: device insert failed (likely duplicate node_id)");
            EnrollError::Conflict
        })?;
    crate::api::pair::insert_credential_row(&mut tx, &bp, &source_id, &label, &device_id, None)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "enroll_peer_core: credential insert failed");
            EnrollError::Internal
        })?;
    tx.commit().await.map_err(|e| {
        tracing::warn!(error = %e, "enroll_peer_core: commit failed");
        EnrollError::Internal
    })?;

    // Fan out per-credential actions, then allowlist the EndpointId + register it
    // with atlas so the relay admits it.
    let action_ids = crate::api::pair::assemble_action_fanout(pool, &bp.credential_id)
        .await
        .unwrap_or_default();
    crate::relay::after_pairing_change(pool.clone());

    Ok(EnrolledPeer {
        device_id,
        credential_id: bp.credential_id,
        bearer: bp.bearer,
        bearer_ciphertext: bp.ciphertext,
        action_ids,
    })
}

// ─── Link a device (fully-remote enrollment via atlas rendezvous) ───────────

#[derive(Debug, serde::Deserialize)]
pub struct LinkStartRequest {
    /// The future device's kind (default `mobile_app`).
    pub kind: Option<String>,
    pub label: Option<String>,
}

/// `POST /api/devices/link/start` — a voucher (already-paired, AuthUser) opens a
/// link session: mint a one-time code, store `H(code)` locally, open an atlas
/// rendezvous, and return the code to display. See docs/reach-enrollment.md.
pub async fn link_start(
    State(pool): State<PgPool>,
    _user: AuthUser,
    Json(_body): Json<LinkStartRequest>,
) -> impl IntoResponse {
    let (box_node_id, relay_url) = match crate::api::pair::box_reach() {
        Some(v) => v,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "box_not_relay_ready"}))).into_response(),
    };
    let api_key = match crate::virtues_api::renew::read_api_key(&pool).await {
        Ok(Some(k)) => k,
        _ => return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "no_api_key"}))).into_response(),
    };
    let code = crate::api::pair::random_link_code();
    let code_hash = crate::api::pair::hash_token(&code);
    let ttl: i64 = 600;
    let expires = Utc::now() + chrono::Duration::seconds(ttl);
    let _ = sqlx::query("DELETE FROM app_link_session WHERE expires_at < now()").execute(&pool).await;
    if let Err(e) = sqlx::query(
        "INSERT INTO app_link_session (code_hash, status, expires_at) VALUES ($1, 'pending', $2)",
    )
    .bind(&code_hash)
    .bind(expires)
    .execute(&pool)
    .await
    {
        tracing::warn!(error = %e, "link_start: local insert failed");
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal"}))).into_response();
    }
    // Open the atlas rendezvous (blind — code hash + public reach only).
    let http = crate::http_client::virtues_api_client();
    let atlas = crate::virtues_api::atlas_url();
    let ok = matches!(
        http.post(format!("{}/link/session", atlas.trim_end_matches('/')))
            .json(&json!({"api_key": api_key, "code_hash": code_hash, "box_node_id": box_node_id, "relay_url": relay_url, "ttl_secs": ttl}))
            .send()
            .await,
        Ok(r) if r.status().is_success()
    );
    if !ok {
        tracing::warn!("link_start: atlas session open failed");
        let _ = sqlx::query("DELETE FROM app_link_session WHERE code_hash = $1").bind(&code_hash).execute(&pool).await;
        return (StatusCode::BAD_GATEWAY, Json(json!({"error": "atlas_unreachable"}))).into_response();
    }
    (StatusCode::OK, Json(json!({"code": code, "expires_in": ttl}))).into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct LinkCodeRequest {
    pub code: String,
}

/// `POST /api/devices/link/status` — voucher polls whether the new device has
/// shown up (so it can offer Approve). Returns the atlas-side status.
pub async fn link_status(
    State(pool): State<PgPool>,
    _user: AuthUser,
    Json(body): Json<LinkCodeRequest>,
) -> impl IntoResponse {
    let code_hash = crate::api::pair::hash_token(body.code.trim());
    let local: Option<(String,)> = sqlx::query_as(
        "SELECT status FROM app_link_session WHERE code_hash = $1 AND expires_at > now()",
    )
    .bind(&code_hash)
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten();
    let Some((local_status,)) = local else {
        return (StatusCode::OK, Json(json!({"status": "expired"}))).into_response();
    };
    if local_status != "pending" {
        return (StatusCode::OK, Json(json!({"status": local_status, "device_waiting": false}))).into_response();
    }
    let (status, waiting) = match atlas_link_poll(&pool, &code_hash).await {
        Some(v) => (v.0.clone(), v.0 == "requested"),
        None => ("pending".to_string(), false),
    };
    (StatusCode::OK, Json(json!({"status": status, "device_waiting": waiting}))).into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct LinkApproveRequest {
    pub code: String,
    pub kind: Option<String>,
    pub label: Option<String>,
}

/// `POST /api/devices/link/approve` — voucher approves: verify the MAC binds the
/// new device's EndpointId (detects an atlas-swapped id), enroll the peer, stash
/// the bearer for iroh redeem, and mark the atlas session approved.
pub async fn link_approve(
    State(pool): State<PgPool>,
    _user: AuthUser,
    Json(body): Json<LinkApproveRequest>,
) -> impl IntoResponse {
    let code = body.code.trim().to_string();
    let code_hash = crate::api::pair::hash_token(&code);
    // Local session must be pending.
    match sqlx::query_as::<_, (String,)>(
        "SELECT status FROM app_link_session WHERE code_hash = $1 AND expires_at > now()",
    )
    .bind(&code_hash)
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten()
    {
        Some((s,)) if s == "pending" => {}
        Some(_) => return (StatusCode::CONFLICT, Json(json!({"error": "already_handled"}))).into_response(),
        None => return (StatusCode::GONE, Json(json!({"error": "expired"}))).into_response(),
    }
    // Fetch the device's submitted EndpointId + MAC from atlas.
    let Some((status, endpoint_id, mac)) = atlas_link_poll(&pool, &code_hash).await else {
        return (StatusCode::BAD_GATEWAY, Json(json!({"error": "atlas_unreachable"}))).into_response();
    };
    let (endpoint_id, mac) = match (status.as_str(), endpoint_id, mac) {
        ("requested", Some(e), Some(m)) if !e.is_empty() && !m.is_empty() => (e, m),
        _ => return (StatusCode::CONFLICT, Json(json!({"error": "no_device_yet"}))).into_response(),
    };
    // Verify the MAC binds this EndpointId to the code (atlas can't forge it
    // without the code; a mismatch means the id was tampered with in transit).
    let expected = virtues_helpers::crypto::hmac_sha256_hex(code.as_bytes(), endpoint_id.as_bytes());
    if !virtues_helpers::crypto::constant_time_eq(expected.as_bytes(), mac.as_bytes()) {
        tracing::warn!("link_approve: MAC mismatch — endpoint_id may have been tampered");
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "mac_mismatch"}))).into_response();
    }
    // Enroll the peer (mint credential + allowlist + register with atlas).
    let kind = body.kind.as_deref().unwrap_or("mobile_app");
    let enrolled = match enroll_peer_core(&pool, &endpoint_id, kind, body.label.as_deref(), None).await {
        Ok(e) => e,
        Err(e) => return e.into_response(),
    };
    // Stash the bearer (ciphertext) for the iroh redeem + mark approved.
    let action_ids_json = serde_json::to_value(&enrolled.action_ids).unwrap_or_else(|_| json!({}));
    if let Err(e) = sqlx::query(
        "UPDATE app_link_session SET status = 'approved', device_endpoint_id = $2, \
         bearer_ciphertext = $3, credential_id = $4, action_ids = $5 WHERE code_hash = $1",
    )
    .bind(&code_hash)
    .bind(&endpoint_id)
    .bind(&enrolled.bearer_ciphertext)
    .bind(&enrolled.credential_id)
    .bind(&action_ids_json)
    .execute(&pool)
    .await
    {
        tracing::warn!(error = %e, "link_approve: stash failed");
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal"}))).into_response();
    }
    // Tell atlas the device may proceed to redeem (best-effort).
    if let Ok(Some(api_key)) = crate::virtues_api::renew::read_api_key(&pool).await {
        let http = crate::http_client::virtues_api_client();
        let atlas = crate::virtues_api::atlas_url();
        let _ = http
            .post(format!("{}/link/approve", atlas.trim_end_matches('/')))
            .json(&json!({"api_key": api_key, "code_hash": code_hash}))
            .send()
            .await;
    }
    (StatusCode::OK, Json(json!({"ok": true, "device_id": enrolled.device_id}))).into_response()
}

/// Poll atlas `/link/status` → `(status, device_endpoint_id, mac)`. `None` on a
/// transport/auth failure.
async fn atlas_link_poll(pool: &PgPool, code_hash: &str) -> Option<(String, Option<String>, Option<String>)> {
    let api_key = crate::virtues_api::renew::read_api_key(pool).await.ok().flatten()?;
    let http = crate::http_client::virtues_api_client();
    let atlas = crate::virtues_api::atlas_url();
    let resp = http
        .post(format!("{}/link/status", atlas.trim_end_matches('/')))
        .json(&json!({"api_key": api_key, "code_hash": code_hash}))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: Value = resp.json().await.ok()?;
    let status = v.get("status")?.as_str()?.to_string();
    let endpoint_id = v.get("device_endpoint_id").and_then(|x| x.as_str()).map(String::from);
    let mac = v.get("mac").and_then(|x| x.as_str()).map(String::from);
    Some((status, endpoint_id, mac))
}

// WG peer eviction is no longer done inline here. Kernel `wg0` state has a
// single writer — the `virtues-wireguard` daemon — which reconciles from the
// active peer set (see virtues_wg::reconcile + signal). Revoke marks the row
// and fires `NOTIFY wg_reconcile`; the daemon drops the peer. This removes the
// previous dual-writer (a direct `remove_peer` here racing the daemon's poll).

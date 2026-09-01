//! Sudo — the "prove physical access to the box" gate for high-sensitivity
//! actions (export-all, BYO-key swap, wipe, revoke-last-device,
//! import-applet-package). `GATED_ACTIONS` below is the list; keep this
//! sentence in step with it.
//!
//! Flow (v1, CLI-confirm):
//!
//!   1. Web UI: user clicks a gated action → `POST /api/sudo/request`.
//!      Server inserts an `app_sudo_request` row in `pending` with a 5-min
//!      TTL and returns its id. The UI displays "Run `virtues sudo` on the
//!      box" and polls `/api/sudo/status/:id`.
//!
//!   2. Box CLI: user runs `virtues sudo`. The CLI lists open `pending`
//!      requests (action + requesting device label + IP), prompts y/N, and
//!      on `y` flips the row to `approved` with `approved_by = 'cli'`.
//!
//!   3. Web UI: polling sees `status = 'approved'` and proceeds with the
//!      action. The same request id is presented as a one-time capability;
//!      the gated handler verifies `status = 'approved'` AND consumes it
//!      (single-use).
//!
//! v1.1 will add a push-confirm channel from the iOS app; the request/poll
//! shapes don't change — only the approve path gains an alternative source.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::middleware::auth::AuthUser;
use crate::middleware::{client_ip, OWNER_USER_ID};

/// Window during which a sudo request is approvable. Five minutes matches
/// modern OS sudo / `gh auth refresh` norms.
const REQUEST_TTL_MIN: i64 = 5;

/// Gated actions. Adding a new one requires explicit listing here — keeps the
/// surface auditable.
const GATED_ACTIONS: &[&str] = &[
    "export_data",
    "change_byo_key",
    "wipe_box",
    "revoke_last_device",
    // Installing a third party's package runs their code on this box. It was
    // the only route in the app that did that, and it was gated by nothing
    // while changing an API key was gated by this — an asymmetry worth
    // correcting even now that imported code runs jailed.
    "import_applet_package",
];

// ─── HTTP: request + status (web side) ──────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RequestBody {
    pub action: String,
    #[serde(default)]
    pub action_payload: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct RequestResponse {
    pub id: String,
    pub expires_at: String,
    /// The CLI command the user should run on the box to approve this
    /// request. Server-controlled so non-standard deployments (different
    /// daemon user, system-wide binary at a non-default path) print the
    /// right instructions in the web modal.
    ///
    /// Configurable via `VIRTUES_SUDO_COMMAND` env var; defaults to the
    /// canonical `sudo -u virtues virtues sudo` for installs from the
    /// standard installer.
    pub cli_command: String,
}

fn sudo_cli_command() -> String {
    std::env::var("VIRTUES_SUDO_COMMAND")
        .unwrap_or_else(|_| "sudo -u virtues virtues sudo".to_string())
}

/// `POST /api/sudo/request` — auth'd. Mint a sudo request.
pub async fn request_handler(
    State(pool): State<PgPool>,
    user: AuthUser,
    headers: axum::http::HeaderMap,
    Json(req): Json<RequestBody>,
) -> impl IntoResponse {
    if !GATED_ACTIONS.contains(&req.action.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "ungated_action"})),
        )
            .into_response();
    }
    let id = crate::ids::generate_id(
        crate::ids::SUDO_REQUEST_PREFIX,
        &[&user.device_id, &req.action, &Utc::now().to_rfc3339()],
    );
    let expires_at = Utc::now() + Duration::minutes(REQUEST_TTL_MIN);
    let payload = req.action_payload.unwrap_or_else(|| json!({}));
    let ip = client_ip(&headers);

    if let Err(e) = sqlx::query(
        "INSERT INTO app_sudo_request \
         (id, requested_by, action, action_payload, requested_ip, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(&id)
    .bind(&user.device_id)
    .bind(&req.action)
    .bind(&payload)
    .bind(ip.clone())
    .bind(expires_at)
    .execute(&pool)
    .await
    {
        tracing::warn!("sudo request insert failed: {e:#}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "internal"})),
        )
            .into_response();
    }

    let _ = sqlx::query(
        "INSERT INTO app_auth_event (user_id, device_id, event_type, detail, ip) \
         VALUES ($1, $2, 'sudo_requested', $3, $4)",
    )
    .bind(&user.id)
    .bind(&user.device_id)
    .bind(json!({"action": &req.action, "request_id": &id}))
    .bind(ip)
    .execute(&pool)
    .await;

    (
        StatusCode::OK,
        Json(RequestResponse {
            id,
            expires_at: expires_at.to_rfc3339(),
            cli_command: sudo_cli_command(),
        }),
    )
        .into_response()
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub status: String,
    pub action: String,
    pub expires_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_at: Option<String>,
}

/// `GET /api/sudo/status/:id` — auth'd. UI polls for state.
pub async fn status_handler(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let row: Option<(String, String, DateTime<Utc>, Option<DateTime<Utc>>)> = sqlx::query_as(
        "SELECT status, action, expires_at, approved_at \
         FROM app_sudo_request \
         WHERE id = $1 AND requested_by = $2",
    )
    .bind(&id)
    .bind(&user.device_id)
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten();

    match row {
        Some((status, action, expires_at, approved_at)) => {
            // Surface expiry without mutating state — the periodic sweeper
            // (or `verify_and_consume`) flips the column when needed.
            let effective_status = if status == "pending" && Utc::now() > expires_at {
                "expired".to_string()
            } else {
                status
            };
            (
                StatusCode::OK,
                Json(StatusResponse {
                    status: effective_status,
                    action,
                    expires_at: expires_at.to_rfc3339(),
                    approved_at: approved_at.map(|d| d.to_rfc3339()),
                }),
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, Json(json!({"error": "not_found"})))
            .into_response(),
    }
}

// ─── Gated-action helper (used by handlers that gate on sudo) ───────────────

/// Atomically verify that an approval is fresh + consume it (single-use).
/// Returns `Ok(action_payload)` on success. Handlers gating on sudo call this
/// with the request id submitted by the client (e.g. as a query param or
/// header on the gated request); they then perform the action.
pub async fn verify_and_consume(
    pool: &PgPool,
    request_id: &str,
    expected_action: &str,
    requesting_device: &str,
) -> crate::Result<Value> {
    // Set status to 'consumed' (NOT 'expired') so the audit log can distinguish
    // "approval was used to perform the action" from "approval timed out."
    let row: Option<(Value,)> = sqlx::query_as(
        "UPDATE app_sudo_request \
         SET status = 'consumed', consumed_at = now() \
         WHERE id = $1 \
           AND requested_by = $2 \
           AND action = $3 \
           AND status = 'approved' \
           AND expires_at > now() \
         RETURNING action_payload",
    )
    .bind(request_id)
    .bind(requesting_device)
    .bind(expected_action)
    .fetch_optional(pool)
    .await
    .map_err(|e| crate::Error::Database(format!("verify_and_consume: {e}")))?;

    row.map(|(payload,)| payload).ok_or_else(|| {
        crate::Error::Unauthorized("sudo not approved or expired".to_string())
    })
}

// ─── CLI side: list pending + approve ───────────────────────────────────────
//
// These two functions are called by `virtues sudo` (the CLI command). They
// don't need an HTTP route — the CLI talks directly to the DB.

#[derive(Debug)]
pub struct PendingSudoRequest {
    pub id: String,
    pub action: String,
    pub requesting_device_label: String,
    pub requested_ip: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// Pull every still-pending, non-expired sudo request. Order: oldest first
/// (the user usually wants to approve in the order they triggered).
pub async fn list_pending(pool: &PgPool) -> crate::Result<Vec<PendingSudoRequest>> {
    let rows: Vec<(String, String, String, Option<String>, DateTime<Utc>, DateTime<Utc>)> =
        sqlx::query_as(
            "SELECT r.id, r.action, d.label, r.requested_ip, r.created_at, r.expires_at \
             FROM app_sudo_request r \
             JOIN app_device d ON d.id = r.requested_by \
             WHERE r.status = 'pending' AND r.expires_at > now() \
             ORDER BY r.created_at",
        )
        .fetch_all(pool)
        .await
        .map_err(|e| crate::Error::Database(format!("list_pending: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|(id, action, label, ip, created_at, expires_at)| PendingSudoRequest {
            id,
            action,
            requesting_device_label: label,
            requested_ip: ip,
            created_at,
            expires_at,
        })
        .collect())
}

/// CLI approve. Idempotent: if the row is already approved it returns Ok.
pub async fn approve_from_cli(pool: &PgPool, request_id: &str) -> crate::Result<()> {
    let n = sqlx::query(
        "UPDATE app_sudo_request \
         SET status = 'approved', approved_at = now(), approved_by = 'cli' \
         WHERE id = $1 AND status = 'pending' AND expires_at > now()",
    )
    .bind(request_id)
    .execute(pool)
    .await
    .map_err(|e| crate::Error::Database(format!("approve: {e}")))?
    .rows_affected();

    if n == 0 {
        return Err(crate::Error::Other(format!(
            "request {request_id} is not pending or has expired"
        )));
    }

    // Event log
    let _ = sqlx::query(
        "INSERT INTO app_auth_event (user_id, device_id, event_type, detail) \
         VALUES ($1, NULL, 'sudo_approved', $2)",
    )
    .bind(OWNER_USER_ID)
    .bind(json!({"request_id": request_id, "approved_by": "cli"}))
    .execute(pool)
    .await;

    Ok(())
}

/// CLI deny.
pub async fn deny_from_cli(pool: &PgPool, request_id: &str) -> crate::Result<()> {
    let _ = sqlx::query(
        "UPDATE app_sudo_request SET status = 'denied' \
         WHERE id = $1 AND status = 'pending'",
    )
    .bind(request_id)
    .execute(pool)
    .await
    .map_err(|e| crate::Error::Database(format!("deny: {e}")))?;

    let _ = sqlx::query(
        "INSERT INTO app_auth_event (user_id, device_id, event_type, detail) \
         VALUES ($1, NULL, 'sudo_denied', $2)",
    )
    .bind(OWNER_USER_ID)
    .bind(json!({"request_id": request_id, "denied_by": "cli"}))
    .execute(pool)
    .await;
    Ok(())
}


//! Webhook ingestion endpoint for device / service pushes.
//!
//! Single route: `POST /webhook/:action_id`.
//!
//! Auth: the caller's **proven, allowlisted iroh key**. Devices (iOS, the Mac
//! collector) reach the box over iroh, so the transport proved their key and
//! `AuthUser` resolves it — ingest is gated by the SAME allowlist as every other
//! route, with no long-lived bearer anywhere. A proven device may only drive
//! actions anchored to IT (`app_applets.device_id`); the on-box console
//! (loopback) may drive any action.
//!
//! The unified `action_runner::run_action` enforces trigger validation,
//! condition evaluation, and dispatch. This handler only does auth + routing.

use std::sync::Arc;

use axum::{
    extract::{rejection::JsonRejection, Json, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::Value;

use crate::action_runner::{ActionRunStatus, RunnerDeps};
use crate::api::chat::ChatCancellationState;
use crate::database::Database;
use crate::middleware::auth::AuthUser;

#[derive(Debug, Serialize)]
pub struct WebhookResponse {
    pub run_id: Option<String>,
    pub status: &'static str,
}

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub storage: Arc<crate::storage::Storage>,
    pub drive_config: crate::api::DriveConfig,
    pub tool_executor: Option<Arc<crate::tools::ToolExecutor>>,
    pub yjs_state: super::yjs::YjsState,
    pub chat_cancel_state: ChatCancellationState,
}

impl axum::extract::FromRef<AppState> for sqlx::PgPool {
    fn from_ref(state: &AppState) -> Self {
        state.db.pool().clone()
    }
}

impl axum::extract::FromRef<AppState> for super::yjs::YjsState {
    fn from_ref(state: &AppState) -> Self {
        state.yjs_state.clone()
    }
}

impl axum::extract::FromRef<AppState> for ChatCancellationState {
    fn from_ref(state: &AppState) -> Self {
        state.chat_cancel_state.clone()
    }
}

/// Handler for `POST /webhook/:action_id`.
///
/// Flow:
/// 1. `AuthUser` (proven iroh key / loopback console) — a hard extractor, so an
///    unauthenticated caller is rejected before this runs.
/// 2. Fetch action → 404 if missing.
/// 3. Ownership: the proven device must own the action (`app_applets.device_id`);
///    the on-box console may drive any action → 403 otherwise.
/// 4. Dispatch via `run_action(.., "webhook", payload)`.
pub async fn webhook(
    State(state): State<AppState>,
    Path(action_id): Path<String>,
    user: AuthUser,
    headers: HeaderMap,
    payload: std::result::Result<Json<Value>, JsonRejection>,
) -> Response {
    // Do NOT swallow a body-parse failure into Value::Null and dispatch anyway.
    // A null payload reaches the action with no top-level `stream`, producing a
    // misleading "no stream selector" 0-row run that masquerades as an action
    // bug (the exact failure that hid the real cause for a week). Two rules:
    //
    //   1. LOG the rejection reason + Content-Length. The reason text
    //      distinguishes the real cause — "length limit exceeded" (raise the
    //      route's DefaultBodyLimit) vs "Failed to buffer the request body" /
    //      EOF-while-parsing (a truncated body in transit, e.g. the in-app WG
    //      tunnel not delivering large audio batches) vs a Content-Type issue.
    //   2. Return a RETRYABLE 409, never 400. The iOS client maps 400 ->
    //      `badRequest` -> markAsFailed -> the batch is DELETED (silent data
    //      loss). 409 -> `notProcessed` -> the batch is kept and resent. A
    //      rejected body was not ingested, so it must be retryable.
    let body = match payload {
        Ok(Json(v)) => v,
        Err(rej) => {
            tracing::warn!(
                action_id = %action_id,
                rejection = %rej,
                content_length = ?headers.get("content-length"),
                "webhook body rejected by Json extractor; returning 409 (retryable)"
            );
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "run_id": serde_json::Value::Null,
                    "status": "skipped",
                    "error": "invalid webhook body",
                    "detail": rej.body_text(),
                })),
            )
                .into_response();
        }
    };

    // A parsed-but-non-object body (e.g. a bare array or a JSON scalar) also has
    // no top-level `stream`. Reject it here rather than letting the action emit
    // the opaque selector error.
    if !body.is_object() {
        let kind = match &body {
            Value::Null => "null",
            Value::Bool(_) => "bool",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        };
        tracing::warn!(
            action_id = %action_id,
            kind = %kind,
            "webhook body is not a JSON object; returning 409 (retryable)"
        );
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "run_id": serde_json::Value::Null,
                "status": "skipped",
                "error": "webhook body must be a JSON object",
            })),
        )
            .into_response();
    }

    // `user` (AuthUser) is already proven by the hard extractor above: the
    // request arrived over iroh with an allowlisted key (or from the on-box
    // console). Confirm the action exists, then that this device owns it.
    tracing::debug!(device_id = %user.device_id, action_id = %action_id, "webhook authed by proven key");

    if crate::scheduler::actions::get_action(state.db.pool(), &action_id)
        .await
        .is_err()
    {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "action not found" })),
        )
            .into_response();
    }

    // Ownership: a proven device may only drive actions anchored to IT
    // (`app_applets.device_id`). The on-box console (loopback) may drive any
    // action; an action with no device anchor (e.g. an OAuth action reachable
    // only from the owner's own devices) is likewise owner-level.
    if user.device_id != crate::middleware::auth::CONSOLE_DEVICE_ID {
        let action_device: Option<String> =
            sqlx::query_scalar("SELECT device_id FROM app_applets WHERE id = $1")
                .bind(&action_id)
                .fetch_one(state.db.pool())
                .await
                .unwrap_or(None);
        if let Some(owner_device) = action_device {
            if owner_device != user.device_id {
                tracing::warn!(
                    action_id = %action_id,
                    proven_device = %user.device_id,
                    action_device = %owner_device,
                    "webhook: proven device does not own this action"
                );
                return (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "device does not own this action" })),
                )
                    .into_response();
            }
        }
    }

    let deps = RunnerDeps {
        db: state.db.pool().clone(),
        yjs: state.yjs_state.clone(),
    };

    match crate::action_runner::run_action(&deps, &action_id, "webhook", Some(&body)).await {
        Ok(result) => match result.status {
            ActionRunStatus::Success => (
                StatusCode::OK,
                Json(WebhookResponse {
                    run_id: result.run_id,
                    status: "success",
                }),
            )
                .into_response(),
            // A skipped run did NOT durably ingest the payload (concurrency
            // gate — a previous run still active — or a falsy condition). It
            // must NOT look like success: a 2xx here makes the device delete
            // the batch from its queue (silent data loss). Return a retryable
            // 409 so the client keeps the records and resends on the next
            // cycle. `skipped` is not a 5xx, so it never trips the device's
            // server-error circuit breaker.
            ActionRunStatus::Skipped => (
                StatusCode::CONFLICT,
                Json(WebhookResponse {
                    run_id: result.run_id,
                    status: "skipped",
                }),
            )
                .into_response(),
            ActionRunStatus::Forbidden => (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": result.error.unwrap_or_else(|| "webhook trigger not allowed".into()),
                })),
            )
                .into_response(),
            ActionRunStatus::NotFound => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "action disabled or not found" })),
            )
                .into_response(),
            ActionRunStatus::Failed => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "webhook action failed",
                    "detail": result.error,
                    "run_id": result.run_id,
                })),
            )
                .into_response(),
            // Webhook dispatch awaits via `run_action`, never `_detached`,
            // so this arm is unreachable in practice.
            ActionRunStatus::Running => (
                StatusCode::ACCEPTED,
                Json(WebhookResponse {
                    run_id: result.run_id,
                    status: "running",
                }),
            )
                .into_response(),
        },
        Err(e) => {
            tracing::error!(error = %e, "webhook runner error");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}

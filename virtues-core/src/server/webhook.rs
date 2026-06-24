//! Webhook ingestion endpoint for device / service pushes.
//!
//! Single route: `POST /webhook/:action_id`.
//!
//! Auth: `Authorization: Bearer <device_token>`. The token is decrypted and
//! matched against a credential via `validate_device_token`; the returned
//! credential_id must match the target action's `credential_id`, otherwise
//! a token leaked from one device can't be used to post at another's action.
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
    /// Handle to the long-running app supervisor — needed by the
    /// `/api/admin/reconcile` handler to diff/restart apps after the user
    /// (or LLM) edits a manifest. Optional only because some test setups
    /// don't boot the supervisor.
    pub service_supervisor: Option<crate::services::ServiceSupervisor>,
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
/// 1. Extract bearer → 401 if missing.
/// 2. `validate_device_token` → 401 on fail; returns the caller's credential_id.
/// 3. Fetch action → 404 if missing.
/// 4. Assert `action.credential_id == Some(caller_credential_id)` → 403 otherwise.
/// 5. Dispatch via `run_action(.., "webhook", payload)`.
pub async fn webhook(
    State(state): State<AppState>,
    Path(action_id): Path<String>,
    headers: HeaderMap,
    payload: std::result::Result<Json<Value>, JsonRejection>,
) -> Response {
    let body = match payload {
        Ok(Json(v)) => v,
        Err(_) => Value::Null,
    };

    let Some(device_token) = extract_bearer(&headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "Missing bearer token",
                "hint": "Include 'Authorization: Bearer <device_token>' header"
            })),
        )
            .into_response();
    };

    let caller_credential_id =
        match crate::api::validate_device_token(state.db.pool(), &device_token).await {
            Ok(id) => id,
            Err(e) => {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({
                        "error": "Invalid or revoked device token",
                        "message": e.to_string()
                    })),
                )
                    .into_response();
            }
        };

    if let Err(e) = crate::api::update_last_seen(state.db.pool(), &caller_credential_id).await {
        tracing::warn!("Failed to update last_seen: {}", e);
    }

    let action = match crate::scheduler::actions::get_action(state.db.pool(), &action_id).await {
        Ok(a) => a,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "action not found" })),
            )
                .into_response();
        }
    };

    if action.credential_id.as_deref() != Some(caller_credential_id.as_str()) {
        tracing::warn!(
            action_id = %action_id,
            caller_credential_id = %caller_credential_id,
            action_credential_id = ?action.credential_id,
            "webhook auth mismatch: caller credential does not own this action"
        );
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "token does not authorize this action",
            })),
        )
            .into_response();
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

/// Extract a bearer token from `Authorization: Bearer <token>`.
fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

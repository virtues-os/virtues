//! REST API handlers.

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

use super::webhook::AppState;
use crate::error::Error;

/// Sanitize a filename for use in Content-Disposition headers.
/// Removes characters that could cause header injection or parsing issues.
fn sanitize_content_disposition(filename: &str) -> String {
    filename
        .replace('"', "'")
        .replace('\\', "_")
        .replace('\r', "")
        .replace('\n', "")
}

/// Helper to convert Result to Response with proper status code
fn api_response<T: Serialize>(result: crate::error::Result<T>) -> Response {
    match result {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => error_response(e),
    }
}

/// Helper to convert Error to Response with appropriate status code
fn error_response(error: Error) -> Response {
    let (status, message) = match &error {
        Error::NotFound(_) => (StatusCode::NOT_FOUND, error.to_string()),
        Error::Unauthorized(_) => (StatusCode::UNAUTHORIZED, error.to_string()),
        Error::InvalidInput(_) => (StatusCode::BAD_REQUEST, error.to_string()),
        Error::Database(msg) if msg.contains("already has an active") => {
            (StatusCode::CONFLICT, error.to_string())
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    };

    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

/// Helper to create a success message response
fn success_message(message: &str) -> Response {
    (
        StatusCode::OK,
        Json(serde_json::json!({ "message": message })),
    )
        .into_response()
}

// Legacy source/stream/OAuth/plaid/ontology/catalog handlers were
// removed in the actions cutover.

// ============================================================================
// Actions + runs API
// ============================================================================

/// Get a single action run by ID (used for polling status)
pub async fn get_action_run_handler(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Response {
    match crate::scheduler::actions::get_run(state.db.pool(), &run_id).await {
        Ok(run) => (StatusCode::OK, Json(run)).into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// Optional body for manual trigger — forwarded as the action payload.
#[derive(Debug, Deserialize, Default)]
pub struct TriggerActionBody {
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
}

/// Manually trigger an action run.
pub async fn trigger_action_handler(
    State(state): State<AppState>,
    Path(action_id): Path<String>,
    body: Option<Json<TriggerActionBody>>,
) -> Response {
    let payload = body.and_then(|Json(b)| b.payload);

    let deps = crate::action_runner::RunnerDeps {
        db: state.db.pool().clone(),
        yjs: state.yjs_state.clone(),
    };

    // Detach the heavy phase (subprocess + agent) onto a tokio task so a
    // client disconnect can't drop the future mid-run and leave the row
    // stuck in `running`. The handler returns 202 with the run_id as soon
    // as the row is created; the UI polls `app_action_runs` for the final
    // status.
    let result = match crate::action_runner::run_action_detached(
        &deps,
        &action_id,
        "manual",
        payload.as_ref(),
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    use crate::action_runner::ActionRunStatus;
    let (status_code, status_label) = match result.status {
        ActionRunStatus::Running => (StatusCode::ACCEPTED, "running"),
        ActionRunStatus::Success => (StatusCode::OK, "success"),
        ActionRunStatus::Skipped => (StatusCode::OK, "skipped"),
        ActionRunStatus::Failed => (StatusCode::INTERNAL_SERVER_ERROR, "error"),
        ActionRunStatus::NotFound => (StatusCode::NOT_FOUND, "not_found"),
        ActionRunStatus::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
    };

    (
        status_code,
        Json(serde_json::json!({
            "run_id": result.run_id,
            "action_id": action_id,
            "status": status_label,
            "summary": result.summary,
            "error": result.error,
        })),
    )
        .into_response()
}

/// POST /api/chat-import/upload — multipart upload of a Claude / ChatGPT /
/// Gemini conversation export (Tier 3 "one-time import"). The file is staged to
/// a transient local path and the `chat_import` action is run synchronously
/// (one-time imports are user-initiated and expected to take a moment), so the
/// response carries the "Imported N messages" summary for the confirmation UI.
///
/// Mounted with a raised body limit (chat exports can exceed the 105MB default).
pub async fn chat_import_upload_handler(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Response {
    let mut provider = "unknown".to_string();
    let mut data: Option<axum::body::Bytes> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        match field.name().unwrap_or("") {
            "provider" => {
                if let Ok(t) = field.text().await {
                    provider = t;
                }
            }
            "file" => {
                if let Ok(b) = field.bytes().await {
                    data = Some(b);
                }
            }
            _ => {}
        }
    }

    let Some(bytes) = data else {
        return error_response(crate::error::Error::InvalidInput(
            "no file provided".into(),
        ));
    };

    // Stage to a transient local path the action subprocess reads then deletes.
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let file_path = std::env::temp_dir().join(format!("virtues_chat_import_{unique}.json"));
    if let Err(e) = std::fs::write(&file_path, &bytes) {
        return error_response(crate::error::Error::Other(format!(
            "failed to stage upload: {e}"
        )));
    }

    let deps = crate::action_runner::RunnerDeps {
        db: state.db.pool().clone(),
        yjs: state.yjs_state.clone(),
    };
    let payload = serde_json::json!({
        "file_path": file_path.to_string_lossy(),
        "provider": provider,
    });

    match crate::action_runner::run_action(&deps, "action_chat_import", "manual", Some(&payload))
        .await
    {
        Ok(r) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "summary": r.summary,
                "run_id": r.run_id,
            })),
        )
            .into_response(),
        Err(e) => error_response(crate::error::Error::Other(e.to_string())),
    }
}

/// List all actions with their latest run status.
pub async fn list_actions_handler(State(state): State<AppState>) -> Response {
    let pool = state.db.pool();

    let rows = sqlx::query(
        r#"SELECT
            t.id, t.owner, t.name, t.agent, t.cron_schedule,
            t.enabled, t.config, t.condition, t.triggers,
            t.memory, t.credential_id,
            t.runtime, t.command,
            t.created_at, t.updated_at,
            r.status AS last_run_status,
            r.started_at AS last_run_at,
            r.records_processed AS last_run_records,
            r.error AS last_run_error,
            r.result_summary AS last_run_summary
           FROM app_actions t
           LEFT JOIN app_action_runs r ON r.id = (
               SELECT id FROM app_action_runs
               WHERE action_id = t.id
               ORDER BY created_at DESC LIMIT 1
           )
           ORDER BY t.name"#,
    )
    .fetch_all(pool)
    .await;

    match rows {
        Ok(rows) => {
            use sqlx::Row;
            let actions: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| {
                    let id: String = r.try_get("id").unwrap_or_default();
                    let owner: String = r.try_get("owner").unwrap_or_else(|_| "user".to_string());
                    let name: String = r.try_get("name").unwrap_or_default();
                    let agent: Option<String> = r.try_get("agent").unwrap_or(None);
                    let cron: Option<String> = r.try_get("cron_schedule").unwrap_or(None);
                    let enabled: bool = r.try_get("enabled").unwrap_or(false);
                    let config_raw: String = r.try_get("config").unwrap_or_else(|_| "{}".into());
                    let config: serde_json::Value =
                        serde_json::from_str(&config_raw).unwrap_or(serde_json::json!({}));
                    let condition: Option<String> = r.try_get("condition").unwrap_or(None);
                    let triggers_raw: String =
                        r.try_get("triggers").unwrap_or_else(|_| "[]".into());
                    let triggers: Vec<String> =
                        serde_json::from_str(&triggers_raw).unwrap_or_default();
                    let memory: Option<String> = r.try_get("memory").unwrap_or(None);
                    let credential_id: Option<String> = r.try_get("credential_id").unwrap_or(None);
                    let runtime: String = r
                        .try_get("runtime")
                        .unwrap_or_else(|_| "function".to_string());
                    let command_raw: Option<String> = r.try_get("command").unwrap_or(None);
                    let command: Option<Vec<String>> = command_raw
                        .as_deref()
                        .and_then(|s| serde_json::from_str(s).ok());
                    let created: String = r.try_get("created_at").unwrap_or_default();
                    let updated: String = r.try_get("updated_at").unwrap_or_default();

                    let last_run_status: Option<String> =
                        r.try_get("last_run_status").unwrap_or(None);
                    let last_run = last_run_status.map(|s| {
                        let at: Option<String> = r.try_get("last_run_at").unwrap_or(None);
                        let records: Option<i64> = r.try_get("last_run_records").unwrap_or(None);
                        let err: Option<String> = r.try_get("last_run_error").unwrap_or(None);
                        let sum: Option<String> = r.try_get("last_run_summary").unwrap_or(None);
                        serde_json::json!({
                            "status": s,
                            "started_at": at,
                            "records_processed": records,
                            "error": err,
                            "summary": sum,
                        })
                    });

                    serde_json::json!({
                        "id": id,
                        "owner": owner,
                        "name": name,
                        "agent": agent,
                        "cron_schedule": cron,
                        "enabled": enabled,
                        "config": config,
                        "condition": condition,
                        "triggers": triggers,
                        "memory": memory,
                        "credential_id": credential_id,
                        "runtime": runtime,
                        "command": command,
                        "created_at": created,
                        "updated_at": updated,
                        "is_system": owner == "system",
                        "last_run": last_run,
                    })
                })
                .collect();

            (StatusCode::OK, Json(actions)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// GET /api/actions/:id — single action with its last run inlined.
pub async fn get_action_handler(
    State(state): State<AppState>,
    Path(action_id): Path<String>,
) -> Response {
    let pool = state.db.pool();
    match crate::scheduler::actions::get_action(pool, &action_id).await {
        Ok(action) => {
            let last_run = crate::scheduler::actions::last_run(pool, &action_id)
                .await
                .ok()
                .flatten();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "id": action.id,
                    "owner": action.owner,
                    "name": action.name,
                    "agent": action.agent,
                    "cron_schedule": action.cron_schedule,
                    "enabled": action.enabled,
                    "config": action.config,
                    "condition": action.condition,
                    "triggers": action.triggers,
                    "memory": action.memory,
                    "command": action.command,
                    "credential_id": action.credential_id,
                    "runtime": action.runtime,
                    "created_at": action.created_at,
                    "updated_at": action.updated_at,
                    "is_system": action.owner == "system",
                    "last_run": last_run,
                })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// POST /api/actions — create a user-owned action.
#[derive(Debug, Deserialize)]
pub struct CreateActionBody {
    pub name: String,
    pub agent: Option<String>,
    pub cron_schedule: Option<String>,
    #[serde(default)]
    pub triggers: Option<Vec<String>>,
    pub config: Option<serde_json::Value>,
}

pub async fn create_action_handler(
    State(state): State<AppState>,
    Json(body): Json<CreateActionBody>,
) -> Response {
    let triggers = body.triggers.unwrap_or_else(|| {
        if body.cron_schedule.is_some() {
            vec!["cron".into(), "manual".into(), "tool".into()]
        } else {
            vec!["manual".into(), "tool".into()]
        }
    });

    match crate::scheduler::actions::create_user_action(
        state.db.pool(),
        None,
        &body.name,
        body.agent.as_deref(),
        body.cron_schedule.as_deref(),
        &triggers,
        body.config.as_ref(),
    )
    .await
    {
        Ok(action) => (StatusCode::CREATED, Json(action)).into_response(),
        Err(e) => {
            let status = match e.http_status() {
                400 => StatusCode::BAD_REQUEST,
                404 => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(serde_json::json!({ "error": e.to_string() }))).into_response()
        }
    }
}

/// PATCH /api/actions/:id — partial update. Enforces system-owner guard.
pub async fn patch_action_handler(
    State(state): State<AppState>,
    Path(action_id): Path<String>,
    Json(patch): Json<serde_json::Value>,
) -> Response {
    match crate::scheduler::actions::update_action(state.db.pool(), &action_id, &patch).await {
        Ok(action) => (StatusCode::OK, Json(action)).into_response(),
        Err(e) => {
            let status = match e.http_status() {
                400 => StatusCode::BAD_REQUEST,
                404 => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(serde_json::json!({ "error": e.to_string() }))).into_response()
        }
    }
}

/// DELETE /api/actions/:id — delete a user-owned action. System rows refused.
pub async fn delete_action_handler(
    State(state): State<AppState>,
    Path(action_id): Path<String>,
) -> Response {
    match crate::scheduler::actions::delete_action(state.db.pool(), &action_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            let status = match e.http_status() {
                400 => StatusCode::BAD_REQUEST,
                404 => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(serde_json::json!({ "error": e.to_string() }))).into_response()
        }
    }
}

/// GET /api/actions/:id/runs?limit=&offset= — paginated run history.
#[derive(Debug, Deserialize)]
pub struct RunsQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub status: Option<String>,
    pub action_id: Option<String>,
}

pub async fn list_action_runs_handler(
    State(state): State<AppState>,
    Path(action_id): Path<String>,
    axum::extract::Query(q): axum::extract::Query<RunsQuery>,
) -> Response {
    let limit = q.limit.unwrap_or(20).clamp(1, 200);
    match crate::scheduler::actions::query_runs(
        state.db.pool(),
        Some(&action_id),
        q.status.as_deref(),
        limit,
    )
    .await
    {
        Ok(runs) => (StatusCode::OK, Json(runs)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// GET /api/runs?status=&action_id=&limit=&offset= — global run history.
pub async fn list_runs_handler(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<RunsQuery>,
) -> Response {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    match crate::scheduler::actions::query_runs(
        state.db.pool(),
        q.action_id.as_deref(),
        q.status.as_deref(),
        limit,
    )
    .await
    {
        Ok(runs) => (StatusCode::OK, Json(runs)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

// ============================================================================
// Credentials API
// ============================================================================

/// GET /api/credentials — list all credentials (active + pending + revoked).
pub async fn list_credentials_handler(State(state): State<AppState>) -> Response {
    match crate::api::list_credentials(state.db.pool()).await {
        Ok(creds) => (StatusCode::OK, Json(creds)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

// ============================================================================
// Sources API
// ============================================================================

/// One catalog tile, derived from a `[[source]]` row in `actions/templates.toml`
/// plus live credential counts.
#[derive(Debug, Serialize)]
pub struct SourceCatalogItem {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub description: String,
    /// `'self_issued_bearer' | 'via_proxy' | 'api_key'`.
    pub auth_kind: &'static str,
    /// Number of `active` credentials (passwords) for this source.
    pub credential_count: i64,
}

/// GET /api/sources — catalog tiles for the Sources UI.
///
/// Reads from the `[[source]]` rows in `actions/templates.toml`. Adding a new
/// provider = a TOML edit; no code change. Frontend dispatches the Connect
/// button on `auth_kind`:
///
///   self_issued_bearer → DevicePairModal
///   via_proxy          → server-side redirect to the proxy
///   api_key            → text-input modal
pub async fn list_sources_handler(State(state): State<AppState>) -> Response {
    let pool = state.db.pool();

    let sources = crate::action_templates::list_sources_sorted();
    let mut items = Vec::with_capacity(sources.len());

    // One COUNT query per source — cheap; the catalog has at most a handful
    // of entries even at scale.
    for s in sources {
        let credential_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM credentials WHERE source_id = $1 AND status = 'active'",
        )
        .bind(&s.id)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

        items.push(SourceCatalogItem {
            id: s.id.clone(),
            name: s.display_name.clone(),
            icon: s.icon.clone(),
            description: s.description.clone(),
            auth_kind: s.auth.kind_str(),
            credential_count,
        });
    }

    (StatusCode::OK, Json(items)).into_response()
}


/// PATCH /api/credentials/:id — rename or toggle active.
#[derive(Debug, Deserialize)]
pub struct PatchCredentialBody {
    pub name: Option<String>,
    pub is_active: Option<bool>,
}

pub async fn patch_credential_handler(
    State(state): State<AppState>,
    Path(credential_id): Path<String>,
    Json(body): Json<PatchCredentialBody>,
) -> Response {
    let pool = state.db.pool();
    if let Some(name) = &body.name {
        if let Err(e) = crate::api::rename_credential(pool, &credential_id, name).await {
            let status = if e.http_status() == 404 {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_REQUEST
            };
            return (status, Json(serde_json::json!({ "error": e.to_string() }))).into_response();
        }
    }
    if let Some(active) = body.is_active {
        if !active {
            if let Err(e) = crate::api::revoke_credential(pool, &credential_id).await {
                let status = if e.http_status() == 404 {
                    StatusCode::NOT_FOUND
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                };
                return (status, Json(serde_json::json!({ "error": e.to_string() }))).into_response();
            }
        } else {
            // Re-activating is not supported via PATCH. A revoked device must
            // be re-paired via the QR / manual-link flow so a fresh
            // device_token and action_ids fan-out are generated.
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "re-activating a revoked credential requires re-pairing"
                })),
            )
                .into_response();
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

/// DELETE /api/credentials/:id
///
/// Dispatch by current status:
/// - `pending`  → hard-delete the row (used when the user cancels mid-pair
///   modal; the row never had a token or fan-out actions, so nothing to
///   preserve).
/// - `active`   → revoke (clear `secret_lookup_hash`, drop fan-out actions,
///   keep history with `action_id = NULL`).
/// - `revoked`  → already revoked, idempotent 204.
pub async fn delete_credential_handler(
    State(state): State<AppState>,
    Path(credential_id): Path<String>,
) -> Response {
    let pool = state.db.pool();

    let status: Option<(String,)> =
        match sqlx::query_as("SELECT status FROM credentials WHERE id = $1")
            .bind(&credential_id)
            .fetch_optional(pool)
            .await
        {
            Ok(s) => s,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
                    .into_response()
            }
        };

    let result = match status.as_ref().map(|s| s.0.as_str()) {
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "credential not found" })),
            )
                .into_response()
        }
        Some("pending") => crate::api::delete_pending_credential(pool, &credential_id).await,
        Some("revoked") => return StatusCode::NO_CONTENT.into_response(),
        _ => crate::api::revoke_credential(pool, &credential_id).await,
    };

    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            let status_code = if e.http_status() == 404 {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status_code, Json(serde_json::json!({ "error": e.to_string() }))).into_response()
        }
    }
}

// ============================================================================
// System (operator surface — running apps, logs, resources)
// ============================================================================

/// `GET /api/system/apps`
///
/// Snapshot of the `app`-runtime supervisor's registry. Used by the System
/// subtab on `/actions` to render running apps with status, port, PID,
/// restart count, and started-at timestamp.
pub async fn list_system_apps_handler(State(state): State<AppState>) -> Response {
    let apps = match &state.service_supervisor {
        Some(sup) => sup.registry.list().await,
        None => Vec::new(),
    };
    (StatusCode::OK, Json(apps)).into_response()
}

/// `GET /api/actions/:id/logs`
///
/// Returns the per-app captured stdout/stderr ring buffer (oldest → newest).
/// For `function`-runtime actions, returns an empty list (logs live in
/// `app_action_runs.error` / `result_summary` per run instead).
///
/// v1: JSON polling at ~1Hz from the frontend. SSE streaming is a v1.1 add.
pub async fn get_action_logs_handler(
    State(state): State<AppState>,
    axum::extract::Path(action_id): axum::extract::Path<String>,
) -> Response {
    let logs = match &state.service_supervisor {
        Some(sup) => sup.registry.logs(&action_id).await,
        None => Vec::new(),
    };
    (StatusCode::OK, Json(logs)).into_response()
}

// ============================================================================
// Admin
// ============================================================================

/// `POST /api/admin/reconcile`
///
/// Re-reads `actions/sources.toml` + every `actions/<name>/manifest.toml` from
/// disk, upserts `app_actions` rows accordingly, then asks the supervisor to
/// diff/spawn/stop `app`-runtime children.
///
/// This is the LLM-authoring on-ramp: an LLM creates a new action folder,
/// hits this endpoint, and the action is live without restarting core.
///
/// Response shape:
///   { "upserted": <count>, "added": [...], "removed": [...], "restarted": [...] }
pub async fn admin_reconcile_handler(State(state): State<AppState>) -> Response {
    // 1. Force a re-read of the on-disk catalog (sources.toml + per-action
    //    manifests). Subsequent lookup_source / list_sources_sorted /
    //    reconcile calls see the new data.
    crate::action_templates::reload_catalog();

    // 2. Reconcile `app_actions` SQL rows against the fresh catalog. Manifest
    //    fields overwrite for system actions; user-managed runtime state
    //    (enabled, cron_schedule, config) is preserved per the field-ownership
    //    rule documented in action_templates/mod.rs.
    let upserted = match crate::action_templates::reconcile_templates(state.db.pool()).await {
        Ok(n) => n,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("reconcile_templates failed: {e}")
                })),
            )
                .into_response();
        }
    };

    // 3. Diff/apply running apps. Supervisor stops apps no longer in the DB
    //    (or disabled), spawns newly-added ones, and (in v1.1) restarts ones
    //    whose command/config changed. Skipped if no supervisor (test setup).
    let outcome = match &state.service_supervisor {
        Some(sup) => match sup.reload(state.db.pool()).await {
            Ok(o) => o,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": format!("supervisor.reload failed: {e}")
                    })),
                )
                    .into_response();
            }
        },
        None => crate::services::supervisor::ReloadOutcome::default(),
    };

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "upserted": upserted,
            "added": outcome.added,
            "removed": outcome.removed,
            "restarted": outcome.restarted,
        })),
    )
        .into_response()
}

/// `POST /api/admin/actions/import-git`
///
/// Clone (or update) a Git repo into `actions/<slug>/` and reconcile so the
/// new manifests show up as `app_actions` rows. Same diff/spawn machinery
/// that `/api/admin/reconcile` uses; we just scope the per-row diff to the
/// slug prefix and clean up rows for manifests that disappeared upstream.
pub async fn import_git_actions_handler(
    State(state): State<AppState>,
    Json(body): Json<crate::action_git_import::ImportRequest>,
) -> Response {
    let outcome = match crate::action_git_import::import(state.db.pool(), body).await {
        Ok(o) => o,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    // Diff/apply running apps. Same supervisor reload Reconcile uses; if
    // there's no supervisor (test) we still return the import diff.
    if let Some(sup) = &state.service_supervisor {
        if let Err(e) = sup.reload(state.db.pool()).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("supervisor.reload failed: {e}")
                })),
            )
                .into_response();
        }
    }

    (StatusCode::OK, Json(outcome)).into_response()
}

// ============================================================================
// Developer API
// ============================================================================

/// Execute a read-only SQL query
pub async fn execute_sql_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::ExecuteSqlRequest>,
) -> Response {
    match crate::api::execute_sql(state.db.pool(), request).await {
        Ok(results) => (StatusCode::OK, Json(results)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// List all tables
pub async fn list_tables_handler(State(state): State<AppState>) -> Response {
    match crate::api::list_tables(state.db.pool()).await {
        Ok(tables) => (StatusCode::OK, Json(tables)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}


/// GET /api/devices/action-ids — devices refresh their action_id routing map.
///
/// Used by paired devices when their local routing table goes stale (e.g. after
/// templates.toml adds a new stream, or the device reinstalls and lost its
/// Keychain). Auth is the same `Bearer <device_token>` as `/webhook/{action_id}`.
pub async fn device_action_ids_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Response {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));

    let Some(token) = token else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Missing bearer token" })),
        )
            .into_response();
    };

    let credential_id = match crate::api::credentials::validate_device_token(state.db.pool(), token)
        .await
    {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Invalid or revoked device token" })),
            )
                .into_response();
        }
    };

    match virtues_helpers::auth::fanout_action_ids(state.db.pool(), &credential_id).await {
        Ok(action_ids) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "credential_id": credential_id,
                "action_ids": action_ids,
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// GET /api/devices/actions/:id/runs — a paired device reads the run history of
/// one of ITS OWN actions, so the app can show real server-side outcome
/// (success/failure/timing/error) per stream rather than just "did the POST
/// return 2xx."
///
/// Device-bearer auth, same as `/webhook/{action_id}` and
/// `/api/devices/action-ids`. The action's `credential_id` must match the
/// caller's credential or it's 403 — a token leaked from one device can't read
/// another device's run history. This is the device-scoped sibling of the
/// session-authed `list_action_runs_handler`.
pub async fn device_action_runs_handler(
    State(state): State<AppState>,
    Path(action_id): Path<String>,
    axum::extract::Query(q): axum::extract::Query<RunsQuery>,
    headers: axum::http::HeaderMap,
) -> Response {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));

    let Some(token) = token else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Missing bearer token" })),
        )
            .into_response();
    };

    let credential_id =
        match crate::api::credentials::validate_device_token(state.db.pool(), token).await {
            Ok(id) => id,
            Err(_) => {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({ "error": "Invalid or revoked device token" })),
                )
                    .into_response();
            }
        };

    // Ownership: the action must belong to this credential. EXISTS returns a
    // non-null bool, so a missing action and a foreign action both → false.
    let owned: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM app_actions WHERE id = $1 AND credential_id = $2)",
    )
    .bind(&action_id)
    .bind(&credential_id)
    .fetch_one(state.db.pool())
    .await
    .unwrap_or(false);

    if !owned {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "Action not found for this device" })),
        )
            .into_response();
    }

    let limit = q.limit.unwrap_or(10).clamp(1, 50);
    match crate::scheduler::actions::query_runs(
        state.db.pool(),
        Some(&action_id),
        q.status.as_deref(),
        limit,
    )
    .await
    {
        Ok(runs) => (StatusCode::OK, Json(runs)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// Health check endpoint for devices to validate their authentication
///
/// This lightweight endpoint allows devices to verify their token is still valid
/// without creating any side effects. Used for startup validation and periodic health checks.
pub async fn device_health_check_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Response {
    // Extract device token from Authorization header or X-Device-Token header
    let token = if let Some(value) = headers.get(axum::http::header::AUTHORIZATION) {
        let auth_str = value.to_str().unwrap_or("");
        if let Some(t) = auth_str.strip_prefix("Bearer ") {
            t.to_string()
        } else {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Invalid Authorization header format. Expected: Bearer <token>"
                })),
            )
                .into_response();
        }
    } else if let Some(value) = headers.get("X-Device-Token") {
        value.to_str().unwrap_or("").to_string()
    } else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "Missing authentication header. Provide Authorization: Bearer <token> or X-Device-Token: <token>"
            })),
        )
            .into_response();
    };

    // Validate the device token
    match crate::api::credentials::validate_device_token(state.db.pool(), &token).await {
        Ok(source_id) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "active",
                "source_id": source_id,
            })),
        )
            .into_response(),
        Err(_) => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "Invalid or revoked device token"
            })),
        )
            .into_response(),
    }
}

// =============================================================================
// Profile API
// =============================================================================

/// Get user profile
pub async fn get_profile_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::get_profile(state.db.pool()).await)
}

/// Update user profile
pub async fn update_profile_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::UpdateProfileRequest>,
) -> Response {
    api_response(crate::api::update_profile(state.db.pool(), request).await)
}

// =============================================================================
// Assistant Profile API
// =============================================================================

/// Get assistant profile
pub async fn get_assistant_profile_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::get_assistant_profile(state.db.pool()).await)
}

/// Update assistant profile
pub async fn update_assistant_profile_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::UpdateAssistantProfileRequest>,
) -> Response {
    api_response(crate::api::update_assistant_profile(state.db.pool(), request).await)
}

// =============================================================================
// Tools API
// =============================================================================

/// List all tools with optional filtering
pub async fn list_tools_handler(
    State(state): State<AppState>,
    Query(query): Query<crate::api::ListToolsQuery>,
) -> Response {
    api_response(crate::api::list_tools(state.db.pool(), query).await)
}

/// Get a specific tool by ID
pub async fn get_tool_handler(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    api_response(crate::api::get_tool(state.db.pool(), id).await)
}

// Note: Built-in tools cannot be updated. They are read-only from the registry.
// MCP tools can be managed via separate endpoints (to be implemented).

// =============================================================================
// Models API
// =============================================================================

/// List all available models
pub async fn list_models_handler() -> Response {
    api_response(crate::api::list_models().await)
}

/// Get a specific model by ID
pub async fn get_model_handler(Path(model_id): Path<String>) -> Response {
    api_response(crate::api::get_model(&model_id).await)
}

/// List recommended models with slot assignments
pub async fn list_recommended_models_handler() -> Response {
    api_response(crate::api::list_recommended_models().await)
}

// =============================================================================
// Agents API
// =============================================================================

/// List all available agents
pub async fn list_agents_handler() -> Response {
    api_response(crate::api::list_agents().await)
}

/// Get a specific agent by ID
pub async fn get_agent_handler(Path(agent_id): Path<String>) -> Response {
    api_response(crate::api::get_agent(&agent_id).await)
}

// =============================================================================
// Personas API
// =============================================================================

/// List all personas (excluding hidden ones)
pub async fn list_personas_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_personas(state.db.pool()).await)
}

/// Get a specific persona by ID
pub async fn get_persona_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::get_persona(state.db.pool(), &id).await)
}

/// Create a new custom persona
pub async fn create_persona_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreatePersonaRequest>,
) -> Response {
    api_response(crate::api::create_persona(state.db.pool(), request).await)
}

/// Update an existing persona
pub async fn update_persona_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::UpdatePersonaRequest>,
) -> Response {
    api_response(crate::api::update_persona(state.db.pool(), &id, request).await)
}

/// Hide a persona (soft delete for system, hard delete for custom)
pub async fn hide_persona_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::hide_persona(state.db.pool(), &id).await)
}

/// Unhide a previously hidden persona
pub async fn unhide_persona_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::unhide_persona(state.db.pool(), &id).await)
}

/// Reset personas to defaults (re-seed from registry)
pub async fn reset_personas_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::reset_personas(state.db.pool()).await)
}

// =============================================================================
// Seed Testing API
// =============================================================================

/// Get pipeline status (archive, transform, clustering)
pub async fn seed_pipeline_status_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::get_pipeline_status(&state.db).await)
}

#[derive(Debug, serde::Deserialize)]
pub struct DataQualityQuery {
    pub start: String, // RFC3339 timestamp
    pub end: String,   // RFC3339 timestamp
}

/// Get data quality metrics for seed data
pub async fn seed_data_quality_handler(
    State(state): State<AppState>,
    Query(params): Query<DataQualityQuery>,
) -> Response {
    let start = match chrono::DateTime::parse_from_rfc3339(&params.start) {
        Ok(dt) => dt.with_timezone(&chrono::Utc),
        Err(_) => {
            return error_response(crate::error::Error::InvalidInput(
                "Invalid start timestamp. Use RFC3339 format".to_string(),
            ))
        }
    };

    let end = match chrono::DateTime::parse_from_rfc3339(&params.end) {
        Ok(dt) => dt.with_timezone(&chrono::Utc),
        Err(_) => {
            return error_response(crate::error::Error::InvalidInput(
                "Invalid end timestamp. Use RFC3339 format".to_string(),
            ))
        }
    };

    api_response(crate::api::get_data_quality_metrics(&state.db, start, end).await)
}

// ============================================================================
// Metrics handlers
// ============================================================================

/// Get activity metrics (job statistics, time windows, recent errors)
pub async fn get_activity_metrics_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::get_activity_metrics(&state.db).await)
}

// Plaid Link handlers were removed in the actions cutover.

// ============================================================================
// Onboarding API
// ============================================================================

// =============================================================================
// Places API Handlers (Google Places proxy)
// =============================================================================

/// Get autocomplete predictions for an address query
pub async fn places_autocomplete_handler(
    State(state): State<AppState>,
    Query(request): Query<crate::api::AutocompleteRequest>,
) -> Response {
    // Check usage limit first
    if let Err(e) =
        crate::api::check_limit(state.db.pool(), crate::api::Service::GooglePlaces).await
    {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": "usage_limit_exceeded",
                "service": e.service,
                "used": e.used,
                "limit": e.limit,
                "unit": e.unit,
                "resets_at": e.resets_at,
                "message": format!("Monthly Google Places limit reached. Resets at {}", e.resets_at)
            })),
        )
            .into_response();
    }

    match crate::api::autocomplete(state.db.pool(), request).await {
        Ok(response) => {
            // Record usage on success - warn but don't fail if recording fails
            // The user already received their response, so this is a billing/tracking issue only
            if let Err(e) = crate::api::record_service_usage(
                state.db.pool(),
                crate::api::Service::GooglePlaces,
                1,
            )
            .await
            {
                tracing::warn!(
                    service = "google_places",
                    error = %e,
                    "Usage recording failed - request succeeded but usage may be undercounted"
                );
            }
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => error_response(e),
    }
}

/// Get details for a specific place by ID
pub async fn places_details_handler(
    State(state): State<AppState>,
    Query(request): Query<crate::api::PlaceDetailsRequest>,
) -> Response {
    // Check usage limit first
    if let Err(e) =
        crate::api::check_limit(state.db.pool(), crate::api::Service::GooglePlaces).await
    {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": "usage_limit_exceeded",
                "service": e.service,
                "used": e.used,
                "limit": e.limit,
                "unit": e.unit,
                "resets_at": e.resets_at,
                "message": format!("Monthly Google Places limit reached. Resets at {}", e.resets_at)
            })),
        )
            .into_response();
    }

    match crate::api::get_place_details(state.db.pool(), request).await {
        Ok(response) => {
            // Record usage on success - warn but don't fail if recording fails
            if let Err(e) = crate::api::record_service_usage(
                state.db.pool(),
                crate::api::Service::GooglePlaces,
                1,
            )
            .await
            {
                tracing::warn!(
                    service = "google_places",
                    error = %e,
                    "Usage recording failed - request succeeded but usage may be undercounted"
                );
            }
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => error_response(e),
    }
}

// =============================================================================
// Usage API Handlers
// =============================================================================

/// Get usage summary for all services
pub async fn usage_handler(State(state): State<AppState>) -> Response {
    match crate::api::get_all_usage(state.db.pool()).await {
        Ok(summary) => (StatusCode::OK, Json(summary)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("Failed to get usage: {}", e) })),
        )
            .into_response(),
    }
}

/// Check remaining usage for a service
#[derive(Debug, Deserialize)]
pub struct UsageCheckQuery {
    pub service: String,
}

pub async fn usage_check_handler(
    State(state): State<AppState>,
    Query(query): Query<UsageCheckQuery>,
) -> Response {
    let service = match query.service.as_str() {
        "ai_gateway" => crate::api::Service::AiGateway,
        "google_places" => crate::api::Service::GooglePlaces,
        "exa" => crate::api::Service::Exa,
        _ => {
            return error_response(crate::error::Error::InvalidInput(format!(
                "Invalid service: {}. Valid services: ai_gateway, google_places, exa",
                query.service
            )))
        }
    };

    match crate::api::check_limit(state.db.pool(), service).await {
        Ok(remaining) => (StatusCode::OK, Json(remaining)).into_response(),
        Err(e) => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": "usage_limit_exceeded",
                "service": e.service,
                "used": e.used,
                "limit": e.limit,
                "unit": e.unit,
                "resets_at": e.resets_at,
                "message": format!("Monthly {} limit reached. Resets at {}", e.service, e.resets_at)
            })),
        )
            .into_response(),
    }
}

// =============================================================================
// Subscription & Billing API Handlers
// =============================================================================

/// GET /api/subscription - Local subscription signal (voucher model).
///
/// Derived from the credential vault: reports whether a billing token has
/// been claimed on this box. Gating itself is by bearer expiry, not this
/// endpoint — see `crate::api::subscription`.
pub async fn get_subscription_handler(State(pool): State<sqlx::PgPool>) -> Response {
    match crate::api::subscription::get_subscription_status(&pool).await {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => {
            tracing::debug!("Subscription status check failed: {}", e);
            // Safe fallback so the app works even if the vault read hiccups.
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "active",
                    "trial_expires_at": null,
                    "days_remaining": null,
                    "is_active": true
                })),
            )
                .into_response()
        }
    }
}

/// POST /api/billing/portal - Stripe billing portal.
///
/// The portal belongs to Atlas (which holds the Stripe customer). We read the
/// billing token from the local vault, ask Atlas to mint a Stripe-hosted
/// Customer Portal session, and return its `url` for BillingView to open.
/// Any failure (no billing token yet, inactive subscription, Stripe hiccup)
/// returns a clean `{error}` string the button renders inline — never a 500.
pub async fn create_billing_portal_handler(State(pool): State<sqlx::PgPool>) -> Response {
    let billing_token = match crate::virtues_api::renew::read_billing_token(&pool).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            return (
                StatusCode::OK,
                Json(serde_json::json!({
                    "error": "Connect your subscription first, then you can manage billing here."
                })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::warn!("billing portal: vault read failed: {e}");
            return (
                StatusCode::OK,
                Json(serde_json::json!({ "error": "Couldn't open the billing portal. Try again." })),
            )
                .into_response();
        }
    };

    let atlas_url =
        std::env::var("VIRTUES_ATLAS_URL").unwrap_or_else(|_| "http://localhost:9100".to_string());
    // The box has no stable public URL, so we don't supply a return_url —
    // Atlas defaults it to its own public billing page (where Stripe sends the
    // customer after they click "Return to Virtues").
    let http = crate::http_client::virtues_api_client();

    match crate::virtues_api::renew::fetch_portal_session(&http, &atlas_url, &billing_token, "")
        .await
    {
        Ok(url) => (StatusCode::OK, Json(serde_json::json!({ "url": url }))).into_response(),
        Err(e) => {
            tracing::warn!("billing portal session failed: {e}");
            (
                StatusCode::OK,
                Json(serde_json::json!({ "error": "Couldn't open the billing portal. Try again." })),
            )
                .into_response()
        }
    }
}

#[derive(serde::Deserialize)]
pub struct ClaimRequest {
    /// Stripe Checkout `session_id` from the post-purchase success URL.
    pub session_id: String,
}

/// POST /api/billing/claim — one-time onboarding step.
///
/// Exchanges the Stripe checkout `session_id` for a long-lived billing token
/// (via Atlas `/claim`) and stores it in the local credential vault. This is
/// the only place the customer↔device link is established, and it lives only
/// on this box. We then eagerly mint the first monthly bearer so AI works
/// immediately; if that renewal fails it's non-fatal — the lazy
/// `bearer_expired` → renew path will mint one on the first AI call.
pub async fn claim_billing_handler(
    State(pool): State<sqlx::PgPool>,
    Json(req): Json<ClaimRequest>,
) -> Response {
    let atlas_url =
        std::env::var("VIRTUES_ATLAS_URL").unwrap_or_else(|_| "http://localhost:9100".to_string());
    let http = crate::http_client::virtues_api_client();

    let claim = match crate::virtues_api::renew::claim(&http, &atlas_url, &req.session_id).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("billing claim failed: {e}");
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    if let Err(e) = crate::virtues_api::renew::store_billing_token(&pool, &claim.billing_token).await
    {
        tracing::error!("failed to store billing token: {e}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "failed to store billing token" })),
        )
            .into_response();
    }

    // Eager first-bearer mint (best-effort).
    let api_url =
        std::env::var("VIRTUES_API_URL").unwrap_or_else(|_| "http://localhost:9002".to_string());
    let bearer_ready =
        match crate::virtues_api::renew::renew(&pool, &http, &atlas_url, &api_url).await {
            Ok(_) => true,
            Err(e) => {
                tracing::warn!("eager bearer mint after claim failed (lazy renew will retry): {e}");
                false
            }
        };

    (
        StatusCode::OK,
        Json(serde_json::json!({ "claimed": true, "bearer_ready": bearer_ready })),
    )
        .into_response()
}

/// POST /api/billing/link/start — begin the device-authorization link flow.
///
/// The web "Connect subscription" button calls this, then opens the returned
/// `verification_uri_complete` and polls `link/status`. The secret device_code
/// stays box-side; only the user-facing bits are returned to the browser.
pub async fn billing_link_start_handler(State(pool): State<sqlx::PgPool>) -> Response {
    let atlas_url =
        std::env::var("VIRTUES_ATLAS_URL").unwrap_or_else(|_| "http://localhost:9100".to_string());
    let http = crate::http_client::virtues_api_client();
    match crate::virtues_api::link::start(&pool, &http, &atlas_url).await {
        Ok(s) => (StatusCode::OK, Json(serde_json::json!(s))).into_response(),
        Err(e) => {
            tracing::warn!("billing link start failed: {e}");
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}

/// GET /api/billing/link/status — poll the in-flight link. On `ready` this
/// stores the billing token and mints the first bearer as a side effect.
pub async fn billing_link_status_handler(State(pool): State<sqlx::PgPool>) -> Response {
    let atlas_url =
        std::env::var("VIRTUES_ATLAS_URL").unwrap_or_else(|_| "http://localhost:9100".to_string());
    let api_url =
        std::env::var("VIRTUES_API_URL").unwrap_or_else(|_| "http://localhost:9002".to_string());
    let http = crate::http_client::virtues_api_client();
    match crate::virtues_api::link::poll(&pool, &http, &atlas_url, &api_url).await {
        Ok(status) => (StatusCode::OK, Json(serde_json::json!({ "status": status }))).into_response(),
        Err(e) => {
            tracing::warn!("billing link status failed: {e}");
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}

// =============================================================================
// System Update API Handlers
// =============================================================================

// =============================================================================
// Exa Search API Handlers
// =============================================================================

/// Perform a web search using Exa AI
pub async fn exa_search_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::ExaSearchRequest>,
) -> Response {
    // Check usage limit first
    if let Err(e) = crate::api::check_limit(state.db.pool(), crate::api::Service::Exa).await {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": "usage_limit_exceeded",
                "service": e.service,
                "used": e.used,
                "limit": e.limit,
                "unit": e.unit,
                "resets_at": e.resets_at,
                "message": format!("Monthly Exa search limit reached. Resets at {}", e.resets_at)
            })),
        )
            .into_response();
    }

    // Perform the search
    match crate::api::exa_search(state.db.pool(), request).await {
        Ok(response) => {
            // Record usage on success - warn but don't fail if recording fails
            if let Err(e) =
                crate::api::record_service_usage(state.db.pool(), crate::api::Service::Exa, 1).await
            {
                tracing::warn!(
                    service = "exa",
                    error = %e,
                    "Usage recording failed - request succeeded but usage may be undercounted"
                );
            }
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => error_response(e),
    }
}

// =============================================================================
// Unsplash API Handler
// =============================================================================

/// Search Unsplash photos for cover images
pub async fn unsplash_search_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::UnsplashSearchRequest>,
) -> Response {
    match crate::api::unsplash_search(state.db.pool(), request).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => error_response(e),
    }
}

// =============================================================================
// Storage API Handlers
// =============================================================================

/// Query parameters for listing storage objects
#[derive(Debug, Deserialize)]
pub struct ListStorageObjectsParams {
    pub limit: Option<i64>,
}

/// List recent storage objects
pub async fn list_storage_objects_handler(
    State(state): State<AppState>,
    Query(params): Query<ListStorageObjectsParams>,
) -> Response {
    let limit = params.limit.unwrap_or(10);
    api_response(crate::api::list_recent_objects(state.db.pool(), limit).await)
}

/// Get decrypted content of a storage object
pub async fn get_storage_object_content_handler(
    State(state): State<AppState>,
    Path(object_id): Path<String>,
) -> Response {
    api_response(crate::api::get_object_content(state.db.pool(), &*state.storage, object_id).await)
}

// =============================================================================
// Entities API - Places
// =============================================================================

/// List all known places
pub async fn list_places_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_places(state.db.pool()).await)
}

/// Get a specific place by ID
pub async fn get_place_handler(
    State(state): State<AppState>,
    Path(place_id): Path<String>,
) -> Response {
    api_response(crate::api::get_place(state.db.pool(), place_id).await)
}

/// Create a new place
pub async fn create_place_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreatePlaceRequest>,
) -> Response {
    match crate::api::create_place(state.db.pool(), request).await {
        Ok(response) => (StatusCode::CREATED, Json(response)).into_response(),
        Err(e) => error_response(e),
    }
}

/// Update an existing place
pub async fn update_place_handler(
    State(state): State<AppState>,
    Path(place_id): Path<String>,
    Json(request): Json<crate::api::UpdatePlaceRequest>,
) -> Response {
    api_response(crate::api::update_place(state.db.pool(), place_id, request).await)
}

/// Delete a place
pub async fn delete_place_handler(
    State(state): State<AppState>,
    Path(place_id): Path<String>,
) -> Response {
    match crate::api::delete_place(state.db.pool(), place_id).await {
        Ok(_) => success_message("Place deleted successfully"),
        Err(e) => error_response(e),
    }
}

/// Set a place as the user's home
pub async fn set_place_as_home_handler(
    State(state): State<AppState>,
    Path(place_id): Path<String>,
) -> Response {
    match crate::api::set_home_place_entity(state.db.pool(), place_id).await {
        Ok(_) => success_message("Home place updated"),
        Err(e) => error_response(e),
    }
}

// ============================================================================
// Wiki API Handlers
// ============================================================================

/// Resolve an entity ID to its type
pub async fn wiki_resolve_id_handler(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::resolve_id(&id))
}

// --- Person ---

/// Get a person by ID
pub async fn wiki_get_person_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::get_person(state.db.pool(), id).await)
}

/// List all people
pub async fn wiki_list_people_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_people(state.db.pool()).await)
}

/// Update a person by ID
pub async fn wiki_update_person_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::UpdateWikiPersonRequest>,
) -> Response {
    api_response(crate::api::update_person(state.db.pool(), id, request).await)
}

// --- Place ---

/// Get a place by ID
pub async fn wiki_get_place_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::get_wiki_place(state.db.pool(), id).await)
}

/// List all places (wiki view)
pub async fn wiki_list_places_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_wiki_places(state.db.pool()).await)
}

/// Update a place by ID (wiki fields)
pub async fn wiki_update_place_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::UpdateWikiPlaceRequest>,
) -> Response {
    api_response(crate::api::update_wiki_place(state.db.pool(), id, request).await)
}

// --- Organization ---

/// Get an organization by ID
pub async fn wiki_get_organization_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::get_organization(state.db.pool(), id).await)
}

/// List all organizations
pub async fn wiki_list_organizations_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_organizations(state.db.pool()).await)
}

/// Update an organization by ID
pub async fn wiki_update_organization_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::UpdateWikiOrganizationRequest>,
) -> Response {
    api_response(crate::api::update_organization(state.db.pool(), id, request).await)
}

// --- Thing ---

/// Get a thing by ID
pub async fn wiki_get_thing_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::get_thing(state.db.pool(), id).await)
}

/// List all things
pub async fn wiki_list_things_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_things(state.db.pool()).await)
}

/// Update a thing by ID
pub async fn wiki_update_thing_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::UpdateWikiThingRequest>,
) -> Response {
    api_response(crate::api::update_thing(state.db.pool(), id, request).await)
}

// --- Narrative Identity ---

/// Get narrative identity
pub async fn wiki_get_narrative_identity_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::get_narrative_identity(state.db.pool()).await)
}

/// Update narrative identity
pub async fn wiki_update_narrative_identity_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::UpdateNarrativeIdentityRequest>,
) -> Response {
    api_response(crate::api::update_narrative_identity(state.db.pool(), request).await)
}

// --- Telos ---

/// Get active telos
pub async fn wiki_get_active_telos_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::get_active_telos(state.db.pool()).await)
}

/// Get a telos by ID
pub async fn wiki_get_telos_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::get_telos(state.db.pool(), &id).await)
}

// --- Act ---

/// Get an act by ID
pub async fn wiki_get_act_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::get_act(state.db.pool(), id).await)
}

/// List all acts
pub async fn wiki_list_acts_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_acts(state.db.pool()).await)
}

// --- Chapter ---

/// Get a chapter by ID
pub async fn wiki_get_chapter_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::get_chapter(state.db.pool(), id).await)
}

/// List chapters for an act
pub async fn wiki_list_chapters_handler(
    State(state): State<AppState>,
    Path(act_id): Path<String>,
) -> Response {
    api_response(crate::api::list_chapters_for_act(state.db.pool(), act_id).await)
}

// --- Day ---

#[derive(Deserialize)]
pub struct WikiDayQuery {
    pub start_date: Option<chrono::NaiveDate>,
    pub end_date: Option<chrono::NaiveDate>,
}

/// Get a day by date
pub async fn wiki_get_day_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => {
            api_response(crate::api::get_or_create_day(state.db.pool(), parsed_date).await)
        }
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}

/// Serve a day's illustration as a PNG image.
/// Returns 200 with image/png body if the illustration exists, 404 otherwise.
pub async fn wiki_get_day_illustration_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    let parsed = match date.parse::<chrono::NaiveDate>() {
        Ok(d) => d,
        Err(_) => {
            return error_response(Error::InvalidInput(format!("Invalid date: {date}")));
        }
    };
    match crate::api::wiki::get_day_illustration(state.db.pool(), parsed).await {
        Ok(Some(bytes)) => {
            use axum::http::header;
            (
                axum::http::StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "image/png"),
                    (header::CACHE_CONTROL, "public, max-age=86400, immutable"),
                ],
                bytes,
            )
                .into_response()
        }
        Ok(None) => (axum::http::StatusCode::NOT_FOUND, "no illustration").into_response(),
        Err(e) => error_response(e),
    }
}

/// Update a day by date
pub async fn wiki_update_day_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
    Json(request): Json<crate::api::UpdateWikiDayRequest>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => {
            api_response(crate::api::update_day(state.db.pool(), parsed_date, request).await)
        }
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}

/// List days in a date range
pub async fn wiki_list_days_handler(
    State(state): State<AppState>,
    Query(query): Query<WikiDayQuery>,
) -> Response {
    let today = chrono::Utc::now().date_naive();
    let start_date = query
        .start_date
        .unwrap_or(today - chrono::Duration::days(30));
    let end_date = query.end_date.unwrap_or(today);
    api_response(crate::api::list_days(state.db.pool(), start_date, end_date).await)
}

// =============================================================================
// =============================================================================
// Wiki Temporal Events API
// =============================================================================

/// Get events for a day by date
pub async fn wiki_get_day_events_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => {
            api_response(crate::api::get_events_by_date(state.db.pool(), parsed_date).await)
        }
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}

/// Create a temporal event
pub async fn wiki_create_event_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreateTemporalEventRequest>,
) -> Response {
    match crate::api::create_temporal_event(state.db.pool(), request).await {
        Ok(event) => (StatusCode::CREATED, Json(event)).into_response(),
        Err(e) => error_response(e),
    }
}

/// Update a temporal event
pub async fn wiki_update_event_handler(
    State(state): State<AppState>,
    Path(event_id): Path<String>,
    Json(request): Json<crate::api::UpdateTemporalEventRequest>,
) -> Response {
    api_response(crate::api::update_temporal_event(state.db.pool(), event_id, request).await)
}

/// Delete a temporal event
pub async fn wiki_delete_event_handler(
    State(state): State<AppState>,
    Path(event_id): Path<String>,
) -> Response {
    match crate::api::delete_temporal_event(state.db.pool(), event_id).await {
        Ok(_) => success_message("Event deleted"),
        Err(e) => error_response(e),
    }
}

/// Delete all auto-generated events for a day (regeneration support)
pub async fn wiki_delete_auto_events_handler(
    State(state): State<AppState>,
    Path(day_id): Path<String>,
) -> Response {
    match crate::api::delete_auto_events_for_day(state.db.pool(), day_id).await {
        Ok(count) => (
            StatusCode::OK,
            Json(serde_json::json!({ "deleted": count })),
        )
            .into_response(),
        Err(e) => error_response(e),
    }
}

/// Get timeline location chunks for a day (movement map)
pub async fn timeline_get_day_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => {
            api_response(crate::api::get_timeline_day(state.db.pool(), parsed_date).await)
        }
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}

/// Get data sources (ontology records) for a day
pub async fn wiki_get_day_sources_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => {
            api_response(crate::api::get_day_sources(state.db.pool(), parsed_date).await)
        }
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}

/// Get AI chats (in-app Virtues + external imported) for a day
pub async fn wiki_get_day_chats_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => {
            api_response(crate::api::get_day_chats(state.db.pool(), parsed_date).await)
        }
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}

/// Get all ontology data streams for a day (dynamic query across all ontologies)
pub async fn wiki_get_day_streams_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => {
            api_response(crate::api::get_day_streams(state.db.pool(), parsed_date).await)
        }
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}


// =============================================================================
// Code Execution API (AI Sandbox)
// =============================================================================

/// Execute Python code in a sandboxed environment
///
/// Used by the AI agent's code_interpreter tool.
/// On Linux, uses nsjail for process isolation.
/// On dev machines (macOS/Windows), runs Python directly.
pub async fn execute_code_handler(Json(request): Json<crate::api::ExecuteCodeRequest>) -> Response {
    let response = crate::api::execute_code(request).await;
    (StatusCode::OK, Json(response)).into_response()
}

// =============================================================================
// Chat Usage & Compaction API Handlers
// =============================================================================

/// Get token usage for a chat
pub async fn get_chat_usage_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
) -> Response {
    api_response(crate::api::get_chat_usage(state.db.pool(), chat_id).await)
}

/// Compact a chat (summarize older messages)
pub async fn compact_chat_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    Json(request): Json<Option<CompactChatRequest>>,
) -> Response {
    let options = request.unwrap_or_default().into();
    api_response(crate::api::compaction::compact_chat(state.db.pool(), chat_id, options).await)
}

/// Request body for compaction
#[derive(Debug, Deserialize, Default)]
pub struct CompactChatRequest {
    /// Number of recent exchanges to keep verbatim (default: 4)
    pub keep_recent_exchanges: Option<usize>,
    /// Force compaction even if under threshold
    #[serde(default)]
    pub force: bool,
}

impl From<CompactChatRequest> for crate::api::compaction::CompactionOptions {
    fn from(req: CompactChatRequest) -> Self {
        let default_opts = crate::api::compaction::CompactionOptions::default();
        Self {
            keep_recent_exchanges: req
                .keep_recent_exchanges
                .unwrap_or(default_opts.keep_recent_exchanges),
            force: req.force,
            model_id: None, // API compaction uses default model context window
        }
    }
}

// =============================================================================
// Chats API Handlers
// =============================================================================

/// List chats
pub async fn list_chats_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::chats::list_chats(state.db.pool(), 25).await)
}

/// Create a new chat with initial messages
pub async fn create_chat_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::chats::CreateChatRequest>,
) -> Response {
    api_response(crate::api::chats::create_chat_from_request(state.db.pool(), request).await)
}

/// Get a chat by ID
pub async fn get_chat_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
) -> Response {
    api_response(crate::api::chats::get_chat(state.db.pool(), chat_id).await)
}

/// Update a chat (title and/or icon)
pub async fn update_chat_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    Json(request): Json<crate::api::chats::UpdateChatRequest>,
) -> Response {
    api_response(
        crate::api::chats::update_chat(state.db.pool(), chat_id, &request).await,
    )
}

/// Delete a chat
pub async fn delete_chat_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
) -> Response {
    api_response(crate::api::chats::delete_chat(state.db.pool(), chat_id).await)
}

/// Generate a title for a chat
pub async fn generate_chat_title_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::chats::GenerateTitleRequest>,
) -> Response {
    api_response(
        crate::api::chats::generate_title(state.db.pool(), request.chat_id, &request.messages)
            .await,
    )
}

// =============================================================================
// Chat API Handler
// =============================================================================

/// POST /api/chat - Stream chat completion (requires authentication)
pub async fn chat_handler(
    State(state): State<AppState>,
    user: crate::middleware::auth::AuthUser,
    Json(request): Json<crate::api::chat::ChatRequest>,
) -> Response {
    crate::api::chat::chat_handler(
        axum::extract::State(state.db.pool().clone()),
        axum::extract::State(state.yjs_state.clone()),
        axum::extract::State(state.chat_cancel_state.clone()),
        user,
        Json(request),
    )
    .await
}

/// POST /api/chat/cancel - Cancel an in-progress chat request
pub async fn cancel_chat_handler(
    State(state): State<AppState>,
    user: crate::middleware::auth::AuthUser,
    Json(request): Json<crate::api::chat::CancelChatRequest>,
) -> impl IntoResponse {
    crate::api::chat::cancel_chat_handler(
        axum::extract::State(state.chat_cancel_state.clone()),
        user,
        Json(request),
    )
    .await
}

// =============================================================================
// Chat Edit Permissions API Handlers
// =============================================================================

/// GET /api/chats/:id/permissions - List edit permissions for a chat
pub async fn list_chat_permissions_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
) -> Response {
    api_response(crate::api::chat_permissions::list_permissions(state.db.pool(), &chat_id).await)
}

/// POST /api/chats/:id/permissions - Add an edit permission
pub async fn add_chat_permission_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    Json(request): Json<crate::api::chat_permissions::AddPermissionRequest>,
) -> Response {
    api_response(
        crate::api::chat_permissions::add_permission(state.db.pool(), &chat_id, request).await,
    )
}

/// DELETE /api/chats/:id/permissions/:entity_id - Remove an edit permission
pub async fn remove_chat_permission_handler(
    State(state): State<AppState>,
    Path((chat_id, entity_id)): Path<(String, String)>,
) -> Response {
    match crate::api::chat_permissions::remove_permission(state.db.pool(), &chat_id, &entity_id)
        .await
    {
        Ok(_) => success_message("Permission removed"),
        Err(e) => error_response(e),
    }
}

// =============================================================================
// Auth API Handlers
// =============================================================================

/// POST /auth/signout — sign out of this tab; device row untouched.
pub async fn auth_signout_handler(
    State(state): State<AppState>,
    jar: axum_extra::extract::cookie::CookieJar,
) -> Response {
    crate::api::auth::signout_handler(axum::extract::State(state.db.pool().clone()), jar)
        .await
        .into_response()
}

/// GET /auth/session — current session (or null if not paired).
///
/// Loopback peers (localhost — the box's own browser, or dev via the vite proxy)
/// are auto-authenticated as the owner, mirroring the `AuthUser` extractor's
/// loopback bypass — INCLUDING the same forwarding-header guard: a reverse
/// proxy also connects from loopback, so a proxied request (carrying
/// `X-Forwarded-For` / `Forwarded`) must NOT be reported as owner. Without this,
/// the probe would tell a proxied remote browser it's the owner. Remote peers
/// fall through to the cookie check.
pub async fn auth_session_handler(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    jar: axum_extra::extract::cookie::CookieJar,
) -> Response {
    let is_proxied =
        headers.contains_key("x-forwarded-for") || headers.contains_key("forwarded");
    if addr.ip().is_loopback() && !is_proxied {
        return axum::Json(crate::api::auth::SessionResponse {
            user: Some(crate::api::auth::SessionUser {
                id: crate::middleware::http::OWNER_USER_ID.to_string(),
                device_id: crate::middleware::auth::CONSOLE_DEVICE_ID.to_string(),
                device_label: crate::middleware::auth::CONSOLE_DEVICE_LABEL.to_string(),
            }),
            expires: None,
        })
        .into_response();
    }
    crate::api::auth::session_handler(axum::extract::State(state.db.pool().clone()), headers, jar)
        .await
        .into_response()
}

// =============================================================================
// Drive API Handlers (User File Storage)
// =============================================================================

/// GET /api/drive/usage - Get drive usage statistics
pub async fn get_drive_usage_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::get_drive_usage(state.db.pool()).await)
}

/// GET /api/drive/warnings - Get quota warnings
pub async fn get_drive_warnings_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::check_drive_warnings(state.db.pool()).await)
}

/// Query params for listing drive files
#[derive(Debug, Deserialize)]
pub struct ListDriveFilesQuery {
    #[serde(default = "default_drive_path")]
    pub path: String,
}

fn default_drive_path() -> String {
    String::new() // Empty string = root directory
}

/// GET /api/drive/files - List files in a directory
pub async fn list_drive_files_handler(
    State(state): State<AppState>,
    Query(params): Query<ListDriveFilesQuery>,
) -> Response {
    api_response(crate::api::list_drive_files(state.db.pool(), &params.path).await)
}

/// GET /api/drive/files/:id - Get file metadata
pub async fn get_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    api_response(crate::api::get_drive_file(state.db.pool(), &file_id).await)
}

/// GET /api/drive/files/:id/download - Download file content
pub async fn download_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    // Lake objects use in-memory download (different storage layer)
    if crate::api::is_lake_object_id(&file_id) {
        let result =
            crate::api::download_lake_object(state.db.pool(), &state.storage, &file_id).await;
        return match result {
            Ok((file, content)) => {
                let content_type = file
                    .mime_type
                    .unwrap_or_else(|| "application/octet-stream".to_string());
                let filename = sanitize_content_disposition(&file.filename);
                (
                    [
                        (axum::http::header::CONTENT_TYPE, content_type),
                        (
                            axum::http::header::CONTENT_DISPOSITION,
                            format!("attachment; filename=\"{}\"", filename),
                        ),
                        (
                            axum::http::header::CONTENT_LENGTH,
                            content.len().to_string(),
                        ),
                    ],
                    content,
                )
                    .into_response()
            }
            Err(e) => error_response(e),
        };
    }

    // Regular drive files: stream from storage
    let result =
        crate::api::download_drive_file_stream(state.db.pool(), &state.drive_config, &file_id)
            .await;
    match result {
        Ok((file, stream)) => {
            let content_type = file
                .mime_type
                .unwrap_or_else(|| "application/octet-stream".to_string());
            let filename = sanitize_content_disposition(&file.filename);
            (
                [
                    (axum::http::header::CONTENT_TYPE, content_type),
                    (
                        axum::http::header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{}\"", filename),
                    ),
                    (
                        axum::http::header::CONTENT_LENGTH,
                        file.size_bytes.to_string(),
                    ),
                ],
                Body::from_stream(stream),
            )
                .into_response()
        }
        Err(e) => error_response(e),
    }
}

/// DELETE /api/drive/files/:id - Delete a file or folder
pub async fn delete_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    match crate::api::delete_drive_file(state.db.pool(), &state.drive_config, &file_id).await {
        Ok(_) => success_message("File deleted"),
        Err(e) => error_response(e),
    }
}

/// PUT /api/drive/files/:id/move - Move or rename a file
pub async fn move_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
    Json(request): Json<crate::api::DriveMoveFileRequest>,
) -> Response {
    api_response(
        crate::api::move_drive_file(
            state.db.pool(),
            &state.drive_config,
            &file_id,
            &request.new_path,
        )
        .await,
    )
}

/// POST /api/drive/upload - Upload a file (multipart form)
pub async fn upload_drive_file_handler(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Response {
    // Parse multipart form
    let mut path: Option<String> = None;
    let mut filename: Option<String> = None;
    let mut mime_type: Option<String> = None;
    let mut data: Option<axum::body::Bytes> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "path" => {
                if let Ok(text) = field.text().await {
                    path = Some(text);
                }
            }
            "file" => {
                filename = field.file_name().map(|s| s.to_string());
                mime_type = field.content_type().map(|s| s.to_string());
                if let Ok(bytes) = field.bytes().await {
                    data = Some(bytes);
                }
            }
            _ => {}
        }
    }

    let request = crate::api::DriveUploadRequest {
        path: path.unwrap_or_else(|| "uploads".to_string()),
        filename: filename.unwrap_or_else(|| "unnamed".to_string()),
        mime_type,
    };

    match data {
        Some(bytes) => {
            match crate::api::upload_drive_file(
                state.db.pool(),
                &state.drive_config,
                request,
                bytes,
            )
            .await
            {
                Ok(file) => (StatusCode::CREATED, Json(file)).into_response(),
                Err(e) => error_response(e),
            }
        }
        None => error_response(crate::error::Error::InvalidInput(
            "No file data provided".into(),
        )),
    }
}

/// POST /api/drive/folders - Create a folder
pub async fn create_drive_folder_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::DriveCreateFolderRequest>,
) -> Response {
    match crate::api::create_drive_folder(state.db.pool(), &state.drive_config, request).await {
        Ok(folder) => (StatusCode::CREATED, Json(folder)).into_response(),
        Err(e) => error_response(e),
    }
}

/// POST /api/drive/reconcile - Reconcile usage with storage (admin)
pub async fn reconcile_drive_usage_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::reconcile_drive_usage(state.db.pool(), &state.drive_config).await)
}

// =============================================================================
// Drive Trash Handlers
// =============================================================================

/// GET /api/drive/trash - List files in trash
pub async fn list_drive_trash_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_drive_trash(state.db.pool()).await)
}

/// POST /api/drive/files/:id/restore - Restore a file from trash
pub async fn restore_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    api_response(crate::api::restore_drive_file(state.db.pool(), &file_id).await)
}

/// DELETE /api/drive/files/:id/purge - Permanently delete a file (skip trash)
pub async fn purge_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    match crate::api::purge_drive_file(state.db.pool(), &state.drive_config, &file_id).await {
        Ok(_) => success_message("File permanently deleted"),
        Err(e) => error_response(e),
    }
}

/// POST /api/drive/trash/empty - Empty all files from trash
pub async fn empty_drive_trash_handler(State(state): State<AppState>) -> Response {
    match crate::api::empty_drive_trash(state.db.pool(), &state.drive_config).await {
        Ok(count) => (
            StatusCode::OK,
            Json(serde_json::json!({ "deleted_count": count })),
        )
            .into_response(),
        Err(e) => error_response(e),
    }
}

// =============================================================================
// Media Handlers
// =============================================================================

/// POST /api/media/upload - Upload media file with content-addressed dedup
///
/// Accepts multipart form with:
/// - `file`: The file data (required)
/// - `filename`: Override filename (optional, uses file's name by default)
///
/// Returns MediaFile with URL for embedding in pages.
/// If identical content already exists, returns existing file (dedup).
pub async fn upload_media_handler(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Response {
    // Parse multipart form
    let mut filename: Option<String> = None;
    let mut mime_type: Option<String> = None;
    let mut data: Option<axum::body::Bytes> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "filename" => {
                if let Ok(text) = field.text().await {
                    filename = Some(text);
                }
            }
            "file" => {
                // Use form field filename if no explicit filename provided
                if filename.is_none() {
                    filename = field.file_name().map(|s| s.to_string());
                }
                mime_type = field.content_type().map(|s| s.to_string());
                if let Ok(bytes) = field.bytes().await {
                    data = Some(bytes);
                }
            }
            _ => {}
        }
    }

    let filename = filename.unwrap_or_else(|| "unnamed".to_string());

    match data {
        Some(bytes) => {
            match crate::api::upload_media(
                state.db.pool(),
                &state.drive_config,
                &filename,
                mime_type,
                bytes,
            )
            .await
            {
                Ok(file) => (StatusCode::CREATED, Json(file)).into_response(),
                Err(e) => error_response(e),
            }
        }
        None => error_response(crate::error::Error::InvalidInput(
            "No file data provided".into(),
        )),
    }
}

/// GET /api/media/:id - Get media file metadata
pub async fn get_media_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    api_response(crate::api::get_media(state.db.pool(), &file_id).await)
}

// =============================================================================
// Internal API Handlers (virtues-api Integration)
// =============================================================================

/// POST /internal/hydrate - Hydrate user profile from virtues-api
///
/// This endpoint is called by virtues-api on the first request to a newly
/// provisioned container. It seeds the profile with data from Atlas
/// provisioning and marks the server as ready.
pub async fn hydrate_profile_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(request): Json<crate::api::HydrateRequest>,
) -> Response {
    // Validate virtues-api secret
    let expected_secret = std::env::var("VIRTUES_API_INTERNAL_SECRET").unwrap_or_default();
    let provided_secret = headers
        .get("X-Virtues-Api-Secret")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // In production, require the secret; in dev, allow any request
    let is_production = std::env::var("RUST_ENV")
        .map(|v| v == "production")
        .unwrap_or(false);

    if is_production && (expected_secret.is_empty() || provided_secret != expected_secret) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "Invalid or missing X-Virtues-Api-Secret header"
            })),
        )
            .into_response();
    }

    api_response(crate::api::hydrate_profile(state.db.pool(), request).await)
}

/// GET /internal/server-status - Get current server status
pub async fn get_server_status_handler(State(state): State<AppState>) -> Response {
    match crate::api::get_server_status(state.db.pool()).await {
        Ok(status) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": status.as_str(),
                "is_ready": status == crate::api::ServerStatus::Ready
            })),
        )
            .into_response(),
        Err(e) => error_response(e),
    }
}

/// POST /internal/mark-ready - Mark server as ready (dev/admin use)
pub async fn mark_server_ready_handler(State(state): State<AppState>) -> Response {
    match crate::api::mark_server_ready(state.db.pool()).await {
        Ok(_) => success_message("Server marked as ready"),
        Err(e) => error_response(e),
    }
}

// ============================================================================
// Pages Handlers
// ============================================================================

/// Query params for pages list
#[derive(Debug, Deserialize)]
pub struct ListPagesQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub workspace_id: Option<String>,
}

/// GET /api/pages - List all pages
pub async fn list_pages_handler(
    State(state): State<AppState>,
    Query(query): Query<ListPagesQuery>,
) -> Response {
    // Note: workspace_id filter removed - views handle filtering now
    api_response(crate::api::list_pages(state.db.pool(), query.limit, query.offset).await)
}

/// GET /api/pages/:id - Get a single page
pub async fn get_page_handler(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    api_response(crate::api::get_page(state.db.pool(), &id).await)
}

/// POST /api/pages - Create a new page
pub async fn create_page_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreatePageRequest>,
) -> Response {
    match crate::api::create_page(state.db.pool(), request).await {
        Ok(page) => (StatusCode::CREATED, Json(page)).into_response(),
        Err(e) => error_response(e),
    }
}

/// PUT /api/pages/:id - Update a page
pub async fn update_page_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::UpdatePageRequest>,
) -> Response {
    api_response(crate::api::update_page(state.db.pool(), &id, request).await)
}

/// DELETE /api/pages/:id - Delete a page
pub async fn delete_page_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::delete_page(state.db.pool(), &id).await {
        Ok(_) => success_message("Page deleted successfully"),
        Err(e) => error_response(e),
    }
}

/// GET /api/pages/reflections/:date - Get all reflections for a date
pub async fn get_reflections_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    api_response(crate::api::get_reflections_for_date(state.db.pool(), &date).await)
}

/// POST /api/pages/reflections/:date - Create a new reflection for a date
pub async fn create_reflection_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match crate::api::create_reflection(state.db.pool(), &date, None).await {
        Ok(page) => (StatusCode::CREATED, Json(page)).into_response(),
        Err(e) => error_response(e),
    }
}

/// Query params for entity search
#[derive(Debug, Deserialize)]
pub struct EntitySearchQuery {
    pub q: String,
}

/// GET /api/pages/search/entities - Search entities for autocomplete
pub async fn search_entities_handler(
    State(state): State<AppState>,
    Query(query): Query<EntitySearchQuery>,
) -> Response {
    api_response(crate::api::search_entities(state.db.pool(), &query.q).await)
}

// ============================================================================
// Page Sharing Handlers
// ============================================================================

/// POST /api/pages/:id/share - Create or replace a share link for a page
pub async fn create_page_share_handler(
    State(state): State<AppState>,
    Path(page_id): Path<String>,
) -> Response {
    match crate::api::create_page_share(state.db.pool(), &page_id).await {
        Ok(share) => (StatusCode::CREATED, Json(share)).into_response(),
        Err(e) => error_response(e),
    }
}

/// GET /api/pages/:id/share - Get the active share for a page
pub async fn get_page_share_handler(
    State(state): State<AppState>,
    Path(page_id): Path<String>,
) -> Response {
    api_response(crate::api::get_page_share(state.db.pool(), &page_id).await)
}

/// DELETE /api/pages/:id/share - Revoke the share for a page
pub async fn delete_page_share_handler(
    State(state): State<AppState>,
    Path(page_id): Path<String>,
) -> Response {
    match crate::api::delete_page_share(state.db.pool(), &page_id).await {
        Ok(_) => success_message("Share revoked"),
        Err(e) => error_response(e),
    }
}

/// GET /api/s/:token - Get a shared page (public, no auth)
pub async fn get_shared_page_handler(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Response {
    api_response(crate::api::get_shared_page(state.db.pool(), &token).await)
}

/// GET /api/s/:token/files/:file_id - Download a file from a shared page (public, no auth)
/// Validates that the file is referenced by the shared page's content
pub async fn shared_file_download_handler(
    State(state): State<AppState>,
    Path((token, file_id)): Path<(String, String)>,
) -> Response {
    // Validate the share token and that this file belongs to the shared page
    if let Err(e) = crate::api::validate_shared_file(state.db.pool(), &token, &file_id).await {
        return error_response(e);
    }

    // Lake objects use in-memory download
    if crate::api::is_lake_object_id(&file_id) {
        let result =
            crate::api::download_lake_object(state.db.pool(), &state.storage, &file_id).await;
        return match result {
            Ok((file, content)) => {
                let content_type = file
                    .mime_type
                    .unwrap_or_else(|| "application/octet-stream".to_string());
                let filename = sanitize_content_disposition(&file.filename);
                (
                    [
                        (axum::http::header::CONTENT_TYPE, content_type),
                        (
                            axum::http::header::CONTENT_DISPOSITION,
                            format!("inline; filename=\"{}\"", filename),
                        ),
                        (
                            axum::http::header::CONTENT_LENGTH,
                            content.len().to_string(),
                        ),
                    ],
                    content,
                )
                    .into_response()
            }
            Err(e) => error_response(e),
        };
    }

    // Regular drive files: stream from storage
    let result =
        crate::api::download_drive_file_stream(state.db.pool(), &state.drive_config, &file_id)
            .await;
    match result {
        Ok((file, stream)) => {
            let content_type = file
                .mime_type
                .unwrap_or_else(|| "application/octet-stream".to_string());
            let filename = sanitize_content_disposition(&file.filename);
            (
                [
                    (axum::http::header::CONTENT_TYPE, content_type),
                    (
                        axum::http::header::CONTENT_DISPOSITION,
                        format!("inline; filename=\"{}\"", filename),
                    ),
                ],
                axum::body::Body::from_stream(stream),
            )
                .into_response()
        }
        Err(e) => error_response(e),
    }
}

// ============================================================================
// Page Versions Handlers
// ============================================================================

/// Query params for versions list
#[derive(Debug, Deserialize)]
pub struct ListVersionsQuery {
    pub limit: Option<i64>,
}

/// GET /api/pages/:id/versions - List versions for a page
pub async fn list_page_versions_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ListVersionsQuery>,
) -> Response {
    api_response(crate::api::list_versions(state.db.pool(), &id, query.limit).await)
}

/// POST /api/pages/:id/versions - Create a new version snapshot
pub async fn create_page_version_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::CreateVersionRequest>,
) -> Response {
    match crate::api::create_version(state.db.pool(), &id, request).await {
        Ok(version) => (StatusCode::CREATED, Json(version)).into_response(),
        Err(e) => error_response(e),
    }
}

/// GET /api/pages/versions/:version_id - Get a single version (with snapshot for restore)
pub async fn get_page_version_handler(
    State(state): State<AppState>,
    Path(version_id): Path<String>,
) -> Response {
    api_response(crate::api::get_version(state.db.pool(), &version_id).await)
}

// ============================================================================
// Things Handlers (long-running named anchors — projects, pets, goals, ...)
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ListThingsQuery {
    pub category: Option<String>,
}

/// GET /api/things — list things, optional `?category=...` filter.
pub async fn list_things_handler(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<ListThingsQuery>,
) -> Response {
    api_response(
        crate::api::things::list_things(
            state.db.pool(),
            crate::api::ListThingsParams { category: q.category },
        )
        .await,
    )
}

/// GET /api/things/:id — single thing with pins.
pub async fn get_thing_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::things::get_thing(state.db.pool(), &id).await)
}

/// POST /api/things — create.
pub async fn create_thing_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreateThingRequest>,
) -> Response {
    match crate::api::things::create_thing(state.db.pool(), request).await {
        Ok(thing) => (StatusCode::CREATED, Json(thing)).into_response(),
        Err(e) => error_response(e),
    }
}

/// PATCH /api/things/:id — update.
pub async fn update_thing_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::UpdateThingRequest>,
) -> Response {
    api_response(crate::api::things::update_thing(state.db.pool(), &id, request).await)
}

/// DELETE /api/things/:id — delete (cascades to pins).
pub async fn delete_thing_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::things::delete_thing(state.db.pool(), &id).await {
        Ok(_) => success_message("Thing deleted"),
        Err(e) => error_response(e),
    }
}

/// POST /api/things/:id/pins — add a pin (dedupes on url).
pub async fn add_thing_pin_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::AddThingPinRequest>,
) -> Response {
    match crate::api::things::add_thing_pin(state.db.pool(), &id, request).await {
        Ok(pin) => (StatusCode::CREATED, Json(pin)).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Debug, Deserialize)]
pub struct RemoveThingPinRequest {
    pub url: String,
}

/// DELETE /api/things/:id/pins — remove a pin by url.
pub async fn remove_thing_pin_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<RemoveThingPinRequest>,
) -> Response {
    match crate::api::things::remove_thing_pin(state.db.pool(), &id, &request.url).await {
        Ok(_) => success_message("Pin removed from thing"),
        Err(e) => error_response(e),
    }
}

/// PUT /api/things/:id/pins/reorder — reorder pins.
pub async fn reorder_thing_pins_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::ReorderThingPinsRequest>,
) -> Response {
    match crate::api::things::reorder_thing_pins(state.db.pool(), &id, request).await {
        Ok(_) => success_message("Thing pins reordered"),
        Err(e) => error_response(e),
    }
}

// ============================================================================
// Pins Handlers (sidebar pinned URLs)
// ============================================================================

/// GET /api/pins — list all pins, ordered by `sort_order`.
pub async fn list_pins_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_pins(state.db.pool()).await)
}

/// POST /api/pins — pin a URL (idempotent on URL).
pub async fn create_pin_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreatePinRequest>,
) -> Response {
    match crate::api::create_pin(state.db.pool(), request).await {
        Ok(pin) => (StatusCode::CREATED, Json(pin)).into_response(),
        Err(e) => error_response(e),
    }
}

/// PATCH /api/pins/:id — update label / icon / sort_order.
pub async fn update_pin_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::UpdatePinRequest>,
) -> Response {
    api_response(crate::api::update_pin(state.db.pool(), &id, request).await)
}

/// DELETE /api/pins/:id — unpin.
pub async fn delete_pin_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::delete_pin(state.db.pool(), &id).await {
        Ok(_) => success_message("Pin removed"),
        Err(e) => error_response(e),
    }
}

#[derive(serde::Deserialize)]
pub struct ReorderPinsRequest {
    pub urls: Vec<String>,
}

/// PUT /api/pins/reorder — reorder all pins to match the supplied URL list.
pub async fn reorder_pins_handler(
    State(state): State<AppState>,
    Json(request): Json<ReorderPinsRequest>,
) -> Response {
    match crate::api::reorder_pins(state.db.pool(), &request.urls).await {
        Ok(_) => success_message("Pins reordered"),
        Err(e) => error_response(e),
    }
}

// ============================================================================
// Spaces Handlers
// ============================================================================

/// GET /api/spaces - List all spaces
pub async fn list_spaces_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::spaces::list_spaces(state.db.pool()).await)
}

/// GET /api/spaces/:id - Get a single space
pub async fn get_space_handler(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    api_response(crate::api::spaces::get_space(state.db.pool(), &id).await)
}

/// PUT /api/spaces/:id - Update a space
pub async fn update_space_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::spaces::UpdateSpaceRequest>,
) -> Response {
    api_response(crate::api::spaces::update_space(state.db.pool(), &id, request).await)
}

/// GET /api/spaces/:id/views - Get views for a space
pub async fn list_space_views_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::views::list_views(state.db.pool(), &id).await)
}

/// GET /api/spaces/:id/items - Get root-level items for a space
pub async fn list_space_items_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::views::resolve_space_items(state.db.pool(), &id).await)
}

/// POST /api/spaces/:id/items - Add item to space root level
pub async fn add_space_item_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ViewItemRequest>,
) -> Response {
    match crate::api::views::add_space_item(state.db.pool(), &id, &request.url).await {
        Ok(item) => (StatusCode::CREATED, Json(item)).into_response(),
        Err(e) => error_response(e),
    }
}

/// DELETE /api/spaces/:id/items - Remove item from space root level
pub async fn remove_space_item_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ViewItemRequest>,
) -> Response {
    match crate::api::views::remove_space_item(state.db.pool(), &id, &request.url).await {
        Ok(_) => success_message("Item removed from space"),
        Err(e) => error_response(e),
    }
}

/// Request to reorder space items with explicit sort_order values
#[derive(serde::Deserialize)]
pub struct ReorderSpaceItemsRequest {
    pub items: Vec<crate::api::views::ItemSortOrder>,
}

/// PUT /api/spaces/:id/items/reorder - Reorder space root items
pub async fn reorder_space_items_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ReorderSpaceItemsRequest>,
) -> Response {
    match crate::api::views::reorder_space_items(state.db.pool(), &id, request.items).await {
        Ok(_) => success_message("Space items reordered"),
        Err(e) => error_response(e),
    }
}

// ============================================================================
// Namespaces Handlers
// ============================================================================

/// GET /api/namespaces - List all namespaces
pub async fn list_namespaces_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::namespaces::list_namespaces(state.db.pool()).await)
}

/// GET /api/namespaces/:name - Get a specific namespace
pub async fn get_namespace_handler(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Response {
    api_response(crate::api::namespaces::get_namespace(state.db.pool(), &name).await)
}

// ============================================================================
// Views Handlers
// ============================================================================

/// POST /api/views - Create a new view
pub async fn create_view_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::views::CreateViewRequest>,
) -> Response {
    match crate::api::views::create_view(state.db.pool(), request).await {
        Ok(view) => (StatusCode::CREATED, Json(view)).into_response(),
        Err(e) => error_response(e),
    }
}

/// GET /api/views/:id - Get a view
pub async fn get_view_handler(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    api_response(crate::api::views::get_view(state.db.pool(), &id).await)
}

/// PUT /api/views/:id - Update a view
pub async fn update_view_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::views::UpdateViewRequest>,
) -> Response {
    api_response(crate::api::views::update_view(state.db.pool(), &id, request).await)
}

/// DELETE /api/views/:id - Delete a view
pub async fn delete_view_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::views::delete_view(state.db.pool(), &id).await {
        Ok(_) => success_message("View deleted successfully"),
        Err(e) => error_response(e),
    }
}

/// Request for resolve view with optional pagination
#[derive(serde::Deserialize)]
pub struct ResolveViewQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// POST /api/views/:id/resolve - Resolve a view to its entities
pub async fn resolve_view_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ResolveViewQuery>,
) -> Response {
    api_response(
        crate::api::views::resolve_view(state.db.pool(), &id, query.limit, query.offset).await,
    )
}

/// Request to add/remove item from view
#[derive(serde::Deserialize)]
pub struct ViewItemRequest {
    pub url: String,
}

/// POST /api/views/:id/items - Add an item to a manual view
pub async fn add_view_item_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ViewItemRequest>,
) -> Response {
    api_response(crate::api::views::add_item_to_view(state.db.pool(), &id, &request.url).await)
}

/// DELETE /api/views/:id/items - Remove an item from a manual view
pub async fn remove_view_item_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ViewItemRequest>,
) -> Response {
    api_response(crate::api::views::remove_item_from_view(state.db.pool(), &id, &request.url).await)
}

/// GET /api/views/:id/items - List items in a manual view
pub async fn list_view_items_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::views::list_view_items(state.db.pool(), &id).await)
}

/// Request to reorder view items
#[derive(serde::Deserialize)]
pub struct ReorderViewItemsRequest {
    pub url_order: Vec<String>,
}

/// PUT /api/views/:id/items/reorder - Reorder items in a manual view
pub async fn reorder_view_items_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ReorderViewItemsRequest>,
) -> Response {
    api_response(
        crate::api::views::reorder_view_items(state.db.pool(), &id, request.url_order).await,
    )
}

// ============================================================================
// Lake API handlers
// ============================================================================

/// GET /api/lake/summary - Get lake summary statistics
pub async fn get_lake_summary_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::lake::get_lake_summary(state.db.pool()).await)
}

/// GET /api/lake/streams - List all streams in the lake
pub async fn list_lake_streams_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::lake::list_lake_streams(state.db.pool()).await)
}

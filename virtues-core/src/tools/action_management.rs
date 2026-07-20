//! Chat tools for listing, fetching, editing, deleting, and manually running
//! actions. These exist in addition to `setup_action` (which creates an
//! action from the current chat) and wrap the same core helpers the HTTP
//! layer uses in `crate::scheduler::actions` + `crate::action_runner`.
//!
//! System-row edit/delete protection is enforced inside the core helpers, so
//! these tools can just forward user input. The tool layer's job is
//! parameter validation and shaping the JSON response the LLM will see.

use sqlx::PgPool;

use super::executor::{ToolContext, ToolError, ToolResult};
use crate::action_runner::{ActionRunStatus, RunnerDeps};
use crate::scheduler::actions;
use crate::server::yjs::YjsState;

fn map_err(e: crate::error::Error) -> ToolError {
    match e {
        crate::error::Error::NotFound(msg) => {
            ToolError::ExecutionFailed(format!("not found: {msg}"))
        }
        crate::error::Error::InvalidInput(msg) => ToolError::InvalidParameters(msg),
        other => ToolError::ExecutionFailed(other.to_string()),
    }
}

/// `list_actions` — return a lightweight array of actions with their last run.
///
/// Optional filters:
/// - `owner`: `"system"` or `"user"`
/// - `enabled`: bool
/// - `trigger`: one of `cron|manual|tool|api|webhook`
pub async fn list_actions(
    pool: &PgPool,
    arguments: serde_json::Value,
) -> Result<ToolResult, ToolError> {
    let owner_filter = arguments.get("owner").and_then(|v| v.as_str());
    let enabled_filter = arguments.get("enabled").and_then(|v| v.as_bool());
    let trigger_filter = arguments.get("trigger").and_then(|v| v.as_str());

    let include_archived = arguments
        .get("include_archived")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let all = actions::get_all_actions(pool).await.map_err(map_err)?;
    let mut items = Vec::with_capacity(all.len());
    for a in all {
        // Archived applets (lifecycle complete) are hidden by default —
        // the list holds living things.
        if a.archived_at.is_some() && !include_archived {
            continue;
        }
        if let Some(o) = owner_filter {
            if a.owner != o {
                continue;
            }
        }
        if let Some(e) = enabled_filter {
            if a.enabled != e {
                continue;
            }
        }
        if let Some(t) = trigger_filter {
            if !a.triggers.iter().any(|x| x == t) {
                continue;
            }
        }
        let last = actions::last_run(pool, &a.id).await.ok().flatten();
        items.push(serde_json::json!({
            "id": a.id,
            "name": a.name,
            "owner": a.owner,
            "enabled": a.enabled,
            "cron_schedule": a.cron_schedule,
            "triggers": a.triggers,
            "last_run": last.map(|r| serde_json::json!({
                "status": r.status,
                "started_at": r.started_at,
                "completed_at": r.completed_at,
                "error": r.error,
                "summary": r.result_summary,
            })),
        }));
    }

    Ok(ToolResult::success(serde_json::json!({
        "count": items.len(),
        "actions": items,
    })))
}

/// `get_action` — fetch a single action with its last 10 runs inlined.
pub async fn get_action(
    pool: &PgPool,
    arguments: serde_json::Value,
) -> Result<ToolResult, ToolError> {
    let id = arguments
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::InvalidParameters("id is required".into()))?;

    let action = actions::get_action(pool, id).await.map_err(map_err)?;
    let runs = actions::query_runs(pool, Some(id), None, 10)
        .await
        .map_err(map_err)?;

    Ok(ToolResult::success(serde_json::json!({
        "action": action,
        "recent_runs": runs,
    })))
}

/// `edit_action` — partial update. System rows accept only `enabled`,
/// `cron_schedule`, `config`, `memory`; user rows accept all fields.
pub async fn edit_action(
    pool: &PgPool,
    arguments: serde_json::Value,
) -> Result<ToolResult, ToolError> {
    let id = arguments
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::InvalidParameters("id is required".into()))?;
    let patch = arguments
        .get("patch")
        .cloned()
        .ok_or_else(|| ToolError::InvalidParameters("patch is required".into()))?;

    let updated = actions::update_action(pool, id, &patch)
        .await
        .map_err(map_err)?;
    Ok(ToolResult::success(serde_json::json!({
        "action": updated,
    })))
}

/// `delete_action` — remove a user-owned action. System rows are refused.
pub async fn delete_action(
    pool: &PgPool,
    arguments: serde_json::Value,
) -> Result<ToolResult, ToolError> {
    let id = arguments
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::InvalidParameters("id is required".into()))?;

    actions::delete_action(pool, id).await.map_err(map_err)?;
    Ok(ToolResult::success(serde_json::json!({
        "deleted": true,
        "id": id,
    })))
}

/// `run_action` — dispatch an action manually with `trigger = "tool"`.
/// Optional `payload` is forwarded to the runner; optional `date` is merged
/// into the action's `config.date` for actions that accept a date override
/// (e.g. `day_summary_eod`).
pub async fn run_action(
    pool: &PgPool,
    yjs: &YjsState,
    arguments: serde_json::Value,
    _context: &ToolContext,
) -> Result<ToolResult, ToolError> {
    let id = arguments
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::InvalidParameters("id is required".into()))?;

    let payload = arguments.get("payload").cloned();

    // Date override: if present, merge into the stored config so the binary
    // picks it up via stdin. Uses `update_action` with just the config key
    // so the system-owner guard still applies correctly.
    if let Some(date) = arguments.get("date").and_then(|v| v.as_str()) {
        let current = actions::get_action(pool, id).await.map_err(map_err)?;
        let mut config = current.config.clone();
        if let Some(obj) = config.as_object_mut() {
            obj.insert("date".to_string(), serde_json::json!(date));
        }
        let patch = serde_json::json!({ "config": config });
        actions::update_action(pool, id, &patch)
            .await
            .map_err(map_err)?;
    }

    let deps = RunnerDeps {
        db: pool.clone(),
        yjs: yjs.clone(),
    };

    let result = crate::action_runner::run_action(&deps, id, "tool", payload.as_ref())
        .await
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

    let status_label = match result.status {
        ActionRunStatus::Success => "success",
        ActionRunStatus::Skipped => "skipped",
        ActionRunStatus::Failed => "error",
        ActionRunStatus::NotFound => "not_found",
        ActionRunStatus::Forbidden => "forbidden",
        // Tool-call dispatch awaits the full run via `run_action`, never
        // `run_action_detached`, so this arm is unreachable in practice.
        ActionRunStatus::Running => "running",
    };

    Ok(ToolResult::success(serde_json::json!({
        "run_id": result.run_id,
        "status": status_label,
        "summary": result.summary,
        "error": result.error,
    })))
}

//! Chat tools for listing, fetching, editing, deleting, and manually running
//! actions. These exist in addition to `setup_action` (which creates an
//! action from the current chat) and wrap the same core helpers the HTTP
//! layer uses in `crate::scheduler::applets` + `crate::applet_runner`.
//!
//! System-row edit/delete protection is enforced inside the core helpers, so
//! these tools can just forward user input. The tool layer's job is
//! parameter validation and shaping the JSON response the LLM will see.

use sqlx::PgPool;

use super::executor::{ToolContext, ToolError, ToolResult};
use crate::applet_runner::{AppletRunStatus, RunnerDeps};
use crate::scheduler::applets;
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

/// `list_applets` — return a lightweight array of actions with their last run.
///
/// Optional filters:
/// - `owner`: `"system"` or `"user"`
/// - `enabled`: bool
/// - `trigger`: one of `cron|manual|tool|api|webhook`
pub async fn list_applets(
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

    let all = applets::get_all_applets(pool).await.map_err(map_err)?;
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
        let last = applets::last_run(pool, &a.id).await.ok().flatten();
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

/// `get_applet` — fetch a single action with its last 10 runs inlined.
pub async fn get_applet(
    pool: &PgPool,
    arguments: serde_json::Value,
) -> Result<ToolResult, ToolError> {
    let id = arguments
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::InvalidParameters("id is required".into()))?;

    let action = applets::get_applet(pool, id).await.map_err(map_err)?;
    let runs = applets::query_runs(pool, Some(id), None, 10)
        .await
        .map_err(map_err)?;

    Ok(ToolResult::success(serde_json::json!({
        "action": action,
        "recent_runs": runs,
    })))
}

/// `edit_applet` — partial update. System rows accept only `enabled`,
/// `cron_schedule`, `config`, `memory`; user rows accept all fields.
pub async fn edit_applet(
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

    // The gate invariant: no path from model output to an enabled, scheduled
    // ai-owned row without a user-surface action. Two vectors are closed here:
    //   (1) enabling directly (enabled:true), and
    //   (2) adding a boundary (schedule / api|webhook trigger) to an applet
    //       that is ALREADY enabled — which would silently create the
    //       forbidden enabled∧scheduled state without touching `enabled`.
    let mut patch = patch;
    if let Some(obj) = patch.as_object() {
        let sets_enabled_true = obj.get("enabled").and_then(|v| v.as_bool()) == Some(true);
        let adds_schedule = obj
            .get("cron_schedule")
            .is_some_and(|v| v.as_str().is_some_and(|s| !s.trim().is_empty()));
        let adds_remote_trigger = obj
            .get("triggers")
            .and_then(|v| v.as_array())
            .is_some_and(|arr| {
                arr.iter().any(|t| matches!(t.as_str(), Some("api") | Some("webhook")))
            });
        if sets_enabled_true || adds_schedule || adds_remote_trigger {
            let current = applets::get_applet(pool, id).await.map_err(map_err)?;
            if current.owner == "ai" {
                if sets_enabled_true {
                    return Ok(ToolResult::success(serde_json::json!({
                        "status": "refused",
                        "error": "enabling an AI-authored applet is a user action — ask the user to enable it on the applet page",
                    })));
                }
                // Boundary added: force-disable so the user must re-enable
                // (re-gate). Never leave an ai row enabled∧scheduled via a tool.
                if current.enabled {
                    if let Some(o) = patch.as_object_mut() {
                        o.insert("enabled".into(), serde_json::Value::Bool(false));
                    }
                }
            }
        }
    }

    let updated = applets::update_applet(pool, id, &patch)
        .await
        .map_err(map_err)?;
    Ok(ToolResult::success(serde_json::json!({
        "action": updated,
    })))
}

/// `delete_applet` — remove a user-owned action. System rows are refused.
pub async fn delete_applet(
    pool: &PgPool,
    arguments: serde_json::Value,
) -> Result<ToolResult, ToolError> {
    let id = arguments
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::InvalidParameters("id is required".into()))?;

    // Full teardown (row + on-disk folder) lives in `applets::delete_applet`.
    // The tool keeps the applet's data by default — dropping owned tables is a
    // user decision made on the delete confirm, not something the model does.
    applets::delete_applet(pool, id, false)
        .await
        .map_err(map_err)?;

    Ok(ToolResult::success(serde_json::json!({
        "deleted": true,
        "id": id,
    })))
}

/// `run_applet` — dispatch an action manually with `trigger = "tool"`.
/// Optional `payload` is forwarded to the runner; optional `date` is merged
/// into the action's `config.date` for actions that accept a date override
/// (e.g. `day_summary_eod`).
pub async fn run_applet(
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
    // picks it up via stdin. Uses `update_applet` with just the config key
    // so the system-owner guard still applies correctly.
    if let Some(date) = arguments.get("date").and_then(|v| v.as_str()) {
        let current = applets::get_applet(pool, id).await.map_err(map_err)?;
        let mut config = current.config.clone();
        if let Some(obj) = config.as_object_mut() {
            obj.insert("date".to_string(), serde_json::json!(date));
        }
        let patch = serde_json::json!({ "config": config });
        applets::update_applet(pool, id, &patch)
            .await
            .map_err(map_err)?;
    }

    let deps = RunnerDeps {
        db: pool.clone(),
        yjs: yjs.clone(),
    };

    let result = crate::applet_runner::run_applet(&deps, id, "tool", payload.as_ref())
        .await
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

    let status_label = match result.status {
        AppletRunStatus::Success => "success",
        AppletRunStatus::Skipped => "skipped",
        AppletRunStatus::Failed => "error",
        AppletRunStatus::NotFound => "not_found",
        AppletRunStatus::Forbidden => "forbidden",
        // Tool-call dispatch awaits the full run via `run_applet`, never
        // `run_applet_detached`, so this arm is unreachable in practice.
        AppletRunStatus::Running => "running",
    };

    Ok(ToolResult::success(serde_json::json!({
        "run_id": result.run_id,
        "status": status_label,
        "summary": result.summary,
        "error": result.error,
    })))
}

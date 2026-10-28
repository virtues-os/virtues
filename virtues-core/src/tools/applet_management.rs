//! The `applet` tool: list, get, setup, edit, run and delete, chosen by its
//! `action`. Setup lives in `applet_setup`; the rest wrap the same core
//! helpers the HTTP layer uses in `crate::scheduler::applets` +
//! `crate::applet_runner`.
//!
//! System-row edit/delete protection is enforced inside the core helpers, so
//! these can just forward user input. The tool layer's job is parameter
//! validation and shaping the JSON response the LLM will see.

use sqlx::PgPool;

use super::executor::{ToolContext, ToolError, ToolResult};
use crate::applet_runner::{AppletRunStatus, RunnerDeps};
use crate::scheduler::applets;
use crate::server::yjs::YjsState;
use virtues_registry::tools::APPLET_READ_ACTIONS;

/// The `applet` tool's `action`.
pub fn action(arguments: &serde_json::Value) -> Option<&str> {
    arguments.get("action").and_then(|v| v.as_str())
}

/// Whether the call only reads (list or get).
pub fn is_read(arguments: &serde_json::Value) -> bool {
    action(arguments).is_some_and(|a| APPLET_READ_ACTIONS.contains(&a))
}

/// Run one `applet` call. A running applet may only read: it runs ungated,
/// with nobody present to approve a change, so its own tool list offers list
/// and get and this refuses anything else whatever the list says.
pub async fn execute(
    pool: &PgPool,
    yjs: Option<&YjsState>,
    arguments: serde_json::Value,
    context: &ToolContext,
) -> Result<ToolResult, ToolError> {
    let Some(action) = action(&arguments).map(str::to_string) else {
        return Err(ToolError::InvalidParameters(
            "action is required: list, get, setup, edit, run or delete".into(),
        ));
    };
    if context.applet_id.is_some() && !APPLET_READ_ACTIONS.contains(&action.as_str()) {
        return Err(ToolError::InvalidParameters(format!(
            "a running applet may list and get applets, not {action} them"
        )));
    }
    let mut arguments = arguments;
    if let Some(args) = arguments.as_object_mut() {
        args.remove("action");
    }
    match action.as_str() {
        "list" => list_applets(pool, arguments).await,
        "get" => get_applet(pool, arguments).await,
        "setup" => super::applet_setup::execute(pool, arguments, context).await,
        "edit" => edit_applet(pool, arguments).await,
        "delete" => delete_applet(pool, arguments).await,
        "run" => {
            let yjs = yjs.ok_or_else(|| {
                ToolError::ExecutionFailed("running an applet needs YjsState; this executor was built without it".into())
            })?;
            run_applet(pool, yjs, arguments, context).await
        }
        other => Err(ToolError::InvalidParameters(format!(
            "unknown action {other:?}: use list, get, setup, edit, run or delete"
        ))),
    }
}

fn map_err(e: crate::error::Error) -> ToolError {
    match e {
        crate::error::Error::NotFound(msg) => {
            ToolError::ExecutionFailed(format!("not found: {msg}"))
        }
        crate::error::Error::InvalidInput(msg) => ToolError::InvalidParameters(msg),
        other => ToolError::ExecutionFailed(other.to_string()),
    }
}

/// list: a lightweight array of actions with their last run.
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
            "schedule": a.schedule,
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

/// get: a single action with its last 10 runs inlined.
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

/// edit: a partial update. System rows accept only `enabled`,
/// `schedule`, `config`, `memory`; user rows accept all fields.
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
            .get("schedule")
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

/// delete: remove a user-owned action. System rows are refused.
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

/// The trigger a run action runs under. The owner typing
/// `virtues applet run` is a manual run, the same as the app's Run button;
/// the model calling the tool from chat is `tool`, which an applet has to
/// opt into (sync applets allow only `cron` and `manual`).
fn run_trigger(context: &ToolContext) -> &'static str {
    if context.console {
        "manual"
    } else {
        "tool"
    }
}

/// run: dispatch an action now, under [`run_trigger`].
/// Optional `payload` is forwarded to the runner; optional `date` goes into
/// the payload as `date` for actions that accept a date override (e.g.
/// `day_summary_eod`). It is never written to the row, so it lasts one run.
pub async fn run_applet(
    pool: &PgPool,
    yjs: &YjsState,
    arguments: serde_json::Value,
    context: &ToolContext,
) -> Result<ToolResult, ToolError> {
    let id = arguments
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::InvalidParameters("id is required".into()))?;

    let payload = with_date(arguments.get("payload").cloned(), arguments.get("date").and_then(|v| v.as_str()));

    let deps = RunnerDeps {
        db: pool.clone(),
        yjs: yjs.clone(),
    };

    let result = crate::applet_runner::run_applet(&deps, id, run_trigger(context), payload.as_ref())
        .await
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

    let status_label = match result.status {
        AppletRunStatus::Success => "success",
        AppletRunStatus::Skipped => "skipped",
        AppletRunStatus::Failed => "error",
        AppletRunStatus::BudgetExceeded => "budget_exceeded",
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

/// The run's payload with a date override added. The runner saves a run's
/// returned config to `state` only, so an override written to the row's
/// `config` would stay there and pin every later scheduled run to that day.
fn with_date(payload: Option<serde_json::Value>, date: Option<&str>) -> Option<serde_json::Value> {
    let Some(date) = date else { return payload };
    let mut obj = match payload {
        Some(serde_json::Value::Object(m)) => m,
        _ => serde_json::Map::new(),
    };
    obj.insert("date".to_string(), serde_json::json!(date));
    Some(serde_json::Value::Object(obj))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_date_override_rides_in_the_payload() {
        assert_eq!(with_date(None, None), None);
        assert_eq!(with_date(None, Some("2026-03-02")), Some(serde_json::json!({ "date": "2026-03-02" })));
        assert_eq!(
            with_date(Some(serde_json::json!({ "x": 1 })), Some("2026-03-02")),
            Some(serde_json::json!({ "x": 1, "date": "2026-03-02" }))
        );
    }

    #[test]
    fn the_console_runs_an_applet_manually() {
        let console = ToolContext { console: true, ..Default::default() };
        assert_eq!(run_trigger(&console), "manual");
    }

    #[test]
    fn chat_runs_an_applet_as_a_tool() {
        assert_eq!(run_trigger(&ToolContext::default()), "tool");
    }
}

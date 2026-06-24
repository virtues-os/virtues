//! `setup_action` tool — lets an LLM turn the current chat into a scheduled
//! action.
//!
//! Writes directly to `app_actions` using the new actions-paradigm schema:
//!
//! - `agent` — the LLM instruction prompt (what this action does each run)
//! - `cron_schedule` — optional recurrence
//! - `triggers` — which invocation sources are allowed
//! - `command` — always NULL for chat-authored actions (LLM-only path)

use sqlx::PgPool;

use super::executor::{ToolContext, ToolError, ToolResult};

/// Execute the setup_action tool.
pub async fn execute(
    pool: &PgPool,
    arguments: serde_json::Value,
    context: &ToolContext,
) -> Result<ToolResult, ToolError> {
    let chat_id = context
        .chat_id
        .as_ref()
        .ok_or_else(|| ToolError::MissingContext("chat_id is required for setup_action".into()))?;

    let name = arguments
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::InvalidParameters("name is required".into()))?;

    // `agent` is the instruction prompt. Accept legacy `instruction` as an
    // alias so older callers keep working without retraining.
    let agent = arguments
        .get("agent")
        .and_then(|v| v.as_str())
        .or_else(|| arguments.get("instruction").and_then(|v| v.as_str()))
        .ok_or_else(|| ToolError::InvalidParameters("agent (or instruction) is required".into()))?;

    let cron_schedule = arguments
        .get("cron_schedule")
        .and_then(|v| v.as_str());

    // Validate cron expression shape (5 or 6 field)
    if let Some(cron) = cron_schedule {
        let fields: Vec<&str> = cron.split_whitespace().collect();
        if fields.len() < 5 || fields.len() > 6 {
            return Err(ToolError::InvalidParameters(format!(
                "Invalid cron expression '{}': expected 5 or 6 fields",
                cron
            )));
        }
    }

    // Optional triggers array. Default: ["manual"] if no cron, ["cron","manual"] otherwise.
    let triggers: Vec<String> = arguments
        .get("triggers")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_else(|| {
            if cron_schedule.is_some() {
                vec!["cron".into(), "manual".into(), "tool".into()]
            } else {
                vec!["manual".into(), "tool".into()]
            }
        });

    // Validate trigger enum values
    for t in &triggers {
        if !matches!(t.as_str(), "cron" | "manual" | "tool" | "api" | "webhook") {
            return Err(ToolError::InvalidParameters(format!(
                "Invalid trigger '{}': must be one of cron, manual, tool, api, webhook",
                t
            )));
        }
    }

    // `condition` is opt-in. Current runner still evaluates it as raw SQL, so
    // the LLM should treat it carefully — prefer a named evaluator once the
    // registry lands. For now we accept a plain string and leave it to the
    // runner's `SELECT ({condition})` path.
    let condition = arguments
        .get("condition")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let triggers_json = serde_json::to_string(&triggers)
        .map_err(|e| ToolError::ExecutionFailed(format!("failed to serialize triggers: {e}")))?;

    let config = serde_json::json!({ "chat_id": chat_id });
    let config_json = config.to_string();

    let action_id = format!("action_agent_{}", chat_id);

    sqlx::query(
        r#"
        INSERT INTO app_actions (
            id, name, owner, agent, cron_schedule, enabled, config,
            condition, triggers
        )
        VALUES ($1, $2, 'user', $3, $4, TRUE, $5::jsonb, $6, $7::jsonb)
        ON CONFLICT(id) DO UPDATE SET
            name          = excluded.name,
            agent         = excluded.agent,
            cron_schedule = excluded.cron_schedule,
            enabled       = TRUE,
            config        = excluded.config,
            condition     = excluded.condition,
            triggers      = excluded.triggers,
            updated_at    = now()
        "#,
    )
    .bind(&action_id)
    .bind(name)
    .bind(agent)
    .bind(cron_schedule)
    .bind(&config_json)
    .bind(&condition)
    .bind(&triggers_json)
    .execute(pool)
    .await
    .map_err(|e| ToolError::ExecutionFailed(format!("Failed to create action: {}", e)))?;

    // Update chat title to match action name
    sqlx::query("UPDATE app_chats SET title = $1, updated_at = now() WHERE id = $2")
        .bind(name)
        .bind(chat_id)
        .execute(pool)
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to update chat: {}", e)))?;

    let state = if cron_schedule.is_some() { "scheduled" } else { "listening" };
    tracing::info!(chat_id, name, state, "Action configured");

    let mut response = serde_json::json!({
        "action_id": action_id,
        "action_name": name,
        "action_state": state,
        "triggers": triggers,
        "has_condition": condition.is_some(),
    });

    if let Some(cron) = cron_schedule {
        response["cron_schedule"] = serde_json::json!(cron);
        response["schedule_description"] = serde_json::json!(describe_cron(cron));
    }

    Ok(ToolResult::success(response))
}

/// Describe a cron expression in human-readable form.
fn describe_cron(cron: &str) -> String {
    let parts: Vec<&str> = cron.split_whitespace().collect();

    let (min, hour, day, _month, dow) = match parts.len() {
        5 => (parts[0], parts[1], parts[2], parts[3], parts[4]),
        6 => (parts[1], parts[2], parts[3], parts[4], parts[5]),
        _ => return format!("Custom schedule: {}", cron),
    };

    if min == "0" && hour.starts_with("*/") && day == "*" && dow == "*" {
        let interval = &hour[2..];
        return format!("Every {} hours", interval);
    }

    if min == "0" && day == "*" && dow == "*" {
        if let Ok(h) = hour.parse::<u32>() {
            let (display_hour, ampm) = if h == 0 {
                (12, "am")
            } else if h < 12 {
                (h, "am")
            } else if h == 12 {
                (12, "pm")
            } else {
                (h - 12, "pm")
            };
            return format!("Daily at {}{} UTC", display_hour, ampm);
        }
    }

    if min.starts_with("*/") && hour == "*" && day == "*" && dow == "*" {
        let interval = &min[2..];
        return format!("Every {} minutes", interval);
    }

    format!("Custom schedule: {}", cron)
}

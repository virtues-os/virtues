//! setup_action tool implementation
//!
//! Turns the current chat into an action by setting instruction fields on app_chats.
//! If activation_code is provided, it runs a dry-run test first so the LLM can fix errors.

use sqlx::SqlitePool;

use super::executor::{ToolContext, ToolError, ToolResult};
use crate::api::code::{execute_code, ExecuteCodeRequest};

/// Execute the setup_action tool
pub async fn execute(
    pool: &SqlitePool,
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

    let instruction = arguments
        .get("instruction")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::InvalidParameters("instruction is required".into()))?;

    let cron_schedule = arguments
        .get("cron_schedule")
        .and_then(|v| v.as_str());

    let endpoint = arguments
        .get("endpoint")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let activation_code = arguments
        .get("activation_code")
        .and_then(|v| v.as_str());

    // Basic cron expression validation
    if let Some(cron) = cron_schedule {
        let fields: Vec<&str> = cron.split_whitespace().collect();
        if fields.len() < 5 || fields.len() > 6 {
            return Err(ToolError::InvalidParameters(format!(
                "Invalid cron expression '{}': expected 5-field (min hour day month dow) or 6-field (sec min hour day month dow) format",
                cron
            )));
        }
    }

    // Dry-run test activation code if provided — the LLM can fix errors before we save
    let mut dry_run_output: Option<String> = None;
    if let Some(code) = activation_code {
        let response = execute_code(ExecuteCodeRequest {
            code: code.to_string(),
            timeout: 30,
        })
        .await;

        if !response.success {
            let err_detail = response.error.unwrap_or_default();
            let stderr = response.stderr;
            return Ok(ToolResult::error(format!(
                "Activation code dry-run failed. Fix the code and call setup_action again.\n\nError: {}\nStderr:\n{}",
                err_detail, stderr
            )));
        }

        let stdout = response.stdout.trim().to_string();
        tracing::info!(stdout = %stdout, "Activation code dry-run passed");
        dry_run_output = Some(stdout);
    }

    // Must have either a cron schedule or endpoint
    if cron_schedule.is_none() && !endpoint {
        return Err(ToolError::InvalidParameters(
            "Action must have either a cron_schedule or endpoint=true".into(),
        ));
    }

    // Generate trigger token for endpoint actions
    let trigger_token = if endpoint {
        Some(generate_trigger_token())
    } else {
        None
    };

    // Build config for the scheduled_task
    let mut config = serde_json::json!({ "chat_id": chat_id });
    if let Some(ref token) = trigger_token {
        config["trigger_token"] = serde_json::json!(token);
    }

    let action_id = format!("action_agent_{}", chat_id);

    // Upsert action with instruction on the action record (not the chat)
    sqlx::query(
        r#"
        INSERT INTO app_actions (id, action_type, owner, name, instruction, cron_schedule, enabled, config, activation_code)
        VALUES (?, 'agent', 'user', ?, ?, ?, 1, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            instruction = excluded.instruction,
            cron_schedule = excluded.cron_schedule,
            enabled = 1,
            config = excluded.config,
            activation_code = excluded.activation_code,
            updated_at = datetime('now')
        "#,
    )
    .bind(&action_id)
    .bind(name)
    .bind(instruction)
    .bind(cron_schedule)
    .bind(&config)
    .bind(activation_code)
    .execute(pool)
    .await
    .map_err(|e| ToolError::ExecutionFailed(format!("Failed to create action task: {}", e)))?;

    // Update chat title
    sqlx::query(
        r#"
        UPDATE app_chats
        SET title = ?,
            updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(name)
    .bind(chat_id)
    .execute(pool)
    .await
    .map_err(|e| ToolError::ExecutionFailed(format!("Failed to update chat: {}", e)))?;

    let action_state = if cron_schedule.is_some() { "scheduled" } else { "listening" };

    tracing::info!(
        chat_id,
        name,
        state = action_state,
        "Action configured"
    );

    // Build response
    let mut response = serde_json::json!({
        "action_name": name,
        "action_state": action_state,
        "has_activation": activation_code.is_some(),
    });

    if let Some(cron) = cron_schedule {
        response["cron_schedule"] = serde_json::json!(cron);
        response["schedule_description"] = serde_json::json!(describe_cron(cron));
    }

    if let Some(ref token) = trigger_token {
        response["trigger_token"] = serde_json::json!(token);
        response["trigger_url"] = serde_json::json!(format!("/api/actions/{}/run", action_id));
    }

    if let Some(ref output) = dry_run_output {
        response["activation_dry_run"] = serde_json::json!(output);
    }

    Ok(ToolResult::success(response))
}

/// Generate a random trigger token for endpoint actions
fn generate_trigger_token() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    let bytes: [u8; 24] = rng.random();
    hex::encode(bytes)
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

    if min == "0" && hour.contains(',') && day == "*" {
        let hours: Vec<&str> = hour.split(',').collect();
        let time_parts: Vec<String> = hours.iter().filter_map(|h| {
            h.parse::<u32>().ok().map(|h| {
                let (dh, ap) = if h == 0 { (12, "am") } else if h < 12 { (h, "am") } else if h == 12 { (12, "pm") } else { (h - 12, "pm") };
                format!("{}{}", dh, ap)
            })
        }).collect();
        if !time_parts.is_empty() {
            let times = time_parts.join(", ");
            let days = match dow {
                "6,0" | "0,6" => "Sat & Sun".to_string(),
                "1-5" => "weekdays".to_string(),
                "*" => "daily".to_string(),
                _ => format!("days {}", dow),
            };
            return format!("{} at {} UTC", days, times);
        }
    }

    format!("Custom schedule: {}", cron)
}

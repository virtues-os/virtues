//! dayline_event tool — scoped event CRUD for dayline actions.
//!
//! Used by the hourly and EOD actions to create, continue, revise, or mark
//! events as no-data. This is a structured tool with known-safe operations —
//! actions use this instead of raw sql_query for writes.

use chrono::{DateTime, Utc};
use sqlx::PgPool;

use super::executor::{ToolContext, ToolError, ToolResult};

/// Execute the dayline_event tool.
pub async fn execute(
    pool: &PgPool,
    arguments: serde_json::Value,
    context: &ToolContext,
) -> Result<ToolResult, ToolError> {
    tracing::debug!(
        user_id = ?context.user_id,
        chat_id = ?context.chat_id,
        "dayline_event tool invoked"
    );
    let action = arguments
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::InvalidParameters("action is required".into()))?;

    match action {
        "NEW" => create_event(pool, &arguments).await,
        "CONTINUE" => continue_event(pool, &arguments).await,
        "REVISE" => revise_event(pool, &arguments).await,
        "NO_DATA" => mark_no_data(pool, &arguments).await,
        _ => Err(ToolError::InvalidParameters(format!(
            "Unknown action '{}'. Must be NEW, CONTINUE, REVISE, or NO_DATA.",
            action
        ))),
    }
}

/// NEW: Create a new event from this hour's data.
async fn create_event(
    pool: &PgPool,
    args: &serde_json::Value,
) -> Result<ToolResult, ToolError> {
    let event_summary = args
        .get("event_summary")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::InvalidParameters("event_summary is required for NEW".into()))?;

    let start_time = args
        .get("start_time")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::InvalidParameters("start_time is required for NEW".into()))?;

    let end_time = args
        .get("end_time")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::InvalidParameters("end_time is required for NEW".into()))?;

    let topics = args
        .get("topics")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));

    let auto_label = args
        .get("auto_label")
        .and_then(|v| v.as_str())
        .unwrap_or("Event");

    let source_ontologies = args
        .get("source_ontologies")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));

    // Resolve the day — parse the date from start_time
    let start_dt: DateTime<Utc> = start_time
        .parse()
        .map_err(|e| ToolError::InvalidParameters(format!("Invalid start_time: {e}")))?;
    let date_str = start_dt.format("%Y-%m-%d").to_string();

    // Get or create the day
    let day_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM wiki_days WHERE date = $1::date",
    )
    .bind(&date_str)
    .fetch_optional(pool)
    .await
    .map_err(|e| ToolError::ExecutionFailed(format!("DB error: {e}")))?;

    let day_id = match day_id {
        Some(id) => id,
        None => {
            let id = format!("day_{}", date_str);
            sqlx::query("INSERT INTO wiki_days (id, date) VALUES ($1, $2::date)")
                .bind(&id)
                .bind(&date_str)
                .execute(pool)
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!("Failed to create day: {e}")))?;
            id
        }
    };

    // Generate event ID
    let event_id = crate::ids::generate_id("ev", &[&date_str, start_time]);

    let topics_json = serde_json::to_string(&topics).unwrap_or_else(|_| "[]".to_string());
    let sources_json = serde_json::to_string(&source_ontologies).unwrap_or_else(|_| "[]".to_string());

    sqlx::query(
        r#"
        INSERT INTO wiki_events (
            id, day_id, start_time, end_time,
            auto_label, event_summary, topics, source_ontologies,
            agent_action
        ) VALUES ($1, $2, $3::timestamptz, $4::timestamptz, $5, $6, $7::jsonb, $8::jsonb, 'NEW')
        "#,
    )
    .bind(&event_id)
    .bind(&day_id)
    .bind(start_time)
    .bind(end_time)
    .bind(auto_label)
    .bind(event_summary)
    .bind(&topics_json)
    .bind(&sources_json)
    .execute(pool)
    .await
    .map_err(|e| ToolError::ExecutionFailed(format!("Failed to create event: {e}")))?;

    // Trigger novelty computation for this event
    let date = start_dt.date_naive();
    tokio::spawn({
        let pool = pool.clone();
        let eid = event_id.clone();
        let summary = event_summary.to_string();
        async move {
            if let Err(e) = crate::dayline::novelty::compute_and_store_novelty(
                &pool, &eid, &summary, date,
            ).await {
                tracing::warn!(event_id = eid, error = %e, "Failed to compute novelty for new event");
            }
        }
    });

    Ok(ToolResult::success(serde_json::json!({
        "event_id": event_id,
        "action": "NEW",
        "day_id": day_id,
    })))
}

/// CONTINUE: Extend the current event's end_time and update its summary.
async fn continue_event(
    pool: &PgPool,
    args: &serde_json::Value,
) -> Result<ToolResult, ToolError> {
    let event_id = args
        .get("event_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::InvalidParameters("event_id is required for CONTINUE".into()))?;

    let end_time = args
        .get("end_time")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::InvalidParameters("end_time is required for CONTINUE".into()))?;

    let event_summary = args
        .get("event_summary")
        .and_then(|v| v.as_str());

    let topics = args.get("topics");

    // Build the SET clause with `$N` placeholders (Postgres). `start_time`/
    // `end_time` are TIMESTAMPTZ and `topics` is JSONB, so the placeholders we
    // bind string values to are cast; `WHERE id` takes the final placeholder.
    let mut sets: Vec<String> = Vec::new();
    let mut binds: Vec<String> = Vec::new();
    let mut idx = 0u32;
    let mut next = || {
        idx += 1;
        idx
    };

    sets.push(format!("end_time = ${}::timestamptz", next()));
    binds.push(end_time.to_string());
    sets.push("agent_action = 'CONTINUE'".to_string());

    if let Some(summary) = event_summary {
        sets.push(format!("event_summary = ${}", next()));
        binds.push(summary.to_string());
    }

    if let Some(t) = topics {
        let topics_json = serde_json::to_string(t).unwrap_or_else(|_| "[]".to_string());
        sets.push(format!("topics = ${}::jsonb", next()));
        binds.push(topics_json);
    }

    // Also reset novelty so it gets recomputed with the updated summary
    sets.push("novelty_z = NULL".to_string());
    sets.push("embedding = NULL".to_string());
    sets.push("updated_at = now()".to_string());

    let id_param = next();
    let query = format!(
        "UPDATE wiki_events SET {} WHERE id = ${}",
        sets.join(", "),
        id_param
    );
    binds.push(event_id.to_string());

    // Build and execute the dynamic query
    let mut q = sqlx::query(&query);
    for b in &binds {
        q = q.bind(b);
    }

    let result = q
        .execute(pool)
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to continue event: {e}")))?;

    if result.rows_affected() == 0 {
        return Err(ToolError::ExecutionFailed(format!(
            "Event not found: {event_id}"
        )));
    }

    // Recompute novelty if summary was updated
    if let Some(summary) = event_summary {
        let end_dt: Result<DateTime<Utc>, _> = end_time.parse();
        if let Ok(dt) = end_dt {
            let pool = pool.clone();
            let eid = event_id.to_string();
            let summary = summary.to_string();
            let date = dt.date_naive();
            tokio::spawn(async move {
                let _ = crate::dayline::novelty::compute_and_store_novelty(
                    &pool, &eid, &summary, date,
                ).await;
            });
        }
    }

    Ok(ToolResult::success(serde_json::json!({
        "event_id": event_id,
        "action": "CONTINUE",
    })))
}

/// REVISE: Update an existing event (merge, split, or modify).
async fn revise_event(
    pool: &PgPool,
    args: &serde_json::Value,
) -> Result<ToolResult, ToolError> {
    let event_id = args
        .get("event_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::InvalidParameters("event_id is required for REVISE".into()))?;

    // Check the event isn't user-edited (protected)
    let is_user_edited: Option<bool> = sqlx::query_scalar(
        "SELECT CASE WHEN is_user_edited = TRUE THEN 1 ELSE 0 END FROM wiki_events WHERE id = $1",
    )
    .bind(event_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| ToolError::ExecutionFailed(format!("DB error: {e}")))?;

    if is_user_edited == Some(true) {
        return Ok(ToolResult::error(format!(
            "Event {} is user-edited and cannot be revised by the agent. The user must clear their edit first.",
            event_id
        )));
    }

    // Apply updates
    let event_summary = args.get("event_summary").and_then(|v| v.as_str());
    let start_time = args.get("start_time").and_then(|v| v.as_str());
    let end_time = args.get("end_time").and_then(|v| v.as_str());
    let auto_label = args.get("auto_label").and_then(|v| v.as_str());
    let topics = args.get("topics");

    let mut sets: Vec<String> = vec!["agent_action = 'REVISE'".to_string()];
    let mut binds: Vec<String> = Vec::new();
    let mut idx = 0u32;
    let mut next = || {
        idx += 1;
        idx
    };

    if let Some(v) = event_summary {
        sets.push(format!("event_summary = ${}", next()));
        binds.push(v.to_string());
        // Reset novelty for recomputation
        sets.push("novelty_z = NULL".to_string());
        sets.push("embedding = NULL".to_string());
    }
    if let Some(v) = start_time {
        sets.push(format!("start_time = ${}::timestamptz", next()));
        binds.push(v.to_string());
    }
    if let Some(v) = end_time {
        sets.push(format!("end_time = ${}::timestamptz", next()));
        binds.push(v.to_string());
    }
    if let Some(v) = auto_label {
        sets.push(format!("auto_label = ${}", next()));
        binds.push(v.to_string());
    }
    if let Some(t) = topics {
        sets.push(format!("topics = ${}::jsonb", next()));
        binds.push(serde_json::to_string(t).unwrap_or_else(|_| "[]".to_string()));
    }

    sets.push("updated_at = now()".to_string());

    let id_param = next();
    let query = format!(
        "UPDATE wiki_events SET {} WHERE id = ${}",
        sets.join(", "),
        id_param
    );
    binds.push(event_id.to_string());

    let mut q = sqlx::query(&query);
    for b in &binds {
        q = q.bind(b);
    }

    let result = q
        .execute(pool)
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to revise event: {e}")))?;

    if result.rows_affected() == 0 {
        return Err(ToolError::ExecutionFailed(format!(
            "Event not found: {event_id}"
        )));
    }

    // Trigger novelty recompute if summary changed
    if let Some(summary) = event_summary {
        // Resolve the event's date: prefer provided times, fall back to DB lookup
        let event_date = if let Some(et) = end_time.or(start_time) {
            et.parse::<DateTime<Utc>>().ok().map(|dt| dt.date_naive())
        } else {
            // No time params provided — look up the event's existing start_time
            sqlx::query_scalar::<_, String>(
                "SELECT start_time::text FROM wiki_events WHERE id = $1"
            )
            .bind(event_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .and_then(|st| st.parse::<DateTime<Utc>>().ok().map(|dt| dt.date_naive()))
        };

        if let Some(date) = event_date {
            let pool = pool.clone();
            let eid = event_id.to_string();
            let summary = summary.to_string();
            tokio::spawn(async move {
                let _ = crate::dayline::novelty::compute_and_store_novelty(
                    &pool, &eid, &summary, date,
                ).await;
            });
        }
    }

    Ok(ToolResult::success(serde_json::json!({
        "event_id": event_id,
        "action": "REVISE",
    })))
}

/// NO_DATA: Mark this hour as unknown — insufficient signal.
/// If the most recent event on this day is already unknown, extend it
/// rather than creating a new one (avoids 24 unknown events on quiet days).
async fn mark_no_data(
    pool: &PgPool,
    args: &serde_json::Value,
) -> Result<ToolResult, ToolError> {
    let start_time = args
        .get("start_time")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::InvalidParameters("start_time is required for NO_DATA".into()))?;

    let end_time = args
        .get("end_time")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::InvalidParameters("end_time is required for NO_DATA".into()))?;

    let start_dt: DateTime<Utc> = start_time
        .parse()
        .map_err(|e| ToolError::InvalidParameters(format!("Invalid start_time: {e}")))?;
    let date_str = start_dt.format("%Y-%m-%d").to_string();

    // Check if there's a temporally adjacent unknown event — extend it instead of creating a new one.
    // Only extend if the previous unknown's end_time matches our start_time (contiguous).
    let prev_unknown: Option<String> = sqlx::query_scalar(
        r#"SELECT e.id FROM wiki_events e
           JOIN wiki_days d ON e.day_id = d.id
           WHERE d.date = $1::date AND e.is_unknown = TRUE AND e.end_time = $2::timestamptz
           ORDER BY e.end_time DESC LIMIT 1"#,
    )
    .bind(&date_str)
    .bind(start_time)
    .fetch_optional(pool)
    .await
    .map_err(|e| ToolError::ExecutionFailed(format!("DB error: {e}")))?;

    if let Some(prev_id) = prev_unknown {
        // Extend the existing unknown event
        sqlx::query("UPDATE wiki_events SET end_time = $1::timestamptz, updated_at = now() WHERE id = $2")
            .bind(end_time)
            .bind(&prev_id)
            .execute(pool)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to extend unknown event: {e}")))?;

        return Ok(ToolResult::success(serde_json::json!({
            "event_id": prev_id,
            "action": "NO_DATA",
            "extended": true,
        })));
    }

    // No existing unknown event — create a new one

    // Get or create day
    let day_id: Option<String> = sqlx::query_scalar("SELECT id FROM wiki_days WHERE date = $1::date")
        .bind(&date_str)
        .fetch_optional(pool)
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("DB error: {e}")))?;

    let day_id = match day_id {
        Some(id) => id,
        None => {
            let id = format!("day_{}", date_str);
            sqlx::query("INSERT INTO wiki_days (id, date) VALUES ($1, $2::date)")
                .bind(&id)
                .bind(&date_str)
                .execute(pool)
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!("Failed to create day: {e}")))?;
            id
        }
    };

    let event_id = crate::ids::generate_id("ev", &[&date_str, start_time, "unknown"]);

    sqlx::query(
        r#"
        INSERT INTO wiki_events (
            id, day_id, start_time, end_time,
            auto_label, kind, agent_action
        ) VALUES ($1, $2, $3::timestamptz, $4::timestamptz, 'Unknown', 'unknown', 'NO_DATA')
        "#,
    )
    .bind(&event_id)
    .bind(&day_id)
    .bind(start_time)
    .bind(end_time)
    .execute(pool)
    .await
    .map_err(|e| ToolError::ExecutionFailed(format!("Failed to create no-data event: {e}")))?;

    Ok(ToolResult::success(serde_json::json!({
        "event_id": event_id,
        "action": "NO_DATA",
    })))
}

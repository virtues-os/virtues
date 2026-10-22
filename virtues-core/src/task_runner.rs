//! The request worker: works tasks assigned to the assistant in the
//! background (agents/plan/tasks-plan.md).
//!
//! Once a minute, and straight after each run, it takes the next request
//! that is due (`api::tasks::claim_next_request`) and runs one headless pass
//! of the agent loop in the request's own work chat. The run's reply becomes
//! the request's result and the request waits as `ready` until the owner
//! accepts it. Requests run one at a time.

use std::time::Duration;

use sqlx::PgPool;

use crate::agent::headless::{model_for_slot, run_headless, HeadlessJob};
use crate::api::tasks::{self, Actor, Task, Writer};
use crate::applet_runner::limits::format_usd;
use crate::error::Result;
use crate::server::yjs::YjsState;
use virtues_registry::models::ModelSlot;

/// How long the worker waits when nothing is due.
const IDLE: Duration = Duration::from_secs(60);

/// The wake for a run the owner said nothing new to, such as a retry after
/// a failed run.
const CARRY_ON: &str = "Please carry on with this request.";

/// `app_ai_calls.feature` for request runs.
const FEATURE: &str = "task";

pub fn spawn(pool: PgPool, yjs_state: YjsState) {
    tokio::spawn(async move {
        match tasks::requeue_interrupted_requests(&pool).await {
            Ok(0) => {}
            Ok(n) => tracing::info!(n, "tasks: requeued requests a restart interrupted"),
            Err(e) => tracing::error!("tasks: could not requeue interrupted requests: {e}"),
        }
        loop {
            match run_next(&pool, &yjs_state).await {
                Ok(true) => continue,
                Ok(false) => {}
                Err(e) => tracing::error!("tasks: request run failed: {e}"),
            }
            tokio::time::sleep(IDLE).await;
        }
    });
}

/// Run the next due request. `Ok(false)` when nothing ran. The owner's
/// spending limits live on the assistant profile (Settings, Assistant). When
/// the day's limit is reached, requests stay queued until the day's spend
/// falls under it.
async fn run_next(pool: &PgPool, yjs_state: &YjsState) -> Result<bool> {
    let profile = crate::api::assistant_profile::get_assistant_profile(pool).await?;
    let spent = spend_last_day(pool).await?;
    if spent >= profile.request_day_limit_micros {
        tracing::debug!(spent, "tasks: daily spend ceiling reached; requests wait");
        return Ok(false);
    }
    let Some(task) = tasks::claim_next_request(pool).await? else {
        return Ok(false);
    };
    let id = task.id.clone();
    if let Err(e) = run_request(pool, yjs_state, task, profile.request_run_limit_micros).await {
        // Anything that broke outside the model (the database, the chat)
        // counts against the request the same way a model failure does, so a
        // request never stays active with nothing working on it.
        let writer = Writer { author: Actor::Assistant, chat_id: None, from_request: true };
        tasks::fail_request_run(pool, &id, &e.to_string(), &writer).await?;
    }
    Ok(true)
}

async fn run_request(pool: &PgPool, yjs_state: &YjsState, task: Task, run_limit: i64) -> Result<()> {
    let first_run = task.chat_id.is_none();
    let chat_id = match &task.chat_id {
        Some(id) => id.clone(),
        None => {
            let chat = crate::api::chats::create_chat(pool, &task.title, Vec::new()).await?;
            tasks::set_request_chat(pool, &task.id, &chat.id).await?;
            chat.id
        }
    };
    let writer = Writer { author: Actor::Assistant, chat_id: Some(chat_id.clone()), from_request: true };

    let message = if first_run {
        brief(&task)
    } else {
        owner_replies(pool, &task.id).await?.unwrap_or_else(|| CARRY_ON.to_string())
    };

    let tool_context = crate::tools::ToolContext {
        user_id: Some("system".to_string()),
        chat_id: Some(chat_id.clone()),
        task_id: Some(task.id.clone()),
        project_id: task.project_id.clone(),
        turn: Some(crate::api::chat::generate_id()),
        ..Default::default()
    };
    // A request's result is prose the owner reads, so it gets the model chat
    // uses, not the background one.
    let model = model_for_slot(pool, ModelSlot::Standard).await;

    let outcome = run_headless(
        pool,
        yjs_state,
        HeadlessJob {
            label: &task.id,
            model,
            system_prompt: system_prompt(pool, &task).await,
            chat_id: Some(chat_id),
            message: Some(&message),
            tools: crate::tools::get_tools_for_request(),
            tool_context,
            max_cost_micros: Some(run_limit),
            feature: FEATURE,
            applet_run_id: None,
        },
    )
    .await?;

    if let Some(error) = outcome.error {
        tasks::fail_request_run(pool, &task.id, &error, &writer).await?;
    } else if outcome.budget_stopped.is_some() {
        let said = outcome.message.unwrap_or_default();
        let result = format!(
            "{said}\n\nI stopped here because this request reached its spending limit of {} for one run. \
             Reply to let me continue, or raise the limit in Settings under Assistant.",
            format_usd(run_limit)
        );
        tasks::finish_request_run(pool, &task.id, result.trim(), &writer).await?;
    } else if let Some(result) = outcome.message.filter(|m| !m.trim().is_empty()) {
        tasks::finish_request_run(pool, &task.id, &result, &writer).await?;
    } else {
        tasks::fail_request_run(pool, &task.id, "The run ended without an answer.", &writer).await?;
    }
    Ok(())
}

/// The first message in a request's work chat: the request as the owner
/// wrote it.
fn brief(task: &Task) -> String {
    let mut out = task.title.clone();
    if let Some(notes) = &task.notes {
        out.push_str("\n\n");
        out.push_str(notes);
    }
    out
}

/// What the owner said since the last run started: their notes, in order.
/// `None` when they said nothing new, as on a retry after a failed run.
async fn owner_replies(pool: &PgPool, task_id: &str) -> Result<Option<String>> {
    let events = tasks::list_events(pool, task_id).await?;
    // The newest start is this run's own; the one before it is the last run.
    let starts: Vec<_> = events
        .iter()
        .filter(|e| e.kind == "status" && e.body.get("to").and_then(|v| v.as_str()) == Some("active"))
        .map(|e| e.created_at)
        .collect();
    let since = starts.len().checked_sub(2).map(|i| starts[i]);
    let notes: Vec<&str> = events
        .iter()
        .filter(|e| e.kind == "note" && e.author == "owner")
        .filter(|e| since.map_or(true, |at| e.created_at > at))
        .filter_map(|e| e.body.get("text").and_then(|v| v.as_str()))
        .collect();
    Ok((!notes.is_empty()).then(|| notes.join("\n\n")))
}

async fn system_prompt(pool: &PgPool, task: &Task) -> String {
    let assistant_name = crate::api::assistant_profile::get_assistant_name(pool)
        .await
        .unwrap_or_else(|_| "Ari".to_string());
    let user_name = crate::api::profile::get_display_name(pool)
        .await
        .unwrap_or_else(|_| "there".to_string());
    let datetime = crate::api::profile::local_datetime_line(pool).await;
    let due = match (task.due_on, task.due_at) {
        (_, Some(at)) => format!("\nWanted by: {}", at.to_rfc3339()),
        (Some(day), None) => format!("\nWanted by: {day}"),
        _ => String::new(),
    };

    format!(
        "You are {assistant_name}, {user_name}'s personal AI assistant. {user_name} left you a \
         request on their task list, and you are working on it in the background while they are away.\n\n\
         Current date/time: {datetime}\n\n\
         <request id=\"{id}\">\n{title}{due}\n</request>\n\n\
         Use your tools to do the work. Your final reply is the result {user_name} reads on their task \
         list, so write it to them: complete, self-contained, and in plain prose. If you wrote a page \
         for it, call update_task with result_url. If you cannot finish without something from them, \
         say exactly what you need. Do not mark the request done; {user_name} accepts it.",
        id = task.id,
        title = task.title,
    )
}

/// Wallet spend on request runs in the last 24 hours, in micros-USD. BYO
/// calls report no cost, so they never count against the ceiling.
async fn spend_last_day(pool: &PgPool) -> Result<i64> {
    let total: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(cost_micros), 0)::bigint FROM app_ai_calls
         WHERE feature = $1 AND route = 'wallet' AND created_at > now() - interval '24 hours'",
    )
    .bind(FEATURE)
    .fetch_one(pool)
    .await?;
    Ok(total)
}

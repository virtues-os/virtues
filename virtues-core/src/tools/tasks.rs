//! `create_task` and `update_task`: the assistant's hands on the task list.
//!
//! Both write through `api::tasks`, signed by the assistant and tagged with
//! the chat they came from, so every change lands in the task's history.
//! A task assigned to the assistant is a request: `api::tasks` queues it and
//! `crate::task_runner` works it in the background.
//! Reading the list is `sql_query` against `app_tasks`. Neither tool asks
//! first: a task is cheap to undo and its history shows who did what.

use chrono::{NaiveDate, NaiveDateTime, TimeZone};
use serde_json::{json, Value};
use sqlx::PgPool;

use super::{ToolContext, ToolResult};
use crate::api::tasks::{self, Actor, NewTask, Origin, Status, TaskPatch, Writer};
use crate::types::Timestamp;

pub const CREATE_TOOL: &str = "create_task";
pub const UPDATE_TOOL: &str = "update_task";

/// More than anyone asks for in one breath; a longer list is a loop.
const MAX_PER_CALL: usize = 20;

fn writer(context: &ToolContext) -> Writer {
    Writer {
        author: Actor::Assistant,
        chat_id: context.chat_id.clone(),
        from_request: context.task_id.is_some(),
    }
}

pub async fn create(pool: &PgPool, args: &Value, context: &ToolContext) -> ToolResult {
    let Some(items) = args.get("tasks").and_then(Value::as_array) else {
        return ToolResult::error("Pass `tasks`: a list with one entry per task.");
    };
    if items.is_empty() || items.len() > MAX_PER_CALL {
        return ToolResult::error(format!("Pass between 1 and {MAX_PER_CALL} tasks."));
    }
    let origin = match args.get("origin").and_then(Value::as_str) {
        Some("own") => Origin::Assistant,
        _ => Origin::Chat,
    };

    let mut new = Vec::with_capacity(items.len());
    for item in items {
        let Some(title) = item.get("title").and_then(Value::as_str) else {
            return ToolResult::error("Every task needs a `title`.");
        };
        let (due_on, due_at) = match item.get("due").and_then(Value::as_str) {
            Some(raw) => match parse_due(raw, context.timezone.as_deref()) {
                Ok(due) => due,
                Err(e) => return ToolResult::error(e),
            },
            None => (None, None),
        };
        let assignee = match item.get("for").and_then(Value::as_str) {
            Some("assistant") => Actor::Assistant,
            _ => Actor::Owner,
        };
        new.push(NewTask {
            title: title.to_string(),
            notes: item.get("notes").and_then(Value::as_str).map(str::to_string),
            assignee: Some(assignee),
            due_on,
            due_at,
            project_id: item
                .get("project_id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| context.project_id.clone()),
            source_url: context.chat_id.as_ref().map(|id| format!("/chat/{id}")),
        });
    }

    match tasks::create_tasks(pool, new, origin, &writer(context)).await {
        Ok(created) => ToolResult::success(json!({ "tasks": created })),
        Err(e) => ToolResult::error(e.to_string()),
    }
}

pub async fn update(pool: &PgPool, args: &Value, context: &ToolContext) -> ToolResult {
    let Some(id) = args.get("id").and_then(Value::as_str) else {
        return ToolResult::error("Pass the task's `id` (from app_tasks).");
    };

    let mut patch = TaskPatch::default();
    if let Some(title) = args.get("title").and_then(Value::as_str) {
        patch.title = Some(title.to_string());
    }
    if let Some(notes) = args.get("notes").and_then(Value::as_str) {
        patch.notes = Some(Some(notes.to_string()).filter(|n| !n.trim().is_empty()));
    }
    if let Some(raw) = args.get("due").and_then(Value::as_str) {
        if raw.trim().is_empty() || raw == "none" {
            patch.due_on = Some(None);
        } else {
            match parse_due(raw, context.timezone.as_deref()) {
                Ok((on, at)) => {
                    patch.due_on = Some(on);
                    patch.due_at = Some(at);
                }
                Err(e) => return ToolResult::error(e),
            }
        }
    }
    if let Some(project) = args.get("project_id").and_then(Value::as_str) {
        patch.project_id = Some(Some(project.to_string()).filter(|p| !p.is_empty() && p != "none"));
    }
    if let Some(who) = args.get("for").and_then(Value::as_str) {
        patch.assignee = Some(if who == "assistant" { Actor::Assistant } else { Actor::Owner });
    }
    if let Some(status) = args.get("status").and_then(Value::as_str) {
        patch.status = Some(match status {
            "open" => Status::Open,
            "active" => Status::Active,
            "ready" => Status::Ready,
            "done" => Status::Done,
            "dismissed" => Status::Dismissed,
            other => {
                return ToolResult::error(format!(
                    "unknown status '{other}' (open | active | ready | done | dismissed)"
                ))
            }
        });
    }
    if let Some(result) = args.get("result").and_then(Value::as_str) {
        patch.result = Some(result.to_string());
    }
    if let Some(url) = args.get("result_url").and_then(Value::as_str) {
        patch.result_url = Some(Some(url.to_string()).filter(|u| !u.trim().is_empty()));
    }
    let note = args.get("note").and_then(Value::as_str).map(str::trim).filter(|n| !n.is_empty());

    let writer = writer(context);
    // The note first, so a note explaining a status change reads before it.
    if let Some(note) = note {
        if let Err(e) = tasks::add_note(pool, id, note, &writer).await {
            return ToolResult::error(e.to_string());
        }
    }
    match tasks::update_task(pool, id, patch, &writer).await {
        Ok(task) => ToolResult::success(json!({ "task": task })),
        Err(e) => ToolResult::error(e.to_string()),
    }
}

/// A due date as the model writes it: a day (`2026-10-23`), or a day and a
/// local time in the owner's zone (`2026-10-23T17:00`).
fn parse_due(raw: &str, tz: Option<&str>) -> Result<(Option<NaiveDate>, Option<Timestamp>), String> {
    let raw = raw.trim();
    if let Ok(day) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        return Ok((Some(day), None));
    }
    let local = ["%Y-%m-%dT%H:%M", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M"]
        .iter()
        .find_map(|f| NaiveDateTime::parse_from_str(raw, f).ok())
        .ok_or_else(|| format!("\"{raw}\" is not a date. Pass `due` as 2026-10-23, or 2026-10-23T17:00 for a time."))?;
    let zone: chrono_tz::Tz = tz.and_then(|t| t.parse().ok()).unwrap_or(chrono_tz::UTC);
    let at = zone
        .from_local_datetime(&local)
        .earliest()
        .ok_or_else(|| format!("{raw} does not exist in {zone} (the clocks skip it). Pick another time."))?;
    Ok((Some(local.date()), Some(Timestamp::from_utc(at.with_timezone(&chrono::Utc)))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn due_reads_a_day_or_a_local_time() {
        let (on, at) = parse_due("2030-01-04", None).unwrap();
        assert_eq!(on, NaiveDate::from_ymd_opt(2030, 1, 4));
        assert!(at.is_none());

        // 17:00 in New York in January is 22:00 UTC, and the day stays local.
        let (on, at) = parse_due("2030-01-04T17:00", Some("America/New_York")).unwrap();
        assert_eq!(on, NaiveDate::from_ymd_opt(2030, 1, 4));
        assert_eq!(at.unwrap().to_rfc3339(), "2030-01-04T22:00:00Z");

        assert!(parse_due("friday", None).is_err());
    }

    /// The calls a chat makes: three tasks in one call, then the assistant
    /// working one of them. Every step is signed by the assistant and tagged
    /// with the chat, and the note reads before the status it explains.
    #[sqlx::test(migrations = "./migrations")]
    async fn the_assistant_adds_and_works_tasks(pool: PgPool) {
        let ctx = ToolContext { chat_id: Some("chat_t".into()), ..Default::default() };
        let made = create(
            &pool,
            &json!({ "tasks": [
                { "title": "Renew passport" },
                { "title": "Call Nick", "due": "2030-01-04" },
                { "title": "Check whether the reply came in", "for": "assistant" }
            ], "origin": "asked" }),
            &ctx,
        )
        .await;
        assert!(made.success, "{:?}", made.error);
        let tasks = made.data["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[1]["due_on"], "2030-01-04");
        assert_eq!(tasks[2]["assignee"], "assistant");
        assert_eq!(tasks[0]["source_url"], "/chat/chat_t");
        assert_eq!(tasks[0]["origin"], "chat");

        let id = tasks[2]["id"].as_str().unwrap();
        assert!(tasks[2]["next_run_at"].is_string(), "a new request is queued");
        assert!(tasks[0]["next_run_at"].is_null(), "the owner's to-do never runs");

        // The assistant never closes its own request: it hands back a result.
        let closed = update(&pool, &json!({ "id": id, "status": "done" }), &ctx).await;
        assert!(!closed.success);
        let bare = update(&pool, &json!({ "id": id, "status": "ready" }), &ctx).await;
        assert!(!bare.success, "ready needs a result");
        let ready = update(
            &pool,
            &json!({ "id": id, "status": "ready", "result": "No reply yet.", "note": "Asked again." }),
            &ctx,
        )
        .await;
        assert!(ready.success, "{:?}", ready.error);
        assert_eq!(ready.data["task"]["status"], "ready");
        assert_eq!(ready.data["task"]["result"], "No reply yet.");
        assert!(ready.data["task"]["next_run_at"].is_null(), "a ready request is out of the queue");

        let events = crate::api::tasks::list_events(&pool, id).await.unwrap();
        let kinds: Vec<_> = events.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(kinds, ["created", "note", "status"]);
        assert!(events.iter().all(|e| e.author == "assistant" && e.chat_id.as_deref() == Some("chat_t")));

        let bad = update(&pool, &json!({ "id": id, "status": "finished" }), &ctx).await;
        assert!(!bad.success);
        assert!(!create(&pool, &json!({ "tasks": [] }), &ctx).await.success);
    }

    /// A request's own run can add to-dos for the owner but cannot start
    /// another request, by creating one or by handing a to-do over.
    #[sqlx::test(migrations = "./migrations")]
    async fn a_request_run_cannot_start_another_request(pool: PgPool) {
        let run = ToolContext { chat_id: Some("chat_r".into()), task_id: Some("task_r".into()), ..Default::default() };
        let own = create(&pool, &json!({ "tasks": [{ "title": "Look into it", "for": "assistant" }] }), &run).await;
        assert!(!own.success);
        let todo = create(&pool, &json!({ "tasks": [{ "title": "Sign the form" }] }), &run).await;
        assert!(todo.success, "{:?}", todo.error);
        let id = todo.data["tasks"][0]["id"].as_str().unwrap();
        assert!(!update(&pool, &json!({ "id": id, "for": "assistant" }), &run).await.success);
    }
}

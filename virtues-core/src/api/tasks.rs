//! Tasks: one to-do list the owner and the assistant share.
//!
//! Every write goes through this module, which changes the row and appends
//! its `app_task_events` entry in one transaction. The HTTP handlers, the
//! chat tools and the request worker (`crate::task_runner`) all call it, so a
//! task's history is complete whoever acted. See agents/plan/tasks-plan.md.
//!
//! A task assigned to the assistant is a request. Whenever a write leaves a
//! request open, `queue_time` decides when the worker may run it.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sqlx::{PgPool, Postgres, Transaction};

use crate::error::{Error, Result};
use crate::ids::{generate_id, TASK_PREFIX};
use crate::types::Timestamp;

const TASK_COLUMNS: &str = "id, title, notes, assignee, status, due_on, due_at, project_id, \
     origin, source_url, sort_order, closed_at, created_at, updated_at, result, result_url, \
     chat_id, next_run_at, attempts";

/// Failed runs in a row before the worker stops retrying and hands the
/// request back to the owner as ready, with the failure as its result.
pub const MAX_ATTEMPTS: i32 = 3;

/// Who acted: signs every history entry, and is who a task is assigned to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Actor {
    Owner,
    Assistant,
}

impl Actor {
    fn as_str(self) -> &'static str {
        match self {
            Actor::Owner => "owner",
            Actor::Assistant => "assistant",
        }
    }
}

/// Where a task came from: typed into the list, asked for in chat, or added
/// by the assistant for something it committed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    Owner,
    Chat,
    Assistant,
}

impl Origin {
    fn as_str(self) -> &'static str {
        match self {
            Origin::Owner => "owner",
            Origin::Chat => "chat",
            Origin::Assistant => "assistant",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Open,
    Active,
    /// A request's run finished and its result waits for the owner.
    Ready,
    Done,
    Dismissed,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::Active => "active",
            Status::Ready => "ready",
            Status::Done => "done",
            Status::Dismissed => "dismissed",
        }
    }

    fn is_closed(self) -> bool {
        matches!(self, Status::Done | Status::Dismissed)
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub notes: Option<String>,
    pub assignee: String,
    pub status: String,
    pub due_on: Option<NaiveDate>,
    pub due_at: Option<Timestamp>,
    pub project_id: Option<String>,
    pub origin: String,
    pub source_url: Option<String>,
    pub sort_order: f64,
    pub closed_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    /// What a request's run produced, as markdown.
    pub result: Option<String>,
    /// A page or other app URL the run made.
    pub result_url: Option<String>,
    /// The request's own work chat.
    pub chat_id: Option<String>,
    /// When the worker may run the request. Null: it does not run on its own.
    pub next_run_at: Option<Timestamp>,
    /// Failed runs in a row.
    pub attempts: i32,
}

impl Task {
    fn is_queued(&self) -> bool {
        self.assignee == "assistant" && self.status == "open"
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TaskEvent {
    pub id: i64,
    pub task_id: String,
    pub author: String,
    pub kind: String,
    pub body: Value,
    pub chat_id: Option<String>,
    pub created_at: Timestamp,
}

/// Who is writing, and from where. `chat_id` is set when the change came
/// from a chat, so the history can link back to it.
#[derive(Debug, Clone)]
pub struct Writer {
    pub author: Actor,
    pub chat_id: Option<String>,
    /// Set when the write comes from a request's own run. A run may not
    /// create or hand over requests, or one request could start another
    /// without end.
    pub from_request: bool,
}

impl Writer {
    pub fn owner() -> Self {
        Self { author: Actor::Owner, chat_id: None, from_request: false }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListFilter {
    /// `open` (open, active and ready, the default), `closed` (done and
    /// dismissed) or `all`.
    pub status: Option<String>,
    pub assignee: Option<Actor>,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewTask {
    pub title: String,
    pub notes: Option<String>,
    pub assignee: Option<Actor>,
    pub due_on: Option<NaiveDate>,
    pub due_at: Option<Timestamp>,
    pub project_id: Option<String>,
    pub source_url: Option<String>,
}

/// A partial edit. `Some(None)` clears a nullable field; `None` leaves it.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TaskPatch {
    pub title: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub notes: Option<Option<String>>,
    pub assignee: Option<Actor>,
    pub status: Option<Status>,
    #[serde(default, deserialize_with = "double_option")]
    pub due_on: Option<Option<NaiveDate>>,
    #[serde(default, deserialize_with = "double_option")]
    pub due_at: Option<Option<Timestamp>>,
    #[serde(default, deserialize_with = "double_option")]
    pub project_id: Option<Option<String>>,
    pub sort_order: Option<f64>,
    /// Only the assistant writes a result, when it marks a request ready.
    #[serde(skip)]
    pub result: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub result_url: Option<Option<String>>,
}

/// Lets a JSON `null` mean "clear" while an absent key means "leave".
fn double_option<'de, T, D>(de: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

pub async fn list_tasks(pool: &PgPool, filter: &ListFilter) -> Result<Vec<Task>> {
    let statuses: &[&str] = match filter.status.as_deref().unwrap_or("open") {
        "open" => &["open", "active", "ready"],
        "closed" => &["done", "dismissed"],
        "all" => &["open", "active", "ready", "done", "dismissed"],
        other => {
            return Err(Error::InvalidInput(format!(
                "unknown status filter '{other}' (open | closed | all)"
            )))
        }
    };
    let statuses: Vec<String> = statuses.iter().map(|s| s.to_string()).collect();
    let sql = format!(
        "SELECT {TASK_COLUMNS} FROM app_tasks
         WHERE status = ANY($1)
           AND ($2::text IS NULL OR assignee = $2)
           AND ($3::text IS NULL OR project_id = $3)
         ORDER BY closed_at DESC NULLS FIRST, due_on ASC NULLS LAST,
                  due_at ASC NULLS LAST, sort_order ASC, created_at DESC
         LIMIT 1000"
    );
    Ok(sqlx::query_as::<_, Task>(&sql)
        .bind(&statuses)
        .bind(filter.assignee.map(Actor::as_str))
        .bind(&filter.project_id)
        .fetch_all(pool)
        .await?)
}

pub async fn get_task(pool: &PgPool, id: &str) -> Result<Task> {
    let sql = format!("SELECT {TASK_COLUMNS} FROM app_tasks WHERE id = $1");
    sqlx::query_as::<_, Task>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("task not found: {id}")))
}

/// Create one or more tasks in one transaction: all of them or none.
pub async fn create_tasks(
    pool: &PgPool,
    tasks: Vec<NewTask>,
    origin: Origin,
    writer: &Writer,
) -> Result<Vec<Task>> {
    if tasks.is_empty() {
        return Err(Error::InvalidInput("no tasks to create".into()));
    }
    let mut tx = pool.begin().await?;
    // New tasks go to the top of their group.
    let top: f64 = sqlx::query_scalar("SELECT COALESCE(MIN(sort_order), 0) FROM app_tasks")
        .fetch_one(&mut *tx)
        .await?;

    let mut out = Vec::with_capacity(tasks.len());
    let count = tasks.len();
    for (i, new) in tasks.into_iter().enumerate() {
        let title = clean_title(&new.title)?;
        let due_on = due_day(new.due_on, new.due_at);
        let id = generate_id(TASK_PREFIX, &[&uuid::Uuid::new_v4().to_string()]);
        let assignee = new.assignee.unwrap_or(Actor::Owner);
        if assignee == Actor::Assistant && writer.from_request {
            return Err(Error::InvalidInput(
                "a request's run can add tasks for the owner only, not new requests".into(),
            ));
        }
        let next_run_at = (assignee == Actor::Assistant).then(|| queue_time(new.due_at));
        // Keep the order they were given: the first ends up on top.
        let sort_order = top - (count - i) as f64;
        let sql = format!(
            "INSERT INTO app_tasks (id, title, notes, assignee, due_on, due_at, project_id,
                                    origin, source_url, sort_order, next_run_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             RETURNING {TASK_COLUMNS}"
        );
        let task = sqlx::query_as::<_, Task>(&sql)
            .bind(&id)
            .bind(&title)
            .bind(clean_text(new.notes))
            .bind(assignee.as_str())
            .bind(due_on)
            .bind(new.due_at)
            .bind(&new.project_id)
            .bind(origin.as_str())
            .bind(clean_text(new.source_url))
            .bind(sort_order)
            .bind(next_run_at)
            .fetch_one(&mut *tx)
            .await?;
        append_event(&mut tx, &task.id, writer, "created", json!({ "title": title })).await?;
        out.push(task);
    }
    tx.commit().await?;
    Ok(out)
}

pub async fn update_task(
    pool: &PgPool,
    id: &str,
    patch: TaskPatch,
    writer: &Writer,
) -> Result<Task> {
    let mut tx = pool.begin().await?;
    let sql = format!("SELECT {TASK_COLUMNS} FROM app_tasks WHERE id = $1 FOR UPDATE");
    let before = sqlx::query_as::<_, Task>(&sql)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| Error::NotFound(format!("task not found: {id}")))?;

    let mut next = before.clone();
    if let Some(title) = &patch.title {
        next.title = clean_title(title)?;
    }
    if let Some(notes) = patch.notes {
        next.notes = clean_text(notes);
    }
    if let Some(assignee) = patch.assignee {
        if assignee == Actor::Assistant && writer.from_request && before.assignee != "assistant" {
            return Err(Error::InvalidInput(
                "a request's run cannot hand a task to the assistant".into(),
            ));
        }
        next.assignee = assignee.as_str().into();
    }
    if let Some(due_at) = patch.due_at {
        next.due_at = due_at;
    }
    if let Some(due_on) = patch.due_on {
        next.due_on = due_on;
        // Clearing the day clears the time with it.
        if due_on.is_none() {
            next.due_at = None;
        }
    }
    next.due_on = due_day(next.due_on, next.due_at);
    if let Some(project_id) = patch.project_id {
        next.project_id = project_id;
    }
    if let Some(sort_order) = patch.sort_order {
        next.sort_order = sort_order;
    }
    if let Some(status) = patch.status {
        check_status(status, &before, patch.result.as_deref(), writer)?;
        next.status = status.as_str().into();
        next.closed_at = match (status.is_closed(), before.closed_at) {
            (true, Some(at)) => Some(at),
            (true, None) => Some(Timestamp::now()),
            (false, _) => None,
        };
    }

    if let Some(result) = &patch.result {
        next.result = clean_text(Some(result.clone()));
    }
    if let Some(result_url) = patch.result_url {
        next.result_url = clean_text(result_url);
    }
    // A request queues when a write leaves it open and it was not already
    // waiting on the same time. A request that is not open never runs.
    if next.is_queued() {
        if !before.is_queued() || before.due_at != next.due_at {
            next.next_run_at = Some(queue_time(next.due_at));
            next.attempts = 0;
        }
    } else {
        next.next_run_at = None;
    }

    let changes = edited_fields(&before, &next);
    let status_changed = before.status != next.status;
    let quiet_change = before.sort_order != next.sort_order
        || before.result != next.result
        || before.next_run_at != next.next_run_at;
    if changes.is_empty() && !status_changed && !quiet_change {
        tx.commit().await?;
        return Ok(before);
    }

    let sql = format!(
        "UPDATE app_tasks
         SET title = $2, notes = $3, assignee = $4, status = $5, due_on = $6, due_at = $7,
             project_id = $8, sort_order = $9, closed_at = $10, result = $11,
             result_url = $12, next_run_at = $13, attempts = $14, updated_at = now()
         WHERE id = $1
         RETURNING {TASK_COLUMNS}"
    );
    let task = sqlx::query_as::<_, Task>(&sql)
        .bind(id)
        .bind(&next.title)
        .bind(&next.notes)
        .bind(&next.assignee)
        .bind(&next.status)
        .bind(next.due_on)
        .bind(next.due_at)
        .bind(&next.project_id)
        .bind(next.sort_order)
        .bind(next.closed_at)
        .bind(&next.result)
        .bind(&next.result_url)
        .bind(next.next_run_at)
        .bind(next.attempts)
        .fetch_one(&mut *tx)
        .await?;

    // Reordering and queueing alone are not history. A result shows in the
    // status event that marks the request ready.
    if !changes.is_empty() {
        append_event(&mut tx, id, writer, "edited", Value::Object(changes)).await?;
    }
    if status_changed {
        append_event(
            &mut tx,
            id,
            writer,
            "status",
            json!({ "from": before.status, "to": next.status }),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(task)
}

/// The status changes only some writers may make. `ready` belongs to the
/// assistant and always carries a result. The assistant never closes its own
/// request as done: it marks it ready, and the owner accepts it.
fn check_status(status: Status, before: &Task, result: Option<&str>, writer: &Writer) -> Result<()> {
    let request = before.assignee == "assistant";
    match status {
        Status::Ready if writer.author != Actor::Assistant || !request => Err(Error::InvalidInput(
            "only the assistant marks a request ready".into(),
        )),
        Status::Ready if result.map_or(true, |r| r.trim().is_empty()) => {
            Err(Error::InvalidInput("marking a request ready needs a `result`".into()))
        }
        Status::Done if writer.author == Actor::Assistant && request => Err(Error::InvalidInput(
            "mark the request ready with a result; the owner closes it".into(),
        )),
        _ => Ok(()),
    }
}

/// When a newly queued request may run: at its due time when it has one
/// still ahead, otherwise now. A date with no time says when the owner
/// wants the answer, not when to start.
fn queue_time(due_at: Option<Timestamp>) -> Timestamp {
    let now = Timestamp::now();
    due_at.filter(|at| *at > now).unwrap_or(now)
}

// ── the request worker's writes ─────────────────────────────────────────────

/// Take the next request that is due to run and mark it active, or `None`
/// when nothing is due. `SKIP LOCKED` keeps two workers from taking the same
/// request, though the box runs one.
pub async fn claim_next_request(pool: &PgPool) -> Result<Option<Task>> {
    let mut tx = pool.begin().await?;
    let sql = format!(
        "UPDATE app_tasks SET status = 'active', next_run_at = NULL, updated_at = now()
         WHERE id = (
             SELECT id FROM app_tasks
             WHERE assignee = 'assistant' AND status = 'open' AND next_run_at <= now()
             ORDER BY next_run_at
             LIMIT 1
             FOR UPDATE SKIP LOCKED
         )
         RETURNING {TASK_COLUMNS}"
    );
    let Some(task) = sqlx::query_as::<_, Task>(&sql).fetch_optional(&mut *tx).await? else {
        tx.commit().await?;
        return Ok(None);
    };
    let writer = Writer { author: Actor::Assistant, chat_id: task.chat_id.clone(), from_request: true };
    append_event(&mut tx, &task.id, &writer, "status", json!({ "from": "open", "to": "active" })).await?;
    tx.commit().await?;
    Ok(Some(task))
}

/// Give a request its work chat on its first run.
pub async fn set_request_chat(pool: &PgPool, id: &str, chat_id: &str) -> Result<()> {
    sqlx::query("UPDATE app_tasks SET chat_id = $2 WHERE id = $1")
        .bind(id)
        .bind(chat_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// End a run with its answer: the request becomes ready with `result`. When
/// the owner changed the request during the run (dismissed it, took it back),
/// their change stands and the answer is only noted. When the run already
/// marked it ready through `update_task`, that result stands.
pub async fn finish_request_run(pool: &PgPool, id: &str, result: &str, writer: &Writer) -> Result<Task> {
    let current = get_task(pool, id).await?;
    if current.status == "active" && current.assignee == "assistant" {
        let patch = TaskPatch {
            status: Some(Status::Ready),
            result: Some(result.to_string()),
            ..Default::default()
        };
        let task = update_task(pool, id, patch, writer).await?;
        sqlx::query("UPDATE app_tasks SET attempts = 0 WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        return Ok(task);
    }
    if current.status != "ready" {
        add_note(pool, id, result, writer).await?;
    }
    get_task(pool, id).await
}

/// End a run that failed. The request goes back in the queue after a
/// backoff, or, once it has failed `MAX_ATTEMPTS` times in a row, comes back
/// to the owner as ready with what went wrong.
pub async fn fail_request_run(pool: &PgPool, id: &str, error: &str, writer: &Writer) -> Result<Task> {
    let current = get_task(pool, id).await?;
    if current.status != "active" || current.assignee != "assistant" {
        return Ok(current);
    }
    let attempts = current.attempts + 1;
    if attempts >= MAX_ATTEMPTS {
        let result = format!(
            "I couldn't finish this. The last attempt failed with: {error}\n\n\
             Reply to try again, or check the model settings if this keeps happening."
        );
        let task = finish_request_run(pool, id, &result, writer).await?;
        sqlx::query("UPDATE app_tasks SET attempts = $2 WHERE id = $1")
            .bind(id)
            .bind(attempts)
            .execute(pool)
            .await?;
        return Ok(task);
    }

    let mut tx = pool.begin().await?;
    // 5 minutes, then 20: long enough for a provider outage to pass.
    let backoff_minutes = 5 * i64::from(attempts).pow(2);
    let sql = format!(
        "UPDATE app_tasks
         SET status = 'open', attempts = $2,
             next_run_at = now() + make_interval(mins => $3), updated_at = now()
         WHERE id = $1
         RETURNING {TASK_COLUMNS}"
    );
    let task = sqlx::query_as::<_, Task>(&sql)
        .bind(id)
        .bind(attempts)
        .bind(backoff_minutes as i32)
        .fetch_one(&mut *tx)
        .await?;
    append_event(&mut tx, id, writer, "note", json!({ "text": format!("This run failed: {error}") })).await?;
    append_event(&mut tx, id, writer, "status", json!({ "from": "active", "to": "open" })).await?;
    tx.commit().await?;
    Ok(task)
}

/// Requests left active when the server stopped mid-run go back in the
/// queue. Only requests with a work chat are the worker's; a chat may have
/// marked another request active and still be working on it.
pub async fn requeue_interrupted_requests(pool: &PgPool) -> Result<u64> {
    let done = sqlx::query(
        "UPDATE app_tasks SET status = 'open', next_run_at = now(), updated_at = now()
         WHERE assignee = 'assistant' AND status = 'active' AND chat_id IS NOT NULL",
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

pub async fn add_note(pool: &PgPool, id: &str, text: &str, writer: &Writer) -> Result<TaskEvent> {
    let text = text.trim();
    if text.is_empty() {
        return Err(Error::InvalidInput("a note needs some text".into()));
    }
    let mut tx = pool.begin().await?;
    let touched = sqlx::query("UPDATE app_tasks SET updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if touched.rows_affected() == 0 {
        return Err(Error::NotFound(format!("task not found: {id}")));
    }
    let event = append_event(&mut tx, id, writer, "note", json!({ "text": text })).await?;
    tx.commit().await?;
    Ok(event)
}

pub async fn list_events(pool: &PgPool, id: &str) -> Result<Vec<TaskEvent>> {
    Ok(sqlx::query_as::<_, TaskEvent>(
        "SELECT id, task_id, author, kind, body, chat_id, created_at
         FROM app_task_events WHERE task_id = $1
         ORDER BY created_at ASC, id ASC",
    )
    .bind(id)
    .fetch_all(pool)
    .await?)
}

async fn append_event(
    tx: &mut Transaction<'_, Postgres>,
    task_id: &str,
    writer: &Writer,
    kind: &str,
    body: Value,
) -> Result<TaskEvent> {
    Ok(sqlx::query_as::<_, TaskEvent>(
        "INSERT INTO app_task_events (task_id, author, kind, body, chat_id)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, task_id, author, kind, body, chat_id, created_at",
    )
    .bind(task_id)
    .bind(writer.author.as_str())
    .bind(kind)
    .bind(body)
    .bind(&writer.chat_id)
    .fetch_one(&mut **tx)
    .await?)
}

/// The fields an edit changed, each with its new value, for the history.
fn edited_fields(before: &Task, after: &Task) -> Map<String, Value> {
    let mut out = Map::new();
    let mut put = |key: &str, a: Value, b: Value| {
        if a != b {
            out.insert(key.into(), b);
        }
    };
    put("title", json!(before.title), json!(after.title));
    put("notes", json!(before.notes), json!(after.notes));
    put("assignee", json!(before.assignee), json!(after.assignee));
    put("due_on", json!(before.due_on), json!(after.due_on));
    put("due_at", json!(before.due_at), json!(after.due_at));
    put("project_id", json!(before.project_id), json!(after.project_id));
    put("result_url", json!(before.result_url), json!(after.result_url));
    out
}

fn clean_title(title: &str) -> Result<String> {
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        return Err(Error::InvalidInput("a task needs a title".into()));
    }
    Ok(title)
}

fn clean_text(text: Option<String>) -> Option<String> {
    text.map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}

/// A time implies its day. Without a stated day, the day is the time's UTC
/// date, which the client avoids by always sending both.
fn due_day(due_on: Option<NaiveDate>, due_at: Option<Timestamp>) -> Option<NaiveDate> {
    due_on.or_else(|| due_at.map(|at| at.date_naive()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new(title: &str) -> NewTask {
        NewTask {
            title: title.into(),
            notes: None,
            assignee: None,
            due_on: None,
            due_at: None,
            project_id: None,
            source_url: None,
        }
    }

    /// A task's whole life: created in order, edited, closed and reopened,
    /// with every change in its history and `closed_at` tracking the status.
    #[sqlx::test(migrations = "./migrations")]
    async fn a_task_keeps_its_history(pool: PgPool) {
        let made = create_tasks(&pool, vec![new("Renew passport"), new("  Book   the dentist ")], Origin::Chat, &Writer::owner())
            .await
            .unwrap();
        assert_eq!(made[1].title, "Book the dentist");
        let open = list_tasks(&pool, &ListFilter::default()).await.unwrap();
        assert_eq!(open.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["Renew passport", "Book the dentist"]);

        let id = &made[0].id;
        let assistant = Writer { author: Actor::Assistant, chat_id: Some("chat_x".into()), from_request: false };
        let edited = update_task(
            &pool,
            id,
            TaskPatch { due_on: Some(NaiveDate::from_ymd_opt(2030, 1, 4)), status: Some(Status::Done), ..Default::default() },
            &assistant,
        )
        .await
        .unwrap();
        assert!(edited.closed_at.is_some());
        assert_eq!(list_tasks(&pool, &ListFilter::default()).await.unwrap().len(), 1);

        let reopened = update_task(&pool, id, TaskPatch { status: Some(Status::Open), ..Default::default() }, &Writer::owner())
            .await
            .unwrap();
        assert!(reopened.closed_at.is_none());

        // Saving what is already there writes nothing.
        update_task(&pool, id, TaskPatch { title: Some("Renew passport".into()), ..Default::default() }, &Writer::owner())
            .await
            .unwrap();
        add_note(&pool, id, "Form is in the drawer.", &assistant).await.unwrap();

        let events = list_events(&pool, id).await.unwrap();
        let kinds: Vec<_> = events.iter().map(|e| (e.kind.as_str(), e.author.as_str())).collect();
        assert_eq!(
            kinds,
            [("created", "owner"), ("edited", "assistant"), ("status", "assistant"), ("status", "owner"), ("note", "assistant")]
        );
        assert_eq!(events[1].chat_id.as_deref(), Some("chat_x"));
    }

    fn request(title: &str) -> NewTask {
        NewTask { assignee: Some(Actor::Assistant), ..new(title) }
    }

    fn run_writer() -> Writer {
        Writer { author: Actor::Assistant, chat_id: None, from_request: true }
    }

    /// A request queues when it is created or handed over, runs when the
    /// worker claims it, and waits as ready with its result. Replying puts it
    /// back in the queue; accepting closes it.
    #[sqlx::test(migrations = "./migrations")]
    async fn a_request_runs_and_waits_for_the_owner(pool: PgPool) {
        let made = create_tasks(&pool, vec![request("Compare the two quotes"), new("Water the plants")], Origin::Owner, &Writer::owner())
            .await
            .unwrap();
        assert!(made[0].next_run_at.is_some(), "a new request is queued");
        assert!(made[1].next_run_at.is_none(), "a to-do never runs");

        let claimed = claim_next_request(&pool).await.unwrap().expect("the request is due");
        assert_eq!(claimed.id, made[0].id);
        assert_eq!(claimed.status, "active");
        assert!(claim_next_request(&pool).await.unwrap().is_none(), "one run per request");

        let ready = finish_request_run(&pool, &claimed.id, "The second quote is cheaper.", &run_writer()).await.unwrap();
        assert_eq!(ready.status, "ready");
        assert_eq!(ready.result.as_deref(), Some("The second quote is cheaper."));
        assert!(ready.closed_at.is_none(), "ready is not closed");
        assert_eq!(list_tasks(&pool, &ListFilter::default()).await.unwrap().len(), 2, "ready stays on the list");

        // The owner replies: a note, and the request goes back in the queue.
        add_note(&pool, &claimed.id, "Include delivery costs.", &Writer::owner()).await.unwrap();
        let again = update_task(&pool, &claimed.id, TaskPatch { status: Some(Status::Open), ..Default::default() }, &Writer::owner())
            .await
            .unwrap();
        assert!(again.next_run_at.is_some());
        claim_next_request(&pool).await.unwrap().expect("queued again");
        finish_request_run(&pool, &claimed.id, "With delivery, the first is cheaper.", &run_writer()).await.unwrap();

        let done = update_task(&pool, &claimed.id, TaskPatch { status: Some(Status::Done), ..Default::default() }, &Writer::owner())
            .await
            .unwrap();
        assert!(done.closed_at.is_some());

        // Handing a to-do over queues it; taking it back takes it out.
        let handed = update_task(&pool, &made[1].id, TaskPatch { assignee: Some(Actor::Assistant), ..Default::default() }, &Writer::owner())
            .await
            .unwrap();
        assert!(handed.next_run_at.is_some());
        let back = update_task(&pool, &made[1].id, TaskPatch { assignee: Some(Actor::Owner), ..Default::default() }, &Writer::owner())
            .await
            .unwrap();
        assert!(back.next_run_at.is_none());
    }

    /// A request due at a time waits for it, and one with a date only runs
    /// now: the date says when the answer is wanted.
    #[sqlx::test(migrations = "./migrations")]
    async fn a_request_with_a_time_waits_for_it(pool: PgPool) {
        let later = Timestamp::from_utc(chrono::Utc::now() + chrono::Duration::hours(3));
        let timed = NewTask { due_at: Some(later), ..request("Check whether the reply came in") };
        let dated = NewTask { due_on: NaiveDate::from_ymd_opt(2030, 1, 4), ..request("Plan the trip") };
        create_tasks(&pool, vec![timed, dated], Origin::Owner, &Writer::owner()).await.unwrap();
        let first = claim_next_request(&pool).await.unwrap().unwrap();
        assert_eq!(first.title, "Plan the trip");
        assert!(claim_next_request(&pool).await.unwrap().is_none(), "the timed request is not due yet");
    }

    /// A failed run retries after a backoff, and the third failure in a row
    /// hands the request back as ready with what went wrong.
    #[sqlx::test(migrations = "./migrations")]
    async fn failed_runs_back_off_then_come_back_to_the_owner(pool: PgPool) {
        let id = create_tasks(&pool, vec![request("Summarize the week")], Origin::Owner, &Writer::owner())
            .await
            .unwrap()
            .remove(0)
            .id;
        for attempt in 1..MAX_ATTEMPTS {
            claim_next_request(&pool).await.unwrap().expect("due");
            let failed = fail_request_run(&pool, &id, "upstream timeout", &run_writer()).await.unwrap();
            assert_eq!(failed.status, "open");
            assert_eq!(failed.attempts, attempt);
            assert!(failed.next_run_at.unwrap() > Timestamp::now(), "it waits before trying again");
            assert!(claim_next_request(&pool).await.unwrap().is_none());
            sqlx::query("UPDATE app_tasks SET next_run_at = now() WHERE id = $1").bind(&id).execute(&pool).await.unwrap();
        }
        claim_next_request(&pool).await.unwrap().expect("due");
        let last = fail_request_run(&pool, &id, "upstream timeout", &run_writer()).await.unwrap();
        assert_eq!(last.status, "ready");
        assert!(last.result.unwrap().contains("upstream timeout"));
    }

    /// The owner's change during a run stands: a request they dismissed
    /// while it ran is not brought back by the run's answer.
    #[sqlx::test(migrations = "./migrations")]
    async fn a_run_does_not_undo_the_owner(pool: PgPool) {
        let id = create_tasks(&pool, vec![request("Find a plumber")], Origin::Owner, &Writer::owner())
            .await
            .unwrap()
            .remove(0)
            .id;
        claim_next_request(&pool).await.unwrap().unwrap();
        update_task(&pool, &id, TaskPatch { status: Some(Status::Dismissed), ..Default::default() }, &Writer::owner())
            .await
            .unwrap();
        let after = finish_request_run(&pool, &id, "Here are three.", &run_writer()).await.unwrap();
        assert_eq!(after.status, "dismissed");
        assert!(after.result.is_none());
    }

    /// Only the assistant marks a request ready, and never closes it itself.
    #[sqlx::test(migrations = "./migrations")]
    async fn ready_belongs_to_the_assistant(pool: PgPool) {
        let id = create_tasks(&pool, vec![request("Draft the note")], Origin::Owner, &Writer::owner())
            .await
            .unwrap()
            .remove(0)
            .id;
        let by_owner = TaskPatch { status: Some(Status::Ready), result: Some("x".into()), ..Default::default() };
        assert!(update_task(&pool, &id, by_owner, &Writer::owner()).await.is_err());
        let done = TaskPatch { status: Some(Status::Done), ..Default::default() };
        assert!(update_task(&pool, &id, done, &run_writer()).await.is_err());
    }

    /// Existing assistant tasks are not queued by the migration, and an edit
    /// that leaves them where they are does not queue them either.
    #[sqlx::test(migrations = "./migrations")]
    async fn an_unqueued_request_stays_unqueued_until_handed_over(pool: PgPool) {
        let id = create_tasks(&pool, vec![request("Old request")], Origin::Owner, &Writer::owner())
            .await
            .unwrap()
            .remove(0)
            .id;
        sqlx::query("UPDATE app_tasks SET next_run_at = NULL WHERE id = $1").bind(&id).execute(&pool).await.unwrap();
        let edited = update_task(&pool, &id, TaskPatch { title: Some("Old request, renamed".into()), ..Default::default() }, &Writer::owner())
            .await
            .unwrap();
        assert!(edited.next_run_at.is_none());
        assert!(claim_next_request(&pool).await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_blank_title_is_refused(pool: PgPool) {
        assert!(create_tasks(&pool, vec![new("   ")], Origin::Owner, &Writer::owner()).await.is_err());
        assert!(list_tasks(&pool, &ListFilter { status: Some("all".into()), ..Default::default() }).await.unwrap().is_empty());
    }
}

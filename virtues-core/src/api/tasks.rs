//! Tasks: one to-do list the owner and the assistant share.
//!
//! Every write goes through this module, which changes the row and appends
//! its `app_task_events` entry in one transaction. The HTTP handlers and the
//! chat tools both call it, so a task's history is complete whoever acted.
//! See agents/plan/tasks-plan.md.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sqlx::{PgPool, Postgres, Transaction};

use crate::error::{Error, Result};
use crate::ids::{generate_id, TASK_PREFIX};
use crate::types::Timestamp;

const TASK_COLUMNS: &str = "id, title, notes, assignee, status, due_on, due_at, project_id, \
     origin, source_url, sort_order, closed_at, created_at, updated_at";

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
    Done,
    Dismissed,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::Active => "active",
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
}

impl Writer {
    pub fn owner() -> Self {
        Self { author: Actor::Owner, chat_id: None }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListFilter {
    /// `open` (open + active, the default), `closed` (done + dismissed) or `all`.
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
        "open" => &["open", "active"],
        "closed" => &["done", "dismissed"],
        "all" => &["open", "active", "done", "dismissed"],
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
        // Keep the order they were given: the first ends up on top.
        let sort_order = top - (count - i) as f64;
        let sql = format!(
            "INSERT INTO app_tasks (id, title, notes, assignee, due_on, due_at, project_id,
                                    origin, source_url, sort_order)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
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
        next.status = status.as_str().into();
        next.closed_at = match (status.is_closed(), before.closed_at) {
            (true, Some(at)) => Some(at),
            (true, None) => Some(Timestamp::now()),
            (false, _) => None,
        };
    }

    let changes = edited_fields(&before, &next);
    let status_changed = before.status != next.status;
    if changes.is_empty() && !status_changed && before.sort_order == next.sort_order {
        tx.commit().await?;
        return Ok(before);
    }

    let sql = format!(
        "UPDATE app_tasks
         SET title = $2, notes = $3, assignee = $4, status = $5, due_on = $6, due_at = $7,
             project_id = $8, sort_order = $9, closed_at = $10, updated_at = now()
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
        .fetch_one(&mut *tx)
        .await?;

    // Reordering alone is not history.
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
        let assistant = Writer { author: Actor::Assistant, chat_id: Some("chat_x".into()) };
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

    #[sqlx::test(migrations = "./migrations")]
    async fn a_blank_title_is_refused(pool: PgPool) {
        assert!(create_tasks(&pool, vec![new("   ")], Origin::Owner, &Writer::owner()).await.is_err());
        assert!(list_tasks(&pool, &ListFilter { status: Some("all".into()), ..Default::default() }).await.unwrap().is_empty());
    }
}

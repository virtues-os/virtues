//! Threads: something happening in the person's life that they are following.
//!
//! A thread unfolds and is not settled: raising a round, getting back to the
//! gym, a birthday dinner, health. Some finish and some never do. The person
//! makes it; the server never makes one on its own. It ends when they finish it
//! or drop it (`archived_at` with an `outcome`), and it can sit under a thing
//! (`thing_id`, `crate::api::things`), the noun it is about. One step is a task.
//!
//! A thread gathers chats, pages, files and subjects as URL-native members
//! (`app_thread_items`) and carries an icon, a color and a brief
//! (`instructions`), the sentence that says what belongs in it.
//!
//! No note or memo: `app_threads.current_status(_at)` stay in the table,
//! unread, so notes written while the table had one are kept.
//!
//! A chat lives in at most one thread (`app_chats.thread_id`). Entering a thread
//! weights its members in retrieval; the chat is folded into the thread's
//! corpus. Membership is manual for now. A thread cannot be a member of a
//! thread.
//!
//! `thread_id` here is one of these threads. A message or email conversation
//! is `conversation_id` on `data_communication_message` and
//! `data_communication_email`.
//!
//! Ids keep the `nb_` prefix (`ids::THREAD_PREFIX`) from when threads were
//! notebooks; a thread's ref URL is `/thread/{id}` (`refs::split_ref`).

use crate::error::{Error, Result};
use crate::ids::{generate_id, THREAD_PREFIX};
use crate::types::Timestamp;
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::PgPool;

/// Distinguishes a missing field from an explicit `null` for `Option<Option<T>>`:
/// missing → `None` (leave alone), `null` → `Some(None)` (clear), value → set.
/// Without this, serde folds an explicit `null` into `None`, so a field can be
/// set but never cleared — which is why "Remove icon" silently did nothing.
fn deserialize_double_option<'de, D, T>(
    deserializer: D,
) -> std::result::Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Some(Option::deserialize(deserializer)?))
}

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Thread {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub accent_color: Option<String>,
    /// The brief: standing directions the assistant follows in every chat in
    /// this thread. Written by the owner, never by the model.
    pub instructions: Option<String>,
    pub sort_order: i32,
    /// When the thread ended: kept, out of the working view. Distinct from
    /// `deleted_at` (the trash): an ended thread is one the owner closed, a
    /// trashed one is one they removed. Ended threads leave the Home panel,
    /// the threads list and the "Add to thread" submenu; their chats and
    /// pages stay filed in them, and the thread reopens from the threads
    /// page.
    pub archived_at: Option<Timestamp>,
    /// How it ended: `finished` or `dropped`. Set with `archived_at`, and
    /// null while the thread is open.
    #[serde(default)]
    pub outcome: Option<String>,
    /// The thing it sits under, if any (`wiki_things.id`).
    #[serde(default)]
    pub thing_id: Option<String>,
    /// That thing's name, resolved on read.
    #[serde(default)]
    pub thing_title: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// List-view summary — adds member and chat counts.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadSummary {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub accent_color: Option<String>,
    pub instructions: Option<String>,
    pub sort_order: i32,
    /// Members other than chats. Chats are counted in `chat_count` from the
    /// chat's own `thread_id`, so counting their `/chat/` rows here as well
    /// made every filed chat show up twice on a thread card.
    pub item_count: i64,
    pub chat_count: i64,
    pub archived_at: Option<Timestamp>,
    #[serde(default)]
    pub outcome: Option<String>,
    #[serde(default)]
    pub thing_id: Option<String>,
    #[serde(default)]
    pub thing_title: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A single URL-native member of a Thread.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadItem {
    pub url: String,
    pub sort_order: i32,
    /// `library` (grounds chat) | `manuscript` (yours to write; excluded from
    /// retrieval so a draft is never cited back at you) | `pin` (nav-only).
    pub role: String,
    pub added_at: Timestamp,
    /// What the member is called and what it is, resolved in one batch by
    /// `get_thread` (the same resolver the chat prompt uses). The page used
    /// to ask for each member's name itself, one request per member.
    #[sqlx(skip)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[sqlx(skip)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Drive files only: the extraction status, for the page's status column.
    #[sqlx(skip)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadDetail {
    #[serde(flatten)]
    pub thread: Thread,
    pub items: Vec<ThreadItem>,
    /// Every live chat filed here, newest conversation first. The page used to
    /// list chats from the session list, which holds only the most recent
    /// chats across the whole box, so a thread's older chats were counted on
    /// its card and missing from its page.
    pub chats: Vec<ThreadChat>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadChat {
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
    pub message_count: i64,
    pub last_message_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadListResponse {
    pub threads: Vec<ThreadSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateThreadRequest {
    pub name: String,
    pub icon: Option<String>,
    pub accent_color: Option<String>,
    /// The thing to start it under, when it is made from a thing's page.
    #[serde(default)]
    pub thing_id: Option<String>,
}

/// How a thread ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Finished,
    Dropped,
}

impl Outcome {
    fn as_str(self) -> &'static str {
        match self {
            Outcome::Finished => "finished",
            Outcome::Dropped => "dropped",
        }
    }
}

/// Update a Thread. `Option<Option<T>>` fields are tri-state: absent = leave,
/// `Some(None)` = clear, `Some(Some(v))` = set.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateThreadRequest {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub icon: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub accent_color: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub instructions: Option<Option<String>>,
    pub sort_order: Option<i32>,
    /// The thing this thread sits under. `null` takes it out from under one.
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub thing_id: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddThreadItemRequest {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorderThreadItemsRequest {
    pub urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetThreadItemRoleRequest {
    pub url: String,
    /// `library` | `manuscript` | `pin`.
    pub role: String,
}

/// One entity referenced across a thread's members.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadGraphNode {
    /// Ref URL for the entity, e.g. `/person/pe_abc` — also the node's identity.
    pub url: String,
    pub entity_type: String,
    pub name: String,
    /// Member urls that reference this entity. Drives click-to-filter.
    pub item_urls: Vec<String>,
}

/// Two entities that appear together in at least one member.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadGraphEdge {
    pub source: String,
    pub target: String,
    pub weight: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadGraph {
    pub nodes: Vec<ThreadGraphNode>,
    pub edges: Vec<ThreadGraphEdge>,
}

// ============================================================================
// Thread CRUD
// ============================================================================

/// A thread row as [`Thread`] reads it, from `app_threads` unaliased. The
/// thing's name is resolved here so no reader needs a second query to say
/// what a thread sits under.
const THREAD_COLUMNS: &str = "id, name, icon, accent_color, instructions, sort_order, \
     archived_at, outcome, thing_id, \
     (SELECT t.title FROM wiki_things t WHERE t.id = app_threads.thing_id) AS thing_title, \
     created_at, updated_at";

/// List Threads, most-recently-active first, with member and chat counts.
/// Open ones only unless `include_archived` — the threads page asks for both
/// and folds the ended ones; every other reader (Home panel, ⌘K, the
/// "Add to thread" submenu) wants the working set.
///
/// A trashed member keeps its membership row so a restore is whole (see
/// `api::trash`), which is why the counts and `get_thread`'s member list
/// filter members through their own table's `deleted_at` rather than trusting
/// the row's existence.
///
/// `member` narrows the list to the threads that hold that url — a page
/// asking which threads it is in. A page can be in several, unlike a chat.
pub async fn list_threads(
    pool: &PgPool,
    include_archived: bool,
    member: Option<&str>,
) -> Result<ThreadListResponse> {
    let threads = sqlx::query_as::<_, ThreadSummary>(
        r#"
        SELECT
            s.id, s.name, s.icon, s.accent_color,
            s.instructions, s.sort_order, s.archived_at, s.outcome, s.thing_id,
            (SELECT t.title FROM wiki_things t WHERE t.id = s.thing_id) AS thing_title,
            COALESCE((SELECT COUNT(*) FROM app_thread_items
                      WHERE thread_id = s.id
              AND url NOT LIKE '/chat/%'
              AND NOT EXISTS (SELECT 1 FROM app_pages p WHERE url = '/page/' || p.id AND p.deleted_at IS NOT NULL)), 0) AS item_count,
            COALESCE((SELECT COUNT(*) FROM app_chats
                      WHERE thread_id = s.id AND deleted_at IS NULL), 0) AS chat_count,
            s.created_at, s.updated_at
        FROM app_threads s
        WHERE s.deleted_at IS NULL AND ($1 OR s.archived_at IS NULL)
          AND ($2::text IS NULL OR EXISTS (
              SELECT 1 FROM app_thread_items i WHERE i.thread_id = s.id AND i.url = $2
          ))
        ORDER BY s.sort_order ASC, s.updated_at DESC
        "#,
    )
    .bind(include_archived)
    .bind(member)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list threads: {}", e)))?;

    Ok(ThreadListResponse { threads })
}

/// Get a single Thread with its ordered members.
pub async fn get_thread(pool: &PgPool, id: &str) -> Result<ThreadDetail> {
    let thread = sqlx::query_as::<_, Thread>(&format!(
        "SELECT {THREAD_COLUMNS} FROM app_threads WHERE id = $1 AND deleted_at IS NULL"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get thread: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Thread not found: {}", id)))?;

    let mut items = sqlx::query_as::<_, ThreadItem>(
        r#"
        SELECT url, sort_order, role, added_at
        FROM app_thread_items
        WHERE thread_id = $1
              AND NOT EXISTS (SELECT 1 FROM app_pages p WHERE url = '/page/' || p.id AND p.deleted_at IS NOT NULL)
              AND NOT EXISTS (SELECT 1 FROM app_chats c WHERE url = '/chat/' || c.id AND c.deleted_at IS NOT NULL)
        ORDER BY sort_order ASC, added_at ASC
        "#,
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get thread items: {}", e)))?;

    let urls: Vec<String> = items.iter().map(|i| i.url.clone()).collect();
    let mut resolved = crate::api::refs::resolve_refs(pool, &urls).await;
    for item in &mut items {
        if let Some(r) = resolved.remove(&item.url) {
            item.title = Some(r.title);
            item.kind = Some(r.kind);
            item.status = r.status;
        }
    }

    let chats = sqlx::query_as::<_, ThreadChat>(
        r#"
        SELECT id, title, icon, message_count::bigint AS message_count,
               COALESCE(
                   (SELECT MAX(m.created_at) FROM app_chat_messages m WHERE m.chat_id = c.id),
                   c.created_at
               ) AS last_message_at
        FROM app_chats c
        WHERE thread_id = $1 AND deleted_at IS NULL
        ORDER BY last_message_at DESC
        "#,
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get thread chats: {}", e)))?;

    Ok(ThreadDetail { thread, items, chats })
}

/// Create a new Thread.
pub async fn create_thread(pool: &PgPool, req: CreateThreadRequest) -> Result<Thread> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(Error::InvalidInput("Thread name cannot be empty".into()));
    }

    if let Some(thing_id) = &req.thing_id {
        crate::api::things::get_thing(pool, thing_id).await?;
    }

    let timestamp = chrono::Utc::now().to_rfc3339();
    let id = generate_id(THREAD_PREFIX, &[name, &timestamp]);

    let thread = sqlx::query_as::<_, Thread>(&format!(
        "INSERT INTO app_threads (id, name, icon, accent_color, thing_id) \
         VALUES ($1, $2, $3, $4, $5) \
         RETURNING {THREAD_COLUMNS}"
    ))
    .bind(&id)
    .bind(name)
    .bind(&req.icon)
    .bind(&req.accent_color)
    .bind(&req.thing_id)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create thread: {}", e)))?;

    Ok(thread)
}

/// Update a Thread. Only provided fields change.
pub async fn update_thread(pool: &PgPool, id: &str, req: UpdateThreadRequest) -> Result<Thread> {
    let existing = sqlx::query_as::<_, Thread>(&format!(
        "SELECT {THREAD_COLUMNS} FROM app_threads WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get thread: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Thread not found: {}", id)))?;

    let name = req.name.as_deref().unwrap_or(&existing.name).trim().to_string();
    if name.is_empty() {
        return Err(Error::InvalidInput("Thread name cannot be empty".into()));
    }

    let icon = match req.icon {
        Some(val) => val,
        None => existing.icon,
    };
    let accent_color = match req.accent_color {
        Some(val) => val,
        None => existing.accent_color,
    };
    let sort_order = req.sort_order.unwrap_or(existing.sort_order);
    let instructions = match req.instructions {
        Some(val) => val,
        None => existing.instructions,
    };
    let thing_id = match req.thing_id {
        Some(val) => val.filter(|t| !t.is_empty()),
        None => existing.thing_id.clone(),
    };
    if let Some(thing_id) = thing_id.as_ref().filter(|t| existing.thing_id.as_ref() != Some(*t)) {
        crate::api::things::get_thing(pool, thing_id).await?;
    }

    let thread = sqlx::query_as::<_, Thread>(&format!(
        "UPDATE app_threads \
         SET name = $2, icon = $3, accent_color = $4, sort_order = $5, \
             instructions = $6, thing_id = $7 \
         WHERE id = $1 \
         RETURNING {THREAD_COLUMNS}"
    ))
    .bind(id)
    .bind(&name)
    .bind(&icon)
    .bind(&accent_color)
    .bind(sort_order)
    .bind(&instructions)
    .bind(&thing_id)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update thread: {}", e)))?;

    Ok(thread)
}

/// End a thread as finished or dropped: it leaves the working surfaces and
/// keeps everything. Ending an ended thread again changes only how it ended
/// and keeps the day it ended, so "finished" can be corrected to "dropped"
/// without reopening it.
pub async fn archive_thread(pool: &PgPool, id: &str, outcome: Outcome) -> Result<()> {
    let hit: Option<String> = sqlx::query_scalar(
        "UPDATE app_threads SET archived_at = COALESCE(archived_at, now()), outcome = $2 \
         WHERE id = $1 AND deleted_at IS NULL RETURNING id",
    )
    .bind(id)
    .bind(outcome.as_str())
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to end thread {id}: {e}")))?;
    hit.map(|_| ()).ok_or_else(|| Error::NotFound(format!("Thread not found: {id}")))
}

/// Reopen an ended thread.
pub async fn unarchive_thread(pool: &PgPool, id: &str) -> Result<()> {
    let hit: Option<String> = sqlx::query_scalar(
        "UPDATE app_threads SET archived_at = NULL, outcome = NULL \
         WHERE id = $1 AND deleted_at IS NULL RETURNING id",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to reopen thread {id}: {e}")))?;
    hit.map(|_| ()).ok_or_else(|| Error::NotFound(format!("Thread not found: {id}")))
}

/// Delete a Thread — into the trash. Members and the chats' `thread_id`
/// stay on the row so a restore brings the thread back whole; the FK
/// cascade and SET NULL only fire on `trash::purge`, the hard delete the
/// trash and the sweeper reach after `TRASH_RETENTION_DAYS`.
pub async fn delete_thread(pool: &PgPool, id: &str) -> Result<()> {
    crate::api::trash::trash(pool, crate::api::trash::TrashKind::Thread, id).await
}

/// Touch a Thread's updated_at to reflect activity.
pub async fn touch_thread(pool: &PgPool, id: &str) -> Result<()> {
    sqlx::query(r#"UPDATE app_threads SET updated_at = now() WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to touch thread: {}", e)))?;
    Ok(())
}

// ============================================================================
// Membership
// ============================================================================

/// Add a member URL to a Thread. Idempotent on (thread_id, url).
pub async fn add_thread_item(pool: &PgPool, thread_id: &str, req: AddThreadItemRequest) -> Result<ThreadItem> {
    let url = req.url.trim();
    if url.is_empty() {
        return Err(Error::InvalidInput("Member url cannot be empty".into()));
    }

    // A thread cannot be a member of a thread. A nested one would be a cycle
    // in scope resolution, and this keeps those rows from being written.
    if url.starts_with("/thread/") {
        return Err(Error::InvalidInput(
            "You can't put a thread inside another thread.".into(),
        ));
    }

    let found: Option<Option<Timestamp>> = sqlx::query_scalar(
        r#"SELECT archived_at FROM app_threads WHERE id = $1 AND deleted_at IS NULL"#,
    )
    .bind(thread_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to verify thread: {}", e)))?;
    let Some(archived_at) = found else {
        return Err(Error::NotFound(format!("Thread not found: {}", thread_id)));
    };
    // An ended thread does not take new members; reopen it first.
    if archived_at.is_some() {
        return Err(Error::InvalidInput("this thread has ended".into()));
    }

    // A chat is filed by its own `thread_id`, not by a member row: the
    // thread view lists chats from the session list and ignores `/chat/`
    // member rows, so a bare row here was an add that did nothing (VIR-359).
    // Bind the chat; the insert below then keeps the membership row in step.
    if let Some(chat_id) = url.strip_prefix("/chat/") {
        refuse_reserved_chat(chat_id)?;
        set_chat_thread(pool, chat_id, Some(thread_id)).await?;
    }

    // role='library' = grounds chat, which is what membership means. (The old
    // nav-only 'pin' role for a nested notebook is gone with nesting itself;
    // 'pin' survives only as a role the user can set explicitly.)
    let item = sqlx::query_as::<_, ThreadItem>(
        r#"
        INSERT INTO app_thread_items (thread_id, url, sort_order, role)
        VALUES (
            $1, $2,
            (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM app_thread_items WHERE thread_id = $1),
            'library'
        )
        -- Re-adding an existing member upgrades a legacy nav-only 'pin' to
        -- 'library', but must not demote a 'manuscript' back to source material.
        ON CONFLICT (thread_id, url) DO UPDATE SET
            role = CASE
                WHEN app_thread_items.role = 'pin' THEN 'library'
                ELSE app_thread_items.role
            END
        RETURNING url, sort_order, role, added_at
        "#,
    )
    .bind(thread_id)
    .bind(url)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to add thread item: {}", e)))?;

    touch_thread(pool, thread_id).await.ok();
    Ok(item)
}

/// Setup and the interview are rooms of their own, and the interview is the
/// most private text on the box: neither is ever filed, so a thread's brief
/// and members can't reach them and their words can't reach a thread.
fn refuse_reserved_chat(chat_id: &str) -> Result<()> {
    if chat_id == crate::api::getting_started::GETTING_STARTED_CHAT_ID
        || chat_id == crate::api::narrative_draft::INTERVIEW_CHAT_ID
    {
        return Err(Error::InvalidInput(
            "You can't file Setup or the interview in a thread.".into(),
        ));
    }
    Ok(())
}

/// Remove a member URL from a Thread.
pub async fn remove_thread_item(pool: &PgPool, thread_id: &str, url: &str) -> Result<()> {
    // The mirror of the add: a chat leaves by its `thread_id`, which is what
    // the thread view lists it by. Deleting only the row left the chat
    // filed, so "Remove from thread" on a chat did nothing you could see.
    // Guarded on this thread so a stale remove cannot unfile a chat that has
    // since moved elsewhere.
    if let Some(chat_id) = url.strip_prefix("/chat/") {
        sqlx::query(r#"UPDATE app_chats SET thread_id = NULL WHERE id = $1 AND thread_id = $2"#)
            .bind(chat_id)
            .bind(thread_id)
            .execute(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to detach chat from thread: {}", e)))?;
    }

    let result = sqlx::query(r#"DELETE FROM app_thread_items WHERE thread_id = $1 AND url = $2"#)
        .bind(thread_id)
        .bind(url)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to remove thread item: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!(
            "Member not found in thread: {} / {}",
            thread_id, url
        )));
    }

    touch_thread(pool, thread_id).await.ok();
    Ok(())
}

/// Remove all membership entries for a given URL across every Thread.
/// Called when the underlying entity (chat/page/...) is deleted.
pub async fn remove_items_by_url(pool: &PgPool, url: &str) -> Result<i64> {
    let result = sqlx::query(r#"DELETE FROM app_thread_items WHERE url = $1"#)
        .bind(url)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to remove items by URL: {}", e)))?;

    Ok(result.rows_affected() as i64)
}

/// Reorder a Thread's members. Unknown URLs are ignored.
pub async fn reorder_thread_items(
    pool: &PgPool,
    thread_id: &str,
    req: ReorderThreadItemsRequest,
) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| Error::Database(format!("Failed to start transaction: {}", e)))?;

    for (idx, url) in req.urls.iter().enumerate() {
        sqlx::query(
            r#"UPDATE app_thread_items SET sort_order = $1 WHERE thread_id = $2 AND url = $3"#,
        )
        .bind(idx as i64)
        .bind(thread_id)
        .bind(url)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to reorder thread items: {}", e)))?;
    }

    sqlx::query(r#"UPDATE app_threads SET updated_at = now() WHERE id = $1"#)
        .bind(thread_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to touch thread: {}", e)))?;

    tx.commit()
        .await
        .map_err(|e| Error::Database(format!("Failed to commit reorder: {}", e)))?;

    Ok(())
}

// ============================================================================
// Chat ↔ Thread binding (one active Thread per chat)
// ============================================================================

/// Set or clear a chat's Thread. Passing `Some(thread_id)` also folds the chat
/// into that Thread's membership (idempotent); passing `None` detaches it. A
/// chat lives in one thread, so its `/chat/<id>` row in any other thread goes
/// in the same transaction — the chat's `thread_id` and its membership rows
/// can never diverge.
pub async fn set_chat_thread(pool: &PgPool, chat_id: &str, thread_id: Option<&str>) -> Result<()> {
    if thread_id.is_some() {
        refuse_reserved_chat(chat_id)?;
    }
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| Error::Database(format!("Failed to start transaction: {}", e)))?;

    sqlx::query(r#"UPDATE app_chats SET thread_id = $2 WHERE id = $1"#)
        .bind(chat_id)
        .bind(thread_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to set chat thread: {}", e)))?;

    sqlx::query(r#"DELETE FROM app_thread_items WHERE url = $1 AND thread_id IS DISTINCT FROM $2"#)
        .bind(format!("/chat/{}", chat_id))
        .bind(thread_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to clear chat's old membership: {}", e)))?;

    if let Some(thread_id) = thread_id {
        sqlx::query(
            r#"
            INSERT INTO app_thread_items (thread_id, url, sort_order)
            VALUES (
                $1, $2,
                (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM app_thread_items WHERE thread_id = $1)
            )
            ON CONFLICT (thread_id, url) DO NOTHING
            "#,
        )
        .bind(thread_id)
        .bind(format!("/chat/{}", chat_id))
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to fold chat into thread: {}", e)))?;

        sqlx::query(r#"UPDATE app_threads SET updated_at = now() WHERE id = $1"#)
            .bind(thread_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::Database(format!("Failed to touch thread: {}", e)))?;
    }

    tx.commit()
        .await
        .map_err(|e| Error::Database(format!("Failed to commit chat thread binding: {}", e)))?;

    Ok(())
}

/// Set a member's role. `library` grounds chat, `manuscript` is yours to write
/// (kept out of retrieval), `pin` is nav-only.
pub async fn set_thread_item_role(
    pool: &PgPool,
    thread_id: &str,
    req: SetThreadItemRoleRequest,
) -> Result<ThreadItem> {
    if !matches!(req.role.as_str(), "library" | "manuscript" | "pin") {
        return Err(Error::InvalidInput(format!(
            "Unknown thread item role: {}",
            req.role
        )));
    }

    let item = sqlx::query_as::<_, ThreadItem>(
        r#"
        UPDATE app_thread_items SET role = $3
        WHERE thread_id = $1 AND url = $2
        RETURNING url, sort_order, role, added_at
        "#,
    )
    .bind(thread_id)
    .bind(&req.url)
    .bind(&req.role)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to set thread item role: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Thread member not found: {}", req.url)))?;

    touch_thread(pool, thread_id).await.ok();
    Ok(item)
}

/// Entity ref-URL prefixes that can appear as a thread member or inside a
/// page's markdown. Kept in sync with the frontend's ref routes. (`/thing/`
/// is gone: migration 0071 dropped wiki_things and swept every stored
/// `/thing/` url — the stragglers here were what kept its ghost walking.)
const ENTITY_PREFIXES: [&str; 3] = ["/person/", "/place/", "/org/"];

/// Pull entity ref URLs out of markdown. Refs are stored inline as
/// `[@Label](/person/pe_x)` — there is no link table — so this scans for the
/// closing `](` of a link and reads the URL, mirroring `get_page_backlinks`.
fn extract_entity_urls(content: &str) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for prefix in ENTITY_PREFIXES {
        let needle = format!("]({}", prefix);
        let mut from = 0usize;
        while let Some(hit) = content[from..].find(&needle) {
            let start = from + hit + 2; // skip "]("
            let rest = &content[start..];
            match rest.find(')') {
                Some(end) => {
                    let url = &rest[..end];
                    // Ignore anything with a fragment/query — a ref is a bare route.
                    if !url.is_empty() && !url.contains(['#', '?', ' ']) {
                        out.insert(url.to_string());
                    }
                    from = start + end;
                }
                None => break,
            }
        }
    }
    out
}

/// The entities referenced across a thread's members, with co-occurrence
/// edges. Nodes come only from things the user explicitly wrote or filed —
/// entity members, and `[@ref]` links inside member pages. Nothing is inferred:
/// there is no NER over free text, so an entity that is merely *mentioned* in a
/// PDF does not appear here.
pub async fn thread_graph(pool: &PgPool, thread_id: &str) -> Result<ThreadGraph> {
    use std::collections::{HashMap, HashSet};

    // Nav-only pins are excluded: they are shortcuts, not content.
    let members: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT url FROM app_thread_items
        WHERE thread_id = $1 AND role <> 'pin'
        ORDER BY sort_order ASC, added_at ASC
        "#,
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to load thread members: {}", e)))?;

    if members.is_empty() {
        return Ok(ThreadGraph { nodes: Vec::new(), edges: Vec::new() });
    }

    // item url -> the entity urls it references.
    let mut per_item: Vec<(String, HashSet<String>)> = Vec::new();

    // Page members contribute whatever they link to; fetch their content in one go.
    let page_ids: Vec<String> = members
        .iter()
        .filter_map(|u| u.strip_prefix("/page/").map(str::to_string))
        .collect();
    let mut page_content: HashMap<String, String> = HashMap::new();
    if !page_ids.is_empty() {
        let rows: Vec<(String, String)> =
            sqlx::query_as(r#"SELECT id, content FROM app_pages WHERE id = ANY($1) AND deleted_at IS NULL"#)
                .bind(&page_ids)
                .fetch_all(pool)
                .await
                .map_err(|e| Error::Database(format!("Failed to load member pages: {}", e)))?;
        page_content.extend(rows);
    }

    for url in &members {
        let mut refs = HashSet::new();
        if ENTITY_PREFIXES.iter().any(|p| url.starts_with(p)) {
            // An entity filed directly in the thread is a node in its own right.
            refs.insert(url.clone());
        } else if let Some(pid) = url.strip_prefix("/page/") {
            if let Some(content) = page_content.get(pid) {
                refs = extract_entity_urls(content);
            }
        }
        if !refs.is_empty() {
            per_item.push((url.clone(), refs));
        }
    }

    // Resolve display names. Ids are prefixed and unique across types, so one
    // id array filters every table.
    let entity_ids: Vec<String> = per_item
        .iter()
        .flat_map(|(_, refs)| refs.iter())
        .filter_map(|u| u.rsplit('/').next().map(str::to_string))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    let mut names: HashMap<String, String> = HashMap::new();
    if !entity_ids.is_empty() {
        let rows: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT id, name AS name FROM wiki_people WHERE id = ANY($1)
            UNION ALL SELECT id, name FROM wiki_places WHERE id = ANY($1)
            UNION ALL SELECT id, name AS name FROM wiki_orgs WHERE id = ANY($1)
            "#,
        )
        .bind(&entity_ids)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to resolve entity names: {}", e)))?;
        names.extend(rows);
    }

    // Nodes, in descending reference count so the caller can size them.
    let mut node_items: HashMap<String, Vec<String>> = HashMap::new();
    for (item_url, refs) in &per_item {
        for ent in refs {
            node_items.entry(ent.clone()).or_default().push(item_url.clone());
        }
    }

    let mut nodes: Vec<ThreadGraphNode> = node_items
        .into_iter()
        .filter_map(|(url, item_urls)| {
            let mut parts = url.trim_start_matches('/').splitn(2, '/');
            let entity_type = parts.next()?.to_string();
            let id = parts.next()?.to_string();
            // An unresolvable id is a dangling ref (entity deleted, stale link).
            // Drop it rather than render a node with no name.
            let name = names.get(&id)?.clone();
            Some(ThreadGraphNode { url, entity_type, name, item_urls })
        })
        .collect();
    nodes.sort_by(|a, b| {
        b.item_urls
            .len()
            .cmp(&a.item_urls.len())
            .then_with(|| a.name.cmp(&b.name))
    });

    let live: HashSet<&str> = nodes.iter().map(|n| n.url.as_str()).collect();

    // Edges: entities sharing a member. Undirected, deduped by sorted pair.
    let mut pair_weight: HashMap<(String, String), i64> = HashMap::new();
    for (_, refs) in &per_item {
        let mut present: Vec<&str> = refs
            .iter()
            .map(String::as_str)
            .filter(|u| live.contains(u))
            .collect();
        present.sort_unstable();
        for i in 0..present.len() {
            for j in (i + 1)..present.len() {
                *pair_weight
                    .entry((present[i].to_string(), present[j].to_string()))
                    .or_insert(0) += 1;
            }
        }
    }

    let mut edges: Vec<ThreadGraphEdge> = pair_weight
        .into_iter()
        .map(|((source, target), weight)| ThreadGraphEdge { source, target, weight })
        .collect();
    edges.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.source.cmp(&b.source)));

    Ok(ThreadGraph { nodes, edges })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn seed_chat(pool: &PgPool, id: &str) {
        sqlx::query("INSERT INTO app_chats (id, title, message_count) VALUES ($1, 'Chat', 0)")
            .bind(id)
            .execute(pool)
            .await
            .expect("seed a chat");
    }

    async fn chat_rows(pool: &PgPool, chat_id: &str) -> Vec<String> {
        sqlx::query_scalar("SELECT thread_id FROM app_thread_items WHERE url = $1 ORDER BY thread_id")
            .bind(format!("/chat/{chat_id}"))
            .fetch_all(pool)
            .await
            .expect("read membership")
    }

    async fn chat_project(pool: &PgPool, chat_id: &str) -> Option<String> {
        sqlx::query_scalar("SELECT thread_id FROM app_chats WHERE id = $1")
            .bind(chat_id)
            .fetch_one(pool)
            .await
            .expect("read chat")
    }

    fn named(name: &str) -> CreateThreadRequest {
        CreateThreadRequest { name: name.into(), icon: None, accent_color: None, thing_id: None }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_chat_moved_between_projects_leaves_the_first(pool: PgPool) {
        let a = create_thread(&pool, named("A")).await.expect("create A");
        let b = create_thread(&pool, named("B")).await.expect("create B");
        seed_chat(&pool, "chat_move").await;

        set_chat_thread(&pool, "chat_move", Some(&a.id)).await.expect("file in A");
        set_chat_thread(&pool, "chat_move", Some(&b.id)).await.expect("move to B");

        assert_eq!(chat_project(&pool, "chat_move").await.as_deref(), Some(b.id.as_str()));
        assert_eq!(chat_rows(&pool, "chat_move").await, vec![b.id.clone()]);

        set_chat_thread(&pool, "chat_move", None).await.expect("unfile");
        assert_eq!(chat_project(&pool, "chat_move").await, None);
        assert!(chat_rows(&pool, "chat_move").await.is_empty());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn removing_a_chat_from_its_project_unfiles_it(pool: PgPool) {
        let a = create_thread(&pool, named("A")).await.expect("create A");
        seed_chat(&pool, "chat_rm").await;
        add_thread_item(&pool, &a.id, AddThreadItemRequest { url: "/chat/chat_rm".into() })
            .await
            .expect("add chat");
        assert_eq!(chat_project(&pool, "chat_rm").await.as_deref(), Some(a.id.as_str()));

        remove_thread_item(&pool, &a.id, "/chat/chat_rm").await.expect("remove chat");
        assert_eq!(chat_project(&pool, "chat_rm").await, None);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn the_detail_lists_every_chat_filed_there(pool: PgPool) {
        let a = create_thread(&pool, named("A")).await.expect("create A");
        for i in 0..30 {
            let id = format!("chat_many_{i}");
            seed_chat(&pool, &id).await;
            set_chat_thread(&pool, &id, Some(&a.id)).await.expect("file chat");
        }
        let detail = get_thread(&pool, &a.id).await.expect("detail");
        assert_eq!(detail.chats.len(), 30);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn setup_and_the_interview_are_never_filed(pool: PgPool) {
        let a = create_thread(&pool, named("A")).await.expect("create A");
        for id in [
            crate::api::getting_started::GETTING_STARTED_CHAT_ID,
            crate::api::narrative_draft::INTERVIEW_CHAT_ID,
        ] {
            seed_chat(&pool, id).await;
            let url = format!("/chat/{id}");
            assert!(add_thread_item(&pool, &a.id, AddThreadItemRequest { url }).await.is_err());
            assert!(set_chat_thread(&pool, id, Some(&a.id)).await.is_err());
            assert_eq!(chat_project(&pool, id).await, None);
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn the_detail_names_its_members(pool: PgPool) {
        let a = create_thread(&pool, named("A")).await.expect("create A");
        add_thread_item(&pool, &a.id, AddThreadItemRequest { url: "https://www.example.com/quotes".into() })
            .await
            .expect("add link");
        let detail = get_thread(&pool, &a.id).await.expect("detail");
        let item = &detail.items[0];
        assert_eq!(item.title.as_deref(), Some("example.com"));
        assert_eq!(item.kind.as_deref(), Some("web"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_page_lists_the_projects_it_is_in(pool: PgPool) {
        let a = create_thread(&pool, named("A")).await.expect("create A");
        let b = create_thread(&pool, named("B")).await.expect("create B");
        create_thread(&pool, named("C")).await.expect("create C");
        for p in [&a, &b] {
            add_thread_item(&pool, &p.id, AddThreadItemRequest { url: "/page/page_x".into() })
                .await
                .expect("file page");
        }
        let listed = list_threads(&pool, false, Some("/page/page_x")).await.expect("list");
        let mut ids: Vec<String> = listed.threads.into_iter().map(|p| p.id).collect();
        ids.sort();
        let mut want = vec![a.id, b.id];
        want.sort();
        assert_eq!(ids, want);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn item_count_does_not_count_chats_twice(pool: PgPool) {
        let a = create_thread(&pool, named("A")).await.expect("create A");
        seed_chat(&pool, "chat_count").await;
        set_chat_thread(&pool, "chat_count", Some(&a.id)).await.expect("file chat");
        add_thread_item(&pool, &a.id, AddThreadItemRequest { url: "https://example.com".into() })
            .await
            .expect("add link");

        let listed = list_threads(&pool, false, None).await.expect("list");
        let row = listed.threads.iter().find(|p| p.id == a.id).expect("thread listed");
        assert_eq!((row.chat_count, row.item_count), (1, 1));
    }

    /// A block page's `content` is its export, and a mention exports as the
    /// ref the graph reads, so a person a block page mentions is on the
    /// graph of a thread the page is in.
    #[sqlx::test(migrations = "./migrations")]
    async fn a_block_page_puts_who_it_mentions_on_the_graph(pool: PgPool) {
        use crate::api::pages::{create_page_as, CreatePageRequest, PageFormat};
        sqlx::query("INSERT INTO wiki_people (id, name) VALUES ('person_1', 'Nick')")
            .execute(&pool)
            .await
            .expect("seed a person");
        let a = create_thread(&pool, named("Trip")).await.expect("create");
        let page = create_page_as(
            &pool,
            CreatePageRequest {
                title: "Plan".into(),
                content: "## Plan\n\nLunch with [@Nick](/person/person_1).\n".into(),
                thread_id: Some(a.id.clone()),
                icon: None,
                icon_color: None,
                cover_url: None,
                tags: None,
                format: Some(crate::api::pages::PageFormat::Markdown),
            },
            PageFormat::Tree,
        )
        .await
        .expect("create a block page")
        .page;

        let graph = thread_graph(&pool, &a.id).await.expect("graph");
        assert_eq!(graph.nodes.len(), 1, "{:?}", graph.nodes.iter().map(|n| &n.url).collect::<Vec<_>>());
        let nick = &graph.nodes[0];
        assert_eq!((nick.url.as_str(), nick.name.as_str()), ("/person/person_1", "Nick"));
        assert_eq!(nick.item_urls, [format!("/page/{}", page.id)]);
    }

    /// A thread ends finished or dropped and leaves the working list.
    /// Correcting how it ended keeps the day it ended, and reopening clears
    /// both.
    #[sqlx::test(migrations = "./migrations")]
    async fn a_project_ends_finished_or_dropped(pool: PgPool) {
        let p = create_thread(&pool, named("Repaint the hallway")).await.unwrap();
        assert!(p.archived_at.is_none() && p.outcome.is_none());

        archive_thread(&pool, &p.id, Outcome::Finished).await.unwrap();
        let ended = get_thread(&pool, &p.id).await.unwrap().thread;
        assert_eq!(ended.outcome.as_deref(), Some("finished"));
        let ended_at = ended.archived_at.expect("an ended thread says when");
        assert!(list_threads(&pool, false, None).await.unwrap().threads.is_empty());
        assert_eq!(list_threads(&pool, true, None).await.unwrap().threads[0].outcome.as_deref(), Some("finished"));

        archive_thread(&pool, &p.id, Outcome::Dropped).await.unwrap();
        let corrected = get_thread(&pool, &p.id).await.unwrap().thread;
        assert_eq!(corrected.outcome.as_deref(), Some("dropped"));
        assert_eq!(corrected.archived_at, Some(ended_at), "correcting how it ended keeps when");

        unarchive_thread(&pool, &p.id).await.unwrap();
        let open = get_thread(&pool, &p.id).await.unwrap().thread;
        assert!(open.archived_at.is_none() && open.outcome.is_none());
        assert!(archive_thread(&pool, "nb_missing", Outcome::Finished).await.is_err());
    }

    /// A thread sits under a thing or alone, and reads back the thing's name.
    #[sqlx::test(migrations = "./migrations")]
    async fn a_project_sits_under_a_thing(pool: PgPool) {
        let house = crate::api::things::create_thing(&pool, "The house").await.unwrap();
        let under = CreateThreadRequest { thing_id: Some(house.id.clone()), ..named("Repaint the hallway") };
        let p = create_thread(&pool, under).await.unwrap();
        assert_eq!(p.thing_title.as_deref(), Some("The house"));
        let listed = list_threads(&pool, false, None).await.unwrap().threads.remove(0);
        assert_eq!(listed.thing_id.as_deref(), Some(house.id.as_str()));
        assert_eq!(listed.thing_title.as_deref(), Some("The house"));

        // An edit that says nothing about the thing leaves it where it is.
        let renamed = UpdateThreadRequest { name: Some("Repaint the hall".into()), ..Default::default() };
        assert_eq!(update_thread(&pool, &p.id, renamed).await.unwrap().thing_id, Some(house.id.clone()));

        let alone = UpdateThreadRequest { thing_id: Some(None), ..Default::default() };
        let out = update_thread(&pool, &p.id, alone).await.unwrap();
        assert!(out.thing_id.is_none() && out.thing_title.is_none());

        let nowhere = UpdateThreadRequest { thing_id: Some(Some("thing_missing".into())), ..Default::default() };
        assert!(update_thread(&pool, &p.id, nowhere).await.is_err());
        let orphan = CreateThreadRequest { thing_id: Some("thing_missing".into()), ..named("Fix the gate") };
        assert!(create_thread(&pool, orphan).await.is_err());
    }
}

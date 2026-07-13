//! Notebooks API — the "room" a chat lives in.
//!
//! A Notebook is a manual collection the user returns to: a project, pet, hobby,
//! goal, or topic. It gathers entities, chats, and pages as URL-native members
//! (`wiki_story_members`) and carries a single accent tint plus a catch-up memo
//! (`current_status`) shown when you re-enter the room.
//!
//! A chat lives in at most one Notebook (`app_chats.notebook_id`). Entering a Notebook
//! weights its members in retrieval; conversely the chat is folded into the
//! Notebook's corpus. Membership is manual in v1 — there is no smart/query view.
//!
//! This absorbs the old workspace-shell and the folder role that `wiki_things`
//! used to play (pins + memo); Things are now pure reference entities.

use crate::error::{Error, Result};
use crate::ids::{generate_id, NOTEBOOK_PREFIX};
use crate::types::Timestamp;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Notebook {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub accent_color: Option<String>,
    /// Transient "state of the room" catch-up memo (what you read on re-entry).
    pub current_status: Option<String>,
    pub current_status_at: Option<Timestamp>,
    /// Persistent behavior for the assistant in this notebook (Claude-Projects-
    /// style custom instructions) — distinct from the transient memo above.
    pub instructions: Option<String>,
    pub sort_order: i32,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// List-view summary — adds member and chat counts.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct NotebookSummary {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub accent_color: Option<String>,
    pub current_status: Option<String>,
    pub current_status_at: Option<Timestamp>,
    pub instructions: Option<String>,
    pub sort_order: i32,
    pub item_count: i64,
    pub chat_count: i64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A single URL-native member of a Notebook.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct NotebookItem {
    pub url: String,
    pub sort_order: i32,
    pub added_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookDetail {
    #[serde(flatten)]
    pub notebook: Notebook,
    pub items: Vec<NotebookItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookListResponse {
    pub notebooks: Vec<NotebookSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNotebookRequest {
    pub name: String,
    pub icon: Option<String>,
    pub accent_color: Option<String>,
}

/// Update a Notebook. `Option<Option<T>>` fields are tri-state: absent = leave,
/// `Some(None)` = clear, `Some(Some(v))` = set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNotebookRequest {
    pub name: Option<String>,
    pub icon: Option<Option<String>>,
    pub accent_color: Option<Option<String>>,
    pub current_status: Option<Option<String>>,
    pub instructions: Option<Option<String>>,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddNotebookItemRequest {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorderNotebookItemsRequest {
    pub urls: Vec<String>,
}

// ============================================================================
// Notebook CRUD
// ============================================================================

/// List all Notebooks, most-recently-active first, with member and chat counts.
pub async fn list_notebooks(pool: &PgPool) -> Result<NotebookListResponse> {
    let notebooks = sqlx::query_as::<_, NotebookSummary>(
        r#"
        SELECT
            s.id, s.name, s.icon, s.accent_color,
            s.current_status, s.current_status_at, s.instructions, s.sort_order,
            COALESCE((SELECT COUNT(*) FROM wiki_story_members WHERE notebook_id = s.id), 0) AS item_count,
            COALESCE((SELECT COUNT(*) FROM app_chats       WHERE notebook_id = s.id), 0) AS chat_count,
            s.created_at, s.updated_at
        FROM wiki_stories s
        ORDER BY s.sort_order ASC, s.updated_at DESC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list notebooks: {}", e)))?;

    Ok(NotebookListResponse { notebooks })
}

/// Get a single Notebook with its ordered members.
pub async fn get_notebook(pool: &PgPool, id: &str) -> Result<NotebookDetail> {
    let notebook = sqlx::query_as::<_, Notebook>(
        r#"
        SELECT id, name, icon, accent_color, current_status, current_status_at,
               instructions, sort_order, created_at, updated_at
        FROM wiki_stories
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get notebook: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Notebook not found: {}", id)))?;

    let items = sqlx::query_as::<_, NotebookItem>(
        r#"
        SELECT url, sort_order, added_at
        FROM wiki_story_members
        WHERE notebook_id = $1
        ORDER BY sort_order ASC, added_at ASC
        "#,
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get notebook items: {}", e)))?;

    Ok(NotebookDetail { notebook, items })
}

/// Create a new Notebook.
pub async fn create_notebook(pool: &PgPool, req: CreateNotebookRequest) -> Result<Notebook> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(Error::InvalidInput("Notebook name cannot be empty".into()));
    }

    let timestamp = chrono::Utc::now().to_rfc3339();
    let id = generate_id(NOTEBOOK_PREFIX, &[name, &timestamp]);

    let notebook = sqlx::query_as::<_, Notebook>(
        r#"
        INSERT INTO wiki_stories (id, name, icon, accent_color)
        VALUES ($1, $2, $3, $4)
        RETURNING id, name, icon, accent_color, current_status, current_status_at,
                  instructions, sort_order, created_at, updated_at
        "#,
    )
    .bind(&id)
    .bind(name)
    .bind(&req.icon)
    .bind(&req.accent_color)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create notebook: {}", e)))?;

    Ok(notebook)
}

/// Update a Notebook. Only provided fields change. Touching `current_status`
/// stamps `current_status_at`.
pub async fn update_notebook(pool: &PgPool, id: &str, req: UpdateNotebookRequest) -> Result<Notebook> {
    let existing = sqlx::query_as::<_, Notebook>(
        r#"
        SELECT id, name, icon, accent_color, current_status, current_status_at,
               instructions, sort_order, created_at, updated_at
        FROM wiki_stories WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get notebook: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Notebook not found: {}", id)))?;

    let name = req.name.as_deref().unwrap_or(&existing.name).trim().to_string();
    if name.is_empty() {
        return Err(Error::InvalidInput("Notebook name cannot be empty".into()));
    }

    let icon = match req.icon {
        Some(val) => val,
        None => existing.icon,
    };
    let accent_color = match req.accent_color {
        Some(val) => val,
        None => existing.accent_color,
    };
    let status_changed = req.current_status.is_some();
    let current_status = match req.current_status {
        Some(val) => val,
        None => existing.current_status,
    };
    let sort_order = req.sort_order.unwrap_or(existing.sort_order);
    let instructions = match req.instructions {
        Some(val) => val,
        None => existing.instructions,
    };

    let notebook = sqlx::query_as::<_, Notebook>(
        r#"
        UPDATE wiki_stories
        SET name = $2,
            icon = $3,
            accent_color = $4,
            current_status = $5,
            current_status_at = CASE WHEN $6 THEN now() ELSE current_status_at END,
            sort_order = $7,
            instructions = $8
        WHERE id = $1
        RETURNING id, name, icon, accent_color, current_status, current_status_at,
                  instructions, sort_order, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(&name)
    .bind(&icon)
    .bind(&accent_color)
    .bind(&current_status)
    .bind(status_changed)
    .bind(sort_order)
    .bind(&instructions)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update notebook: {}", e)))?;

    Ok(notebook)
}

/// Delete a Notebook. Members cascade; chats in it have `notebook_id` set to NULL.
pub async fn delete_notebook(pool: &PgPool, id: &str) -> Result<()> {
    let result = sqlx::query(r#"DELETE FROM wiki_stories WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to delete notebook: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Notebook not found: {}", id)));
    }
    Ok(())
}

/// Touch a Notebook's updated_at to reflect activity.
pub async fn touch_notebook(pool: &PgPool, id: &str) -> Result<()> {
    sqlx::query(r#"UPDATE wiki_stories SET updated_at = now() WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to touch notebook: {}", e)))?;
    Ok(())
}

// ============================================================================
// Membership
// ============================================================================

/// Add a member URL to a Notebook. Idempotent on (notebook_id, url).
pub async fn add_notebook_item(pool: &PgPool, notebook_id: &str, req: AddNotebookItemRequest) -> Result<NotebookItem> {
    let url = req.url.trim();
    if url.is_empty() {
        return Err(Error::InvalidInput("Member url cannot be empty".into()));
    }

    let exists: Option<String> = sqlx::query_scalar(r#"SELECT id FROM wiki_stories WHERE id = $1"#)
        .bind(notebook_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to verify notebook: {}", e)))?;
    if exists.is_none() {
        return Err(Error::NotFound(format!("Notebook not found: {}", notebook_id)));
    }

    let item = sqlx::query_as::<_, NotebookItem>(
        r#"
        INSERT INTO wiki_story_members (notebook_id, url, sort_order)
        VALUES (
            $1, $2,
            (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM wiki_story_members WHERE notebook_id = $1)
        )
        ON CONFLICT (notebook_id, url) DO UPDATE SET url = EXCLUDED.url
        RETURNING url, sort_order, added_at
        "#,
    )
    .bind(notebook_id)
    .bind(url)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to add notebook item: {}", e)))?;

    touch_notebook(pool, notebook_id).await.ok();
    Ok(item)
}

/// Remove a member URL from a Notebook.
pub async fn remove_notebook_item(pool: &PgPool, notebook_id: &str, url: &str) -> Result<()> {
    let result = sqlx::query(r#"DELETE FROM wiki_story_members WHERE notebook_id = $1 AND url = $2"#)
        .bind(notebook_id)
        .bind(url)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to remove notebook item: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!(
            "Member not found in notebook: {} / {}",
            notebook_id, url
        )));
    }

    touch_notebook(pool, notebook_id).await.ok();
    Ok(())
}

/// Remove all membership entries for a given URL across every Notebook.
/// Called when the underlying entity (chat/page/...) is deleted.
pub async fn remove_items_by_url(pool: &PgPool, url: &str) -> Result<i64> {
    let result = sqlx::query(r#"DELETE FROM wiki_story_members WHERE url = $1"#)
        .bind(url)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to remove items by URL: {}", e)))?;

    Ok(result.rows_affected() as i64)
}

/// Reorder a Notebook's members. Unknown URLs are ignored.
pub async fn reorder_notebook_items(
    pool: &PgPool,
    notebook_id: &str,
    req: ReorderNotebookItemsRequest,
) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| Error::Database(format!("Failed to start transaction: {}", e)))?;

    for (idx, url) in req.urls.iter().enumerate() {
        sqlx::query(
            r#"UPDATE wiki_story_members SET sort_order = $1 WHERE notebook_id = $2 AND url = $3"#,
        )
        .bind(idx as i64)
        .bind(notebook_id)
        .bind(url)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to reorder notebook items: {}", e)))?;
    }

    sqlx::query(r#"UPDATE wiki_stories SET updated_at = now() WHERE id = $1"#)
        .bind(notebook_id)
        .execute(&mut *tx)
        .await
        .ok();

    tx.commit()
        .await
        .map_err(|e| Error::Database(format!("Failed to commit reorder: {}", e)))?;

    Ok(())
}

// ============================================================================
// Chat ↔ Notebook binding (one active Notebook per chat)
// ============================================================================

/// Set or clear a chat's Notebook. Passing `Some(notebook_id)` also folds the chat
/// into that Notebook's membership (idempotent); passing `None` detaches it. The
/// row update and the membership fold run in one transaction so the chat's
/// `notebook_id` and its `/chat/<id>` membership row can never diverge.
pub async fn set_chat_notebook(pool: &PgPool, chat_id: &str, notebook_id: Option<&str>) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| Error::Database(format!("Failed to start transaction: {}", e)))?;

    sqlx::query(r#"UPDATE app_chats SET notebook_id = $2 WHERE id = $1"#)
        .bind(chat_id)
        .bind(notebook_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to set chat notebook: {}", e)))?;

    if let Some(notebook_id) = notebook_id {
        sqlx::query(
            r#"
            INSERT INTO wiki_story_members (notebook_id, url, sort_order)
            VALUES (
                $1, $2,
                (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM wiki_story_members WHERE notebook_id = $1)
            )
            ON CONFLICT (notebook_id, url) DO NOTHING
            "#,
        )
        .bind(notebook_id)
        .bind(format!("/chat/{}", chat_id))
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to fold chat into notebook: {}", e)))?;

        sqlx::query(r#"UPDATE wiki_stories SET updated_at = now() WHERE id = $1"#)
            .bind(notebook_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::Database(format!("Failed to touch notebook: {}", e)))?;
    }

    tx.commit()
        .await
        .map_err(|e| Error::Database(format!("Failed to commit chat notebook binding: {}", e)))?;

    Ok(())
}

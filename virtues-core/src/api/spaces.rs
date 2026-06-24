//! Spaces API — the "room" a chat lives in.
//!
//! A Space is a manual collection the user returns to: a project, pet, hobby,
//! goal, or topic. It gathers entities, chats, and pages as URL-native members
//! (`app_space_items`) and carries a single accent tint plus a catch-up memo
//! (`current_status`) shown when you re-enter the room.
//!
//! A chat lives in at most one Space (`app_chats.space_id`). Entering a Space
//! weights its members in retrieval; conversely the chat is folded into the
//! Space's corpus. Membership is manual in v1 — there is no smart/query view.
//!
//! This replaces the old workspace-shell `app_spaces` (theme/tab-state) and the
//! folder role that `wiki_things` used to play (pins + memo).

use crate::error::{Error, Result};
use crate::ids::{generate_id, SPACE_PREFIX};
use crate::types::Timestamp;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Space {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub accent_color: Option<String>,
    pub current_status: Option<String>,
    pub current_status_at: Option<Timestamp>,
    pub sort_order: i32,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// List-view summary — adds member and chat counts.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SpaceSummary {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub accent_color: Option<String>,
    pub current_status: Option<String>,
    pub current_status_at: Option<Timestamp>,
    pub sort_order: i32,
    pub item_count: i64,
    pub chat_count: i64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A single URL-native member of a Space.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SpaceItem {
    pub url: String,
    pub sort_order: i32,
    pub added_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceDetail {
    #[serde(flatten)]
    pub space: Space,
    pub items: Vec<SpaceItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceListResponse {
    pub spaces: Vec<SpaceSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSpaceRequest {
    pub name: String,
    pub icon: Option<String>,
    pub accent_color: Option<String>,
}

/// Update a Space. `Option<Option<T>>` fields are tri-state: absent = leave,
/// `Some(None)` = clear, `Some(Some(v))` = set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSpaceRequest {
    pub name: Option<String>,
    pub icon: Option<Option<String>>,
    pub accent_color: Option<Option<String>>,
    pub current_status: Option<Option<String>>,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddSpaceItemRequest {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorderSpaceItemsRequest {
    pub urls: Vec<String>,
}

// ============================================================================
// Space CRUD
// ============================================================================

/// List all Spaces, most-recently-active first, with member and chat counts.
pub async fn list_spaces(pool: &PgPool) -> Result<SpaceListResponse> {
    let spaces = sqlx::query_as::<_, SpaceSummary>(
        r#"
        SELECT
            s.id, s.name, s.icon, s.accent_color,
            s.current_status, s.current_status_at, s.sort_order,
            COALESCE((SELECT COUNT(*) FROM app_space_items WHERE space_id = s.id), 0) AS item_count,
            COALESCE((SELECT COUNT(*) FROM app_chats       WHERE space_id = s.id), 0) AS chat_count,
            s.created_at, s.updated_at
        FROM app_spaces s
        ORDER BY s.sort_order ASC, s.updated_at DESC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list spaces: {}", e)))?;

    Ok(SpaceListResponse { spaces })
}

/// Get a single Space with its ordered members.
pub async fn get_space(pool: &PgPool, id: &str) -> Result<SpaceDetail> {
    let space = sqlx::query_as::<_, Space>(
        r#"
        SELECT id, name, icon, accent_color, current_status, current_status_at,
               sort_order, created_at, updated_at
        FROM app_spaces
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get space: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Space not found: {}", id)))?;

    let items = sqlx::query_as::<_, SpaceItem>(
        r#"
        SELECT url, sort_order, added_at
        FROM app_space_items
        WHERE space_id = $1
        ORDER BY sort_order ASC, added_at ASC
        "#,
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get space items: {}", e)))?;

    Ok(SpaceDetail { space, items })
}

/// Create a new Space.
pub async fn create_space(pool: &PgPool, req: CreateSpaceRequest) -> Result<Space> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(Error::InvalidInput("Space name cannot be empty".into()));
    }

    let timestamp = chrono::Utc::now().to_rfc3339();
    let id = generate_id(SPACE_PREFIX, &[name, &timestamp]);

    let space = sqlx::query_as::<_, Space>(
        r#"
        INSERT INTO app_spaces (id, name, icon, accent_color)
        VALUES ($1, $2, $3, $4)
        RETURNING id, name, icon, accent_color, current_status, current_status_at,
                  sort_order, created_at, updated_at
        "#,
    )
    .bind(&id)
    .bind(name)
    .bind(&req.icon)
    .bind(&req.accent_color)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create space: {}", e)))?;

    Ok(space)
}

/// Update a Space. Only provided fields change. Touching `current_status`
/// stamps `current_status_at`.
pub async fn update_space(pool: &PgPool, id: &str, req: UpdateSpaceRequest) -> Result<Space> {
    let existing = sqlx::query_as::<_, Space>(
        r#"
        SELECT id, name, icon, accent_color, current_status, current_status_at,
               sort_order, created_at, updated_at
        FROM app_spaces WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get space: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Space not found: {}", id)))?;

    let name = req.name.as_deref().unwrap_or(&existing.name).trim().to_string();
    if name.is_empty() {
        return Err(Error::InvalidInput("Space name cannot be empty".into()));
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

    let space = sqlx::query_as::<_, Space>(
        r#"
        UPDATE app_spaces
        SET name = $2,
            icon = $3,
            accent_color = $4,
            current_status = $5,
            current_status_at = CASE WHEN $6 THEN now() ELSE current_status_at END,
            sort_order = $7
        WHERE id = $1
        RETURNING id, name, icon, accent_color, current_status, current_status_at,
                  sort_order, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(&name)
    .bind(&icon)
    .bind(&accent_color)
    .bind(&current_status)
    .bind(status_changed)
    .bind(sort_order)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update space: {}", e)))?;

    Ok(space)
}

/// Delete a Space. Members cascade; chats in it have `space_id` set to NULL.
pub async fn delete_space(pool: &PgPool, id: &str) -> Result<()> {
    let result = sqlx::query(r#"DELETE FROM app_spaces WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to delete space: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Space not found: {}", id)));
    }
    Ok(())
}

/// Touch a Space's updated_at to reflect activity.
pub async fn touch_space(pool: &PgPool, id: &str) -> Result<()> {
    sqlx::query(r#"UPDATE app_spaces SET updated_at = now() WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to touch space: {}", e)))?;
    Ok(())
}

// ============================================================================
// Membership
// ============================================================================

/// Add a member URL to a Space. Idempotent on (space_id, url).
pub async fn add_space_item(pool: &PgPool, space_id: &str, req: AddSpaceItemRequest) -> Result<SpaceItem> {
    let url = req.url.trim();
    if url.is_empty() {
        return Err(Error::InvalidInput("Member url cannot be empty".into()));
    }

    let exists: Option<String> = sqlx::query_scalar(r#"SELECT id FROM app_spaces WHERE id = $1"#)
        .bind(space_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to verify space: {}", e)))?;
    if exists.is_none() {
        return Err(Error::NotFound(format!("Space not found: {}", space_id)));
    }

    let item = sqlx::query_as::<_, SpaceItem>(
        r#"
        INSERT INTO app_space_items (space_id, url, sort_order)
        VALUES (
            $1, $2,
            (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM app_space_items WHERE space_id = $1)
        )
        ON CONFLICT (space_id, url) DO UPDATE SET url = EXCLUDED.url
        RETURNING url, sort_order, added_at
        "#,
    )
    .bind(space_id)
    .bind(url)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to add space item: {}", e)))?;

    touch_space(pool, space_id).await.ok();
    Ok(item)
}

/// Remove a member URL from a Space.
pub async fn remove_space_item(pool: &PgPool, space_id: &str, url: &str) -> Result<()> {
    let result = sqlx::query(r#"DELETE FROM app_space_items WHERE space_id = $1 AND url = $2"#)
        .bind(space_id)
        .bind(url)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to remove space item: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!(
            "Member not found in space: {} / {}",
            space_id, url
        )));
    }

    touch_space(pool, space_id).await.ok();
    Ok(())
}

/// Remove all membership entries for a given URL across every Space.
/// Called when the underlying entity (chat/page/...) is deleted.
pub async fn remove_items_by_url(pool: &PgPool, url: &str) -> Result<i64> {
    let result = sqlx::query(r#"DELETE FROM app_space_items WHERE url = $1"#)
        .bind(url)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to remove items by URL: {}", e)))?;

    Ok(result.rows_affected() as i64)
}

/// Reorder a Space's members. Unknown URLs are ignored.
pub async fn reorder_space_items(
    pool: &PgPool,
    space_id: &str,
    req: ReorderSpaceItemsRequest,
) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| Error::Database(format!("Failed to start transaction: {}", e)))?;

    for (idx, url) in req.urls.iter().enumerate() {
        sqlx::query(
            r#"UPDATE app_space_items SET sort_order = $1 WHERE space_id = $2 AND url = $3"#,
        )
        .bind(idx as i64)
        .bind(space_id)
        .bind(url)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to reorder space items: {}", e)))?;
    }

    sqlx::query(r#"UPDATE app_spaces SET updated_at = now() WHERE id = $1"#)
        .bind(space_id)
        .execute(&mut *tx)
        .await
        .ok();

    tx.commit()
        .await
        .map_err(|e| Error::Database(format!("Failed to commit reorder: {}", e)))?;

    Ok(())
}

// ============================================================================
// Chat ↔ Space binding (one active Space per chat)
// ============================================================================

/// Set or clear a chat's Space. Passing `Some(space_id)` also folds the chat
/// into that Space's membership (idempotent); passing `None` detaches it. The
/// row update and the membership fold run in one transaction so the chat's
/// `space_id` and its `/chat/<id>` membership row can never diverge.
pub async fn set_chat_space(pool: &PgPool, chat_id: &str, space_id: Option<&str>) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| Error::Database(format!("Failed to start transaction: {}", e)))?;

    sqlx::query(r#"UPDATE app_chats SET space_id = $2 WHERE id = $1"#)
        .bind(chat_id)
        .bind(space_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to set chat space: {}", e)))?;

    if let Some(space_id) = space_id {
        sqlx::query(
            r#"
            INSERT INTO app_space_items (space_id, url, sort_order)
            VALUES (
                $1, $2,
                (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM app_space_items WHERE space_id = $1)
            )
            ON CONFLICT (space_id, url) DO NOTHING
            "#,
        )
        .bind(space_id)
        .bind(format!("/chat/{}", chat_id))
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to fold chat into space: {}", e)))?;

        sqlx::query(r#"UPDATE app_spaces SET updated_at = now() WHERE id = $1"#)
            .bind(space_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::Database(format!("Failed to touch space: {}", e)))?;
    }

    tx.commit()
        .await
        .map_err(|e| Error::Database(format!("Failed to commit chat space binding: {}", e)))?;

    Ok(())
}

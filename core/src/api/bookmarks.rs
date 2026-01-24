//! Bookmarks API
//!
//! This module provides functions for managing user bookmarks.
//! Bookmarks can reference either tabs (routes/pages) or wiki entities
//! (Person, Place, Organization, Thing).

use crate::error::{Error, Result};
use crate::types::Timestamp;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

// ============================================================================
// Types
// ============================================================================

/// A bookmark record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Bookmark {
    pub id: String,
    pub bookmark_type: String,
    pub route: Option<String>,
    pub tab_type: Option<String>,
    pub label: String,
    pub icon: Option<String>,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub entity_slug: Option<String>,
    pub sort_order: i32,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Request to create a tab bookmark
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTabBookmarkRequest {
    pub route: String,
    pub tab_type: String,
    pub label: String,
    pub icon: Option<String>,
}

/// Request to create an entity bookmark
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEntityBookmarkRequest {
    pub entity_type: String,
    pub entity_id: String,
    pub entity_slug: String,
    pub label: String,
    pub icon: Option<String>,
}

/// Response for checking if something is bookmarked
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookmarkStatus {
    pub is_bookmarked: bool,
    pub bookmark_id: Option<String>,
}

/// Response for toggle operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToggleBookmarkResponse {
    pub bookmarked: bool,
    pub bookmark: Option<Bookmark>,
}

// ============================================================================
// CRUD Operations
// ============================================================================

/// List all bookmarks ordered by sort_order, then created_at
pub async fn list_bookmarks(pool: &SqlitePool) -> Result<Vec<Bookmark>> {
    let bookmarks = sqlx::query_as::<_, Bookmark>(
        r#"
        SELECT id, bookmark_type, route, tab_type, label, icon,
               entity_type, entity_id, entity_slug,
               sort_order, created_at, updated_at
        FROM app_bookmarks
        ORDER BY sort_order ASC, created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list bookmarks: {}", e)))?;

    Ok(bookmarks)
}

/// Create a tab bookmark
pub async fn create_tab_bookmark(
    pool: &SqlitePool,
    req: CreateTabBookmarkRequest,
) -> Result<Bookmark> {
    // Check for duplicate route
    let existing = sqlx::query_scalar::<_, String>(
        r#"SELECT id FROM app_bookmarks WHERE bookmark_type = 'tab' AND route = $1"#,
    )
    .bind(&req.route)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to check existing bookmark: {}", e)))?;

    if existing.is_some() {
        return Err(Error::InvalidInput(
            "This page is already bookmarked".into(),
        ));
    }

    // Get next sort order
    let next_order: i32 =
        sqlx::query_scalar(r#"SELECT COALESCE(MAX(sort_order), -1) + 1 FROM app_bookmarks"#)
            .fetch_one(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to get sort order: {}", e)))?;

    let id = crate::ids::generate_id(crate::ids::BOOKMARK_PREFIX, &[&req.route]);
    let bookmark = sqlx::query_as::<_, Bookmark>(
        r#"
        INSERT INTO app_bookmarks (id, bookmark_type, route, tab_type, label, icon, sort_order)
        VALUES ($1, 'tab', $2, $3, $4, $5, $6)
        RETURNING id, bookmark_type, route, tab_type, label, icon,
                  entity_type, entity_id, entity_slug, sort_order, created_at, updated_at
        "#,
    )
    .bind(&id)
    .bind(&req.route)
    .bind(&req.tab_type)
    .bind(&req.label)
    .bind(&req.icon)
    .bind(next_order)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create bookmark: {}", e)))?;

    Ok(bookmark)
}

/// Create an entity bookmark
pub async fn create_entity_bookmark(
    pool: &SqlitePool,
    req: CreateEntityBookmarkRequest,
) -> Result<Bookmark> {
    // Validate entity_type
    let valid_types = ["person", "place", "organization", "thing"];
    if !valid_types.contains(&req.entity_type.as_str()) {
        return Err(Error::InvalidInput(format!(
            "Invalid entity_type '{}'. Must be one of: {:?}",
            req.entity_type, valid_types
        )));
    }

    // Check for duplicate entity
    let entity_id_str = &req.entity_id;
    let existing = sqlx::query_scalar::<_, String>(
        r#"SELECT id FROM app_bookmarks WHERE bookmark_type = 'entity' AND entity_id = $1"#,
    )
    .bind(entity_id_str)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to check existing bookmark: {}", e)))?;

    if existing.is_some() {
        return Err(Error::InvalidInput(
            "This entity is already bookmarked".into(),
        ));
    }

    // Get next sort order
    let next_order: i32 =
        sqlx::query_scalar(r#"SELECT COALESCE(MAX(sort_order), -1) + 1 FROM app_bookmarks"#)
            .fetch_one(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to get sort order: {}", e)))?;

    let id = crate::ids::generate_id(crate::ids::BOOKMARK_PREFIX, &[entity_id_str]);
    let bookmark = sqlx::query_as::<_, Bookmark>(
        r#"
        INSERT INTO app_bookmarks (id, bookmark_type, entity_type, entity_id, entity_slug, label, icon, sort_order)
        VALUES ($1, 'entity', $2, $3, $4, $5, $6, $7)
        RETURNING id, bookmark_type, route, tab_type, label, icon,
                  entity_type, entity_id, entity_slug, sort_order, created_at, updated_at
        "#,
    )
    .bind(&id)
    .bind(&req.entity_type)
    .bind(entity_id_str)
    .bind(&req.entity_slug)
    .bind(&req.label)
    .bind(&req.icon)
    .bind(next_order)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create bookmark: {}", e)))?;

    Ok(bookmark)
}

/// Delete a bookmark by ID
pub async fn delete_bookmark(pool: &SqlitePool, id: String) -> Result<()> {
    let id_str = id;
    let result = sqlx::query(r#"DELETE FROM app_bookmarks WHERE id = $1"#)
        .bind(&id_str)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to delete bookmark: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Bookmark not found: {}", id_str)));
    }

    Ok(())
}

/// Delete bookmark by route (for tab bookmarks)
pub async fn delete_bookmark_by_route(pool: &SqlitePool, route: &str) -> Result<()> {
    let result =
        sqlx::query(r#"DELETE FROM app_bookmarks WHERE bookmark_type = 'tab' AND route = $1"#)
            .bind(route)
            .execute(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to delete bookmark: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!(
            "Bookmark not found for route: {}",
            route
        )));
    }

    Ok(())
}

/// Delete bookmark by entity ID (for entity bookmarks)
pub async fn delete_bookmark_by_entity(pool: &SqlitePool, entity_id: String) -> Result<()> {
    let entity_id_str = entity_id;
    let result = sqlx::query(
        r#"DELETE FROM app_bookmarks WHERE bookmark_type = 'entity' AND entity_id = $1"#,
    )
    .bind(&entity_id_str)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to delete bookmark: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!(
            "Bookmark not found for entity: {}",
            entity_id_str
        )));
    }

    Ok(())
}

/// Check if a route is bookmarked
pub async fn is_route_bookmarked(pool: &SqlitePool, route: &str) -> Result<BookmarkStatus> {
    let bookmark_id = sqlx::query_scalar::<_, String>(
        r#"SELECT id FROM app_bookmarks WHERE bookmark_type = 'tab' AND route = $1"#,
    )
    .bind(route)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to check bookmark status: {}", e)))?;

    Ok(BookmarkStatus {
        is_bookmarked: bookmark_id.is_some(),
        bookmark_id,
    })
}

/// Check if an entity is bookmarked
pub async fn is_entity_bookmarked(pool: &SqlitePool, entity_id: String) -> Result<BookmarkStatus> {
    let entity_id_str = entity_id;
    let bookmark_id = sqlx::query_scalar::<_, String>(
        r#"SELECT id FROM app_bookmarks WHERE bookmark_type = 'entity' AND entity_id = $1"#,
    )
    .bind(&entity_id_str)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to check bookmark status: {}", e)))?;

    Ok(BookmarkStatus {
        is_bookmarked: bookmark_id.is_some(),
        bookmark_id,
    })
}

/// Toggle bookmark for a route (create or delete)
pub async fn toggle_route_bookmark(
    pool: &SqlitePool,
    req: CreateTabBookmarkRequest,
) -> Result<ToggleBookmarkResponse> {
    // Check if already bookmarked
    let status = is_route_bookmarked(pool, &req.route).await?;

    if status.is_bookmarked {
        // Delete existing bookmark
        delete_bookmark_by_route(pool, &req.route).await?;
        Ok(ToggleBookmarkResponse {
            bookmarked: false,
            bookmark: None,
        })
    } else {
        // Create new bookmark
        let bookmark = create_tab_bookmark(pool, req).await?;
        Ok(ToggleBookmarkResponse {
            bookmarked: true,
            bookmark: Some(bookmark),
        })
    }
}

/// Toggle bookmark for an entity (create or delete)
pub async fn toggle_entity_bookmark(
    pool: &SqlitePool,
    req: CreateEntityBookmarkRequest,
) -> Result<ToggleBookmarkResponse> {
    // Check if already bookmarked
    let status = is_entity_bookmarked(pool, req.entity_id.clone()).await?;

    if status.is_bookmarked {
        // Delete existing bookmark
        delete_bookmark_by_entity(pool, req.entity_id).await?;
        Ok(ToggleBookmarkResponse {
            bookmarked: false,
            bookmark: None,
        })
    } else {
        // Create new bookmark
        let bookmark = create_entity_bookmark(pool, req).await?;
        Ok(ToggleBookmarkResponse {
            bookmarked: true,
            bookmark: Some(bookmark),
        })
    }
}

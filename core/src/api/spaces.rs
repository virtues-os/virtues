//! Spaces API
//!
//! This module provides CRUD operations for spaces - the swipeable
//! contexts that organize content (like Arc browser spaces).

use crate::error::{Error, Result};
use crate::ids::{generate_id, SPACE_PREFIX};
use crate::types::Timestamp;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

// ============================================================================
// Types
// ============================================================================

/// A space record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Space {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub is_system: bool,
    pub sort_order: i32,
    pub theme_id: String,                    // Required - CSS theme name
    pub accent_color: Option<String>,
    pub active_tab_state_json: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Summary of a space (for list views)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SpaceSummary {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub is_system: bool,
    pub sort_order: i32,
    pub theme_id: String,                    // Required - CSS theme name
    pub accent_color: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Request to create a space
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSpaceRequest {
    pub name: String,
    pub icon: Option<String>,
    pub theme_id: Option<String>,            // Defaults to 'pemberley' if not provided
    pub accent_color: Option<String>,
}

/// Request to update a space
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSpaceRequest {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub sort_order: Option<i32>,
    pub theme_id: Option<String>,
    pub accent_color: Option<String>,
}

/// Request to save tab state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveTabStateRequest {
    pub active_tab_state_json: String,
}

/// List response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceListResponse {
    pub spaces: Vec<SpaceSummary>,
}

// ============================================================================
// CRUD Operations
// ============================================================================

/// List all spaces ordered by sort_order
pub async fn list_spaces(pool: &SqlitePool) -> Result<SpaceListResponse> {
    let spaces = sqlx::query_as::<_, SpaceSummary>(
        r#"
        SELECT id, name, icon, is_system, sort_order, theme_id, accent_color,
               created_at, updated_at
        FROM spaces
        ORDER BY sort_order ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list spaces: {}", e)))?;

    Ok(SpaceListResponse { spaces })
}

/// Get a single space by ID
pub async fn get_space(pool: &SqlitePool, id: &str) -> Result<Space> {
    let space = sqlx::query_as::<_, Space>(
        r#"
        SELECT id, name, icon, is_system, sort_order,
               theme_id, accent_color, active_tab_state_json,
               created_at, updated_at
        FROM spaces
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get space: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Space not found: {}", id)))?;

    Ok(space)
}

/// Default theme for new spaces
const DEFAULT_THEME_ID: &str = "pemberley";

/// Create a new space
pub async fn create_space(pool: &SqlitePool, req: CreateSpaceRequest) -> Result<Space> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(Error::InvalidInput("Space name cannot be empty".into()));
    }

    // Generate ID
    let timestamp = chrono::Utc::now().to_rfc3339();
    let id = generate_id(SPACE_PREFIX, &[name, &timestamp]);

    // Get next sort_order
    let max_sort_order: Option<i32> = sqlx::query_scalar(
        r#"SELECT MAX(sort_order) FROM spaces"#
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get max sort_order: {}", e)))?;

    let sort_order = max_sort_order.unwrap_or(0) + 1;

    // Use provided theme_id or default
    let theme_id = req.theme_id.as_deref().unwrap_or(DEFAULT_THEME_ID);

    let space = sqlx::query_as::<_, Space>(
        r#"
        INSERT INTO spaces (id, name, icon, is_system, sort_order, theme_id, accent_color)
        VALUES ($1, $2, $3, FALSE, $4, $5, $6)
        RETURNING id, name, icon, is_system, sort_order,
                  theme_id, accent_color, active_tab_state_json,
                  created_at, updated_at
        "#,
    )
    .bind(&id)
    .bind(name)
    .bind(&req.icon)
    .bind(sort_order)
    .bind(theme_id)
    .bind(&req.accent_color)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create space: {}", e)))?;

    Ok(space)
}

/// Update an existing space
pub async fn update_space(pool: &SqlitePool, id: &str, req: UpdateSpaceRequest) -> Result<Space> {
    // Verify space exists and check if it's system
    let existing = get_space(pool, id).await?;

    // System spaces can only update certain fields
    if existing.is_system {
        if req.name.is_some() {
            return Err(Error::InvalidInput("Cannot rename system space".into()));
        }
    }

    let name = req.name.as_deref().unwrap_or(&existing.name);
    let icon = req.icon.as_ref().or(existing.icon.as_ref());
    let sort_order = req.sort_order.unwrap_or(existing.sort_order);
    let theme_id = req.theme_id.as_deref().unwrap_or(&existing.theme_id);
    let accent_color = req.accent_color.as_ref().or(existing.accent_color.as_ref());

    if name.trim().is_empty() {
        return Err(Error::InvalidInput("Space name cannot be empty".into()));
    }

    let space = sqlx::query_as::<_, Space>(
        r#"
        UPDATE spaces
        SET name = $2, icon = $3, sort_order = $4, theme_id = $5, accent_color = $6
        WHERE id = $1
        RETURNING id, name, icon, is_system, sort_order,
                  theme_id, accent_color, active_tab_state_json,
                  created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(name.trim())
    .bind(icon)
    .bind(sort_order)
    .bind(theme_id)
    .bind(accent_color)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update space: {}", e)))?;

    Ok(space)
}

/// Save tab state for a space
pub async fn save_tab_state(pool: &SqlitePool, id: &str, req: SaveTabStateRequest) -> Result<()> {
    // Verify space exists
    let _ = get_space(pool, id).await?;

    sqlx::query(
        r#"
        UPDATE spaces
        SET active_tab_state_json = $2
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(&req.active_tab_state_json)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to save tab state: {}", e)))?;

    Ok(())
}

/// Delete a space by ID
pub async fn delete_space(pool: &SqlitePool, id: &str) -> Result<()> {
    // Verify space exists and check if it's system
    let existing = get_space(pool, id).await?;

    if existing.is_system {
        return Err(Error::InvalidInput("Cannot delete system space".into()));
    }

    let result = sqlx::query(r#"DELETE FROM spaces WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to delete space: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Space not found: {}", id)));
    }

    Ok(())
}

/// Touch a space's updated_at timestamp to reflect activity.
/// Call this when items are added/removed or views are created/deleted.
pub async fn touch_space(pool: &SqlitePool, space_id: &str) -> Result<()> {
    sqlx::query(r#"UPDATE spaces SET updated_at = datetime('now') WHERE id = $1"#)
        .bind(space_id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to touch space: {}", e)))?;
    Ok(())
}

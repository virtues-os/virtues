//! Workspace API (formerly multi-space)
//!
//! After collapsing the multi-space carousel, a single system workspace
//! remains (`space_system`). This module retains get/update for theming
//! and identity. Create/delete/list/tab-state are removed.

use crate::error::{Error, Result};
use crate::types::Timestamp;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

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
    pub theme_id: String,
    pub accent_color: Option<String>,
    pub vectorize: bool,
    pub active_tab_state_json: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Summary of a space (for list views — kept for backwards-compat in web client)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SpaceSummary {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub is_system: bool,
    pub sort_order: i32,
    pub theme_id: String,
    pub accent_color: Option<String>,
    pub vectorize: bool,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Request to create a space (kept for type compat — endpoint removed)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSpaceRequest {
    pub name: String,
    pub icon: Option<String>,
    pub theme_id: Option<String>,
    pub accent_color: Option<String>,
}

/// Request to update the workspace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSpaceRequest {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub sort_order: Option<i32>,
    pub theme_id: Option<String>,
    pub accent_color: Option<String>,
    pub vectorize: Option<bool>,
}

/// Request to save tab state (kept for type compat — endpoint removed)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveTabStateRequest {
    pub active_tab_state_json: String,
}

/// List response (kept for backwards compat — returns single system space)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceListResponse {
    pub spaces: Vec<SpaceSummary>,
}

// ============================================================================
// Operations (kept: get, list, update — removed: create, delete, save_tab_state, touch)
// ============================================================================

/// List all spaces (now returns only the single system workspace)
pub async fn list_spaces(pool: &PgPool) -> Result<SpaceListResponse> {
    let spaces = sqlx::query_as::<_, SpaceSummary>(
        r#"
        SELECT id, name, icon, is_system, sort_order, theme_id, accent_color,
               vectorize, created_at, updated_at
        FROM app_spaces
        ORDER BY sort_order ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list spaces: {}", e)))?;

    Ok(SpaceListResponse { spaces })
}

/// Get a single space by ID
pub async fn get_space(pool: &PgPool, id: &str) -> Result<Space> {
    let space = sqlx::query_as::<_, Space>(
        r#"
        SELECT id, name, icon, is_system, sort_order,
               theme_id, accent_color, vectorize, active_tab_state_json,
               created_at, updated_at
        FROM app_spaces
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

/// Touch a space's updated_at timestamp to reflect activity.
pub async fn touch_space(pool: &PgPool, space_id: &str) -> Result<()> {
    sqlx::query(r#"UPDATE app_spaces SET updated_at = now() WHERE id = $1"#)
        .bind(space_id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to touch space: {}", e)))?;
    Ok(())
}

/// Update the workspace (theming, icon, etc.)
pub async fn update_space(pool: &PgPool, id: &str, req: UpdateSpaceRequest) -> Result<Space> {
    let existing = get_space(pool, id).await?;

    let name = req.name.as_deref().unwrap_or(&existing.name);
    let icon = req.icon.as_ref().or(existing.icon.as_ref());
    let sort_order = req.sort_order.unwrap_or(existing.sort_order);
    let theme_id = req.theme_id.as_deref().unwrap_or(&existing.theme_id);
    let accent_color = req.accent_color.as_ref().or(existing.accent_color.as_ref());
    let vectorize = req.vectorize.unwrap_or(existing.vectorize);

    let space = sqlx::query_as::<_, Space>(
        r#"
        UPDATE app_spaces
        SET name = $2, icon = $3, sort_order = $4, theme_id = $5, accent_color = $6, vectorize = $7
        WHERE id = $1
        RETURNING id, name, icon, is_system, sort_order,
                  theme_id, accent_color, vectorize, active_tab_state_json,
                  created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(name.trim())
    .bind(icon)
    .bind(sort_order)
    .bind(theme_id)
    .bind(accent_color)
    .bind(vectorize)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update space: {}", e)))?;

    Ok(space)
}

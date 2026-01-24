//! Workspaces API
//!
//! This module provides CRUD operations for workspaces - the swipeable
//! contexts that organize content (like Arc browser spaces).

use crate::error::{Error, Result};
use crate::ids::{generate_id, WORKSPACE_PREFIX};
use crate::types::Timestamp;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

// ============================================================================
// Types
// ============================================================================

/// A workspace record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub is_system: bool,
    pub is_locked: bool,
    pub sort_order: i32,
    pub accent_color: Option<String>,
    pub theme_mode: Option<String>,
    pub active_tab_state_json: Option<String>,
    pub expanded_nodes_json: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Summary of a workspace (for list views)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkspaceSummary {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub is_system: bool,
    pub is_locked: bool,
    pub sort_order: i32,
    pub accent_color: Option<String>,
    pub theme_mode: Option<String>,
}

/// Request to create a workspace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub icon: Option<String>,
    pub accent_color: Option<String>,
    pub theme_mode: Option<String>,
}

/// Request to update a workspace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateWorkspaceRequest {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub sort_order: Option<i32>,
    pub accent_color: Option<String>,
    pub theme_mode: Option<String>,
    pub expanded_nodes_json: Option<String>,
}

/// Request to save tab state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveTabStateRequest {
    pub active_tab_state_json: String,
}

/// List response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceListResponse {
    pub workspaces: Vec<WorkspaceSummary>,
}

// ============================================================================
// CRUD Operations
// ============================================================================

/// List all workspaces ordered by sort_order
pub async fn list_workspaces(pool: &SqlitePool) -> Result<WorkspaceListResponse> {
    let workspaces = sqlx::query_as::<_, WorkspaceSummary>(
        r#"
        SELECT id, name, icon, is_system, is_locked, sort_order, accent_color, theme_mode
        FROM workspaces
        ORDER BY sort_order ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list workspaces: {}", e)))?;

    Ok(WorkspaceListResponse { workspaces })
}

/// Get a single workspace by ID
pub async fn get_workspace(pool: &SqlitePool, id: &str) -> Result<Workspace> {
    let workspace = sqlx::query_as::<_, Workspace>(
        r#"
        SELECT id, name, icon, is_system, is_locked, sort_order, 
               accent_color, theme_mode, active_tab_state_json, expanded_nodes_json,
               created_at, updated_at
        FROM workspaces
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get workspace: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Workspace not found: {}", id)))?;

    Ok(workspace)
}

/// Create a new workspace
pub async fn create_workspace(pool: &SqlitePool, req: CreateWorkspaceRequest) -> Result<Workspace> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(Error::InvalidInput("Workspace name cannot be empty".into()));
    }

    // Generate ID
    let timestamp = chrono::Utc::now().to_rfc3339();
    let id = generate_id(WORKSPACE_PREFIX, &[name, &timestamp]);

    // Get next sort_order
    let max_sort_order: Option<i32> = sqlx::query_scalar(
        r#"SELECT MAX(sort_order) FROM workspaces"#
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get max sort_order: {}", e)))?;
    
    let sort_order = max_sort_order.unwrap_or(0) + 1;

    let workspace = sqlx::query_as::<_, Workspace>(
        r#"
        INSERT INTO workspaces (id, name, icon, is_system, is_locked, sort_order, accent_color, theme_mode)
        VALUES ($1, $2, $3, FALSE, FALSE, $4, $5, $6)
        RETURNING id, name, icon, is_system, is_locked, sort_order, 
                  accent_color, theme_mode, active_tab_state_json, expanded_nodes_json,
                  created_at, updated_at
        "#,
    )
    .bind(&id)
    .bind(name)
    .bind(&req.icon)
    .bind(sort_order)
    .bind(&req.accent_color)
    .bind(&req.theme_mode)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create workspace: {}", e)))?;

    Ok(workspace)
}

/// Update an existing workspace
pub async fn update_workspace(pool: &SqlitePool, id: &str, req: UpdateWorkspaceRequest) -> Result<Workspace> {
    // Verify workspace exists and check if it's system
    let existing = get_workspace(pool, id).await?;
    
    // System workspaces can only update certain fields
    if existing.is_system {
        if req.name.is_some() {
            return Err(Error::InvalidInput("Cannot rename system workspace".into()));
        }
    }

    let name = req.name.as_deref().unwrap_or(&existing.name);
    let icon = req.icon.as_ref().or(existing.icon.as_ref());
    let sort_order = req.sort_order.unwrap_or(existing.sort_order);
    let accent_color = req.accent_color.as_ref().or(existing.accent_color.as_ref());
    let theme_mode = req.theme_mode.as_ref().or(existing.theme_mode.as_ref());
    let expanded_nodes_json = req.expanded_nodes_json.as_ref().or(existing.expanded_nodes_json.as_ref());

    if name.trim().is_empty() {
        return Err(Error::InvalidInput("Workspace name cannot be empty".into()));
    }

    let workspace = sqlx::query_as::<_, Workspace>(
        r#"
        UPDATE workspaces
        SET name = $2, icon = $3, sort_order = $4, accent_color = $5, 
            theme_mode = $6, expanded_nodes_json = $7
        WHERE id = $1
        RETURNING id, name, icon, is_system, is_locked, sort_order, 
                  accent_color, theme_mode, active_tab_state_json, expanded_nodes_json,
                  created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(name.trim())
    .bind(icon)
    .bind(sort_order)
    .bind(accent_color)
    .bind(theme_mode)
    .bind(expanded_nodes_json)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update workspace: {}", e)))?;

    Ok(workspace)
}

/// Save tab state for a workspace
pub async fn save_tab_state(pool: &SqlitePool, id: &str, req: SaveTabStateRequest) -> Result<()> {
    // Verify workspace exists
    let _ = get_workspace(pool, id).await?;

    sqlx::query(
        r#"
        UPDATE workspaces
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

/// Delete a workspace by ID
pub async fn delete_workspace(pool: &SqlitePool, id: &str) -> Result<()> {
    // Verify workspace exists and check if it's system
    let existing = get_workspace(pool, id).await?;
    
    if existing.is_system {
        return Err(Error::InvalidInput("Cannot delete system workspace".into()));
    }

    let result = sqlx::query(r#"DELETE FROM workspaces WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to delete workspace: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Workspace not found: {}", id)));
    }

    Ok(())
}

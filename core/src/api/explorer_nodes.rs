//! Explorer Nodes API
//!
//! This module provides CRUD operations for explorer nodes - the unified
//! hierarchy system that organizes all content types (pages, chats, wiki, etc.)
//!
//! Node types:
//! - folder: Manual container - you drag items into it
//! - view: Smart folder - auto-populates from a query
//! - shortcut: Link to a specific entity

use crate::error::{Error, Result};
use crate::ids::{generate_id, EXPLORER_NODE_PREFIX};
use crate::types::Timestamp;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

// ============================================================================
// Types
// ============================================================================

/// An explorer node record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ExplorerNode {
    pub id: String,
    pub workspace_id: String,
    pub parent_id: Option<String>,
    pub sort_order: i32,
    pub node_type: String,
    pub name: Option<String>,
    pub icon: Option<String>,
    pub entity_id: Option<String>,
    pub view_config_json: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A tree node with children (for tree responses)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub id: String,
    pub workspace_id: String,
    pub parent_id: Option<String>,
    pub sort_order: i32,
    pub node_type: String,
    pub name: Option<String>,
    pub icon: Option<String>,
    pub entity_id: Option<String>,
    pub view_config_json: Option<String>,
    pub children: Vec<TreeNode>,
}

/// Tree response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceTreeResponse {
    pub workspace_id: String,
    pub nodes: Vec<TreeNode>,
}

/// View configuration for smart folders
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewConfig {
    #[serde(rename = "type")]
    pub view_type: String,  // pages, chats, wiki, drive, sources
    pub subtype: Option<String>,  // For wiki: people, places, orgs, things, days
    pub folder_id: Option<String>,  // For drive: specific folder
    #[serde(default = "default_true")]
    pub workspace_scoped: bool,  // Filter by workspace_id
    pub limit: Option<i64>,
    pub show_more_link: Option<bool>,
}

fn default_true() -> bool { true }

/// Entity returned from view resolution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewEntity {
    pub id: String,
    pub name: String,
    pub entity_type: String,
    pub icon: String,
    pub updated_at: Option<String>,
}

/// View resolution response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewResolutionResponse {
    pub entities: Vec<ViewEntity>,
    pub total: i64,
    pub has_more: bool,
}

/// Request to create a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNodeRequest {
    pub workspace_id: String,
    pub parent_id: Option<String>,
    pub node_type: String,  // folder, view, shortcut
    pub name: Option<String>,
    pub icon: Option<String>,
    pub entity_id: Option<String>,  // For shortcuts
    pub view_config_json: Option<String>,  // For views
}

/// Request to update a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNodeRequest {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub parent_id: Option<Option<String>>,
    pub sort_order: Option<i32>,
    pub view_config_json: Option<String>,
}

/// Request to move nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveNodesRequest {
    pub node_ids: Vec<String>,
    pub target_parent_id: Option<String>,
    pub target_sort_order: i32,
}

/// Request to resolve a view
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveViewRequest {
    pub config: ViewConfig,
    pub workspace_id: String,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// ============================================================================
// CRUD Operations
// ============================================================================

/// Get a single node by ID
pub async fn get_node(pool: &SqlitePool, id: &str) -> Result<ExplorerNode> {
    let node = sqlx::query_as::<_, ExplorerNode>(
        r#"
        SELECT id, workspace_id, parent_id, sort_order, node_type, name, icon,
               entity_id, view_config_json, created_at, updated_at
        FROM explorer_nodes
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get node: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Node not found: {}", id)))?;

    Ok(node)
}

/// Get the tree for a workspace
pub async fn get_workspace_tree(pool: &SqlitePool, workspace_id: &str) -> Result<WorkspaceTreeResponse> {
    // Fetch all nodes for this workspace
    let nodes = sqlx::query_as::<_, ExplorerNode>(
        r#"
        SELECT id, workspace_id, parent_id, sort_order, node_type, name, icon,
               entity_id, view_config_json, created_at, updated_at
        FROM explorer_nodes
        WHERE workspace_id = $1
        ORDER BY sort_order ASC
        "#,
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get workspace tree: {}", e)))?;

    // Build the tree structure
    use std::collections::HashMap;
    
    let mut node_map: HashMap<String, TreeNode> = nodes
        .into_iter()
        .map(|n| {
            (n.id.clone(), TreeNode {
                id: n.id,
                workspace_id: n.workspace_id,
                parent_id: n.parent_id,
                sort_order: n.sort_order,
                node_type: n.node_type,
                name: n.name,
                icon: n.icon,
                entity_id: n.entity_id,
                view_config_json: n.view_config_json,
                children: Vec::new(),
            })
        })
        .collect();

    // Collect parent-child relationships
    let mut children_map: HashMap<Option<String>, Vec<String>> = HashMap::new();
    for (id, node) in &node_map {
        children_map.entry(node.parent_id.clone()).or_default().push(id.clone());
    }

    // Build tree recursively
    fn build_subtree(
        node_id: &str,
        node_map: &mut HashMap<String, TreeNode>,
        children_map: &HashMap<Option<String>, Vec<String>>,
    ) -> Option<TreeNode> {
        let mut node = node_map.remove(node_id)?;
        
        if let Some(child_ids) = children_map.get(&Some(node_id.to_string())) {
            let mut children: Vec<TreeNode> = child_ids
                .iter()
                .filter_map(|id| build_subtree(id, node_map, children_map))
                .collect();
            children.sort_by_key(|c| c.sort_order);
            node.children = children;
        }
        
        Some(node)
    }

    // Get root nodes (parent_id is NULL)
    let root_ids = children_map.get(&None).cloned().unwrap_or_default();
    let mut root_nodes: Vec<TreeNode> = root_ids
        .iter()
        .filter_map(|id| build_subtree(id, &mut node_map, &children_map))
        .collect();
    root_nodes.sort_by_key(|n| n.sort_order);

    Ok(WorkspaceTreeResponse {
        workspace_id: workspace_id.to_string(),
        nodes: root_nodes,
    })
}

/// Create a new node
pub async fn create_node(pool: &SqlitePool, req: CreateNodeRequest) -> Result<ExplorerNode> {
    // Validate node_type
    if !["folder", "view", "shortcut"].contains(&req.node_type.as_str()) {
        return Err(Error::InvalidInput(format!("Invalid node_type: {}", req.node_type)));
    }

    // Validate based on node_type
    match req.node_type.as_str() {
        "folder" => {
            if req.name.as_ref().map(|n| n.trim().is_empty()).unwrap_or(true) {
                return Err(Error::InvalidInput("Folder name is required".into()));
            }
        }
        "view" => {
            if req.name.as_ref().map(|n| n.trim().is_empty()).unwrap_or(true) {
                return Err(Error::InvalidInput("View name is required".into()));
            }
            if req.view_config_json.is_none() {
                return Err(Error::InvalidInput("View config is required".into()));
            }
        }
        "shortcut" => {
            if req.entity_id.is_none() {
                return Err(Error::InvalidInput("Entity ID is required for shortcuts".into()));
            }
        }
        _ => {}
    }

    // Check if workspace is locked
    let workspace = crate::api::workspaces::get_workspace(pool, &req.workspace_id).await?;
    if workspace.is_locked {
        return Err(Error::InvalidInput("Cannot modify locked workspace".into()));
    }

    // Generate ID
    let timestamp = chrono::Utc::now().to_rfc3339();
    let unique_component = req.name.as_deref().unwrap_or(&req.entity_id.as_deref().unwrap_or("node"));
    let id = generate_id(EXPLORER_NODE_PREFIX, &[&req.workspace_id, unique_component, &timestamp]);

    // Get next sort_order for the parent
    let max_sort_order: Option<i32> = if let Some(ref parent_id) = req.parent_id {
        sqlx::query_scalar(
            r#"SELECT MAX(sort_order) FROM explorer_nodes WHERE workspace_id = $1 AND parent_id = $2"#
        )
        .bind(&req.workspace_id)
        .bind(parent_id)
        .fetch_one(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get max sort_order: {}", e)))?
    } else {
        sqlx::query_scalar(
            r#"SELECT MAX(sort_order) FROM explorer_nodes WHERE workspace_id = $1 AND parent_id IS NULL"#
        )
        .bind(&req.workspace_id)
        .fetch_one(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get max sort_order: {}", e)))?
    };
    
    let sort_order = max_sort_order.unwrap_or(0) + 1000;  // Gap-based ordering

    let node = sqlx::query_as::<_, ExplorerNode>(
        r#"
        INSERT INTO explorer_nodes (id, workspace_id, parent_id, sort_order, node_type, name, icon, entity_id, view_config_json)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id, workspace_id, parent_id, sort_order, node_type, name, icon,
                  entity_id, view_config_json, created_at, updated_at
        "#,
    )
    .bind(&id)
    .bind(&req.workspace_id)
    .bind(&req.parent_id)
    .bind(sort_order)
    .bind(&req.node_type)
    .bind(req.name.as_ref().map(|n| n.trim()))
    .bind(&req.icon)
    .bind(&req.entity_id)
    .bind(&req.view_config_json)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create node: {}", e)))?;

    Ok(node)
}

/// Update an existing node
pub async fn update_node(pool: &SqlitePool, id: &str, req: UpdateNodeRequest) -> Result<ExplorerNode> {
    let existing = get_node(pool, id).await?;

    // Check if workspace is locked
    let workspace = crate::api::workspaces::get_workspace(pool, &existing.workspace_id).await?;
    if workspace.is_locked {
        return Err(Error::InvalidInput("Cannot modify locked workspace".into()));
    }

    let name = req.name.as_deref().or(existing.name.as_deref());
    let icon = req.icon.as_ref().or(existing.icon.as_ref());
    let parent_id = match &req.parent_id {
        Some(val) => val.clone(),
        None => existing.parent_id,
    };
    let sort_order = req.sort_order.unwrap_or(existing.sort_order);
    let view_config_json = req.view_config_json.as_ref().or(existing.view_config_json.as_ref());

    let node = sqlx::query_as::<_, ExplorerNode>(
        r#"
        UPDATE explorer_nodes
        SET name = $2, icon = $3, parent_id = $4, sort_order = $5, view_config_json = $6
        WHERE id = $1
        RETURNING id, workspace_id, parent_id, sort_order, node_type, name, icon,
                  entity_id, view_config_json, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(name)
    .bind(icon)
    .bind(parent_id)
    .bind(sort_order)
    .bind(view_config_json)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update node: {}", e)))?;

    Ok(node)
}

/// Delete a node by ID
pub async fn delete_node(pool: &SqlitePool, id: &str) -> Result<()> {
    let existing = get_node(pool, id).await?;

    // Check if workspace is locked
    let workspace = crate::api::workspaces::get_workspace(pool, &existing.workspace_id).await?;
    if workspace.is_locked {
        return Err(Error::InvalidInput("Cannot modify locked workspace".into()));
    }

    let result = sqlx::query(r#"DELETE FROM explorer_nodes WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to delete node: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Node not found: {}", id)));
    }

    Ok(())
}

/// Move nodes to a new parent
pub async fn move_nodes(pool: &SqlitePool, req: MoveNodesRequest) -> Result<()> {
    if req.node_ids.is_empty() {
        return Ok(());
    }

    // Check first node to get workspace and verify it's not locked
    let first_node = get_node(pool, &req.node_ids[0]).await?;
    let workspace = crate::api::workspaces::get_workspace(pool, &first_node.workspace_id).await?;
    if workspace.is_locked {
        return Err(Error::InvalidInput("Cannot modify locked workspace".into()));
    }

    // TODO: Add circular reference check for folders

    let mut sort_order = req.target_sort_order;
    for node_id in &req.node_ids {
        sqlx::query(
            r#"
            UPDATE explorer_nodes
            SET parent_id = $2, sort_order = $3
            WHERE id = $1
            "#,
        )
        .bind(node_id)
        .bind(&req.target_parent_id)
        .bind(sort_order)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to move node: {}", e)))?;
        
        sort_order += 1000;  // Gap-based ordering
    }

    Ok(())
}

// ============================================================================
// View Resolution
// ============================================================================

/// Resolve a view configuration to actual entities
pub async fn resolve_view(pool: &SqlitePool, req: ResolveViewRequest) -> Result<ViewResolutionResponse> {
    let limit = req.limit.unwrap_or(50).min(100);
    let offset = req.offset.unwrap_or(0);

    let (entities, total) = match req.config.view_type.as_str() {
        "pages" => resolve_pages_view(pool, &req.config, &req.workspace_id, limit, offset).await?,
        "chats" => resolve_chats_view(pool, &req.config, &req.workspace_id, limit, offset).await?,
        "wiki" => resolve_wiki_view(pool, &req.config, limit, offset).await?,
        "drive" => resolve_drive_view(pool, &req.config, limit, offset).await?,
        "sources" => resolve_sources_view(pool, limit, offset).await?,
        _ => return Err(Error::InvalidInput(format!("Unknown view type: {}", req.config.view_type))),
    };

    Ok(ViewResolutionResponse {
        entities,
        total,
        has_more: offset + limit < total,
    })
}

async fn resolve_pages_view(
    pool: &SqlitePool,
    config: &ViewConfig,
    workspace_id: &str,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ViewEntity>, i64)> {
    if config.workspace_scoped {
        let total: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM pages WHERE workspace_id = $1"#
        )
        .bind(workspace_id)
        .fetch_one(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to count pages: {}", e)))?;

        let entities: Vec<ViewEntity> = sqlx::query_as::<_, (String, String, String)>(
            r#"
            SELECT id, title, updated_at
            FROM pages
            WHERE workspace_id = $1
            ORDER BY updated_at DESC
            LIMIT $2 OFFSET $3
            "#
        )
        .bind(workspace_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to fetch pages: {}", e)))?
        .into_iter()
        .map(|(id, title, updated_at)| ViewEntity {
            id,
            name: title,
            entity_type: "page".to_string(),
            icon: "ri:file-text-line".to_string(),
            updated_at: Some(updated_at),
        })
        .collect();

        Ok((entities, total))
    } else {
        let total: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM pages"#)
            .fetch_one(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to count pages: {}", e)))?;

        let entities: Vec<ViewEntity> = sqlx::query_as::<_, (String, String, String)>(
            r#"
            SELECT id, title, updated_at
            FROM pages
            ORDER BY updated_at DESC
            LIMIT $1 OFFSET $2
            "#
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to fetch pages: {}", e)))?
        .into_iter()
        .map(|(id, title, updated_at)| ViewEntity {
            id,
            name: title,
            entity_type: "page".to_string(),
            icon: "ri:file-text-line".to_string(),
            updated_at: Some(updated_at),
        })
        .collect();

        Ok((entities, total))
    }
}

async fn resolve_chats_view(
    pool: &SqlitePool,
    config: &ViewConfig,
    workspace_id: &str,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ViewEntity>, i64)> {
    if config.workspace_scoped {
        let total: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM app_chat_sessions WHERE workspace_id = $1"#
        )
        .bind(workspace_id)
        .fetch_one(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to count chats: {}", e)))?;

        let entities: Vec<ViewEntity> = sqlx::query_as::<_, (String, String, String)>(
            r#"
            SELECT id, title, updated_at
            FROM app_chat_sessions
            WHERE workspace_id = $1
            ORDER BY updated_at DESC
            LIMIT $2 OFFSET $3
            "#
        )
        .bind(workspace_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to fetch chats: {}", e)))?
        .into_iter()
        .map(|(id, title, updated_at)| ViewEntity {
            id: format!("chat_{}", id), // Prefix for route detection
            name: title,
            entity_type: "chat".to_string(),
            icon: "ri:chat-1-line".to_string(),
            updated_at: Some(updated_at),
        })
        .collect();

        Ok((entities, total))
    } else {
        let total: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM app_chat_sessions"#)
            .fetch_one(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to count chats: {}", e)))?;

        let entities: Vec<ViewEntity> = sqlx::query_as::<_, (String, String, String)>(
            r#"
            SELECT id, title, updated_at
            FROM app_chat_sessions
            ORDER BY updated_at DESC
            LIMIT $1 OFFSET $2
            "#
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to fetch chats: {}", e)))?
        .into_iter()
        .map(|(id, title, updated_at)| ViewEntity {
            id: format!("chat_{}", id), // Prefix for route detection
            name: title,
            entity_type: "chat".to_string(),
            icon: "ri:chat-1-line".to_string(),
            updated_at: Some(updated_at),
        })
        .collect();

        Ok((entities, total))
    }
}

async fn resolve_wiki_view(
    pool: &SqlitePool,
    config: &ViewConfig,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ViewEntity>, i64)> {
    let subtype = config.subtype.as_deref().unwrap_or("people");
    
    match subtype {
        "people" => {
            let total: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM wiki_people"#)
                .fetch_one(pool)
                .await
                .map_err(|e| Error::Database(format!("Failed to count people: {}", e)))?;

            let entities: Vec<ViewEntity> = sqlx::query_as::<_, (String, String)>(
                r#"
                SELECT id, canonical_name
                FROM wiki_people
                ORDER BY canonical_name ASC
                LIMIT $1 OFFSET $2
                "#
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to fetch people: {}", e)))?
            .into_iter()
            .map(|(id, name)| ViewEntity {
                id,
                name,
                entity_type: "wiki_person".to_string(),
                icon: "ri:user-line".to_string(),
                updated_at: None,
            })
            .collect();

            Ok((entities, total))
        }
        "places" => {
            let total: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM wiki_places"#)
                .fetch_one(pool)
                .await
                .map_err(|e| Error::Database(format!("Failed to count places: {}", e)))?;

            let entities: Vec<ViewEntity> = sqlx::query_as::<_, (String, String)>(
                r#"
                SELECT id, name
                FROM wiki_places
                ORDER BY name ASC
                LIMIT $1 OFFSET $2
                "#
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to fetch places: {}", e)))?
            .into_iter()
            .map(|(id, name)| ViewEntity {
                id,
                name,
                entity_type: "wiki_place".to_string(),
                icon: "ri:map-pin-line".to_string(),
                updated_at: None,
            })
            .collect();

            Ok((entities, total))
        }
        "orgs" => {
            let total: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM wiki_orgs"#)
                .fetch_one(pool)
                .await
                .map_err(|e| Error::Database(format!("Failed to count orgs: {}", e)))?;

            let entities: Vec<ViewEntity> = sqlx::query_as::<_, (String, String)>(
                r#"
                SELECT id, canonical_name
                FROM wiki_orgs
                ORDER BY canonical_name ASC
                LIMIT $1 OFFSET $2
                "#
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to fetch orgs: {}", e)))?
            .into_iter()
            .map(|(id, name)| ViewEntity {
                id,
                name,
                entity_type: "wiki_org".to_string(),
                icon: "ri:building-line".to_string(),
                updated_at: None,
            })
            .collect();

            Ok((entities, total))
        }
        "things" => {
            let total: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM wiki_things"#)
                .fetch_one(pool)
                .await
                .map_err(|e| Error::Database(format!("Failed to count things: {}", e)))?;

            let entities: Vec<ViewEntity> = sqlx::query_as::<_, (String, String)>(
                r#"
                SELECT id, canonical_name
                FROM wiki_things
                ORDER BY canonical_name ASC
                LIMIT $1 OFFSET $2
                "#
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to fetch things: {}", e)))?
            .into_iter()
            .map(|(id, name)| ViewEntity {
                id,
                name,
                entity_type: "wiki_thing".to_string(),
                icon: "ri:box-3-line".to_string(),
                updated_at: None,
            })
            .collect();

            Ok((entities, total))
        }
        "days" => {
            let total: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM wiki_days"#)
                .fetch_one(pool)
                .await
                .map_err(|e| Error::Database(format!("Failed to count days: {}", e)))?;

            let entities: Vec<ViewEntity> = sqlx::query_as::<_, (String, String)>(
                r#"
                SELECT id, date
                FROM wiki_days
                ORDER BY date DESC
                LIMIT $1 OFFSET $2
                "#
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to fetch days: {}", e)))?
            .into_iter()
            .map(|(id, date)| ViewEntity {
                id,
                name: date,
                entity_type: "wiki_day".to_string(),
                icon: "ri:calendar-line".to_string(),
                updated_at: None,
            })
            .collect();

            Ok((entities, total))
        }
        _ => Err(Error::InvalidInput(format!("Unknown wiki subtype: {}", subtype))),
    }
}

async fn resolve_drive_view(
    pool: &SqlitePool,
    config: &ViewConfig,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ViewEntity>, i64)> {
    let parent_id = config.folder_id.as_deref();
    
    let (total, entities) = if let Some(folder_id) = parent_id {
        let total: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM drive_files WHERE parent_id = $1 AND deleted_at IS NULL"#
        )
        .bind(folder_id)
        .fetch_one(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to count files: {}", e)))?;

        let entities: Vec<ViewEntity> = sqlx::query_as::<_, (String, String, bool)>(
            r#"
            SELECT id, filename, is_folder
            FROM drive_files
            WHERE parent_id = $1 AND deleted_at IS NULL
            ORDER BY is_folder DESC, filename ASC
            LIMIT $2 OFFSET $3
            "#
        )
        .bind(folder_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to fetch files: {}", e)))?
        .into_iter()
        .map(|(id, filename, is_folder)| ViewEntity {
            id,
            name: filename,
            entity_type: if is_folder { "drive_folder".to_string() } else { "drive_file".to_string() },
            icon: if is_folder { "ri:folder-line".to_string() } else { "ri:file-line".to_string() },
            updated_at: None,
        })
        .collect();

        (total, entities)
    } else {
        // Root level
        let total: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM drive_files WHERE parent_id IS NULL AND deleted_at IS NULL"#
        )
        .fetch_one(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to count files: {}", e)))?;

        let entities: Vec<ViewEntity> = sqlx::query_as::<_, (String, String, bool)>(
            r#"
            SELECT id, filename, is_folder
            FROM drive_files
            WHERE parent_id IS NULL AND deleted_at IS NULL
            ORDER BY is_folder DESC, filename ASC
            LIMIT $1 OFFSET $2
            "#
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to fetch files: {}", e)))?
        .into_iter()
        .map(|(id, filename, is_folder)| ViewEntity {
            id,
            name: filename,
            entity_type: if is_folder { "drive_folder".to_string() } else { "drive_file".to_string() },
            icon: if is_folder { "ri:folder-line".to_string() } else { "ri:file-line".to_string() },
            updated_at: None,
        })
        .collect();

        (total, entities)
    };

    Ok((entities, total))
}

async fn resolve_sources_view(
    pool: &SqlitePool,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ViewEntity>, i64)> {
    let total: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM elt_source_connections"#)
        .fetch_one(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to count sources: {}", e)))?;

    let entities: Vec<ViewEntity> = sqlx::query_as::<_, (String, String)>(
        r#"
        SELECT id, name
        FROM elt_source_connections
        ORDER BY name ASC
        LIMIT $1 OFFSET $2
        "#
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch sources: {}", e)))?
    .into_iter()
    .map(|(id, name)| ViewEntity {
        id,
        name,
        entity_type: "source_connection".to_string(),
        icon: "ri:database-2-line".to_string(),
        updated_at: None,
    })
    .collect();

    Ok((entities, total))
}

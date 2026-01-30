//! Views API
//!
//! This module provides CRUD operations for views - collections of entities
//! that can be either manual (user-curated playlist) or smart (query-based).
//!
//! Views replace the old explorer_nodes hierarchy with a flat, simple model.

use crate::api::spaces::touch_space;
use crate::error::{Error, Result};
use crate::ids::generate_id;
use crate::types::Timestamp;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

// ============================================================================
// Constants
// ============================================================================

pub const VIEW_PREFIX: &str = "view";

// ============================================================================
// Types
// ============================================================================

/// A view record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct View {
    pub id: String,
    pub space_id: String,
    pub parent_view_id: Option<String>, // For shallow nesting (depth=1)
    pub name: String,
    pub icon: Option<String>,
    pub sort_order: i32,
    pub view_type: String,            // 'manual' or 'smart'
    pub query_config: Option<String>, // JSON query config (smart views only)
    pub is_system: bool,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A view item record (URL-native storage for manual views or space root)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ViewItem {
    pub id: i64,
    pub view_id: Option<String>,      // Set if item belongs to a view/folder
    pub space_id: Option<String>, // Set if item is at space root level
    pub url: String,                  // URL-native: "/person/abc" or "https://arxiv.org"
    pub sort_order: i32,
    pub created_at: Timestamp,
}

/// Summary for list views
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ViewSummary {
    pub id: String,
    pub space_id: String,
    pub parent_view_id: Option<String>,
    pub name: String,
    pub icon: Option<String>,
    pub sort_order: i32,
    pub view_type: String,
    pub is_system: bool,
}

/// Smart view query configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryConfig {
    pub namespace: String,            // person, page, chat, etc.
    pub filters: Option<serde_json::Value>, // Optional filters
    pub sort: Option<String>,         // Sort field
    pub sort_dir: Option<String>,     // asc or desc
    pub limit: Option<i64>,
    pub static_prefix: Option<Vec<String>>, // Static URLs to prepend before dynamic results
}

/// Entity returned from view resolution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewEntity {
    pub id: String,
    pub name: String,
    pub namespace: String,            // person, page, chat, etc.
    pub icon: String,
    pub updated_at: Option<String>,
}

/// View resolution response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewResolutionResponse {
    pub view: ViewSummary,
    pub entities: Vec<ViewEntity>,
    pub total: i64,
    pub has_more: bool,
}

/// Request to create a view
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateViewRequest {
    pub space_id: String,
    pub parent_view_id: Option<String>, // For nesting (depth=1 enforced)
    pub name: String,
    pub icon: Option<String>,
    pub view_type: String,            // 'manual' or 'smart'
    pub query_config: Option<QueryConfig>, // For smart views only
}

/// Request to update a view
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateViewRequest {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub sort_order: Option<i32>,
    pub query_config: Option<QueryConfig>,
}

/// Request to add an item to a manual view
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddViewItemRequest {
    pub url: String,  // URL-native: "/person/abc" or "https://arxiv.org"
}

/// List response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewListResponse {
    pub views: Vec<ViewSummary>,
}

// ============================================================================
// CRUD Operations
// ============================================================================

/// List views for a space
pub async fn list_views(pool: &SqlitePool, space_id: &str) -> Result<ViewListResponse> {
    let views = sqlx::query_as::<_, ViewSummary>(
        r#"
        SELECT id, space_id, parent_view_id, name, icon, sort_order, view_type, is_system
        FROM views
        WHERE space_id = $1
        ORDER BY sort_order ASC
        "#,
    )
    .bind(space_id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list views: {}", e)))?;

    Ok(ViewListResponse { views })
}

/// Get a single view by ID
pub async fn get_view(pool: &SqlitePool, id: &str) -> Result<View> {
    let view = sqlx::query_as::<_, View>(
        r#"
        SELECT id, space_id, parent_view_id, name, icon, sort_order, view_type,
               query_config, is_system, created_at, updated_at
        FROM views
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get view: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("View not found: {}", id)))?;

    Ok(view)
}

/// Create a new view
pub async fn create_view(pool: &SqlitePool, req: CreateViewRequest) -> Result<View> {
    // Validate view_type
    if !["manual", "smart"].contains(&req.view_type.as_str()) {
        return Err(Error::InvalidInput(format!(
            "Invalid view_type: {}",
            req.view_type
        )));
    }

    // Validate based on view_type
    match req.view_type.as_str() {
        "manual" => {
            // Manual views don't require initial items
        }
        "smart" => {
            if req.query_config.is_none() {
                return Err(Error::InvalidInput(
                    "query_config is required for smart views".into(),
                ));
            }
        }
        _ => {}
    }

    if req.name.trim().is_empty() {
        return Err(Error::InvalidInput("View name cannot be empty".into()));
    }

    // Validate parent_view_id for shallow nesting (depth=1)
    if let Some(parent_id) = &req.parent_view_id {
        let parent = get_view(pool, parent_id).await?;
        // Depth=1: parent cannot itself have a parent
        if parent.parent_view_id.is_some() {
            return Err(Error::InvalidInput(
                "Cannot nest views more than 1 level deep".into(),
            ));
        }
    }

    // Generate ID
    let timestamp = chrono::Utc::now().to_rfc3339();
    let id = generate_id(VIEW_PREFIX, &[&req.space_id, &req.name, &timestamp]);

    // Get next sort_order
    let max_sort_order: Option<i32> =
        sqlx::query_scalar(r#"SELECT MAX(sort_order) FROM views WHERE space_id = $1"#)
            .bind(&req.space_id)
            .fetch_one(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to get max sort_order: {}", e)))?;

    let sort_order = max_sort_order.unwrap_or(0) + 1000;

    // Serialize query_config for smart views
    let query_config_json = req
        .query_config
        .as_ref()
        .map(|config| serde_json::to_string(config))
        .transpose()
        .map_err(|e| Error::InvalidInput(format!("Failed to serialize query_config: {}", e)))?;

    let view = sqlx::query_as::<_, View>(
        r#"
        INSERT INTO views (id, space_id, parent_view_id, name, icon, sort_order, view_type, query_config, is_system)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, FALSE)
        RETURNING id, space_id, parent_view_id, name, icon, sort_order, view_type,
                  query_config, is_system, created_at, updated_at
        "#,
    )
    .bind(&id)
    .bind(&req.space_id)
    .bind(&req.parent_view_id)
    .bind(req.name.trim())
    .bind(&req.icon)
    .bind(sort_order)
    .bind(&req.view_type)
    .bind(&query_config_json)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create view: {}", e)))?;

    // Touch the space to reflect activity
    touch_space(pool, &req.space_id).await?;

    Ok(view)
}

/// Update an existing view
pub async fn update_view(pool: &SqlitePool, id: &str, req: UpdateViewRequest) -> Result<View> {
    let existing = get_view(pool, id).await?;

    // System views cannot be edited
    if existing.is_system {
        return Err(Error::InvalidInput("Cannot modify system view".into()));
    }

    let name = req.name.as_deref().unwrap_or(&existing.name);
    let icon = req.icon.as_ref().or(existing.icon.as_ref());
    let sort_order = req.sort_order.unwrap_or(existing.sort_order);

    // Handle query_config update (smart views only)
    let query_config_json = if let Some(config) = &req.query_config {
        Some(
            serde_json::to_string(config)
                .map_err(|e| Error::InvalidInput(format!("Failed to serialize query_config: {}", e)))?,
        )
    } else {
        existing.query_config.clone()
    };

    if name.trim().is_empty() {
        return Err(Error::InvalidInput("View name cannot be empty".into()));
    }

    let view = sqlx::query_as::<_, View>(
        r#"
        UPDATE views
        SET name = $2, icon = $3, sort_order = $4, query_config = $5
        WHERE id = $1
        RETURNING id, space_id, parent_view_id, name, icon, sort_order, view_type,
                  query_config, is_system, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(name.trim())
    .bind(icon)
    .bind(sort_order)
    .bind(&query_config_json)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update view: {}", e)))?;

    Ok(view)
}

/// Delete a view by ID
pub async fn delete_view(pool: &SqlitePool, id: &str) -> Result<()> {
    let existing = get_view(pool, id).await?;

    // System views cannot be deleted
    if existing.is_system {
        return Err(Error::InvalidInput("Cannot delete system view".into()));
    }

    let result = sqlx::query(r#"DELETE FROM views WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to delete view: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("View not found: {}", id)));
    }

    // Touch the space to reflect activity
    touch_space(pool, &existing.space_id).await?;

    Ok(())
}

// ============================================================================
// Manual View Item Management (URL-native via space_items table)
// ============================================================================

/// Add a URL to a manual view
pub async fn add_item_to_view(pool: &SqlitePool, view_id: &str, url: &str) -> Result<ViewItem> {
    let existing = get_view(pool, view_id).await?;

    if existing.view_type != "manual" {
        return Err(Error::InvalidInput(
            "Can only add items to manual views".into(),
        ));
    }

    if existing.is_system {
        return Err(Error::InvalidInput("Cannot modify system view".into()));
    }

    // Get next sort_order for this view
    let max_sort_order: Option<i32> =
        sqlx::query_scalar(r#"SELECT MAX(sort_order) FROM space_items WHERE view_id = $1"#)
            .bind(view_id)
            .fetch_one(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to get max sort_order: {}", e)))?;

    let sort_order = max_sort_order.unwrap_or(0) + 1;

    // Insert into space_items (UNIQUE constraint prevents duplicates)
    let item = sqlx::query_as::<_, ViewItem>(
        r#"
        INSERT INTO space_items (view_id, space_id, url, sort_order)
        VALUES ($1, NULL, $2, $3)
        ON CONFLICT(view_id, url) WHERE view_id IS NOT NULL DO UPDATE SET sort_order = sort_order
        RETURNING id, view_id, space_id, url, sort_order, created_at
        "#,
    )
    .bind(view_id)
    .bind(url)
    .bind(sort_order)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to add item to view: {}", e)))?;

    // Touch the space to reflect activity
    touch_space(pool, &existing.space_id).await?;

    Ok(item)
}

/// Remove a URL from a manual view
pub async fn remove_item_from_view(
    pool: &SqlitePool,
    view_id: &str,
    url: &str,
) -> Result<()> {
    let existing = get_view(pool, view_id).await?;

    if existing.view_type != "manual" {
        return Err(Error::InvalidInput(
            "Can only remove items from manual views".into(),
        ));
    }

    if existing.is_system {
        return Err(Error::InvalidInput("Cannot modify system view".into()));
    }

    let result = sqlx::query(r#"DELETE FROM space_items WHERE view_id = $1 AND url = $2"#)
        .bind(view_id)
        .bind(url)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to remove item from view: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!(
            "Item not found in view: {}",
            url
        )));
    }

    // Touch the space to reflect activity
    touch_space(pool, &existing.space_id).await?;

    Ok(())
}

/// List all items in a manual view
pub async fn list_view_items(pool: &SqlitePool, view_id: &str) -> Result<Vec<ViewItem>> {
    let items = sqlx::query_as::<_, ViewItem>(
        r#"
        SELECT id, view_id, space_id, url, sort_order, created_at
        FROM space_items
        WHERE view_id = $1
        ORDER BY sort_order ASC
        "#,
    )
    .bind(view_id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list view items: {}", e)))?;

    Ok(items)
}

/// Reorder items in a manual view
pub async fn reorder_view_items(
    pool: &SqlitePool,
    view_id: &str,
    url_order: Vec<String>,
) -> Result<()> {
    let existing = get_view(pool, view_id).await?;

    if existing.is_system {
        return Err(Error::InvalidInput("Cannot modify system view".into()));
    }

    // Update sort_order for each item
    for (index, url) in url_order.iter().enumerate() {
        sqlx::query(r#"UPDATE space_items SET sort_order = $3 WHERE view_id = $1 AND url = $2"#)
            .bind(view_id)
            .bind(url)
            .bind(index as i32)
            .execute(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to reorder items: {}", e)))?;
    }

    Ok(())
}

// ============================================================================
// Space Root Items (items not inside any folder)
// ============================================================================

/// List items at space root level (not in any folder)
pub async fn list_space_items(pool: &SqlitePool, space_id: &str) -> Result<Vec<ViewItem>> {
    let items = sqlx::query_as::<_, ViewItem>(
        r#"
        SELECT id, view_id, space_id, url, sort_order, created_at
        FROM space_items
        WHERE space_id = $1
        ORDER BY sort_order ASC
        "#,
    )
    .bind(space_id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list space items: {}", e)))?;

    Ok(items)
}

/// Resolve space items to entities (with full metadata)
pub async fn resolve_space_items(
    pool: &SqlitePool,
    space_id: &str,
) -> Result<Vec<ViewEntity>> {
    // Get URLs for this space
    let urls: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT url FROM space_items
        WHERE space_id = $1
        ORDER BY sort_order ASC
        "#,
    )
    .bind(space_id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch space items: {}", e)))?;

    // Resolve each URL to its entity details
    let mut entities = Vec::new();
    for url in &urls {
        if let Some(entity) = resolve_url(pool, url).await? {
            entities.push(entity);
        }
    }

    Ok(entities)
}

/// Add a URL to space root level
pub async fn add_space_item(
    pool: &SqlitePool,
    space_id: &str,
    url: &str,
) -> Result<ViewItem> {
    // Get next sort_order for this space
    let max_sort_order: Option<i32> =
        sqlx::query_scalar(r#"SELECT MAX(sort_order) FROM space_items WHERE space_id = $1"#)
            .bind(space_id)
            .fetch_one(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to get max sort_order: {}", e)))?;

    let sort_order = max_sort_order.unwrap_or(0) + 1;

    // Insert into space_items (UNIQUE constraint prevents duplicates)
    let item = sqlx::query_as::<_, ViewItem>(
        r#"
        INSERT INTO space_items (view_id, space_id, url, sort_order)
        VALUES (NULL, $1, $2, $3)
        ON CONFLICT(space_id, url) WHERE space_id IS NOT NULL DO UPDATE SET sort_order = sort_order
        RETURNING id, view_id, space_id, url, sort_order, created_at
        "#,
    )
    .bind(space_id)
    .bind(url)
    .bind(sort_order)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to add space item: {}", e)))?;

    // Touch the space to reflect activity
    touch_space(pool, space_id).await?;

    Ok(item)
}

/// Remove a URL from space root level
pub async fn remove_space_item(
    pool: &SqlitePool,
    space_id: &str,
    url: &str,
) -> Result<()> {
    let result = sqlx::query(r#"DELETE FROM space_items WHERE space_id = $1 AND url = $2"#)
        .bind(space_id)
        .bind(url)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to remove space item: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!(
            "Item not found in space: {}",
            url
        )));
    }

    // Touch the space to reflect activity
    touch_space(pool, space_id).await?;

    Ok(())
}

/// Reorder items at space root level
pub async fn reorder_space_items(
    pool: &SqlitePool,
    space_id: &str,
    url_order: Vec<String>,
) -> Result<()> {
    // Update sort_order for each item
    for (index, url) in url_order.iter().enumerate() {
        sqlx::query(r#"UPDATE space_items SET sort_order = $3 WHERE space_id = $1 AND url = $2"#)
            .bind(space_id)
            .bind(url)
            .bind(index as i32)
            .execute(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to reorder space items: {}", e)))?;
    }

    Ok(())
}

/// Remove all space_items entries for a given URL (orphan cleanup)
/// Called when an entity is deleted to clean up all references
pub async fn remove_items_by_url(pool: &SqlitePool, url: &str) -> Result<i64> {
    let result = sqlx::query(r#"DELETE FROM space_items WHERE url = $1"#)
        .bind(url)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to remove items by URL: {}", e)))?;

    Ok(result.rows_affected() as i64)
}

// ============================================================================
// View Resolution
// ============================================================================

/// Resolve a view to its entities
pub async fn resolve_view(
    pool: &SqlitePool,
    view_id: &str,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<ViewResolutionResponse> {
    let view = get_view(pool, view_id).await?;
    let limit = limit.unwrap_or(50).min(100);
    let offset = offset.unwrap_or(0);

    let (entities, total) = match view.view_type.as_str() {
        "manual" => resolve_manual_view(pool, &view, limit, offset).await?,
        "smart" => resolve_smart_view(pool, &view, limit, offset).await?,
        _ => {
            return Err(Error::InvalidInput(format!(
                "Unknown view_type: {}",
                view.view_type
            )))
        }
    };

    Ok(ViewResolutionResponse {
        view: ViewSummary {
            id: view.id,
            space_id: view.space_id,
            parent_view_id: view.parent_view_id,
            name: view.name,
            icon: view.icon,
            sort_order: view.sort_order,
            view_type: view.view_type,
            is_system: view.is_system,
        },
        entities,
        total,
        has_more: offset + limit < total,
    })
}

async fn resolve_manual_view(
    pool: &SqlitePool,
    view: &View,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ViewEntity>, i64)> {
    // Get total count
    let total: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM space_items WHERE view_id = $1"#)
        .bind(&view.id)
        .fetch_one(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to count view items: {}", e)))?;

    // Get paginated URLs
    let urls: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT url FROM space_items
        WHERE view_id = $1
        ORDER BY sort_order ASC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(&view.id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch view items: {}", e)))?;

    // Resolve each URL to its entity details
    let mut entities = Vec::new();
    for url in &urls {
        if let Some(entity) = resolve_url(pool, url).await? {
            entities.push(entity);
        }
    }

    Ok((entities, total))
}

async fn resolve_smart_view(
    pool: &SqlitePool,
    view: &View,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ViewEntity>, i64)> {
    let config: QueryConfig = view
        .query_config
        .as_ref()
        .map(|json| serde_json::from_str(json))
        .transpose()
        .map_err(|e| Error::InvalidInput(format!("Invalid query_config: {}", e)))?
        .ok_or_else(|| Error::InvalidInput("Smart view missing query_config".into()))?;

    // Resolve static prefix items first (if any)
    let mut prefix_entities = Vec::new();
    if let Some(static_urls) = &config.static_prefix {
        for url in static_urls {
            if let Some(entity) = resolve_url(pool, url).await? {
                prefix_entities.push(entity);
            }
        }
    }

    // Route to namespace-specific resolver
    let (mut entities, total) = match config.namespace.as_str() {
        "chat" => resolve_chats(pool, &config, limit, offset).await?,
        "page" => resolve_pages(pool, &config, limit, offset).await?,
        "person" => resolve_people(pool, &config, limit, offset).await?,
        "place" => resolve_places(pool, &config, limit, offset).await?,
        "org" => resolve_orgs(pool, &config, limit, offset).await?,
        "thing" => resolve_things(pool, &config, limit, offset).await?,
        "day" => resolve_days(pool, &config, limit, offset).await?,
        "year" => resolve_years(pool, &config, limit, offset).await?,
        "source" => resolve_sources(pool, &config, limit, offset).await?,
        "drive" => resolve_drive(pool, &config, limit, offset).await?,
        _ => {
            return Err(Error::InvalidInput(format!(
                "Unknown namespace: {}",
                config.namespace
            )))
        }
    };

    // Prepend static prefix entities
    if !prefix_entities.is_empty() {
        prefix_entities.append(&mut entities);
        entities = prefix_entities;
    }

    Ok((entities, total + config.static_prefix.as_ref().map_or(0, |v| v.len() as i64)))
}

// ============================================================================
// URL Resolution (URL-native approach)
// ============================================================================

/// Resolve a URL to its entity details
/// URLs can be:
/// - Internal paths: /person/person_abc, /page/page_xyz, /wiki, /virtues/sitemap
/// - External links: https://arxiv.org, http://example.com
async fn resolve_url(pool: &SqlitePool, url: &str) -> Result<Option<ViewEntity>> {
    // External links
    if url.starts_with("http://") || url.starts_with("https://") {
        return Ok(Some(ViewEntity {
            id: url.to_string(),
            name: extract_domain(url),
            namespace: "external".to_string(),
            icon: "ri:external-link-line".to_string(),
            updated_at: None,
        }));
    }

    // Internal paths - extract namespace from URL
    let parts: Vec<&str> = url.trim_start_matches('/').split('/').collect();
    let namespace = parts.first().unwrap_or(&"");

    // If no ID part, treat as app route (list page)
    let id = parts.get(1).map(|s| *s).unwrap_or("");
    if id.is_empty() {
        // No ID = list page, use app route resolution
        return Ok(Some(resolve_app_route(url)));
    }

    match *namespace {
        // Entity namespaces (SQLite-backed with IDs)
        "chat" => resolve_chat_by_id(pool, id).await,
        "page" => resolve_page_by_id(pool, id).await,
        "person" => resolve_person_by_id(pool, id).await,
        "place" => resolve_place_by_id(pool, id).await,
        "org" => resolve_org_by_id(pool, id).await,
        "thing" => resolve_thing_by_id(pool, id).await,
        "day" => resolve_day_by_id(pool, id).await,
        "year" => resolve_year_by_id(pool, id).await,
        "source" => resolve_source_by_id(pool, id).await,
        "view" => resolve_view_by_id(pool, id).await,
        // App routes (frontend-rendered, no backend lookup)
        "wiki" | "drive" | "virtues" | "" => Ok(Some(resolve_app_route(url))),
        // Unknown namespace
        _ => Ok(None),
    }
}

/// Extract domain from URL for display name
fn extract_domain(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(url)
        .to_string()
}

/// Resolve an app route to a ViewEntity
fn resolve_app_route(url: &str) -> ViewEntity {
    let (name, icon) = match url {
        "/" => ("New Chat", "ri:add-line"),
        "/chat" => ("All Chats", "ri:chat-history-line"),
        "/page" => ("All Pages", "ri:file-list-3-line"),
        "/wiki" => ("Overview", "ri:compass-line"),
        "/day" => ("Today", "ri:calendar-todo-line"),
        "/person" => ("People", "ri:user-line"),
        "/place" => ("Places", "ri:map-pin-line"),
        "/org" => ("Organizations", "ri:building-line"),
        "/thing" => ("Things", "ri:box-3-line"),
        "/source" => ("Sources", "ri:plug-line"),
        "/drive" => ("Drive", "ri:folder-line"),
        "/virtues/sql" => ("SQL Viewer", "ri:database-line"),
        "/virtues/terminal" => ("Terminal", "ri:terminal-box-line"),
        "/virtues/sitemap" => ("Sitemap", "ri:road-map-line"),
        _ => {
            // Derive name from URL path
            let name = url.split('/').last().unwrap_or("Page");
            (name, "ri:file-line")
        }
    };

    ViewEntity {
        id: url.to_string(),
        name: name.to_string(),
        namespace: "app".to_string(),
        icon: icon.to_string(),
        updated_at: None,
    }
}

// ============================================================================
// Namespace-specific resolvers
// ============================================================================

async fn resolve_chats(
    pool: &SqlitePool,
    config: &QueryConfig,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ViewEntity>, i64)> {
    let total: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM chats"#)
        .fetch_one(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to count chats: {}", e)))?;

    let entities: Vec<ViewEntity> = sqlx::query_as::<_, (String, String, String)>(
        r#"
        SELECT id, title, updated_at
        FROM chats
        ORDER BY updated_at DESC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(config.limit.unwrap_or(limit).min(limit))
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch chats: {}", e)))?
    .into_iter()
    .map(|(id, title, updated_at)| ViewEntity {
        id: format!("/chat/{}", id),
        name: title,
        namespace: "chat".to_string(),
        icon: "ri:chat-1-line".to_string(),
        updated_at: Some(updated_at),
    })
    .collect();

    Ok((entities, total))
}

async fn resolve_pages(
    pool: &SqlitePool,
    config: &QueryConfig,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ViewEntity>, i64)> {
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
        "#,
    )
    .bind(config.limit.unwrap_or(limit).min(limit))
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch pages: {}", e)))?
    .into_iter()
    .map(|(id, title, updated_at)| ViewEntity {
        id: format!("/page/{}", id),
        name: title,
        namespace: "page".to_string(),
        icon: "ri:file-text-line".to_string(),
        updated_at: Some(updated_at),
    })
    .collect();

    Ok((entities, total))
}

async fn resolve_people(
    pool: &SqlitePool,
    config: &QueryConfig,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ViewEntity>, i64)> {
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
        "#,
    )
    .bind(config.limit.unwrap_or(limit).min(limit))
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch people: {}", e)))?
    .into_iter()
    .map(|(id, name)| ViewEntity {
        id: format!("/person/{}", id),
        name,
        namespace: "person".to_string(),
        icon: "ri:user-line".to_string(),
        updated_at: None,
    })
    .collect();

    Ok((entities, total))
}

async fn resolve_places(
    pool: &SqlitePool,
    config: &QueryConfig,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ViewEntity>, i64)> {
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
        "#,
    )
    .bind(config.limit.unwrap_or(limit).min(limit))
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch places: {}", e)))?
    .into_iter()
    .map(|(id, name)| ViewEntity {
        id: format!("/place/{}", id),
        name,
        namespace: "place".to_string(),
        icon: "ri:map-pin-line".to_string(),
        updated_at: None,
    })
    .collect();

    Ok((entities, total))
}

async fn resolve_orgs(
    pool: &SqlitePool,
    config: &QueryConfig,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ViewEntity>, i64)> {
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
        "#,
    )
    .bind(config.limit.unwrap_or(limit).min(limit))
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch orgs: {}", e)))?
    .into_iter()
    .map(|(id, name)| ViewEntity {
        id: format!("/org/{}", id),
        name,
        namespace: "org".to_string(),
        icon: "ri:building-line".to_string(),
        updated_at: None,
    })
    .collect();

    Ok((entities, total))
}

async fn resolve_things(
    pool: &SqlitePool,
    config: &QueryConfig,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ViewEntity>, i64)> {
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
        "#,
    )
    .bind(config.limit.unwrap_or(limit).min(limit))
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch things: {}", e)))?
    .into_iter()
    .map(|(id, name)| ViewEntity {
        id: format!("/thing/{}", id),
        name,
        namespace: "thing".to_string(),
        icon: "ri:box-3-line".to_string(),
        updated_at: None,
    })
    .collect();

    Ok((entities, total))
}

async fn resolve_days(
    pool: &SqlitePool,
    config: &QueryConfig,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ViewEntity>, i64)> {
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
        "#,
    )
    .bind(config.limit.unwrap_or(limit).min(limit))
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch days: {}", e)))?
    .into_iter()
    .map(|(id, date)| ViewEntity {
        id: format!("/day/{}", id),
        name: date,
        namespace: "day".to_string(),
        icon: "ri:calendar-line".to_string(),
        updated_at: None,
    })
    .collect();

    Ok((entities, total))
}

async fn resolve_years(
    pool: &SqlitePool,
    config: &QueryConfig,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ViewEntity>, i64)> {
    let total: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM wiki_years"#)
        .fetch_one(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to count years: {}", e)))?;

    let entities: Vec<ViewEntity> = sqlx::query_as::<_, (String, String)>(
        r#"
        SELECT id, year
        FROM wiki_years
        ORDER BY year DESC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(config.limit.unwrap_or(limit).min(limit))
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch years: {}", e)))?
    .into_iter()
    .map(|(id, year)| ViewEntity {
        id: format!("/year/{}", id),
        name: year,
        namespace: "year".to_string(),
        icon: "ri:calendar-line".to_string(),
        updated_at: None,
    })
    .collect();

    Ok((entities, total))
}

async fn resolve_sources(
    pool: &SqlitePool,
    config: &QueryConfig,
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
        "#,
    )
    .bind(config.limit.unwrap_or(limit).min(limit))
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch sources: {}", e)))?
    .into_iter()
    .map(|(id, name)| ViewEntity {
        id: format!("/source/{}", id),
        name,
        namespace: "source".to_string(),
        icon: "ri:database-2-line".to_string(),
        updated_at: None,
    })
    .collect();

    Ok((entities, total))
}

async fn resolve_drive(
    pool: &SqlitePool,
    config: &QueryConfig,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ViewEntity>, i64)> {
    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM drive_files WHERE parent_id IS NULL AND deleted_at IS NULL"#,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to count drive files: {}", e)))?;

    let entities: Vec<ViewEntity> = sqlx::query_as::<_, (String, String, bool)>(
        r#"
        SELECT id, filename, is_folder
        FROM drive_files
        WHERE parent_id IS NULL AND deleted_at IS NULL
        ORDER BY is_folder DESC, filename ASC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(config.limit.unwrap_or(limit).min(limit))
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch drive files: {}", e)))?
    .into_iter()
    .map(|(id, filename, is_folder)| ViewEntity {
        id: format!("/drive/{}", id),
        name: filename,
        namespace: "drive".to_string(),
        icon: if is_folder {
            "ri:folder-line".to_string()
        } else {
            "ri:file-line".to_string()
        },
        updated_at: None,
    })
    .collect();

    Ok((entities, total))
}

// ============================================================================
// Individual entity resolvers (for URL-native view items)
// ============================================================================

async fn resolve_chat_by_id(pool: &SqlitePool, id: &str) -> Result<Option<ViewEntity>> {
    let result = sqlx::query_as::<_, (String, String)>(
        r#"SELECT id, title FROM chats WHERE id = $1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch chat: {}", e)))?;

    Ok(result.map(|(id, title)| ViewEntity {
        id: format!("/chat/{}", id),
        name: title,
        namespace: "chat".to_string(),
        icon: "ri:chat-1-line".to_string(),
        updated_at: None,
    }))
}

async fn resolve_page_by_id(pool: &SqlitePool, id: &str) -> Result<Option<ViewEntity>> {
    let result = sqlx::query_as::<_, (String, String)>(r#"SELECT id, title FROM pages WHERE id = $1"#)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to fetch page: {}", e)))?;

    Ok(result.map(|(id, title)| ViewEntity {
        id: format!("/page/{}", id),
        name: title,
        namespace: "page".to_string(),
        icon: "ri:file-text-line".to_string(),
        updated_at: None,
    }))
}

async fn resolve_person_by_id(pool: &SqlitePool, id: &str) -> Result<Option<ViewEntity>> {
    let result = sqlx::query_as::<_, (String, String)>(
        r#"SELECT id, canonical_name FROM wiki_people WHERE id = $1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch person: {}", e)))?;

    Ok(result.map(|(id, name)| ViewEntity {
        id: format!("/person/{}", id),
        name,
        namespace: "person".to_string(),
        icon: "ri:user-line".to_string(),
        updated_at: None,
    }))
}

async fn resolve_place_by_id(pool: &SqlitePool, id: &str) -> Result<Option<ViewEntity>> {
    let result =
        sqlx::query_as::<_, (String, String)>(r#"SELECT id, name FROM wiki_places WHERE id = $1"#)
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to fetch place: {}", e)))?;

    Ok(result.map(|(id, name)| ViewEntity {
        id: format!("/place/{}", id),
        name,
        namespace: "place".to_string(),
        icon: "ri:map-pin-line".to_string(),
        updated_at: None,
    }))
}

async fn resolve_org_by_id(pool: &SqlitePool, id: &str) -> Result<Option<ViewEntity>> {
    let result = sqlx::query_as::<_, (String, String)>(
        r#"SELECT id, canonical_name FROM wiki_orgs WHERE id = $1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch org: {}", e)))?;

    Ok(result.map(|(id, name)| ViewEntity {
        id: format!("/org/{}", id),
        name,
        namespace: "org".to_string(),
        icon: "ri:building-line".to_string(),
        updated_at: None,
    }))
}

async fn resolve_thing_by_id(pool: &SqlitePool, id: &str) -> Result<Option<ViewEntity>> {
    let result = sqlx::query_as::<_, (String, String)>(
        r#"SELECT id, canonical_name FROM wiki_things WHERE id = $1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch thing: {}", e)))?;

    Ok(result.map(|(id, name)| ViewEntity {
        id: format!("/thing/{}", id),
        name,
        namespace: "thing".to_string(),
        icon: "ri:box-3-line".to_string(),
        updated_at: None,
    }))
}

async fn resolve_day_by_id(pool: &SqlitePool, id: &str) -> Result<Option<ViewEntity>> {
    let result =
        sqlx::query_as::<_, (String, String)>(r#"SELECT id, date FROM wiki_days WHERE id = $1"#)
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to fetch day: {}", e)))?;

    Ok(result.map(|(id, date)| ViewEntity {
        id: format!("/day/{}", id),
        name: date,
        namespace: "day".to_string(),
        icon: "ri:calendar-line".to_string(),
        updated_at: None,
    }))
}

async fn resolve_year_by_id(pool: &SqlitePool, id: &str) -> Result<Option<ViewEntity>> {
    let result =
        sqlx::query_as::<_, (String, String)>(r#"SELECT id, year FROM wiki_years WHERE id = $1"#)
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to fetch year: {}", e)))?;

    Ok(result.map(|(id, year)| ViewEntity {
        id: format!("/year/{}", id),
        name: year,
        namespace: "year".to_string(),
        icon: "ri:calendar-line".to_string(),
        updated_at: None,
    }))
}

async fn resolve_source_by_id(pool: &SqlitePool, id: &str) -> Result<Option<ViewEntity>> {
    let result = sqlx::query_as::<_, (String, String)>(
        r#"SELECT id, name FROM elt_source_connections WHERE id = $1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch source: {}", e)))?;

    Ok(result.map(|(id, name)| ViewEntity {
        id: format!("/source/{}", id),
        name,
        namespace: "source".to_string(),
        icon: "ri:database-2-line".to_string(),
        updated_at: None,
    }))
}

async fn resolve_view_by_id(pool: &SqlitePool, id: &str) -> Result<Option<ViewEntity>> {
    let result = sqlx::query_as::<_, (String, String, String)>(
        r#"SELECT id, name, view_type FROM views WHERE id = $1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch view: {}", e)))?;

    Ok(result.map(|(id, name, view_type)| ViewEntity {
        id: format!("/view/{}", id),
        name,
        namespace: "view".to_string(),
        icon: if view_type == "smart" {
            "ri:filter-line".to_string()
        } else {
            "ri:folder-line".to_string()
        },
        updated_at: None,
    }))
}

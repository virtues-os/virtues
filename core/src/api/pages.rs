//! Pages API
//!
//! This module provides CRUD operations for user-authored pages.
//! Pages are knowledge documents with entity linking support using
//! the format: ((Display Name))[[prefix_hash]]
//!
//! Note: Page organization (folders, hierarchy) is now handled by explorer_nodes.
//! Pages only store workspace_id for view filtering.

use crate::error::{Error, Result};
use crate::ids::{generate_id, PAGE_PREFIX};
use crate::types::Timestamp;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

// ============================================================================
// Types
// ============================================================================

/// A page record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Page {
    pub id: String,
    pub title: String,
    pub content: String,
    pub workspace_id: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Summary of a page (for list views)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PageSummary {
    pub id: String,
    pub title: String,
    pub workspace_id: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Request to create a page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePageRequest {
    pub title: String,
    #[serde(default)]
    pub content: String,
    pub workspace_id: Option<String>,
}

/// Request to update a page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePageRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    pub workspace_id: Option<Option<String>>,
}

/// Paginated list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageListResponse {
    pub pages: Vec<PageSummary>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

/// Entity search result for autocomplete
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EntitySearchResult {
    pub id: String,
    pub name: String,
    pub entity_type: String,
    pub icon: String,
}

/// Entity search response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySearchResponse {
    pub results: Vec<EntitySearchResult>,
}

// ============================================================================
// CRUD Operations
// ============================================================================

/// List pages with pagination, ordered by updated_at descending
pub async fn list_pages(
    pool: &SqlitePool,
    limit: Option<i64>,
    offset: Option<i64>,
    workspace_id: Option<&str>,
) -> Result<PageListResponse> {
    let limit = limit.unwrap_or(50).min(100);
    let offset = offset.unwrap_or(0);

    // Get total count (with optional workspace filter)
    let total: i64 = if let Some(ws_id) = workspace_id {
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM pages WHERE workspace_id = $1"#)
            .bind(ws_id)
            .fetch_one(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to count pages: {}", e)))?
    } else {
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM pages"#)
            .fetch_one(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to count pages: {}", e)))?
    };

    // Get pages (with optional workspace filter)
    let pages = if let Some(ws_id) = workspace_id {
        sqlx::query_as::<_, PageSummary>(
            r#"
            SELECT id, title, workspace_id, created_at, updated_at
            FROM pages
            WHERE workspace_id = $1
            ORDER BY updated_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(ws_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to list pages: {}", e)))?
    } else {
        sqlx::query_as::<_, PageSummary>(
            r#"
            SELECT id, title, workspace_id, created_at, updated_at
            FROM pages
            ORDER BY updated_at DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to list pages: {}", e)))?
    };

    Ok(PageListResponse {
        pages,
        total,
        limit,
        offset,
    })
}

/// Get a single page by ID
pub async fn get_page(pool: &SqlitePool, id: &str) -> Result<Page> {
    let page = sqlx::query_as::<_, Page>(
        r#"
        SELECT id, title, content, workspace_id, created_at, updated_at
        FROM pages
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get page: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Page not found: {}", id)))?;

    Ok(page)
}

/// Create a new page
pub async fn create_page(pool: &SqlitePool, req: CreatePageRequest) -> Result<Page> {
    let title = req.title.trim();
    if title.is_empty() {
        return Err(Error::InvalidInput("Page title cannot be empty".into()));
    }

    // Generate ID using title and current timestamp for uniqueness
    let timestamp = chrono::Utc::now().to_rfc3339();
    let id = generate_id(PAGE_PREFIX, &[title, &timestamp]);

    let page = sqlx::query_as::<_, Page>(
        r#"
        INSERT INTO pages (id, title, content, workspace_id)
        VALUES ($1, $2, $3, $4)
        RETURNING id, title, content, workspace_id, created_at, updated_at
        "#,
    )
    .bind(&id)
    .bind(title)
    .bind(&req.content)
    .bind(&req.workspace_id)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create page: {}", e)))?;

    Ok(page)
}

/// Update an existing page
pub async fn update_page(pool: &SqlitePool, id: &str, req: UpdatePageRequest) -> Result<Page> {
    // Verify page exists
    let existing = get_page(pool, id).await?;

    let title = req.title.as_deref().unwrap_or(&existing.title);
    let content = req.content.as_deref().unwrap_or(&existing.content);
    let workspace_id = match &req.workspace_id {
        Some(val) => val.clone(),
        None => existing.workspace_id,
    };

    if title.trim().is_empty() {
        return Err(Error::InvalidInput("Page title cannot be empty".into()));
    }

    let page = sqlx::query_as::<_, Page>(
        r#"
        UPDATE pages
        SET title = $2, content = $3, workspace_id = $4
        WHERE id = $1
        RETURNING id, title, content, workspace_id, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(title.trim())
    .bind(content)
    .bind(workspace_id)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update page: {}", e)))?;

    Ok(page)
}

/// Delete a page by ID
pub async fn delete_page(pool: &SqlitePool, id: &str) -> Result<()> {
    let result = sqlx::query(r#"DELETE FROM pages WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to delete page: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Page not found: {}", id)));
    }

    Ok(())
}

// ============================================================================
// Entity Search (for [[]] autocomplete)
// ============================================================================

/// Search for entities across wiki_people, wiki_places, wiki_organizations, and pages
/// Used for autocomplete when typing [[ in the editor
pub async fn search_entities(pool: &SqlitePool, query: &str) -> Result<EntitySearchResponse> {
    let query = query.trim();
    let search_pattern = if query.is_empty() {
        "%".to_string()
    } else {
        format!("%{}%", query)
    };
    
    let limit = 10i64;

    // Search across multiple tables with UNION
    let results = sqlx::query_as::<_, EntitySearchResult>(
        r#"
        SELECT id, canonical_name as name, 'person' as entity_type, 'ri:user-line' as icon
        FROM wiki_people
        WHERE canonical_name LIKE $1
        UNION ALL
        SELECT id, name, 'place' as entity_type, 'ri:map-pin-line' as icon
        FROM wiki_places
        WHERE name LIKE $1
        UNION ALL
        SELECT id, canonical_name as name, 'thing' as entity_type, 'ri:box-3-line' as icon
        FROM wiki_things
        WHERE canonical_name LIKE $1
        UNION ALL
        SELECT id, filename as name, 'file' as entity_type, 'ri:file-line' as icon
        FROM drive_files
        WHERE filename LIKE $1 AND deleted_at IS NULL
        UNION ALL
        SELECT id, title as name, 'page' as entity_type, 'ri:file-text-line' as icon
        FROM pages
        WHERE title LIKE $1
        ORDER BY name
        LIMIT $2
        "#,
    )
    .bind(&search_pattern)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to search entities: {}", e)))?;

    Ok(EntitySearchResponse { results })
}

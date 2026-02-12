//! Pages API
//!
//! This module provides CRUD operations for user-authored pages.
//! Pages are knowledge documents with entity linking support using
//! the format: ((Display Name))[[prefix_hash]]
//!
//! Note: Pages don't "belong" to spaces - they're just URL-native entities.
//! Organization is handled by space_items which hold URL references.

use crate::error::{Error, Result};
use crate::ids::{generate_id, PAGE_PREFIX, PAGE_VERSION_PREFIX};
use crate::types::Timestamp;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::SqlitePool;

/// Custom deserializer for Option<Option<T>> that distinguishes between:
/// - Missing field → None (don't change)
/// - Explicit null → Some(None) (clear the value)
/// - A value → Some(Some(value)) (set the value)
fn deserialize_double_option<'de, D, T>(deserializer: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    // This deserializer is only called when the field is present in JSON
    // If the field is missing, serde uses the default (None) due to #[serde(default)]
    // So if we're here, the field was present - deserialize its value
    Ok(Some(Option::deserialize(deserializer)?))
}

// System space ID - pages created here don't get auto-added
const SYSTEM_SPACE_ID: &str = "space_system";

// ============================================================================
// Types
// ============================================================================

/// A page record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Page {
    pub id: String,
    pub title: String,
    pub content: String,
    pub icon: Option<String>,
    pub cover_url: Option<String>,
    pub tags: Option<String>, // JSON array: ["tag1", "tag2"]
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Summary of a page (for list views)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PageSummary {
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
    pub cover_url: Option<String>,
    pub tags: Option<String>, // JSON array: ["tag1", "tag2"]
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Request to create a page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePageRequest {
    pub title: String,
    #[serde(default)]
    pub content: String,
    #[serde(rename = "spaceId")]
    pub space_id: Option<String>,  // For auto-add to space_items (not stored on page)
    pub icon: Option<String>,
    pub cover_url: Option<String>,
    pub tags: Option<String>, // JSON array: ["tag1", "tag2"]
}

/// Request to update a page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePageRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub icon: Option<Option<String>>,      // None = don't change, Some(None) = clear, Some(Some(x)) = set
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub cover_url: Option<Option<String>>, // None = don't change, Some(None) = clear, Some(Some(x)) = set
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub tags: Option<Option<String>>,      // None = don't change, Some(None) = clear, Some(Some(x)) = set
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySearchResult {
    pub id: String,
    pub name: String,
    pub entity_type: String,
    pub icon: String,
    pub url: String,
    pub mime_type: Option<String>,
}

/// Entity search response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySearchResponse {
    pub results: Vec<EntitySearchResult>,
}

// ============================================================================
// Version History Types
// ============================================================================

/// A page version summary (for list views, without snapshot data)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PageVersionSummary {
    pub id: String,
    pub page_id: String,
    pub version_number: i64,
    pub content_preview: Option<String>,
    pub created_at: Timestamp,
    pub created_by: String,
    pub description: Option<String>,
}

/// A page version with snapshot (for restore operations)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageVersionDetail {
    pub id: String,
    pub page_id: String,
    pub version_number: i64,
    pub snapshot: Option<String>, // base64-encoded Yjs snapshot
    pub content_preview: Option<String>,
    pub created_at: Timestamp,
    pub created_by: String,
    pub description: Option<String>,
}

/// Internal struct for database query (snapshot as blob)
#[derive(Debug, Clone, sqlx::FromRow)]
struct PageVersionRow {
    id: String,
    page_id: String,
    version_number: i64,
    yjs_snapshot: Option<Vec<u8>>,
    content_preview: Option<String>,
    created_at: Timestamp,
    created_by: String,
    description: Option<String>,
}

/// Request to create a page version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVersionRequest {
    pub snapshot: String, // base64-encoded Yjs snapshot
    pub content_preview: String,
    pub description: Option<String>,
    pub created_by: String,
}

/// List versions response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageVersionsListResponse {
    pub versions: Vec<PageVersionSummary>,
}

// ============================================================================
// CRUD Operations
// ============================================================================

/// List pages with pagination, ordered by updated_at descending
pub async fn list_pages(
    pool: &SqlitePool,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<PageListResponse> {
    let limit = limit.unwrap_or(50).min(100);
    let offset = offset.unwrap_or(0);

    // Get total count
    let total: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM app_pages"#)
        .fetch_one(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to count pages: {}", e)))?;

    // Get pages
    let pages = sqlx::query_as::<_, PageSummary>(
        r#"
        SELECT id, title, icon, cover_url, tags, created_at, updated_at
        FROM app_pages
        ORDER BY updated_at DESC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list pages: {}", e)))?;

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
        SELECT id, title, content, icon, cover_url, tags, created_at, updated_at
        FROM app_pages
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
/// If space_id is provided and not the system space, auto-adds to space_items
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
        INSERT INTO app_pages (id, title, content, icon, cover_url, tags)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, title, content, icon, cover_url, tags, created_at, updated_at
        "#,
    )
    .bind(&id)
    .bind(title)
    .bind(&req.content)
    .bind(&req.icon)
    .bind(&req.cover_url)
    .bind(&req.tags)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create page: {}", e)))?;

    // Auto-add to space_items if space_id provided and not system space
    if let Some(space_id) = &req.space_id {
        if space_id != SYSTEM_SPACE_ID {
            let url = format!("/page/{}", page.id);
            if let Err(e) = crate::api::views::add_space_item(pool, space_id, &url).await {
                tracing::warn!("Failed to auto-add page to space {}: {}", space_id, e);
                // Don't fail page creation if auto-add fails
            }
        }
    }

    Ok(page)
}

/// Update an existing page
pub async fn update_page(pool: &SqlitePool, id: &str, req: UpdatePageRequest) -> Result<Page> {
    // Verify page exists
    let existing = get_page(pool, id).await?;

    let title = req.title.as_deref().unwrap_or(&existing.title);
    let content = req.content.as_deref().unwrap_or(&existing.content);
    let icon = match &req.icon {
        Some(val) => val.clone(),
        None => existing.icon,
    };
    let cover_url = match &req.cover_url {
        Some(val) => val.clone(),
        None => existing.cover_url,
    };
    let tags = match &req.tags {
        Some(val) => val.clone(),
        None => existing.tags,
    };

    if title.trim().is_empty() {
        return Err(Error::InvalidInput("Page title cannot be empty".into()));
    }

    let page = sqlx::query_as::<_, Page>(
        r#"
        UPDATE app_pages
        SET title = $2, content = $3, icon = $4, cover_url = $5, tags = $6
        WHERE id = $1
        RETURNING id, title, content, icon, cover_url, tags, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(title.trim())
    .bind(content)
    .bind(icon)
    .bind(cover_url)
    .bind(tags)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update page: {}", e)))?;

    Ok(page)
}

/// Delete a page by ID
/// Also cleans up all space_items references (orphan cleanup)
pub async fn delete_page(pool: &SqlitePool, id: &str) -> Result<()> {
    // First delete the page
    let result = sqlx::query(r#"DELETE FROM app_pages WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to delete page: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Page not found: {}", id)));
    }

    // Clean up all space_items references
    let url = format!("/page/{}", id);
    if let Err(e) = crate::api::views::remove_items_by_url(pool, &url).await {
        tracing::warn!("Failed to clean up space_items for page {}: {}", id, e);
        // Don't fail deletion if cleanup fails
    }

    Ok(())
}

// ============================================================================
// Entity Search (for [[]] autocomplete)
// ============================================================================

/// Raw entity search result from database (before URL computation)
#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
struct RawEntitySearchResult {
    id: String,
    name: String,
    entity_type: String,
    icon: String,
    mime_type: Option<String>,
    updated_at: String,
    relevance: i32,
}

/// Compute the canonical URL for an entity based on its type and ID
/// All URLs follow the format: /{type}/{id}
fn get_entity_url(entity_type: &str, id: &str) -> String {
    match entity_type {
        "person" => format!("/person/{}", id),
        "place" => format!("/place/{}", id),
        "org" => format!("/org/{}", id),
        "page" => format!("/page/{}", id),
        "day" => format!("/day/{}", id),
        "year" => format!("/year/{}", id),
        "source" => format!("/source/{}", id),
        "chat" => format!("/chat/{}", id),
        "file" => format!("/drive/{}", id),
        _ => format!("/{}/{}", entity_type, id),
    }
}

/// Search for entities across wiki_people, wiki_places, wiki_organizations, pages, and files
/// Used for autocomplete when typing @ in the editor
/// Returns canonical URLs for each entity (everything is a URL)
///
/// Results are ranked by:
/// 1. Relevance: prefix matches (name starts with query) come before contains matches
/// 2. Recency: within each relevance tier, most recently updated items come first
pub async fn search_entities(pool: &SqlitePool, query: &str) -> Result<EntitySearchResponse> {
    let query = query.trim();

    // For empty query, show most recent items
    let (contains_pattern, prefix_pattern) = if query.is_empty() {
        ("%".to_string(), "%".to_string())
    } else {
        (format!("%{}%", query), format!("{}%", query))
    };

    let limit = 15i64;

    // Search across multiple tables with UNION
    // Relevance: 0 = prefix match (highest), 1 = contains match
    // Note: wiki_things table doesn't exist yet, so we skip it
    let raw_results = sqlx::query_as::<_, RawEntitySearchResult>(
        r#"
        SELECT id, canonical_name as name, 'person' as entity_type, 'ri:user-line' as icon,
               NULL as mime_type, updated_at,
               CASE WHEN canonical_name LIKE $2 THEN 0 ELSE 1 END as relevance
        FROM wiki_people
        WHERE canonical_name LIKE $1
        UNION ALL
        SELECT id, name, 'place' as entity_type, 'ri:map-pin-line' as icon,
               NULL as mime_type, updated_at,
               CASE WHEN name LIKE $2 THEN 0 ELSE 1 END as relevance
        FROM wiki_places
        WHERE name LIKE $1
        UNION ALL
        SELECT id, canonical_name as name, 'org' as entity_type, 'ri:building-line' as icon,
               NULL as mime_type, updated_at,
               CASE WHEN canonical_name LIKE $2 THEN 0 ELSE 1 END as relevance
        FROM wiki_orgs
        WHERE canonical_name LIKE $1
        UNION ALL
        SELECT id, filename as name, 'file' as entity_type, 'ri:file-line' as icon,
               mime_type, updated_at,
               CASE WHEN filename LIKE $2 THEN 0 ELSE 1 END as relevance
        FROM drive_files
        WHERE filename LIKE $1 AND deleted_at IS NULL
        UNION ALL
        SELECT id, title as name, 'page' as entity_type, 'ri:file-text-line' as icon,
               NULL as mime_type, updated_at,
               CASE WHEN title LIKE $2 THEN 0 ELSE 1 END as relevance
        FROM app_pages
        WHERE title LIKE $1
        ORDER BY relevance ASC, updated_at DESC
        LIMIT $3
        "#,
    )
    .bind(&contains_pattern)
    .bind(&prefix_pattern)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to search entities: {}", e)))?;

    // Convert raw results to EntitySearchResult with computed URLs
    let results: Vec<EntitySearchResult> = raw_results
        .into_iter()
        .map(|r| EntitySearchResult {
            url: get_entity_url(&r.entity_type, &r.id),
            id: r.id,
            name: r.name,
            entity_type: r.entity_type,
            icon: r.icon,
            mime_type: r.mime_type,
        })
        .collect();

    Ok(EntitySearchResponse { results })
}

// ============================================================================
// Version History Operations
// ============================================================================

/// Create a new version snapshot for a page
pub async fn create_version(
    pool: &SqlitePool,
    page_id: &str,
    req: CreateVersionRequest,
) -> Result<PageVersionSummary> {
    // Verify page exists
    let _ = get_page(pool, page_id).await?;

    // Decode base64 snapshot
    let snapshot_bytes = BASE64
        .decode(&req.snapshot)
        .map_err(|e| Error::InvalidInput(format!("Invalid base64 snapshot: {}", e)))?;

    // Get next version number
    let max_version: Option<i64> = sqlx::query_scalar(
        "SELECT MAX(version_number) FROM app_page_versions WHERE page_id = ?",
    )
    .bind(page_id)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get max version: {}", e)))?;

    let version_number = max_version.unwrap_or(0) + 1;

    // Generate version ID
    let timestamp = chrono::Utc::now().to_rfc3339();
    let id = generate_id(PAGE_VERSION_PREFIX, &[page_id, &timestamp]);

    // Insert version
    let version = sqlx::query_as::<_, PageVersionSummary>(
        r#"
        INSERT INTO app_page_versions (id, page_id, version_number, yjs_snapshot, content_preview, created_by, description)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, page_id, version_number, content_preview, created_at, created_by, description
        "#,
    )
    .bind(&id)
    .bind(page_id)
    .bind(version_number)
    .bind(&snapshot_bytes)
    .bind(&req.content_preview)
    .bind(&req.created_by)
    .bind(&req.description)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create version: {}", e)))?;

    // Prune old versions beyond the cap (keep most recent 50)
    sqlx::query(
        r#"
        DELETE FROM app_page_versions
        WHERE page_id = $1 AND id NOT IN (
            SELECT id FROM app_page_versions
            WHERE page_id = $1
            ORDER BY version_number DESC
            LIMIT 50
        )
        "#,
    )
    .bind(page_id)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to prune versions: {}", e)))?;

    Ok(version)
}

/// List versions for a page (without snapshot data)
pub async fn list_versions(
    pool: &SqlitePool,
    page_id: &str,
    limit: Option<i64>,
) -> Result<PageVersionsListResponse> {
    let limit = limit.unwrap_or(20).min(100);

    let versions = sqlx::query_as::<_, PageVersionSummary>(
        r#"
        SELECT id, page_id, version_number, content_preview, created_at, created_by, description
        FROM app_page_versions
        WHERE page_id = $1
        ORDER BY version_number DESC
        LIMIT $2
        "#,
    )
    .bind(page_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list versions: {}", e)))?;

    Ok(PageVersionsListResponse { versions })
}

/// Get a single version by ID (includes snapshot for restore)
pub async fn get_version(pool: &SqlitePool, version_id: &str) -> Result<PageVersionDetail> {
    let row = sqlx::query_as::<_, PageVersionRow>(
        r#"
        SELECT id, page_id, version_number, yjs_snapshot, content_preview, created_at, created_by, description
        FROM app_page_versions
        WHERE id = $1
        "#,
    )
    .bind(version_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get version: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Version not found: {}", version_id)))?;

    // Convert blob to base64
    let snapshot = row.yjs_snapshot.map(|bytes| BASE64.encode(&bytes));

    Ok(PageVersionDetail {
        id: row.id,
        page_id: row.page_id,
        version_number: row.version_number,
        snapshot,
        content_preview: row.content_preview,
        created_at: row.created_at,
        created_by: row.created_by,
        description: row.description,
    })
}

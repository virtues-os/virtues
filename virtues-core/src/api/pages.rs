//! Pages API
//!
//! This module provides CRUD operations for user-authored pages.
//! Pages are knowledge documents with entity linking support using
//! the format: ((Display Name))[[prefix_hash]]
//!
//! Note: Pages don't "belong" to notebooks - they're just URL-native entities.
//! Organization is handled by notebook_items which hold URL references.

use crate::error::{Error, Result};
use crate::ids::{generate_id, PAGE_PREFIX, PAGE_SHARE_PREFIX, PAGE_VERSION_PREFIX};
use crate::types::Timestamp;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::PgPool;
use yrs::{updates::decoder::Decode, Doc, GetString, ReadTxn, Transact, Update};

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
    /// `--cat-*` token key ('orange', 'emerald'), never a hex. See migration 0079.
    pub icon_color: Option<String>,
    pub cover_url: Option<String>,
    pub tags: Option<serde_json::Value>, // JSONB array: ["tag1", "tag2"]
    pub date: Option<String>, // YYYY-MM-DD — if set, this page is a reflection for that day
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Summary of a page (for list views)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PageSummary {
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
    pub icon_color: Option<String>,
    pub cover_url: Option<String>,
    pub tags: Option<serde_json::Value>, // JSONB array: ["tag1", "tag2"]
    pub date: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Request to create a page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePageRequest {
    pub title: String,
    #[serde(default)]
    pub content: String,
    #[serde(rename = "notebookId")]
    pub notebook_id: Option<String>,  // For auto-add to notebook_items (not stored on page)
    pub icon: Option<String>,
    pub icon_color: Option<String>,
    pub cover_url: Option<String>,
    pub tags: Option<serde_json::Value>, // JSONB array: ["tag1", "tag2"]
}

/// Request to update a page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePageRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub icon: Option<Option<String>>,      // None = don't change, Some(None) = clear, Some(Some(x)) = set
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub icon_color: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub cover_url: Option<Option<String>>, // None = don't change, Some(None) = clear, Some(Some(x)) = set
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub tags: Option<Option<serde_json::Value>>, // None = don't change, Some(None) = clear, Some(Some(x)) = set
}

/// Paginated list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageListResponse {
    pub pages: Vec<PageSummary>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

/// An inbound reference — a page that links TO the queried page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backlink {
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
    /// A one-line plain-text snippet of the surrounding context.
    pub snippet: String,
    pub updated_at: Timestamp,
}

/// Backlinks (references) response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacklinksResponse {
    pub backlinks: Vec<Backlink>,
}

/// Entity search result for autocomplete
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefSearchResult {
    pub id: String,
    pub name: String,
    pub entity_type: String,
    pub icon: String,
    pub url: String,
    pub mime_type: Option<String>,
}

/// Entity search response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefSearchResponse {
    pub results: Vec<RefSearchResult>,
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
    pool: &PgPool,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<PageListResponse> {
    let limit = limit.unwrap_or(50).min(100);
    let offset = offset.unwrap_or(0);

    // Two exclusions, for two different reasons.
    //
    // `date IS NULL` drops day-linked reflections, which belong to their day.
    //
    // `kind = 'page'` drops ARTICLES (migration 0081). An article is a page in
    // storage — same table, same editor, same revision history — but it is not
    // a document a person made, and the Pages list is a list of things you
    // made. Without this, opening the wiki on a real box would eventually push
    // hundreds of machine-written entity articles into it, and the destination
    // would be swallowed by an implementation detail. Articles remain in
    // SEARCH, because prose about your life is exactly what you want to find.
    let total: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM app_pages WHERE date IS NULL AND kind = 'page'"#)
        .fetch_one(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to count pages: {}", e)))?;

    let pages = sqlx::query_as::<_, PageSummary>(
        r#"
        SELECT id, title, icon, icon_color, cover_url, tags, date, created_at, updated_at
        FROM app_pages
        WHERE date IS NULL AND kind = 'page'
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
pub async fn get_page(pool: &PgPool, id: &str) -> Result<Page> {
    let page = sqlx::query_as::<_, Page>(
        r#"
        SELECT id, title, content, icon, icon_color, cover_url, tags, date, created_at, updated_at
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

/// Get inbound references (backlinks) for a page.
///
/// Links are stored inline in markdown as `[@Label](/page/{id})`. On a
/// single-tenant box the page count is small, so we pre-filter candidate pages
/// with a `LIKE` on the target URL and extract a context snippet in Rust.
pub async fn get_page_backlinks(pool: &PgPool, id: &str) -> Result<BacklinksResponse> {
    // The trailing `)` pins the match to the exact id (so `pg_ab` doesn't match
    // `pg_abc`) and to a real markdown link, not a bare mention of the id.
    let needle = format!("/page/{})", id);
    let like = format!("%{}%", needle);

    #[derive(sqlx::FromRow)]
    struct Row {
        id: String,
        title: String,
        icon: Option<String>,
        content: String,
        updated_at: Timestamp,
    }

    let rows = sqlx::query_as::<_, Row>(
        r#"
        SELECT id, title, icon, content, updated_at
        FROM app_pages
        WHERE id <> $1 AND content LIKE $2
        ORDER BY updated_at DESC
        "#,
    )
    .bind(id)
    .bind(&like)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get backlinks: {}", e)))?;

    let backlinks = rows
        .into_iter()
        .filter_map(|row| {
            let snippet = backlink_snippet(&row.content, &needle)?;
            Some(Backlink {
                id: row.id,
                title: row.title,
                icon: row.icon,
                snippet,
                updated_at: row.updated_at,
            })
        })
        .collect();

    Ok(BacklinksResponse { backlinks })
}

/// Extract a one-line, plain-text snippet around the first link matching
/// `needle` within markdown `content`. Returns `None` if the line is empty
/// after stripping markup.
fn backlink_snippet(content: &str, needle: &str) -> Option<String> {
    let pos = content.find(needle)?;
    // Bound the snippet to the enclosing line.
    let start = content[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let end = content[pos..]
        .find('\n')
        .map(|i| pos + i)
        .unwrap_or(content.len());
    let plain = strip_markdown(&content[start..end]);
    let trimmed = plain.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(truncate_chars(trimmed, 160))
}

/// Reduce a line of markdown to plain text: `[text](url)` → `text`, and strip
/// leading heading/list/quote markers.
fn strip_markdown(line: &str) -> String {
    use std::sync::OnceLock;
    static LINK_RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = LINK_RE.get_or_init(|| regex::Regex::new(r"\[([^\]]*)\]\([^)]*\)").unwrap());
    let no_links = re.replace_all(line, "$1");
    no_links
        .trim_start_matches(|c: char| matches!(c, '#' | '-' | '*' | '>' | ' ' | '\t'))
        .to_string()
}

/// Truncate to at most `max` characters (not bytes), appending an ellipsis.
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

/// Create a new page
/// If notebook_id is provided and not the system notebook, auto-adds to notebook_items
pub async fn create_page(pool: &PgPool, req: CreatePageRequest) -> Result<Page> {
    let title = req.title.trim();
    if title.is_empty() {
        return Err(Error::InvalidInput("Page title cannot be empty".into()));
    }

    // Generate ID using title and current timestamp for uniqueness
    let timestamp = chrono::Utc::now().to_rfc3339();
    let id = generate_id(PAGE_PREFIX, &[title, &timestamp]);

    let page = sqlx::query_as::<_, Page>(
        r#"
        INSERT INTO app_pages (id, title, content, icon, icon_color, cover_url, tags)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, title, content, icon, icon_color, cover_url, tags, date, created_at, updated_at
        "#,
    )
    .bind(&id)
    .bind(title)
    .bind(&req.content)
    .bind(&req.icon)
    .bind(&req.icon_color)
    .bind(&req.cover_url)
    .bind(req.tags.clone().unwrap_or_else(|| serde_json::json!([])))
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create page: {}", e)))?;

    // Auto-add the page as a member of the Notebook it was created in.
    if let Some(notebook_id) = &req.notebook_id {
        let url = format!("/page/{}", page.id);
        if let Err(e) = crate::api::notebooks::add_notebook_item(
            pool,
            notebook_id,
            crate::api::notebooks::AddNotebookItemRequest { url },
        )
        .await
        {
            tracing::warn!("Failed to auto-add page to notebook {}: {}", notebook_id, e);
            // Don't fail page creation if auto-add fails
        }
    }

    Ok(page)
}

/// Update an existing page
pub async fn update_page(pool: &PgPool, id: &str, req: UpdatePageRequest) -> Result<Page> {
    // Verify page exists
    let existing = get_page(pool, id).await?;

    let title = req.title.as_deref().unwrap_or(&existing.title);
    let content = req.content.as_deref().unwrap_or(&existing.content);
    let icon = match &req.icon {
        Some(val) => val.clone(),
        None => existing.icon,
    };
    let icon_color = match &req.icon_color {
        Some(val) => val.clone(),
        None => existing.icon_color,
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
        SET title = $2, content = $3, icon = $4, icon_color = $5, cover_url = $6, tags = $7
        WHERE id = $1
        RETURNING id, title, content, icon, icon_color, cover_url, tags, date, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(title.trim())
    .bind(content)
    .bind(icon)
    .bind(icon_color)
    .bind(cover_url)
    .bind(tags)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update page: {}", e)))?;

    Ok(page)
}

/// Delete a page by ID
/// Also cleans up all notebook_items references (orphan cleanup)
pub async fn delete_page(pool: &PgPool, id: &str) -> Result<()> {
    // First delete the page
    let result = sqlx::query(r#"DELETE FROM app_pages WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to delete page: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Page not found: {}", id)));
    }

    // Clean up all notebook_items references
    let url = format!("/page/{}", id);
    if let Err(e) = crate::api::notebooks::remove_items_by_url(pool, &url).await {
        tracing::warn!("Failed to clean up notebook_items for page {}: {}", id, e);
        // Don't fail deletion if cleanup fails
    }

    Ok(())
}

// ============================================================================
// Reflections (pages linked to a day)
// ============================================================================

/// Legacy reflections for a date — READ ONLY. The reflection primitive is
/// retired (2026-08-03): writing about a day belongs to the day's article
/// (or a note on the day), not to a parallel date-linked page. This reader
/// stays so pages minted before the retirement remain reachable; nothing
/// creates new ones.
pub async fn get_reflections_for_date(pool: &PgPool, date: &str) -> Result<Vec<Page>> {
    let pages = sqlx::query_as::<_, Page>(
        r#"
        SELECT id, title, content, icon, icon_color, cover_url, tags, date, created_at, updated_at
        FROM app_pages
        WHERE date = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(date)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get reflections: {}", e)))?;

    Ok(pages)
}

// ============================================================================
// Entity Search (for [[]] autocomplete)
// ============================================================================

/// Raw entity search result from database (before URL computation)
#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
struct RawRefSearchResult {
    id: String,
    name: String,
    entity_type: String,
    icon: String,
    mime_type: Option<String>,
    updated_at: Timestamp,
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
        "notebook" => format!("/notebook/{}", id),
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
pub async fn search_refs(pool: &PgPool, query: &str) -> Result<RefSearchResponse> {
    let query = query.trim();

    // For empty query, show most recent items
    let (contains_pattern, prefix_pattern) = if query.is_empty() {
        ("%".to_string(), "%".to_string())
    } else {
        (format!("%{}%", query), format!("{}%", query))
    };

    // The exact surface, lowercased, for the alias leg.
    //
    // A separate bind because $1 and $2 are LIKE PATTERNS (`%q%`, `q%`) and
    // aliases are matched by containment, not by pattern: `jsonb_exists` on
    // `%sarah%` finds nothing. 0037 stores aliases lowercased and the resolver
    // lowercases the surface before matching, so this must too.
    let exact = query.to_lowercase();

    let limit = 15i64;

    // Search across multiple tables with UNION
    // Relevance: 0 = prefix match (highest), 1 = contains match
    let raw_results = sqlx::query_as::<_, RawRefSearchResult>(
        r#"
        -- Aliases are the whole point of 0037: "a mention resolves iff its
        -- normalized surface matches EXACTLY ONE entity, by canonical name,
        -- nickname, or an alias a human put here". The column shipped and this
        -- navigator never read it, so linking "Sarah" once resolved nothing —
        -- the decision had a home and no door. An exact alias hit ranks with a
        -- prefix hit: it is not a fuzzy match, it is a name you declared.
        SELECT id, name, 'person' as entity_type, 'ri:user-line' as icon,
               NULL as mime_type, updated_at,
               CASE WHEN name ILIKE $2 OR jsonb_exists(aliases, $4)
                    THEN 0 ELSE 1 END as relevance
        FROM wiki_people
        WHERE name ILIKE $1 OR nickname ILIKE $1 OR jsonb_exists(aliases, $4)
        UNION ALL
        SELECT id, name, 'place' as entity_type, 'ri:map-pin-line' as icon,
               NULL as mime_type, updated_at,
               CASE WHEN name ILIKE $2 OR jsonb_exists(aliases, $4)
                    THEN 0 ELSE 1 END as relevance
        FROM wiki_places
        WHERE name ILIKE $1 OR jsonb_exists(aliases, $4)
        UNION ALL
        SELECT id, name, 'org' as entity_type, 'ri:building-line' as icon,
               NULL as mime_type, updated_at,
               CASE WHEN name ILIKE $2 OR jsonb_exists(aliases, $4)
                    THEN 0 ELSE 1 END as relevance
        FROM wiki_orgs
        WHERE name ILIKE $1 OR jsonb_exists(aliases, $4)
        UNION ALL
        SELECT id, filename as name, 'file' as entity_type, 'ri:file-line' as icon,
               mime_type, updated_at,
               CASE WHEN filename ILIKE $2 THEN 0 ELSE 1 END as relevance
        FROM app_drive_files
        WHERE filename ILIKE $1 AND deleted_at IS NULL
        UNION ALL
        SELECT id, title as name, 'page' as entity_type, 'ri:file-text-line' as icon,
               NULL as mime_type, updated_at,
               CASE WHEN title ILIKE $2 THEN 0 ELSE 1 END as relevance
        FROM app_pages
        -- Articles are excluded here and surfaced under their SUBJECT instead:
        -- typing "Sarah" should land on Sarah, not on a page that happens to be
        -- about her (migration 0081).
        WHERE title ILIKE $1 AND kind = 'page'
        UNION ALL
        SELECT id, title as name, 'chat' as entity_type,
               CASE WHEN icon LIKE 'ri:%' THEN icon ELSE 'ri:chat-3-line' END as icon,
               NULL as mime_type, updated_at,
               CASE WHEN title ILIKE $2 THEN 0 ELSE 1 END as relevance
        FROM app_chats
        WHERE title ILIKE $1 AND title <> ''
        UNION ALL
        SELECT id, name, 'notebook' as entity_type,
               CASE WHEN icon LIKE 'ri:%' THEN icon ELSE 'ri:folder-line' END as icon,
               NULL as mime_type, updated_at,
               CASE WHEN name ILIKE $2 THEN 0 ELSE 1 END as relevance
        FROM app_notebooks
        WHERE name ILIKE $1
        ORDER BY relevance ASC, updated_at DESC
        LIMIT $3
        "#,
    )
    .bind(&contains_pattern)
    .bind(&prefix_pattern)
    .bind(limit)
    .bind(&exact)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to search entities: {}", e)))?;

    // Convert raw results to RefSearchResult with computed URLs
    let results: Vec<RefSearchResult> = raw_results
        .into_iter()
        .map(|r| RefSearchResult {
            url: get_entity_url(&r.entity_type, &r.id),
            id: r.id,
            name: r.name,
            entity_type: r.entity_type,
            icon: r.icon,
            mime_type: r.mime_type,
        })
        .collect();

    Ok(RefSearchResponse { results })
}

// ============================================================================
// Version History Operations
// ============================================================================

/// Create a new version snapshot for a page
pub async fn create_version(
    pool: &PgPool,
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
        "SELECT MAX(version_number) FROM app_page_versions WHERE page_id = $1",
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
    pool: &PgPool,
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

// ============================================================================
// Page Sharing
// ============================================================================

/// A page share record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PageShare {
    pub id: String,
    pub page_id: String,
    pub token: String,
    pub created_at: Timestamp,
}

/// Public shared page data (minimal, no timestamps or tags)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedPage {
    pub title: String,
    pub content: String,
    pub icon: Option<String>,
    pub cover_url: Option<String>,
    pub share_token: String,
}

/// Create or replace a share token for a page
pub async fn create_page_share(pool: &PgPool, page_id: &str) -> Result<PageShare> {
    // Verify page exists
    let _ = get_page(pool, page_id).await?;

    // Delete existing share for this page (one share per page)
    sqlx::query("DELETE FROM app_page_shares WHERE page_id = $1")
        .bind(page_id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to clear existing share: {}", e)))?;

    // Generate new share
    let token = uuid::Uuid::new_v4().to_string();
    let timestamp = chrono::Utc::now().to_rfc3339();
    let id = generate_id(PAGE_SHARE_PREFIX, &[page_id, &timestamp]);

    let share = sqlx::query_as::<_, PageShare>(
        r#"
        INSERT INTO app_page_shares (id, page_id, token)
        VALUES ($1, $2, $3)
        RETURNING id, page_id, token, created_at
        "#,
    )
    .bind(&id)
    .bind(page_id)
    .bind(&token)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create page share: {}", e)))?;

    Ok(share)
}

/// Get the active share for a page (if any)
pub async fn get_page_share(pool: &PgPool, page_id: &str) -> Result<Option<PageShare>> {
    let share = sqlx::query_as::<_, PageShare>(
        r#"
        SELECT id, page_id, token, created_at
        FROM app_page_shares
        WHERE page_id = $1
        "#,
    )
    .bind(page_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get page share: {}", e)))?;

    Ok(share)
}

/// Revoke the share for a page
pub async fn delete_page_share(pool: &PgPool, page_id: &str) -> Result<()> {
    let result = sqlx::query("DELETE FROM app_page_shares WHERE page_id = $1")
        .bind(page_id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to delete page share: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!(
            "No share found for page: {}",
            page_id
        )));
    }

    Ok(())
}

/// Get a shared page by its share token (public, no auth required)
///
/// Materializes markdown from the Yjs document state for proper rendering.
/// Falls back to the raw content column if no Yjs state exists.
pub async fn get_shared_page(pool: &PgPool, token: &str) -> Result<SharedPage> {
    let row = sqlx::query_as::<_, SharedPageRow>(
        r#"
        SELECT p.title, p.content, p.yjs_state, p.icon, p.cover_url
        FROM app_page_shares s
        JOIN app_pages p ON s.page_id = p.id
        WHERE s.token = $1
        "#,
    )
    .bind(token)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get shared page: {}", e)))?
    .ok_or_else(|| Error::NotFound("Invalid or expired share link".into()))?;

    // Materialize markdown from Yjs state, fall back to raw content column.
    // catch_unwind guards against panics in yrs (e.g. corrupted state).
    let content = if let Some(yjs_state) = &row.yjs_state {
        match std::panic::catch_unwind(|| yjs_state_to_markdown(yjs_state)) {
            Ok(md) => md,
            Err(_) => {
                tracing::warn!("yrs panic decoding shared page, falling back to raw content");
                row.content
            }
        }
    } else {
        row.content
    };

    Ok(SharedPage {
        title: row.title,
        content,
        icon: row.icon,
        cover_url: row.cover_url,
        share_token: token.to_string(),
    })
}

#[derive(sqlx::FromRow)]
struct SharedPageRow {
    title: String,
    content: String,
    yjs_state: Option<Vec<u8>>,
    icon: Option<String>,
    cover_url: Option<String>,
}

/// Decode Yjs binary state into markdown (Y.Text format).
fn yjs_state_to_markdown(yjs_state: &[u8]) -> String {
    let doc = Doc::new();
    if let Ok(update) = Update::decode_v1(yjs_state) {
        let mut txn = doc.transact_mut();
        txn.apply_update(update);
    }
    let txn = doc.transact();

    if let Some(text) = txn.get_text("content") {
        text.get_string(&txn)
    } else {
        String::new()
    }
}

/// Validate that a file ID is referenced by a shared page (for public file access)
pub async fn validate_shared_file(
    pool: &PgPool,
    token: &str,
    file_id: &str,
) -> Result<String> {
    // Get the page content + cover_url for this share token
    let row: Option<(String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT p.content, p.cover_url
        FROM app_page_shares s
        JOIN app_pages p ON s.page_id = p.id
        WHERE s.token = $1
        "#,
    )
    .bind(token)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to validate shared file: {}", e)))?;

    let (content, cover_url) = row
        .ok_or_else(|| Error::NotFound("Invalid share token".into()))?;

    // Check if file_id appears in the page content or cover_url
    let in_content = content.contains(file_id);
    let in_cover = cover_url
        .as_ref()
        .map(|url| url.contains(file_id))
        .unwrap_or(false);

    if !in_content && !in_cover {
        return Err(Error::NotFound("File not found in shared page".into()));
    }

    Ok(file_id.to_string())
}

/// Get a single version by ID (includes snapshot for restore)
pub async fn get_version(pool: &PgPool, version_id: &str) -> Result<PageVersionDetail> {
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

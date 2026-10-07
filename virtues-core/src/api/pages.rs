//! Pages API
//!
//! This module provides CRUD operations for user-authored pages.
//! Pages are knowledge documents with entity linking support using
//! the format: ((Display Name))[[prefix_hash]]
//!
//! Note: Pages don't "belong" to projects - they're just URL-native entities.
//! Organization is handled by project_items which hold URL references.

use crate::error::{Error, Result};
use crate::ids::{generate_id, PAGE_PREFIX, PAGE_VERSION_PREFIX};
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
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Request to create a page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePageRequest {
    pub title: String,
    #[serde(default)]
    pub content: String,
    #[serde(rename = "projectId", alias = "notebookId")]
    pub project_id: Option<String>,  // For auto-add to project_items (not stored on page)
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
    //
    // `kind = 'page'` drops ARTICLES (migration 0081). An article is a page in
    // storage — same table, same editor, same revision history — but it is not
    // a document a person made, and the Pages list is a list of things you
    // made. Without this, opening the wiki on a real box would eventually push
    // hundreds of machine-written entity articles into it, and the destination
    // would be swallowed by an implementation detail. Articles remain in
    // SEARCH, because prose about your life is exactly what you want to find.
    let total: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM app_pages WHERE kind = 'page' AND deleted_at IS NULL"#)
        .fetch_one(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to count pages: {}", e)))?;

    let pages = sqlx::query_as::<_, PageSummary>(
        r#"
        SELECT id, title, icon, icon_color, cover_url, tags, created_at, updated_at
        FROM app_pages
        WHERE kind = 'page' AND deleted_at IS NULL
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
        SELECT id, title, content, icon, icon_color, cover_url, tags, created_at, updated_at
        FROM app_pages
        WHERE id = $1 AND deleted_at IS NULL
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
        WHERE id <> $1 AND content LIKE $2 AND deleted_at IS NULL
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
/// If project_id is provided and not the system project, auto-adds to project_items
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
        RETURNING id, title, content, icon, icon_color, cover_url, tags, created_at, updated_at
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

    // Auto-add the page as a member of the Project it was created in.
    if let Some(project_id) = &req.project_id {
        let url = format!("/page/{}", page.id);
        if let Err(e) = crate::api::projects::add_project_item(
            pool,
            project_id,
            crate::api::projects::AddProjectItemRequest { url },
        )
        .await
        {
            tracing::warn!("Failed to auto-add page to project {}: {}", project_id, e);
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
        RETURNING id, title, content, icon, icon_color, cover_url, tags, created_at, updated_at
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

/// Delete a page — into the trash. The row keeps its versions, shares and
/// project membership and leaves every listing and the search index; Recently
/// deleted holds it for `trash::TRASH_RETENTION_DAYS`. The hard delete is
/// `trash::purge`, which the wiki article path calls directly because an
/// article page belongs to its `wiki_articles` row, not to the owner's desk.
pub async fn delete_page(pool: &PgPool, id: &str) -> Result<()> {
    crate::api::trash::trash(pool, crate::api::trash::TrashKind::Page, id).await
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
///
/// `entity_type` here is an id PREFIX (`org`), not a subject_type
/// (`organization`), because every caller has an id. Wiki subjects answer from
/// the registry so this cannot drift from the rest of the wiki again; the rest
/// are namespaces with no subject behind them.
fn get_entity_url(entity_type: &str, id: &str) -> String {
    if let Some(subject) = crate::api::subjects::by_id(id) {
        if let Some(route) = subject.route {
            return format!("/{route}/{id}");
        }
    }
    match entity_type {
        "page" => format!("/page/{}", id),
        "source" => format!("/source/{}", id),
        "chat" => format!("/chat/{}", id),
        "project" => format!("/project/{}", id),
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
        WHERE title ILIKE $1 AND kind = 'page' AND deleted_at IS NULL
        UNION ALL
        SELECT id, title as name, 'chat' as entity_type,
               CASE WHEN icon LIKE 'ri:%' THEN icon ELSE 'ri:chat-3-line' END as icon,
               NULL as mime_type, updated_at,
               CASE WHEN title ILIKE $2 THEN 0 ELSE 1 END as relevance
        FROM app_chats
        WHERE title ILIKE $1 AND title <> '' AND deleted_at IS NULL
        UNION ALL
        SELECT id, name, 'project' as entity_type,
               CASE WHEN icon LIKE 'ri:%' THEN icon ELSE 'ri:folder-line' END as icon,
               NULL as mime_type, updated_at,
               CASE WHEN name ILIKE $2 THEN 0 ELSE 1 END as relevance
        FROM app_projects
        WHERE name ILIKE $1 AND deleted_at IS NULL
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

/// Cut a version from bytes the server already holds.
///
/// `create_version` takes a base64 snapshot from a client request; no
/// component under `components/wiki/` calls it, so without this a wiki
/// article's history would be whatever the generic page editor happened to
/// autosave. The machine's writers cut their own versions, so the history of
/// an article is a real account of who wrote what.
///
/// The convention every writer follows: a version's snapshot is the state
/// AFTER the edit it records, and `created_by` names who produced that state.
/// A writer keeps the state before its edit with `cut_restore_point` first,
/// and versions the edit with `cut_version`.
#[allow(clippy::too_many_arguments)]
pub async fn create_version_from_snapshot(
    pool: &PgPool,
    page_id: &str,
    snapshot: &[u8],
    content_preview: &str,
    created_by: &str,
    description: Option<&str>,
) -> Result<PageVersionSummary> {
    create_version(
        pool,
        page_id,
        CreateVersionRequest {
            snapshot: BASE64.encode(snapshot),
            content_preview: content_preview.chars().take(500).collect(),
            created_by: created_by.to_string(),
            description: description.map(|s| s.to_string()),
        },
    )
    .await
}

/// What a restore point says in `description`. With `created_by = 'auto'`
/// it is how History tells a restore point from the owner's own autosave
/// (`'auto'` too, described "Auto-saved (idle)" and the like), so it is a
/// contract with `wiki_articles`' reader, not a label to reword.
pub const RESTORE_POINT: &str = "Restore point";

/// Whether a version row is a restore point: the page as it stood before a
/// machine changed the whole of it, kept so the change can be put back.
///
/// It is not an edit, so History shows no entry for it; it is the text the
/// next entry is diffed against.
pub fn is_restore_point(created_by: &str, description: Option<&str>) -> bool {
    created_by == "auto" && description == Some(RESTORE_POINT)
}

/// A version that holds what a page said before a machine changed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorePoint {
    pub version_number: i64,
    /// The text it holds: the page as `cut_restore_point` read it.
    pub text: String,
}

/// Keep what a page says now, before a machine changes it, so the change can
/// be put back. The one way to cut a restore point of a page as it stands;
/// a page's first draft, before anything has versioned it, is kept by
/// `keep_first_draft`.
///
/// Returns a version that holds exactly the current text: the latest
/// version when it already does (nothing new is cut), otherwise a new
/// restore point. Undoing the change means putting that version back.
///
/// Labelled `'auto'`, the browser's word for a save nobody asked for, rather
/// than `'ai'` or `'user'`: the text in it was written by whoever wrote it,
/// and History credits that to the versions around it. Like every machine
/// row it can be pruned once fifty newer versions exist; the owner's own are
/// never pruned.
pub async fn cut_restore_point(
    pool: &PgPool,
    yjs: &crate::server::yjs::YjsState,
    page_id: &str,
) -> Result<RestorePoint> {
    let now = yjs
        .text_and_state(page_id)
        .await
        .map_err(|e| Error::Other(format!("could not read the page: {e}")))?;

    let latest: Option<(i64, Option<Vec<u8>>)> = sqlx::query_as(
        "SELECT version_number, yjs_snapshot FROM app_page_versions \
         WHERE page_id = $1 ORDER BY version_number DESC LIMIT 1",
    )
    .bind(page_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to read the latest version: {e}")))?;
    if let Some((number, Some(snapshot))) = latest {
        if crate::server::yjs::extract_text_content(&snapshot) == now.text {
            return Ok(RestorePoint {
                version_number: number,
                text: now.text,
            });
        }
    }

    let kept = create_version_from_snapshot(
        pool,
        page_id,
        &now.state,
        &now.text,
        "auto",
        Some(RESTORE_POINT),
    )
    .await?;
    Ok(RestorePoint {
        version_number: kept.version_number,
        text: now.text,
    })
}

/// Keep a page's first draft as its first version, when it has no version
/// yet: a restore point, so History has the text every later edit is diffed
/// against, and putting it back undoes them.
///
/// For a draft the server has as markdown rather than in a live doc: an
/// article as it is created, or as the editor last wrote it when nothing
/// versioned it then. The state is built from the text
/// (`yjs::state_from_text`); a version is put back by its text, so a state
/// built apart from the page's own doc serves. Takes any executor, so an
/// article is created with its first version in one transaction.
///
/// Returns the version's number, or `None` when the page already has a
/// version and nothing is cut.
pub async fn keep_first_draft<'e>(
    executor: impl sqlx::PgExecutor<'e>,
    page_id: &str,
    text: &str,
) -> Result<Option<i64>> {
    let id = generate_id(
        PAGE_VERSION_PREFIX,
        &[page_id, &chrono::Utc::now().to_rfc3339()],
    );
    let preview: String = text.chars().take(500).collect();
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO app_page_versions \
             (id, page_id, version_number, yjs_snapshot, content_preview, created_by, description) \
         SELECT $1, $2, 1, $3, $4, 'auto', $5 \
         WHERE NOT EXISTS (SELECT 1 FROM app_page_versions WHERE page_id = $2) \
         ON CONFLICT (page_id, version_number) DO NOTHING \
         RETURNING version_number",
    )
    .bind(&id)
    .bind(page_id)
    .bind(crate::server::yjs::state_from_text(text))
    .bind(&preview)
    .bind(RESTORE_POINT)
    .fetch_optional(executor)
    .await
    .map_err(|e| Error::Database(format!("Failed to keep the first draft: {e}")))
}

/// Version what a write left on the page (`written`, as the write returned
/// it), credited to `created_by`. The one way to cut a version after an
/// edit.
///
/// Cut from the write's own state rather than from a fresh read, so the
/// version holds exactly that write: an open editor's keystroke landing just
/// after it goes into the owner's next autosave instead.
///
/// Returns the version's number, or `None` when it could not be saved. The
/// change is already on the page by then, so a failure costs History its
/// entry, not the change, and is logged rather than returned.
pub async fn cut_version(
    pool: &PgPool,
    page_id: &str,
    written: &crate::server::yjs::Written,
    created_by: &str,
    description: Option<&str>,
) -> Option<i64> {
    match create_version_from_snapshot(
        pool,
        page_id,
        &written.state,
        &written.text,
        created_by,
        description,
    )
    .await
    {
        Ok(version) => Some(version.version_number),
        Err(e) => {
            tracing::error!(page = %page_id, created_by, error = %e,
                "the change is on the page but its version was not saved");
            None
        }
    }
}

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

    // The number is MAX + 1 under UNIQUE (page_id, version_number), so two
    // writers cutting a version of one page at the same moment (an open
    // editor's autosave and a server-side edit) can read the same MAX. The
    // loser tries once more with the winner's row in place.
    let mut retried = false;
    let version = loop {
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

        let inserted = sqlx::query_as::<_, PageVersionSummary>(
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
        .await;
        match inserted {
            Ok(version) => break version,
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() && !retried => {
                retried = true;
            }
            Err(e) => return Err(Error::Database(format!("Failed to create version: {}", e))),
        }
    };

    // Prune old versions beyond the cap (keep most recent 50)
    sqlx::query(
        r#"
        DELETE FROM app_page_versions
        WHERE page_id = $1
          -- A version the PERSON wrote is never pruned. The cap exists to stop
          -- machine editions accumulating, and an article the record maintains
          -- reaches fifty of those in a year — at which point the oldest row
          -- the cap would drop is the person's own first edit, the one version
          -- on the page nobody can reconstruct.
          AND created_by IS DISTINCT FROM 'user'
          AND id NOT IN (
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
// Page text from its document
// ============================================================================

/// Decode Yjs binary state into markdown (Y.Text format). Sharing freezes a
/// page from this (`api::publish_page`).
pub(crate) fn yjs_state_to_markdown(yjs_state: &[u8]) -> String {
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

/// For a test that stops a writer partway: wait until a statement of this
/// test's database whose text matches `like` is waiting on a lock the test
/// holds.
#[cfg(test)]
pub(crate) async fn until_a_statement_waits(pool: &PgPool, like: &str) {
    for _ in 0..500 {
        let waiting: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity \
             WHERE datname = current_database() AND wait_event_type = 'Lock' AND query LIKE $1",
        )
        .bind(like)
        .fetch_one(pool)
        .await
        .unwrap();
        if waiting > 0 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("no statement like {like} came to wait on the lock");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::yjs::YjsState;

    async fn page(pool: &PgPool, content: &str) -> String {
        create_page(
            pool,
            CreatePageRequest {
                title: "Notes".into(),
                content: content.into(),
                project_id: None,
                icon: None,
                icon_color: None,
                cover_url: None,
                tags: None,
            },
        )
        .await
        .unwrap()
        .id
    }

    /// Two writers cutting a version of one page at once both read the same
    /// MAX. Both versions must land, numbered apart, instead of the second
    /// failing on the unique constraint.
    #[sqlx::test]
    async fn two_versions_cut_at_once_both_land(pool: PgPool) {
        let page_id = page(&pool, "Text.\n").await;
        let yjs = YjsState::new(pool.clone());
        let state = yjs.text_and_state(&page_id).await.unwrap().state;
        let cut = |who: &'static str| {
            let (pool, page_id, state) = (pool.clone(), page_id.clone(), state.clone());
            async move {
                create_version_from_snapshot(&pool, &page_id, &state, "Text.\n", who, None).await
            }
        };
        let (a, b) = tokio::join!(cut("auto"), cut("ai"));
        let mut numbers = vec![a.unwrap().version_number, b.unwrap().version_number];
        numbers.sort();
        assert_eq!(numbers, vec![1, 2]);
    }

    /// A restore point holds exactly what the page said, and is cut only when
    /// no version already does: the number it returns is always one whose text
    /// is the page's.
    #[sqlx::test]
    async fn a_restore_point_keeps_the_current_text_once(pool: PgPool) {
        let page_id = page(&pool, "The page as it was.\n").await;
        let yjs = YjsState::new(pool.clone());

        let kept = cut_restore_point(&pool, &yjs, &page_id).await.unwrap();
        assert_eq!(kept.text, "The page as it was.\n");
        let (by, description, snapshot): (String, Option<String>, Vec<u8>) = sqlx::query_as(
            "SELECT created_by, description, yjs_snapshot FROM app_page_versions \
             WHERE page_id = $1 AND version_number = $2",
        )
        .bind(&page_id)
        .bind(kept.version_number)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(is_restore_point(&by, description.as_deref()));
        assert_eq!(
            crate::server::yjs::extract_text_content(&snapshot),
            "The page as it was.\n"
        );

        // Nothing changed since: the same version answers, and no row is added.
        assert_eq!(cut_restore_point(&pool, &yjs, &page_id).await.unwrap(), kept);

        // The page moves on: a new restore point is cut for the new text.
        yjs.replace_text(&page_id, "The page as it was.\n", "The page now.\n")
            .await
            .unwrap();
        let next = cut_restore_point(&pool, &yjs, &page_id).await.unwrap();
        assert!(next.version_number > kept.version_number);
        assert_eq!(next.text, "The page now.\n");
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM app_page_versions WHERE page_id = $1")
                .bind(&page_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 2);
    }

    /// A first draft is kept as a page's first version, a restore point
    /// holding its text, and only while the page has no version at all.
    #[sqlx::test]
    async fn a_first_draft_is_kept_only_on_a_page_with_no_version(pool: PgPool) {
        let page_id = page(&pool, "The draft.\n").await;

        assert_eq!(keep_first_draft(&pool, &page_id, "The draft.\n").await.unwrap(), Some(1));
        let (by, description, snapshot): (String, Option<String>, Vec<u8>) = sqlx::query_as(
            "SELECT created_by, description, yjs_snapshot FROM app_page_versions \
             WHERE page_id = $1 AND version_number = 1",
        )
        .bind(&page_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(is_restore_point(&by, description.as_deref()));
        assert_eq!(crate::server::yjs::extract_text_content(&snapshot), "The draft.\n");

        assert_eq!(keep_first_draft(&pool, &page_id, "Another.\n").await.unwrap(), None);
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM app_page_versions WHERE page_id = $1")
                .bind(&page_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 1);
    }

    /// A version holds what the write that returned it left on the page, not
    /// whatever the page says by the time the version is cut: a keystroke
    /// that lands in between belongs to whoever typed it, in their next
    /// version.
    #[sqlx::test]
    async fn a_version_holds_what_its_write_left(pool: PgPool) {
        let page_id = page(&pool, "Text.\n").await;
        let yjs = YjsState::new(pool.clone());

        let written = yjs.replace_text(&page_id, "Text.\n", "Text, revised.\n").await.unwrap();
        yjs.apply_text_diff(&page_id, "Text, revised.\n", "Text, revised.\nTyped after.\n")
            .await
            .unwrap();

        let number = cut_version(&pool, &page_id, &written, "ai", Some("revised"))
            .await
            .expect("the version is saved");
        let (by, preview, snapshot): (String, String, Vec<u8>) = sqlx::query_as(
            "SELECT created_by, content_preview, yjs_snapshot FROM app_page_versions \
             WHERE page_id = $1 AND version_number = $2",
        )
        .bind(&page_id)
        .bind(number)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(by, "ai");
        assert_eq!(preview, "Text, revised.\n");
        assert_eq!(crate::server::yjs::extract_text_content(&snapshot), "Text, revised.\n");
    }

    /// An owner's autosave is labelled `'auto'` too; only the fixed
    /// description makes a restore point.
    #[test]
    fn only_the_fixed_label_is_a_restore_point() {
        assert!(is_restore_point("auto", Some(RESTORE_POINT)));
        assert!(!is_restore_point("auto", Some("Auto-saved (idle)")));
        assert!(!is_restore_point("auto", None));
        assert!(!is_restore_point("ai", Some(RESTORE_POINT)));
    }
}

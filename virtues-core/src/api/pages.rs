//! Pages API
//!
//! CRUD, versions and restore for user-authored pages. A page's text is
//! markdown in a Y.Text or a Yjs tree under the document contract
//! (`PageFormat`); either way `content` holds markdown, in which a ref to
//! a person, place or other page reads `[@Label](/kind/id)`.
//!
//! Note: Pages don't "belong" to projects - they're just URL-native entities.
//! Organization is handled by project_items which hold URL references.

use crate::error::{Error, Result};
use crate::ids::{generate_id, PAGE_PREFIX, PAGE_VERSION_PREFIX};
use crate::types::Timestamp;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::PgPool;

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

/// What a page's text is (`app_pages.format`): markdown in a Y.Text, or a
/// Yjs XML tree under the document contract (`crates/virtues-document`).
/// Every reader and writer of page text routes on it; `content` holds
/// markdown either way, a tree page's export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PageFormat {
    Markdown,
    Tree,
}

impl PageFormat {
    /// The value `app_pages.format` holds.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Tree => "tree",
        }
    }
}

impl TryFrom<String> for PageFormat {
    type Error = String;

    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        match value.as_str() {
            "markdown" => Ok(Self::Markdown),
            "tree" => Ok(Self::Tree),
            other => Err(format!("unknown page format `{other}`")),
        }
    }
}

/// `VIRTUES_TREE_PAGES=1`, read once. Gates creation only: tree pages that
/// exist keep opening, editing and saving with it off.
pub fn tree_pages_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("VIRTUES_TREE_PAGES").is_ok_and(|v| v.trim() == "1")
    })
}

/// The format a new user page is created in when the request names none:
/// tree when the flag is on, else markdown.
pub fn default_page_format() -> PageFormat {
    if tree_pages_enabled() {
        PageFormat::Tree
    } else {
        PageFormat::Markdown
    }
}

/// The page's format. `NotFound` for a missing or trashed page.
pub async fn page_format(pool: &PgPool, page_id: &str) -> Result<PageFormat> {
    let format: Option<String> = sqlx::query_scalar(
        "SELECT format FROM app_pages WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(page_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to read the page's format: {e}")))?;
    let format = format.ok_or_else(|| Error::NotFound(format!("Page not found: {page_id}")))?;
    PageFormat::try_from(format).map_err(Error::Other)
}

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
    /// How the page's text is held, which decides how it is edited.
    #[sqlx(try_from = "String")]
    pub format: PageFormat,
}

/// The columns every query returning a `Page` selects.
const PAGE_COLUMNS: &str =
    "id, title, content, icon, icon_color, cover_url, tags, created_at, updated_at, format";

/// A page as created, with what converting its markdown into blocks changed
/// on the way in (a tree page only; never present for markdown).
#[derive(Debug, Clone, Serialize)]
pub struct CreatedPage {
    #[serde(flatten)]
    pub page: Page,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<virtues_document::Problem>,
}

/// Why `PUT /api/pages/:id` refuses `content` for a tree page: its markdown
/// is an export, and writing it back would drop what markdown cannot hold.
pub const TREE_CONTENT_REFUSED: &str =
    "This page holds blocks, so you can't replace its text in one piece. Edit it in the page instead.";

/// Why a tree page is refused while the flag is off.
const TREE_PAGES_OFF: &str = "Block pages aren't turned on for this server.";

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
    /// The format to create the page in. Absent: the server's default
    /// (`default_page_format`). `tree` is refused while the flag is off.
    #[serde(default)]
    pub format: Option<PageFormat>,
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

/// A page version and what it says, for History's preview and restore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageVersionDetail {
    pub id: String,
    pub page_id: String,
    pub version_number: i64,
    /// The base64 Yjs state, for a markdown page's editor to put back.
    /// `None` for a tree version: the server puts a tree page back
    /// (`restore_version`), never a client from its own copy.
    pub snapshot: Option<String>,
    pub content_preview: Option<String>,
    pub created_at: Timestamp,
    pub created_by: String,
    pub description: Option<String>,
    /// What the version's state holds, read from its bytes: a tree, or a
    /// markdown page's text. A tree page's history can hold both, its
    /// versions from before it held blocks being markdown.
    pub format: PageFormat,
    /// The version's markdown: the text, or the tree's export. Empty when
    /// the state does not read.
    pub markdown: String,
    /// A tree version's canonical HTML without block ids, for its preview.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
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

/// A version to store as given: the server's own snapshots
/// (`create_version_from_snapshot`), and a markdown page editor's copy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVersionRequest {
    pub snapshot: String, // base64-encoded Yjs snapshot
    pub content_preview: String,
    pub description: Option<String>,
    pub created_by: String,
}

/// A version a client asks for (`POST /api/pages/:id/versions`). Without
/// `snapshot` the server cuts it from the page as it is now, either format
/// (`cut_version_now`). With one, a markdown page's editor sends its own
/// copy; a tree page refuses that (`post_version`).
#[derive(Debug, Clone, Deserialize)]
pub struct NewVersionRequest {
    /// The base64 Yjs state the editor holds.
    #[serde(default)]
    pub snapshot: Option<String>,
    /// The start of its text; read from `snapshot` when absent.
    #[serde(default)]
    pub content_preview: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub created_by: String,
}

/// Why a tree page takes no version from a client: a client's copy would go
/// into History unchecked, and the server cuts a tree page's versions from
/// the document it checks every write against.
pub const TREE_SNAPSHOT_REFUSED: &str = "Your server keeps this page's versions itself.";

/// Why the server does not restore a markdown page: its editor puts a
/// version back through its own copy of the text.
pub const RESTORE_IN_EDITOR: &str = "Restore this page from its editor.";

/// What putting a version back did (`restore_version`).
#[derive(Debug, Clone, Serialize)]
pub struct Restored {
    /// The version that records the restore, "Put back vN", the owner's.
    /// `None` when the page already read as the version, or when History
    /// could not take the entry (the page is restored either way).
    pub version_number: Option<i64>,
    /// Whether the page changed; false when it already read as the version.
    pub changed: bool,
    /// What converting a version from before the page held blocks changed.
    pub notes: Vec<virtues_document::Problem>,
}

/// Markdown as the blocks it makes (`convert_markdown`).
#[derive(Debug, Clone, Serialize)]
pub struct Converted {
    /// Canonical HTML of the blocks, without block ids: the editor gives
    /// each block its own when it inserts them.
    pub html: String,
    /// What the conversion changed on the way in.
    pub notes: Vec<virtues_document::Problem>,
}

/// Why a paste is too large to convert: one conversion reads at most
/// `virtues_document::MAX_INPUT_BYTES`.
pub const PASTE_TOO_LARGE: &str = "That's more markdown than one paste converts. Paste it in parts.";

/// How many markdown conversions run at once. One holds up to about half a
/// gigabyte (`virtues_document::migrate::MAX_EVENTS`) on a box whose memory
/// the on-device models share, so they take turns.
const CONVERSIONS_AT_ONCE: usize = 1;

/// The longest a conversion waits for its turn and runs, together. One at
/// the size limit takes well under a second.
const CONVERSION_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);

static CONVERSIONS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(CONVERSIONS_AT_ONCE);

/// When a conversion did not get its turn before the deadline.
const CONVERSION_BUSY: &str =
    "Your server is converting other markdown and couldn't get to this in time. Try again in a moment.";

/// When a conversion did not finish before the deadline.
const CONVERSION_TOO_SLOW: &str =
    "Your server couldn't convert this markdown in time. Try a smaller piece of it.";

/// Run a conversion of markdown into blocks (`virtues_document::parse_markdown`
/// and what is built from its tree) as every server path that converts
/// runs one: off the async workers, on a blocking thread, one conversion at
/// a time, and given up on past [`CONVERSION_DEADLINE`]. A conversion given
/// up on keeps its turn until it ends, so ones that run long cannot pile up.
pub async fn convert_off_workers<T: Send + 'static>(
    convert: impl FnOnce() -> T + Send + 'static,
) -> Result<T> {
    convert_taking_turns(&CONVERSIONS, CONVERSION_DEADLINE, convert).await
}

/// [`convert_off_workers`], with its turns and its deadline named.
async fn convert_taking_turns<T: Send + 'static>(
    turns: &'static tokio::sync::Semaphore,
    within: std::time::Duration,
    convert: impl FnOnce() -> T + Send + 'static,
) -> Result<T> {
    let deadline = tokio::time::Instant::now() + within;
    let turn = match tokio::time::timeout_at(deadline, turns.acquire()).await {
        Ok(Ok(turn)) => turn,
        Ok(Err(e)) => return Err(Error::Other(format!("Your server couldn't convert the markdown: {e}"))),
        Err(_) => return Err(Error::Other(CONVERSION_BUSY.into())),
    };
    let task = tokio::task::spawn_blocking(move || {
        let _turn = turn;
        convert()
    });
    match tokio::time::timeout_at(deadline, task).await {
        Ok(Ok(converted)) => Ok(converted),
        Ok(Err(e)) => Err(Error::Other(format!("Your server couldn't convert the markdown: {e}"))),
        Err(_) => {
            tracing::error!("a markdown conversion ran past its deadline");
            Err(Error::Other(CONVERSION_TOO_SLOW.into()))
        }
    }
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
    let page = sqlx::query_as::<_, Page>(&format!(
        "SELECT {PAGE_COLUMNS} FROM app_pages WHERE id = $1 AND deleted_at IS NULL"
    ))
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

/// Create a new page, in the format the request names or the server's
/// default (`create_page_reporting`). For callers that do not show what a
/// tree page's conversion noted.
pub async fn create_page(pool: &PgPool, req: CreatePageRequest) -> Result<Page> {
    Ok(create_page_reporting(pool, req).await?.page)
}

/// Create a new page and report what converting its markdown changed.
///
/// The one gate on the format, shared by every door that creates a page
/// (`POST /api/pages`, the model's `create_page`, `virtues page new`): the
/// request's format, or `default_page_format` when it names none; `tree`
/// is refused while the flag is off.
pub async fn create_page_reporting(pool: &PgPool, req: CreatePageRequest) -> Result<CreatedPage> {
    let format = match req.format {
        Some(PageFormat::Tree) if !tree_pages_enabled() => {
            return Err(Error::InvalidInput(TREE_PAGES_OFF.into()))
        }
        Some(format) => format,
        None => default_page_format(),
    };
    create_page_as(pool, req, format).await
}

/// Create a new page in `format`, whatever the flag says.
///
/// A tree page's markdown is converted to blocks through the contract's
/// converter and stored as its document, with the export as `content`; a
/// page with no text gets one empty paragraph, since an editor bound to an
/// empty document writes its own into it. Markdown the converter refuses
/// creates nothing.
///
/// If project_id is provided, the page is added to that project.
pub async fn create_page_as(
    pool: &PgPool,
    req: CreatePageRequest,
    format: PageFormat,
) -> Result<CreatedPage> {
    let title = req.title.trim();
    if title.is_empty() {
        return Err(Error::InvalidInput("Page title cannot be empty".into()));
    }

    let (content, yjs_state, notes) = match format {
        PageFormat::Markdown => (req.content.clone(), None, vec![]),
        PageFormat::Tree => {
            let markdown = req.content.clone();
            let (content, state, notes) = convert_off_workers(move || tree_document(&markdown)).await??;
            (content, Some(state), notes)
        }
    };

    // Generate ID using title and current timestamp for uniqueness
    let timestamp = chrono::Utc::now().to_rfc3339();
    let id = generate_id(PAGE_PREFIX, &[title, &timestamp]);

    let page = sqlx::query_as::<_, Page>(&format!(
        "INSERT INTO app_pages (id, title, content, icon, icon_color, cover_url, tags, format, yjs_state) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
         RETURNING {PAGE_COLUMNS}"
    ))
    .bind(&id)
    .bind(title)
    .bind(&content)
    .bind(&req.icon)
    .bind(&req.icon_color)
    .bind(&req.cover_url)
    .bind(req.tags.clone().unwrap_or_else(|| serde_json::json!([])))
    .bind(format.as_str())
    .bind(&yjs_state)
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

    Ok(CreatedPage { page, notes })
}

/// A new tree page's document from its markdown: the export, the state, and
/// what the conversion changed. Every page created from markdown is a
/// writer's, so any suggestion in it comes in accepted
/// (`parse_written_markdown`). A conversion: run it through
/// [`convert_off_workers`].
fn tree_document(markdown: &str) -> Result<(String, Vec<u8>, Vec<virtues_document::Problem>)> {
    use yrs::Transact;
    let converted = virtues_document::parse_written_markdown(markdown);
    if !converted.errors.is_empty() {
        return Err(Error::InvalidInput(format!(
            "Your server couldn't turn the page's markdown into blocks ({}). Fix those parts and create it again.",
            crate::server::yjs::problems_summary(&converted.errors)
        )));
    }
    let doc = virtues_document::doc_from_nodes(at_least_one_block(converted.nodes));
    let txn = doc.transact();
    let tree = virtues_document::read_doc(&txn);
    let content = virtues_document::to_markdown(&tree);
    let state = virtues_document::encode_state(&txn, &yrs::StateVector::default());
    Ok((content, state, converted.notes))
}

/// A page's blocks, with one empty paragraph when there are none: a page
/// holds at least one block (`doc: block+`), and an editor bound to an empty
/// document writes its own paragraph into it.
fn at_least_one_block(mut nodes: Vec<virtues_document::Node>) -> Vec<virtues_document::Node> {
    if nodes.is_empty() {
        nodes.push(virtues_document::Node::element(
            "paragraph",
            Default::default(),
            vec![],
        ));
    }
    nodes
}

/// Update an existing page.
///
/// `content` is written only when the request sends it, so a title-only
/// update never writes back a copy of the text read before a save that
/// landed meanwhile. A tree page refuses `content` with nothing written: its
/// text is an export of its document, edited in the page.
pub async fn update_page(pool: &PgPool, id: &str, req: UpdatePageRequest) -> Result<Page> {
    // Verify page exists
    let existing = get_page(pool, id).await?;
    if existing.format == PageFormat::Tree && req.content.is_some() {
        return Err(Error::InvalidInput(TREE_CONTENT_REFUSED.into()));
    }

    let title = req.title.as_deref().unwrap_or(&existing.title);
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

    // Checked again in the write: a page that became a tree since it was
    // read above takes no `content` either.
    let page = sqlx::query_as::<_, Page>(&format!(
        "UPDATE app_pages \
         SET title = $2, content = COALESCE($3, content), icon = $4, icon_color = $5, \
             cover_url = $6, tags = $7 \
         WHERE id = $1 AND ($3 IS NULL OR format = 'markdown') \
         RETURNING {PAGE_COLUMNS}"
    ))
    .bind(id)
    .bind(title.trim())
    .bind(req.content.as_deref())
    .bind(icon)
    .bind(icon_color)
    .bind(cover_url)
    .bind(tags)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update page: {}", e)))?;

    match page {
        Some(page) => Ok(page),
        None if req.content.is_some() => Err(Error::InvalidInput(TREE_CONTENT_REFUSED.into())),
        None => Err(Error::NotFound(format!("Page not found: {}", id))),
    }
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
#[derive(Debug, Clone, PartialEq)]
pub struct RestorePoint {
    pub version_number: i64,
    /// The text it holds: the page as `cut_restore_point` read it, a tree
    /// page's export.
    pub text: String,
    /// The tree it holds, for a tree page; `None` for markdown.
    pub tree: Option<Vec<virtues_document::Node>>,
}

impl RestorePoint {
    /// Whether this version holds exactly what `written` says. A tree is
    /// compared as a tree, block ids included: the export carries no ids and
    /// does not read back to the same tree in every case, so two trees with
    /// one export can differ (an id, an image's width), and that difference
    /// is still a change worth keeping.
    pub fn holds(&self, written: &crate::server::yjs::Written) -> bool {
        match (&self.tree, &written.tree) {
            (Some(kept), Some(now)) => kept == now,
            (None, None) => self.text == written.text,
            _ => false,
        }
    }
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
        // A snapshot that does not read holds nothing this page says.
        if let Ok(then) = crate::server::yjs::page_text_of_state(&snapshot) {
            let latest = RestorePoint {
                version_number: number,
                text: then.markdown,
                tree: then.tree,
            };
            if latest.holds(&now) {
                return Ok(latest);
            }
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
        tree: now.tree,
    })
}

/// Keep a page's first draft as its first version, when it has no version
/// yet: a restore point, so History has the text every later edit is diffed
/// against, and putting it back undoes them.
///
/// For a draft the server has as markdown rather than in a live doc: an
/// article as it is created, or as the editor last wrote it when nothing
/// versioned it then. Only articles call it, and the table keeps articles
/// markdown, so the draft is a Y.Text. The state is built from the text
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

/// Cut the version a client asks for (`POST /api/pages/:id/versions`).
///
/// Without a snapshot the server cuts it from the page as it is now, for
/// either format (`cut_version_now`). A snapshot is a markdown page
/// editor's own copy, stored as sent; a tree page refuses one with nothing
/// stored, since its versions come only from the document the server checks
/// every write against. The refusal is here and not in `create_version`,
/// which the server's own tree versions go through.
pub async fn post_version(
    pool: &PgPool,
    yjs: &crate::server::yjs::YjsState,
    page_id: &str,
    req: NewVersionRequest,
) -> Result<PageVersionSummary> {
    let Some(snapshot) = req.snapshot else {
        return cut_version_now(pool, yjs, page_id, &req.created_by, req.description.as_deref())
            .await;
    };
    if page_format(pool, page_id).await? == PageFormat::Tree {
        return Err(Error::InvalidInput(TREE_SNAPSHOT_REFUSED.into()));
    }
    // A snapshot that does not decode is refused by `create_version`.
    let content_preview = req.content_preview.unwrap_or_else(|| {
        BASE64
            .decode(&snapshot)
            .map(|bytes| {
                crate::server::yjs::extract_text_content(&bytes)
                    .chars()
                    .take(500)
                    .collect()
            })
            .unwrap_or_default()
    });
    create_version(
        pool,
        page_id,
        CreateVersionRequest {
            snapshot,
            content_preview,
            description: req.description,
            created_by: req.created_by,
        },
    )
    .await
}

/// Cut a version of the page as it is now, from the server's live doc:
/// History's Save and an open editor's autosave on a tree page, which sends
/// no copy of its own. Either format.
///
/// Taken in the page's write turn, so a machine edit in progress finishes
/// and versions its change as its own first; cut in the middle of one, the
/// change would be credited to whoever asked for this version. The turn is
/// not reentrant: a caller already holding it cuts with `cut_version`.
pub async fn cut_version_now(
    pool: &PgPool,
    yjs: &crate::server::yjs::YjsState,
    page_id: &str,
    created_by: &str,
    description: Option<&str>,
) -> Result<PageVersionSummary> {
    get_page(pool, page_id).await?;
    let _turn = yjs.write_turn(page_id).await;
    let now = yjs
        .text_and_state(page_id)
        .await
        .map_err(|e| Error::Other(format!("Your server couldn't read the page: {e}")))?;
    create_version_from_snapshot(pool, page_id, &now.state, &now.text, created_by, description)
        .await
}

/// What a version holding the owner's typing says, cut when that typing
/// reached the page between its restore point and a restore over it.
pub(crate) const TYPED_BEFORE_RESTORE: &str = "Auto-saved (before a restore)";

/// Put a tree page back as one of its versions, on the server.
///
/// The target is the version's tree, or, for a version from before the page
/// held blocks, its markdown converted through the contract's converter,
/// whose notes come back. In the page's write turn: the page as it stands
/// is kept first (`cut_restore_point`), so the restore can be undone; the
/// page is then turned into the version block by block
/// (`YjsState::replace_tree`), so a block the two share keeps its Yjs items
/// and any typing in it; typing an editor sent while the restore point was
/// being cut is kept as the owner's own version; and the restore is
/// versioned as the owner's, "Put back vN", as History labels a markdown
/// page's restore. A page that already reads as the version is not written
/// and gains no version.
///
/// A version of another page is `NotFound`. A markdown page is refused: its
/// editor restores it from its own copy of the text. Takes the write turn
/// itself, so a caller must not hold it.
pub async fn restore_version(
    pool: &PgPool,
    yjs: &crate::server::yjs::YjsState,
    page_id: &str,
    version_id: &str,
) -> Result<Restored> {
    use crate::server::yjs::{TextWriteError, TreeError};

    let row: Option<(String, i64, Option<Vec<u8>>)> = sqlx::query_as(
        "SELECT page_id, version_number, yjs_snapshot FROM app_page_versions WHERE id = $1",
    )
    .bind(version_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to read the version: {e}")))?;
    let Some((_, number, snapshot)) = row.filter(|(owner, ..)| owner == page_id) else {
        return Err(Error::NotFound(format!("Version not found: {version_id}")));
    };
    if page_format(pool, page_id).await? != PageFormat::Tree {
        return Err(Error::InvalidInput(RESTORE_IN_EDITOR.into()));
    }
    let snapshot = snapshot.ok_or_else(|| {
        Error::InvalidInput(format!(
            "Version {number} has no copy of the page, so your server can't put it back. Choose another version."
        ))
    })?;
    let then = crate::server::yjs::page_text_of_state(&snapshot)
        .map_err(|e| Error::Other(format!("Your server couldn't read version {number}: {e}")))?;
    // A version with no block, or no text from before the page held blocks,
    // would put a blank page over every block this one holds. No version a
    // block page cut holds no block, so such a version is a state read as
    // something it is not, or a page saved empty before it held blocks;
    // either way the page is left as it is.
    let empty = || {
        Error::InvalidInput(format!(
            "Version {number} is empty, and putting it back would clear this page, so your server left the page as it is. Choose another version."
        ))
    };
    let (target, notes) = match then.tree {
        Some(tree) if tree.is_empty() => return Err(empty()),
        Some(tree) => (tree, vec![]),
        None if then.markdown.trim().is_empty() => return Err(empty()),
        None => {
            let markdown = then.markdown.clone();
            let converted =
                convert_off_workers(move || virtues_document::parse_markdown(&markdown)).await?;
            if !converted.errors.is_empty() {
                return Err(Error::InvalidInput(format!(
                    "Your server couldn't turn version {number} into blocks ({}). Choose another version.",
                    crate::server::yjs::problems_summary(&converted.errors)
                )));
            }
            (at_least_one_block(converted.nodes), converted.notes)
        }
    };
    let unchanged = Restored {
        version_number: None,
        changed: false,
        notes: notes.clone(),
    };

    let _turn = yjs.write_turn(page_id).await;
    let current = yjs.read_tree(page_id).await.map_err(|e| match e {
        TreeError::NotTree => Error::InvalidInput(RESTORE_IN_EDITOR.into()),
        e => Error::Other(e.to_string()),
    })?;
    if virtues_document::diff_ops(&current, &target).is_empty() {
        return Ok(unchanged);
    }
    let kept = cut_restore_point(pool, yjs, page_id).await?;
    let edit = match yjs.replace_tree(page_id, &target).await {
        Ok(Some(edit)) => edit,
        Ok(None) => return Ok(unchanged),
        Err(e) => return Err(Error::Other(e.to_string())),
    };
    let after = match edit.after {
        Ok(written) => written,
        // On the page and in every open editor; the save loop retries it.
        Err(TextWriteError::NotSaved { written, error }) => {
            tracing::warn!(page = %page_id, %error, "a restore is on the page but not saved yet");
            written
        }
        Err(e) => return Err(Error::Other(e.to_string())),
    };
    // Typing that landed after the restore point was cut and before the
    // restore wrote over it is the owner's, and in no version yet: kept as
    // theirs, so putting the version back loses none of it.
    if !kept.holds(&edit.before) {
        cut_version(pool, page_id, &edit.before, "auto", Some(TYPED_BEFORE_RESTORE)).await;
    }
    let version_number = cut_version(
        pool,
        page_id,
        &after,
        "user",
        Some(&format!("Put back v{number}")),
    )
    .await;
    Ok(Restored {
        version_number,
        changed: true,
        notes,
    })
}

/// Markdown as the blocks it makes, for a paste into a tree page and the
/// inline writer's output: through the contract's converter, the one the
/// server's own writers use, so a paste reads markdown exactly as they do.
/// A suggestion in it comes in accepted: suggestions are made in the page,
/// never brought into it (`parse_written_markdown`). A person's `paste` is
/// read as one (`parse_pasted_markdown`): a raw tag that is no HTML and
/// CriticMarkup stay in its text as written.
pub async fn convert_markdown(markdown: String, paste: bool) -> Result<Converted> {
    if markdown.len() > virtues_document::MAX_INPUT_BYTES {
        return Err(Error::InvalidInput(PASTE_TOO_LARGE.into()));
    }
    convert_off_workers(move || {
        let converted = if paste {
            virtues_document::parse_pasted_markdown(&markdown)
        } else {
            virtues_document::parse_written_markdown(&markdown)
        };
        if converted.errors.iter().any(virtues_document::migrate::past_one_conversion) {
            return Err(Error::InvalidInput(PASTE_TOO_LARGE.into()));
        }
        if !converted.errors.is_empty() {
            return Err(Error::InvalidInput(format!(
                "Your server couldn't turn this markdown into blocks ({}). Fix those parts and paste it again.",
                crate::server::yjs::problems_summary(&converted.errors)
            )));
        }
        Ok(Converted {
            html: virtues_document::to_html(&converted.nodes, false),
            notes: converted.notes,
        })
    })
    .await?
}

/// A version and what it says. Its format is read from its bytes: a tree
/// version comes back as its export and preview HTML with no snapshot, so an
/// editor cannot write it back into the page itself; a markdown version
/// keeps its snapshot for the markdown editor's restore.
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

    let read = row.yjs_snapshot.as_deref().map(crate::server::yjs::page_text_of_state);
    let (format, markdown, html) = match read {
        Some(Ok(crate::server::yjs::PageText {
            markdown,
            tree: Some(tree),
        })) => {
            let html = virtues_document::to_html(&tree, false);
            (PageFormat::Tree, markdown, Some(html))
        }
        Some(Ok(text)) => (PageFormat::Markdown, text.markdown, None),
        Some(Err(error)) => {
            tracing::warn!(version = %row.id, %error, "a page version's state does not read");
            (PageFormat::Markdown, String::new(), None)
        }
        None => (PageFormat::Markdown, String::new(), None),
    };
    let snapshot = match format {
        PageFormat::Tree => None,
        PageFormat::Markdown => row.yjs_snapshot.map(|bytes| BASE64.encode(&bytes)),
    };

    Ok(PageVersionDetail {
        id: row.id,
        page_id: row.page_id,
        version_number: row.version_number,
        snapshot,
        content_preview: row.content_preview,
        created_at: row.created_at,
        created_by: row.created_by,
        description: row.description,
        format,
        markdown,
        html,
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
                format: None,
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

    // ── formats: the table, creation, updates, restore points ──────────────

    fn request(content: &str) -> CreatePageRequest {
        CreatePageRequest {
            title: "Trip".into(),
            content: content.into(),
            project_id: None,
            icon: None,
            icon_color: None,
            cover_url: None,
            tags: None,
            format: None,
        }
    }

    async fn tree_page(pool: &PgPool, markdown: &str) -> CreatedPage {
        create_page_as(pool, request(markdown), PageFormat::Tree).await.unwrap()
    }

    fn refused_by(result: std::result::Result<sqlx::postgres::PgQueryResult, sqlx::Error>, check: &str) {
        let err = result.expect_err(check).to_string();
        assert!(err.contains(check), "{check}: {err}");
    }

    /// Rows from before migration 0048 read as markdown pages, untouched.
    #[sqlx::test(migrations = false)]
    async fn pages_from_before_the_format_read_as_markdown(pool: PgPool) {
        use crate::database::EMBEDDED_MIGRATIONS;
        let before = sqlx::migrate::Migrator {
            migrations: std::borrow::Cow::Owned(
                EMBEDDED_MIGRATIONS.iter().filter(|m| m.version < 48).cloned().collect(),
            ),
            ignore_missing: false,
            locking: true,
            no_tx: false,
        };
        before.run(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO app_pages (id, title, content, updated_at) \
             VALUES ('page_old', 'Old', 'Old text.', '2026-01-02T03:04:05Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

        EMBEDDED_MIGRATIONS.run(&pool).await.unwrap();
        let (format, updated_at): (String, Timestamp) =
            sqlx::query_as("SELECT format, updated_at FROM app_pages WHERE id = 'page_old'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(format, "markdown");
        assert_eq!(
            updated_at.into_inner(),
            "2026-01-02T03:04:05Z".parse::<chrono::DateTime<chrono::Utc>>().unwrap(),
            "no row was rewritten"
        );
        assert_eq!(page_format(&pool, "page_old").await.unwrap(), PageFormat::Markdown);
    }

    /// The table makes the states no load could open impossible: a tree
    /// without its document, an article as a tree, a format nobody reads.
    #[sqlx::test]
    async fn the_table_refuses_a_tree_row_it_could_not_open(pool: PgPool) {
        let id = page(&pool, "Text.\n").await;
        refused_by(
            sqlx::query("UPDATE app_pages SET format = 'tree' WHERE id = $1")
                .bind(&id)
                .execute(&pool)
                .await,
            "app_pages_tree_has_document",
        );
        refused_by(
            sqlx::query("UPDATE app_pages SET format = 'x', yjs_state = '\\x00' WHERE id = $1")
                .bind(&id)
                .execute(&pool)
                .await,
            "app_pages_format_check",
        );
        refused_by(
            sqlx::query(
                "INSERT INTO app_pages (id, title, kind, format, yjs_state) \
                 VALUES ('page_article', 'Nick', 'article', 'tree', '\\x00')",
            )
            .execute(&pool)
            .await,
            "app_pages_tree_is_a_user_page",
        );
        let tree = tree_page(&pool, "Lunch.\n").await.page.id;
        refused_by(
            sqlx::query("UPDATE app_pages SET yjs_state = NULL WHERE id = $1")
                .bind(&tree)
                .execute(&pool)
                .await,
            "app_pages_tree_has_document",
        );
    }

    #[sqlx::test]
    async fn a_tree_page_is_created_from_markdown(pool: PgPool) {
        let markdown = "---\ntitle: Trip\n---\n\n## Plan\n\nLunch with [@Nick](/person/person_1).\n\n\
            ![photo.jpg](/api/drive/files/df_1/download)\n\n- [ ] Book seats\n- [x] Pack\n\n\
            | Day | Where |\n| --- | --- |\n| Friday | Lisbon |\n";
        let created = tree_page(&pool, markdown).await;
        assert_eq!(created.page.format, PageFormat::Tree);
        assert!(
            created.notes.iter().any(|n| n.message.contains("front matter")),
            "{:?}",
            created.notes
        );

        let state: Vec<u8> = sqlx::query_scalar("SELECT yjs_state FROM app_pages WHERE id = $1")
            .bind(&created.page.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let doc = {
            use yrs::{updates::decoder::Decode, Transact};
            let doc = virtues_document::new_doc();
            doc.transact_mut()
                .apply_update(yrs::Update::decode_v1(&state).unwrap())
                .unwrap();
            doc
        };
        let tree = {
            use yrs::Transact;
            let txn = doc.transact();
            assert_eq!(virtues_document::stamped_version(&txn), Some(1));
            assert_eq!(virtues_document::validate_doc(&txn), []);
            virtues_document::read_doc(&txn)
        };
        assert_eq!(created.page.content, virtues_document::to_markdown(&tree));
        let kinds: Vec<&str> = tree.iter().map(|n| n.kind.as_str()).collect();
        assert_eq!(kinds, ["heading", "paragraph", "image", "taskList", "table"]);
        let mut blocks = 0;
        for n in &tree {
            n.walk(&mut |n| {
                if virtues_document::contract().node(&n.kind).is_some_and(|s| s.spec.id) {
                    blocks += 1;
                    assert!(n.id().is_some(), "{} has no id", n.kind);
                }
            });
        }
        assert!(blocks > kinds.len(), "nested blocks counted too");
        assert!(created.page.content.contains("[@Nick](/person/person_1)"));
        assert_eq!(get_page(&pool, &created.page.id).await.unwrap().format, PageFormat::Tree);

        // No text: one empty paragraph, never an empty document.
        let empty = tree_page(&pool, "").await;
        let yjs = crate::server::yjs::YjsState::new(pool.clone());
        let tree = yjs.read_tree(&empty.page.id).await.unwrap();
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].kind, "paragraph");
        assert!(tree[0].id().is_some());
        assert_eq!(empty.page.content, virtues_document::to_markdown(&tree));
    }

    /// The flag gates every door that creates a page, in one place. Tests
    /// never set it, so it is off here as on every real box.
    #[sqlx::test]
    async fn a_tree_page_is_refused_while_the_flag_is_off(pool: PgPool) {
        if tree_pages_enabled() {
            eprintln!("VIRTUES_TREE_PAGES is set; the flag-off case cannot run here");
            return;
        }
        assert_eq!(default_page_format(), PageFormat::Markdown);
        let mut tree = request("Lunch.\n");
        tree.format = Some(PageFormat::Tree);
        let err = create_page_reporting(&pool, tree.clone()).await.unwrap_err();
        assert!(matches!(&err, Error::InvalidInput(m) if m == TREE_PAGES_OFF), "{err}");
        assert!(create_page(&pool, tree).await.is_err());
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app_pages")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "nothing created");

        let created = create_page_reporting(&pool, request("Lunch.\n")).await.unwrap();
        assert_eq!(created.page.format, PageFormat::Markdown);
        assert!(created.notes.is_empty());
        let mut markdown = request("Lunch.\n");
        markdown.format = Some(PageFormat::Markdown);
        assert_eq!(create_page(&pool, markdown).await.unwrap().format, PageFormat::Markdown);
    }

    #[sqlx::test]
    async fn an_update_never_writes_text_over_a_tree_or_a_newer_save(pool: PgPool) {
        let tree = tree_page(&pool, "Lunch.\n").await.page;
        let before: (String, Vec<u8>) =
            sqlx::query_as("SELECT content, yjs_state FROM app_pages WHERE id = $1")
                .bind(&tree.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let err = update_page(
            &pool,
            &tree.id,
            UpdatePageRequest {
                title: Some("Trip to Lisbon".into()),
                content: Some("Overwritten.\n".into()),
                icon: None,
                icon_color: None,
                cover_url: None,
                tags: None,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(&err, Error::InvalidInput(m) if m == TREE_CONTENT_REFUSED), "{err}");
        let after: (String, Vec<u8>, String) =
            sqlx::query_as("SELECT content, yjs_state, title FROM app_pages WHERE id = $1")
                .bind(&tree.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!((after.0, after.1), before, "nothing written");
        assert_eq!(after.2, "Trip", "not even the title");

        // A title alone renames a tree page.
        let renamed = update_page(
            &pool,
            &tree.id,
            UpdatePageRequest {
                title: Some("Trip to Lisbon".into()),
                content: None,
                icon: None,
                icon_color: None,
                cover_url: None,
                tags: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(renamed.title, "Trip to Lisbon");
        assert_eq!(renamed.content, "Lunch.\n");

        // A title-only update of a markdown page leaves a save that landed
        // after the row was read: the update writes no `content`.
        let markdown = page(&pool, "Old.\n").await;
        let mut hold = pool.begin().await.unwrap();
        sqlx::query("SELECT 1 FROM app_pages WHERE id = $1 FOR UPDATE")
            .bind(&markdown)
            .execute(&mut *hold)
            .await
            .unwrap();
        let rename = tokio::spawn({
            let (pool, id) = (pool.clone(), markdown.clone());
            async move {
                update_page(
                    &pool,
                    &id,
                    UpdatePageRequest {
                        title: Some("Renamed".into()),
                        content: None,
                        icon: None,
                        icon_color: None,
                        cover_url: None,
                        tags: None,
                    },
                )
                .await
            }
        });
        until_a_statement_waits(&pool, "%UPDATE app_pages%").await;
        sqlx::query("UPDATE app_pages SET content = 'Saved meanwhile.\n' WHERE id = $1")
            .bind(&markdown)
            .execute(&mut *hold)
            .await
            .unwrap();
        hold.commit().await.unwrap();
        let renamed = rename.await.unwrap().unwrap();
        assert_eq!(renamed.title, "Renamed");
        assert_eq!(renamed.content, "Saved meanwhile.\n");
    }

    /// "Already kept" compares trees, block ids included: an id or an
    /// image's width is a change the export does not show.
    #[sqlx::test]
    async fn a_tree_restore_point_is_cut_for_any_change_to_the_tree(pool: PgPool) {
        use yrs::{Transact, WriteTxn, Xml, XmlFragment, XmlOut};
        let page_id = tree_page(&pool, "Lunch.\n\n![photo.jpg](https://images.example.com/a.jpg)\n")
            .await
            .page
            .id;
        let yjs = crate::server::yjs::YjsState::new(pool.clone());

        let kept = cut_restore_point(&pool, &yjs, &page_id).await.unwrap();
        assert!(kept.tree.is_some());
        assert_eq!(kept.text, "Lunch.\n\n![photo.jpg](https://images.example.com/a.jpg)\n");
        let again = cut_restore_point(&pool, &yjs, &page_id).await.unwrap();
        assert_eq!(again.version_number, kept.version_number, "nothing changed, nothing cut");

        let set = |block: u32, name: &'static str, value: yrs::Any| {
            let yjs = yjs.clone();
            let page_id = page_id.clone();
            async move {
                let doc = yjs.doc_cache.get_or_create(&page_id, &yjs.pool).await.unwrap();
                let doc = doc.write().await;
                let mut txn = doc.doc.transact_mut();
                let frag = txn.get_or_insert_xml_fragment("doc");
                let Some(XmlOut::Element(el)) = frag.get(&txn, block) else {
                    panic!("a block")
                };
                el.insert_attribute(&mut txn, name, value);
            }
        };

        set(0, "id", yrs::Any::from("renamed1")).await;
        let after_id = cut_restore_point(&pool, &yjs, &page_id).await.unwrap();
        assert!(after_id.version_number > kept.version_number, "an id-only change");
        assert_eq!(after_id.text, kept.text, "the export did not change");

        set(1, "width", yrs::Any::from(600i64)).await;
        let after_width = cut_restore_point(&pool, &yjs, &page_id).await.unwrap();
        assert!(after_width.version_number > after_id.version_number, "a width change");
        assert_eq!(
            cut_restore_point(&pool, &yjs, &page_id).await.unwrap().version_number,
            after_width.version_number
        );
    }

    // ── block pages: references, versions, restore, paste ─────────────────

    fn block_ids(tree: &[virtues_document::Node]) -> Vec<String> {
        tree.iter().map(|n| n.id().expect("a block id").to_string()).collect()
    }

    fn texts(tree: &[virtues_document::Node]) -> Vec<String> {
        tree.iter().map(virtues_document::Node::text_content).collect()
    }

    fn asked(created_by: &str, snapshot: Option<&[u8]>) -> NewVersionRequest {
        NewVersionRequest {
            snapshot: snapshot.map(|s| BASE64.encode(s)),
            content_preview: None,
            description: Some("Saved".into()),
            created_by: created_by.into(),
        }
    }

    /// Every version of a page, oldest first: who cut it, why, and its state.
    async fn versions_of(pool: &PgPool, page_id: &str) -> Vec<(i64, String, Option<String>, Vec<u8>)> {
        sqlx::query_as(
            "SELECT version_number, created_by, description, yjs_snapshot FROM app_page_versions \
             WHERE page_id = $1 ORDER BY version_number",
        )
        .bind(page_id)
        .fetch_all(pool)
        .await
        .unwrap()
    }

    fn tree_of(state: &[u8]) -> Vec<virtues_document::Node> {
        crate::server::yjs::page_text_of_state(state)
            .unwrap()
            .tree
            .expect("a tree")
    }

    /// References read `content`, which holds a block page's export, and a
    /// mention exports as the link they look for.
    #[sqlx::test]
    async fn a_block_page_is_found_by_the_page_it_mentions(pool: PgPool) {
        let trip = page(&pool, "The trip.\n").await;
        let notes = tree_page(&pool, &format!("## Notes\n\nLunch plans, see [@Trip](/page/{trip}).\n"))
            .await
            .page
            .id;
        let found = get_page_backlinks(&pool, &trip).await.unwrap();
        assert_eq!(found.backlinks.len(), 1);
        assert_eq!(found.backlinks[0].id, notes);
        assert_eq!(found.backlinks[0].snippet, "Lunch plans, see @Trip.");
    }

    /// A version asked for without a copy is cut by the server from the
    /// live document, for either format, edits not yet saved included.
    #[sqlx::test]
    async fn a_version_asked_for_without_a_copy_holds_the_live_page(pool: PgPool) {
        let yjs = YjsState::new(pool.clone());

        let markdown = page(&pool, "Coffee.\n").await;
        let v = post_version(&pool, &yjs, &markdown, asked("user", None)).await.unwrap();
        assert_eq!(v.version_number, 1);
        assert_eq!(v.created_by, "user");
        assert_eq!(v.description.as_deref(), Some("Saved"));
        assert_eq!(v.content_preview.as_deref(), Some("Coffee.\n"));
        let kept = &versions_of(&pool, &markdown).await[0].3;
        assert_eq!(crate::server::yjs::extract_text_content(kept), "Coffee.\n");

        let tree = tree_page(&pool, "## Plan\n\nLunch.\n").await.page.id;
        // Queued for the save loop, which no test runs: on the page, not saved.
        yjs.append_markdown(&tree, "Typed after.\n").await.unwrap();
        let v = post_version(&pool, &yjs, &tree, asked("auto", None)).await.unwrap();
        assert_eq!(v.content_preview.as_deref(), Some("## Plan\n\nLunch.\n\nTyped after.\n"));
        let kept = &versions_of(&pool, &tree).await[0].3;
        assert_eq!(tree_of(kept), yjs.read_tree(&tree).await.unwrap());
    }

    /// A client's copy goes into a markdown page's history as sent, and into
    /// a block page's not at all.
    #[sqlx::test]
    async fn a_block_page_takes_no_copy_from_a_client(pool: PgPool) {
        let yjs = YjsState::new(pool.clone());
        let tree = tree_page(&pool, "Lunch.\n").await.page.id;
        let state = yjs.text_and_state(&tree).await.unwrap().state;
        let err = post_version(&pool, &yjs, &tree, asked("user", Some(&state)))
            .await
            .unwrap_err();
        assert!(matches!(&err, Error::InvalidInput(m) if m == TREE_SNAPSHOT_REFUSED), "{err}");
        assert!(versions_of(&pool, &tree).await.is_empty(), "nothing stored");

        let markdown = page(&pool, "Text.\n").await;
        let copy = crate::server::yjs::state_from_text("The editor's copy.\n");
        let v = post_version(&pool, &yjs, &markdown, asked("auto", Some(&copy))).await.unwrap();
        assert_eq!(v.content_preview.as_deref(), Some("The editor's copy.\n"), "read from the copy");
        let mut with_preview = asked("user", Some(&copy));
        with_preview.content_preview = Some("As sent".into());
        let v = post_version(&pool, &yjs, &markdown, with_preview).await.unwrap();
        assert_eq!(v.content_preview.as_deref(), Some("As sent"));
        let kept = versions_of(&pool, &markdown).await;
        assert_eq!((kept.len(), &kept[0].3), (2, &copy), "stored as sent");
    }

    /// A tree version comes back as its export and preview HTML, never as a
    /// snapshot an editor could write into the page; a markdown version
    /// keeps its snapshot.
    #[sqlx::test]
    async fn a_version_says_what_it_holds(pool: PgPool) {
        let yjs = YjsState::new(pool.clone());
        let tree = tree_page(&pool, "## Plan\n\nLunch with [@Nick](/person/person_1).\n")
            .await
            .page
            .id;
        let v = cut_version_now(&pool, &yjs, &tree, "user", None).await.unwrap();
        let detail = get_version(&pool, &v.id).await.unwrap();
        assert_eq!(detail.format, PageFormat::Tree);
        assert_eq!(detail.markdown, "## Plan\n\nLunch with [@Nick](/person/person_1).\n");
        let html = detail.html.clone().expect("preview html");
        assert_eq!(html, virtues_document::to_html(&yjs.read_tree(&tree).await.unwrap(), false));
        assert!(!html.contains("data-id"), "{html}");
        assert!(detail.snapshot.is_none());
        let json = serde_json::to_value(&detail).unwrap();
        assert_eq!((json["format"].as_str(), json["snapshot"].is_null()), (Some("tree"), true));

        let markdown = page(&pool, "Coffee.\n").await;
        let v = cut_version_now(&pool, &yjs, &markdown, "user", None).await.unwrap();
        let detail = get_version(&pool, &v.id).await.unwrap();
        assert_eq!((detail.format, detail.markdown.as_str()), (PageFormat::Markdown, "Coffee.\n"));
        let snapshot = BASE64.decode(detail.snapshot.as_deref().expect("a snapshot")).unwrap();
        assert_eq!(crate::server::yjs::extract_text_content(&snapshot), "Coffee.\n");
        let json = serde_json::to_value(&detail).unwrap();
        assert!(json.get("html").is_none(), "{json}");
    }

    /// A tree version is put back on the server: the blocks it shares with
    /// the page keep their ids, the page as it stood is kept first, and the
    /// restore is the owner's version after it.
    #[sqlx::test]
    async fn a_tree_version_is_put_back_on_the_server(pool: PgPool) {
        use virtues_document::Op;
        let page_id = tree_page(&pool, "## Plan\n\nLunch on Friday.\n\nOld line.\n").await.page.id;
        let yjs = YjsState::new(pool.clone());
        let v1 = cut_version_now(&pool, &yjs, &page_id, "user", Some("Saved")).await.unwrap();
        let then = yjs.read_tree(&page_id).await.unwrap();
        let ids = block_ids(&then);

        let ops = [
            Op::Replace {
                id: ids[1].clone(),
                html: "<p>Lunch on Saturday.</p>".into(),
            },
            Op::Delete { id: ids[2].clone() },
            Op::Append {
                html: "<p>New line.</p>".into(),
            },
        ];
        yjs.edit_tree(&page_id, Some(&then), &ops).await.unwrap();
        let edited = yjs.read_tree(&page_id).await.unwrap();

        let restored = restore_version(&pool, &yjs, &page_id, &v1.id).await.unwrap();
        assert!(restored.changed);
        assert!(restored.notes.is_empty(), "{:?}", restored.notes);
        let now = yjs.read_tree(&page_id).await.unwrap();
        assert_eq!(texts(&now), texts(&then));
        assert_eq!(block_ids(&now), ids, "the ids the version holds");
        assert_eq!(now, then);

        let history = versions_of(&pool, &page_id).await;
        let [.., (kept_n, kept_by, kept_why, kept), (put_n, put_by, put_why, put)] = &history[..] else {
            panic!("{} versions", history.len());
        };
        assert_eq!((kept_by.as_str(), kept_why.as_deref()), ("auto", Some(RESTORE_POINT)));
        assert_eq!(tree_of(kept), edited, "the page as it stood");
        assert_eq!((put_by.as_str(), put_why.as_deref()), ("user", Some("Put back v1")));
        assert_eq!(tree_of(put), then);
        assert!(put_n > kept_n);
        assert_eq!(restored.version_number, Some(*put_n));

        let content: String = sqlx::query_scalar("SELECT content FROM app_pages WHERE id = $1")
            .bind(&page_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(content, "## Plan\n\nLunch on Friday.\n\nOld line.\n", "saved at once");
    }

    /// A version from before the page held blocks is markdown; putting it
    /// back converts it and says what the conversion changed.
    #[sqlx::test]
    async fn a_markdown_version_is_put_back_as_blocks(pool: PgPool) {
        let page_id = tree_page(&pool, "Now.\n").await.page.id;
        let yjs = YjsState::new(pool.clone());
        let before = "## Before\n\n> [!CAUTION]\n> Mind the step.\n";
        let old = crate::server::yjs::state_from_text(before);
        let v = create_version_from_snapshot(&pool, &page_id, &old, before, "user", None)
            .await
            .unwrap();

        let restored = restore_version(&pool, &yjs, &page_id, &v.id).await.unwrap();
        assert!(restored.changed);
        assert!(!restored.notes.is_empty(), "the CAUTION alert became a warning");
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let kinds: Vec<&str> = tree.iter().map(|n| n.kind.as_str()).collect();
        assert_eq!(kinds, ["heading", "callout"]);
        assert_eq!(tree[1].attrs["tone"], "warning");
        assert_eq!(texts(&tree), ["Before", "Mind the step."]);
        assert!(tree.iter().all(|n| n.id().is_some()));
    }

    /// Putting back what the page already says writes nothing and cuts no
    /// version.
    #[sqlx::test]
    async fn putting_back_what_the_page_says_writes_nothing(pool: PgPool) {
        use yrs::{ReadTxn, Transact};
        let page_id = tree_page(&pool, "Lunch.\n\nTea.\n").await.page.id;
        let yjs = YjsState::new(pool.clone());
        let v = cut_version_now(&pool, &yjs, &page_id, "user", None).await.unwrap();
        let doc = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        let before = doc.read().await.doc.transact().state_vector();

        let restored = restore_version(&pool, &yjs, &page_id, &v.id).await.unwrap();
        assert!(!restored.changed);
        assert_eq!(restored.version_number, None);
        assert_eq!(doc.read().await.doc.transact().state_vector(), before, "nothing written");
        assert_eq!(versions_of(&pool, &page_id).await.len(), 1, "no version cut");
    }

    /// The editor keeps spaces as typed. A version holding two between
    /// words, a tab and one at the line's end was put back as HTML's
    /// whitespace rules read it, one space each and none at the end, and
    /// putting it back again wrote again and cut another version.
    #[sqlx::test]
    async fn a_version_put_back_keeps_the_spaces_the_editor_typed(pool: PgPool) {
        use virtues_document::Op;
        use yrs::{Text, Transact, WriteTxn, XmlFragment, XmlOut};
        let page_id = tree_page(&pool, "Lunch\n").await.page.id;
        let yjs = YjsState::new(pool.clone());
        {
            let doc = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
            let doc = doc.write().await;
            let mut txn = doc.doc.transact_mut();
            let frag = txn.get_or_insert_xml_fragment("doc");
            let Some(XmlOut::Element(p)) = frag.get(&txn, 0) else {
                panic!("a block")
            };
            let Some(XmlOut::Text(text)) = p.get(&txn, 0) else {
                panic!("its text")
            };
            text.insert(&mut txn, 5, "  with\tNick ");
        }
        let typed = yjs.read_tree(&page_id).await.unwrap();
        assert_eq!(texts(&typed), ["Lunch  with\tNick "]);
        let v = cut_version_now(&pool, &yjs, &page_id, "user", None).await.unwrap();
        let ids = block_ids(&typed);
        let changed = [Op::Replace { id: ids[0].clone(), html: "<p>Changed.</p>".into() }];
        yjs.edit_tree(&page_id, Some(&typed), &changed).await.unwrap();

        let restored = restore_version(&pool, &yjs, &page_id, &v.id).await.unwrap();
        assert!(restored.changed);
        assert_eq!(yjs.read_tree(&page_id).await.unwrap(), typed);
        let cut = versions_of(&pool, &page_id).await.len();
        let again = restore_version(&pool, &yjs, &page_id, &v.id).await.unwrap();
        assert!(!again.changed, "the page already says what the version says");
        assert_eq!(versions_of(&pool, &page_id).await.len(), cut, "no version cut");
    }

    #[sqlx::test]
    async fn a_restore_is_refused_for_what_the_server_does_not_put_back(pool: PgPool) {
        let yjs = YjsState::new(pool.clone());
        let markdown = page(&pool, "Text.\n").await;
        let v = cut_version_now(&pool, &yjs, &markdown, "user", None).await.unwrap();
        let err = restore_version(&pool, &yjs, &markdown, &v.id).await.unwrap_err();
        assert!(matches!(&err, Error::InvalidInput(m) if m == RESTORE_IN_EDITOR), "{err}");

        let tree = tree_page(&pool, "Lunch.\n").await.page.id;
        let err = restore_version(&pool, &yjs, &tree, &v.id).await.unwrap_err();
        assert!(matches!(err, Error::NotFound(_)), "another page's version: {err}");
        let err = restore_version(&pool, &yjs, &tree, "pv_nosuch").await.unwrap_err();
        assert!(matches!(err, Error::NotFound(_)), "{err}");
        assert_eq!(yjs.read_text(&tree).await.unwrap(), "Lunch.\n");
    }

    /// A page created from markdown, or markdown converted for a paste or
    /// the inline writer, holds no suggestion: raw tags in any case and
    /// CriticMarkup come in accepted. Suggestions are the owner's to make.
    #[sqlx::test]
    async fn markdown_from_a_writer_makes_no_suggestion(pool: PgPool) {
        let markdown = "Dinner on <virtues-del proposal=\"p1\">Friday</virtues-del>\
                        <virtues-ins proposal=\"p1\">Saturday</virtues-ins>.\n\n\
                        <VIRTUES-INS proposal=\"x\">Hidden from the shared page</VIRTUES-INS> once.\n\n\
                        The {--quick--}{++slow++} fox.\n";
        let created = tree_page(&pool, markdown).await;
        assert_eq!(
            created.page.content,
            "Dinner on Saturday.\n\nHidden from the shared page once.\n\nThe slow fox.\n"
        );
        let yjs = YjsState::new(pool.clone());
        let tree = yjs.read_tree(&created.page.id).await.unwrap();
        assert!(!crate::tools::holds_proposal(&tree));

        let converted = convert_markdown(markdown.into(), false).await.unwrap();
        assert!(!converted.html.contains("virtues-ins") && !converted.html.contains("virtues-del"));
        assert!(converted.html.contains("Dinner on Saturday."), "{}", converted.html);
        assert!(converted.notes.iter().any(|n| n.message.contains("as accepted")));
    }

    #[tokio::test]
    async fn markdown_converts_to_blocks_without_ids() {
        let converted = convert_markdown(
            "---\ntitle: Trip\n---\n\n## Plan\n\n- [ ] Book seats\n\nLunch with [@Nick](/person/person_1).\n"
                .into(),
            false,
        )
        .await
        .unwrap();
        assert_eq!(
            converted.html,
            "<h2>Plan</h2><ul data-type=\"taskList\"><li data-type=\"taskItem\"><p>Book seats</p></li></ul>\
             <p>Lunch with <virtues-mention to=\"/person/person_1\" label=\"Nick\"></virtues-mention>.</p>"
        );
        assert!(
            converted.notes.iter().any(|n| n.message.contains("front matter")),
            "{:?}",
            converted.notes
        );

        let deep = format!("{}Deep.\n", "> ".repeat(150));
        let err = convert_markdown(deep, false).await.unwrap_err();
        assert!(
            matches!(&err, Error::InvalidInput(m) if m.contains("couldn't turn this markdown into blocks")),
            "{err}"
        );

        let too_large = "a".repeat(virtues_document::MAX_INPUT_BYTES + 1);
        let err = convert_markdown(too_large, true).await.unwrap_err();
        assert!(matches!(&err, Error::InvalidInput(m) if m == PASTE_TOO_LARGE), "{err}");
        // Under the size limit, but more than one conversion holds.
        let parts = "*a ".repeat(virtues_document::MAX_INPUT_BYTES / 3 - 1);
        let err = convert_markdown(parts, false).await.unwrap_err();
        assert!(matches!(&err, Error::InvalidInput(m) if m == PASTE_TOO_LARGE), "{err}");
    }

    /// Conversions take turns, and one that runs past the deadline is given
    /// up on without holding its caller, while it keeps its turn, so the
    /// next one waits and is told the server is busy rather than starting
    /// beside it.
    #[tokio::test]
    async fn conversions_take_turns_and_have_a_deadline() {
        static TURNS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
        let within = std::time::Duration::from_millis(200);
        let (done, finished) = std::sync::mpsc::channel();

        let start = std::time::Instant::now();
        let slow = convert_taking_turns(&TURNS, within, move || {
            std::thread::sleep(std::time::Duration::from_secs(1));
            done.send(()).ok();
        })
        .await;
        assert!(matches!(&slow, Err(Error::Other(m)) if m == CONVERSION_TOO_SLOW), "{slow:?}");
        assert!(start.elapsed() < std::time::Duration::from_millis(900), "the caller waited");

        let next = convert_taking_turns(&TURNS, within, || 1).await;
        assert!(matches!(&next, Err(Error::Other(m)) if m == CONVERSION_BUSY), "{next:?}");

        finished.recv().unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(convert_taking_turns(&TURNS, within, || 1).await.unwrap(), 1);
    }
}

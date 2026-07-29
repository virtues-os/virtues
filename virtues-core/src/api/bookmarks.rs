//! In-app bookmark save — the manual capture door (docs/bookmarks-plan.md).
//!
//! `POST /api/bookmarks` is the zero-dependency way to get a URL into
//! `data_content_bookmark`: the URL box in the web app, chat, and (until the
//! share extension lands) anything else that can reach the box. Rows land
//! through the same shared normalizer as every sync source; what's different
//! here is identity and the note:
//!
//! - **Identity is the canonicalized URL**, not a source GUID — re-saving the
//!   same page upserts (bumps the timestamp, merges tags) instead of
//!   duplicating. Tracking params and fragments don't split identity.
//! - **`note` is written here and only here.** The shared upsert deliberately
//!   never touches it (a sync row's NULL would clobber the user's words), so
//!   the user-authored note lands in its own deliberate UPDATE.
//!
//! No title/content fetch happens at save time — capture must be instant; the
//! enrichment sweep backfills titles and extraction records later.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;
use virtues_helpers::bookmarks::{canonical_url_for_identity, upsert_bookmarks, BookmarkRow};

#[derive(Debug, Clone, Deserialize)]
pub struct SaveBookmarkRequest {
    pub url: String,
    /// User marginalia — the whisper. Optional, never required.
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SavedBookmark {
    pub id: String,
    pub url: String,
    pub title: Option<String>,
    pub note: Option<String>,
    pub tags: Option<serde_json::Value>,
    pub timestamp: crate::types::Timestamp,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct BookmarkListItem {
    pub id: String,
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub note: Option<String>,
    pub source_platform: Option<String>,
    pub bookmark_type: Option<String>,
    pub author: Option<String>,
    pub tags: Option<serde_json::Value>,
    pub thumbnail_url: Option<String>,
    pub timestamp: crate::types::Timestamp,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListBookmarksQuery {
    #[serde(default)]
    pub offset: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub search: Option<String>,
    /// `asc` sorts oldest-first; anything else is newest-first.
    #[serde(default)]
    pub dir: Option<String>,
}

fn default_limit() -> i64 {
    50
}

#[derive(Debug, Clone, Serialize)]
pub struct BookmarkPage {
    pub items: Vec<BookmarkListItem>,
    pub total: i64,
}

/// One page of bookmarks, newest first.
///
/// Tombstoned rows (`deleted_at_source`) are hidden: the user removed the
/// bookmark at its source, so it should leave the shelf — but the row stays in
/// the table, because a note they wrote on it is theirs and a re-add should
/// restore rather than duplicate.
pub async fn list_bookmarks(db: &PgPool, q: ListBookmarksQuery) -> Result<BookmarkPage> {
    let limit = q.limit.clamp(1, 200);
    let offset = q.offset.max(0);
    // `%` and `_` in a user's search string are LIKE wildcards, not text —
    // escape them so searching for "50%" doesn't match everything.
    let search = q.search.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let pattern = search.map(|s| {
        format!(
            "%{}%",
            s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
        )
    });
    let ascending = q.dir.as_deref() == Some("asc");

    let where_sql = "WHERE deleted_at_source IS NULL
           AND ($1::text IS NULL
                OR title ILIKE $1 ESCAPE '\\'
                OR url ILIKE $1 ESCAPE '\\'
                OR description ILIKE $1 ESCAPE '\\'
                OR note ILIKE $1 ESCAPE '\\')";

    let total: (i64,) = sqlx::query_as(&format!(
        "SELECT COUNT(*) FROM data_content_bookmark {where_sql}"
    ))
    .bind(&pattern)
    .fetch_one(db)
    .await?;

    let items = sqlx::query_as::<_, BookmarkListItem>(&format!(
        "SELECT id, url, title, description, note, source_platform, bookmark_type,
                author, tags, thumbnail_url, timestamp
           FROM data_content_bookmark
           {where_sql}
          ORDER BY timestamp {}, id
          LIMIT $2 OFFSET $3",
        if ascending { "ASC" } else { "DESC" }
    ))
    .bind(&pattern)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await?;

    Ok(BookmarkPage {
        items,
        total: total.0,
    })
}

/// Save a URL as a bookmark (idempotent on canonical URL).
pub async fn save_bookmark(db: &PgPool, req: SaveBookmarkRequest) -> Result<SavedBookmark> {
    let url = req.url.trim().to_string();
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(Error::InvalidInput(
            "url must start with http:// or https://".to_string(),
        ));
    }

    let canonical = canonical_url_for_identity(&url);
    // Hash the canonical URL rather than embedding it: source_stream_id is a
    // UNIQUE btree key with a ~2.7KB ceiling (see mac_ingest's browser-history
    // lesson), and URLs can be arbitrarily long.
    let url_hash = Uuid::new_v5(&Uuid::NAMESPACE_URL, canonical.as_bytes());
    let source_stream_id = format!("app:url:{url_hash}");

    // A re-save must not regress the row: title/description/thumbnail are in
    // the shared upsert's update set (sync sources own them there), so a
    // manual save carrying None for them would NULL a previously enriched
    // title — and a tag-less re-save would wipe the user's tags. Carry the
    // existing values through; the request's tags win only when provided.
    let existing: Option<(
        Option<String>,
        Option<String>,
        Option<String>,
        Option<serde_json::Value>,
    )> = sqlx::query_as(
        "SELECT title, description, thumbnail_url, tags
               FROM data_content_bookmark WHERE source_stream_id = $1",
    )
    .bind(&source_stream_id)
    .fetch_optional(db)
    .await?;
    let (prev_title, prev_description, prev_thumbnail, prev_tags) =
        existing.unwrap_or((None, None, None, None));

    let tags = req
        .tags
        .as_ref()
        .filter(|t| !t.is_empty())
        .map(|t| serde_json::json!(t))
        .or(prev_tags);

    let row = BookmarkRow {
        url,
        title: prev_title,
        description: prev_description,
        source_platform: Some("web".to_string()),
        bookmark_type: Some("save".to_string()),
        author: None,
        tags,
        thumbnail_url: prev_thumbnail,
        // For a manual save, creation IS the event — the one producer where
        // the wall clock is the honest timestamp. Identity comes from the URL,
        // so a re-save bumps the row to now instead of minting a duplicate.
        timestamp: chrono::Utc::now(),
        source_stream_id: source_stream_id.clone(),
        source_table: "app_saves".to_string(),
        source_provider: "app".to_string(),
        metadata: serde_json::json!({ "canonical_url": canonical }),
    };
    upsert_bookmarks(db, &[row]).await.map_err(from_anyhow)?;

    // The note travels outside the shared upsert (see module docs). Guarded on
    // presence, so a note-less re-save keeps the existing note untouched.
    if let Some(note) = req.note.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        sqlx::query(
            "UPDATE data_content_bookmark SET note = $2, updated_at = now()
              WHERE source_stream_id = $1",
        )
        .bind(&source_stream_id)
        .bind(note)
        .execute(db)
        .await?;
    }

    let saved = sqlx::query_as::<_, SavedBookmark>(
        "SELECT id, url, title, note, tags, timestamp
           FROM data_content_bookmark WHERE source_stream_id = $1",
    )
    .bind(&source_stream_id)
    .fetch_one(db)
    .await?;
    Ok(saved)
}

fn from_anyhow(e: anyhow::Error) -> Error {
    Error::Database(e.to_string())
}

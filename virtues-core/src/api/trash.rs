//! Recently deleted — the trash for chats, pages and projects.
//!
//! Delete is not permanent. `DELETE /api/chats/:id`, `/api/pages/:id` and
//! `/api/projects/:id` stamp `deleted_at` (migration 0030) and the row leaves
//! every listing, fetch, day view and search index; it sits here for
//! [`TRASH_RETENTION_DAYS`] with a restore, and then the sweeper purges it.
//! `app_drive_files` has worked this way since the squash; this is the same
//! duty for the three things a person makes in the app.
//!
//! Why: the box already lost a real interview to one misclick of a hard delete
//! that said "cannot be undone". A record of a life should not be one click
//! from unrecoverable.
//!
//! What trashing does and does not touch:
//!
//!   - The row keeps everything. Messages, versions, shares, project
//!     membership and a chat's `project_id` all stay, so a restore brings the
//!     thing back whole. Membership rows for a trashed member are filtered out
//!     of the project's listing and counts (`projects.rs`), not deleted.
//!   - The search index is cleared for the record at once. `search_embeddings`
//!     has no FK to any source table, so a hard delete used to leave the text
//!     searchable and citable forever — that bug is closed here rather than
//!     carried. Restore needs nothing: the indexer's staleness test sees no
//!     embedding row and re-embeds on its next pass (and `embed_where` in the
//!     registry keeps trashed rows out of that pass).
//!   - Purge is the old hard delete, moved: the FKs cascade the children, and
//!     the membership rows go with `remove_items_by_url`.
//!
//! The sweep every listing must carry — `deleted_at IS NULL` — is spelled out
//! in: `chats::list_chats` / `get_chat`, `pages::{list_pages, get_page,
//! get_backlinks, search_entities}`, `projects::{list_projects, get_project,
//! add_project_item}`, `circumstances` (threads), `wiki_streams::get_day_chats`,
//! `day_summary`'s dossier, and the registry's `embed_where` / `extra_where`
//! for `app_chat` and `app_page`. `sqlx::query` is untyped: a listing that
//! forgets the predicate shows deleted rows and nothing warns.

use crate::error::{Error, Result};
use crate::types::Timestamp;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

/// How long a trashed thing waits before the sweeper purges it. Mirrors the
/// Drive trash, and the copy in the confirm dialogs.
pub const TRASH_RETENTION_DAYS: i64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrashKind {
    Chat,
    Page,
    Project,
}

impl TrashKind {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "chat" => Ok(Self::Chat),
            "page" => Ok(Self::Page),
            "project" => Ok(Self::Project),
            other => Err(Error::InvalidInput(format!(
                "unknown trash kind '{other}' (chat | page | project)"
            ))),
        }
    }

    fn table(self) -> &'static str {
        match self {
            Self::Chat => "app_chats",
            Self::Page => "app_pages",
            Self::Project => "app_projects",
        }
    }

    /// The `search_embeddings.ontology` names a record of this kind can be
    /// indexed under. A page is `app_page` or `wiki_article` by its `kind`;
    /// clearing both is cheaper than asking. Projects are not embedded.
    fn ontologies(self) -> &'static [&'static str] {
        match self {
            Self::Chat => &["app_chat"],
            Self::Page => &["app_page", "wiki_article"],
            Self::Project => &[],
        }
    }
}

/// One row of Recently deleted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashItem {
    pub kind: TrashKind,
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
    pub deleted_at: Timestamp,
    /// When the sweeper will purge it. Computed here so every client agrees.
    pub expires_at: Timestamp,
}

/// Everything in the trash, newest deletion first, all three kinds together.
pub async fn list_trash(pool: &PgPool) -> Result<Vec<TrashItem>> {
    let rows = sqlx::query(
        r#"
        SELECT 'chat' AS kind, id, COALESCE(NULLIF(title, ''), 'New chat') AS title,
               icon, deleted_at
        FROM app_chats WHERE deleted_at IS NOT NULL
        UNION ALL
        SELECT 'page', id, COALESCE(NULLIF(title, ''), 'Untitled'), icon, deleted_at
        FROM app_pages WHERE deleted_at IS NOT NULL
        UNION ALL
        SELECT 'project', id, COALESCE(NULLIF(name, ''), 'Untitled'), icon, deleted_at
        FROM app_projects WHERE deleted_at IS NOT NULL
        ORDER BY deleted_at DESC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list trash: {e}")))?;

    let ttl = chrono::Duration::days(TRASH_RETENTION_DAYS);
    rows.into_iter()
        .map(|row| {
            let kind: String = row.get("kind");
            let deleted_at: Timestamp = row.get("deleted_at");
            Ok(TrashItem {
                kind: TrashKind::parse(&kind)?,
                id: row.get("id"),
                title: row.get("title"),
                icon: row.get("icon"),
                deleted_at,
                expires_at: Timestamp::from_utc(*deleted_at + ttl),
            })
        })
        .collect()
}

// ============================================================================
// Trashing (the soft delete every DELETE endpoint now performs)
// ============================================================================

/// Stamp a live row deleted and clear its search index. `NotFound` when the
/// id does not exist or is already in the trash — a second delete of the same
/// thing is a stale client, not a request to purge.
pub async fn trash(pool: &PgPool, kind: TrashKind, id: &str) -> Result<()> {
    let stamped: Option<String> = sqlx::query_scalar(&format!(
        "UPDATE {} SET deleted_at = now() WHERE id = $1 AND deleted_at IS NULL RETURNING id",
        kind.table()
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to trash {kind:?} {id}: {e}")))?;

    if stamped.is_none() {
        return Err(Error::NotFound(format!("{kind:?} not found: {id}")));
    }

    drop_embeddings(pool, kind, id).await
}

/// Bring a trashed row back. The index refills itself: the indexer sees no
/// embedding row and re-embeds on its next pass.
pub async fn restore(pool: &PgPool, kind: TrashKind, id: &str) -> Result<()> {
    let restored: Option<String> = sqlx::query_scalar(&format!(
        "UPDATE {} SET deleted_at = NULL WHERE id = $1 AND deleted_at IS NOT NULL RETURNING id",
        kind.table()
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to restore {kind:?} {id}: {e}")))?;

    if restored.is_none() {
        return Err(Error::NotFound(format!("{kind:?} is not in the trash: {id}")));
    }
    Ok(())
}

// ============================================================================
// Purging (the hard delete, reachable only through the trash and the sweeper)
// ============================================================================

/// Delete a trashed row for good. Refuses a live row: the only door to a hard
/// delete is the trash, so nothing can skip the 30 days by calling this.
pub async fn purge_trashed(pool: &PgPool, kind: TrashKind, id: &str) -> Result<()> {
    let in_trash: Option<String> = sqlx::query_scalar(&format!(
        "SELECT id FROM {} WHERE id = $1 AND deleted_at IS NOT NULL",
        kind.table()
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to check trash for {kind:?} {id}: {e}")))?;
    if in_trash.is_none() {
        return Err(Error::NotFound(format!("{kind:?} is not in the trash: {id}")));
    }
    purge(pool, kind, id).await
}

/// The hard delete itself, trashed or not. Internal callers only — the wiki
/// article path, which owns its page and clears its own index, comes through
/// here so an article page never lingers half-deleted in the trash.
pub async fn purge(pool: &PgPool, kind: TrashKind, id: &str) -> Result<()> {
    // Index first, row second: a crash between them leaves an orphan
    // embedding at worst, which `embed_where` keeps out of retrieval anyway.
    drop_embeddings(pool, kind, id).await?;

    let deleted = sqlx::query(&format!("DELETE FROM {} WHERE id = $1", kind.table()))
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to purge {kind:?} {id}: {e}")))?
        .rows_affected();
    if deleted == 0 {
        return Err(Error::NotFound(format!("{kind:?} not found: {id}")));
    }

    // A project's members cascade by FK. A chat or page is a member BY URL,
    // which no FK knows about, so its rows are swept by hand — best-effort,
    // as the hard deletes always did it.
    let url = match kind {
        TrashKind::Chat => Some(format!("/chat/{id}")),
        TrashKind::Page => Some(format!("/page/{id}")),
        TrashKind::Project => None,
    };
    if let Some(url) = url {
        if let Err(e) = crate::api::projects::remove_items_by_url(pool, &url).await {
            tracing::warn!(kind = ?kind, id, "purge: project membership sweep failed: {e}");
        }
    }
    Ok(())
}

/// Empty the trash. Returns how many were purged.
pub async fn empty_trash(pool: &PgPool) -> Result<u64> {
    let mut n = 0;
    for item in list_trash(pool).await? {
        purge(pool, item.kind, &item.id).await?;
        n += 1;
    }
    Ok(n)
}

/// Purge everything past [`TRASH_RETENTION_DAYS`]. The sweeper's tick.
pub async fn purge_expired(pool: &PgPool) -> Result<u64> {
    let mut n = 0;
    for item in list_trash(pool).await? {
        if *item.expires_at > chrono::Utc::now() {
            continue;
        }
        purge(pool, item.kind, &item.id).await?;
        n += 1;
    }
    Ok(n)
}

/// Clear a record's rows from `search_embeddings`, keeping the BM25 corpus
/// stats honest (mirrors `extraction::sweep_orphaned_embeddings`).
async fn drop_embeddings(pool: &PgPool, kind: TrashKind, id: &str) -> Result<()> {
    let ontologies = kind.ontologies();
    if ontologies.is_empty() {
        return Ok(());
    }
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| Error::Database(format!("drop embeddings tx: {e}")))?;
    let dropped: Vec<Option<i64>> = sqlx::query_scalar(
        "DELETE FROM search_embeddings \
         WHERE ontology = ANY($1) AND record_id = $2 \
         RETURNING bm25_len",
    )
    .bind(ontologies)
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| Error::Database(format!("drop embeddings: {e}")))?;
    if !dropped.is_empty() {
        let dropped_len: i64 = dropped.iter().flatten().sum();
        sqlx::query(
            "UPDATE search_index_meta \
             SET n_docs = GREATEST(n_docs - $1, 0), sum_len = GREATEST(sum_len - $2, 0) \
             WHERE singleton",
        )
        .bind(dropped.len() as i64)
        .bind(dropped_len)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("drop embeddings stats: {e}")))?;
    }
    tx.commit()
        .await
        .map_err(|e| Error::Database(format!("drop embeddings commit: {e}")))
}

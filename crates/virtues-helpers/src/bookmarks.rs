//! Bookmark normalization and writes for `data_content_bookmark`.
//!
//! Shared by every producer — the mac_ingest browser-bookmark transform, the
//! github_stars_sync applet, and core's `POST /api/bookmarks` — so the row
//! shape, identity rules, and tombstone semantics live in exactly one place
//! (docs/bookmarks-plan.md).
//!
//! Two sync models write here, and they differ on deletion:
//!
//!   **Snapshot sources** (browser bookmark files): the payload is the full
//!   current state. Presence upserts; absence means *removed at the source* —
//!   callers follow `upsert_bookmarks` with `tombstone_absent` over their id
//!   prefix. Rows are never deleted: `deleted_at_source` is stamped, and
//!   cleared again if the bookmark reappears (the upsert always writes NULL).
//!
//!   **Event sources** (GitHub stars, X bookmarks, manual saves): items arrive
//!   individually; absence from a batch means nothing. Upsert only.
//!
//! `deleted_at_source` is strictly "the source no longer has this". It is NOT
//! the user's in-app triage state — that is `is_archived`, which no source
//! sync may touch. Likewise `note` is user-authored marginalia: transforms
//! never write it, and the upsert never updates it (a source row's NULL would
//! clobber what the user wrote). The API endpoint sets it in a separate,
//! deliberate UPDATE.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::dedup::{build_batch_upsert_query, dedup_refs_keep_last, BATCH_SIZE};

/// A normalized bookmark, ready to land. Producers fill this from their raw
/// payloads; identity (`source_stream_id`) is the producer's job because only
/// it knows what's stable at its source (`mac:{device}:{browser}:{guid}`,
/// `github:star:{node_id}`, `app:url:{hash}`).
#[derive(Debug, Clone)]
pub struct BookmarkRow {
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    /// Where the save lives: "safari" | "chrome" | "arc" | "github" | "web" …
    pub source_platform: Option<String>,
    /// What kind of save: "bookmark" | "reading_list" | "star" | "save" …
    pub bookmark_type: Option<String>,
    pub author: Option<String>,
    /// User-authored containers harvested at the source — folder path
    /// segments, star lists, collection names. JSON array of strings.
    pub tags: Option<Value>,
    pub thumbnail_url: Option<String>,
    /// The source's own creation moment (date_added, starred_at). Producers
    /// must NOT fall back to the wall clock for records they can't place in
    /// time — see the dedup-key poisoning note in mac_ingest/transform.rs.
    pub timestamp: DateTime<Utc>,
    pub source_stream_id: String,
    pub source_table: String,
    pub source_provider: String,
    pub metadata: Value,
}

/// Batch-upsert normalized rows. Returns rows written (new + corrected).
///
/// Re-sync corrects source-owned fields and clears any tombstone (a present
/// row is by definition not deleted at its source). `note` and `is_archived`
/// are user-owned and deliberately not in the update set; `metadata` merges
/// rather than replaces (see build_batch_upsert_query).
pub async fn upsert_bookmarks(pool: &PgPool, rows: &[BookmarkRow]) -> Result<usize> {
    let mut written = 0;
    let deduped = dedup_refs_keep_last(rows, |r| r.source_stream_id.clone());
    for chunk in deduped.chunks(BATCH_SIZE) {
        written += flush(pool, chunk).await?;
    }
    Ok(written)
}

async fn flush(pool: &PgPool, rows: &[&BookmarkRow]) -> Result<usize> {
    if rows.is_empty() {
        return Ok(0);
    }
    let sql = build_batch_upsert_query(
        "data_content_bookmark",
        &[
            "id",
            "url",
            "title",
            "description",
            "source_platform",
            "bookmark_type",
            "author",
            "tags",
            "thumbnail_url",
            "occurred_at",
            "source_stream_id",
            "source_table",
            "source_provider",
            "deleted_at_source",
            "metadata",
        ],
        "source_stream_id",
        &[
            "url",
            "title",
            "description",
            "bookmark_type",
            "author",
            "tags",
            "thumbnail_url",
            "occurred_at",
            "deleted_at_source",
            "metadata",
        ],
        rows.len(),
    );

    let mut q = sqlx::query(&sql);
    for r in rows {
        let id = Uuid::new_v5(&Uuid::NAMESPACE_OID, r.source_stream_id.as_bytes()).to_string();
        q = q
            .bind(id)
            .bind(&r.url)
            .bind(&r.title)
            .bind(&r.description)
            .bind(&r.source_platform)
            .bind(&r.bookmark_type)
            .bind(&r.author)
            .bind(&r.tags)
            .bind(&r.thumbnail_url)
            .bind(r.timestamp)
            .bind(&r.source_stream_id)
            .bind(&r.source_table)
            .bind(&r.source_provider)
            .bind(Option::<DateTime<Utc>>::None)
            .bind(&r.metadata);
    }
    let result = q.execute(pool).await?;
    Ok(result.rows_affected() as usize)
}

/// Snapshot reconcile: tombstone every live row under `prefix` that is not in
/// `present_ids`. Returns rows tombstoned.
///
/// The prefix scopes the reconcile to exactly one snapshot's universe — one
/// browser on one device (`mac:{device}:{browser}:`) — so two Macs, or Safari
/// vs Chrome, can never tombstone each other's rows. `starts_with` rather
/// than LIKE so a device id containing `_`/`%` can't widen the match.
pub async fn tombstone_absent(pool: &PgPool, prefix: &str, present_ids: &[String]) -> Result<u64> {
    let result = sqlx::query(
        "UPDATE data_content_bookmark
            SET deleted_at_source = now(), updated_at = now()
          WHERE starts_with(source_stream_id, $1)
            AND deleted_at_source IS NULL
            AND NOT (source_stream_id = ANY($2))",
    )
    .bind(prefix)
    .bind(present_ids)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Canonical form of a URL for *identity hashing* of manual saves — NOT for
/// storage (rows keep the URL as the source gave it). Re-saving the same page
/// must upsert, not duplicate, so: scheme+host lowercased, fragment dropped,
/// tracking params stripped, default ports and a lone trailing slash removed.
/// Conservative on purpose — a false split (two rows for one page) is
/// annoying; a false merge (one row for two pages) loses a save.
pub fn canonical_url_for_identity(raw: &str) -> String {
    let trimmed = raw.trim();
    let Ok(mut parsed) = url::Url::parse(trimmed) else {
        return trimmed.to_string();
    };

    parsed.set_fragment(None);

    let kept: Vec<(String, String)> = parsed
        .query_pairs()
        .filter(|(k, _)| !is_tracking_param(k))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if kept.is_empty() {
        parsed.set_query(None);
    } else {
        let mut qp = parsed.query_pairs_mut();
        qp.clear();
        for (k, v) in &kept {
            qp.append_pair(k, v);
        }
    }

    let mut s = parsed.to_string();
    // Url normalizes scheme/host case and default ports already; the one
    // cosmetic variant left is a lone trailing slash on a bare path.
    if s.ends_with('/') && parsed.path() == "/" && parsed.query().is_none() {
        s.pop();
    }
    s
}

fn is_tracking_param(key: &str) -> bool {
    key.starts_with("utm_")
        || matches!(
            key,
            "fbclid" | "gclid" | "dclid" | "msclkid" | "igshid" | "mc_cid" | "mc_eid" | "ref_src"
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_url_strips_tracking_fragment_and_case() {
        assert_eq!(
            canonical_url_for_identity(
                "HTTPS://Example.com/Path?utm_source=x&keep=1&fbclid=abc#section"
            ),
            "https://example.com/Path?keep=1"
        );
    }

    #[test]
    fn canonical_url_bare_host_variants_converge() {
        assert_eq!(
            canonical_url_for_identity("https://example.com/"),
            canonical_url_for_identity("https://example.com:443")
        );
    }

    #[test]
    fn canonical_url_preserves_meaningful_query_and_path() {
        // Query order and non-tracking params are meaningful; do not touch.
        assert_eq!(
            canonical_url_for_identity("https://youtube.com/watch?v=abc123"),
            "https://youtube.com/watch?v=abc123"
        );
    }

    #[test]
    fn canonical_url_unparseable_passes_through() {
        assert_eq!(canonical_url_for_identity("  not a url  "), "not a url");
    }

    /// Full round-trip against a real `data_content_bookmark` (needs
    /// DATABASE_URL; run explicitly):
    ///
    ///     cargo test -p virtues-helpers -- --ignored bookmarks_roundtrip
    ///
    /// Uses a throwaway id prefix and deletes its rows on the way out, so it
    /// is safe against a dev database.
    #[tokio::test]
    #[ignore]
    async fn bookmarks_roundtrip_against_real_table() {
        let _ = dotenv::dotenv();
        let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL required");
        let pool = sqlx::PgPool::connect(&db_url).await.expect("connect");

        let prefix = format!("test:bmk:{}:", Uuid::new_v4());
        let mk = |guid: &str, title: &str| BookmarkRow {
            url: format!("https://example.com/{guid}"),
            title: Some(title.to_string()),
            description: None,
            source_platform: Some("safari".to_string()),
            bookmark_type: Some("bookmark".to_string()),
            author: None,
            tags: Some(serde_json::json!(["Favorites", "Design"])),
            thumbnail_url: None,
            timestamp: chrono::Utc::now(),
            source_stream_id: format!("{prefix}{guid}"),
            source_table: "test".to_string(),
            source_provider: "test".to_string(),
            metadata: serde_json::json!({"t": true}),
        };

        // 1. Two fresh rows land.
        let n = upsert_bookmarks(&pool, &[mk("a", "A"), mk("b", "B")])
            .await
            .unwrap();
        assert_eq!(n, 2);

        // 2. User writes a note; a re-sync corrects the title but must NOT
        //    touch the note (it is not in the update set).
        sqlx::query("UPDATE data_content_bookmark SET note = 'mine' WHERE source_stream_id = $1")
            .bind(format!("{prefix}a"))
            .execute(&pool)
            .await
            .unwrap();
        upsert_bookmarks(&pool, &[mk("a", "A-renamed")])
            .await
            .unwrap();
        let (title, note): (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT title, note FROM data_content_bookmark WHERE source_stream_id = $1",
        )
        .bind(format!("{prefix}a"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(title.as_deref(), Some("A-renamed"));
        assert_eq!(
            note.as_deref(),
            Some("mine"),
            "re-sync clobbered the user's note"
        );

        // 3. Snapshot reconcile: only `a` present → `b` tombstoned.
        let gone = tombstone_absent(&pool, &prefix, &[format!("{prefix}a")])
            .await
            .unwrap();
        assert_eq!(gone, 1);

        // 4. `b` reappears in a later snapshot → tombstone clears.
        upsert_bookmarks(&pool, &[mk("b", "B")]).await.unwrap();
        let (deleted,): (Option<chrono::DateTime<chrono::Utc>>,) = sqlx::query_as(
            "SELECT deleted_at_source FROM data_content_bookmark WHERE source_stream_id = $1",
        )
        .bind(format!("{prefix}b"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(deleted.is_none(), "reappearance must clear the tombstone");

        sqlx::query("DELETE FROM data_content_bookmark WHERE starts_with(source_stream_id, $1)")
            .bind(&prefix)
            .execute(&pool)
            .await
            .unwrap();
    }
}

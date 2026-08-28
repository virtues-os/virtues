//! Articles — the record's own prose about a subject.
//!
//! An article IS a page (migration 0081). `app_pages` already carries Yjs
//! editing, `app_page_versions` with a `created_by` column, and an AI write
//! path that goes *through* the CRDT rather than around it; a second prose
//! column on the wiki side would mean a second write path with no
//! reconciliation and no pre-edit snapshot. So the wiki owns a join row and the
//! prose stays where the machinery already is.
//!
//! **This module is pool-only, and deliberately.** Applets link virtues-core as
//! a library and run as separate binaries holding nothing but a `PgPool` — no
//! `AppState`, no axum, no `YjsState`. Putting `create_article` in the server
//! layer would break the "exactly one creation path" invariant on day one, for
//! every applet. So nothing here takes anything richer than a pool.
//!
//! Creating a page with `content` and no `yjs_state` is the correct first
//! write: the Yjs layer seeds `Y.Text` from the `content` column the first time
//! a document is opened, so the CRDT is created lazily and correctly with
//! nothing constructing one server-side. EDITING an existing article is a
//! different matter and cannot happen here — once `yjs_state` is non-null it is
//! authoritative, and a pool-only write to `content` is silently discarded on
//! the next save. That work belongs in an applet's agent phase, which holds a
//! real `YjsState`.

use sqlx::PgPool;

use crate::api::pages;
use crate::error::{Error, Result};
use crate::ids::{generate_id, PAGE_PREFIX, WIKI_ARTICLE_PREFIX};

/// The subjects that can carry an article.
///
/// Mirrors the `subject_type` CHECK in migration 0081. `'organization'`, not
/// `'org'`: the entity-ref table and every live query use the long form, and
/// the sweep joins articles to refs — the short form would make that join
/// silently return zero organization rows. The frontend route stays `/org`.
pub const SUBJECT_TYPES: [&str; 6] = [
    "person",
    "place",
    "organization",
    "day",
    "story",
    "narrative_identity",
];

/// A subject's article, if it has one.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Article {
    pub id: String,
    pub subject_type: String,
    pub subject_id: String,
    pub page_id: String,
    pub auto_update: bool,
    pub source_ref_count: i32,
}

/// Look up a subject's article. `None` is the ordinary case, not an error:
/// articles are opt-in, so most subjects will never have one.
pub async fn get_article(
    pool: &PgPool,
    subject_type: &str,
    subject_id: &str,
) -> Result<Option<Article>> {
    let row = sqlx::query!(
        r#"
        SELECT id, subject_type, subject_id, page_id, auto_update, source_ref_count
        FROM wiki_articles
        WHERE subject_type = $1 AND subject_id = $2
        "#,
        subject_type,
        subject_id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to load article: {}", e)))?;

    Ok(row.map(|r| Article {
        id: r.id,
        subject_type: r.subject_type,
        subject_id: r.subject_id,
        page_id: r.page_id,
        auto_update: r.auto_update,
        source_ref_count: r.source_ref_count,
    }))
}

/// A subject's article as a reader wants it: the prose, when it was last
/// written, and whether it is being maintained.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ArticleProse {
    pub content: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub auto_update: bool,
}

/// Read a subject's article prose.
///
/// Reads `app_pages.content`, which the Yjs layer materialises on every save,
/// so this is the same text search indexes and the same text the editor shows.
///
/// Callers should fall back to the legacy per-entity `article` column while it
/// still exists: drops trail their phase by a release, so for now a box can
/// hold prose in either place — old articles in the column, new ones here.
pub async fn get_article_prose(
    pool: &PgPool,
    subject_type: &str,
    subject_id: &str,
) -> Result<Option<ArticleProse>> {
    let row = sqlx::query!(
        r#"
        SELECT p.content, p.updated_at, a.auto_update
        FROM wiki_articles a
        JOIN app_pages p ON p.id = a.page_id
        WHERE a.subject_type = $1 AND a.subject_id = $2
        "#,
        subject_type,
        subject_id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to load article prose: {}", e)))?;

    Ok(row.filter(|r| !r.content.trim().is_empty()).map(|r| ArticleProse {
        content: r.content,
        updated_at: r.updated_at,
        auto_update: r.auto_update,
    }))
}

/// Create a subject's article. **The only way an article page is minted.**
///
/// Both rows are written in one transaction, and `app_pages.kind` is set to
/// `'article'` in the same statement that creates the page. That pairing is the
/// whole containment story for the deliberate denormalization: `kind` and "has
/// a `wiki_articles` row" encode the same fact twice and could drift, and the
/// only thing preventing drift is that exactly one function writes both.
///
/// `date` is left NULL on purpose even for day articles. The page ontology's
/// day source filters on `t.date`, so setting it would make a day's article
/// appear inside that day as "you wrote a page today" — the exact provenance
/// failure the ontology split exists to prevent, arriving through a different
/// door. The day linkage lives on `subject_id`.
pub async fn create_article(
    pool: &PgPool,
    subject_type: &str,
    subject_id: &str,
    title: &str,
    content: &str,
) -> Result<Article> {
    if !SUBJECT_TYPES.contains(&subject_type) {
        return Err(Error::InvalidInput(format!(
            "Unknown subject type: {subject_type}"
        )));
    }
    if let Some(existing) = get_article(pool, subject_type, subject_id).await? {
        return Ok(existing);
    }

    // Deterministic in the subject, so a retry after a failed commit cannot
    // strand a second orphan page for the same subject.
    let page_id = generate_id(PAGE_PREFIX, &["article", subject_type, subject_id]);
    let article_id = generate_id(WIKI_ARTICLE_PREFIX, &[subject_type, subject_id]);

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| Error::Database(format!("Failed to begin transaction: {}", e)))?;

    sqlx::query!(
        r#"
        INSERT INTO app_pages (id, title, content, kind)
        VALUES ($1, $2, $3, 'article')
        ON CONFLICT (id) DO NOTHING
        "#,
        &page_id,
        title,
        content,
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| Error::Database(format!("Failed to create article page: {}", e)))?;

    let row = sqlx::query!(
        r#"
        INSERT INTO wiki_articles (id, subject_type, subject_id, page_id, last_written_at)
        VALUES ($1, $2, $3, $4, now())
        RETURNING id, subject_type, subject_id, page_id, auto_update, source_ref_count
        "#,
        &article_id,
        subject_type,
        subject_id,
        &page_id,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| Error::Database(format!("Failed to create article: {}", e)))?;

    tx.commit()
        .await
        .map_err(|e| Error::Database(format!("Failed to commit article: {}", e)))?;

    Ok(Article {
        id: row.id,
        subject_type: row.subject_type,
        subject_id: row.subject_id,
        page_id: row.page_id,
        auto_update: row.auto_update,
        source_ref_count: row.source_ref_count,
    })
}

/// Delete a subject's article, index rows included.
///
/// The `wiki_articles` row cascades from the page, but `search_embeddings` does
/// not: it has no FK, is keyed `(ontology, record_id)`, and nothing in the
/// search layer ever reaps records that vanished. Without this a deleted
/// person's prose stays searchable and citable forever. `annotations.rs` already
/// does exactly this for its own ontology; this is the same duty for ours.
pub async fn delete_article(pool: &PgPool, subject_type: &str, subject_id: &str) -> Result<()> {
    let Some(article) = get_article(pool, subject_type, subject_id).await? else {
        return Ok(());
    };

    sqlx::query!(
        "DELETE FROM search_embeddings WHERE ontology = 'wiki_article' AND record_id = $1",
        &article.page_id
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to clear article index: {}", e)))?;

    // Cascades the wiki_articles row.
    pages::delete_page(pool, &article.page_id).await
}

/// One page that mentions a subject.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SubjectBacklink {
    pub page_id: String,
    pub title: String,
    /// Where this prose lives: the subject's own route if it is an article,
    /// otherwise the page route. So a backlink always opens something real.
    pub route: String,
    pub is_article: bool,
}

/// "Mentioned in N articles" — every page whose prose links to this subject.
///
/// **The edge points at a SUBJECT, not at an article**, and that is the whole
/// correction. Articles are opt-in, so most subjects will never have prose; an
/// article↔article graph would be empty on day one and near-empty forever.
/// Production ref-routes name subjects (`/person/person_ab12`,
/// `/day/day_2026-03-03`), there is no article route and no article id in any
/// link, and a backlink whose target has no article still renders — on the
/// subject's own page, which always exists.
///
/// Keyed by route identity rather than a foreign key, deliberately:
/// `/day/day_2026-03-03` may have no `wiki_days` row at all (42 rows across 155
/// days on a real box), so an FK would not merely be inconvenient, it would be
/// wrong.
///
/// Derived at READ time, like `get_page_backlinks`, rather than maintained in
/// an edge table. The corpus is small and the query is a single indexed-ish
/// scan; an on-save table is an optimization that should be justified by a
/// measurement rather than assumed. If one is ever built, the only correct hook
/// is `save_and_materialize` — the sole place `content` is written.
pub async fn get_subject_backlinks(
    pool: &PgPool,
    subject_type: &str,
    subject_id: &str,
) -> Result<Vec<SubjectBacklink>> {
    // The route prefix the frontend actually uses. `organization` is the schema
    // word; `/org` is the route — the one place that mapping happens.
    let prefix = match subject_type {
        "person" => "person",
        "place" => "place",
        "organization" => "org",
        "day" => "day",
        "story" => "story",
        other => {
            return Err(Error::InvalidInput(format!(
                "No route for subject type {other}"
            )))
        }
    };

    // Trailing `)` pins the match to a real markdown link and to the exact id,
    // so `person_ab` cannot match `person_abc` — same reasoning as
    // `get_page_backlinks`.
    let needle = format!("/{prefix}/{subject_id})");
    let like = format!("%{needle}%");

    let rows = sqlx::query!(
        r#"
        -- `?` marks the LEFT JOIN columns nullable; sqlx assumes NOT NULL
        -- otherwise and would hand back a String that is sometimes absent.
        SELECT p.id, p.title, p.kind,
               a.subject_type AS "subject_type?", a.subject_id AS "subject_id?"
        FROM app_pages p
        LEFT JOIN wiki_articles a ON a.page_id = p.id
        WHERE p.content LIKE $1
        ORDER BY p.updated_at DESC
        LIMIT 100
        "#,
        &like
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get subject backlinks: {}", e)))?;

    Ok(rows
        .into_iter()
        // A subject's own article naturally contains its own name; listing it
        // as a mention of itself is noise.
        .filter(|r| !(r.subject_type.as_deref() == Some(subject_type)
            && r.subject_id.as_deref() == Some(subject_id)))
        .map(|r| {
            let is_article = r.kind == "article";
            let route = match (&r.subject_type, &r.subject_id) {
                (Some(st), Some(sid)) => {
                    let p = match st.as_str() {
                        "organization" => "org",
                        other => other,
                    };
                    format!("/{p}/{sid}")
                }
                _ => format!("/page/{}", r.id),
            };
            SubjectBacklink {
                page_id: r.id,
                title: r.title,
                route,
                is_article,
            }
        })
        .collect())
}

/// One edit in an article's history, with what actually changed.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ArticleRevision {
    pub version_number: i64,
    /// Who made the edit this entry describes.
    pub author: String,
    pub at: chrono::DateTime<chrono::Utc>,
    /// Unified-ish diff of the edit: what this author changed.
    pub diff: Vec<DiffLine>,
    /// True when the edit produced the text currently on the page.
    pub is_current: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DiffLine {
    /// "add" | "del" | "ctx"
    pub kind: &'static str,
    pub text: String,
}

/// An article's history: who changed it, when, and what changed.
///
/// **The version table is off by one, and reading it naively gets authorship
/// backwards.** A row is written as a snapshot taken *before* an edit, stamped
/// with the editor about to write (`page_editor.rs` saves `created_by: "ai"`
/// then applies the change). So `version[n].created_by` names the author of the
/// NEXT state, not of the text stored in that row — and nothing is written
/// after an edit, so the current text is in no version row at all.
///
/// That is recoverable rather than fatal, and this is where it gets recovered:
/// the edit made by `version[n].created_by` turns `version[n]` into
/// `version[n+1]`, or into the live page for the most recent one. Pair the rows
/// that way and the feed reads correctly — "the record rewrote this on Tuesday,
/// here is the diff" — without changing how versions are written, which would
/// invalidate every row already on disk.
pub async fn get_article_history(
    pool: &PgPool,
    subject_type: &str,
    subject_id: &str,
) -> Result<Vec<ArticleRevision>> {
    let Some(article) = get_article(pool, subject_type, subject_id).await? else {
        return Ok(Vec::new());
    };

    let rows = sqlx::query!(
        r#"
        SELECT version_number, created_by, created_at, yjs_snapshot
        FROM app_page_versions
        WHERE page_id = $1
        ORDER BY version_number ASC
        "#,
        &article.page_id
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to load history: {}", e)))?;

    let current: String = sqlx::query_scalar!(
        "SELECT content FROM app_pages WHERE id = $1",
        &article.page_id
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to load current text: {}", e)))?;

    // Text lives in the Yjs snapshot, not in `content_preview` — that column
    // holds a human label ("Auto-saved before AI edit"), never the prose.
    let texts: Vec<String> = rows
        .iter()
        .map(|r| {
            r.yjs_snapshot
                .as_deref()
                .map(crate::server::yjs::extract_text_content)
                .unwrap_or_default()
        })
        .collect();

    let mut out = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let before = &texts[i];
        let after = texts.get(i + 1).unwrap_or(&current);
        out.push(ArticleRevision {
            version_number: row.version_number,
            author: row.created_by.clone(),
            at: row.created_at,
            diff: diff_lines(before, after),
            is_current: i + 1 == rows.len(),
        });
    }
    out.reverse(); // newest first
    Ok(out)
}

/// One entry in the wiki's History room.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HistoryEntry {
    pub subject_type: String,
    pub subject_id: String,
    pub route: String,
    pub title: String,
    pub author: String,
    pub at: chrono::DateTime<chrono::Utc>,
    pub version_number: i64,
}

/// Every recent edit to any article, newest first — the room's front page.
///
/// This is the review surface that makes `auto_update` safe to turn on: the
/// switch is the consent, and this is where you see what that consent produced.
/// Without it the machine edits prose in a room nobody visits.
///
/// Authorship carries the same off-by-one as `get_article_history` and is
/// resolved the same way — `created_by` names the author of the edit this row
/// precedes, which is exactly what a feed wants to say.
pub async fn get_history_feed(pool: &PgPool, limit: i64) -> Result<Vec<HistoryEntry>> {
    let rows = sqlx::query!(
        r#"
        SELECT v.version_number, v.created_by, v.created_at,
               a.subject_type, a.subject_id, p.title
        FROM app_page_versions v
        JOIN wiki_articles a ON a.page_id = v.page_id
        JOIN app_pages p ON p.id = v.page_id
        ORDER BY v.created_at DESC
        LIMIT $1
        "#,
        limit.clamp(1, 200)
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to load history feed: {}", e)))?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let prefix = match r.subject_type.as_str() {
                "organization" => "org",
                other => other,
            };
            HistoryEntry {
                route: format!("/{prefix}/{}", r.subject_id),
                subject_type: r.subject_type,
                subject_id: r.subject_id,
                title: r.title,
                author: r.created_by,
                at: r.created_at,
                version_number: r.version_number,
            }
        })
        .collect())
}

/// Line diff, with a little context.
///
/// Whole-document rewrites are why articles are edited rather than regenerated:
/// a full replace diffs at 100% and "everything changed" on every entry is the
/// same as showing nothing. Surgical edits make this readable, so the diff and
/// the write strategy are the same design decision seen from two sides.
fn diff_lines(before: &str, after: &str) -> Vec<DiffLine> {
    use similar::{ChangeTag, TextDiff};

    const CONTEXT: usize = 1;
    let diff = TextDiff::from_lines(before, after);
    let mut out = Vec::new();
    for group in diff.grouped_ops(CONTEXT).iter() {
        for op in group {
            for change in diff.iter_changes(op) {
                let kind = match change.tag() {
                    ChangeTag::Insert => "add",
                    ChangeTag::Delete => "del",
                    ChangeTag::Equal => "ctx",
                };
                out.push(DiffLine {
                    kind,
                    text: change.value().trim_end_matches('\n').to_string(),
                });
            }
        }
    }
    out
}

/// Turn maintenance on or off for one article.
///
/// `false` means the AI never touches it — not a pending-approval queue,
/// nothing held for review. The sweep skips it, and it changes only when a
/// person regenerates it or flips this back. The switch IS the consent.
pub async fn set_auto_update(
    pool: &PgPool,
    subject_type: &str,
    subject_id: &str,
    on: bool,
) -> Result<()> {
    let n = sqlx::query!(
        "UPDATE wiki_articles SET auto_update = $3 WHERE subject_type = $1 AND subject_id = $2",
        subject_type,
        subject_id,
        on
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to set auto_update: {}", e)))?
    .rows_affected();

    if n == 0 {
        return Err(Error::NotFound(format!(
            "No article for {subject_type}/{subject_id}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The invariant the whole design leans on: one function writes both rows,
    /// so `app_pages.kind` and "has a `wiki_articles` row" cannot disagree.
    #[sqlx::test]
    async fn create_writes_both_rows_and_marks_the_page(pool: PgPool) {
        sqlx::query("INSERT INTO wiki_people (id, name) VALUES ('p_1', 'Sarah')")
            .execute(&pool)
            .await
            .unwrap();

        let a = create_article(&pool, "person", "p_1", "Sarah", "Prose.")
            .await
            .unwrap();

        let kind: String = sqlx::query_scalar("SELECT kind FROM app_pages WHERE id = $1")
            .bind(&a.page_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(kind, "article");
        assert!(!a.auto_update, "maintenance is opt-in, never on by default");

        // (The old `date must stay NULL` assertion is gone with the column —
        // reflections were retired 2026-08-03, the column dropped 2026-08-28;
        // day-source separation is enforced by `kind` now, asserted above.)
    }

    /// Articles are storage-identical to pages, so nothing but this predicate
    /// keeps them out of the Pages list. If it regresses, the wiki quietly
    /// empties a destination the user built by hand.
    #[sqlx::test]
    async fn articles_stay_out_of_the_pages_list(pool: PgPool) {
        sqlx::query("INSERT INTO wiki_people (id, name) VALUES ('p_1', 'Sarah')")
            .execute(&pool)
            .await
            .unwrap();
        pages::create_page(
            &pool,
            pages::CreatePageRequest {
                title: "A page I wrote".into(),
                content: String::new(),
                icon: None,
                icon_color: None,
                cover_url: None,
                tags: None,
                notebook_id: None,
            },
        )
        .await
        .unwrap();
        create_article(&pool, "person", "p_1", "Sarah", "Prose.")
            .await
            .unwrap();

        let listed = pages::list_pages(&pool, None, None).await.unwrap();
        assert_eq!(listed.pages.len(), 1, "only the hand-made page is listed");
        assert_eq!(listed.pages[0].title, "A page I wrote");
    }

    /// Creating twice is a no-op, not a second page. The ids are derived from
    /// the subject precisely so a retry cannot strand an orphan.
    #[sqlx::test]
    async fn create_is_idempotent_per_subject(pool: PgPool) {
        sqlx::query("INSERT INTO wiki_people (id, name) VALUES ('p_1', 'Sarah')")
            .execute(&pool)
            .await
            .unwrap();

        let a = create_article(&pool, "person", "p_1", "Sarah", "One.")
            .await
            .unwrap();
        let b = create_article(&pool, "person", "p_1", "Sarah", "Two.")
            .await
            .unwrap();
        assert_eq!(a.id, b.id);

        let pages_made: i64 =
            sqlx::query_scalar("SELECT count(*) FROM app_pages WHERE kind = 'article'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(pages_made, 1);
    }

    /// The edge points at a SUBJECT. A day article naming a person must show up
    /// on that person's page — even though the person has no article of their
    /// own, which is the ordinary case under opt-in.
    #[sqlx::test]
    async fn backlinks_find_a_subject_with_no_article(pool: PgPool) {
        sqlx::query("INSERT INTO wiki_people (id, name) VALUES ('p_1', 'Maya')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO wiki_days (id, date) VALUES ('day_1', '2026-03-03')")
            .execute(&pool)
            .await
            .unwrap();

        create_article(
            &pool,
            "day",
            "day_1",
            "3 March 2026",
            "Coffee with [Maya](/person/p_1) before the train.",
        )
        .await
        .unwrap();

        let links = get_subject_backlinks(&pool, "person", "p_1").await.unwrap();
        assert_eq!(links.len(), 1, "the day article mentions Maya");
        assert_eq!(links[0].route, "/day/day_1", "opens the SUBJECT, not the page");
        assert!(links[0].is_article);
    }

    /// Ids are prefixes of each other all the time (`p_1` / `p_12`). The
    /// trailing `)` is what stops a link to one being counted for the other.
    #[sqlx::test]
    async fn backlinks_do_not_match_an_id_prefix(pool: PgPool) {
        for (id, name) in [("p_1", "Maya"), ("p_12", "Mayara")] {
            sqlx::query("INSERT INTO wiki_people (id, name) VALUES ($1, $2)")
                .bind(id)
                .bind(name)
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO wiki_days (id, date) VALUES ('day_1', '2026-03-03')")
            .execute(&pool)
            .await
            .unwrap();
        create_article(&pool, "day", "day_1", "3 March", "Saw [Mayara](/person/p_12).")
            .await
            .unwrap();

        assert_eq!(
            get_subject_backlinks(&pool, "person", "p_1").await.unwrap().len(),
            0,
            "a link to p_12 is not a link to p_1"
        );
        assert_eq!(
            get_subject_backlinks(&pool, "person", "p_12").await.unwrap().len(),
            1
        );
    }

    /// `organization` is the schema word and `/org` is the route. Getting that
    /// mapping wrong makes every org backlink silently empty.
    #[sqlx::test]
    async fn org_backlinks_use_the_org_route(pool: PgPool) {
        sqlx::query("INSERT INTO wiki_orgs (id, name) VALUES ('o_1', 'Acme')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO wiki_days (id, date) VALUES ('day_1', '2026-03-03')")
            .execute(&pool)
            .await
            .unwrap();
        create_article(&pool, "day", "day_1", "3 March", "Met [Acme](/org/o_1).")
            .await
            .unwrap();

        let links = get_subject_backlinks(&pool, "organization", "o_1").await.unwrap();
        assert_eq!(links.len(), 1, "schema says organization, the route says org");
    }

    /// Authorship is off by one in the table and must not be off by one in the
    /// feed. A version row is a snapshot taken BEFORE an edit, stamped with the
    /// editor about to write — so the diff for `created_by` is that row's text
    /// against the NEXT row (or the live page, for the most recent).
    #[sqlx::test]
    async fn history_pairs_each_author_with_the_edit_they_made(pool: PgPool) {
        use base64::Engine as _;

        sqlx::query("INSERT INTO wiki_people (id, name) VALUES ('p_1', 'Sarah')")
            .execute(&pool)
            .await
            .unwrap();
        let a = create_article(&pool, "person", "p_1", "Sarah", "First line.\n")
            .await
            .unwrap();

        // A snapshot of the ORIGINAL text, stamped with the editor about to
        // change it — exactly what page_editor.rs writes before an AI edit.
        let mut doc = yrs::Doc::new();
        {
            use yrs::{Text, Transact};
            let text = doc.get_or_insert_text("content");
            let mut txn = doc.transact_mut();
            text.insert(&mut txn, 0, "First line.\n");
        }
        let snapshot = {
            use yrs::{ReadTxn, Transact};
            let txn = doc.transact();
            txn.encode_state_as_update_v1(&yrs::StateVector::default())
        };
        pages::create_version(
            &pool,
            &a.page_id,
            pages::CreateVersionRequest {
                snapshot: base64::engine::general_purpose::STANDARD.encode(&snapshot),
                content_preview: "Auto-saved before AI edit".into(),
                created_by: "ai".into(),
                description: None,
            },
        )
        .await
        .unwrap();

        // …then the edit itself lands on the page.
        sqlx::query("UPDATE app_pages SET content = $2 WHERE id = $1")
            .bind(&a.page_id)
            .bind("First line.\nSecond line.\n")
            .execute(&pool)
            .await
            .unwrap();

        let hist = get_article_history(&pool, "person", "p_1").await.unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].author, "ai");
        assert!(hist[0].is_current);

        let added: Vec<&str> = hist[0]
            .diff
            .iter()
            .filter(|l| l.kind == "add")
            .map(|l| l.text.as_str())
            .collect();
        assert_eq!(
            added,
            vec!["Second line."],
            "the diff must show what THIS author added, not the state before them"
        );
    }

    /// A subject with no article has no history — not an error.
    #[sqlx::test]
    async fn history_is_empty_without_an_article(pool: PgPool) {
        sqlx::query("INSERT INTO wiki_people (id, name) VALUES ('p_1', 'Sarah')")
            .execute(&pool)
            .await
            .unwrap();
        assert!(get_article_history(&pool, "person", "p_1").await.unwrap().is_empty());
    }

    /// Deleting must clear the index too. `search_embeddings` has no FK and the
    /// search layer never reaps vanished records, so without this a deleted
    /// person's prose stays searchable and citable forever.
    #[sqlx::test]
    async fn delete_clears_the_search_index(pool: PgPool) {
        sqlx::query("INSERT INTO wiki_people (id, name) VALUES ('p_1', 'Sarah')")
            .execute(&pool)
            .await
            .unwrap();
        let a = create_article(&pool, "person", "p_1", "Sarah", "Prose.")
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO search_embeddings (id, ontology, record_id, text_hash, model, \
             chunk_index, content, doc_hash) \
             VALUES ('se_1', 'wiki_article', $1, 'th', 'test-model', 0, 'Prose.', 'h')",
        )
        .bind(&a.page_id)
        .execute(&pool)
        .await
        .unwrap();

        delete_article(&pool, "person", "p_1").await.unwrap();

        let left: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM search_embeddings WHERE record_id = $1",
        )
        .bind(&a.page_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(left, 0, "index rows must not outlive the article");

        assert!(get_article(&pool, "person", "p_1").await.unwrap().is_none());
    }
}

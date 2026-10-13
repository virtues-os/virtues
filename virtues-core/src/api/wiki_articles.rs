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

// The list of subjects that may carry an article is
// [`crate::api::subjects::SUBJECTS`], and `is_subject` is how to ask. It used
// to be a second array here, which is how `year` came to pass every route,
// fold and brief and then fail at this function's own allowlist.

/// A subject's article, if it has one.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Article {
    pub id: String,
    pub subject_type: String,
    pub subject_id: String,
    pub page_id: String,
    /// `auto` | `never` (the schema's constraint; see `set_maintenance`).
    pub maintenance: String,
}

/// Look up a subject's article. `None` is an ordinary answer, not an error:
/// days, years, chapters and the narrative identity get articles from their
/// own pipelines, but a person, place or org gets one only when someone asks
/// (`entity_article_gen::write_entity_article_now`), so most entities never
/// have one.
pub async fn get_article<'e>(
    executor: impl sqlx::PgExecutor<'e>,
    subject_type: &str,
    subject_id: &str,
) -> Result<Option<Article>> {
    let row = sqlx::query!(
        r#"
        SELECT id, subject_type, subject_id, page_id, maintenance
        FROM wiki_articles
        WHERE subject_type = $1 AND subject_id = $2
        "#,
        subject_type,
        subject_id
    )
    .fetch_optional(executor)
    .await
    .map_err(|e| Error::Database(format!("Failed to load article: {}", e)))?;

    Ok(row.map(|r| Article {
        id: r.id,
        subject_type: r.subject_type,
        subject_id: r.subject_id,
        page_id: r.page_id,
        maintenance: r.maintenance,
    }))
}

/// A subject's article as a reader wants it: the prose, when it was last
/// written, and whether it is being maintained.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ArticleProse {
    pub content: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Whether the record keeps this article up to date.
    pub maintained: bool,
}

/// Read a subject's article prose.
///
/// Reads `app_pages.content`, which the Yjs layer materialises on every save,
/// so this is the same text search indexes and the same text the editor shows.
///
/// The only place article prose lives. The per-entity `article` columns this
/// used to fall back to were dropped in migration 0025, so a fallback written
/// against them today would not compile — and one written defensively would be
/// dead code pretending a second source of truth still exists.
pub async fn get_article_prose(
    pool: &PgPool,
    subject_type: &str,
    subject_id: &str,
) -> Result<Option<ArticleProse>> {
    let row = sqlx::query!(
        r#"
        SELECT p.content, p.updated_at, (a.maintenance <> 'never') AS "maintained!"
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
        maintained: r.maintained,
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
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| Error::Database(format!("Failed to begin transaction: {}", e)))?;
    let article = create_article_in(&mut tx, subject_type, subject_id, title, content).await?;
    tx.commit()
        .await
        .map_err(|e| Error::Database(format!("Failed to commit article: {}", e)))?;
    Ok(article)
}

/// [`create_article`] inside the caller's transaction, for a writer whose
/// article lands together with what it records about it: a day's narration
/// also records the edition and stamps the day, and a page left without
/// those would be written again, and paid for again.
pub async fn create_article_in(
    tx: &mut sqlx::PgConnection,
    subject_type: &str,
    subject_id: &str,
    title: &str,
    content: &str,
) -> Result<Article> {
    if !crate::api::subjects::is_subject(subject_type) {
        return Err(Error::InvalidInput(format!(
            "Unknown subject type: {subject_type}"
        )));
    }
    if let Some(existing) = get_article(&mut *tx, subject_type, subject_id).await? {
        return Ok(existing);
    }

    // Deterministic in the subject, so a retry after a failed commit cannot
    // strand a second orphan page for the same subject.
    let page_id = generate_id(PAGE_PREFIX, &["article", subject_type, subject_id]);
    let article_id = generate_id(WIKI_ARTICLE_PREFIX, &[subject_type, subject_id]);

    let page = sqlx::query!(
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

    // The first draft is the page's first version, so History has the text
    // the first edit to it (the owner's or the editor's) is diffed against,
    // and putting that version back undoes the edit. Only for a page this
    // call made: a page already there keeps whatever it says.
    if page.rows_affected() == 1 {
        pages::keep_first_draft(&mut *tx, &page_id, content).await?;
    }

    let row = sqlx::query!(
        r#"
        INSERT INTO wiki_articles (id, subject_type, subject_id, page_id, last_written_at)
        VALUES ($1, $2, $3, $4, now())
        RETURNING id, subject_type, subject_id, page_id, maintenance
        "#,
        &article_id,
        subject_type,
        subject_id,
        &page_id,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| Error::Database(format!("Failed to create article: {}", e)))?;

    Ok(Article {
        id: row.id,
        subject_type: row.subject_type,
        subject_id: row.subject_id,
        page_id: row.page_id,
        maintenance: row.maintenance,
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

    // The hard delete, not the trash: an article page belongs to its
    // `wiki_articles` row, and a trashed one would sit in Recently deleted
    // with the row still answering `get_article`. Cascades the row.
    crate::api::trash::purge(pool, crate::api::trash::TrashKind::Page, &article.page_id).await
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

/// How an article is maintained: `auto` or `never`, and nothing else.
///
/// This used to accept a third value, `always`, described here as "for an
/// article someone wants revisited whenever anything moves". It never did
/// that. The only reader of the column is `wiki_editor::due_articles`, which
/// tests `maintenance <> 'never'` — so `always` and `auto` selected the same
/// articles on the same interval behind the same drift gate, and nothing in
/// the app could produce the third value anyway. Migration 0034 took it out of
/// the constraint; this is the same removal one layer up.
pub async fn set_maintenance(
    pool: &PgPool,
    subject_type: &str,
    subject_id: &str,
    mode: &str,
) -> Result<()> {
    if !matches!(mode, "auto" | "never") {
        return Err(Error::InvalidInput(format!(
            "maintenance is auto or never — not {mode:?}"
        )));
    }
    let n = sqlx::query!(
        "UPDATE wiki_articles SET maintenance = $3 WHERE subject_type = $1 AND subject_id = $2",
        subject_type,
        subject_id,
        mode
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to set maintenance: {}", e)))?
    .rows_affected();
    if n == 0 {
        return Err(Error::NotFound(format!(
            "No article for {subject_type} {subject_id}"
        )));
    }
    Ok(())
}

/// "Mentioned in N articles" — every page whose prose links to this subject.
///
/// **The edge points at a SUBJECT, not at an article**, and that is the whole
/// correction. Entity articles are opt-in, so most subjects will never have prose; an
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
    let subject = crate::api::subjects::by_kind(subject_type).ok_or_else(|| {
        Error::InvalidInput(format!("Not a subject type: {subject_type}"))
    })?;
    // A subject with no route has no links pointing at it, because there is no
    // href anything could have written. Searching for `/chapter/{id})` anyway
    // returned an empty list that read as "nothing links here" rather than
    // "this cannot be linked", which are different answers.
    let Some(prefix) = subject.route else {
        return Ok(Vec::new());
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
        WHERE p.content LIKE $1 AND p.deleted_at IS NULL
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
    /// The version this edit produced. A page's last chat edit from before
    /// the after-convention (`VersionKind::BeforeChatEdit`) produced no
    /// version, so it carries the one it was made on, same as
    /// `before_version`, and the entry for the change INTO that row carries
    /// the same number. So `version_number` alone can repeat within a page;
    /// the pair (`version_number`, `before_version`) never does, and is what
    /// tells one entry from another.
    pub version_number: i64,
    /// The version this edit started from: putting it back undoes the edit.
    /// `None` only for an edit with nothing before it, which History never
    /// shows (a page's first version is its baseline).
    pub before_version: Option<i64>,
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

/// What `page_editor` wrote, labelled `'ai'`, on the snapshot it cut BEFORE
/// applying a chat edit, until chat edits followed the after-convention.
/// Rows with it are still on disk.
const BEFORE_CHAT_EDIT: &str = "Auto-saved before AI edit";

/// What one version row is, for reading History.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VersionKind {
    /// The state after someone's edit: the change into it is an entry,
    /// credited to its `created_by`. Every writer cuts these today
    /// (`pages::create_version_from_snapshot`), and the browser's autosaves
    /// always did.
    Edit,
    /// The state before a machine changed the whole page
    /// (`pages::cut_restore_point`). Not an edit, so no entry of its own; the
    /// next entry is diffed against it, so that entry's undo puts it back.
    RestorePoint,
    /// A chat edit's snapshot from before the convention: the page as it was
    /// before the chat changed it. A restore point too, except that the
    /// change OUT of it is the chat's, whoever cut the next row (usually an
    /// open editor autosaving the chat's change as its own).
    BeforeChatEdit,
}

fn version_kind(created_by: &str, description: Option<&str>) -> VersionKind {
    if created_by == "ai" && description == Some(BEFORE_CHAT_EDIT) {
        VersionKind::BeforeChatEdit
    } else if pages::is_restore_point(created_by, description) {
        VersionKind::RestorePoint
    } else {
        VersionKind::Edit
    }
}

/// Who made the change from the previous row into this one, or `None` when
/// that change is not an entry of its own.
///
/// `previous` is the row before this one (`version_number`), with its number.
/// Both readers decide every entry here, so they cannot disagree.
fn credit(
    previous: Option<(i64, VersionKind)>,
    version_number: i64,
    this: VersionKind,
    created_by: &str,
) -> Option<String> {
    // Numbers are MAX + 1 and pruning is the only delete (`pages::create_version`),
    // so a gap means the versions between were pruned. A diff across it would
    // hand their edits (a machine's revisions, usually) to this row's author,
    // so the row after a gap is a baseline, like a page's first.
    let previous = previous
        .filter(|(number, _)| number + 1 == version_number)
        .map(|(_, kind)| kind);
    match (previous, this) {
        // A page's first version is its baseline. There is no earlier text to
        // diff it against, and "wrote the whole page" is not an edit.
        (None, _) => None,
        (Some(VersionKind::BeforeChatEdit), _) => Some("ai".to_string()),
        (Some(_), VersionKind::Edit) => Some(created_by.to_string()),
        (Some(_), _) => None,
    }
}

#[derive(sqlx::FromRow)]
struct VersionRow {
    version_number: i64,
    created_by: String,
    description: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    yjs_snapshot: Option<Vec<u8>>,
}

/// An article's history: who changed it, when, and what changed. Newest first.
///
/// A version's snapshot is the state AFTER the edit it records, and its
/// `created_by` names who produced that state (`pages::create_version_from_snapshot`).
/// So the entry for version n is the diff from the version before it to
/// version n, credited to version n's author, and it carries `before_version`
/// so History can undo exactly that change.
///
/// Three kinds of row are read differently (`VersionKind`): a page's first
/// version is a baseline, not an edit; a restore point is a diff base, not an
/// edit; and a chat edit's snapshot from before the convention is a diff base
/// whose outgoing change is the chat's. A version after pruned ones is a
/// baseline too (`credit`). An entry whose text is the same on both sides is
/// no edit at all (an open editor autosaving a change it was sent) and is
/// left out.
pub async fn get_article_history(
    pool: &PgPool,
    subject_type: &str,
    subject_id: &str,
) -> Result<Vec<ArticleRevision>> {
    let Some(article) = get_article(pool, subject_type, subject_id).await? else {
        return Ok(Vec::new());
    };

    let rows: Vec<VersionRow> = sqlx::query_as(
        "SELECT version_number, created_by, description, created_at, yjs_snapshot \
         FROM app_page_versions WHERE page_id = $1 ORDER BY version_number ASC",
    )
    .bind(&article.page_id)
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

    // Text lives in the Yjs snapshot, not in `content_preview`, which holds
    // only the first 500 characters (or, on old rows, a label).
    let texts: Vec<String> = rows
        .iter()
        .map(|r| {
            r.yjs_snapshot
                .as_deref()
                .map(crate::server::yjs::extract_text_content)
                .unwrap_or_default()
        })
        .collect();
    let kinds: Vec<VersionKind> = rows
        .iter()
        .map(|r| version_kind(&r.created_by, r.description.as_deref()))
        .collect();

    // (version it produced, index of the row it started from, text after, author, when)
    let mut edits: Vec<(i64, usize, &str, String, chrono::DateTime<chrono::Utc>)> = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let Some(p) = i.checked_sub(1) else { continue };
        let previous = Some((rows[p].version_number, kinds[p]));
        let Some(author) = credit(previous, row.version_number, kinds[i], &row.created_by) else {
            continue;
        };
        edits.push((row.version_number, p, texts[i].as_str(), author, row.created_at));
    }
    if let Some(last) = rows.len().checked_sub(1) {
        if kinds[last] == VersionKind::BeforeChatEdit {
            edits.push((
                rows[last].version_number,
                last,
                current.as_str(),
                "ai".to_string(),
                rows[last].created_at,
            ));
        }
    }
    edits.retain(|(_, before, after, _, _)| texts[*before] != *after);

    let newest = edits.len().checked_sub(1);
    let mut out: Vec<ArticleRevision> = edits
        .into_iter()
        .enumerate()
        .map(|(k, (version_number, before, after, author, at))| ArticleRevision {
            version_number,
            before_version: Some(rows[before].version_number),
            author,
            at,
            diff: diff_lines(&texts[before], after),
            is_current: Some(k) == newest && after == current,
        })
        .collect();
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
    /// As `ArticleRevision::version_number`: it can repeat within a page, and
    /// the pair (`version_number`, `before_version`) never does.
    pub version_number: i64,
    /// The version this edit started from: putting it back undoes the edit.
    pub before_version: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct FeedRow {
    page_id: String,
    version_number: i64,
    created_by: String,
    description: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    before_version: Option<i64>,
    before_created_by: Option<String>,
    before_description: Option<String>,
    is_last: bool,
    subject_type: String,
    subject_id: String,
    title: String,
}

/// Every recent edit to any article, newest first — the room's front page.
///
/// This is the review surface that makes maintenance safe to turn on: the
/// switch is the consent, and this is where you see what that consent
/// produced. Without it the machine edits prose in a room nobody visits.
///
/// Read by exactly the rules of `get_article_history` (`credit`, the trailing
/// chat edit, and no entry where nothing changed), so an entry here is an
/// entry there with the same author and `before_version`.
///
/// Rows that are not entries (baselines, restore points, an open editor
/// autosaving a change it was sent) come in no fixed proportion to the ones
/// that are, so the rows are read a window at a time, newest first, until
/// `limit` entries survive or the rows run out. The scan stops after ten
/// rows per entry asked for, so a feed of almost nothing but baselines still
/// answers quickly; only then can it return fewer than `limit` while older
/// entries exist.
pub async fn get_history_feed(pool: &PgPool, limit: i64) -> Result<Vec<HistoryEntry>> {
    let limit = limit.clamp(1, 200);
    let window = limit * 2 + 10;
    let most_rows = limit * 10;

    let mut entries = Vec::new();
    let mut scanned = 0;
    // The last row read, as (created_at, version_number, page_id): the next
    // window starts below it.
    let mut below: Option<(chrono::DateTime<chrono::Utc>, i64, String)> = None;
    loop {
        // `lag` and `lead` run over the whole of each page's versions before
        // the window is cut, so a row at a window's edge still knows its
        // neighbours.
        let rows: Vec<FeedRow> = sqlx::query_as(
            r#"
            SELECT v.page_id, v.version_number, v.created_by, v.description, v.created_at,
                   v.before_version, v.before_created_by, v.before_description, v.is_last,
                   a.subject_type, a.subject_id, p.title
            FROM (
                SELECT page_id, version_number, created_by, description, created_at,
                       lag(version_number) OVER w AS before_version,
                       lag(created_by) OVER w AS before_created_by,
                       lag(description) OVER w AS before_description,
                       lead(version_number) OVER w IS NULL AS is_last
                FROM app_page_versions
                WHERE page_id IN (SELECT page_id FROM wiki_articles)
                WINDOW w AS (PARTITION BY page_id ORDER BY version_number)
            ) v
            JOIN wiki_articles a ON a.page_id = v.page_id
            JOIN app_pages p ON p.id = v.page_id
            WHERE $1::timestamptz IS NULL
               OR (v.created_at, v.version_number, v.page_id) < ($1, $2, $3)
            ORDER BY v.created_at DESC, v.version_number DESC, v.page_id DESC
            LIMIT $4
            "#,
        )
        .bind(below.as_ref().map(|b| b.0))
        .bind(below.as_ref().map(|b| b.1))
        .bind(below.as_ref().map(|b| b.2.clone()))
        .bind(window)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to load history feed: {}", e)))?;

        scanned += rows.len() as i64;
        entries.extend(feed_entries(pool, &rows).await?);
        let Some(last) = rows.last() else { break };
        if entries.len() as i64 >= limit || (rows.len() as i64) < window || scanned >= most_rows {
            break;
        }
        below = Some((last.created_at, last.version_number, last.page_id.clone()));
    }
    entries.truncate(limit as usize);
    Ok(entries)
}

/// The entries one window of feed rows holds, newest first.
async fn feed_entries(pool: &PgPool, rows: &[FeedRow]) -> Result<Vec<HistoryEntry>> {
    /// An entry before the no-change check. `after: None` is the live page.
    struct Candidate<'r> {
        row: &'r FeedRow,
        author: String,
        before: i64,
        after: Option<i64>,
    }
    let mut candidates = Vec::new();
    for row in rows {
        let this = version_kind(&row.created_by, row.description.as_deref());
        // A chat edit recorded before the convention is newer than the row it
        // was made on, so it comes first in a newest-first list.
        if row.is_last && this == VersionKind::BeforeChatEdit {
            candidates.push(Candidate {
                row,
                author: "ai".into(),
                before: row.version_number,
                after: None,
            });
        }
        let previous = row.before_version.zip(
            row.before_created_by
                .as_deref()
                .map(|by| version_kind(by, row.before_description.as_deref())),
        );
        if let (Some((before, _)), Some(author)) = (
            previous,
            credit(previous, row.version_number, this, &row.created_by),
        ) {
            candidates.push(Candidate {
                row,
                author,
                before,
                after: Some(row.version_number),
            });
        }
    }

    // The texts the no-change check compares, read once each.
    let mut wanted: Vec<(String, i64)> = Vec::new();
    for c in &candidates {
        wanted.push((c.row.page_id.clone(), c.before));
        if let Some(after) = c.after {
            wanted.push((c.row.page_id.clone(), after));
        }
    }
    wanted.sort();
    wanted.dedup();
    let (page_ids, numbers): (Vec<String>, Vec<i64>) = wanted.into_iter().unzip();
    let snapshots: Vec<(String, i64, Option<Vec<u8>>)> = sqlx::query_as(
        "SELECT v.page_id, v.version_number, v.yjs_snapshot FROM app_page_versions v \
         JOIN unnest($1::text[], $2::bigint[]) AS w(page_id, version_number) \
           ON w.page_id = v.page_id AND w.version_number = v.version_number",
    )
    .bind(&page_ids)
    .bind(&numbers)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to load history texts: {}", e)))?;
    let texts: std::collections::HashMap<(String, i64), String> = snapshots
        .into_iter()
        .map(|(page_id, number, snapshot)| {
            let text = snapshot
                .as_deref()
                .map(crate::server::yjs::extract_text_content)
                .unwrap_or_default();
            ((page_id, number), text)
        })
        .collect();

    let live_pages: Vec<String> = candidates
        .iter()
        .filter(|c| c.after.is_none())
        .map(|c| c.row.page_id.clone())
        .collect();
    let live: std::collections::HashMap<String, String> = if live_pages.is_empty() {
        Default::default()
    } else {
        sqlx::query_as::<_, (String, String)>(
            "SELECT id, content FROM app_pages WHERE id = ANY($1)",
        )
        .bind(&live_pages)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to load current texts: {}", e)))?
        .into_iter()
        .collect()
    };

    let empty = String::new();
    Ok(candidates
        .into_iter()
        .filter(|c| {
            let page = &c.row.page_id;
            let before = texts.get(&(page.clone(), c.before)).unwrap_or(&empty);
            let after = match c.after {
                Some(n) => texts.get(&(page.clone(), n)).unwrap_or(&empty),
                None => live.get(page).unwrap_or(&empty),
            };
            before != after
        })
        .map(|c| {
            let prefix = match c.row.subject_type.as_str() {
                "organization" => "org",
                other => other,
            };
            HistoryEntry {
                route: format!("/{prefix}/{}", c.row.subject_id),
                subject_type: c.row.subject_type.clone(),
                subject_id: c.row.subject_id.clone(),
                title: c.row.title.clone(),
                author: c.author,
                at: c.row.created_at,
                version_number: c.row.version_number,
                before_version: Some(c.before),
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


/// A subject's own name, which its article page is titled by. `None` for a
/// subject with no name of its own to follow (a day, a year) or none found.
pub async fn subject_name(pool: &PgPool, subject_type: &str, subject_id: &str) -> Result<Option<String>> {
    let sql = match subject_type {
        "person" => "SELECT name FROM wiki_people WHERE id = $1",
        "place" => "SELECT name FROM wiki_places WHERE id = $1",
        "organization" => "SELECT name FROM wiki_orgs WHERE id = $1",
        "story" => "SELECT title FROM wiki_stories WHERE id = $1",
        _ => return Ok(None),
    };
    sqlx::query_scalar::<_, String>(sql)
        .bind(subject_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to read the subject's name: {e}")))
}

/// Re-title a subject's article page from the subject. The page is created
/// with a copy of the name; every rename of a person, place, org or story
/// calls this, so the copy follows instead of keeping the old name forever.
/// A no-op when the subject has no article or the title already matches.
pub async fn retitle_article(pool: &PgPool, subject_type: &str, subject_id: &str) -> Result<()> {
    let Some(name) = subject_name(pool, subject_type, subject_id).await? else {
        return Ok(());
    };
    let name = name.trim();
    if name.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "UPDATE app_pages p SET title = $3 \
           FROM wiki_articles a \
          WHERE a.subject_type = $1 AND a.subject_id = $2 AND p.id = a.page_id \
            AND p.title IS DISTINCT FROM $3",
    )
    .bind(subject_type)
    .bind(subject_id)
    .bind(name)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to retitle the article: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The article page follows its subject's name, and a subject without
    /// an article is a no-op rather than an error.
    #[sqlx::test(migrations = "./migrations")]
    async fn an_article_follows_its_subjects_rename(pool: PgPool) {
        sqlx::query("INSERT INTO wiki_people (id, name) VALUES ('person_r1', 'Nick')")
            .execute(&pool)
            .await
            .unwrap();
        let a = create_article(&pool, "person", "person_r1", "Nick", "Prose.").await.unwrap();
        sqlx::query("UPDATE wiki_people SET name = 'David Okafor' WHERE id = 'person_r1'")
            .execute(&pool)
            .await
            .unwrap();
        retitle_article(&pool, "person", "person_r1").await.unwrap();
        let title: String = sqlx::query_scalar("SELECT title FROM app_pages WHERE id = $1")
            .bind(&a.page_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(title, "David Okafor");
        retitle_article(&pool, "person", "person_none").await.unwrap();
    }

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
        assert_eq!(a.maintenance, "auto", "an article is maintained unless the owner says otherwise");

        // (The old `date must stay NULL` assertion is gone with the column —
        // reflections were retired 2026-08-03, the column dropped 2026-08-28;
        // day-source separation is enforced by `kind` now, asserted above.)
    }

    /// The column had a third value that behaved exactly like `auto`, because
    /// the only reader tests `<> 'never'`. Both halves of its removal are
    /// asserted here: the validator refuses the word, and the constraint that
    /// migration 0034 rewrote refuses it underneath even if the validator ever
    /// stops.
    #[sqlx::test]
    async fn maintenance_is_auto_or_never_and_always_is_gone(pool: PgPool) {
        let a = create_article(&pool, "person", "person_m1", "Nick", "Prose.")
            .await
            .unwrap();
        assert_eq!(a.maintenance, "auto");

        for mode in ["auto", "never"] {
            set_maintenance(&pool, "person", "person_m1", mode)
                .await
                .unwrap_or_else(|e| panic!("{mode} should be accepted: {e}"));
        }

        let refused = set_maintenance(&pool, "person", "person_m1", "always")
            .await
            .unwrap_err()
            .to_string();
        assert!(refused.contains("auto or never"), "{refused}");

        let by_the_schema = sqlx::query("UPDATE wiki_articles SET maintenance = 'always' WHERE id = $1")
            .bind(&a.id)
            .execute(&pool)
            .await;
        assert!(
            by_the_schema.is_err(),
            "the check constraint must refuse it too, not just the validator"
        );
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
                project_id: None,
                format: None,
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

    /// A version of an article's page holding `text`, labelled the way its
    /// writer labels it.
    async fn version(
        pool: &PgPool,
        page_id: &str,
        text: &str,
        by: &str,
        description: Option<&str>,
    ) -> i64 {
        let snapshot = crate::server::yjs::state_from_text(text);
        pages::create_version_from_snapshot(pool, page_id, &snapshot, text, by, description)
            .await
            .unwrap()
            .version_number
    }

    async fn set_live(pool: &PgPool, page_id: &str, text: &str) {
        sqlx::query("UPDATE app_pages SET content = $2 WHERE id = $1")
            .bind(page_id)
            .bind(text)
            .execute(pool)
            .await
            .unwrap();
    }

    fn added(rev: &ArticleRevision) -> Vec<&str> {
        rev.diff
            .iter()
            .filter(|l| l.kind == "add")
            .map(|l| l.text.as_str())
            .collect()
    }

    /// The article's history and the feed's entries for it must say the same
    /// thing: History opens a feed entry by finding its revision.
    async fn same_in_the_feed(pool: &PgPool, subject_id: &str, hist: &[ArticleRevision]) {
        let feed: Vec<(i64, Option<i64>, String)> = get_history_feed(pool, 50)
            .await
            .unwrap()
            .into_iter()
            .filter(|e| e.subject_id == subject_id)
            .map(|e| (e.version_number, e.before_version, e.author))
            .collect();
        let article: Vec<(i64, Option<i64>, String)> = hist
            .iter()
            .map(|r| (r.version_number, r.before_version, r.author.clone()))
            .collect();
        assert_eq!(feed, article, "the feed and the article's history disagree");
    }

    /// A version's snapshot is the state AFTER the edit it records, and its
    /// `created_by` names who produced that state. So the entry for a version
    /// is the diff from the version before it, credited to that version's
    /// author, and the page's first version is a baseline rather than an edit.
    #[sqlx::test]
    async fn history_pairs_each_author_with_the_edit_they_made(pool: PgPool) {
        sqlx::query("INSERT INTO wiki_people (id, name) VALUES ('p_1', 'Sarah')")
            .execute(&pool)
            .await
            .unwrap();
        let a = create_article(&pool, "person", "p_1", "Sarah", "First line.\n")
            .await
            .unwrap();
        let first = version(&pool, &a.page_id, "First line.\n", "auto", Some("Auto-saved (idle)")).await;
        let theirs = version(
            &pool,
            &a.page_id,
            "First line.\nYour line.\n",
            "user",
            Some("edited"),
        )
        .await;
        let machine = version(
            &pool,
            &a.page_id,
            "First line.\nYour line.\nSecond line.\n",
            "ai",
            Some("added the second line"),
        )
        .await;
        set_live(&pool, &a.page_id, "First line.\nYour line.\nSecond line.\n").await;

        let hist = get_article_history(&pool, "person", "p_1").await.unwrap();
        assert_eq!(hist.len(), 2, "the first version is a baseline, not an edit");

        assert_eq!(hist[0].version_number, machine);
        assert_eq!(hist[0].author, "ai");
        assert_eq!(hist[0].before_version, Some(theirs));
        assert_eq!(added(&hist[0]), vec!["Second line."], "what THIS author added");
        assert!(hist[0].is_current);

        assert_eq!(hist[1].version_number, theirs);
        assert_eq!(hist[1].author, "user");
        assert_eq!(hist[1].before_version, Some(first));
        assert_eq!(added(&hist[1]), vec!["Your line."]);
        assert!(!hist[1].is_current);

        same_in_the_feed(&pool, "p_1", &hist).await;
    }

    /// A machine's whole-page change is one entry, credited to it, whose
    /// undo is the restore point kept before it. Neither the restore point
    /// nor an open editor autosaving the change it was sent is an entry,
    /// including a restore point that holds text no earlier version did
    /// (typing nobody autosaved).
    #[sqlx::test]
    async fn a_rewrite_is_one_entry_whose_undo_is_the_restore_point(pool: PgPool) {
        sqlx::query("INSERT INTO wiki_days (id, date) VALUES ('day_1', '2026-03-03')")
            .execute(&pool)
            .await
            .unwrap();
        let a = create_article(&pool, "day", "day_1", "3 March 2026", "The old page.\n")
            .await
            .unwrap();
        version(&pool, &a.page_id, "The old page.\n", "ai", Some("Written")).await;
        let kept = version(
            &pool,
            &a.page_id,
            "The old page.\nUntracked typing.\n",
            "auto",
            Some(pages::RESTORE_POINT),
        )
        .await;
        let rewrite = version(&pool, &a.page_id, "The new page.\n", "ai", Some("Rewritten")).await;
        version(&pool, &a.page_id, "The new page.\n", "auto", Some("Auto-saved (idle)")).await;
        set_live(&pool, &a.page_id, "The new page.\n").await;

        let hist = get_article_history(&pool, "day", "day_1").await.unwrap();
        assert_eq!(hist.len(), 1, "{hist:?}");
        assert_eq!(hist[0].version_number, rewrite);
        assert_eq!(hist[0].author, "ai");
        assert_eq!(hist[0].before_version, Some(kept));
        assert_eq!(added(&hist[0]), vec!["The new page."]);
        assert!(hist[0].is_current);
        assert!(hist.iter().all(|r| r.version_number != kept), "no entry for the restore point");

        same_in_the_feed(&pool, "day_1", &hist).await;
    }

    /// Pruning leaves a gap in the version numbers. The version after it is
    /// a baseline: diffing it against the version before the gap would hand
    /// the pruned edits (the machine's, here) to its author.
    #[sqlx::test]
    async fn a_version_after_pruned_ones_is_a_baseline(pool: PgPool) {
        sqlx::query("INSERT INTO wiki_people (id, name) VALUES ('p_1', 'Sarah')")
            .execute(&pool)
            .await
            .unwrap();
        let a = create_article(&pool, "person", "p_1", "Sarah", "First line.\n")
            .await
            .unwrap();
        version(&pool, &a.page_id, "First line.\n", "user", Some("edited")).await;
        let pruned = version(
            &pool,
            &a.page_id,
            "First line.\nThe machine's line.\n",
            "ai",
            Some("added a line"),
        )
        .await;
        version(
            &pool,
            &a.page_id,
            "First line.\nThe machine's line.\nYour line.\n",
            "user",
            Some("edited"),
        )
        .await;
        sqlx::query("DELETE FROM app_page_versions WHERE page_id = $1 AND version_number = $2")
            .bind(&a.page_id)
            .bind(pruned)
            .execute(&pool)
            .await
            .unwrap();
        set_live(&pool, &a.page_id, "First line.\nThe machine's line.\nYour line.\n").await;

        let hist = get_article_history(&pool, "person", "p_1").await.unwrap();
        assert!(hist.is_empty(), "{hist:?}");
        same_in_the_feed(&pool, "p_1", &hist).await;
    }

    /// Two chat edits in a row from before the convention left two legacy
    /// rows and the live page. Both edits are entries, and they share a
    /// `version_number`: the pair with `before_version` is what tells them
    /// apart, in the article's history and in the feed alike.
    #[sqlx::test]
    async fn two_legacy_chat_edits_in_a_row_are_told_apart(pool: PgPool) {
        sqlx::query("INSERT INTO wiki_people (id, name) VALUES ('p_1', 'Sarah')")
            .execute(&pool)
            .await
            .unwrap();
        let a = create_article(&pool, "person", "p_1", "Sarah", "Draft.\n")
            .await
            .unwrap();
        version(&pool, &a.page_id, "Draft.\n", "auto", Some("Auto-saved (idle)")).await;
        let before_first =
            version(&pool, &a.page_id, "Draft.\nTyped.\n", "ai", Some(BEFORE_CHAT_EDIT)).await;
        let before_second = version(
            &pool,
            &a.page_id,
            "Draft.\nTyped.\nChat line.\n",
            "ai",
            Some(BEFORE_CHAT_EDIT),
        )
        .await;
        set_live(&pool, &a.page_id, "Draft.\nTyped.\nChat line.\nAnother chat line.\n").await;

        let hist = get_article_history(&pool, "person", "p_1").await.unwrap();
        let pairs: Vec<(i64, Option<i64>)> =
            hist.iter().map(|r| (r.version_number, r.before_version)).collect();
        assert_eq!(
            pairs,
            vec![
                (before_second, Some(before_second)),
                (before_second, Some(before_first)),
            ]
        );
        assert!(hist.iter().all(|r| r.author == "ai"));
        assert_eq!(added(&hist[0]), vec!["Another chat line."]);
        assert_eq!(added(&hist[1]), vec!["Chat line."]);

        let feed: Vec<(i64, Option<i64>)> = get_history_feed(&pool, 50)
            .await
            .unwrap()
            .into_iter()
            .map(|e| (e.version_number, e.before_version))
            .collect();
        let mut unique = feed.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), feed.len(), "no two feed entries share an identity: {feed:?}");
        same_in_the_feed(&pool, "p_1", &hist).await;
    }

    /// Rows that are not entries can outnumber the ones that are by any
    /// amount (an open editor autosaving the same text, again and again).
    /// The feed reads on past them to fill its limit with older entries.
    #[sqlx::test]
    async fn the_feed_reads_past_rows_that_are_not_entries(pool: PgPool) {
        sqlx::query("INSERT INTO wiki_people (id, name) VALUES ('p_1', 'Sarah'), ('p_2', 'Nick')")
            .execute(&pool)
            .await
            .unwrap();
        let older = create_article(&pool, "person", "p_1", "Sarah", "First line.\n")
            .await
            .unwrap();
        version(&pool, &older.page_id, "First line.\n", "auto", Some("Auto-saved (idle)")).await;
        let edit = version(&pool, &older.page_id, "First line.\nYour line.\n", "user", None).await;

        let newer = create_article(&pool, "person", "p_2", "Nick", "Same.\n")
            .await
            .unwrap();
        for _ in 0..24 {
            version(&pool, &newer.page_id, "Same.\n", "auto", Some("Auto-saved (idle)")).await;
        }

        let feed = get_history_feed(&pool, 3).await.unwrap();
        assert_eq!(feed.len(), 1, "{feed:?}");
        assert_eq!(feed[0].subject_id, "p_1");
        assert_eq!(feed[0].version_number, edit);
    }

    /// Before chat edits followed the convention, `page_editor` cut its
    /// snapshot BEFORE the edit, labelled 'ai'. Those rows are diff bases,
    /// and the change out of one is the chat's, even when the next row is an
    /// open editor's autosave of it, and even when nothing was cut after it
    /// at all (the edit is then in the live page only).
    #[sqlx::test]
    async fn a_chat_edit_from_before_the_convention_is_still_the_chats(pool: PgPool) {
        sqlx::query("INSERT INTO wiki_people (id, name) VALUES ('p_1', 'Sarah')")
            .execute(&pool)
            .await
            .unwrap();
        let a = create_article(&pool, "person", "p_1", "Sarah", "Draft.\n")
            .await
            .unwrap();
        version(&pool, &a.page_id, "Draft.\n", "auto", Some("Auto-saved (idle)")).await;
        let before_first =
            version(&pool, &a.page_id, "Draft.\nTyped.\n", "ai", Some(BEFORE_CHAT_EDIT)).await;
        let autosaved = version(
            &pool,
            &a.page_id,
            "Draft.\nTyped.\nChat line.\n",
            "auto",
            Some("Auto-saved (idle)"),
        )
        .await;
        let before_second = version(
            &pool,
            &a.page_id,
            "Draft.\nTyped.\nChat line.\n",
            "ai",
            Some(BEFORE_CHAT_EDIT),
        )
        .await;
        set_live(&pool, &a.page_id, "Draft.\nTyped.\nChat line.\nAnother chat line.\n").await;

        let hist = get_article_history(&pool, "person", "p_1").await.unwrap();
        assert_eq!(hist.len(), 2, "{hist:?}");

        assert_eq!(hist[0].author, "ai", "the edit after the last snapshot");
        assert_eq!(hist[0].version_number, before_second);
        assert_eq!(hist[0].before_version, Some(before_second));
        assert_eq!(added(&hist[0]), vec!["Another chat line."]);
        assert!(hist[0].is_current);

        assert_eq!(hist[1].author, "ai", "not the autosave that caught it");
        assert_eq!(hist[1].version_number, autosaved);
        assert_eq!(hist[1].before_version, Some(before_first));
        assert_eq!(added(&hist[1]), vec!["Chat line."]);

        same_in_the_feed(&pool, "p_1", &hist).await;
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
            "INSERT INTO search_embeddings (id, ontology, record_id, model, \
             chunk_index, content, doc_hash) \
             VALUES ('se_1', 'wiki_article', $1, 'test-model', 0, 'Prose.', 'h')",
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

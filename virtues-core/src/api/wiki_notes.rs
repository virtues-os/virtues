//! Notes — the margin of the record.
//!
//! A note is prose about a subject, written for later: a correction, an
//! observation, something the article does not yet say. Migration 0082 renamed
//! this from `wiki_marginalia` and gave the word "marginalia" to the document
//! reader, where a note beside a passage is the literal thing it describes.
//!
//! The covenant this table exists to hold:
//!
//! **Point, don't decide.** A machine note says what it saw and links to where
//! it saw it. *"Sarah may have moved to Denver — group thread, Jul 12, tone
//! ambiguous; the article doesn't mention it"* is useful even when wrong,
//! because the citation makes it checkable in seconds. A bare claim with a
//! confidence score is worthless when wrong, because there is nothing to check.
//! `wiki_notes_machine_must_cite` puts that in the schema rather than in a
//! prompt.
//!
//! **The writer never touches the graph.** Notes are the machine's only channel
//! into the record. It may not write `wiki_entity_refs`, not at any confidence,
//! not flagged. Promotion is a human accepting, or an editor pass gated on
//! `auto_update`.
//!
//! **Notes never age out.** A note whose purpose is "for later" that deletes
//! itself before later arrives has defeated itself, silently. Three exits, all
//! events: accepted, dismissed, absorbed.

use sqlx::PgPool;

use crate::error::{Error, Result};

/// A note in the margin of some subject.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Note {
    pub id: i64,
    pub subject_type: String,
    pub subject_id: String,
    /// One of the 0033 kinds. `correction` disputes what the article says;
    /// `observation` is about the subject and the article lacks it. That
    /// distinction is what an *accept* branches on — edit the sentence, or
    /// append a paragraph — and it is why quote anchors are unnecessary.
    pub kind: String,
    pub body: String,
    pub author: String,
    pub source_refs: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub resolution: Option<String>,
}

/// Notes on a subject. Open ones by default — a resolved note is history, and
/// the rail is a working surface.
pub async fn list_notes(
    pool: &PgPool,
    subject_type: &str,
    subject_id: &str,
    include_resolved: bool,
) -> Result<Vec<Note>> {
    let rows = sqlx::query!(
        r#"
        SELECT id, subject_type, subject_id, kind, body, author, source_refs,
               created_at, resolution
        FROM wiki_notes
        WHERE subject_type = $1 AND subject_id = $2
          AND ($3 OR resolved_at IS NULL)
        ORDER BY created_at DESC
        "#,
        subject_type,
        subject_id,
        include_resolved
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list notes: {}", e)))?;

    Ok(rows
        .into_iter()
        .map(|r| Note {
            id: r.id,
            subject_type: r.subject_type,
            subject_id: r.subject_id,
            kind: r.kind,
            body: r.body,
            author: r.author,
            source_refs: r.source_refs,
            created_at: r.created_at,
            resolution: r.resolution,
        })
        .collect())
}

/// How many notes are open across the whole record — the Overview's
/// what-changed module. Runtime query on purpose: one scalar is not worth
/// an .sqlx entry.
pub async fn count_open_total(pool: &PgPool) -> Result<i64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM wiki_notes WHERE resolved_at IS NULL",
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to count open notes: {}", e)))
}

/// How many open notes a subject has — the badge, and the Overview count.
pub async fn count_open(pool: &PgPool, subject_type: &str, subject_id: &str) -> Result<i64> {
    sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "n!" FROM wiki_notes
        WHERE subject_type = $1 AND subject_id = $2 AND resolved_at IS NULL
        "#,
        subject_type,
        subject_id
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to count notes: {}", e)))
}

/// Leave a note yourself.
///
/// Human notes need no citation — you were there. The CHECK enforces that only
/// for `author = 'ai'`, which is the asymmetry the covenant is built on.
pub async fn create_note(
    pool: &PgPool,
    subject_type: &str,
    subject_id: &str,
    kind: &str,
    body: &str,
) -> Result<Note> {
    let body = body.trim();
    if body.is_empty() {
        return Err(Error::InvalidInput("A note needs a body".into()));
    }

    let r = sqlx::query!(
        r#"
        INSERT INTO wiki_notes (subject_type, subject_id, kind, body, author)
        VALUES ($1, $2, $3, $4, 'human')
        RETURNING id, subject_type, subject_id, kind, body, author, source_refs,
                  created_at, resolution
        "#,
        subject_type,
        subject_id,
        kind,
        body
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create note: {}", e)))?;

    Ok(Note {
        id: r.id,
        subject_type: r.subject_type,
        subject_id: r.subject_id,
        kind: r.kind,
        body: r.body,
        author: r.author,
        source_refs: r.source_refs,
        created_at: r.created_at,
        resolution: r.resolution,
    })
}

/// Close a note.
///
/// `accepted` and `dismissed` are human acts. `absorbed` is the machine
/// reporting that a rewrite used this note — reported, never inferred, because
/// an editor knows which notes it was given but not whether its output reflects
/// any one of them.
pub async fn resolve_note(pool: &PgPool, id: i64, resolution: &str) -> Result<()> {
    let by = match resolution {
        "accepted" | "dismissed" => "human",
        "absorbed" => "ai",
        other => {
            return Err(Error::InvalidInput(format!(
                "Unknown resolution: {other}"
            )))
        }
    };

    let n = sqlx::query!(
        r#"
        UPDATE wiki_notes
        SET resolution = $2, resolved_by = $3, resolved_at = now()
        WHERE id = $1 AND resolved_at IS NULL
        "#,
        id,
        resolution,
        by
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to resolve note: {}", e)))?
    .rows_affected();

    if n == 0 {
        return Err(Error::NotFound(format!("No open note {id}")));
    }
    Ok(())
}

/// The most notes one pass may leave. Enforced in code, not asked for in a
/// prompt.
///
/// Restraint is what models comply with least, and the arithmetic is brutal: a
/// rich day names roughly a dozen subjects, so a note each is ~4,000 a year and
/// the rail becomes a wall nobody reads — the feature dying of its own success.
/// A cap makes the failure visible (it is logged when hit) instead of gradual.
pub const MAX_NOTES_PER_RUN: usize = 3;

/// A note a pass wants to leave, before it is allowed to.
pub struct ProposedNote {
    pub subject_type: String,
    pub subject_id: String,
    pub kind: String,
    pub body: String,
    /// Where it came from. A machine note without this is refused by the
    /// database, not by a code path someone can forget.
    pub source_refs: Vec<String>,
}

/// Write a pass's notes, capped, and stamp the subjects it touched as dirty.
///
/// Returns how many were written. Anything past the cap is dropped and logged —
/// the log line is the measurement that tells you whether the prompt's bar is
/// too low, which is otherwise unfalsifiable.
pub async fn write_machine_notes(
    pool: &PgPool,
    proposed: Vec<ProposedNote>,
) -> Result<usize> {
    let offered = proposed.len();
    if offered > MAX_NOTES_PER_RUN {
        tracing::warn!(
            offered,
            cap = MAX_NOTES_PER_RUN,
            "note cap hit — dropping the excess. If this recurs nightly the \
             writer's bar is too low, which is a prompt to tighten, not a cap to raise"
        );
    }

    let mut written = 0;
    for n in proposed.into_iter().take(MAX_NOTES_PER_RUN) {
        if n.source_refs.is_empty() {
            // Belt as well as braces: the CHECK would refuse this anyway, but
            // failing here says which note and why.
            tracing::warn!(subject = %n.subject_id, "machine note without a citation — refused");
            continue;
        }

        sqlx::query(
            "INSERT INTO wiki_notes (subject_type, subject_id, kind, body, author, source_refs) \
             VALUES ($1, $2, $3, $4, 'ai', $5)",
        )
        .bind(&n.subject_type)
        .bind(&n.subject_id)
        .bind(&n.kind)
        .bind(&n.body)
        .bind(serde_json::json!(n.source_refs))
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to write note: {}", e)))?;

        // New evidence about a settled article. `wiki_articles.dirty_at` is
        // authoritative for prose staleness — the 0033 `dirty_at` columns mean
        // "new evidence about an object" and one of them is already taken by the
        // magnet to mean a stale centroid. Do not conflate them.
        sqlx::query(
            "UPDATE wiki_articles SET dirty_at = now() \
             WHERE subject_type = $1 AND subject_id = $2 AND dirty_at IS NULL",
        )
        .bind(&n.subject_type)
        .bind(&n.subject_id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to stamp dirty: {}", e)))?;

        written += 1;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The covenant, enforced by the database rather than by a prompt: a
    /// machine note without a citation cannot exist.
    #[sqlx::test]
    async fn a_machine_note_must_cite(pool: PgPool) {
        let uncited = sqlx::query(
            "INSERT INTO wiki_notes (subject_type, subject_id, kind, body, author) \
             VALUES ('person', 'p_1', 'observation', 'Sarah moved.', 'ai')",
        )
        .execute(&pool)
        .await;
        assert!(uncited.is_err(), "an uncited machine note must be refused");

        let cited = sqlx::query(
            "INSERT INTO wiki_notes (subject_type, subject_id, kind, body, author, source_refs) \
             VALUES ('person', 'p_1', 'observation', 'Sarah moved.', 'ai', \
             '[\"/record/data_communication_message/m_1\"]'::jsonb)",
        )
        .execute(&pool)
        .await;
        assert!(cited.is_ok(), "a cited machine note is fine");
    }

    /// A human was there; they do not have to cite themselves.
    #[sqlx::test]
    async fn a_human_note_needs_no_citation(pool: PgPool) {
        let n = create_note(&pool, "person", "p_1", "memo", "Ask about the move.")
            .await
            .unwrap();
        assert_eq!(n.author, "human");
        assert_eq!(count_open(&pool, "person", "p_1").await.unwrap(), 1);
    }

    /// Resolution is an event with an author, and it happens once.
    #[sqlx::test]
    async fn resolving_closes_a_note_exactly_once(pool: PgPool) {
        let n = create_note(&pool, "person", "p_1", "memo", "Ask about the move.")
            .await
            .unwrap();

        resolve_note(&pool, n.id, "accepted").await.unwrap();
        assert_eq!(count_open(&pool, "person", "p_1").await.unwrap(), 0);

        // Already closed — resolving again is not a silent no-op.
        assert!(resolve_note(&pool, n.id, "dismissed").await.is_err());

        let all = list_notes(&pool, "person", "p_1", true).await.unwrap();
        assert_eq!(all.len(), 1, "a resolved note is history, not deleted");
        assert_eq!(all[0].resolution.as_deref(), Some("accepted"));
    }

    /// The cap is the floor under the prompt, so it has to hold even when the
    /// pass is enthusiastic.
    #[sqlx::test]
    async fn the_cap_holds_however_many_are_offered(pool: PgPool) {
        let proposed: Vec<ProposedNote> = (0..10)
            .map(|i| ProposedNote {
                subject_type: "person".into(),
                subject_id: format!("p_{i}"),
                kind: "observation".into(),
                body: format!("Something about p_{i}."),
                source_refs: vec!["/record/data_communication_message/m_1".into()],
            })
            .collect();

        let written = write_machine_notes(&pool, proposed).await.unwrap();
        assert_eq!(written, MAX_NOTES_PER_RUN);

        let total: i64 = sqlx::query_scalar("SELECT count(*) FROM wiki_notes")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(total, MAX_NOTES_PER_RUN as i64);
    }

    /// An uncited machine note is dropped by name rather than blowing up the
    /// whole pass — one bad note must not cost the other two.
    #[sqlx::test]
    async fn an_uncited_note_is_skipped_not_fatal(pool: PgPool) {
        let written = write_machine_notes(
            &pool,
            vec![
                ProposedNote {
                    subject_type: "person".into(),
                    subject_id: "p_1".into(),
                    kind: "observation".into(),
                    body: "Uncited.".into(),
                    source_refs: vec![],
                },
                ProposedNote {
                    subject_type: "person".into(),
                    subject_id: "p_2".into(),
                    kind: "observation".into(),
                    body: "Cited.".into(),
                    source_refs: vec!["/record/data_communication_message/m_1".into()],
                },
            ],
        )
        .await
        .unwrap();
        assert_eq!(written, 1, "the cited note still lands");
    }

    /// Writing a note marks the subject's article stale, which is how the
    /// maintenance pass learns there is anything to do.
    #[sqlx::test]
    async fn a_note_stamps_the_article_dirty(pool: PgPool) {
        sqlx::query("INSERT INTO wiki_people (id, canonical_name) VALUES ('p_1', 'Sarah')")
            .execute(&pool)
            .await
            .unwrap();
        let a = crate::api::wiki_articles::create_article(&pool, "person", "p_1", "Sarah", "Prose.")
            .await
            .unwrap();

        write_machine_notes(
            &pool,
            vec![ProposedNote {
                subject_type: "person".into(),
                subject_id: "p_1".into(),
                kind: "correction".into(),
                body: "The article says Denver; the thread says Boulder.".into(),
                source_refs: vec!["/record/data_communication_message/m_1".into()],
            }],
        )
        .await
        .unwrap();

        let dirty: Option<chrono::DateTime<chrono::Utc>> =
            sqlx::query_scalar("SELECT dirty_at FROM wiki_articles WHERE id = $1")
                .bind(&a.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(dirty.is_some(), "a note makes the article due for review");
    }

    /// `resolved_at` and `resolution` cannot disagree.
    #[sqlx::test]
    async fn resolution_columns_stay_in_step(pool: PgPool) {
        let n = create_note(&pool, "person", "p_1", "memo", "x").await.unwrap();
        let half = sqlx::query("UPDATE wiki_notes SET resolved_at = now() WHERE id = $1")
            .bind(n.id)
            .execute(&pool)
            .await;
        assert!(half.is_err(), "a timestamp without a resolution is not a state");
    }
}

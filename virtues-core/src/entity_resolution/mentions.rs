//! Mention resolution — the deterministic half.
//!
//! Its siblings (`people`, `places`) resolve *structured* columns: a
//! `from_email` is already an identity, a GPS fix is already a location. Those
//! are joins, and they are where most links in the system come from.
//!
//! This module handles what falls out of *prose* — a name spoken in a
//! transcript, typed in a message. That is a genuinely different problem, and
//! we refuse to guess at it.
//!
//! # The rule
//!
//! A floating mention resolves iff its normalized surface matches **exactly
//! one** entity, by canonical name, nickname, or a human-written alias.
//!
//!   1 candidate   → link it. wiki_entity_refs, resolved_by = 'alias'.
//!   0 candidates  → stay floating. It goes in the review queue.
//!   2+ candidates → stay floating. Three Sarahs; we cannot know, so we don't.
//!
//! No embeddings. No fuzzy match. No LLM adjudicator. No confidence score. The
//! machine never decides *which* Sarah — a human does that once, in the review
//! queue, and the alias they write makes every future "Sarah" deterministic.
//!
//! # Why nothing links by similarity
//!
//! Because a wrong link is a lie about someone's life, and it is invisible: it
//! looks exactly like a right one. Whereas an unresolved mention is merely
//! *dust* — still stored, still searchable, still surfacing in the queue,
//! costing nothing but a decision the user hasn't made yet. The two failure
//! modes are not symmetric, so the thresholds should not be either. Precision
//! over recall, and precision by *construction* rather than by tuning.
//!
//! The three-Sarahs case is the point, not the gap: those mentions stay dust
//! forever, and that is correct until a human tells us otherwise.

use serde::Serialize;
use sqlx::Row;

use crate::database::Database;
use crate::error::Result;
use crate::ids;

/// One candidate entity for a surface.
struct Candidate {
    entity_type: &'static str,
    entity_id: String,
}

/// Outcome of a single sweep.
#[derive(Debug, Default, Serialize)]
pub struct MentionStats {
    /// Mentions that matched exactly one entity and are now linked.
    pub linked: usize,
    /// Mentions left floating because nothing matched — the review queue.
    pub unmatched: usize,
    /// Mentions left floating because the surface is ambiguous (2+ entities).
    /// Not an error; see the module docs.
    pub ambiguous: usize,
}

/// Resolve every floating mention we can, deterministically.
///
/// Runs over ALL floating mentions, not a time window: a mention floats until a
/// human writes the alias that resolves it, and that can happen months later.
/// When it does, this sweep is what backfills the mention's whole history —
/// which is why linking one surface once retroactively links all 47 of its
/// past occurrences.
/// Teach the resolver who owns the box.
///
/// On real data the single loudest name in the mention queue was the user's own —
/// "Adam", floating across 42 separate records, waiting for a human to answer
/// "who is this?" about himself. The box has always known: `app_user_profile`
/// carries his name. Nothing ever told entity resolution.
///
/// The fix needs no new machinery. The owner is a person like any other, and the
/// alias table already exists — so give him his own names as aliases and the
/// ordinary resolver links them. If a SECOND Adam ever appears in the graph, the
/// surface becomes ambiguous and floats again, which is exactly right: the machine
/// stops guessing the moment guessing becomes possible.
///
/// Idempotent: aliases are a set, and the profile can change.
async fn ensure_owner_is_known(db: &Database) -> Result<()> {
    let owner: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT full_name, preferred_name FROM app_user_profile LIMIT 1")
            .fetch_optional(db.pool())
            .await?;

    let Some((full_name, preferred)) = owner else { return Ok(()) };
    let Some(full_name) = full_name.filter(|s| !s.trim().is_empty()) else {
        return Ok(());
    };

    // The names he is actually called: his given name, and whatever he goes by.
    let mut names: Vec<String> = vec![full_name.trim().to_string()];
    if let Some(first) = full_name.split_whitespace().next() {
        names.push(first.to_string());
    }
    if let Some(p) = preferred.filter(|s| !s.trim().is_empty()) {
        names.push(p.trim().to_string());
    }

    // Attach them to the person record that IS him — matched on his full name,
    // which is how he was created. Never invent a person here: if he is not in the
    // graph yet, there is nothing to be the owner of.
    for name in names {
        sqlx::query(
            "UPDATE wiki_people \
             SET aliases = CASE WHEN aliases ? $1 THEN aliases ELSE aliases || to_jsonb($1::text) END, \
                 relationship_category = COALESCE(relationship_category, 'self') \
             WHERE canonical_name = $2",
        )
        .bind(&name)
        .bind(&full_name)
        .execute(db.pool())
        .await?;
    }

    Ok(())
}

pub async fn resolve_mentions(db: &Database) -> Result<MentionStats> {
    let mut stats = MentionStats::default();

    // Before asking a human who anyone is, make sure we are not about to ask them
    // who THEY are.
    ensure_owner_is_known(db).await?;

    // Group by surface: every mention of "sarah" shares one answer, so we do
    // one lookup per distinct surface rather than per mention. This is also
    // what makes a human's single decision fan out across the backlog.
    let surfaces = sqlx::query(
        r#"
        SELECT normalized, mention_type, COUNT(*) AS n
        FROM er_mentions
        WHERE status = 'floating'
          AND normalized IS NOT NULL
          AND mention_type IN ('person', 'place', 'org')
        GROUP BY normalized, mention_type
        ORDER BY n DESC
        "#,
    )
    .fetch_all(db.pool())
    .await?;

    for row in surfaces {
        let normalized: String = row.get("normalized");
        let mention_type: String = row.get("mention_type");
        let n: i64 = row.get("n");

        let candidates = lookup(db, &normalized, &mention_type).await?;

        match candidates.len() {
            1 => {
                let c = &candidates[0];
                let linked = link_surface(db, &normalized, &mention_type, c).await?;
                stats.linked += linked;
            }
            0 => stats.unmatched += n as usize,
            _ => {
                // Ambiguous. Leave as dust and say so once, quietly — this is
                // expected behavior, not a failure.
                tracing::debug!(
                    surface = %normalized,
                    candidates = candidates.len(),
                    mentions = n,
                    "surface is ambiguous — leaving floating (a human must disambiguate)"
                );
                stats.ambiguous += n as usize;
            }
        }
    }

    if stats.linked > 0 || stats.ambiguous > 0 {
        tracing::info!(
            linked = stats.linked,
            unmatched = stats.unmatched,
            ambiguous = stats.ambiguous,
            "mention resolution swept"
        );
    }

    Ok(stats)
}

/// Every entity a surface could mean. The count is the whole decision.
///
/// Matches canonical name, nickname (people only — it predates aliases and is
/// still user-authored), and the alias array. `aliases` is stored lowercased
/// and `normalized` arrives lowercased, so `?` is an exact containment check.
async fn lookup(db: &Database, normalized: &str, mention_type: &str) -> Result<Vec<Candidate>> {
    let mut out = Vec::new();

    match mention_type {
        "person" => {
            let rows = sqlx::query(
                r#"
                SELECT id FROM wiki_people
                WHERE LOWER(canonical_name) = $1
                   OR LOWER(nickname) = $1
                   OR aliases ? $1
                "#,
            )
            .bind(normalized)
            .fetch_all(db.pool())
            .await?;
            for r in rows {
                out.push(Candidate {
                    entity_type: "person",
                    entity_id: r.get("id"),
                });
            }
        }
        "place" => {
            let rows = sqlx::query(
                r#"
                SELECT id FROM wiki_places
                WHERE LOWER(name) = $1 OR aliases ? $1
                "#,
            )
            .bind(normalized)
            .fetch_all(db.pool())
            .await?;
            for r in rows {
                out.push(Candidate {
                    entity_type: "place",
                    entity_id: r.get("id"),
                });
            }
        }
        "org" => {
            let rows = sqlx::query(
                r#"
                SELECT id FROM wiki_orgs
                WHERE LOWER(canonical_name) = $1 OR aliases ? $1
                "#,
            )
            .bind(normalized)
            .fetch_all(db.pool())
            .await?;
            for r in rows {
                out.push(Candidate {
                    // wiki_entity_refs spells it out; er_mentions abbreviates.
                    entity_type: "organization",
                    entity_id: r.get("id"),
                });
            }
        }
        _ => {}
    }

    Ok(out)
}

/// Link every floating mention of this surface to the entity, and write the
/// refs. Idempotent — the refs table's unique index absorbs replays.
async fn link_surface(
    db: &Database,
    normalized: &str,
    mention_type: &str,
    candidate: &Candidate,
) -> Result<usize> {
    // The refs, first: a mention is only "linked" once its ref exists, so this
    // order means a crash mid-way leaves mentions floating (they get retried)
    // rather than linked-but-unreferenced (invisible, and never retried).
    let mentions = sqlx::query(
        r#"
        SELECT id, source_table, source_id, role, reference_time
        FROM er_mentions
        WHERE status = 'floating' AND normalized = $1 AND mention_type = $2
        "#,
    )
    .bind(normalized)
    .bind(mention_type)
    .fetch_all(db.pool())
    .await?;

    for m in &mentions {
        let source_table: String = m.get("source_table");
        let source_id: String = m.get("source_id");
        let role: Option<String> = m.get("role");
        let role = role.unwrap_or_else(|| "mentioned".to_string());
        let reference_time: Option<chrono::DateTime<chrono::Utc>> = m.get("reference_time");

        let ref_id = ids::generate_id("eref", &[&source_id, &candidate.entity_id, &role]);
        sqlx::query(
            r#"
            INSERT INTO wiki_entity_refs
                (id, entity_type, entity_id, source_table, source_id, role,
                 confidence, resolved_by, timestamp)
            VALUES ($1, $2, $3, $4, $5, $6, 1.0, 'alias', $7)
            ON CONFLICT (entity_id, source_table, source_id, role) DO NOTHING
            "#,
        )
        .bind(&ref_id)
        .bind(candidate.entity_type)
        .bind(&candidate.entity_id)
        .bind(&source_table)
        .bind(&source_id)
        .bind(&role)
        .bind(reference_time)
        .execute(db.pool())
        .await?;
    }

    // Then flip the mentions. `entity_type` here is er_mentions' vocabulary
    // ('org'), not wiki_entity_refs' ('organization') — they differ, and
    // conflating them silently breaks the review page's filters.
    let updated = sqlx::query(
        r#"
        UPDATE er_mentions
        SET status = 'linked', entity_type = $3, entity_id = $4, confidence = 1.0
        WHERE status = 'floating' AND normalized = $1 AND mention_type = $2
        "#,
    )
    .bind(normalized)
    .bind(mention_type)
    .bind(mention_type)
    .bind(&candidate.entity_id)
    .execute(db.pool())
    .await?;

    tracing::debug!(
        surface = %normalized,
        entity_id = %candidate.entity_id,
        mentions = updated.rows_affected(),
        "linked surface to entity"
    );

    Ok(updated.rows_affected() as usize)
}

/// Teach the system that a surface means an entity, then resolve the backlog.
///
/// This is the *only* way a prose mention ever becomes a link, and it is only
/// ever called from the review queue — i.e. by a human. Writing the alias is
/// the decision; the sweep that follows is just bookkeeping, and it is what
/// makes one click resolve every past and future occurrence.
pub async fn add_alias(
    db: &Database,
    entity_type: &str,
    entity_id: &str,
    surface: &str,
) -> Result<MentionStats> {
    let normalized = surface.trim().to_lowercase();
    if normalized.is_empty() {
        return Err(crate::Error::InvalidInput("alias cannot be empty".into()));
    }

    let table = match entity_type {
        "person" => "wiki_people",
        "place" => "wiki_places",
        "org" | "organization" => "wiki_orgs",
        other => {
            return Err(crate::Error::InvalidInput(format!(
                "cannot alias entity type `{other}`"
            )))
        }
    };

    // Append if absent. `-` then `||` rather than a bare `||` so re-adding an
    // alias doesn't duplicate the element.
    let sql = format!(
        "UPDATE {table} SET aliases = (aliases - $2::text) || to_jsonb($2::text) WHERE id = $1"
    );
    let res = sqlx::query(&sql)
        .bind(entity_id)
        .bind(&normalized)
        .execute(db.pool())
        .await?;

    if res.rows_affected() == 0 {
        return Err(crate::Error::NotFound(format!(
            "{entity_type} `{entity_id}` not found"
        )));
    }

    tracing::info!(
        surface = %normalized,
        entity_type,
        entity_id,
        "alias written — resolving backlog"
    );

    resolve_mentions(db).await
}

/// Dismiss a surface: it names nothing we care about ("Unsubscribe", "Sent from
/// my iPhone"). Never asked about again.
///
/// The mentions are NOT deleted — dismissal only removes them from the queue.
/// They stay searchable, and a later `add_alias` for the same surface can still
/// pick them up if the user changes their mind. We do not delete user data.
pub async fn dismiss_surface(db: &Database, normalized: &str, mention_type: &str) -> Result<usize> {
    let res = sqlx::query(
        r#"
        UPDATE er_mentions
        SET status = 'dismissed'
        WHERE status = 'floating' AND normalized = $1 AND mention_type = $2
        "#,
    )
    .bind(normalized.trim().to_lowercase())
    .bind(mention_type)
    .execute(db.pool())
    .await?;

    Ok(res.rows_affected() as usize)
}

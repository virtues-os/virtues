//! The mention review queue — the one place a prose name becomes a person.
//!
//! Nothing in the pipeline links a name from prose by guessing. The resolver
//! links only exact, unambiguous matches; everything else floats here, and a
//! human decides.
//!
//! # Why this is grouped by surface, not by mention
//!
//! A per-mention queue does not converge. A year of transcripts is thousands of
//! rows, the user clears forty, and the backlog grows faster than they work. It
//! becomes an inbox, and an inbox that can't be emptied gets abandoned.
//!
//! Grouped by *surface*, the queue is one row per distinct name — a few hundred
//! at most, the top thirty covering nearly all the mass. And each decision is
//! permanent: linking "Sarah" writes an alias, which backfills all 47 past
//! mentions AND resolves every future one without ever asking again. The cost
//! is one decision per name, once. That is what makes the queue drain and stay
//! drained.
//!
//! # What a decision can be
//!
//!   link    → write the alias, backfill the history, never ask again
//!   create  → mint the entity, then link (same thing, one step earlier)
//!   dismiss → it names nothing ("Unsubscribe"). Never ask again.
//!
//! Dismiss does not delete. The mentions stay searchable — dust — and a later
//! link for the same surface still picks them up. We do not delete user data.

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

use crate::database::Database;
use crate::error::Result;

/// One row of the queue: a surface, how often it occurs, and enough evidence to
/// answer it without leaving the page.
#[derive(Debug, Serialize)]
pub struct SurfaceGroup {
    pub normalized: String,
    /// The most common raw spelling — what we show. ("Sarah", not "sarah".)
    pub surface: String,
    pub mention_type: String,
    pub count: i64,
    /// How many DISTINCT records this surface appears in.
    ///
    /// The recurrence signal, and the one the badge is gated on — not `count`.
    /// A name said three times in one rambling voice memo is one event; a name
    /// that turns up in three separate records is a fixture of the user's life.
    ///
    /// This is what lets the badge reach zero. Badging every floating surface
    /// means a permanently non-zero count — every one-off name anyone ever says
    /// — and a count that never clears becomes wallpaper within a week. Then the
    /// queue is dead no matter how good the page is.
    pub sources: i64,
    /// The sentences it was found in. THE reason this page is answerable: a bare
    /// name is not reviewable, a quotation is.
    pub snippets: Vec<String>,
    pub first_seen: Option<chrono::DateTime<chrono::Utc>>,
    pub last_seen: Option<chrono::DateTime<chrono::Utc>>,
    /// Entities this surface could plausibly mean. Populated ONLY when the
    /// surface is ambiguous (2+ candidates) — a single candidate would already
    /// have been linked by the resolver, so it never reaches this queue.
    /// These are shown as suggestions a human confirms, never applied.
    pub candidates: Vec<Candidate>,
}

#[derive(Debug, Serialize)]
pub struct Candidate {
    pub entity_type: String,
    pub entity_id: String,
    pub name: String,
    /// Why we're suggesting it. "Sarah Smith" for the surface "Sarah" is a
    /// substring, which is exactly the kind of inference the machine must never
    /// act on alone — but is a fine thing to put in front of a human.
    pub reason: String,
}

/// The queue: floating surfaces, most frequent first.
///
/// Frequency order is the whole ergonomic argument. The names that matter in
/// someone's life recur; the noise appears once. Sorting by count puts the
/// thirty decisions that resolve most of the backlog at the top.
pub async fn list_floating_surfaces(db: &PgPool, limit: i64) -> Result<Vec<SurfaceGroup>> {
    let rows = sqlx::query(
        r#"
        SELECT
            normalized,
            mention_type,
            COUNT(*)                                        AS count,
            COUNT(DISTINCT source_id)                       AS sources,
            MODE() WITHIN GROUP (ORDER BY surface)          AS surface,
            MIN(created_at)                                 AS first_seen,
            MAX(created_at)                                 AS last_seen,
            (ARRAY_REMOVE(ARRAY_AGG(snippet ORDER BY created_at DESC), NULL))[1:3]
                                                            AS snippets
        FROM er_mentions
        WHERE status = 'floating'
          AND normalized IS NOT NULL
          AND mention_type IN ('person', 'place', 'org')
        GROUP BY normalized, mention_type
        ORDER BY COUNT(*) DESC, MAX(created_at) DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(db)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let normalized: String = row.get("normalized");
        let mention_type: String = row.get("mention_type");
        let candidates = suggest(db, &normalized, &mention_type).await?;

        out.push(SurfaceGroup {
            surface: row.get("surface"),
            count: row.get("count"),
            sources: row.get("sources"),
            first_seen: row.get("first_seen"),
            last_seen: row.get("last_seen"),
            snippets: row
                .get::<Option<Vec<String>>, _>("snippets")
                .unwrap_or_default(),
            normalized,
            mention_type,
            candidates,
        });
    }

    Ok(out)
}

/// Entities a human might plausibly mean by this surface.
///
/// SUGGESTIONS ONLY — nothing here is ever auto-applied. Substring containment
/// ("Sarah" ⊂ "Sarah Smith") is precisely the inference that manufactures wrong
/// links when a machine acts on it, and precisely the hint a human resolves in
/// half a second. So we compute it, show it, and refuse to use it.
async fn suggest(db: &PgPool, normalized: &str, mention_type: &str) -> Result<Vec<Candidate>> {
    let pattern = format!("%{normalized}%");

    let rows = match mention_type {
        "person" => {
            sqlx::query(
                r#"
                SELECT id, canonical_name AS name FROM wiki_people
                WHERE LOWER(canonical_name) LIKE $1 OR LOWER(nickname) LIKE $1
                LIMIT 5
                "#,
            )
            .bind(&pattern)
            .fetch_all(db)
            .await?
        }
        "place" => {
            sqlx::query("SELECT id, name FROM wiki_places WHERE LOWER(name) LIKE $1 LIMIT 5")
                .bind(&pattern)
                .fetch_all(db)
                .await?
        }
        "org" => {
            sqlx::query(
                "SELECT id, canonical_name AS name FROM wiki_orgs WHERE LOWER(canonical_name) LIKE $1 LIMIT 5",
            )
            .bind(&pattern)
            .fetch_all(db)
            .await?
        }
        _ => return Ok(Vec::new()),
    };

    Ok(rows
        .into_iter()
        .map(|r| {
            let name: String = r.get("name");
            Candidate {
                reason: if name.to_lowercase() == normalized {
                    // Exact match yet still floating ⇒ another entity matches
                    // too. This IS the three-Sarahs case, surfaced honestly.
                    "exact name — ambiguous with another entity".to_string()
                } else {
                    format!("name contains “{normalized}”")
                },
                entity_type: mention_type.to_string(),
                entity_id: r.get("id"),
                name,
            }
        })
        .collect())
}

#[derive(Debug, Deserialize)]
pub struct LinkSurfaceRequest {
    pub normalized: String,
    pub mention_type: String,
    pub entity_id: String,
}

/// Link a surface to an existing entity: write the alias, resolve the backlog.
///
/// This single call is what makes the queue converge — see the module docs.
pub async fn link_surface(db: &Database, req: LinkSurfaceRequest) -> Result<serde_json::Value> {
    let stats = crate::entity_resolution::mentions::add_alias(
        db,
        &req.mention_type,
        &req.entity_id,
        &req.normalized,
    )
    .await?;

    Ok(serde_json::json!({
        "linked": stats.linked,
        "still_floating": stats.unmatched + stats.ambiguous,
    }))
}

#[derive(Debug, Deserialize)]
pub struct DismissSurfaceRequest {
    pub normalized: String,
    pub mention_type: String,
}

/// Dismiss a surface. The mentions are NOT deleted — they stop being asked
/// about. Still searchable, still recoverable if the user changes their mind.
pub async fn dismiss_surface(db: &Database, req: DismissSurfaceRequest) -> Result<serde_json::Value> {
    let n = crate::entity_resolution::mentions::dismiss_surface(
        db,
        &req.normalized,
        &req.mention_type,
    )
    .await?;

    Ok(serde_json::json!({ "dismissed": n }))
}

#[derive(Debug, Deserialize)]
pub struct CreateFromSurfaceRequest {
    pub normalized: String,
    pub mention_type: String,
    /// What to call the new entity. Defaults to the surface as written.
    pub name: Option<String>,
}

/// Mint an entity from a surface, then link it. "Create" in the queue.
pub async fn create_from_surface(
    db: &Database,
    req: CreateFromSurfaceRequest,
) -> Result<serde_json::Value> {
    let name = req
        .name
        .unwrap_or_else(|| req.normalized.clone())
        .trim()
        .to_string();
    if name.is_empty() {
        return Err(crate::Error::InvalidInput("name cannot be empty".into()));
    }

    let (table, id_prefix, name_col) = match req.mention_type.as_str() {
        "person" => ("wiki_people", "person", "canonical_name"),
        "place" => ("wiki_places", "place", "name"),
        "org" => ("wiki_orgs", "org", "canonical_name"),
        other => {
            return Err(crate::Error::InvalidInput(format!(
                "cannot create entity of type `{other}`"
            )))
        }
    };

    let entity_id = crate::ids::generate_id(id_prefix, &[&req.normalized]);
    let sql = format!("INSERT INTO {table} (id, {name_col}) VALUES ($1, $2) ON CONFLICT (id) DO NOTHING");
    sqlx::query(&sql)
        .bind(&entity_id)
        .bind(&name)
        .execute(db.pool())
        .await?;

    // The alias is what does the work; creating the row is just where it lands.
    let stats = crate::entity_resolution::mentions::add_alias(
        db,
        &req.mention_type,
        &entity_id,
        &req.normalized,
    )
    .await?;

    Ok(serde_json::json!({
        "entity_id": entity_id,
        "linked": stats.linked,
    }))
}

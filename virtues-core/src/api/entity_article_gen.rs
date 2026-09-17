//! An entity's FIRST DRAFT — the opening article on a person, place or
//! organization page.
//!
//! **This module writes once.** It is the one-shot half of article resolution
//! (`agents/record/article-resolution.md`): there is no existing text to
//! respect and nothing to go looking for, so one call with a dossier in the
//! prompt is the whole job. Everything after the first draft — deciding the
//! article is due, researching what changed, and making a surgical edit that
//! cannot lose a sentence the person wrote — belongs to the `wiki_editor`
//! applet and its agent phase, which is the only place a Yjs-aware writer can
//! run.
//!
//! It is an *article*, not a summary: it summarizes nothing, because the thing
//! it would summarize (the records) is rendered directly beneath it on the
//! page. It is the prose the page is written in.
//!
//! Shape: dossier → `BearerClient` → `create_article`. The prose lands in the
//! article's own page, where the person's edits and the editor's share one
//! document.
//!
//! **The gate is consent.** An entity has no article until someone asks for
//! one, and none is maintained until they set `maintenance` on it. Writing
//! once and maintaining forever are different decisions and get different
//! switches. The thresholds that used to decide this for them
//! (`MIN_REFS_TO_WRITE`, `MIN_NEW_REFS`) are gone: on the real box they
//! cleared 226 entities on five months of records — hundreds of unrequested
//! model calls, recurring forever, with nothing in the UI to say the box was
//! spending on them.

use sqlx::PgPool;

use crate::error::{Error, Result};

/// Most recent records shown to the model.
const DOSSIER_RECORDS: usize = 40;

/// Hard cap on dossier characters.
const MAX_TOTAL_CHARS: usize = 14000;

const SYSTEM_PROMPT: &str = r#"You are the editor of a private wiki about one person's life — their own personal wikipedia, readable only by them. You are writing the article for ONE entity in that wiki: a person they know, a place they go, or an organization in their life. "You"/"your" in the article always refers to the wiki's owner; the entity is written about in the third person.

You are given the entity's structured facts, the raw records that reference it (messages, emails, calendar events, visits, transactions), and narrated days it appears in.

WRITE:
- Two to four short paragraphs, in the register of a well-edited encyclopedia that happens to be about a private life: precise, warm, unhurried. Markdown is allowed but keep it to plain paragraphs — no headings, no lists.
- Open with what the entity IS in the owner's life (the relationship, the role, the pattern), then how it shows up in the record (rhythms, places, recurring context).
- LINK entities: when you mention an entity listed under "Entities you may link", link it by copying its exact markdown link, e.g. [Maya](/person/person_ab12) or [March 3, 2026](/day/day_2026-03-03) for a listed day. Link each once, on first mention. Never invent a link or link anything not listed.
- Ground every claim in the material given. Describe patterns, never essence ("your lunches with her tend to…", never "she is the kind of person who…"). If the record is one-sided (only messages, only transactions), say so plainly.
- Absence of data is not data: never invent feelings, motives, or events. No flattery, no horoscope lines that could be true of anyone.
- This is the article's FIRST edition. Write it whole. It is maintained afterwards by editing, not by rewriting, so do not write anything that would have to be replaced wholesale to stay true.

Output only the article."#;

/// One due entity: id + which table it lives in.
#[derive(Debug, Clone)]
struct DueEntity {
    id: String,
    kind: String, // 'person' | 'place' | 'organization'
    refs: i64,
}


/// Write a subject's first article, now, because someone asked for it.
///
/// This is the create path, and it is the one an applet subprocess can safely
/// take: a page with `content` and no `yjs_state` seeds its CRDT correctly on
/// first open. Re-writing an existing article is refused rather than silently
/// dropped — see `refresh_due_entity_articles`.
pub async fn write_entity_article_now(
    pool: &PgPool,
    subject_type: &str,
    subject_id: &str,
) -> Result<crate::api::wiki_articles::Article> {
    if let Some(existing) = crate::api::wiki_articles::get_article(pool, subject_type, subject_id).await? {
        return Err(Error::InvalidInput(format!(
            "{subject_type} already has an article (page {}). Rewriting an existing              article needs the agent phase, so it is not available yet.",
            existing.page_id
        )));
    }

    let refs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wiki_refs WHERE entity_id = $1",
    )
    .bind(subject_id)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to count refs: {}", e)))?;

    let entity = DueEntity {
        id: subject_id.to_string(),
        kind: subject_type.to_string(),
        refs,
    };

    let prompt = build_dossier(pool, &entity).await?;
    tracing::info!(
        entity = %entity.id, kind = %entity.kind, refs,
        prompt_chars = prompt.len(),
        "writing first entity article (user-requested)"
    );

    let raw = call_virtues_api(pool, &prompt).await?;
    let article = parse_article(&raw);
    if article.is_empty() {
        return Err(Error::ExternalApi(
            "LLM returned an empty entity article".to_string(),
        ));
    }

    let title = entity_title(pool, subject_type, subject_id).await?;
    let created =
        crate::api::wiki_articles::create_article(pool, subject_type, subject_id, &title, &article)
            .await?;

    // Record what the editor just wrote. Without this the article has no
    // `machine_text`, and the first revision cannot tell the machine's own
    // first draft from something the person typed — it would mark the whole
    // article as theirs and then be forbidden from ever editing it.
    crate::api::wiki_editor::record_edition(pool, &created.id, &article).await?;

    Ok(created)
}

/// The subject's display name, for the article page's title.
/// Named `entity_*` and taking `entity_*` because that is what it means: the
/// three ENTITY-shaped subjects and no others. A subject is the wider word —
/// a day and a year are subjects too — and this function has nothing to say
/// about them. See `agents/build/glossary.md`.
async fn entity_title(pool: &PgPool, entity_type: &str, entity_id: &str) -> Result<String> {
    let sql = match entity_type {
        "person" => "SELECT name FROM wiki_people WHERE id = $1",
        "place" => "SELECT name FROM wiki_places WHERE id = $1",
        "organization" => "SELECT name FROM wiki_orgs WHERE id = $1",
        other => {
            return Err(Error::InvalidInput(format!(
                "Not an entity: {other}. A day or a year is a subject with an \
                 article, but it is not something this writer can title."
            )))
        }
    };
    sqlx::query_scalar(sql)
        .bind(entity_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to load the entity: {}", e)))?
        .ok_or_else(|| Error::NotFound(format!("No {entity_type}: {entity_id}")))
}

/// Assemble everything the editor reads: header facts, the recent record,
/// co-occurring entities (the link allowlist), and narrated days.
///
/// It deliberately reads NO never-written column. `seen_count`, `first_seen`,
/// `last_seen`, `article` and `wiki_people.notes` have no writer anywhere, so
/// every one of them either fed the model a zero or fed it nothing — and
/// "Interactions on record: 0" was reaching the prompt for entities with
/// thousands of refs. The honest count is `entity.refs`, straight from
/// `wiki_refs`, and it is already in the header line.
async fn build_dossier(pool: &PgPool, entity: &DueEntity) -> Result<String> {
    use sqlx::Row;

    let mut p = String::new();

    // ── Header facts + previous edition, per kind ──
    let (name, facts): (String, String) = match entity.kind.as_str() {
        "person" => {
            let row = sqlx::query(
                "SELECT name, relationship_category, nickname \
                 FROM wiki_people WHERE id = $1",
            )
            .bind(&entity.id)
            .fetch_one(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to load person: {}", e)))?;
            let name: String = row.get("name");
            let mut f = String::new();
            if let Ok(Some(v)) = row.try_get::<Option<String>, _>("relationship_category") {
                f.push_str(&format!("- Relationship: {}\n", v));
            }
            if let Ok(Some(v)) = row.try_get::<Option<String>, _>("nickname") {
                f.push_str(&format!("- Nickname: {}\n", v));
            }
            (name, f)
        }
        "place" => {
            let row = sqlx::query(
                "SELECT name, category, address FROM wiki_places WHERE id = $1",
            )
            .bind(&entity.id)
            .fetch_one(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to load place: {}", e)))?;
            let name: String = row.get("name");
            let mut f = String::new();
            if let Ok(Some(v)) = row.try_get::<Option<String>, _>("category") {
                f.push_str(&format!("- Category: {}\n", v));
            }
            if let Ok(Some(v)) = row.try_get::<Option<String>, _>("address") {
                f.push_str(&format!("- Address: {}\n", v));
            }
            (name, f)
        }
        _ => {
            let row = sqlx::query(
                "SELECT name, organization_type, relationship_type, role_title \
                 FROM wiki_orgs WHERE id = $1",
            )
            .bind(&entity.id)
            .fetch_one(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to load org: {}", e)))?;
            let name: String = row.get("name");
            let mut f = String::new();
            if let Ok(Some(v)) = row.try_get::<Option<String>, _>("organization_type") {
                f.push_str(&format!("- Type: {}\n", v));
            }
            if let Ok(Some(v)) = row.try_get::<Option<String>, _>("relationship_type") {
                f.push_str(&format!("- Relationship: {}\n", v));
            }
            if let Ok(Some(v)) = row.try_get::<Option<String>, _>("role_title") {
                f.push_str(&format!("- Owner's role: {}\n", v));
            }
            (name, f)
        }
    };

    p.push_str(&format!(
        "The article is about: {} ({}), total records referencing it: {}\n",
        name, entity.kind, entity.refs
    ));
    if !facts.is_empty() {
        p.push_str(&format!("\n## Structured facts\n{}", facts));
    }

    // ── The recent record ──
    let page = super::wiki::get_entity_records_page(
        pool,
        &entity.id,
        0,
        DOSSIER_RECORDS as i64,
        "",
        &[],
        true,
    )
    .await?;
    if !page.items.is_empty() {
        let lines: Vec<String> = page
            .items
            .iter()
            .map(|r| {
                let role = r.role.as_deref().map(|x| format!(" [{}]", x)).unwrap_or_default();
                let preview = r
                    .preview
                    .as_deref()
                    .map(|x| format!(" — {}", cap(x, 160)))
                    .unwrap_or_default();
                format!(
                    "- {} {}{}: {}{}",
                    r.timestamp.format("%Y-%m-%d"),
                    r.source_type,
                    role,
                    cap(&r.label, 120),
                    preview
                )
            })
            .collect();
        p.push_str(&format!(
            "\n## The record (most recent {} of {})\n{}\n",
            lines.len(),
            page.total,
            lines.join("\n")
        ));
    }

    // ── Link allowlist: co-occurring entities + narrated days ──
    let mut links: Vec<String> = Vec::new();
    let co: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT DISTINCT er2.entity_type, er2.entity_id
        FROM wiki_refs er1
        JOIN wiki_refs er2
          ON er1.source_table = er2.source_table AND er1.source_id = er2.source_id
        WHERE er1.entity_id = $1 AND er2.entity_id <> $1
        LIMIT 12
        "#,
    )
    .bind(&entity.id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (etype, eid) in &co {
        let (route, name_sql) = match etype.as_str() {
            "person" => ("person", "SELECT name FROM wiki_people WHERE id = $1"),
            "place" => ("place", "SELECT name FROM wiki_places WHERE id = $1"),
            "organization" => ("org", "SELECT name FROM wiki_orgs WHERE id = $1"),
            _ => continue,
        };
        if let Ok(Some(n)) = sqlx::query_scalar::<_, String>(name_sql)
            .bind(eid)
            .fetch_optional(pool)
            .await
        {
            links.push(format!("- [{}](/{}/{})", n, route, eid));
        }
    }
    // The day's LEDE, not its `epigraph`: that column is NULL on every row of
    // every box, because the narrate prompt forbids the model to write one.
    // Six narrated days were being offered to the editor as bare dates.
    let days: Vec<(chrono::NaiveDate, Option<String>)> = sqlx::query_as(&format!(
        r#"
        SELECT DISTINCT d.date, {lede} AS lede
        FROM wiki_days d
        JOIN wiki_day_prose dp ON dp.day_id = d.id AND dp.prose IS NOT NULL
        JOIN wiki_refs er ON date(er.occurred_at) = d.date
        WHERE er.entity_id = $1
        ORDER BY d.date DESC
        LIMIT 6
        "#,
        lede = crate::api::wiki_editor::lede_sql("dp.prose")
    ))
    .bind(&entity.id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (date, lede) in &days {
        let label = date.format("%B %-d, %Y");
        match lede {
            Some(e) => links.push(format!(
                "- [{}](/day/day_{}) — narrated day: \"{}\"",
                label,
                date.format("%Y-%m-%d"),
                cap(e, 200)
            )),
            None => links.push(format!("- [{}](/day/day_{})", label, date.format("%Y-%m-%d"))),
        }
    }
    if !links.is_empty() {
        p.push_str(&format!(
            "\n## Entities you may link (copy the exact markdown link)\n{}\n",
            links.join("\n")
        ));
    }

    if p.len() > MAX_TOTAL_CHARS {
        p.truncate(MAX_TOTAL_CHARS);
        p.push_str("\n\n(material truncated)");
    }

    Ok(p)
}

fn cap(s: &str, n: usize) -> String {
    let t = s.trim();
    if t.chars().count() <= n {
        return t.to_string();
    }
    let mut out: String = t.chars().take(n).collect();
    out.push('…');
    out
}

/// Runs on the **Lite** slot, not Chat.
///
/// Background writing must not ride the slot the owner picked for
/// conversation. It silently did, and the effect was that choosing a premium
/// model to talk to made every applet premium too — the same call costing 15×
/// more without anyone deciding that. There is also no per-day spend ceiling
/// anywhere in the system, so the model slot is the actual cost control.
///
/// The shared completion helper resolves the Lite slot through the owner's
/// background pin — that pin exists exactly for jobs like this one.
async fn call_virtues_api(pool: &PgPool, user_prompt: &str) -> Result<String> {
    crate::virtues_api::completion::system_completion(
        pool,
        virtues_registry::models::ModelSlot::Lite,
        "entity_article",
        SYSTEM_PROMPT,
        user_prompt,
        // Arrangement of the person's own words: thinking off, no cap. The
        // 900-token literal that sat here was a guess that stopped being
        // true the day the model started thinking inside it.
        crate::virtues_api::request::Thinking::Off,
        0.4,
    )
    .await
}

/// Strip code fences and return the article prose.
fn parse_article(raw: &str) -> String {
    raw.trim()
        .trim_start_matches("```markdown")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_strips_fences() {
        assert_eq!(parse_article("```markdown\nAn article.\n```"), "An article.");
    }

    #[test]
    fn cap_appends_ellipsis() {
        assert_eq!(cap("abcdef", 3), "abc…");
        assert_eq!(cap("abc", 3), "abc");
    }

    /// THE TEST THAT WOULD HAVE CAUGHT THIS. The header queries here are raw
    /// `sqlx::query`, so a renamed column breaks at runtime, not build time —
    /// which is how a phantom `ref_count` (renamed to `seen_count` in 0002)
    /// silently killed person and place article generation while org survived.
    /// This runs all three header paths against the real migration-built
    /// schema; a future rename fails here instead of on a fielded box.
    #[sqlx::test]
    async fn dossier_header_sql_matches_schema(pool: sqlx::PgPool) {
        for (table, id, kind) in [
            ("wiki_people", "person_t1", "person"),
            ("wiki_places", "place_t1", "place"),
            ("wiki_orgs", "org_t1", "organization"),
        ] {
            sqlx::query(&format!(
                "INSERT INTO {table} (id, name) VALUES ($1, 'Dossier Subject')"
            ))
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();

            let entity = DueEntity { id: id.to_string(), kind: kind.to_string(), refs: 1 };
            let dossier = build_dossier(&pool, &entity)
                .await
                .unwrap_or_else(|e| panic!("{kind} dossier failed against live schema: {e}"));
            assert!(dossier.contains("Dossier Subject"), "{kind} dossier missing subject name");
        }
    }
}

//! Entity summary generation — the wikipedia-style written record on each
//! entity page.
//!
//! Where the day summary narrates a *day*, this narrates a *relationship*: a
//! short, grounded article about a person, place, or organization, written
//! from the raw records that reference it (`wiki_entity_refs`) and revised
//! only when enough NEW evidence has accumulated since the last edition —
//! growth-gated, not timer-gated, so a quiet entity never burns a model call
//! and an active one stays current.
//!
//! Mirrors `narrative_identity_gen.rs`: gate → dossier → `BearerClient` with
//! `Purpose::System` → write back. The summary lives in its own `summary`
//! column; `content`/`notes` remain the user's own writing and are never
//! touched here.

use sqlx::PgPool;

use crate::error::{Error, Result};

/// Below this many total refs an entity has no article — the record is too
/// thin to say anything a contact card doesn't. Deliberately high to start.
const MIN_REFS_TO_SUMMARIZE: i64 = 15;

/// A new edition requires at least this many refs beyond the count recorded
/// at the last edition ("enough new data").
const MIN_NEW_REFS: i64 = 10;

/// Editions per run. The applet runs hourly; a bounded batch drains a backlog
/// without a cost spike.
const MAX_ENTITIES_PER_RUN: i64 = 2;

/// Most recent records shown to the model.
const DOSSIER_RECORDS: usize = 40;

/// Hard cap on dossier characters.
const MAX_TOTAL_CHARS: usize = 14000;

const SYSTEM_PROMPT: &str = r#"You are the editor of a private wiki about one person's life — their own personal wikipedia, readable only by them. You are writing the article for ONE entity in that wiki: a person they know, a place they go, or an organization in their life. "You"/"your" in the article always refers to the wiki's owner; the entity is written about in the third person.

You are given the entity's structured facts, the raw records that reference it (messages, emails, calendar events, visits, transactions), narrated days it appears in, and the previous edition of the article if one exists.

WRITE:
- Two to four short paragraphs, in the register of a well-edited encyclopedia that happens to be about a private life: precise, warm, unhurried. Markdown is allowed but keep it to plain paragraphs — no headings, no lists.
- Open with what the entity IS in the owner's life (the relationship, the role, the pattern), then how it shows up in the record (rhythms, places, recurring context), then what has changed lately if the previous edition missed it.
- LINK entities: when you mention an entity listed under "Entities you may link", link it by copying its exact markdown link, e.g. [Maya](/person/person_ab12) or [March 3, 2026](/day/day_2026-03-03) for a listed day. Link each once, on first mention. Never invent a link or link anything not listed.
- Ground every claim in the material given. Describe patterns, never essence ("your lunches with her tend to…", never "she is the kind of person who…"). If the record is one-sided (only messages, only transactions), say so plainly.
- Absence of data is not data: never invent feelings, motives, or events. No flattery, no horoscope lines that could be true of anyone.
- This is an edition, not an append: rewrite the whole article, carrying forward what the previous edition got right.

Output only the article."#;

/// One due entity: id + which table it lives in.
#[derive(Debug, Clone)]
struct DueEntity {
    id: String,
    kind: String, // 'person' | 'place' | 'organization'
    refs: i64,
}

/// Find entities whose record has outgrown their article, most-outgrown first,
/// and write a fresh edition for each. Returns how many were written.
pub async fn refresh_due_entity_summaries(pool: &PgPool) -> Result<usize> {
    let due: Vec<(String, String, i64)> = sqlx::query_as(
        r#"
        SELECT e.id, e.kind, c.refs
        FROM (
            SELECT id, 'person' AS kind, summary_ref_count FROM wiki_people
            UNION ALL
            SELECT id, 'place', summary_ref_count FROM wiki_places
            UNION ALL
            SELECT id, 'organization', summary_ref_count FROM wiki_orgs
        ) e
        JOIN LATERAL (
            SELECT count(*) AS refs FROM wiki_entity_refs r WHERE r.entity_id = e.id
        ) c ON true
        WHERE c.refs >= $1
          AND c.refs - e.summary_ref_count >= $2
        ORDER BY c.refs - e.summary_ref_count DESC
        LIMIT $3
        "#,
    )
    .bind(MIN_REFS_TO_SUMMARIZE)
    .bind(MIN_NEW_REFS)
    .bind(MAX_ENTITIES_PER_RUN)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to find due entities: {}", e)))?;

    let mut written = 0;
    for (id, kind, refs) in due {
        let entity = DueEntity { id, kind, refs };
        match summarize_entity(pool, &entity).await {
            Ok(()) => written += 1,
            Err(e) => {
                // One failed article must not block the batch — the gate will
                // re-offer this entity next run.
                tracing::warn!(entity = %entity.id, error = %e, "entity summary failed");
            }
        }
    }
    Ok(written)
}

async fn summarize_entity(pool: &PgPool, entity: &DueEntity) -> Result<()> {
    let prompt = build_dossier(pool, entity).await?;

    tracing::info!(
        entity = %entity.id,
        kind = %entity.kind,
        refs = entity.refs,
        prompt_chars = prompt.len(),
        "writing entity summary edition"
    );

    let raw = call_virtues_api(pool, &prompt).await?;
    let article = parse_article(&raw);
    if article.is_empty() {
        return Err(Error::ExternalApi(
            "LLM returned an empty entity summary".to_string(),
        ));
    }

    let update = match entity.kind.as_str() {
        "person" => {
            "UPDATE wiki_people SET summary = $2, summarized_at = now(), \
             summary_ref_count = $3, updated_at = now() WHERE id = $1"
        }
        "place" => {
            "UPDATE wiki_places SET summary = $2, summarized_at = now(), \
             summary_ref_count = $3, updated_at = now() WHERE id = $1"
        }
        _ => {
            "UPDATE wiki_orgs SET summary = $2, summarized_at = now(), \
             summary_ref_count = $3, updated_at = now() WHERE id = $1"
        }
    };
    sqlx::query(update)
        .bind(&entity.id)
        .bind(&article)
        .bind(entity.refs as i32)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to write entity summary: {}", e)))?;

    Ok(())
}

/// Assemble everything the editor reads: header facts, the recent record,
/// co-occurring entities (the link allowlist), narrated days, and the
/// previous edition.
async fn build_dossier(pool: &PgPool, entity: &DueEntity) -> Result<String> {
    use sqlx::Row;

    let mut p = String::new();

    // ── Header facts + previous edition, per kind ──
    let (name, facts, previous): (String, String, Option<String>) = match entity.kind.as_str() {
        "person" => {
            let row = sqlx::query(
                "SELECT canonical_name, relationship_category, nickname, notes, \
                        first_interaction::text AS fi, last_interaction::text AS li, \
                        interaction_count, summary \
                 FROM wiki_people WHERE id = $1",
            )
            .bind(&entity.id)
            .fetch_one(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to load person: {}", e)))?;
            let name: String = row.get("canonical_name");
            let mut f = String::new();
            if let Ok(Some(v)) = row.try_get::<Option<String>, _>("relationship_category") {
                f.push_str(&format!("- Relationship: {}\n", v));
            }
            if let Ok(Some(v)) = row.try_get::<Option<String>, _>("nickname") {
                f.push_str(&format!("- Nickname: {}\n", v));
            }
            if let Ok(Some(v)) = row.try_get::<Option<String>, _>("notes") {
                f.push_str(&format!("- Owner's own notes: {}\n", cap(&v, 400)));
            }
            if let Ok(Some(v)) = row.try_get::<Option<String>, _>("fi") {
                f.push_str(&format!("- First interaction on record: {}\n", v));
            }
            if let Ok(Some(v)) = row.try_get::<Option<String>, _>("li") {
                f.push_str(&format!("- Most recent interaction: {}\n", v));
            }
            let prev: Option<String> = row.try_get("summary").ok().flatten();
            (name, f, prev)
        }
        "place" => {
            let row = sqlx::query(
                "SELECT name, category, address, visit_count, \
                        first_visit::text AS fv, last_visit::text AS lv, summary \
                 FROM wiki_places WHERE id = $1",
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
            if let Ok(v) = row.try_get::<i32, _>("visit_count") {
                f.push_str(&format!("- Visits on record: {}\n", v));
            }
            if let Ok(Some(v)) = row.try_get::<Option<String>, _>("fv") {
                f.push_str(&format!("- First visit: {}\n", v));
            }
            if let Ok(Some(v)) = row.try_get::<Option<String>, _>("lv") {
                f.push_str(&format!("- Most recent visit: {}\n", v));
            }
            let prev: Option<String> = row.try_get("summary").ok().flatten();
            (name, f, prev)
        }
        _ => {
            let row = sqlx::query(
                "SELECT canonical_name, organization_type, relationship_type, role_title, \
                        first_interaction::text AS fi, last_interaction::text AS li, summary \
                 FROM wiki_orgs WHERE id = $1",
            )
            .bind(&entity.id)
            .fetch_one(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to load org: {}", e)))?;
            let name: String = row.get("canonical_name");
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
            if let Ok(Some(v)) = row.try_get::<Option<String>, _>("fi") {
                f.push_str(&format!("- First interaction on record: {}\n", v));
            }
            if let Ok(Some(v)) = row.try_get::<Option<String>, _>("li") {
                f.push_str(&format!("- Most recent interaction: {}\n", v));
            }
            let prev: Option<String> = row.try_get("summary").ok().flatten();
            (name, f, prev)
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
        FROM wiki_entity_refs er1
        JOIN wiki_entity_refs er2
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
            "person" => ("person", "SELECT canonical_name FROM wiki_people WHERE id = $1"),
            "place" => ("place", "SELECT name FROM wiki_places WHERE id = $1"),
            "organization" => ("org", "SELECT canonical_name FROM wiki_orgs WHERE id = $1"),
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
    let days: Vec<(chrono::NaiveDate, Option<String>)> = sqlx::query_as(
        r#"
        SELECT DISTINCT d.date, d.epigraph
        FROM wiki_days d
        JOIN wiki_entity_refs er ON date(er.timestamp) = d.date
        WHERE er.entity_id = $1 AND d.autobiography IS NOT NULL
        ORDER BY d.date DESC
        LIMIT 6
        "#,
    )
    .bind(&entity.id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (date, epigraph) in &days {
        let label = date.format("%B %-d, %Y");
        match epigraph {
            Some(e) => links.push(format!(
                "- [{}](/day/day_{}) — narrated day: \"{}\"",
                label,
                date.format("%Y-%m-%d"),
                cap(e, 100)
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

    // ── Previous edition ──
    if let Some(prev) = previous {
        p.push_str(&format!("\n## Previous edition of this article\n{}\n", cap(&prev, 2000)));
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

/// Same bearer/System-purpose path as narrative identity — debits the OS
/// reserve, not the user's chat budget. Model comes from the Chat slot.
async fn call_virtues_api(pool: &PgPool, user_prompt: &str) -> Result<String> {
    let chat_model = crate::api::assistant_profile::get_chat_model(pool).await?;

    let client = crate::virtues_api::client::BearerClient::from_env(pool.clone())
        .with_purpose(crate::virtues_api::client::Purpose::System)
        .with_feature("entity_summary");
    let response = client
        .post_json(
            "/v1/ai/chat/completions",
            &serde_json::json!({
                "model": chat_model,
                "messages": [
                    {"role": "system", "content": SYSTEM_PROMPT},
                    {"role": "user", "content": user_prompt}
                ],
                "max_tokens": 900,
                "temperature": 0.4
            }),
        )
        .await
        .map_err(|e| Error::Network(format!("virtues-api request failed: {e}")))?;

    if !response.is_success() {
        let error_msg = match response.status {
            402 => "Usage limit reached for entity summaries".to_string(),
            429 => "Rate limited. Please try again later.".to_string(),
            _ => format!("virtues-api error {}: {}", response.status, response.body),
        };
        return Err(Error::ExternalApi(error_msg));
    }

    let content = response.body["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();

    Ok(content)
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
}

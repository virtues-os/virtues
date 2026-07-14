//! The universal prose extractor — one component, every ontology.
//!
//! It walks `virtues_registry::ontologies::get_extractable_ontologies()`, not a
//! hardcoded list of tables. That distinction is the entire design:
//!
//!   a SOURCE is Gmail, Slack, an mbox import — where data came from
//!   an ONTOLOGY is `data_communication_email` — the normalized shape it became
//!
//! Extraction is configured per **ontology**, so every source that normalizes
//! into one inherits it. Slack lands in `data_communication_message` and works
//! with no new code here at all. There is no per-source branch anywhere in this
//! file, and there never should be.
//!
//! Four ontologies carry prose worth reading. The other eighteen — every
//! health, location, financial and activity table — have no free text and never
//! enter extraction. `data_communication_transcription` carries prose but is
//! deliberately excluded: its entities are already extracted by the
//! transcription action's own LLM call, so it is drained for free (see
//! `extract.rs`) rather than billed twice for the same names.
//!
//! # What the model is asked to do
//!
//! NER, not ER. It finds proper nouns, quotes the clause each appeared in, and
//! resolves relative dates against the record's own timestamp. It never chooses
//! an entity — that is the resolver's job (exact match) or a human's (the review
//! queue). Nothing here can manufacture a wrong link, because nothing here
//! links.
//!
//! # Cost
//!
//! Roughly 25¢/month for a heavy user, and that number is a consequence of
//! three things, in descending order of importance:
//!
//!   1. `er_extraction_log` — a record is read ONCE, ever. The failure mode that
//!      matters is not the per-token price; it is a bug that re-reads history on
//!      every tick and quietly bills 720× a month.
//!   2. Batching — N records per call, so the system prompt is amortized rather
//!      than re-sent per record.
//!   3. The prefilters — Gmail's own junk labels, and a character cap. Email is
//!      where token mass hides: mostly signature and quoted reply.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Row};

use crate::database::Database;
use crate::error::Result;
use crate::virtues_api::client::BearerClient;

/// Records per LLM call. Amortizes the system prompt; small enough that one
/// bad batch is cheap to lose and retry.
const BATCH: usize = 20;

/// Batches per ontology per sweep. Bounds a cold start over years of backlog —
/// the next tick continues where this one stopped.
const MAX_BATCHES_PER_ONTOLOGY: usize = 5;

#[derive(Debug, Default, Serialize)]
pub struct ProseStats {
    pub records: usize,
    pub mentions: usize,
}

const SYSTEM_PROMPT: &str = r#"You extract named entities from personal life-log records. Output ONLY a raw JSON object — no markdown, no code fences, no prose.

You are given records, each with an id and a timestamp. For each record, find the PROPER NOUNS that name a specific person, place, or organization.

Schema:
{"records":[{"id":"the record id","mentions":[{"surface":"the name exactly as written","type":"person|place|org","said":"the clause it appears in, verbatim, <=15 words","when":"ISO 8601 datetime if the mention refers to a specific time, else null"}]}]}

Rules:
- surface: copy the name EXACTLY as written. Do not correct, expand, or normalize it.
- Only SPECIFIC named things. "my dentist" is not a person; "Dr. Nguyen" is. "the office" is not a place; "Blue Bottle" is. A generic noun is not an entity.
- If you are not sure something is a name, LEAVE IT OUT. A missed name costs nothing; a wrong one corrupts someone's records.
- No pronouns. No email addresses, phone numbers, or URLs — those are handled elsewhere.
- said: the quote is what lets a human recognize the mention later. A bare name is useless to them.
- when: ONLY if the text refers to a specific time ("next Saturday", "last night", "on the 14th"). Resolve it against that record's timestamp. Otherwise null. Do not guess.
- A record with no names gets an empty mentions array. This is common and correct.
"#;

#[derive(Debug, Deserialize)]
struct ExtractResponse {
    #[serde(default)]
    records: Vec<RecordMentions>,
}

#[derive(Debug, Deserialize)]
struct RecordMentions {
    id: String,
    #[serde(default)]
    mentions: Vec<RawMention>,
}

#[derive(Debug, Deserialize)]
struct RawMention {
    surface: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    said: Option<String>,
    #[serde(default)]
    when: Option<String>,
}

/// One un-extracted record, as pulled from an ontology table.
struct ProseRecord {
    id: String,
    text: String,
    timestamp: Option<chrono::DateTime<chrono::Utc>>,
}

/// Sweep every extractable ontology.
pub async fn extract_from_prose(db: &Database) -> Result<ProseStats> {
    let mut stats = ProseStats::default();

    for ontology in virtues_registry::ontologies::get_extractable_ontologies() {
        let Some(cfg) = ontology.extraction.as_ref() else {
            continue;
        };

        for _ in 0..MAX_BATCHES_PER_ONTOLOGY {
            let records = fetch_unextracted(
                db.pool(),
                ontology.table_name,
                cfg.text_sql,
                cfg.filter_sql,
                ontology.timestamp_column,
                cfg.max_chars,
                BATCH as i64,
            )
            .await?;

            if records.is_empty() {
                break;
            }

            let n = records.len();
            match extract_batch(db, ontology.table_name, &records).await {
                Ok(mentions) => stats.mentions += mentions,
                Err(e) => {
                    // A failed batch is NOT logged as extracted — it will be
                    // retried on the next sweep. Losing names is worse than
                    // paying twice, and `er_extraction_log` is written only on
                    // success precisely so a transient 429 doesn't silently
                    // blackhole a day of records.
                    tracing::warn!(
                        table = ontology.table_name,
                        records = n,
                        "prose extraction batch failed, will retry next sweep: {e}"
                    );
                    break;
                }
            }
            stats.records += n;
        }
    }

    if stats.mentions > 0 {
        tracing::info!(
            records = stats.records,
            mentions = stats.mentions,
            "extracted mentions from prose"
        );
    }

    Ok(stats)
}

/// Records this ontology has never had extracted.
///
/// The LEFT JOIN against `er_extraction_log` is the gate — it is what makes
/// "read once, ever" true, and it is why the log gets a row even for records
/// that yielded nothing.
#[allow(clippy::too_many_arguments)]
async fn fetch_unextracted(
    pool: &PgPool,
    table: &str,
    text_sql: &str,
    filter_sql: Option<&str>,
    timestamp_col: &str,
    max_chars: usize,
    limit: i64,
) -> Result<Vec<ProseRecord>> {
    // Every interpolated fragment here is a compile-time constant from the
    // ontology registry — never user input. Values are still bound.
    let filter = filter_sql.map(|f| format!("AND ({f})")).unwrap_or_default();
    let sql = format!(
        r#"
        SELECT t.id,
               LEFT({text_sql}, {max_chars}) AS text,
               t.{timestamp_col} AS ts
        FROM {table} t
        LEFT JOIN er_extraction_log l
               ON l.source_table = '{table}'
              AND l.source_id = t.id
        WHERE l.source_id IS NULL
          AND COALESCE({text_sql}, '') <> ''
          {filter}
        ORDER BY t.{timestamp_col} DESC
        LIMIT $1
        "#
    );

    let rows = sqlx::query(&sql).bind(limit).fetch_all(pool).await?;

    Ok(rows
        .into_iter()
        .map(|r| ProseRecord {
            id: r.get("id"),
            text: r.get::<Option<String>, _>("text").unwrap_or_default(),
            timestamp: r.get("ts"),
        })
        .collect())
}

/// One LLM call for a batch; write the mentions; log every record as read.
async fn extract_batch(db: &Database, table: &str, records: &[ProseRecord]) -> Result<usize> {
    let model = crate::api::assistant_profile::get_background_model(db.pool()).await?;

    let payload: Vec<Value> = records
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                // The record's own timestamp — what "last night" is relative to.
                "timestamp": r.timestamp.map(|t| t.to_rfc3339()),
                "text": r.text,
            })
        })
        .collect();

    let client = BearerClient::from_env(db.pool().clone()).with_feature("entity_extraction");
    let response = client
        .post_json(
            "/v1/ai/chat/completions",
            &serde_json::json!({
                "model": model,
                "messages": [
                    {"role": "system", "content": SYSTEM_PROMPT},
                    {"role": "user", "content": serde_json::to_string(&payload)?},
                ],
                "response_format": {"type": "json_object"},
                "max_tokens": 4000,
            }),
        )
        .await
        .map_err(|e| crate::Error::Network(format!("entity extraction failed: {e}")))?;

    if !response.is_success() {
        return Err(crate::Error::ExternalApi(format!(
            "entity extraction returned {}",
            response.status
        )));
    }

    let content = response.body["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| crate::Error::ExternalApi("no content in extraction response".into()))?;

    let parsed: ExtractResponse = serde_json::from_str(strip_fences(content))
        .map_err(|e| crate::Error::ExternalApi(format!("extraction JSON was unparseable: {e}")))?;

    // Index by id: the model is told to echo ids back, but a batch where it
    // drops or invents one must not misattribute a name to the wrong record.
    let known: std::collections::HashSet<&str> = records.iter().map(|r| r.id.as_str()).collect();

    let mut written = 0usize;
    let mut counts: std::collections::HashMap<String, i32> = std::collections::HashMap::new();

    for rec in &parsed.records {
        if !known.contains(rec.id.as_str()) {
            tracing::warn!(table, id = %rec.id, "extractor returned an unknown record id — dropping");
            continue;
        }
        for m in &rec.mentions {
            let Some((surface, kind)) = clean(&m.surface, &m.kind) else {
                continue;
            };
            let reference_time = m
                .when
                .as_deref()
                .and_then(|w| chrono::DateTime::parse_from_rfc3339(w).ok())
                .map(|t| t.with_timezone(&chrono::Utc));

            sqlx::query(
                r#"
                INSERT INTO er_mentions
                    (source_table, source_id, surface, normalized, mention_type,
                     snippet, reference_time, reference_granularity, status)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'floating')
                "#,
            )
            .bind(table)
            .bind(&rec.id)
            .bind(&surface)
            .bind(surface.trim().to_lowercase())
            .bind(kind)
            .bind(m.said.as_deref().map(str::trim).filter(|s| !s.is_empty()))
            .bind(reference_time)
            .bind(reference_time.map(|_| "exact"))
            .execute(db.pool())
            .await?;

            *counts.entry(rec.id.clone()).or_default() += 1;
            written += 1;
        }
    }

    // Log EVERY record in the batch — including the ones with no names, and the
    // ones the model silently omitted. A zero-mention record is a completed
    // decision, not a pending one. Without this we re-read (and re-bill) every
    // nameless email forever.
    for r in records {
        sqlx::query(
            r#"
            INSERT INTO er_extraction_log (source_table, source_id, model, mention_count)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (source_table, source_id) DO NOTHING
            "#,
        )
        .bind(table)
        .bind(&r.id)
        .bind(&model)
        .bind(counts.get(&r.id).copied().unwrap_or(0))
        .execute(db.pool())
        .await?;
    }

    Ok(written)
}

/// Models sometimes wrap JSON in a code fence despite being told not to.
fn strip_fences(s: &str) -> &str {
    let t = s.trim();
    let t = t.strip_prefix("```json").or_else(|| t.strip_prefix("```")).unwrap_or(t);
    t.strip_suffix("```").unwrap_or(t).trim()
}

/// Reject what cannot be an entity. A mention that names nothing is worse than
/// no mention: it costs a human a decision.
fn clean(surface: &str, kind: &str) -> Option<(String, &'static str)> {
    let s = surface.trim();
    if s.is_empty() || s.len() > 120 {
        return None;
    }

    let kind = match kind.to_lowercase().as_str() {
        "person" => "person",
        "place" => "place",
        "org" | "organization" => "org",
        _ => return None,
    };

    let lower = s.to_lowercase();
    if matches!(
        lower.as_str(),
        "unknown" | "unclear" | "[unclear]" | "n/a" | "none" | "null"
    ) {
        return None;
    }
    // The model is told not to emit these; belt and braces.
    if s.contains('@') || s.starts_with("http") {
        return None;
    }

    Some((s.to_string(), kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_extractable_ontology_is_reachable_from_the_registry() {
        // The extractor walks the registry — it must never grow a per-source
        // branch. If a new prose source (Slack, Fastmail) normalizes into an
        // existing ontology, it inherits extraction with zero code here.
        let ontologies = virtues_registry::ontologies::get_extractable_ontologies();
        assert_eq!(ontologies.len(), 4);
        for o in &ontologies {
            let cfg = o.extraction.as_ref().expect("filtered on is_some");
            assert!(!cfg.text_sql.is_empty(), "{} has no prose", o.table_name);
            assert!(cfg.max_chars > 0, "{} has no cap", o.table_name);
            assert!(
                !o.timestamp_column.is_empty(),
                "{} has no timestamp — 'last night' would be unresolvable",
                o.table_name
            );
        }
    }

    #[test]
    fn rejects_what_cannot_be_an_entity() {
        assert!(clean("", "person").is_none());
        assert!(clean("  ", "person").is_none());
        assert!(clean("unknown", "person").is_none());
        assert!(clean("[unclear]", "place").is_none());
        // Handled by the structured linker — a join, not a guess. Extracting it
        // here would build a second, worse path to the same person.
        assert!(clean("sarah@example.com", "person").is_none());
        assert!(clean("https://example.com", "org").is_none());
        // Unknown type: the model invented a category.
        assert!(clean("Biscuit", "animal").is_none());

        assert_eq!(clean("Sarah Smith", "person").unwrap().1, "person");
        assert_eq!(clean("Tweetys", "PLACE").unwrap().1, "place");
        // er_mentions abbreviates; the model may spell it out.
        assert_eq!(clean("Acme", "organization").unwrap().1, "org");
    }

    #[test]
    fn strips_code_fences() {
        assert_eq!(strip_fences("```json\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(strip_fences("```\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(strip_fences("{\"a\":1}"), "{\"a\":1}");
    }

    #[test]
    fn parses_the_batch_response() {
        let raw = r#"{"records":[
            {"id":"e1","mentions":[
                {"surface":"Sarah Smith","type":"person","said":"lunch with Sarah Smith","when":null}
            ]},
            {"id":"e2","mentions":[]}
        ]}"#;
        let r: ExtractResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(r.records.len(), 2);
        assert_eq!(r.records[0].mentions[0].surface, "Sarah Smith");
        // A record with no names is common and correct — and still gets logged,
        // or we re-read it forever.
        assert!(r.records[1].mentions.is_empty());
    }
}

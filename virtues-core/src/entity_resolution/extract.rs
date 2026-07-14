//! Mention extraction — turning prose into evidence.
//!
//! # The transcription drain
//!
//! We were already paying for this and throwing it away.
//!
//! `actions/transcription_resolution` sends every audio chunk to a model and
//! asks, among other things, for the people, places and organizations named in
//! it. That answer has been landing in
//! `data_communication_transcription.entities` every two minutes — and nothing
//! has ever read the column. The names of everyone in your recorded life were
//! being extracted, persisted, and dead-ended in a JSONB blob.
//!
//! So the first extractor isn't an extractor at all. It's a drain: read that
//! column, write `er_mentions`. Zero marginal LLM cost, and it works
//! retroactively over every transcript ever recorded.
//!
//! # Two shapes
//!
//! Rows written before this change hold bare strings:
//!
//!   {"people": ["Sarah Smith"], "places": [], "organizations": []}
//!
//! Rows written after hold the sentence too, because a bare name is not
//! reviewable — a human cannot link what they cannot recognize:
//!
//!   {"people": [{"name": "Sarah Smith", "said": "had a great time with Sarah last night"}]}
//!
//! Both parse. The old rows simply arrive without a snippet, and show up in the
//! queue thinner than the new ones. Nothing is discarded to get a cleaner
//! model.
//!
//! # The gate
//!
//! `er_extraction_log` is a row per source record, written whether or not we
//! found anything. It is what stops the hourly sweep from re-reading — or, when
//! extraction costs tokens, re-*billing* — the same record 720 times a month.
//! It is a table rather than a flag on `data_*` because raw records carry no
//! bookkeeping: the lake stays pure, and derived state lives beside it.

use serde::Serialize;
use serde_json::Value;
use sqlx::Row;

use crate::database::Database;
use crate::error::Result;

#[derive(Debug, Default, Serialize)]
pub struct ExtractStats {
    /// Source records read this sweep (each gets exactly one log row, ever).
    pub records: usize,
    /// Mentions written.
    pub mentions: usize,
}

/// One extracted mention, before it becomes a row.
struct Mention {
    surface: String,
    mention_type: &'static str,
    snippet: Option<String>,
}

/// Drain `data_communication_transcription.entities` into `er_mentions`.
///
/// Not time-windowed: it walks the whole backlog once (gated by
/// `er_extraction_log`), then only ever sees new transcripts. `LIMIT` bounds a
/// single sweep so a cold start with years of audio doesn't try to do it all in
/// one transaction; the next tick picks up where this one stopped.
pub async fn extract_from_transcriptions(db: &Database, limit: i64) -> Result<ExtractStats> {
    let mut stats = ExtractStats::default();

    let rows = sqlx::query(
        r#"
        SELECT t.id, t.entities, t.start_time
        FROM data_communication_transcription t
        LEFT JOIN er_extraction_log l
               ON l.source_table = 'data_communication_transcription'
              AND l.source_id = t.id
        WHERE l.source_id IS NULL
        ORDER BY t.start_time DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(db.pool())
    .await?;

    for row in rows {
        let source_id: String = row.get("id");
        let entities: Option<Value> = row.get("entities");
        let start_time: Option<chrono::DateTime<chrono::Utc>> = row.get("start_time");

        let mentions = parse_entities(entities.as_ref());
        let n = mentions.len();

        for m in mentions {
            insert_mention(
                db,
                "data_communication_transcription",
                &source_id,
                &m,
                // A transcript's mentions are anchored to when it was recorded.
                // (Forward references — "let's meet next Tuesday" — are a
                // different problem, handled where reference_time is extracted
                // rather than inherited. Not this path.)
                start_time,
            )
            .await?;
        }

        // Log the record as processed EVEN IF it yielded nothing. A zero-mention
        // transcript is a completed decision, not a pending one — without this
        // row we would re-read every silent recording forever.
        sqlx::query(
            r#"
            INSERT INTO er_extraction_log (source_table, source_id, model, mention_count)
            VALUES ('data_communication_transcription', $1, 'transcription_resolution', $2)
            ON CONFLICT (source_table, source_id) DO NOTHING
            "#,
        )
        .bind(&source_id)
        .bind(n as i32)
        .execute(db.pool())
        .await?;

        stats.records += 1;
        stats.mentions += n;
    }

    if stats.mentions > 0 {
        tracing::info!(
            records = stats.records,
            mentions = stats.mentions,
            "drained transcription entities into er_mentions"
        );
    }

    Ok(stats)
}

/// Parse the `entities` blob, tolerating both the bare-string shape (rows
/// written before snippets existed) and the object shape.
fn parse_entities(entities: Option<&Value>) -> Vec<Mention> {
    let Some(obj) = entities.and_then(|e| e.as_object()) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for (key, mention_type) in [
        ("people", "person"),
        ("places", "place"),
        ("organizations", "org"),
    ] {
        let Some(items) = obj.get(key).and_then(|v| v.as_array()) else {
            continue;
        };
        for item in items {
            let (surface, snippet) = match item {
                // Legacy: a bare name.
                Value::String(s) => (s.clone(), None),
                // Current: name + the clause it was said in.
                Value::Object(o) => {
                    let Some(name) = o.get("name").and_then(|v| v.as_str()) else {
                        continue;
                    };
                    (
                        name.to_string(),
                        o.get("said")
                            .and_then(|v| v.as_str())
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty()),
                    )
                }
                _ => continue,
            };

            if let Some(m) = clean(&surface, mention_type, snippet) {
                out.push(m);
            }
        }
    }
    out
}

/// Reject what can't be an entity. The model is told not to guess, but it
/// sometimes returns its own uncertainty markers, and a mention that names
/// nothing is worse than no mention — it costs a human a decision.
fn clean(surface: &str, mention_type: &'static str, snippet: Option<String>) -> Option<Mention> {
    let s = surface.trim();
    if s.is_empty() || s.len() > 120 {
        return None;
    }
    let lower = s.to_lowercase();
    // The prompt's own escape hatches, plus the usual null-ish strings.
    if matches!(
        lower.as_str(),
        "[unclear]" | "unclear" | "unknown" | "n/a" | "none" | "null" | "speaker 1" | "speaker 2"
    ) {
        return None;
    }
    Some(Mention {
        surface: s.to_string(),
        mention_type,
        snippet,
    })
}

/// Write one mention. Floating by default — the resolver decides afterwards,
/// and it only ever links on an exact, unambiguous match.
async fn insert_mention(
    db: &Database,
    source_table: &str,
    source_id: &str,
    m: &Mention,
    reference_time: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO er_mentions
            (source_table, source_id, surface, normalized, mention_type,
             snippet, reference_time, reference_granularity, status)
        VALUES ($1, $2, $3, $4, $5, $6, $7, 'exact', 'floating')
        "#,
    )
    .bind(source_table)
    .bind(source_id)
    .bind(&m.surface)
    // `normalized` is the join key the resolver groups and matches on. Lowercase
    // is the whole normalization — no stemming, no fuzzy folding. If two
    // surfaces differ, they are different surfaces, and a human decides.
    .bind(m.surface.trim().to_lowercase())
    .bind(m.mention_type)
    .bind(&m.snippet)
    .bind(reference_time)
    .execute(db.pool())
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_the_legacy_bare_string_shape() {
        // Every transcript recorded before snippets existed looks like this.
        // Dropping them to get a tidier model would throw away real history.
        let v = json!({
            "people": ["Sarah Smith"],
            "places": ["Tweetys"],
            "organizations": []
        });
        let m = parse_entities(Some(&v));
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].surface, "Sarah Smith");
        assert_eq!(m[0].mention_type, "person");
        assert!(m[0].snippet.is_none());
        assert_eq!(m[1].mention_type, "place");
    }

    #[test]
    fn parses_the_snippet_shape() {
        let v = json!({
            "people": [{"name": "Sarah Smith", "said": "had a great time with Sarah last night"}],
            "places": [],
            "organizations": [{"name": "Tweetys"}]
        });
        let m = parse_entities(Some(&v));
        assert_eq!(m.len(), 2);
        assert_eq!(
            m[0].snippet.as_deref(),
            Some("had a great time with Sarah last night")
        );
        // `said` is optional even in the new shape — a missing quote is a
        // thinner queue row, not a dropped mention.
        assert_eq!(m[1].surface, "Tweetys");
        assert!(m[1].snippet.is_none());
    }

    #[test]
    fn drops_the_models_uncertainty_markers() {
        // The prompt tells it to omit ambiguous names; it sometimes says so
        // out loud instead. A mention that names nothing costs a human a
        // decision, which is worse than no mention at all.
        let v = json!({
            "people": ["[unclear]", "Speaker 1", "", "  "],
            "places": ["unknown"],
            "organizations": []
        });
        assert!(parse_entities(Some(&v)).is_empty());
    }

    #[test]
    fn empty_and_missing_blobs_are_not_errors() {
        assert!(parse_entities(None).is_empty());
        assert!(parse_entities(Some(&json!({}))).is_empty());
        assert!(parse_entities(Some(&json!(null))).is_empty());
        // A silent transcript: still logged as processed, just yields nothing.
        assert!(parse_entities(Some(&json!({"people": [], "places": []}))).is_empty());
    }
}

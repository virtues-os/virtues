//! Bookmark enrichment: turn a saved URL into something findable.
//!
//! A save writes a row and returns — instantly, and for free. That row is
//! nearly empty: an Instagram URL has no title, and a browser bookmark's title
//! is whatever the page's `<title>` said the day it was saved. Embedded as-is,
//! it is a document with no words in it, which is why bookmarks were storable
//! but unfindable (docs/bookmarks-plan.md).
//!
//! This is the sweep that fixes that. Per bookmark: fetch the page, compose a
//! structured **extraction record**, and write it back. The record then joins
//! the embed text, so the row becomes searchable by what it is actually about
//! rather than by whatever words happened to be in its URL.
//!
//! Three properties this deliberately has:
//!
//! - **Budgeted, never inline.** The hazard is not steady-state saving (tens a
//!   day, cents); it is the first sync of a browser bookmark file — thousands
//!   of rows at once. So the drain is capped per run and per day, and newest
//!   first, because a bookmark saved today matters more than one imported from
//!   a 2014 folder.
//! - **Derived and disposable.** Everything written here can be recomputed
//!   from the URL, so `extraction` is safe to throw away and re-run when models
//!   improve. `enrichment_model` records what produced the current record.
//! - **It never writes the user's words.** `note` is the one user-authored
//!   column and no pass here touches it. The model proposes; the user disposes.
//!
//! Today this is the text path: fetch a page, read it with the Lite slot. The
//! Omni (pixels/audio) path arrives with the iOS share sheet, which is what
//! puts images in the table in the first place.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;

use crate::error::{Error, Result};
use crate::fetch;
use crate::virtues_api::client::{BearerClient, Purpose};
use virtues_registry::models::{default_model_for_slot, ModelSlot};

/// Bookmarks enriched in one run. Small on purpose: the applet is on a cron, so
/// a modest batch every few minutes drains a backlog without one run holding a
/// subprocess open for an hour.
const BATCH_SIZE: i64 = 20;

/// Default ceiling on enrichments per day.
///
/// Measured 2026-08-05 against the live gateway: a real page costs about
/// **$0.0001** on the Lite slot (`zai/glm-4.7-flash`, ~590 tokens round trip).
/// So this cap is around two cents a day, and even a 10,000-row browser import
/// is roughly a dollar in total.
///
/// It is set low anyway, and deliberately. The cap's job is not to save that
/// dollar — it is to make a first sync *visible and interruptible* rather than
/// something that happens to a person all at once. Raise it freely once the
/// Settings knob exists and the user can see what it is doing.
///
/// Overridable with `VIRTUES_BOOKMARK_ENRICH_DAILY_CAP`; the Settings knob
/// (docs/bookmarks-plan.md) writes the same value.
const DEFAULT_DAILY_CAP: i64 = 200;

/// Attempts before a bookmark is given up on. Mirrors the transcription drain:
/// without a cap a permanently-broken URL is re-fetched and re-billed forever,
/// and sitting at the front of a newest-first queue it blocks everything behind
/// it.
const MAX_ATTEMPTS: i32 = 3;

/// Base for exponential backoff between attempts.
const RETRY_BACKOFF_BASE_SECS: f64 = 300.0;

/// How long a claim may sit in 'enriching' before another run may take it.
///
/// Recovery is by age rather than by lock because this runs as a subprocess
/// that can be killed mid-flight — a lock would die with it and a killed run
/// would strand its claim forever.
const STALE_CLAIM_SECS: f64 = 900.0;

/// Page text handed to the model. Well under the fetch layer's own cap; the
/// useful part of a page is at the top, and the tail is usually comments and
/// related-links.
const MAX_PROMPT_CHARS: usize = 12_000;

/// The structured record a pass produces. Every field is optional: a model that
/// cannot tell should say so, and "unknown" must be cheaper than a guess.
///
/// **`why` is deliberately absent.** Significance is user-sourced, never
/// inferred — enforced here by the shape of the type rather than by asking the
/// prompt nicely.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractionRecord {
    /// Free prose, first in the struct so the model describes before it
    /// classifies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub medium: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subject: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// What the person would plausibly type to find this again. The single
    /// highest-leverage field in the record: query-shaped text matches query
    /// language far better than a literal description does.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub likely_queries: Vec<String>,
}

impl ExtractionRecord {
    /// The record as searchable prose.
    ///
    /// Labelled per line so a chunk that lands mid-record still reads as
    /// something, and so the aspects stay legible to a human debugging a bad
    /// result. This is what step 4 concatenates into the embed text.
    pub fn to_embed_text(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(d) = self.description.as_deref().filter(|s| !s.trim().is_empty()) {
            parts.push(d.trim().to_string());
        }
        if let Some(m) = self.medium.as_deref().filter(|s| !s.trim().is_empty()) {
            parts.push(format!("Medium: {}", m.trim()));
        }
        if !self.subject.is_empty() {
            parts.push(format!("Subject: {}", self.subject.join(", ")));
        }
        if !self.entities.is_empty() {
            parts.push(format!("Mentions: {}", self.entities.join(", ")));
        }
        if let Some(s) = self.style.as_deref().filter(|s| !s.trim().is_empty()) {
            parts.push(format!("Style: {}", s.trim()));
        }
        if !self.likely_queries.is_empty() {
            parts.push(self.likely_queries.join(". "));
        }
        parts.join("\n")
    }
}

/// What one run did. Reported in the applet's run summary.
#[derive(Debug, Default, Clone, Serialize)]
pub struct EnrichmentSummary {
    pub enriched: usize,
    pub failed: usize,
    pub skipped: usize,
    /// True when the run stopped because the daily cap was reached rather than
    /// because the queue drained — the difference the run summary must state,
    /// or a throttled queue looks identical to an empty one.
    pub hit_daily_cap: bool,
    /// Fetchable pages still queued.
    pub remaining: i64,
    /// Asset-backed bookmarks (screenshots, shared images) held back because
    /// the pixel pass is not built. Counted separately so a number that cannot
    /// move yet is never reported as a backlog that should be draining.
    pub awaiting_pixels: i64,
}

#[derive(Debug)]
struct Claimed {
    id: String,
    url: String,
    attempts: i32,
}

fn daily_cap() -> i64 {
    std::env::var("VIRTUES_BOOKMARK_ENRICH_DAILY_CAP")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|v| *v >= 0)
        .unwrap_or(DEFAULT_DAILY_CAP)
}

/// Run one enrichment sweep.
pub async fn run_enrichment_job(db: &PgPool) -> Result<EnrichmentSummary> {
    let mut summary = EnrichmentSummary::default();

    let cap = daily_cap();
    let done_today: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM data_content_bookmark
          WHERE enriched_at >= date_trunc('day', now())",
    )
    .fetch_one(db)
    .await?;
    let allowance = (cap - done_today.0).max(0);
    if allowance == 0 {
        summary.hit_daily_cap = true;
        let (remaining, awaiting_pixels) = queue_counts(db).await?;
        summary.remaining = remaining;
        summary.awaiting_pixels = awaiting_pixels;
        return Ok(summary);
    }

    let batch = BATCH_SIZE.min(allowance);
    // `with_feature` is what makes this spend legible: it tags the cost bucket
    // recorded into `app_ai_calls`, so Usage can say what bookmark enrichment
    // cost rather than folding it anonymously into the gateway total.
    let client = BearerClient::from_env(db.clone())
        .with_purpose(Purpose::System)
        .with_feature("bookmark_enrichment");

    for _ in 0..batch {
        let Some(item) = claim_next(db).await? else {
            break;
        };
        match enrich_one(db, &client, &item).await {
            Ok(Outcome::Enriched) => summary.enriched += 1,
            Ok(Outcome::Skipped(reason)) => {
                summary.skipped += 1;
                mark_terminal(db, &item.id, "skipped", Some(&reason)).await?;
            }
            Err(e) => {
                summary.failed += 1;
                // Give up only at the cap; otherwise return it to the queue so
                // backoff can retry it.
                let status = if item.attempts >= MAX_ATTEMPTS {
                    "failed"
                } else {
                    "pending"
                };
                tracing::warn!(id = %item.id, url = %item.url, attempts = item.attempts, error = %e,
                    "bookmark enrichment attempt failed");
                mark_terminal(db, &item.id, status, Some(&e.to_string())).await?;
            }
        }
    }

    summary.hit_daily_cap = summary.enriched as i64 >= allowance;
    let (remaining, awaiting_pixels) = queue_counts(db).await?;
    summary.remaining = remaining;
    summary.awaiting_pixels = awaiting_pixels;
    Ok(summary)
}

/// A bookmark whose artifact is a stored asset rather than a fetchable page.
///
/// Two shapes qualify, and both are the same fact stated where it belongs:
/// `metadata.asset_id` (the general case — an Instagram post has a source URL
/// *and* a screenshot), and a `url` that is already the in-app viewer route
/// (the pure case — a camera-roll screenshot, whose address is where it lives,
/// because there is nowhere it came from).
///
/// SQL rather than Rust because the claim query has to exclude these before
/// handing them out. `->>` instead of the `?` containment operator on purpose:
/// `?` is a bind-parameter marker in enough tooling to be worth avoiding.
const ASSET_BACKED_SQL: &str =
    "(metadata->>'asset_id' IS NOT NULL OR starts_with(url, '/drive/file_'))";

/// Counts for the run summary: pages waiting, and assets waiting on a pass that
/// does not exist yet.
async fn queue_counts(db: &PgPool) -> Result<(i64, i64)> {
    let row: (i64, i64) = sqlx::query_as(&format!(
        "SELECT
           COUNT(*) FILTER (WHERE NOT {ASSET_BACKED_SQL}),
           COUNT(*) FILTER (WHERE {ASSET_BACKED_SQL})
         FROM data_content_bookmark
          WHERE enrichment_status = 'pending' AND deleted_at_source IS NULL"
    ))
    .fetch_one(db)
    .await?;
    Ok(row)
}

/// Take the next bookmark and mark it claimed, atomically.
///
/// `FOR UPDATE SKIP LOCKED` so two runs overlapping (a slow sweep and the next
/// cron tick) never hand the same row to two enrichments and bill twice.
///
/// Tombstoned rows are excluded: the user removed the bookmark at its source,
/// and paying to read a page they deleted is the wrong default. They stay
/// 'pending' rather than being marked skipped, so a re-add — which clears the
/// tombstone — picks them straight back up.
async fn claim_next(db: &PgPool) -> Result<Option<Claimed>> {
    let row: Option<(String, String, i32)> = sqlx::query_as(&format!(
            r#"
            UPDATE data_content_bookmark SET
                enrichment_status = 'enriching',
                enrichment_attempts = enrichment_attempts + 1,
                enrichment_last_attempt = now(),
                updated_at = now()
            WHERE id = (
                SELECT id FROM data_content_bookmark
                 WHERE deleted_at_source IS NULL
                   -- Asset-backed bookmarks are held back rather than claimed
                   -- and marked. They stay 'pending' with no attempt recorded,
                   -- so when the pixel pass lands, deleting this one clause
                   -- picks up every screenshot ever saved — no re-queue
                   -- migration, no terminal state to undo, no attempt budget
                   -- burned failing at something never tried.
                   AND NOT {ASSET_BACKED_SQL}
                   AND (
                     enrichment_status = 'pending'
                     -- A claim abandoned by a killed run becomes available again.
                     OR (enrichment_status = 'enriching'
                         AND enrichment_last_attempt < now() - make_interval(secs => $1))
                   )
                   AND enrichment_attempts < $2
                   AND (
                     enrichment_last_attempt IS NULL
                     OR enrichment_last_attempt
                        < now() - make_interval(secs =>
                            $3::double precision * power(2, enrichment_attempts))
                   )
                 ORDER BY timestamp DESC
                 LIMIT 1
                 FOR UPDATE SKIP LOCKED
            )
            RETURNING id, url, enrichment_attempts
            "#
    ))
    .bind(STALE_CLAIM_SECS)
    .bind(MAX_ATTEMPTS)
    .bind(RETRY_BACKOFF_BASE_SECS)
    .fetch_optional(db)
    .await?;

    Ok(row.map(|(id, url, attempts)| Claimed { id, url, attempts }))
}

enum Outcome {
    Enriched,
    Skipped(String),
}

async fn enrich_one(db: &PgPool, client: &BearerClient, item: &Claimed) -> Result<Outcome> {
    let page = match fetch::fetch_page(&item.url).await {
        Ok(p) => p,
        // A URL we refuse on policy (a private address, a content type this
        // path does not read) is not a failure to retry — it is a bookmark this
        // sweep has nothing to say about. Retrying it three times with backoff
        // would be pure waste.
        Err(Error::InvalidInput(reason)) => return Ok(Outcome::Skipped(reason)),
        Err(e) => return Err(e),
    };

    if page.article.text.trim().is_empty() && page.article.title.is_none() {
        return Ok(Outcome::Skipped("page yielded no text".to_string()));
    }

    let record = compose_record(client, &page).await?;
    let model = default_model_for_slot(ModelSlot::Lite);

    // COALESCE on the way in: a sync source that supplied a title owns it, and
    // enrichment must not overwrite what a source asserted. It fills gaps only.
    sqlx::query(
        r#"
        UPDATE data_content_bookmark SET
            title             = COALESCE(title, $2),
            description       = COALESCE(description, $3),
            thumbnail_url     = COALESCE(thumbnail_url, $4),
            extraction        = $5,
            extraction_text   = NULLIF($6, ''),
            enrichment_model  = $7,
            enrichment_status = 'done',
            enriched_at       = now(),
            updated_at        = now()
        WHERE id = $1
        "#,
    )
    .bind(&item.id)
    .bind(page.article.title.as_deref())
    .bind(
        page.article
            .description
            .as_deref()
            .or(record.description.as_deref()),
    )
    .bind(page.article.image_url.as_deref())
    .bind(serde_json::to_value(&record).unwrap_or(Value::Null))
    // The rendering the search index reads. Written here rather than assembled
    // from JSONB in embed_text_sql so there is one definition of how a record
    // reads, and it is the tested one.
    .bind(record.to_embed_text())
    .bind(model)
    .execute(db)
    .await?;

    Ok(Outcome::Enriched)
}

async fn mark_terminal(db: &PgPool, id: &str, status: &str, reason: Option<&str>) -> Result<()> {
    sqlx::query(
        "UPDATE data_content_bookmark SET
             enrichment_status = $2,
             extraction = CASE WHEN $3::text IS NULL THEN extraction
                               ELSE COALESCE(extraction, '{}'::jsonb)
                                    || jsonb_build_object('error', $3::text) END,
             updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(status)
    .bind(reason)
    .execute(db)
    .await?;
    Ok(())
}

const SYSTEM_PROMPT: &str = r#"You describe a web page a person saved, so they can find it again later by searching in their own words.

Return ONLY a JSON object, no prose and no code fences:
{"description":"1-3 sentences, what this page IS and what it covers","medium":"article|documentation|product|reference|video|repository|social_post|recipe|other","subject":["3-8 concrete topics"],"entities":["named people, organizations, products, places actually named on the page"],"style":"design/tone vocabulary ONLY if the page is visual or has a distinctive aesthetic, else null","likely_queries":["3-6 things this person might later type to find this page again"]}

Rules:
- Report only what the page actually says. Never invent facts, names, or numbers.
- Any field you cannot fill honestly: use null, or an empty array. "Unknown" is a correct answer and costs nothing; a guess is a lie that gets stored.
- likely_queries are phrases a HUMAN would type from memory — "that cream house with the green door", "rust async book chapter on pinning" — not keyword soup and not a restatement of the title.
- NEVER guess WHY the person saved this. You do not know, and inventing a reason is worse than leaving it out. There is no field for it."#;

async fn compose_record(client: &BearerClient, page: &fetch::FetchedPage) -> Result<ExtractionRecord> {
    let text: String = page.article.text.chars().take(MAX_PROMPT_CHARS).collect();
    let user_content = format!(
        "URL: {}\nTitle: {}\nDescription: {}\n\nPage text:\n{}",
        page.final_url,
        page.article.title.as_deref().unwrap_or("(none)"),
        page.article.description.as_deref().unwrap_or("(none)"),
        if text.trim().is_empty() {
            "(no body text extracted)"
        } else {
            &text
        }
    );

    let body = serde_json::json!({
        "model": default_model_for_slot(ModelSlot::Lite),
        "messages": [
            { "role": "system", "content": SYSTEM_PROMPT },
            { "role": "user", "content": user_content },
        ],
        "max_tokens": 1024,
        // Description, not invention.
        "temperature": 0.0,
    });

    let response = client
        .post_json("/v1/ai/chat/completions", &body)
        .await
        .map_err(|e| Error::ExternalApi(format!("enrichment request failed: {e}")))?;
    if !response.is_success() {
        return Err(Error::ExternalApi(format!(
            "enrichment returned {}: {}",
            response.status, response.body
        )));
    }

    let content = response.body["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| Error::ExternalApi("enrichment response had no content".to_string()))?;

    parse_record(content)
}

/// Parse the model's reply into a record.
///
/// Tolerates code fences and leading prose, because "return only JSON" is an
/// instruction models mostly follow rather than always follow, and a fenced
/// reply is a formatting slip, not a reason to retry and re-bill.
fn parse_record(raw: &str) -> Result<ExtractionRecord> {
    let trimmed = raw.trim();
    let candidate = match (trimmed.find('{'), trimmed.rfind('}')) {
        (Some(start), Some(end)) if end > start => &trimmed[start..=end],
        _ => {
            return Err(Error::ExternalApi(format!(
                "enrichment reply was not JSON: {}",
                trimmed.chars().take(200).collect::<String>()
            )))
        }
    };
    serde_json::from_str::<ExtractionRecord>(candidate)
        .map_err(|e| Error::ExternalApi(format!("enrichment reply did not parse: {e}")))
}

/// When the last enrichment ran — for the Settings/status surfaces.
pub async fn last_enriched_at(db: &PgPool) -> Result<Option<DateTime<Utc>>> {
    let row: (Option<DateTime<Utc>>,) =
        sqlx::query_as("SELECT MAX(enriched_at) FROM data_content_bookmark")
            .fetch_one(db)
            .await?;
    Ok(row.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_clean_reply() {
        let r = parse_record(
            r#"{"description":"A guide.","medium":"article","subject":["stucco"],
                "entities":[],"style":null,"likely_queries":["how to patch stucco"]}"#,
        )
        .unwrap();
        assert_eq!(r.description.as_deref(), Some("A guide."));
        assert_eq!(r.medium.as_deref(), Some("article"));
        assert_eq!(r.likely_queries, vec!["how to patch stucco"]);
    }

    #[test]
    fn tolerates_fences_and_preamble() {
        let r = parse_record(
            "Here you go:\n```json\n{\"description\":\"X\",\"subject\":[\"a\"]}\n```",
        )
        .unwrap();
        assert_eq!(r.description.as_deref(), Some("X"));
        assert_eq!(r.subject, vec!["a"]);
    }

    #[test]
    fn missing_fields_are_not_an_error() {
        // Every field is optional by design — a model that cannot fill one
        // should omit it rather than invent it, so omission must parse.
        let r = parse_record(r#"{"description":"Only this."}"#).unwrap();
        assert_eq!(r.description.as_deref(), Some("Only this."));
        assert!(r.subject.is_empty());
        assert!(r.likely_queries.is_empty());
    }

    #[test]
    fn non_json_is_an_error_not_a_silent_empty_record() {
        assert!(parse_record("I could not read that page.").is_err());
    }

    #[test]
    fn embed_text_is_labelled_and_skips_empty_aspects() {
        let r = ExtractionRecord {
            description: Some("A stucco cottage.".into()),
            medium: Some("article".into()),
            subject: vec!["stucco".into(), "render".into()],
            entities: vec![],
            style: None,
            likely_queries: vec!["cream house green door".into()],
        };
        let text = r.to_embed_text();
        assert!(text.contains("A stucco cottage."));
        assert!(text.contains("Medium: article"));
        assert!(text.contains("Subject: stucco, render"));
        assert!(text.contains("cream house green door"));
        // Empty aspects must not leave dangling labels in the embed text.
        assert!(!text.contains("Mentions:"), "got: {text}");
        assert!(!text.contains("Style:"), "got: {text}");
    }

    #[test]
    fn empty_record_embeds_as_nothing() {
        assert_eq!(ExtractionRecord::default().to_embed_text(), "");
    }

    /// The queue SQL against a real table (needs DATABASE_URL; run explicitly):
    ///
    ///     cargo test -p virtues --lib -- --ignored queue_claim
    ///
    /// The unit tests above cover parsing; none of them touch the claim query,
    /// which is where the ordering, the tombstone exclusion, and the
    /// stale-claim recovery actually live. Uses a throwaway id prefix and
    /// deletes its rows on the way out.
    #[tokio::test]
    #[ignore]
    async fn queue_claim_orders_excludes_and_recovers() {
        let _ = dotenv::dotenv();
        let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL required");
        let pool = sqlx::PgPool::connect(&db_url).await.expect("connect");
        let prefix = format!("test:enrich:{}:", uuid::Uuid::new_v4());

        let insert = |suffix: &str, ts: &str, deleted: bool| {
            let id = format!("{prefix}{suffix}");
            let ts = ts.to_string();
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO data_content_bookmark
                       (id, url, timestamp, source_stream_id, source_table, source_provider,
                        deleted_at_source)
                     VALUES ($1, $2, $3::timestamptz, $1, 'test', 'test',
                             CASE WHEN $4 THEN now() ELSE NULL END)",
                )
                .bind(&id)
                .bind(format!("https://example.com/{id}"))
                .bind(ts)
                .bind(deleted)
                .execute(&pool)
                .await
                .unwrap();
            }
        };

        // Far-future timestamps so these sort ahead of whatever real bookmarks
        // the dev database already holds — the drain is global, and a test that
        // assumes an empty table passes only on an empty box.
        // Asset-backed rows, both shapes: a screenshot whose address IS the
        // viewer route, and an Instagram-style save with a source URL plus a
        // stored image. Newest of all, so if the hold-back failed they would be
        // claimed first and the assertion below would catch it immediately.
        sqlx::query(
            "INSERT INTO data_content_bookmark
               (id, url, timestamp, source_stream_id, source_table, source_provider, metadata)
             VALUES ($1, '/drive/file_abc', '2101-01-01T00:00:00Z'::timestamptz, $1,
                     'test', 'test', '{}'::jsonb),
                    ($2, 'https://instagram.com/p/xyz', '2102-01-01T00:00:00Z'::timestamptz, $2,
                     'test', 'test', '{\"asset_id\": \"file_def\"}'::jsonb)",
        )
        .bind(format!("{prefix}screenshot"))
        .bind(format!("{prefix}igpost"))
        .execute(&pool)
        .await
        .unwrap();

        insert("old", "2098-01-01T00:00:00Z", false).await;
        insert("new", "2099-01-01T00:00:00Z", false).await;
        insert("deleted", "2100-01-01T00:00:00Z", true).await;

        // Newest first, and the tombstoned row — newest of all — is never
        // offered: paying to read a page the user deleted is the wrong default.
        let first = claim_next(&pool).await.unwrap().expect("a row to claim");
        assert!(
            first.id.ends_with(":new"),
            "expected the newest live row, got {}",
            first.id
        );

        // A claimed row is not offered twice, so two overlapping runs cannot
        // both enrich (and both bill for) the same bookmark.
        let second = claim_next(&pool).await.unwrap().expect("second row");
        assert!(second.id.ends_with(":old"), "got {}", second.id);

        // Assert the tombstoned row's state directly rather than claiming
        // again: a third claim would reach into this box's real bookmarks and
        // mark one 'enriching' as a side effect of running the tests.
        let (deleted_status,): (String,) = sqlx::query_as(
            "SELECT enrichment_status FROM data_content_bookmark WHERE id = $1",
        )
        .bind(format!("{prefix}deleted"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            deleted_status, "pending",
            "a tombstoned bookmark was claimed for enrichment"
        );

        // Asset-backed rows must be untouched: still pending, and with NO
        // attempt recorded. An attempt would mean the queue spent part of a
        // bookmark's retry budget failing at a pass that does not exist.
        let held: Vec<(String, String, i32)> = sqlx::query_as(
            "SELECT id, enrichment_status, enrichment_attempts
               FROM data_content_bookmark
              WHERE id IN ($1, $2) ORDER BY id",
        )
        .bind(format!("{prefix}screenshot"))
        .bind(format!("{prefix}igpost"))
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(held.len(), 2);
        for (id, status, attempts) in &held {
            assert_eq!(status, "pending", "{id} was claimed despite having an asset");
            assert_eq!(*attempts, 0, "{id} burned a retry attempt on the image pass");
        }

        // A claim stranded by a killed subprocess is recoverable by age.
        sqlx::query(
            "UPDATE data_content_bookmark
                SET enrichment_last_attempt = now() - interval '1 hour', enrichment_attempts = 0
              WHERE id = $1",
        )
        .bind(format!("{prefix}new"))
        .execute(&pool)
        .await
        .unwrap();
        let recovered = claim_next(&pool).await.unwrap().expect("stale claim recovered");
        assert!(recovered.id.ends_with(":new"), "got {}", recovered.id);

        mark_terminal(&pool, &recovered.id, "skipped", Some("test reason"))
            .await
            .unwrap();
        let (status, extraction): (String, Option<serde_json::Value>) = sqlx::query_as(
            "SELECT enrichment_status, extraction FROM data_content_bookmark WHERE id = $1",
        )
        .bind(&recovered.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(status, "skipped");
        assert_eq!(extraction.unwrap()["error"], "test reason");

        sqlx::query("DELETE FROM data_content_bookmark WHERE starts_with(id, $1)")
            .bind(&prefix)
            .execute(&pool)
            .await
            .unwrap();
    }
}

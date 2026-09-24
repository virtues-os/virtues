//! Bookmark enrichment: turn a saved URL into something findable.
//!
//! A save writes a row and returns — instantly, and for free. That row is
//! nearly empty: an Instagram URL has no title, and a browser bookmark's title
//! is whatever the page's `<title>` said the day it was saved. Embedded as-is,
//! it is a document with no words in it, which is why bookmarks were storable
//! but unfindable (agents/plan/bookmarks-plan.md).
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
//! Two paths, one record. A page is fetched and read by the Lite slot. An image
//! — a screenshot, a shared photo, the picture behind an Instagram save — is
//! read from Drive and shown to the Omni slot. Both write the same extraction
//! record, so an image becomes findable by the same text search as a page:
//! everything becomes text, and there is still one index. Video and audio are
//! not read yet; they stay `pending` and unclaimed (see `image_asset_sql`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;

use crate::error::{Error, Result};
use crate::fetch;
use virtues_registry::models::ModelSlot;

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
/// (agents/plan/bookmarks-plan.md) writes the same value.
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
    /// Pages and images still queued — everything a pass will read.
    pub remaining: i64,
    /// Asset-backed bookmarks no pass reads — video, audio, a file that left
    /// Drive. Images are no longer here: the image pass reads them. Counted
    /// separately so a number that cannot move is never reported as a backlog
    /// that should be draining.
    pub unreadable_assets: i64,
}

#[derive(Debug)]
struct Claimed {
    id: String,
    url: String,
    attempts: i32,
    /// The drive file behind an asset-backed bookmark; `None` for a page. The
    /// claim only hands out an asset when it is an image still in Drive.
    asset_file_id: Option<String>,
    /// Where the save came from, as observed at capture ("instagram", "x"). It
    /// is honest context for the image pass; the model is never asked to guess
    /// a source from pixels.
    source_platform: Option<String>,
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
        let (remaining, unreadable_assets) = queue_counts(db).await?;
        summary.remaining = remaining;
        summary.unreadable_assets = unreadable_assets;
        return Ok(summary);
    }

    let batch = BATCH_SIZE.min(allowance);

    for _ in 0..batch {
        let Some(item) = claim_next(db).await? else {
            break;
        };
        match enrich_one(db, &item).await {
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
    let (remaining, unreadable_assets) = queue_counts(db).await?;
    summary.remaining = remaining;
    summary.unreadable_assets = unreadable_assets;
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
/// SQL rather than Rust because the claim query has to decide before handing a
/// row out. `->>` instead of the `?` containment operator on purpose: `?` is a
/// bind-parameter marker in enough tooling to be worth avoiding.
///
/// Columns are qualified with the table name because these fragments are
/// spliced into queries that also open an `EXISTS` over `app_drive_files`,
/// where a bare `url` or `metadata` would be one rename away from binding to
/// the wrong table. Every query they are spliced into reads
/// `FROM data_content_bookmark` unaliased.
pub const ASSET_BACKED_SQL: &str = "(data_content_bookmark.metadata->>'asset_id' IS NOT NULL \
     OR starts_with(data_content_bookmark.url, '/drive/file_'))";

/// The drive file id behind an asset-backed bookmark: `metadata.asset_id`
/// where the source gave one, else the id in a `/drive/file_…` url (`/drive/`
/// is seven characters, so the id starts at the eighth).
pub const ASSET_FILE_ID_SQL: &str = "COALESCE(data_content_bookmark.metadata->>'asset_id', \
     CASE WHEN starts_with(data_content_bookmark.url, '/drive/file_') \
          THEN substring(data_content_bookmark.url from 8) END)";

/// An asset the image pass can read: an image that is still in Drive.
///
/// This is what narrows `held`. It used to mean every asset, because no pass
/// read pixels; now it means an asset no pass reads — a video, an audio clip, a
/// file that left Drive. The claim query and the room's state both take their
/// definition from here, so "the box will read this" and "the room says the box
/// will read this" cannot disagree.
pub fn image_asset_sql() -> String {
    format!(
        "EXISTS (SELECT 1 FROM app_drive_files f \
                  WHERE f.id = {ASSET_FILE_ID_SQL} \
                    AND f.deleted_at IS NULL \
                    AND f.mime_type LIKE 'image/%')"
    )
}

/// Largest image the pass sends. This is one background call, not a chat turn
/// whose context carries the image into every later turn — so it is roomier
/// than chat's 5MB. Base64 inflates by 4/3, and 10MB raw lands near 13MB
/// encoded, inside the ~20MB inline-request ceiling vision providers enforce.
/// Above it the row is skipped with the size stated, never sent to fail.
const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;

/// Counts for the run summary: what the sweep will read (pages and images),
/// and assets no pass reads.
async fn queue_counts(db: &PgPool) -> Result<(i64, i64)> {
    let image = image_asset_sql();
    let row: (i64, i64) = sqlx::query_as(&format!(
        "SELECT
           COUNT(*) FILTER (WHERE NOT {ASSET_BACKED_SQL} OR {image}),
           COUNT(*) FILTER (WHERE {ASSET_BACKED_SQL} AND NOT {image})
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
    let image = image_asset_sql();
    let row: Option<(String, String, i32, Option<String>, Option<String>)> = sqlx::query_as(&format!(
            r#"
            UPDATE data_content_bookmark SET
                enrichment_status = 'enriching',
                enrichment_attempts = enrichment_attempts + 1,
                enrichment_last_attempt = now(),
                updated_at = now()
            WHERE id = (
                SELECT id FROM data_content_bookmark
                 WHERE deleted_at_source IS NULL
                   -- Pages, and images still in Drive. Any other asset (video,
                   -- audio) is held back rather than claimed and marked: it
                   -- stays 'pending' with no attempt recorded, so when a pass
                   -- for it lands, widening this one clause picks up every one
                   -- ever saved — no re-queue, no terminal state to undo, no
                   -- attempt budget burned failing at something never tried.
                   -- That is how the image pass arrived: the clause excluding
                   -- every asset narrowed to this one.
                   AND (NOT {ASSET_BACKED_SQL} OR {image})
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
                 ORDER BY occurred_at DESC
                 LIMIT 1
                 FOR UPDATE SKIP LOCKED
            )
            RETURNING id, url, enrichment_attempts, {ASSET_FILE_ID_SQL}, source_platform
            "#
    ))
    .bind(STALE_CLAIM_SECS)
    .bind(MAX_ATTEMPTS)
    .bind(RETRY_BACKOFF_BASE_SECS)
    .fetch_optional(db)
    .await?;

    Ok(row.map(|(id, url, attempts, asset_file_id, source_platform)| Claimed {
        id,
        url,
        attempts,
        asset_file_id,
        source_platform,
    }))
}

enum Outcome {
    Enriched,
    Skipped(String),
}

async fn enrich_one(db: &PgPool, item: &Claimed) -> Result<Outcome> {
    if let Some(file_id) = item.asset_file_id.as_deref() {
        return enrich_image(db, item, file_id).await;
    }

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

    let record = compose_record(db, &page).await?;
    write_record(
        db,
        &item.id,
        page.article.title.as_deref(),
        page.article
            .description
            .as_deref()
            .or(record.description.as_deref()),
        page.article.image_url.as_deref(),
        &record,
        ModelSlot::Lite,
    )
    .await?;

    Ok(Outcome::Enriched)
}

/// Write a finished record back. Shared by the page and image paths so there is
/// one definition of what "enriched" writes.
///
/// COALESCE on the way in: a sync source that supplied a title owns it, and
/// enrichment must not overwrite what a source asserted. It fills gaps only.
///
/// `enrichment_model` is the model that actually ran. It used to be the slot
/// DEFAULT while `system_completion` resolved Lite through the owner's
/// background pin, so a pinned owner saw a model credited on the page that had
/// never read it — the module promises this column says "what produced the
/// current record", and now it does.
async fn write_record(
    db: &PgPool,
    id: &str,
    title: Option<&str>,
    description: Option<&str>,
    thumbnail_url: Option<&str>,
    record: &ExtractionRecord,
    slot: ModelSlot,
) -> Result<()> {
    let model = crate::virtues_api::completion::background_model_for_slot(db, slot).await?;
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
    .bind(id)
    .bind(title)
    .bind(description)
    .bind(thumbnail_url)
    .bind(serde_json::to_value(record).unwrap_or(Value::Null))
    // The rendering the search index reads. Written here rather than assembled
    // from JSONB in embed_text_sql so there is one definition of how a record
    // reads, and it is the tested one.
    .bind(record.to_embed_text())
    .bind(model)
    .execute(db)
    .await?;
    Ok(())
}

/// Read an image bookmark: load the picture from Drive and show it to the Omni
/// slot, which writes the same extraction record the page path writes.
///
/// No title is written. A page has a `<title>` its author chose; an image has
/// none, and a model's description is not one — so the room keeps saying
/// "Saved image" rather than a headline the box made up.
async fn enrich_image(db: &PgPool, item: &Claimed, file_id: &str) -> Result<Outcome> {
    let storage = crate::storage::Storage::file(
        crate::storage::lake::lake_root()
            .to_string_lossy()
            .into_owned(),
    )
    .map_err(|e| Error::Storage(format!("storage unavailable: {e}")))?;
    let config = crate::api::DriveConfig::new(std::sync::Arc::new(storage));

    let (file, bytes) = match crate::api::drive::download_file(db, &config, file_id).await {
        Ok(v) => v,
        // Gone from Drive, or not a file: there is nothing to read, and a retry
        // cannot bring it back. The claim checked it was there; this is the
        // race where it left between the claim and the read.
        Err(Error::NotFound(reason)) | Err(Error::InvalidInput(reason)) => {
            return Ok(Outcome::Skipped(reason))
        }
        // A storage error is transient until proven otherwise — back to the
        // queue with backoff.
        Err(e) => return Err(e),
    };

    let mime = file.mime_type.clone().unwrap_or_default();
    if !mime.starts_with("image/") {
        return Ok(Outcome::Skipped(format!(
            "the saved file is {}, not an image",
            if mime.is_empty() { "an unknown type" } else { &mime }
        )));
    }
    if bytes.len() > MAX_IMAGE_BYTES {
        return Ok(Outcome::Skipped(format!(
            "the image is {:.1}MB, over the {}MB limit for reading it",
            bytes.len() as f64 / 1_048_576.0,
            MAX_IMAGE_BYTES / 1_048_576
        )));
    }

    let record = read_image(db, &bytes, &mime, &image_context(item)).await?;
    write_record(
        db,
        &item.id,
        None,
        record.description.as_deref(),
        None,
        &record,
        ModelSlot::Omni,
    )
    .await?;
    Ok(Outcome::Enriched)
}

/// Show the Omni slot one image and get its record back.
///
/// Split from `enrich_image` so the model call has one definition whether the
/// bytes come from Drive or from a test: the parts are built, sent, and parsed
/// here and nowhere else.
async fn read_image(
    db: &PgPool,
    bytes: &[u8],
    mime: &str,
    context: &str,
) -> Result<ExtractionRecord> {
    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes);
    let content = serde_json::json!([
        { "type": "text", "text": context },
        { "type": "image_url", "image_url": { "url": format!("data:{mime};base64,{encoded}") } }
    ]);

    // Same discipline as the page path: description, not invention — thinking
    // off, temperature zero, spend tagged to this feature.
    let raw = crate::virtues_api::completion::system_completion_content(
        db,
        ModelSlot::Omni,
        "bookmark_enrichment",
        IMAGE_SYSTEM_PROMPT,
        content,
        crate::virtues_api::request::Thinking::Off,
        0.0,
    )
    .await
    .map_err(|e| Error::ExternalApi(format!("image enrichment request failed: {e}")))?;

    parse_record(&raw)
}

/// The text that rides with the image: only what was observed about the save.
///
/// A source URL and platform are facts the capture recorded, so they go in —
/// they are the difference between "a photo of a chair" and "a chair someone
/// posted on Instagram". A `/drive/file_…` url is the box's own address for the
/// file, not a source, so it stays out. The model is never asked to supply a
/// source it was not given.
fn image_context(item: &Claimed) -> String {
    let mut lines = vec!["A person saved this image.".to_string()];
    if let Some(platform) = item.source_platform.as_deref().filter(|p| !p.trim().is_empty()) {
        lines.push(format!("Saved from: {platform}"));
    }
    if item.url.starts_with("http://") || item.url.starts_with("https://") {
        lines.push(format!("Source URL: {}", item.url));
    }
    lines.join("\n")
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

/// The image path's prompt. Same record, same honesty rules as the page prompt,
/// plus the three that only pixels need: words in the picture are its content,
/// a face is never a name, and a source is never guessed.
const IMAGE_SYSTEM_PROMPT: &str = r#"You describe an image a person saved, so they can find it again later by searching in their own words.

Return ONLY a JSON object, no prose and no code fences:
{"description":"1-3 sentences, what this image IS and what it shows","medium":"photo|screenshot|design|illustration|diagram|chart|text|social_post|product|other","subject":["3-8 concrete things it shows or is about"],"entities":["people, organizations, products, places named in visible text or on a visible logo"],"style":"the visual vocabulary a designer would search with: palette, typography, composition, material, era","likely_queries":["3-6 things this person might later type to find this image again"]}

Rules:
- Describe only what is visible. Never invent facts, names, places, or numbers.
- If the image contains words — a screenshot of a post, an article, a slide, an interface — the words ARE the content. Carry the distinctive phrases into the description and likely_queries verbatim.
- Never identify a person from their face. Name someone only when their name is written in the image.
- Never guess where the image came from or what address it had. The source, when known, is given to you; when it is not, leave it out.
- Any field you cannot fill honestly: use null, or an empty array. "Unknown" is a correct answer and costs nothing; a guess is a lie that gets stored.
- likely_queries are phrases a HUMAN would type from memory — "that cream house with the green door", "the brutalist chair with brass legs" — not keyword soup.
- NEVER guess WHY the person saved this. You do not know, and inventing a reason is worse than leaving it out. There is no field for it."#;

async fn compose_record(db: &PgPool, page: &fetch::FetchedPage) -> Result<ExtractionRecord> {
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

    // Description, not invention: thinking off, temperature zero, no cap
    // (the 1024 that sat here was a guess about a model that answers, and
    // the Lite pin may be one that thinks). The helper resolves the Lite
    // slot through the owner's background pin and tags the spend.
    let content = crate::virtues_api::completion::system_completion(
        db,
        ModelSlot::Lite,
        "bookmark_enrichment",
        SYSTEM_PROMPT,
        &user_content,
        crate::virtues_api::request::Thinking::Off,
        0.0,
    )
    .await
    .map_err(|e| Error::ExternalApi(format!("enrichment request failed: {e}")))?;

    parse_record(&content)
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
                       (id, url, occurred_at, source_stream_id, source_table, source_provider,
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
               (id, url, occurred_at, source_stream_id, source_table, source_provider, metadata)
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

/// The image pass's two decisions, tested where they are made: which rows the
/// claim hands out, and what the model is told about a save.
#[cfg(test)]
mod image_pass_tests {
    use super::*;

    fn claimed(url: &str, platform: Option<&str>) -> Claimed {
        Claimed {
            id: "b".into(),
            url: url.into(),
            attempts: 0,
            asset_file_id: Some("file_x".into()),
            source_platform: platform.map(String::from),
        }
    }

    #[test]
    fn context_carries_only_what_the_capture_observed() {
        let c = image_context(&claimed("https://instagram.com/p/abc", Some("instagram")));
        assert!(c.contains("Saved from: instagram"));
        assert!(c.contains("Source URL: https://instagram.com/p/abc"));
    }

    #[test]
    fn the_boxs_own_drive_address_is_not_offered_as_a_source() {
        // `/drive/file_…` is where the box keeps the file, not where it came
        // from. Handing it to the model as a "source" would invite it to reason
        // about a place that does not exist.
        let c = image_context(&claimed("/drive/file_abc", None));
        assert!(!c.contains("Source URL"), "{c}");
        assert!(!c.contains("Saved from"), "{c}");
    }

    #[test]
    fn a_blank_platform_is_left_out_rather_than_printed_empty() {
        let c = image_context(&claimed("/drive/file_abc", Some("  ")));
        assert!(!c.contains("Saved from"), "{c}");
    }

    /// The claim is the whole policy: an image still in Drive is read, every
    /// other asset is held — untouched, with no attempt spent — so a future
    /// pass for video or audio picks them straight up.
    #[sqlx::test]
    async fn images_are_claimed_and_every_other_asset_is_held(pool: PgPool) {
        // (id, drive file id, mime, trashed)
        for (fid, mime, trashed) in [
            ("file_img", "image/png", false),
            ("file_ig", "image/jpeg", false),
            ("file_vid", "video/mp4", false),
            ("file_gone", "image/png", true),
        ] {
            sqlx::query(
                "INSERT INTO app_drive_files (id, path, filename, mime_type, size_bytes, deleted_at)
                 VALUES ($1, $2, $3, $4, 10, CASE WHEN $5 THEN now() ELSE NULL END)",
            )
            .bind(fid)
            .bind(format!("/test/{fid}"))
            .bind(format!("{fid}.bin"))
            .bind(mime)
            .bind(trashed)
            .execute(&pool)
            .await
            .unwrap();
        }

        // (bookmark id, url, metadata, platform, occurred year for ordering)
        for (id, url, meta, platform, year) in [
            ("page", "https://example.com/a", "{}", "web", 2030),
            ("shot", "/drive/file_img", "{}", "ios", 2031),
            ("ig", "https://instagram.com/p/x", r#"{"asset_id":"file_ig"}"#, "instagram", 2032),
            ("video", "/drive/file_vid", "{}", "ios", 2033),
            ("trashed", "/drive/file_gone", "{}", "ios", 2034),
            ("missing", "/drive/file_never_uploaded", "{}", "ios", 2035),
        ] {
            sqlx::query(
                "INSERT INTO data_content_bookmark
                   (id, url, occurred_at, source_stream_id, source_table, source_provider,
                    source_platform, metadata)
                 VALUES ($1, $2, make_timestamptz($5, 1, 1, 0, 0, 0), $1, 'test', 'test',
                         $4, $3::jsonb)",
            )
            .bind(id)
            .bind(url)
            .bind(meta)
            .bind(platform)
            .bind(year)
            .execute(&pool)
            .await
            .unwrap();
        }

        // Drain the claim. The database is this test's own, so every row the
        // queue will ever hand out is one of ours.
        let mut claimed = Vec::new();
        while let Some(c) = claim_next(&pool).await.unwrap() {
            claimed.push(c);
        }
        let ids: Vec<&str> = claimed.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["ig", "shot", "page"], "newest first, images and pages only");

        // Both asset shapes resolve to their drive file, and a page has none.
        let by_id = |id: &str| claimed.iter().find(|c| c.id == id).unwrap();
        assert_eq!(by_id("shot").asset_file_id.as_deref(), Some("file_img"));
        assert_eq!(by_id("ig").asset_file_id.as_deref(), Some("file_ig"), "metadata.asset_id wins");
        assert_eq!(by_id("ig").source_platform.as_deref(), Some("instagram"));
        assert!(by_id("page").asset_file_id.is_none());

        // Video, a trashed image, and a file that never arrived: all held,
        // untouched, no retry budget spent on a pass that cannot read them.
        let held: Vec<(String, String, i32)> = sqlx::query_as(
            "SELECT id, enrichment_status, enrichment_attempts FROM data_content_bookmark
              WHERE id IN ('video', 'trashed', 'missing') ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(held.len(), 3);
        for (id, status, attempts) in held {
            assert_eq!(status, "pending", "{id} was claimed but no pass can read it");
            assert_eq!(attempts, 0, "{id} spent a retry attempt");
        }

        // The run summary's split agrees with the claim: nothing readable left
        // pending, three held.
        let (remaining, unreadable) = queue_counts(&pool).await.unwrap();
        assert_eq!((remaining, unreadable), (0, 3));
    }
}

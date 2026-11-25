//! Photo favorites: describing each picture so it can be found by what is in it.
//!
//! The phone sends a favorite's date, place and picture
//! (`applets/ios_ingest/photos.rs`). This pass shows the picture to the Omni
//! slot and writes an extraction record, which joins the row's embed text, so
//! a photo is found the way a person remembers one: "the red boat at the
//! lake". The record and the model call are `image_reading`'s, shared with
//! bookmarks; the queue, the prompt and the table are here.
//!
//! The properties of `bookmark_enrichment` hold here too: a daily cap, newest
//! first; a record that can be thrown away and written again; and `note` is
//! the owner's and is never touched.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;

use crate::error::{Error, Result};
use crate::image_reading::{self, ExtractionRecord, Loaded};
use virtues_registry::models::ModelSlot;

/// Photos described in one run.
const BATCH_SIZE: i64 = 20;

/// Default ceiling on photos described per day. It makes a first sync of
/// favorites gradual. `VIRTUES_PHOTO_ENRICH_DAILY_CAP` overrides it.
const DEFAULT_DAILY_CAP: i64 = 100;

/// Attempts before a photo is given up on, so one that always fails is not
/// billed forever.
const MAX_ATTEMPTS: i32 = 3;

/// Base for exponential backoff between attempts.
const RETRY_BACKOFF_BASE_SECS: f64 = 300.0;

/// How long a claim may sit in 'enriching' before another run may take it. A
/// run is a subprocess that can be killed, so recovery is by age.
const STALE_CLAIM_SECS: f64 = 900.0;

#[derive(Debug, Clone, Default, Serialize)]
pub struct PhotoSummary {
    pub described: usize,
    pub failed: usize,
    pub skipped: usize,
    /// The model could not be reached or paid for, so the run ended early.
    pub stopped: bool,
    /// The run stopped at the daily cap rather than at the end of the queue.
    pub hit_daily_cap: bool,
    pub remaining: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct Claimed {
    id: String,
    enrichment_attempts: i32,
    asset_id: String,
    kind: String,
    occurred_at: DateTime<Utc>,
    latitude: Option<f64>,
    longitude: Option<f64>,
}

fn daily_cap() -> i64 {
    std::env::var("VIRTUES_PHOTO_ENRICH_DAILY_CAP")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|v| *v >= 0)
        .unwrap_or(DEFAULT_DAILY_CAP)
}

/// Run one sweep.
pub async fn run_photo_job(db: &PgPool) -> Result<PhotoSummary> {
    let mut summary = PhotoSummary::default();
    let done_today: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM data_content_photo WHERE enriched_at >= date_trunc('day', now())",
    )
    .fetch_one(db)
    .await?;
    let allowance = (daily_cap() - done_today).max(0);

    for _ in 0..BATCH_SIZE.min(allowance) {
        let Some(item) = claim_next(db).await? else { break };
        match describe(db, &item).await {
            Ok(None) => summary.described += 1,
            Ok(Some(reason)) => {
                summary.skipped += 1;
                mark(db, &item, "skipped", &reason).await?;
            }
            // Not about this photo: no model can be reached or paid for now.
            // The photo keeps its attempts and the run ends, so a server that
            // is offline or out of credit gives up on nothing.
            Err(e) if stops_the_run(&e) => {
                tracing::warn!(error = %e, "describing photos stopped for now");
                release_claim(db, &item).await?;
                summary.stopped = true;
                break;
            }
            Err(e) => {
                summary.failed += 1;
                let status = if item.enrichment_attempts >= MAX_ATTEMPTS { "failed" } else { "pending" };
                tracing::warn!(id = %item.id, attempts = item.enrichment_attempts, error = %e,
                    "describing a photo failed");
                mark(db, &item, status, &e.to_string()).await?;
            }
        }
    }

    summary.remaining = sqlx::query_scalar(
        "SELECT COUNT(*) FROM data_content_photo
          WHERE enrichment_status = 'pending' AND asset_id IS NOT NULL
            AND is_favorite AND deleted_at_source IS NULL",
    )
    .fetch_one(db)
    .await?;
    summary.hit_daily_cap = summary.remaining > 0 && summary.described as i64 >= allowance;
    Ok(summary)
}

/// Whether a failure would meet the next photo too: the gateway could not be
/// reached, refused to bill, or was rate limited or down.
fn stops_the_run(e: &Error) -> bool {
    match e {
        Error::Network(_) => true,
        Error::ExternalApi(m) => {
            crate::virtues_api::client::is_payment_refusal(e)
                || m.contains("Rate limited")
                || ["401", "403", "500", "502", "503", "504"]
                    .iter()
                    .any(|code| m.contains(&format!("virtues-api error {code}")))
        }
        _ => false,
    }
}

async fn release_claim(db: &PgPool, item: &Claimed) -> Result<()> {
    sqlx::query(
        "UPDATE data_content_photo SET
             enrichment_status = 'pending', enrichment_attempts = enrichment_attempts - 1,
             enrichment_last_attempt = NULL, updated_at = now()
         WHERE id = $1 AND asset_id = $2 AND enrichment_status = 'enriching'",
    )
    .bind(&item.id)
    .bind(&item.asset_id)
    .execute(db)
    .await?;
    Ok(())
}

/// Take the next photo and mark it claimed. `FOR UPDATE SKIP LOCKED` keeps two
/// overlapping runs from paying for one photo twice.
async fn claim_next(db: &PgPool) -> Result<Option<Claimed>> {
    Ok(sqlx::query_as::<_, Claimed>(
        "UPDATE data_content_photo SET
             enrichment_status = 'enriching',
             enrichment_attempts = enrichment_attempts + 1,
             enrichment_last_attempt = now(),
             updated_at = now()
         WHERE id = (
             SELECT id FROM data_content_photo
              WHERE is_favorite AND deleted_at_source IS NULL AND asset_id IS NOT NULL
                AND (enrichment_status = 'pending'
                     OR (enrichment_status = 'enriching'
                         AND enrichment_last_attempt < now() - make_interval(secs => $1)))
                AND enrichment_attempts < $2
                AND (enrichment_last_attempt IS NULL
                     OR enrichment_last_attempt
                        < now() - make_interval(secs => $3::double precision * power(2, enrichment_attempts)))
              ORDER BY occurred_at DESC
              LIMIT 1
              FOR UPDATE SKIP LOCKED)
         RETURNING id, enrichment_attempts, asset_id, kind, occurred_at, latitude, longitude",
    )
    .bind(STALE_CLAIM_SECS)
    .bind(MAX_ATTEMPTS)
    .bind(RETRY_BACKOFF_BASE_SECS)
    .fetch_optional(db)
    .await?)
}

/// Describe one photo. `Some(reason)` when there is nothing to describe.
async fn describe(db: &PgPool, item: &Claimed) -> Result<Option<String>> {
    let (bytes, mime) = match image_reading::load(db, &item.asset_id).await? {
        Loaded::Image { bytes, mime } => (bytes, mime),
        Loaded::Unreadable(reason) => return Ok(Some(reason)),
    };
    let place = match (item.latitude, item.longitude) {
        (Some(lat), Some(lon)) => crate::api::photos::place_near(db, lat, lon).await?.map(|p| p.name),
        _ => None,
    };
    let context = context(item, place.as_deref());
    let record = image_reading::read(db, "photo_enrichment", SYSTEM_PROMPT, &context, &bytes, &mime).await?;
    write_record(db, item, &record, &found_by(item, place.as_deref())).await?;
    Ok(None)
}

/// Written only while the row still shows the picture that was read. A photo
/// edited or removed during the call keeps what that change left.
async fn write_record(db: &PgPool, item: &Claimed, record: &ExtractionRecord, facts: &str) -> Result<()> {
    let model = crate::virtues_api::completion::background_model_for_slot(db, ModelSlot::Omni).await?;
    let extraction = serde_json::to_value(record)
        .map_err(|e| Error::Other(format!("a photo's record did not serialize: {e}")))?;
    sqlx::query(
        "UPDATE data_content_photo SET
             extraction        = $3,
             extraction_text   = $4,
             enrichment_model  = $5,
             enrichment_status = 'done',
             enriched_at       = now(),
             updated_at        = now()
         WHERE id = $1 AND asset_id = $2 AND deleted_at_source IS NULL",
    )
    .bind(&item.id)
    .bind(&item.asset_id)
    .bind(extraction)
    .bind(format!("{}\n{facts}", record.to_embed_text()))
    .bind(model)
    .execute(db)
    .await?;
    Ok(())
}

async fn mark(db: &PgPool, item: &Claimed, status: &str, reason: &str) -> Result<()> {
    sqlx::query(
        "UPDATE data_content_photo SET
             enrichment_status = $3,
             extraction = COALESCE(extraction, '{}'::jsonb) || jsonb_build_object('error', $4::text),
             updated_at = now()
         WHERE id = $1 AND asset_id = $2 AND deleted_at_source IS NULL",
    )
    .bind(&item.id)
    .bind(&item.asset_id)
    .bind(status)
    .bind(reason)
    .execute(db)
    .await?;
    Ok(())
}

/// The clock where a photo was taken, when it says where that is.
fn zone_of(item: &Claimed) -> Option<chrono_tz::Tz> {
    crate::timezone::coords_to_tz(item.latitude?, item.longitude?)?.parse().ok()
}

/// The date and place as words, for the search text: the model may leave them
/// out of what it writes, and "the lake in June" has to find the photo.
fn found_by(item: &Claimed, place: Option<&str>) -> String {
    let when = match zone_of(item) {
        Some(tz) => item.occurred_at.with_timezone(&tz).format("%B %Y").to_string(),
        None => item.occurred_at.format("%B %Y").to_string(),
    };
    match place {
        Some(place) => format!("Taken: {when}, {place}"),
        None => format!("Taken: {when}"),
    }
}

/// What rides with the picture: when and where it was taken, as words. The
/// coordinates stay here; only a place the owner has named is given.
fn context(item: &Claimed, place: Option<&str>) -> String {
    let mut lines = vec!["A person marked this as one of their favorite photos.".to_string()];
    let taken = match zone_of(item) {
        Some(tz) => item.occurred_at.with_timezone(&tz).format("%A, %B %-d, %Y, %-I:%M %p").to_string(),
        None => item.occurred_at.format("%B %-d, %Y").to_string(),
    };
    lines.push(format!("Taken: {taken}"));
    if let Some(place) = place {
        lines.push(format!("Taken at: {place}"));
    }
    match item.kind.as_str() {
        "video" => lines.push("This is one frame of a video.".into()),
        "screenshot" => lines.push("This is a screenshot.".into()),
        _ => {}
    }
    lines.join("\n")
}

const SYSTEM_PROMPT: &str = r#"You describe a photo a person took and marked as a favorite, so they can find it again later by searching in their own words.

Return ONLY a JSON object, no prose and no code fences:
{"description":"1-3 sentences, what the photo shows and what is happening","medium":"photo|screenshot|document|artwork|other","subject":["3-8 concrete things in it: people by role or number, animals, objects, food, activities"],"entities":["people, organizations, products, places named in visible text or on a visible logo"],"style":"composition and light, as a photographer would say it, or null","colors":["2-5 main colors in plain words, with what has them: red boat, green hills"],"setting":"the kind of place and the time or light: beach at sunset, kitchen in the morning, or null","visible_text":"words readable in the photo, verbatim, or null","likely_queries":["3-6 things this person might later type to find this photo again"]}

Rules:
- Describe only what is visible. Never invent facts, names, places, or numbers.
- Never identify a person from their face. Say "a woman and two children", never who they are, unless a name is written in the photo.
- Never guess anyone's health, faith, politics, ethnicity or relationship to the person.
- The date and the place, when given, are facts. You may use them in the description and likely_queries. Never guess a place that was not given.
- Any field you cannot fill: use null, or an empty array.
- likely_queries are phrases a person would type from memory, such as "the red boat at the lake" or "birthday cake with blue candles". No keyword lists.
- Never guess why the person kept it. There is no field for it."#;

#[cfg(test)]
mod tests {
    use super::*;

    fn item() -> Claimed {
        Claimed {
            id: "p1".into(),
            enrichment_attempts: 1,
            asset_id: "file_1".into(),
            kind: "photo".into(),
            occurred_at: "2031-06-15T01:30:00Z".parse().unwrap(),
            latitude: None,
            longitude: None,
        }
    }

    #[test]
    fn the_context_gives_the_date_and_place_in_words_and_no_coordinates() {
        let bare = context(&item(), None);
        assert!(bare.contains("Taken: June 15, 2031"), "{bare}");
        // Where the photo was taken, it was still the evening before.
        let mut there = item();
        (there.latitude, there.longitude) = (Some(40.7128), Some(-74.0060));
        let words = context(&there, Some("Blue Lake"));
        assert!(words.contains("Saturday, June 14, 2031, 9:30 PM"), "{words}");
        assert!(words.contains("Taken at: Blue Lake"), "{words}");
        assert!(!words.contains("40.7") && !words.contains("74.0"), "{words}");
        assert_eq!(found_by(&there, Some("Blue Lake")), "Taken: June 2031, Blue Lake");
    }

    async fn photo(pool: &PgPool, id: &str, asset: Option<&str>, favorite: bool, day: u32) {
        sqlx::query(
            "INSERT INTO data_content_photo
               (id, occurred_at, kind, is_favorite, asset_id, source_stream_id, source_table, source_provider)
             VALUES ($1, make_timestamptz(2031, 6, $4::int, 12, 0, 0), 'photo', $3, $2, $1, 'test', 'test')",
        )
        .bind(id)
        .bind(asset)
        .bind(favorite)
        .bind(day as i32)
        .execute(pool)
        .await
        .unwrap();
    }

    #[sqlx::test]
    async fn the_newest_favorite_with_a_picture_is_claimed_once(pool: PgPool) {
        photo(&pool, "older", Some("file_a"), true, 1).await;
        photo(&pool, "newer", Some("file_b"), true, 2).await;
        photo(&pool, "not-favorite", Some("file_c"), false, 3).await;
        photo(&pool, "no-picture", None, true, 4).await;
        photo(&pool, "removed", Some("file_d"), true, 5).await;
        sqlx::query("UPDATE data_content_photo SET deleted_at_source = now() WHERE id = 'removed'")
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(claim_next(&pool).await.unwrap().unwrap().id, "newer");
        assert_eq!(claim_next(&pool).await.unwrap().unwrap().id, "older");
        assert!(claim_next(&pool).await.unwrap().is_none());
    }

    #[sqlx::test]
    async fn a_record_is_written_only_for_the_picture_that_was_read(pool: PgPool) {
        photo(&pool, "p", Some("file_a"), true, 1).await;
        let claimed = claim_next(&pool).await.unwrap().unwrap();
        // The photo was edited while the model was reading the old picture.
        sqlx::query("UPDATE data_content_photo SET asset_id = 'file_b', enrichment_status = 'pending' WHERE id = 'p'")
            .execute(&pool)
            .await
            .unwrap();
        mark(&pool, &claimed, "failed", "x").await.unwrap();
        let status: String = sqlx::query_scalar("SELECT enrichment_status FROM data_content_photo WHERE id = 'p'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "pending");
    }

    /// An outage or an empty balance is not the photo's fault, and a reply that
    /// is not a record is.
    #[sqlx::test]
    async fn a_run_that_cannot_reach_a_model_costs_a_photo_no_attempt(pool: PgPool) {
        assert!(stops_the_run(&Error::Network("down".into())));
        assert!(stops_the_run(&Error::ExternalApi("Wallet empty [wallet_empty]".into())));
        assert!(stops_the_run(&Error::ExternalApi("reading an image failed: virtues-api error 503: {}".into())));
        assert!(!stops_the_run(&Error::ExternalApi("virtues-api error 400: {}".into())));
        assert!(!stops_the_run(&Error::ExternalApi("enrichment reply was not JSON: no".into())));

        photo(&pool, "p", Some("file_a"), true, 1).await;
        let claimed = claim_next(&pool).await.unwrap().unwrap();
        release_claim(&pool, &claimed).await.unwrap();
        assert_eq!(claim_next(&pool).await.unwrap().unwrap().enrichment_attempts, 1);
    }
}

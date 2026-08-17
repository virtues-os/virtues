//! day_summary_eod: end-of-day summary action.
//!
//! Runs once per day (via cron trigger, with a SQL condition gating to the
//! user's maintenance hour). Resolves sleep, scores novelty/autonomic/topic/
//! entity signals on each wiki_event, then regenerates the day's autobiography
//! via the LLM.
//!
//! All heavy lifting lives in `virtues-core` — this binary just glues the
//! pieces together and owns the stdin/stdout JSON contract.

use anyhow::{Context, Result};
use chrono::{NaiveDate, Timelike};
use serde_json::json;
use virtues_helpers::{output, read_input};

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let input = read_input()?;
    let pool = virtues_helpers::connect_from_env("virtues-action-day_summary_eod").await?;

    // If an explicit date is supplied in config, skip the maintenance-hour gate
    // — manual / tool invocations with a date are always intentional.
    let explicit_date = input
        .config
        .get("date")
        .and_then(|v| v.as_str())
        .map(|s| {
            NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .with_context(|| format!("invalid config.date: {s}"))
        })
        .transpose()?;

    // Cron-triggered runs arrive every hour. Gate to the user's local maintenance
    // hour (DEFAULT_MAINTENANCE_HOUR = 4am, in the box's home_timezone) so the whole
    // chain runs ONCE a day, on the COMPLETED prior day.
    //
    // The chain is nightly, not hourly, because the detective is now a best-model
    // Chat call. You cannot fuse a day that is not over — its distribution of
    // novelty, its context boundaries, are only knowable once — and re-running a
    // premium call 23×/day to rebuild a half-day it would discard tonight is pure
    // waste. So every non-maintenance tick is a no-op. The live "today" view is a
    // separate, deterministic, zero-LLM read of visits+calendar+sleep and needs
    // nothing from this action.
    let date = match explicit_date {
        // A date was named: the caller means it. Run the chain for that day.
        Some(d) => d,
        None => {
            let (tz, hour) = load_user_maintenance(&pool).await;
            let now_local = chrono::Utc::now().with_timezone(&tz);
            let yesterday = now_local.date_naive() - chrono::Duration::days(1);

            // Catch-up first. `narrated_at` has been written since day segmentation
            // shipped and never read as a work queue, so a box that was asleep,
            // restarting, or erroring during the maintenance hour lost that day
            // *permanently* — the chain only ever looked at `yesterday`, and
            // nothing ever went back. Days older than yesterday are definitively
            // settled, so they can be fused at any hour; the maintenance hour only
            // needs to gate the freshest day, whose late collector data (audio
            // still transcribing, final visits) has not landed yet.
            //
            // One day per tick, oldest first: the chain is two LLM calls deep, and
            // an hourly cron drains a backlog soon enough without risking a run
            // that blows its timeout.
            if let Some(pending) = oldest_unnarrated_day(&pool, yesterday).await {
                tracing::info!(date = %pending, "catching up an unnarrated day");
                pending
            } else {
                if now_local.hour() as i32 != hour {
                    // Any other hour, nothing pending: no-op. The chain only runs
                    // on a completed day, at the maintenance hour.
                    let skip = format!(
                        "skipped: local hour {}, maintenance {}",
                        now_local.hour(),
                        hour
                    );
                    tracing::info!(%skip, "not the maintenance hour — no-op");
                    return output(&skip, &input.config);
                }
                // The maintenance hour: yesterday is complete. Run the whole chain.
                yesterday
            }
        }
    };

    tracing::info!(date = %date, "day_summary_eod starting");

    // ─────────────────────────────────────────────────────────────────────
    // ORDER IS LOad-BEARING. Do not move a scoring step above step 1.
    //
    // `generate_day_summary` SEGMENTS the day: it deletes every auto event
    // (`delete_auto_events_for_day` — `WHERE is_user_added = false`) and
    // re-inserts fresh rows carrying only 14 columns, none of which is a
    // score. So anything computed before it is written to rows that are about
    // to be dropped.
    //
    // This is exactly what used to happen: sleep → novelty → autonomic →
    // topic/entity all ran FIRST, and the summary then deleted the rows
    // holding every value they had just computed. The result was that
    // `embedding`, `novelty_z`, `local_novelty_z`, `lof_raw`, `avg_hr`,
    // `hr_z`, `autonomic_z`, `topic_novelty` and `entity_novelty` were NULL
    // after every single cron run — and because `novelty::load_baseline`
    // requires `embedding IS NOT NULL` on PAST events, the baseline could
    // never accumulate either. The scoring subsystem had never persisted a
    // value. Seed data hand-populates these columns, which is why nobody
    // noticed.
    //
    // Segment first. Then score what actually exists.
    // Guarded by `virtues-core/tests/day_pipeline.rs` — if you reorder this,
    // that test fails.
    // ─────────────────────────────────────────────────────────────────────

    // 0. Roll audio chunks up into context sessions BEFORE the detective reads
    //    them. Mechanical (changepoint on loudness + speaker count, no LLM), so it
    //    is cheap enough to run each pass; idempotent per day. Without this the
    //    detective drowns in 271 five-minute chunks instead of ~20 sessions.
    let audio_sessions = virtues::sessionize::audio::sessionize_day(&pool, date)
        .await
        .context("audio sessionization failed")?;

    // 1. Segment the day into events — THE DETECTIVE (LLM, Chat slot). Fuses the
    //    dossier of clean rollups into a gapless timeline. DESTRUCTIVE — replaces
    //    all auto events. Idempotent: if the day's sources are unchanged since the
    //    last cut, this returns 0 immediately and makes no model call.
    let events = virtues::api::day_summary::segment_day_events(&pool, date)
        .await
        .context("day segmentation failed")?;

    // 2. Sleep resolution. Must follow the segmentation: a sleep event has
    //    `is_user_added = false`, so the delete in step 1 eats it too.
    virtues::dayline::sleep::resolve_sleep_events(&pool, date).await;

    // 2b. Settle the raw spine into its final shape: absorb sub-15-min Unknown
    //     slivers into neighbours and label location-change gaps as Transit
    //     (`is_transit`). AFTER sleep (so it also cleans the short Unknown tails
    //     sleep's split leaves behind) and BEFORE scoring (so transit blocks are
    //     annotated and scored like any event — mode descriptive, salience decisive).
    let gap_ops = virtues::dayline::gaps::classify_day_gaps(&pool, date)
        .await
        .context("gap classification failed")?;

    // 3. Annotate the surviving events from their own time windows: avg_hr
    //    (the input autonomic scoring has always lacked), the entity refs
    //    that overlap them, and which ontologies actually had data.
    let annotated = virtues::dayline::annotate::annotate_events_for_day(&pool, date)
        .await
        .context("event annotation failed")?;

    // 4. Novelty scoring — writes `wiki_events.embedding`, which is the
    //    baseline cache the NEXT night's novelty run reads, and which step 5
    //    (autonomic) reads for *today's* events in this same run. That second
    //    reader is the reason this column exists rather than the scorer just
    //    reading `search_vectors`: the search index is populated by a separate
    //    15-minute cron and will not have seen these ids yet.
    //    Note this is NOT the notebook magnet's input — that reads
    //    `search_vectors` directly (see magnet.rs) and is unaffected by
    //    anything in this chain.
    let novelty_count = virtues::dayline::novelty::compute_novelty_for_day(&pool, date)
        .await
        .context("novelty scoring failed")?;

    // 5. Autonomic scoring (HR against a contextual baseline). Needs step 3's
    //    avg_hr and step 4's embeddings; before this reorder it had neither.
    let autonomic_count =
        virtues::dayline::autonomic_scoring::compute_autonomic_for_day(&pool, date)
            .await
            .context("autonomic scoring failed")?;

    // 6. Topic/entity novelty. Needs the topics step 1 now emits and the
    //    entities step 3 attaches; before this it scored empty arrays.
    let topic_entity_count =
        virtues::dayline::topic_entity_novelty::compute_topic_entity_novelty(&pool, date)
            .await
            .context("topic/entity novelty scoring failed")?;

    // 7. Narrate the day — THE DAY SUMMARY (LLM, Chat slot). Reads the scored
    //    EVENTS (not raw sources) plus the 14-day case file, and names the day's
    //    standout from novelty_z. The whole chain only reaches here at the
    //    maintenance hour on a completed day, so this always runs (gated internally
    //    to days that earned a story).
    let narrated = virtues::api::day_summary::narrate_day(&pool, date)
        .await
        .context("day narration failed")?
        .is_some();

    // Stash the last processed date in config so subsequent cron runs can
    // short-circuit in a condition or observe progress. Also strip any
    // `date` override that the caller passed in — the runner persists the
    // returned config back to app_applets.config, and a sticky `date` would
    // trap subsequent scheduled runs on the override's date forever. Chat
    // tools and manual-trigger with a date are always one-shot.
    let mut config = input.config.clone();
    if let Some(obj) = config.as_object_mut() {
        obj.remove("date");
    }
    config["last_date"] = json!(date.format("%Y-%m-%d").to_string());

    // Every count here must be "rows actually written", never "rows seen".
    // The old `topic_entity=N` reported events *considered*, so it stayed
    // cheerfully non-zero while the function scored nothing at all — the same
    // failure mode as avg_hr, in production, undetected. A metric that can't
    // go to zero can't tell you anything.
    let summary = format!(
        "{date}: audio_sessions={audio_sessions} events={events} gap_ops={gap_ops} annotated={annotated} \
         novelty={novelty_count} autonomic={autonomic_count} topic_entity={topic_entity_count} \
         narrated={narrated}"
    );
    output(&summary, &config)
}

/// Pick "yesterday" in the user's configured timezone. Falls back to UTC.
#[allow(dead_code)]
async fn resolve_user_yesterday(pool: &sqlx::PgPool) -> NaiveDate {
    let (tz, _hour) = load_user_maintenance(pool).await;
    let now_local = chrono::Utc::now().with_timezone(&tz);
    now_local.date_naive() - chrono::Duration::days(1)
}

/// How far back catch-up will reach. Bounded on purpose: this is a repair path
/// for a missed maintenance hour, not a backfill tool. A box returning from a
/// month offline should not silently spend a month of LLM calls reconstructing
/// autobiography nobody asked for — that is an explicit-date decision.
const CATCHUP_HORIZON_DAYS: i64 = 14;

/// Narration's own floor, mirrored from `day_summary::MIN_EVENTS_TO_NARRATE`.
/// The queue must not offer a day narration would refuse, or catch-up jams.
const MIN_EVENTS_TO_NARRATE: i64 = 4;

/// The oldest settled day inside the horizon that *should* have narrated and
/// didn't — i.e. it has a real day's worth of events but no `narrated_at`.
///
/// Strictly BEFORE `yesterday`: yesterday belongs to the maintenance-hour path
/// so its late-arriving collector data keeps its settle window.
///
/// The event-count floor is load-bearing, not a nicety. Narration refuses a day
/// under `MIN_EVENTS_TO_NARRATE` and — correctly — leaves `narrated_at` NULL. A
/// queue that selected on `narrated_at IS NULL` alone would therefore hand the
/// same empty day back every hour forever and, being oldest-first, block every
/// real failure behind it. On the box this was written against, five such days
/// (0–1 events, from setup week) sat exactly where that jam would form.
async fn oldest_unnarrated_day(pool: &sqlx::PgPool, yesterday: NaiveDate) -> Option<NaiveDate> {
    let start = yesterday - chrono::Duration::days(CATCHUP_HORIZON_DAYS);
    let end = yesterday - chrono::Duration::days(1);
    if end < start {
        return None;
    }
    sqlx::query_scalar::<_, NaiveDate>(
        "SELECT w.date \
         FROM wiki_days w \
         WHERE w.date BETWEEN $1 AND $2 \
           AND w.narrated_at IS NULL \
           AND (SELECT count(*) FROM wiki_events e \
                WHERE e.day_id = w.id AND NOT e.is_unknown AND NOT e.user_hidden) >= $3 \
         ORDER BY w.date ASC LIMIT 1",
    )
    .bind(start)
    .bind(end)
    .bind(MIN_EVENTS_TO_NARRATE)
    .fetch_optional(pool)
    .await
    .unwrap_or_else(|e| {
        // Never let the repair path take down the normal path.
        tracing::warn!(error = %e, "catch-up scan failed; falling back to the maintenance hour");
        None
    })
}

/// The local hour the nightly chain runs at. 4am: the day is definitively over,
/// late collector data (audio chunks still transcribing, final visits) has
/// settled, it is before the user wakes so the autobiography is ready for them,
/// and the box is idle. A fixed default until a real per-user setting is migrated.
const DEFAULT_MAINTENANCE_HOUR: i32 = 4;

/// Load the user's timezone and maintenance hour from `app_user_profile`.
/// Defaults: UTC timezone, 8am maintenance hour.
///
/// This used to `SELECT home_timezone, update_check_hour` — but `update_check_hour`
/// ships in NO migration and does not exist on the box, so the whole query errored
/// on the missing column and the `.ok()` swallowed it, silently dropping
/// `home_timezone` too. The nightly then ran at 8am **UTC** for everyone, ignoring
/// the user's timezone entirely (and near midnight, resolving the wrong
/// "yesterday"). Read only the column that exists; the maintenance hour is a fixed
/// default until a configurable setting is actually migrated + wired.
async fn load_user_maintenance(pool: &sqlx::PgPool) -> (chrono_tz::Tz, i32) {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT home_timezone FROM app_user_profile LIMIT 1")
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();

    let tz = row
        .and_then(|(tz_str,)| tz_str)
        .and_then(|s| s.parse::<chrono_tz::Tz>().ok())
        .unwrap_or(chrono_tz::UTC);
    (tz, DEFAULT_MAINTENANCE_HOUR)
}

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
    virtues_actions::init_tracing();

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

    // Cron-triggered runs arrive every hour. Gate to the user's local
    // maintenance hour (profile.update_check_hour, default 8am) so we only
    // actually do the work once per day, aligned to the user's timezone.
    // Running without a date → this is a cron / scheduled invocation.
    // The cron ticks HOURLY, and it used to throw away 23 of every 24 ticks —
    // returning "skipped: local hour 19, maintenance 8" and reporting success.
    //
    // It threw them away because one Opus call produced BOTH the events and the
    // autobiography, so re-segmenting as data arrived would have meant re-writing
    // the day's prose every hour. Now they are two calls with two models:
    //
    //   EVERY HOUR       segment today into events (Lite), then score them. Cheap,
    //                    factual, idempotent — if no new sources landed, it does
    //                    nothing at all and spends nothing.
    //
    //   ONCE A DAY       narrate yesterday (Chat), and only if the day earned it.
    //                    Prose about what it MEANT, standing on the events.
    //
    // That is the hourly cron the plan asked for and the code never had.
    let (narrate, date) = match explicit_date {
        // A date was named: the caller means it. Do both halves for that day.
        Some(d) => (true, d),
        None => {
            let (tz, hour) = load_user_maintenance(&pool).await;
            let now_local = chrono::Utc::now().with_timezone(&tz);
            if now_local.hour() as i32 == hour {
                // The maintenance hour: yesterday is complete. Write it up.
                (true, resolve_user_yesterday(&pool).await)
            } else {
                // Any other hour: keep TODAY's events current as the day happens.
                (false, now_local.date_naive())
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

    // 1. Segment the day into events (LLM, Lite slot). DESTRUCTIVE — replaces all
    //    auto events. Idempotent: if the day's sources are unchanged since the last
    //    cut, this returns 0 immediately and makes no model call.
    let events = virtues::api::day_summary::segment_day_events(&pool, date)
        .await
        .context("day segmentation failed")?;

    // 2. Sleep resolution. Must follow the segmentation: a sleep event has
    //    `is_user_added = false`, so the delete in step 1 eats it too.
    virtues::dayline::sleep::resolve_sleep_events(&pool, date).await;

    // 3. Annotate the surviving events from their own time windows: avg_hr
    //    (the input autonomic scoring has always lacked), the entity refs
    //    that overlap them, and which ontologies actually had data.
    let annotated = virtues::dayline::annotate::annotate_events_for_day(&pool, date)
        .await
        .context("event annotation failed")?;

    // 4. Novelty scoring — writes `embedding`, which everything downstream
    //    (autonomic's similarity baseline, class-by-neighbourhood, the W5
    //    story magnet) depends on.
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

    // 7. Narrate the day (LLM, Chat slot) — ONLY at the maintenance hour, and only
    //    if the day earned it. This is the one call left that costs narrative money,
    //    and it now reads the EVENTS rather than the raw sources: the prompt always
    //    claimed it did ("the event timeline already does that") while being handed
    //    the sources anyway.
    let narrated = if narrate {
        virtues::api::day_summary::narrate_day(&pool, date)
            .await
            .context("day narration failed")?
            .is_some()
    } else {
        false
    };

    // Stash the last processed date in config so subsequent cron runs can
    // short-circuit in a condition or observe progress. Also strip any
    // `date` override that the caller passed in — the runner persists the
    // returned config back to app_actions.config, and a sticky `date` would
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
        "{date}: audio_sessions={audio_sessions} events={events} annotated={annotated} novelty={novelty_count} \
         autonomic={autonomic_count} topic_entity={topic_entity_count} \
         narrated={narrated}"
    );
    output(&summary, &config)
}

/// Pick "yesterday" in the user's configured timezone. Falls back to UTC.
async fn resolve_user_yesterday(pool: &sqlx::PgPool) -> NaiveDate {
    let (tz, _hour) = load_user_maintenance(pool).await;
    let now_local = chrono::Utc::now().with_timezone(&tz);
    now_local.date_naive() - chrono::Duration::days(1)
}

/// Load the user's timezone and maintenance hour from `app_user_profile`.
/// Defaults: UTC timezone, 8 for maintenance hour.
async fn load_user_maintenance(pool: &sqlx::PgPool) -> (chrono_tz::Tz, i32) {
    let row: Option<(Option<String>, Option<i32>)> = sqlx::query_as(
        "SELECT home_timezone, update_check_hour FROM app_user_profile LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let (tz_str, hour) = row.unwrap_or((None, None));
    let tz = tz_str
        .and_then(|s| s.parse::<chrono_tz::Tz>().ok())
        .unwrap_or(chrono_tz::UTC);
    (tz, hour.unwrap_or(8))
}

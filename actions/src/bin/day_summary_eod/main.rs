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
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let input = read_input()?;
    let pool = virtues_helpers::connect_from_env().await?;

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
    if explicit_date.is_none() {
        let (tz, hour) = load_user_maintenance(&pool).await;
        let now_local = chrono::Utc::now().with_timezone(&tz);
        if now_local.hour() as i32 != hour {
            tracing::info!(
                now_local = %now_local.format("%Y-%m-%d %H:%M %Z"),
                maintenance_hour = hour,
                "outside maintenance hour, skipping"
            );
            let summary = format!("skipped: local hour {}, maintenance {}", now_local.hour(), hour);
            return output(&summary, &input.config);
        }
    }

    let date = match explicit_date {
        Some(d) => d,
        None => resolve_user_yesterday(&pool).await,
    };

    tracing::info!(date = %date, "day_summary_eod starting");

    // 1. Sleep resolution (side-effects; doesn't return counts)
    virtues::dayline::sleep::resolve_sleep_events(&pool, date).await;

    // 2. Novelty scoring
    let novelty_count = virtues::dayline::novelty::compute_novelty_for_day(&pool, date)
        .await
        .context("novelty scoring failed")?;

    // 3. Autonomic scoring (HR/HRV against contextual baseline)
    let autonomic_count =
        virtues::dayline::autonomic_scoring::compute_autonomic_for_day(&pool, date)
            .await
            .context("autonomic scoring failed")?;

    // 4. Topic/entity novelty
    let topic_entity_count =
        virtues::dayline::topic_entity_novelty::compute_topic_entity_novelty(&pool, date)
            .await
            .context("topic/entity novelty scoring failed")?;

    // 5. Autobiography generation via LLM
    virtues::api::day_summary::generate_day_summary(&pool, date)
        .await
        .context("autobiography generation failed")?;

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

    let summary = format!(
        "{}: novelty={} autonomic={} topic_entity={}, autobiography generated",
        date, novelty_count, autonomic_count, topic_entity_count
    );
    output(&summary, &config)
}

/// Pick "yesterday" in the user's configured timezone. Falls back to UTC.
async fn resolve_user_yesterday(pool: &sqlx::SqlitePool) -> NaiveDate {
    let (tz, _hour) = load_user_maintenance(pool).await;
    let now_local = chrono::Utc::now().with_timezone(&tz);
    now_local.date_naive() - chrono::Duration::days(1)
}

/// Load the user's timezone and maintenance hour from `app_user_profile`.
/// Defaults: UTC timezone, 8 for maintenance hour.
async fn load_user_maintenance(pool: &sqlx::SqlitePool) -> (chrono_tz::Tz, i32) {
    let row: Option<(Option<String>, Option<i32>)> = sqlx::query_as(
        "SELECT timezone, update_check_hour FROM app_user_profile LIMIT 1",
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

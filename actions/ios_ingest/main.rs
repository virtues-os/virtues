//! iOS ingest — single dispatcher binary for every paired-iPhone stream.
//!
//! Replaces the former one-binary-per-stream layout (`ios_healthkit`,
//! `ios_location`, `ios_eventkit`, `ios_contacts`, `ios_microphone`,
//! `ios_financekit`). Those were six independently-built binaries, each of
//! which could drift from the DB schema on a partial redeploy — exactly the
//! skew that left a stale `started_at: String` decoder panicking after the
//! SQLite→Postgres migration. Collapsing them into one artifact means there is
//! a single binary to build and bake, so that class of drift can't recur.
//!
//! There is a single `ios_ingest` action row; the iPhone posts every stream to
//! its one webhook URL and tags the body with a `stream` field
//! (`{source, stream, device_id, records, ...}`). The dispatcher routes on that
//! field. `argv[1]` is honored first as a manual/cron override (e.g. running
//! `ios_ingest location` by hand), but the production path is the body field.
//!
//! Per-stream enable/disable lives on the device, so there is deliberately no
//! server-side per-stream gating here — an unconfigured stream simply isn't sent.

mod contacts;
mod eventkit;
mod financekit;
mod healthkit;
mod location;
mod microphone;

use anyhow::{anyhow, Result};
use serde_json::Value;
use sqlx::PgPool;
use virtues_helpers::{connect_from_env, output, read_input, ActionInput};

#[tokio::main]
async fn main() -> Result<()> {
    virtues_actions::init_tracing();

    let input = read_input()?;
    let stream = resolve_stream(&input)?;
    let db = connect_from_env("virtues-action-ios_ingest").await?;
    let payload = input.payload.as_ref();

    let summary = match stream.as_str() {
        "healthkit" => healthkit_ingest(&db, records(payload, "healthkit")?).await?,
        "location" => {
            let recs = records(payload, "location")?;
            let written = location::write_locations(&db, recs).await?;
            format!("locations: {}/{}", written, recs.len())
        }
        "eventkit" => {
            let recs = records(payload, "eventkit")?;
            let written = eventkit::write_events(&db, recs).await?;
            format!("events: {} written", written)
        }
        "contacts" => {
            let recs = records(payload, "contacts")?;
            let (resolved, failed) = contacts::resolve_contacts(&db, recs).await?;
            format!("contacts: {} resolved, {} failed", resolved, failed)
        }
        "microphone" => {
            let recs = records(payload, "microphone")?;
            let (written, failed) = microphone::ingest_all(&db, recs).await?;
            format!("audio recordings: {} written, {} failed", written, failed)
        }
        "financekit" => {
            let recs = records(payload, "financekit")?;
            let accounts = financekit::write_accounts(&db, recs).await?;
            let transactions = financekit::write_transactions(&db, recs).await?;
            format!("accounts: {}, transactions: {}", accounts, transactions)
        }
        other => {
            return Err(anyhow!(
                "ios_ingest: unknown stream '{other}' \
                 (expected one of healthkit, location, eventkit, contacts, microphone, financekit)"
            ))
        }
    };

    output(&summary, &input.config)?;
    Ok(())
}

/// Resolve which iOS stream this invocation handles. The production source is
/// the payload's `stream` field (the iOS app sends it on every push); `argv[1]`
/// is honored first only as a manual/cron override. Both forms are normalized —
/// an optional `ios_` prefix is stripped and the result lower-cased — so
/// `"ios_location"` and `"location"` resolve the same.
fn resolve_stream(input: &ActionInput) -> Result<String> {
    if let Some(arg) = std::env::args().nth(1) {
        return Ok(normalize_stream(&arg));
    }
    if let Some(stream) = input
        .payload
        .as_ref()
        .and_then(|p| p.get("stream"))
        .and_then(|v| v.as_str())
    {
        return Ok(normalize_stream(stream));
    }
    Err(anyhow!(
        "ios_ingest: no stream selector — expected payload.stream (or argv[1])"
    ))
}

fn normalize_stream(s: &str) -> String {
    s.trim()
        .strip_prefix("ios_")
        .unwrap_or(s)
        .to_ascii_lowercase()
}

/// Pull the `records` array out of an iOS push payload. The app posts
/// `{source, stream, deviceId, records: [..], ...}`; manual/cron invocations
/// may pass a bare array. Borrows from the payload so the caller's slice
/// outlives this function.
fn records<'a>(payload: Option<&'a Value>, who: &str) -> Result<&'a [Value]> {
    payload
        .and_then(|p| {
            p.get("records")
                .and_then(|r| r.as_array())
                .or_else(|| p.as_array())
        })
        .map(Vec::as_slice)
        .ok_or_else(|| anyhow!("ios_ingest({who}) requires a `records` array in the payload"))
}

/// HealthKit fans one batch across several ontology tables by `metric_type`,
/// so it gets a dedicated entry point rather than a single write call.
async fn healthkit_ingest(db: &PgPool, records: &[Value]) -> Result<String> {
    let mut heart_rate = Vec::new();
    let mut hrv = Vec::new();
    let mut steps = Vec::new();
    let mut sleep = Vec::new();
    let mut workout = Vec::new();
    let mut active_energy = Vec::new();
    let mut distance = Vec::new();
    let mut unknown = 0usize;

    for record in records {
        let metric_type = record
            .get("metric_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        match metric_type {
            "heart_rate" | "resting_heart_rate" => heart_rate.push(record.clone()),
            "heart_rate_variability" => hrv.push(record.clone()),
            "steps" => steps.push(record.clone()),
            "sleep" => sleep.push(record.clone()),
            "workout" => workout.push(record.clone()),
            "active_energy" => active_energy.push(record.clone()),
            "distance" => distance.push(record.clone()),
            other => {
                tracing::warn!(metric_type = other, "unknown healthkit metric_type");
                unknown += 1;
            }
        }
    }

    let mut results: Vec<String> = Vec::new();

    let hr_written = healthkit::write_heart_rate(db, &heart_rate).await?;
    if !heart_rate.is_empty() {
        results.push(format!("heart_rate: {}/{}", hr_written, heart_rate.len()));
    }

    let hrv_written = healthkit::write_hrv(db, &hrv).await?;
    if !hrv.is_empty() {
        results.push(format!("hrv: {}/{}", hrv_written, hrv.len()));
    }

    let steps_written = healthkit::write_steps(db, &steps).await?;
    if !steps.is_empty() {
        results.push(format!("steps: {}/{}", steps_written, steps.len()));
    }

    let sleep_written = healthkit::write_sleep(db, &sleep).await?;
    if !sleep.is_empty() {
        results.push(format!("sleep: {}/{}", sleep_written, sleep.len()));
    }

    let workout_written = healthkit::write_workout(db, &workout).await?;
    if !workout.is_empty() {
        results.push(format!("workout: {}/{}", workout_written, workout.len()));
    }

    let active_energy_written = healthkit::write_active_energy(db, &active_energy).await?;
    if !active_energy.is_empty() {
        results.push(format!(
            "active_energy: {}/{}",
            active_energy_written,
            active_energy.len()
        ));
    }

    let distance_written = healthkit::write_distance(db, &distance).await?;
    if !distance.is_empty() {
        results.push(format!("distance: {}/{}", distance_written, distance.len()));
    }

    if unknown > 0 {
        results.push(format!("unknown: {}", unknown));
    }

    Ok(if results.is_empty() {
        "no records".to_string()
    } else {
        results.join(", ")
    })
}

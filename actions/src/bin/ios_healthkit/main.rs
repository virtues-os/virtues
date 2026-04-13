//! iOS HealthKit action.
//!
//! Receives batches of HealthKit records from the iPhone via `/ingest`,
//! routes each record by `metric_type`, and writes to the corresponding
//! ontology table. Supports: heart_rate, heart_rate_variability, steps,
//! sleep, workout.

mod transform;

use anyhow::Result;
use virtues_action_helpers::{connect_from_env, output, read_input};

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
    let db = connect_from_env().await?;

    let records = input
        .payload
        .as_ref()
        .and_then(|p| p.as_array())
        .ok_or_else(|| anyhow::anyhow!("ios_healthkit requires a payload array"))?;

    // Partition records by metric_type, then batch-write each type.
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

    let hr_written = transform::write_heart_rate(&db, &heart_rate).await?;
    if !heart_rate.is_empty() {
        results.push(format!("heart_rate: {}/{}", hr_written, heart_rate.len()));
    }

    let hrv_written = transform::write_hrv(&db, &hrv).await?;
    if !hrv.is_empty() {
        results.push(format!("hrv: {}/{}", hrv_written, hrv.len()));
    }

    let steps_written = transform::write_steps(&db, &steps).await?;
    if !steps.is_empty() {
        results.push(format!("steps: {}/{}", steps_written, steps.len()));
    }

    let sleep_written = transform::write_sleep(&db, &sleep).await?;
    if !sleep.is_empty() {
        results.push(format!("sleep: {}/{}", sleep_written, sleep.len()));
    }

    let workout_written = transform::write_workout(&db, &workout).await?;
    if !workout.is_empty() {
        results.push(format!("workout: {}/{}", workout_written, workout.len()));
    }

    let active_energy_written = transform::write_active_energy(&db, &active_energy).await?;
    if !active_energy.is_empty() {
        results.push(format!(
            "active_energy: {}/{}",
            active_energy_written,
            active_energy.len()
        ));
    }

    let distance_written = transform::write_distance(&db, &distance).await?;
    if !distance.is_empty() {
        results.push(format!("distance: {}/{}", distance_written, distance.len()));
    }

    if unknown > 0 {
        results.push(format!("unknown: {}", unknown));
    }

    let summary = if results.is_empty() {
        "no records".to_string()
    } else {
        results.join(", ")
    };

    output(&summary, &input.config)?;
    Ok(())
}

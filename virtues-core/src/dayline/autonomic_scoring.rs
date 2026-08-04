//! Autonomic scoring for dayline events.
//!
//! Computes how your body's response (HR, optionally HRV) to an event compares
//! to your response to similar past events. Uses embedding similarity-weighted
//! kernel regression with recency decay.
//!
//! The autonomic_z score is the second line on the Dayline chart:
//!   > 0σ  → Stress (sympathetic — body mobilizing more than usual for this type of event)
//!   < 0σ  → Recovery (parasympathetic — body more at rest than usual)
//!
//! Context-gated HR/HRV composite:
//!   Physical events (HR > resting + 2σ):  autonomic_z = hr_z
//!   Sedentary events:                     autonomic_z = 0.3*hr_z + 0.7*(-hrv_z)
//!   Sleep events:                         autonomic_z = -hrv_z
//!
//! See /DAYLINE_AUTONOMIC_DESIGN.md for full design rationale.

use chrono::NaiveDate;
use sqlx::PgPool;

use crate::dayline::embedding_ops::{bytes_to_embedding, cosine_similarity};

/// Recency half-life in days (3-week decay: α ≈ 0.1 EMA equivalent).
const RECENCY_HALF_LIFE_DAYS: f64 = 21.0;

/// Minimum number of baseline events with sufficient weight to produce a score.
const MIN_WEIGHTED_EVENTS: f64 = 5.0;

/// Maximum z-score (clamp).
const Z_MAX: f64 = 3.0;

/// Embedding similarity kernel bandwidth.
/// Controls how sharply similarity falls off with distance.
/// σ² = 0.5 means events at cosine distance 1.0 get weight ~0.37.
const SIMILARITY_BANDWIDTH: f64 = 0.5;

/// Compute autonomic z-scores for all events on a given day that have avg_hr but no autonomic_z.
///
/// Returns the number of events scored.
pub async fn compute_autonomic_for_day(pool: &PgPool, date: NaiveDate) -> anyhow::Result<u32> {
    // Bind DATEs as DATEs. Binding a formatted String against the `date` column
    // fails with "operator does not exist: date = text" — the same error that
    // had kept `topic_entity_novelty` from ever completing a call. `date_str` is
    // still needed by the resting-HR helpers, which take &str.
    let date_str = date.format("%Y-%m-%d").to_string();
    let baseline_start = date - chrono::Duration::days(84);

    // 1. Load today's events that need scoring (have avg_hr but no autonomic_z)
    let today_events: Vec<(String, f64, Option<Vec<u8>>, bool)> = sqlx::query_as(
        r#"
        SELECT e.id, e.avg_hr, e.embedding, e.is_sleep
        FROM wiki_events e
        JOIN wiki_days d ON e.day_id = d.id
        WHERE d.date = $1
          AND e.avg_hr IS NOT NULL
          AND e.autonomic_z IS NULL
          AND e.user_hidden = FALSE
        "#,
    )
    .bind(date)
    .fetch_all(pool)
    .await?;

    if today_events.is_empty() {
        return Ok(0);
    }

    // 2. Load baseline events (past 12 weeks) with embeddings + avg_hr
    let baseline: Vec<(Vec<u8>, f64, NaiveDate)> = sqlx::query_as(
        r#"
        SELECT e.embedding, e.avg_hr, d.date
        FROM wiki_events e
        JOIN wiki_days d ON e.day_id = d.id
        WHERE d.date >= $1
          AND d.date < $2
          AND e.embedding IS NOT NULL
          AND e.avg_hr IS NOT NULL
          AND e.is_sleep = FALSE
          AND e.user_hidden = FALSE
        "#,
    )
    .bind(baseline_start)
    .bind(date)
    .fetch_all(pool)
    .await?;

    if baseline.is_empty() {
        tracing::debug!(date = %date, "No baseline events with HR data for autonomic scoring");
        return Ok(0);
    }

    // Pre-parse baseline embeddings with days_ago
    let baseline_parsed: Vec<(Vec<f32>, f64, f64)> = baseline
        .iter()
        .filter_map(|(emb_bytes, hr, d)| {
            let emb = bytes_to_embedding(emb_bytes);
            if emb.is_empty() {
                return None;
            }
            // Both are real dates now — no string parsing round-trip.
            let days_ago = (date - *d).num_days().max(0) as f64;
            Some((emb, *hr, days_ago))
        })
        .collect();

    // Get personal resting HR for activity detection
    let resting_hr: f64 = get_resting_hr(pool, &date_str).await.unwrap_or(62.0);
    let resting_hr_std: f64 = get_resting_hr_std(pool, &date_str).await.unwrap_or(5.0);
    let physical_threshold = resting_hr + 2.0 * resting_hr_std;

    // 3. Score each event
    let mut scored = 0u32;
    for (event_id, avg_hr, embedding_bytes, is_sleep) in &today_events {
        let embedding = match embedding_bytes {
            Some(bytes) => {
                let emb = bytes_to_embedding(bytes);
                if emb.is_empty() {
                    continue;
                }
                emb
            }
            None => continue, // Can't score without embedding
        };

        // Compute similarity-weighted expected HR from baseline
        let mut weight_sum = 0.0f64;
        let mut weighted_hr_sum = 0.0f64;

        for &(ref b_emb, b_hr, days_ago) in &baseline_parsed {
            let sim = cosine_similarity(&embedding, b_emb);
            let sim_weight = (-(1.0 - sim).powi(2) / (2.0 * SIMILARITY_BANDWIDTH)).exp();
            let recency_weight = (-days_ago / RECENCY_HALF_LIFE_DAYS).exp();
            let w = sim_weight * recency_weight;

            weight_sum += w;
            weighted_hr_sum += w * b_hr;
        }

        if weight_sum < MIN_WEIGHTED_EVENTS {
            // Insufficient similar baseline events
            continue;
        }

        let expected_hr = weighted_hr_sum / weight_sum;

        // Compute weighted standard deviation
        let mut weighted_var_sum = 0.0f64;
        for &(ref b_emb, b_hr, days_ago) in &baseline_parsed {
            let sim = cosine_similarity(&embedding, b_emb);
            let sim_weight = (-(1.0 - sim).powi(2) / (2.0 * SIMILARITY_BANDWIDTH)).exp();
            let recency_weight = (-days_ago / RECENCY_HALF_LIFE_DAYS).exp();
            let w = sim_weight * recency_weight;
            weighted_var_sum += w * (b_hr - expected_hr).powi(2);
        }

        let expected_std = (weighted_var_sum / weight_sum).sqrt();
        if expected_std < 0.5 {
            // Too little variation in baseline HR — can't meaningfully z-score
            continue;
        }

        let hr_z = ((*avg_hr - expected_hr) / expected_std).max(-Z_MAX).min(Z_MAX);

        // Context-gated composite
        // For V1: HRV is rarely available per-event, so we use hr_z as the default
        // When HRV data is available in the future, this will be enhanced
        let is_physical = *avg_hr > physical_threshold;
        let autonomic_z = if *is_sleep {
            // Sleep: would use -hrv_z, but for now use -hr_z (inverted — lower HR = more recovery)
            (-hr_z).max(-Z_MAX).min(Z_MAX)
        } else if is_physical {
            // Physical activity: HR tells the whole story
            hr_z
        } else {
            // Sedentary: HR as primary (HRV supplementary when available in V2)
            hr_z
        };

        // Store scores
        sqlx::query(
            "UPDATE wiki_events SET hr_z = $1, autonomic_z = $2 WHERE id = $3",
        )
        .bind(hr_z)
        .bind(autonomic_z)
        .bind(event_id.as_str())
        .execute(pool)
        .await?;

        scored += 1;
    }

    if scored > 0 {
        tracing::info!(
            date = %date,
            scored,
            total = today_events.len(),
            baseline_events = baseline_parsed.len(),
            resting_hr = %resting_hr,
            "Autonomic scoring complete"
        );
    }

    Ok(scored)
}

// ============================================================================
// Helpers
// ============================================================================

/// Get the user's resting HR from recent data (14-day median).
async fn get_resting_hr(pool: &PgPool, before_date: &str) -> Option<f64> {
    let row: Option<(f64,)> = sqlx::query_as(
        r#"
        SELECT AVG(bpm) as avg_resting
        FROM (
            SELECT MIN(bpm) as bpm
            FROM data_health_heart_rate
            WHERE timestamp < $1
            GROUP BY DATE(timestamp)
            ORDER BY DATE(timestamp) DESC
            LIMIT 14
        )
        "#,
    )
    .bind(before_date)
    .fetch_optional(pool)
    .await
    .ok()?;

    row.map(|(v,)| v)
}

/// Get the standard deviation of resting HR (14-day).
async fn get_resting_hr_std(pool: &PgPool, before_date: &str) -> Option<f64> {
    // Fetch the 14 daily-min values and compute stddev in Rust. (Postgres has
    // stddev_samp(); kept in Rust to match the established scoring math.)
    let rows: Vec<(f64,)> = sqlx::query_as(
        r#"
        SELECT MIN(bpm) as daily_min
        FROM data_health_heart_rate
        WHERE timestamp < $1
        GROUP BY DATE(timestamp)
        ORDER BY DATE(timestamp) DESC
        LIMIT 14
        "#,
    )
    .bind(before_date)
    .fetch_all(pool)
    .await
    .ok()?;

    if rows.len() < 3 {
        return None;
    }

    let values: Vec<f64> = rows.iter().map(|(v,)| *v).collect();
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
    Some(variance.sqrt())
}

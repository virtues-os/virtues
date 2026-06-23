//! Novelty scoring for dayline events.
//!
//! Embeds event summaries (bge-m3, via the llama-server sidecar) and produces
//! TWO orthogonal z-scored signals against a recency- and phase-weighted
//! baseline of recent events:
//!
//! - **Global novelty** (`novelty_z`): cosine distance from a kernel-weighted
//!   centroid of the baseline. "Rare in your life at all." Below midline =
//!   routine, > 1σ notable, > 2σ rare.
//! - **Local novelty** (`local_novelty_z`, with `lof_raw` underneath): a
//!   density-relative Local Outlier Factor, log-transformed and robustly
//!   standardized onto the same σ axis. "Off-pattern for its KIND" — e.g.
//!   first cardio when you always lift, which global novelty misses because
//!   both are "just a workout" against the global centroid.
//!
//! The two are intentionally NOT blended; any single salience number is
//! derived on read (magnitude/max), never stored.
//!
//! ## Baseline weighting (no hard window, no DOW step-function)
//!
//! Every neighbor's contribution to the GLOBAL centroid is
//! `K_time(days_ago) · K_phase(time-of-day, day-of-week)`:
//! - `K_time` — exponential recency decay (smooth; gives habituation for free).
//! - `K_phase` — a product of two von Mises (circular Gaussian) kernels on
//!   hour-of-day and weekday, so "unusual for a Tuesday morning" is continuous.
//!   This replaces the old hard 84-day cutoff + 6× same-day-of-week multiplier.
//!
//! Local novelty (LOF) is computed over the same recency-bounded baseline; the
//! kNN neighborhood already adapts to local density, which is the point.

use chrono::{DateTime, Datelike, NaiveDate, Timelike, Utc};
use sqlx::PgPool;
use std::f64::consts::{LN_2, PI};

use crate::dayline::embedding_ops::{
    bytes_to_embedding, cosine_distance, embed_input_for_event, embedding_to_bytes, k_nearest,
};
use crate::search::embedder::get_embedder;

/// Minimum distinct baseline DAYS required before any scoring (else NULL,
/// "calibrating").
const MIN_BASELINE_DAYS: usize = 3;

/// How far back to FETCH baseline events. Not a hard relevance cutoff — the
/// exponential `K_time` weight makes anything near the edge negligible — just a
/// bound on the DB read.
const QUERY_HORIZON_DAYS: i64 = 270;

/// Cap on baseline events (most-recent kept) so LOF's O(n²) kNN stays bounded.
const MAX_BASELINE_EVENTS: usize = 1500;

/// Recency half-life for `K_time` (events this many days old count half).
const TIME_HALF_LIFE_DAYS: f64 = 42.0;

/// von Mises concentrations for `K_phase`. Higher = sharper "same time" focus.
const HOUR_KAPPA: f64 = 2.0;
const DOW_KAPPA: f64 = 1.5;

/// Neighbors for the LOF computation.
const LOF_K: usize = 15;

/// Clamp for the local z-score (the global z is unclamped, matching its prior).
const Z_MAX: f64 = 3.0;

/// Compute global + local novelty for all events on a given day that need it.
///
/// Batch-embeds summaries in one call, builds the baseline once, scores each
/// event, and stores `embedding`, `novelty_z`, `lof_raw`, `local_novelty_z`.
/// Returns the number of events that received a global novelty_z.
pub async fn compute_novelty_for_day(pool: &PgPool, date: NaiveDate) -> anyhow::Result<u32> {
    // Events needing scoring. Re-scored if EITHER channel is missing, so a
    // backfill that adds the local channel reaches events already global-scored.
    let events: Vec<(String, String, DateTime<Utc>, Option<String>)> = sqlx::query_as(
        r#"
        SELECT e.id, e.event_summary, e.start_time, d.start_timezone
        FROM wiki_events e
        JOIN wiki_days d ON e.day_id = d.id
        WHERE d.date = $1
          AND e.event_summary IS NOT NULL
          AND e.event_summary != ''
          AND (e.novelty_z IS NULL OR e.local_novelty_z IS NULL)
          AND e.is_user_edited = FALSE
          AND e.is_sleep = FALSE
          AND e.user_hidden = FALSE
        "#,
    )
    .bind(date)
    .fetch_all(pool)
    .await?;

    if events.is_empty() {
        return Ok(0);
    }

    let summaries: Vec<String> = events
        .iter()
        .map(|(_, s, _, _)| embed_input_for_event(s))
        .collect();
    let embedder = get_embedder().await?;
    let embeddings = embedder.embed_batch_async(summaries).await?;

    let baseline = load_baseline(pool, date, true).await?;

    let mut scored = 0u32;
    for (i, (event_id, _summary, start_time, tz)) in events.iter().enumerate() {
        let embedding = match embeddings.get(i) {
            Some(e) => e,
            None => continue,
        };

        let phase = local_phase(*start_time, tz.as_deref());
        let novelty_z = baseline
            .as_ref()
            .and_then(|b| score_global(b, embedding, phase));
        let (lof_raw, local_z) = baseline
            .as_ref()
            .and_then(|b| score_local(b, embedding))
            .map_or((None, None), |(l, z)| (Some(l), Some(z)));

        if let Err(e) = store_scores(pool, event_id, embedding, novelty_z, lof_raw, local_z).await {
            tracing::warn!(event_id = %event_id, error = %e, "Failed to store novelty");
            continue;
        }
        if novelty_z.is_some() {
            scored += 1;
        }
    }

    tracing::info!(
        date = %date,
        scored,
        total = events.len(),
        baseline_days = baseline.as_ref().map_or(0, |b| b.distinct_days),
        baseline_events = baseline.as_ref().map_or(0, |b| b.embeddings.len()),
        lof_ready = baseline.as_ref().map_or(false, |b| b.lof.is_some()),
        "Novelty computation complete"
    );
    Ok(scored)
}

/// Compute novelty for a single event (on-demand, e.g. after a user clears an
/// edit). Returns the global novelty_z. Less efficient than the batch path —
/// it rebuilds the baseline — so prefer `compute_novelty_for_day` in bulk.
pub async fn compute_and_store_novelty(
    pool: &PgPool,
    event_id: &str,
    event_summary: &str,
    event_date: NaiveDate,
) -> anyhow::Result<Option<f64>> {
    if event_summary.trim().is_empty() {
        return Ok(None);
    }

    let embedder = get_embedder().await?;
    let embedding = embedder
        .embed_async(&embed_input_for_event(event_summary))
        .await?;

    // This event's own phase, for the global centroid weighting.
    let phase = match sqlx::query_as::<_, (DateTime<Utc>, Option<String>)>(
        r#"
        SELECT e.start_time, d.start_timezone
        FROM wiki_events e JOIN wiki_days d ON e.day_id = d.id
        WHERE e.id = $1
        "#,
    )
    .bind(event_id)
    .fetch_optional(pool)
    .await?
    {
        Some((start, tz)) => local_phase(start, tz.as_deref()),
        None => (12.0, 0.0), // event vanished mid-flight; harmless default
    };

    // GLOBAL ONLY on this path. The single-event entry points fire on every
    // event NEW/CONTINUE (tools/dayline_events.rs spawns this per edit), and
    // building the LOF model is O(n²). Global novelty is O(n) — the original
    // cost of this path. Local novelty is a chart signal with no latency need,
    // so it's left to the daily batch (`compute_novelty_for_day`), which the
    // re-score predicate reaches because local_novelty_z stays NULL here.
    let baseline = load_baseline(pool, event_date, false).await?;
    let novelty_z = baseline
        .as_ref()
        .and_then(|b| score_global(b, &embedding, phase));

    // Touch only embedding + novelty_z; preserve any local score from a prior
    // batch rather than clobbering it to NULL.
    sqlx::query("UPDATE wiki_events SET embedding = $1, novelty_z = $2 WHERE id = $3")
        .bind(embedding_to_bytes(&embedding))
        .bind(novelty_z)
        .bind(event_id)
        .execute(pool)
        .await?;
    Ok(novelty_z)
}

async fn store_scores(
    pool: &PgPool,
    event_id: &str,
    embedding: &[f32],
    novelty_z: Option<f64>,
    lof_raw: Option<f64>,
    local_z: Option<f64>,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE wiki_events
         SET embedding = $1, novelty_z = $2, lof_raw = $3, local_novelty_z = $4
         WHERE id = $5",
    )
    .bind(embedding_to_bytes(embedding))
    .bind(novelty_z)
    .bind(lof_raw)
    .bind(local_z)
    .bind(event_id)
    .execute(pool)
    .await?;
    Ok(())
}

// ============================================================================
// Baseline
// ============================================================================

/// Recency- and phase-bearing baseline plus the (event-independent) LOF model.
struct Baseline {
    embeddings: Vec<Vec<f32>>,
    phases: Vec<(f64, f64)>, // (hour-of-day ∈ [0,24), weekday ∈ [0,7))
    days_ago: Vec<f64>,
    distinct_days: usize,
    lof: Option<LofModel>,
}

/// Precomputed LOF geometry of the baseline set. Event-independent, so it's
/// built once and the query event is scored against it.
struct LofModel {
    k_distances: Vec<f64>,
    lrds: Vec<f64>, // local reachability density per baseline point
    ln_lof_median: f64,
    ln_lof_mad: f64,
}

/// Load the recency-bounded baseline. `with_lof` controls whether the O(n²)
/// LOF model is built — false on the single-event hot path (global only).
async fn load_baseline(
    pool: &PgPool,
    event_date: NaiveDate,
    with_lof: bool,
) -> anyhow::Result<Option<Baseline>> {
    let horizon_start = event_date - chrono::Duration::days(QUERY_HORIZON_DAYS);

    // Strictly-earlier days only (excludes the day being scored). Most-recent
    // first so the MAX_BASELINE_EVENTS cap keeps the freshest events.
    let rows: Vec<(Vec<u8>, NaiveDate, DateTime<Utc>, Option<String>)> = sqlx::query_as(
        r#"
        SELECT e.embedding, d.date, e.start_time, d.start_timezone
        FROM wiki_events e
        JOIN wiki_days d ON e.day_id = d.id
        WHERE d.date >= $1
          AND d.date < $2
          AND e.embedding IS NOT NULL
          AND e.is_sleep = FALSE
          AND e.user_hidden = FALSE
        ORDER BY d.date DESC
        "#,
    )
    .bind(horizon_start)
    .bind(event_date)
    .fetch_all(pool)
    .await?;

    let mut embeddings = Vec::new();
    let mut phases = Vec::new();
    let mut days_ago = Vec::new();
    let mut distinct_dates: std::collections::HashSet<NaiveDate> = std::collections::HashSet::new();

    for (blob, row_date, start_time, tz) in &rows {
        let emb = bytes_to_embedding(blob);
        if emb.is_empty() {
            continue;
        }
        if embeddings.len() >= MAX_BASELINE_EVENTS {
            break;
        }
        embeddings.push(emb);
        phases.push(local_phase(*start_time, tz.as_deref()));
        days_ago.push((event_date - *row_date).num_days().max(0) as f64);
        distinct_dates.insert(*row_date);
    }

    if distinct_dates.len() < MIN_BASELINE_DAYS || embeddings.is_empty() {
        return Ok(None);
    }

    let lof = if with_lof {
        build_lof_model(&embeddings)
    } else {
        None
    };

    Ok(Some(Baseline {
        embeddings,
        phases,
        days_ago,
        distinct_days: distinct_dates.len(),
        lof,
    }))
}

/// Build the LOF model: k-distance, local reachability density (lrd), and the
/// ln(LOF) median/MAD over the baseline (the robust standardization reference).
fn build_lof_model(embeddings: &[Vec<f32>]) -> Option<LofModel> {
    let n = embeddings.len();
    if n < LOF_K + 1 {
        return None;
    }
    let k = LOF_K.min(n - 1);

    // Pass 1: each point's k nearest neighbors and its k-distance.
    let mut neighbors: Vec<Vec<(usize, f64)>> = Vec::with_capacity(n);
    let mut k_distances = vec![0.0f64; n];
    for (i, emb) in embeddings.iter().enumerate() {
        let nn = k_nearest(emb, embeddings, k, Some(i));
        k_distances[i] = nn.last().map(|x| x.1).unwrap_or(0.0);
        neighbors.push(nn);
    }

    // Pass 2: local reachability density (inverse mean reachability-distance).
    let mut lrds = vec![0.0f64; n];
    for i in 0..n {
        let mut sum_reach = 0.0;
        for &(o, d) in &neighbors[i] {
            sum_reach += k_distances[o].max(d);
        }
        let avg_reach = sum_reach / neighbors[i].len().max(1) as f64;
        lrds[i] = if avg_reach > 1e-12 { 1.0 / avg_reach } else { 1e12 };
    }

    // Pass 3: per-point LOF, collect ln(LOF) for the standardization reference.
    let mut ln_lofs = Vec::with_capacity(n);
    for i in 0..n {
        let mean_neighbor_lrd =
            neighbors[i].iter().map(|&(o, _)| lrds[o]).sum::<f64>() / neighbors[i].len().max(1) as f64;
        let lof = mean_neighbor_lrd / lrds[i];
        if lof > 0.0 && lof.is_finite() {
            ln_lofs.push(lof.ln());
        }
    }

    if ln_lofs.len() < LOF_K {
        return None;
    }
    let med = median(&ln_lofs);
    let mad = median_abs_dev(&ln_lofs, med);
    if mad < 1e-9 {
        // No spread in outlierness — can't meaningfully z-score (super-uniform).
        return None;
    }

    Some(LofModel {
        k_distances,
        lrds,
        ln_lof_median: med,
        ln_lof_mad: mad,
    })
}

// ============================================================================
// Scoring
// ============================================================================

/// Global novelty: z-score of cosine distance from the kernel-weighted
/// centroid. The centroid depends on the event's phase (so it's recomputed per
/// event, cheap: O(n·dim)).
fn score_global(baseline: &Baseline, emb: &[f32], phase: (f64, f64)) -> Option<f64> {
    let n = baseline.embeddings.len();
    let dim = baseline.embeddings.iter().find(|e| !e.is_empty())?.len();

    let mut weights = Vec::with_capacity(n);
    let mut centroid = vec![0.0f64; dim];
    let mut total_w = 0.0;
    for i in 0..n {
        let w = time_weight(baseline.days_ago[i]) * phase_weight(phase, baseline.phases[i]);
        weights.push(w);
        total_w += w;
        let e = &baseline.embeddings[i];
        for (j, c) in centroid.iter_mut().enumerate().take(dim.min(e.len())) {
            *c += e[j] as f64 * w;
        }
    }
    if total_w <= 0.0 {
        return None;
    }
    for c in &mut centroid {
        *c /= total_w;
    }
    let norm: f64 = centroid.iter().map(|v| v * v).sum::<f64>().sqrt();
    if norm <= 0.0 {
        return None;
    }
    for c in &mut centroid {
        *c /= norm;
    }
    let centroid_f32: Vec<f32> = centroid.iter().map(|&v| v as f32).collect();

    // Weighted mean/std of baseline distances to the (weighted) centroid.
    let dists: Vec<f64> = baseline
        .embeddings
        .iter()
        .map(|e| cosine_distance(e, &centroid_f32))
        .collect();
    let mut wmean = 0.0;
    for i in 0..n {
        wmean += weights[i] * dists[i];
    }
    wmean /= total_w;
    let mut wvar = 0.0;
    for i in 0..n {
        wvar += weights[i] * (dists[i] - wmean).powi(2);
    }
    wvar /= total_w;
    let std = wvar.sqrt();
    if std < 1e-10 {
        return None;
    }

    Some((cosine_distance(emb, &centroid_f32) - wmean) / std)
}

/// Local novelty via LOF: returns (raw_lof, local_novelty_z). The z is
/// `clamp(±3, (ln(lof) - median) / (1.4826·MAD))` — robust standardization so
/// a few extreme events don't compress everyone toward zero.
fn score_local(baseline: &Baseline, emb: &[f32]) -> Option<(f64, f64)> {
    let lof = baseline.lof.as_ref()?;
    let n = baseline.embeddings.len();
    let k = LOF_K.min(n.saturating_sub(1)).max(1);

    let nn = k_nearest(emb, &baseline.embeddings, k, None);
    if nn.is_empty() {
        return None;
    }

    let mut sum_reach = 0.0;
    for &(o, d) in &nn {
        sum_reach += lof.k_distances[o].max(d);
    }
    let avg_reach = sum_reach / nn.len() as f64;
    let lrd_q = if avg_reach > 1e-12 { 1.0 / avg_reach } else { 1e12 };

    let mean_neighbor_lrd = nn.iter().map(|&(o, _)| lof.lrds[o]).sum::<f64>() / nn.len() as f64;
    let raw_lof = mean_neighbor_lrd / lrd_q;
    if !raw_lof.is_finite() || raw_lof <= 0.0 {
        return None;
    }

    let z = ((raw_lof.ln() - lof.ln_lof_median) / (1.4826 * lof.ln_lof_mad)).clamp(-Z_MAX, Z_MAX);
    Some((raw_lof, z))
}

// ============================================================================
// Kernels & helpers
// ============================================================================

/// Exponential recency weight: 1.0 today, 0.5 at one half-life.
fn time_weight(days_ago: f64) -> f64 {
    (-(days_ago.max(0.0)) * LN_2 / TIME_HALF_LIFE_DAYS).exp()
}

/// Product of two von Mises kernels (hour-of-day, weekday). 1.0 at identical
/// phase, smoothly decaying with circular distance. Replaces the 6× DOW hack.
fn phase_weight(a: (f64, f64), b: (f64, f64)) -> f64 {
    let hour_term = HOUR_KAPPA * ((2.0 * PI * (a.0 - b.0) / 24.0).cos() - 1.0);
    let dow_term = DOW_KAPPA * ((2.0 * PI * (a.1 - b.1) / 7.0).cos() - 1.0);
    (hour_term + dow_term).exp()
}

/// Local (timezone-aware) hour-of-day and weekday phase of an event.
fn local_phase(start: DateTime<Utc>, tz: Option<&str>) -> (f64, f64) {
    if let Some(z) = tz.and_then(|t| t.parse::<chrono_tz::Tz>().ok()) {
        let l = start.with_timezone(&z);
        (
            l.hour() as f64 + l.minute() as f64 / 60.0,
            l.weekday().num_days_from_sunday() as f64,
        )
    } else {
        (
            start.hour() as f64 + start.minute() as f64 / 60.0,
            start.weekday().num_days_from_sunday() as f64,
        )
    }
}

fn median(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    let mut v = xs.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

fn median_abs_dev(xs: &[f64], med: f64) -> f64 {
    let dev: Vec<f64> = xs.iter().map(|x| (x - med).abs()).collect();
    median(&dev)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_weight_halves_at_half_life() {
        assert!((time_weight(0.0) - 1.0).abs() < 1e-9);
        assert!((time_weight(TIME_HALF_LIFE_DAYS) - 0.5).abs() < 1e-9);
        assert!(time_weight(1000.0) < 1e-6);
    }

    #[test]
    fn phase_weight_peaks_at_same_phase() {
        let p = (9.0, 2.0);
        assert!((phase_weight(p, p) - 1.0).abs() < 1e-9);
        // Same hour, different weekday → lower.
        assert!(phase_weight(p, (9.0, 5.0)) < 1.0);
        // 24h is one full period → identical to 0h offset (circularity).
        assert!((phase_weight((9.0, 2.0), (9.0 + 24.0, 2.0)) - 1.0).abs() < 1e-9);
        // Opposite time of day → minimal.
        assert!(phase_weight((9.0, 2.0), (21.0, 2.0)) < phase_weight((9.0, 2.0), (11.0, 2.0)));
    }

    #[test]
    fn median_and_mad() {
        assert!((median(&[3.0, 1.0, 2.0]) - 2.0).abs() < 1e-9);
        assert!((median(&[1.0, 2.0, 3.0, 4.0]) - 2.5).abs() < 1e-9);
        assert!((median_abs_dev(&[1.0, 2.0, 3.0, 4.0, 5.0], 3.0) - 1.0).abs() < 1e-9);
    }

    /// An event far from a populated cluster should score a higher LOF (and z)
    /// than an event sitting inside the cluster.
    #[test]
    fn lof_flags_outlier_above_core() {
        // 24 points spread along an arc near angle 0 with graded spacing (so the
        // LOF distribution has real spread → MAD > 0), used as the baseline.
        let mut embeddings: Vec<Vec<f32>> = Vec::new();
        for i in 0..24 {
            let theta = (i as f64) * 0.02; // tight, slightly uneven cluster
            embeddings.push(vec![theta.cos() as f32, theta.sin() as f32]);
        }
        let model = build_lof_model(&embeddings).expect("model should build");
        let baseline = Baseline {
            embeddings,
            phases: vec![(12.0, 0.0); 24],
            days_ago: vec![1.0; 24],
            distinct_days: 24,
            lof: Some(model),
        };

        // Core point sits among the cluster; outlier is orthogonal.
        let core = vec![(0.2f64).cos() as f32, (0.2f64).sin() as f32];
        let outlier = vec![0.0f32, 1.0f32];

        let (core_lof, core_z) = score_local(&baseline, &core).expect("core scored");
        let (out_lof, out_z) = score_local(&baseline, &outlier).expect("outlier scored");

        assert!(
            out_lof > core_lof,
            "outlier LOF {out_lof} should exceed core LOF {core_lof}"
        );
        assert!(out_z > core_z, "outlier z {out_z} should exceed core z {core_z}");
    }
}

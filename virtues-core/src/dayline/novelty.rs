//! Novelty scoring for dayline events.
//!
//! Embeds event summaries via nomic-embed-text-v1.5 and computes a z-scored
//! novelty metric against a 12-week weighted baseline. Same-day-of-week events
//! are weighted 6x to capture "is this Tuesday unusual for your Tuesdays?"
//!
//! The novelty_z score IS the Y-axis of the Dayline visualization:
//!   < 0σ  → routine (below midline)
//!   0-1σ  → normal variation
//!   > 1σ  → notable (above midline)
//!   > 2σ  → genuinely rare

use chrono::NaiveDate;
use sqlx::PgPool;

use crate::search::embedder::get_embedder;

/// Minimum number of distinct baseline DAYS (not events) required for z-scoring.
/// Below this, novelty_z is set to NULL ("calibrating").
const MIN_BASELINE_DAYS: usize = 3;

/// Baseline window: 12 weeks (84 days) of history.
const BASELINE_WINDOW_DAYS: i64 = 84;

/// Weight multiplier for same-day-of-week events in the baseline.
/// 6x means "your Tuesdays" get equal weight to "all other days" combined
/// (12 same-day × 6 = 72 effective weight vs 72 other days × 1 = 72).
const SAME_DOW_WEIGHT: f64 = 6.0;

/// Compute novelty for all events on a given day that need scoring.
///
/// Batch-embeds all summaries in one ONNX call, computes the baseline once,
/// then z-scores each event against it. Skips user_edited/sleep/hidden events.
///
/// Returns the number of events that received a novelty_z score.
pub async fn compute_novelty_for_day(pool: &PgPool, date: NaiveDate) -> anyhow::Result<u32> {
    let date_str = date.format("%Y-%m-%d").to_string();

    // Find events needing novelty computation
    let events: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT e.id, e.event_summary
        FROM wiki_events e
        JOIN wiki_days d ON e.day_id = d.id
        WHERE d.date = $1
          AND e.event_summary IS NOT NULL
          AND e.event_summary != ''
          AND e.novelty_z IS NULL
          AND e.is_user_edited = FALSE
          AND e.is_sleep = FALSE
          AND e.user_hidden = FALSE
        "#,
    )
    .bind(&date_str)
    .fetch_all(pool)
    .await?;

    if events.is_empty() {
        return Ok(0);
    }

    // 1. Batch-embed all summaries in a single ONNX call
    let summaries: Vec<String> = events.iter().map(|(_, s)| s.clone()).collect();
    let embedder = get_embedder().await?;
    let embeddings = embedder.embed_batch_async(summaries).await?;

    // 2. Compute the baseline ONCE for this day (same for all events)
    let baseline = load_baseline(pool, date).await?;

    // 3. Score and store each event
    let mut scored = 0u32;
    for (i, (event_id, _summary)) in events.iter().enumerate() {
        let embedding = match embeddings.get(i) {
            Some(e) => e,
            None => continue,
        };

        let embedding_bytes = embedding_to_bytes(embedding);
        let novelty_z = baseline
            .as_ref()
            .and_then(|b| score_against_baseline(embedding, b));

        // Single UPDATE for both embedding and novelty_z
        if let Err(e) = sqlx::query(
            "UPDATE wiki_events SET embedding = $1, novelty_z = $2 WHERE id = $3",
        )
        .bind(&embedding_bytes)
        .bind(novelty_z)
        .bind(event_id.as_str())
        .execute(pool)
        .await
        {
            tracing::warn!(event_id = %event_id, error = %e, "Failed to store novelty for event");
            continue;
        }

        if novelty_z.is_some() {
            scored += 1;
        }
    }

    let embedded = embeddings.len().min(events.len());
    tracing::info!(
        date = %date,
        embedded,
        scored,
        total = events.len(),
        baseline_days = baseline.as_ref().map_or(0, |b| b.distinct_days),
        "Novelty computation complete"
    );
    Ok(scored)
}

/// Compute novelty for a single event. Used for on-demand recomputation
/// (e.g., after a user clears an edit). Less efficient than batch — prefer
/// `compute_novelty_for_day` when processing multiple events.
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
    let embedding = embedder.embed_async(event_summary).await?;
    let embedding_bytes = embedding_to_bytes(&embedding);

    let baseline = load_baseline(pool, event_date).await?;
    let novelty_z = baseline
        .as_ref()
        .and_then(|b| score_against_baseline(&embedding, b));

    sqlx::query("UPDATE wiki_events SET embedding = $1, novelty_z = $2 WHERE id = $3")
        .bind(&embedding_bytes)
        .bind(novelty_z)
        .bind(event_id)
        .execute(pool)
        .await?;

    Ok(novelty_z)
}

// ============================================================================
// Internal: baseline computation
// ============================================================================

/// Pre-computed baseline: weighted centroid + distance statistics.
/// Computed once per day, reused for all events on that day.
struct Baseline {
    centroid: Vec<f32>,
    mean_distance: f64,
    std_distance: f64,
    distinct_days: usize,
}

/// Load and compute the 12-week baseline for a given date.
/// Returns None if fewer than MIN_BASELINE_DAYS distinct days have embeddings.
async fn load_baseline(pool: &PgPool, event_date: NaiveDate) -> anyhow::Result<Option<Baseline>> {
    let baseline_start = (event_date - chrono::Duration::days(BASELINE_WINDOW_DAYS))
        .format("%Y-%m-%d")
        .to_string();
    let event_date_str = event_date.format("%Y-%m-%d").to_string();
    let event_dow = event_date.format("%w").to_string(); // 0=Sun, 1=Mon, ...

    // Fetch all baseline embeddings with their date
    let rows: Vec<(Vec<u8>, String)> = sqlx::query_as(
        r#"
        SELECT e.embedding, d.date
        FROM wiki_events e
        JOIN wiki_days d ON e.day_id = d.id
        WHERE d.date >= $1
          AND d.date < $2
          AND e.embedding IS NOT NULL
          AND e.is_sleep = FALSE
          AND e.user_hidden = FALSE
        "#,
    )
    .bind(&baseline_start)
    .bind(&event_date_str)
    .fetch_all(pool)
    .await?;

    // Check distinct days, not event count
    let mut distinct_dates: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for (_, date_str) in &rows {
        distinct_dates.insert(date_str.as_str());
    }
    if distinct_dates.len() < MIN_BASELINE_DAYS {
        return Ok(None);
    }

    // Parse embeddings and compute DOW weights
    let mut weighted_embeddings: Vec<(Vec<f32>, f64)> = Vec::with_capacity(rows.len());
    for (blob, date_str) in &rows {
        let emb = bytes_to_embedding(blob);
        if emb.is_empty() {
            continue;
        }

        let row_date = date_str.parse::<NaiveDate>().unwrap_or(event_date);
        let row_dow = row_date.format("%w").to_string();
        let weight = if row_dow == event_dow {
            SAME_DOW_WEIGHT
        } else {
            1.0
        };

        weighted_embeddings.push((emb, weight));
    }

    if weighted_embeddings.is_empty() {
        return Ok(None);
    }

    // Compute weighted centroid
    let dim = weighted_embeddings[0].0.len();
    let mut centroid = vec![0.0f64; dim];
    let mut total_weight = 0.0f64;

    for (emb, weight) in &weighted_embeddings {
        total_weight += weight;
        for (j, &val) in emb.iter().enumerate() {
            if j < dim {
                centroid[j] += val as f64 * weight;
            }
        }
    }

    for v in centroid.iter_mut() {
        *v /= total_weight;
    }

    // Normalize centroid to unit vector
    let norm: f64 = centroid.iter().map(|v| v * v).sum::<f64>().sqrt();
    if norm <= 0.0 {
        return Ok(None);
    }
    for v in centroid.iter_mut() {
        *v /= norm;
    }

    let centroid_f32: Vec<f32> = centroid.iter().map(|&v| v as f32).collect();

    // Compute cosine distances of all baseline events to centroid
    let baseline_distances: Vec<f64> = weighted_embeddings
        .iter()
        .map(|(emb, _)| cosine_distance(emb, &centroid_f32))
        .collect();

    let n = baseline_distances.len() as f64;
    let mean = baseline_distances.iter().sum::<f64>() / n;
    let variance = baseline_distances
        .iter()
        .map(|d| (d - mean).powi(2))
        .sum::<f64>()
        / n;
    let std = variance.sqrt();

    if std < 1e-10 {
        return Ok(None);
    }

    Ok(Some(Baseline {
        centroid: centroid_f32,
        mean_distance: mean,
        std_distance: std,
        distinct_days: distinct_dates.len(),
    }))
}

/// Z-score a single event embedding against a pre-computed baseline.
fn score_against_baseline(embedding: &[f32], baseline: &Baseline) -> Option<f64> {
    let distance = cosine_distance(embedding, &baseline.centroid);
    let z = (distance - baseline.mean_distance) / baseline.std_distance;
    Some(z)
}

// ============================================================================
// Helpers
// ============================================================================

/// Cosine distance = 1 - cosine_similarity. Range: [0, 2].
fn cosine_distance(a: &[f32], b: &[f32]) -> f64 {
    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;

    for i in 0..a.len().min(b.len()) {
        let va = a[i] as f64;
        let vb = b[i] as f64;
        dot += va * vb;
        norm_a += va * va;
        norm_b += vb * vb;
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom <= 0.0 {
        return 1.0; // Maximum distance if either is zero vector
    }

    1.0 - (dot / denom)
}

/// Serialize f32 embedding to little-endian bytes for BYTEA storage.
fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Deserialize little-endian bytes from BYTEA to f32 embedding.
fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_distance_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let d = cosine_distance(&a, &b);
        assert!((d - 0.0).abs() < 1e-6, "Identical vectors should have distance ~0, got {d}");
    }

    #[test]
    fn test_cosine_distance_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let d = cosine_distance(&a, &b);
        assert!((d - 1.0).abs() < 1e-6, "Orthogonal vectors should have distance ~1, got {d}");
    }

    #[test]
    fn test_cosine_distance_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let d = cosine_distance(&a, &b);
        assert!((d - 2.0).abs() < 1e-6, "Opposite vectors should have distance ~2, got {d}");
    }

    #[test]
    fn test_cosine_distance_zero_vector() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![0.0, 0.0, 0.0];
        let d = cosine_distance(&a, &b);
        assert!((d - 1.0).abs() < 1e-6, "Zero vector should give distance 1.0, got {d}");
    }

    #[test]
    fn test_embedding_roundtrip() {
        let original = vec![1.0f32, -2.5, 3.14159, 0.0, f32::MIN, f32::MAX];
        let bytes = embedding_to_bytes(&original);
        let restored = bytes_to_embedding(&bytes);
        assert_eq!(original, restored);
    }

    #[test]
    fn test_embedding_empty() {
        let bytes = embedding_to_bytes(&[]);
        assert!(bytes.is_empty());
        let restored = bytes_to_embedding(&bytes);
        assert!(restored.is_empty());
    }

    #[test]
    fn test_score_against_baseline() {
        // Baseline centroid at [1, 0, 0], mean distance 0.5, std 0.2
        let baseline = Baseline {
            centroid: vec![1.0, 0.0, 0.0],
            mean_distance: 0.5,
            std_distance: 0.2,
            distinct_days: 10,
        };

        // Event exactly at centroid → distance 0 → z = (0 - 0.5) / 0.2 = -2.5
        let embedding = vec![1.0, 0.0, 0.0];
        let z = score_against_baseline(&embedding, &baseline).unwrap();
        assert!((z - (-2.5)).abs() < 1e-6, "Expected z=-2.5, got {z}");

        // Event orthogonal → distance 1.0 → z = (1.0 - 0.5) / 0.2 = 2.5
        let embedding = vec![0.0, 1.0, 0.0];
        let z = score_against_baseline(&embedding, &baseline).unwrap();
        assert!((z - 2.5).abs() < 1e-6, "Expected z=2.5, got {z}");
    }
}

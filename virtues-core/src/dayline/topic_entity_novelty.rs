//! Topic and entity novelty scoring.
//!
//! Two different scoring approaches:
//!
//! **Topics** — embedding-based centroid distance (semantic novelty).
//! Each topic string is embedded via nomic-embed and cached in `search_topic_cache`.
//! Novelty = cosine distance from the 12-week baseline topic centroid, z-scored.
//! "house-hunting" is semantically far from her typical topic cloud → high novelty.
//! "design" is close to the centroid → low novelty.
//!
//! **Entities** — frequency-based binary presence (structural novelty).
//! For each entity ID, count how many baseline days it appeared on.
//! Z-score using binomial distribution. Maya appears daily → routine.
//! Rachel appears 3 times in 84 days → novel.
//!
//! All scores clamped to ±3.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use sqlx::PgPool;

use crate::search::embedder::get_embedder;

/// Read a JSONB `["a","b"]` column into a Vec<String>.
///
/// `topics` and `entities` are JSONB, but this module used to decode them as
/// `Option<String>` and then `serde_json::from_str` the result — which fails at
/// the sqlx layer with "Rust type Option<String> (as SQL type TEXT) is not
/// compatible with SQL type JSONB", on every row. One of several type errors
/// that had kept this function from ever completing a single call.
fn as_strings(v: &Option<serde_json::Value>) -> Vec<String> {
    v.as_ref()
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Baseline window: 12 weeks (84 days), same as event novelty.
const BASELINE_WINDOW_DAYS: i64 = 84;

/// Maximum z-score (clamp).
const Z_MAX: f64 = 3.0;

/// Minimum baseline days required for scoring.
const MIN_BASELINE_DAYS: usize = 3;

/// Compute topic and entity novelty for all events on a given day.
///
/// - Topics: scored via embedding centroid distance (semantic)
/// - Entities: scored via frequency z-score (structural)
///
/// Returns the number of events updated.
pub async fn compute_topic_entity_novelty(
    pool: &PgPool,
    date: NaiveDate,
) -> anyhow::Result<u32> {
    // Bind DATEs as DATEs.
    //
    // This function used to bind `date.format("%Y-%m-%d")` — a String — against
    // the `date` column. Postgres has no `date = text` operator, so EVERY call
    // failed with "operator does not exist: date = text" and the function had
    // never once scored a single event, on any day, for any user. The caller
    // swallowed the error in a `match` and the cron reported a cheerful
    // non-zero count of events *seen*. cf. novelty.rs:87, which binds correctly.
    let baseline_start = date - chrono::Duration::days(BASELINE_WINDOW_DAYS);

    // 1. Load today's events (id, topics JSON, entities JSON)
    let today_events: Vec<(String, Option<serde_json::Value>, Option<serde_json::Value>)> = sqlx::query_as(
        r#"
        SELECT e.id, e.topics, e.entities
        FROM wiki_events e
        JOIN wiki_days d ON e.day_id = d.id
        WHERE d.date = $1
          AND e.is_sleep = FALSE
          AND e.user_hidden = FALSE
        "#,
    )
    .bind(date)
    .fetch_all(pool)
    .await?;

    if today_events.is_empty() {
        return Ok(0);
    }

    // 2. Collect all unique topics and entities appearing today
    let mut all_topics: HashSet<String> = HashSet::new();
    let mut all_entities: HashSet<String> = HashSet::new();

    for (_, topics_json, entities_json) in &today_events {
        all_topics.extend(as_strings(topics_json));
        all_entities.extend(as_strings(entities_json));
    }

    // 3. Load baseline data
    let baseline_rows: Vec<(NaiveDate, Option<serde_json::Value>, Option<serde_json::Value>)> = sqlx::query_as(
        r#"
        SELECT d.date, e.topics, e.entities
        FROM wiki_events e
        JOIN wiki_days d ON e.day_id = d.id
        WHERE d.date >= $1
          AND d.date < $2
          AND e.is_sleep = FALSE
          AND e.user_hidden = FALSE
        "#,
    )
    .bind(baseline_start)
    .bind(date)
    .fetch_all(pool)
    .await?;

    // Count distinct baseline days
    let total_baseline_days: usize = {
        let mut dates: HashSet<NaiveDate> = HashSet::new();
        for (d, _, _) in &baseline_rows {
            dates.insert(*d);
        }
        dates.len()
    };

    // 4a. TOPIC SCORING — embedding centroid distance
    let topic_scores: HashMap<String, f64> =
        if !all_topics.is_empty() && total_baseline_days >= MIN_BASELINE_DAYS {
            score_topics_by_embedding(pool, &all_topics, &baseline_rows).await?
        } else {
            // NOT max novelty. Score nothing.
            //
            // This used to assign Z_MAX (3.0) to every topic when the baseline
            // was thin — so a brand-new box declared everything it saw maximally
            // novel, and week one of someone's life came out as one long peak.
            // "I have no baseline" and "this is unprecedented" are opposite
            // claims and must never produce the same number.
            //
            // An empty map leaves `topic_novelty` NULL, which reads as
            // "calibrating" everywhere downstream. Absence of a measurement is
            // not a measurement.
            tracing::debug!(
                baseline_days = total_baseline_days,
                min = MIN_BASELINE_DAYS,
                topics = all_topics.len(),
                "baseline too thin for topic novelty — leaving NULL, not Z_MAX"
            );
            HashMap::new()
        };

    // 4b. ENTITY SCORING — frequency z-score (unchanged)
    let entity_scores = {
        let mut entity_days: HashMap<String, HashSet<NaiveDate>> = HashMap::new();
        for (day_date, _, entities_json) in &baseline_rows {
            for e in as_strings(entities_json) {
                entity_days.entry(e).or_default().insert(*day_date);
            }
        }
        score_entities_by_frequency(&all_entities, &entity_days, total_baseline_days)
    };

    // 5. Write back per-event
    let mut updated = 0u32;
    for (event_id, topics_json, entities_json) in &today_events {
        let event_topics = as_strings(topics_json);
        let event_entities = as_strings(entities_json);

        let raw_topic_novelty: HashMap<&str, f64> = event_topics
            .iter()
            .filter_map(|t| topic_scores.get(t.as_str()).map(|&z| (t.as_str(), z)))
            .collect();

        // Center topic z-scores per event — subtract event mean so topics
        // orbit symmetrically around the event dot instead of biasing +Y
        let event_topic_novelty: HashMap<&str, f64> = if raw_topic_novelty.is_empty() {
            raw_topic_novelty
        } else {
            let mean_z: f64 =
                raw_topic_novelty.values().sum::<f64>() / raw_topic_novelty.len() as f64;
            raw_topic_novelty
                .into_iter()
                .map(|(t, z)| (t, (z - mean_z).max(-Z_MAX).min(Z_MAX)))
                .collect()
        };

        let event_entity_novelty: HashMap<&str, f64> = event_entities
            .iter()
            .filter_map(|e| entity_scores.get(e.as_str()).map(|&z| (e.as_str(), z)))
            .collect();

        let topic_json = if event_topic_novelty.is_empty() {
            None
        } else {
            Some(serde_json::json!(event_topic_novelty))
        };

        let entity_json = if event_entity_novelty.is_empty() {
            None
        } else {
            Some(serde_json::json!(event_entity_novelty))
        };

        // JSONB columns take a Value, not a serialized String. Binding a String
        // gets you "column topic_novelty is of type jsonb but expression is of
        // type text" — the last of six distinct type errors that stood between
        // this function and ever writing a single row.
        sqlx::query(
            "UPDATE wiki_events SET topic_novelty = $1, entity_novelty = $2 WHERE id = $3",
        )
        .bind(&topic_json)
        .bind(&entity_json)
        .bind(event_id)
        .execute(pool)
        .await?;

        updated += 1;
    }

    if updated > 0 {
        tracing::info!(
            date = %date,
            topics = all_topics.len(),
            entities = all_entities.len(),
            events = updated,
            baseline_days = total_baseline_days,
            "Topic/entity novelty computed"
        );
    }

    Ok(updated)
}

// ============================================================================
// Topic scoring: embedding centroid distance
// ============================================================================

/// Score topics by cosine distance from the baseline topic centroid.
///
/// Same algorithm as event novelty (novelty.rs) but applied to individual
/// topic strings instead of full event summaries.
async fn score_topics_by_embedding(
    pool: &PgPool,
    today_topics: &HashSet<String>,
    baseline_rows: &[(NaiveDate, Option<serde_json::Value>, Option<serde_json::Value>)],
) -> anyhow::Result<HashMap<String, f64>> {
    let embedder = get_embedder().await?;

    // Collect all unique baseline topic strings
    let mut baseline_topics: HashSet<String> = HashSet::new();
    for (_, topics_json, _) in baseline_rows {
        baseline_topics.extend(as_strings(topics_json));
    }

    // Merge with today's topics for a complete set to embed
    let mut all_unique: Vec<String> = baseline_topics
        .union(today_topics)
        .cloned()
        .collect();
    all_unique.sort();

    if all_unique.is_empty() {
        return Ok(HashMap::new());
    }

    // Ensure all topics have embeddings in the cache
    let embeddings = ensure_topic_embeddings(pool, &embedder, &all_unique).await?;

    // Build baseline embedding list (one per occurrence, not per unique topic)
    // Weight by how often each topic appeared in the baseline
    let mut baseline_embeddings: Vec<&[f32]> = Vec::new();
    for (_, topics_json, _) in baseline_rows {
        for t in as_strings(topics_json) {
            if let Some(emb) = embeddings.get(t.as_str()) {
                baseline_embeddings.push(emb.as_slice());
            }
        }
    }

    if baseline_embeddings.is_empty() {
        // Score nothing — the same rule the caller applies, for the same reason.
        //
        // The caller already refuses to score when the baseline is too FEW DAYS
        // (see the `MIN_BASELINE_DAYS` branch and its comment: "'I have no
        // baseline' and 'this is unprecedented' are opposite claims and must
        // never produce the same number"). This is the other way to have no
        // baseline: enough days, but none of their rows carried a usable
        // embedding. It returned Z_MAX for every topic — exactly the behavior
        // that comment was written to end, reachable by a different route.
        //
        // An empty map leaves `topic_novelty` NULL, which reads as
        // "calibrating" downstream. Absence of a measurement is not a
        // measurement.
        tracing::debug!(
            topics = today_topics.len(),
            "no usable baseline embeddings — leaving topic novelty NULL, not Z_MAX"
        );
        return Ok(HashMap::new());
    }

    // Compute centroid of all baseline topic embeddings
    let dim = baseline_embeddings[0].len();
    let mut centroid = vec![0.0f64; dim];
    for emb in &baseline_embeddings {
        for (j, &val) in emb.iter().enumerate() {
            if j < dim {
                centroid[j] += val as f64;
            }
        }
    }
    let n = baseline_embeddings.len() as f64;
    for v in centroid.iter_mut() {
        *v /= n;
    }

    // Normalize centroid
    let norm: f64 = centroid.iter().map(|v| v * v).sum::<f64>().sqrt();
    if norm <= 0.0 {
        return Ok(today_topics.iter().map(|t| (t.clone(), Z_MAX)).collect());
    }
    for v in centroid.iter_mut() {
        *v /= norm;
    }
    let centroid_f32: Vec<f32> = centroid.iter().map(|&v| v as f32).collect();

    // Compute baseline distances (for mean/std)
    let baseline_distances: Vec<f64> = baseline_embeddings
        .iter()
        .map(|emb| cosine_distance(emb, &centroid_f32))
        .collect();

    let mean = baseline_distances.iter().sum::<f64>() / baseline_distances.len() as f64;
    let variance = baseline_distances
        .iter()
        .map(|d| (d - mean).powi(2))
        .sum::<f64>()
        / baseline_distances.len() as f64;
    let std = variance.sqrt();

    if std < 1e-10 {
        return Ok(today_topics.iter().map(|t| (t.clone(), 0.0)).collect());
    }

    // Score today's topics
    let mut scores = HashMap::new();
    for topic in today_topics {
        if let Some(emb) = embeddings.get(topic.as_str()) {
            let dist = cosine_distance(emb, &centroid_f32);
            let z = ((dist - mean) / std).max(-Z_MAX).min(Z_MAX);
            scores.insert(topic.clone(), z);
        } else {
            scores.insert(topic.clone(), Z_MAX);
        }
    }

    Ok(scores)
}

/// Ensure all topic strings have embeddings in search_topic_cache.
/// Returns a map of topic → embedding for ALL requested topics.
async fn ensure_topic_embeddings(
    pool: &PgPool,
    embedder: &std::sync::Arc<crate::search::embedder::LocalEmbedder>,
    topics: &[String],
) -> anyhow::Result<HashMap<String, Vec<f32>>> {
    // `search_topic_cache.embedding` is `halfvec` (migration 0030 moved the
    // whole vector store to fp16). It was still being read and written as
    // `vector`, which sqlx rejects outright — yet another reason this function
    // had never completed a call. Cast at the boundary, exactly as
    // `search/query.rs` does with `$1::halfvec`.
    let mut cached: HashMap<String, Vec<f32>> = HashMap::new();
    let rows: Vec<(String, pgvector::Vector)> = sqlx::query_as(
        "SELECT topic, embedding::vector FROM search_topic_cache",
    )
    .fetch_all(pool)
    .await?;

    for (topic, vec) in rows {
        cached.insert(topic, vec.to_vec());
    }

    // Find topics needing embedding
    let need_embedding: Vec<String> = topics
        .iter()
        .filter(|t| !cached.contains_key(t.as_str()))
        .cloned()
        .collect();

    if !need_embedding.is_empty() {
        tracing::debug!("Embedding {} new topics", need_embedding.len());
        let new_embeddings = embedder.embed_batch_async(need_embedding.clone()).await?;

        for (topic, embedding) in need_embedding.iter().zip(new_embeddings.iter()) {
            sqlx::query(
                "INSERT INTO search_topic_cache (topic, embedding) VALUES ($1, $2::halfvec) \
                 ON CONFLICT (topic) DO NOTHING",
            )
            .bind(topic)
            .bind(pgvector::Vector::from(embedding.clone()))
            .execute(pool)
            .await?;

            cached.insert(topic.clone(), embedding.to_vec());
        }
    }

    Ok(cached)
}

// ============================================================================
// Entity scoring: frequency z-score (binary presence per day)
// ============================================================================

/// Score entities by binary presence frequency over the baseline.
fn score_entities_by_frequency(
    today_entities: &HashSet<String>,
    entity_days: &HashMap<String, HashSet<NaiveDate>>,
    total_baseline_days: usize,
) -> HashMap<String, f64> {
    let n = total_baseline_days as f64;
    let mut scores = HashMap::new();

    for entity in today_entities {
        let days_present = entity_days
            .get(entity)
            .map(|s| s.len())
            .unwrap_or(0) as f64;

        if n < MIN_BASELINE_DAYS as f64 || days_present == 0.0 {
            scores.insert(entity.clone(), Z_MAX);
            continue;
        }

        let mean = days_present / n;
        let std = (mean * (1.0 - mean)).sqrt();

        let z = if std < 1e-10 {
            -Z_MAX
        } else {
            (1.0 - mean) / std
        };

        scores.insert(entity.clone(), z.max(-Z_MAX).min(Z_MAX));
    }

    scores
}

// ============================================================================
// Helpers
// ============================================================================

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
        return 1.0;
    }
    1.0 - (dot / denom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_distance_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let d = cosine_distance(&a, &a);
        assert!(d.abs() < 1e-6, "Identical → 0, got {d}");
    }

    #[test]
    fn test_cosine_distance_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let d = cosine_distance(&a, &b);
        assert!((d - 1.0).abs() < 1e-6, "Orthogonal → 1, got {d}");
    }

    #[test]
    fn test_entity_scoring_common() {
        let mut today = HashSet::new();
        today.insert("maya".to_string());

        let mut days = HashMap::new();
        let mut maya_days = HashSet::new();
        for i in 0..60 {
            maya_days.insert(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() + chrono::Duration::days(i));
        }
        days.insert("maya".to_string(), maya_days);

        let scores = score_entities_by_frequency(&today, &days, 84);
        let z = scores["maya"];
        assert!(z > 0.0 && z < 1.5, "Daily entity → low positive z, got {z}");
    }

    #[test]
    fn test_entity_scoring_rare() {
        let mut today = HashSet::new();
        today.insert("rachel".to_string());

        let mut days = HashMap::new();
        let mut rare = HashSet::new();
        rare.insert(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        rare.insert(NaiveDate::from_ymd_opt(2026, 1, 2).unwrap());
        days.insert("rachel".to_string(), rare);

        let scores = score_entities_by_frequency(&today, &days, 84);
        let z = scores["rachel"];
        assert!((z - 3.0).abs() < 0.01, "Rare entity → clamped 3.0, got {z}");
    }

    #[test]
    fn test_entity_scoring_brand_new() {
        let mut today = HashSet::new();
        today.insert("new-person".to_string());
        let days: HashMap<String, HashSet<NaiveDate>> = HashMap::new();
        let scores = score_entities_by_frequency(&today, &days, 84);
        assert!((scores["new-person"] - 3.0).abs() < 0.01);
    }
}

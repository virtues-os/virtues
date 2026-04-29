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
use sqlx::SqlitePool;

use crate::search::embedder::get_embedder;

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
    pool: &SqlitePool,
    date: NaiveDate,
) -> anyhow::Result<u32> {
    let date_str = date.format("%Y-%m-%d").to_string();
    let baseline_start = (date - chrono::Duration::days(BASELINE_WINDOW_DAYS))
        .format("%Y-%m-%d")
        .to_string();

    // 1. Load today's events (id, topics JSON, entities JSON)
    let today_events: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        r#"
        SELECT e.id, e.topics, e.entities
        FROM wiki_events e
        JOIN wiki_days d ON e.day_id = d.id
        WHERE d.date = $1
          AND e.is_sleep = 0
          AND e.user_hidden = 0
        "#,
    )
    .bind(&date_str)
    .fetch_all(pool)
    .await?;

    if today_events.is_empty() {
        return Ok(0);
    }

    // 2. Collect all unique topics and entities appearing today
    let mut all_topics: HashSet<String> = HashSet::new();
    let mut all_entities: HashSet<String> = HashSet::new();

    for (_, topics_json, entities_json) in &today_events {
        if let Some(tj) = topics_json {
            if let Ok(topics) = serde_json::from_str::<Vec<String>>(tj) {
                all_topics.extend(topics);
            }
        }
        if let Some(ej) = entities_json {
            if let Ok(entities) = serde_json::from_str::<Vec<String>>(ej) {
                all_entities.extend(entities);
            }
        }
    }

    // 3. Load baseline data
    let baseline_rows: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        r#"
        SELECT d.date, e.topics, e.entities
        FROM wiki_events e
        JOIN wiki_days d ON e.day_id = d.id
        WHERE d.date >= $1
          AND d.date < $2
          AND e.is_sleep = 0
          AND e.user_hidden = 0
        "#,
    )
    .bind(&baseline_start)
    .bind(&date_str)
    .fetch_all(pool)
    .await?;

    // Count distinct baseline days
    let total_baseline_days: usize = {
        let mut dates: HashSet<&str> = HashSet::new();
        for (d, _, _) in &baseline_rows {
            dates.insert(d.as_str());
        }
        dates.len()
    };

    // 4a. TOPIC SCORING — embedding centroid distance
    let topic_scores = if !all_topics.is_empty() && total_baseline_days >= MIN_BASELINE_DAYS {
        score_topics_by_embedding(pool, &all_topics, &baseline_rows).await?
    } else {
        // Not enough baseline or no topics → max novelty for all
        all_topics.iter().map(|t| (t.clone(), Z_MAX)).collect()
    };

    // 4b. ENTITY SCORING — frequency z-score (unchanged)
    let entity_scores = {
        let mut entity_days: HashMap<String, HashSet<String>> = HashMap::new();
        for (day_date, _, entities_json) in &baseline_rows {
            if let Some(ej) = entities_json {
                if let Ok(entities) = serde_json::from_str::<Vec<String>>(ej) {
                    for e in entities {
                        entity_days
                            .entry(e)
                            .or_default()
                            .insert(day_date.clone());
                    }
                }
            }
        }
        score_entities_by_frequency(&all_entities, &entity_days, total_baseline_days)
    };

    // 5. Write back per-event
    let mut updated = 0u32;
    for (event_id, topics_json, entities_json) in &today_events {
        let event_topics: Vec<String> = topics_json
            .as_ref()
            .and_then(|tj| serde_json::from_str(tj).ok())
            .unwrap_or_default();

        let event_entities: Vec<String> = entities_json
            .as_ref()
            .and_then(|ej| serde_json::from_str(ej).ok())
            .unwrap_or_default();

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
            Some(serde_json::to_string(&event_topic_novelty).unwrap_or_default())
        };

        let entity_json = if event_entity_novelty.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&event_entity_novelty).unwrap_or_default())
        };

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
    pool: &SqlitePool,
    today_topics: &HashSet<String>,
    baseline_rows: &[(String, Option<String>, Option<String>)],
) -> anyhow::Result<HashMap<String, f64>> {
    let embedder = get_embedder().await?;

    // Collect all unique baseline topic strings
    let mut baseline_topics: HashSet<String> = HashSet::new();
    for (_, topics_json, _) in baseline_rows {
        if let Some(tj) = topics_json {
            if let Ok(topics) = serde_json::from_str::<Vec<String>>(tj) {
                baseline_topics.extend(topics);
            }
        }
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
        if let Some(tj) = topics_json {
            if let Ok(topics) = serde_json::from_str::<Vec<String>>(tj) {
                for t in &topics {
                    if let Some(emb) = embeddings.get(t.as_str()) {
                        baseline_embeddings.push(emb.as_slice());
                    }
                }
            }
        }
    }

    if baseline_embeddings.is_empty() {
        return Ok(today_topics.iter().map(|t| (t.clone(), Z_MAX)).collect());
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
    pool: &SqlitePool,
    embedder: &std::sync::Arc<crate::search::embedder::LocalEmbedder>,
    topics: &[String],
) -> anyhow::Result<HashMap<String, Vec<f32>>> {
    // Load existing from cache
    let mut cached: HashMap<String, Vec<f32>> = HashMap::new();
    let rows: Vec<(String, Vec<u8>)> = sqlx::query_as(
        "SELECT topic, embedding FROM search_topic_cache",
    )
    .fetch_all(pool)
    .await?;

    for (topic, blob) in rows {
        cached.insert(topic, bytes_to_embedding(&blob));
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
            let blob = embedding_to_bytes(embedding);
            sqlx::query(
                "INSERT OR IGNORE INTO search_topic_cache (topic, embedding) VALUES ($1, $2)",
            )
            .bind(topic)
            .bind(&blob)
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
    entity_days: &HashMap<String, HashSet<String>>,
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

fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
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
            maya_days.insert(format!("day-{i}"));
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
        rare.insert("day-1".to_string());
        rare.insert("day-2".to_string());
        days.insert("rachel".to_string(), rare);

        let scores = score_entities_by_frequency(&today, &days, 84);
        let z = scores["rachel"];
        assert!((z - 3.0).abs() < 0.01, "Rare entity → clamped 3.0, got {z}");
    }

    #[test]
    fn test_entity_scoring_brand_new() {
        let mut today = HashSet::new();
        today.insert("new-person".to_string());
        let days: HashMap<String, HashSet<String>> = HashMap::new();
        let scores = score_entities_by_frequency(&today, &days, 84);
        assert!((scores["new-person"] - 3.0).abs() < 0.01);
    }
}

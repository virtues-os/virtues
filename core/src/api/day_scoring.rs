//! W6H Embeddings and Chaos/Order Scoring
//!
//! Computes a cross-day chaos score by aggregating per-event embeddings
//! (already computed in `day_summary::store_structured_events`) into a
//! duration-weighted day centroid, then comparing to a 30-day
//! exponentially-decayed rolling centroid.
//!
//! Also provides: ontology text extraction, W6H Shannon entropy,
//! cosine similarity, and embedding storage utilities.

use chrono::NaiveDate;
use sqlx::SqlitePool;

use crate::error::{Error, Result};
use virtues_registry::ontologies::registered_ontologies;

const EMBEDDING_MODEL: &str = "nomic-embed-text-v1.5";
const MAX_DIM_TEXT_CHARS: usize = 2000;
const CENTROID_WINDOW_DAYS: i64 = 30;
const DECAY_RATE: f64 = 0.1;
/// Minimum context weight for an ontology to contribute to a W6H dimension
const W6H_WEIGHT_THRESHOLD: f32 = 0.2;

/// The 7 W6H dimensions: who, whom, what, when, where, why, how
pub const W6H_DIMENSIONS: &[&str] = &["who", "whom", "what", "when", "where", "why", "how"];

// ── Public API ───────────────────────────────────────────────────────────────

/// Result of chaos/entropy scoring for a day.
#[derive(Debug)]
pub struct ChaosScoreResult {
    /// The chaos score (0.0 = ordered, 1.0 = chaotic). None if no centroids existed.
    pub score: Option<f64>,
    /// How many distinct prior days contributed centroid data (0 = baseline day).
    pub calibration_days: i32,
}

/// Compute the cross-day chaos score from pre-computed event embeddings.
///
/// Aggregates event embeddings into a duration-weighted day centroid, stores it,
/// and compares against the 30-day rolling centroid.
///
/// `event_embeddings` is `(embedding, duration_minutes)` from `store_structured_events()`.
pub async fn generate_embeddings_and_score(
    pool: &SqlitePool,
    date: NaiveDate,
    _context_vector: &[f32; 7],
    event_embeddings: &[(Vec<f32>, f32)],
) -> Result<ChaosScoreResult> {
    let date_str = date.to_string();
    let calibration_days = count_calibration_days(pool, &date_str).await;

    if event_embeddings.is_empty() {
        tracing::debug!(date = %date, "No event embeddings for chaos scoring");
        return Ok(ChaosScoreResult { score: None, calibration_days });
    }

    // 1. Compute duration-weighted centroid of event embeddings
    let dim = event_embeddings[0].0.len();
    let mut weighted_sum = vec![0.0f64; dim];
    let mut total_weight = 0.0f64;

    for (emb, duration) in event_embeddings {
        let w = *duration as f64;
        total_weight += w;
        for (i, &val) in emb.iter().enumerate() {
            if i < dim {
                weighted_sum[i] += val as f64 * w;
            }
        }
    }

    let centroid: Vec<f32> = weighted_sum.iter().map(|v| (v / total_weight) as f32).collect();

    // Normalize to unit length
    let norm: f32 = centroid.iter().map(|v| v * v).sum::<f32>().sqrt();
    let day_centroid = if norm > 0.0 {
        centroid.iter().map(|v| v / norm).collect::<Vec<f32>>()
    } else {
        return Ok(ChaosScoreResult { score: None, calibration_days });
    };

    // 2. Store combined centroid in wiki_day_embeddings
    let text_hash = format!("{}_events_{}", date_str, event_embeddings.len());
    store_embedding(pool, &date_str, "combined", &day_centroid, &text_hash).await?;

    // 3. Compare to 30-day rolling centroid
    let rolling_centroid = compute_dimension_centroid(pool, "combined", &date_str, CENTROID_WINDOW_DAYS).await?;

    let chaos = match rolling_centroid {
        Some(ref rc) => {
            let score = (1.0 - cosine_similarity(&day_centroid, rc) as f64).clamp(0.0, 1.0);
            tracing::info!(
                date = %date,
                chaos_score = score,
                calibration_days = calibration_days,
                event_count = event_embeddings.len(),
                "Chaos score computed (event centroid)"
            );
            Some(score)
        }
        None => {
            tracing::info!(date = %date, "No rolling centroid yet (baseline day)");
            None
        }
    };

    Ok(ChaosScoreResult { score: chaos, calibration_days })
}

/// Count distinct prior days that have embeddings (within the centroid window).
async fn count_calibration_days(pool: &SqlitePool, before_date: &str) -> i32 {
    let before = NaiveDate::parse_from_str(before_date, "%Y-%m-%d").unwrap_or_default();
    let window_start = before - chrono::Duration::days(CENTROID_WINDOW_DAYS);

    sqlx::query_scalar::<_, i32>(
        "SELECT COUNT(DISTINCT day_date) FROM wiki_day_embeddings \
         WHERE day_date >= $1 AND day_date < $2",
    )
    .bind(window_start.to_string())
    .bind(before_date)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
}

// ── Per-ontology text extraction ─────────────────────────────────────────────

/// A text blob extracted from a single ontology's data for a day.
pub(crate) struct OntologyText {
    #[allow(dead_code)]
    pub(crate) ontology_name: String,
    pub(crate) text: String,
    /// The 7 W6H context weights for this ontology
    pub(crate) context_weights: [f32; 7],
}

/// Collect text from all ontologies that have data for the given time window.
/// `date_str` is YYYY-MM-DD, used for app_* tables which store local timestamps.
pub(crate) async fn collect_ontology_texts(
    pool: &SqlitePool,
    start: &str,
    end: &str,
    date_str: &str,
) -> Vec<OntologyText> {
    let ontologies = registered_ontologies();
    let mut results = Vec::new();

    for ont in &ontologies {
        // Skip ontologies with all-zero weights (e.g. financial_account)
        if ont.context_weights.iter().all(|&w| w < W6H_WEIGHT_THRESHOLD) {
            continue;
        }

        let text = match ont.name {
            "communication_message" => extract_message_text(pool, start, end).await,
            "communication_email" => extract_email_text(pool, start, end).await,
            "communication_transcription" => extract_transcription_text(pool, start, end).await,
            "calendar_event" => extract_calendar_text(pool, start, end).await,
            "health_workout" => extract_workout_text(pool, start, end).await,
            "health_sleep" => extract_sleep_text(pool, start, end).await,
            "health_heart_rate" => extract_heart_rate_text(pool, start, end).await,
            "health_steps" => extract_steps_text(pool, start, end).await,
            "location_visit" => extract_location_text(pool, start, end).await,
            "financial_transaction" => extract_financial_text(pool, start, end).await,
            "activity_app_usage" => extract_app_usage_text(pool, start, end).await,
            "activity_web_browsing" => extract_web_browsing_text(pool, start, end).await,
            "content_document" => extract_document_text(pool, start, end).await,
            "content_conversation" => extract_conversation_text(pool, start, end).await,
            "content_bookmark" => extract_bookmark_text(pool, start, end).await,
            "app_chat" => extract_chat_text(pool, date_str).await,
            "app_page" => extract_page_text(pool, date_str).await,
            _ => None,
        };

        if let Some(t) = text {
            if !t.trim().is_empty() {
                results.push(OntologyText {
                    ontology_name: ont.name.to_string(),
                    text: t,
                    context_weights: ont.context_weights,
                });
            }
        }
    }

    results
}

/// Build the text blob for a single W6H dimension by collecting from all ontologies
/// whose weight for that dimension exceeds the threshold.
/// (Legacy: was used by per-dimension embedding approach. Kept for diagnostics.)
#[allow(dead_code)]
fn build_w6h_dimension_text(
    ontology_texts: &[OntologyText],
    dim_index: usize,
) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();

    for ot in ontology_texts {
        if ot.context_weights[dim_index] >= W6H_WEIGHT_THRESHOLD {
            parts.push(&ot.text);
        }
    }

    if parts.is_empty() {
        return None;
    }

    let mut combined = parts.join("\n");
    if combined.len() > MAX_DIM_TEXT_CHARS {
        // Find a char boundary
        let mut end = MAX_DIM_TEXT_CHARS;
        while !combined.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        combined.truncate(end);
    }

    Some(combined)
}

// ── Individual ontology text extractors ──────────────────────────────────────

async fn extract_message_text(pool: &SqlitePool, start: &str, end: &str) -> Option<String> {
    use sqlx::Row;
    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT from_name, body FROM data_communication_message \
         WHERE timestamp >= $1 AND timestamp <= $2 ORDER BY timestamp LIMIT 30",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    if rows.is_empty() { return None; }

    let parts: Vec<String> = rows.iter().map(|row| {
        let from: String = row.try_get("from_name").ok().flatten().unwrap_or_default();
        let body: String = row.try_get("body").ok().flatten().unwrap_or_default();
        format!("{}: {}", from, truncate_str(&body, 200))
    }).collect();

    Some(parts.join("\n"))
}

async fn extract_email_text(pool: &SqlitePool, start: &str, end: &str) -> Option<String> {
    use sqlx::Row;
    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT from_name, subject, snippet FROM data_communication_email \
         WHERE timestamp >= $1 AND timestamp <= $2 ORDER BY timestamp LIMIT 20",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    if rows.is_empty() { return None; }

    let parts: Vec<String> = rows.iter().map(|row| {
        let from: String = row.try_get("from_name").ok().flatten().unwrap_or_default();
        let subject: String = row.try_get("subject").ok().flatten().unwrap_or_default();
        let snippet: String = row.try_get("snippet").ok().flatten().unwrap_or_default();
        format!("Email from {}: {} - {}", from, subject, truncate_str(&snippet, 150))
    }).collect();

    Some(parts.join("\n"))
}

async fn extract_transcription_text(pool: &SqlitePool, start: &str, end: &str) -> Option<String> {
    use sqlx::Row;
    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT text FROM data_communication_transcription \
         WHERE start_time >= $1 AND start_time <= $2 ORDER BY start_time LIMIT 10",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    if rows.is_empty() { return None; }

    let parts: Vec<String> = rows.iter().filter_map(|row| {
        row.try_get::<Option<String>, _>("text").ok().flatten()
            .map(|text| format!("Transcription: {}", truncate_str(&text, 500)))
    }).collect();

    if parts.is_empty() { None } else { Some(parts.join("\n")) }
}

async fn extract_calendar_text(pool: &SqlitePool, start: &str, end: &str) -> Option<String> {
    use sqlx::Row;
    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT title, description, location_name FROM data_calendar_event \
         WHERE start_time >= $1 AND start_time <= $2 ORDER BY start_time LIMIT 20",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    if rows.is_empty() { return None; }

    let parts: Vec<String> = rows.iter().map(|row| {
        let title: String = row.try_get("title").ok().flatten().unwrap_or_default();
        let desc: String = row.try_get("description").ok().flatten().unwrap_or_default();
        let loc: String = row.try_get("location_name").ok().flatten().unwrap_or_default();
        let mut s = title;
        if !desc.is_empty() { s.push_str(&format!(": {}", truncate_str(&desc, 100))); }
        if !loc.is_empty() { s.push_str(&format!(" at {}", loc)); }
        s
    }).collect();

    Some(parts.join("\n"))
}

async fn extract_workout_text(pool: &SqlitePool, start: &str, end: &str) -> Option<String> {
    use sqlx::Row;
    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT workout_type, duration_minutes, calories_burned \
         FROM data_health_workout WHERE start_time >= $1 AND start_time <= $2",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    if rows.is_empty() { return None; }

    let parts: Vec<String> = rows.iter().map(|row| {
        let wtype: String = row.try_get("workout_type").ok().flatten().unwrap_or_default();
        let dur: Option<f64> = row.try_get("duration_minutes").ok().flatten();
        let cal: Option<f64> = row.try_get("calories_burned").ok().flatten();
        let mut s = format!("Workout: {}", wtype);
        if let Some(d) = dur { s.push_str(&format!(" {:.0}min", d)); }
        if let Some(c) = cal { s.push_str(&format!(" {:.0}kcal", c)); }
        s
    }).collect();

    Some(parts.join("\n"))
}

async fn extract_sleep_text(pool: &SqlitePool, start: &str, end: &str) -> Option<String> {
    use sqlx::Row;
    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT duration_minutes, sleep_quality_score FROM data_health_sleep \
         WHERE start_time >= $1 AND start_time <= $2",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    if rows.is_empty() { return None; }

    let parts: Vec<String> = rows.iter().map(|row| {
        let mins: Option<i64> = row.try_get("duration_minutes").ok().flatten();
        let quality: Option<f64> = row.try_get("sleep_quality_score").ok().flatten();
        let mut s = "Sleep:".to_string();
        if let Some(m) = mins { s.push_str(&format!(" {}h{}m", m / 60, m % 60)); }
        if let Some(q) = quality { s.push_str(&format!(" quality={:.1}", q)); }
        s
    }).collect();

    Some(parts.join("\n"))
}

async fn extract_heart_rate_text(pool: &SqlitePool, start: &str, end: &str) -> Option<String> {
    let hr: Option<(Option<f64>, Option<i32>, Option<i32>)> = sqlx::query_as(
        "SELECT ROUND(AVG(bpm)), MIN(bpm), MAX(bpm) FROM data_health_heart_rate \
         WHERE timestamp >= $1 AND timestamp <= $2",
    )
    .bind(start)
    .bind(end)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    match hr {
        Some((Some(avg), Some(min), Some(max))) if avg > 0.0 => {
            Some(format!("Heart rate: avg {:.0} bpm (range {}-{})", avg, min, max))
        }
        _ => None,
    }
}

async fn extract_steps_text(pool: &SqlitePool, start: &str, end: &str) -> Option<String> {
    let steps: Option<(Option<i64>,)> = sqlx::query_as(
        "SELECT SUM(step_count) FROM data_health_steps \
         WHERE timestamp >= $1 AND timestamp <= $2",
    )
    .bind(start)
    .bind(end)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    match steps {
        Some((Some(total),)) if total > 0 => Some(format!("Steps: {}", total)),
        _ => None,
    }
}

async fn extract_location_text(pool: &SqlitePool, start: &str, end: &str) -> Option<String> {
    use sqlx::Row;
    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT place_name, duration_minutes \
         FROM data_location_visit WHERE arrival_time >= $1 AND arrival_time <= $2 \
         ORDER BY arrival_time LIMIT 20",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    if rows.is_empty() { return None; }

    let parts: Vec<String> = rows.iter().map(|row| {
        let name: String = row.try_get("place_name").ok().flatten().unwrap_or_default();
        let dur: Option<f64> = row.try_get("duration_minutes").ok().flatten();
        let mut s = name;
        if let Some(d) = dur { s.push_str(&format!(" ({:.0}min)", d)); }
        s
    }).collect();

    Some(parts.join("\n"))
}

async fn extract_financial_text(pool: &SqlitePool, start: &str, end: &str) -> Option<String> {
    use sqlx::Row;
    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT merchant_name, amount, category FROM data_financial_transaction \
         WHERE timestamp >= $1 AND timestamp <= $2 ORDER BY timestamp LIMIT 30",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    if rows.is_empty() { return None; }

    let parts: Vec<String> = rows.iter().map(|row| {
        let merchant: String = row.try_get("merchant_name").ok().flatten().unwrap_or_default();
        let amount_cents: Option<i64> = row.try_get("amount").ok().flatten();
        let category_json: String = row.try_get("category").ok().flatten().unwrap_or_default();
        let category = serde_json::from_str::<Vec<String>>(&category_json)
            .map(|cats| cats.join(", "))
            .unwrap_or(category_json);
        match amount_cents {
            Some(c) => format!("{} ${:.2} ({})", merchant, c as f64 / 100.0, category),
            None => format!("{} ({})", merchant, category),
        }
    }).collect();

    Some(parts.join("\n"))
}

async fn extract_app_usage_text(pool: &SqlitePool, start: &str, end: &str) -> Option<String> {
    use sqlx::Row;
    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT app_name, COUNT(*) as sessions, \
         CAST(SUM((julianday(end_time) - julianday(start_time)) * 86400) AS INTEGER) as secs \
         FROM data_activity_app_usage WHERE start_time >= $1 AND start_time <= $2 \
         AND app_name IS NOT NULL GROUP BY app_name ORDER BY secs DESC LIMIT 10",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    if rows.is_empty() { return None; }

    let parts: Vec<String> = rows.iter().filter_map(|row| {
        let name: String = row.try_get("app_name").ok().flatten().unwrap_or_default();
        let secs: i64 = row.try_get("secs").ok().unwrap_or(0);
        if secs > 60 { Some(format!("App: {} ({}min)", name, secs / 60)) } else { None }
    }).collect();

    if parts.is_empty() { None } else { Some(parts.join("\n")) }
}

async fn extract_web_browsing_text(pool: &SqlitePool, start: &str, end: &str) -> Option<String> {
    use sqlx::Row;
    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT page_title, url FROM data_activity_web_browsing \
         WHERE timestamp >= $1 AND timestamp <= $2 \
         AND page_title IS NOT NULL ORDER BY visit_duration_seconds DESC LIMIT 10",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    if rows.is_empty() { return None; }

    let parts: Vec<String> = rows.iter().map(|row| {
        let title: String = row.try_get("page_title").ok().flatten().unwrap_or_default();
        format!("Web: {}", truncate_str(&title, 100))
    }).collect();

    Some(parts.join("\n"))
}

async fn extract_document_text(pool: &SqlitePool, start: &str, end: &str) -> Option<String> {
    use sqlx::Row;
    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT title, document_type FROM data_content_document \
         WHERE created_time >= $1 AND created_time <= $2 AND title IS NOT NULL LIMIT 10",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    if rows.is_empty() { return None; }

    let parts: Vec<String> = rows.iter().map(|row| {
        let title: String = row.try_get("title").ok().flatten().unwrap_or_default();
        let dtype: String = row.try_get("document_type").ok().flatten().unwrap_or_default();
        format!("Doc [{}]: {}", dtype, title)
    }).collect();

    Some(parts.join("\n"))
}

async fn extract_conversation_text(pool: &SqlitePool, start: &str, end: &str) -> Option<String> {
    use sqlx::Row;
    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT model, MIN(CASE WHEN role = 'user' THEN content END) as first_msg \
         FROM data_content_conversation \
         WHERE timestamp >= $1 AND timestamp <= $2 \
         GROUP BY conversation_id ORDER BY MIN(timestamp) LIMIT 10",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    if rows.is_empty() { return None; }

    let parts: Vec<String> = rows.iter().map(|row| {
        let model: String = row.try_get("model").ok().flatten().unwrap_or_default();
        let msg: String = row.try_get("first_msg").ok().flatten()
            .unwrap_or_else(|| "(conversation)".to_string());
        format!("AI ({}): {}", model, truncate_str(&msg, 150))
    }).collect();

    Some(parts.join("\n"))
}

async fn extract_bookmark_text(pool: &SqlitePool, start: &str, end: &str) -> Option<String> {
    use sqlx::Row;
    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT title, description, url FROM data_content_bookmark \
         WHERE timestamp >= $1 AND timestamp <= $2 AND title IS NOT NULL LIMIT 10",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    if rows.is_empty() { return None; }

    let parts: Vec<String> = rows.iter().map(|row| {
        let title: String = row.try_get("title").ok().flatten().unwrap_or_default();
        let desc: String = row.try_get("description").ok().flatten().unwrap_or_default();
        if desc.is_empty() { format!("Bookmark: {}", title) }
        else { format!("Bookmark: {} - {}", title, truncate_str(&desc, 100)) }
    }).collect();

    Some(parts.join("\n"))
}

async fn extract_chat_text(pool: &SqlitePool, date_str: &str) -> Option<String> {
    use sqlx::Row;
    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT title, conversation_summary FROM app_chats \
         WHERE date(created_at) = $1 ORDER BY created_at LIMIT 20",
    )
    .bind(date_str)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    if rows.is_empty() { return None; }

    let parts: Vec<String> = rows.iter().map(|row| {
        let title: String = row.try_get("title").ok().flatten().unwrap_or_default();
        let summary: Option<String> = row.try_get("conversation_summary").ok().flatten();
        match summary {
            Some(s) if !s.is_empty() => format!("Chat: {} — {}", title, truncate_str(&s, 300)),
            _ => format!("Chat: {}", title),
        }
    }).collect();

    Some(parts.join("\n"))
}

async fn extract_page_text(pool: &SqlitePool, date_str: &str) -> Option<String> {
    use sqlx::Row;
    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT title, content FROM app_pages \
         WHERE date(updated_at) = $1 AND title != 'Untitled' \
         ORDER BY updated_at LIMIT 10",
    )
    .bind(date_str)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    if rows.is_empty() { return None; }

    let parts: Vec<String> = rows.iter().map(|row| {
        let title: String = row.try_get("title").ok().flatten().unwrap_or_default();
        let content: String = row.try_get("content").ok().flatten().unwrap_or_default();
        if content.is_empty() {
            format!("Page: {}", title)
        } else {
            format!("Page: {} — {}", title, truncate_str(&content, 300))
        }
    }).collect();

    Some(parts.join("\n"))
}

// ── Embedding storage ────────────────────────────────────────────────────────

/// Store a W6H embedding in the database.
async fn store_embedding(
    pool: &SqlitePool,
    day_date: &str,
    dimension: &str,
    embedding: &[f32],
    text_hash: &str,
) -> Result<()> {
    let id = format!("{}_{}", day_date, dimension);
    let embedding_bytes = embedding_to_bytes(embedding);

    sqlx::query(
        "INSERT INTO wiki_day_embeddings (id, day_date, dimension, embedding, text_hash, model, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, datetime('now')) \
         ON CONFLICT(day_date, dimension) DO UPDATE SET \
         embedding = excluded.embedding, text_hash = excluded.text_hash, \
         model = excluded.model, created_at = datetime('now')",
    )
    .bind(&id)
    .bind(day_date)
    .bind(dimension)
    .bind(&embedding_bytes)
    .bind(text_hash)
    .bind(EMBEDDING_MODEL)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to store embedding: {e}")))?;

    Ok(())
}

/// Get a stored embedding (text_hash, embedding vector) for a dimension on a date.
pub async fn get_stored_embedding(
    pool: &SqlitePool,
    day_date: &str,
    dimension: &str,
) -> Result<Option<(String, Vec<f32>)>> {
    use sqlx::Row;

    let row: Option<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT text_hash, embedding FROM wiki_day_embeddings \
         WHERE day_date = $1 AND dimension = $2",
    )
    .bind(day_date)
    .bind(dimension)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch embedding: {e}")))?;

    match row {
        Some(row) => {
            let hash: String = row.try_get("text_hash").map_err(|e| {
                Error::Database(format!("Failed to read text_hash: {e}"))
            })?;
            let bytes: Vec<u8> = row.try_get("embedding").map_err(|e| {
                Error::Database(format!("Failed to read embedding: {e}"))
            })?;
            Ok(Some((hash, bytes_to_embedding(&bytes))))
        }
        None => Ok(None),
    }
}

/// Get all stored W6H embeddings for a day.
pub async fn get_day_embeddings(
    pool: &SqlitePool,
    day_date: &str,
) -> Result<Vec<(String, Vec<f32>)>> {
    use sqlx::Row;

    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT dimension, embedding FROM wiki_day_embeddings \
         WHERE day_date = $1 ORDER BY dimension",
    )
    .bind(day_date)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch day embeddings: {e}")))?;

    let mut results = Vec::new();
    for row in &rows {
        let dim: String = row.try_get("dimension").map_err(|e| {
            Error::Database(format!("Failed to read dimension: {e}"))
        })?;
        let bytes: Vec<u8> = row.try_get("embedding").map_err(|e| {
            Error::Database(format!("Failed to read embedding: {e}"))
        })?;
        results.push((dim, bytes_to_embedding(&bytes)));
    }

    Ok(results)
}

// ── Centroid computation ─────────────────────────────────────────────────────

/// Compute an exponentially-decayed centroid for a W6H dimension over the past N days.
/// Returns None if no historical embeddings exist.
async fn compute_dimension_centroid(
    pool: &SqlitePool,
    dimension: &str,
    before_date: &str,
    window_days: i64,
) -> Result<Option<Vec<f32>>> {
    use sqlx::Row;

    let before = NaiveDate::parse_from_str(before_date, "%Y-%m-%d")
        .map_err(|e| Error::InvalidInput(format!("Invalid date: {e}")))?;
    let window_start = before - chrono::Duration::days(window_days);

    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT day_date, embedding FROM wiki_day_embeddings \
         WHERE dimension = $1 AND day_date >= $2 AND day_date < $3 \
         ORDER BY day_date DESC",
    )
    .bind(dimension)
    .bind(window_start.to_string())
    .bind(before_date)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch centroid data: {e}")))?;

    if rows.is_empty() {
        return Ok(None);
    }

    let mut weighted_sum: Option<Vec<f64>> = None;
    let mut total_weight: f64 = 0.0;

    for row in &rows {
        let date_str: String = row.try_get("day_date").map_err(|e| {
            Error::Database(format!("Failed to read day_date: {e}"))
        })?;
        let bytes: Vec<u8> = row.try_get("embedding").map_err(|e| {
            Error::Database(format!("Failed to read embedding: {e}"))
        })?;

        let embedding = bytes_to_embedding(&bytes);
        let date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d").unwrap_or(before);
        let days_ago = (before - date).num_days() as f64;
        let weight = (-DECAY_RATE * days_ago).exp();

        total_weight += weight;

        match &mut weighted_sum {
            Some(sum) => {
                for (i, val) in embedding.iter().enumerate() {
                    if i < sum.len() {
                        sum[i] += (*val as f64) * weight;
                    }
                }
            }
            None => {
                weighted_sum =
                    Some(embedding.iter().map(|v| (*v as f64) * weight).collect());
            }
        }
    }

    let sum = weighted_sum.unwrap();
    let centroid: Vec<f32> = sum.iter().map(|v| (v / total_weight) as f32).collect();

    // Normalize to unit length
    let norm: f32 = centroid.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        Ok(Some(centroid.iter().map(|v| v / norm).collect()))
    } else {
        Ok(None)
    }
}

// ── Chaos score computation ──────────────────────────────────────────────────

/// Compute the chaos score from W6H embeddings and context coverage.
///
/// For each W6H dimension:
///   1. Cosine similarity to 30-day centroid
///   2. dim_chaos = 1 - similarity
///
/// Final: chaos = sum(chaos[dim] * coverage[dim]) / sum(coverage[dim])
/// (Legacy: replaced by event centroid approach in generate_embeddings_and_score.)
#[allow(dead_code)]
async fn compute_chaos_score(
    pool: &SqlitePool,
    date_str: &str,
    w6h_embeddings: &[(String, Vec<f32>)],
    context_vector: &[f32; 7],
) -> Result<f64> {
    let mut numerator = 0.0f64;
    let mut denominator = 0.0f64;

    for (dim_name, embedding) in w6h_embeddings {
        let dim_idx = W6H_DIMENSIONS.iter().position(|&d| d == dim_name);
        let dim_idx = match dim_idx {
            Some(i) => i,
            None => continue,
        };

        let centroid = compute_dimension_centroid(pool, dim_name, date_str, CENTROID_WINDOW_DAYS).await?;

        let dim_chaos = match centroid {
            Some(ref c) => {
                (1.0 - cosine_similarity(embedding, c) as f64).clamp(0.0, 1.0)
            }
            None => continue,
        };

        let coverage = context_vector[dim_idx] as f64;
        numerator += dim_chaos * coverage;
        denominator += coverage;
    }

    Ok(if denominator > 0.0 {
        (numerator / denominator).clamp(0.0, 1.0)
    } else {
        0.0
    })
}

// ── Utility functions ────────────────────────────────────────────────────────

/// Cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for i in 0..len {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom > 0.0 { dot / denom } else { 0.0 }
}

/// Convert f32 embedding to bytes for BLOB storage.
pub fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Convert bytes from BLOB back to f32 embedding.
pub fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

/// Compute a simple hash for text content to detect changes.
#[allow(dead_code)]
fn compute_text_hash(text: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Max Shannon entropy for 7 dimensions: log₂(7) ≈ 2.807
const MAX_SHANNON_7: f64 = 2.807354922057604;

/// Compute normalized Shannon entropy of a W6H activation vector.
///
/// Measures internal complexity/richness of a moment — how spread the activation
/// is across the 7 experiential dimensions. Returns 0–1 (0 = concentrated in
/// one dimension, 1 = perfectly uniform across all 7).
pub fn compute_w6h_entropy(w6h: &[f32; 7]) -> f64 {
    let sum: f32 = w6h.iter().sum();
    if sum <= 0.0 {
        return 0.0;
    }
    let mut h = 0.0f64;
    for &val in w6h {
        let p = val as f64 / sum as f64;
        if p > 0.0 {
            h -= p * p.log2();
        }
    }
    (h / MAX_SHANNON_7).clamp(0.0, 1.0)
}

/// Truncate a string to max chars, returning a reference.
fn truncate_str(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        let mut end = max;
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        &s[..end]
    }
}

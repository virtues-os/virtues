//! Domain Embeddings and Chaos/Order Scoring
//!
//! Computes a chaos/order score for a given day by:
//! 1. Aggregating the day's data into per-domain text blobs
//! 2. Embedding each domain via Tollbooth /v1/embeddings
//! 3. Comparing each embedding to a 30-day exponentially-decayed centroid
//! 4. Distributing domain chaos across 7 W5H dimensions via ontology weights
//! 5. Normalizing by that day's context coverage so sparse days aren't artificially chaotic
//!
//! Formula: chaos = sum(chaos[dim] * coverage[dim]) / sum(coverage[dim])
//! Where chaos[dim] = 1 - weighted_similarity[dim]

use chrono::NaiveDate;
use sqlx::SqlitePool;

use crate::error::{Error, Result};
use virtues_registry::ontologies::get_ontologies_by_domain;

const EMBEDDING_MODEL: &str = "text-embedding-3-small";
const MAX_DOMAIN_TEXT_CHARS: usize = 2000;
const CENTROID_WINDOW_DAYS: i64 = 30;
const DECAY_RATE: f64 = 0.1;

// Domain names matching ontology registry domains
const DOMAINS: &[&str] = &[
    "communication",
    "calendar",
    "health",
    "location",
    "financial",
    "activity",
    "content",
];

// ── Public API ───────────────────────────────────────────────────────────────

/// Result of chaos/entropy scoring for a day.
#[derive(Debug)]
pub struct ChaosScoreResult {
    /// The chaos score (0.0 = ordered, 1.0 = chaotic). None if no centroids existed.
    pub score: Option<f64>,
    /// How many distinct prior days contributed centroid data (0 = baseline day).
    pub calibration_days: i32,
}

/// Generate domain embeddings for a day and compute the chaos score.
/// Returns calibration info even when score can't be computed (baseline).
pub async fn generate_embeddings_and_score(
    pool: &SqlitePool,
    date: NaiveDate,
    context_vector: &[f32; 7],
) -> Result<ChaosScoreResult> {
    let timezone = super::profile::get_timezone(pool).await.unwrap_or(None);
    let (start_str, end_str) = super::day_summary::day_boundaries_utc(date, timezone.as_deref());
    let date_str = date.to_string(); // YYYY-MM-DD

    // Count distinct prior days with embeddings (calibration depth)
    let calibration_days = count_calibration_days(pool, &date_str).await;

    // 1. Build per-domain text blobs
    let domain_texts = build_domain_texts(pool, &start_str, &end_str).await;

    if domain_texts.is_empty() {
        tracing::debug!(date = %date, "No domain texts for chaos scoring");
        return Ok(ChaosScoreResult { score: None, calibration_days });
    }

    // 2. Embed each domain and store
    let mut domain_embeddings: Vec<(String, Vec<f32>)> = Vec::new();

    for (domain, text) in &domain_texts {
        let text_hash = compute_text_hash(text);

        // Check if we already have this exact embedding
        if let Some(existing) = get_stored_embedding(pool, &date_str, domain).await? {
            if existing.0 == text_hash {
                domain_embeddings.push((domain.clone(), existing.1));
                continue;
            }
        }

        // Embed via Tollbooth
        match embed_text(pool, text).await {
            Ok(embedding) => {
                store_domain_embedding(pool, &date_str, domain, &embedding, &text_hash).await?;
                domain_embeddings.push((domain.clone(), embedding));
            }
            Err(e) => {
                tracing::warn!(domain = %domain, error = %e, "Failed to embed domain text, skipping");
            }
        }
    }

    if domain_embeddings.is_empty() {
        return Ok(ChaosScoreResult { score: None, calibration_days });
    }

    // 3. Compute chaos score
    let chaos = compute_chaos_score(pool, &date_str, &domain_embeddings, context_vector).await?;

    tracing::info!(
        date = %date,
        chaos_score = chaos,
        calibration_days = calibration_days,
        domains_embedded = domain_embeddings.len(),
        "Chaos score computed"
    );

    Ok(ChaosScoreResult { score: Some(chaos), calibration_days })
}

/// Count distinct prior days that have domain embeddings (within the centroid window).
async fn count_calibration_days(pool: &SqlitePool, before_date: &str) -> i32 {
    let before = NaiveDate::parse_from_str(before_date, "%Y-%m-%d").unwrap_or_default();
    let window_start = before - chrono::Duration::days(CENTROID_WINDOW_DAYS);

    sqlx::query_scalar::<_, i32>(
        "SELECT COUNT(DISTINCT day_date) FROM wiki_day_domain_embeddings \
         WHERE day_date >= $1 AND day_date < $2",
    )
    .bind(window_start.to_string())
    .bind(before_date)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
}

// ── Domain text aggregation ──────────────────────────────────────────────────

/// Build per-domain text blobs from the day's data.
/// Each domain's text is truncated to MAX_DOMAIN_TEXT_CHARS.
async fn build_domain_texts(
    pool: &SqlitePool,
    start_str: &str,
    end_str: &str,
) -> Vec<(String, String)> {
    let mut texts = Vec::new();

    for domain in DOMAINS {
        let text = match *domain {
            "communication" => build_communication_text(pool, start_str, end_str).await,
            "calendar" => build_calendar_text(pool, start_str, end_str).await,
            "health" => build_health_text(pool, start_str, end_str).await,
            "location" => build_location_text(pool, start_str, end_str).await,
            "financial" => build_financial_text(pool, start_str, end_str).await,
            "activity" => build_activity_text(pool, start_str, end_str).await,
            "content" => build_content_text(pool, start_str, end_str).await,
            _ => None,
        };

        if let Some(mut t) = text {
            if !t.trim().is_empty() {
                // Truncate to max chars
                if t.len() > MAX_DOMAIN_TEXT_CHARS {
                    t.truncate(MAX_DOMAIN_TEXT_CHARS);
                }
                texts.push((domain.to_string(), t));
            }
        }
    }

    texts
}

async fn build_communication_text(pool: &SqlitePool, start: &str, end: &str) -> Option<String> {
    use sqlx::Row;

    // Messages
    let msgs: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT from_name, body FROM data_communication_message \
         WHERE timestamp >= $1 AND timestamp <= $2 ORDER BY timestamp LIMIT 30",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    // Emails
    let emails: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT from_name, subject, snippet FROM data_communication_email \
         WHERE timestamp >= $1 AND timestamp <= $2 ORDER BY timestamp LIMIT 20",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    // Transcriptions (absorbed from former speech domain)
    let transcriptions: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT text FROM data_communication_transcription \
         WHERE start_time >= $1 AND start_time <= $2 ORDER BY start_time LIMIT 10",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    if msgs.is_empty() && emails.is_empty() && transcriptions.is_empty() {
        return None;
    }

    let mut parts = Vec::new();
    for row in &msgs {
        let from: String = row.try_get("from_name").ok().flatten().unwrap_or_default();
        let body: String = row.try_get("body").ok().flatten().unwrap_or_default();
        parts.push(format!("{}: {}", from, truncate_str(&body, 200)));
    }
    for row in &emails {
        let from: String = row.try_get("from_name").ok().flatten().unwrap_or_default();
        let subject: String = row.try_get("subject").ok().flatten().unwrap_or_default();
        let snippet: String = row.try_get("snippet").ok().flatten().unwrap_or_default();
        parts.push(format!("Email from {}: {} - {}", from, subject, truncate_str(&snippet, 150)));
    }
    for row in &transcriptions {
        if let Ok(Some(text)) = row.try_get::<Option<String>, _>("text") {
            parts.push(format!("Transcription: {}", truncate_str(&text, 500)));
        }
    }

    Some(parts.join("\n"))
}

async fn build_calendar_text(pool: &SqlitePool, start: &str, end: &str) -> Option<String> {
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

    if rows.is_empty() {
        return None;
    }

    let parts: Vec<String> = rows
        .iter()
        .map(|row| {
            let title: String = row.try_get("title").ok().flatten().unwrap_or_default();
            let desc: String = row.try_get("description").ok().flatten().unwrap_or_default();
            let loc: String = row.try_get("location_name").ok().flatten().unwrap_or_default();
            let mut s = title;
            if !desc.is_empty() {
                s.push_str(&format!(": {}", truncate_str(&desc, 100)));
            }
            if !loc.is_empty() {
                s.push_str(&format!(" at {}", loc));
            }
            s
        })
        .collect();

    Some(parts.join("\n"))
}

async fn build_health_text(pool: &SqlitePool, start: &str, end: &str) -> Option<String> {
    use sqlx::Row;

    let mut parts = Vec::new();

    // Workouts
    let workouts: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT workout_type, duration_minutes, calories_burned \
         FROM data_health_workout WHERE start_time >= $1 AND start_time <= $2",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    for row in &workouts {
        let wtype: String = row.try_get("workout_type").ok().flatten().unwrap_or_default();
        let dur: Option<f64> = row.try_get("duration_minutes").ok().flatten();
        let cal: Option<f64> = row.try_get("calories_burned").ok().flatten();
        let mut s = format!("Workout: {}", wtype);
        if let Some(d) = dur {
            s.push_str(&format!(" {:.0}min", d));
        }
        if let Some(c) = cal {
            s.push_str(&format!(" {:.0}kcal", c));
        }
        parts.push(s);
    }

    // Sleep
    let sleep: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT duration_minutes, sleep_quality_score FROM data_health_sleep \
         WHERE start_time >= $1 AND start_time <= $2",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    for row in &sleep {
        let mins: Option<i64> = row.try_get("duration_minutes").ok().flatten();
        let quality: Option<f64> = row.try_get("sleep_quality_score").ok().flatten();
        let mut s = "Sleep:".to_string();
        if let Some(m) = mins {
            s.push_str(&format!(" {}h{}m", m / 60, m % 60));
        }
        if let Some(q) = quality {
            s.push_str(&format!(" quality={:.1}", q));
        }
        parts.push(s);
    }

    // HR summary
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

    if let Some((Some(avg), Some(min), Some(max))) = hr {
        if avg > 0.0 {
            parts.push(format!("HR: avg {:.0} (range {}-{})", avg, min, max));
        }
    }

    // Steps
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

    if let Some((Some(total),)) = steps {
        if total > 0 {
            parts.push(format!("Steps: {}", total));
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

async fn build_location_text(pool: &SqlitePool, start: &str, end: &str) -> Option<String> {
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

    if rows.is_empty() {
        return None;
    }

    let parts: Vec<String> = rows
        .iter()
        .map(|row| {
            let name: String = row.try_get("place_name").ok().flatten().unwrap_or_default();
            let dur: Option<f64> = row.try_get("duration_minutes").ok().flatten();

            let mut s = name;
            if let Some(d) = dur {
                s.push_str(&format!(" ({:.0}min)", d));
            }
            s
        })
        .collect();

    Some(parts.join("\n"))
}

async fn build_financial_text(pool: &SqlitePool, start: &str, end: &str) -> Option<String> {
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

    if rows.is_empty() {
        return None;
    }

    let parts: Vec<String> = rows
        .iter()
        .map(|row| {
            let merchant: String =
                row.try_get("merchant_name").ok().flatten().unwrap_or_default();
            let amount_cents: Option<i64> = row.try_get("amount").ok().flatten();
            let category_json: String = row.try_get("category").ok().flatten().unwrap_or_default();

            // Parse category JSON array to readable string
            let category = serde_json::from_str::<Vec<String>>(&category_json)
                .map(|cats| cats.join(", "))
                .unwrap_or(category_json);

            match amount_cents {
                Some(c) => format!("{} ${:.2} ({})", merchant, c as f64 / 100.0, category),
                None => format!("{} ({})", merchant, category),
            }
        })
        .collect();

    Some(parts.join("\n"))
}

async fn build_activity_text(pool: &SqlitePool, start: &str, end: &str) -> Option<String> {
    use sqlx::Row;

    let mut parts = Vec::new();

    // App usage — grouped by app
    let apps: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
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

    for row in &apps {
        let name: String = row.try_get("app_name").ok().flatten().unwrap_or_default();
        let secs: i64 = row.try_get("secs").ok().unwrap_or(0);
        if secs > 60 {
            parts.push(format!("App: {} ({}min)", name, secs / 60));
        }
    }

    // Web browsing
    let pages: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
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

    for row in &pages {
        let title: String = row.try_get("page_title").ok().flatten().unwrap_or_default();
        parts.push(format!("Web: {}", truncate_str(&title, 100)));
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

async fn build_content_text(pool: &SqlitePool, start: &str, end: &str) -> Option<String> {
    use sqlx::Row;

    let mut parts = Vec::new();

    // Documents
    let docs: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT title, document_type FROM data_content_document \
         WHERE created_time >= $1 AND created_time <= $2 AND title IS NOT NULL LIMIT 10",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    for row in &docs {
        let title: String = row.try_get("title").ok().flatten().unwrap_or_default();
        let dtype: String = row.try_get("document_type").ok().flatten().unwrap_or_default();
        parts.push(format!("Doc [{}]: {}", dtype, title));
    }

    // AI conversations — group by conversation, take first user message
    let convos: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
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

    for row in &convos {
        let model: String = row.try_get("model").ok().flatten().unwrap_or_default();
        let msg: String = row
            .try_get("first_msg")
            .ok()
            .flatten()
            .unwrap_or_else(|| "(conversation)".to_string());
        parts.push(format!("AI ({}): {}", model, truncate_str(&msg, 150)));
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

// ── Embedding client ─────────────────────────────────────────────────────────

/// Call Tollbooth /v1/embeddings to get a vector for text.
async fn embed_text(pool: &SqlitePool, text: &str) -> Result<Vec<f32>> {
    let _ = pool; // pool available for future use (e.g., caching)

    let tollbooth_url = std::env::var("TOLLBOOTH_URL").unwrap_or_else(|_| {
        "http://localhost:9002".into()
    });
    let secret = std::env::var("TOLLBOOTH_INTERNAL_SECRET")
        .map_err(|_| Error::Configuration("TOLLBOOTH_INTERNAL_SECRET not set".into()))?;

    let client = crate::http_client::tollbooth_client();
    let response = crate::tollbooth::with_system_auth(
        client.post(format!("{}/v1/embeddings", tollbooth_url)),
        &secret,
    )
    .json(&serde_json::json!({
        "model": EMBEDDING_MODEL,
        "input": text
    }))
    .send()
    .await
    .map_err(|e| Error::Network(format!("Embedding request failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let error_text = response.text().await.unwrap_or_default();
        return Err(Error::ExternalApi(format!(
            "Embedding error {}: {}",
            status, error_text
        )));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| Error::ExternalApi(format!("Failed to parse embedding response: {e}")))?;

    let embedding: Vec<f32> = body["data"][0]["embedding"]
        .as_array()
        .ok_or_else(|| Error::ExternalApi("No embedding in response".into()))?
        .iter()
        .filter_map(|v| v.as_f64().map(|f| f as f32))
        .collect();

    if embedding.is_empty() {
        return Err(Error::ExternalApi("Empty embedding vector".into()));
    }

    Ok(embedding)
}

// ── Embedding storage ────────────────────────────────────────────────────────

/// Store a domain embedding in the database.
async fn store_domain_embedding(
    pool: &SqlitePool,
    day_date: &str,
    domain: &str,
    embedding: &[f32],
    text_hash: &str,
) -> Result<()> {
    let id = format!("{}_{}", day_date, domain);
    let embedding_bytes = embedding_to_bytes(embedding);

    sqlx::query(
        "INSERT INTO wiki_day_domain_embeddings (id, day_date, domain, embedding, text_hash, model, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, datetime('now')) \
         ON CONFLICT(day_date, domain) DO UPDATE SET \
         embedding = excluded.embedding, text_hash = excluded.text_hash, \
         model = excluded.model, created_at = datetime('now')",
    )
    .bind(&id)
    .bind(day_date)
    .bind(domain)
    .bind(&embedding_bytes)
    .bind(text_hash)
    .bind(EMBEDDING_MODEL)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to store embedding: {e}")))?;

    Ok(())
}

/// Get a stored embedding (text_hash, embedding vector) for a domain on a date.
async fn get_stored_embedding(
    pool: &SqlitePool,
    day_date: &str,
    domain: &str,
) -> Result<Option<(String, Vec<f32>)>> {
    use sqlx::Row;

    let row: Option<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT text_hash, embedding FROM wiki_day_domain_embeddings \
         WHERE day_date = $1 AND domain = $2",
    )
    .bind(day_date)
    .bind(domain)
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

// ── Centroid computation ─────────────────────────────────────────────────────

/// Compute an exponentially-decayed centroid for a domain over the past N days.
/// Returns None if no historical embeddings exist.
async fn compute_domain_centroid(
    pool: &SqlitePool,
    domain: &str,
    before_date: &str,
    window_days: i64,
) -> Result<Option<Vec<f32>>> {
    use sqlx::Row;

    let before = NaiveDate::parse_from_str(before_date, "%Y-%m-%d")
        .map_err(|e| Error::InvalidInput(format!("Invalid date: {e}")))?;
    let window_start = before - chrono::Duration::days(window_days);

    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT day_date, embedding FROM wiki_day_domain_embeddings \
         WHERE domain = $1 AND day_date >= $2 AND day_date < $3 \
         ORDER BY day_date DESC",
    )
    .bind(domain)
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

/// Compute the chaos score from domain embeddings and context coverage.
///
/// For each domain:
///   1. Cosine similarity to 30-day centroid
///   2. domain_chaos = 1 - similarity
///   3. Distribute across 7 dims via ontology context weights
///
/// Final: chaos = sum(chaos[dim] * coverage[dim]) / sum(coverage[dim])
async fn compute_chaos_score(
    pool: &SqlitePool,
    date_str: &str,
    domain_embeddings: &[(String, Vec<f32>)],
    context_vector: &[f32; 7],
) -> Result<f64> {
    let mut chaos_contributions = [0.0f64; 7];
    let mut weight_sums = [0.0f64; 7];

    for (domain, embedding) in domain_embeddings {
        // Get centroid for this domain
        let centroid = compute_domain_centroid(pool, domain, date_str, CENTROID_WINDOW_DAYS).await?;

        let domain_chaos = match centroid {
            Some(ref c) => {
                let sim = cosine_similarity(embedding, c);
                (1.0 - sim as f64).max(0.0).min(1.0)
            }
            None => {
                // No historical data — can't compute chaos, skip this domain
                continue;
            }
        };

        // Get context weights for this domain's ontologies
        let domain_onts = get_ontologies_by_domain(domain);
        if domain_onts.is_empty() {
            continue;
        }

        // Average context weights across this domain's ontologies
        let mut avg_weights = [0.0f64; 7];
        for ont in &domain_onts {
            for dim in 0..7 {
                avg_weights[dim] += ont.context_weights[dim] as f64;
            }
        }
        let n = domain_onts.len() as f64;
        for dim in 0..7 {
            avg_weights[dim] /= n;
        }

        // Distribute domain chaos across dimensions
        for dim in 0..7 {
            if avg_weights[dim] > 0.0 {
                chaos_contributions[dim] += domain_chaos * avg_weights[dim];
                weight_sums[dim] += avg_weights[dim];
            }
        }
    }

    // Normalize chaos per dimension
    let mut dim_chaos = [0.0f64; 7];
    for dim in 0..7 {
        if weight_sums[dim] > 0.0 {
            dim_chaos[dim] = chaos_contributions[dim] / weight_sums[dim];
        }
    }

    // Coverage-weighted final score
    let mut numerator = 0.0f64;
    let mut denominator = 0.0f64;
    for dim in 0..7 {
        let coverage = context_vector[dim] as f64;
        numerator += dim_chaos[dim] * coverage;
        denominator += coverage;
    }

    let chaos = if denominator > 0.0 {
        (numerator / denominator).max(0.0).min(1.0)
    } else {
        0.0
    };

    Ok(chaos)
}

// ── Utility functions ────────────────────────────────────────────────────────

/// Cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
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
    if denom > 0.0 {
        dot / denom
    } else {
        0.0
    }
}

/// Convert f32 embedding to bytes for BLOB storage.
fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding
        .iter()
        .flat_map(|f| f.to_le_bytes())
        .collect()
}

/// Convert bytes from BLOB back to f32 embedding.
fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

/// Compute a simple hash for text content to detect changes.
fn compute_text_hash(text: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Truncate a string to max chars, returning a reference.
fn truncate_str(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        // Find a char boundary
        let mut end = max;
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        &s[..end]
    }
}

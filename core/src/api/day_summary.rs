//! Daily Summary Generation
//!
//! Gathers a day's structured data (sources, health aggregates, messages),
//! builds a text prompt, calls an LLM via Tollbooth, and saves the result
//! as the day's autobiography. Also computes the 7-dimension context vector
//! (who, whom, what, when, where, why, how) from ontology data presence.

use chrono::{NaiveDate, TimeZone};
use chrono_tz::Tz;
use sqlx::SqlitePool;

use crate::error::{Error, Result};

use super::wiki::{
    get_day_sources, get_or_create_day, update_day, DaySource, UpdateWikiDayRequest, WikiDay,
};
use virtues_registry::ontologies::registered_ontologies;

// ── Constants ────────────────────────────────────────────────────────────────

const SYSTEM_PROMPT: &str = r#"You are writing a brief first-person diary entry for a personal journal. Write 2-5 sentences that capture what mattered about this day — not a log of every event, but the through-line or shape of the day. Prioritize the most meaningful events and interactions over comprehensive coverage. Be direct and concrete, never poetic or flowery. If the data is sparse, write less — even a single sentence is fine. Never infer emotions, motivations, or details not present in the data."#;

/// Max characters per prompt section before truncation
const MAX_SECTION_CHARS: usize = 1500;
/// Max total user prompt characters (~4000 tokens)
const MAX_TOTAL_CHARS: usize = 16000;

// ── Timezone helpers ─────────────────────────────────────────────────────────

/// Compute day boundaries in the user's timezone, converted to UTC RFC3339 strings.
/// Falls back to wide UTC window (00:00 → 12:00 next day) if timezone is None or invalid.
pub fn day_boundaries_utc(date: NaiveDate, timezone: Option<&str>) -> (String, String) {
    if let Some(tz_str) = timezone {
        if let Ok(tz) = tz_str.parse::<Tz>() {
            let start_local = date.and_hms_opt(0, 0, 0).unwrap();
            let end_local = date.succ_opt().unwrap().and_hms_opt(0, 0, 0).unwrap();

            let start_utc = tz
                .from_local_datetime(&start_local)
                .earliest()
                .map(|dt| dt.with_timezone(&chrono::Utc));
            let end_utc = tz
                .from_local_datetime(&end_local)
                .earliest()
                .map(|dt| dt.with_timezone(&chrono::Utc));

            if let (Some(s), Some(e)) = (start_utc, end_utc) {
                return (s.to_rfc3339(), e.to_rfc3339());
            }
        }
    }

    // Fallback: existing wide UTC window for backward compatibility
    let start = date.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let end = date
        .succ_opt()
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap()
        .and_utc();
    (start.to_rfc3339(), end.to_rfc3339())
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Generate a daily summary from the day's data and save it as the autobiography.
pub async fn generate_day_summary(pool: &SqlitePool, date: NaiveDate) -> Result<WikiDay> {
    // 1. Gather structured sources (calendar, locations, transactions, chats, pages, etc.)
    let sources = get_day_sources(pool, date).await?;
    let _day = get_or_create_day(pool, date).await?;

    // 2. Compute date boundaries using profile timezone
    let timezone = super::profile::get_timezone(pool).await.unwrap_or(None);
    let (start_str, end_str) = day_boundaries_utc(date, timezone.as_deref());

    // 2b. Early exit if zero ontology data exists for this day
    let ontology_presence = detect_ontology_presence(pool, &start_str, &end_str).await;
    if !ontology_presence.iter().any(|(_, present)| *present) {
        tracing::debug!(date = %date, "No ontology data for this day, skipping summary generation");
        return get_or_create_day(pool, date).await;
    }

    // 3. Inline health aggregations
    let health_snapshot = build_health_snapshot(pool, &start_str, &end_str).await;

    // 4. Fetch full social messages
    let messages_section = build_messages_section(pool, &start_str, &end_str).await;

    // 5. Assemble prompt from all sections
    let day_of_week = date.format("%A").to_string();
    let date_display = date.format("%B %e, %Y").to_string();

    let mut prompt = format!("Date: {}, {}\n", day_of_week, date_display);

    // Group sources by type and build sections
    let grouped = group_sources_for_prompt(&sources);
    for section in grouped {
        append_section(&mut prompt, &section);
    }

    // Add health snapshot
    if let Some(health) = health_snapshot {
        append_section(&mut prompt, &health);
    }

    // Add messages
    if let Some(msgs) = messages_section {
        append_section(&mut prompt, &msgs);
    }

    // 5b. Supplemental sources (Phase 4: missing ontologies)
    let transcription_section = build_transcription_section(pool, &start_str, &end_str).await;
    let app_usage_section = build_app_usage_section(pool, &start_str, &end_str).await;
    let web_browsing_section = build_web_browsing_section(pool, &start_str, &end_str).await;
    let knowledge_section = build_content_section(pool, &start_str, &end_str).await;
    let chat_section = build_chat_section(pool, &start_str, &end_str).await;
    let page_section = build_page_section(pool, &start_str, &end_str).await;

    if let Some(s) = transcription_section {
        append_section(&mut prompt, &s);
    }
    if let Some(s) = app_usage_section {
        append_section(&mut prompt, &s);
    }
    if let Some(s) = web_browsing_section {
        append_section(&mut prompt, &s);
    }
    if let Some(s) = knowledge_section {
        append_section(&mut prompt, &s);
    }
    if let Some(s) = chat_section {
        append_section(&mut prompt, &s);
    }
    if let Some(s) = page_section {
        append_section(&mut prompt, &s);
    }

    // Truncate total if needed
    if prompt.len() > MAX_TOTAL_CHARS {
        prompt.truncate(MAX_TOTAL_CHARS);
        prompt.push_str("\n\n(data truncated)");
    }

    tracing::info!(
        date = %date,
        prompt_chars = prompt.len(),
        source_count = sources.len(),
        "Generating daily summary"
    );

    // 6. Compute 7-dim context vector from ontology presence (already fetched in step 2b)
    let context_vector = compute_context_vector(&ontology_presence);
    let context_vector_json = serde_json::json!({
        "who": context_vector[0],
        "whom": context_vector[1],
        "what": context_vector[2],
        "when": context_vector[3],
        "where": context_vector[4],
        "why": context_vector[5],
        "how": context_vector[6],
    });

    // 7. Call Tollbooth
    let summary_text = call_tollbooth(pool, &prompt).await?;

    // 8. Generate domain embeddings + chaos score
    let chaos_result = super::day_scoring::generate_embeddings_and_score(
        pool, date, &context_vector,
    )
    .await
    .unwrap_or_else(|e| {
        tracing::warn!(error = %e, "Chaos scoring failed, skipping");
        super::day_scoring::ChaosScoreResult { score: None, calibration_days: 0 }
    });

    // 9. Save to wiki_days
    update_day(
        pool,
        date,
        UpdateWikiDayRequest {
            autobiography: Some(summary_text),
            autobiography_sections: None,
            last_edited_by: Some("ai".to_string()),
            cover_image: None,
            context_vector: Some(context_vector_json),
            chaos_score: chaos_result.score,
            entropy_calibration_days: Some(chaos_result.calibration_days),
            start_timezone: timezone,
        },
    )
    .await
}

// ── Section builders ─────────────────────────────────────────────────────────

/// A prompt section with a heading and body
struct PromptSection {
    heading: String,
    body: String,
}

/// Group DaySources by type into prompt sections
fn group_sources_for_prompt(sources: &[DaySource]) -> Vec<PromptSection> {
    use std::collections::BTreeMap;

    // Group by source_type, preserve order
    let mut groups: BTreeMap<String, Vec<&DaySource>> = BTreeMap::new();
    for source in sources {
        groups
            .entry(source_type_heading(&source.source_type))
            .or_default()
            .push(source);
    }

    let mut sections = Vec::new();
    for (heading, items) in groups {
        let mut lines = Vec::new();
        let mut char_count = 0;

        for item in &items {
            let time = item.timestamp.format("%H:%M").to_string();
            let line = match &item.preview {
                Some(preview) => format!("- {} {} — {}", time, item.label, preview),
                None => format!("- {} {}", time, item.label),
            };

            char_count += line.len();
            if char_count > MAX_SECTION_CHARS {
                lines.push(format!("  ... and {} more", items.len() - lines.len()));
                break;
            }
            lines.push(line);
        }

        if !lines.is_empty() {
            sections.push(PromptSection {
                heading,
                body: lines.join("\n"),
            });
        }
    }

    sections
}

/// Map source_type to a readable heading for the prompt
fn source_type_heading(source_type: &str) -> String {
    match source_type {
        "calendar" => "Schedule".to_string(),
        "email" => "Emails".to_string(),
        "location" => "Places".to_string(),
        "workout" => "Workouts".to_string(),
        "sleep" => "Sleep".to_string(),
        "transaction" => "Transactions".to_string(),
        "transcription" => "Voice Recordings".to_string(),
        "chat" => "Chats".to_string(),
        "page" => "Pages Updated".to_string(),
        "steps" => "Steps".to_string(),
        other if other.starts_with("message:") => {
            let platform = other.strip_prefix("message:").unwrap_or("unknown");
            format!("Messages ({})", platform)
        }
        other => other.to_string(),
    }
}

/// Build health snapshot from aggregation queries
async fn build_health_snapshot(
    pool: &SqlitePool,
    start_str: &str,
    end_str: &str,
) -> Option<PromptSection> {
    let mut lines = Vec::new();

    // Heart rate
    let hr: Option<(Option<i32>, Option<i32>, Option<f64>, i32)> = sqlx::query_as(
        r#"
        SELECT MIN(bpm), MAX(bpm), ROUND(AVG(bpm)), COUNT(*)
        FROM data_health_heart_rate
        WHERE timestamp >= $1 AND timestamp <= $2
        "#,
    )
    .bind(start_str)
    .bind(end_str)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    if let Some((Some(min_hr), Some(max_hr), Some(avg_hr), count)) = hr {
        if count > 0 {
            lines.push(format!(
                "- Heart rate: avg {:.0}, min {}, max {} ({} readings)",
                avg_hr, min_hr, max_hr, count
            ));
        }
    }

    // Steps
    let steps: Option<(Option<i64>,)> = sqlx::query_as(
        r#"
        SELECT SUM(step_count)
        FROM data_health_steps
        WHERE timestamp >= $1 AND timestamp <= $2
        "#,
    )
    .bind(start_str)
    .bind(end_str)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    if let Some((Some(total_steps),)) = steps {
        if total_steps > 0 {
            lines.push(format!("- Steps: {}", total_steps));
        }
    }

    if lines.is_empty() {
        None
    } else {
        Some(PromptSection {
            heading: "Health Snapshot".to_string(),
            body: lines.join("\n"),
        })
    }
}

/// Build messages section with full body text (for semantic richness)
async fn build_messages_section(
    pool: &SqlitePool,
    start_str: &str,
    end_str: &str,
) -> Option<PromptSection> {
    use sqlx::Row;

    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        r#"
        SELECT from_name, body, channel, timestamp
        FROM data_communication_message
        WHERE timestamp >= $1 AND timestamp <= $2
        ORDER BY timestamp ASC
        LIMIT 30
        "#,
    )
    .bind(start_str)
    .bind(end_str)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    if rows.is_empty() {
        return None;
    }

    let total_count = rows.len();
    let mut lines = Vec::new();
    let mut char_count = 0;

    for row in &rows {
        let from_name: String = row
            .try_get("from_name")
            .ok()
            .flatten()
            .unwrap_or_else(|| "Unknown".to_string());
        let body: String = row
            .try_get("body")
            .ok()
            .flatten()
            .unwrap_or_default();

        // Truncate individual message bodies
        let body_preview: String = body.chars().take(120).collect();
        let body_display = if body_preview.len() < body.len() {
            format!("{}...", body_preview)
        } else {
            body_preview
        };

        let line = format!("- {}: \"{}\"", from_name, body_display);
        char_count += line.len();
        if char_count > MAX_SECTION_CHARS {
            lines.push(format!("  ... and {} more messages", total_count - lines.len()));
            break;
        }
        lines.push(line);
    }

    Some(PromptSection {
        heading: format!("Messages ({} total)", total_count),
        body: lines.join("\n"),
    })
}

/// Build transcription section with full transcript text (truncated per-item)
async fn build_transcription_section(
    pool: &SqlitePool,
    start_str: &str,
    end_str: &str,
) -> Option<PromptSection> {
    use sqlx::Row;

    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        r#"
        SELECT text, title, start_time
        FROM data_communication_transcription
        WHERE start_time >= $1 AND start_time <= $2
        ORDER BY start_time ASC
        LIMIT 20
        "#,
    )
    .bind(start_str)
    .bind(end_str)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    if rows.is_empty() {
        return None;
    }

    let mut lines = Vec::new();
    let mut char_count = 0;

    for row in &rows {
        let title: Option<String> = row.try_get("title").ok().flatten();
        let text: String = row.try_get("text").ok().flatten().unwrap_or_default();

        // Truncate individual transcripts to ~500 chars
        let preview: String = text.chars().take(500).collect();
        let display = if preview.len() < text.len() {
            format!("{}...", preview)
        } else {
            preview
        };

        let line = match title {
            Some(t) => format!("- {}: \"{}\"", t, display),
            None => format!("- \"{}\"", display),
        };

        char_count += line.len();
        if char_count > MAX_SECTION_CHARS {
            lines.push(format!("  ... and {} more transcriptions", rows.len() - lines.len()));
            break;
        }
        lines.push(line);
    }

    Some(PromptSection {
        heading: format!("Voice Transcriptions ({} recordings)", rows.len()),
        body: lines.join("\n"),
    })
}

/// Build app usage section grouped by app, showing top apps by duration
async fn build_app_usage_section(
    pool: &SqlitePool,
    start_str: &str,
    end_str: &str,
) -> Option<PromptSection> {
    use sqlx::Row;

    // Group by app_name, sum duration (end_time - start_time in seconds)
    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        r#"
        SELECT app_name,
               COUNT(*) as sessions,
               CAST(SUM(
                   (julianday(end_time) - julianday(start_time)) * 86400
               ) AS INTEGER) as total_seconds
        FROM data_activity_app_usage
        WHERE start_time >= $1 AND start_time <= $2
          AND app_name IS NOT NULL
        GROUP BY app_name
        ORDER BY total_seconds DESC
        LIMIT 10
        "#,
    )
    .bind(start_str)
    .bind(end_str)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    if rows.is_empty() {
        return None;
    }

    let mut lines = Vec::new();
    for row in &rows {
        let app: String = row.try_get("app_name").ok().flatten().unwrap_or_default();
        let seconds: i64 = row.try_get("total_seconds").ok().unwrap_or(0);
        let minutes = seconds / 60;

        if minutes > 0 {
            lines.push(format!("- {} — {} min", app, minutes));
        }
    }

    if lines.is_empty() {
        return None;
    }

    Some(PromptSection {
        heading: "App Usage (top by time)".to_string(),
        body: lines.join("\n"),
    })
}

/// Build web browsing section showing top pages by duration
async fn build_web_browsing_section(
    pool: &SqlitePool,
    start_str: &str,
    end_str: &str,
) -> Option<PromptSection> {
    use sqlx::Row;

    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        r#"
        SELECT page_title, url, visit_duration_seconds
        FROM data_activity_web_browsing
        WHERE timestamp >= $1 AND timestamp <= $2
          AND page_title IS NOT NULL
        ORDER BY visit_duration_seconds DESC
        LIMIT 10
        "#,
    )
    .bind(start_str)
    .bind(end_str)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    if rows.is_empty() {
        return None;
    }

    let mut lines = Vec::new();
    let mut char_count = 0;

    for row in &rows {
        let title: String = row.try_get("page_title").ok().flatten().unwrap_or_default();
        let duration: Option<i64> = row.try_get("visit_duration_seconds").ok().flatten();

        let line = match duration {
            Some(s) if s >= 60 => format!("- {} ({} min)", title, s / 60),
            Some(s) if s > 0 => format!("- {} ({}s)", title, s),
            _ => format!("- {}", title),
        };

        char_count += line.len();
        if char_count > MAX_SECTION_CHARS {
            break;
        }
        lines.push(line);
    }

    if lines.is_empty() {
        return None;
    }

    Some(PromptSection {
        heading: "Web Browsing".to_string(),
        body: lines.join("\n"),
    })
}

/// Build content section (documents + AI conversations)
async fn build_content_section(
    pool: &SqlitePool,
    start_str: &str,
    end_str: &str,
) -> Option<PromptSection> {
    use sqlx::Row;

    let mut lines = Vec::new();

    // Documents
    let docs: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        r#"
        SELECT title, document_type
        FROM data_content_document
        WHERE created_time >= $1 AND created_time <= $2
          AND title IS NOT NULL
        ORDER BY created_time ASC
        LIMIT 10
        "#,
    )
    .bind(start_str)
    .bind(end_str)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    for row in &docs {
        let title: String = row.try_get("title").ok().flatten().unwrap_or_default();
        let doc_type: Option<String> = row.try_get("document_type").ok().flatten();
        let line = match doc_type {
            Some(t) => format!("- [{}] {}", t, title),
            None => format!("- {}", title),
        };
        lines.push(line);
    }

    // AI conversations — group by conversation_id, show first user message as title
    let convos: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        r#"
        SELECT conversation_id, model,
               MIN(CASE WHEN role = 'user' THEN content END) as first_user_msg
        FROM data_content_conversation
        WHERE timestamp >= $1 AND timestamp <= $2
        GROUP BY conversation_id
        ORDER BY MIN(timestamp) ASC
        LIMIT 10
        "#,
    )
    .bind(start_str)
    .bind(end_str)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    for row in &convos {
        let model: Option<String> = row.try_get("model").ok().flatten();
        let first_msg: Option<String> = row.try_get("first_user_msg").ok().flatten();

        let preview: String = first_msg
            .unwrap_or_else(|| "(conversation)".to_string())
            .chars()
            .take(80)
            .collect();

        let line = match model {
            Some(m) => format!("- AI chat ({}): \"{}\"", m, preview),
            None => format!("- AI chat: \"{}\"", preview),
        };
        lines.push(line);
    }

    if lines.is_empty() {
        return None;
    }

    Some(PromptSection {
        heading: "Knowledge & Documents".to_string(),
        body: lines.join("\n"),
    })
}

/// Build Virtues chat sessions section — shows chat titles and first user message
async fn build_chat_section(
    pool: &SqlitePool,
    start_str: &str,
    end_str: &str,
) -> Option<PromptSection> {
    use sqlx::Row;

    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        r#"
        SELECT c.title, c.message_count,
               (SELECT content FROM app_chat_messages
                WHERE chat_id = c.id AND role = 'user'
                ORDER BY rowid ASC LIMIT 1) as first_msg
        FROM app_chats c
        WHERE c.created_at >= $1 AND c.created_at <= $2
        ORDER BY c.created_at ASC
        LIMIT 10
        "#,
    )
    .bind(start_str)
    .bind(end_str)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    if rows.is_empty() {
        return None;
    }

    let mut lines = Vec::new();
    for row in &rows {
        let title: String = row.try_get("title").ok().flatten().unwrap_or_default();
        let msg_count: i32 = row.try_get("message_count").unwrap_or(0);
        let first_msg: Option<String> = row.try_get("first_msg").ok().flatten();

        let preview: String = first_msg
            .unwrap_or_default()
            .chars()
            .take(80)
            .collect();

        if preview.is_empty() {
            lines.push(format!("- {} ({} messages)", title, msg_count));
        } else {
            lines.push(format!("- {}: \"{}\" ({} messages)", title, preview, msg_count));
        }
    }

    Some(PromptSection {
        heading: format!("Virtues Chat Sessions ({} total)", rows.len()),
        body: lines.join("\n"),
    })
}

/// Build page edits section — shows pages created/edited this day
async fn build_page_section(
    pool: &SqlitePool,
    start_str: &str,
    end_str: &str,
) -> Option<PromptSection> {
    use sqlx::Row;

    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        r#"
        SELECT title,
               CASE WHEN created_at >= $1 AND created_at <= $2 THEN 'created' ELSE 'edited' END as action
        FROM app_pages
        WHERE updated_at >= $1 AND updated_at <= $2
        ORDER BY updated_at ASC
        LIMIT 15
        "#,
    )
    .bind(start_str)
    .bind(end_str)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    if rows.is_empty() {
        return None;
    }

    let lines: Vec<String> = rows
        .iter()
        .map(|row| {
            let title: String = row.try_get("title").ok().flatten().unwrap_or_default();
            let action: String = row.try_get("action").ok().flatten().unwrap_or_default();
            format!("- {} ({})", title, action)
        })
        .collect();

    Some(PromptSection {
        heading: format!("Wiki Pages ({} edits)", rows.len()),
        body: lines.join("\n"),
    })
}

/// Append a section to the prompt string
fn append_section(prompt: &mut String, section: &PromptSection) {
    prompt.push_str(&format!("\n## {}\n{}\n", section.heading, section.body));
}

// ── Context vector computation ───────────────────────────────────────────────

/// Detect which ontologies have data for a given time window.
/// Returns Vec<(ontology_name, has_data)> for all registered ontologies.
async fn detect_ontology_presence(
    pool: &SqlitePool,
    start_str: &str,
    end_str: &str,
) -> Vec<(String, bool)> {
    let ontologies = registered_ontologies();
    let mut presence = Vec::with_capacity(ontologies.len());

    for ont in &ontologies {
        let ts_col = ont.timestamp_column;
        let table = ont.table_name;
        let query = format!(
            "SELECT COUNT(*) as cnt FROM {} WHERE {} >= $1 AND {} <= $2 LIMIT 1",
            table, ts_col, ts_col
        );

        let has_data: bool = sqlx::query_scalar::<_, i32>(&query)
            .bind(start_str)
            .bind(end_str)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .unwrap_or(0)
            > 0;

        presence.push((ont.name.to_string(), has_data));
    }

    presence
}

/// Compute the 7-dimension context vector from ontology presence.
/// Each dimension = sum(weights of present ontologies) / sum(weights of all ontologies).
/// Dimensions: [who, whom, what, when, where, why, how]
fn compute_context_vector(ontology_presence: &[(String, bool)]) -> [f32; 7] {
    let ontologies = registered_ontologies();
    let mut total_weights = [0.0f32; 7];
    let mut present_weights = [0.0f32; 7];

    for ont in &ontologies {
        for dim in 0..7 {
            total_weights[dim] += ont.context_weights[dim];
        }

        // Check if this ontology has data
        let has_data = ontology_presence
            .iter()
            .any(|(name, present)| name == ont.name && *present);

        if has_data {
            for dim in 0..7 {
                present_weights[dim] += ont.context_weights[dim];
            }
        }
    }

    let mut vector = [0.0f32; 7];
    for dim in 0..7 {
        if total_weights[dim] > 0.0 {
            vector[dim] = present_weights[dim] / total_weights[dim];
        }
    }

    vector
}

// ── Tollbooth call ───────────────────────────────────────────────────────────

/// Call Tollbooth for the summary generation
async fn call_tollbooth(pool: &SqlitePool, user_prompt: &str) -> Result<String> {
    let chat_model = crate::api::assistant_profile::get_chat_model(pool).await?;

    let tollbooth_url = std::env::var("TOLLBOOTH_URL").unwrap_or_else(|_| {
        tracing::warn!("TOLLBOOTH_URL not set, using default localhost:9002");
        "http://localhost:9002".into()
    });
    let secret = std::env::var("TOLLBOOTH_INTERNAL_SECRET")
        .map_err(|_| Error::Configuration("TOLLBOOTH_INTERNAL_SECRET not set".into()))?;

    let client = crate::http_client::tollbooth_client();
    let response = crate::tollbooth::with_system_auth(
        client.post(format!("{}/v1/chat/completions", tollbooth_url)),
        &secret,
    )
    .json(&serde_json::json!({
        "model": chat_model,
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": user_prompt}
        ],
        "max_tokens": 500,
        "temperature": 0.3
    }))
    .send()
    .await
    .map_err(|e| Error::Network(format!("Tollbooth request failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let error_text = response.text().await.unwrap_or_default();
        let error_msg = match status {
            402 => "Usage limit reached for summary generation".to_string(),
            429 => "Rate limited. Please try again later.".to_string(),
            _ => format!("Tollbooth error {}: {}", status, error_text),
        };
        return Err(Error::ExternalApi(error_msg));
    }

    let response_json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| Error::ExternalApi(format!("Failed to parse Tollbooth response: {e}")))?;

    let summary = response_json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();

    if summary.is_empty() {
        return Err(Error::ExternalApi(
            "LLM returned empty summary".to_string(),
        ));
    }

    tracing::info!(
        summary_chars = summary.len(),
        "Daily summary generated"
    );

    Ok(summary)
}

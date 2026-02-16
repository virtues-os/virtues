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
    create_temporal_event, delete_auto_events_for_day, get_day_sources, get_or_create_day,
    update_day, CreateTemporalEventRequest, DaySource, UpdateWikiDayRequest, WikiDay,
};
use virtues_registry::ontologies::registered_ontologies;

// ── Constants ────────────────────────────────────────────────────────────────

const SYSTEM_PROMPT: &str = r#"You are writing a brief first-person diary entry for a personal journal. Write 2-5 sentences that capture what mattered about this day — not a log of every event, but the through-line or shape of the day. Prioritize the most meaningful events and interactions over comprehensive coverage. Be direct and concrete, never poetic or flowery. If the data is sparse, write less — even a single sentence is fine. Never infer emotions, motivations, or details not present in the data.

After the diary entry, on a new line, output a JSON block with the day's events as a perfect 24-hour calendar. Use this exact format:
---EVENTS---
[{"start": "HH:MM", "end": "HH:MM", "label": "Brief description"}]

Rules:
- Events MUST cover the full 24 hours: first event starts at "00:00", last event ends at "24:00". No gaps, no overlaps.
- Use 24-hour time format (HH:MM). Events are contiguous — each event's end time equals the next event's start time.
- Label events based ONLY on data present in the sources. Do NOT infer activities not evidenced by the data.
- "Sleep" is valid ONLY when sleep tracking data (e.g., Apple Health, Oura) is present in the sources. Do not infer sleep from absence of data. Do not guess wake-up times — if sleep data ends at 06:30 but the next data point is at 09:00, end the sleep event at 06:30 and mark 06:30-09:00 as "Unknown".
- Use "Unknown" for any time period where the data is sparse or absent. It is perfectly fine to have multiple "Unknown" segments.
- Aim for 6-16 events depending on how much data exists. A sparse day might have 6 events (mostly "Unknown"). A rich day might have 12-16.
- Do not pad or fabricate events to reach a minimum count. If the data only supports 4 labeled events plus "Unknown" gaps, that is correct.
- Good labels: "Morning routine", "Work session", "Lunch with Sarah", "Evening walk", "Reading", "Commute", "Sleep" (when sleep data exists). Bad labels: "Sleep" (no sleep data), "Relaxing at home" (inferred)."#;

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
    let raw_response = call_tollbooth(pool, &prompt).await?;

    // 8. Parse response: extract diary text and structured events
    let (summary_text, event_json) = parse_tollbooth_response(&raw_response);

    // 9. Store structured events + compute per-event W6H activation + embeddings + entropy
    //    Must happen BEFORE chaos scoring — chaos now aggregates event embeddings.
    let day_stub = get_or_create_day(pool, date).await?;
    let event_embeddings = if let Some(events) = event_json {
        store_structured_events(pool, &day_stub, date, timezone.as_deref(), &events, &start_str, &end_str).await
    } else {
        Vec::new()
    };

    // 10. Generate chaos score from aggregated event embeddings
    let chaos_result = super::day_scoring::generate_embeddings_and_score(
        pool, date, &context_vector, &event_embeddings,
    )
    .await
    .unwrap_or_else(|e| {
        tracing::warn!(error = %e, "Chaos scoring failed, skipping");
        super::day_scoring::ChaosScoreResult { score: None, calibration_days: 0 }
    });

    // 11. Save to wiki_days
    let day = update_day(
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
            start_timezone: timezone.clone(),
            snapshot: None,
        },
    )
    .await?;

    Ok(day)
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
        "max_tokens": 1000,
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

// ── Structured event parsing + W6H activation ───────────────────────────────

/// LLM event parsed from Tollbooth response
#[derive(Debug, serde::Deserialize)]
struct LlmEvent {
    start: String,
    end: String,
    label: String,
}

/// Split Tollbooth response into diary text and optional events JSON.
/// Handles markdown code fences (```json ... ```) that LLMs sometimes wrap around JSON.
fn parse_tollbooth_response(response: &str) -> (String, Option<Vec<LlmEvent>>) {
    if let Some(idx) = response.find("---EVENTS---") {
        let diary = response[..idx].trim().to_string();
        let mut events_str = response[idx + "---EVENTS---".len()..].trim();
        // Strip markdown code fences if present
        events_str = events_str
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        let events: Option<Vec<LlmEvent>> = serde_json::from_str(events_str)
            .map_err(|e| {
                tracing::warn!(error = %e, raw = events_str, "Failed to parse structured events from LLM");
                e
            })
            .ok();
        (diary, events)
    } else {
        (response.trim().to_string(), None)
    }
}

/// Store LLM-identified events, compute W6H activation, embed, and compute entropy.
///
/// Three-pass approach:
///   Pass 1: Create events in DB, compute W6H activation, collect ontology text per event
///   Pass 2: Batch embed all event texts in a single ONNX call
///   Pass 3: Compute entropy scores (embedding novelty + Shannon W6H), store everything
///
/// Returns `Vec<(embedding, duration_minutes)>` for events that were successfully
/// embedded. The caller uses this to compute a duration-weighted day centroid for
/// cross-day chaos scoring.
async fn store_structured_events(
    pool: &SqlitePool,
    day: &WikiDay,
    date: NaiveDate,
    timezone: Option<&str>,
    events: &[LlmEvent],
    _start_str: &str,
    _end_str: &str,
) -> Vec<(Vec<f32>, f32)> {
    use super::day_scoring::{
        collect_ontology_texts, compute_w6h_entropy, cosine_similarity, embedding_to_bytes,
    };

    // Clear previous auto events
    if let Err(e) = delete_auto_events_for_day(pool, day.id.clone()).await {
        tracing::warn!(error = %e, "Failed to delete existing auto events");
        return Vec::new();
    }

    let tz: Option<Tz> = timezone.and_then(|s| s.parse().ok());
    let date_str = date.to_string();

    // ── Backfill gaps to ensure perfect 24h coverage (00:00–24:00) ───────

    let all_events = backfill_24h_events(events, date, tz.as_ref());

    // ── Pass 1: Create events, compute W6H, collect text ─────────────────

    struct EventWork {
        event_id: String,
        w6h: [f32; 7],
        text: String,
        duration_minutes: f32,
    }

    let mut work: Vec<EventWork> = Vec::new();

    for event in &all_events {
        let start_rfc = event.start_utc.to_rfc3339();
        let end_rfc = event.end_utc.to_rfc3339();

        // Collect ontology texts for this event's time range (needed for embedding + source tracking)
        let ontology_texts = collect_ontology_texts(pool, &start_rfc, &end_rfc, &date_str).await;

        // Extract source ontology names for this event
        let source_names: Vec<String> = ontology_texts
            .iter()
            .map(|ot| ot.ontology_name.clone())
            .collect();

        // Extract auto_location from location_visit data (longest visit in time range)
        let auto_location = extract_event_location(pool, &start_rfc, &end_rfc).await;

        // Create the event row
        let created = create_temporal_event(
            pool,
            CreateTemporalEventRequest {
                day_id: day.id.clone(),
                start_time: event.start_utc,
                end_time: event.end_utc,
                auto_label: Some(event.label.clone()),
                auto_location,
                user_label: None,
                user_location: None,
                user_notes: None,
                source_ontologies: if source_names.is_empty() {
                    None
                } else {
                    Some(serde_json::json!(source_names))
                },
                is_unknown: Some(event.is_unknown),
                is_transit: Some(false),
                is_user_added: Some(false),
            },
        )
        .await;

        let created_event = match created {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(error = %e, label = event.label, "Failed to create temporal event");
                continue;
            }
        };

        // Compute W6H activation for this event's time range
        let w6h = compute_event_w6h(pool, &start_rfc, &end_rfc).await;

        // Build text for embedding (cap at 2000 chars)
        let combined_text: String = ontology_texts
            .iter()
            .map(|ot| ot.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let text = if combined_text.len() > 2000 {
            combined_text[..2000].to_string()
        } else {
            combined_text
        };

        let duration_minutes = (event.end_utc - event.start_utc).num_minutes().max(1) as f32;

        work.push(EventWork {
            event_id: created_event.id,
            w6h,
            text,
            duration_minutes,
        });
    }

    // ── Pass 2: Batch embed all event texts ──────────────────────────────

    // Collect non-empty texts and their indices
    let texts_with_idx: Vec<(usize, String)> = work
        .iter()
        .enumerate()
        .filter(|(_, w)| !w.text.trim().is_empty())
        .map(|(i, w)| (i, w.text.clone()))
        .collect();

    let mut embeddings: Vec<Option<Vec<f32>>> = vec![None; work.len()];

    if !texts_with_idx.is_empty() {
        let batch_texts: Vec<String> = texts_with_idx.iter().map(|(_, t)| t.clone()).collect();
        let batch_indices: Vec<usize> = texts_with_idx.iter().map(|(i, _)| *i).collect();

        match crate::search::embedder::get_embedder().await {
            Ok(embedder) => match embedder.embed_batch_async(batch_texts).await {
                Ok(batch_results) => {
                    for (result_idx, &work_idx) in batch_indices.iter().enumerate() {
                        if result_idx < batch_results.len() {
                            embeddings[work_idx] = Some(batch_results[result_idx].clone());
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Batch embedding failed, proceeding with Shannon-only entropy");
                }
            },
            Err(e) => {
                tracing::warn!(error = %e, "Failed to load embedder, proceeding with Shannon-only entropy");
            }
        }
    }

    // ── Compute day centroid from available embeddings ─────────────────
    let day_centroid: Option<Vec<f32>> = {
        let embedded: Vec<(&Vec<f32>, f32)> = work
            .iter()
            .enumerate()
            .filter_map(|(i, w)| embeddings[i].as_ref().map(|e| (e, w.duration_minutes)))
            .collect();

        if embedded.is_empty() {
            None
        } else {
            let dim = embedded[0].0.len();
            let mut weighted_sum = vec![0.0f64; dim];
            let mut total_weight = 0.0f64;

            for (emb, duration) in &embedded {
                let w = *duration as f64;
                total_weight += w;
                for (j, &val) in emb.iter().enumerate() {
                    if j < dim {
                        weighted_sum[j] += val as f64 * w;
                    }
                }
            }

            let centroid: Vec<f32> = weighted_sum.iter().map(|v| (v / total_weight) as f32).collect();
            let norm: f32 = centroid.iter().map(|v| v * v).sum::<f32>().sqrt();
            if norm > 0.0 {
                Some(centroid.iter().map(|v| v / norm).collect())
            } else {
                None
            }
        }
    };

    // ── Pass 3: Compute entropy + store everything ───────────────────────

    let mut event_embeddings: Vec<(Vec<f32>, f32)> = Vec::new();

    for (i, item) in work.iter().enumerate() {
        let w6h_json = serde_json::to_string(&item.w6h).unwrap_or_default();
        let w6h_entropy = compute_w6h_entropy(&item.w6h);

        let embedding_ref = embeddings[i].as_ref();

        // Semantic distinctness: 1 - cosine_sim(this, day_centroid). Falls back to Shannon when embedding or centroid unavailable.
        let entropy = match (embedding_ref, day_centroid.as_ref()) {
            (Some(curr), Some(centroid)) => {
                (1.0 - cosine_similarity(curr, centroid) as f64).clamp(0.0, 1.0)
            }
            _ => w6h_entropy,
        };

        // Store embedding bytes (or NULL)
        let embedding_bytes = embedding_ref.map(|e| embedding_to_bytes(e));

        if let Err(e) = sqlx::query(
            "UPDATE wiki_events SET w6h_activation = $1, embedding = $2, entropy = $3, w6h_entropy = $4 WHERE id = $5",
        )
        .bind(&w6h_json)
        .bind(&embedding_bytes)
        .bind(entropy)
        .bind(w6h_entropy)
        .bind(&item.event_id)
        .execute(pool)
        .await
        {
            tracing::warn!(error = %e, event_id = item.event_id, "Failed to store event data");
        }

        // Track for day centroid computation (used by caller for chaos scoring)
        if let Some(emb) = embedding_ref {
            event_embeddings.push((emb.clone(), item.duration_minutes));
        }
    }

    tracing::info!(
        date = %date,
        event_count = all_events.len(),
        embedded_count = event_embeddings.len(),
        "Stored structured events with W6H activation and entropy"
    );

    event_embeddings
}

/// Extract the primary location for an event's time range from location_visit data.
/// Returns the place name with the longest visit duration, or None if no location data.
async fn extract_event_location(pool: &SqlitePool, start: &str, end: &str) -> Option<String> {
    use sqlx::Row;
    let row: Option<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT place_name FROM data_location_visit \
         WHERE arrival_time >= $1 AND arrival_time <= $2 \
         ORDER BY duration_minutes DESC LIMIT 1",
    )
    .bind(start)
    .bind(end)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    row.and_then(|r| r.try_get::<Option<String>, _>("place_name").ok().flatten())
        .filter(|s| !s.is_empty())
}

/// An event with pre-computed UTC times (either from LLM or gap-filled).
struct ResolvedEvent {
    start_utc: chrono::DateTime<chrono::Utc>,
    end_utc: chrono::DateTime<chrono::Utc>,
    label: String,
    is_unknown: bool,
}

/// Take LLM events and produce a perfect 24h timeline (00:00–24:00) by filling gaps
/// with "Unknown" events. Events are sorted by start time and clamped to day boundaries.
fn backfill_24h_events(
    llm_events: &[LlmEvent],
    date: NaiveDate,
    tz: Option<&Tz>,
) -> Vec<ResolvedEvent> {
    // Day boundaries in UTC
    let day_start = parse_hhmm_to_utc("00:00", date, tz)
        .unwrap_or_else(|| date.and_hms_opt(0, 0, 0).unwrap().and_utc());
    let day_end = parse_hhmm_to_utc("00:00", date + chrono::Duration::days(1), tz)
        .unwrap_or_else(|| (date + chrono::Duration::days(1)).and_hms_opt(0, 0, 0).unwrap().and_utc());

    // Parse and sort LLM events
    let mut parsed: Vec<ResolvedEvent> = llm_events
        .iter()
        .filter_map(|e| {
            let start = parse_hhmm_to_utc(&e.start, date, tz)?;
            let end = parse_hhmm_to_utc(&e.end, date, tz)?;
            if end <= start { return None; } // skip invalid
            Some(ResolvedEvent {
                start_utc: start.max(day_start),
                end_utc: end.min(day_end),
                label: e.label.clone(),
                is_unknown: false,
            })
        })
        .collect();
    parsed.sort_by_key(|e| e.start_utc);

    // Resolve overlaps: if event B starts before event A ends, truncate A's end to B's start.
    // If that makes A zero-width, drop it.
    let mut resolved: Vec<ResolvedEvent> = Vec::new();
    for event in parsed {
        if let Some(prev) = resolved.last_mut() {
            if event.start_utc < prev.end_utc {
                // Overlap: truncate previous event
                prev.end_utc = event.start_utc;
                if prev.end_utc <= prev.start_utc {
                    resolved.pop(); // zero-width, remove it
                }
            }
        }
        resolved.push(event);
    }

    // Build complete timeline with gaps filled
    let mut result: Vec<ResolvedEvent> = Vec::new();
    let mut cursor = day_start;

    for event in resolved {
        // Fill gap before this event
        if event.start_utc > cursor {
            result.push(ResolvedEvent {
                start_utc: cursor,
                end_utc: event.start_utc,
                label: "Unknown".to_string(),
                is_unknown: true,
            });
        }
        cursor = event.end_utc;
        result.push(event);
    }

    // Fill gap after last event to end of day
    if cursor < day_end {
        result.push(ResolvedEvent {
            start_utc: cursor,
            end_utc: day_end,
            label: "Unknown".to_string(),
            is_unknown: true,
        });
    }

    result
}

/// Parse "HH:MM" string into UTC DateTime for the given date and timezone.
/// Handles "24:00" as midnight of the next day.
fn parse_hhmm_to_utc(
    hhmm: &str,
    date: NaiveDate,
    tz: Option<&Tz>,
) -> Option<chrono::DateTime<chrono::Utc>> {
    let parts: Vec<&str> = hhmm.split(':').collect();
    if parts.len() != 2 {
        return None;
    }
    let hour: u32 = parts[0].parse().ok()?;
    let minute: u32 = parts[1].parse().ok()?;

    // "24:00" means midnight of the next day
    if hour == 24 {
        let next_day = date + chrono::Duration::days(1);
        let naive = next_day.and_hms_opt(0, 0, 0)?;
        return if let Some(tz) = tz {
            tz.from_local_datetime(&naive)
                .earliest()
                .map(|dt| dt.with_timezone(&chrono::Utc))
        } else {
            Some(naive.and_utc())
        };
    }

    let naive = date.and_hms_opt(hour, minute, 0)?;

    if let Some(tz) = tz {
        tz.from_local_datetime(&naive)
            .earliest()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    } else {
        Some(naive.and_utc())
    }
}

/// Compute W6H activation for a time range by checking ontology presence.
/// Same algorithm as compute_context_vector() but for an arbitrary time range.
async fn compute_event_w6h(pool: &SqlitePool, start: &str, end: &str) -> [f32; 7] {
    let presence = detect_ontology_presence(pool, start, end).await;
    compute_context_vector(&presence)
}

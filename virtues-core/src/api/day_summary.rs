//! Daily Summary Generation
//!
//! Gathers a day's structured data (sources, health aggregates, messages),
//! builds a text prompt, calls an LLM via virtues-api, and saves the result
//! as the day's autobiography with structured timeline events.

use chrono::{NaiveDate, TimeZone};
use chrono_tz::Tz;
use sqlx::PgPool;

use crate::error::{Error, Result};

use super::wiki::{
    create_temporal_event, delete_auto_events_for_day, get_day_sources, get_or_create_day,
    update_day, CreateTemporalEventRequest, DaySource, UpdateWikiDayRequest, WikiDay,
};
use virtues_registry::ontologies::registered_ontologies;

// ── Constants ────────────────────────────────────────────────────────────────

const SYSTEM_PROMPT: &str = r#"You are writing a brief second-person autobiography for a personal day page. Your job is to surface the meaning layer — what connected, what was unusual, what the cross-domain data reveals — not to log what happened when (the event timeline already does that).

LENGTH — scale to data density:
- Sparse day (a handful of data points, a few hours of coverage): 1-3 sentences. Often a single sentence is the right answer.
- Moderate day (data across most of the waking hours, a few distinct activities): 3-6 sentences.
- Rich day (continuous coverage, many distinct activities, multiple ontologies firing): up to 180 words.
- Length follows evidence. Padding a sparse day with prose is the most common failure here — don't.

GUIDELINES:
- Lead with a pattern or insight, not a timestamp. "The routine held until 4:12 PM, when you broke your usual commute to visit the Seaholm Trader Joe's for the first time in your logged history."
- Weave health/location/duration data into narrative context. Don't report metrics in isolation — connect them to what was happening.
- Reference specific events, people, and places by name so the text stays grounded.
- Plain text only — no markdown, no formatting, no bold, no italic.
- Second person ("you"), past tense. Tone: warm but precise, like a perceptive friend reflecting the day back. Never clinical, never saccharine.
- Never infer emotions, motivations, activities, or details that aren't in the source data. If the sources show heart rate and a few messages, write about heart rate and those messages — not about "an extended social outing" or "a productive morning" you imagined to fill space.
- Absence of data is not data. If the morning has no sources, do not write about the morning.

ELEVATED MOVES (use when the data supports them — required on rich days, optional on moderate days, skip on sparse days):
1. A cross-ontology causal beat with timestamps. Tie a specific signal in one ontology to a specific signal in another, in the same sentence. ("Today you lingered in the aisles for 52 minutes and swiped for $328.50.") This is the synthesis only this system can write — no single source could.
2. A dated temporal echo to a specific past moment. Not "your baseline" or "in recent weeks" — a specific date or named past event. ("The longest unscheduled stretch you've spent anywhere since the Friday before you signed the Holly Street lease in 2022.") Specific dates re-contextualize the present and prove memory.
3. A behavioral fingerprint stated as a generalization. ("The pace you run when you're metabolizing a decision, not winding down from one.") This earns the synthesis: it should read as something only someone who's watched the user for a year could say. Only assert a fingerprint when the historical data actually shows the pattern.
4. A quantified closer. End on a count or a date when you can. ("The first time in 184 days all three have been in the same twelve hours.") A number makes the synthesis falsifiable.

After the diary, output a single-line EPIGRAPH — a literary subtitle for the day, in the voice of an observing third-person narrator (Jane Austen, George Eliot, middle Dickens register). This sits at the top of the day page as a chapter-heading flourish.

EPIGRAPH rules:
- 5-14 words. Sentence case. No ending punctuation. No quotation marks.
- Draw concrete imagery from the day's actual events — a specific noun, place, or beat from the data.
- Prefer parallelism, juxtaposition, or a small observed aphorism.
- Gently ironic, implicit, observed from outside. Never saccharine, never explicit.
- Never use "I", "me", "today", "the day was…". Never use first-person constructions.
- Never a summary or list. It should feel like a line you'd remember.

Good epigraphs:
- sunlight on old tile has a way of proposing things
- a morning led by questions, an afternoon led by a house
- three cups of coffee, and then the hard conversation
- a text at lunch, and the afternoon had other plans
- the design review gave way to a backyard bigger than its photos
- some afternoons arrive with questions of their own

Bad epigraphs (do NOT produce anything like these):
- "A productive Friday" (cliche)
- "Today I worked and then saw a house" (explicit, first-person)
- "A day of mixed emotions" (abstract, saccharine)
- "Work in the morning, house viewing in the afternoon" (a summary, not an epigraph)

After the epigraph, assess the DATA QUALITY of the source material using the W6H framework (Who, Whom, What, When, Where, Why, How). Think like a journalist: how well does today's data answer each dimension?

Score each 1-5:
- 1 = no signal (dimension completely absent from sources)
- 2 = trace (a hint, but not enough to narrate)
- 3 = routine (typical weekday coverage)
- 4 = good (multiple corroborating sources)
- 5 = unusually rich (deep, multi-faceted coverage)

The "overall" score is your holistic judgment — NOT an average. A day with 5/5 Where but 1/1 everything else is still a 2.
The "note" is one sentence: what's strong, what's missing.

After data quality, output a JSON block with the day's events as a perfect 24-hour calendar. Use this exact output format:

[diary]
---EPIGRAPH---
[one-line epigraph]
---DATA_QUALITY---
{"coverage":{"who":3,"whom":2,"what":4,"when":5,"where":4,"why":1,"how":2},"overall":3,"note":"One sentence about coverage."}
---EVENTS---
[{"start": "HH:MM", "end": "HH:MM", "label": "Brief label", "summary": "1-3 factual sentences about what the source data shows."}]

WHAT AN EVENT IS:
An event is a contiguous block of time that the source data lets you classify. There are exactly two valid classifications:

1. **A definitively understood block** — the sources for this time window evidence a specific, nameable activity. The `label` is a short noun phrase (2-5 words) naming what the data shows. The `summary` is 1-3 plain factual sentences grounded in the actual data points (who/where/what was logged, durations, message counts, heart rate during the block, etc.). No inference, no mood, no motivation.

2. **Unknown** — the sources for this time window do not support a specific classification. The `label` is exactly "Unknown" and the `summary` is omitted (or empty). Do not invent a label like "Morning routine", "Rest", "Quiet time", "Sleep" to fill an unknown block.

Every event you emit must fall into one of these two buckets. There is no third "probably this" category.

SALIENCE FLOOR — what actually deserves to be an event:
An event must represent meaningful continuous activity, not scattered pings. Specifically:
- An event should cover a recognisable block — a calendar meeting, a workout, a commute leg, a sleep cycle, a phone call, a meal, an extended conversation — or a continuous stretch of activity (roughly ≥15 minutes of correlated source data: a voice recording, sustained app usage, a location dwell, a real back-and-forth messaging thread, etc.).
- A handful of sparse data points is NOT an event. A few text messages spread over an hour, one AI chat query, an isolated web visit, a single transaction, a lone notification — these are signals that exist *within* Unknown blocks. They should NOT be promoted to their own labeled event.
- When in doubt, prefer Unknown. A day with one or two clear events and the rest Unknown is more truthful than a day with five speculative event labels stretched over thin data. Truthful sparseness beats pleasant fabrication.

EVENTS rules:
- Events MUST cover the full 24 hours: first event starts at "00:00", last event ends at "24:00". No gaps, no overlaps.
- Use 24-hour time format (HH:MM). Events are contiguous — each event's end time equals the next event's start time.
- A label like "Morning routine", "Wake up", "Sleep", "Commute", "Work", "Relaxing", "Dinner" is only valid if the sources within that exact window evidence it (a sleep tracker logged a sleep cycle there, a calendar event covers it, location/transit data shows the commute, etc.). Otherwise the block is "Unknown".
- "Sleep" specifically requires sleep-tracking data (Apple Health, Oura, etc.) inside the window. Never infer sleep from absence of other data, and never guess wake-up times — clip the sleep event at the last sleep data point and mark the rest as "Unknown".
- It is perfectly fine — and common — for a sparse day to be mostly "Unknown" with only 1-3 understood events. That is the right answer.
- Event count scales with evidence. A rich day might have 10-16 events; a sparse day might have 3-5 events. Do not pad to reach a minimum.
- The `summary` field is the single most useful thing about an event. For understood events, it must reference the actual data points: "Three iMessages with Sarah about dinner plans, sent between 12:34 and 12:51. Heart rate stayed in the mid-70s." Not: "A pleasant exchange about dinner.""#;

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

    // Fallback: a true 24h UTC day when no/invalid timezone is available. (This
    // should rarely execute — home_timezone is seeded from the server's own
    // system clock; see docs/timezone-model.md.)
    let start = date.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let end = date
        .succ_opt()
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();
    (start.to_rfc3339(), end.to_rfc3339())
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Generate a daily summary from the day's data and save it as the autobiography.
pub async fn generate_day_summary(pool: &PgPool, date: NaiveDate) -> Result<WikiDay> {
    // 1. Gather structured sources (calendar, locations, transactions, chats, pages, etc.)
    let sources = get_day_sources(pool, date, None).await?;

    // 2. Compute date boundaries using the per-day "where the owner was" timezone
    //    (fixed at the day's start), falling back to the box's home_timezone.
    //    See docs/timezone-model.md.
    let home_tz = super::profile::get_timezone(pool)
        .await
        .unwrap_or(None)
        .unwrap_or_else(|| "UTC".to_string());
    let day_tz = crate::timezone::resolve_day_timezone(pool, date, &home_tz).await;
    let (start_str, end_str) = day_boundaries_utc(date, Some(&day_tz));
    let timezone: Option<String> = Some(day_tz);

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
    let tz_for_display: Option<Tz> = timezone.as_deref().and_then(|s| s.parse().ok());

    let tz_label = timezone.as_deref().unwrap_or("UTC");
    let mut prompt = format!(
        "Date: {}, {} ({} local time)\n\
         All timestamps in the source data below are in the user's local timezone ({}). \
         Emit event start/end times in the same local timezone.\n",
        day_of_week, date_display, tz_label, tz_label
    );

    // Group sources by type and build sections
    let grouped = group_sources_for_prompt(&sources, tz_for_display.as_ref());
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

    // 6. Call virtues-api
    let raw_response = call_virtues_api(pool, &prompt).await?;

    // 7. Parse response: extract diary text, epigraph, data quality, and structured events
    let parsed = parse_virtues_api_response(&raw_response);

    // 8. Store structured events (event creation + location extraction)
    let day_stub = get_or_create_day(pool, date).await?;
    if let Some(events) = parsed.events {
        store_structured_events(pool, &day_stub, date, timezone.as_deref(), &events).await;
    }

    // 9. Save autobiography + epigraph + data quality to wiki_days
    let day = update_day(
        pool,
        date,
        UpdateWikiDayRequest {
            autobiography: Some(parsed.diary),
            autobiography_sections: None,
            epigraph: parsed.epigraph,
            last_edited_by: Some("ai".to_string()),
            cover_image: None,
            start_timezone: timezone.clone(),
            data_quality: parsed
                .data_quality
                .as_deref()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()),
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

/// Group DaySources by type into prompt sections.
///
/// `tz` is the user's profile timezone — source timestamps are stored in UTC
/// but must be rendered in local time so the LLM emits matching local HH:MM
/// values (which `parse_hhmm_to_utc` will then re-localise on the way back in).
fn group_sources_for_prompt(sources: &[DaySource], tz: Option<&Tz>) -> Vec<PromptSection> {
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
            let time = match tz {
                Some(tz) => item.timestamp.with_timezone(tz).format("%H:%M").to_string(),
                None => item.timestamp.format("%H:%M").to_string(),
            };
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
    pool: &PgPool,
    start_str: &str,
    end_str: &str,
) -> Option<PromptSection> {
    let mut lines = Vec::new();

    // Heart rate
    let hr: Option<(Option<i32>, Option<i32>, Option<f64>, i32)> = sqlx::query_as(
        r#"
        SELECT MIN(bpm), MAX(bpm), ROUND(AVG(bpm)), COUNT(*)
        FROM data_health_heart_rate
        WHERE timestamp >= $1::timestamptz AND timestamp <= $2::timestamptz
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
        WHERE timestamp >= $1::timestamptz AND timestamp <= $2::timestamptz
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
    pool: &PgPool,
    start_str: &str,
    end_str: &str,
) -> Option<PromptSection> {
    use sqlx::Row;

    let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
        r#"
        SELECT from_name, body, channel, timestamp
        FROM data_communication_message
        WHERE timestamp >= $1::timestamptz AND timestamp <= $2::timestamptz
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
    pool: &PgPool,
    start_str: &str,
    end_str: &str,
) -> Option<PromptSection> {
    use sqlx::Row;

    let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
        r#"
        SELECT text, title, start_time
        FROM data_communication_transcription
        WHERE start_time >= $1::timestamptz AND start_time <= $2::timestamptz
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
    pool: &PgPool,
    start_str: &str,
    end_str: &str,
) -> Option<PromptSection> {
    use sqlx::Row;

    // Group by app_name, sum duration (end_time - start_time in seconds)
    let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
        r#"
        SELECT app_name,
               COUNT(*) as sessions,
               CAST(SUM(
                   EXTRACT(EPOCH FROM (end_time - start_time))
               ) AS BIGINT) as total_seconds
        FROM data_activity_app_usage
        WHERE start_time >= $1::timestamptz AND start_time <= $2::timestamptz
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
    pool: &PgPool,
    start_str: &str,
    end_str: &str,
) -> Option<PromptSection> {
    use sqlx::Row;

    let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
        r#"
        SELECT page_title, url, visit_duration_seconds
        FROM data_activity_web_browsing
        WHERE timestamp >= $1::timestamptz AND timestamp <= $2::timestamptz
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
    pool: &PgPool,
    start_str: &str,
    end_str: &str,
) -> Option<PromptSection> {
    use sqlx::Row;

    let mut lines = Vec::new();

    // Documents
    let docs: Vec<sqlx::postgres::PgRow> = sqlx::query(
        r#"
        SELECT title, document_type
        FROM data_content_document
        WHERE created_time >= $1::timestamptz AND created_time <= $2::timestamptz
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
    let convos: Vec<sqlx::postgres::PgRow> = sqlx::query(
        r#"
        SELECT conversation_id, model,
               MIN(CASE WHEN role = 'user' THEN content END) as first_user_msg
        FROM data_content_conversation
        WHERE timestamp >= $1::timestamptz AND timestamp <= $2::timestamptz
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
    pool: &PgPool,
    start_str: &str,
    end_str: &str,
) -> Option<PromptSection> {
    use sqlx::Row;

    let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
        r#"
        SELECT c.title, c.message_count,
               (SELECT content FROM app_chat_messages
                WHERE chat_id = c.id AND role = 'user'
                ORDER BY sequence_num ASC LIMIT 1) as first_msg
        FROM app_chats c
        WHERE c.created_at >= $1::timestamptz AND c.created_at <= $2::timestamptz
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
        let msg_count: i64 = row.try_get("message_count").unwrap_or(0);
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
    pool: &PgPool,
    start_str: &str,
    end_str: &str,
) -> Option<PromptSection> {
    use sqlx::Row;

    let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
        r#"
        SELECT title,
               CASE WHEN created_at >= $1::timestamptz AND created_at <= $2::timestamptz THEN 'created' ELSE 'edited' END as action
        FROM app_pages
        WHERE updated_at >= $1::timestamptz AND updated_at <= $2::timestamptz
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
    pool: &PgPool,
    start_str: &str,
    end_str: &str,
) -> Vec<(String, bool)> {
    let ontologies = registered_ontologies();
    let mut presence = Vec::with_capacity(ontologies.len());

    for ont in &ontologies {
        let ts_col = ont.timestamp_column;
        let table = ont.table_name;
        let query = format!(
            "SELECT COUNT(*) as cnt FROM {} WHERE {} >= $1::timestamptz AND {} <= $2::timestamptz LIMIT 1",
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

// ── virtues-api call ───────────────────────────────────────────────────────────

/// Call virtues-api for the summary generation
async fn call_virtues_api(pool: &PgPool, user_prompt: &str) -> Result<String> {
    let chat_model = crate::api::assistant_profile::get_chat_model(pool).await?;

    // api_key-auth path: the device's own key funds this background call,
    // with one auto-top-up-and-retry on a 402 wallet_empty.
    let client = crate::virtues_api::client::BearerClient::from_env(pool.clone())
        .with_purpose(crate::virtues_api::client::Purpose::System);
    let response = client
        .post_json(
            "/v1/ai/chat/completions",
            &serde_json::json!({
                "model": chat_model,
                "messages": [
                    {"role": "system", "content": SYSTEM_PROMPT},
                    {"role": "user", "content": user_prompt}
                ],
                "max_tokens": 1000,
                "temperature": 0.3
            }),
        )
        .await
        .map_err(|e| Error::Network(format!("virtues-api request failed: {e}")))?;

    if !response.is_success() {
        let error_msg = match response.status {
            402 => "Usage limit reached for summary generation".to_string(),
            429 => "Rate limited. Please try again later.".to_string(),
            _ => format!("virtues-api error {}: {}", response.status, response.body),
        };
        return Err(Error::ExternalApi(error_msg));
    }

    let response_json = response.body;

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

// ── Structured event parsing ─────────────────────────────────────────────────

/// LLM event parsed from virtues-api response
#[derive(Debug, serde::Deserialize)]
struct LlmEvent {
    start: String,
    end: String,
    label: String,
    /// 1-3 sentence factual description grounded in the source data. Optional
    /// because the model may omit it for Unknown blocks.
    #[serde(default)]
    summary: Option<String>,
}

/// Parsed day summary from LLM response
struct ParsedDaySummary {
    diary: String,
    epigraph: Option<String>,
    data_quality: Option<String>,
    events: Option<Vec<LlmEvent>>,
}

/// Split virtues-api response into diary text, epigraph, data quality, and optional events JSON.
/// Expected format:
///   [diary text]
///   ---EPIGRAPH---
///   [one-line epigraph]
///   ---DATA_QUALITY---
///   {"coverage":{...},"overall":3,"note":"..."}
///   ---EVENTS---
///   [JSON events]
///
/// All markers except the diary are optional. Handles markdown code fences around JSON.
fn parse_virtues_api_response(response: &str) -> ParsedDaySummary {
    // 1. Split off events JSON first (it's always at the end)
    let (before_events, events) = if let Some(idx) = response.find("---EVENTS---") {
        let before = &response[..idx];
        let mut events_str = response[idx + "---EVENTS---".len()..].trim();
        events_str = events_str
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        let parsed: Option<Vec<LlmEvent>> = serde_json::from_str(events_str)
            .map_err(|e| {
                tracing::warn!(error = %e, raw = events_str, "Failed to parse structured events from LLM");
                e
            })
            .ok();
        (before, parsed)
    } else {
        (response, None)
    };

    // 2. Split off data_quality from the remaining text
    let (before_quality, data_quality) = if let Some(idx) = before_events.find("---DATA_QUALITY---")
    {
        let before = &before_events[..idx];
        let mut dq_str = before_events[idx + "---DATA_QUALITY---".len()..].trim();
        dq_str = dq_str
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        // Validate it's parseable JSON, then store as raw string
        let validated: Option<String> = serde_json::from_str::<serde_json::Value>(dq_str)
            .map_err(|e| {
                tracing::warn!(error = %e, raw = dq_str, "Failed to parse data_quality from LLM");
                e
            })
            .ok()
            .map(|v| v.to_string());
        (before, validated)
    } else {
        (before_events, None)
    };

    // 3. Split off epigraph from the remaining text
    let (diary, epigraph) = if let Some(idx) = before_quality.find("---EPIGRAPH---") {
        let d = before_quality[..idx].trim().to_string();
        let e_raw = before_quality[idx + "---EPIGRAPH---".len()..].trim();
        // Epigraph is a single line — take only the first non-empty line
        let e = e_raw
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(|l| l.trim_matches(['"', '\'', '—', '–']).trim().to_string())
            .filter(|l| !l.is_empty());
        (d, e)
    } else {
        (before_quality.trim().to_string(), None)
    };

    ParsedDaySummary {
        diary,
        epigraph,
        data_quality,
        events,
    }
}

/// Store LLM-identified events as wiki_events rows.
///
/// Creates events in DB with location extraction. Embedding and novelty scoring
/// are handled separately by the dayline novelty pipeline (Phase 1).
async fn store_structured_events(
    pool: &PgPool,
    day: &WikiDay,
    date: NaiveDate,
    timezone: Option<&str>,
    events: &[LlmEvent],
) {
    // Clear previous auto events
    if let Err(e) = delete_auto_events_for_day(pool, day.id.clone()).await {
        tracing::warn!(error = %e, "Failed to delete existing auto events");
        return;
    }

    let tz: Option<Tz> = timezone.and_then(|s| s.parse().ok());

    // Backfill gaps to ensure perfect 24h coverage (00:00–24:00)
    let all_events = backfill_24h_events(events, date, tz.as_ref());

    let mut created_count = 0;

    for event in &all_events {
        let start_rfc = event.start_utc.to_rfc3339();
        let end_rfc = event.end_utc.to_rfc3339();

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
                source_ontologies: None,
                is_unknown: Some(event.is_unknown),
                is_transit: Some(false),
                is_user_added: Some(false),
                event_summary: event.summary.clone(),
            },
        )
        .await;

        match created {
            Ok(_) => created_count += 1,
            Err(e) => {
                tracing::warn!(error = %e, label = event.label, "Failed to create temporal event");
            }
        }
    }

    tracing::info!(
        date = %date,
        event_count = all_events.len(),
        created_count,
        "Stored structured events"
    );
}

/// Extract the primary location for an event's time range from location_visit data.
/// Returns the place name with the longest visit duration, or None if no location data.
async fn extract_event_location(pool: &PgPool, start: &str, end: &str) -> Option<String> {
    use sqlx::Row;
    // `data_location_visit.place_name` is never populated by entity resolution —
    // the resolved name lives in `wiki_places`, linked via `wiki_entity_refs`
    // (same shape the timeline reader uses). JOIN through to get the real name;
    // selecting the visit's own `place_name` column always returned NULL.
    let row: Option<sqlx::postgres::PgRow> = sqlx::query(
        "SELECT p.name AS place_name \
         FROM data_location_visit v \
         JOIN wiki_entity_refs er \
           ON er.source_table = 'data_location_visit' \
          AND er.source_id = v.id \
          AND er.entity_type = 'place' \
         JOIN wiki_places p ON p.id = er.entity_id \
         WHERE v.arrival_time >= $1::timestamptz AND v.arrival_time <= $2::timestamptz \
         ORDER BY v.duration_minutes DESC LIMIT 1",
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
    summary: Option<String>,
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
            // Treat a literal "Unknown" label as an unknown block even when the
            // LLM emits it explicitly — keeps downstream classification honest.
            let is_unknown = e.label.eq_ignore_ascii_case("unknown");
            Some(ResolvedEvent {
                start_utc: start.max(day_start),
                end_utc: end.min(day_end),
                label: e.label.clone(),
                summary: e.summary.clone().filter(|s| !s.trim().is_empty()),
                is_unknown,
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
                summary: None,
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
            summary: None,
            is_unknown: true,
        });
    }

    // Merge consecutive Unknown blocks into one — keeps the timeline cleaner
    // when the LLM emits its own "Unknown" event adjacent to a backfilled gap.
    let mut merged: Vec<ResolvedEvent> = Vec::with_capacity(result.len());
    for ev in result {
        if let Some(last) = merged.last_mut() {
            if last.is_unknown && ev.is_unknown && last.end_utc == ev.start_utc {
                last.end_utc = ev.end_utc;
                continue;
            }
        }
        merged.push(ev);
    }
    merged
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


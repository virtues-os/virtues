//! Context builder for the dayline hourly action.
//!
//! Queries processed ontology tables for the current hour and formats
//! structured text for injection into the action's system prompt.
//! Uses location_visit (not location_point), aggregated data (not raw readings).

use chrono::{DateTime, Duration, Utc};
use sqlx::{Row, PgPool};

/// Build the context string for the hourly dayline action.
///
/// Queries ontology tables for records in the time window, formats them
/// as structured text, and includes today's existing events for continuity.
pub async fn build_hourly_context(
    pool: &PgPool,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
) -> String {
    let start = window_start.to_rfc3339();
    let end = window_end.to_rfc3339();
    let today = window_start.format("%Y-%m-%d").to_string();

    let mut sections: Vec<String> = Vec::new();

    sections.push(format!(
        "Time window: {} to {}",
        window_start.format("%H:%M"),
        window_end.format("%H:%M")
    ));

    // Calendar events. Columns are `location_name` and `attendee_identifiers`
    // (JSONB) — the prior query named them `location`/`attendees`, which don't
    // exist, so it errored and the calendar section was always empty. Decode the
    // timestamps as `DateTime<Utc>` directly (the prior `try_get::<String>` on a
    // timestamptz column would also have failed).
    if let Ok(rows) = sqlx::query(
        r#"SELECT title, start_time, end_time, location_name, attendee_identifiers
           FROM data_calendar_event
           WHERE start_time >= $1 AND start_time < $2
           ORDER BY start_time"#,
    )
    .bind(&start)
    .bind(&end)
    .fetch_all(pool)
    .await
    {
        let items: Vec<String> = rows.iter().filter_map(|r| {
            let title: String = r.try_get("title").ok()?;
            let start: DateTime<Utc> = r.try_get("start_time").ok()?;
            let end: DateTime<Utc> = r.try_get("end_time").ok()?;
            let loc: Option<String> = r.try_get("location_name").ok().flatten();
            let attendees: Option<serde_json::Value> = r.try_get("attendee_identifiers").ok();
            let mut s = format!("- {} ({} to {})", title, start.format("%H:%M"), end.format("%H:%M"));
            if let Some(l) = loc {
                if !l.is_empty() { s.push_str(&format!(" at {l}")); }
            }
            if let Some(serde_json::Value::Array(arr)) = attendees {
                let names: Vec<String> =
                    arr.iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
                if !names.is_empty() { s.push_str(&format!(" with {}", names.join(", "))); }
            }
            Some(s)
        }).collect();
        if !items.is_empty() {
            sections.push(format!("CALENDAR:\n{}", items.join("\n")));
        }
    }

    // Location visits. The resolved place name lives in `wiki_places` (linked
    // via `wiki_entity_refs`); `data_location_visit.place_name` is never
    // populated, so the prior query returned NULL names and the section was
    // empty. JOIN through to the place, and decode the timestamp as DateTime.
    if let Ok(rows) = sqlx::query(
        r#"SELECT p.name AS place_name, v.arrival_time, v.duration_minutes
           FROM data_location_visit v
           JOIN wiki_entity_refs er
             ON er.source_table = 'data_location_visit'
            AND er.source_id = v.id
            AND er.entity_type = 'place'
           JOIN wiki_places p ON p.id = er.entity_id
           WHERE v.arrival_time >= $1 AND v.arrival_time < $2
           ORDER BY v.arrival_time"#,
    )
    .bind(&start)
    .bind(&end)
    .fetch_all(pool)
    .await
    {
        let items: Vec<String> = rows.iter().filter_map(|r| {
            let name: String = r.try_get::<Option<String>, _>("place_name").ok().flatten()?;
            let arrival: DateTime<Utc> = r.try_get("arrival_time").ok()?;
            let dur: Option<i32> = r.try_get("duration_minutes").ok().flatten();
            let mut s = format!("- {} (arrived {})", name, arrival.format("%H:%M"));
            if let Some(d) = dur { s.push_str(&format!(", {}min", d)); }
            Some(s)
        }).collect();
        if !items.is_empty() {
            sections.push(format!("LOCATIONS:\n{}", items.join("\n")));
        }
    }

    // App usage
    if let Ok(rows) = sqlx::query(
        r#"SELECT app_name, duration_minutes, window_title
           FROM data_activity_app_usage
           WHERE start_time >= $1 AND start_time < $2
           ORDER BY duration_minutes DESC
           LIMIT 10"#,
    )
    .bind(&start)
    .bind(&end)
    .fetch_all(pool)
    .await
    {
        let items: Vec<String> = rows.iter().filter_map(|r| {
            let app: String = r.try_get("app_name").ok()?;
            let dur: Option<i32> = r.try_get("duration_minutes").ok().flatten();
            let window: Option<String> = r.try_get("window_title").ok().flatten();
            let mut s = format!("- {}", app);
            if let Some(d) = dur { s.push_str(&format!(" ({}min)", d)); }
            if let Some(w) = window {
                if !w.is_empty() { s.push_str(&format!(": {}", truncate(&w, 60))); }
            }
            Some(s)
        }).collect();
        if !items.is_empty() {
            sections.push(format!("APPS:\n{}", items.join("\n")));
        }
    }

    // Messages
    if let Ok(rows) = sqlx::query(
        r#"SELECT from_name, channel, body
           FROM data_communication_message
           WHERE timestamp >= $1 AND timestamp < $2
           ORDER BY timestamp
           LIMIT 15"#,
    )
    .bind(&start)
    .bind(&end)
    .fetch_all(pool)
    .await
    {
        let items: Vec<String> = rows.iter().filter_map(|r| {
            let sender: Option<String> = r.try_get("from_name").ok().flatten();
            let channel: Option<String> = r.try_get("channel").ok().flatten();
            let body: Option<String> = r.try_get("body").ok().flatten();
            let who = sender.or(channel).unwrap_or_else(|| "unknown".to_string());
            let preview = body.map(|c| truncate(&c, 80)).unwrap_or_default();
            Some(format!("- {}: {}", who, preview))
        }).collect();
        if !items.is_empty() {
            sections.push(format!("MESSAGES:\n{}", items.join("\n")));
        }
    }

    // Voice transcriptions
    if let Ok(rows) = sqlx::query(
        r#"SELECT text, start_time, duration_seconds
           FROM data_communication_transcription
           WHERE start_time >= $1 AND start_time < $2
           ORDER BY start_time
           LIMIT 5"#,
    )
    .bind(&start)
    .bind(&end)
    .fetch_all(pool)
    .await
    {
        let items: Vec<String> = rows.iter().filter_map(|r| {
            let text: Option<String> = r.try_get("text").ok()?;
            let text = text?;
            if text.is_empty() { return None; }
            let dur: Option<f64> = r.try_get("duration_seconds").ok().flatten();
            let mut s = format!("- {}", truncate(&text, 100));
            if let Some(d) = dur { s.push_str(&format!(" ({:.0}s)", d)); }
            Some(s)
        }).collect();
        if !items.is_empty() {
            sections.push(format!("TRANSCRIPTIONS:\n{}", items.join("\n")));
        }
    }

    // Health summary (aggregated, not per-reading)
    if let Ok(row) = sqlx::query(
        "SELECT COUNT(*) as cnt, AVG(bpm) as avg_bpm FROM data_health_heart_rate WHERE timestamp >= $1 AND timestamp < $2",
    )
    .bind(&start)
    .bind(&end)
    .fetch_optional(pool)
    .await
    {
        if let Some(r) = row {
            let cnt: i32 = r.try_get("cnt").unwrap_or(0);
            let avg: Option<f64> = r.try_get("avg_bpm").ok().flatten();
            if cnt > 0 {
                if let Some(a) = avg {
                    sections.push(format!("HEALTH: avg HR {:.0} bpm ({} readings)", a, cnt));
                }
            }
        }
    }

    if let Ok(row) = sqlx::query(
        "SELECT SUM(step_count) as total FROM data_health_steps WHERE timestamp >= $1 AND timestamp < $2",
    )
    .bind(&start)
    .bind(&end)
    .fetch_optional(pool)
    .await
    {
        if let Some(r) = row {
            let total: Option<i64> = r.try_get("total").ok().flatten();
            if let Some(t) = total {
                if t > 0 {
                    sections.push(format!("STEPS: {} this hour", t));
                }
            }
        }
    }

    // Web browsing
    if let Ok(rows) = sqlx::query(
        r#"SELECT url, page_title, visit_duration_seconds
           FROM data_activity_web_browsing
           WHERE timestamp >= $1 AND timestamp < $2
           ORDER BY visit_duration_seconds DESC
           LIMIT 5"#,
    )
    .bind(&start)
    .bind(&end)
    .fetch_all(pool)
    .await
    {
        let items: Vec<String> = rows.iter().filter_map(|r| {
            let title: Option<String> = r.try_get("page_title").ok().flatten();
            let dur: Option<i32> = r.try_get("visit_duration_seconds").ok().flatten();
            let display = title.unwrap_or_else(|| {
                r.try_get::<Option<String>, _>("url").ok().flatten().unwrap_or_default()
            });
            if display.is_empty() { return None; }
            let mut s = format!("- {}", truncate(&display, 60));
            if let Some(d) = dur { s.push_str(&format!(" ({}s)", d)); }
            Some(s)
        }).collect();
        if !items.is_empty() {
            sections.push(format!("WEB:\n{}", items.join("\n")));
        }
    }

    // Listening (Spotify etc)
    if let Ok(rows) = sqlx::query(
        r#"SELECT track_name, artist_name
           FROM data_activity_listening
           WHERE played_at >= $1 AND played_at < $2
           ORDER BY played_at
           LIMIT 5"#,
    )
    .bind(&start)
    .bind(&end)
    .fetch_all(pool)
    .await
    {
        let items: Vec<String> = rows.iter().filter_map(|r| {
            let track: String = r.try_get("track_name").ok()?;
            let artist: Option<String> = r.try_get("artist_name").ok().flatten();
            if let Some(a) = artist {
                Some(format!("- {} by {}", track, a))
            } else {
                Some(format!("- {}", track))
            }
        }).collect();
        if !items.is_empty() {
            sections.push(format!("LISTENING:\n{}", items.join("\n")));
        }
    }

    // Today's events so far (for continuity — the agent needs to know what it already created)
    if let Ok(rows) = sqlx::query(
        r#"SELECT e.id, e.start_time, e.end_time, e.auto_label, e.event_summary, e.agent_action
           FROM wiki_events e
           JOIN wiki_days d ON e.day_id = d.id
           WHERE d.date = $1
             AND e.is_unknown = FALSE
             AND e.user_hidden = FALSE
           ORDER BY e.start_time"#,
    )
    .bind(&today)
    .fetch_all(pool)
    .await
    {
        let items: Vec<String> = rows.iter().filter_map(|r| {
            let id: String = r.try_get("id").ok()?;
            let start: String = r.try_get("start_time").ok()?;
            let end: String = r.try_get("end_time").ok()?;
            let label: Option<String> = r.try_get("auto_label").ok().flatten();
            let summary: Option<String> = r.try_get("event_summary").ok().flatten();
            let action: Option<String> = r.try_get("agent_action").ok().flatten();
            let display = summary.or(label).unwrap_or_else(|| "Event".to_string());
            Some(format!(
                "- [{}] {} to {}: {} ({})",
                id,
                time_hhmm(&start),
                time_hhmm(&end),
                display,
                action.unwrap_or_else(|| "legacy".to_string())
            ))
        }).collect();
        if !items.is_empty() {
            sections.push(format!("TODAY'S EVENTS SO FAR:\n{}", items.join("\n")));
        }
    }

    // User profile (brief)
    if let Ok(row) = sqlx::query(
        "SELECT preferred_name, occupation, employer, timezone FROM app_user_profile LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    {
        if let Some(r) = row {
            let name: Option<String> = r.try_get("preferred_name").ok().flatten();
            let occ: Option<String> = r.try_get("occupation").ok().flatten();
            let emp: Option<String> = r.try_get("employer").ok().flatten();
            let tz: Option<String> = r.try_get("timezone").ok().flatten();
            let mut parts: Vec<String> = Vec::new();
            if let Some(n) = name { parts.push(n); }
            if let Some(o) = occ {
                if let Some(e) = emp {
                    parts.push(format!("{} at {}", o, e));
                } else {
                    parts.push(o);
                }
            }
            if let Some(t) = tz { parts.push(format!("({})", t)); }
            if !parts.is_empty() {
                sections.push(format!("USER: {}", parts.join(", ")));
            }
        }
    }

    if sections.len() <= 1 {
        // Only the time window header — no actual data
        return String::new();
    }

    sections.join("\n\n")
}

/// Build context for the EOD action — full day's data.
pub async fn build_eod_context(
    pool: &PgPool,
    date: chrono::NaiveDate,
) -> String {
    let start = date.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let end = (date + Duration::days(1)).and_hms_opt(0, 0, 0).unwrap().and_utc();

    // Reuse the hourly builder with a full-day window
    let mut context = build_hourly_context(pool, start, end).await;

    if context.is_empty() {
        context = format!("Date: {}. No ontology data found for this day.", date);
    } else {
        context = format!("Full day context for {}:\n\n{}", date, context);
    }

    context
}

/// Extract HH:MM from an ISO 8601 timestamp (e.g., "2026-04-01T14:30:00Z" → "14:30").
/// Returns "??:??" if the string is too short or malformed.
fn time_hhmm(ts: &str) -> &str {
    if ts.len() >= 16 && ts.as_bytes().get(10) == Some(&b'T') {
        &ts[11..16]
    } else {
        "??:??"
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        // Find the last valid char boundary at or before `max` to avoid
        // panicking on multi-byte UTF-8 (emoji, CJK, accented chars).
        let end = s.char_indices()
            .map(|(i, c)| i + c.len_utf8())
            .take_while(|&i| i <= max)
            .last()
            .unwrap_or(0);
        format!("{}...", &s[..end])
    }
}

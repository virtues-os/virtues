//! iOS EventKit → `data_calendar_event` transform.
//!
//! Ported from `core/src/sources/ios/eventkit/transform.rs`. Only processes
//! records with `record_type = "event"` — reminders are skipped.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;
use virtues_helpers::dedup::{build_batch_insert_query, BATCH_SIZE};
use virtues_helpers::ios::{stream_id_or_new, EVENTKIT_STREAM_TABLE, IOS_PROVIDER};

struct CalendarRow {
    id: String,
    title: String,
    description: Option<String>,
    calendar_name: Option<String>,
    location_name: Option<String>,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    is_all_day: bool,
    external_id: Option<String>,
    external_url: Option<String>,
    stream_id: String,
    metadata: Value,
}

pub async fn write_events(db: &PgPool, records: &[Value]) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let mut pending: Vec<CalendarRow> = Vec::new();
    let mut written = 0;

    for record in records {
        // Skip reminders — they need a different ontology
        let record_type = record
            .get("record_type")
            .and_then(|v| v.as_str())
            .unwrap_or("event");
        if record_type != "event" {
            continue;
        }

        let title = record
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if title.is_empty() {
            continue;
        }

        let calendar_name = record
            .get("calendarTitle")
            .and_then(|v| v.as_str())
            .map(String::from);
        let location_name = record
            .get("location")
            .and_then(|v| v.as_str())
            .map(String::from);
        let description = record.get("notes").and_then(|v| v.as_str()).map(String::from);

        let start_time = record
            .get("startDate")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<DateTime<Utc>>().ok())
            .unwrap_or_else(Utc::now);

        let end_time = record
            .get("endDate")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<DateTime<Utc>>().ok())
            .unwrap_or_else(Utc::now);

        let is_all_day = record.get("isAllDay").and_then(|v| v.as_bool()).unwrap_or(false);
        let external_id = record.get("id").and_then(|v| v.as_str()).map(String::from);
        let external_url = record.get("url").and_then(|v| v.as_str()).map(String::from);
        let stream_id = stream_id_or_new(record);

        let metadata = serde_json::json!({
            "calendar_id": record.get("calendarId"),
            "last_modified": record.get("lastModified"),
        });

        pending.push(CalendarRow {
            id: Uuid::new_v4().to_string(),
            title,
            description,
            calendar_name,
            location_name,
            start_time,
            end_time,
            is_all_day,
            external_id,
            external_url,
            stream_id,
            metadata,
        });

        if pending.len() >= BATCH_SIZE {
            written += flush_events(db, &pending).await?;
            pending.clear();
        }
    }

    if !pending.is_empty() {
        written += flush_events(db, &pending).await?;
    }

    Ok(written)
}

async fn flush_events(db: &PgPool, records: &[CalendarRow]) -> Result<usize> {
    let sql = build_batch_insert_query(
        "data_calendar_event",
        &[
            "id",
            "title",
            "description",
            "calendar_name",
            "location_name",
            "start_time",
            "end_time",
            "is_all_day",
            "external_id",
            "external_url",
            "source_stream_id",
            "source_table",
            "source_provider",
            "metadata",
        ],
        "source_stream_id",
        records.len(),
    );

    let mut q = sqlx::query(&sql);
    for rec in records {
        q = q
            .bind(&rec.id)
            .bind(&rec.title)
            .bind(&rec.description)
            .bind(&rec.calendar_name)
            .bind(&rec.location_name)
            .bind(&rec.start_time)
            .bind(&rec.end_time)
            .bind(rec.is_all_day)
            .bind(&rec.external_id)
            .bind(&rec.external_url)
            .bind(&rec.stream_id)
            .bind(EVENTKIT_STREAM_TABLE)
            .bind(IOS_PROVIDER)
            .bind(&rec.metadata);
    }
    let result = q.execute(db).await?;
    Ok(result.rows_affected() as usize)
}

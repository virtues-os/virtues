//! Google Calendar events → `data_calendar_event` transform.
//!
//! Adapted from `core/src/sources/google/calendar/transform.rs`. Maps a Google
//! Calendar API event resource to the same `data_calendar_event` schema iOS
//! EventKit writes to. Cancelled events are tracked via `deleted_at_source`.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;
use virtues_helpers::dedup::{build_batch_insert_query, BATCH_SIZE};

#[allow(clippy::type_complexity)]
type EventRow = (
    String,                    // id
    String,                    // title
    Option<String>,            // description
    Option<String>,            // calendar_name (calendar id)
    Option<String>,            // location_name
    DateTime<Utc>,             // start_time
    DateTime<Utc>,             // end_time
    bool,                      // is_all_day
    Option<String>,            // external_id
    Option<String>,            // external_url (htmlLink)
    String,                    // source_stream_id
    Value,                     // metadata
    Option<DateTime<Utc>>,     // deleted_at_source
);

/// `events` is the array from `https://www.googleapis.com/calendar/v3/calendars/{cal_id}/events`.
/// `calendar_id` is the calendar this batch came from (used for `calendar_name`).
pub async fn write_events(
    db: &PgPool,
    calendar_id: &str,
    events: &[Value],
) -> Result<usize> {
    let mut pending: Vec<EventRow> = Vec::new();
    let mut written = 0;

    for event in events {
        let google_id = event.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if google_id.is_empty() {
            continue;
        }

        // Cancelled events: parse `start`/`end` may be missing. Skip these
        // unless we want to mark them deleted; for now we just store the
        // cancellation in metadata + set deleted_at_source.
        let status = event.get("status").and_then(|v| v.as_str()).unwrap_or("");
        let is_cancelled = status == "cancelled";

        let title = event
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("(no title)")
            .to_string();
        let description = event
            .get("description")
            .and_then(|v| v.as_str())
            .map(String::from);
        let location_name = event
            .get("location")
            .and_then(|v| v.as_str())
            .map(String::from);

        let (start_time, end_time, is_all_day) = match parse_event_times(event) {
            Some(t) => t,
            None if is_cancelled => {
                // Cancelled events without times: use the updated timestamp as
                // both start and end so the row is well-formed.
                let updated = event
                    .get("updated")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<DateTime<Utc>>().ok())
                    .unwrap_or_else(Utc::now);
                (updated, updated, false)
            }
            None => continue,
        };

        let external_url = event.get("htmlLink").and_then(|v| v.as_str()).map(String::from);

        let metadata = serde_json::json!({
            "google_event_id": google_id,
            "calendar_id": calendar_id,
            "status": status,
            "organizer": event.get("organizer"),
            "attendees": event.get("attendees"),
            "creator": event.get("creator"),
            "recurringEventId": event.get("recurringEventId"),
            "recurrence": event.get("recurrence"),
            "hangoutLink": event.get("hangoutLink"),
            "conferenceData": event.get("conferenceData"),
        });

        let deleted_at_source = if is_cancelled {
            event
                .get("updated")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<DateTime<Utc>>().ok())
                .or_else(|| Some(Utc::now()))
        } else {
            None
        };

        let id = Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("google:calendar:{calendar_id}:{google_id}").as_bytes(),
        )
        .to_string();

        pending.push((
            id,
            title,
            description,
            Some(calendar_id.to_string()),
            location_name,
            start_time,
            end_time,
            is_all_day,
            Some(google_id.to_string()),
            external_url,
            format!("{calendar_id}:{google_id}"),
            metadata,
            deleted_at_source,
        ));

        if pending.len() >= BATCH_SIZE {
            written += flush(db, &pending).await?;
            pending.clear();
        }
    }
    if !pending.is_empty() {
        written += flush(db, &pending).await?;
    }
    Ok(written)
}

fn parse_event_times(event: &Value) -> Option<(DateTime<Utc>, DateTime<Utc>, bool)> {
    let start = event.get("start")?;
    let end = event.get("end")?;

    // dateTime present → timed event; date-only → all-day.
    if let (Some(s), Some(e)) = (
        start.get("dateTime").and_then(|v| v.as_str()),
        end.get("dateTime").and_then(|v| v.as_str()),
    ) {
        let st = s.parse::<DateTime<Utc>>().ok()?;
        let et = e.parse::<DateTime<Utc>>().ok()?;
        return Some((st, et, false));
    }

    if let (Some(s), Some(e)) = (
        start.get("date").and_then(|v| v.as_str()),
        end.get("date").and_then(|v| v.as_str()),
    ) {
        let st = format!("{s}T00:00:00Z").parse::<DateTime<Utc>>().ok()?;
        let et = format!("{e}T00:00:00Z").parse::<DateTime<Utc>>().ok()?;
        return Some((st, et, true));
    }

    None
}

async fn flush(db: &PgPool, records: &[EventRow]) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }
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
            "deleted_at_source",
        ],
        "source_stream_id",
        records.len(),
    );

    let mut q = sqlx::query(&sql);
    for r in records {
        q = q
            .bind(&r.0)
            .bind(&r.1)
            .bind(&r.2)
            .bind(&r.3)
            .bind(&r.4)
            .bind(r.5)
            .bind(r.6)
            .bind(r.7)
            .bind(&r.8)
            .bind(&r.9)
            .bind(&r.10)
            .bind("google_calendar")
            .bind("google")
            .bind(&r.11)
            .bind(r.12);
    }
    let result = q.execute(db).await?;
    Ok(result.rows_affected() as usize)
}

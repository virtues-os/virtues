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
use virtues_helpers::dedup::{build_batch_upsert_query, BATCH_SIZE};

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
    String,                    // status (confirmed | tentative | cancelled)
    Option<String>,            // calendar_access_role (owner | writer | reader | freeBusyReader)
    Option<String>,            // response_status — the OWNER's RSVP, not anyone else's
    Value,                     // attendee_identifiers (jsonb array of emails)
    Option<String>,            // organizer_identifier
);

/// Pull the owner's own RSVP out of the attendee list. Google flags exactly one
/// attendee `self: true`; everyone else's answer is none of this column's business.
///
/// Returns None far more often than not, and that is correct: a self-created
/// event has no attendee list at all, so there is no RSVP to read. None means
/// "not asked", never "did not go".
fn self_response_status(event: &Value) -> Option<String> {
    event
        .get("attendees")?
        .as_array()?
        .iter()
        .find(|a| a.get("self").and_then(|v| v.as_bool()).unwrap_or(false))
        .and_then(|a| a.get("responseStatus"))
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// Everyone invited, by email (falling back to display name). Rooms and
/// equipment come back in the same array flagged `resource: true` — they are not
/// people and would corrupt any "who was I with" reading of this column.
fn attendee_identifiers(event: &Value) -> Value {
    let list: Vec<String> = event
        .get("attendees")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|a| !a.get("resource").and_then(|v| v.as_bool()).unwrap_or(false))
                .filter_map(|a| {
                    a.get("email")
                        .or_else(|| a.get("displayName"))
                        .and_then(|v| v.as_str())
                        .map(String::from)
                })
                .collect()
        })
        .unwrap_or_default();
    serde_json::json!(list)
}

/// `events` is the array from `https://www.googleapis.com/calendar/v3/calendars/{cal_id}/events`.
/// `calendar_id` is the calendar this batch came from (used for `calendar_name`).
/// `access_role` is that calendar's role from the calendarList — the flag that
/// says whether these are the owner's own plans or a calendar they subscribe to.
///
/// Returns the `source_stream_id` of every row upserted. The caller needs the
/// keys, not just a count: on a full resync, the events Google did *not* send
/// are the ones deleted while we weren't listening, and absence is only
/// legible against the set that did arrive.
pub async fn write_events(
    db: &PgPool,
    calendar_id: &str,
    access_role: Option<&str>,
    events: &[Value],
) -> Result<Vec<String>> {
    let mut pending: Vec<EventRow> = Vec::new();
    let mut written: Vec<String> = Vec::new();

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
            status.to_string(),
            access_role.map(String::from),
            self_response_status(event),
            attendee_identifiers(event),
            event
                .get("organizer")
                .and_then(|o| o.get("email").or_else(|| o.get("displayName")))
                .and_then(|v| v.as_str())
                .map(String::from),
        ));

        if pending.len() >= BATCH_SIZE {
            written.extend(flush(db, &pending).await?);
            pending.clear();
        }
    }
    if !pending.is_empty() {
        written.extend(flush(db, &pending).await?);
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

async fn flush(db: &PgPool, records: &[EventRow]) -> Result<Vec<String>> {
    if records.is_empty() {
        return Ok(Vec::new());
    }
    // UPSERT, not DO NOTHING. A calendar event is mutable by nature — it gets
    // renamed, rescheduled, and cancelled after we first see it — and the sync
    // key is `{calendar_id}:{google_id}`, stable across all of that. Under
    // DO NOTHING every correction Google sent was discarded on arrival, so a
    // meeting cancelled after its first sync stayed on the calendar forever.
    let sql = build_batch_upsert_query(
        "data_calendar_event",
        &[
            "id",
            "title",
            "description",
            "calendar_name",
            "location_name",
            "started_at",
            "ended_at",
            "is_all_day",
            "external_id",
            "external_url",
            "source_stream_id",
            "source_table",
            "source_provider",
            "metadata",
            "deleted_at_source",
            "status",
            "calendar_access_role",
            "response_status",
            "attendee_identifiers",
            "organizer_identifier",
        ],
        "source_stream_id",
        &[
            "title",
            "description",
            "location_name",
            "started_at",
            "ended_at",
            "is_all_day",
            "external_url",
            "metadata",
            "deleted_at_source",
            "status",
            // All four are mutable and must be re-projected on every sync, for the
            // same reason the row is an UPSERT at all: an RSVP flips from
            // needsAction to declined days after the invite lands, attendees are
            // added and removed, and a calendar can be shared or unshared, which
            // moves it between `owner` and `reader`. Leaving them out of the
            // update list would freeze the first answer we ever saw — and for
            // `response_status` the first answer is almost always the useless one.
            "calendar_access_role",
            "response_status",
            "attendee_identifiers",
            "organizer_identifier",
        ],
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
            .bind(r.12)
            .bind(&r.13)
            .bind(&r.14)
            .bind(&r.15)
            .bind(&r.16)
            .bind(&r.17);
    }
    // The upsert builder appends RETURNING, so drain the rows. This is
    // ON CONFLICT DO UPDATE, so every record sent is written — the returned
    // count is a sanity check, not a filter, and the keys are what the caller
    // needs (they mark these events as "still on the calendar").
    let returned = q.fetch_all(db).await?.len();
    if returned != records.len() {
        tracing::warn!(
            sent = records.len(),
            returned,
            "calendar upsert returned fewer rows than sent"
        );
    }
    Ok(records.iter().map(|r| r.10.clone()).collect())
}

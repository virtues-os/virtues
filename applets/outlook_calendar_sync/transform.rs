//! Outlook calendar events from Microsoft Graph → `data_calendar_event`.
//!
//! Same table and columns as the Google Calendar transform, with Graph's
//! values mapped onto the vocabulary that transform writes: `status` is
//! `confirmed` or `cancelled`, `response_status` is Google's RSVP words, and
//! `calendar_access_role` is `owner`, `writer` or `reader`.

use anyhow::Result;
use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;
use virtues_helpers::dedup::{build_batch_upsert_changed_query, dedup_refs_keep_last, BATCH_SIZE};

const SOURCE_TABLE: &str = "outlook_calendar";
const SOURCE_PROVIDER: &str = "outlook";

/// The calendar a page of events came from.
#[derive(Debug, Clone)]
pub struct Calendar {
    pub id: String,
    pub name: Option<String>,
    pub access_role: &'static str,
}

impl Calendar {
    /// From one entry of Graph's `/me/calendars`.
    pub fn from_graph(cal: &Value, user_email: &str) -> Option<Calendar> {
        Some(Calendar {
            id: cal.get("id")?.as_str()?.to_string(),
            name: cal.get("name").and_then(Value::as_str).map(String::from),
            access_role: access_role(cal, user_email),
        })
    }
}

/// Graph's calendar permissions in Google's words. Only the owner of a
/// calendar can share it, so `canShare` marks the owner when the owner's
/// address is missing or written differently.
pub fn access_role(cal: &Value, user_email: &str) -> &'static str {
    let owner = cal.pointer("/owner/address").and_then(Value::as_str).unwrap_or("");
    let owns = (!user_email.is_empty() && owner.eq_ignore_ascii_case(user_email))
        || cal.get("canShare").and_then(Value::as_bool).unwrap_or(false);
    if owns {
        "owner"
    } else if cal.get("canEdit").and_then(Value::as_bool).unwrap_or(false) {
        "writer"
    } else {
        "reader"
    }
}

/// One `data_calendar_event` row.
#[derive(Debug, Clone, PartialEq)]
pub struct EventRow {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub calendar_name: Option<String>,
    pub location_name: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub is_all_day: bool,
    pub external_id: String,
    pub external_url: Option<String>,
    pub source_stream_id: String,
    pub metadata: Value,
    pub deleted_at_source: Option<DateTime<Utc>>,
    pub status: &'static str,
    pub calendar_access_role: &'static str,
    pub response_status: Option<&'static str>,
    pub attendee_identifiers: Value,
    pub organizer_identifier: Option<String>,
}

/// What became of one Graph event.
#[derive(Debug, PartialEq)]
pub enum Shaped {
    Row(Box<EventRow>),
    /// Its start or end names a time zone this applet cannot read.
    UnknownZone,
    /// No id, or no usable start and end.
    Invalid,
}

/// What one page did.
#[derive(Debug, Default)]
pub struct Written {
    /// `source_stream_id` of every event Graph returned, changed or not.
    pub seen: Vec<String>,
    /// Events inserted or changed.
    pub changed: usize,
    /// Events skipped for a time zone this applet cannot read.
    pub unknown_zone: usize,
}

/// Upsert one page of a calendar's events.
pub async fn write_events(db: &PgPool, cal: &Calendar, events: &[Value]) -> Result<Written> {
    let mut written = Written::default();
    let mut rows: Vec<EventRow> = Vec::new();
    for event in events {
        match shape_event(event, cal) {
            Shaped::Row(row) => rows.push(*row),
            Shaped::UnknownZone => {
                written.unknown_zone += 1;
                // Graph still lists it, so its stored row must not be
                // tombstoned as gone.
                if let Some(id) = event.get("id").and_then(Value::as_str) {
                    written.seen.push(format!("{}:{id}", cal.id));
                }
            }
            Shaped::Invalid => {}
        }
    }
    // One statement cannot update the same row twice.
    let rows: Vec<&EventRow> = dedup_refs_keep_last(&rows, |r| r.source_stream_id.clone());
    for chunk in rows.chunks(BATCH_SIZE) {
        written.changed += flush(db, chunk).await?;
        written.seen.extend(chunk.iter().map(|r| r.source_stream_id.clone()));
    }
    Ok(written)
}

pub fn shape_event(event: &Value, cal: &Calendar) -> Shaped {
    let Some(graph_id) = event.get("id").and_then(Value::as_str).filter(|s| !s.is_empty()) else {
        return Shaped::Invalid;
    };
    let is_all_day = event.get("isAllDay").and_then(Value::as_bool).unwrap_or(false);
    let (started_at, ended_at) = match (event_time(event.get("start"), is_all_day), event_time(event.get("end"), is_all_day)) {
        (Time::At(s), Time::At(e)) => (s, e),
        (Time::UnknownZone, _) | (_, Time::UnknownZone) => return Shaped::UnknownZone,
        _ => return Shaped::Invalid,
    };

    let is_cancelled = event.get("isCancelled").and_then(Value::as_bool).unwrap_or(false);
    let deleted_at_source = is_cancelled.then(|| {
        event
            .get("lastModifiedDateTime")
            .and_then(Value::as_str)
            .and_then(|s| s.parse::<DateTime<Utc>>().ok())
            .unwrap_or_else(Utc::now)
    });

    let key = format!("{}:{graph_id}", cal.id);
    let metadata = serde_json::json!({
        "graph_event_id": graph_id,
        "calendar_id": cal.id,
        "calendar_name": cal.name,
        "type": event.get("type"),
        "series_master_id": event.get("seriesMasterId"),
        "ical_uid": event.get("iCalUId"),
        "show_as": event.get("showAs"),
        "sensitivity": event.get("sensitivity"),
        "is_organizer": event.get("isOrganizer"),
        "organizer": event.get("organizer"),
        "attendees": event.get("attendees"),
        "online_meeting": event.get("onlineMeeting"),
        "response": event.pointer("/responseStatus/response"),
    });

    Shaped::Row(Box::new(EventRow {
        id: Uuid::new_v5(&Uuid::NAMESPACE_OID, format!("outlook:calendar:{key}").as_bytes()).to_string(),
        title: event
            .get("subject")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("(no title)")
            .to_string(),
        description: event.get("bodyPreview").and_then(Value::as_str).filter(|s| !s.is_empty()).map(String::from),
        calendar_name: cal.name.clone(),
        location_name: event
            .pointer("/location/displayName")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(String::from),
        started_at,
        ended_at,
        is_all_day,
        external_id: graph_id.to_string(),
        external_url: event.get("webLink").and_then(Value::as_str).map(String::from),
        source_stream_id: key,
        metadata,
        deleted_at_source,
        status: if is_cancelled { "cancelled" } else { "confirmed" },
        calendar_access_role: cal.access_role,
        response_status: response_status(event),
        attendee_identifiers: attendee_identifiers(event),
        organizer_identifier: event
            .pointer("/organizer/emailAddress")
            .and_then(|e| e.get("address").or_else(|| e.get("name")))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(String::from),
    }))
}

/// The owner's RSVP in Google's words: `accepted`, `tentative`, `declined` or
/// `needsAction`. `None` means nobody was asked, as on an event the owner made
/// with no attendees; it never means "did not go".
pub fn response_status(event: &Value) -> Option<&'static str> {
    let has_attendees = event.get("attendees").and_then(Value::as_array).is_some_and(|a| !a.is_empty());
    let is_organizer = event.get("isOrganizer").and_then(Value::as_bool).unwrap_or(false);
    match event.pointer("/responseStatus/response").and_then(Value::as_str)? {
        "accepted" => Some("accepted"),
        "tentativelyAccepted" => Some("tentative"),
        "declined" => Some("declined"),
        "notResponded" => Some("needsAction"),
        // Google lists the organizer of a meeting as an attendee who accepted.
        "organizer" => has_attendees.then_some("accepted"),
        // Graph also says `none` for an invitation not yet answered.
        "none" if has_attendees && !is_organizer => Some("needsAction"),
        _ => None,
    }
}

/// Everyone invited, by address (falling back to name). Rooms and equipment
/// come back as attendees of type `resource`; they are not people.
fn attendee_identifiers(event: &Value) -> Value {
    let list: Vec<String> = event
        .get("attendees")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter(|a| a.get("type").and_then(Value::as_str) != Some("resource"))
                .filter_map(|a| {
                    let e = a.get("emailAddress")?;
                    e.get("address")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .or_else(|| e.get("name").and_then(Value::as_str))
                        .map(String::from)
                })
                .collect()
        })
        .unwrap_or_default();
    serde_json::json!(list)
}

#[derive(Debug, PartialEq)]
enum Time {
    At(DateTime<Utc>),
    UnknownZone,
    Invalid,
}

/// A Graph `dateTimeTimeZone`, `{"dateTime": "2026-11-21T15:00:00.0000000",
/// "timeZone": "UTC"}`, as an instant. The applet asks for UTC; a zone Graph
/// sends anyway is read as an IANA name. An all-day event keeps its date at
/// midnight UTC, as the Google transform stores one.
fn event_time(t: Option<&Value>, all_day: bool) -> Time {
    let Some(raw) = t.and_then(|t| t.get("dateTime")).and_then(Value::as_str) else { return Time::Invalid };
    if all_day {
        return match raw.get(..10).and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()) {
            Some(d) => Time::At(d.and_hms_opt(0, 0, 0).expect("midnight exists").and_utc()),
            None => Time::Invalid,
        };
    }
    let Ok(naive) = NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.f") else {
        // An offset in the string itself settles the zone.
        return raw.parse::<DateTime<Utc>>().map_or(Time::Invalid, Time::At);
    };
    let zone = t.and_then(|t| t.get("timeZone")).and_then(Value::as_str).unwrap_or("UTC");
    if matches!(zone, "UTC" | "Etc/UTC" | "GMT" | "Etc/GMT" | "tzone://Microsoft/Utc") {
        return Time::At(naive.and_utc());
    }
    match zone.parse::<chrono_tz::Tz>() {
        // In a DST fold the earlier reading is taken; a time skipped by DST
        // does not exist and cannot be stored.
        Ok(tz) => tz.from_local_datetime(&naive).earliest().map_or(Time::Invalid, |d| Time::At(d.with_timezone(&Utc))),
        Err(_) => Time::UnknownZone,
    }
}

/// Upsert rows, returning how many were inserted or changed.
async fn flush(db: &PgPool, rows: &[&EventRow]) -> Result<usize> {
    if rows.is_empty() {
        return Ok(0);
    }
    // The whole window is read on every run, so rows that match what Graph
    // sent are left untouched.
    let sql = build_batch_upsert_changed_query(
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
            "calendar_name",
            "location_name",
            "started_at",
            "ended_at",
            "is_all_day",
            "external_url",
            "metadata",
            "deleted_at_source",
            "status",
            "calendar_access_role",
            "response_status",
            "attendee_identifiers",
            "organizer_identifier",
        ],
        rows.len(),
    );
    let mut q = sqlx::query(&sql);
    for r in rows {
        q = q
            .bind(&r.id)
            .bind(&r.title)
            .bind(&r.description)
            .bind(&r.calendar_name)
            .bind(&r.location_name)
            .bind(r.started_at)
            .bind(r.ended_at)
            .bind(r.is_all_day)
            .bind(&r.external_id)
            .bind(&r.external_url)
            .bind(&r.source_stream_id)
            .bind(SOURCE_TABLE)
            .bind(SOURCE_PROVIDER)
            .bind(&r.metadata)
            .bind(r.deleted_at_source)
            .bind(r.status)
            .bind(r.calendar_access_role)
            .bind(r.response_status)
            .bind(&r.attendee_identifiers)
            .bind(&r.organizer_identifier);
    }
    Ok(q.fetch_all(db).await?.len())
}

/// Mark this calendar's stored events inside the window that Graph did not
/// return as deleted. Call it only after the whole window was read.
///
/// Scoped to Outlook rows from this calendar: Google and iOS write the same
/// table. A row marked here by mistake heals on the next run that sees the
/// event, because the upsert writes `deleted_at_source` as null for a live
/// event.
pub async fn tombstone_absent(
    db: &PgPool,
    calendar_id: &str,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    seen: &[String],
) -> Result<u64> {
    let res = sqlx::query(
        "UPDATE data_calendar_event \
         SET deleted_at_source = now() \
         WHERE source_provider = 'outlook' \
           AND source_table = 'outlook_calendar' \
           AND metadata->>'calendar_id' = $1 \
           AND started_at >= $2 AND started_at < $3 \
           AND deleted_at_source IS NULL \
           AND source_stream_id <> ALL($4)",
    )
    .bind(calendar_id)
    .bind(window_start)
    .bind(window_end)
    .bind(seen)
    .execute(db)
    .await?;
    Ok(res.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cal() -> Calendar {
        Calendar { id: "AAMkCal1=".into(), name: Some("Calendar".into()), access_role: "owner" }
    }

    fn event() -> Value {
        json!({
            "id": "AAMkEvt1",
            "subject": "Planning with David",
            "bodyPreview": "Agenda: the spring trip.",
            "location": { "displayName": "Cafe Lumen" },
            "start": { "dateTime": "2026-11-24T15:00:00.0000000", "timeZone": "UTC" },
            "end": { "dateTime": "2026-11-24T15:30:00.0000000", "timeZone": "UTC" },
            "isAllDay": false,
            "isCancelled": false,
            "isOrganizer": false,
            "webLink": "https://outlook.example.com/calendar/item/AAMkEvt1",
            "responseStatus": { "response": "tentativelyAccepted", "time": "2026-11-20T10:00:00Z" },
            "organizer": { "emailAddress": { "name": "David Okafor", "address": "david@example.com" } },
            "attendees": [
                { "type": "required", "emailAddress": { "name": "Nick", "address": "nick@example.com" } },
                { "type": "optional", "emailAddress": { "name": "Ana", "address": "" } },
                { "type": "resource", "emailAddress": { "name": "Room 4", "address": "room4@example.com" } }
            ]
        })
    }

    fn row(v: &Value) -> EventRow {
        match shape_event(v, &cal()) {
            Shaped::Row(r) => *r,
            other => panic!("expected a row, got {other:?}"),
        }
    }

    #[test]
    fn a_graph_event_becomes_an_event_row() {
        let r = row(&event());
        assert_eq!(r.source_stream_id, "AAMkCal1=:AAMkEvt1");
        assert_eq!(r.external_id, "AAMkEvt1");
        assert_eq!(r.title, "Planning with David");
        assert_eq!(r.description.as_deref(), Some("Agenda: the spring trip."));
        assert_eq!(r.location_name.as_deref(), Some("Cafe Lumen"));
        assert_eq!(r.calendar_name.as_deref(), Some("Calendar"));
        assert_eq!(r.started_at, "2026-11-24T15:00:00Z".parse::<DateTime<Utc>>().unwrap());
        assert_eq!(r.ended_at, "2026-11-24T15:30:00Z".parse::<DateTime<Utc>>().unwrap());
        assert!(!r.is_all_day);
        assert_eq!(r.status, "confirmed");
        assert_eq!(r.deleted_at_source, None);
        assert_eq!(r.response_status, Some("tentative"));
        assert_eq!(r.attendee_identifiers, json!(["nick@example.com", "Ana"]));
        assert_eq!(r.organizer_identifier.as_deref(), Some("david@example.com"));
        assert_eq!(r.calendar_access_role, "owner");
        assert_eq!(r.metadata["calendar_id"], "AAMkCal1=");
    }

    #[test]
    fn a_cancelled_event_is_marked_deleted() {
        let mut e = event();
        e["isCancelled"] = json!(true);
        e["lastModifiedDateTime"] = json!("2026-11-22T09:00:00Z");
        let r = row(&e);
        assert_eq!(r.status, "cancelled");
        assert_eq!(r.deleted_at_source, Some("2026-11-22T09:00:00Z".parse().unwrap()));
    }

    #[test]
    fn an_all_day_event_keeps_its_date_at_midnight_utc() {
        let mut e = event();
        e["isAllDay"] = json!(true);
        e["start"] = json!({ "dateTime": "2026-12-24T00:00:00.0000000", "timeZone": "Pacific Standard Time" });
        e["end"] = json!({ "dateTime": "2026-12-25T00:00:00.0000000", "timeZone": "Pacific Standard Time" });
        let r = row(&e);
        assert!(r.is_all_day);
        assert_eq!(r.started_at, "2026-12-24T00:00:00Z".parse::<DateTime<Utc>>().unwrap());
        assert_eq!(r.ended_at, "2026-12-25T00:00:00Z".parse::<DateTime<Utc>>().unwrap());
    }

    #[test]
    fn a_zone_other_than_utc_is_read_as_an_iana_name_or_skipped() {
        let mut e = event();
        e["start"] = json!({ "dateTime": "2026-07-01T09:00:00.0000000", "timeZone": "America/Chicago" });
        e["end"] = json!({ "dateTime": "2026-07-01T10:00:00.0000000", "timeZone": "America/Chicago" });
        let r = row(&e);
        assert_eq!(r.started_at, "2026-07-01T14:00:00Z".parse::<DateTime<Utc>>().unwrap());

        e["start"]["timeZone"] = json!("Central Standard Time");
        assert_eq!(shape_event(&e, &cal()), Shaped::UnknownZone);

        let mut no_id = event();
        no_id.as_object_mut().unwrap().remove("id");
        assert_eq!(shape_event(&no_id, &cal()), Shaped::Invalid);
    }

    #[test]
    fn graph_responses_map_onto_google_rsvp_words() {
        let with = |response: &str, organizer: bool, attendees: Value| {
            response_status(&json!({
                "responseStatus": { "response": response },
                "isOrganizer": organizer,
                "attendees": attendees,
            }))
        };
        let some = json!([{ "emailAddress": { "address": "nick@example.com" } }]);
        assert_eq!(with("accepted", false, some.clone()), Some("accepted"));
        assert_eq!(with("tentativelyAccepted", false, some.clone()), Some("tentative"));
        assert_eq!(with("declined", false, some.clone()), Some("declined"));
        assert_eq!(with("notResponded", false, some.clone()), Some("needsAction"));
        assert_eq!(with("none", false, some.clone()), Some("needsAction"));
        assert_eq!(with("organizer", true, some.clone()), Some("accepted"));
        // An event the owner made for themselves: nobody was asked.
        assert_eq!(with("organizer", true, json!([])), None);
        assert_eq!(with("none", true, json!([])), None);
        assert_eq!(response_status(&json!({})), None);
    }

    /// An unchanged event is not rewritten, a changed one is, and only this
    /// calendar's Outlook rows that Graph stopped listing are tombstoned.
    #[sqlx::test(migrations = "../virtues-core/migrations")]
    async fn rereading_a_window_writes_only_changes_and_tombstones_what_is_gone(pool: PgPool) {
        let first = write_events(&pool, &cal(), &[event(), event()]).await.unwrap();
        assert_eq!((first.changed, first.seen.len()), (1, 1));
        let again = write_events(&pool, &cal(), &[event()]).await.unwrap();
        assert_eq!(again.changed, 0, "an unchanged event is left alone");
        let mut renamed = event();
        renamed["subject"] = json!("Planning with David and Nick");
        assert_eq!(write_events(&pool, &cal(), &[renamed]).await.unwrap().changed, 1);

        // A Google row that names the same calendar id is not Outlook's to mark.
        sqlx::query(
            "INSERT INTO data_calendar_event (id, title, started_at, ended_at, source_stream_id, source_table, source_provider, metadata) \
             VALUES ('g1', 'Dentist', '2026-11-25T10:00:00Z', '2026-11-25T11:00:00Z', 'g:1', 'google_calendar', 'google', \
                     '{\"calendar_id\": \"AAMkCal1=\"}')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let start = "2026-11-01T00:00:00Z".parse().unwrap();
        let end = "2026-12-01T00:00:00Z".parse().unwrap();
        let kept = tombstone_absent(&pool, "AAMkCal1=", start, end, &["AAMkCal1=:AAMkEvt1".to_string()]).await.unwrap();
        assert_eq!(kept, 0);
        let gone = tombstone_absent(&pool, "AAMkCal1=", start, end, &[]).await.unwrap();
        assert_eq!(gone, 1);
        let deleted: Vec<String> = sqlx::query_scalar(
            "SELECT source_stream_id FROM data_calendar_event WHERE deleted_at_source IS NOT NULL",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(deleted, vec!["AAMkCal1=:AAMkEvt1".to_string()]);

        // Graph lists it again: the upsert clears the tombstone.
        assert_eq!(write_events(&pool, &cal(), &[event()]).await.unwrap().changed, 1);
        let live: Option<DateTime<Utc>> = sqlx::query_scalar(
            "SELECT deleted_at_source FROM data_calendar_event WHERE source_stream_id = 'AAMkCal1=:AAMkEvt1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(live, None);
    }

    #[test]
    fn calendar_permissions_map_onto_google_roles() {
        let own = json!({ "id": "c1", "name": "Calendar", "canEdit": true, "canShare": true,
            "owner": { "name": "David Okafor", "address": "david@example.com" } });
        assert_eq!(access_role(&own, "DAVID@example.com"), "owner");
        let shared_edit = json!({ "id": "c2", "canEdit": true, "canShare": false,
            "owner": { "address": "nick@example.com" } });
        assert_eq!(access_role(&shared_edit, "david@example.com"), "writer");
        let shared_read = json!({ "id": "c3", "canEdit": false, "owner": { "address": "nick@example.com" } });
        assert_eq!(access_role(&shared_read, "david@example.com"), "reader");
        let cal = Calendar::from_graph(&own, "david@example.com").unwrap();
        assert_eq!((cal.id.as_str(), cal.name.as_deref(), cal.access_role), ("c1", Some("Calendar"), "owner"));
    }
}

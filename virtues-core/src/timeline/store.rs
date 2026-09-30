//! Reading the raw record, writing the `wiki_timeline_*` tables, and serving a
//! window of them to the view.
//!
//! The tables are created by `migrations/0000_timeline_derived.sql.pending`,
//! applied by hand while the Timeline is a local build. Where they don't exist
//! the rebuild does nothing and a window comes back empty with `is_built:
//! false`, so a database without them is never an error.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, Postgres, Row, Transaction};

use super::moments::Window;
use super::spine::Span;
use super::{derive, Fix, Ms, Record};
use crate::error::Result;
use crate::ids;

#[derive(Debug, Clone, Serialize)]
pub struct RebuildStats {
    pub places: usize,
    pub spans: usize,
    pub moments: usize,
    pub duration_ms: u128,
}

/// Rebuild the Timeline's tables from the whole raw record, in one
/// transaction, so a reader never sees half a rebuild. `None` when the tables
/// aren't in this database.
pub async fn rebuild(pool: &PgPool) -> Result<Option<RebuildStats>> {
    if !tables_exist(pool).await? {
        return Ok(None);
    }
    let started = std::time::Instant::now();
    let record = load(pool).await?;
    let derived = derive(&record);

    let wiki_places = load_wiki_places(pool).await?;
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM wiki_timeline_moments").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM wiki_timeline_spans").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM wiki_timeline_places").execute(&mut *tx).await?;

    let mut place_ids = Vec::with_capacity(derived.places.len());
    for p in &derived.places {
        let mut id = ids::generate_id("tlp", &[&format!("{:.4},{:.4}", p.lat, p.lon)]);
        if place_ids.contains(&id) {
            id = ids::generate_id("tlp", &[&format!("{:.4},{:.4}", p.lat, p.lon), &place_ids.len().to_string()]);
        }
        let wiki_place = wiki_place_at(&wiki_places, p.lat, p.lon);
        sqlx::query(
            "INSERT INTO wiki_timeline_places \
             (id, latitude, longitude, stop_count, dwell_minutes, overnight_minutes, is_home, is_work, place_id) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(&id)
        .bind(p.lat)
        .bind(p.lon)
        .bind(p.stop_count)
        .bind(minutes(p.dwell))
        .bind(minutes(p.overnight))
        .bind(p.is_home)
        .bind(p.is_work)
        .bind(wiki_place)
        .execute(&mut *tx)
        .await?;
        place_ids.push(id);
    }

    for s in &derived.stretches {
        let id = ids::generate_id("tls", &[s.kind, &s.s.to_string(), &s.e.to_string()]);
        let place = s.place.map(|i| place_ids[i].clone());
        insert_span(&mut tx, &id, s.kind, s.s, s.e, place, &s.metadata).await?;
    }
    for m in &derived.moments {
        let id = ids::generate_id("tlm", &[m.kind, &m.s.to_string(), &m.e.to_string()]);
        sqlx::query(
            "INSERT INTO wiki_timeline_moments (id, kind, started_at, ended_at, title, metadata) \
             VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (id) DO NOTHING",
        )
        .bind(&id)
        .bind(m.kind)
        .bind(instant(m.s))
        .bind(instant(m.e))
        .bind(&m.title)
        .bind(&m.metadata)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    Ok(Some(RebuildStats {
        places: derived.places.len(),
        spans: derived.stretches.len(),
        moments: derived.moments.len(),
        duration_ms: started.elapsed().as_millis(),
    }))
}

async fn insert_span(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    kind: &str,
    s: Ms,
    e: Ms,
    place: Option<String>,
    metadata: &serde_json::Value,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO wiki_timeline_spans (id, kind, started_at, ended_at, timeline_place_id, metadata) \
         VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (id) DO NOTHING",
    )
    .bind(id)
    .bind(kind)
    .bind(instant(s))
    .bind(instant(e))
    .bind(place)
    .bind(metadata)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn tables_exist(pool: &PgPool) -> Result<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT to_regclass('public.wiki_timeline_places') IS NOT NULL \
            AND to_regclass('public.wiki_timeline_spans') IS NOT NULL \
            AND to_regclass('public.wiki_timeline_moments') IS NOT NULL",
    )
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// The raw record: every fix, transcription window, HealthKit sleep row and
/// step reading still live at its source.
async fn load(pool: &PgPool) -> Result<Record> {
    let fixes = sqlx::query(
        "SELECT occurred_at, latitude, longitude, horizontal_accuracy, speed FROM data_location_point \
         WHERE deleted_at_source IS NULL AND NOT is_archived ORDER BY occurred_at",
    )
    .fetch_all(pool)
    .await?
    .iter()
    .map(|r| {
        Ok(Fix {
            t: ms(r.try_get("occurred_at")?),
            lat: r.try_get("latitude")?,
            lon: r.try_get("longitude")?,
            accuracy_m: r.try_get("horizontal_accuracy")?,
            speed_mps: r.try_get("speed")?,
        })
    })
    .collect::<std::result::Result<Vec<_>, sqlx::Error>>()?;

    let windows = sqlx::query(
        "SELECT id, started_at, ended_at, speaker_count, confidence, title FROM data_communication_transcription \
         WHERE deleted_at_source IS NULL AND NOT is_archived ORDER BY started_at",
    )
    .fetch_all(pool)
    .await?
    .iter()
    .map(|r| {
        let s = ms(r.try_get("started_at")?);
        let ended: Option<DateTime<Utc>> = r.try_get("ended_at")?;
        // No end recorded: the recorder's window is five minutes (`build.py:398`).
        let e = ended.map_or(s + 5 * super::MIN, ms);
        let speakers: Option<i32> = r.try_get("speaker_count")?;
        let confidence: Option<f64> = r.try_get("confidence")?;
        let title: Option<String> = r.try_get("title")?;
        Ok(Window {
            id: r.try_get("id")?,
            s,
            e,
            // A window with no count heard no one; one with no confidence ranks last.
            speakers: speakers.unwrap_or(0),
            confidence: confidence.unwrap_or(0.0),
            title: title.filter(|t| !t.trim().is_empty()).unwrap_or_else(|| "Conversation".into()),
        })
    })
    .collect::<std::result::Result<Vec<_>, sqlx::Error>>()?;

    let sleep_rows = sqlx::query(
        "SELECT started_at, ended_at FROM data_health_sleep \
         WHERE deleted_at_source IS NULL AND NOT is_archived ORDER BY started_at",
    )
    .fetch_all(pool)
    .await?
    .iter()
    .map(|r| Ok(Span { s: ms(r.try_get("started_at")?), e: ms(r.try_get("ended_at")?) }))
    .collect::<std::result::Result<Vec<_>, sqlx::Error>>()?;

    let step_times = sqlx::query(
        "SELECT occurred_at FROM data_health_steps \
         WHERE deleted_at_source IS NULL AND NOT is_archived AND step_count > 0 ORDER BY occurred_at",
    )
    .fetch_all(pool)
    .await?
    .iter()
    .map(|r| Ok(ms(r.try_get("occurred_at")?)))
    .collect::<std::result::Result<Vec<_>, sqlx::Error>>()?;

    let home = home_zone(pool).await?;
    Ok(Record { fixes, windows, sleep_rows, step_times, home })
}

/// The home zone: the profile's, else the server's own clock's.
async fn home_zone(pool: &PgPool) -> Result<chrono_tz::Tz> {
    let stored: Option<Option<String>> =
        sqlx::query_scalar("SELECT home_timezone FROM app_user_profile LIMIT 1").fetch_optional(pool).await?;
    Ok(match stored.flatten().or_else(crate::timezone::system_timezone) {
        Some(name) => match name.parse::<chrono_tz::Tz>() {
            Ok(tz) => tz,
            Err(_) => {
                tracing::warn!(zone = %name, "home time zone not in the zone database; days without a fix read in UTC");
                chrono_tz::UTC
            }
        },
        // No profile zone and no system zone: a day without a fix reads in UTC.
        None => chrono_tz::UTC,
    })
}

/// Steps counted in one 10-minute bin, stamped at the bin's middle.
#[derive(Debug, Clone, Serialize)]
pub struct StepBin {
    pub occurred_at: DateTime<Utc>,
    pub step_count: i64,
}

/// A timed calendar event.
#[derive(Debug, Clone, Serialize)]
pub struct CalendarEvent {
    pub id: String,
    pub title: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub calendar_name: Option<String>,
    pub location_name: Option<String>,
}

/// What the scrubber's Body, Calendar and Finance lanes draw over a window.
#[derive(Debug, Clone, Serialize)]
pub struct LaneWindow {
    /// Steps in 10-minute bins over the window, bins with steps only.
    pub steps: Vec<StepBin>,
    /// The 92nd-percentile bin across the whole record: the bars' scale, so
    /// ordinary movement fills the lane rather than the one peak's floor
    /// (`dayback/src/main.js:1361`).
    pub step_scale: f64,
    /// Whether any calendar has ever synced: without one, the lane asks to
    /// connect one instead of drawing an empty row.
    pub has_calendar: bool,
    /// Timed events over the window; an all-day event has no place on a
    /// timeline of hours and is left out.
    pub calendar: Vec<CalendarEvent>,
    /// Whether any financial account has synced.
    pub has_finance: bool,
}

const STEP_BIN: &str = "10 minutes";

/// The lanes over `start`..`end`: step bins aligned to `start`, their
/// record-wide scale, the calendar's timed events, and which sources exist.
pub async fn lanes(pool: &PgPool, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<LaneWindow> {
    let steps = sqlx::query(&format!(
        "SELECT date_bin('{STEP_BIN}', occurred_at, $1) + interval '5 minutes' AS at, sum(step_count)::bigint AS n \
         FROM data_health_steps \
         WHERE deleted_at_source IS NULL AND NOT is_archived AND occurred_at >= $1 AND occurred_at < $2 \
         GROUP BY 1 HAVING sum(step_count) > 0 ORDER BY 1"
    ))
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|r| Ok(StepBin { occurred_at: r.try_get("at")?, step_count: r.try_get("n")? }))
    .collect::<std::result::Result<Vec<_>, sqlx::Error>>()?;

    let scale: Option<f64> = sqlx::query_scalar(&format!(
        "SELECT percentile_disc(0.92) WITHIN GROUP (ORDER BY n)::float8 FROM ( \
           SELECT sum(step_count) AS n FROM data_health_steps \
           WHERE deleted_at_source IS NULL AND NOT is_archived \
           GROUP BY date_bin('{STEP_BIN}', occurred_at, TIMESTAMPTZ 'epoch') HAVING sum(step_count) > 0) bins"
    ))
    .fetch_one(pool)
    .await?;

    let has_calendar: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM data_calendar_event WHERE deleted_at_source IS NULL AND NOT is_archived)",
    )
    .fetch_one(pool)
    .await?;
    let calendar = sqlx::query(
        "SELECT id, title, started_at, ended_at, calendar_name, location_name FROM data_calendar_event \
         WHERE deleted_at_source IS NULL AND NOT is_archived AND NOT is_all_day \
           AND status IS DISTINCT FROM 'cancelled' AND started_at < $2 AND ended_at > $1 \
         ORDER BY started_at",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|r| {
        Ok(CalendarEvent {
            id: r.try_get("id")?,
            title: r.try_get("title")?,
            started_at: r.try_get("started_at")?,
            ended_at: r.try_get("ended_at")?,
            calendar_name: r.try_get("calendar_name")?,
            location_name: r.try_get("location_name")?,
        })
    })
    .collect::<std::result::Result<Vec<_>, sqlx::Error>>()?;

    let has_finance: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM data_financial_account WHERE deleted_at_source IS NULL AND NOT is_archived) \
             OR EXISTS (SELECT 1 FROM data_financial_transaction WHERE deleted_at_source IS NULL AND NOT is_archived)",
    )
    .fetch_one(pool)
    .await?;

    Ok(LaneWindow {
        steps,
        // absent-ok: no step was ever recorded, so any scale draws nothing; 1 keeps the division sound.
        step_scale: scale.unwrap_or(1.0).max(1.0),
        has_calendar,
        calendar,
        has_finance,
    })
}

/// One transcription window, as the rail reads it.
#[derive(Debug, Clone, Serialize)]
pub struct VoiceWindow {
    pub id: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    /// Voices the recorder heard: two or more is a conversation, none is
    /// silence the mic still recorded.
    pub speaker_count: i32,
    pub title: Option<String>,
    /// One utterance per line, "[Speaker 1]: ...".
    pub text: Option<String>,
    /// Names spoken in it: mentioned, never known to be present.
    pub people: Vec<String>,
}

/// Every transcription window overlapping `start`..`end`, in time order: the
/// mic's coverage (a window at all means it was recording) and, with two or
/// more speakers, the conversations a rail row opens (`dayback/build.py:
/// 387-402, 501-511`).
pub async fn voice(pool: &PgPool, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<VoiceWindow>> {
    sqlx::query(
        "SELECT id, started_at, ended_at, speaker_count, title, text, entities->'people' AS people \
         FROM data_communication_transcription \
         WHERE deleted_at_source IS NULL AND NOT is_archived \
           AND started_at < $2 AND coalesce(ended_at, started_at + interval '5 minutes') > $1 \
         ORDER BY started_at",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|r| {
        let started_at: DateTime<Utc> = r.try_get("started_at")?;
        let ended_at: Option<DateTime<Utc>> = r.try_get("ended_at")?;
        let speakers: Option<i32> = r.try_get("speaker_count")?;
        let people: Option<serde_json::Value> = r.try_get("people")?;
        Ok(VoiceWindow {
            id: r.try_get("id")?,
            started_at,
            // No end recorded: the recorder's window is five minutes (`build.py:398`).
            ended_at: ended_at.unwrap_or(started_at + chrono::Duration::minutes(5)),
            // A window with no count heard no one.
            speaker_count: speakers.unwrap_or(0),
            title: r.try_get("title")?,
            text: r.try_get("text")?,
            people: people
                .as_ref()
                .and_then(|p| p.as_array())
                .map(|a| a.iter().filter_map(|p| p.get("name")?.as_str()).filter(|n| !n.is_empty()).map(String::from).collect())
                .unwrap_or_default(),
        })
    })
    .collect::<std::result::Result<Vec<_>, sqlx::Error>>()
    .map_err(Into::into)
}

#[derive(Debug, Clone, Serialize)]
pub struct DayWindow {
    /// The zone the day woke up in (IANA).
    pub zone: String,
    pub started_at: DateTime<Utc>,
    /// Where the next day begins, in the next day's zone: days tile across a
    /// zone change, so a flight day runs 22 or 26 hours.
    pub ended_at: DateTime<Utc>,
}

/// One local day's bounds (`dayback/build.py:252-267`): it starts at local
/// midnight in the zone of its first fix (the day taken in the home zone to
/// find that fix) and ends where the next day starts.
pub async fn day_window(pool: &PgPool, date: chrono::NaiveDate) -> Result<DayWindow> {
    let home = home_zone(pool).await?;
    let zone_of = |date: chrono::NaiveDate| async move {
        let from = super::zone::midnight(date, home);
        let first = sqlx::query(
            "SELECT latitude, longitude FROM data_location_point \
             WHERE occurred_at >= $1 AND occurred_at < $2 AND deleted_at_source IS NULL AND NOT is_archived \
             ORDER BY occurred_at LIMIT 1",
        )
        .bind(instant(from))
        .bind(instant(from + 24 * super::HOUR))
        .fetch_optional(pool)
        .await?;
        let zone = match first {
            Some(r) => super::zone::at(r.try_get("latitude")?, r.try_get("longitude")?).unwrap_or(home),
            // No fix that day: it is read in the home zone.
            None => home,
        };
        Ok::<_, crate::error::Error>(zone)
    };
    let zone = zone_of(date).await?;
    let next = date.succ_opt().expect("a date within range");
    let next_zone = zone_of(next).await?;
    Ok(DayWindow {
        zone: zone.name().to_string(),
        started_at: instant(super::zone::midnight(date, zone)),
        ended_at: instant(super::zone::midnight(next, next_zone)),
    })
}

/// The days in `from..=to` that hold any record of you - a location fix, a
/// transcription window or a step reading - each read over its own local
/// day, the same bounds the day view uses. The month's dots: a day with none
/// has nothing to open.
pub async fn recorded_days(pool: &PgPool, from: chrono::NaiveDate, to: chrono::NaiveDate) -> Result<Vec<chrono::NaiveDate>> {
    let mut dates = Vec::new();
    let mut d = from;
    while d <= to {
        dates.push(d);
        d = d.succ_opt().expect("a date within range");
    }
    let mut starts = Vec::with_capacity(dates.len());
    let mut ends = Vec::with_capacity(dates.len());
    for &date in &dates {
        let w = day_window(pool, date).await?;
        starts.push(w.started_at);
        ends.push(w.ended_at);
    }
    let hits: Vec<i64> = sqlx::query_scalar(
        "SELECT w.i FROM unnest($1::timestamptz[], $2::timestamptz[]) WITH ORDINALITY AS w(s, e, i) \
         WHERE EXISTS (SELECT 1 FROM data_location_point p \
                       WHERE p.deleted_at_source IS NULL AND NOT p.is_archived AND p.occurred_at >= w.s AND p.occurred_at < w.e) \
            OR EXISTS (SELECT 1 FROM data_communication_transcription t \
                       WHERE t.deleted_at_source IS NULL AND NOT t.is_archived AND t.started_at >= w.s AND t.started_at < w.e) \
            OR EXISTS (SELECT 1 FROM data_health_steps h \
                       WHERE h.deleted_at_source IS NULL AND NOT h.is_archived AND h.occurred_at >= w.s AND h.occurred_at < w.e) \
         ORDER BY w.i",
    )
    .bind(&starts)
    .bind(&ends)
    .fetch_all(pool)
    .await?;
    Ok(hits.into_iter().map(|i| dates[(i - 1) as usize]).collect())
}

struct WikiPlace {
    id: String,
    lat: f64,
    lon: f64,
    radius_m: f64,
}

async fn load_wiki_places(pool: &PgPool) -> Result<Vec<WikiPlace>> {
    sqlx::query(
        "SELECT id, latitude, longitude, radius_m FROM wiki_places \
         WHERE latitude IS NOT NULL AND longitude IS NOT NULL",
    )
    .fetch_all(pool)
    .await?
    .iter()
    .map(|r| {
        Ok(WikiPlace {
            id: r.try_get("id")?,
            lat: r.try_get("latitude")?,
            lon: r.try_get("longitude")?,
            radius_m: r.try_get("radius_m")?,
        })
    })
    .collect::<std::result::Result<Vec<_>, sqlx::Error>>()
    .map_err(Into::into)
}

/// The wiki place a centre sits in: the nearest whose own radius holds it, as
/// the wiki's resolver matches a visit.
fn wiki_place_at(places: &[WikiPlace], lat: f64, lon: f64) -> Option<String> {
    places
        .iter()
        .map(|p| (p, crate::geo::haversine_distance(lat, lon, p.lat, p.lon)))
        .filter(|(p, d)| *d <= p.radius_m)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(p, _)| p.id.clone())
}

fn ms(t: DateTime<Utc>) -> Ms {
    t.timestamp_millis()
}

fn instant(t: Ms) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(t).expect("a derived instant is within range")
}

fn minutes(t: Ms) -> i32 {
    i32::try_from(t / super::MIN).unwrap_or(i32::MAX)
}

// ---------------------------------------------------------------------------
// The view's window
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct TimelinePlace {
    pub id: String,
    pub latitude: f64,
    pub longitude: f64,
    pub stop_count: i32,
    pub dwell_minutes: i32,
    pub overnight_minutes: i32,
    pub is_home: bool,
    pub is_work: bool,
    pub place_id: Option<String>,
    /// The wiki place's name, when the centre sits in one.
    pub place_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelineSpan {
    pub id: String,
    pub kind: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub timeline_place_id: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelineMoment {
    pub id: String,
    pub kind: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub title: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelineWindow {
    /// False when this database has no Timeline tables yet.
    pub is_built: bool,
    pub spans: Vec<TimelineSpan>,
    pub moments: Vec<TimelineMoment>,
    /// Every place a span in the window stays at.
    pub places: Vec<TimelinePlace>,
}

/// Every stretch and moment overlapping `start`..`end`, and their places.
pub async fn window(pool: &PgPool, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<TimelineWindow> {
    if !tables_exist(pool).await? {
        return Ok(TimelineWindow { is_built: false, spans: vec![], moments: vec![], places: vec![] });
    }
    let spans = sqlx::query(
        "SELECT id, kind, started_at, ended_at, timeline_place_id, metadata FROM wiki_timeline_spans \
         WHERE started_at < $2 AND ended_at > $1 ORDER BY started_at, ended_at",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|r| {
        Ok(TimelineSpan {
            id: r.try_get("id")?,
            kind: r.try_get("kind")?,
            started_at: r.try_get("started_at")?,
            ended_at: r.try_get("ended_at")?,
            timeline_place_id: r.try_get("timeline_place_id")?,
            metadata: r.try_get("metadata")?,
        })
    })
    .collect::<std::result::Result<Vec<_>, sqlx::Error>>()?;

    let moments = sqlx::query(
        "SELECT id, kind, started_at, ended_at, title, metadata FROM wiki_timeline_moments \
         WHERE started_at < $2 AND ended_at > $1 ORDER BY started_at, ended_at",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|r| {
        Ok(TimelineMoment {
            id: r.try_get("id")?,
            kind: r.try_get("kind")?,
            started_at: r.try_get("started_at")?,
            ended_at: r.try_get("ended_at")?,
            title: r.try_get("title")?,
            metadata: r.try_get("metadata")?,
        })
    })
    .collect::<std::result::Result<Vec<_>, sqlx::Error>>()?;

    let place_ids: Vec<String> = spans.iter().filter_map(|s| s.timeline_place_id.clone()).collect();
    let places = sqlx::query(
        "SELECT t.id, t.latitude, t.longitude, t.stop_count, t.dwell_minutes, t.overnight_minutes, \
                t.is_home, t.is_work, t.place_id, w.name AS place_name \
         FROM wiki_timeline_places t LEFT JOIN wiki_places w ON w.id = t.place_id \
         WHERE t.id = ANY($1)",
    )
    .bind(&place_ids)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|r| {
        Ok(TimelinePlace {
            id: r.try_get("id")?,
            latitude: r.try_get("latitude")?,
            longitude: r.try_get("longitude")?,
            stop_count: r.try_get("stop_count")?,
            dwell_minutes: r.try_get("dwell_minutes")?,
            overnight_minutes: r.try_get("overnight_minutes")?,
            is_home: r.try_get("is_home")?,
            is_work: r.try_get("is_work")?,
            place_id: r.try_get("place_id")?,
            place_name: r.try_get("place_name")?,
        })
    })
    .collect::<std::result::Result<Vec<_>, sqlx::Error>>()?;

    Ok(TimelineWindow { is_built: true, spans, moments, places })
}

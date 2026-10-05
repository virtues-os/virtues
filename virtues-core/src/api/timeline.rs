//! The Timeline: one local day of the record, read whole.
//!
//! Everything here is read from the tables every other reader uses: stays are
//! `data_location_visit` joined through `wiki_refs` to `wiki_places`, nights are
//! `dayline::sleep`'s joined nights, conversations are `data_audio_session`.
//! What lies between two stays (a drive, a walk, a stretch with no location) is
//! the view's to read from the track, so nothing here classifies or names.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Serialize;
use sqlx::{PgPool, Row};

use super::wiki_days::{timeline_point, TimelinePoint};
use crate::error::Result;

/// Steps are summed into bins this wide.
const STEP_BIN_MINUTES: i64 = 10;

#[derive(Debug, Serialize)]
pub struct TimelineDay {
    pub date: NaiveDate,
    /// The zone the day is read in (`timezone::day_timezone`).
    pub zone: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    /// Visits overlapping the day, unclipped: one that began last night keeps
    /// its real start.
    pub stays: Vec<Stay>,
    /// Every fix inside the day, in time order.
    pub points: Vec<TimelinePoint>,
    /// The last fix before the day, so a day without one can say where the
    /// phone was last measured, and when.
    pub last_point_before: Option<TimelinePoint>,
    /// Nights overlapping the day: the one it woke from and the one it went
    /// to bed in.
    pub nights: Vec<NightSpan>,
    pub sessions: Vec<AudioSession>,
    pub steps: Vec<StepBin>,
    /// A busy 10 minutes for this person: the 92nd percentile of their step
    /// bins over the last 90 days, so ordinary movement fills the lane rather
    /// than one peak setting its ceiling. 1 when nothing was ever counted.
    pub step_scale: f64,
    pub calendar: Vec<CalendarEvent>,
}

#[derive(Debug, Serialize)]
pub struct Stay {
    pub id: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub latitude: f64,
    pub longitude: f64,
    /// Resolved at read time: place ids can be merged, visit ids cannot.
    pub place_id: Option<String>,
    pub place_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NightSpan {
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub asleep_minutes: i64,
}

#[derive(Debug, Serialize)]
pub struct AudioSession {
    pub id: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    /// 0 ambient, 1 one voice, 2 a conversation, 3 a group.
    pub speaker_mode: i16,
    /// The title the transcriber gave the session's longest stretch of
    /// speech; none when nothing in it was speech.
    pub title: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StepBin {
    /// The bin's middle.
    pub at: DateTime<Utc>,
    pub steps: i64,
}

#[derive(Debug, Serialize)]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub is_all_day: bool,
    pub calendar_name: Option<String>,
    pub location_name: Option<String>,
    pub status: Option<String>,
    pub response_status: Option<String>,
}

pub async fn get_day(pool: &PgPool, date: NaiveDate) -> Result<TimelineDay> {
    let zone = crate::timezone::day_timezone(pool, date).await?;
    let (start, end) = super::day_summary::day_bounds(date, Some(&zone));

    let stays = sqlx::query(
        r#"SELECT v.id, v.started_at, v.ended_at, v.latitude, v.longitude,
                  p.id AS place_id, p.name AS place_name
           FROM data_location_visit v
           -- A visit can carry more than one place ref; the newest is the one
           -- resolution meant, and one visit must stay one stay.
           LEFT JOIN LATERAL (
               SELECT er.entity_id FROM wiki_refs er
               WHERE er.source_table = 'data_location_visit'
                 AND er.source_id = v.id
                 AND er.entity_type = 'place'
               ORDER BY er.created_at DESC LIMIT 1
           ) er ON TRUE
           LEFT JOIN wiki_places p ON p.id = er.entity_id
           WHERE v.started_at < $2 AND v.ended_at > $1
             AND v.deleted_at_source IS NULL AND NOT v.is_archived
           ORDER BY v.started_at"#,
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|r| {
        Ok(Stay {
            id: r.try_get("id")?,
            started_at: r.try_get("started_at")?,
            ended_at: r.try_get("ended_at")?,
            latitude: r.try_get("latitude")?,
            longitude: r.try_get("longitude")?,
            place_id: r.try_get("place_id")?,
            place_name: r.try_get("place_name")?,
        })
    })
    .collect::<std::result::Result<Vec<_>, sqlx::Error>>()?;

    const POINT_COLUMNS: &str = "latitude, longitude, occurred_at, horizontal_accuracy, speed";
    let points = sqlx::query(&format!(
        "SELECT {POINT_COLUMNS} FROM data_location_point \
         WHERE occurred_at >= $1 AND occurred_at < $2 \
           AND deleted_at_source IS NULL AND NOT is_archived \
         ORDER BY occurred_at"
    ))
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?
    .iter()
    .filter_map(timeline_point)
    .collect();

    let last_point_before = sqlx::query(&format!(
        "SELECT {POINT_COLUMNS} FROM data_location_point \
         WHERE occurred_at < $1 AND deleted_at_source IS NULL AND NOT is_archived \
         ORDER BY occurred_at DESC LIMIT 1"
    ))
    .bind(start)
    .fetch_optional(pool)
    .await?
    .and_then(|r| timeline_point(&r));

    // A night overlapping the day ends inside it or within a day after it.
    let nights = crate::dayline::sleep::nights_ending_between(pool, start, end + Duration::days(1))
        .await?
        .into_iter()
        .filter(|n| n.start < end)
        .map(|n| NightSpan { started_at: n.start, ended_at: n.end, asleep_minutes: n.asleep_minutes })
        .collect();

    let sessions = sqlx::query(
        "SELECT s.id, s.started_at, s.ended_at, s.speaker_mode, s.content, t.title \
         FROM data_audio_session s \
         LEFT JOIN LATERAL ( \
             SELECT title FROM data_communication_transcription \
             WHERE started_at >= s.started_at AND started_at < s.ended_at \
               AND text <> '' AND title IS NOT NULL \
               AND deleted_at_source IS NULL AND NOT is_archived \
             ORDER BY length(text) DESC LIMIT 1 \
         ) t ON TRUE \
         WHERE s.started_at < $2 AND s.ended_at > $1 ORDER BY s.started_at",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|r| {
        Ok(AudioSession {
            id: r.try_get("id")?,
            started_at: r.try_get("started_at")?,
            ended_at: r.try_get("ended_at")?,
            speaker_mode: r.try_get("speaker_mode")?,
            title: r.try_get("title")?,
            content: r.try_get("content")?,
        })
    })
    .collect::<std::result::Result<Vec<_>, sqlx::Error>>()?;

    let steps = sqlx::query(
        "SELECT to_timestamp(floor(extract(epoch FROM occurred_at) / $3) * $3 + $3 / 2) AS bin, \
                sum(step_count)::int8 AS steps \
         FROM data_health_steps \
         WHERE occurred_at >= $1 AND occurred_at < $2 AND step_count > 0 \
           AND deleted_at_source IS NULL AND NOT is_archived \
         GROUP BY 1 ORDER BY 1",
    )
    .bind(start)
    .bind(end)
    .bind((STEP_BIN_MINUTES * 60) as f64)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|r| Ok(StepBin { at: r.try_get("bin")?, steps: r.try_get("steps")? }))
    .collect::<std::result::Result<Vec<_>, sqlx::Error>>()?;

    let step_scale: Option<f64> = sqlx::query_scalar(
        "SELECT percentile_disc(0.92) WITHIN GROUP (ORDER BY n)::float8 FROM ( \
           SELECT sum(step_count) AS n FROM data_health_steps \
           WHERE occurred_at >= $1 - interval '90 days' AND occurred_at < $1 \
             AND deleted_at_source IS NULL AND NOT is_archived \
           GROUP BY floor(extract(epoch FROM occurred_at) / $2) \
           HAVING sum(step_count) > 0) bins",
    )
    .bind(end)
    .bind((STEP_BIN_MINUTES * 60) as f64)
    .fetch_one(pool)
    .await?;

    let calendar = sqlx::query(
        "SELECT id, title, started_at, ended_at, is_all_day, calendar_name, location_name, status, response_status \
         FROM data_calendar_event \
         WHERE started_at < $2 AND ended_at > $1 \
           AND deleted_at_source IS NULL AND NOT is_archived \
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
            is_all_day: r.try_get("is_all_day")?,
            calendar_name: r.try_get("calendar_name")?,
            location_name: r.try_get("location_name")?,
            status: r.try_get("status")?,
            response_status: r.try_get("response_status")?,
        })
    })
    .collect::<std::result::Result<Vec<_>, sqlx::Error>>()?;

    Ok(TimelineDay {
        date,
        zone,
        started_at: start,
        ended_at: end,
        stays,
        points,
        last_point_before,
        nights,
        sessions,
        steps,
        // absent-ok: no step was ever counted, so any scale draws nothing.
        step_scale: step_scale.unwrap_or(1.0).max(1.0),
        calendar,
    })
}

/// The days in `from..=to` with a stay, a fix or a recorded conversation, each
/// read over its own local day. Step readings alone don't count: the phone's
/// step history runs years past its location and audio, and those days would
/// open on an empty map.
pub async fn recorded_days(pool: &PgPool, from: NaiveDate, to: NaiveDate) -> Result<Vec<NaiveDate>> {
    let mut days = Vec::new();
    for date in from.iter_days().take_while(|d| *d <= to) {
        let (start, end) = crate::timezone::day_window(pool, date).await?;
        let recorded: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM data_location_point \
                            WHERE occurred_at >= $1 AND occurred_at < $2 \
                              AND deleted_at_source IS NULL AND NOT is_archived) \
                 OR EXISTS (SELECT 1 FROM data_audio_session \
                            WHERE started_at >= $1 AND started_at < $2)",
        )
        .bind(start)
        .bind(end)
        .fetch_one(pool)
        .await?;
        if recorded {
            days.push(date);
        }
    }
    Ok(days)
}

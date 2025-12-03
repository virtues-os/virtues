//! Day view builder - constructs chunks from location_visits

use chrono::{NaiveDate, TimeZone, Utc};
use sqlx::PgPool;

use crate::error::Result;

use super::{
    AttachedCalendarEvent, AttachedEmail, AttachedHealthEvent, AttachedMessage,
    AttachedTranscript, Chunk, DayView, LocationChunk, MissingDataChunk, MissingReason,
    TransitChunk, MIN_VISIT_DURATION_MINUTES,
};

/// Build the day view for a given date
///
/// This is the main entry point for generating the location-first day view.
/// It queries location_visits, builds chunks from visits + gaps, classifies
/// transit, and attaches ontology data.
pub async fn build_day_view(db: &PgPool, date: NaiveDate) -> Result<DayView> {
    // Calculate day boundaries in UTC
    let day_start = Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap());
    let day_end = Utc.from_utc_datetime(&date.and_hms_opt(23, 59, 59).unwrap());

    // Query location visits for the day, ordered by start_time
    let visits = query_location_visits(db, day_start, day_end).await?;

    // Build chunks from visits
    let mut chunks = build_chunks_from_visits(&visits, day_start, day_end, db).await?;

    // Attach ontology data to chunks
    attach_ontology_data(db, &mut chunks, day_start, day_end).await?;

    // Calculate totals
    let mut total_location = 0;
    let mut total_transit = 0;
    let mut total_missing = 0;

    for chunk in &chunks {
        match chunk {
            Chunk::Location(c) => total_location += c.duration_minutes,
            Chunk::Transit(c) => total_transit += c.duration_minutes,
            Chunk::MissingData(c) => total_missing += c.duration_minutes,
        }
    }

    Ok(DayView {
        date,
        chunks,
        total_location_minutes: total_location,
        total_transit_minutes: total_transit,
        total_missing_minutes: total_missing,
    })
}

/// Raw location visit from database
#[derive(Debug)]
struct LocationVisit {
    _id: uuid::Uuid,
    place_id: Option<uuid::Uuid>,
    place_name: Option<String>,
    latitude: f64,
    longitude: f64,
    start_time: chrono::DateTime<Utc>,
    end_time: chrono::DateTime<Utc>,
}

/// Query location visits for a date range
async fn query_location_visits(
    db: &PgPool,
    start: chrono::DateTime<Utc>,
    end: chrono::DateTime<Utc>,
) -> Result<Vec<LocationVisit>> {
    let rows = sqlx::query_as::<_, (uuid::Uuid, Option<uuid::Uuid>, Option<String>, f64, f64, chrono::DateTime<Utc>, chrono::DateTime<Utc>)>(
        r#"
        SELECT
            lv.id,
            lv.place_id,
            p.canonical_name as place_name,
            lv.latitude,
            lv.longitude,
            lv.start_time,
            lv.end_time
        FROM data.location_visit lv
        LEFT JOIN data.entities_place p ON lv.place_id = p.id
        WHERE lv.start_time >= $1
          AND lv.start_time <= $2
        ORDER BY lv.start_time ASC
        "#,
    )
    .bind(start)
    .bind(end)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, place_id, place_name, lat, lon, start, end)| LocationVisit {
            _id: id,
            place_id,
            place_name,
            latitude: lat,
            longitude: lon,
            start_time: start,
            end_time: end,
        })
        .collect())
}

/// Build chunks from location visits, filling gaps with transit or missing data
async fn build_chunks_from_visits(
    visits: &[LocationVisit],
    day_start: chrono::DateTime<Utc>,
    day_end: chrono::DateTime<Utc>,
    db: &PgPool,
) -> Result<Vec<Chunk>> {
    let mut chunks = Vec::new();

    if visits.is_empty() {
        // No visits - entire day is missing data
        chunks.push(Chunk::MissingData(MissingDataChunk {
            start_time: day_start,
            end_time: day_end,
            duration_minutes: 24 * 60,
            likely_reason: MissingReason::Unknown,
            last_known_location: None,
            next_known_location: None,
            messages: vec![],
            transcripts: vec![],
        }));
        return Ok(chunks);
    }

    let mut current_time = day_start;

    for (i, visit) in visits.iter().enumerate() {
        let visit_duration = (visit.end_time - visit.start_time).num_minutes() as i32;

        // Check for gap before this visit
        if visit.start_time > current_time {
            let gap_duration = (visit.start_time - current_time).num_minutes() as i32;

            // Classify gap as transit or missing data
            let gap_chunk = classify_gap(
                db,
                current_time,
                visit.start_time,
                gap_duration,
                if i > 0 { visits.get(i - 1).map(|v| v.place_name.clone()).flatten() } else { None },
                visit.place_name.clone(),
            )
            .await?;

            chunks.push(gap_chunk);
        }

        // Add location chunk if duration >= minimum
        if visit_duration >= MIN_VISIT_DURATION_MINUTES {
            chunks.push(Chunk::Location(LocationChunk {
                start_time: visit.start_time,
                end_time: visit.end_time,
                duration_minutes: visit_duration,
                place_id: visit.place_id,
                place_name: visit.place_name.clone(),
                latitude: visit.latitude,
                longitude: visit.longitude,
                is_known_place: visit.place_id.is_some(),
                messages: vec![],
                transcripts: vec![],
                calendar_events: vec![],
                emails: vec![],
                health_events: vec![],
            }));
        }

        current_time = visit.end_time;
    }

    // Check for gap after last visit until end of day
    if current_time < day_end {
        let gap_duration = (day_end - current_time).num_minutes() as i32;
        let last_place = visits.last().and_then(|v| v.place_name.clone());

        // Evening gap after last visit - likely sleep or home time
        chunks.push(Chunk::MissingData(MissingDataChunk {
            start_time: current_time,
            end_time: day_end,
            duration_minutes: gap_duration,
            likely_reason: if gap_duration > 300 {
                MissingReason::Sleep
            } else {
                MissingReason::Indoors
            },
            last_known_location: last_place,
            next_known_location: None,
            messages: vec![],
            transcripts: vec![],
        }));
    }

    Ok(chunks)
}

/// Classify a gap between visits as transit or missing data
async fn classify_gap(
    db: &PgPool,
    start: chrono::DateTime<Utc>,
    end: chrono::DateTime<Utc>,
    duration_minutes: i32,
    from_place: Option<String>,
    to_place: Option<String>,
) -> Result<Chunk> {
    // Query GPS points in the gap to calculate distance and speed
    let points = sqlx::query_as::<_, (f64, f64, chrono::DateTime<Utc>)>(
        r#"
        SELECT latitude, longitude, timestamp
        FROM data.location_point
        WHERE timestamp >= $1 AND timestamp <= $2
        ORDER BY timestamp ASC
        "#,
    )
    .bind(start)
    .bind(end)
    .fetch_all(db)
    .await?;

    if points.len() < 2 {
        // Not enough GPS data - classify as missing
        return Ok(Chunk::MissingData(MissingDataChunk {
            start_time: start,
            end_time: end,
            duration_minutes,
            likely_reason: if duration_minutes > 60 {
                MissingReason::Indoors
            } else {
                MissingReason::Unknown
            },
            last_known_location: from_place,
            next_known_location: to_place,
            messages: vec![],
            transcripts: vec![],
        }));
    }

    // Calculate total distance using haversine
    let mut total_distance_km = 0.0;
    for i in 1..points.len() {
        let (lat1, lon1, _) = points[i - 1];
        let (lat2, lon2, _) = points[i];
        total_distance_km += haversine_distance(lat1, lon1, lat2, lon2);
    }

    // Calculate average speed
    let duration_hours = duration_minutes as f64 / 60.0;
    let avg_speed_kmh = if duration_hours > 0.0 {
        total_distance_km / duration_hours
    } else {
        0.0
    };

    // Minimum 100 meters of movement required to classify as transit
    // Otherwise it's likely stationary GPS drift or indoor movement
    const MIN_TRANSIT_DISTANCE_KM: f64 = 0.1;

    if total_distance_km < MIN_TRANSIT_DISTANCE_KM {
        return Ok(Chunk::MissingData(MissingDataChunk {
            start_time: start,
            end_time: end,
            duration_minutes,
            likely_reason: if duration_minutes > 300 {
                MissingReason::Sleep
            } else if duration_minutes > 60 {
                MissingReason::Indoors
            } else {
                MissingReason::Unknown
            },
            last_known_location: from_place,
            next_known_location: to_place,
            messages: vec![],
            transcripts: vec![],
        }));
    }

    Ok(Chunk::Transit(TransitChunk {
        start_time: start,
        end_time: end,
        duration_minutes,
        distance_km: total_distance_km,
        avg_speed_kmh,
        from_place,
        to_place,
        messages: vec![],
        transcripts: vec![],
    }))
}

/// Calculate haversine distance between two points in kilometers
fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6371.0;

    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();

    let a = (dlat / 2.0).sin().powi(2)
        + lat1_rad.cos() * lat2_rad.cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();

    EARTH_RADIUS_KM * c
}

/// Attach ontology data to chunks by temporal intersection
async fn attach_ontology_data(
    db: &PgPool,
    chunks: &mut [Chunk],
    _day_start: chrono::DateTime<Utc>,
    _day_end: chrono::DateTime<Utc>,
) -> Result<()> {
    for chunk in chunks.iter_mut() {
        let (start, end) = (chunk.start_time(), chunk.end_time());

        // Query messages in time range
        let messages = query_messages(db, start, end).await?;

        // Query transcripts in time range
        let transcripts = query_transcripts(db, start, end).await?;

        // Query calendar events overlapping time range
        let calendar_events = query_calendar_events(db, start, end).await?;

        // Query emails in time range
        let emails = query_emails(db, start, end).await?;

        // Query health events in time range
        let health_events = query_health_events(db, start, end).await?;

        // Attach to chunk
        match chunk {
            Chunk::Location(c) => {
                c.messages = messages;
                c.transcripts = transcripts;
                c.calendar_events = calendar_events;
                c.emails = emails;
                c.health_events = health_events;
            }
            Chunk::Transit(c) => {
                c.messages = messages;
                c.transcripts = transcripts;
            }
            Chunk::MissingData(c) => {
                c.messages = messages;
                c.transcripts = transcripts;
            }
        }
    }

    Ok(())
}

async fn query_messages(
    db: &PgPool,
    start: chrono::DateTime<Utc>,
    end: chrono::DateTime<Utc>,
) -> Result<Vec<AttachedMessage>> {
    let rows = sqlx::query_as::<_, (uuid::Uuid, chrono::DateTime<Utc>, String, String, Option<String>, Option<String>)>(
        r#"
        SELECT id, timestamp, channel, direction, from_name, body
        FROM data.social_message
        WHERE timestamp >= $1 AND timestamp <= $2
        ORDER BY timestamp ASC
        "#,
    )
    .bind(start)
    .bind(end)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, timestamp, channel, direction, from_name, body)| AttachedMessage {
            id,
            timestamp,
            channel,
            direction,
            from_name,
            body_preview: body.unwrap_or_default().chars().take(100).collect(),
        })
        .collect())
}

async fn query_transcripts(
    db: &PgPool,
    start: chrono::DateTime<Utc>,
    end: chrono::DateTime<Utc>,
) -> Result<Vec<AttachedTranscript>> {
    let rows = sqlx::query_as::<_, (uuid::Uuid, chrono::DateTime<Utc>, i32, Option<String>, Option<i32>)>(
        r#"
        SELECT id, recorded_at, audio_duration_seconds, transcript_text, speaker_count
        FROM data.speech_transcription
        WHERE recorded_at >= $1 AND recorded_at <= $2
        ORDER BY recorded_at ASC
        "#,
    )
    .bind(start)
    .bind(end)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, recorded_at, duration, text, speaker_count)| AttachedTranscript {
            id,
            recorded_at,
            duration_seconds: duration,
            transcript_preview: text.unwrap_or_default().chars().take(200).collect(),
            speaker_count,
        })
        .collect())
}

async fn query_calendar_events(
    db: &PgPool,
    start: chrono::DateTime<Utc>,
    end: chrono::DateTime<Utc>,
) -> Result<Vec<AttachedCalendarEvent>> {
    let rows = sqlx::query_as::<_, (uuid::Uuid, chrono::DateTime<Utc>, chrono::DateTime<Utc>, String, Option<String>)>(
        r#"
        SELECT id, start_time, end_time, title, location_name
        FROM data.praxis_calendar
        WHERE start_time <= $2 AND end_time >= $1
        ORDER BY start_time ASC
        "#,
    )
    .bind(start)
    .bind(end)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, start_time, end_time, title, location_name)| AttachedCalendarEvent {
            id,
            start_time,
            end_time,
            title,
            location_name,
        })
        .collect())
}

async fn query_emails(
    db: &PgPool,
    start: chrono::DateTime<Utc>,
    end: chrono::DateTime<Utc>,
) -> Result<Vec<AttachedEmail>> {
    let rows = sqlx::query_as::<_, (uuid::Uuid, chrono::DateTime<Utc>, String, Option<String>, Option<String>)>(
        r#"
        SELECT id, timestamp, direction, from_name, subject
        FROM data.social_email
        WHERE timestamp >= $1 AND timestamp <= $2
        ORDER BY timestamp ASC
        "#,
    )
    .bind(start)
    .bind(end)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, timestamp, direction, from_name, subject)| AttachedEmail {
            id,
            timestamp,
            direction,
            from_name,
            subject,
        })
        .collect())
}

async fn query_health_events(
    db: &PgPool,
    start: chrono::DateTime<Utc>,
    end: chrono::DateTime<Utc>,
) -> Result<Vec<AttachedHealthEvent>> {
    // Query sleep end times that fall within the range
    let rows = sqlx::query_as::<_, (uuid::Uuid, chrono::DateTime<Utc>, i32)>(
        r#"
        SELECT id, end_time, total_duration_minutes
        FROM data.health_sleep
        WHERE end_time >= $1 AND end_time <= $2
        ORDER BY end_time ASC
        "#,
    )
    .bind(start)
    .bind(end)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, end_time, duration)| AttachedHealthEvent {
            id,
            event_type: "sleep_end".to_string(),
            timestamp: end_time,
            description: format!("Woke up after {} hours of sleep", duration / 60),
        })
        .collect())
}

//! Timeline Chunk Processor
//!
//! Continuous processing of location_visits into stored timeline_chunks.
//! Designed to run as an hourly cron job, incrementally building chunks.
//!
//! ## Algorithm
//!
//! 1. Find the last processed timestamp from timeline_chunk
//! 2. Query location_visits since that timestamp
//! 3. Build chunks (using existing builder logic)
//! 4. Write chunks to timeline_chunk table
//! 5. Attach ontology data to chunks

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::Result;

use super::{
    AttachedCalendarEvent, AttachedEmail, AttachedHealthEvent, AttachedMessage,
    AttachedTranscript, Chunk, DayView, LocationChunk, MissingDataChunk, MissingReason,
    TransitChunk, MIN_VISIT_DURATION_MINUTES,
};

/// Minimum 100 meters of movement required to classify as transit
const MIN_TRANSIT_DISTANCE_KM: f64 = 0.1;

/// Default lookback window for initial processing (hours)
const DEFAULT_LOOKBACK_HOURS: i64 = 24;

/// Process timeline chunks incrementally
///
/// This is the main entry point for the cron job. It:
/// 1. Finds the last processed time
/// 2. Processes all location_visits since then
/// 3. Stores chunks in timeline_chunk table
///
/// Returns the number of chunks created/updated.
pub async fn process_timeline_chunks(db: &PgPool) -> Result<usize> {
    // Find the last processed timestamp
    let last_chunk_end = get_last_chunk_end_time(db).await?;

    // Default to 24 hours ago if no chunks exist
    let process_from = last_chunk_end
        .unwrap_or_else(|| Utc::now() - Duration::hours(DEFAULT_LOOKBACK_HOURS));

    let process_to = Utc::now();

    tracing::info!(
        from = %process_from,
        to = %process_to,
        "Processing timeline chunks"
    );

    // Process the time window
    process_time_window(db, process_from, process_to).await
}

/// Process a specific time window (for backfill or testing)
pub async fn process_time_window(
    db: &PgPool,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<usize> {
    // Delete existing chunks in this time window first
    // This ensures stale chunks (e.g., incorrectly classified transits) are replaced
    let deleted = sqlx::query(
        "DELETE FROM data.timeline_chunk WHERE start_time >= $1 AND start_time < $2",
    )
    .bind(start)
    .bind(end)
    .execute(db)
    .await?;

    if deleted.rows_affected() > 0 {
        tracing::debug!(
            deleted = deleted.rows_affected(),
            "Cleared existing chunks in time window"
        );
    }

    // Query location visits in the window
    let visits = query_location_visits(db, start, end).await?;

    if visits.is_empty() {
        tracing::debug!("No location visits in window");
        return Ok(0);
    }

    tracing::debug!(
        visit_count = visits.len(),
        "Found location visits to process"
    );

    // Build chunks from visits
    let chunks = build_chunks_from_visits(db, &visits, start, end).await?;

    // Write chunks to database
    let mut chunks_written = 0;
    for chunk in &chunks {
        match write_chunk(db, chunk).await {
            Ok(_) => chunks_written += 1,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Failed to write chunk"
                );
            }
        }
    }

    tracing::info!(
        chunks_written,
        "Timeline chunk processing complete"
    );

    Ok(chunks_written)
}

/// Get the end time of the most recent chunk
async fn get_last_chunk_end_time(db: &PgPool) -> Result<Option<DateTime<Utc>>> {
    let result = sqlx::query_scalar::<_, DateTime<Utc>>(
        r#"
        SELECT MAX(end_time)
        FROM data.timeline_chunk
        "#,
    )
    .fetch_optional(db)
    .await?;

    Ok(result)
}

/// Query timeline chunks for a date range (for API queries)
pub async fn query_chunks(
    db: &PgPool,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<Chunk>> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"
        SELECT
            id,
            chunk_type::text as chunk_type,
            start_time,
            end_time,
            duration_minutes,
            place_id,
            place_name,
            latitude,
            longitude,
            is_known_place,
            distance_km,
            avg_speed_kmh,
            from_place,
            to_place,
            likely_reason::text as likely_reason,
            last_known_location,
            next_known_location,
            attached_data
        FROM data.timeline_chunk
        WHERE start_time < $2 AND end_time > $1
        ORDER BY start_time ASC
        "#,
    )
    .bind(start)
    .bind(end)
    .fetch_all(db)
    .await?;

    let chunks = rows
        .into_iter()
        .map(|row| {
            let chunk_type: String = row.get("chunk_type");
            let attached_data: serde_json::Value = row
                .get::<Option<serde_json::Value>, _>("attached_data")
                .unwrap_or_else(|| serde_json::json!({}));

            // Extract attached data from unified JSONB
            let messages: Vec<AttachedMessage> = attached_data
                .get("messages")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let transcripts: Vec<AttachedTranscript> = attached_data
                .get("transcripts")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let calendar_events: Vec<AttachedCalendarEvent> = attached_data
                .get("calendar_events")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let emails: Vec<AttachedEmail> = attached_data
                .get("emails")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let health_events: Vec<AttachedHealthEvent> = attached_data
                .get("health_events")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            match chunk_type.as_str() {
                "location" => Chunk::Location(LocationChunk {
                    start_time: row.get("start_time"),
                    end_time: row.get("end_time"),
                    duration_minutes: row.get::<Option<i32>, _>("duration_minutes").unwrap_or(0),
                    place_id: row.get("place_id"),
                    place_name: row.get("place_name"),
                    latitude: row.get::<Option<f64>, _>("latitude").unwrap_or(0.0),
                    longitude: row.get::<Option<f64>, _>("longitude").unwrap_or(0.0),
                    is_known_place: row.get::<Option<bool>, _>("is_known_place").unwrap_or(false),
                    messages,
                    transcripts,
                    calendar_events,
                    emails,
                    health_events,
                }),
                "transit" => Chunk::Transit(TransitChunk {
                    start_time: row.get("start_time"),
                    end_time: row.get("end_time"),
                    duration_minutes: row.get::<Option<i32>, _>("duration_minutes").unwrap_or(0),
                    distance_km: row.get::<Option<f64>, _>("distance_km").unwrap_or(0.0),
                    avg_speed_kmh: row.get::<Option<f64>, _>("avg_speed_kmh").unwrap_or(0.0),
                    from_place: row.get("from_place"),
                    to_place: row.get("to_place"),
                    messages,
                    transcripts,
                }),
                "missing_data" | _ => {
                    let likely_reason: Option<String> = row.get("likely_reason");
                    Chunk::MissingData(MissingDataChunk {
                        start_time: row.get("start_time"),
                        end_time: row.get("end_time"),
                        duration_minutes: row.get::<Option<i32>, _>("duration_minutes").unwrap_or(0),
                        likely_reason: match likely_reason.as_deref() {
                            Some("sleep") => MissingReason::Sleep,
                            Some("indoors") => MissingReason::Indoors,
                            Some("phone_off") => MissingReason::PhoneOff,
                            _ => MissingReason::Unknown,
                        },
                        last_known_location: row.get("last_known_location"),
                        next_known_location: row.get("next_known_location"),
                        messages,
                        transcripts,
                    })
                }
            }
        })
        .collect();

    Ok(chunks)
}

/// Build a DayView from stored chunks (convenience method for API)
pub async fn get_day_view(
    db: &PgPool,
    date: chrono::NaiveDate,
    timezone_offset_hours: i32,
) -> Result<DayView> {
    use chrono::TimeZone;

    // Calculate day boundaries with timezone offset
    let day_start = Utc
        .from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
        - Duration::hours(timezone_offset_hours as i64);
    let day_end = day_start + Duration::hours(24);

    let chunks = query_chunks(db, day_start, day_end).await?;

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

// ============================================================================
// Internal helpers (adapted from builder.rs)
// ============================================================================

#[derive(Debug)]
struct LocationVisit {
    _id: Uuid,
    place_id: Option<Uuid>,
    place_name: Option<String>,
    latitude: f64,
    longitude: f64,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
}

async fn query_location_visits(
    db: &PgPool,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<LocationVisit>> {
    let rows = sqlx::query_as::<_, (Uuid, Option<Uuid>, Option<String>, f64, f64, DateTime<Utc>, DateTime<Utc>)>(
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

async fn build_chunks_from_visits(
    db: &PgPool,
    visits: &[LocationVisit],
    _window_start: DateTime<Utc>,
    _window_end: DateTime<Utc>,
) -> Result<Vec<Chunk>> {
    let mut chunks = Vec::new();

    if visits.is_empty() {
        return Ok(chunks);
    }

    // Start from first visit time (don't create leading gaps for missing historical data)
    let mut current_time = visits.first().unwrap().start_time;

    for (i, visit) in visits.iter().enumerate() {
        let visit_duration = (visit.end_time - visit.start_time).num_minutes() as i32;

        // Check for gap before this visit (only after the first visit)
        if i > 0 && visit.start_time > current_time {
            let gap_duration = (visit.start_time - current_time).num_minutes() as i32;

            // Only create gap chunks for significant gaps (> 5 min)
            if gap_duration > 5 {
                let gap_chunk = classify_gap(
                    db,
                    current_time,
                    visit.start_time,
                    gap_duration,
                    visits.get(i - 1).and_then(|v| v.place_name.clone()),
                    visit.place_name.clone(),
                )
                .await?;

                // Attach ontology data to gap chunk
                let gap_chunk = attach_ontology_data_to_chunk(db, gap_chunk).await?;
                chunks.push(gap_chunk);
            }
        }

        // Add location chunk if duration >= minimum
        if visit_duration >= MIN_VISIT_DURATION_MINUTES {
            let mut loc_chunk = Chunk::Location(LocationChunk {
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
            });

            loc_chunk = attach_ontology_data_to_chunk(db, loc_chunk).await?;
            chunks.push(loc_chunk);
        }

        current_time = visit.end_time;
    }

    // Note: We don't create trailing gaps after the last visit.
    // The continuous processor will pick up new visits in the next run.
    // For day view display, gaps between known chunks are shown by the UI.

    Ok(chunks)
}

async fn classify_gap(
    db: &PgPool,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    duration_minutes: i32,
    from_place: Option<String>,
    to_place: Option<String>,
) -> Result<Chunk> {
    // Query GPS points in the gap to calculate distance and speed
    let points = sqlx::query_as::<_, (f64, f64, DateTime<Utc>)>(
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

    // Calculate total distance
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

    // Minimum distance check - if no significant movement, it's missing data
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

async fn attach_ontology_data_to_chunk(db: &PgPool, chunk: Chunk) -> Result<Chunk> {
    let (start, end) = (chunk.start_time(), chunk.end_time());

    let messages = query_messages(db, start, end).await?;
    let transcripts = query_transcripts(db, start, end).await?;

    match chunk {
        Chunk::Location(mut c) => {
            c.messages = messages;
            c.transcripts = transcripts;
            c.calendar_events = query_calendar_events(db, start, end).await?;
            c.emails = query_emails(db, start, end).await?;
            c.health_events = query_health_events(db, start, end).await?;
            Ok(Chunk::Location(c))
        }
        Chunk::Transit(mut c) => {
            c.messages = messages;
            c.transcripts = transcripts;
            Ok(Chunk::Transit(c))
        }
        Chunk::MissingData(mut c) => {
            c.messages = messages;
            c.transcripts = transcripts;
            Ok(Chunk::MissingData(c))
        }
    }
}

async fn write_chunk(db: &PgPool, chunk: &Chunk) -> Result<()> {
    match chunk {
        Chunk::Location(c) => {
            // Build unified attached_data JSONB
            let attached_data = serde_json::json!({
                "messages": c.messages,
                "transcripts": c.transcripts,
                "calendar_events": c.calendar_events,
                "emails": c.emails,
                "health_events": c.health_events,
            });

            sqlx::query(
                r#"
                INSERT INTO data.timeline_chunk (
                    chunk_type, start_time, end_time,
                    place_id, place_name, latitude, longitude, is_known_place,
                    attached_data
                ) VALUES (
                    'location'::data.chunk_type, $1, $2,
                    $3, $4, $5, $6, $7,
                    $8
                )
                ON CONFLICT DO NOTHING
                "#,
            )
            .bind(c.start_time)
            .bind(c.end_time)
            .bind(c.place_id)
            .bind(&c.place_name)
            .bind(c.latitude)
            .bind(c.longitude)
            .bind(c.is_known_place)
            .bind(attached_data)
            .execute(db)
            .await?;
        }
        Chunk::Transit(c) => {
            // Build unified attached_data JSONB
            let attached_data = serde_json::json!({
                "messages": c.messages,
                "transcripts": c.transcripts,
            });

            sqlx::query(
                r#"
                INSERT INTO data.timeline_chunk (
                    chunk_type, start_time, end_time,
                    distance_km, avg_speed_kmh, from_place, to_place,
                    attached_data
                ) VALUES (
                    'transit'::data.chunk_type, $1, $2,
                    $3, $4, $5, $6,
                    $7
                )
                ON CONFLICT DO NOTHING
                "#,
            )
            .bind(c.start_time)
            .bind(c.end_time)
            .bind(c.distance_km)
            .bind(c.avg_speed_kmh)
            .bind(&c.from_place)
            .bind(&c.to_place)
            .bind(attached_data)
            .execute(db)
            .await?;
        }
        Chunk::MissingData(c) => {
            let reason_str = match c.likely_reason {
                MissingReason::Sleep => "sleep",
                MissingReason::Indoors => "indoors",
                MissingReason::PhoneOff => "phone_off",
                MissingReason::Unknown => "unknown",
            };

            // Build unified attached_data JSONB
            let attached_data = serde_json::json!({
                "messages": c.messages,
                "transcripts": c.transcripts,
            });

            sqlx::query(
                r#"
                INSERT INTO data.timeline_chunk (
                    chunk_type, start_time, end_time,
                    likely_reason, last_known_location, next_known_location,
                    attached_data
                ) VALUES (
                    'missing_data'::data.chunk_type, $1, $2,
                    $3::data.missing_reason, $4, $5,
                    $6
                )
                ON CONFLICT DO NOTHING
                "#,
            )
            .bind(c.start_time)
            .bind(c.end_time)
            .bind(reason_str)
            .bind(&c.last_known_location)
            .bind(&c.next_known_location)
            .bind(attached_data)
            .execute(db)
            .await?;
        }
    }

    Ok(())
}

// Query helpers for attached ontology data

async fn query_messages(
    db: &PgPool,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<AttachedMessage>> {
    let rows = sqlx::query_as::<_, (Uuid, DateTime<Utc>, String, String, Option<String>, Option<String>)>(
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
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<AttachedTranscript>> {
    let rows = sqlx::query_as::<_, (Uuid, DateTime<Utc>, i32, Option<String>, Option<i32>)>(
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
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<AttachedCalendarEvent>> {
    let rows = sqlx::query_as::<_, (Uuid, DateTime<Utc>, DateTime<Utc>, String, Option<String>)>(
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
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<AttachedEmail>> {
    let rows = sqlx::query_as::<_, (Uuid, DateTime<Utc>, String, Option<String>, Option<String>)>(
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
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<AttachedHealthEvent>> {
    let rows = sqlx::query_as::<_, (Uuid, DateTime<Utc>, i32)>(
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

//! Location-first day view chunks
//!
//! This module provides the core abstractions for the location-first day view:
//! - `Chunk`: Enum representing Location, Transit, or MissingData segments
//! - `DayView`: The complete day view with ordered chunks
//! - Functions to build chunks from location_visits and attach ontology data
//!
//! ## Two Modes
//!
//! 1. **On-demand (legacy)**: `build_day_view` queries location_visits live
//! 2. **Stored (recommended)**: `processor::process_timeline_chunks` builds
//!    and stores chunks incrementally, `processor::query_chunks` reads them

mod builder;
mod processor;

pub use builder::build_day_view;
pub use processor::{get_day_view, process_timeline_chunks, process_time_window, query_chunks};

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Minimum visit duration in minutes to be considered a location chunk
/// Visits shorter than this are folded into adjacent transit
pub const MIN_VISIT_DURATION_MINUTES: i32 = 10;

/// A chunk in the day view - either a location visit, transit, or missing data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Chunk {
    Location(LocationChunk),
    Transit(TransitChunk),
    MissingData(MissingDataChunk),
}

impl Chunk {
    pub fn start_time(&self) -> DateTime<Utc> {
        match self {
            Chunk::Location(c) => c.start_time,
            Chunk::Transit(c) => c.start_time,
            Chunk::MissingData(c) => c.start_time,
        }
    }

    pub fn end_time(&self) -> DateTime<Utc> {
        match self {
            Chunk::Location(c) => c.end_time,
            Chunk::Transit(c) => c.end_time,
            Chunk::MissingData(c) => c.end_time,
        }
    }

    pub fn duration_minutes(&self) -> i32 {
        match self {
            Chunk::Location(c) => c.duration_minutes,
            Chunk::Transit(c) => c.duration_minutes,
            Chunk::MissingData(c) => c.duration_minutes,
        }
    }
}

/// A location visit chunk - stationary at a place for >= MIN_VISIT_DURATION_MINUTES
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationChunk {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub duration_minutes: i32,

    // Location identity
    pub place_id: Option<Uuid>,
    pub place_name: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    pub is_known_place: bool,

    // Attached data (by temporal intersection)
    pub messages: Vec<AttachedMessage>,
    pub transcripts: Vec<AttachedTranscript>,
    pub calendar_events: Vec<AttachedCalendarEvent>,
    pub emails: Vec<AttachedEmail>,
    pub health_events: Vec<AttachedHealthEvent>,
}

/// A transit chunk - moving between locations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitChunk {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub duration_minutes: i32,

    // Movement characteristics (raw data, no mode inference)
    pub distance_km: f64,
    pub avg_speed_kmh: f64,

    // Origin/destination
    pub from_place: Option<String>,
    pub to_place: Option<String>,

    // Attached data (less common during transit)
    pub messages: Vec<AttachedMessage>,
    pub transcripts: Vec<AttachedTranscript>,
}

/// A missing data chunk - no GPS signal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissingDataChunk {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub duration_minutes: i32,

    pub likely_reason: MissingReason,
    pub last_known_location: Option<String>,
    pub next_known_location: Option<String>,

    // Data that arrived during gap
    pub messages: Vec<AttachedMessage>,
    pub transcripts: Vec<AttachedTranscript>,
}

/// Reason for missing GPS data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingReason {
    Sleep,
    Indoors,
    PhoneOff,
    Unknown,
}

/// The complete day view
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayView {
    pub date: NaiveDate,
    pub chunks: Vec<Chunk>,
    pub total_location_minutes: i32,
    pub total_transit_minutes: i32,
    pub total_missing_minutes: i32,
}

// Attached data types (lightweight references to ontology data)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachedMessage {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub channel: String,
    pub direction: String,
    pub from_name: Option<String>,
    pub body_preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachedTranscript {
    pub id: Uuid,
    pub recorded_at: DateTime<Utc>,
    pub duration_seconds: i32,
    pub transcript_preview: String,
    pub speaker_count: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachedCalendarEvent {
    pub id: Uuid,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub title: String,
    pub location_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachedEmail {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub direction: String,
    pub from_name: Option<String>,
    pub subject: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachedHealthEvent {
    pub id: Uuid,
    pub event_type: String,
    pub timestamp: DateTime<Utc>,
    pub description: String,
}

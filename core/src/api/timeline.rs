//! Timeline API - Location-first day views
//!
//! Provides access to structured day views based on stored timeline chunks.
//! Each day is composed of chunks: Location, Transit, or MissingData.
//!
//! ## Architecture
//!
//! Timeline chunks are pre-computed by a continuous processor (hourly cron job)
//! and stored in the `timeline_chunk` table. This API queries stored chunks
//! for fast, consistent access.

use crate::error::Result;
use chrono::NaiveDate;
use sqlx::PgPool;

// Re-export chunk types for API consumers
pub use crate::timeline::chunks::{
    AttachedCalendarEvent, AttachedEmail, AttachedHealthEvent, AttachedMessage,
    AttachedTranscript, Chunk, DayView, LocationChunk, MissingDataChunk, TransitChunk,
};

/// Get a day view with location-based chunks
///
/// Returns a structured view of the day organized around location visits.
/// Each chunk is one of:
/// - Location: Time spent at a place (known or unknown)
/// - Transit: Movement between places
/// - MissingData: Gaps where no location data exists
///
/// Ontology data (messages, transcripts, calendar, etc.) is attached to
/// chunks by temporal intersection.
///
/// # Arguments
///
/// * `db` - Database connection pool
/// * `date` - The date to get the day view for
/// * `timezone_offset_hours` - Timezone offset from UTC in hours (e.g., 1 for Rome, -8 for PST)
///
/// # Example
///
/// ```ignore
/// // Get day view for Nov 10, 2025 in Rome timezone (UTC+1)
/// let view = get_day_view(&db, date, 1).await?;
/// ```
pub async fn get_day_view(
    db: &PgPool,
    date: NaiveDate,
    timezone_offset_hours: i32,
) -> Result<DayView> {
    crate::timeline::chunks::get_day_view(db, date, timezone_offset_hours).await
}

/// Get a day view using legacy on-demand building (for backwards compatibility)
///
/// This function builds chunks on-demand from location_visits. It's slower
/// and doesn't support timezone offsets. Prefer `get_day_view` for new code.
#[deprecated(note = "Use get_day_view with timezone_offset_hours instead")]
pub async fn get_day_view_legacy(db: &PgPool, date: NaiveDate) -> Result<DayView> {
    crate::timeline::chunks::build_day_view(db, date).await
}

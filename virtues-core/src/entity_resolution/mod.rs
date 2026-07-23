//! Entity Resolution Module
//!
//! Pre-resolution pipeline that converts raw ontology primitives into canonical entities.
//! This module is called inline by the NarrativePrimitivePipeline, NOT as a separate cron job.
//!
//! ## Architecture
//!
//! Entity resolution happens BEFORE changepoint detection to ensure boundaries reference
//! properly resolved entities (places with IDs, people with canonical names).
//!
//! ## Modules
//!
//! - `places`: Location clustering (location_point → location_visit → entities_place)
//! - `people`: Calendar attendee resolution (calendar attendees → entities_person)
//!
//! ## Usage
//!
//! ```ignore
//! let window = TimeWindow::new(start, end);
//! let stats = entity_resolution::resolve_entities(db, window).await?;
//! ```

pub mod people;
pub mod places;

use crate::database::Database;
use crate::error::Result;
use chrono::{DateTime, Utc};

/// Time window for entity resolution
#[derive(Debug, Clone, Copy)]
pub struct TimeWindow {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl TimeWindow {
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        Self { start, end }
    }

    /// Create a window from now - duration to now
    pub fn from_lookback_hours(hours: i64) -> Self {
        let end = Utc::now();
        let start = end - chrono::Duration::hours(hours);
        Self { start, end }
    }
}

/// Statistics from entity resolution
#[derive(Debug, Default)]
pub struct ResolutionStats {
    pub places_resolved: usize,
    pub people_resolved: usize,
    pub duration_ms: u128,
}

/// Main entry point: Resolve all entities in time window
///
/// This function orchestrates place and people resolution.
/// Called inline by the narrative primitive pipeline.
pub async fn resolve_entities(db: &Database, window: TimeWindow) -> Result<ResolutionStats> {
    let start = std::time::Instant::now();

    tracing::info!(
        start = %window.start,
        end = %window.end,
        "Starting entity resolution"
    );

    // 1. Resolve places (location clustering)
    let places_resolved = places::resolve_places(db, window).await?;

    // 2. Resolve people (calendar attendees, email senders)
    let people_resolved = people::resolve_people(db, window).await?;

    // Semantic ER (prose/NER extraction into `er_mentions`, then mention
    // linking) used to run here as steps 3-4. It is gone deliberately.
    //
    // The graph is deterministic + user-authored, and the numbers were decisive:
    // of 130,777 entity refs on a real box, the semantic path produced 189
    // (0.14%) — and even those linked only via a human-written alias, never by
    // the machine. Meanwhile it accrued 11,113 permanently-floating mentions, a
    // review queue that was never cleared, 172k extraction-log rows, and a
    // per-sweep LLM call. Handle matching, merchant resolution and place
    // clustering above produced the other 99.86% for free.
    let duration_ms = start.elapsed().as_millis();

    tracing::info!(
        places_resolved,
        people_resolved,
        duration_ms,
        "Entity resolution completed"
    );

    Ok(ResolutionStats {
        places_resolved,
        people_resolved,
        duration_ms,
    })
}

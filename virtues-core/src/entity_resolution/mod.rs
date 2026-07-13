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
//! ```rust
//! let window = TimeWindow::new(start, end);
//! let stats = entity_resolution::resolve_entities(db, window).await?;
//! ```

pub mod extract;
pub mod mentions;
pub mod prose;
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
    /// Mentions drained out of prose into `er_mentions` (the evidence layer).
    pub mentions_extracted: usize,
    /// Mentions that matched exactly one entity and are now linked.
    pub mentions_linked: usize,
    /// Mentions still floating — nothing matched, or the surface is ambiguous.
    /// These are the review queue. They are dust, not failures.
    pub mentions_floating: usize,
    pub duration_ms: u128,
}

/// How many un-extracted source records one sweep will read. Bounds a cold
/// start over years of backlog; the next tick continues where this stopped.
const EXTRACT_BATCH: i64 = 500;

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

    // 3. Drain prose mentions into the evidence layer. Steps 1-2 read STRUCTURED
    //    columns — an email's From: header is already an identity, a GPS fix is
    //    already a place. Those are joins, and they are where most links come
    //    from. This step handles what only appears as prose: a name spoken in a
    //    transcript. Different problem, different guarantees.
    //
    //    Not time-windowed, deliberately. A mention floats until a human writes
    //    the alias that resolves it, and that can happen months after the
    //    recording — so the sweep must be able to reach the whole backlog.
    let extracted = extract::extract_from_transcriptions(db, EXTRACT_BATCH).await?;

    //    ...and out of the four ontologies that carry prose but have no
    //    extraction of their own (email, messages, documents, AI chats). One
    //    component, driven by the ontology registry — never a per-source branch.
    //    A new source (Slack, Fastmail) normalizes into an existing ontology and
    //    is extracted with no code change. Best-effort: an LLM hiccup must not
    //    take down the deterministic resolvers above it.
    let prose = match prose::extract_from_prose(db).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("prose extraction failed (deterministic resolution unaffected): {e}");
            prose::ProseStats::default()
        }
    };

    // 4. Resolve those mentions — but ONLY on an exact, unambiguous match
    //    (canonical name, nickname, or a human-written alias). One candidate
    //    links; zero or many stay floating. The machine never picks which Sarah.
    let mention_stats = mentions::resolve_mentions(db).await?;

    let duration_ms = start.elapsed().as_millis();

    tracing::info!(
        places_resolved,
        people_resolved,
        mentions_extracted = extracted.mentions + prose.mentions,
        mentions_linked = mention_stats.linked,
        mentions_floating = mention_stats.unmatched + mention_stats.ambiguous,
        duration_ms,
        "Entity resolution completed"
    );

    Ok(ResolutionStats {
        places_resolved,
        people_resolved,
        mentions_extracted: extracted.mentions + prose.mentions,
        mentions_linked: mention_stats.linked,
        mentions_floating: mention_stats.unmatched + mention_stats.ambiguous,
        duration_ms,
    })
}

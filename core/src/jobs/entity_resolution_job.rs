//! Entity Resolution Job Transforms
//!
//! Wraps entity resolution (places and people) as transform stages that can be
//! chained from ontology transforms. These transforms don't use the standard
//! data source pattern - they query ontology tables directly.
//!
//! ## Usage
//!
//! These transforms are triggered via transform chaining:
//! - iOS Location Transform → PlaceResolutionTransform
//! - Google Calendar Transform → PeopleResolutionTransform
//!
//! The parent transform returns a `ChainedTransform` with `transform_stage: "entity_resolution"`,
//! which causes the job system to create a child job that executes these transforms.

use async_trait::async_trait;
use uuid::Uuid;

use crate::database::Database;
use crate::entity_resolution::{self, TimeWindow};
use crate::error::Result;
use crate::jobs::TransformContext;
use crate::sources::base::{
    ChainedTransform, OntologyTransform, TransformRegistration, TransformResult,
};

/// Default lookback window for entity resolution (24 hours)
const DEFAULT_LOOKBACK_HOURS: i64 = 24;

/// Place Resolution Transform
///
/// Clusters location_point records into location_visit records and links
/// them to entities_place. Uses density-adaptive spatial-temporal clustering.
pub struct PlaceResolutionTransform;

#[async_trait]
impl OntologyTransform for PlaceResolutionTransform {
    fn source_table(&self) -> &str {
        "location_point"
    }

    fn target_table(&self) -> &str {
        "location_visit"
    }

    fn domain(&self) -> &str {
        "location"
    }

    async fn transform(
        &self,
        db: &Database,
        _context: &TransformContext,
        _source_id: Uuid,
    ) -> Result<TransformResult> {
        // Entity resolution doesn't use the standard data source pattern.
        // It queries the ontology tables directly with a time window.
        let window = TimeWindow::from_lookback_hours(DEFAULT_LOOKBACK_HOURS);

        tracing::info!(
            start = %window.start,
            end = %window.end,
            "Running place resolution transform"
        );

        let visits_created = entity_resolution::places::resolve_places(db, window).await?;

        tracing::info!(visits_created, "Place resolution transform completed");

        Ok(TransformResult {
            records_read: visits_created, // Approximation - actual points read is internal
            records_written: visits_created,
            records_failed: 0,
            last_processed_id: None,
            chained_transforms: vec![], // Terminal - no further chaining
        })
    }
}

/// Registration for PlaceResolutionTransform
struct PlaceResolutionRegistration;

impl TransformRegistration for PlaceResolutionRegistration {
    fn source_table(&self) -> &'static str {
        "location_point"
    }

    fn target_table(&self) -> &'static str {
        "location_visit"
    }

    fn create(&self, _context: &TransformContext) -> Result<Box<dyn OntologyTransform>> {
        Ok(Box::new(PlaceResolutionTransform))
    }
}

inventory::submit! {
    &PlaceResolutionRegistration as &dyn TransformRegistration
}

/// People Resolution Transform
///
/// Resolves calendar attendees to entities_person records. Extracts email
/// addresses from praxis_calendar.attendee_identifiers and creates/links
/// person entities.
pub struct PeopleResolutionTransform;

#[async_trait]
impl OntologyTransform for PeopleResolutionTransform {
    fn source_table(&self) -> &str {
        "praxis_calendar"
    }

    fn target_table(&self) -> &str {
        "entities_person"
    }

    fn domain(&self) -> &str {
        "social"
    }

    async fn transform(
        &self,
        db: &Database,
        _context: &TransformContext,
        _source_id: Uuid,
    ) -> Result<TransformResult> {
        // Entity resolution doesn't use the standard data source pattern.
        // It queries the ontology tables directly with a time window.
        let window = TimeWindow::from_lookback_hours(DEFAULT_LOOKBACK_HOURS);

        tracing::info!(
            start = %window.start,
            end = %window.end,
            "Running people resolution transform"
        );

        let people_resolved = entity_resolution::people::resolve_people(db, window).await?;

        tracing::info!(people_resolved, "People resolution transform completed");

        Ok(TransformResult {
            records_read: people_resolved, // Approximation - actual events read is internal
            records_written: people_resolved,
            records_failed: 0,
            last_processed_id: None,
            chained_transforms: vec![], // Terminal - no further chaining
        })
    }
}

/// Registration for PeopleResolutionTransform
struct PeopleResolutionRegistration;

impl TransformRegistration for PeopleResolutionRegistration {
    fn source_table(&self) -> &'static str {
        "praxis_calendar"
    }

    fn target_table(&self) -> &'static str {
        "entities_person"
    }

    fn create(&self, _context: &TransformContext) -> Result<Box<dyn OntologyTransform>> {
        Ok(Box::new(PeopleResolutionTransform))
    }
}

inventory::submit! {
    &PeopleResolutionRegistration as &dyn TransformRegistration
}

/// Helper function to create a ChainedTransform for place resolution
///
/// Use this in location transforms to chain to entity resolution.
pub fn chain_to_place_resolution(source_id: Uuid) -> ChainedTransform {
    ChainedTransform {
        source_table: "location_point".to_string(),
        target_tables: vec!["location_visit".to_string(), "entities_place".to_string()],
        domain: "location".to_string(),
        source_record_id: source_id,
        transform_stage: "entity_resolution".to_string(),
    }
}

/// Helper function to create a ChainedTransform for people resolution
///
/// Use this in calendar transforms to chain to entity resolution.
pub fn chain_to_people_resolution(source_id: Uuid) -> ChainedTransform {
    ChainedTransform {
        source_table: "praxis_calendar".to_string(),
        target_tables: vec!["entities_person".to_string()],
        domain: "social".to_string(),
        source_record_id: source_id,
        transform_stage: "entity_resolution".to_string(),
    }
}

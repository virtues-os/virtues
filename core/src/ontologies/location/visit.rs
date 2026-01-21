//! Location visit ontology
//!
//! Clustered visits derived from location_point via the place resolution job.
//! This is a derived ontology - not sourced directly from a stream.

use crate::ontologies::{Ontology, OntologyBuilder, OntologyDescriptor};

pub struct LocationVisitOntology;

impl OntologyDescriptor for LocationVisitOntology {
    fn descriptor() -> Ontology {
        OntologyBuilder::new("location_visit")
            .display_name("Location Visits")
            .description("Clustered location visits with place resolution")
            .domain("location")
            .table_name("location_visit")
            // No source_streams - derived from location_point via clustering job
            .timestamp_column("arrival_time")
            .end_timestamp_column("departure_time")
            .build()
    }
}

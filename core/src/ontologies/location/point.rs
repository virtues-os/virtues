//! Location point ontology
//!
//! Raw GPS coordinates from iOS location services.
//! This is the source data that gets clustered into visits.

use crate::ontologies::{Ontology, OntologyBuilder, OntologyDescriptor};

pub struct LocationPointOntology;

impl OntologyDescriptor for LocationPointOntology {
    fn descriptor() -> Ontology {
        OntologyBuilder::new("location_point")
            .display_name("Location Points")
            .description("Raw GPS coordinates from device location services")
            .domain("location")
            .table_name("location_point")
            .source_streams(vec!["stream_ios_location"])
            .timestamp_column("timestamp")
            .build()
    }
}

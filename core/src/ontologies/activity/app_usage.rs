//! App usage ontology
//!
//! Application focus events from macOS.

use crate::ontologies::{Ontology, OntologyBuilder, OntologyDescriptor};

pub struct AppUsageOntology;

impl OntologyDescriptor for AppUsageOntology {
    fn descriptor() -> Ontology {
        OntologyBuilder::new("activity_app_usage")
            .display_name("App Usage")
            .description("Application focus events from macOS")
            .domain("activity")
            .table_name("activity_app_usage")
            .source_streams(vec!["stream_mac_apps"])
            .build()
    }
}

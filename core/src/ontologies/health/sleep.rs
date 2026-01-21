//! Sleep ontology
//!
//! Sleep sessions from HealthKit with start/end times and quality metrics.

use crate::ontologies::{Ontology, OntologyBuilder, OntologyDescriptor};

pub struct SleepOntology;

impl OntologyDescriptor for SleepOntology {
    fn descriptor() -> Ontology {
        OntologyBuilder::new("health_sleep")
            .display_name("Sleep Sessions")
            .description("Sleep analysis from HealthKit with quality metrics")
            .domain("health")
            .table_name("health_sleep")
            .source_streams(vec!["stream_ios_healthkit"])
            .timestamp_column("start_time")
            .end_timestamp_column("end_time")
            .build()
    }
}

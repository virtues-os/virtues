//! Steps ontology
//!
//! Step count data from HealthKit.

use crate::ontologies::{Ontology, OntologyBuilder, OntologyDescriptor};

pub struct StepsOntology;

impl OntologyDescriptor for StepsOntology {
    fn descriptor() -> Ontology {
        OntologyBuilder::new("health_steps")
            .display_name("Steps")
            .description("Step count data from HealthKit")
            .domain("health")
            .table_name("health_steps")
            .source_streams(vec!["stream_ios_healthkit"])
            .timestamp_column("timestamp")
            .build()
    }
}

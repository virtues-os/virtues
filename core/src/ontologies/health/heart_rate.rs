//! Heart rate ontology
//!
//! Heart rate measurements from HealthKit.

use crate::ontologies::{Ontology, OntologyBuilder, OntologyDescriptor};

pub struct HeartRateOntology;

impl OntologyDescriptor for HeartRateOntology {
    fn descriptor() -> Ontology {
        OntologyBuilder::new("health_heart_rate")
            .display_name("Heart Rate")
            .description("Heart rate measurements from HealthKit")
            .domain("health")
            .table_name("health_heart_rate")
            .source_streams(vec!["stream_ios_healthkit"])
            .build()
    }
}

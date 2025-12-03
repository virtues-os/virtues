//! HRV (Heart Rate Variability) ontology
//!
//! HRV measurements indicating stress/recovery state.

use crate::ontologies::{Ontology, OntologyBuilder, OntologyDescriptor};

pub struct HrvOntology;

impl OntologyDescriptor for HrvOntology {
    fn descriptor() -> Ontology {
        OntologyBuilder::new("health_hrv")
            .display_name("Heart Rate Variability")
            .description("HRV measurements indicating stress and recovery")
            .domain("health")
            .table_name("health_hrv")
            .source_streams(vec!["stream_ios_healthkit"])
            .build()
    }
}

//! Pressure ontology

use crate::ontologies::{Ontology, OntologyBuilder, OntologyDescriptor};

pub struct PressureOntology;

impl OntologyDescriptor for PressureOntology {
    fn descriptor() -> Ontology {
        OntologyBuilder::new("environment_pressure")
            .display_name("Atmospheric Pressure")
            .description("Barometric pressure and relative altitude changes")
            .domain("environment")
            .table_name("environment_pressure")
            .source_streams(vec!["stream_ios_barometer"])
            .timestamp_column("timestamp")
            .build()
    }
}

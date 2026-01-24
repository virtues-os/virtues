//! Battery ontology

use crate::ontologies::{Ontology, OntologyBuilder, OntologyDescriptor};

pub struct BatteryOntology;

impl OntologyDescriptor for BatteryOntology {
    fn descriptor() -> Ontology {
        OntologyBuilder::new("device_battery")
            .display_name("Battery Status")
            .description("Device battery level and charging state telemetry")
            .domain("device")
            .table_name("device_battery")
            .source_streams(vec!["stream_ios_battery"])
            .timestamp_column("timestamp")
            .build()
    }
}

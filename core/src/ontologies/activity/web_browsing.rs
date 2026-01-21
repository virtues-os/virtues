//! Web browsing ontology
//!
//! Browser history from macOS browsers.

use crate::ontologies::{Ontology, OntologyBuilder, OntologyDescriptor};

pub struct WebBrowsingOntology;

impl OntologyDescriptor for WebBrowsingOntology {
    fn descriptor() -> Ontology {
        OntologyBuilder::new("activity_web_browsing")
            .display_name("Web Browsing")
            .description("Browser history from Safari and Chrome")
            .domain("activity")
            .table_name("activity_web_browsing")
            .source_streams(vec!["stream_mac_browser"])
            .timestamp_column("timestamp")
            .build()
    }
}

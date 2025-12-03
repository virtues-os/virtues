//! Message ontology
//!
//! SMS and iMessage data from macOS.

use crate::ontologies::{Ontology, OntologyBuilder, OntologyDescriptor};

pub struct MessageOntology;

impl OntologyDescriptor for MessageOntology {
    fn descriptor() -> Ontology {
        OntologyBuilder::new("social_message")
            .display_name("Messages")
            .description("SMS and iMessage conversations")
            .domain("social")
            .table_name("social_message")
            .source_streams(vec!["stream_mac_imessage"])
            .embedding(
                "'From ' || COALESCE(from_name, 'Unknown') || ': ' || COALESCE(body, '')",
                "message",
                None,
                "LEFT(body, 200)",
                Some("from_name"),
                "timestamp",
            )
            .build()
    }
}

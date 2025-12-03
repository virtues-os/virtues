//! Email ontology
//!
//! Email messages from Gmail and other providers.

use crate::ontologies::{Ontology, OntologyBuilder, OntologyDescriptor};

pub struct EmailOntology;

impl OntologyDescriptor for EmailOntology {
    fn descriptor() -> Ontology {
        OntologyBuilder::new("social_email")
            .display_name("Email")
            .description("Email messages from Gmail and other providers")
            .domain("social")
            .table_name("social_email")
            .source_streams(vec!["stream_google_gmail"])
            .embedding(
                "COALESCE(subject, '') || '\n\n' || COALESCE(body_plain, '')",
                "email",
                Some("subject"),
                "COALESCE(LEFT(snippet, 200), LEFT(body_plain, 200), '')",
                Some("from_name"),
                "timestamp",
            )
            .build()
    }
}

//! Calendar ontology
//!
//! Calendar events from Google Calendar and other providers.

use crate::ontologies::{Ontology, OntologyBuilder, OntologyDescriptor};

pub struct CalendarOntology;

impl OntologyDescriptor for CalendarOntology {
    fn descriptor() -> Ontology {
        OntologyBuilder::new("praxis_calendar")
            .display_name("Calendar Events")
            .description("Scheduled events from Google Calendar")
            .domain("praxis")
            .table_name("praxis_calendar")
            .source_streams(vec!["stream_google_calendar"])
            .embedding(
                "COALESCE(title, '') || '\n\n' || COALESCE(description, '')",
                "calendar",
                Some("title"),
                "COALESCE(LEFT(description, 200), '')",
                None,
                "start_time",
            )
            .build()
    }
}

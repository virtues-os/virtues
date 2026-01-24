//! Calendar event ontology
//!
//! Calendar events from Google Calendar and other providers.

use crate::ontologies::{Ontology, OntologyBuilder, OntologyDescriptor};

pub struct CalendarEventOntology;

impl OntologyDescriptor for CalendarEventOntology {
    fn descriptor() -> Ontology {
        OntologyBuilder::new("calendar")
            .display_name("Calendar Events")
            .description("Scheduled events from Google Calendar")
            .domain("calendar")
            .table_name("calendar")
            .source_streams(vec!["stream_google_calendar"])
            .timestamp_column("start_time")
            .end_timestamp_column("end_time")
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

//! Document ontology
//!
//! Documents from Notion and other knowledge management tools.

use crate::ontologies::{Ontology, OntologyBuilder, OntologyDescriptor};

pub struct DocumentOntology;

impl OntologyDescriptor for DocumentOntology {
    fn descriptor() -> Ontology {
        OntologyBuilder::new("knowledge_document")
            .display_name("Documents")
            .description("Pages from Notion and other document sources")
            .domain("knowledge")
            .table_name("knowledge_document")
            .source_streams(vec!["stream_notion_pages"])
            .timestamp_column("created_time")
            .embedding(
                "COALESCE(title, '') || '\n\n' || COALESCE(content_summary, LEFT(content, 8000), '')",
                "document",
                Some("title"),
                "COALESCE(LEFT(content_summary, 200), LEFT(content, 200), '')",
                Some("source_provider"),
                "COALESCE(last_modified_time, created_at)",
            )
            .build()
    }
}

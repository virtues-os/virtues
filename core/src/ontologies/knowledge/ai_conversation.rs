//! AI Conversation ontology
//!
//! Exported chat sessions from Virtues AI conversations.

use crate::ontologies::{Ontology, OntologyBuilder, OntologyDescriptor};

pub struct AiConversationOntology;

impl OntologyDescriptor for AiConversationOntology {
    fn descriptor() -> Ontology {
        OntologyBuilder::new("knowledge_ai_conversation")
            .display_name("AI Conversations")
            .description("Chat sessions from Virtues AI assistant")
            .domain("knowledge")
            .table_name("knowledge_ai_conversation")
            .source_streams(vec!["stream_virtues_ai_chat"])
            .timestamp_column("timestamp")
            .embedding(
                "content",
                "ai_conversation",
                None,
                "LEFT(content, 200)",
                Some("role"),
                "timestamp",
            )
            .build()
    }
}

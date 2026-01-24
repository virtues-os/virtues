//! AI Conversation ontology
//!
//! Chat sessions from Virtues AI assistant, stored directly by the chat system.
//! Note: This ontology doesn't have a source stream - messages are created
//! directly by the chat API, not transformed from raw stream data.

use crate::ontologies::{Ontology, OntologyBuilder, OntologyDescriptor};

pub struct AiConversationOntology;

impl OntologyDescriptor for AiConversationOntology {
    fn descriptor() -> Ontology {
        OntologyBuilder::new("knowledge_ai_conversation")
            .display_name("AI Conversations")
            .description("Chat sessions from Virtues AI assistant")
            .domain("knowledge")
            .table_name("knowledge_ai_conversation")
            .source_streams(vec![]) // No source stream - messages created directly by chat API
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

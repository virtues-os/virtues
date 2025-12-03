//! Virtues Source Registry
//!
//! Defines the source descriptor for the internal Virtues web application source.

use crate::registry::{AuthType, RegisteredSource, SourceRegistry, RegisteredStream};
use serde_json::json;

/// Virtues source registration
pub struct VirtuesSource;

impl SourceRegistry for VirtuesSource {
    fn descriptor() -> RegisteredSource {
        RegisteredSource {
            name: "virtues",
            display_name: "Virtues",
            description: "Internal operational data from Virtues web application",
            auth_type: AuthType::None, // Internal source, no external auth needed
            oauth_config: None,
            icon: Some("ri:app-store-fill"),
            streams: vec![RegisteredStream::new("app_export")
                .display_name("Chat Export")
                .description("Exports chat sessions from app.chat_sessions to ELT pipeline")
                .table_name("stream_virtues_ai_chat")
                .target_ontologies(vec!["knowledge_ai_conversation"])
                .config_schema(json!({}))
                .config_example(json!({}))
                .supports_incremental(true)
                .supports_full_refresh(true) // Allow full refresh for recovery scenarios
                .default_cron_schedule("0 */5 * * * *") // Every 5 minutes (6-field: sec min hour day month dow)
                .build()],
        }
    }
}

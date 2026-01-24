//! Notion source registration for the catalog
//!
//! This module provides the unified registration for Notion sources, including
//! both UI metadata and transform logic in a single place.

use crate::registry::{RegisteredSource, RegisteredStream, SourceRegistry};
use serde_json::json;

// Import transform for unified registration
use super::pages::transform::NotionPageTransform;

/// Notion source registration
pub struct NotionSource;

impl SourceRegistry for NotionSource {
    fn descriptor() -> RegisteredSource {
        let descriptor = virtues_registry::sources::get_source("notion")
            .expect("Notion source not found in virtues-registry");

        RegisteredSource {
            descriptor,
            streams: vec![
                // Pages stream with unified transform
                RegisteredStream::new("pages")
                    .config_schema(pages_config_schema())
                    .config_example(pages_config_example())
                    .transform("knowledge_document", |_ctx| Ok(Box::new(NotionPageTransform)))
                    .build(),
            ],
        }
    }
}

/// JSON schema for Notion pages configuration
fn pages_config_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "database_ids": {
                "type": "array",
                "items": { "type": "string" },
                "description": "List of database IDs to sync (leave empty to sync all accessible pages)"
            },
            "include_archived": {
                "type": "boolean",
                "default": false,
                "description": "Include archived pages"
            },
            "sync_strategy": {
                "type": "object",
                "description": "Strategy for sync operations (Note: Notion API doesn't support time-based filtering, so only full_history is effective)",
                "default": {
                    "type": "full_history",
                    "max_records": null
                },
                "oneOf": [
                    {
                        "type": "object",
                        "required": ["type"],
                        "properties": {
                            "type": { "const": "full_history" },
                            "max_records": {
                                "type": "integer",
                                "nullable": true,
                                "description": "Optional limit on number of pages to sync"
                            }
                        }
                    }
                ]
            },
            "page_size": {
                "type": "integer",
                "default": 100,
                "minimum": 1,
                "maximum": 100,
                "description": "Number of pages per API request batch"
            }
        }
    })
}

/// Example configuration for Notion pages
fn pages_config_example() -> serde_json::Value {
    json!({
        "database_ids": [],
        "include_archived": false,
        "sync_strategy": {
            "type": "full_history",
            "max_records": null
        },
        "page_size": 100
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::AuthType;

    #[test]
    fn test_notion_descriptor() {
        let desc = NotionSource::descriptor();
        assert_eq!(desc.descriptor.name, "notion");
        assert_eq!(desc.descriptor.auth_type, AuthType::OAuth2);
        assert!(desc.descriptor.oauth_config.is_some());
        assert_eq!(desc.streams.len(), 1);
    }

    #[test]
    fn test_pages_stream() {
        let desc = NotionSource::descriptor();
        let pages = desc.streams.iter().find(|s| s.descriptor.name == "pages");
        assert!(pages.is_some());

        let p = pages.unwrap();
        assert_eq!(p.descriptor.table_name, "stream_notion_pages");
        assert!(!p.descriptor.supports_incremental); // Notion doesn't support incremental sync
        assert!(p.descriptor.supports_full_refresh);
    }
}

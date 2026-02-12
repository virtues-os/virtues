//! GitHub source registration for the catalog
//!
//! This module provides the unified registration for GitHub sources, including
//! UI metadata, transform logic, and stream creation in a single place.

use crate::registry::{RegisteredSource, RegisteredStream, SourceRegistry};
use serde_json::json;

// Import transforms and stream types for unified registration
use super::events::{transform::GitHubBookmarkTransform, GitHubEventsStream};

/// GitHub source registration
pub struct GitHubSource;

impl SourceRegistry for GitHubSource {
    fn descriptor() -> RegisteredSource {
        // Metadata is now in virtues-registry
        let descriptor = virtues_registry::sources::get_source("github")
            .expect("GitHub source not found in virtues-registry");

        RegisteredSource {
            descriptor,
            streams: vec![
                // Events stream with transform and stream creator
                RegisteredStream::new("events")
                    .config_schema(events_config_schema())
                    .config_example(events_config_example())
                    .transform("content_bookmark", |_ctx| {
                        Ok(Box::new(GitHubBookmarkTransform))
                    })
                    .stream_creator(|ctx| {
                        Ok(crate::sources::stream_type::StreamType::Pull(Box::new(
                            GitHubEventsStream::new(
                                ctx.source_id.clone(),
                                ctx.db.clone(),
                                ctx.stream_writer.clone(),
                                ctx.auth.clone(),
                            ),
                        )))
                    })
                    .build(),
            ],
        }
    }
}

/// JSON schema for GitHub events configuration
fn events_config_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "max_pages": {
                "type": "integer",
                "default": 4,
                "minimum": 1,
                "maximum": 10,
                "description": "Maximum number of pages to fetch per sync (100 events per page)"
            }
        }
    })
}

/// Example configuration for GitHub events
fn events_config_example() -> serde_json::Value {
    json!({
        "max_pages": 4
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::AuthType;

    #[test]
    fn test_github_descriptor() {
        let desc = GitHubSource::descriptor();
        assert_eq!(desc.descriptor.name, "github");
        assert_eq!(desc.descriptor.auth_type, AuthType::OAuth2);
        assert!(desc.descriptor.oauth_config.is_some());
        assert_eq!(desc.streams.len(), 1);
    }

    #[test]
    fn test_events_stream() {
        let desc = GitHubSource::descriptor();
        let events = desc.streams.iter().find(|s| s.descriptor.name == "events");
        assert!(events.is_some());

        let ev = events.unwrap();
        assert_eq!(ev.descriptor.table_name, "stream_github_events");
        assert!(ev.descriptor.supports_incremental);
        assert!(ev.descriptor.supports_full_refresh);
        assert!(ev.descriptor.enabled);
    }

    #[test]
    fn test_config_schemas_valid() {
        let schema = events_config_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"].is_object());
    }
}

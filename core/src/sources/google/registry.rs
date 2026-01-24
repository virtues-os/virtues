//! Google source registration for the catalog
//!
//! This module provides the unified registration for Google sources, including
//! both UI metadata, transform logic, and stream creation in a single place.

use crate::registry::{RegisteredSource, RegisteredStream, SourceRegistry};
use crate::sources::base::SyncStrategy;
use crate::sources::stream_type::StreamType;
use serde_json::json;

// Import transforms and stream types for unified registration
use super::calendar::{transform::GoogleCalendarTransform, GoogleCalendarStream};
use super::gmail::{transform::GmailEmailTransform, GoogleGmailStream};

/// Google source registration
pub struct GoogleSource;

impl SourceRegistry for GoogleSource {
    fn descriptor() -> RegisteredSource {
        // Metadata is now in virtues-registry
        let descriptor = virtues_registry::sources::get_source("google")
            .expect("Google source not found in virtues-registry");

        RegisteredSource {
            descriptor,
            streams: vec![
                // Calendar stream with unified transform and stream creator registration
                RegisteredStream::new("calendar")
                    .config_schema(calendar_config_schema())
                    .config_example(calendar_config_example())
                    .transform("calendar", |_ctx| Ok(Box::new(GoogleCalendarTransform)))
                    .stream_creator(|ctx| {
                        Ok(StreamType::Pull(Box::new(GoogleCalendarStream::new(
                            ctx.source_id.clone(),
                            ctx.db.clone(),
                            ctx.stream_writer.clone(),
                            ctx.auth.clone(),
                        ))))
                    })
                    .build(),
                // Gmail stream with unified transform and stream creator registration
                RegisteredStream::new("gmail")
                    .config_schema(gmail_config_schema())
                    .config_example(gmail_config_example())
                    .transform("social_email", |_ctx| Ok(Box::new(GmailEmailTransform)))
                    .stream_creator(|ctx| {
                        Ok(StreamType::Pull(Box::new(GoogleGmailStream::new(
                            ctx.source_id.clone(),
                            ctx.db.clone(),
                            ctx.stream_writer.clone(),
                            ctx.auth.clone(),
                        ))))
                    })
                    .build(),
            ],
        }
    }
}

/// JSON schema for GoogleCalendarConfig
fn calendar_config_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "calendar_ids": {
                "type": "array",
                "items": { "type": "string" },
                "default": ["primary"],
                "description": "List of calendar IDs to sync (use 'primary' for main calendar)"
            },
            "sync_strategy": SyncStrategy::json_schema(),
            "include_declined": {
                "type": "boolean",
                "default": false,
                "description": "Include events where you declined the invitation"
            },
            "include_cancelled": {
                "type": "boolean",
                "default": false,
                "description": "Include cancelled events"
            },
            "max_events_per_sync": {
                "type": "integer",
                "default": 500,
                "minimum": 1,
                "maximum": 2500,
                "description": "Maximum number of events to fetch per sync operation"
            }
        }
    })
}

/// Example configuration for Google Calendar
fn calendar_config_example() -> serde_json::Value {
    json!({
        "calendar_ids": ["primary", "work@company.com"],
        "sync_strategy": {
            "type": "time_window",
            "days_back": 365
        },
        "include_declined": false,
        "include_cancelled": false,
        "max_events_per_sync": 500
    })
}

/// JSON schema for GoogleGmailConfig
fn gmail_config_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "label_ids": {
                "type": "array",
                "items": { "type": "string" },
                "default": ["INBOX", "SENT"],
                "description": "Gmail labels to sync (e.g., INBOX, SENT, DRAFT)"
            },
            "include_spam_trash": {
                "type": "boolean",
                "default": false,
                "description": "Include spam and trash folders"
            },
            "sync_mode": {
                "type": "string",
                "enum": ["messages", "threads"],
                "default": "messages",
                "description": "Sync individual messages or conversation threads"
            },
            "fetch_body": {
                "type": "boolean",
                "default": true,
                "description": "Fetch full message body content"
            },
            "sync_strategy": SyncStrategy::json_schema(),
            "max_messages_per_sync": {
                "type": "integer",
                "default": 500,
                "minimum": 1,
                "maximum": 1000,
                "description": "Maximum number of messages to fetch per sync"
            },
            "query": {
                "type": "string",
                "description": "Gmail search query filter (optional, uses Gmail search syntax)"
            }
        }
    })
}

/// Example configuration for Gmail
fn gmail_config_example() -> serde_json::Value {
    json!({
        "label_ids": ["INBOX", "SENT"],
        "include_spam_trash": false,
        "sync_mode": "messages",
        "fetch_body": true,
        "sync_strategy": {
            "type": "time_window",
            "days_back": 365
        },
        "max_messages_per_sync": 500,
        "query": null
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::AuthType;

    #[test]
    fn test_google_descriptor() {
        let desc = GoogleSource::descriptor();
        assert_eq!(desc.descriptor.name, "google");
        assert_eq!(desc.descriptor.auth_type, AuthType::OAuth2);
        assert!(desc.descriptor.oauth_config.is_some());
        assert_eq!(desc.streams.len(), 2);
    }

    #[test]
    fn test_calendar_stream() {
        let desc = GoogleSource::descriptor();
        let calendar = desc.streams.iter().find(|s| s.descriptor.name == "calendar");
        assert!(calendar.is_some());

        let cal = calendar.unwrap();
        assert_eq!(cal.descriptor.table_name, "stream_google_calendar");
        assert!(cal.descriptor.supports_incremental);
        assert!(cal.descriptor.supports_full_refresh);
    }

    #[test]
    fn test_gmail_stream() {
        let desc = GoogleSource::descriptor();
        let gmail = desc.streams.iter().find(|s| s.descriptor.name == "gmail");
        assert!(gmail.is_some());

        let gm = gmail.unwrap();
        assert_eq!(gm.descriptor.table_name, "stream_google_gmail");
        assert!(gm.descriptor.supports_incremental);
        // Gmail is disabled in virtues-registry by default
        assert!(!gm.descriptor.enabled);
    }

    #[test]
    fn test_config_schemas_valid() {
        // Ensure schemas are valid JSON
        let cal_schema = calendar_config_schema();
        assert_eq!(cal_schema["type"], "object");
        assert!(cal_schema["properties"].is_object());

        let gmail_schema = gmail_config_schema();
        assert_eq!(gmail_schema["type"], "object");
        assert!(gmail_schema["properties"].is_object());
    }
}

//! Strava source registration for the catalog
//!
//! This module provides the unified registration for Strava sources, including
//! both UI metadata, transform logic, and stream creation in a single place.

use crate::registry::{RegisteredSource, RegisteredStream, SourceRegistry};
use crate::sources::base::SyncStrategy;
use crate::sources::stream_type::StreamType;
use serde_json::json;

// Import transforms and stream types for unified registration
use super::activities::{transform::StravaWorkoutTransform, StravaActivitiesStream};

/// Strava source registration
pub struct StravaSource;

impl SourceRegistry for StravaSource {
    fn descriptor() -> RegisteredSource {
        // Metadata is now in virtues-registry
        let descriptor = virtues_registry::sources::get_source("strava")
            .expect("Strava source not found in virtues-registry");

        RegisteredSource {
            descriptor,
            streams: vec![
                // Activities stream with unified transform and stream creator registration
                RegisteredStream::new("activities")
                    .config_schema(activities_config_schema())
                    .config_example(activities_config_example())
                    .transform("health_workout", |_ctx| Ok(Box::new(StravaWorkoutTransform)))
                    .stream_creator(|ctx| {
                        Ok(StreamType::Pull(Box::new(StravaActivitiesStream::new(
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

/// JSON schema for Strava activities configuration
fn activities_config_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "sync_strategy": SyncStrategy::json_schema(),
            "per_page": {
                "type": "integer",
                "default": 200,
                "minimum": 1,
                "maximum": 200,
                "description": "Number of activities per API page (max 200)"
            }
        }
    })
}

/// Example configuration for Strava activities
fn activities_config_example() -> serde_json::Value {
    json!({
        "sync_strategy": {
            "type": "time_window",
            "days_back": 365
        },
        "per_page": 200
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::AuthType;

    #[test]
    fn test_strava_descriptor() {
        let desc = StravaSource::descriptor();
        assert_eq!(desc.descriptor.name, "strava");
        assert_eq!(desc.descriptor.auth_type, AuthType::OAuth2);
        assert!(desc.descriptor.oauth_config.is_some());
        assert_eq!(desc.streams.len(), 1);
    }

    #[test]
    fn test_activities_stream() {
        let desc = StravaSource::descriptor();
        let activities = desc.streams.iter().find(|s| s.descriptor.name == "activities");
        assert!(activities.is_some());

        let act = activities.unwrap();
        assert_eq!(act.descriptor.table_name, "stream_strava_activities");
        assert!(act.descriptor.supports_incremental);
        assert!(act.descriptor.supports_full_refresh);
    }

    #[test]
    fn test_config_schemas_valid() {
        // Ensure schemas are valid JSON
        let schema = activities_config_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"].is_object());
    }
}

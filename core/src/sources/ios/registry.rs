//! iOS source registration for the catalog
//!
//! This module provides the unified registration for iOS sources, including
//! both UI metadata, transform logic, and stream creation in a single place.

use crate::registry::{RegisteredSource, RegisteredStream, SourceRegistry};
use crate::sources::stream_type::StreamType;
use serde_json::json;

// Import transforms for unified registration
use super::healthkit::transform::{
    HealthKitHeartRateTransform, HealthKitHRVTransform, HealthKitSleepTransform,
    HealthKitStepsTransform, HealthKitWorkoutTransform,
};
use super::financekit::transform::FinanceKitTransactionTransform;
use super::location::transform::IosLocationTransform;
use super::battery::transform::IosBatteryTransform;
use super::barometer::transform::IosBarometerTransform;

// Import stream types for unified registration
use super::{
    IosBarometerStream, IosBatteryStream, IosContactsStream, IosFinanceKitStream,
    IosHealthKitStream, IosLocationStream, IosMicrophoneStream,
};

/// iOS source registration
pub struct IosSource;

impl SourceRegistry for IosSource {
    fn descriptor() -> RegisteredSource {
        let descriptor = virtues_registry::sources::get_source("ios")
            .expect("iOS source not found in virtues-registry");

        RegisteredSource {
            descriptor,
            streams: vec![
                // HealthKit stream with unified transforms and stream creator
                RegisteredStream::new("healthkit")
                    .config_schema(healthkit_config_schema())
                    .config_example(healthkit_config_example())
                    .transform("health_heart_rate", |_ctx| Ok(Box::new(HealthKitHeartRateTransform)))
                    .transform("health_hrv", |_ctx| Ok(Box::new(HealthKitHRVTransform)))
                    .transform("health_steps", |_ctx| Ok(Box::new(HealthKitStepsTransform)))
                    .transform("health_sleep", |_ctx| Ok(Box::new(HealthKitSleepTransform)))
                    .transform("health_workout", |_ctx| Ok(Box::new(HealthKitWorkoutTransform)))
                    .stream_creator(|ctx| {
                        Ok(StreamType::Push(Box::new(IosHealthKitStream::new(
                            ctx.db.clone(),
                            ctx.stream_writer.clone(),
                        ))))
                    })
                    .build(),

                // Location stream with unified transform and stream creator
                RegisteredStream::new("location")
                    .config_schema(location_config_schema())
                    .config_example(location_config_example())
                    .transform("location_point", |_ctx| Ok(Box::new(IosLocationTransform)))
                    .stream_creator(|ctx| {
                        Ok(StreamType::Push(Box::new(IosLocationStream::new(
                            ctx.db.clone(),
                            ctx.stream_writer.clone(),
                        ))))
                    })
                    .build(),

                // Microphone stream with stream creator (no transform yet - transcription handled separately)
                RegisteredStream::new("microphone")
                    .config_schema(microphone_config_schema())
                    .config_example(microphone_config_example())
                    .stream_creator(|ctx| {
                        Ok(StreamType::Push(Box::new(IosMicrophoneStream::new(
                            ctx.db.clone(),
                            ctx.storage.clone(),
                            ctx.stream_writer.clone(),
                        ))))
                    })
                    .build(),

                // Contacts stream with stream creator
                RegisteredStream::new("contacts")
                    .config_schema(serde_json::json!({}))
                    .config_example(serde_json::json!({}))
                    .stream_creator(|ctx| {
                        Ok(StreamType::Push(Box::new(IosContactsStream::new(
                            ctx.db.clone(),
                            ctx.stream_writer.clone(),
                        ))))
                    })
                    .build(),

                // Battery stream with stream creator
                RegisteredStream::new("battery")
                    .config_schema(serde_json::json!({}))
                    .config_example(serde_json::json!({}))
                    .transform("device_battery", |_ctx| Ok(Box::new(IosBatteryTransform)))
                    .stream_creator(|ctx| {
                        Ok(StreamType::Push(Box::new(IosBatteryStream::new(
                            ctx.db.clone(),
                            ctx.stream_writer.clone(),
                        ))))
                    })
                    .build(),

                // Barometer stream with stream creator
                RegisteredStream::new("barometer")
                    .config_schema(serde_json::json!({}))
                    .config_example(serde_json::json!({}))
                    .transform("environment_pressure", |_ctx| Ok(Box::new(IosBarometerTransform)))
                    .stream_creator(|ctx| {
                        Ok(StreamType::Push(Box::new(IosBarometerStream::new(
                            ctx.db.clone(),
                            ctx.stream_writer.clone(),
                        ))))
                    })
                    .build(),

                // FinanceKit stream with unified transform and stream creator
                RegisteredStream::new("financekit")
                    .config_schema(serde_json::json!({}))
                    .config_example(serde_json::json!({}))
                    .transform("financial_transaction", |_ctx| Ok(Box::new(FinanceKitTransactionTransform)))
                    .stream_creator(|ctx| {
                        Ok(StreamType::Push(Box::new(IosFinanceKitStream::new(
                            ctx.db.clone(),
                            ctx.stream_writer.clone(),
                        ))))
                    })
                    .build(),
            ],
        }
    }
}

/// JSON schema for HealthKit configuration
fn healthkit_config_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "enabled_metrics": {
                "type": "array",
                "items": { "type": "string" },
                "description": "List of metrics to collect: heart_rate, steps, sleep, workouts, etc."
            },
            "sampling_interval_seconds": {
                "type": "integer",
                "default": 60,
                "minimum": 10,
                "description": "How often to sample health metrics (in seconds)"
            }
        }
    })
}

fn healthkit_config_example() -> serde_json::Value {
    json!({
        "enabled_metrics": ["heart_rate", "steps", "sleep"],
        "sampling_interval_seconds": 60
    })
}

/// JSON schema for Location configuration
fn location_config_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "accuracy": {
                "type": "string",
                "enum": ["best", "high", "medium", "low"],
                "default": "high",
                "description": "GPS accuracy level"
            },
            "update_interval_seconds": {
                "type": "integer",
                "default": 30,
                "minimum": 5,
                "description": "Location update frequency (in seconds)"
            },
            "enable_background": {
                "type": "boolean",
                "default": false,
                "description": "Track location in background"
            }
        }
    })
}

fn location_config_example() -> serde_json::Value {
    json!({
        "accuracy": "high",
        "update_interval_seconds": 30,
        "enable_background": false
    })
}

/// JSON schema for Microphone configuration
fn microphone_config_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "enable_transcription": {
                "type": "boolean",
                "default": false,
                "description": "Enable speech-to-text transcription"
            },
            "store_audio": {
                "type": "boolean",
                "default": false,
                "description": "Store raw audio files in MinIO"
            },
            "sample_duration_seconds": {
                "type": "integer",
                "default": 5,
                "minimum": 1,
                "maximum": 60,
                "description": "Duration of each audio sample"
            }
        }
    })
}

fn microphone_config_example() -> serde_json::Value {
    json!({
        "enable_transcription": true,
        "store_audio": false,
        "sample_duration_seconds": 5
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::AuthType;

    #[test]
    fn test_ios_descriptor() {
        let desc = IosSource::descriptor();
        assert_eq!(desc.descriptor.name, "ios");
        assert_eq!(desc.descriptor.auth_type, AuthType::Device);
        assert_eq!(desc.streams.len(), 7);
    }

    #[test]
    fn test_healthkit_stream() {
        let desc = IosSource::descriptor();
        let healthkit = desc
            .streams
            .iter()
            .find(|s| s.descriptor.name == "healthkit")
            .expect("HealthKit stream not found");

        assert_eq!(healthkit.descriptor.display_name, "HealthKit");
        assert_eq!(healthkit.descriptor.table_name, "stream_ios_healthkit");
        assert!(!healthkit.descriptor.supports_incremental);
    }

    #[test]
    fn test_location_stream() {
        let desc = IosSource::descriptor();
        let location = desc
            .streams
            .iter()
            .find(|s| s.descriptor.name == "location")
            .expect("Location stream not found");

        assert_eq!(location.descriptor.display_name, "Location");
        assert_eq!(location.descriptor.table_name, "stream_ios_location");
    }

    #[test]
    fn test_microphone_stream() {
        let desc = IosSource::descriptor();
        let mic = desc
            .streams
            .iter()
            .find(|s| s.descriptor.name == "microphone")
            .expect("Microphone stream not found");

        assert_eq!(mic.descriptor.display_name, "Microphone");
        assert_eq!(mic.descriptor.table_name, "stream_ios_microphone");
    }
}

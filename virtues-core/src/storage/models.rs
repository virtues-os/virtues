//! Data models for stream object storage metadata

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::types::Timestamp;

/// Metadata for a stream data object stored on the local filesystem.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StreamObject {
    pub id: String,
    pub source_id: String,
    pub stream_name: String,
    pub storage_key: String,
    pub record_count: i32,
    pub size_bytes: i64,
    pub min_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub max_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Transform checkpoint tracking which objects have been processed
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StreamTransformCheckpoint {
    pub id: String,
    pub source_id: String,
    pub stream_name: String,
    pub transform_name: String,
    pub last_processed_storage_key: Option<String>,
    pub last_processed_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub last_processed_object_id: Option<String>,
    pub records_processed: i64,
    pub objects_processed: i64,
    pub last_run_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// User profile - biographical metadata (singleton table)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserProfile {
    pub id: String,
    // Identity
    pub full_name: Option<String>,
    pub preferred_name: Option<String>,
    pub birth_date: Option<String>,
    // Physical/Biometric (f64 / double precision)
    pub height_cm: Option<f64>,
    pub weight_kg: Option<f64>,
    pub ethnicity: Option<String>,
    // Work/Occupation
    pub occupation: Option<String>,
    pub employer: Option<String>,
    // Home place (FK to entities_place)
    pub home_place_id: Option<String>,
    // Onboarding - single status field (deprecated, kept for compatibility)
    pub onboarding_status: String,
    // Server status - controls provisioning state (set by virtues-api hydration)
    pub server_status: String,
    // Preferences
    pub theme: Option<String>,
    /// Timezone of the box's physical home location (IANA). Stable anchor +
    /// fallback floor — NOT the owner's current location. See docs/timezone-model.md.
    pub home_timezone: Option<String>,
    // Discovery context
    pub crux: Option<String>,
    pub technology_vision: Option<String>,
    pub pain_point_primary: Option<String>,
    pub pain_point_secondary: Option<String>,
    pub excited_features: Option<serde_json::Value>,
    // Audit
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Assistant profile - AI assistant preferences (singleton table)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AssistantProfile {
    pub id: String,
    pub assistant_name: Option<String>,
    pub default_agent_id: Option<String>,
    // Legacy model fields (kept for backward compatibility)
    pub default_model_id: Option<String>,
    pub background_model_id: Option<String>,
    // Purpose-based model slots
    pub chat_model_id: Option<String>,
    pub lite_model_id: Option<String>,
    pub coding_model_id: Option<String>,
    pub image_model_id: Option<String>,
    pub enabled_tools: Option<serde_json::Value>,
    pub ui_preferences: Option<serde_json::Value>,
    pub embedding_model_id: Option<String>,
    /// AI persona/tone: selected persona ID
    pub persona: Option<String>,
    /// JSON blob storing persona definitions: { "items": [...], "hidden": [...] }
    /// Column is `jsonb` (migration 0003) — bind as `serde_json::Value`, not String.
    pub personas: Option<serde_json::Value>,
    /// AI-managed persistent memory. Column is `jsonb` (migration 0003).
    pub memory: Option<serde_json::Value>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Builder for stream-archive storage keys.
///
/// Keys look like:
/// `streams/{provider}/{source_id}/{stream_name}/date={YYYY-MM-DD}/records_{unix_timestamp}.jsonl`
pub struct StreamKeyBuilder {
    provider: String,
    source_id: String,
    stream_name: String,
    date: NaiveDate,
}

impl StreamKeyBuilder {
    pub fn new(
        provider: impl Into<String>,
        source_id: impl Into<String>,
        stream_name: impl Into<String>,
        date: NaiveDate,
    ) -> Self {
        Self {
            provider: provider.into(),
            source_id: source_id.into(),
            stream_name: stream_name.into(),
            date,
        }
    }

    /// Build a key with the current timestamp.
    pub fn build(&self) -> String {
        self.build_with_timestamp(chrono::Utc::now().timestamp())
    }

    /// Build a key with an explicit timestamp.
    pub fn build_with_timestamp(&self, timestamp: i64) -> String {
        format!(
            "streams/{}/{}/{}/date={}/records_{}.jsonl",
            self.provider,
            self.source_id,
            self.stream_name,
            self.date.format("%Y-%m-%d"),
            timestamp
        )
    }

    /// Prefix for listing every object for this source/stream.
    pub fn build_stream_prefix(&self) -> String {
        format!(
            "streams/{}/{}/{}/",
            self.provider, self.source_id, self.stream_name
        )
    }

    /// Prefix for listing every object for this source/stream/date.
    pub fn build_date_prefix(&self) -> String {
        format!(
            "streams/{}/{}/{}/date={}/",
            self.provider,
            self.source_id,
            self.stream_name,
            self.date.format("%Y-%m-%d")
        )
    }
}

/// Parser for extracting metadata from stream-archive storage keys.
pub struct StreamKeyParser {
    key: String,
}

impl StreamKeyParser {
    pub fn new(key: impl Into<String>) -> Self {
        Self { key: key.into() }
    }

    fn parts(&self) -> Vec<&str> {
        self.key.split('/').collect()
    }

    /// Extract provider — `streams/{provider}/...`.
    pub fn provider(&self) -> Option<String> {
        let parts = self.parts();
        if parts.first() != Some(&"streams") || parts.len() < 2 {
            return None;
        }
        Some(parts[1].to_string())
    }

    /// Extract source_id — `streams/{provider}/{source_id}/...`.
    pub fn source_id(&self) -> Option<String> {
        let parts = self.parts();
        if parts.first() != Some(&"streams") || parts.len() < 3 {
            return None;
        }
        Some(parts[2].to_string())
    }

    /// Extract stream name.
    pub fn stream_name(&self) -> Option<String> {
        let parts = self.parts();
        if parts.first() != Some(&"streams") || parts.len() < 4 {
            return None;
        }
        Some(parts[3].to_string())
    }

    /// Extract date from `date=YYYY-MM-DD` segment.
    pub fn date(&self) -> Option<NaiveDate> {
        let parts = self.parts();
        if parts.first() != Some(&"streams") || parts.len() < 5 {
            return None;
        }
        let date_str = parts[4].strip_prefix("date=")?;
        NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()
    }

    /// Extract timestamp from `records_{ts}.jsonl` filename.
    pub fn timestamp(&self) -> Option<i64> {
        let parts = self.parts();
        if parts.first() != Some(&"streams") || parts.len() < 6 {
            return None;
        }
        let ts = parts[5]
            .strip_prefix("records_")?
            .strip_suffix(".jsonl")?;
        ts.parse().ok()
    }

    /// Extract all metadata at once.
    pub fn parse_all(&self) -> Option<(String, String, String, NaiveDate, i64)> {
        Some((
            self.provider()?,
            self.source_id()?,
            self.stream_name()?,
            self.date()?,
            self.timestamp()?,
        ))
    }

    /// Static helper used by stream-encryption key derivation.
    pub fn parse_date_from_key(key: &str) -> crate::error::Result<NaiveDate> {
        Self::new(key).date().ok_or_else(|| {
            crate::error::Error::Other(format!("Failed to parse date from storage key: {}", key))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_key_builder() {
        let source_id = "source_ios-healthkit";
        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        let builder = StreamKeyBuilder::new("ios", source_id, "healthkit", date);

        assert_eq!(
            builder.build_with_timestamp(1736899200),
            "streams/ios/source_ios-healthkit/healthkit/date=2025-01-15/records_1736899200.jsonl"
        );
        assert_eq!(
            builder.build_stream_prefix(),
            "streams/ios/source_ios-healthkit/healthkit/"
        );
        assert_eq!(
            builder.build_date_prefix(),
            "streams/ios/source_ios-healthkit/healthkit/date=2025-01-15/"
        );
    }

    #[test]
    fn test_stream_key_parser() {
        let key =
            "streams/ios/source_ios-healthkit/healthkit/date=2025-01-15/records_1736899200.jsonl";
        let parser = StreamKeyParser::new(key);

        let (provider, source_id, stream_name, date, timestamp) = parser.parse_all().unwrap();
        assert_eq!(provider, "ios");
        assert_eq!(source_id, "source_ios-healthkit");
        assert_eq!(stream_name, "healthkit");
        assert_eq!(date, NaiveDate::from_ymd_opt(2025, 1, 15).unwrap());
        assert_eq!(timestamp, 1736899200);
    }

    #[test]
    fn test_stream_key_parser_invalid() {
        let parser = StreamKeyParser::new("invalid/key/format");
        assert!(parser.source_id().is_none());
        assert!(parser.stream_name().is_none());
        assert!(parser.date().is_none());
        assert!(parser.timestamp().is_none());
    }
}

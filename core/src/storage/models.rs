//! Data models for stream object storage metadata

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::types::Timestamp;

/// Metadata for a stream data object stored in storage (S3/MinIO/local)
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
    // Physical/Biometric (f64 for SQLite REAL)
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
    // Server status - controls provisioning state (set by Tollbooth hydration)
    pub server_status: String,
    // Preferences
    pub theme: Option<String>,
    pub update_check_hour: Option<i32>,
    // Discovery context
    pub crux: Option<String>,
    pub technology_vision: Option<String>,
    pub pain_point_primary: Option<String>,
    pub pain_point_secondary: Option<String>,
    pub excited_features: Option<String>,
    // Owner (Seed and Drift pattern)
    pub owner_email: Option<String>,
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
    pub default_model_id: Option<String>,
    pub background_model_id: Option<String>,
    pub enabled_tools: Option<serde_json::Value>,
    pub ui_preferences: Option<serde_json::Value>,
    pub embedding_model_id: Option<String>,
    pub ollama_endpoint: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Validate subdomain format for security
///
/// Only allows lowercase alphanumeric and hyphens, 1-63 chars, no leading/trailing hyphens.
/// This prevents path traversal attacks (e.g., `../../../other-tenant`).
///
/// # Arguments
/// * `subdomain` - The subdomain to validate
///
/// # Returns
/// `Ok(())` if valid, `Err` with message if invalid
pub fn validate_subdomain(subdomain: &str) -> Result<(), &'static str> {
    if subdomain.is_empty() || subdomain.len() > 63 {
        return Err("Subdomain must be 1-63 characters");
    }
    if subdomain.starts_with('-') || subdomain.ends_with('-') {
        return Err("Subdomain cannot start or end with hyphen");
    }
    if !subdomain
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("Subdomain must be lowercase alphanumeric with hyphens only");
    }
    Ok(())
}

/// S3 key builder for consistent object naming
///
/// Supports multi-tenant storage with optional subdomain prefix.
/// When `subdomain` is set, keys are prefixed with `tenants/{subdomain}/`.
///
/// # Security
///
/// The subdomain is validated to prevent path traversal attacks.
/// Only lowercase alphanumeric characters and hyphens are allowed.
pub struct StreamKeyBuilder {
    tenant_prefix: Option<String>,
    provider: String,
    source_id: String,
    stream_name: String,
    date: NaiveDate,
}

/// Error type for StreamKeyBuilder construction
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubdomainValidationError(pub &'static str);

impl std::fmt::Display for SubdomainValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for SubdomainValidationError {}

impl StreamKeyBuilder {
    /// Create a new key builder with validated subdomain
    ///
    /// # Arguments
    ///
    /// * `subdomain` - Optional subdomain (e.g., `Some("adamjace")`). Will be validated and prefixed.
    /// * `provider` - Source provider (e.g., "ios", "google")
    /// * `source_id` - Source connection ID (semantic string ID like "source_google-calendar")
    /// * `stream_name` - Stream name (e.g., "healthkit", "calendar")
    /// * `date` - Date for the data partition
    ///
    /// # Errors
    ///
    /// Returns `SubdomainValidationError` if the subdomain contains invalid characters,
    /// path traversal attempts, or doesn't meet format requirements.
    ///
    /// # Security
    ///
    /// The subdomain is strictly validated to prevent path traversal attacks.
    /// Only lowercase letters, digits, and hyphens are allowed.
    pub fn new(
        subdomain: Option<&str>,
        provider: impl Into<String>,
        source_id: impl Into<String>,
        stream_name: impl Into<String>,
        date: NaiveDate,
    ) -> Result<Self, SubdomainValidationError> {
        let tenant_prefix = match subdomain {
            Some(sub) => {
                validate_subdomain(sub).map_err(SubdomainValidationError)?;
                Some(format!("tenants/{}", sub))
            }
            None => None,
        };
        Ok(Self {
            tenant_prefix,
            provider: provider.into(),
            source_id: source_id.into(),
            stream_name: stream_name.into(),
            date,
        })
    }

    /// Build S3 key with current timestamp
    ///
    /// Pattern without tenant: `streams/{provider}/{source_id}/{stream_name}/date={YYYY-MM-DD}/records_{unix_timestamp}.jsonl`
    /// Pattern with tenant: `tenants/{subdomain}/streams/{provider}/{source_id}/{stream_name}/date={YYYY-MM-DD}/records_{unix_timestamp}.jsonl`
    ///
    /// Example: `tenants/adamjace/streams/ios/550e8400-e29b-41d4-a716-446655440000/healthkit/date=2025-01-15/records_1736899200.jsonl`
    pub fn build(&self) -> String {
        let timestamp = chrono::Utc::now().timestamp();
        self.build_with_timestamp(timestamp)
    }

    /// Build S3 key with explicit timestamp
    pub fn build_with_timestamp(&self, timestamp: i64) -> String {
        let base = format!(
            "streams/{}/{}/{}/date={}/records_{}.jsonl",
            self.provider,
            self.source_id,
            self.stream_name,
            self.date.format("%Y-%m-%d"),
            timestamp
        );
        match &self.tenant_prefix {
            Some(prefix) => format!("{}/{}", prefix, base),
            None => base,
        }
    }

    /// Build prefix for listing all objects for a source/stream
    ///
    /// Pattern: `[tenants/{subdomain}/]streams/{provider}/{source_id}/{stream_name}/`
    pub fn build_stream_prefix(&self) -> String {
        let base = format!(
            "streams/{}/{}/{}/",
            self.provider, self.source_id, self.stream_name
        );
        match &self.tenant_prefix {
            Some(prefix) => format!("{}/{}", prefix, base),
            None => base,
        }
    }

    /// Build prefix for listing all objects for a source/stream/date
    ///
    /// Pattern: `[tenants/{subdomain}/]streams/{provider}/{source_id}/{stream_name}/date={YYYY-MM-DD}/`
    pub fn build_date_prefix(&self) -> String {
        let base = format!(
            "streams/{}/{}/{}/date={}/",
            self.provider,
            self.source_id,
            self.stream_name,
            self.date.format("%Y-%m-%d")
        );
        match &self.tenant_prefix {
            Some(prefix) => format!("{}/{}", prefix, base),
            None => base,
        }
    }
}

/// Parser for extracting metadata from S3 keys
///
/// Handles both old format (`streams/...`) and new multi-tenant format (`tenants/{subdomain}/streams/...`)
pub struct StreamKeyParser {
    key: String,
}

impl StreamKeyParser {
    pub fn new(key: impl Into<String>) -> Self {
        Self { key: key.into() }
    }

    /// Get the base offset to skip tenant prefix if present
    ///
    /// Returns 2 for `tenants/{subdomain}/streams/...` format, 0 for `streams/...` format
    fn base_offset(&self) -> usize {
        let parts: Vec<&str> = self.key.split('/').collect();
        if parts.first() == Some(&"tenants") && parts.len() > 2 && parts.get(2) == Some(&"streams")
        {
            2 // Skip "tenants/{subdomain}"
        } else {
            0
        }
    }

    /// Extract tenant subdomain from key (if present)
    ///
    /// Example: `tenants/adamjace/streams/ios/...` returns `Some("adamjace")`
    /// Example: `streams/ios/...` returns `None`
    pub fn tenant(&self) -> Option<String> {
        let parts: Vec<&str> = self.key.split('/').collect();
        if parts.first() == Some(&"tenants") && parts.len() > 1 {
            Some(parts[1].to_string())
        } else {
            None
        }
    }

    /// Extract provider from key
    ///
    /// Example: `streams/ios/550e8400.../healthkit/date=2025-01-15/records_1736899200.jsonl`
    /// Example: `tenants/adamjace/streams/ios/550e8400.../healthkit/date=2025-01-15/records_1736899200.jsonl`
    /// Returns: `ios`
    pub fn provider(&self) -> Option<String> {
        let parts: Vec<&str> = self.key.split('/').collect();
        let offset = self.base_offset();
        // parts[offset] is "streams", parts[offset+1] is provider
        if parts.len() <= offset + 1 || parts.get(offset) != Some(&"streams") {
            return None;
        }
        Some(parts[offset + 1].to_string())
    }

    /// Extract source_id from key
    ///
    /// Example: `streams/ios/source_google-calendar/healthkit/date=2025-01-15/records_1736899200.jsonl`
    /// Returns: `source_google-calendar`
    pub fn source_id(&self) -> Option<String> {
        let parts: Vec<&str> = self.key.split('/').collect();
        let offset = self.base_offset();
        if parts.len() <= offset + 2 || parts.get(offset) != Some(&"streams") {
            return None;
        }
        Some(parts[offset + 2].to_string())
    }

    /// Extract stream name from key
    ///
    /// Example: Returns: `healthkit`
    pub fn stream_name(&self) -> Option<String> {
        let parts: Vec<&str> = self.key.split('/').collect();
        let offset = self.base_offset();
        if parts.len() <= offset + 3 || parts.get(offset) != Some(&"streams") {
            return None;
        }
        Some(parts[offset + 3].to_string())
    }

    /// Extract date from key
    ///
    /// Example: Returns: `2025-01-15`
    pub fn date(&self) -> Option<NaiveDate> {
        let parts: Vec<&str> = self.key.split('/').collect();
        let offset = self.base_offset();
        if parts.len() <= offset + 4 || parts.get(offset) != Some(&"streams") {
            return None;
        }

        // Parse "date=2025-01-15" format
        let date_part = parts[offset + 4];
        if !date_part.starts_with("date=") {
            return None;
        }

        let date_str = date_part.strip_prefix("date=")?;
        NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()
    }

    /// Extract timestamp from key
    ///
    /// Example: Returns: `1736899200`
    pub fn timestamp(&self) -> Option<i64> {
        let parts: Vec<&str> = self.key.split('/').collect();
        let offset = self.base_offset();
        if parts.len() <= offset + 5 || parts.get(offset) != Some(&"streams") {
            return None;
        }

        // Parse "records_1736899200.jsonl" format
        let filename = parts[offset + 5];
        if !filename.starts_with("records_") || !filename.ends_with(".jsonl") {
            return None;
        }

        let timestamp_str = filename.strip_prefix("records_")?.strip_suffix(".jsonl")?;
        timestamp_str.parse().ok()
    }

    /// Extract all metadata from key
    pub fn parse_all(&self) -> Option<(String, String, String, NaiveDate, i64)> {
        Some((
            self.provider()?,
            self.source_id()?,
            self.stream_name()?,
            self.date()?,
            self.timestamp()?,
        ))
    }

    /// Static helper to parse date from S3 key (for use in encryption key derivation)
    ///
    /// # Arguments
    /// * `key` - S3 key to parse
    ///
    /// # Returns
    /// NaiveDate extracted from the key
    pub fn parse_date_from_key(key: &str) -> crate::error::Result<NaiveDate> {
        let parser = Self::new(key);
        parser.date().ok_or_else(|| {
            crate::error::Error::Other(format!("Failed to parse date from S3 key: {}", key))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_key_builder_without_tenant() {
        let source_id = "source_ios-healthkit";
        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();

        let builder = StreamKeyBuilder::new(None, "ios", source_id, "healthkit", date).unwrap();

        let key = builder.build_with_timestamp(1736899200);
        assert_eq!(
            key,
            "streams/ios/source_ios-healthkit/healthkit/date=2025-01-15/records_1736899200.jsonl"
        );

        let prefix = builder.build_stream_prefix();
        assert_eq!(prefix, "streams/ios/source_ios-healthkit/healthkit/");

        let date_prefix = builder.build_date_prefix();
        assert_eq!(
            date_prefix,
            "streams/ios/source_ios-healthkit/healthkit/date=2025-01-15/"
        );
    }

    #[test]
    fn test_stream_key_builder_with_tenant() {
        let source_id = "source_ios-healthkit";
        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();

        // Note: Now we pass raw subdomain, not "tenants/adamjace"
        let builder =
            StreamKeyBuilder::new(Some("adamjace"), "ios", source_id, "healthkit", date).unwrap();

        let key = builder.build_with_timestamp(1736899200);
        assert_eq!(
            key,
            "tenants/adamjace/streams/ios/source_ios-healthkit/healthkit/date=2025-01-15/records_1736899200.jsonl"
        );

        let prefix = builder.build_stream_prefix();
        assert_eq!(
            prefix,
            "tenants/adamjace/streams/ios/source_ios-healthkit/healthkit/"
        );

        let date_prefix = builder.build_date_prefix();
        assert_eq!(
            date_prefix,
            "tenants/adamjace/streams/ios/source_ios-healthkit/healthkit/date=2025-01-15/"
        );
    }

    // Security tests for path traversal and invalid subdomains
    #[test]
    fn test_subdomain_validation_rejects_path_traversal() {
        let source_id = "source_test";
        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();

        // Path traversal attempts
        assert!(
            StreamKeyBuilder::new(Some("../../../etc"), "ios", source_id, "healthkit", date)
                .is_err()
        );
        assert!(StreamKeyBuilder::new(
            Some("tenant/../other"),
            "ios",
            source_id,
            "healthkit",
            date
        )
        .is_err());
        assert!(StreamKeyBuilder::new(Some(".."), "ios", source_id, "healthkit", date).is_err());
        assert!(
            StreamKeyBuilder::new(Some("foo/bar"), "ios", source_id, "healthkit", date).is_err()
        );
    }

    #[test]
    fn test_subdomain_validation_rejects_special_chars() {
        let source_id = "source_test";
        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();

        let too_long = "a".repeat(64);
        let bad_subdomains: Vec<&str> = vec![
            "tenant/other",      // slashes
            "tenant..other",     // double dots (contains .)
            "UPPERCASE",         // uppercase
            "tenant_underscore", // underscores
            "-starts-with",      // leading hyphen
            "ends-with-",        // trailing hyphen
            "",                  // empty
            &too_long,           // too long (64 chars)
        ];

        for bad in bad_subdomains {
            assert!(
                StreamKeyBuilder::new(Some(bad), "ios", source_id, "healthkit", date).is_err(),
                "Should reject: {}",
                bad
            );
        }
    }

    #[test]
    fn test_subdomain_validation_accepts_valid() {
        let source_id = "source_test";
        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();

        let max_length = "a".repeat(63);
        let good_subdomains: Vec<&str> = vec![
            "adamjace",
            "my-tenant",
            "tenant123",
            "a",
            "a-b-c-1-2-3",
            "123",
            &max_length, // exactly 63 chars is ok
        ];

        for good in good_subdomains {
            assert!(
                StreamKeyBuilder::new(Some(good), "ios", source_id, "healthkit", date).is_ok(),
                "Should accept: {}",
                good
            );
        }
    }

    #[test]
    fn test_stream_key_parser_without_tenant() {
        let key =
            "streams/ios/source_ios-healthkit/healthkit/date=2025-01-15/records_1736899200.jsonl";
        let parser = StreamKeyParser::new(key);

        assert!(parser.tenant().is_none());
        assert_eq!(parser.provider().unwrap(), "ios");
        assert_eq!(parser.source_id().unwrap(), "source_ios-healthkit");
        assert_eq!(parser.stream_name().unwrap(), "healthkit");
        assert_eq!(
            parser.date().unwrap(),
            NaiveDate::from_ymd_opt(2025, 1, 15).unwrap()
        );
        assert_eq!(parser.timestamp().unwrap(), 1736899200);

        let (provider, source_id, stream_name, date, timestamp) = parser.parse_all().unwrap();
        assert_eq!(provider, "ios");
        assert_eq!(source_id, "source_ios-healthkit");
        assert_eq!(stream_name, "healthkit");
        assert_eq!(date, NaiveDate::from_ymd_opt(2025, 1, 15).unwrap());
        assert_eq!(timestamp, 1736899200);
    }

    #[test]
    fn test_stream_key_parser_with_tenant() {
        let key = "tenants/adamjace/streams/ios/source_ios-healthkit/healthkit/date=2025-01-15/records_1736899200.jsonl";
        let parser = StreamKeyParser::new(key);

        assert_eq!(parser.tenant().unwrap(), "adamjace");
        assert_eq!(parser.provider().unwrap(), "ios");
        assert_eq!(parser.source_id().unwrap(), "source_ios-healthkit");
        assert_eq!(parser.stream_name().unwrap(), "healthkit");
        assert_eq!(
            parser.date().unwrap(),
            NaiveDate::from_ymd_opt(2025, 1, 15).unwrap()
        );
        assert_eq!(parser.timestamp().unwrap(), 1736899200);

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
        assert!(parser.tenant().is_none());
        assert!(parser.source_id().is_none());
        assert!(parser.stream_name().is_none());
        assert!(parser.date().is_none());
        assert!(parser.timestamp().is_none());
    }
}

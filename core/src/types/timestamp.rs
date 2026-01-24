//! Timestamp type for SQLite datetime handling.
//!
//! SQLite stores datetimes as TEXT in the format "YYYY-MM-DD HH:MM:SS" (UTC, no timezone marker).
//! JavaScript's `Date` constructor interprets this format as local time, causing timezone bugs.
//!
//! This module provides a `Timestamp` newtype that:
//! - Reads from SQLite's format
//! - Writes to SQLite's format (preserving compatibility with `datetime()` functions)
//! - Serializes to RFC 3339 format ("2024-01-22T15:30:00Z") for JavaScript compatibility
//!
//! # Example
//!
//! ```rust
//! use virtues::types::Timestamp;
//!
//! #[derive(Serialize, Deserialize, sqlx::FromRow)]
//! pub struct Job {
//!     pub started_at: Timestamp,
//!     pub completed_at: Option<Timestamp>,
//! }
//! ```

use chrono::{DateTime, NaiveDateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sqlx::decode::Decode;
use sqlx::encode::Encode;
use sqlx::sqlite::{Sqlite, SqliteArgumentValue, SqliteTypeInfo, SqliteValueRef};
use std::ops::Deref;

/// SQLite datetime format: "YYYY-MM-DD HH:MM:SS"
const SQLITE_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// A UTC timestamp that handles SQLite's datetime format and serializes to RFC 3339.
///
/// This type encapsulates the conversion between:
/// - SQLite's `datetime('now')` format: `"2024-01-22 15:30:00"` (UTC, no timezone)
/// - RFC 3339 / ISO 8601 format: `"2024-01-22T15:30:00Z"` (explicit UTC)
///
/// Using this type ensures that all timestamps are correctly interpreted as UTC
/// by JavaScript clients, preventing timezone-related duration calculation bugs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(DateTime<Utc>);

impl Timestamp {
    /// Create a timestamp for the current moment.
    pub fn now() -> Self {
        Self(Utc::now())
    }

    /// Create a timestamp from a `DateTime<Utc>`.
    pub fn from_utc(dt: DateTime<Utc>) -> Self {
        Self(dt)
    }

    /// Get the inner `DateTime<Utc>`.
    pub fn into_inner(self) -> DateTime<Utc> {
        self.0
    }

    /// Format as SQLite datetime string for database storage.
    pub fn to_sqlite_string(&self) -> String {
        self.0.format(SQLITE_FORMAT).to_string()
    }

    /// Format as RFC 3339 string for JSON/API responses.
    pub fn to_rfc3339(&self) -> String {
        self.0
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    }

    /// Parse from a string, handling both SQLite and RFC 3339 formats.
    pub fn parse(s: &str) -> Result<Self, chrono::ParseError> {
        s.parse()
    }
}

impl Default for Timestamp {
    fn default() -> Self {
        Self::now()
    }
}

impl Deref for Timestamp {
    type Target = DateTime<Utc>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<DateTime<Utc>> for Timestamp {
    fn from(dt: DateTime<Utc>) -> Self {
        Self(dt)
    }
}

impl From<Timestamp> for DateTime<Utc> {
    fn from(ts: Timestamp) -> Self {
        ts.0
    }
}

impl std::fmt::Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_rfc3339())
    }
}

impl std::str::FromStr for Timestamp {
    type Err = chrono::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Try RFC 3339 first (contains 'T' separator or timezone info)
        if s.contains('T') || s.contains('+') || s.ends_with('Z') {
            DateTime::parse_from_rfc3339(s)
                .map(|dt| Self(dt.with_timezone(&Utc)))
        } else {
            // SQLite format: "2024-01-22 15:30:00"
            NaiveDateTime::parse_from_str(s, SQLITE_FORMAT).map(|dt| Self(dt.and_utc()))
        }
    }
}

// =============================================================================
// Serde Implementation
// =============================================================================

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Always serialize to RFC 3339 for JSON/API compatibility
        serializer.serialize_str(&self.to_rfc3339())
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

// =============================================================================
// JsonSchema Implementation (for MCP tools)
// =============================================================================

impl JsonSchema for Timestamp {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Timestamp")
    }

    fn json_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "format": "date-time"
        })
    }
}

// =============================================================================
// SQLx Implementation
// =============================================================================

impl sqlx::Type<Sqlite> for Timestamp {
    fn type_info() -> SqliteTypeInfo {
        <String as sqlx::Type<Sqlite>>::type_info()
    }

    fn compatible(ty: &SqliteTypeInfo) -> bool {
        <String as sqlx::Type<Sqlite>>::compatible(ty)
    }
}

impl<'r> Decode<'r, Sqlite> for Timestamp {
    fn decode(value: SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <String as Decode<Sqlite>>::decode(value)?;
        s.parse()
            .map_err(|e: chrono::ParseError| Box::new(e) as sqlx::error::BoxDynError)
    }
}

impl Encode<'_, Sqlite> for Timestamp {
    fn encode_by_ref(
        &self,
        args: &mut Vec<SqliteArgumentValue<'_>>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        // Store in SQLite format for compatibility with datetime() functions
        let formatted = self.to_sqlite_string();
        <String as Encode<Sqlite>>::encode(formatted, args)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sqlite_format() {
        let ts: Timestamp = "2024-01-22 15:30:00".parse().unwrap();
        assert_eq!(ts.to_sqlite_string(), "2024-01-22 15:30:00");
    }

    #[test]
    fn test_parse_rfc3339_format() {
        let ts: Timestamp = "2024-01-22T15:30:00Z".parse().unwrap();
        assert_eq!(ts.to_sqlite_string(), "2024-01-22 15:30:00");
    }

    #[test]
    fn test_serialize_to_rfc3339() {
        let ts: Timestamp = "2024-01-22 15:30:00".parse().unwrap();
        let json = serde_json::to_string(&ts).unwrap();
        assert_eq!(json, r#""2024-01-22T15:30:00Z""#);
    }

    #[test]
    fn test_deserialize_from_sqlite_format() {
        let ts: Timestamp = serde_json::from_str(r#""2024-01-22 15:30:00""#).unwrap();
        assert_eq!(ts.to_rfc3339(), "2024-01-22T15:30:00Z");
    }

    #[test]
    fn test_deserialize_from_rfc3339() {
        let ts: Timestamp = serde_json::from_str(r#""2024-01-22T15:30:00Z""#).unwrap();
        assert_eq!(ts.to_rfc3339(), "2024-01-22T15:30:00Z");
    }

    #[test]
    fn test_now() {
        let ts = Timestamp::now();
        // Should not panic and should produce valid strings
        let _ = ts.to_sqlite_string();
        let _ = ts.to_rfc3339();
    }

    #[test]
    fn test_ordering() {
        let ts1: Timestamp = "2024-01-22 15:30:00".parse().unwrap();
        let ts2: Timestamp = "2024-01-22 15:30:01".parse().unwrap();
        assert!(ts1 < ts2);
    }
}

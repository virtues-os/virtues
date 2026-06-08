//! Timestamp newtype with RFC 3339 serialization.
//!
//! Wraps `chrono::DateTime<Utc>` so:
//! - sqlx round-trips it natively as Postgres `TIMESTAMPTZ` (no format
//!   ambiguity — pg stores timestamps with timezone semantics).
//! - serde always serializes to RFC 3339 / ISO 8601 (`"2024-01-22T15:30:00Z"`)
//!   so JavaScript `new Date(...)` parses it as UTC, not local time.
//!
//! This wrapper survived the Postgres migration mostly as a serde shim. With
//! pg's native TIMESTAMPTZ the database-side concerns are gone — but the
//! serde behavior still matters for clients, and removing the wrapper would
//! touch ~130 call sites, so it stays.

use chrono::{DateTime, NaiveDateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sqlx::Postgres;
use std::ops::Deref;

/// Space-separated datetime format (`2024-01-22 15:30:00`). Still accepted on
/// parse: day-boundary queries (`api::wiki`) and MCP tool output (`mcp::tools`)
/// emit this shape rather than RFC 3339.
const SPACE_SEPARATED_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// A UTC timestamp that serializes to RFC 3339 and stores as `TIMESTAMPTZ`.
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

    /// Format as RFC 3339 string for JSON/API responses.
    pub fn to_rfc3339(&self) -> String {
        self.0.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    }

    /// Parse from a string, accepting RFC 3339 or a space-separated datetime.
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
        // Try RFC 3339 first (the canonical form).
        if s.contains('T') || s.contains('+') || s.ends_with('Z') {
            DateTime::parse_from_rfc3339(s).map(|dt| Self(dt.with_timezone(&Utc)))
        } else {
            NaiveDateTime::parse_from_str(s, SPACE_SEPARATED_FORMAT).map(|dt| Self(dt.and_utc()))
        }
    }
}

// =============================================================================
// Serde
// =============================================================================

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
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
// JsonSchema (for MCP tools)
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
// sqlx — delegate to chrono::DateTime<Utc> which maps to TIMESTAMPTZ.
// =============================================================================

impl sqlx::Type<Postgres> for Timestamp {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <DateTime<Utc> as sqlx::Type<Postgres>>::type_info()
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        <DateTime<Utc> as sqlx::Type<Postgres>>::compatible(ty)
    }
}

impl<'r> sqlx::decode::Decode<'r, Postgres> for Timestamp {
    fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        <DateTime<Utc> as sqlx::decode::Decode<Postgres>>::decode(value).map(Self)
    }
}

impl<'q> sqlx::encode::Encode<'q, Postgres> for Timestamp {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        <DateTime<Utc> as sqlx::encode::Encode<Postgres>>::encode_by_ref(&self.0, buf)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rfc3339_format() {
        let ts: Timestamp = "2024-01-22T15:30:00Z".parse().unwrap();
        assert_eq!(ts.to_rfc3339(), "2024-01-22T15:30:00Z");
    }

    #[test]
    fn test_parse_space_separated_format() {
        let ts: Timestamp = "2024-01-22 15:30:00".parse().unwrap();
        assert_eq!(ts.to_rfc3339(), "2024-01-22T15:30:00Z");
    }

    #[test]
    fn test_serialize_to_rfc3339() {
        let ts: Timestamp = "2024-01-22T15:30:00Z".parse().unwrap();
        let json = serde_json::to_string(&ts).unwrap();
        assert_eq!(json, r#""2024-01-22T15:30:00Z""#);
    }

    #[test]
    fn test_deserialize_from_space_separated_format() {
        let ts: Timestamp = serde_json::from_str(r#""2024-01-22 15:30:00""#).unwrap();
        assert_eq!(ts.to_rfc3339(), "2024-01-22T15:30:00Z");
    }

    #[test]
    fn test_ordering() {
        let ts1: Timestamp = "2024-01-22T15:30:00Z".parse().unwrap();
        let ts2: Timestamp = "2024-01-22T15:30:01Z".parse().unwrap();
        assert!(ts1 < ts2);
    }
}

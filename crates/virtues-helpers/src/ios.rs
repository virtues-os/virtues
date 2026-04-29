//! iOS-specific shared utilities for iOS action binaries.
//!
//! Timestamp parsing, field extraction patterns, and common iOS constants used
//! across healthkit, location, microphone, contacts, eventkit, and financekit.

use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

/// Source provider string written to all iOS ontology rows.
pub const IOS_PROVIDER: &str = "ios";

/// Parse an iOS timestamp from a record field. Falls back to `Utc::now()` if missing/invalid.
///
/// iOS sends ISO-8601 strings (e.g., `"2026-04-10T14:00:00Z"`).
pub fn parse_timestamp(record: &Value, field: &str) -> DateTime<Utc> {
    record
        .get(field)
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .unwrap_or_else(Utc::now)
}

/// Extract the stream ID from a record's `id` field, generating a new UUID if absent.
///
/// This is the deterministic dedup key — the iOS app sends a stable identifier per record
/// so replays don't create duplicates.
pub fn stream_id_or_new(record: &Value) -> String {
    record
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

/// The source_table string used for all iOS HealthKit records.
pub const HEALTHKIT_STREAM_TABLE: &str = "stream_ios_healthkit";
pub const LOCATION_STREAM_TABLE: &str = "stream_ios_location";
pub const MICROPHONE_STREAM_TABLE: &str = "stream_ios_microphone";
pub const CONTACTS_STREAM_TABLE: &str = "stream_ios_contacts";
pub const EVENTKIT_STREAM_TABLE: &str = "stream_ios_eventkit";
pub const FINANCEKIT_STREAM_TABLE: &str = "stream_ios_financekit";

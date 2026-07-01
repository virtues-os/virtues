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

/// Fixed namespace for deterministic iOS stream ids (UUIDv5), domain-separating
/// them from any other UUIDv5 used in the system.
const IOS_STREAM_NS: Uuid = Uuid::from_u128(0x1f9a3c7e_4b2d_5e8f_a1c6_9d0b2e4f6a8c);

/// Stable dedup id for an iOS stream record. Prefers a client-supplied `id`;
/// otherwise derives a **deterministic** id from the record's content so a retry
/// of the same record yields the same id — letting `ON CONFLICT
/// (source_stream_id)` absorb the duplicate instead of inserting a fresh row.
///
/// This closes the location/HealthKit duplication bug: those streams don't send
/// an `id`, so the old [`stream_id_or_new`] fell back to a random UUID and every
/// retry created a duplicate. Two records with identical content ARE the same
/// reading (a device can't emit two distinct readings that serialize byte-for-
/// byte identically), so collapsing them is correct. `stream` domain-separates
/// ids across streams; a serialization failure falls back to a random id (never
/// happens for well-formed records) rather than dropping the record.
pub fn stream_id_or_hash(record: &Value, stream: &str) -> String {
    if let Some(id) = record
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        return id.to_string();
    }
    match serde_json::to_string(record) {
        Ok(canon) => {
            let name = format!("{stream}\u{1f}{canon}");
            Uuid::new_v5(&IOS_STREAM_NS, name.as_bytes()).to_string()
        }
        Err(_) => Uuid::new_v4().to_string(),
    }
}

/// The source_table string used for all iOS HealthKit records.
pub const HEALTHKIT_STREAM_TABLE: &str = "stream_ios_healthkit";
pub const LOCATION_STREAM_TABLE: &str = "stream_ios_location";
pub const MICROPHONE_STREAM_TABLE: &str = "stream_ios_microphone";
pub const CONTACTS_STREAM_TABLE: &str = "stream_ios_contacts";
pub const EVENTKIT_STREAM_TABLE: &str = "stream_ios_eventkit";
pub const FINANCEKIT_STREAM_TABLE: &str = "stream_ios_financekit";

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hash_id_is_deterministic_across_retries() {
        // A location record with no `id` (the bug case): the same content must
        // yield the same id on every call, so a retry dedupes instead of dup'ing.
        let rec = json!({"latitude": 37.7749, "longitude": -122.4194, "timestamp": "2026-07-01T14:00:00Z"});
        let a = stream_id_or_hash(&rec, LOCATION_STREAM_TABLE);
        let b = stream_id_or_hash(&rec.clone(), LOCATION_STREAM_TABLE);
        assert_eq!(a, b, "identical record → identical id (dedupes on retry)");
        // A genuinely different reading → different id (not over-collapsed).
        let other = json!({"latitude": 37.7750, "longitude": -122.4194, "timestamp": "2026-07-01T14:00:00Z"});
        assert_ne!(a, stream_id_or_hash(&other, LOCATION_STREAM_TABLE));
        // Stream domain separates ids so distinct streams can't collide.
        assert_ne!(a, stream_id_or_hash(&rec, HEALTHKIT_STREAM_TABLE));
    }

    #[test]
    fn client_supplied_id_wins_over_hash() {
        let rec = json!({"id": "abc-123", "latitude": 1.0, "longitude": 2.0});
        assert_eq!(stream_id_or_hash(&rec, LOCATION_STREAM_TABLE), "abc-123");
        // Empty id is ignored (falls through to the deterministic hash, not "").
        let rec_empty = json!({"id": "", "latitude": 1.0, "longitude": 2.0});
        assert_ne!(stream_id_or_hash(&rec_empty, LOCATION_STREAM_TABLE), "");
    }
}

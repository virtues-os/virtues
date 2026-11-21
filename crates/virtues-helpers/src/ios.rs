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

/// Fixed namespace for deterministic iOS stream ids (UUIDv5), domain-separating
/// them from any other UUIDv5 used in the system.
const IOS_STREAM_NS: Uuid = Uuid::from_u128(0x1f9a3c7e_4b2d_5e8f_a1c6_9d0b2e4f6a8c);

/// The ontology row's primary key, derived deterministically from its dedup key.
///
/// A random `Uuid::new_v4()` here means the same logical record gets a different
/// primary key every time it is written. `ON CONFLICT (source_stream_id) DO
/// NOTHING` hides that today — the second write is simply skipped — but it makes
/// the row's identity accidental: anything that references it by id
/// (`search_embeddings.record_id`, `wiki_refs.source_id`) is orphaned the
/// moment the row is ever rewritten. Deriving the id from the stream id makes a
/// record's identity a function of the record, which is what it always should
/// have been.
pub fn row_id(stream: &str, stream_id: &str) -> String {
    let name = format!("{stream}\u{1f}{stream_id}");
    Uuid::new_v5(&IOS_STREAM_NS, name.as_bytes()).to_string()
}

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

/// [`stream_id_or_hash`] for a record read back from the lake, where `line`
/// is the record's archived text and `record` is that text parsed.
///
/// The lake writes each record as `serde_json::to_string(record)`, which is
/// the text the hash was taken over, so an id-less record's id is rebuilt from
/// the line itself. Parsing the line and serializing it again is not the same
/// thing: `serde_json` reads a float to within a unit in the last place unless
/// built with `float_roundtrip`, so a reparsed latitude can print differently
/// and hash to another id.
pub fn stream_id_of_archived_line(line: &str, record: &Value, stream: &str) -> String {
    if let Some(id) = record.get("id").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        return id.to_string();
    }
    let name = format!("{stream}\u{1f}{line}");
    Uuid::new_v5(&IOS_STREAM_NS, name.as_bytes()).to_string()
}

/// The stream table of an iOS stream whose records are each one row with a
/// [`stream_id_or_hash`] id, by the stream's name on the wire. None for a
/// stream whose records wrap their rows or carry another key (financekit,
/// bookmark, contacts).
pub fn hashed_stream_table(stream: &str) -> Option<&'static str> {
    match stream {
        "healthkit" => Some(HEALTHKIT_STREAM_TABLE),
        "location" => Some(LOCATION_STREAM_TABLE),
        "microphone" => Some(MICROPHONE_STREAM_TABLE),
        "eventkit" => Some(EVENTKIT_STREAM_TABLE),
        _ => None,
    }
}

/// The source_table string used for all iOS HealthKit records.
pub const HEALTHKIT_STREAM_TABLE: &str = "stream_ios_healthkit";
pub const LOCATION_STREAM_TABLE: &str = "stream_ios_location";
pub const MICROPHONE_STREAM_TABLE: &str = "stream_ios_microphone";
pub const CONTACTS_STREAM_TABLE: &str = "stream_ios_contacts";
pub const EVENTKIT_STREAM_TABLE: &str = "stream_ios_eventkit";
pub const FINANCEKIT_STREAM_TABLE: &str = "stream_ios_financekit";
pub const PHOTOS_STREAM_TABLE: &str = "stream_ios_photos";

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

    #[test]
    fn an_archived_line_gives_the_id_its_record_was_written_under() {
        // No `id`: the hash is over the record's serialized text, which is
        // the line the lake wrote. Coordinates with every digit a double
        // holds, where a reparse is most likely to print differently.
        let rec = json!({"latitude": 30.267153000000002, "longitude": -97.74306079999999, "timestamp": "2026-07-01T14:00:00Z"});
        let line = serde_json::to_string(&rec).unwrap();
        let parsed: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            stream_id_of_archived_line(&line, &parsed, LOCATION_STREAM_TABLE),
            stream_id_or_hash(&rec, LOCATION_STREAM_TABLE),
        );
        // A client id wins, as it does when the record is written.
        let rec = json!({"id": "abc-123", "latitude": 1.0});
        let line = serde_json::to_string(&rec).unwrap();
        assert_eq!(stream_id_of_archived_line(&line, &rec, LOCATION_STREAM_TABLE), "abc-123");
    }

    #[test]
    fn the_hashed_streams_are_the_four_whose_records_are_rows() {
        assert_eq!(hashed_stream_table("healthkit"), Some(HEALTHKIT_STREAM_TABLE));
        assert_eq!(hashed_stream_table("location"), Some(LOCATION_STREAM_TABLE));
        assert_eq!(hashed_stream_table("microphone"), Some(MICROPHONE_STREAM_TABLE));
        assert_eq!(hashed_stream_table("eventkit"), Some(EVENTKIT_STREAM_TABLE));
        assert_eq!(hashed_stream_table("financekit"), None);
    }
}

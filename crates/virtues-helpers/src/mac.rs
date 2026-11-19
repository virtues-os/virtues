//! Mac-specific ids shared by `mac_ingest` and the lake.

use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

/// The dedup id of one browser visit: a hash of its URL and its time.
///
/// A hash, not `{url}:{ts}`: `source_stream_id` is UNIQUE, a btree index row
/// cannot exceed about 2704 bytes, and one URL with a page of tracking
/// parameters would fail the INSERT and with it the whole batch. A UUIDv5 is
/// 36 bytes whatever the URL and just as deterministic.
pub fn browser_stream_id(url: &str, at: DateTime<Utc>) -> String {
    Uuid::new_v5(&Uuid::NAMESPACE_OID, format!("mac:browse:{}:{}", url, at.timestamp_millis()).as_bytes())
        .to_string()
}

/// [`browser_stream_id`] from a raw visit record, or None for a visit with no
/// URL or no time it can be placed at, which `mac_ingest` does not write.
pub fn browser_visit_stream_id(visit: &Value) -> Option<String> {
    let url = visit.get("url").and_then(|v| v.as_str()).filter(|u| !u.is_empty())?;
    let at = visit.get("timestamp").and_then(|v| v.as_str())?.parse::<DateTime<Utc>>().ok()?;
    Some(browser_stream_id(url, at))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_visit_has_an_id_only_with_a_url_and_a_time() {
        let visit = json!({"url": "https://example.com/a", "timestamp": "2026-03-03T10:00:00Z"});
        let at = "2026-03-03T10:00:00Z".parse().unwrap();
        assert_eq!(browser_visit_stream_id(&visit), Some(browser_stream_id("https://example.com/a", at)));
        assert_eq!(browser_visit_stream_id(&json!({"url": "", "timestamp": "2026-03-03T10:00:00Z"})), None);
        assert_eq!(browser_visit_stream_id(&json!({"url": "https://example.com/a"})), None);
    }
}

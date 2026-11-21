//! One item from Spotify's recently-played list, shaped for
//! `data_activity_listening`.
//!
//! `spotify_import`'s tests read this file too, to show that a play from a
//! data download and the same play from this list share a key. Its tests are
//! in `main.rs` for that reason, so they run once.

use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use virtues_applets::listening::{stream_id, Play};

/// The play one item describes, or `None` for an item with no time or no
/// track URI.
///
/// Spotify does not document whether `played_at` is when a track started or
/// when it stopped. The data download's `ts` is the stop time, so this treats
/// `played_at` as the stop time too: that is what lets a later download land
/// on the same row. The list does not say how long the track played, so
/// `played_ms` is the track's length, marked `played_ms_estimated`.
pub fn shape(item: &Value) -> Option<Play> {
    let ended_at: DateTime<Utc> = item.get("played_at")?.as_str()?.parse().ok()?;
    let track = item.get("track")?;
    let uri = text(track, "uri")?;
    let duration_ms = track.get("duration_ms").and_then(Value::as_i64).unwrap_or(0).max(0);
    let artists: Vec<&str> = track
        .get("artists")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x.get("name")?.as_str()).map(str::trim).filter(|n| !n.is_empty()).collect())
        .unwrap_or_default();
    let context_uri = item.get("context").and_then(|c| text(c, "uri"));

    Some(Play {
        started_at: ended_at - Duration::milliseconds(duration_ms),
        ended_at,
        played_ms: duration_ms,
        kind: "track",
        title: text(track, "name"),
        artist_name: (!artists.is_empty()).then(|| artists.join(", ")),
        album_name: track.get("album").and_then(|a| text(a, "name")),
        source_stream_id: stream_id(ended_at, Some(&uri), &[]),
        uri: Some(uri),
        is_skipped: None,
        is_shuffle: None,
        reason_start: None,
        reason_end: None,
        metadata: json!({ "context_uri": context_uri, "played_ms_estimated": true }),
    })
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

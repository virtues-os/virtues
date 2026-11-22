//! One Spotify play from either data download, shaped for
//! `data_activity_listening`.
//!
//! The extended streaming history (`Streaming_History_Audio_*.json`, and
//! `endsong_*.json` before 2023) records each play's stop time to the second
//! and its Spotify URI. The account data download
//! (`StreamingHistory_music_*.json`, `StreamingHistory_podcast_*.json`)
//! records the stop time to the minute and no URI. Both record how long the
//! play ran, so a play's start is its stop less that.
//!
//! Spotify also records the IP address, country and, in older files, the
//! account name and user agent of each play. Only the fields named here are
//! read, so none of those are kept.

use chrono::Duration;
use serde_json::{json, Value};
use virtues_applets::import::parse_time;
use virtues_applets::listening::{stream_id, Play};

/// The play one element of either download describes, or `None` for an
/// element with no time or nothing that says what played.
pub fn parse_play(v: &Value) -> Option<Play> {
    if v.get("ts").is_some() {
        extended(v)
    } else if v.get("endTime").is_some() {
        account(v)
    } else {
        None
    }
}

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

fn played_ms(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(Value::as_i64).unwrap_or(0).max(0)
}

fn extended(v: &Value) -> Option<Play> {
    let ended_at = parse_time(v.get("ts")?.as_str()?)?;
    let ms = played_ms(v, "ms_played");

    let track = s(v, "master_metadata_track_name");
    let track_uri = s(v, "spotify_track_uri");
    let episode = s(v, "episode_name");
    let episode_uri = s(v, "spotify_episode_uri");
    let book = s(v, "audiobook_title");
    let chapter = s(v, "audiobook_chapter_title");
    let book_uri = s(v, "audiobook_chapter_uri").or_else(|| s(v, "audiobook_uri"));

    let (kind, title, artist_name, album_name, uri) = if track.is_some() || track_uri.is_some() {
        ("track", track, s(v, "master_metadata_album_artist_name"), s(v, "master_metadata_album_album_name"), track_uri)
    } else if episode.is_some() || episode_uri.is_some() {
        ("episode", episode, s(v, "episode_show_name"), None, episode_uri)
    } else if book.is_some() || chapter.is_some() || book_uri.is_some() {
        ("audiobook", chapter.or_else(|| book.clone()), None, book, book_uri)
    } else {
        return None;
    };

    let source_stream_id = stream_id(
        ended_at,
        uri.as_deref(),
        &[title.as_deref().unwrap_or(""), artist_name.as_deref().unwrap_or("")],
    );
    Some(Play {
        started_at: ended_at - Duration::milliseconds(ms),
        ended_at,
        played_ms: ms,
        kind,
        title,
        artist_name,
        album_name,
        uri,
        is_skipped: v.get("skipped").and_then(Value::as_bool),
        is_shuffle: v.get("shuffle").and_then(Value::as_bool),
        reason_start: s(v, "reason_start"),
        reason_end: s(v, "reason_end"),
        source_stream_id,
        metadata: json!({
            "platform": v.get("platform"),
            "offline": v.get("offline"),
            "incognito_mode": v.get("incognito_mode"),
        }),
    })
}

fn account(v: &Value) -> Option<Play> {
    let ended_at = parse_time(v.get("endTime")?.as_str()?)?;
    let ms = played_ms(v, "msPlayed");
    let (kind, title, artist_name) = if let Some(track) = s(v, "trackName") {
        ("track", track, s(v, "artistName"))
    } else {
        ("episode", s(v, "episodeName")?, s(v, "podcastName"))
    };
    let source_stream_id =
        stream_id(ended_at, None, &[&title, artist_name.as_deref().unwrap_or(""), &ms.to_string()]);
    Some(Play {
        started_at: ended_at - Duration::milliseconds(ms),
        ended_at,
        played_ms: ms,
        kind,
        title: Some(title),
        artist_name,
        album_name: None,
        uri: None,
        is_skipped: None,
        is_shuffle: None,
        reason_start: None,
        reason_end: None,
        source_stream_id,
        metadata: json!({}),
    })
}

/// A file of the extended streaming history, by its name in the zip.
pub fn is_extended_file(base: &str) -> bool {
    let named = base.starts_with("Streaming_History_Audio_")
        || base.starts_with("Streaming_History_Video_")
        || base.starts_with("endsong_");
    named && base.ends_with(".json")
}

/// A file of the account data download's listening history.
pub fn is_account_file(base: &str) -> bool {
    base.starts_with("StreamingHistory") && base.ends_with(".json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extended_track() -> Value {
        json!({
            "ts": "2024-03-05T08:15:30Z",
            "platform": "ios",
            "ms_played": 183_000,
            "conn_country": "US",
            "ip_addr": "192.0.2.10",
            "master_metadata_track_name": "Paper Lanterns",
            "master_metadata_album_artist_name": "The Example Band",
            "master_metadata_album_album_name": "Night Market",
            "spotify_track_uri": "spotify:track:0000000000000000000001",
            "episode_name": null, "episode_show_name": null, "spotify_episode_uri": null,
            "audiobook_title": null, "audiobook_uri": null,
            "audiobook_chapter_uri": null, "audiobook_chapter_title": null,
            "reason_start": "trackdone", "reason_end": "trackdone",
            "shuffle": true, "skipped": false, "offline": false, "incognito_mode": false
        })
    }

    #[test]
    fn an_extended_track_is_a_span_with_its_uri() {
        let p = parse_play(&extended_track()).unwrap();
        assert_eq!(p.kind, "track");
        assert_eq!(p.title.as_deref(), Some("Paper Lanterns"));
        assert_eq!(p.artist_name.as_deref(), Some("The Example Band"));
        assert_eq!(p.album_name.as_deref(), Some("Night Market"));
        assert_eq!(p.ended_at.to_rfc3339(), "2024-03-05T08:15:30+00:00");
        assert_eq!(p.started_at.to_rfc3339(), "2024-03-05T08:12:27+00:00");
        assert_eq!(p.source_stream_id, "spotify:2024-03-05T08:15:30Z:spotify:track:0000000000000000000001");
        assert_eq!(p.is_shuffle, Some(true));
        assert_eq!(p.is_skipped, Some(false));
        assert_eq!(p.metadata["platform"], "ios");
        let stored = serde_json::to_string(&p.metadata).unwrap();
        assert!(!stored.contains("192.0.2.10") && !stored.contains("US"), "{stored}");
    }

    #[test]
    fn an_extended_episode_and_audiobook_take_their_own_names() {
        let mut ep = extended_track();
        ep["master_metadata_track_name"] = Value::Null;
        ep["spotify_track_uri"] = Value::Null;
        ep["episode_name"] = json!("Tide Tables, Part 2");
        ep["episode_show_name"] = json!("Coastal Hours");
        ep["spotify_episode_uri"] = json!("spotify:episode:0000000000000000000002");
        let p = parse_play(&ep).unwrap();
        assert_eq!((p.kind, p.title.as_deref(), p.artist_name.as_deref()), ("episode", Some("Tide Tables, Part 2"), Some("Coastal Hours")));
        assert_eq!(p.album_name, None);

        let mut book = ep.clone();
        book["episode_name"] = Value::Null;
        book["episode_show_name"] = Value::Null;
        book["spotify_episode_uri"] = Value::Null;
        book["audiobook_title"] = json!("The Lighthouse Keeper");
        book["audiobook_chapter_title"] = json!("Chapter 3");
        book["audiobook_chapter_uri"] = json!("spotify:chapter:0000000000000000000003");
        let p = parse_play(&book).unwrap();
        assert_eq!(p.kind, "audiobook");
        assert_eq!(p.title.as_deref(), Some("Chapter 3"));
        assert_eq!(p.album_name.as_deref(), Some("The Lighthouse Keeper"));
        assert_eq!(p.uri.as_deref(), Some("spotify:chapter:0000000000000000000003"));

        let mut nothing = book;
        for k in ["audiobook_title", "audiobook_chapter_title", "audiobook_chapter_uri"] {
            nothing[k] = Value::Null;
        }
        assert!(parse_play(&nothing).is_none());
    }

    #[test]
    fn account_data_plays_key_on_their_text() {
        let song = json!({"endTime": "2022-07-01 21:04", "artistName": "The Example Band", "trackName": "Paper Lanterns", "msPlayed": 60_000});
        let p = parse_play(&song).unwrap();
        assert_eq!(p.kind, "track");
        assert_eq!(p.ended_at.to_rfc3339(), "2022-07-01T21:04:00+00:00");
        assert_eq!(p.started_at.to_rfc3339(), "2022-07-01T21:03:00+00:00");
        assert!(p.source_stream_id.starts_with("spotify:2022-07-01T21:04:00Z:"));
        assert_eq!(p.source_stream_id, parse_play(&song).unwrap().source_stream_id);
        let longer = json!({"endTime": "2022-07-01 21:04", "artistName": "The Example Band", "trackName": "Paper Lanterns", "msPlayed": 61_000});
        assert_ne!(p.source_stream_id, parse_play(&longer).unwrap().source_stream_id);

        let pod = json!({"endTime": "2022-07-02 07:30", "podcastName": "Coastal Hours", "episodeName": "Tide Tables", "msPlayed": 1_200_000});
        let p = parse_play(&pod).unwrap();
        assert_eq!((p.kind, p.title.as_deref(), p.artist_name.as_deref()), ("episode", Some("Tide Tables"), Some("Coastal Hours")));
    }

    #[test]
    fn file_names_say_which_download() {
        assert!(is_extended_file("Streaming_History_Audio_2021-2023_0.json"));
        assert!(is_extended_file("Streaming_History_Video_2022-2024.json"));
        assert!(is_extended_file("endsong_0.json"));
        assert!(!is_extended_file("Streaming_History_Audio_2021.pdf"));
        assert!(is_account_file("StreamingHistory_music_0.json"));
        assert!(is_account_file("StreamingHistory_podcast_0.json"));
        assert!(is_account_file("StreamingHistory0.json"));
        assert!(!is_account_file("Streaming_History_Audio_2021.json"));
    }
}

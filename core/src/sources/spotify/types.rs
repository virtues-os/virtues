//! Spotify API response types
//!
//! Based on the Spotify Web API: https://developer.spotify.com/documentation/web-api

use serde::{Deserialize, Serialize};

/// Response from GET /me/player/recently-played
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentlyPlayedResponse {
    pub items: Vec<PlayHistoryItem>,
    pub cursors: Option<Cursors>,
    pub limit: Option<i32>,
    pub href: Option<String>,
}

/// A single play history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayHistoryItem {
    pub track: TrackObject,
    pub played_at: String, // ISO 8601
    pub context: Option<ContextObject>,
}

/// Pagination cursors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cursors {
    pub after: Option<String>,  // Unix timestamp in ms
    pub before: Option<String>,
}

/// Simplified track object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackObject {
    pub id: String,
    pub name: String,
    pub uri: String,
    pub duration_ms: i64,
    pub artists: Vec<ArtistObject>,
    pub album: Option<AlbumObject>,
    pub explicit: Option<bool>,
    pub is_local: Option<bool>,
}

/// Simplified artist object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtistObject {
    pub id: Option<String>,
    pub name: String,
    pub uri: Option<String>,
}

/// Simplified album object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumObject {
    pub id: Option<String>,
    pub name: String,
    pub uri: Option<String>,
    pub images: Option<Vec<ImageObject>>,
}

/// Album image
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageObject {
    pub url: String,
    pub height: Option<i32>,
    pub width: Option<i32>,
}

/// Play context (playlist, album, artist page, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextObject {
    #[serde(rename = "type")]
    pub context_type: String, // playlist, album, artist, collection
    pub uri: Option<String>,
    pub href: Option<String>,
}

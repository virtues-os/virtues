//! Spotify integration
//!
//! Syncs listening history from Spotify's API into the activity_listening ontology.

pub mod client;
pub mod recently_played;
pub mod registry;
pub mod types;

pub use recently_played::SpotifyRecentlyPlayedStream;

//! Spotify source registration for the catalog

use crate::registry::{RegisteredSource, RegisteredStream, SourceRegistry};
use crate::sources::stream_type::StreamType;
use serde_json::json;

use super::recently_played::{transform::SpotifyListeningTransform, SpotifyRecentlyPlayedStream};

/// Spotify source registration
pub struct SpotifySource;

impl SourceRegistry for SpotifySource {
    fn descriptor() -> RegisteredSource {
        let descriptor = virtues_registry::sources::get_source("spotify")
            .expect("Spotify source not found in virtues-registry");

        RegisteredSource {
            descriptor,
            streams: vec![
                RegisteredStream::new("recently_played")
                    .config_schema(recently_played_config_schema())
                    .config_example(recently_played_config_example())
                    .transform("activity_listening", |_ctx| Ok(Box::new(SpotifyListeningTransform)))
                    .stream_creator(|ctx| {
                        Ok(StreamType::Pull(Box::new(SpotifyRecentlyPlayedStream::new(
                            ctx.source_id.clone(),
                            ctx.db.clone(),
                            ctx.stream_writer.clone(),
                            ctx.auth.clone(),
                        ))))
                    })
                    .build(),
            ],
        }
    }
}

fn recently_played_config_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "limit": {
                "type": "integer",
                "default": 50,
                "minimum": 1,
                "maximum": 50,
                "description": "Number of tracks per API request (max 50)"
            }
        }
    })
}

fn recently_played_config_example() -> serde_json::Value {
    json!({
        "limit": 50
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::AuthType;

    #[test]
    fn test_spotify_descriptor() {
        let desc = SpotifySource::descriptor();
        assert_eq!(desc.descriptor.name, "spotify");
        assert_eq!(desc.descriptor.auth_type, AuthType::OAuth2);
        assert!(desc.descriptor.oauth_config.is_some());
        assert_eq!(desc.streams.len(), 1);
    }

    #[test]
    fn test_recently_played_stream() {
        let desc = SpotifySource::descriptor();
        let stream = desc.streams.iter().find(|s| s.descriptor.name == "recently_played");
        assert!(stream.is_some());

        let s = stream.unwrap();
        assert_eq!(s.descriptor.table_name, "stream_spotify_recently_played");
        assert!(s.descriptor.supports_incremental);
        assert!(!s.descriptor.supports_full_refresh);
    }

    #[test]
    fn test_config_schemas_valid() {
        let schema = recently_played_config_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"].is_object());
    }
}

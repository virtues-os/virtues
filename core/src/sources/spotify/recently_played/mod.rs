//! Spotify recently played stream implementation

pub mod transform;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::client::SpotifyClient;
use super::types::RecentlyPlayedResponse;
use crate::{
    error::Result,
    sources::{
        auth::SourceAuth,
        base::{SyncMode, SyncResult},
        pull_stream::PullStream,
    },
    storage::stream_writer::StreamWriter,
};

/// Spotify recently played stream
///
/// Syncs the user's recently played tracks from Spotify API.
/// The API returns max 50 tracks per request. We use the `after` cursor
/// (Unix timestamp in ms) for incremental sync.
pub struct SpotifyRecentlyPlayedStream {
    source_id: String,
    client: SpotifyClient,
    db: SqlitePool,
    stream_writer: Arc<Mutex<StreamWriter>>,
}

impl SpotifyRecentlyPlayedStream {
    pub fn new(
        source_id: String,
        db: SqlitePool,
        stream_writer: Arc<Mutex<StreamWriter>>,
        auth: SourceAuth,
    ) -> Self {
        let token_manager = auth
            .token_manager()
            .expect("SpotifyRecentlyPlayedStream requires OAuth2 auth")
            .clone();

        let client = SpotifyClient::new(source_id.clone(), token_manager);

        Self {
            source_id,
            client,
            db,
            stream_writer,
        }
    }

    #[tracing::instrument(skip(self), fields(source_id = %self.source_id, mode = ?sync_mode))]
    async fn sync_internal(&self, sync_mode: &SyncMode) -> Result<SyncResult> {
        let started_at = Utc::now();
        let records_fetched;
        let mut records_written = 0;
        let mut records_failed = 0;
        let mut next_cursor = None;
        let mut earliest_record_at: Option<DateTime<Utc>> = None;
        let mut latest_record_at: Option<DateTime<Utc>> = None;

        // Determine the `after` cursor (Unix timestamp in ms) for incremental sync
        let after_ms: Option<String> = match sync_mode {
            SyncMode::Incremental { cursor } => {
                cursor.clone().or(self.get_last_sync_token().await?)
            }
            SyncMode::FullRefresh => None,
            SyncMode::Backfill { start_date, .. } => {
                Some((start_date.timestamp_millis()).to_string())
            }
        };

        tracing::info!(
            after_cursor = ?after_ms,
            "Starting Spotify recently played sync"
        );

        // Build params — Spotify accepts limit + after (ms timestamp)
        let limit = "50";
        let mut params: Vec<(&str, &str)> = vec![("limit", limit)];

        let after_str;
        if let Some(ref after) = after_ms {
            after_str = after.clone();
            params.push(("after", &after_str));
        }

        let response: RecentlyPlayedResponse = self
            .client
            .get_with_params("me/player/recently-played", &params)
            .await?;

        records_fetched = response.items.len();

        tracing::debug!(
            count = response.items.len(),
            has_cursors = response.cursors.is_some(),
            "Fetched Spotify recently played tracks"
        );

        for item in &response.items {
            // Track watermarks
            if let Ok(ts) = item.played_at.parse::<DateTime<Utc>>() {
                earliest_record_at = Some(match earliest_record_at {
                    Some(min) if ts < min => ts,
                    Some(min) => min,
                    None => ts,
                });
                latest_record_at = Some(match latest_record_at {
                    Some(max) if ts > max => ts,
                    Some(max) => max,
                    None => ts,
                });
            }

            // Primary artist name (first in the list)
            let artist_name = item.track.artists.first().map(|a| a.name.clone());
            let album_name = item.track.album.as_ref().map(|a| a.name.clone());

            let record = serde_json::json!({
                "track_id": item.track.id,
                "track_name": item.track.name,
                "track_uri": item.track.uri,
                "artist_name": artist_name,
                "album_name": album_name,
                "duration_ms": item.track.duration_ms,
                "played_at": item.played_at,
                "context_type": item.context.as_ref().map(|c| &c.context_type),
                "context_uri": item.context.as_ref().and_then(|c| c.uri.as_ref()),
                "explicit": item.track.explicit,
                "is_local": item.track.is_local,
                "all_artists": item.track.artists.iter().map(|a| &a.name).collect::<Vec<_>>(),
                "album_images": item.track.album.as_ref().and_then(|a| a.images.as_ref()),
            });

            let event_ts = item.played_at.parse::<DateTime<Utc>>().ok();

            match {
                let mut writer = self.stream_writer.lock().await;
                writer.write_record(&self.source_id, "recently_played", record, event_ts)
            } {
                Ok(_) => {
                    records_written += 1;
                }
                Err(e) => {
                    tracing::warn!(
                        track = %item.track.name,
                        error = %e,
                        "Failed to write Spotify track"
                    );
                    records_failed += 1;
                }
            }
        }

        // Save cursor: use the `after` cursor from the response for next sync
        if let Some(ref cursors) = response.cursors {
            if let Some(ref after) = cursors.after {
                self.save_sync_token(after).await?;
                next_cursor = Some(after.clone());
            }
        }

        let completed_at = Utc::now();

        // Collect records from StreamWriter
        let records = {
            let mut writer = self.stream_writer.lock().await;
            let collected = writer
                .collect_records(&self.source_id, "recently_played")
                .map(|(records, _, _)| records);

            if let Some(ref recs) = collected {
                tracing::info!(
                    record_count = recs.len(),
                    "Collected Spotify records from StreamWriter"
                );
            }

            collected
        };

        tracing::info!(
            records_fetched,
            records_written,
            records_failed,
            "Spotify recently played sync completed"
        );

        Ok(SyncResult {
            records_fetched,
            records_written,
            records_failed,
            next_cursor,
            earliest_record_at,
            latest_record_at,
            started_at,
            completed_at,
            records,
            archive_job_id: None,
        })
    }

    async fn get_last_sync_token(&self) -> Result<Option<String>> {
        let row = sqlx::query_as::<_, (Option<String>,)>(
            "SELECT last_sync_token FROM elt_stream_connections WHERE source_connection_id = $1 AND stream_name = 'recently_played'",
        )
        .bind(&self.source_id)
        .fetch_optional(&self.db)
        .await?;

        Ok(row.and_then(|(token,)| token))
    }

    async fn save_sync_token(&self, token: &str) -> Result<()> {
        sqlx::query(
            "UPDATE elt_stream_connections SET last_sync_token = $1, last_sync_at = $2 WHERE source_connection_id = $3 AND stream_name = 'recently_played'"
        )
        .bind(token)
        .bind(Utc::now())
        .bind(&self.source_id)
        .execute(&self.db)
        .await?;

        Ok(())
    }
}

#[async_trait]
impl PullStream for SpotifyRecentlyPlayedStream {
    async fn sync_pull(&self, mode: SyncMode) -> Result<SyncResult> {
        self.sync_internal(&mode).await
    }

    async fn load_config(&mut self, _db: &SqlitePool, _source_id: &str) -> Result<()> {
        Ok(())
    }

    fn table_name(&self) -> &str {
        "stream_spotify_recently_played"
    }

    fn stream_name(&self) -> &str {
        "recently_played"
    }

    fn source_name(&self) -> &str {
        "spotify"
    }

    fn supports_incremental(&self) -> bool {
        true
    }

    fn supports_full_refresh(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_spotify_params_construction() {
        let limit = "50";
        let after = "1707000000000";

        let params: Vec<(&str, &str)> = vec![
            ("limit", limit),
            ("after", after),
        ];

        assert_eq!(params.len(), 2);
        assert_eq!(params[0], ("limit", "50"));
        assert_eq!(params[1], ("after", "1707000000000"));
    }

    #[test]
    fn test_dedup_key_format() {
        let track_id = "4uLU6hMCjMI75M1A2tKUQC";
        let played_at = "2024-02-04T13:30:00.000Z";
        let dedup_key = format!("{}:{}", track_id, played_at);
        assert_eq!(dedup_key, "4uLU6hMCjMI75M1A2tKUQC:2024-02-04T13:30:00.000Z");
    }
}

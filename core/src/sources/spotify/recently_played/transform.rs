//! Spotify recently played to activity_listening ontology transformation

use async_trait::async_trait;

use crate::database::Database;
use crate::error::Result;
use crate::jobs::TransformContext;
use crate::sources::base::{OntologyTransform, TransformRegistration, TransformResult};

const BATCH_SIZE: usize = 500;

/// Transform Spotify recently played tracks to activity_listening ontology
pub struct SpotifyListeningTransform;

#[async_trait]
impl OntologyTransform for SpotifyListeningTransform {
    fn source_table(&self) -> &str {
        "stream_spotify_recently_played"
    }

    fn target_table(&self) -> &str {
        "activity_listening"
    }

    fn domain(&self) -> &str {
        "activity"
    }

    #[tracing::instrument(skip(self, db, context), fields(source_table = %self.source_table(), target_table = %self.target_table()))]
    async fn transform(
        &self,
        db: &Database,
        context: &crate::jobs::transform_context::TransformContext,
        source_id: String,
    ) -> Result<TransformResult> {
        let mut records_read = 0;
        let mut records_written = 0;
        let mut records_failed = 0;
        let mut last_processed_id: Option<String> = None;

        tracing::info!(
            source_id = %source_id,
            "Starting Spotify recently played to activity_listening transformation"
        );

        let checkpoint_key = "spotify_recently_played_to_activity_listening";
        let data_source = context.get_data_source().ok_or_else(|| {
            crate::Error::Other("No data source available for transform".to_string())
        })?;
        let batches = data_source
            .read_with_checkpoint(&source_id, "recently_played", checkpoint_key)
            .await?;

        tracing::info!(
            batch_count = batches.len(),
            "Fetched Spotify recently played batches from data source"
        );

        // Batch insert buffer
        let mut pending_records: Vec<(
            String,         // id
            String,         // track_name
            Option<String>, // artist_name
            Option<String>, // album_name
            Option<i64>,    // duration_ms
            String,         // played_at
            Option<String>, // spotify_track_id
            Option<String>, // spotify_uri
            Option<String>, // context_type
            Option<String>, // context_uri
            String,         // source_stream_id (dedup key)
            serde_json::Value, // metadata
        )> = Vec::new();

        for batch in batches {
            for record in &batch.records {
                records_read += 1;

                let Some(track_id) = record.get("track_id").and_then(|v| v.as_str()) else {
                    records_failed += 1;
                    continue;
                };

                let Some(played_at) = record.get("played_at").and_then(|v| v.as_str()) else {
                    records_failed += 1;
                    continue;
                };

                let track_name = record
                    .get("track_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown Track")
                    .to_string();

                let artist_name = record
                    .get("artist_name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let album_name = record
                    .get("album_name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let duration_ms = record
                    .get("duration_ms")
                    .and_then(|v| v.as_i64());

                let track_uri = record
                    .get("track_uri")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let context_type = record
                    .get("context_type")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let context_uri = record
                    .get("context_uri")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                // Dedup key: track_id:played_at
                let source_stream_id = format!("{}:{}", track_id, played_at);

                let stream_id = record
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| source_stream_id.clone());

                // Deterministic ID
                let id = crate::ids::generate_id(
                    "activity_listening",
                    &[&source_id, track_id, played_at],
                );

                // Extra metadata (all artists, album art, etc.)
                let metadata = serde_json::json!({
                    "all_artists": record.get("all_artists"),
                    "explicit": record.get("explicit"),
                    "is_local": record.get("is_local"),
                    "album_images": record.get("album_images"),
                });

                last_processed_id = Some(stream_id);

                pending_records.push((
                    id,
                    track_name,
                    artist_name,
                    album_name,
                    duration_ms,
                    played_at.to_string(),
                    Some(track_id.to_string()),
                    track_uri,
                    context_type,
                    context_uri,
                    source_stream_id,
                    metadata,
                ));

                if pending_records.len() >= BATCH_SIZE {
                    match execute_listening_batch_insert(db, &source_id, &pending_records).await {
                        Ok(written) => records_written += written,
                        Err(e) => {
                            tracing::warn!(error = %e, "Batch insert failed");
                            records_failed += pending_records.len();
                        }
                    }
                    pending_records.clear();
                }
            }

            if let Some(max_ts) = batch.max_timestamp {
                data_source
                    .update_checkpoint(&source_id, "recently_played", checkpoint_key, max_ts)
                    .await?;
            }
        }

        // Final batch
        if !pending_records.is_empty() {
            match execute_listening_batch_insert(db, &source_id, &pending_records).await {
                Ok(written) => records_written += written,
                Err(e) => {
                    tracing::warn!(error = %e, "Final batch insert failed");
                    records_failed += pending_records.len();
                }
            }
        }

        tracing::info!(
            source_id = %source_id,
            records_read,
            records_written,
            records_failed,
            "Spotify to activity_listening transformation completed"
        );

        Ok(TransformResult {
            records_read,
            records_written,
            records_failed,
            last_processed_id,
            chained_transforms: vec![],
        })
    }
}

async fn execute_listening_batch_insert(
    db: &Database,
    source_connection_id: &str,
    records: &[(
        String,         // id
        String,         // track_name
        Option<String>, // artist_name
        Option<String>, // album_name
        Option<i64>,    // duration_ms
        String,         // played_at
        Option<String>, // spotify_track_id
        Option<String>, // spotify_uri
        Option<String>, // context_type
        Option<String>, // context_uri
        String,         // source_stream_id
        serde_json::Value, // metadata
    )],
) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let query_str = Database::build_batch_insert_query(
        "data_activity_listening",
        &[
            "id",
            "track_name",
            "artist_name",
            "album_name",
            "duration_ms",
            "played_at",
            "spotify_track_id",
            "spotify_uri",
            "context_type",
            "context_uri",
            "source_stream_id",
            "source_connection_id",
            "source_table",
            "source_provider",
            "metadata",
        ],
        "id",
        records.len(),
    );

    let mut query = sqlx::query(&query_str);

    for (
        id,
        track_name,
        artist_name,
        album_name,
        duration_ms,
        played_at,
        spotify_track_id,
        spotify_uri,
        context_type,
        context_uri,
        source_stream_id,
        metadata,
    ) in records
    {
        query = query
            .bind(id)
            .bind(track_name)
            .bind(artist_name)
            .bind(album_name)
            .bind(duration_ms)
            .bind(played_at)
            .bind(spotify_track_id)
            .bind(spotify_uri)
            .bind(context_type)
            .bind(context_uri)
            .bind(source_stream_id)
            .bind(source_connection_id)
            .bind("stream_spotify_recently_played")
            .bind("spotify")
            .bind(metadata);
    }

    let result = query.execute(db.pool()).await?;
    Ok(result.rows_affected() as usize)
}

// Self-registration for backward compatibility
struct SpotifyListeningTransformRegistration;

impl TransformRegistration for SpotifyListeningTransformRegistration {
    fn source_table(&self) -> &'static str {
        "stream_spotify_recently_played"
    }
    fn target_table(&self) -> &'static str {
        "activity_listening"
    }
    fn create(&self, _context: &TransformContext) -> Result<Box<dyn OntologyTransform>> {
        Ok(Box::new(SpotifyListeningTransform))
    }
}

inventory::submit! {
    &SpotifyListeningTransformRegistration as &dyn TransformRegistration
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_metadata() {
        let transform = SpotifyListeningTransform;
        assert_eq!(transform.source_table(), "stream_spotify_recently_played");
        assert_eq!(transform.target_table(), "activity_listening");
        assert_eq!(transform.domain(), "activity");
    }

    #[test]
    fn test_dedup_key_construction() {
        let track_id = "4uLU6hMCjMI75M1A2tKUQC";
        let played_at = "2024-02-04T13:30:00.000Z";
        let key = format!("{}:{}", track_id, played_at);
        assert!(key.contains(':'));
        assert!(key.starts_with("4uLU6h"));
    }

    #[test]
    fn test_duration_ms_to_minutes() {
        let duration_ms = 234000i64;
        let minutes = duration_ms / 60000;
        assert_eq!(minutes, 3);
    }
}

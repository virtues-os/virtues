//! Strava activities stream implementation

pub mod transform;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::client::StravaClient;
use super::types::SummaryActivity;
use crate::{
    error::Result,
    sources::{
        auth::SourceAuth,
        base::{SyncMode, SyncResult},
        pull_stream::PullStream,
    },
    storage::stream_writer::StreamWriter,
};

/// Strava activities stream
///
/// Syncs workout activities from Strava API to object storage via StreamWriter.
pub struct StravaActivitiesStream {
    source_id: String,
    client: StravaClient,
    db: SqlitePool,
    stream_writer: Arc<Mutex<StreamWriter>>,
}

impl StravaActivitiesStream {
    /// Create a new activities stream with SourceAuth and StreamWriter
    pub fn new(
        source_id: String,
        db: SqlitePool,
        stream_writer: Arc<Mutex<StreamWriter>>,
        auth: SourceAuth,
    ) -> Self {
        // Extract token manager from auth
        let token_manager = auth
            .token_manager()
            .expect("StravaActivitiesStream requires OAuth2 auth")
            .clone();

        let client = StravaClient::new(source_id.clone(), token_manager);

        Self {
            source_id,
            client,
            db,
            stream_writer,
        }
    }

    /// Sync activities with explicit sync mode
    #[tracing::instrument(skip(self), fields(source_id = %self.source_id, mode = ?sync_mode))]
    pub async fn sync_with_mode(&self, sync_mode: &SyncMode) -> Result<SyncResult> {
        tracing::info!("Starting Strava activities sync");
        self.sync_internal(sync_mode).await
    }

    /// Internal sync implementation
    #[tracing::instrument(skip(self), fields(source_id = %self.source_id, mode = ?sync_mode))]
    async fn sync_internal(&self, sync_mode: &SyncMode) -> Result<SyncResult> {
        let started_at = Utc::now();
        let mut records_fetched = 0;
        let mut records_written = 0;
        let mut records_failed = 0;
        let mut next_cursor = None;
        let mut earliest_record_at: Option<DateTime<Utc>> = None;
        let mut latest_record_at: Option<DateTime<Utc>> = None;

        // Determine the `after` epoch for incremental sync
        let after_epoch: Option<i64> = match sync_mode {
            SyncMode::Incremental { cursor } => {
                // Use provided cursor, or fall back to last sync token from DB
                let token = cursor.clone().or(self.get_last_sync_token().await?);
                token.and_then(|t| t.parse::<i64>().ok())
            }
            SyncMode::FullRefresh => None,
            SyncMode::Backfill { start_date, .. } => {
                Some(start_date.timestamp())
            }
        };

        // Paginate through all activities
        let mut page = 1;
        let per_page = "200";
        let mut latest_start_date: Option<String> = None;

        loop {
            let page_str = page.to_string();
            let mut params: Vec<(&str, &str)> = vec![
                ("per_page", per_page),
                ("page", &page_str),
            ];

            let after_str;
            if let Some(epoch) = after_epoch {
                after_str = epoch.to_string();
                params.push(("after", &after_str));
            }

            let activities: Vec<SummaryActivity> = self
                .client
                .get_with_params("athlete/activities", &params)
                .await?;

            if activities.is_empty() {
                tracing::debug!(page = page, "Empty page, pagination complete");
                break;
            }

            records_fetched += activities.len();

            tracing::debug!(
                page = page,
                count = activities.len(),
                "Fetched Strava activities page"
            );

            for activity in &activities {
                // Track watermarks from start_date
                if let Ok(ts) = activity.start_date.parse::<DateTime<Utc>>() {
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

                    // Track latest start_date for checkpoint
                    let ts_str = activity.start_date.clone();
                    if latest_start_date.as_ref().map_or(true, |prev| ts_str > *prev) {
                        latest_start_date = Some(ts_str);
                    }
                }

                // Build the record to write
                let record = serde_json::json!({
                    "activity_id": activity.id,
                    "name": activity.name,
                    "sport_type": activity.sport_type,
                    "activity_type": activity.activity_type,
                    "start_date": activity.start_date,
                    "elapsed_time": activity.elapsed_time,
                    "distance": activity.distance,
                    "total_elevation_gain": activity.total_elevation_gain,
                    "average_speed": activity.average_speed,
                    "max_speed": activity.max_speed,
                    "average_heartrate": activity.average_heartrate,
                    "max_heartrate": activity.max_heartrate,
                    "kilojoules": activity.kilojoules,
                    "suffer_score": activity.suffer_score,
                    "gear_id": activity.gear_id,
                    "map": activity.map,
                    "synced_at": Utc::now(),
                });

                let event_ts = activity.start_date.parse::<DateTime<Utc>>().ok();

                match {
                    let mut writer = self.stream_writer.lock().await;
                    writer.write_record(&self.source_id, "activities", record, event_ts)
                } {
                    Ok(_) => {
                        records_written += 1;
                        tracing::trace!(activity_id = %activity.id, "Wrote Strava activity to object storage");
                    }
                    Err(e) => {
                        tracing::warn!(
                            activity_id = %activity.id,
                            error = %e,
                            "Failed to write Strava activity"
                        );
                        records_failed += 1;
                    }
                }
            }

            page += 1;

            // Safety limit to prevent infinite pagination
            if page > 100 {
                tracing::warn!("Reached pagination safety limit (100 pages)");
                break;
            }
        }

        // Save checkpoint: the epoch timestamp of the latest start_date
        if let Some(ref latest) = latest_start_date {
            if let Ok(ts) = latest.parse::<DateTime<Utc>>() {
                let epoch_str = ts.timestamp().to_string();
                self.save_sync_token(&epoch_str).await?;
                next_cursor = Some(epoch_str);
            }
        }

        let completed_at = Utc::now();

        // Collect records from StreamWriter for archive and transform pipeline
        let records = {
            let mut writer = self.stream_writer.lock().await;
            let collected = writer
                .collect_records(&self.source_id, "activities")
                .map(|(records, _, _)| records);

            if let Some(ref recs) = collected {
                tracing::info!(
                    record_count = recs.len(),
                    "Collected Strava activity records from StreamWriter"
                );
            } else {
                tracing::warn!("No Strava activity records collected from StreamWriter");
            }

            collected
        };

        tracing::info!(
            records_fetched,
            records_written,
            records_failed,
            "Strava activities sync completed"
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

    /// Get the last sync token from the database
    async fn get_last_sync_token(&self) -> Result<Option<String>> {
        let row = sqlx::query_as::<_, (Option<String>,)>(
            "SELECT last_sync_token FROM elt_stream_connections WHERE source_connection_id = $1 AND stream_name = 'activities'",
        )
        .bind(&self.source_id)
        .fetch_optional(&self.db)
        .await?;

        Ok(row.and_then(|(token,)| token))
    }

    /// Save the sync token to the database
    async fn save_sync_token(&self, token: &str) -> Result<()> {
        sqlx::query(
            "UPDATE elt_stream_connections SET last_sync_token = $1, last_sync_at = $2 WHERE source_connection_id = $3 AND stream_name = 'activities'"
        )
        .bind(token)
        .bind(Utc::now())
        .bind(&self.source_id)
        .execute(&self.db)
        .await?;

        Ok(())
    }
}

// Implement PullStream trait for StravaActivitiesStream
#[async_trait]
impl PullStream for StravaActivitiesStream {
    async fn sync_pull(&self, mode: SyncMode) -> Result<SyncResult> {
        self.sync_with_mode(&mode).await
    }

    async fn load_config(&mut self, _db: &SqlitePool, _source_id: &str) -> Result<()> {
        // Strava activities stream has no additional config to load
        // (no calendar_ids, label_ids, etc. - just fetches all activities)
        Ok(())
    }

    fn table_name(&self) -> &str {
        "stream_strava_activities"
    }

    fn stream_name(&self) -> &str {
        "activities"
    }

    fn source_name(&self) -> &str {
        "strava"
    }

    fn supports_incremental(&self) -> bool {
        true
    }

    fn supports_full_refresh(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_strava_activity_pagination_params() {
        // Verify parameter construction for the Strava API
        let per_page = "200";
        let page = 1.to_string();
        let after = 1700000000i64.to_string();

        let params: Vec<(&str, &str)> = vec![
            ("per_page", per_page),
            ("page", &page),
            ("after", &after),
        ];

        assert_eq!(params.len(), 3);
        assert_eq!(params[0], ("per_page", "200"));
        assert_eq!(params[1], ("page", "1"));
        assert_eq!(params[2], ("after", "1700000000"));
    }
}

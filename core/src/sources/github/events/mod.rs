//! GitHub Events stream implementation
//!
//! Pulls activity events from the GitHub Events API and stores them
//! in the stream_github_events table via StreamWriter.

pub mod transform;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::{
    client::GitHubClient,
    types::{GitHubEvent, GitHubUser},
};
use crate::{
    error::Result,
    sources::{
        auth::SourceAuth,
        base::SyncResult,
        pull_stream::{PullStream, SyncMode},
    },
    storage::stream_writer::StreamWriter,
};

/// Maximum number of pages to fetch (GitHub caps at 10 pages / 300 events)
const MAX_PAGES: u32 = 4;

/// Events per page (GitHub maximum is 100)
const PER_PAGE: u32 = 100;

/// GitHub Events stream
///
/// Syncs user activity events from the GitHub Events API to object storage
/// via StreamWriter. Events include stars, forks, pushes, PR activity, etc.
///
/// GitHub API constraints:
/// - Only returns the last 90 days of events (in practice ~30 days)
/// - Maximum 300 events total (10 pages of 30, or ~3 pages of 100)
/// - Events are returned in reverse chronological order
pub struct GitHubEventsStream {
    source_id: String,
    client: GitHubClient,
    db: SqlitePool,
    stream_writer: Arc<Mutex<StreamWriter>>,
}

impl GitHubEventsStream {
    /// Create a new GitHub events stream with SourceAuth and StreamWriter
    pub fn new(
        source_id: String,
        db: SqlitePool,
        stream_writer: Arc<Mutex<StreamWriter>>,
        auth: SourceAuth,
    ) -> Self {
        let token_manager = auth
            .token_manager()
            .expect("GitHubEventsStream requires OAuth2 auth")
            .clone();

        let client = GitHubClient::new(source_id.clone(), token_manager);

        Self {
            source_id,
            client,
            db,
            stream_writer,
        }
    }

    /// Sync events with explicit sync mode
    #[tracing::instrument(skip(self), fields(source_id = %self.source_id, mode = ?sync_mode))]
    async fn sync_with_mode(&self, sync_mode: &SyncMode) -> Result<SyncResult> {
        tracing::info!("Starting GitHub events sync");
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

        // Fetch the authenticated user's login name
        let user: GitHubUser = self.client.get("user").await?;
        let username = user.login;

        tracing::info!(username = %username, "Fetching events for GitHub user");

        // Get the cursor (newest event timestamp from last sync) for incremental mode
        let last_cursor = match sync_mode {
            SyncMode::Incremental { cursor } => {
                let stored = cursor.clone().or(self.get_last_cursor().await?);
                stored
            }
            _ => None,
        };

        // Paginate through events
        let mut page = 1u32;
        'pagination: loop {
            if page > MAX_PAGES {
                tracing::debug!("Reached max pages ({}), stopping", MAX_PAGES);
                break;
            }

            let page_str = page.to_string();
            let per_page_str = PER_PAGE.to_string();
            let params = vec![
                ("per_page", per_page_str.as_str()),
                ("page", page_str.as_str()),
            ];

            let events: Vec<GitHubEvent> = self
                .client
                .get_with_params(&format!("users/{username}/events"), &params)
                .await?;

            if events.is_empty() {
                tracing::debug!(page = page, "Empty page, stopping pagination");
                break;
            }

            records_fetched += events.len();

            tracing::debug!(
                page = page,
                events_count = events.len(),
                "Fetched GitHub events page"
            );

            for event in &events {
                // Parse the event timestamp
                let event_time = DateTime::parse_from_rfc3339(&event.created_at)
                    .ok()
                    .map(|dt| dt.with_timezone(&Utc));

                // For incremental sync: skip events we've already seen
                if let (Some(cursor_ts), Some(evt_ts)) = (&last_cursor, &event_time) {
                    if let Ok(cursor_dt) = DateTime::parse_from_rfc3339(cursor_ts) {
                        let cursor_utc = cursor_dt.with_timezone(&Utc);
                        if *evt_ts <= cursor_utc {
                            // We've reached events we already have, stop
                            tracing::debug!(
                                event_id = %event.id,
                                event_time = %evt_ts,
                                cursor = %cursor_utc,
                                "Reached already-synced events, stopping"
                            );
                            break 'pagination;
                        }
                    }
                }

                // Update watermarks
                if let Some(ts) = event_time {
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

                // Build record for StreamWriter
                let record = serde_json::json!({
                    "event_id": event.id,
                    "event_type": event.event_type,
                    "actor_login": event.actor.login,
                    "actor_id": event.actor.id,
                    "repo_name": event.repo.name,
                    "repo_id": event.repo.id,
                    "payload": event.payload,
                    "public": event.public,
                    "created_at": event.created_at,
                    "org": event.org.as_ref().map(|o| serde_json::json!({
                        "id": o.id,
                        "login": o.login,
                    })),
                    "synced_at": Utc::now(),
                });

                // Write to object storage via StreamWriter
                match {
                    let mut writer = self.stream_writer.lock().await;
                    writer.write_record(
                        &self.source_id,
                        "events",
                        record,
                        event_time,
                    )
                } {
                    Ok(_) => {
                        records_written += 1;
                        tracing::trace!(event_id = %event.id, "Wrote GitHub event to object storage");
                    }
                    Err(e) => {
                        tracing::warn!(
                            event_id = %event.id,
                            error = %e,
                            "Failed to write GitHub event"
                        );
                        records_failed += 1;
                    }
                }
            }

            page += 1;
        }

        // Save cursor (newest event timestamp) for incremental sync
        // Transaction scoped tightly to avoid holding DB lock during network I/O
        if let Some(latest) = latest_record_at {
            let cursor = latest.to_rfc3339();
            let mut tx = self.db.begin().await?;
            self.save_cursor_with_tx(&cursor, &mut tx).await?;
            tx.commit().await?;
            next_cursor = Some(cursor);
        }

        let completed_at = Utc::now();

        // Collect records from StreamWriter for archive and transform pipeline
        let records = {
            let mut writer = self.stream_writer.lock().await;
            let collected = writer
                .collect_records(&self.source_id, "events")
                .map(|(records, _, _)| records);

            if let Some(ref recs) = collected {
                tracing::info!(
                    record_count = recs.len(),
                    "Collected GitHub events records from StreamWriter"
                );
            } else {
                tracing::warn!("No GitHub events records collected from StreamWriter");
            }

            collected
        };

        tracing::info!(
            records_fetched,
            records_written,
            records_failed,
            "GitHub events sync completed"
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

    /// Get the last sync cursor from the database
    async fn get_last_cursor(&self) -> Result<Option<String>> {
        let row = sqlx::query_as::<_, (Option<String>,)>(
            "SELECT last_sync_token FROM elt_stream_connections WHERE source_connection_id = $1 AND stream_name = 'events'",
        )
        .bind(&self.source_id)
        .fetch_optional(&self.db)
        .await?;

        Ok(row.and_then(|(token,)| token))
    }

    /// Save the sync cursor within a transaction
    async fn save_cursor_with_tx(
        &self,
        cursor: &str,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE elt_stream_connections SET last_sync_token = $1, last_sync_at = $2 WHERE source_connection_id = $3 AND stream_name = 'events'"
        )
        .bind(cursor)
        .bind(Utc::now())
        .bind(&self.source_id)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }
}

// Implement PullStream trait for GitHubEventsStream
#[async_trait]
impl PullStream for GitHubEventsStream {
    async fn sync_pull(&self, mode: SyncMode) -> Result<SyncResult> {
        self.sync_with_mode(&mode).await
    }

    async fn load_config(&mut self, _db: &SqlitePool, _source_id: &str) -> Result<()> {
        // GitHub events stream has no user-configurable options currently
        Ok(())
    }

    fn table_name(&self) -> &str {
        "stream_github_events"
    }

    fn stream_name(&self) -> &str {
        "events"
    }

    fn source_name(&self) -> &str {
        "github"
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
    fn test_constants() {
        assert_eq!(super::MAX_PAGES, 4);
        assert_eq!(super::PER_PAGE, 100);
    }
}

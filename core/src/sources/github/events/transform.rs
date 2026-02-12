//! GitHub Events to content_bookmark ontology transformation
//!
//! Transforms WatchEvent (stars) and ForkEvent from stream_github_events
//! into the normalized data_content_bookmark ontology table.
//!
//! Other event types (PushEvent, PullRequestEvent, etc.) are silently skipped
//! and will be handled by future transforms.

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::database::Database;
use crate::error::Result;
use crate::jobs::TransformContext;
use crate::sources::base::{OntologyTransform, TransformRegistration, TransformResult};

/// Batch size for bulk inserts
const BATCH_SIZE: usize = 500;

/// Transform GitHub events (WatchEvent, ForkEvent) to content_bookmark ontology
///
/// This transform is registered with the stream in the unified registry,
/// and also self-registers via inventory for backward compatibility.
pub struct GitHubBookmarkTransform;

#[async_trait]
impl OntologyTransform for GitHubBookmarkTransform {
    fn source_table(&self) -> &str {
        "stream_github_events"
    }

    fn target_table(&self) -> &str {
        "content_bookmark"
    }

    fn domain(&self) -> &str {
        "content"
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

        let transform_start = std::time::Instant::now();

        tracing::info!(
            source_id = %source_id,
            "Starting GitHub events to content_bookmark transformation"
        );

        // Read stream data using data source (memory for hot path)
        let checkpoint_key = "github_events_to_content_bookmark";
        let read_start = std::time::Instant::now();
        let data_source = context.get_data_source().ok_or_else(|| {
            crate::Error::Other("No data source available for transform".to_string())
        })?;
        let batches = data_source
            .read_with_checkpoint(&source_id, "events", checkpoint_key)
            .await?;
        let read_duration = read_start.elapsed();

        tracing::info!(
            batch_count = batches.len(),
            read_duration_ms = read_duration.as_millis(),
            source_type = ?data_source.source_type(),
            "Fetched GitHub events batches from data source"
        );

        // Pending records for batch insert
        // Fields: (id, source_connection_id, url, title, description, source_platform,
        //          bookmark_type, content_type, author, timestamp, source_stream_id,
        //          source_table, source_provider, metadata)
        let mut pending_records: Vec<(
            String,            // id
            String,            // source_connection_id
            String,            // url
            Option<String>,    // title
            Option<String>,    // description
            &'static str,     // source_platform
            String,            // bookmark_type
            &'static str,     // content_type
            Option<String>,    // author
            DateTime<Utc>,     // timestamp
            String,            // source_stream_id
            &'static str,     // source_table
            &'static str,     // source_provider
            serde_json::Value, // metadata
        )> = Vec::new();

        let mut batch_insert_total_ms = 0u128;
        let mut batch_insert_count = 0;

        let processing_start = std::time::Instant::now();

        for batch in batches {
            tracing::debug!(batch_record_count = batch.records.len(), "Processing batch");

            for record in &batch.records {
                records_read += 1;

                // Extract event type - only process WatchEvent and ForkEvent
                let Some(event_type) = record.get("event_type").and_then(|v| v.as_str()) else {
                    continue;
                };

                let bookmark_type = match event_type {
                    "WatchEvent" => "star",
                    "ForkEvent" => "fork",
                    _ => continue, // Silently skip non-bookmark event types
                };

                // Extract required fields
                let Some(event_id) = record.get("event_id").and_then(|v| v.as_str()) else {
                    records_failed += 1;
                    continue;
                };

                let Some(repo_name) = record.get("repo_name").and_then(|v| v.as_str()) else {
                    records_failed += 1;
                    continue;
                };

                let Some(created_at_str) = record.get("created_at").and_then(|v| v.as_str())
                else {
                    records_failed += 1;
                    continue;
                };

                let timestamp = match DateTime::parse_from_rfc3339(created_at_str) {
                    Ok(dt) => dt.with_timezone(&Utc),
                    Err(_) => {
                        records_failed += 1;
                        continue;
                    }
                };

                let stream_id = record
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or(event_id)
                    .to_string();

                let actor_login = record
                    .get("actor_login")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                // Extract description from payload
                // ForkEvent: payload.forkee.description, WatchEvent: no description
                let description = record
                    .get("payload")
                    .and_then(|p| {
                        p.get("forkee")
                            .and_then(|f| f.get("description"))
                            .or_else(|| p.get("description"))
                    })
                    .and_then(|d| d.as_str())
                    .map(String::from);

                // Build GitHub URL from repo name
                let url = format!("https://github.com/{}", repo_name);

                // Generate deterministic ID
                let id = crate::ids::generate_id(
                    "content_bookmark",
                    &[&source_id, event_id],
                );

                // Build metadata
                let metadata = serde_json::json!({
                    "github_event_id": event_id,
                    "github_event_type": event_type,
                    "actor_login": actor_login,
                    "repo_name": repo_name,
                    "source_connection_id": source_id,
                });

                pending_records.push((
                    id,
                    source_id.clone(),
                    url,
                    Some(repo_name.to_string()), // title
                    description,
                    "github",        // source_platform
                    bookmark_type.to_string(),
                    "repository",    // content_type
                    actor_login,     // author
                    timestamp,
                    stream_id,
                    "stream_github_events", // source_table
                    "github",               // source_provider
                    metadata,
                ));

                last_processed_id = Some(event_id.to_string());

                // Execute batch insert when we reach batch size
                if pending_records.len() >= BATCH_SIZE {
                    let insert_start = std::time::Instant::now();
                    let batch_result =
                        execute_bookmark_batch_insert(db, &pending_records).await;
                    let insert_duration = insert_start.elapsed();
                    batch_insert_total_ms += insert_duration.as_millis();
                    batch_insert_count += 1;

                    tracing::info!(
                        batch_size = pending_records.len(),
                        insert_duration_ms = insert_duration.as_millis(),
                        "Executed batch insert"
                    );

                    match batch_result {
                        Ok(written) => {
                            records_written += written;
                        }
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                batch_size = pending_records.len(),
                                "Batch insert failed"
                            );
                            records_failed += pending_records.len();
                        }
                    }
                    pending_records.clear();
                }
            }

            // Update checkpoint after processing batch
            if let Some(max_ts) = batch.max_timestamp {
                data_source
                    .update_checkpoint(&source_id, "events", checkpoint_key, max_ts)
                    .await?;
            }
        }

        // Insert any remaining records
        if !pending_records.is_empty() {
            let insert_start = std::time::Instant::now();
            let batch_result = execute_bookmark_batch_insert(db, &pending_records).await;
            let insert_duration = insert_start.elapsed();
            batch_insert_total_ms += insert_duration.as_millis();
            batch_insert_count += 1;

            tracing::info!(
                batch_size = pending_records.len(),
                insert_duration_ms = insert_duration.as_millis(),
                "Executed final batch insert"
            );

            match batch_result {
                Ok(written) => {
                    records_written += written;
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        batch_size = pending_records.len(),
                        "Final batch insert failed"
                    );
                    records_failed += pending_records.len();
                }
            }
        }

        let processing_duration = processing_start.elapsed();
        let total_duration = transform_start.elapsed();

        tracing::info!(
            source_id = %source_id,
            records_read,
            records_written,
            records_failed,
            total_duration_ms = total_duration.as_millis(),
            processing_duration_ms = processing_duration.as_millis(),
            read_duration_ms = read_duration.as_millis(),
            batch_insert_total_ms,
            batch_insert_count,
            avg_batch_insert_ms = if batch_insert_count > 0 { batch_insert_total_ms / batch_insert_count as u128 } else { 0 },
            "GitHub events to content_bookmark transformation completed"
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

/// Execute batch insert for content_bookmark records
///
/// Builds and executes a multi-row INSERT statement for efficient bulk insertion.
async fn execute_bookmark_batch_insert(
    db: &Database,
    records: &[(
        String,            // id
        String,            // source_connection_id
        String,            // url
        Option<String>,    // title
        Option<String>,    // description
        &str,              // source_platform
        String,            // bookmark_type
        &str,              // content_type
        Option<String>,    // author
        DateTime<Utc>,     // timestamp
        String,            // source_stream_id
        &str,              // source_table
        &str,              // source_provider
        serde_json::Value, // metadata
    )],
) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let query_str = Database::build_batch_insert_query(
        "data_content_bookmark",
        &[
            "id",
            "source_connection_id",
            "url",
            "title",
            "description",
            "source_platform",
            "bookmark_type",
            "content_type",
            "author",
            "timestamp",
            "source_stream_id",
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
        source_connection_id,
        url,
        title,
        description,
        source_platform,
        bookmark_type,
        content_type,
        author,
        timestamp,
        source_stream_id,
        source_table,
        source_provider,
        metadata,
    ) in records
    {
        let metadata_str = serde_json::to_string(metadata).unwrap_or_else(|_| "{}".to_string());

        query = query
            .bind(id)
            .bind(source_connection_id)
            .bind(url)
            .bind(title)
            .bind(description)
            .bind(source_platform)
            .bind(bookmark_type)
            .bind(content_type)
            .bind(author)
            .bind(timestamp)
            .bind(source_stream_id)
            .bind(source_table)
            .bind(source_provider)
            .bind(metadata_str);
    }

    let result = query.execute(db.pool()).await?;
    Ok(result.rows_affected() as usize)
}

// Self-registration for backward compatibility with inventory-based lookup
struct GitHubBookmarkTransformRegistration;

impl TransformRegistration for GitHubBookmarkTransformRegistration {
    fn source_table(&self) -> &'static str {
        "stream_github_events"
    }
    fn target_table(&self) -> &'static str {
        "content_bookmark"
    }
    fn create(&self, _context: &TransformContext) -> Result<Box<dyn OntologyTransform>> {
        Ok(Box::new(GitHubBookmarkTransform))
    }
}

inventory::submit! {
    &GitHubBookmarkTransformRegistration as &dyn TransformRegistration
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_metadata() {
        let transform = GitHubBookmarkTransform;
        assert_eq!(transform.source_table(), "stream_github_events");
        assert_eq!(transform.target_table(), "content_bookmark");
        assert_eq!(transform.domain(), "content");
    }
}

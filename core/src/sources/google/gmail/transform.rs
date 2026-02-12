//! Gmail to communication_email ontology transformation
//!
//! Transforms raw Gmail messages from stream_google_gmail into the normalized
//! communication_email ontology table.
//!
//! ## Multi-Account Support
//!
//! The communication_email table uses UNIQUE (source_stream_id) for deduplication.
//! This allows the same Gmail message to exist across multiple Gmail accounts
//! (e.g., personal@gmail.com and work@gmail.com) since each account has its own
//! unique source_stream_id. This is the correct behavior when an email is CC'd
//! to multiple accounts owned by the same user.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::database::Database;
use crate::error::Result;
use crate::jobs::TransformContext;
use crate::sources::base::{OntologyTransform, TransformRegistration, TransformResult};

/// Batch size for bulk inserts
const BATCH_SIZE: usize = 500;

/// Transform Gmail messages to communication_email ontology
pub struct GmailEmailTransform;

#[async_trait]
impl OntologyTransform for GmailEmailTransform {
    fn source_table(&self) -> &str {
        "stream_google_gmail"
    }

    fn target_table(&self) -> &str {
        "communication_email"
    }

    fn domain(&self) -> &str {
        "communication"
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
            "Starting Gmail to communication_email transformation"
        );

        // Read stream data using data source (memory for hot path)
        let checkpoint_key = "gmail_to_communication_email";
        let read_start = std::time::Instant::now();
        let data_source = context.get_data_source().ok_or_else(|| {
            crate::Error::Other("No data source available for transform".to_string())
        })?;
        let batches = data_source
            .read_with_checkpoint(&source_id, "gmail", checkpoint_key)
            .await?;
        let read_duration = read_start.elapsed();

        tracing::info!(
            batch_count = batches.len(),
            read_duration_ms = read_duration.as_millis(),
            source_type = ?data_source.source_type(),
            "Fetched Gmail batches from data source"
        );

        // Batch insert configuration
        // Tuple: (id, message_id, thread_id, subject, body_preview, body, timestamp, from_email, from_name,
        //         to_emails, to_names, cc_emails, cc_names, direction, labels, is_read, is_starred, has_attachments, stream_id)
        let mut pending_records: Vec<(
            String,        // id (deterministic)
            String,        // message_id
            String,        // thread_id
            Option<String>, // subject
            Option<String>, // body_preview (snippet)
            Option<String>, // body
            DateTime<Utc>, // timestamp
            Option<String>, // from_email
            Option<String>, // from_name
            Vec<String>,   // to_emails
            Vec<String>,   // to_names
            Vec<String>,   // cc_emails
            Vec<String>,   // cc_names
            &'static str,  // direction
            Vec<String>,   // labels
            bool,          // is_read
            bool,          // is_starred
            bool,          // has_attachments
            Uuid,          // source_stream_id
        )> = Vec::new();
        let mut batch_insert_total_ms = 0u128;
        let mut batch_insert_count = 0;

        let processing_start = std::time::Instant::now();

        for batch in batches {
            tracing::debug!(batch_record_count = batch.records.len(), "Processing batch");

            for record in &batch.records {
                records_read += 1;

                // Extract required fields from JSONL record
                let Some(message_id) = record.get("message_id").and_then(|v| v.as_str()) else {
                    continue; // Skip records without message_id
                };
                let Some(thread_id) = record.get("thread_id").and_then(|v| v.as_str()) else {
                    continue; // Skip records without thread_id
                };

                let timestamp = record
                    .get("date")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<DateTime<Utc>>().ok())
                    .unwrap_or_else(|| Utc::now());

                let stream_id = record
                    .get("id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .unwrap_or_else(|| Uuid::new_v4());

                let subject = record
                    .get("subject")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let snippet = record
                    .get("snippet")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let body_plain = record
                    .get("body_plain")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let _body_html = record
                    .get("body_html")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let from_email = record
                    .get("from_email")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let from_name = record
                    .get("from_name")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let to_emails: Vec<String> = record
                    .get("to_emails")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let to_names: Vec<String> = record
                    .get("to_names")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let cc_emails: Vec<String> = record
                    .get("cc_emails")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let cc_names: Vec<String> = record
                    .get("cc_names")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                let labels: Vec<String> = record
                    .get("labels")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                let is_unread = record
                    .get("is_unread")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let is_starred = record
                    .get("is_starred")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let has_attachments = record
                    .get("has_attachments")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let is_sent = record
                    .get("is_sent")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                // Determine direction
                let direction = if is_sent { "sent" } else { "received" };

                // Get source_connection_id for deterministic ID generation
                let source_connection_id = record
                    .get("source_connection_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                // Generate deterministic ID for idempotency
                let id = crate::ids::generate_id("email", &[source_connection_id, message_id]);

                // Add to pending batch
                pending_records.push((
                    id,
                    message_id.to_string(),
                    thread_id.to_string(),
                    subject,
                    snippet,      // body_preview
                    body_plain,   // body
                    timestamp,
                    from_email,
                    from_name,
                    to_emails,
                    to_names,
                    cc_emails,
                    cc_names,
                    direction,
                    labels,
                    !is_unread, // is_read = !is_unread
                    is_starred,
                    has_attachments,
                    stream_id,
                ));

                last_processed_id = Some(stream_id.to_string());

                // Execute batch insert when we reach batch size
                if pending_records.len() >= BATCH_SIZE {
                    let insert_start = std::time::Instant::now();
                    let batch_result = execute_email_batch_insert(db, &pending_records).await;
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
                    .update_checkpoint(&source_id, "gmail", checkpoint_key, max_ts)
                    .await?;
            }
        }

        // Insert any remaining records
        if !pending_records.is_empty() {
            let insert_start = std::time::Instant::now();
            let batch_result = execute_email_batch_insert(db, &pending_records).await;
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
            "Gmail to communication_email transformation completed"
        );

        Ok(TransformResult {
            records_read,
            records_written,
            records_failed,
            last_processed_id,
            chained_transforms: vec![], // Gmail transform doesn't chain to other transforms
        })
    }
}

/// Execute batch insert for email records
///
/// Builds and executes a multi-row INSERT statement for efficient bulk insertion.
async fn execute_email_batch_insert(
    db: &Database,
    records: &[(
        String,        // id (deterministic)
        String,        // message_id
        String,        // thread_id
        Option<String>, // subject
        Option<String>, // body_preview (snippet)
        Option<String>, // body
        DateTime<Utc>, // timestamp
        Option<String>, // from_email
        Option<String>, // from_name
        Vec<String>,   // to_emails
        Vec<String>,   // to_names
        Vec<String>,   // cc_emails
        Vec<String>,   // cc_names
        &str,          // direction
        Vec<String>,   // labels
        bool,          // is_read
        bool,          // is_starred
        bool,          // has_attachments
        Uuid,          // source_stream_id
    )],
) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let query_str = Database::build_batch_insert_query(
        "data_communication_email",
        &[
            "id",
            "message_id",
            "thread_id",
            "subject",
            "body_preview",
            "body",
            "timestamp",
            "from_email",
            "from_name",
            "to_emails",
            "to_names",
            "cc_emails",
            "cc_names",
            "direction",
            "labels",
            "is_read",
            "is_starred",
            "has_attachments",
            "source_stream_id",
            "source_table",
            "source_provider",
        ],
        "id",
        records.len(),
    );

    let mut query = sqlx::query(&query_str);

    // Bind all parameters row by row
    for (
        id,
        message_id,
        thread_id,
        subject,
        body_preview,
        body,
        timestamp,
        from_email,
        from_name,
        to_emails,
        to_names,
        cc_emails,
        cc_names,
        direction,
        labels,
        is_read,
        is_starred,
        has_attachments,
        stream_id,
    ) in records
    {
        // SQLite doesn't support array types, convert to JSON strings
        let to_emails_json =
            serde_json::to_string(&to_emails).unwrap_or_else(|_| "[]".to_string());
        let to_names_json = serde_json::to_string(&to_names).unwrap_or_else(|_| "[]".to_string());
        let cc_emails_json =
            serde_json::to_string(&cc_emails).unwrap_or_else(|_| "[]".to_string());
        let cc_names_json = serde_json::to_string(&cc_names).unwrap_or_else(|_| "[]".to_string());
        let labels_json = serde_json::to_string(&labels).unwrap_or_else(|_| "[]".to_string());

        query = query
            .bind(id)
            .bind(message_id)
            .bind(thread_id)
            .bind(subject)
            .bind(body_preview)
            .bind(body)
            .bind(timestamp)
            .bind(from_email)
            .bind(from_name)
            .bind(to_emails_json)
            .bind(to_names_json)
            .bind(cc_emails_json)
            .bind(cc_names_json)
            .bind(direction)
            .bind(labels_json)
            .bind(is_read)
            .bind(is_starred)
            .bind(has_attachments)
            .bind(stream_id)
            .bind("stream_google_gmail")
            .bind("google");
    }

    let result = query.execute(db.pool()).await?;
    Ok(result.rows_affected() as usize)
}

// Self-registration
struct GmailTransformRegistration;

impl TransformRegistration for GmailTransformRegistration {
    fn source_table(&self) -> &'static str {
        "stream_google_gmail"
    }
    fn target_table(&self) -> &'static str {
        "communication_email"
    }
    fn create(&self, _context: &TransformContext) -> Result<Box<dyn OntologyTransform>> {
        Ok(Box::new(GmailEmailTransform))
    }
}

inventory::submit! {
    &GmailTransformRegistration as &dyn TransformRegistration
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_metadata() {
        let transform = GmailEmailTransform;
        assert_eq!(transform.source_table(), "stream_google_gmail");
        assert_eq!(transform.target_table(), "communication_email");
        assert_eq!(transform.domain(), "communication");
    }
}

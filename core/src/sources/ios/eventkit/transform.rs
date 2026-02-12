//! EventKit stream to calendar_event ontology transformation

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::database::Database;
use crate::error::Result;
use crate::jobs::TransformContext;
use crate::sources::base::{OntologyTransform, TransformResult};

/// Batch size for bulk inserts
const BATCH_SIZE: usize = 500;

/// Transform iOS EventKit events to calendar_event ontology
pub struct IosEventKitTransform;

#[async_trait]
impl OntologyTransform for IosEventKitTransform {
    fn source_table(&self) -> &str {
        "stream_ios_eventkit"
    }

    fn target_table(&self) -> &str {
        "calendar_event"
    }

    fn domain(&self) -> &str {
        "calendar"
    }

    #[tracing::instrument(skip(self, db, context), fields(source_table = %self.source_table(), target_table = %self.target_table()))]
    async fn transform(
        &self,
        db: &Database,
        context: &TransformContext,
        source_id: String,
    ) -> Result<TransformResult> {
        let mut records_read = 0;
        let mut records_written = 0;
        let mut last_processed_id: Option<String> = None;

        let data_source = context.get_data_source().ok_or_else(|| {
            crate::Error::Other("No data source available for transform".to_string())
        })?;

        let checkpoint_key = "ios_eventkit_to_calendar_event";
        let batches = data_source
            .read_with_checkpoint(&source_id, "eventkit", checkpoint_key)
            .await?;

        // Collect calendar event records (skip reminders for now - they need a different ontology)
        let mut pending_records: Vec<CalendarRecord> = Vec::new();

        for batch in batches {
            for record in &batch.records {
                records_read += 1;

                // Only process events (not reminders) for the calendar ontology
                let record_type = record.get("record_type").and_then(|v| v.as_str()).unwrap_or("event");
                if record_type != "event" {
                    continue;
                }

                let title = record.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if title.is_empty() {
                    continue;
                }

                let calendar_name = record.get("calendarTitle").and_then(|v| v.as_str()).map(|s| s.to_string());
                let location_name = record.get("location").and_then(|v| v.as_str()).map(|s| s.to_string());
                let description = record.get("notes").and_then(|v| v.as_str()).map(|s| s.to_string());

                let start_time = record
                    .get("startDate")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<DateTime<Utc>>().ok())
                    .unwrap_or_else(Utc::now);

                let end_time = record
                    .get("endDate")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<DateTime<Utc>>().ok())
                    .unwrap_or_else(Utc::now);

                let is_all_day = record.get("isAllDay").and_then(|v| v.as_bool()).unwrap_or(false);

                let external_id = record.get("id").and_then(|v| v.as_str()).map(|s| s.to_string());
                let external_url = record.get("url").and_then(|v| v.as_str()).map(|s| s.to_string());

                let stream_id = record
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| Uuid::new_v4().to_string());

                let metadata = serde_json::json!({
                    "calendar_id": record.get("calendarId"),
                    "last_modified": record.get("lastModified"),
                });

                last_processed_id = Some(stream_id.clone());

                pending_records.push(CalendarRecord {
                    id: Uuid::new_v4().to_string(),
                    title,
                    description,
                    calendar_name,
                    location_name,
                    start_time,
                    end_time,
                    is_all_day,
                    external_id,
                    external_url,
                    stream_id,
                    metadata,
                });

                if pending_records.len() >= BATCH_SIZE {
                    let written = execute_calendar_batch_insert(db, &pending_records).await?;
                    records_written += written;
                    pending_records.clear();
                }
            }

            if let Some(max_ts) = batch.max_timestamp {
                data_source
                    .update_checkpoint(&source_id, "eventkit", checkpoint_key, max_ts)
                    .await?;
            }
        }

        if !pending_records.is_empty() {
            let written = execute_calendar_batch_insert(db, &pending_records).await?;
            records_written += written;
        }

        Ok(TransformResult {
            records_read,
            records_written,
            records_failed: 0,
            last_processed_id,
            chained_transforms: vec![],
        })
    }
}

struct CalendarRecord {
    id: String,
    title: String,
    description: Option<String>,
    calendar_name: Option<String>,
    location_name: Option<String>,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    is_all_day: bool,
    external_id: Option<String>,
    external_url: Option<String>,
    stream_id: String,
    metadata: serde_json::Value,
}

async fn execute_calendar_batch_insert(
    db: &Database,
    records: &[CalendarRecord],
) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let query_str = Database::build_batch_insert_query(
        "data_calendar_event",
        &[
            "id",
            "title",
            "description",
            "calendar_name",
            "location_name",
            "start_time",
            "end_time",
            "is_all_day",
            "external_id",
            "external_url",
            "source_stream_id",
            "source_table",
            "source_provider",
            "metadata",
        ],
        "source_stream_id",
        records.len(),
    );

    let mut query = sqlx::query(&query_str);

    for rec in records {
        query = query
            .bind(&rec.id)
            .bind(&rec.title)
            .bind(&rec.description)
            .bind(&rec.calendar_name)
            .bind(&rec.location_name)
            .bind(&rec.start_time)
            .bind(&rec.end_time)
            .bind(if rec.is_all_day { 1 } else { 0 })
            .bind(&rec.external_id)
            .bind(&rec.external_url)
            .bind(&rec.stream_id)
            .bind("stream_ios_eventkit")
            .bind("ios")
            .bind(&rec.metadata);
    }

    let result = query.execute(db.pool()).await?;
    Ok(result.rows_affected() as usize)
}

// Self-registration
struct IosEventKitRegistration;
impl crate::sources::base::TransformRegistration for IosEventKitRegistration {
    fn source_table(&self) -> &'static str {
        "stream_ios_eventkit"
    }
    fn target_table(&self) -> &'static str {
        "calendar_event"
    }
    fn create(&self, _context: &TransformContext) -> Result<Box<dyn OntologyTransform>> {
        Ok(Box::new(IosEventKitTransform))
    }
}
inventory::submit! { &IosEventKitRegistration as &dyn crate::sources::base::TransformRegistration }

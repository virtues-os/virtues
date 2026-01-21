//! Seed Testing API
//!
//! Provides endpoints to inspect the results of the Monday in Rome seed data,
//! allowing validation of the full pipeline: Archive → Transform → Entity Resolution

use crate::database::Database;
use crate::ontologies::registry::list_ontologies;
use crate::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;

/// Pipeline status overview showing all stages
#[derive(Debug, Serialize, Deserialize)]
pub struct PipelineStatus {
    pub archive_jobs: ArchiveJobsStatus,
    pub transform_jobs: TransformJobsStatus,
    pub location_clustering: LocationClusteringStatus,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ArchiveJobsStatus {
    pub total: i64,
    pub completed: i64,
    pub failed: i64,
    pub records_archived: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TransformJobsStatus {
    pub total: i64,
    pub completed: i64,
    pub records_processed: i64,
    pub ontology_tables_populated: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LocationClusteringStatus {
    pub raw_points: i64,
    pub visits_created: i64,
    pub has_data: bool,
}

/// Data quality metrics for seed data
#[derive(Debug, Serialize, Deserialize)]
pub struct DataQualityMetrics {
    pub total_records: i64,
    pub location_points: i64,
    pub location_visits: i64,
}

/// Get pipeline status for all stages
pub async fn get_pipeline_status(db: &Database) -> Result<PipelineStatus> {
    // Archive jobs status
    let archive_row = sqlx::query(
        r#"
        SELECT
            COUNT(*) as total,
            COUNT(*) FILTER (WHERE status = 'completed') as completed,
            COUNT(*) FILTER (WHERE status = 'failed') as failed,
            COALESCE(SUM(records_processed), 0)::bigint as records_archived
        FROM data_jobs
        WHERE job_type = 'archive'
        "#,
    )
    .fetch_one(db.pool())
    .await?;

    let archive_jobs = ArchiveJobsStatus {
        total: archive_row.try_get("total")?,
        completed: archive_row.try_get("completed")?,
        failed: archive_row.try_get("failed")?,
        records_archived: archive_row.try_get("records_archived")?,
    };

    // Transform jobs status
    let transform_row = sqlx::query(
        r#"
        SELECT
            COUNT(*) as total,
            COUNT(*) FILTER (WHERE status = 'completed') as completed,
            COALESCE(SUM(records_processed), 0)::bigint as records_processed
        FROM data_jobs
        WHERE job_type = 'transform'
        "#,
    )
    .fetch_one(db.pool())
    .await?;

    // Count unique ontology tables populated (rough estimate from jobs metadata)
    let ontology_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT json_extract(metadata, '$.stream_name')) FROM data_jobs WHERE job_type = 'transform' AND status = 'completed'"
    )
    .fetch_one(db.pool())
    .await
    .unwrap_or(0);

    let transform_jobs = TransformJobsStatus {
        total: transform_row.try_get("total")?,
        completed: transform_row.try_get("completed")?,
        records_processed: transform_row.try_get("records_processed")?,
        ontology_tables_populated: ontology_count,
    };

    // Location clustering status
    let location_points: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM data_location_point")
        .fetch_one(db.pool())
        .await
        .unwrap_or(0);

    let location_visits: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM data_location_visit")
        .fetch_one(db.pool())
        .await
        .unwrap_or(0);

    let location_clustering = LocationClusteringStatus {
        raw_points: location_points,
        visits_created: location_visits,
        has_data: location_visits > 0,
    };

    Ok(PipelineStatus {
        archive_jobs,
        transform_jobs,
        location_clustering,
    })
}

/// Get data quality metrics for seed data
pub async fn get_data_quality_metrics(
    db: &Database,
    _start: DateTime<Utc>,
    _end: DateTime<Utc>,
) -> Result<DataQualityMetrics> {
    // Total records across all ontologies (dynamically discovered from registry)
    let count_parts: Vec<String> = list_ontologies()
        .iter()
        .map(|o| format!("(SELECT COUNT(*) FROM data_{})", o.table_name))
        .collect();

    let total_records: i64 = if count_parts.is_empty() {
        0
    } else {
        sqlx::query_scalar(&format!("SELECT {}", count_parts.join(" + ")))
            .fetch_one(db.pool())
            .await
            .unwrap_or(0)
    };

    let location_points: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM data_location_point")
        .fetch_one(db.pool())
        .await
        .unwrap_or(0);

    let location_visits: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM data_location_visit")
        .fetch_one(db.pool())
        .await
        .unwrap_or(0);

    Ok(DataQualityMetrics {
        total_records,
        location_points,
        location_visits,
    })
}

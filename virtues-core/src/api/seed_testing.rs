//! Seed Testing API
//!
//! Provides endpoints to inspect the results of the Monday in Rome seed data,
//! allowing validation of the full pipeline: Sync → Transform → Entity Resolution

use crate::database::Database;
use virtues_registry::ontologies::registered_ontologies;
use crate::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;

/// Pipeline status overview showing all stages
#[derive(Debug, Serialize, Deserialize)]
pub struct PipelineStatus {
    pub sync_runs: SyncRunsStatus,
    pub transform_runs: TransformRunsStatus,
    pub location_clustering: LocationClusteringStatus,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncRunsStatus {
    pub total: i64,
    pub completed: i64,
    pub failed: i64,
    pub records_synced: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TransformRunsStatus {
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
    // Sync runs status (runs with a action_id, no parent_run_id)
    let sync_row = sqlx::query(
        r#"
        SELECT
            COUNT(*) as total,
            SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END) as completed,
            SUM(CASE WHEN status = 'error' THEN 1 ELSE 0 END) as failed,
            CAST(COALESCE(SUM(records_processed), 0) AS INTEGER) as records_synced
        FROM app_applet_runs
        WHERE action_id IS NOT NULL AND parent_run_id IS NULL
        "#,
    )
    .fetch_one(db.pool())
    .await?;

    let sync_runs = SyncRunsStatus {
        total: sync_row.try_get("total")?,
        completed: sync_row.try_get("completed")?,
        failed: sync_row.try_get("failed")?,
        records_synced: sync_row.try_get("records_synced")?,
    };

    // Transform runs status (runs with parent_run_id set)
    let transform_row = sqlx::query(
        r#"
        SELECT
            COUNT(*) as total,
            SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END) as completed,
            CAST(COALESCE(SUM(records_processed), 0) AS INTEGER) as records_processed
        FROM app_applet_runs
        WHERE parent_run_id IS NOT NULL
        "#,
    )
    .fetch_one(db.pool())
    .await?;

    // Count unique transform stages
    let ontology_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT transform_stage) FROM app_applet_runs WHERE parent_run_id IS NOT NULL AND status = 'success'"
    )
    .fetch_one(db.pool())
    .await
    .unwrap_or(0);

    let transform_runs = TransformRunsStatus {
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
        sync_runs,
        transform_runs,
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
    let count_parts: Vec<String> = registered_ontologies()
        .iter()
        .map(|o| format!("(SELECT COUNT(*) FROM {})", o.table_name))
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

//! Seed Testing API
//!
//! Provides endpoints to inspect the results of the Monday in Rome seed data,
//! allowing validation of the full pipeline: Archive → Transform → Clustering → Timeline Chunks

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
    pub timeline_chunks: TimelineChunkStatus,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct TimelineChunkStatus {
    pub chunks_created: i64,
    pub location_chunks: i64,
    pub transit_chunks: i64,
    pub missing_data_chunks: i64,
    pub has_data: bool,
}

/// Summary of timeline chunks, grouped by type
#[derive(Debug, Serialize, Deserialize)]
pub struct ChunksSummary {
    pub date_range: DateRange,
    pub by_type: Vec<ChunkTypeStats>,
    pub total_chunks: i64,
    pub total_duration_minutes: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DateRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChunkTypeStats {
    pub chunk_type: String,
    pub count: i64,
    pub total_duration_minutes: i64,
    pub samples: Vec<ChunkSample>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChunkSample {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub place_name: Option<String>,
    pub duration_minutes: i32,
}

/// Data quality metrics for seed data
#[derive(Debug, Serialize, Deserialize)]
pub struct DataQualityMetrics {
    pub total_records: i64,
    pub chunk_coverage_percent: f64,
    pub time_coverage: TimeCoverage,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TimeCoverage {
    pub total_hours: f64,
    pub covered_hours: f64,
    pub coverage_percent: f64,
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
        FROM data.jobs
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
        FROM data.jobs
        WHERE job_type = 'transform'
        "#,
    )
    .fetch_one(db.pool())
    .await?;

    // Count unique ontology tables populated (rough estimate from jobs metadata)
    let ontology_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT metadata->>'stream_name') FROM data.jobs WHERE job_type = 'transform' AND status = 'completed'"
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
    let location_points: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM data.location_point")
        .fetch_one(db.pool())
        .await
        .unwrap_or(0);

    let location_visits: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM data.location_visit")
        .fetch_one(db.pool())
        .await
        .unwrap_or(0);

    let location_clustering = LocationClusteringStatus {
        raw_points: location_points,
        visits_created: location_visits,
        has_data: location_visits > 0,
    };

    // Timeline chunks status
    let chunk_row = sqlx::query(
        r#"
        SELECT
            COUNT(*) as total,
            COUNT(*) FILTER (WHERE chunk_type = 'location') as location_count,
            COUNT(*) FILTER (WHERE chunk_type = 'transit') as transit_count,
            COUNT(*) FILTER (WHERE chunk_type = 'missing_data') as missing_count
        FROM data.timeline_chunk
        "#,
    )
    .fetch_one(db.pool())
    .await?;

    let timeline_chunks = TimelineChunkStatus {
        chunks_created: chunk_row.try_get("total")?,
        location_chunks: chunk_row.try_get("location_count")?,
        transit_chunks: chunk_row.try_get("transit_count")?,
        missing_data_chunks: chunk_row.try_get("missing_count")?,
        has_data: chunk_row.try_get::<i64, _>("total")? > 0,
    };

    Ok(PipelineStatus {
        archive_jobs,
        transform_jobs,
        location_clustering,
        timeline_chunks,
    })
}

/// Get summary of timeline chunks grouped by type
pub async fn get_chunks_summary(
    db: &Database,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<ChunksSummary> {
    // Get chunks grouped by type with stats
    let rows = sqlx::query(
        r#"
        SELECT
            chunk_type::text as chunk_type,
            COUNT(*) as count,
            COALESCE(SUM(duration_minutes), 0) as total_duration
        FROM data.timeline_chunk
        WHERE start_time >= $1 AND end_time <= $2
        GROUP BY chunk_type
        ORDER BY count DESC
        "#,
    )
    .bind(start)
    .bind(end)
    .fetch_all(db.pool())
    .await?;

    let mut by_type = Vec::new();
    let mut total_chunks = 0i64;
    let mut total_duration = 0i64;

    for row in rows {
        let chunk_type: String = row.try_get("chunk_type")?;
        let count: i64 = row.try_get("count")?;
        let duration: i64 = row.try_get("total_duration")?;

        total_chunks += count;
        total_duration += duration;

        // Fetch sample chunks for this type (limit 3)
        let sample_rows = sqlx::query(
            r#"
            SELECT start_time, end_time, place_name, duration_minutes
            FROM data.timeline_chunk
            WHERE chunk_type::text = $1 AND start_time >= $2 AND end_time <= $3
            ORDER BY start_time
            LIMIT 3
            "#,
        )
        .bind(&chunk_type)
        .bind(start)
        .bind(end)
        .fetch_all(db.pool())
        .await?;

        let samples = sample_rows
            .iter()
            .map(|r| ChunkSample {
                start_time: r.try_get("start_time").unwrap(),
                end_time: r.try_get("end_time").unwrap(),
                place_name: r.try_get("place_name").ok(),
                duration_minutes: r.try_get("duration_minutes").unwrap_or(0),
            })
            .collect();

        by_type.push(ChunkTypeStats {
            chunk_type,
            count,
            total_duration_minutes: duration,
            samples,
        });
    }

    Ok(ChunksSummary {
        date_range: DateRange { start, end },
        by_type,
        total_chunks,
        total_duration_minutes: total_duration,
    })
}

/// Get data quality metrics for seed data
pub async fn get_data_quality_metrics(
    db: &Database,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<DataQualityMetrics> {
    // Total records across all ontologies (dynamically discovered from registry)
    let count_parts: Vec<String> = list_ontologies()
        .iter()
        .map(|o| format!("(SELECT COUNT(*) FROM data.{})", o.table_name))
        .collect();

    let total_records: i64 = if count_parts.is_empty() {
        0
    } else {
        sqlx::query_scalar(&format!("SELECT {}", count_parts.join(" + ")))
            .fetch_one(db.pool())
            .await
            .unwrap_or(0)
    };

    // Time coverage from timeline chunks
    let duration = end.signed_duration_since(start);
    let total_hours = duration.num_seconds() as f64 / 3600.0;

    // Get covered hours from timeline chunks (excluding missing_data)
    let covered_minutes: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(SUM(duration_minutes), 0)
        FROM data.timeline_chunk
        WHERE start_time >= $1 AND end_time <= $2
          AND chunk_type != 'missing_data'
        "#,
    )
    .bind(start)
    .bind(end)
    .fetch_one(db.pool())
    .await
    .unwrap_or(0);

    let covered_hours = covered_minutes as f64 / 60.0;
    let coverage_percent = if total_hours > 0.0 {
        (covered_hours / total_hours) * 100.0
    } else {
        0.0
    };

    let time_coverage = TimeCoverage {
        total_hours,
        covered_hours,
        coverage_percent,
    };

    Ok(DataQualityMetrics {
        total_records,
        chunk_coverage_percent: coverage_percent,
        time_coverage,
    })
}

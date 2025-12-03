//! Activity Metrics API
//!
//! Provides aggregated job metrics with time window comparisons for the Activity dashboard.

use crate::database::Database;
use crate::Result;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;

/// Query parameters for activity metrics
#[derive(Debug, Deserialize)]
pub struct ActivityMetricsQuery {
    pub source_id: Option<String>, // Optional filter by source
}

/// Complete activity metrics response
#[derive(Debug, Serialize, Deserialize)]
pub struct ActivityMetrics {
    pub summary: MetricsSummary,
    pub by_job_type: Vec<JobTypeStats>,
    pub by_stream: Vec<StreamStats>,
    pub time_windows: TimeWindowMetrics,
    pub recent_errors: Vec<RecentError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MetricsSummary {
    pub total_jobs: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub cancelled: i64,
    pub active: i64,
    pub success_rate_percent: f64,
    pub total_records_processed: i64,
    pub avg_duration_seconds: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JobTypeStats {
    pub job_type: String,
    pub total: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub avg_duration_seconds: Option<f64>,
    pub total_records: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StreamStats {
    pub stream_name: String,
    pub job_count: i64,
    pub success_rate_percent: f64,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub total_records: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TimeWindowMetrics {
    pub last_24h: PeriodStats,
    pub last_7d: PeriodStats,
    pub last_30d: PeriodStats,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PeriodStats {
    pub jobs_completed: i64,
    pub jobs_failed: i64,
    pub success_rate_percent: f64,
    pub records_processed: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RecentError {
    pub job_id: String,
    pub job_type: String,
    pub stream_name: Option<String>,
    pub error_message: String,
    pub error_class: Option<String>,
    pub failed_at: DateTime<Utc>,
}

/// Get comprehensive activity metrics
pub async fn get_activity_metrics(db: &Database) -> Result<ActivityMetrics> {
    // Summary query with FILTER clauses
    let summary_row = sqlx::query(
        r#"
        SELECT
            COUNT(*) as total,
            COUNT(*) FILTER (WHERE status = 'succeeded') as succeeded,
            COUNT(*) FILTER (WHERE status = 'failed') as failed,
            COUNT(*) FILTER (WHERE status = 'cancelled') as cancelled,
            COUNT(*) FILTER (WHERE status IN ('pending', 'running')) as active,
            COALESCE(SUM(records_processed), 0)::bigint as total_records,
            AVG(EXTRACT(EPOCH FROM (completed_at - started_at)))
                FILTER (WHERE completed_at IS NOT NULL) as avg_duration
        FROM data.jobs
        "#,
    )
    .fetch_one(db.pool())
    .await?;

    let total: i64 = summary_row.try_get("total")?;
    let succeeded: i64 = summary_row.try_get("succeeded")?;
    let failed: i64 = summary_row.try_get("failed")?;

    let success_rate = if (succeeded + failed) > 0 {
        (succeeded as f64 / (succeeded + failed) as f64) * 100.0
    } else {
        0.0
    };

    let summary = MetricsSummary {
        total_jobs: total,
        succeeded,
        failed,
        cancelled: summary_row.try_get("cancelled")?,
        active: summary_row.try_get("active")?,
        success_rate_percent: success_rate,
        total_records_processed: summary_row.try_get("total_records")?,
        avg_duration_seconds: summary_row.try_get("avg_duration").ok(),
    };

    // Job type breakdown
    let job_type_rows = sqlx::query(
        r#"
        SELECT
            job_type,
            COUNT(*) as total,
            COUNT(*) FILTER (WHERE status = 'succeeded') as succeeded,
            COUNT(*) FILTER (WHERE status = 'failed') as failed,
            AVG(EXTRACT(EPOCH FROM (completed_at - started_at)))
                FILTER (WHERE completed_at IS NOT NULL) as avg_duration,
            COALESCE(SUM(records_processed), 0)::bigint as total_records
        FROM data.jobs
        GROUP BY job_type
        ORDER BY total DESC
        "#,
    )
    .fetch_all(db.pool())
    .await?;

    let by_job_type: Vec<JobTypeStats> = job_type_rows
        .iter()
        .map(|row| JobTypeStats {
            job_type: row.try_get("job_type").unwrap_or_default(),
            total: row.try_get("total").unwrap_or(0),
            succeeded: row.try_get("succeeded").unwrap_or(0),
            failed: row.try_get("failed").unwrap_or(0),
            avg_duration_seconds: row.try_get("avg_duration").ok(),
            total_records: row.try_get("total_records").unwrap_or(0),
        })
        .collect();

    // Time window metrics
    let time_windows = get_time_window_metrics(db).await?;

    // Recent errors (last 10)
    let error_rows = sqlx::query(
        r#"
        SELECT id, job_type, stream_name, error_message, error_class, completed_at
        FROM data.jobs
        WHERE status = 'failed' AND error_message IS NOT NULL
        ORDER BY completed_at DESC NULLS LAST
        LIMIT 10
        "#,
    )
    .fetch_all(db.pool())
    .await?;

    let recent_errors: Vec<RecentError> = error_rows
        .iter()
        .map(|row| RecentError {
            job_id: row
                .try_get::<uuid::Uuid, _>("id")
                .map(|u| u.to_string())
                .unwrap_or_default(),
            job_type: row.try_get("job_type").unwrap_or_default(),
            stream_name: row.try_get("stream_name").ok(),
            error_message: row.try_get("error_message").unwrap_or_default(),
            error_class: row.try_get("error_class").ok(),
            failed_at: row
                .try_get("completed_at")
                .unwrap_or_else(|_| Utc::now()),
        })
        .collect();

    Ok(ActivityMetrics {
        summary,
        by_job_type,
        by_stream: vec![], // Can add stream breakdown if needed
        time_windows,
        recent_errors,
    })
}

async fn get_time_window_metrics(db: &Database) -> Result<TimeWindowMetrics> {
    let now = Utc::now();

    Ok(TimeWindowMetrics {
        last_24h: get_period_stats(db, now - Duration::hours(24)).await?,
        last_7d: get_period_stats(db, now - Duration::days(7)).await?,
        last_30d: get_period_stats(db, now - Duration::days(30)).await?,
    })
}

async fn get_period_stats(db: &Database, since: DateTime<Utc>) -> Result<PeriodStats> {
    let row = sqlx::query(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE status = 'succeeded') as completed,
            COUNT(*) FILTER (WHERE status = 'failed') as failed,
            COALESCE(SUM(records_processed), 0)::bigint as records
        FROM data.jobs
        WHERE created_at >= $1
        "#,
    )
    .bind(since)
    .fetch_one(db.pool())
    .await?;

    let completed: i64 = row.try_get("completed")?;
    let failed: i64 = row.try_get("failed")?;
    let success_rate = if (completed + failed) > 0 {
        (completed as f64 / (completed + failed) as f64) * 100.0
    } else {
        0.0
    };

    Ok(PeriodStats {
        jobs_completed: completed,
        jobs_failed: failed,
        success_rate_percent: success_rate,
        records_processed: row.try_get("records")?,
    })
}

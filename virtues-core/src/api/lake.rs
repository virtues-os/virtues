//! Lake API — summary + stream listing for the raw archive.
//!
//! Reads `lake_objects`, the physical inventory of everything the box has on
//! disk: raw stream archives, media blobs, drive files. Until that table existed
//! these two endpoints returned hardcoded zeros, so `DeveloperLakeView` rendered
//! an empty page over a lake that was, by then, several hundred megabytes.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::Result;
use crate::types::Timestamp;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LakeSummary {
    pub total_bytes: i64,
    pub compressed_bytes: i64,
    pub compression_ratio: f64,
    pub encrypted: bool,
    pub stream_count: i64,
    pub object_count: i64,
    pub record_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LakeStream {
    pub source_id: String,
    pub source_name: String,
    pub source_type: String,
    pub stream_name: String,
    pub size_bytes: i64,
    pub record_count: i64,
    pub object_count: i64,
    pub earliest_at: Option<Timestamp>,
    pub latest_at: Option<Timestamp>,
}

pub async fn get_lake_summary(pool: &PgPool) -> Result<LakeSummary> {
    let row: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT coalesce(sum(size_bytes), 0)::bigint,
                count(*)::bigint,
                coalesce(sum(record_count), 0)::bigint,
                count(DISTINCT (provider, stream_name))::bigint
         FROM lake_objects",
    )
    .fetch_one(pool)
    .await?;

    // Bytes on disk are what they are. `compressed_bytes` exists because the
    // frontend shows a compression figure; today nothing is compressed
    // (content_encoding = 'none'), so report the honest 1.0 rather than invent a
    // ratio. When zstd lands, this becomes a real sum over encoded sizes.
    let total_bytes = row.0;

    Ok(LakeSummary {
        total_bytes,
        compressed_bytes: total_bytes,
        compression_ratio: 1.0,
        // The box's disk is not encrypted at rest by us; saying "true" here was a
        // decoration on a stub. Don't claim a property we don't provide.
        encrypted: false,
        stream_count: row.3,
        object_count: row.1,
        record_count: row.2,
    })
}

pub async fn list_lake_streams(pool: &PgPool) -> Result<Vec<LakeStream>> {
    let rows: Vec<(String, String, String, i64, i64, i64, Option<Timestamp>, Option<Timestamp>)> =
        sqlx::query_as(
            "SELECT provider,
                    stream_name,
                    kind,
                    coalesce(sum(size_bytes), 0)::bigint,
                    coalesce(sum(record_count), 0)::bigint,
                    count(*)::bigint,
                    min(coalesce(min_timestamp, created_at)),
                    max(coalesce(max_timestamp, created_at))
             FROM lake_objects
             GROUP BY provider, stream_name, kind
             ORDER BY sum(size_bytes) DESC",
        )
        .fetch_all(pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|r| LakeStream {
            source_id: r.0.clone(),
            source_name: r.0,
            // `kind` (raw_stream | media | drive) is what distinguishes an archive
            // of records from the blobs they point at — the useful axis here.
            source_type: r.2,
            stream_name: r.1,
            size_bytes: r.3,
            record_count: r.4,
            object_count: r.5,
            earliest_at: r.6,
            latest_at: r.7,
        })
        .collect())
}

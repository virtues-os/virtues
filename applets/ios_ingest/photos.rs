//! iOS photo library → `data_content_photo`.
//!
//! The phone sends one record per photo or video: what PhotoKit says about it
//! (`local_id` is PhotoKit's local identifier; `id` is that plus a version,
//! because the phone's outbox keeps one row per `id`), and for a favorite a
//! picture about
//! 1024 pixels wide as `image_data` (base64) with `image_mime`. The Hidden
//! album is never read on the phone, so nothing hidden arrives here.
//!
//! A record with `deleted: true` is a photo removed from the phone: its row
//! keeps what it had and gets `deleted_at_source`. A photo sent again updates
//! its row. A picture is kept once it exists: a later record without one (the
//! photo was unfavorited) leaves the Drive copy and `asset_id` in place, so
//! the copy goes only when the owner deletes it.

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::PgPool;
use virtues::api::DriveConfig;
use virtues::bookmark_media::{store_image_bytes_at, PHOTOS_DIR};
use virtues::storage::Storage;
use virtues_helpers::ios::{row_id, IOS_PROVIDER, PHOTOS_STREAM_TABLE};

use crate::bookmark::{take_shared_image, SharedImage};

const KINDS: [&str; 4] = ["photo", "live_photo", "screenshot", "video"];

/// Keep each favorite's picture in Drive and point its record at it.
///
/// Runs BEFORE the records are archived, as the bookmark arm does, so the lake
/// holds the record and never a second base64 copy of the picture. A picture
/// that can never be kept (it will not decode, is not an image, is over the
/// cap) is dropped and the photo's row is still written; anything else fails
/// the batch so the phone sends it again.
pub async fn externalize_pictures(db: &PgPool, storage: &Storage, records: &[Value]) -> Result<Vec<Value>> {
    let config = DriveConfig::new(std::sync::Arc::new(storage.clone()));
    let mut out = Vec::with_capacity(records.len());
    for record in records {
        let mut rec = record.clone();
        match take_shared_image(&mut rec) {
            SharedImage::Absent => {}
            SharedImage::Unusable(why) => tracing::warn!(why, "photo picture dropped; keeping the photo without it"),
            SharedImage::Keep { bytes, declared_mime } => {
                match store_image_bytes_at(db, &config, PHOTOS_DIR, &bytes, &declared_mime).await {
                    Ok(stored) => rec["asset_id"] = json!(stored.file_id),
                    Err(virtues::error::Error::InvalidInput(reason)) => {
                        tracing::warn!(reason, "photo picture refused; keeping the photo without it")
                    }
                    Err(e) => return Err(anyhow!("could not store a photo's picture: {e}")),
                }
            }
        }
        out.push(rec);
    }
    Ok(out)
}

#[derive(Debug, PartialEq)]
struct PhotoRow {
    id: String,
    stream_id: String,
    occurred_at: DateTime<Utc>,
    kind: String,
    is_favorite: bool,
    latitude: Option<f64>,
    longitude: Option<f64>,
    width_px: Option<i32>,
    height_px: Option<i32>,
    duration_seconds: Option<f64>,
    asset_id: Option<String>,
    metadata: Value,
}

#[derive(Debug, PartialEq)]
enum Shaped {
    Row(Box<PhotoRow>),
    Deleted(String),
    Skip(&'static str),
}

fn stream_id(local_id: &str) -> String {
    format!("ios:photo:{local_id}")
}

fn shape(rec: &Value) -> Shaped {
    let Some(local_id) = ["local_id", "id"]
        .iter()
        .find_map(|k| rec.get(*k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()))
    else {
        return Shaped::Skip("no id");
    };
    let stream_id = stream_id(local_id);
    if rec.get("deleted").and_then(Value::as_bool) == Some(true) {
        return Shaped::Deleted(stream_id);
    }
    let Some(occurred_at) = rec
        .get("created_at")
        .and_then(Value::as_str)
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc))
    else {
        return Shaped::Skip("no creation date");
    };
    let kind = rec.get("kind").and_then(Value::as_str).unwrap_or("");
    if !KINDS.contains(&kind) {
        // The CHECK would reject the whole batch, and the phone would send it
        // again forever.
        return Shaped::Skip("unknown kind");
    }
    let f = |k: &str| rec.get(k).and_then(Value::as_f64);
    // PhotoKit reports a photo with no place as 0,0 on some imports.
    let (latitude, longitude) = match (f("latitude"), f("longitude")) {
        (Some(la), Some(lo)) if (-90.0..=90.0).contains(&la) && (-180.0..=180.0).contains(&lo) && (la, lo) != (0.0, 0.0) => {
            (Some(la), Some(lo))
        }
        _ => (None, None),
    };
    let px = |k: &str| rec.get(k).and_then(Value::as_i64).and_then(|v| i32::try_from(v).ok()).filter(|v| *v > 0);
    let mut metadata = json!({});
    for key in ["modified_at", "burst_id", "subtypes", "source_type"] {
        if let Some(v) = rec.get(key).filter(|v| !v.is_null()) {
            metadata[key] = v.clone();
        }
    }
    Shaped::Row(Box::new(PhotoRow {
        id: row_id(PHOTOS_STREAM_TABLE, &stream_id),
        stream_id,
        occurred_at,
        kind: kind.to_string(),
        is_favorite: rec.get("is_favorite").and_then(Value::as_bool).unwrap_or(false),
        latitude,
        longitude,
        width_px: px("width_px"),
        height_px: px("height_px"),
        duration_seconds: f("duration_seconds").filter(|d| *d > 0.0),
        asset_id: rec.get("asset_id").and_then(Value::as_str).map(String::from),
        metadata,
    }))
}

/// What one batch did.
#[derive(Debug, Default, PartialEq)]
pub struct Written {
    pub written: u64,
    pub deleted: u64,
    pub skipped: usize,
}

/// Write a batch of photo records.
pub async fn write_photos(db: &PgPool, records: &[Value]) -> Result<Written> {
    let mut rows = Vec::new();
    let mut gone = Vec::new();
    let mut out = Written::default();
    for rec in records {
        match shape(rec) {
            Shaped::Row(r) => rows.push(*r),
            Shaped::Deleted(id) => gone.push(id),
            Shaped::Skip(why) => {
                tracing::warn!(why, "photo record skipped");
                out.skipped += 1;
            }
        }
    }

    // One statement per column array, so a first pass of tens of thousands of
    // photos is a handful of round trips rather than one per photo.
    for chunk in rows.chunks(2000) {
        let col = |f: fn(&PhotoRow) -> Option<String>| chunk.iter().map(f).collect::<Vec<_>>();
        let ids = col(|r| Some(r.id.clone()));
        let streams = col(|r| Some(r.stream_id.clone()));
        let kinds = col(|r| Some(r.kind.clone()));
        let assets = col(|r| r.asset_id.clone());
        let occurred: Vec<DateTime<Utc>> = chunk.iter().map(|r| r.occurred_at).collect();
        let favorite: Vec<bool> = chunk.iter().map(|r| r.is_favorite).collect();
        let lat: Vec<Option<f64>> = chunk.iter().map(|r| r.latitude).collect();
        let lon: Vec<Option<f64>> = chunk.iter().map(|r| r.longitude).collect();
        let width: Vec<Option<i32>> = chunk.iter().map(|r| r.width_px).collect();
        let height: Vec<Option<i32>> = chunk.iter().map(|r| r.height_px).collect();
        let duration: Vec<Option<f64>> = chunk.iter().map(|r| r.duration_seconds).collect();
        let metadata: Vec<Value> = chunk.iter().map(|r| r.metadata.clone()).collect();
        let done = sqlx::query(
            "INSERT INTO data_content_photo AS p \
                 (id, source_stream_id, kind, asset_id, occurred_at, is_favorite, latitude, longitude, \
                  width_px, height_px, duration_seconds, metadata, source_table, source_provider) \
             SELECT *, $13, $14 FROM UNNEST($1::text[], $2::text[], $3::text[], $4::text[], $5::timestamptz[], \
                  $6::bool[], $7::float8[], $8::float8[], $9::int4[], $10::int4[], $11::float8[], $12::jsonb[]) \
             ON CONFLICT (source_stream_id) DO UPDATE SET \
                 kind = EXCLUDED.kind, \
                 asset_id = COALESCE(EXCLUDED.asset_id, p.asset_id), \
                 occurred_at = EXCLUDED.occurred_at, \
                 is_favorite = EXCLUDED.is_favorite, \
                 latitude = EXCLUDED.latitude, \
                 longitude = EXCLUDED.longitude, \
                 width_px = EXCLUDED.width_px, \
                 height_px = EXCLUDED.height_px, \
                 duration_seconds = EXCLUDED.duration_seconds, \
                 metadata = EXCLUDED.metadata, \
                 deleted_at_source = NULL, \
                 updated_at = now() \
             WHERE (p.kind, p.asset_id, p.occurred_at, p.is_favorite, p.latitude, p.longitude, \
                    p.width_px, p.height_px, p.duration_seconds, p.metadata, p.deleted_at_source) \
                   IS DISTINCT FROM \
                   (EXCLUDED.kind, COALESCE(EXCLUDED.asset_id, p.asset_id), EXCLUDED.occurred_at, \
                    EXCLUDED.is_favorite, EXCLUDED.latitude, EXCLUDED.longitude, EXCLUDED.width_px, \
                    EXCLUDED.height_px, EXCLUDED.duration_seconds, EXCLUDED.metadata, NULL::timestamptz)",
        )
        .bind(&ids)
        .bind(&streams)
        .bind(&kinds)
        .bind(&assets)
        .bind(&occurred)
        .bind(&favorite)
        .bind(&lat)
        .bind(&lon)
        .bind(&width)
        .bind(&height)
        .bind(&duration)
        .bind(&metadata)
        .bind(PHOTOS_STREAM_TABLE)
        .bind(IOS_PROVIDER)
        .execute(db)
        .await?;
        out.written += done.rows_affected();
    }

    if !gone.is_empty() {
        out.deleted = sqlx::query(
            "UPDATE data_content_photo SET deleted_at_source = now(), updated_at = now() \
             WHERE source_stream_id = ANY($1) AND deleted_at_source IS NULL",
        )
        .bind(&gone)
        .execute(db)
        .await?
        .rows_affected();
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo(id: &str) -> Value {
        json!({
            "id": id,
            "created_at": "2031-05-04T09:30:00Z",
            "kind": "photo",
            "is_favorite": false,
            "latitude": 30.2672,
            "longitude": -97.7431,
            "width_px": 4032,
            "height_px": 3024,
        })
    }

    #[test]
    fn a_photo_becomes_a_row_keyed_by_its_local_id() {
        let Shaped::Row(r) = shape(&photo("A1B2/L0/001")) else { panic!("not a row") };
        assert_eq!(r.stream_id, "ios:photo:A1B2/L0/001");
        assert_eq!(r.id, row_id(PHOTOS_STREAM_TABLE, "ios:photo:A1B2/L0/001"));
        assert_eq!((r.latitude, r.longitude), (Some(30.2672), Some(-97.7431)));
        assert_eq!((r.width_px, r.height_px), (Some(4032), Some(3024)));
        assert_eq!(r.duration_seconds, None);
        assert!(!r.is_favorite);
    }

    #[test]
    fn the_photo_is_keyed_on_local_id_when_the_record_is_versioned() {
        let mut p = photo("A1B2/L0/001#2031-05-04T09:30:00Z|f");
        p["local_id"] = json!("A1B2/L0/001");
        let Shaped::Row(r) = shape(&p) else { panic!("not a row") };
        assert_eq!(r.stream_id, "ios:photo:A1B2/L0/001");
        let gone = json!({ "id": "A1B2/L0/001#deleted", "local_id": "A1B2/L0/001", "deleted": true });
        assert_eq!(shape(&gone), Shaped::Deleted("ios:photo:A1B2/L0/001".into()));
    }

    #[test]
    fn records_that_cannot_be_rows_are_skipped_or_deleted() {
        assert_eq!(shape(&json!({ "id": "x", "deleted": true })), Shaped::Deleted("ios:photo:x".into()));
        assert_eq!(shape(&json!({ "created_at": "2031-05-04T09:30:00Z", "kind": "photo" })), Shaped::Skip("no id"));
        let mut odd = photo("y");
        odd["kind"] = json!("hologram");
        assert_eq!(shape(&odd), Shaped::Skip("unknown kind"));
        let mut undated = photo("z");
        undated["created_at"] = json!("yesterday");
        assert_eq!(shape(&undated), Shaped::Skip("no creation date"));
    }

    #[test]
    fn a_place_of_zero_zero_is_no_place() {
        let mut p = photo("p");
        p["latitude"] = json!(0.0);
        p["longitude"] = json!(0.0);
        let Shaped::Row(r) = shape(&p) else { panic!("not a row") };
        assert_eq!((r.latitude, r.longitude), (None, None));
    }

    #[sqlx::test(migrations = "../virtues-core/migrations")]
    async fn a_photo_sent_again_updates_and_keeps_its_picture(pool: PgPool) {
        let mut fav = photo("f1");
        fav["is_favorite"] = json!(true);
        fav["asset_id"] = json!("file_123");
        let w = write_photos(&pool, &[fav.clone(), photo("p2")]).await.unwrap();
        assert_eq!(w, Written { written: 2, deleted: 0, skipped: 0 });

        // The same batch again changes nothing.
        assert_eq!(write_photos(&pool, &[fav, photo("p2")]).await.unwrap().written, 0);

        // Unfavorited: the flag changes, the picture stays.
        let unfav = photo("f1");
        assert_eq!(write_photos(&pool, &[unfav]).await.unwrap().written, 1);
        let (is_favorite, asset): (bool, Option<String>) =
            sqlx::query_as("SELECT is_favorite, asset_id FROM data_content_photo WHERE source_stream_id = 'ios:photo:f1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(!is_favorite);
        assert_eq!(asset.as_deref(), Some("file_123"));

        // Deleted on the phone: the row stays, marked.
        let w = write_photos(&pool, &[json!({ "id": "p2", "deleted": true })]).await.unwrap();
        assert_eq!(w.deleted, 1);
        let gone: Option<DateTime<Utc>> =
            sqlx::query_scalar("SELECT deleted_at_source FROM data_content_photo WHERE source_stream_id = 'ios:photo:p2'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(gone.is_some());
    }
}

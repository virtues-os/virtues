//! iOS photo favorites → `data_content_photo`.
//!
//! The phone sends one record per favorite photo or video: what PhotoKit says
//! about it (`local_id` is PhotoKit's local identifier; `id` is that plus a
//! suffix, because the phone's outbox keeps one row per `id`), and a picture
//! at most 2048 pixels on its longest side as `image_data` (base64) with
//! `image_mime`. Nothing is sent for a photo that is not a favorite. The
//! picture becomes a file in Drive and the row names it; `photo_enrichment`
//! then describes it.
//!
//! A record with `deleted: true` is a photo that stopped being a favorite, or
//! was hidden or deleted on the phone. Its row keeps the owner's note and gets
//! `deleted_at_source`; its picture, its place and its description go. The
//! phone sends a photo's records in order, so the last one is what the phone
//! says now.

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::PgPool;
use virtues::api::DriveConfig;
use virtues::bookmark_media::{store_image_bytes_at, PHOTOS_DIR};
use virtues::storage::{raw_copies, Storage};
use virtues_helpers::dedup::dedup_refs_keep_last;
use virtues_helpers::ios::{row_id, IOS_PROVIDER, PHOTOS_STREAM_TABLE};

use crate::bookmark::{take_shared_image, SharedImage};

const KINDS: [&str; 4] = ["photo", "live_photo", "screenshot", "video"];

/// Keep each picture in Drive and point its record at it. A photo the owner
/// deleted from this server is dropped first, so its picture is not stored
/// again.
///
/// A picture that can never be kept (it will not decode, is not an image, is
/// over the cap) is dropped and the photo's row is still written; anything
/// else fails the batch so the phone sends it again.
pub async fn externalize_pictures(db: &PgPool, storage: &Storage, records: &[Value]) -> Result<Vec<Value>> {
    let records = raw_copies::without_tombstoned(db, IOS_PROVIDER, "photos", records).await?;
    let config = DriveConfig::new(std::sync::Arc::new(storage.clone()));
    let mut out = Vec::with_capacity(records.len());
    for record in records.iter() {
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
    Removed(String),
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
        return Shaped::Removed(stream_id);
    }
    if rec.get("is_favorite").and_then(Value::as_bool) != Some(true) {
        return Shaped::Skip("not a favorite");
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
    if let Some(v) = rec.get("modified_at").filter(|v| !v.is_null()) {
        metadata["modified_at"] = v.clone();
    }
    Shaped::Row(Box::new(PhotoRow {
        id: row_id(PHOTOS_STREAM_TABLE, &stream_id),
        stream_id,
        occurred_at,
        kind: kind.to_string(),
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
    pub removed: u64,
    pub skipped: usize,
}

/// Write a batch of photo records.
///
/// A picture a photo no longer shows (replaced or removed) is let go of here,
/// and the photo job deletes the file (`api::photos::release_unused`).
///
/// One batch can hold several records for one photo. Postgres refuses an
/// `ON CONFLICT DO UPDATE` that meets one row twice, which would fail the
/// batch on every retry, so only the last record per photo is kept, whether
/// that is a favorite or a removal.
pub async fn write_photos(db: &PgPool, records: &[Value]) -> Result<Written> {
    let mut shaped = Vec::with_capacity(records.len());
    let mut out = Written::default();
    for rec in records {
        match shape(rec) {
            Shaped::Skip(why) => {
                tracing::warn!(why, "photo record skipped");
                out.skipped += 1;
            }
            s => shaped.push(s),
        }
    }
    let mut rows: Vec<&PhotoRow> = Vec::new();
    let mut gone: Vec<&str> = Vec::new();
    for s in dedup_refs_keep_last(&shaped, |s| match s {
        Shaped::Row(r) => r.stream_id.as_str(),
        Shaped::Removed(id) => id.as_str(),
        Shaped::Skip(_) => unreachable!("skips are counted above"),
    }) {
        match s {
            Shaped::Row(r) => rows.push(r),
            Shaped::Removed(id) => gone.push(id),
            Shaped::Skip(_) => {}
        }
    }

    // One statement per column array, so a batch is a handful of round trips
    // rather than one per photo.
    for chunk in rows.chunks(2000) {
        let col = |f: fn(&PhotoRow) -> Option<String>| chunk.iter().map(|r| f(r)).collect::<Vec<_>>();
        let ids = col(|r| Some(r.id.clone()));
        let streams = col(|r| Some(r.stream_id.clone()));
        let kinds = col(|r| Some(r.kind.clone()));
        let assets = col(|r| r.asset_id.clone());
        let occurred: Vec<DateTime<Utc>> = chunk.iter().map(|r| r.occurred_at).collect();
        let lat: Vec<Option<f64>> = chunk.iter().map(|r| r.latitude).collect();
        let lon: Vec<Option<f64>> = chunk.iter().map(|r| r.longitude).collect();
        let width: Vec<Option<i32>> = chunk.iter().map(|r| r.width_px).collect();
        let height: Vec<Option<i32>> = chunk.iter().map(|r| r.height_px).collect();
        let duration: Vec<Option<f64>> = chunk.iter().map(|r| r.duration_seconds).collect();
        let metadata: Vec<Value> = chunk.iter().map(|r| r.metadata.clone()).collect();
        let done = sqlx::query(
            "INSERT INTO data_content_photo AS p \
                 (id, source_stream_id, kind, asset_id, occurred_at, latitude, longitude, \
                  width_px, height_px, duration_seconds, metadata, is_favorite, source_table, source_provider) \
             SELECT *, true, $12, $13 FROM UNNEST($1::text[], $2::text[], $3::text[], $4::text[], $5::timestamptz[], \
                  $6::float8[], $7::float8[], $8::int4[], $9::int4[], $10::float8[], $11::jsonb[]) \
             ON CONFLICT (source_stream_id) DO UPDATE SET \
                 kind = EXCLUDED.kind, \
                 asset_id = COALESCE(EXCLUDED.asset_id, p.asset_id), \
                 occurred_at = EXCLUDED.occurred_at, \
                 is_favorite = true, \
                 latitude = EXCLUDED.latitude, \
                 longitude = EXCLUDED.longitude, \
                 width_px = EXCLUDED.width_px, \
                 height_px = EXCLUDED.height_px, \
                 duration_seconds = EXCLUDED.duration_seconds, \
                 metadata = EXCLUDED.metadata, \
                 deleted_at_source = NULL, \
                 enrichment_status = CASE WHEN p.asset_id IS NOT DISTINCT FROM COALESCE(EXCLUDED.asset_id, p.asset_id) \
                                          THEN p.enrichment_status ELSE 'pending' END, \
                 enrichment_attempts = CASE WHEN p.asset_id IS NOT DISTINCT FROM COALESCE(EXCLUDED.asset_id, p.asset_id) \
                                            THEN p.enrichment_attempts ELSE 0 END, \
                 enrichment_last_attempt = CASE WHEN p.asset_id IS NOT DISTINCT FROM COALESCE(EXCLUDED.asset_id, p.asset_id) \
                                                THEN p.enrichment_last_attempt END, \
                 updated_at = now() \
             WHERE (p.kind, p.asset_id, p.occurred_at, p.is_favorite, p.latitude, p.longitude, \
                    p.width_px, p.height_px, p.duration_seconds, p.metadata, p.deleted_at_source) \
                   IS DISTINCT FROM \
                   (EXCLUDED.kind, COALESCE(EXCLUDED.asset_id, p.asset_id), EXCLUDED.occurred_at, \
                    true, EXCLUDED.latitude, EXCLUDED.longitude, EXCLUDED.width_px, \
                    EXCLUDED.height_px, EXCLUDED.duration_seconds, EXCLUDED.metadata, NULL::timestamptz)",
        )
        .bind(&ids)
        .bind(&streams)
        .bind(&kinds)
        .bind(&assets)
        .bind(&occurred)
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
        out.removed = sqlx::query(
            "UPDATE data_content_photo SET \
                 deleted_at_source = now(), is_favorite = false, asset_id = NULL, \
                 latitude = NULL, longitude = NULL, extraction = NULL, extraction_text = NULL, \
                 enrichment_status = 'pending', enrichment_attempts = 0, enrichment_last_attempt = NULL, \
                 enrichment_model = NULL, enriched_at = NULL, updated_at = now() \
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
            "is_favorite": true,
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
    }

    #[test]
    fn the_photo_is_keyed_on_local_id_when_the_record_is_versioned() {
        let mut p = photo("A1B2/L0/001#2031-05-04T09:30:00Z|f");
        p["local_id"] = json!("A1B2/L0/001");
        let Shaped::Row(r) = shape(&p) else { panic!("not a row") };
        assert_eq!(r.stream_id, "ios:photo:A1B2/L0/001");
        let gone = json!({ "id": "A1B2/L0/001#deleted", "local_id": "A1B2/L0/001", "deleted": true });
        assert_eq!(shape(&gone), Shaped::Removed("ios:photo:A1B2/L0/001".into()));
    }

    #[test]
    fn records_that_cannot_be_rows_are_skipped_or_deleted() {
        assert_eq!(shape(&json!({ "id": "x", "deleted": true })), Shaped::Removed("ios:photo:x".into()));
        assert_eq!(shape(&json!({ "created_at": "2031-05-04T09:30:00Z", "kind": "photo", "is_favorite": true })), Shaped::Skip("no id"));
        let mut odd = photo("y");
        odd["kind"] = json!("hologram");
        assert_eq!(shape(&odd), Shaped::Skip("unknown kind"));
        let mut plain = photo("w");
        plain["is_favorite"] = json!(false);
        assert_eq!(shape(&plain), Shaped::Skip("not a favorite"));
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

    async fn row(pool: &PgPool, id: &str) -> (bool, Option<String>, Option<f64>, Option<String>, String, bool) {
        sqlx::query_as(
            "SELECT is_favorite, asset_id, latitude, extraction_text, enrichment_status, deleted_at_source IS NOT NULL
               FROM data_content_photo WHERE source_stream_id = $1",
        )
        .bind(stream_id(id))
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[sqlx::test(migrations = "../virtues-core/migrations")]
    async fn an_edited_photo_is_described_again_and_a_removed_one_keeps_only_its_note(pool: PgPool) {
        let mut first = photo("f1");
        first["asset_id"] = json!("file_1");
        let w = write_photos(&pool, &[first.clone(), photo("p2")]).await.unwrap();
        assert_eq!(w, Written { written: 2, removed: 0, skipped: 0 });
        sqlx::query(
            "UPDATE data_content_photo SET enrichment_status = 'done', enrichment_attempts = 1,
                    extraction_text = 'a red boat', note = 'the first swim'",
        )
        .execute(&pool)
        .await
        .unwrap();

        // The same batch again changes nothing, the description included.
        assert_eq!(write_photos(&pool, &[first.clone(), photo("p2")]).await.unwrap().written, 0);
        assert_eq!(row(&pool, "f1").await.4, "done");

        // A new picture is described again.
        let mut edited = first.clone();
        edited["asset_id"] = json!("file_2");
        assert_eq!(write_photos(&pool, &[edited]).await.unwrap().written, 1);
        let r = row(&pool, "f1").await;
        assert_eq!((r.1.as_deref(), r.4.as_str()), (Some("file_2"), "pending"));

        // No longer a favorite: the note stays, and nothing else about it.
        let w = write_photos(&pool, &[json!({ "local_id": "f1", "id": "f1#x", "deleted": true })]).await.unwrap();
        assert_eq!(w.removed, 1);
        assert_eq!(row(&pool, "f1").await, (false, None, None, None, "pending".into(), true));
        let note: Option<String> = sqlx::query_scalar("SELECT note FROM data_content_photo WHERE source_stream_id = $1")
            .bind(stream_id("f1"))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(note.as_deref(), Some("the first swim"));

        // A favorite again.
        write_photos(&pool, &[first]).await.unwrap();
        let r = row(&pool, "f1").await;
        assert_eq!((r.0, r.1.as_deref(), r.5), (true, Some("file_1"), false));
    }

    #[sqlx::test(migrations = "../virtues-core/migrations")]
    async fn the_last_record_for_a_photo_decides_it(pool: PgPool) {
        let fav = |n: &str| {
            let mut p = photo(&format!("A1B2/L0/001#{n}"));
            p["local_id"] = json!("A1B2/L0/001");
            p["asset_id"] = json!(format!("file_{n}"));
            p
        };
        let gone = json!({ "id": "A1B2/L0/001#x", "local_id": "A1B2/L0/001", "deleted": true });

        let w = write_photos(&pool, &[fav("1"), fav("2")]).await.unwrap();
        assert_eq!(w, Written { written: 1, removed: 0, skipped: 0 });
        assert_eq!(row(&pool, "A1B2/L0/001").await.1.as_deref(), Some("file_2"));

        let w = write_photos(&pool, &[fav("3"), gone.clone()]).await.unwrap();
        assert_eq!((w.written, w.removed), (0, 1));
        assert!(row(&pool, "A1B2/L0/001").await.5);

        let w = write_photos(&pool, &[gone, fav("4")]).await.unwrap();
        assert_eq!((w.written, w.removed), (1, 0));
        assert!(!row(&pool, "A1B2/L0/001").await.5);
    }

    #[sqlx::test(migrations = "../virtues-core/migrations")]
    async fn a_photo_deleted_on_the_server_does_not_store_its_picture_again(pool: PgPool) {
        sqlx::query(
            "INSERT INTO app_tombstones (source_table, source_stream_id, source_id) VALUES ('data_content_photo', $1, 'x')",
        )
        .bind(stream_id("A1B2/L0/001"))
        .execute(&pool)
        .await
        .unwrap();
        let dir = std::env::temp_dir().join(format!("photo-tombstone-{}", std::process::id()));
        let storage = Storage::file(dir.to_string_lossy().into_owned()).unwrap();
        let mut p = photo("A1B2/L0/001#1");
        p["local_id"] = json!("A1B2/L0/001");
        p["image_mime"] = json!("image/jpeg");
        p["image_data"] = json!("/9j/4AAQ");
        let kept = externalize_pictures(&pool, &storage, &[p]).await.unwrap();
        assert!(kept.is_empty());
        let files: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_drive_files").fetch_one(&pool).await.unwrap();
        assert_eq!(files, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

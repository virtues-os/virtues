//! Photo favorites: the photos the owner favorited on their iPhone.
//!
//! The rows are `data_content_photo` (written by `ios_ingest`), the pictures
//! are files in Drive, and what is in each picture is described by
//! `photo_enrichment`. A photo that is no longer a favorite is not listed.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;

use super::bookmarks::{link_mentions, Mention, UpdateNoteRequest};
use super::DriveConfig;
use crate::bookmark_media::PHOTOS_DIR;
use crate::error::{Error, Result};

/// What every photo in the list and on its page has.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PhotoListItem {
    pub id: String,
    /// `photo`, `live_photo`, `screenshot` or `video`. A video's picture is
    /// one frame of it.
    pub kind: String,
    pub occurred_at: crate::types::Timestamp,
    pub width_px: Option<i32>,
    pub height_px: Option<i32>,
    pub duration_seconds: Option<f64>,
    pub note: Option<String>,
    /// What the picture shows, in a sentence or three.
    pub description: Option<String>,
    /// The picture's address on this server.
    pub picture_url: Option<String>,
}

const LIST_COLUMNS: &str = "id, kind, occurred_at, width_px, height_px, duration_seconds, note,
     extraction->>'description' AS description,
     CASE WHEN asset_id IS NOT NULL
          THEN '/api/drive/files/' || asset_id || '/download?disposition=inline' END AS picture_url";

#[derive(Debug, Clone, Deserialize)]
pub struct ListPhotosQuery {
    #[serde(default)]
    pub offset: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub search: Option<String>,
}

fn default_limit() -> i64 {
    50
}

#[derive(Debug, Clone, Serialize)]
pub struct PhotoPage {
    pub items: Vec<PhotoListItem>,
    /// Another page follows this one. There is no count: the page shows none.
    pub has_more: bool,
}

/// One page of photo favorites, most recently taken first.
///
/// Every word of the search must appear in what describes a photo: the
/// description, the note, the month and year it was taken. "lake june" finds
/// the lake in June.
pub async fn list_photos(db: &PgPool, q: ListPhotosQuery) -> Result<PhotoPage> {
    let limit = q.limit.clamp(1, 200);
    // `%` and `_` are LIKE wildcards; in a search they are text.
    let patterns: Vec<String> = q
        .search
        .as_deref()
        .unwrap_or_default()
        .split_whitespace()
        .map(|w| format!("%{}%", w.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")))
        .collect();
    let mut items = sqlx::query_as::<_, PhotoListItem>(&format!(
        "SELECT {LIST_COLUMNS} FROM data_content_photo
          WHERE is_favorite AND deleted_at_source IS NULL AND asset_id IS NOT NULL
            AND NOT EXISTS (
                SELECT 1 FROM unnest($1::text[]) AS word
                 WHERE (COALESCE(extraction_text, '') || ' ' || COALESCE(note, '') || ' '
                        || to_char(occurred_at, 'FMMonth YYYY')) NOT ILIKE word ESCAPE '\\')
          ORDER BY occurred_at DESC, id
          LIMIT $2 OFFSET $3"
    ))
    .bind(&patterns)
    .bind(limit + 1)
    .bind(q.offset.max(0))
    .fetch_all(db)
    .await?;
    let has_more = items.len() as i64 > limit;
    items.truncate(limit as usize);
    Ok(PhotoPage { items, has_more })
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PhotoPlace {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PhotoDetail {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub photo: PhotoListItem,
    pub deleted_at_source: Option<crate::types::Timestamp>,
    pub extraction: Option<Value>,
    pub enrichment_model: Option<String>,
    pub enriched_at: Option<crate::types::Timestamp>,
    /// `described`, `waiting`, or `not_described` when the picture could not
    /// be read.
    pub state: String,
    #[serde(skip)]
    latitude: Option<f64>,
    #[serde(skip)]
    longitude: Option<f64>,
    /// The place the owner has named where it was taken.
    #[sqlx(skip)]
    pub place: Option<PhotoPlace>,
    #[sqlx(skip)]
    pub mentions: Vec<Mention>,
}

/// One photo, for its page. A photo no longer a favorite still opens, so a
/// link to it finds the owner's note; its picture is gone.
pub async fn get_photo(db: &PgPool, id: &str) -> Result<PhotoDetail> {
    let mut detail = sqlx::query_as::<_, PhotoDetail>(&format!(
        "SELECT {LIST_COLUMNS}, deleted_at_source, extraction, enrichment_model, enriched_at,
                latitude, longitude,
                CASE WHEN enrichment_status = 'done' THEN 'described'
                     WHEN enrichment_status IN ('failed', 'skipped') THEN 'not_described'
                     ELSE 'waiting' END AS state
           FROM data_content_photo WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| Error::NotFound(format!("no photo {id}")))?;
    if let (Some(lat), Some(lon)) = (detail.latitude, detail.longitude) {
        detail.place = place_near(db, lat, lon).await?;
    }
    let names: Vec<String> = detail
        .extraction
        .as_ref()
        .and_then(|e| e.get("entities"))
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
        .unwrap_or_default();
    detail.mentions = link_mentions(db, &names).await?;
    Ok(detail)
}

/// Write the owner's note. Blank clears it. The note is part of the embed
/// text, so the indexer reads the photo again on its next pass.
pub async fn update_note(db: &PgPool, id: &str, req: UpdateNoteRequest) -> Result<PhotoDetail> {
    let note = req.note.as_deref().map(str::trim).filter(|n| !n.is_empty());
    let updated = sqlx::query("UPDATE data_content_photo SET note = $2, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(note)
        .execute(db)
        .await?;
    if updated.rows_affected() == 0 {
        return Err(Error::NotFound(format!("no photo {id}")));
    }
    get_photo(db, id).await
}

/// The named place a point falls in, nearest first. A place the owner has not
/// named is not given.
pub async fn place_near(db: &PgPool, lat: f64, lon: f64) -> Result<Option<PhotoPlace>> {
    let (min_lat, max_lat, min_lon, max_lon) = crate::geo::bounding_box(lat, lon, 2_000.0);
    let near: Vec<(String, String, f64, f64, f64)> = sqlx::query_as(
        "SELECT id, name, latitude, longitude, radius_m FROM wiki_places
          WHERE is_named AND latitude BETWEEN $1 AND $2 AND longitude BETWEEN $3 AND $4",
    )
    .bind(min_lat)
    .bind(max_lat)
    .bind(min_lon)
    .bind(max_lon)
    .fetch_all(db)
    .await?;
    Ok(near
        .into_iter()
        .map(|(id, name, la, lo, radius)| {
            (crate::geo::haversine_distance(lat, lon, la, lo), radius, PhotoPlace { id, name })
        })
        .filter(|(distance, radius, _)| distance <= radius)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, _, place)| place))
}

/// Delete pictures no photo shows any more, and say how many went. `except`
/// names photos that do not count, because they are about to be deleted.
///
/// A picture is stored under its own hash, so two photos can share one: a file
/// another row still names stays. Only files in the photo folder are ever
/// deleted here.
pub async fn release_pictures(
    db: &PgPool,
    config: &DriveConfig,
    file_ids: &[String],
    except: &[String],
) -> Result<u64> {
    if file_ids.is_empty() {
        return Ok(0);
    }
    let unused: Vec<String> = sqlx::query_scalar(
        "SELECT f.id FROM app_drive_files f
          WHERE f.id = ANY($1) AND starts_with(f.path, $2) AND NOT f.is_folder
            AND NOT EXISTS (SELECT 1 FROM data_content_photo p WHERE p.asset_id = f.id AND p.id <> ALL($3))",
    )
    .bind(file_ids)
    .bind(format!("{PHOTOS_DIR}/"))
    .bind(except)
    .fetch_all(db)
    .await?;
    for id in &unused {
        super::drive::purge_file(db, config, id).await?;
    }
    Ok(unused.len() as u64)
}

/// Delete every stored picture no photo shows: one an edit replaced, one a
/// removed photo had, one stored for a record that was then refused. Run by
/// the photo job. A file younger than an hour may belong to a batch still
/// being written, so it waits.
pub async fn release_unused(db: &PgPool, config: &DriveConfig) -> Result<u64> {
    let unused: Vec<String> = sqlx::query_scalar(
        "SELECT f.id FROM app_drive_files f
          WHERE starts_with(f.path, $1) AND NOT f.is_folder
            AND f.created_at < now() - interval '1 hour'
            AND NOT EXISTS (SELECT 1 FROM data_content_photo p WHERE p.asset_id = f.id)",
    )
    .bind(format!("{PHOTOS_DIR}/"))
    .fetch_all(db)
    .await?;
    release_pictures(db, config, &unused, &[]).await
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn photo(pool: &PgPool, id: &str, taken: &str, words: Option<&str>) {
        sqlx::query(
            "INSERT INTO data_content_photo
               (id, occurred_at, kind, is_favorite, asset_id, source_stream_id, source_table, source_provider,
                extraction, extraction_text, enrichment_status)
             VALUES ($1, $2::timestamptz, 'photo', true, 'file_' || $1, $1, 'test', 'test',
                     jsonb_build_object('description', $3::text, 'entities', '[]'::jsonb), $3,
                     CASE WHEN $3 IS NULL THEN 'pending' ELSE 'done' END)",
        )
        .bind(id)
        .bind(taken)
        .bind(words)
        .execute(pool)
        .await
        .unwrap();
    }

    fn query(search: Option<&str>, limit: i64) -> ListPhotosQuery {
        ListPhotosQuery { offset: 0, limit, search: search.map(String::from) }
    }

    async fn ids(pool: &PgPool, search: &str) -> Vec<String> {
        list_photos(pool, query(Some(search), 50)).await.unwrap().items.into_iter().map(|p| p.id).collect()
    }

    #[sqlx::test]
    async fn photos_are_found_by_what_is_in_them_and_when(pool: PgPool) {
        photo(&pool, "boat", "2031-06-14T20:00:00Z", Some("A red boat on a lake")).await;
        photo(&pool, "cake", "2031-03-02T20:00:00Z", Some("A cake with blue candles, 100% chocolate")).await;
        photo(&pool, "waiting", "2031-07-01T20:00:00Z", None).await;
        photo(&pool, "gone", "2031-08-01T20:00:00Z", Some("A red kite")).await;
        sqlx::query("UPDATE data_content_photo SET deleted_at_source = now() WHERE id = 'gone'")
            .execute(&pool)
            .await
            .unwrap();

        let all = list_photos(&pool, query(None, 2)).await.unwrap();
        assert_eq!(all.items.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["waiting", "boat"]);
        assert!(all.has_more);
        assert_eq!(ids(&pool, "lake june").await, ["boat"], "every word, in any order");
        assert_eq!(ids(&pool, "RED").await, ["boat"], "a photo no longer a favorite is not found");
        assert_eq!(ids(&pool, "100%").await, ["cake"], "a percent sign is text");
        assert!(ids(&pool, "lake march").await.is_empty());
    }

    #[sqlx::test]
    async fn a_photo_page_has_its_place_note_and_state(pool: PgPool) {
        photo(&pool, "boat", "2031-06-14T20:00:00Z", Some("A red boat on a lake")).await;
        sqlx::query("UPDATE data_content_photo SET latitude = 10.0, longitude = 20.0 WHERE id = 'boat'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO wiki_places (id, name, latitude, longitude, radius_m, is_named) VALUES
               ('place_far', 'Far Hill', 10.01, 20.0, 100, true),
               ('place_lake', 'Blue Lake', 10.0005, 20.0, 100, true),
               ('place_unnamed', 'Location 10, 20', 10.0, 20.0, 100, false)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let d = update_note(&pool, "boat", UpdateNoteRequest { note: Some("  the first swim ".into()) }).await.unwrap();
        assert_eq!(d.photo.note.as_deref(), Some("the first swim"));
        assert_eq!(d.state, "described");
        assert_eq!(d.photo.description.as_deref(), Some("A red boat on a lake"));
        assert_eq!(d.place, Some(PhotoPlace { id: "place_lake".into(), name: "Blue Lake".into() }));
        assert_eq!(ids(&pool, "first swim").await, ["boat"], "the note is searched");

        assert!(matches!(get_photo(&pool, "nothing").await, Err(Error::NotFound(_))));
    }

    /// A picture two photos share stays until neither shows it, a fresh file
    /// waits, and a file outside the photo folder is never touched.
    #[sqlx::test]
    async fn a_picture_goes_when_no_photo_shows_it(pool: PgPool) {
        let dir = std::env::temp_dir().join(format!("photo-release-{}", std::process::id()));
        let config = DriveConfig::new(std::sync::Arc::new(
            crate::storage::Storage::file(dir.to_string_lossy().into_owned()).unwrap(),
        ));
        photo(&pool, "one", "2031-06-14T20:00:00Z", None).await;
        photo(&pool, "two", "2031-06-15T20:00:00Z", None).await;
        sqlx::query("UPDATE data_content_photo SET asset_id = 'file_shared'").execute(&pool).await.unwrap();
        for (id, path, age) in [
            ("file_shared", ".photos/favorites/shared.jpg", "2 hours"),
            ("file_fresh", ".photos/favorites/fresh.jpg", "1 minute"),
            ("file_unused", ".photos/favorites/unused.jpg", "2 hours"),
            ("file_other", "holiday.jpg", "2 hours"),
        ] {
            config.storage.upload(path, b"picture".to_vec()).await.unwrap();
            sqlx::query(
                "INSERT INTO app_drive_files (id, path, filename, mime_type, size_bytes, is_folder, created_at)
                 VALUES ($1, $2, $1, 'image/jpeg', 7, false, now() - $3::interval)",
            )
            .bind(id)
            .bind(path)
            .bind(age)
            .execute(&pool)
            .await
            .unwrap();
        }

        assert_eq!(release_unused(&pool, &config).await.unwrap(), 1);
        let shared = ["file_shared".to_string()];
        let one = ["one".to_string()];
        assert_eq!(release_pictures(&pool, &config, &shared, &one).await.unwrap(), 0, "the other photo shows it");
        let both = ["one".to_string(), "two".to_string()];
        assert_eq!(release_pictures(&pool, &config, &shared, &both).await.unwrap(), 1);
        assert_eq!(release_pictures(&pool, &config, &["file_other".to_string()], &both).await.unwrap(), 0);

        let left: Vec<String> =
            sqlx::query_scalar("SELECT id FROM app_drive_files ORDER BY id").fetch_all(&pool).await.unwrap();
        assert_eq!(left, ["file_fresh", "file_other"]);
        assert!(!dir.join(".photos/favorites/shared.jpg").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

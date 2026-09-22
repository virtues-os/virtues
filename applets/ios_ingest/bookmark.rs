//! iOS share → `data_content_bookmark`.
//!
//! The phone's half of the share sheet (agents/plan/bookmarks-plan.md §5). The
//! extension writes a payload into a shared App Group container and returns
//! immediately; the app drains that container and posts here. Nothing in this
//! file assumes which door sent it — a Shortcut posting the same shape works
//! identically, which is what keeps the box side unblocked while the extension
//! waits on provisioning.
//!
//! Payload per record:
//! ```json
//! {
//!   "url": "https://instagram.com/p/xyz",   // optional if asset_id is present
//!   "asset_id": "file_abc",                 // optional; a stored image
//!   "note": "the green door",               // optional, the user's own words
//!   "title": "…", "source_app": "com.burbn.instagram",
//!   "content_hash": "9f2c…",                // optional; identity for assets
//!   "image_data": "iVBORw0KGgo…",           // optional; the picture itself, base64
//!   "image_mime": "image/png",              // optional; a hint — the bytes decide
//!   "timestamp": "2026-02-11T23:14:00Z"
//! }
//! ```
//!
//! **`image_data` is the share sheet's picture, sent inline** — the same road
//! audio takes (`microphone::externalize_blobs`). [`externalize_images`] takes
//! it out of the record BEFORE the record is archived, keeps it in Drive, and
//! leaves `asset_id` + `content_hash` in its place; from there the record is an
//! ordinary asset share, and the image pass reads the picture.
//!
//! **`url` means where the thing IS, not where it came from** — the contract
//! settled in the plan. A share carrying a source URL keeps it and names the
//! image in `metadata.asset_id`; a camera-roll screenshot with no source gets
//! the in-app viewer route, because the artifact is the thing. Provenance goes
//! to `source_platform`, never into the URL.

use anyhow::{anyhow, Result};
use base64::Engine;
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::PgPool;
use virtues::api::DriveConfig;
use virtues::bookmark_media::{sha256_hex, store_image_bytes_in};
use virtues::storage::Storage;
use virtues_helpers::bookmarks::{upsert_bookmarks, BookmarkRow};
use virtues_helpers::ios::{parse_timestamp, IOS_PROVIDER};

/// Map a sharing app's bundle id to the platform name the room shows.
///
/// Unknown bundles fall through to the bundle id itself rather than to
/// "unknown": it is still true, still greppable, and tells whoever adds the
/// next mapping exactly what string to add.
fn platform_of(source_app: Option<&str>) -> Option<String> {
    let bundle = source_app?;
    Some(
        match bundle {
            "com.burbn.instagram" => "instagram",
            "com.atebits.Tweetie2" | "com.twitter.twitter" => "x",
            "com.apple.mobilesafari" => "safari",
            "com.google.chrome.ios" => "chrome",
            "com.apple.MobileSMS" => "messages",
            "com.toyopagroup.picaboo" => "snapchat",
            "com.zhiliaoapp.musically" => "tiktok",
            "com.pinterest" => "pinterest",
            other => other,
        }
        .to_string(),
    )
}

/// What a share's `image_data` turns into, decided without touching storage.
#[derive(Debug)]
enum SharedImage {
    /// No picture on this share.
    Absent,
    /// A picture to keep, with the type its sender claimed (a hint only).
    Keep { bytes: Vec<u8>, declared_mime: String },
    /// A picture that can never be kept — it will not decode on any retry. The
    /// share survives without it.
    Unusable(String),
}

/// Take the picture OUT of a share record, whatever becomes of it.
///
/// Out unconditionally: the record is archived to the lake next, and the base64
/// must not ride along — the Drive copy is the picture's one home, as the lake
/// object is audio's.
fn take_shared_image(rec: &mut Value) -> SharedImage {
    let Some(raw) = rec.as_object_mut().and_then(|o| o.remove("image_data")) else {
        return SharedImage::Absent;
    };
    let Some(b64) = raw.as_str() else {
        return SharedImage::Unusable("image_data was not a string".into());
    };
    let declared_mime = rec
        .get("image_mime")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    match base64::engine::general_purpose::STANDARD.decode(b64.trim()) {
        Ok(bytes) if !bytes.is_empty() => SharedImage::Keep {
            bytes,
            declared_mime,
        },
        Ok(_) => SharedImage::Unusable("image_data was empty".into()),
        Err(e) => SharedImage::Unusable(format!("image_data did not decode: {e}")),
    }
}

/// Keep each shared picture in Drive and point its record at it.
///
/// Runs BEFORE the records are archived — the microphone arm's order — so the
/// lake holds the record, never a second base64 copy of the picture.
///
/// Failure is split by whether a retry could help. A picture that will not
/// decode, or is not an image, or is over the cap, will be the same on every
/// retry: it is dropped, the share itself is kept, and a share left with
/// nothing to point at is counted by `write_bookmarks`. Anything else — the
/// disk, the database — is transient, so the batch fails and the phone's
/// outbox, which is at-least-once, sends it again. A share is not lost to a
/// hiccup.
pub async fn externalize_images(
    db: &PgPool,
    storage: &Storage,
    records: &[Value],
) -> Result<Vec<Value>> {
    let config = DriveConfig::new(std::sync::Arc::new(storage.clone()));
    let mut out = Vec::with_capacity(records.len());
    for record in records {
        let mut rec = record.clone();
        match take_shared_image(&mut rec) {
            SharedImage::Absent => {}
            SharedImage::Unusable(why) => {
                tracing::warn!(why, "shared image dropped; keeping the share without it");
            }
            SharedImage::Keep {
                bytes,
                declared_mime,
            } => match store_image_bytes_in(db, &config, &bytes, &declared_mime).await {
                Ok(stored) => {
                    rec["asset_id"] = json!(stored.file_id);
                    // The bytes' own hash is the identity: re-sharing the same
                    // screenshot upserts. It outranks any hash the phone sent.
                    rec["content_hash"] = json!(sha256_hex(&bytes));
                }
                Err(virtues::error::Error::InvalidInput(reason)) => {
                    tracing::warn!(reason, "shared image refused; keeping the share without it");
                }
                Err(e) => return Err(anyhow!("could not store a shared image: {e}")),
            },
        }
        out.push(rec);
    }
    Ok(out)
}

/// Write shared items as bookmarks. Returns (written, skipped).
pub async fn write_bookmarks(db: &PgPool, records: &[Value]) -> Result<(usize, usize)> {
    if records.is_empty() {
        return Ok((0, 0));
    }

    let mut rows: Vec<BookmarkRow> = Vec::new();
    let mut notes: Vec<(String, String)> = Vec::new();
    let mut skipped = 0usize;

    for record in records {
        let url = record
            .get("url")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let asset_id = record
            .get("asset_id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());

        // Identity: the image's content hash when there is one, so re-sharing
        // the same screenshot upserts rather than duplicating; otherwise the
        // URL, matching what the in-app save does.
        let source_stream_id = match (
            record.get("content_hash").and_then(|v| v.as_str()),
            url,
            asset_id,
        ) {
            (Some(hash), _, _) if !hash.trim().is_empty() => {
                format!("ios:share:sha256:{}", hash.trim())
            }
            (_, Some(u), _) => format!(
                "ios:share:url:{}",
                uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, u.as_bytes())
            ),
            (_, None, Some(a)) => format!("ios:share:asset:{a}"),
            // Nothing to point at is not a bookmark. Skip rather than write a
            // row that can never be opened.
            _ => {
                skipped += 1;
                continue;
            }
        };

        // The url column is NOT NULL, and for an asset-only share the honest
        // address is where it lives.
        let stored_url = match (url, asset_id) {
            (Some(u), _) => u.to_string(),
            (None, Some(a)) => format!("/drive/{a}"),
            _ => unreachable!("guarded above"),
        };

        let platform = platform_of(record.get("source_app").and_then(|v| v.as_str()));
        let mut metadata = json!({});
        if let Some(a) = asset_id {
            metadata["asset_id"] = json!(a);
        }
        if let Some(app) = record.get("source_app").and_then(|v| v.as_str()) {
            metadata["source_app"] = json!(app);
        }

        if let Some(note) = record
            .get("note")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|n| !n.is_empty())
        {
            notes.push((source_stream_id.clone(), note.to_string()));
        }

        rows.push(BookmarkRow {
            url: stored_url,
            title: record
                .get("title")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_string),
            description: None,
            source_platform: platform,
            bookmark_type: Some(if asset_id.is_some() { "screenshot" } else { "save" }.to_string()),
            author: None,
            // Deliberately none: `tags` is harvested from a source's own
            // containers, and a share sheet has none. Leaving it NULL also
            // keeps it out of the way of the rule that sync owns this column.
            tags: None,
            thumbnail_url: None,
            // A share is an event, so the phone's timestamp is the honest one;
            // absent that, the moment it landed.
            timestamp: record
                .get("timestamp")
                .map(|_| parse_timestamp(record, "timestamp"))
                .unwrap_or_else(Utc::now),
            source_stream_id,
            source_table: "ios_share".to_string(),
            source_provider: IOS_PROVIDER.to_string(),
            metadata,
        });
    }

    if rows.is_empty() {
        return Ok((0, skipped));
    }
    let written = upsert_bookmarks(db, &rows).await?;

    // The note travels outside the shared upsert, which never writes it — a
    // sync row's NULL would clobber what the user typed. Guarded on presence,
    // so re-sharing something without a note keeps the note already there.
    for (stream_id, note) in &notes {
        sqlx::query(
            "UPDATE data_content_bookmark SET note = $2, updated_at = now()
              WHERE source_stream_id = $1 AND (note IS NULL OR note = '')",
        )
        .bind(stream_id)
        .bind(note)
        .execute(db)
        .await
        .map_err(|e| anyhow!("failed to write share note: {e}"))?;
    }

    Ok((written, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_share_without_a_picture_is_left_alone() {
        let mut rec = json!({ "url": "https://example.com/a" });
        assert!(matches!(take_shared_image(&mut rec), SharedImage::Absent));
        assert_eq!(rec, json!({ "url": "https://example.com/a" }));
    }

    #[test]
    fn a_picture_is_taken_out_of_the_record_and_decoded() {
        let mut rec = json!({ "image_data": "iVBORw0KGgo=", "image_mime": "image/png", "note": "n" });
        match take_shared_image(&mut rec) {
            SharedImage::Keep { bytes, declared_mime } => {
                assert_eq!(&bytes[..4], &[0x89, b'P', b'N', b'G']);
                assert_eq!(declared_mime, "image/png");
            }
            other => panic!("expected Keep, got {other:?}"),
        }
        assert!(rec.get("image_data").is_none(), "the base64 must not reach the archive");
        assert_eq!(rec["note"], "n", "the rest of the share is untouched");
    }

    #[test]
    fn a_picture_that_will_never_decode_is_dropped_but_still_removed() {
        for bad in [json!("not base64 !!"), json!(""), json!(42)] {
            let mut rec = json!({ "url": "https://example.com/a", "image_data": bad });
            assert!(
                matches!(take_shared_image(&mut rec), SharedImage::Unusable(_)),
                "{rec}"
            );
            assert!(rec.get("image_data").is_none(), "removed even when unusable");
            assert_eq!(rec["url"], "https://example.com/a", "the share survives");
        }
    }

    #[test]
    fn known_apps_map_to_platform_names() {
        assert_eq!(platform_of(Some("com.burbn.instagram")).as_deref(), Some("instagram"));
        assert_eq!(platform_of(Some("com.apple.mobilesafari")).as_deref(), Some("safari"));
    }

    /// The three identity shapes against a real table (needs DATABASE_URL):
    ///
    ///     cargo test -p virtues-applets --bin ios_ingest -- --ignored shares_land
    #[tokio::test]
    #[ignore]
    async fn shares_land_with_the_right_identity_and_url() {
        let _ = dotenv::dotenv();
        let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL").unwrap())
            .await
            .unwrap();
        let hash = format!("test{}", uuid::Uuid::new_v4().simple());

        let recs = vec![
            // A share carrying BOTH a source URL and a stored image: the url
            // stays the source, the asset is named in metadata.
            json!({
                "url": "https://instagram.com/p/test-xyz", "asset_id": "file_test1",
                "content_hash": hash, "note": "the green door",
                "source_app": "com.burbn.instagram",
                "timestamp": "2026-02-11T23:14:00Z"
            }),
            // A camera-roll screenshot with no source: url becomes the viewer route.
            json!({ "asset_id": "file_test2", "source_app": "com.apple.mobileslideshow" }),
            // A plain link share.
            json!({ "url": "https://example.com/test-share", "source_app": "com.apple.mobilesafari" }),
            // Nothing to point at — must be skipped, not written.
            json!({ "note": "orphan" }),
        ];

        let (written, skipped) = write_bookmarks(&pool, &recs).await.unwrap();
        assert_eq!(skipped, 1, "a share with no url and no asset should be skipped");
        assert!(written >= 3, "expected three rows, got {written}");

        let row: (String, Option<String>, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT url, note, source_platform, metadata->>'asset_id'
               FROM data_content_bookmark WHERE source_stream_id = $1",
        )
        .bind(format!("ios:share:sha256:{hash}"))
        .fetch_one(&pool)
        .await
        .expect("content-hash identity row");
        assert_eq!(row.0, "https://instagram.com/p/test-xyz", "source url was replaced");
        assert_eq!(row.1.as_deref(), Some("the green door"));
        assert_eq!(row.2.as_deref(), Some("instagram"));
        assert_eq!(row.3.as_deref(), Some("file_test1"), "asset not recorded");

        let (asset_url,): (String,) = sqlx::query_as(
            "SELECT url FROM data_content_bookmark WHERE source_stream_id = 'ios:share:asset:file_test2'",
        )
        .fetch_one(&pool)
        .await
        .expect("asset-only row");
        assert_eq!(asset_url, "/drive/file_test2", "asset share needs the viewer route");

        // Re-sharing the same screenshot upserts rather than duplicating, and
        // must not clobber a note the user has since edited.
        sqlx::query("UPDATE data_content_bookmark SET note = 'edited by hand' WHERE source_stream_id = $1")
            .bind(format!("ios:share:sha256:{hash}"))
            .execute(&pool)
            .await
            .unwrap();
        write_bookmarks(&pool, &recs[..1]).await.unwrap();
        let (note,): (Option<String>,) = sqlx::query_as(
            "SELECT note FROM data_content_bookmark WHERE source_stream_id = $1",
        )
        .bind(format!("ios:share:sha256:{hash}"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(note.as_deref(), Some("edited by hand"), "re-share clobbered the note");

        sqlx::query(
            "DELETE FROM data_content_bookmark
              WHERE source_stream_id IN ($1, 'ios:share:asset:file_test2')
                 OR url = 'https://example.com/test-share'",
        )
        .bind(format!("ios:share:sha256:{hash}"))
        .execute(&pool)
        .await
        .unwrap();
    }

    #[test]
    fn unknown_bundle_passes_through_rather_than_becoming_unknown() {
        // Still true, still greppable, and it tells the next person the exact
        // string to add to the table.
        assert_eq!(platform_of(Some("com.acme.reader")).as_deref(), Some("com.acme.reader"));
        assert_eq!(platform_of(None), None);
    }
}

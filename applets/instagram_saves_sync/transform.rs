//! Instagram Saved feed → `data_content_bookmark` rows.
//!
//! The payload is the web app's `GET /api/v1/feed/saved/posts/` response:
//! `{"items":[{"media":{…}}, …], "more_available", "next_max_id"}`. Each item
//! wraps one saved `media`:
//!
//!   - `pk` — the media's stable numeric id (the row's identity)
//!   - `code` — the shortcode in the permalink `instagram.com/p/{code}/`
//!   - `media_type` — 1 photo, 2 video, 8 carousel
//!   - `caption.text` — the words (nullable; many posts have none)
//!   - `user.username` — the author
//!   - `taken_at` — unix seconds, the POST's time (see below)
//!   - `image_versions2.candidates[0].url` — the picture, highest first; a
//!     carousel keeps its images under `carousel_media[].image_versions2`, and a
//!     video's cover sits in its own `image_versions2`
//!   - `saved_collection_ids` — which of the person's collections hold it
//!
//! These field names are from the documented shape of this endpoint, not from a
//! captured page: the parser is written to tolerate their absence (a missing
//! caption, a missing image) rather than assume them, and the fixture test pins
//! the shape it expects.
//!
//! What Instagram does NOT give: when you SAVED it. `taken_at` is when the post
//! was made, so `occurred_at` is the post's time — the same honest-but-imperfect
//! choice as X, over inventing a save time. No wall-clock fallback: a post that
//! cannot be placed in time is skipped.

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use sqlx::PgPool;
use virtues::bookmark_media::StoredImage;
use virtues_helpers::bookmarks::{self, BookmarkRow};

/// One parsed page.
pub struct ParsedPage {
    pub items: Vec<Save>,
    pub more_available: bool,
    pub next_max_id: Option<String>,
}

/// One saved post, before its image is stored. The row is complete except for
/// `metadata.asset_id`, which only exists once the image has been kept.
pub struct Save {
    /// The high-water key and the identity (`ig:saved:{pk}`).
    pub media_id: String,
    /// The picture to keep, if the post has one.
    pub image_url: Option<String>,
    row: BookmarkRow,
}

/// Parse one Saved-feed page.
///
/// A body with no `items` array is an error, not an empty page: a session that
/// lost its login can come back 200 with an error envelope, and reading that as
/// "no saves" would look like a drained feed. An `items` array that is present
/// but empty is a legitimate last page.
pub fn parse_page(body: &Value) -> Result<ParsedPage> {
    let items = body
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            anyhow!(
                "instagram saved-feed response has no items array (status: {})",
                body.get("status").and_then(Value::as_str).unwrap_or("unknown")
            )
        })?;

    let saves = items.iter().filter_map(parse_item).collect();

    Ok(ParsedPage {
        items: saves,
        more_available: body
            .get("more_available")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        next_max_id: body
            .get("next_max_id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(String::from),
    })
}

/// One item → a save, or `None` when the shape is unexpected or the post has no
/// time we can place it at.
fn parse_item(item: &Value) -> Option<Save> {
    let media = item.get("media")?;

    // `pk` is numeric in the JSON; fall back to the string `id` form.
    let media_id = media
        .get("pk")
        .map(|v| match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        })
        .filter(|s| !s.is_empty() && s != "null")
        .or_else(|| media.get("id").and_then(Value::as_str).map(String::from))?;

    let taken_at = media
        .get("taken_at")
        .and_then(Value::as_i64)
        .and_then(|secs| chrono::DateTime::from_timestamp(secs, 0))?;

    let code = media.get("code").and_then(Value::as_str);
    // The permalink is where the post IS. Without a shortcode there is nothing
    // to open, and `url` is never empty (the room, the citation chip and
    // canonicalization all assume it), so such an item is skipped.
    let url = format!("https://www.instagram.com/p/{}/", code?);

    let caption = media
        .get("caption")
        .and_then(|c| c.get("text"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(String::from);

    let username = media
        .get("user")
        .and_then(|u| u.get("username"))
        .and_then(Value::as_str)
        .map(String::from);

    let media_type = media.get("media_type").and_then(Value::as_i64);

    let collections: Vec<String> = media
        .get("saved_collection_ids")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| match v {
                    Value::String(s) => Some(s.clone()),
                    Value::Number(n) => Some(n.to_string()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();

    let image_url = first_image_url(media);

    let row = BookmarkRow {
        url,
        // A post has no title; its caption is the readable content.
        title: None,
        description: caption,
        source_platform: Some("instagram".to_string()),
        bookmark_type: Some("save".to_string()),
        author: username.clone(),
        // Collection membership arrives as opaque ids — the endpoint that named
        // them is dead (404). Kept in metadata, not surfaced as tags, until the
        // names can be resolved; an id is not something a person can file by.
        tags: None,
        // Deliberately NOT the CDN image URL: it expires within hours, and a
        // thumbnail that dies is worse than none. The stored Drive copy is the
        // durable picture; the room renders asset-backed saves from it.
        thumbnail_url: None,
        timestamp: taken_at,
        source_stream_id: format!("ig:saved:{media_id}"),
        source_table: "instagram_saves".to_string(),
        source_provider: "instagram".to_string(),
        metadata: json!({
            "media_pk": media_id,
            "code": code,
            "media_type": media_type,
            "username": username,
            "saved_collection_ids": collections,
        }),
    };

    Some(Save {
        media_id,
        image_url,
        row,
    })
}

/// The picture worth keeping: a photo's own image, a carousel's first image, a
/// video's cover. `candidates` are ordered highest resolution first.
fn first_image_url(media: &Value) -> Option<String> {
    let from_versions = |m: &Value| -> Option<String> {
        m.get("image_versions2")?
            .get("candidates")?
            .as_array()?
            .first()?
            .get("url")?
            .as_str()
            .map(String::from)
    };
    from_versions(media).or_else(|| {
        media
            .get("carousel_media")
            .and_then(Value::as_array)
            .and_then(|c| c.first())
            .and_then(from_versions)
    })
}

/// Write one save, naming its stored image when there is one. Idempotent on
/// `ig:saved:{pk}`.
///
/// `metadata.asset_id` is what turns the row into an image bookmark: with it,
/// the enrichment sweep reads the picture from Drive instead of fetching the
/// login-walled permalink. Without it (the image could not be kept), the row is
/// still written — the caption is findable text on its own.
pub async fn write_save(db: &PgPool, save: &Save, stored: Option<&StoredImage>) -> Result<usize> {
    let mut row = save.row.clone();
    if let Some(img) = stored {
        row.metadata["asset_id"] = json!(img.file_id);
    }
    bookmarks::upsert_bookmarks(db, &[row]).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page in the endpoint's documented shape: a photo, a carousel, a
    /// captionless video, and one item with no shortcode. Hand-built — a real
    /// page is one person's private saves and must never live in the repo.
    fn fixture() -> Value {
        json!({
            "status": "ok",
            "more_available": true,
            "next_max_id": "CURSOR_2",
            "items": [
                { "media": {
                    "pk": 1111, "code": "PHOTO1", "media_type": 1, "taken_at": 1_700_000_000,
                    "caption": { "text": "  the cream house with the green door  " },
                    "user": { "username": "davidokafor" },
                    "saved_collection_ids": ["1790000000000001"],
                    "image_versions2": { "candidates": [
                        { "url": "https://cdn.example.com/photo-hi.jpg" },
                        { "url": "https://cdn.example.com/photo-lo.jpg" }
                    ]}
                }},
                { "media": {
                    "pk": "2222", "code": "CAROUSEL", "media_type": 8, "taken_at": 1_700_000_100,
                    "user": { "username": "nick" },
                    "carousel_media": [
                        { "image_versions2": { "candidates": [{ "url": "https://cdn.example.com/c1.jpg" }] } },
                        { "image_versions2": { "candidates": [{ "url": "https://cdn.example.com/c2.jpg" }] } }
                    ]
                }},
                { "media": {
                    "pk": 3333, "code": "VIDEO1", "media_type": 2, "taken_at": 1_700_000_200,
                    "caption": null,
                    "image_versions2": { "candidates": [{ "url": "https://cdn.example.com/cover.jpg" }] }
                }},
                { "media": { "pk": 4444, "media_type": 1, "taken_at": 1_700_000_300 } }
            ]
        })
    }

    #[test]
    fn parses_photo_carousel_and_video_and_the_cursor() {
        let page = parse_page(&fixture()).expect("page parses");
        assert!(page.more_available);
        assert_eq!(page.next_max_id.as_deref(), Some("CURSOR_2"));
        // The shortcode-less item is dropped; the other three survive.
        assert_eq!(page.items.len(), 3);

        let photo = &page.items[0];
        assert_eq!(photo.media_id, "1111", "numeric pk read as its digits");
        assert_eq!(photo.row.url, "https://www.instagram.com/p/PHOTO1/");
        assert_eq!(photo.row.description.as_deref(), Some("the cream house with the green door"));
        assert_eq!(photo.row.author.as_deref(), Some("davidokafor"));
        assert_eq!(photo.row.source_stream_id, "ig:saved:1111");
        assert_eq!(photo.image_url.as_deref(), Some("https://cdn.example.com/photo-hi.jpg"),
            "highest-resolution candidate first");
        assert_eq!(photo.row.metadata["saved_collection_ids"][0], "1790000000000001");
        assert!(photo.row.thumbnail_url.is_none(), "the expiring CDN URL is never stored");

        let carousel = &page.items[1];
        assert_eq!(carousel.media_id, "2222", "string pk accepted");
        assert_eq!(carousel.image_url.as_deref(), Some("https://cdn.example.com/c1.jpg"),
            "a carousel keeps its first image");
        assert!(carousel.row.description.is_none(), "no caption is None, not empty");

        let video = &page.items[2];
        assert_eq!(video.image_url.as_deref(), Some("https://cdn.example.com/cover.jpg"),
            "a video keeps its cover");
        assert!(video.row.description.is_none(), "a null caption is None");
    }

    #[test]
    fn a_body_without_items_is_an_error_not_an_empty_feed() {
        let err = parse_page(&json!({ "status": "fail", "message": "login_required" }))
            .err()
            .expect("must error");
        assert!(err.to_string().contains("no items array"), "{err}");
        assert!(err.to_string().contains("fail"), "status carried into the error: {err}");
    }

    #[test]
    fn an_empty_items_array_is_a_legitimate_last_page() {
        let page = parse_page(&json!({ "status": "ok", "items": [], "more_available": false }))
            .expect("empty is valid");
        assert!(page.items.is_empty());
        assert!(!page.more_available);
        assert!(page.next_max_id.is_none());
    }

    #[test]
    fn a_post_without_a_time_is_dropped_rather_than_stamped_now() {
        let mut f = fixture();
        f["items"][0]["media"]["taken_at"] = json!(null);
        let page = parse_page(&f).expect("parses");
        assert!(page.items.iter().all(|s| s.media_id != "1111"));
    }

    #[test]
    fn the_stored_image_becomes_the_asset_id() {
        let page = parse_page(&fixture()).unwrap();
        let mut row = page.items[0].row.clone();
        let img = StoredImage { file_id: "file_kept".into(), mime: "image/jpeg".into() };
        row.metadata["asset_id"] = json!(img.file_id);
        // Mirrors write_save: the asset id lands in metadata, which is exactly
        // what bookmark_enrichment::ASSET_BACKED_SQL keys on.
        assert_eq!(row.metadata["asset_id"], "file_kept");
        assert_eq!(page.items[0].row.metadata.get("asset_id"), None,
            "the parsed row carries no asset until an image is kept");
    }
}

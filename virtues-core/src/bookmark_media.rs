//! Keep a saved post's image on the box.
//!
//! A saved Instagram (or similar) post is a caption plus a picture, and the
//! picture is the half a caption cannot make findable. But its URL is signed
//! and expires within hours, so the delayed enrichment sweep would always
//! arrive to a dead link. The fix, ratified in the plan's screenshot decision
//! (agents/plan/bookmarks-plan.md): keep the source URL as the bookmark's
//! address *and* copy the image into Drive as an asset, named in
//! `metadata.asset_id`. The image pass then reads it from Drive — which does
//! not expire — long after the CDN link has died.
//!
//! Content-addressed: the file is stored under its own SHA-256, so re-saving
//! the same image (or two collections holding it) writes one file, and Drive's
//! path-dedup returns the existing row rather than a copy.

use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::api::{drive, DriveConfig};
use crate::error::{Error, Result};

/// The Drive folder saved-post images land in. A hidden system path, out of the
/// person's own file tree — these are enrichment inputs, not files they filed.
const MEDIA_DIR: &str = ".bookmarks/media";

/// Largest image kept. A saved post's display image is well under this; the cap
/// is here so a URL that turns out to be a video (or a decompression bomb) is
/// refused rather than pulled whole. Matches the image pass's own read limit —
/// storing something the reader would then reject helps no one.
const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;

/// What a stored image leaves behind for the bookmark row.
pub struct StoredImage {
    /// The Drive file id, for `metadata.asset_id`.
    pub file_id: String,
    pub mime: String,
}

/// Fetch an image URL and store it in Drive, returning its file id.
///
/// Guarded and size-capped by [`crate::fetch::fetch_image`]. Errors are the
/// caller's to soften: a saved post whose image cannot be fetched is still a
/// bookmark worth keeping for its caption, so the sync writes the row without an
/// asset rather than dropping it.
pub async fn store_saved_image(pool: &PgPool, url: &str) -> Result<StoredImage> {
    // Read one byte past the cap so "exactly the cap" and "over it" are
    // distinguishable — a truncated image would not decode.
    let img = crate::fetch::fetch_image(url, MAX_IMAGE_BYTES + 1).await?;
    store_image_bytes(pool, &img.bytes, &img.mime).await
}

/// Store image bytes that already arrived — the phone's share sheet sends the
/// picture itself — under the same rules as a fetched one: an `image/*` type,
/// not empty, within the cap, content-addressed in the same folder.
///
/// One door for both, so a screenshot shared from the phone and a picture
/// fetched by a sync land identically and dedup against each other.
pub async fn store_image_bytes(pool: &PgPool, bytes: &[u8], mime: &str) -> Result<StoredImage> {
    let storage = crate::storage::Storage::file(
        crate::storage::lake::lake_root()
            .to_string_lossy()
            .into_owned(),
    )
    .map_err(|e| Error::Storage(format!("storage unavailable: {e}")))?;
    let config = DriveConfig::new(std::sync::Arc::new(storage));
    store_image_bytes_in(pool, &config, bytes, mime).await
}

/// [`store_image_bytes`] against a given Drive. The storage is a parameter so
/// the store can be exercised against a scratch directory: the default resolves
/// the lake from a process-wide environment variable, which a test cannot move
/// without racing every other test in the binary.
pub async fn store_image_bytes_in(
    pool: &PgPool,
    config: &DriveConfig,
    bytes: &[u8],
    declared_mime: &str,
) -> Result<StoredImage> {
    // The bytes are the fact; the sender's label is a hint. A PNG a phone
    // labelled JPEG would otherwise be stored, and shown to the model, as the
    // wrong thing.
    let mime = sniff_image_mime(bytes)
        .map(str::to_string)
        .unwrap_or_else(|| declared_mime.trim().to_ascii_lowercase());
    if !mime.starts_with("image/") {
        return Err(Error::InvalidInput(format!(
            "expected an image, got {}",
            if mime.is_empty() { "an unknown type" } else { &mime }
        )));
    }
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(Error::InvalidInput(format!(
            "image is over the {}MB limit",
            MAX_IMAGE_BYTES / 1_048_576
        )));
    }
    if bytes.is_empty() {
        return Err(Error::InvalidInput("image was empty".into()));
    }

    let sha = sha256_hex(bytes);
    let filename = format!("{sha}.{}", extension_for(&mime));

    let file = drive::upload_system_file(
        pool,
        config,
        MEDIA_DIR,
        &filename,
        Some(mime.clone()),
        bytes,
    )
    .await?;

    Ok(StoredImage {
        file_id: file.id,
        mime,
    })
}

/// The content hash an image is stored under — also the identity a shared
/// screenshot is keyed by, so re-sharing the same picture upserts.
pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// The image type the bytes declare by their magic numbers, or `None` when they
/// are not an image this recognizes. Covers what phones and CDNs actually send:
/// PNG (iOS screenshots), JPEG, GIF, WebP, and the ISO-BMFF family (HEIC photos,
/// AVIF).
pub fn sniff_image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF8") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        match &bytes[8..12] {
            b"heic" | b"heix" | b"hevc" | b"hevx" | b"mif1" | b"msf1" => return Some("image/heic"),
            b"avif" | b"avis" => return Some("image/avif"),
            _ => {}
        }
    }
    None
}

/// A file extension for an image content type. The stored name is cosmetic —
/// the mime type on the Drive row is what the image pass reads — so an unknown
/// image subtype falls back to `img` rather than failing the store.
fn extension_for(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/heic" => "heic",
        "image/avif" => "avif",
        _ => "img",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1x1 PNG — the smallest real image, so the store is exercised end to end.
    const PNG_1X1: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
        0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
        0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78,
        0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
        0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    fn scratch_drive() -> (tempfile::TempDir, DriveConfig) {
        let dir = tempfile::tempdir().unwrap();
        let storage =
            crate::storage::Storage::file(dir.path().to_string_lossy().into_owned()).unwrap();
        (dir, DriveConfig::new(std::sync::Arc::new(storage)))
    }

    #[sqlx::test]
    async fn an_image_is_stored_once_under_its_hash_with_the_type_its_bytes_declare(pool: PgPool) {
        let (dir, config) = scratch_drive();

        // Declared as JPEG by a careless sender; the bytes say PNG, and PNG wins.
        let first = store_image_bytes_in(&pool, &config, PNG_1X1, "image/jpeg")
            .await
            .expect("stores");
        assert_eq!(first.mime, "image/png");

        let (path, mime): (String, Option<String>) =
            sqlx::query_as("SELECT path, mime_type FROM app_drive_files WHERE id = $1")
                .bind(&first.file_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(mime.as_deref(), Some("image/png"));
        assert!(path.starts_with(MEDIA_DIR), "lands in the hidden media folder: {path}");
        assert!(path.ends_with(&format!("{}.png", sha256_hex(PNG_1X1))), "named by its hash: {path}");
        assert!(dir.path().join(&path).is_file(), "the bytes are on disk");

        // The same picture again — a re-share, or a second collection holding it
        // — is the same file, not a copy.
        let again = store_image_bytes_in(&pool, &config, PNG_1X1, "image/png").await.unwrap();
        assert_eq!(again.file_id, first.file_id);
        let (n,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM app_drive_files WHERE is_folder = false AND path LIKE $1",
        )
        .bind(format!("{MEDIA_DIR}/%"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(n, 1);
    }

    #[sqlx::test]
    async fn bytes_that_are_not_an_image_are_refused_before_anything_is_written(pool: PgPool) {
        let (dir, config) = scratch_drive();
        let err = store_image_bytes_in(&pool, &config, b"just some text", "text/plain")
            .await
            .err()
            .expect("refused");
        assert!(matches!(err, Error::InvalidInput(_)), "{err}");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0, "nothing on disk");
    }

    #[test]
    fn sniffing_reads_the_magic_numbers() {
        assert_eq!(sniff_image_mime(PNG_1X1), Some("image/png"));
        assert_eq!(sniff_image_mime(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(sniff_image_mime(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(sniff_image_mime(b"\0\0\0\x18ftypheic\0\0"), Some("image/heic"));
        assert_eq!(sniff_image_mime(b"hello"), None);
    }

    #[test]
    fn extension_covers_the_common_types_and_falls_back() {
        assert_eq!(extension_for("image/jpeg"), "jpg");
        assert_eq!(extension_for("image/webp"), "webp");
        assert_eq!(extension_for("image/tiff"), "img");
        assert_eq!(extension_for(""), "img");
    }
}

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
    if img.bytes.len() > MAX_IMAGE_BYTES {
        return Err(Error::InvalidInput(format!(
            "image is over the {}MB limit",
            MAX_IMAGE_BYTES / 1_048_576
        )));
    }
    if img.bytes.is_empty() {
        return Err(Error::InvalidInput("image was empty".into()));
    }

    let sha = format!("{:x}", Sha256::digest(&img.bytes));
    let ext = extension_for(&img.mime);
    let filename = format!("{sha}.{ext}");

    let storage = crate::storage::Storage::file(
        crate::storage::lake::lake_root()
            .to_string_lossy()
            .into_owned(),
    )
    .map_err(|e| Error::Storage(format!("storage unavailable: {e}")))?;
    let config = DriveConfig::new(std::sync::Arc::new(storage));

    let file = drive::upload_system_file(
        pool,
        &config,
        MEDIA_DIR,
        &filename,
        Some(img.mime.clone()),
        &img.bytes,
    )
    .await?;

    Ok(StoredImage {
        file_id: file.id,
        mime: img.mime,
    })
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

    #[test]
    fn extension_covers_the_common_types_and_falls_back() {
        assert_eq!(extension_for("image/jpeg"), "jpg");
        assert_eq!(extension_for("image/webp"), "webp");
        assert_eq!(extension_for("image/tiff"), "img");
        assert_eq!(extension_for(""), "img");
    }
}

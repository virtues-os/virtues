//! Media API - Content-addressed storage for page-embedded media
//!
//! Handles upload and retrieval of images, videos, and audio files embedded in pages.
//! Uses SHA-256 content addressing for automatic deduplication.
//!
//! Storage structure: `.media/{hash_prefix}/{full_hash}.{ext}`
//! - Files are stored by their content hash, not filename
//! - Duplicate uploads return the existing file (dedup)
//! - Protected from user deletion (system folder)

use axum::body::Bytes;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::api::drive::{self, DriveConfig, DriveFile, MEDIA_FOLDER};
use crate::error::{Error, Result};

// =============================================================================
// Types
// =============================================================================

/// Response from media upload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaFile {
    /// Drive file ID
    pub id: String,
    /// URL to download the file
    pub url: String,
    /// Original filename
    pub filename: String,
    /// MIME type
    pub mime_type: Option<String>,
    /// Size in bytes
    pub size_bytes: i64,
    /// Image width (if applicable)
    pub width: Option<u32>,
    /// Image height (if applicable)
    pub height: Option<u32>,
    /// Whether this was a duplicate (existing file returned)
    pub deduplicated: bool,
}

/// Upload request (for API layer)
#[derive(Debug, Deserialize)]
pub struct UploadMediaRequest {
    /// Original filename
    pub filename: String,
    /// Optional MIME type (auto-detected if not provided)
    pub mime_type: Option<String>,
}

// =============================================================================
// Constants
// =============================================================================

/// Supported image MIME types
const IMAGE_TYPES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
    "image/svg+xml",
    "image/bmp",
    "image/x-icon",
];

/// Supported video MIME types
const VIDEO_TYPES: &[&str] = &[
    "video/mp4",
    "video/webm",
    "video/quicktime",
    "video/x-msvideo",
    "video/x-matroska",
];

/// Supported audio MIME types
const AUDIO_TYPES: &[&str] = &[
    "audio/mpeg",
    "audio/wav",
    "audio/ogg",
    "audio/flac",
    "audio/aac",
    "audio/x-m4a",
];

/// Maximum file size (50 MB)
const MAX_FILE_SIZE: usize = 50 * 1024 * 1024;

// =============================================================================
// API Functions
// =============================================================================

/// Upload media file with content-addressed deduplication
///
/// Files are stored at `.media/{hash_prefix}/{full_hash}.{ext}` where:
/// - `hash_prefix` is the first 2 characters of the SHA-256 hash
/// - `full_hash` is the complete SHA-256 hash
/// - `ext` is the file extension
///
/// If a file with the same content already exists, returns the existing file
/// without uploading again (dedup).
pub async fn upload_media(
    pool: &SqlitePool,
    config: &DriveConfig,
    filename: &str,
    mime_type: Option<String>,
    data: Bytes,
) -> Result<MediaFile> {
    let size = data.len();

    // Validate file size
    if size > MAX_FILE_SIZE {
        return Err(Error::InvalidInput(format!(
            "File too large. Maximum size is {} MB",
            MAX_FILE_SIZE / 1024 / 1024
        )));
    }

    // Determine MIME type
    let mime = mime_type
        .or_else(|| mime_guess::from_path(filename).first().map(|m| m.to_string()))
        .unwrap_or_else(|| "application/octet-stream".to_string());

    // Validate MIME type
    let is_supported = IMAGE_TYPES.contains(&mime.as_str())
        || VIDEO_TYPES.contains(&mime.as_str())
        || AUDIO_TYPES.contains(&mime.as_str());

    if !is_supported {
        return Err(Error::InvalidInput(format!(
            "Unsupported media type: {}. Supported types: images, videos, audio",
            mime
        )));
    }

    // Calculate SHA-256 hash
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let hash = format!("{:x}", hasher.finalize());

    // Extract file extension
    let ext = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");

    // Build content-addressed path: .media/{first 2 chars}/{full hash}.{ext}
    let hash_prefix = &hash[..2];
    let media_path = format!("{}/{}", MEDIA_FOLDER, hash_prefix);
    let content_filename = format!("{}.{}", hash, ext);
    let full_path = format!("{}/{}", media_path, content_filename);

    // Check if file already exists (dedup)
    if let Ok(existing) = drive::get_file_by_path(pool, &full_path).await {
        // Extract dimensions if this is an image
        let (width, height) = if mime.starts_with("image/") {
            extract_image_dimensions(&data)
        } else {
            (None, None)
        };

        return Ok(MediaFile {
            id: existing.id.clone(),
            url: format!("/api/drive/files/{}/download", existing.id),
            filename: filename.to_string(),
            mime_type: existing.mime_type,
            size_bytes: existing.size_bytes,
            width,
            height,
            deduplicated: true,
        });
    }

    // Upload to system path
    let drive_file = drive::upload_system_file(
        pool,
        config,
        &media_path,
        &content_filename,
        Some(mime.clone()),
        &data,
    )
    .await?;

    // Extract dimensions if this is an image
    let (width, height) = if mime.starts_with("image/") {
        extract_image_dimensions(&data)
    } else {
        (None, None)
    };

    Ok(MediaFile {
        id: drive_file.id.clone(),
        url: format!("/api/drive/files/{}/download", drive_file.id),
        filename: filename.to_string(),
        mime_type: drive_file.mime_type,
        size_bytes: drive_file.size_bytes,
        width,
        height,
        deduplicated: false,
    })
}

/// Get media file by ID
///
/// Returns the drive file metadata. Use the drive download endpoint to get the actual content.
pub async fn get_media(pool: &SqlitePool, file_id: &str) -> Result<DriveFile> {
    let file = drive::get_file_metadata(pool, file_id).await?;

    // Verify this is actually a media file (in .media folder)
    if !file.path.starts_with(MEDIA_FOLDER) {
        return Err(Error::NotFound("Media file not found".into()));
    }

    Ok(file)
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Extract image dimensions from raw image data
///
/// Currently returns None - dimensions can be extracted on the frontend.
/// TODO: Add imagesize crate for lightweight dimension extraction if needed.
fn extract_image_dimensions(_data: &[u8]) -> (Option<u32>, Option<u32>) {
    // Skip dimension extraction for now - frontend can handle it
    // Adding the `image` crate would add significant compile time
    (None, None)
}

/// Check if a MIME type is a supported media type
pub fn is_supported_media_type(mime: &str) -> bool {
    IMAGE_TYPES.contains(&mime)
        || VIDEO_TYPES.contains(&mime)
        || AUDIO_TYPES.contains(&mime)
}

/// Check if a MIME type is an image
pub fn is_image_type(mime: &str) -> bool {
    IMAGE_TYPES.contains(&mime)
}

/// Check if a MIME type is a video
pub fn is_video_type(mime: &str) -> bool {
    VIDEO_TYPES.contains(&mime)
}

/// Check if a MIME type is audio
pub fn is_audio_type(mime: &str) -> bool {
    AUDIO_TYPES.contains(&mime)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_supported_media_type() {
        // Images
        assert!(is_supported_media_type("image/png"));
        assert!(is_supported_media_type("image/jpeg"));
        assert!(is_supported_media_type("image/webp"));

        // Videos
        assert!(is_supported_media_type("video/mp4"));
        assert!(is_supported_media_type("video/webm"));

        // Audio
        assert!(is_supported_media_type("audio/mpeg"));
        assert!(is_supported_media_type("audio/wav"));

        // Unsupported
        assert!(!is_supported_media_type("application/pdf"));
        assert!(!is_supported_media_type("text/plain"));
    }

    #[test]
    fn test_type_detection() {
        assert!(is_image_type("image/png"));
        assert!(!is_image_type("video/mp4"));

        assert!(is_video_type("video/mp4"));
        assert!(!is_video_type("audio/mpeg"));

        assert!(is_audio_type("audio/mpeg"));
        assert!(!is_audio_type("image/png"));
    }
}

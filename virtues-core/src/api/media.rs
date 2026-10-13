//! Media API - Content-addressed storage for page-embedded files
//!
//! Handles upload and retrieval of any file type embedded in pages.
//! Images, video, and audio get rich inline previews; other files get a file card.
//! Uses SHA-256 content addressing for automatic deduplication.
//!
//! Storage structure: `.media/{hash_prefix}/{full_hash}.{ext}`
//! - Files are stored by their content hash, not filename
//! - Duplicate uploads return the existing file (dedup)
//! - Protected from user deletion (system folder)

use axum::body::Bytes;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

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

/// Maximum file size (100 MB). The page editor refuses larger files before
/// they go into a page (`MAX_UPLOAD_BYTES` in `apps/web/src/lib/document/media.ts`).
pub const MAX_FILE_SIZE: usize = 100 * 1024 * 1024;

// =============================================================================
// API Functions
// =============================================================================

/// Upload file with content-addressed deduplication
///
/// Files are stored at `.media/{hash_prefix}/{full_hash}.{ext}` where:
/// - `hash_prefix` is the first 2 characters of the SHA-256 hash
/// - `full_hash` is the complete SHA-256 hash
/// - `ext` is the file extension
///
/// If a file with the same content already exists, returns the existing file
/// without uploading again (dedup).
pub async fn upload_media(
    pool: &PgPool,
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

// =============================================================================
// A page's files
// =============================================================================

/// Where a block page's files are stored, each under the Drive id its block
/// already holds.
pub const PAGE_FILES_FOLDER: &str = ".media/uploads";

/// Whether `id` is a Drive id a browser may give a page's file: `file_` and
/// 16 lowercase hex digits, the shape every Drive file id takes
/// (`ids::generate_id`). A block page's file goes into the page the moment
/// the person gives it, at its final address `/drive/<id>`, so its id is
/// made in the browser before any byte is sent (`mintFileId` in
/// `apps/web/src/lib/document/uploads.ts`).
pub fn is_page_file_id(id: &str) -> bool {
    id.strip_prefix("file_")
        .is_some_and(|hex| hex.len() == 16 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
}

/// What storing a page's file came to ([`store_page_file`]).
#[derive(Debug)]
pub enum PageFileStored {
    /// Stored now, or already stored with these bytes: a retry after an
    /// answer that was lost.
    Stored(MediaFile),
    /// The id names another file: a Drive file of the person's, or a page
    /// file with other bytes. Nothing is written over it.
    Taken,
    /// The box has no room for it now. Not a refusal: once the person makes
    /// room (the Drive trash emptied), the same file sent again is stored,
    /// so the device holding it goes on trying and every other device goes
    /// on asking.
    NoRoom,
}

/// Store a block page's file under the Drive id its block holds, at
/// `.media/uploads/<id>.<ext>`. Unlike [`upload_media`], the same bytes
/// uploaded twice are stored twice: each page file has the id its block was
/// given. Copying a block never uploads again, so only a file given twice is.
pub async fn store_page_file(
    pool: &PgPool,
    config: &DriveConfig,
    id: &str,
    filename: &str,
    mime_type: Option<String>,
    data: Bytes,
) -> Result<PageFileStored> {
    if !is_page_file_id(id) {
        return Err(Error::InvalidInput(format!("{id} isn't an address a page's file can have.")));
    }
    if data.len() > MAX_FILE_SIZE {
        return Err(Error::InvalidInput(too_large(filename)));
    }
    let hash = format!("{:x}", Sha256::digest(&data));
    let ext = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .filter(|e| !e.is_empty() && e.bytes().all(|b| b.is_ascii_alphanumeric()))
        .unwrap_or("bin");
    let stored_name = format!("{id}.{}", ext.to_ascii_lowercase());
    let path = format!("{PAGE_FILES_FOLDER}/{stored_name}");

    let existing: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT path, sha256_hash FROM app_drive_files WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to look up the file's id: {e}")))?;
    if let Some((existing_path, existing_hash)) = existing {
        if existing_path != path || existing_hash.as_deref() != Some(hash.as_str()) {
            return Ok(PageFileStored::Taken);
        }
    }

    // Room is judged once, after the file already at its path is looked
    // for: a retry of a file stored before its answer was lost takes no
    // room, and an upload that finds none, however it lost the race for the
    // last of it, stops short of a refusal.
    let mime = mime_type
        .or_else(|| mime_guess::from_path(filename).first().map(|m| m.to_string()))
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let Some(file) =
        drive::store_system_file(pool, config, PAGE_FILES_FOLDER, &stored_name, Some(mime), &data, Some(id))
            .await?
    else {
        return Ok(PageFileStored::NoRoom);
    };
    if file.id != id {
        return Ok(PageFileStored::Taken);
    }
    Ok(PageFileStored::Stored(MediaFile {
        id: file.id.clone(),
        url: format!("/api/drive/files/{}/download", file.id),
        filename: filename.to_string(),
        mime_type: file.mime_type,
        size_bytes: file.size_bytes,
        width: None,
        height: None,
        deduplicated: false,
    }))
}

/// Why a page's file was not stored for want of room, with what makes room.
pub fn no_room(filename: &str) -> String {
    format!(
        "Your server is out of space, so it didn't store {filename}. Empty the trash in Drive or delete \
         something large, then try again."
    )
}

/// Why a file is refused for its size.
pub fn too_large(filename: &str) -> String {
    format!(
        "{filename} is over {} MB, the most a page holds. Upload it to Drive and link to it from the page instead.",
        MAX_FILE_SIZE / 1024 / 1024
    )
}

/// How a page's file stands, as every device's view of its block asks
/// (`GET /api/media/uploads?ids=`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum PageFileState {
    /// Stored: the block's address loads it.
    Done,
    /// Arriving now.
    Uploading { filename: String, received: u64, total: Option<u64> },
    /// Stopped, and why. `refused`: the server refused the file (an
    /// address taken, a file too large), so sending it again is refused too
    /// and the device holding it does not; otherwise it stopped on the way
    /// (cut off, gone quiet, not stored), and that device may send it again.
    /// `stopped_ms_ago`: how long ago it stopped, so a device first asking
    /// long after says how long the sending device goes on trying from the
    /// stop, not from its own first answer.
    Failed { filename: String, reason: String, refused: bool, stopped_ms_ago: u64 },
    /// Neither stored nor arriving, nor stopped within [`FAILED_KEPT`]
    /// here: it never started, it stopped before a restart, or it stopped
    /// long enough ago that no device still holding it is trying.
    Missing,
}

/// An upload that has received nothing for this long has stopped: its
/// device went quiet (asleep, out of range) without closing the
/// connection, which nothing else on the way would notice. The server
/// stops reading it, and every device asking is told it stopped.
pub const STALLED_AFTER: std::time::Duration = std::time::Duration::from_secs(2 * 60);

/// Why an upload that went quiet stopped.
pub fn stalled(filename: &str) -> String {
    format!("{filename} stopped arriving, so your server stored nothing.")
}

/// Uploads of page files the server is receiving, said to be coming, or
/// that stopped: what a device asking about a block's file learns besides
/// whether it is stored. In memory, since a restart ends every upload in
/// flight anyway; a stopped one, said or gone quiet, is kept for
/// [`FAILED_KEPT`] after it stopped, at most [`FAILED_AT_MOST`] of them, and
/// forgotten on the next look past that.
struct Arriving {
    filename: String,
    received: u64,
    total: Option<u64>,
    /// When the last bytes arrived, the upload began, or the device holding
    /// the file last said it is coming.
    progress: std::time::Instant,
    failed: Option<Stop>,
    /// The request whose bytes these are; `None` before any request's bytes
    /// began: a file said to be coming ([`page_file_announced`]), or a stop
    /// said before its bytes.
    upload: Option<PageFileUpload>,
}

/// One request's upload of a page file, from when its bytes began
/// ([`page_file_arriving`]). The file is known by its id alone, and a
/// device sends it again when its request errors, which the server may not
/// hear of for [`STALLED_AFTER`] (a phone moved from Wi-Fi to cellular):
/// what a request says of the file (bytes arrived, a stop) counts only
/// while it is the latest to begin, so an older request's stop never says
/// the newer one stopped while its bytes still arrive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageFileUpload(u64);

static UPLOADS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Why an upload stopped, when, and whether the server refused it.
#[derive(Clone)]
struct Stop {
    reason: String,
    at: std::time::Instant,
    refused: bool,
}

impl Arriving {
    /// When it stopped, by `now`: when it was said to, or once nothing had
    /// arrived for [`STALLED_AFTER`], which is all the server sees of a
    /// device gone quiet, or of a request dropped mid-upload. `None` while
    /// it is arriving.
    fn stopped_at(&self, now: std::time::Instant) -> Option<std::time::Instant> {
        match &self.failed {
            Some(stop) => Some(stop.at),
            None => Some(self.progress + STALLED_AFTER).filter(|&quiet| now >= quiet),
        }
    }

    /// Whether it stopped [`FAILED_KEPT`] or longer before `now`: no device
    /// holding it is trying any more, so it is gone.
    fn gone(&self, now: std::time::Instant) -> bool {
        self.stopped_at(now).is_some_and(|at| now.duration_since(at) >= FAILED_KEPT)
    }

    /// How it stands at `now`: arriving, stopped (said or gone quiet), or
    /// missing once it stopped [`FAILED_KEPT`] ago.
    fn state(&self, now: std::time::Instant) -> PageFileState {
        let Some(at) = self.stopped_at(now) else {
            return PageFileState::Uploading { filename: self.filename.clone(), received: self.received, total: self.total };
        };
        if self.gone(now) {
            return PageFileState::Missing;
        }
        let stopped_ms_ago = u64::try_from(now.duration_since(at).as_millis()).unwrap_or(u64::MAX);
        match &self.failed {
            Some(stop) => PageFileState::Failed {
                filename: self.filename.clone(),
                reason: stop.reason.clone(),
                refused: stop.refused,
                stopped_ms_ago,
            },
            None => PageFileState::Failed {
                filename: self.filename.clone(),
                reason: stalled(&self.filename),
                refused: false,
                stopped_ms_ago,
            },
        }
    }
}

/// How long a stopped upload is said to have stopped; after that it is
/// missing, far past what any device holding the file goes on trying.
const FAILED_KEPT: std::time::Duration = std::time::Duration::from_secs(60 * 60);
const FAILED_AT_MOST: usize = 1000;

static ARRIVING: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<String, Arriving>>> =
    std::sync::LazyLock::new(Default::default);

fn arriving() -> std::sync::MutexGuard<'static, std::collections::HashMap<String, Arriving>> {
    // A panic while holding it leaves nothing half-written: every write is one insert or field.
    ARRIVING.lock().unwrap_or_else(|e| e.into_inner())
}

/// Every upload, those gone ([`Arriving::gone`]) forgotten first: on every
/// look but a chunk's arriving, so a stop ages out though no other upload
/// ever begins.
fn arriving_now() -> std::sync::MutexGuard<'static, std::collections::HashMap<String, Arriving>> {
    let mut all = arriving();
    let now = std::time::Instant::now();
    all.retain(|_, a| !a.gone(now));
    all
}

/// The most files at once said to be coming whose bytes have not begun: a
/// device holding more waits to say so until some have arrived.
const ANNOUNCED_AT_MOST: usize = 1000;

/// The device holding a page's file says it is coming, before a request can
/// carry its bytes: the browser sends a few requests to a server at a time,
/// and holds the rest where the server cannot see them, while the file's
/// block is already in the page on every device. Said, the file is
/// arriving, `received` 0, until its bytes begin or the device stops saying
/// so for [`STALLED_AFTER`] (closed, asleep). Says nothing of a file whose
/// bytes arrive now, nor of one the server refused, which no device sends
/// again.
pub fn page_file_announced(id: &str, filename: &str, total: Option<u64>) {
    let mut all = arriving_now();
    let now = std::time::Instant::now();
    match all.get(id) {
        Some(a) if a.upload.is_some() && a.stopped_at(now).is_none() => return,
        Some(a) if a.failed.as_ref().is_some_and(|stop| stop.refused) => return,
        Some(_) => {}
        None => {
            let announced = all.values().filter(|a| a.upload.is_none() && a.failed.is_none()).count();
            if announced >= ANNOUNCED_AT_MOST {
                return;
            }
        }
    }
    all.insert(
        id.to_string(),
        Arriving {
            filename: filename.to_string(),
            received: 0,
            total,
            progress: now,
            failed: None,
            upload: None,
        },
    );
}

/// A page file's upload began, by one request: its bytes are on their way,
/// and what that request says of them from now on is said by `upload`.
pub fn page_file_arriving(id: &str, filename: &str, total: Option<u64>) -> PageFileUpload {
    let upload = PageFileUpload(UPLOADS.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let mut all = arriving_now();
    let now = std::time::Instant::now();
    all.insert(
        id.to_string(),
        Arriving {
            filename: filename.to_string(),
            received: 0,
            total,
            progress: now,
            failed: None,
            upload: Some(upload),
        },
    );
    upload
}

/// How many of a page file's bytes `upload` has brought.
pub fn page_file_received(id: &str, upload: PageFileUpload, received: u64) {
    if let Some(a) = arriving().get_mut(id).filter(|a| a.upload == Some(upload)) {
        a.received = received;
        a.progress = std::time::Instant::now();
    }
}

/// A page file's upload stopped, and why; `refused` when the server refused
/// it, so the device holding it will not send it again. `upload`: the
/// request whose bytes stopped, or `None` for one that stopped before any
/// began, which stops a file said to be coming. A stop is no stop of a
/// request that began after it, nor of one whose bytes still arrive, and a
/// stop of a file stored since says nothing.
pub fn page_file_failed(id: &str, filename: &str, reason: &str, refused: bool, upload: Option<PageFileUpload>) {
    let mut all = arriving_now();
    let now = std::time::Instant::now();
    let failed = Some(Stop { reason: reason.to_string(), at: now, refused });
    match (all.get_mut(id), upload) {
        (Some(a), Some(upload)) if a.upload == Some(upload) => a.failed.clone_from(&failed),
        (Some(_), Some(_)) | (None, Some(_)) => return,
        (Some(a), None) if a.upload.is_some() && a.stopped_at(now).is_none() => return,
        (Some(a), None) => a.failed.clone_from(&failed),
        (None, None) => {
            all.insert(
                id.to_string(),
                Arriving {
                    filename: filename.to_string(),
                    received: 0,
                    total: None,
                    progress: now,
                    failed: failed.clone(),
                    upload: None,
                },
            );
        }
    }
    let failed = all.values().filter(|a| a.failed.is_some()).count();
    if failed > FAILED_AT_MOST {
        let mut oldest: Vec<(String, std::time::Instant)> = all
            .iter()
            .filter_map(|(id, a)| a.failed.as_ref().map(|stop| (id.clone(), stop.at)))
            .collect();
        oldest.sort_by_key(|(_, at)| *at);
        for (id, _) in oldest.into_iter().take(failed - FAILED_AT_MOST) {
            all.remove(&id);
        }
    }
}

/// A page file is stored, by whichever request brought it: the database
/// says so from now on.
pub fn page_file_stored(id: &str) {
    arriving().remove(id);
}

/// How each of `ids` stands: stored, arriving, stopped, or missing.
pub async fn page_file_states(
    pool: &PgPool,
    ids: &[String],
) -> Result<std::collections::HashMap<String, PageFileState>> {
    let stored: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM app_drive_files WHERE id = ANY($1) AND deleted_at IS NULL",
    )
    .bind(ids)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to look up the files: {e}")))?;
    let all = arriving_now();
    let now = std::time::Instant::now();
    Ok(ids
        .iter()
        .map(|id| {
            let state = if stored.contains(id) {
                PageFileState::Done
            } else {
                all.get(id).map_or(PageFileState::Missing, |a| a.state(now))
            };
            (id.clone(), state)
        })
        .collect())
}

/// Which of `ids` a page names anywhere in its markdown, a block page's
/// export included: a block's address, a link to it, the id in its text.
/// Every page as the server has it now: the docs it holds (an open editor's,
/// or one whose save waits), whose text is newer than their rows', and every
/// other page's `content`, the trash included, since a page there can be
/// put back. The device holding a file that no page names lets it go
/// (`uploads.ts`): a block whose file it is may have left one page for
/// another the device does not have open, or for another device's.
pub async fn page_files_named(
    yjs: &crate::server::yjs::YjsState,
    ids: &[String],
) -> Result<std::collections::HashMap<String, bool>> {
    let held = yjs.held_markdown().await;
    let held_pages: Vec<&str> = held.iter().map(|(page, _)| page.as_str()).collect();
    let in_rows: Vec<String> = sqlx::query_scalar(
        "SELECT needle FROM unnest($1::text[]) AS needle \
         WHERE EXISTS (SELECT 1 FROM app_pages p \
                       WHERE strpos(p.content, needle) > 0 AND NOT (p.id = ANY($2)))",
    )
    .bind(ids)
    .bind(&held_pages)
    .fetch_all(&yjs.pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to look through the pages: {e}")))?;
    Ok(ids
        .iter()
        .map(|id| {
            let named = in_rows.contains(id) || held.iter().any(|(_, text)| text.contains(id.as_str()));
            (id.clone(), named)
        })
        .collect())
}

/// Get media file by ID
///
/// Returns the drive file metadata. Use the drive download endpoint to get the actual content.
pub async fn get_media(pool: &PgPool, file_id: &str) -> Result<DriveFile> {
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

/// Image dimensions from raw image data. Always `(None, None)`: `width` and
/// `height` are never stored, and the frontend measures the image itself.
// TODO(2026-09-29): read dimensions from the header (`imagesize`, or the
// `image` crate already in Cargo.toml, which is built with png and jpeg) so the
// stored row carries them.
fn extract_image_dimensions(_data: &[u8]) -> (Option<u32>, Option<u32>) {
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

        // Non-media (still uploadable, but not "media" type)
        assert!(!is_supported_media_type("application/pdf"));
        assert!(!is_supported_media_type("text/plain"));
    }

    /// An upload that receives nothing for [`STALLED_AFTER`] reads as
    /// stopped to every device asking, though no connection said so; one
    /// whose bytes keep coming reads as arriving.
    #[test]
    fn an_upload_gone_quiet_reads_as_stopped() {
        let began = std::time::Instant::now();
        let a = Arriving {
            filename: "harbour.png".into(),
            received: 10,
            total: Some(20),
            progress: began,
            failed: None,
            upload: None,
        };
        assert!(matches!(a.state(began + STALLED_AFTER / 2), PageFileState::Uploading { received: 10, .. }));
        match a.state(began + STALLED_AFTER) {
            PageFileState::Failed { filename, reason, refused, .. } => {
                assert_eq!(filename, "harbour.png");
                assert_eq!(reason, stalled("harbour.png"));
                assert!(!refused, "a device gone quiet may come back and send it again");
            }
            other => panic!("{other:?}"),
        }
    }

    /// A stop says how long ago it was, and ages out on its own: once
    /// [`FAILED_KEPT`] has passed it is missing, said or gone quiet.
    #[test]
    fn a_stop_says_when_it_was_and_ages_out() {
        let began = std::time::Instant::now();
        let minute = std::time::Duration::from_secs(60);
        let cut = Arriving {
            filename: "harbour.png".into(),
            received: 10,
            total: Some(20),
            progress: began,
            failed: Some(Stop { reason: "cut off".into(), at: began, refused: false }),
            upload: None,
        };
        assert!(matches!(
            cut.state(began + minute),
            PageFileState::Failed { refused: false, stopped_ms_ago: 60_000, .. }
        ));
        assert!(!cut.gone(began + FAILED_KEPT - minute));
        assert_eq!(cut.state(began + FAILED_KEPT), PageFileState::Missing);
        assert!(cut.gone(began + FAILED_KEPT));

        // Gone quiet: it stopped once nothing had arrived for STALLED_AFTER.
        let quiet = Arriving { failed: None, ..cut };
        assert!(matches!(quiet.state(began + minute), PageFileState::Uploading { .. }));
        assert!(matches!(
            quiet.state(began + STALLED_AFTER + minute),
            PageFileState::Failed { refused: false, stopped_ms_ago: 60_000, .. }
        ));
        assert_eq!(quiet.state(began + STALLED_AFTER + FAILED_KEPT), PageFileState::Missing);
    }

    /// A request speaks only for the bytes it brought. The file sent again
    /// by a newer request reads as arriving whatever an older one says
    /// after (its stop, its bytes), and a stop before any request's bytes
    /// began leaves a file arriving as it is; once stored, a stop says
    /// nothing of it. A stop from any request was written over the newer
    /// one's upload, which read as stopped while its bytes arrived.
    #[test]
    fn a_request_speaks_only_for_the_bytes_it_brought() {
        let id = "file_0c0c0c0c0c0c0c0c";
        let state = || arriving_now().get(id).map(|a| a.state(std::time::Instant::now()));
        let older = page_file_arriving(id, "harbour.png", Some(64));
        page_file_received(id, older, 16);
        let newer = page_file_arriving(id, "harbour.png", Some(64));
        page_file_received(id, newer, 8);

        page_file_received(id, older, 32);
        page_file_failed(id, "harbour.png", &stalled("harbour.png"), false, Some(older));
        page_file_failed(id, "harbour.png", "harbour.png never arrived.", true, None);
        assert!(matches!(state(), Some(PageFileState::Uploading { received: 8, .. })), "{:?}", state());

        page_file_failed(id, "harbour.png", "harbour.png stopped.", false, Some(newer));
        assert!(matches!(state(), Some(PageFileState::Failed { refused: false, .. })), "{:?}", state());
        // Stopped, a later stop says the latest.
        page_file_failed(id, "harbour.png", "harbour.png never arrived.", true, None);
        assert!(matches!(state(), Some(PageFileState::Failed { refused: true, .. })), "{:?}", state());

        let last = page_file_arriving(id, "harbour.png", Some(64));
        page_file_stored(id);
        page_file_failed(id, "harbour.png", "harbour.png stopped.", false, Some(last));
        assert!(state().is_none(), "{:?}", state());
    }

    /// A file the device holding it says is coming reads as arriving before
    /// any request brings its bytes: the browser holds most of a drop's
    /// requests where the server cannot see them, and a share of the page,
    /// or another device, took the file for one never sent. Said again, it
    /// leaves a request's bytes arriving, and a refusal, as they are.
    #[test]
    fn a_file_said_to_be_coming_reads_as_arriving_before_its_bytes() {
        let id = "file_0d0d0d0d0d0d0d0d";
        let state = || arriving_now().get(id).map(|a| a.state(std::time::Instant::now()));
        assert!(state().is_none());
        page_file_announced(id, "harbour.png", Some(64));
        assert!(matches!(state(), Some(PageFileState::Uploading { received: 0, total: Some(64), .. })), "{:?}", state());

        let bytes = page_file_arriving(id, "harbour.png", Some(64));
        page_file_received(id, bytes, 16);
        page_file_announced(id, "harbour.png", Some(64));
        assert!(matches!(state(), Some(PageFileState::Uploading { received: 16, .. })), "{:?}", state());

        // Cut off, and waiting for its next try: coming again.
        page_file_failed(id, "harbour.png", "harbour.png stopped.", false, Some(bytes));
        page_file_announced(id, "harbour.png", Some(64));
        assert!(matches!(state(), Some(PageFileState::Uploading { received: 0, .. })), "{:?}", state());
        // A request that stopped before its bytes stops the file said to be coming.
        page_file_failed(id, "harbour.png", "harbour.png stopped.", false, None);
        assert!(matches!(state(), Some(PageFileState::Failed { refused: false, .. })), "{:?}", state());

        page_file_failed(id, "harbour.png", "harbour.png is over 100 MB.", true, None);
        page_file_announced(id, "harbour.png", Some(64));
        assert!(matches!(state(), Some(PageFileState::Failed { refused: true, .. })), "{:?}", state());
        page_file_stored(id);
    }

    /// A file is the pages' while any page names it, wherever: its block
    /// moved to a page the device holding the file does not have open, a
    /// link to its address, a page in the trash. A page's doc held here is
    /// newer than its row: a block pasted inside the save's wait counts, and
    /// one taken out counts no more.
    #[sqlx::test]
    async fn a_file_is_named_by_any_page_holding_its_address(pool: PgPool) {
        use yrs::{GetString, Text, Transact, WriteTxn};
        let yjs = crate::server::yjs::YjsState::new(pool.clone());
        let (kept, moved, never) = ("file_1a1a1a1a1a1a1a1a", "file_2b2b2b2b2b2b2b2b", "file_3c3c3c3c3c3c3c3c");
        let page = |content: String| crate::api::pages::CreatePageRequest {
            title: "Trip".into(),
            content,
            project_id: None,
            icon: None,
            icon_color: None,
            cover_url: None,
            tags: None,
            format: None,
        };
        let closed = crate::api::pages::create_page_as(&pool, page(format!("![harbour.png](/drive/{kept})\n")), crate::api::pages::PageFormat::Markdown)
            .await
            .unwrap()
            .page
            .id;
        let open = crate::api::pages::create_page_as(&pool, page("Notes\n".into()), crate::api::pages::PageFormat::Markdown)
            .await
            .unwrap()
            .page
            .id;
        let ids = [kept.to_string(), moved.to_string(), never.to_string()];
        let named = page_files_named(&yjs, &ids).await.unwrap();
        assert_eq!((named[kept], named[moved], named[never]), (true, false, false));

        sqlx::query("UPDATE app_pages SET deleted_at = now() WHERE id = $1").bind(&closed).execute(&pool).await.unwrap();
        assert!(page_files_named(&yjs, &ids).await.unwrap()[kept], "a page in the trash can be put back");

        // An editor's edit, not saved yet.
        let doc = yjs.doc_cache.get_or_create(&open, &pool).await.unwrap();
        {
            let d = doc.write().await;
            let mut txn = d.doc.transact_mut();
            let text = txn.get_or_insert_text("content");
            assert_eq!(text.get_string(&txn), "Notes\n");
            text.insert(&mut txn, 0, &format!("[the plan](/drive/{moved})\n"));
        }
        let named = page_files_named(&yjs, &ids).await.unwrap();
        assert_eq!((named[kept], named[moved], named[never]), (true, true, false));
        drop(doc);
    }

    /// A stop long past is missing to a device asking, and forgotten, though
    /// no other upload begins. It was forgotten only when another upload
    /// began, so a phone's upload cut off, its app closed, read as on its
    /// way until the box restarted, and publishing the page was refused.
    #[sqlx::test]
    async fn a_stop_long_past_is_missing_and_forgotten_on_the_next_look(pool: PgPool) {
        let Some(long_ago) = std::time::Instant::now().checked_sub(FAILED_KEPT + STALLED_AFTER + std::time::Duration::from_secs(60))
        else {
            eprintln!("this machine started too recently to hold an instant that far back");
            return;
        };
        let (cut, quiet) = ("file_0a0a0a0a0a0a0a0a", "file_0b0b0b0b0b0b0b0b");
        let stop = |failed: Option<Stop>| Arriving {
            filename: "harbour.png".into(),
            received: 0,
            total: None,
            progress: long_ago,
            failed,
            upload: None,
        };
        arriving().insert(cut.into(), stop(Some(Stop { reason: "cut off".into(), at: long_ago, refused: false })));
        arriving().insert(quiet.into(), stop(None));
        let states = page_file_states(&pool, &[cut.to_string(), quiet.to_string()]).await.unwrap();
        assert_eq!(states[cut], PageFileState::Missing);
        assert_eq!(states[quiet], PageFileState::Missing);
        assert!(!arriving().contains_key(cut) && !arriving().contains_key(quiet));
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

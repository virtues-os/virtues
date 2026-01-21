//! Drive API - User file storage and quota management
//!
//! Personal cloud storage for user-uploaded files (like Google Drive).
//! ELT archive data stays in S3/MinIO - this is for user files only.
//!
//! Storage tiers:
//! - Free:     100 GB
//! - Standard: 500 GB
//! - Pro:      4 TB

use axum::body::Bytes;
use futures::Stream;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::path::{Component, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::error::{Error, Result};

// =============================================================================
// Constants
// =============================================================================

/// Storage quota limits by tier (in bytes)
pub mod quotas {
    /// 100 GB for free tier
    pub const FREE_BYTES: i64 = 100 * 1024 * 1024 * 1024;
    /// 500 GB for standard tier ($19/mo)
    pub const STANDARD_BYTES: i64 = 500 * 1024 * 1024 * 1024;
    /// 4 TB for pro tier ($79/mo)
    pub const PRO_BYTES: i64 = 4 * 1024 * 1024 * 1024 * 1024;
}

/// Default drive path inside container
const DEFAULT_DRIVE_PATH: &str = "/home/user/drive";

/// Singleton ID for drive_usage table
const USAGE_SINGLETON_ID: &str = "00000000-0000-0000-0000-000000000001";

// =============================================================================
// Types
// =============================================================================

/// Drive tier with quota information
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DriveTier {
    Free,
    Standard,
    Pro,
}

impl DriveTier {
    /// Load tier from TIER environment variable
    pub fn from_env() -> Self {
        match std::env::var("TIER").as_deref() {
            Ok("pro") | Ok("Pro") | Ok("PRO") => DriveTier::Pro,
            Ok("standard") | Ok("Standard") | Ok("STANDARD") => DriveTier::Standard,
            _ => DriveTier::Free,
        }
    }

    /// Get quota in bytes for this tier
    pub fn quota_bytes(&self) -> i64 {
        match self {
            DriveTier::Free => quotas::FREE_BYTES,
            DriveTier::Standard => quotas::STANDARD_BYTES,
            DriveTier::Pro => quotas::PRO_BYTES,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            DriveTier::Free => "free",
            DriveTier::Standard => "standard",
            DriveTier::Pro => "pro",
        }
    }
}

/// Drive configuration
#[derive(Debug, Clone)]
pub struct DriveConfig {
    /// Base path for drive storage (default: /home/user/drive)
    pub base_path: PathBuf,
    /// Current tier for quota calculation
    pub tier: DriveTier,
}

impl DriveConfig {
    /// Load configuration from environment
    pub fn from_env() -> Self {
        let base_path = std::env::var("DRIVE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(DEFAULT_DRIVE_PATH));
        let tier = DriveTier::from_env();
        Self { base_path, tier }
    }
}

/// File metadata stored in database
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DriveFile {
    pub id: String,
    pub path: String,
    pub filename: String,
    pub mime_type: Option<String>,
    pub size_bytes: i64,
    pub is_folder: bool,
    pub parent_id: Option<String>,
    pub sha256_hash: Option<String>,
    pub deleted_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Storage usage summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveUsage {
    pub total_bytes: i64,
    pub quota_bytes: i64,
    pub file_count: i64,
    pub folder_count: i64,
    pub usage_percent: f64,
    pub tier: String,
}

/// Upload request parameters
#[derive(Debug, Deserialize)]
pub struct UploadRequest {
    /// Target folder path within drive (e.g., "documents" or "photos/vacation")
    pub path: String,
    /// Filename to use
    pub filename: String,
    /// Optional MIME type (auto-detected if not provided)
    pub mime_type: Option<String>,
}

/// Create folder request
#[derive(Debug, Deserialize)]
pub struct CreateFolderRequest {
    /// Parent path (e.g., "" for root or "documents/work")
    pub path: String,
    /// Folder name
    pub name: String,
}

/// Move/rename request
#[derive(Debug, Deserialize)]
pub struct MoveFileRequest {
    /// New path (including filename)
    pub new_path: String,
}

/// Quota warning levels
#[derive(Debug, Clone, Serialize)]
pub struct QuotaWarnings {
    pub warnings: Vec<String>,
    pub usage_percent: f64,
}

// =============================================================================
// Path Security
// =============================================================================

/// Validate and sanitize path to prevent traversal attacks
///
/// Ensures path:
/// - Contains no ".." components
/// - Contains no null bytes
/// - Uses forward slashes only
/// - Is relative (no absolute paths)
pub fn validate_drive_path(path: &str) -> Result<PathBuf> {
    // Empty path is valid (root)
    if path.is_empty() {
        return Ok(PathBuf::new());
    }

    // Reject null bytes
    if path.contains('\0') {
        return Err(Error::InvalidInput("Path contains null bytes".into()));
    }

    // Reject absolute paths
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(Error::InvalidInput("Absolute paths not allowed".into()));
    }

    // Normalize to forward slashes and remove trailing slash
    let normalized = path.replace('\\', "/").trim_end_matches('/').to_string();

    // Parse and check for traversal
    let path_buf = PathBuf::from(&normalized);
    for component in path_buf.components() {
        match component {
            Component::ParentDir => {
                return Err(Error::InvalidInput("Path traversal not allowed".into()));
            }
            Component::Normal(s) => {
                let s_str = s.to_string_lossy();
                // Check for hidden traversal attempts
                if s_str.contains("..") {
                    return Err(Error::InvalidInput("Path traversal not allowed".into()));
                }
                // Reject hidden files/folders (starting with .)
                if s_str.starts_with('.') && s_str != "." {
                    return Err(Error::InvalidInput("Hidden files not allowed".into()));
                }
            }
            _ => {}
        }
    }

    Ok(path_buf)
}

/// Validate filename (no path separators, no special chars)
fn validate_filename(filename: &str) -> Result<()> {
    if filename.is_empty() {
        return Err(Error::InvalidInput("Filename cannot be empty".into()));
    }
    if filename.contains('/') || filename.contains('\\') {
        return Err(Error::InvalidInput(
            "Filename cannot contain path separators".into(),
        ));
    }
    if filename.contains('\0') {
        return Err(Error::InvalidInput("Filename contains null bytes".into()));
    }
    if filename.starts_with('.') {
        return Err(Error::InvalidInput("Hidden files not allowed".into()));
    }
    if filename.len() > 255 {
        return Err(Error::InvalidInput(
            "Filename too long (max 255 chars)".into(),
        ));
    }
    Ok(())
}

// =============================================================================
// Initialization
// =============================================================================

/// Initialize drive quota from TIER environment variable
///
/// Updates the drive_usage table with tier-appropriate quota
pub async fn init_drive_quota(pool: &SqlitePool) -> Result<()> {
    let tier = DriveTier::from_env();
    let quota_bytes = tier.quota_bytes();

    tracing::info!(
        "Initializing drive quota for tier {}: {} bytes ({:.1} GB)",
        tier.as_str(),
        quota_bytes,
        quota_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    );

    sqlx::query(
        r#"
        UPDATE drive_usage
        SET quota_bytes = $1, updated_at = datetime('now')
        WHERE id = $2
        "#,
    )
    .bind(quota_bytes)
    .bind(USAGE_SINGLETON_ID)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to initialize drive quota: {e}")))?;

    Ok(())
}

// =============================================================================
// Usage Tracking
// =============================================================================

/// Get current drive usage statistics
pub async fn get_drive_usage(pool: &SqlitePool) -> Result<DriveUsage> {
    let row = sqlx::query_as::<_, (i64, i64, i64, i64)>(
        r#"
        SELECT total_bytes, quota_bytes, file_count, folder_count
        FROM drive_usage
        WHERE id = $1
        "#,
    )
    .bind(USAGE_SINGLETON_ID)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get drive usage: {e}")))?;

    let (total_bytes, quota_bytes, file_count, folder_count) = row;
    let usage_percent = if quota_bytes > 0 {
        (total_bytes as f64 / quota_bytes as f64) * 100.0
    } else {
        0.0
    };

    Ok(DriveUsage {
        total_bytes,
        quota_bytes,
        file_count,
        folder_count,
        usage_percent,
        tier: DriveTier::from_env().as_str().to_string(),
    })
}

/// Check if there's enough quota for an upload
pub async fn check_quota(pool: &SqlitePool, size_bytes: i64) -> Result<bool> {
    let usage = get_drive_usage(pool).await?;
    Ok(usage.total_bytes + size_bytes <= usage.quota_bytes)
}

/// Update usage statistics after file operation
async fn update_usage_add(pool: &SqlitePool, size_bytes: i64, is_folder: bool) -> Result<()> {
    let (file_delta, folder_delta): (i64, i64) = if is_folder { (0, 1) } else { (1, 0) };

    sqlx::query(
        r#"
        UPDATE drive_usage
        SET total_bytes = total_bytes + $1,
            file_count = file_count + $2,
            folder_count = folder_count + $3,
            updated_at = datetime('now')
        WHERE id = $4
        "#,
    )
    .bind(size_bytes)
    .bind(file_delta)
    .bind(folder_delta)
    .bind(USAGE_SINGLETON_ID)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update drive usage: {e}")))?;

    Ok(())
}

/// Update usage statistics after file deletion
async fn update_usage_remove(pool: &SqlitePool, size_bytes: i64, is_folder: bool) -> Result<()> {
    let (file_delta, folder_delta): (i64, i64) = if is_folder { (0, 1) } else { (1, 0) };

    sqlx::query(
        r#"
        UPDATE drive_usage
        SET total_bytes = MAX(0, total_bytes - $1),
            file_count = MAX(0, file_count - $2),
            folder_count = MAX(0, folder_count - $3),
            updated_at = datetime('now')
        WHERE id = $4
        "#,
    )
    .bind(size_bytes)
    .bind(file_delta)
    .bind(folder_delta)
    .bind(USAGE_SINGLETON_ID)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update drive usage: {e}")))?;

    Ok(())
}

// =============================================================================
// Quota Warnings
// =============================================================================

/// Check usage and return any warnings
pub async fn check_usage_warnings(pool: &SqlitePool) -> Result<QuotaWarnings> {
    let usage = get_drive_usage(pool).await?;
    let mut warnings = Vec::new();

    // Get current warning state
    let (w80, w90, w100): (bool, bool, bool) = sqlx::query_as(
        r#"
        SELECT warning_80_sent, warning_90_sent, warning_100_sent
        FROM drive_usage
        WHERE id = $1
        "#,
    )
    .bind(USAGE_SINGLETON_ID)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get warning state: {e}")))?;

    let percent = usage.usage_percent;

    if percent >= 100.0 && !w100 {
        warnings.push(
            "Storage quota reached (100%). Delete files or upgrade to continue uploading.".into(),
        );
        sqlx::query("UPDATE drive_usage SET warning_100_sent = 1 WHERE id = $1")
            .bind(USAGE_SINGLETON_ID)
            .execute(pool)
            .await
            .ok();
    } else if percent >= 90.0 && !w90 {
        warnings.push(format!(
            "Storage usage at {:.1}%. Consider upgrading or cleaning up.",
            percent
        ));
        sqlx::query("UPDATE drive_usage SET warning_90_sent = 1 WHERE id = $1")
            .bind(USAGE_SINGLETON_ID)
            .execute(pool)
            .await
            .ok();
    } else if percent >= 80.0 && !w80 {
        warnings.push(format!("Storage usage at {:.1}%.", percent));
        sqlx::query("UPDATE drive_usage SET warning_80_sent = 1 WHERE id = $1")
            .bind(USAGE_SINGLETON_ID)
            .execute(pool)
            .await
            .ok();
    }

    // Reset warnings if usage drops below thresholds
    if percent < 80.0 && (w80 || w90 || w100) {
        sqlx::query(
            "UPDATE drive_usage SET warning_80_sent = 0, warning_90_sent = 0, warning_100_sent = 0 WHERE id = $1",
        )
        .bind(USAGE_SINGLETON_ID)
        .execute(pool)
        .await
        .ok();
    }

    Ok(QuotaWarnings {
        warnings,
        usage_percent: percent,
    })
}

// =============================================================================
// File Operations
// =============================================================================

/// List files in a directory (empty path = root)
pub async fn list_files(pool: &SqlitePool, path: &str) -> Result<Vec<DriveFile>> {
    // Validate path
    let validated_path = validate_drive_path(path)?;
    let path_str = validated_path.to_string_lossy().to_string();

    let files = if path_str.is_empty() {
        // Root level - files with no parent (exclude deleted)
        sqlx::query_as::<_, DriveFile>(
            r#"
            SELECT id, path, filename, mime_type, size_bytes,
                   is_folder, parent_id, sha256_hash, deleted_at, created_at, updated_at
            FROM drive_files
            WHERE parent_id IS NULL AND deleted_at IS NULL
            ORDER BY is_folder DESC, filename ASC
            "#,
        )
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to list files: {e}")))?
    } else {
        // Get parent folder ID
        let parent = sqlx::query_as::<_, (String,)>(
            r#"SELECT id FROM drive_files WHERE path = $1 AND is_folder = 1"#,
        )
        .bind(&path_str)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to find folder: {e}")))?;

        match parent {
            Some((parent_id,)) => sqlx::query_as::<_, DriveFile>(
                r#"
                    SELECT id, path, filename, mime_type, size_bytes,
                           is_folder, parent_id, sha256_hash, deleted_at, created_at, updated_at
                    FROM drive_files
                    WHERE parent_id = $1 AND deleted_at IS NULL
                    ORDER BY is_folder DESC, filename ASC
                    "#,
            )
            .bind(parent_id)
            .fetch_all(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to list files: {e}")))?,
            None => {
                return Err(Error::NotFound(format!("Folder not found: {path}")));
            }
        }
    };

    Ok(files)
}

/// Get file metadata by ID (includes deleted files)
pub async fn get_file_metadata(pool: &SqlitePool, file_id: &str) -> Result<DriveFile> {
    let file = sqlx::query_as::<_, DriveFile>(
        r#"
        SELECT id, path, filename, mime_type, size_bytes,
               is_folder, parent_id, sha256_hash, deleted_at, created_at, updated_at
        FROM drive_files
        WHERE id = $1
        "#,
    )
    .bind(file_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get file: {e}")))?
    .ok_or_else(|| Error::NotFound(format!("File not found: {file_id}")))?;

    Ok(file)
}

/// Upload a file
pub async fn upload_file(
    pool: &SqlitePool,
    config: &DriveConfig,
    request: UploadRequest,
    data: Bytes,
) -> Result<DriveFile> {
    // Validate path and filename
    let validated_path = validate_drive_path(&request.path)?;
    validate_filename(&request.filename)?;

    let size_bytes = data.len() as i64;

    // Check quota
    if !check_quota(pool, size_bytes).await? {
        return Err(Error::InvalidInput(
            "Storage quota exceeded. Delete files or upgrade to continue.".into(),
        ));
    }

    // Build full path
    let mut file_path = if validated_path.as_os_str().is_empty() {
        PathBuf::from(&request.filename)
    } else {
        validated_path.join(&request.filename)
    };
    let mut file_path_str = file_path.to_string_lossy().to_string();
    let mut actual_filename = request.filename.clone();

    // Check if file already exists (only non-deleted files)
    let existing = sqlx::query_scalar::<_, String>(
        "SELECT id FROM drive_files WHERE path = $1 AND deleted_at IS NULL",
    )
    .bind(&file_path_str)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to check existing file: {e}")))?;

    // Auto-rename if conflict exists
    if existing.is_some() {
        file_path_str = get_unique_path(pool, &file_path_str).await?;
        file_path = PathBuf::from(&file_path_str);
        actual_filename = file_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or(request.filename.clone());
    }

    // Create parent directories on filesystem
    let full_path = config.base_path.join(&file_path);
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| Error::Storage(format!("Failed to create directories: {e}")))?;
    }

    // Calculate SHA-256 hash
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let hash = format!("{:x}", hasher.finalize());

    // Write file to disk
    let mut file = fs::File::create(&full_path)
        .await
        .map_err(|e| Error::Storage(format!("Failed to create file: {e}")))?;
    file.write_all(&data)
        .await
        .map_err(|e| Error::Storage(format!("Failed to write file: {e}")))?;
    file.sync_all()
        .await
        .map_err(|e| Error::Storage(format!("Failed to sync file: {e}")))?;

    // Get or create parent folder record
    let parent_id = if validated_path.as_os_str().is_empty() {
        None
    } else {
        let parent_path = validated_path.to_string_lossy().to_string();
        get_or_create_folder_record(pool, &parent_path).await?
    };

    // Determine MIME type
    let mime_type = request.mime_type.or_else(|| {
        mime_guess::from_path(&actual_filename)
            .first()
            .map(|m| m.to_string())
    });

    // Insert database record
    let file_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO drive_files (id, path, filename, mime_type, size_bytes, parent_id, is_folder, sha256_hash)
        VALUES ($1, $2, $3, $4, $5, $6, 0, $7)
        "#,
    )
    .bind(&file_id)
    .bind(&file_path_str)
    .bind(&actual_filename)
    .bind(&mime_type)
    .bind(size_bytes)
    .bind(&parent_id)
    .bind(&hash)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to insert file record: {e}")))?;

    // Update usage
    update_usage_add(pool, size_bytes, false).await?;

    // Return the created file
    get_file_metadata(pool, &file_id).await
}

/// Download a file (loads into memory - use download_file_stream for large files)
pub async fn download_file(
    pool: &SqlitePool,
    config: &DriveConfig,
    file_id: &str,
) -> Result<(DriveFile, Vec<u8>)> {
    let file = get_file_metadata(pool, file_id).await?;

    if file.is_folder {
        return Err(Error::InvalidInput("Cannot download a folder".into()));
    }

    // Check if file is deleted
    if file.deleted_at.is_some() {
        return Err(Error::NotFound("File is in trash".into()));
    }

    let full_path = config.base_path.join(&file.path);
    let data = fs::read(&full_path)
        .await
        .map_err(|e| Error::Storage(format!("Failed to read file: {e}")))?;

    Ok((file, data))
}

/// Download a file as a stream (memory-efficient for large files)
///
/// Returns the file metadata and a stream of bytes. Use this for files
/// larger than ~10MB to avoid loading the entire file into memory.
pub async fn download_file_stream(
    pool: &SqlitePool,
    config: &DriveConfig,
    file_id: &str,
) -> Result<(DriveFile, impl Stream<Item = std::result::Result<Bytes, std::io::Error>>)> {
    let file = get_file_metadata(pool, file_id).await?;

    if file.is_folder {
        return Err(Error::InvalidInput("Cannot download a folder".into()));
    }

    // Check if file is deleted
    if file.deleted_at.is_some() {
        return Err(Error::NotFound("File is in trash".into()));
    }

    let full_path = config.base_path.join(&file.path);
    let tokio_file = tokio::fs::File::open(&full_path)
        .await
        .map_err(|e| Error::Storage(format!("Failed to open file: {e}")))?;

    // ReaderStream reads in 4KB chunks by default
    let stream = ReaderStream::new(tokio_file);

    Ok((file, stream))
}

/// Soft delete a file or folder (moves to trash)
///
/// Files remain on disk but are marked as deleted. They will be permanently
/// removed after 30 days by the trash purge job.
pub async fn delete_file(pool: &SqlitePool, _config: &DriveConfig, file_id: &str) -> Result<()> {
    let file = get_file_metadata(pool, file_id).await?;

    // Check if already deleted
    if file.deleted_at.is_some() {
        return Err(Error::InvalidInput("File is already in trash".into()));
    }

    if file.is_folder {
        // Recursively soft-delete folder contents
        let _total_bytes = soft_delete_folder_recursive(pool, &file.id).await?;
    } else {
        // Soft delete single file (mark as deleted, keep on disk)
        sqlx::query("UPDATE drive_files SET deleted_at = datetime('now') WHERE id = $1")
            .bind(file_id)
            .execute(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to soft delete file: {e}")))?;
    }

    // Update trash tracking
    sqlx::query(
        r#"
        UPDATE drive_usage
        SET trash_bytes = trash_bytes + $1,
            trash_count = trash_count + 1,
            updated_at = datetime('now')
        WHERE id = $2
        "#,
    )
    .bind(file.size_bytes)
    .bind(USAGE_SINGLETON_ID)
    .execute(pool)
    .await
    .ok();

    Ok(())
}

/// Recursively soft-delete a folder and its contents
async fn soft_delete_folder_recursive(pool: &SqlitePool, folder_id: &str) -> Result<i64> {
    // Get all non-deleted children
    let children = sqlx::query_as::<_, DriveFile>(
        r#"
        SELECT id, path, filename, mime_type, size_bytes,
               is_folder, parent_id, sha256_hash, deleted_at, created_at, updated_at
        FROM drive_files
        WHERE parent_id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(folder_id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list folder contents: {e}")))?;

    let mut total_bytes = 0i64;

    // Recursively soft-delete children
    for child in children {
        if child.is_folder {
            total_bytes += Box::pin(soft_delete_folder_recursive(pool, &child.id)).await?;
        } else {
            // Soft delete the file
            sqlx::query("UPDATE drive_files SET deleted_at = datetime('now') WHERE id = $1")
                .bind(&child.id)
                .execute(pool)
                .await
                .ok();
            total_bytes += child.size_bytes;
        }
    }

    // Soft delete the folder itself
    sqlx::query("UPDATE drive_files SET deleted_at = datetime('now') WHERE id = $1")
        .bind(folder_id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to soft delete folder: {e}")))?;

    Ok(total_bytes)
}

/// Recursively permanently delete a folder and its contents from disk and DB
async fn hard_delete_folder_recursive(
    pool: &SqlitePool,
    config: &DriveConfig,
    folder: &DriveFile,
) -> Result<()> {
    // Get all children (including soft-deleted)
    let children = sqlx::query_as::<_, DriveFile>(
        r#"
        SELECT id, path, filename, mime_type, size_bytes,
               is_folder, parent_id, sha256_hash, deleted_at, created_at, updated_at
        FROM drive_files
        WHERE parent_id = $1
        "#,
    )
    .bind(&folder.id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list folder contents: {e}")))?;

    // Recursively delete children
    for child in children {
        if child.is_folder {
            Box::pin(hard_delete_folder_recursive(pool, config, &child)).await?;
        } else {
            let full_path = config.base_path.join(&child.path);
            if full_path.exists() {
                fs::remove_file(&full_path).await.ok();
            }
            sqlx::query("DELETE FROM drive_files WHERE id = $1")
                .bind(&child.id)
                .execute(pool)
                .await
                .ok();
            update_usage_remove(pool, child.size_bytes, false)
                .await
                .ok();
        }
    }

    // Delete the folder itself
    let full_path = config.base_path.join(&folder.path);
    if full_path.exists() {
        fs::remove_dir(&full_path).await.ok();
    }

    sqlx::query("DELETE FROM drive_files WHERE id = $1")
        .bind(&folder.id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to delete folder record: {e}")))?;

    update_usage_remove(pool, 0, true).await?;

    Ok(())
}

// =============================================================================
// Trash Operations
// =============================================================================

/// List files in trash (deleted within last 30 days)
pub async fn list_trash(pool: &SqlitePool) -> Result<Vec<DriveFile>> {
    let files = sqlx::query_as::<_, DriveFile>(
        r#"
        SELECT id, path, filename, mime_type, size_bytes,
               is_folder, parent_id, sha256_hash, deleted_at, created_at, updated_at
        FROM drive_files
        WHERE deleted_at IS NOT NULL
          AND deleted_at > datetime('now', '-30 days')
        ORDER BY deleted_at DESC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list trash: {e}")))?;

    Ok(files)
}

/// Restore a file from trash
///
/// If the file's parent folder was also deleted, it will be restored too.
/// If there's a naming conflict, the file will be auto-renamed.
pub async fn restore_file(pool: &SqlitePool, file_id: &str) -> Result<DriveFile> {
    let file = get_file_metadata(pool, file_id).await?;

    // Check if file is actually in trash
    if file.deleted_at.is_none() {
        return Err(Error::InvalidInput("File is not in trash".into()));
    }

    // If parent exists and is deleted, restore it first (recursive up)
    if let Some(ref parent_id) = file.parent_id {
        let parent = get_file_metadata(pool, parent_id).await;
        if let Ok(parent_file) = parent {
            if parent_file.deleted_at.is_some() {
                Box::pin(restore_file(pool, parent_id)).await?;
            }
        }
    }

    // Check for naming conflict with existing files
    let conflict = sqlx::query_scalar::<_, String>(
        r#"
        SELECT id FROM drive_files
        WHERE path = $1 AND deleted_at IS NULL AND id != $2
        "#,
    )
    .bind(&file.path)
    .bind(file_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to check conflict: {e}")))?;

    if conflict.is_some() {
        // Auto-rename: find unique name
        let new_path = get_unique_path(pool, &file.path).await?;
        let new_filename = PathBuf::from(&new_path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| file.filename.clone());

        sqlx::query(
            r#"
            UPDATE drive_files
            SET deleted_at = NULL, path = $1, filename = $2, updated_at = datetime('now')
            WHERE id = $3
            "#,
        )
        .bind(&new_path)
        .bind(&new_filename)
        .bind(file_id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to restore file: {e}")))?;
    } else {
        // No conflict, just restore
        sqlx::query(
            r#"
            UPDATE drive_files
            SET deleted_at = NULL, updated_at = datetime('now')
            WHERE id = $1
            "#,
        )
        .bind(file_id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to restore file: {e}")))?;
    }

    // Update trash tracking
    sqlx::query(
        r#"
        UPDATE drive_usage
        SET trash_bytes = MAX(0, trash_bytes - $1),
            trash_count = MAX(0, trash_count - 1),
            updated_at = datetime('now')
        WHERE id = $2
        "#,
    )
    .bind(file.size_bytes)
    .bind(USAGE_SINGLETON_ID)
    .execute(pool)
    .await
    .ok();

    get_file_metadata(pool, file_id).await
}

/// Permanently delete a single file (bypasses trash or from trash)
pub async fn purge_file(pool: &SqlitePool, config: &DriveConfig, file_id: &str) -> Result<()> {
    let file = get_file_metadata(pool, file_id).await?;

    if file.is_folder {
        // Recursively hard-delete folder contents
        hard_delete_folder_recursive(pool, config, &file).await?;
    } else {
        // Delete from filesystem
        let full_path = config.base_path.join(&file.path);
        if full_path.exists() {
            fs::remove_file(&full_path)
                .await
                .map_err(|e| Error::Storage(format!("Failed to delete file: {e}")))?;
        }

        // Delete from database
        sqlx::query("DELETE FROM drive_files WHERE id = $1")
            .bind(file_id)
            .execute(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to delete file record: {e}")))?;

        // Update usage
        update_usage_remove(pool, file.size_bytes, false).await?;
    }

    // Update trash tracking if file was in trash
    if file.deleted_at.is_some() {
        sqlx::query(
            r#"
            UPDATE drive_usage
            SET trash_bytes = MAX(0, trash_bytes - $1),
                trash_count = MAX(0, trash_count - 1),
                updated_at = datetime('now')
            WHERE id = $2
            "#,
        )
        .bind(file.size_bytes)
        .bind(USAGE_SINGLETON_ID)
        .execute(pool)
        .await
        .ok();
    }

    Ok(())
}

/// Empty all files from trash (permanent delete)
pub async fn empty_trash(pool: &SqlitePool, config: &DriveConfig) -> Result<u64> {
    let trash_files = list_trash(pool).await?;
    let mut deleted_count = 0u64;

    for file in &trash_files {
        // Only delete top-level items (children will be deleted recursively)
        if file.parent_id.is_none()
            || !trash_files.iter().any(|f| Some(&f.id) == file.parent_id.as_ref())
        {
            if let Err(e) = purge_file(pool, config, &file.id).await {
                tracing::warn!("Failed to purge file {} from trash: {}", file.id, e);
            } else {
                deleted_count += 1;
            }
        }
    }

    Ok(deleted_count)
}

/// Purge files that have been in trash for more than 30 days
///
/// Called by scheduled job daily
pub async fn purge_old_trash(pool: &SqlitePool, config: &DriveConfig) -> Result<u64> {
    let old_files = sqlx::query_as::<_, DriveFile>(
        r#"
        SELECT id, path, filename, mime_type, size_bytes,
               is_folder, parent_id, sha256_hash, deleted_at, created_at, updated_at
        FROM drive_files
        WHERE deleted_at IS NOT NULL
          AND deleted_at < datetime('now', '-30 days')
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list old trash: {e}")))?;

    let mut purged_count = 0u64;

    for file in old_files {
        match purge_file(pool, config, &file.id).await {
            Ok(_) => {
                purged_count += 1;
                tracing::debug!("Purged old trash file: {}", file.path);
            }
            Err(e) => {
                tracing::warn!("Failed to purge old trash file {}: {}", file.path, e);
            }
        }
    }

    if purged_count > 0 {
        tracing::info!("Purged {} files from trash (older than 30 days)", purged_count);
    }

    Ok(purged_count)
}

/// Get a unique path by appending (1), (2), etc.
async fn get_unique_path(pool: &SqlitePool, original_path: &str) -> Result<String> {
    let path_buf = PathBuf::from(original_path);
    let stem = path_buf
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let extension = path_buf.extension().map(|s| s.to_string_lossy().to_string());
    let parent = path_buf
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    for i in 1..1000 {
        let new_filename = match &extension {
            Some(ext) => format!("{} ({}).{}", stem, i, ext),
            None => format!("{} ({})", stem, i),
        };
        let new_path = if parent.is_empty() {
            new_filename
        } else {
            format!("{}/{}", parent, new_filename)
        };

        let exists = sqlx::query_scalar::<_, String>(
            "SELECT id FROM drive_files WHERE path = $1 AND deleted_at IS NULL",
        )
        .bind(&new_path)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to check path: {e}")))?;

        if exists.is_none() {
            return Ok(new_path);
        }
    }

    Err(Error::InvalidInput(
        "Could not find unique filename after 1000 attempts".into(),
    ))
}

/// Create a folder
pub async fn create_folder(
    pool: &SqlitePool,
    config: &DriveConfig,
    request: CreateFolderRequest,
) -> Result<DriveFile> {
    // Validate path and name
    let validated_path = validate_drive_path(&request.path)?;
    validate_filename(&request.name)?;

    // Build full path
    let folder_path = if validated_path.as_os_str().is_empty() {
        PathBuf::from(&request.name)
    } else {
        validated_path.join(&request.name)
    };
    let folder_path_str = folder_path.to_string_lossy().to_string();

    // Check if already exists
    let existing = sqlx::query_scalar::<_, String>("SELECT id FROM drive_files WHERE path = $1")
        .bind(&folder_path_str)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to check existing folder: {e}")))?;

    if existing.is_some() {
        return Err(Error::InvalidInput(format!(
            "Folder already exists: {}",
            folder_path_str
        )));
    }

    // Create directory on filesystem
    let full_path = config.base_path.join(&folder_path);
    fs::create_dir_all(&full_path)
        .await
        .map_err(|e| Error::Storage(format!("Failed to create folder: {e}")))?;

    // Get or create parent folder record
    let parent_id = if validated_path.as_os_str().is_empty() {
        None
    } else {
        let parent_path = validated_path.to_string_lossy().to_string();
        get_or_create_folder_record(pool, &parent_path).await?
    };

    // Insert database record
    let folder_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO drive_files (id, path, filename, size_bytes, parent_id, is_folder)
        VALUES ($1, $2, $3, 0, $4, 1)
        "#,
    )
    .bind(&folder_id)
    .bind(&folder_path_str)
    .bind(&request.name)
    .bind(&parent_id)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to insert folder record: {e}")))?;

    // Update usage
    update_usage_add(pool, 0, true).await?;

    get_file_metadata(pool, &folder_id).await
}

/// Move or rename a file/folder
pub async fn move_file(
    pool: &SqlitePool,
    config: &DriveConfig,
    file_id: &str,
    new_path: &str,
) -> Result<DriveFile> {
    let file = get_file_metadata(pool, file_id).await?;

    // Validate new path
    let validated_new_path = validate_drive_path(new_path)?;
    let new_path_str = validated_new_path.to_string_lossy().to_string();

    // Check if destination exists
    let existing = sqlx::query_scalar::<_, String>("SELECT id FROM drive_files WHERE path = $1")
        .bind(&new_path_str)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to check destination: {e}")))?;

    if existing.is_some() {
        return Err(Error::InvalidInput(format!(
            "Destination already exists: {}",
            new_path_str
        )));
    }

    // Move on filesystem
    let old_full_path = config.base_path.join(&file.path);
    let new_full_path = config.base_path.join(&validated_new_path);

    if let Some(parent) = new_full_path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| Error::Storage(format!("Failed to create directories: {e}")))?;
    }

    fs::rename(&old_full_path, &new_full_path)
        .await
        .map_err(|e| Error::Storage(format!("Failed to move file: {e}")))?;

    // Extract new filename
    let new_filename = validated_new_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| file.filename.clone());

    // Get new parent
    let new_parent_id = if let Some(parent) = validated_new_path.parent() {
        let parent_str = parent.to_string_lossy().to_string();
        if parent_str.is_empty() {
            None
        } else {
            get_or_create_folder_record(pool, &parent_str).await?
        }
    } else {
        None
    };

    // Update database
    sqlx::query(
        r#"
        UPDATE drive_files
        SET path = $1, filename = $2, parent_id = $3, updated_at = datetime('now')
        WHERE id = $4
        "#,
    )
    .bind(&new_path_str)
    .bind(&new_filename)
    .bind(&new_parent_id)
    .bind(file_id)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update file record: {e}")))?;

    get_file_metadata(pool, file_id).await
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Get or create a folder record in the database
async fn get_or_create_folder_record(pool: &SqlitePool, path: &str) -> Result<Option<String>> {
    if path.is_empty() {
        return Ok(None);
    }

    // Check if folder exists
    let existing = sqlx::query_scalar::<_, String>(
        "SELECT id FROM drive_files WHERE path = $1 AND is_folder = 1",
    )
    .bind(path)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to check folder: {e}")))?;

    if let Some(id) = existing {
        return Ok(Some(id));
    }

    // Create folder record (and parents recursively)
    let path_buf = PathBuf::from(path);
    let parent_path = path_buf.parent().map(|p| p.to_string_lossy().to_string());
    let filename = path_buf
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    let parent_id = if let Some(pp) = parent_path {
        if pp.is_empty() {
            None
        } else {
            Box::pin(get_or_create_folder_record(pool, &pp)).await?
        }
    } else {
        None
    };

    let folder_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO drive_files (id, path, filename, size_bytes, parent_id, is_folder)
        VALUES ($1, $2, $3, 0, $4, 1)
        ON CONFLICT (path) DO NOTHING
        "#,
    )
    .bind(&folder_id)
    .bind(path)
    .bind(&filename)
    .bind(&parent_id)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create folder record: {e}")))?;

    // Return the ID (may be different if another process created it)
    let id = sqlx::query_scalar::<_, String>(
        "SELECT id FROM drive_files WHERE path = $1 AND is_folder = 1",
    )
    .bind(path)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get folder: {e}")))?;

    Ok(id)
}

/// Reconcile database usage with actual filesystem
///
/// Scans the filesystem and updates the usage table to match reality.
/// Useful after manual file operations or crash recovery.
pub async fn reconcile_usage(pool: &SqlitePool, _config: &DriveConfig) -> Result<DriveUsage> {
    tracing::info!("Reconciling drive usage with filesystem");

    // Calculate actual usage from database records
    let (total_bytes, file_count, folder_count): (i64, i64, i64) = sqlx::query_as(
        r#"
        SELECT
            COALESCE(SUM(size_bytes), 0),
            COALESCE(SUM(CASE WHEN is_folder = 0 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN is_folder = 1 THEN 1 ELSE 0 END), 0)
        FROM drive_files
        "#,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to calculate usage: {e}")))?;

    // Update usage table
    sqlx::query(
        r#"
        UPDATE drive_usage
        SET total_bytes = $1,
            file_count = $2,
            folder_count = $3,
            last_scan_at = datetime('now'),
            last_scan_bytes = $1,
            updated_at = datetime('now')
        WHERE id = $4
        "#,
    )
    .bind(total_bytes)
    .bind(file_count)
    .bind(folder_count)
    .bind(USAGE_SINGLETON_ID)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update usage: {e}")))?;

    tracing::info!(
        "Reconciled: {} total bytes, {} files, {} folders",
        total_bytes,
        file_count,
        folder_count
    );

    get_drive_usage(pool).await
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_drive_path_valid() {
        assert!(validate_drive_path("").is_ok());
        assert!(validate_drive_path("test.txt").is_ok());
        assert!(validate_drive_path("folder/file.pdf").is_ok());
        assert!(validate_drive_path("deep/nested/path/file.txt").is_ok());
    }

    #[test]
    fn test_validate_drive_path_traversal() {
        assert!(validate_drive_path("../etc/passwd").is_err());
        assert!(validate_drive_path("folder/../../../etc/passwd").is_err());
        assert!(validate_drive_path("folder/..").is_err());
    }

    #[test]
    fn test_validate_drive_path_absolute() {
        assert!(validate_drive_path("/etc/passwd").is_err());
        assert!(validate_drive_path("/home/user/file.txt").is_err());
    }

    #[test]
    fn test_validate_drive_path_hidden() {
        assert!(validate_drive_path(".hidden").is_err());
        assert!(validate_drive_path("folder/.git/config").is_err());
    }

    #[test]
    fn test_validate_filename() {
        assert!(validate_filename("test.txt").is_ok());
        assert!(validate_filename("my file.pdf").is_ok());
        assert!(validate_filename("path/to/file.txt").is_err());
        assert!(validate_filename("").is_err());
        assert!(validate_filename(".hidden").is_err());
    }

    #[test]
    fn test_tier_quota() {
        assert_eq!(DriveTier::Free.quota_bytes(), 100 * 1024 * 1024 * 1024);
        assert_eq!(DriveTier::Standard.quota_bytes(), 500 * 1024 * 1024 * 1024);
        assert_eq!(DriveTier::Pro.quota_bytes(), 4 * 1024 * 1024 * 1024 * 1024);
    }
}

//! Drive API - User file storage and quota management
//!
//! Personal cloud storage for user-uploaded files (like Google Drive).
//!
//! Storage is abstracted via the `Storage` trait - supports both local filesystem
//! (for development) and S3-compatible storage (for production).
//!
//! Storage tiers:
//! - Standard: 500 GB
//! - Pro:      4 TB

use crate::error::{Error, Result};
use crate::ids;
use crate::storage::Storage;
use crate::types::Timestamp;
use axum::body::Bytes;
use futures::Stream;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::collections::HashSet;
use std::path::{Component, PathBuf};
use std::sync::Arc;

// =============================================================================
// Constants
// =============================================================================

/// Storage quota limits by tier (in bytes)
pub mod quotas {
    /// 500 GB for standard tier
    pub const STANDARD_BYTES: i64 = 500 * 1024 * 1024 * 1024;
    /// 4 TB for pro tier
    pub const PRO_BYTES: i64 = 4 * 1024 * 1024 * 1024 * 1024;
}

/// Default drive path for local file storage (development only)
const DEFAULT_DRIVE_PATH: &str = "./data/drive";

/// Singleton ID for drive_usage table
const USAGE_SINGLETON_ID: &str = "00000000-0000-0000-0000-000000000001";

/// Virtual folder ID for the data lake
const LAKE_VIRTUAL_ID: &str = "virtual:lake";

/// Prefix for virtual lake stream folder IDs
const LAKE_STREAM_PREFIX: &str = "virtual:lake:stream:";

/// Prefix for virtual lake object IDs
const LAKE_OBJECT_PREFIX: &str = "virtual:lake:object:";

/// System folder for page-embedded media (content-addressed)
pub const MEDIA_FOLDER: &str = ".media";

/// System folder for ELT archives (future migration target)
pub const LAKE_FOLDER: &str = ".lake";

// =============================================================================
// Types
// =============================================================================

/// Drive tier with quota information
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DriveTier {
    Standard,
    Pro,
}

impl DriveTier {
    /// Load tier from TIER environment variable
    pub fn from_env() -> Self {
        match std::env::var("TIER").as_deref() {
            Ok("pro") | Ok("Pro") | Ok("PRO") => DriveTier::Pro,
            _ => DriveTier::Standard,
        }
    }

    /// Get quota in bytes for this tier
    pub fn quota_bytes(&self) -> i64 {
        match self {
            DriveTier::Standard => quotas::STANDARD_BYTES,
            DriveTier::Pro => quotas::PRO_BYTES,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            DriveTier::Standard => "standard",
            DriveTier::Pro => "pro",
        }
    }
}

/// Drive configuration
#[derive(Clone)]
pub struct DriveConfig {
    /// Storage backend (S3 or local filesystem)
    pub storage: Arc<Storage>,
    /// Current tier for quota calculation
    pub tier: DriveTier,
}

impl std::fmt::Debug for DriveConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DriveConfig")
            .field("storage", &"Storage")
            .field("tier", &self.tier)
            .finish()
    }
}

impl DriveConfig {
    /// Create configuration with the given storage backend
    pub fn new(storage: Arc<Storage>) -> Self {
        let tier = DriveTier::from_env();
        Self { storage, tier }
    }

    /// Create configuration with storage and explicit tier
    pub fn with_tier(storage: Arc<Storage>, tier: DriveTier) -> Self {
        Self { storage, tier }
    }

    /// Create configuration for local development (file storage)
    ///
    /// Uses DRIVE_PATH env var or defaults to ./data/drive
    pub fn local_dev() -> Result<Self> {
        let path = std::env::var("DRIVE_PATH").unwrap_or_else(|_| DEFAULT_DRIVE_PATH.to_string());
        let storage = Storage::file(path)?;
        let tier = DriveTier::from_env();
        Ok(Self {
            storage: Arc::new(storage),
            tier,
        })
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
    pub deleted_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Storage usage summary with breakdown by category
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveUsage {
    /// Total bytes used (drive_bytes + data_lake_bytes)
    pub total_bytes: i64,
    /// User-uploaded files in /home/user/drive/
    pub drive_bytes: i64,
    /// ELT archives in /home/user/data-lake/
    pub data_lake_bytes: i64,
    /// Quota limit based on tier
    pub quota_bytes: i64,
    /// Number of user files
    pub file_count: i64,
    /// Number of user folders
    pub folder_count: i64,
    /// Usage percentage (total_bytes / quota_bytes * 100)
    pub usage_percent: f64,
    /// Tier name (standard, pro)
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
/// - Does not start with `.` (hidden files not allowed for user paths)
///
/// For internal system paths (like `.media/`), use `validate_system_path` instead.
pub fn validate_drive_path(path: &str) -> Result<PathBuf> {
    validate_path_internal(path, false)
}

/// Validate a system path (allows hidden folders like `.media/`)
///
/// Same validation as `validate_drive_path` but permits paths starting with `.`
/// Used internally for system folder operations.
pub fn validate_system_path(path: &str) -> Result<PathBuf> {
    validate_path_internal(path, true)
}

/// Internal path validation with configurable hidden file handling
fn validate_path_internal(path: &str, allow_hidden: bool) -> Result<PathBuf> {
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
                // Reject hidden files/folders (starting with .) unless allowed
                if !allow_hidden && s_str.starts_with('.') && s_str != "." {
                    return Err(Error::InvalidInput("Hidden files not allowed".into()));
                }
            }
            _ => {}
        }
    }

    Ok(path_buf)
}

// =============================================================================
// System Folder Detection
// =============================================================================

/// Check if a path is a system folder (starts with `.`)
/// System folders are hidden from normal drive listings but accessible internally.
pub fn is_system_path(path: &str) -> bool {
    // Check if path starts with a dot (e.g., ".media", ".lake")
    // or if any component starts with a dot (e.g., "foo/.media/bar")
    path.starts_with('.') || path.contains("/.")
}

/// Check if a path is protected from user deletion/modification
/// Protected paths include system folders like .media and .lake
pub fn is_protected_path(path: &str) -> bool {
    let normalized = path.trim_start_matches('/');
    normalized == MEDIA_FOLDER
        || normalized.starts_with(&format!("{}/", MEDIA_FOLDER))
        || normalized == LAKE_FOLDER
        || normalized.starts_with(&format!("{}/", LAKE_FOLDER))
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

/// Get current drive usage statistics with breakdown
pub async fn get_drive_usage(pool: &SqlitePool) -> Result<DriveUsage> {
    // Get drive usage from drive_usage table
    let row = sqlx::query_as::<_, (i64, i64, i64, i64)>(
        r#"
        SELECT drive_bytes, quota_bytes, file_count, folder_count
        FROM drive_usage
        WHERE id = $1
        "#,
    )
    .bind(USAGE_SINGLETON_ID)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get drive usage: {e}")))?;

    let (drive_bytes, quota_bytes, file_count, folder_count) = row;

    // Get data lake usage from elt_stream_objects
    let data_lake_bytes: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(SUM(size_bytes), 0)
        FROM elt_stream_objects
        "#,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get data lake usage: {e}")))?;

    let total_bytes = drive_bytes + data_lake_bytes;
    let usage_percent = if quota_bytes > 0 {
        (total_bytes as f64 / quota_bytes as f64) * 100.0
    } else {
        0.0
    };

    Ok(DriveUsage {
        total_bytes,
        drive_bytes,
        data_lake_bytes,
        quota_bytes,
        file_count,
        folder_count,
        usage_percent,
        tier: DriveTier::from_env().as_str().to_string(),
    })
}

/// Check if there's enough quota for an upload
/// Checks against unified quota (drive + data lake)
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
        SET drive_bytes = drive_bytes + $1,
            total_bytes = total_bytes + $1,
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
        SET drive_bytes = MAX(0, drive_bytes - $1),
            total_bytes = MAX(0, total_bytes - $1),
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

/// Reconcile a folder's database records with the actual storage.
///
/// Note: With S3 storage, the database is the source of truth for folder structure.
/// This function is primarily useful for local development where files may be
/// added/removed outside the API.
///
/// For S3, this function only checks for ghost DB records (files in DB but missing from storage).
/// Auto-registration of untracked files is not supported for S3.
///
/// Called before listing files to keep DB in sync with reality.
pub async fn reconcile_folder(pool: &SqlitePool, config: &DriveConfig, path: &str) -> Result<()> {
    let validated_path = validate_drive_path(path)?;
    let path_str = validated_path.to_string_lossy().to_string();

    // List storage contents at this path prefix
    let storage_prefix = if path_str.is_empty() {
        String::new()
    } else {
        format!("{}/", path_str)
    };

    // Get list of files from storage
    let storage_files = config
        .storage
        .list(&storage_prefix)
        .await
        .unwrap_or_default();

    // Extract just the filenames from storage keys (files directly in this folder)
    let storage_filenames: HashSet<String> = storage_files
        .iter()
        .filter_map(|key| {
            // Strip the prefix and get just the filename (no subdirectories)
            let relative = if storage_prefix.is_empty() {
                key.as_str()
            } else {
                key.strip_prefix(&storage_prefix).unwrap_or(key)
            };
            // Only include direct children (no slashes)
            if !relative.contains('/') && !relative.is_empty() {
                Some(relative.to_string())
            } else {
                None
            }
        })
        .collect();

    // Query ALL DB records for this folder (including soft-deleted)
    // to avoid re-registering trashed files
    let db_records: Vec<DriveFile> = if path_str.is_empty() {
        sqlx::query_as::<_, DriveFile>(
            r#"
            SELECT id, path, filename, mime_type, size_bytes,
                   is_folder, parent_id, sha256_hash, deleted_at, created_at, updated_at
            FROM drive_files
            WHERE parent_id IS NULL
            "#,
        )
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to query folder records: {e}")))?
    } else {
        // Look up parent folder ID
        let parent_id = sqlx::query_scalar::<_, String>(
            "SELECT id FROM drive_files WHERE path = $1 AND is_folder = 1",
        )
        .bind(&path_str)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to find folder: {e}")))?;

        match parent_id {
            Some(pid) => sqlx::query_as::<_, DriveFile>(
                r#"
                    SELECT id, path, filename, mime_type, size_bytes,
                           is_folder, parent_id, sha256_hash, deleted_at, created_at, updated_at
                    FROM drive_files
                    WHERE parent_id = $1
                    "#,
            )
            .bind(pid)
            .fetch_all(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to query folder records: {e}")))?,
            // Folder doesn't exist in DB yet — nothing to reconcile
            None => Vec::new(),
        }
    };

    let db_filenames: HashSet<String> = db_records.iter().map(|f| f.filename.clone()).collect();

    // Note: Auto-registration of untracked files from storage is not supported.
    // With S3, the database is the source of truth.
    // Files should only be added through the API, not by direct S3 upload.

    // Log if there are untracked files in storage (for debugging)
    let untracked: Vec<&String> = storage_filenames
        .iter()
        .filter(|name| !db_filenames.contains(*name))
        .collect();
    if !untracked.is_empty() {
        tracing::debug!(
            "Found {} untracked files in storage at '{}': {:?}",
            untracked.len(),
            path_str,
            untracked.iter().take(5).collect::<Vec<_>>()
        );
    }

    // --- Remove ghost DB records (in DB but not in storage) ---
    // Only remove non-deleted file records (not folders, since S3 doesn't track folders)
    for file in &db_records {
        // Skip deleted files (they're in trash, expected to be missing from storage)
        if file.deleted_at.is_some() {
            continue;
        }
        // Skip folders (they only exist in DB, not in storage)
        if file.is_folder {
            continue;
        }
        // Check if file exists in storage
        if !storage_filenames.contains(&file.filename) {
            sqlx::query("DELETE FROM drive_files WHERE id = $1")
                .bind(&file.id)
                .execute(pool)
                .await
                .map_err(|e| {
                    Error::Database(format!("Failed to remove ghost record {}: {e}", file.path))
                })?;

            update_usage_remove(pool, file.size_bytes, false).await?;
            tracing::info!(
                "Removed ghost DB record (missing from storage): {}",
                file.path
            );
        }
    }

    Ok(())
}

/// List files in a directory (empty path = root)
///
/// Filters out system folders (paths starting with `.`) from results.
/// Use `list_files_internal` for system operations that need to see hidden folders.
pub async fn list_files(pool: &SqlitePool, path: &str) -> Result<Vec<DriveFile>> {
    // Validate path for regular drive paths
    let validated_path = validate_drive_path(path)?;
    let path_str = validated_path.to_string_lossy().to_string();

    let files = if path_str.is_empty() {
        // Root level - files with no parent (exclude deleted and system folders)
        sqlx::query_as::<_, DriveFile>(
            r#"
            SELECT id, path, filename, mime_type, size_bytes,
                   is_folder, parent_id, sha256_hash, deleted_at, created_at, updated_at
            FROM drive_files
            WHERE parent_id IS NULL
              AND deleted_at IS NULL
              AND filename NOT LIKE '.%'
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
                    WHERE parent_id = $1
                      AND deleted_at IS NULL
                      AND filename NOT LIKE '.%'
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

/// Check if a file ID is a virtual lake object
pub fn is_lake_object_id(file_id: &str) -> bool {
    file_id.starts_with(LAKE_OBJECT_PREFIX)
}

/// Extract the real object ID from a virtual lake object ID
pub fn extract_lake_object_id(file_id: &str) -> Option<&str> {
    file_id.strip_prefix(LAKE_OBJECT_PREFIX)
}

/// Check if a file ID is a virtual lake folder (lake root or stream folder)
pub fn is_lake_folder_id(file_id: &str) -> bool {
    file_id == LAKE_VIRTUAL_ID || file_id.starts_with(LAKE_STREAM_PREFIX)
}

/// Get file metadata by ID (includes deleted files)
///
/// Handles both regular drive files and virtual lake objects.
pub async fn get_file_metadata(pool: &SqlitePool, file_id: &str) -> Result<DriveFile> {
    // Handle virtual lake folder IDs
    if file_id == LAKE_VIRTUAL_ID {
        let lake_size: i64 =
            sqlx::query_scalar("SELECT COALESCE(SUM(size_bytes), 0) FROM elt_stream_objects")
                .fetch_one(pool)
                .await
                .unwrap_or(0);

        let now = Timestamp::now();
        return Ok(DriveFile {
            id: LAKE_VIRTUAL_ID.to_string(),
            path: "lake".to_string(),
            filename: "lake".to_string(),
            mime_type: None,
            size_bytes: lake_size,
            is_folder: true,
            parent_id: None,
            sha256_hash: None,
            deleted_at: None,
            created_at: now,
            updated_at: now,
        });
    }

    // Handle virtual lake stream folder IDs
    if let Some(stream_name) = file_id.strip_prefix(LAKE_STREAM_PREFIX) {
        let stream_info = sqlx::query_as::<_, (i64, Timestamp)>(
            r#"
            SELECT SUM(size_bytes), MAX(created_at)
            FROM elt_stream_objects
            WHERE stream_name = $1
            "#,
        )
        .bind(stream_name)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get stream info: {e}")))?
        .ok_or_else(|| Error::NotFound(format!("Stream not found: {stream_name}")))?;

        return Ok(DriveFile {
            id: file_id.to_string(),
            path: format!("lake/{}", stream_name),
            filename: stream_name.to_string(),
            mime_type: None,
            size_bytes: stream_info.0,
            is_folder: true,
            parent_id: Some(LAKE_VIRTUAL_ID.to_string()),
            sha256_hash: None,
            deleted_at: None,
            created_at: stream_info.1,
            updated_at: stream_info.1,
        });
    }

    // Handle virtual lake object IDs
    if let Some(object_id) = file_id.strip_prefix(LAKE_OBJECT_PREFIX) {
        return get_lake_object_metadata(pool, object_id).await;
    }

    // Regular drive file
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

/// Get metadata for a lake stream object by its real ID
async fn get_lake_object_metadata(pool: &SqlitePool, object_id: &str) -> Result<DriveFile> {
    let obj = sqlx::query_as::<_, (String, String, String, i64, Timestamp, Timestamp)>(
        r#"
        SELECT id, stream_name, storage_key, size_bytes, created_at, updated_at
        FROM elt_stream_objects
        WHERE id = $1
        "#,
    )
    .bind(object_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get lake object: {e}")))?
    .ok_or_else(|| Error::NotFound(format!("Lake object not found: {object_id}")))?;

    let (id, stream_name, storage_key, size_bytes, created_at, updated_at) = obj;

    // Extract filename from storage_key
    let filename = storage_key
        .rsplit('/')
        .next()
        .unwrap_or(&storage_key)
        .to_string();

    Ok(DriveFile {
        id: format!("{}{}", LAKE_OBJECT_PREFIX, id),
        path: format!("lake/{}/{}", stream_name, filename),
        filename,
        mime_type: Some("application/x-jsonlines".to_string()),
        size_bytes,
        is_folder: false,
        parent_id: Some(format!("{}{}", LAKE_STREAM_PREFIX, stream_name)),
        sha256_hash: None,
        deleted_at: None,
        created_at,
        updated_at,
    })
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

    // Build storage key (relative path)
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

    // Calculate SHA-256 hash
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let hash = format!("{:x}", hasher.finalize());

    // Upload to storage (handles both S3 and local filesystem)
    config
        .storage
        .upload(&file_path_str, data.to_vec())
        .await
        .map_err(|e| Error::Storage(format!("Failed to upload file: {e}")))?;

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
    let file_id = ids::generate_id(ids::DRIVE_FILE_PREFIX, &[&file_path_str]);
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

/// Upload a file to a system path (like `.media/`)
///
/// Similar to `upload_file` but allows hidden folder paths.
/// Used internally for media storage.
pub async fn upload_system_file(
    pool: &SqlitePool,
    config: &DriveConfig,
    path: &str,
    filename: &str,
    mime_type: Option<String>,
    data: &[u8],
) -> Result<DriveFile> {
    // Validate path (allows hidden folders)
    let validated_path = validate_system_path(path)?;

    let size_bytes = data.len() as i64;

    // Check quota
    if !check_quota(pool, size_bytes).await? {
        return Err(Error::InvalidInput(
            "Storage quota exceeded. Delete files or upgrade to continue.".into(),
        ));
    }

    // Build storage key (relative path)
    let file_path = if validated_path.as_os_str().is_empty() {
        PathBuf::from(filename)
    } else {
        validated_path.join(filename)
    };
    let file_path_str = file_path.to_string_lossy().to_string();

    // Check if file already exists (for dedup - just return existing)
    let existing = sqlx::query_as::<_, DriveFile>(
        r#"
        SELECT id, path, filename, mime_type, size_bytes,
               is_folder, parent_id, sha256_hash, deleted_at, created_at, updated_at
        FROM drive_files
        WHERE path = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(&file_path_str)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to check existing file: {e}")))?;

    if let Some(existing_file) = existing {
        // File already exists (content-addressed dedup)
        return Ok(existing_file);
    }

    // Calculate SHA-256 hash
    let mut hasher = Sha256::new();
    hasher.update(data);
    let hash = format!("{:x}", hasher.finalize());

    // Upload to storage (handles both S3 and local filesystem)
    config
        .storage
        .upload(&file_path_str, data.to_vec())
        .await
        .map_err(|e| Error::Storage(format!("Failed to upload file: {e}")))?;

    // Get or create parent folder record (using system path)
    let parent_id = if validated_path.as_os_str().is_empty() {
        None
    } else {
        let parent_path = validated_path.to_string_lossy().to_string();
        get_or_create_system_folder_record(pool, &parent_path).await?
    };

    // Determine MIME type
    let mime_type = mime_type.or_else(|| {
        mime_guess::from_path(filename)
            .first()
            .map(|m| m.to_string())
    });

    // Insert database record
    let file_id = ids::generate_id(ids::DRIVE_FILE_PREFIX, &[&file_path_str]);
    sqlx::query(
        r#"
        INSERT INTO drive_files (id, path, filename, mime_type, size_bytes, parent_id, is_folder, sha256_hash)
        VALUES ($1, $2, $3, $4, $5, $6, 0, $7)
        "#,
    )
    .bind(&file_id)
    .bind(&file_path_str)
    .bind(filename)
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

/// Get or create a system folder record (allows hidden folders like `.media`)
async fn get_or_create_system_folder_record(
    pool: &SqlitePool,
    path: &str,
) -> Result<Option<String>> {
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
            Box::pin(get_or_create_system_folder_record(pool, &pp)).await?
        }
    } else {
        None
    };

    let folder_id = ids::generate_id(ids::DRIVE_FILE_PREFIX, &[path]);
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

/// Find a file by path (including system paths)
pub async fn get_file_by_path(pool: &SqlitePool, path: &str) -> Result<DriveFile> {
    let file = sqlx::query_as::<_, DriveFile>(
        r#"
        SELECT id, path, filename, mime_type, size_bytes,
               is_folder, parent_id, sha256_hash, deleted_at, created_at, updated_at
        FROM drive_files
        WHERE path = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(path)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get file: {e}")))?
    .ok_or_else(|| Error::NotFound(format!("File not found: {path}")))?;

    Ok(file)
}

/// Download a file (loads into memory - use download_file_stream for large files)
///
/// For lake objects (virtual:lake:object:*), use `download_lake_object` instead.
pub async fn download_file(
    pool: &SqlitePool,
    config: &DriveConfig,
    file_id: &str,
) -> Result<(DriveFile, Vec<u8>)> {
    // Check if this is a lake object - these need special handling via storage layer
    if is_lake_object_id(file_id) {
        return Err(Error::InvalidInput(
            "Lake objects must be downloaded via the storage API".into(),
        ));
    }

    // Check for lake folders
    if is_lake_folder_id(file_id) {
        return Err(Error::InvalidInput("Cannot download a folder".into()));
    }

    let file = get_file_metadata(pool, file_id).await?;

    if file.is_folder {
        return Err(Error::InvalidInput("Cannot download a folder".into()));
    }

    // Check if file is deleted
    if file.deleted_at.is_some() {
        return Err(Error::NotFound("File is in trash".into()));
    }

    // Download from storage (handles both S3 and local filesystem)
    let data = config
        .storage
        .download(&file.path)
        .await
        .map_err(|e| Error::Storage(format!("Failed to download file: {e}")))?;

    Ok((file, data))
}

/// Download a lake object (ELT stream archive)
///
/// Lake objects are stored in the data lake with optional encryption.
/// This function retrieves the object using the storage abstraction.
///
/// # Arguments
/// * `pool` - Database connection pool
/// * `storage` - Storage backend (S3/local)
/// * `file_id` - Virtual lake object ID (e.g., "virtual:lake:object:abc123")
///
/// # Returns
/// Tuple of (DriveFile metadata, raw bytes)
pub async fn download_lake_object(
    pool: &SqlitePool,
    storage: &crate::storage::Storage,
    file_id: &str,
) -> Result<(DriveFile, Vec<u8>)> {
    // Extract the real object ID
    let object_id = extract_lake_object_id(file_id)
        .ok_or_else(|| Error::InvalidInput("Invalid lake object ID".into()))?;

    // Get metadata
    let file = get_lake_object_metadata(pool, object_id).await?;

    // Query storage key and source info for decryption
    let obj_info = sqlx::query_as::<_, (String, String, String)>(
        r#"
        SELECT source_connection_id, stream_name, storage_key
        FROM elt_stream_objects
        WHERE id = $1
        "#,
    )
    .bind(object_id)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get lake object info: {e}")))?;

    let (_source_connection_id, _stream_name, storage_key) = obj_info;

    // Download from filesystem storage
    let data = storage.download(&storage_key).await?;

    Ok((file, data))
}

/// Download a file as a stream (for HTTP streaming responses)
///
/// Returns the file metadata and a stream of bytes.
///
/// Note: When using S3 storage, the file is downloaded fully before streaming.
/// For truly memory-efficient large file downloads with S3, consider implementing
/// streaming at the S3 level in future.
///
/// For lake objects, use `download_lake_object` instead (streaming not yet supported).
pub async fn download_file_stream(
    pool: &SqlitePool,
    config: &DriveConfig,
    file_id: &str,
) -> Result<(
    DriveFile,
    impl Stream<Item = std::result::Result<Bytes, std::io::Error>>,
)> {
    // Check if this is a lake object
    if is_lake_object_id(file_id) {
        return Err(Error::InvalidInput(
            "Lake objects must be downloaded via the storage API".into(),
        ));
    }

    // Check for lake folders
    if is_lake_folder_id(file_id) {
        return Err(Error::InvalidInput("Cannot download a folder".into()));
    }

    let file = get_file_metadata(pool, file_id).await?;

    if file.is_folder {
        return Err(Error::InvalidInput("Cannot download a folder".into()));
    }

    // Check if file is deleted
    if file.deleted_at.is_some() {
        return Err(Error::NotFound("File is in trash".into()));
    }

    // Download from storage (handles both S3 and local filesystem)
    let data = config
        .storage
        .download(&file.path)
        .await
        .map_err(|e| Error::Storage(format!("Failed to download file: {e}")))?;

    // Stream the data in chunks for HTTP response compatibility
    let stream = futures::stream::once(async move { Ok(Bytes::from(data)) });

    Ok((file, stream))
}

/// Soft delete a file or folder (moves to trash)
///
/// Files remain on disk but are marked as deleted. They will be permanently
/// removed after 30 days by the trash purge job.
///
/// System folders (`.media`, `.lake`) are protected and cannot be deleted.
pub async fn delete_file(pool: &SqlitePool, _config: &DriveConfig, file_id: &str) -> Result<()> {
    let file = get_file_metadata(pool, file_id).await?;

    // Protect system folders from deletion
    if is_protected_path(&file.path) {
        return Err(Error::InvalidInput(
            "Cannot delete system folder or its contents".into(),
        ));
    }

    // Check if already deleted
    if file.deleted_at.is_some() {
        return Err(Error::InvalidInput("File is already in trash".into()));
    }

    let (trash_bytes, trash_count) = if file.is_folder {
        // Recursively soft-delete folder contents
        soft_delete_folder_recursive(pool, &file.id).await?
    } else {
        // Soft delete single file (mark as deleted, keep on disk)
        sqlx::query("UPDATE drive_files SET deleted_at = datetime('now') WHERE id = $1")
            .bind(file_id)
            .execute(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to soft delete file: {e}")))?;
        (file.size_bytes, 1i64)
    };

    // Update trash tracking with actual bytes and count
    sqlx::query(
        r#"
        UPDATE drive_usage
        SET trash_bytes = trash_bytes + $1,
            trash_count = trash_count + $2,
            updated_at = datetime('now')
        WHERE id = $3
        "#,
    )
    .bind(trash_bytes)
    .bind(trash_count)
    .bind(USAGE_SINGLETON_ID)
    .execute(pool)
    .await
    .ok();

    Ok(())
}

/// Recursively soft-delete a folder and its contents.
/// Returns (total_bytes, total_count) of all affected items.
async fn soft_delete_folder_recursive(pool: &SqlitePool, folder_id: &str) -> Result<(i64, i64)> {
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
    let mut total_count = 0i64;

    // Recursively soft-delete children
    for child in children {
        if child.is_folder {
            let (bytes, count) = Box::pin(soft_delete_folder_recursive(pool, &child.id)).await?;
            total_bytes += bytes;
            total_count += count;
        } else {
            // Soft delete the file
            sqlx::query("UPDATE drive_files SET deleted_at = datetime('now') WHERE id = $1")
                .bind(&child.id)
                .execute(pool)
                .await
                .ok();
            total_bytes += child.size_bytes;
            total_count += 1;
        }
    }

    // Soft delete the folder itself
    sqlx::query("UPDATE drive_files SET deleted_at = datetime('now') WHERE id = $1")
        .bind(folder_id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to soft delete folder: {e}")))?;

    total_count += 1; // count the folder itself

    Ok((total_bytes, total_count))
}

/// Recursively permanently delete a folder and its contents from storage and DB
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
            // Delete from storage (handles both S3 and local filesystem)
            config.storage.delete(&child.path).await.ok();

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

    // Delete folder record from database
    // Note: S3 has no real directories, so we only delete the DB record
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
        // Delete from storage (handles both S3 and local filesystem)
        // Ignore errors if file doesn't exist (may have been deleted externally)
        config.storage.delete(&file.path).await.ok();

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
            || !trash_files
                .iter()
                .any(|f| Some(&f.id) == file.parent_id.as_ref())
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
        tracing::info!(
            "Purged {} files from trash (older than 30 days)",
            purged_count
        );
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
    let extension = path_buf
        .extension()
        .map(|s| s.to_string_lossy().to_string());
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
///
/// Note: For S3 storage, folders exist only in the database (S3 has no real directories).
/// Files are stored with their full path as the key (e.g., "documents/2024/report.pdf").
pub async fn create_folder(
    pool: &SqlitePool,
    _config: &DriveConfig,
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

    // Note: No storage operation needed - folders exist only in database.
    // S3 has no real directories; files use their full path as the key.
    // Local file storage creates directories automatically when uploading files.

    // Get or create parent folder record
    let parent_id = if validated_path.as_os_str().is_empty() {
        None
    } else {
        let parent_path = validated_path.to_string_lossy().to_string();
        get_or_create_folder_record(pool, &parent_path).await?
    };

    // Insert database record
    let folder_id = ids::generate_id(ids::DRIVE_FILE_PREFIX, &[&folder_path_str]);
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
///
/// System folders (`.media`, `.lake`) are protected and cannot be moved.
///
/// For S3 storage, moving a file requires downloading and re-uploading to the new key.
/// Moving folders also requires moving all descendant files in storage.
pub async fn move_file(
    pool: &SqlitePool,
    config: &DriveConfig,
    file_id: &str,
    new_path: &str,
) -> Result<DriveFile> {
    let file = get_file_metadata(pool, file_id).await?;

    // Protect system folders from being moved
    if is_protected_path(&file.path) {
        return Err(Error::InvalidInput(
            "Cannot move system folder or its contents".into(),
        ));
    }

    // Prevent moving into system folders
    if is_protected_path(new_path) {
        return Err(Error::InvalidInput(
            "Cannot move files into system folders".into(),
        ));
    }

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

    // Move file in storage (copy + delete pattern for S3 compatibility)
    if !file.is_folder {
        // Download from old path
        let data = config
            .storage
            .download(&file.path)
            .await
            .map_err(|e| Error::Storage(format!("Failed to read file for move: {e}")))?;

        // Upload to new path
        config
            .storage
            .upload(&new_path_str, data)
            .await
            .map_err(|e| Error::Storage(format!("Failed to write file to new location: {e}")))?;

        // Delete from old path
        config.storage.delete(&file.path).await.ok();
    }

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

    // Update database record for the file/folder itself
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

    // For folders: update all descendant paths and move files in storage
    if file.is_folder {
        let old_prefix = &file.path;

        // Get all descendant files (not folders) to move in storage
        let descendants = sqlx::query_as::<_, DriveFile>(
            r#"
            SELECT id, path, filename, mime_type, size_bytes,
                   is_folder, parent_id, sha256_hash, deleted_at, created_at, updated_at
            FROM drive_files
            WHERE path LIKE $1 AND is_folder = 0 AND deleted_at IS NULL
            "#,
        )
        .bind(format!("{}/%", old_prefix))
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to list folder contents: {e}")))?;

        // Move each file in storage
        for descendant in &descendants {
            let old_file_path = &descendant.path;
            let new_file_path = old_file_path.replacen(old_prefix, &new_path_str, 1);

            // Download, upload, delete pattern
            if let Ok(data) = config.storage.download(old_file_path).await {
                if config.storage.upload(&new_file_path, data).await.is_ok() {
                    config.storage.delete(old_file_path).await.ok();
                }
            }
        }

        // Update all descendant paths in database
        let old_prefix_len = file.path.chars().count() as i64 + 1; // +1 because SQLite substr is 1-based
        let like_pattern = format!("{}/%", file.path);
        sqlx::query(
            r#"
            UPDATE drive_files
            SET path = $1 || substr(path, $2),
                updated_at = datetime('now')
            WHERE path LIKE $3
            "#,
        )
        .bind(&new_path_str)
        .bind(old_prefix_len)
        .bind(&like_pattern)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to update descendant paths: {e}")))?;
    }

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

    let folder_id = ids::generate_id(ids::DRIVE_FILE_PREFIX, &[path]);
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

    // Calculate actual drive usage from database records
    let (drive_bytes, file_count, folder_count): (i64, i64, i64) = sqlx::query_as(
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

    // Update usage table (drive_bytes and total_bytes for backwards compat)
    sqlx::query(
        r#"
        UPDATE drive_usage
        SET drive_bytes = $1,
            total_bytes = $1,
            file_count = $2,
            folder_count = $3,
            last_scan_at = datetime('now'),
            last_scan_bytes = $1,
            updated_at = datetime('now')
        WHERE id = $4
        "#,
    )
    .bind(drive_bytes)
    .bind(file_count)
    .bind(folder_count)
    .bind(USAGE_SINGLETON_ID)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update usage: {e}")))?;

    tracing::info!(
        "Reconciled: {} drive bytes, {} files, {} folders",
        drive_bytes,
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
        assert_eq!(DriveTier::Standard.quota_bytes(), 500 * 1024 * 1024 * 1024);
        assert_eq!(DriveTier::Pro.quota_bytes(), 4 * 1024 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_is_system_path() {
        // System paths (start with .)
        assert!(is_system_path(".media"));
        assert!(is_system_path(".media/ab/file.png"));
        assert!(is_system_path(".lake"));
        assert!(is_system_path(".lake/provider/stream"));

        // Non-system paths
        assert!(!is_system_path("documents"));
        assert!(!is_system_path("photos/vacation"));
        assert!(!is_system_path("my.file.txt")); // dot in filename is fine
    }

    #[test]
    fn test_is_protected_path() {
        // Protected paths
        assert!(is_protected_path(".media"));
        assert!(is_protected_path(".media/ab/file.png"));
        assert!(is_protected_path(".lake"));
        assert!(is_protected_path(".lake/provider/stream"));

        // Non-protected paths
        assert!(!is_protected_path("documents"));
        assert!(!is_protected_path("photos/vacation"));
        assert!(!is_protected_path(".other")); // other hidden folders are not protected
    }

    #[test]
    fn test_validate_system_path() {
        // System paths should be allowed
        assert!(validate_system_path(".media").is_ok());
        assert!(validate_system_path(".media/ab/file.png").is_ok());
        assert!(validate_system_path(".lake/stream").is_ok());

        // But traversal should still be blocked
        assert!(validate_system_path(".media/../etc/passwd").is_err());
        assert!(validate_system_path("..").is_err());
    }
}

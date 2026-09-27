//! Drive: user files, uploads and downloads, annotations, the drive trash
//! and page media.

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post, put},
    Json,
    Router,
};
use serde::Deserialize;

use super::{api_response, error_response, sanitize_content_disposition, success_message};
use crate::server::AppState;

/// This area's authenticated routes. Merged into the protected router, whose
/// `route_layer` requires a resolved `AuthUser`.
pub fn routes() -> Router<AppState> {
    Router::new()
        // Annotations API (document highlights + margin notes)
        .route(
            "/api/annotations",
            get(list_annotations_handler).post(create_annotation_handler),
        )
        .route(
            "/api/annotations/:id",
            patch(update_annotation_handler).delete(delete_annotation_handler),
        )
        // Bulk annotation export as markdown (D4.3)
        .route(
            "/api/annotations/export",
            get(export_file_annotations_handler),
        )
        // Drive API (user file storage)
        .route(
            "/api/drive/files/:id/reextract",
            post(reextract_drive_file_handler),
        )
        .route("/api/drive/usage", get(get_drive_usage_handler))
        .route("/api/backup/status", get(get_backup_status_handler))
        .route("/api/drive/files", get(list_drive_files_handler))
        .route(
            "/api/drive/files/:id",
            get(get_drive_file_handler).delete(delete_drive_file_handler),
        )
        .route(
            "/api/drive/files/:id/download",
            get(download_drive_file_handler),
        )
        .route(
            "/api/drive/files/:id/move",
            put(move_drive_file_handler),
        )
        .route("/api/drive/upload", post(upload_drive_file_handler))
        .route("/api/drive/folders", post(create_drive_folder_handler))
        // Drive trash endpoints
        .route("/api/drive/media", get(list_drive_media_handler))
        .route("/api/drive/trash", get(list_drive_trash_handler))
        .route(
            "/api/drive/trash/empty",
            post(empty_drive_trash_handler),
        )
        .route(
            "/api/drive/files/:id/restore",
            post(restore_drive_file_handler),
        )
        .route(
            "/api/drive/files/:id/purge",
            delete(purge_drive_file_handler),
        )
        // Media API (content-addressed storage for page-embedded media)
        .route("/api/media/upload", post(upload_media_handler))
        .route("/api/media/:id", get(get_media_handler))
}


// =============================================================================
// Drive API Handlers (User File Storage)
// =============================================================================

/// GET /api/drive/usage - Get drive usage statistics
pub async fn get_drive_usage_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::drive::get_drive_usage(state.db.pool(), &state.drive_config).await)
}

/// GET /api/backup/status - age of the newest good backup, per volume
pub async fn get_backup_status_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::backup_status::get_backup_status(state.db.pool()).await)
}

/// Query params for listing drive files
#[derive(Debug, Deserialize)]
pub struct ListDriveFilesQuery {
    #[serde(default = "default_drive_path")]
    pub path: String,
}

fn default_drive_path() -> String {
    String::new() // Empty string = root directory
}

/// GET /api/drive/files - List files in a directory
pub async fn list_drive_files_handler(
    State(state): State<AppState>,
    Query(params): Query<ListDriveFilesQuery>,
) -> Response {
    api_response(crate::api::drive::list_files(state.db.pool(), &params.path).await)
}

/// GET /api/drive/files/:id - Get file metadata
pub async fn get_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    api_response(crate::api::drive::get_file_metadata(state.db.pool(), &file_id).await)
}

/// GET /api/drive/files/:id/download - Download file content
/// Query parameters for drive downloads.
#[derive(Debug, Deserialize)]
pub struct DriveDownloadQuery {
    /// `inline` renders in-browser (viewer surfaces); default is attachment.
    pub disposition: Option<String>,
}

/// Outcome of resolving a Range header against an object size.
#[derive(Debug, PartialEq)]
pub(super) enum RangeOutcome {
    /// No (or ignorable) range — serve the full object with 200.
    Full,
    /// Serve `(start, len)` with 206.
    Partial(u64, u64),
    /// Range present but unsatisfiable — 416.
    Unsatisfiable,
}

/// Resolve a single-range `Range: bytes=…` header against a total size.
/// Malformed and multi-range headers are ignored (RFC 7233 permits a full 200
/// response); syntactically valid but out-of-bounds ranges are unsatisfiable.
pub(super) fn resolve_range(header: Option<&str>, total: u64) -> RangeOutcome {
    let Some(header) = header else {
        return RangeOutcome::Full;
    };
    let Some(spec) = header.trim().strip_prefix("bytes=") else {
        return RangeOutcome::Full;
    };
    if spec.contains(',') {
        return RangeOutcome::Full;
    }
    let Some((start_s, end_s)) = spec.split_once('-') else {
        return RangeOutcome::Full;
    };
    let (start_s, end_s) = (start_s.trim(), end_s.trim());
    match (start_s.is_empty(), end_s.is_empty()) {
        (true, true) => RangeOutcome::Full,
        // Suffix form: last N bytes.
        (true, false) => {
            let Ok(n) = end_s.parse::<u64>() else {
                return RangeOutcome::Full;
            };
            if n == 0 || total == 0 {
                return RangeOutcome::Unsatisfiable;
            }
            let start = total.saturating_sub(n);
            RangeOutcome::Partial(start, total - start)
        }
        // Open-ended: from start to EOF.
        (false, true) => {
            let Ok(start) = start_s.parse::<u64>() else {
                return RangeOutcome::Full;
            };
            if start >= total {
                return RangeOutcome::Unsatisfiable;
            }
            RangeOutcome::Partial(start, total - start)
        }
        // Bounded: start–end inclusive, end clamped to EOF.
        (false, false) => {
            let (Ok(start), Ok(end)) = (start_s.parse::<u64>(), end_s.parse::<u64>()) else {
                return RangeOutcome::Full;
            };
            if start > end {
                return RangeOutcome::Full;
            }
            if start >= total {
                return RangeOutcome::Unsatisfiable;
            }
            let end = end.min(total - 1);
            RangeOutcome::Partial(start, end - start + 1)
        }
    }
}

pub async fn download_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
    Query(query): Query<DriveDownloadQuery>,
    headers: axum::http::HeaderMap,
) -> Response {
    let disposition = if query.disposition.as_deref() == Some("inline") {
        "inline"
    } else {
        "attachment"
    };

    // Lake objects use in-memory download (different storage layer)
    if crate::api::drive::is_lake_object_id(&file_id) {
        let result =
            crate::api::drive::download_lake_object(state.db.pool(), &state.storage, &file_id).await;
        return match result {
            Ok((file, content)) => {
                let content_type = file
                    .mime_type
                    .unwrap_or_else(|| "application/octet-stream".to_string());
                let filename = sanitize_content_disposition(&file.filename);
                (
                    [
                        (axum::http::header::CONTENT_TYPE, content_type),
                        (
                            axum::http::header::CONTENT_DISPOSITION,
                            format!("{disposition}; filename=\"{filename}\""),
                        ),
                        (
                            axum::http::header::CONTENT_LENGTH,
                            content.len().to_string(),
                        ),
                    ],
                    content,
                )
                    .into_response()
            }
            Err(e) => error_response(e),
        };
    }

    // Regular drive files: resolve any Range against the stored size, then
    // stream straight from disk — 206 for partials, 416 when unsatisfiable.
    let meta = match crate::api::drive::get_file_metadata(state.db.pool(), &file_id).await {
        Ok(f) => f,
        Err(e) => return error_response(e),
    };
    let total = meta.size_bytes.max(0) as u64;
    let range_header = headers
        .get(axum::http::header::RANGE)
        .and_then(|v| v.to_str().ok());
    let range = match resolve_range(range_header, total) {
        RangeOutcome::Unsatisfiable => {
            return (
                StatusCode::RANGE_NOT_SATISFIABLE,
                [
                    (axum::http::header::ACCEPT_RANGES, "bytes".to_string()),
                    (
                        axum::http::header::CONTENT_RANGE,
                        format!("bytes */{total}"),
                    ),
                ],
            )
                .into_response();
        }
        RangeOutcome::Full => None,
        RangeOutcome::Partial(start, len) => Some((start, len)),
    };

    let result = crate::api::drive::download_file_stream(
        state.db.pool(),
        &state.drive_config,
        &file_id,
        range,
    )
    .await;
    match result {
        Ok((file, _disk_total, stream)) => {
            let content_type = file
                .mime_type
                .unwrap_or_else(|| "application/octet-stream".to_string());
            let filename = sanitize_content_disposition(&file.filename);
            let (status, content_length, content_range) = match range {
                Some((start, len)) => (
                    StatusCode::PARTIAL_CONTENT,
                    len,
                    Some(format!("bytes {}-{}/{}", start, start + len - 1, total)),
                ),
                None => (StatusCode::OK, total, None),
            };
            let mut builder = axum::http::Response::builder()
                .status(status)
                .header(axum::http::header::CONTENT_TYPE, content_type)
                .header(
                    axum::http::header::CONTENT_DISPOSITION,
                    format!("{disposition}; filename=\"{filename}\""),
                )
                .header(axum::http::header::ACCEPT_RANGES, "bytes")
                .header(axum::http::header::CONTENT_LENGTH, content_length);
            if let Some(cr) = content_range {
                builder = builder.header(axum::http::header::CONTENT_RANGE, cr);
            }
            match builder.body(Body::from_stream(stream)) {
                Ok(resp) => resp.into_response(),
                Err(e) => error_response(crate::error::Error::Other(format!(
                    "Failed to build response: {e}"
                ))),
            }
        }
        Err(e) => error_response(e),
    }
}

// ── Annotations (document highlights + margin notes, researcher-plan D2) ──

/// GET /api/annotations?file_id=… — list a file's annotations.
#[derive(Debug, Deserialize)]
pub struct ListAnnotationsQuery {
    pub file_id: String,
}
pub async fn list_annotations_handler(
    State(state): State<AppState>,
    Query(q): Query<ListAnnotationsQuery>,
) -> Response {
    api_response(crate::api::annotations::list_annotations(state.db.pool(), &q.file_id).await)
}

/// GET /api/annotations/export?file_id=… — a file's highlights as markdown.
pub async fn export_file_annotations_handler(
    State(state): State<AppState>,
    Query(q): Query<ListAnnotationsQuery>,
) -> Response {
    match crate::api::annotations::export_file_annotations_md(state.db.pool(), &q.file_id).await {
        Ok(md) => markdown_response(md),
        Err(e) => error_response(e),
    }
}

/// Serve markdown as a downloadable text body.
fn markdown_response(md: String) -> Response {
    (
        [(axum::http::header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        md,
    )
        .into_response()
}

/// POST /api/annotations — create (or upsert) a highlight.
pub async fn create_annotation_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::api::annotations::CreateAnnotationRequest>,
) -> Response {
    api_response(crate::api::annotations::create_annotation(state.db.pool(), req).await)
}

/// PATCH /api/annotations/:id — edit note/color.
pub async fn update_annotation_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<crate::api::annotations::UpdateAnnotationRequest>,
) -> Response {
    api_response(crate::api::annotations::update_annotation(state.db.pool(), &id, req).await)
}

/// DELETE /api/annotations/:id
pub async fn delete_annotation_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::annotations::delete_annotation(state.db.pool(), &id).await {
        Ok(_) => success_message("Annotation deleted"),
        Err(e) => error_response(e),
    }
}

/// POST /api/drive/files/:id/reextract — queue a file for (re-)extraction.
pub async fn reextract_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    api_response(crate::api::drive::reextract_file(state.db.pool(), &file_id).await)
}

/// DELETE /api/drive/files/:id - Delete a file or folder
pub async fn delete_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    match crate::api::drive::delete_file(state.db.pool(), &state.drive_config, &file_id).await {
        Ok(_) => success_message("File deleted"),
        Err(e) => error_response(e),
    }
}

/// PUT /api/drive/files/:id/move - Move or rename a file
pub async fn move_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
    Json(request): Json<crate::api::drive::MoveFileRequest>,
) -> Response {
    api_response(
        crate::api::drive::move_file(
            state.db.pool(),
            &state.drive_config,
            &file_id,
            &request.new_path,
        )
        .await,
    )
}

/// POST /api/drive/upload - Upload a file (multipart form)
pub async fn upload_drive_file_handler(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Response {
    use sha2::Digest as _;
    use tokio::io::AsyncWriteExt;

    /// Per-file ceiling, enforced while streaming. The router body limit
    /// (260MB) is only a backstop above this, so the honest 413 below fires
    /// first and the client gets a real message instead of a connection reset.
    const MAX_UPLOAD_FILE_BYTES: u64 = 250 * 1024 * 1024;

    let too_large = || {
        (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({
                "error": "Your server didn't take that file: it's over the 250 MB upload limit. Split it or compress it, then upload again."
            })),
        )
            .into_response()
    };

    // Stream the multipart form. The file field is written chunk-by-chunk to a
    // staging file on the drive filesystem while hashing incrementally — the
    // upload is never held in memory (committing later is a rename).
    let mut path: Option<String> = None;
    let mut filename: Option<String> = None;
    let mut mime_type: Option<String> = None;
    let mut staged: Option<crate::api::drive::StagedUpload> = None;

    let cleanup = |staged: &Option<crate::api::drive::StagedUpload>| {
        if let Some(s) = staged {
            let p = s.temp_path.clone();
            tokio::spawn(async move {
                let _ = tokio::fs::remove_file(p).await;
            });
        }
    };

    loop {
        let mut field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(_) => {
                // With the in-stream cap below the router backstop, an error
                // here is a malformed body or an aborted connection.
                cleanup(&staged);
                return error_response(crate::error::Error::InvalidInput(
                    "Upload interrupted or malformed".into(),
                ));
            }
        };
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "path" => {
                if let Ok(text) = field.text().await {
                    path = Some(text);
                }
            }
            "file" => {
                filename = field.file_name().map(|s| s.to_string());
                mime_type = field.content_type().map(|s| s.to_string());

                // Repeated file fields: keep the last, drop the earlier stage.
                cleanup(&staged);

                let staging_dir = match state.drive_config.storage.staging_dir().await {
                    Ok(d) => d,
                    Err(e) => return error_response(e),
                };
                static STAGING_SEQ: std::sync::atomic::AtomicU64 =
                    std::sync::atomic::AtomicU64::new(0);
                let temp_path = staging_dir.join(format!(
                    "{}-{}.part",
                    std::process::id(),
                    STAGING_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                ));

                let mut out = match tokio::fs::File::create(&temp_path).await {
                    Ok(f) => f,
                    Err(e) => {
                        return error_response(crate::error::Error::Storage(format!(
                            "Failed to stage upload: {e}"
                        )))
                    }
                };
                let mut hasher = sha2::Sha256::new();
                let mut written: u64 = 0;
                loop {
                    match field.chunk().await {
                        Ok(Some(chunk)) => {
                            written += chunk.len() as u64;
                            if written > MAX_UPLOAD_FILE_BYTES {
                                drop(out);
                                let _ = tokio::fs::remove_file(&temp_path).await;
                                return too_large();
                            }
                            hasher.update(&chunk);
                            if let Err(e) = out.write_all(&chunk).await {
                                let _ = tokio::fs::remove_file(&temp_path).await;
                                return error_response(crate::error::Error::Storage(format!(
                                    "Failed to stage upload: {e}"
                                )));
                            }
                        }
                        Ok(None) => break,
                        Err(_) => {
                            let _ = tokio::fs::remove_file(&temp_path).await;
                            return error_response(crate::error::Error::InvalidInput(
                                "Upload interrupted or malformed".into(),
                            ));
                        }
                    }
                }
                if let Err(e) = out.flush().await {
                    let _ = tokio::fs::remove_file(&temp_path).await;
                    return error_response(crate::error::Error::Storage(format!(
                        "Failed to stage upload: {e}"
                    )));
                }
                staged = Some(crate::api::drive::StagedUpload {
                    temp_path,
                    size_bytes: written as i64,
                    sha256: format!("{:x}", hasher.finalize()),
                });
            }
            _ => {}
        }
    }

    let request = crate::api::drive::UploadRequest {
        path: path.unwrap_or_else(|| "uploads".to_string()),
        filename: filename.unwrap_or_else(|| "unnamed".to_string()),
        mime_type,
    };

    match staged {
        Some(staged) => {
            let temp_path = staged.temp_path.clone();
            match crate::api::drive::upload_file(
                state.db.pool(),
                &state.drive_config,
                request,
                staged,
            )
            .await
            {
                Ok(file) => (StatusCode::CREATED, Json(file)).into_response(),
                Err(e) => {
                    // Commit failed before the rename — drop the staged file.
                    let _ = tokio::fs::remove_file(&temp_path).await;
                    error_response(e)
                }
            }
        }
        None => error_response(crate::error::Error::InvalidInput(
            "No file data provided".into(),
        )),
    }
}

/// POST /api/drive/folders - Create a folder
pub async fn create_drive_folder_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::drive::CreateFolderRequest>,
) -> Response {
    match crate::api::drive::create_folder(state.db.pool(), &state.drive_config, request).await {
        Ok(folder) => (StatusCode::CREATED, Json(folder)).into_response(),
        Err(e) => error_response(e),
    }
}

// =============================================================================
// Drive Trash Handlers
// =============================================================================

/// GET /api/drive/trash - List files in trash
/// GET /api/drive/media — the app's internal assets (.media/). Read-only.
pub async fn list_drive_media_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::drive::list_media(state.db.pool()).await)
}

pub async fn list_drive_trash_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::drive::list_trash(state.db.pool()).await)
}

/// POST /api/drive/files/:id/restore - Restore a file from trash
pub async fn restore_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    api_response(crate::api::drive::restore_file(state.db.pool(), &file_id).await)
}

/// DELETE /api/drive/files/:id/purge - Permanently delete a file (skip trash)
pub async fn purge_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    match crate::api::drive::purge_file(state.db.pool(), &state.drive_config, &file_id).await {
        Ok(_) => success_message("File permanently deleted"),
        Err(e) => error_response(e),
    }
}

/// POST /api/drive/trash/empty - Empty all files from trash
pub async fn empty_drive_trash_handler(State(state): State<AppState>) -> Response {
    match crate::api::drive::empty_trash(state.db.pool(), &state.drive_config).await {
        Ok(count) => (
            StatusCode::OK,
            Json(serde_json::json!({ "deleted_count": count })),
        )
            .into_response(),
        Err(e) => error_response(e),
    }
}

// =============================================================================
// Media Handlers
// =============================================================================

/// POST /api/media/upload - Upload media file with content-addressed dedup
///
/// Accepts multipart form with:
/// - `file`: The file data (required)
/// - `filename`: Override filename (optional, uses file's name by default)
///
/// Returns MediaFile with URL for embedding in pages.
/// If identical content already exists, returns existing file (dedup).
pub async fn upload_media_handler(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Response {
    // Parse multipart form
    let mut filename: Option<String> = None;
    let mut mime_type: Option<String> = None;
    let mut data: Option<axum::body::Bytes> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "filename" => {
                if let Ok(text) = field.text().await {
                    filename = Some(text);
                }
            }
            "file" => {
                // Use form field filename if no explicit filename provided
                if filename.is_none() {
                    filename = field.file_name().map(|s| s.to_string());
                }
                mime_type = field.content_type().map(|s| s.to_string());
                if let Ok(bytes) = field.bytes().await {
                    data = Some(bytes);
                }
            }
            _ => {}
        }
    }

    let filename = filename.unwrap_or_else(|| "unnamed".to_string());

    match data {
        Some(bytes) => {
            match crate::api::media::upload_media(
                state.db.pool(),
                &state.drive_config,
                &filename,
                mime_type,
                bytes,
            )
            .await
            {
                Ok(file) => (StatusCode::CREATED, Json(file)).into_response(),
                Err(e) => error_response(e),
            }
        }
        None => error_response(crate::error::Error::InvalidInput(
            "No file data provided".into(),
        )),
    }
}

/// GET /api/media/:id - Get media file metadata
pub async fn get_media_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    api_response(crate::api::media::get_media(state.db.pool(), &file_id).await)
}

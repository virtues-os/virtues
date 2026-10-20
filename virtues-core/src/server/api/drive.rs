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
        // How a block page's files stand, every block a device asks about at
        // once; and the files a device says are coming, before their bytes.
        .route("/api/media/uploads", get(page_file_states_handler).post(page_files_coming_handler))
        // Which page files any page still names.
        .route("/api/media/uploads/named", get(page_files_named_handler))
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

// ── Annotations (document highlights + margin notes, agents/record/researcher.md D2) ──

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
///
/// A form whose first field is `id` is a block page's file, stored under
/// that Drive id ([`upload_page_file`]).
pub async fn upload_media_handler(
    State(state): State<AppState>,
    multipart: axum::extract::Multipart,
) -> Response {
    receive_media(state.db.pool(), &state.drive_config, multipart).await
}

/// [`upload_media_handler`]'s work, given the database and the Drive.
async fn receive_media(
    pool: &sqlx::PgPool,
    drive: &crate::api::drive::DriveConfig,
    multipart: axum::extract::Multipart,
) -> Response {
    receive_media_within(pool, drive, multipart, crate::api::media::STALLED_AFTER).await
}

/// [`receive_media`], a page's file stopped once nothing has arrived for
/// `quiet`.
async fn receive_media_within(
    pool: &sqlx::PgPool,
    drive: &crate::api::drive::DriveConfig,
    mut multipart: axum::extract::Multipart,
    quiet: std::time::Duration,
) -> Response {
    // Parse multipart form
    let mut filename: Option<String> = None;
    let mut mime_type: Option<String> = None;
    let mut data: Option<axum::body::Bytes> = None;

    let mut first = true;
    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if std::mem::take(&mut first) && name == "id" {
            return match field.text().await {
                Ok(id) => upload_page_file(pool, drive, id, multipart, quiet).await,
                Err(_) => error_response(crate::error::Error::InvalidInput("Upload interrupted or malformed".into())),
            };
        }
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
                pool,
                drive,
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

/// The rest of a block page's file upload, after its `id`: `filename`,
/// `size` and `file`, in that order. The page already holds the file's block
/// at `/drive/<id>`; the upload is known by that id from its first field, so
/// every device can say it is arriving (`GET /api/media/uploads`).
///
/// - An id of the wrong shape is refused (400).
/// - An id another file has is refused, and that file left as it is (409),
///   unless it holds these very bytes: a retry after a lost answer, answered
///   with the file stored (201).
/// - Past the size a page holds, the upload stops (413).
/// - Once nothing has arrived for `quiet` ([`media::STALLED_AFTER`]), the
///   upload stops (408): a device that went quiet without closing the
///   connection (asleep, out of range) would otherwise hold it, and every
///   device asking would hear it is arriving, until a restart.
///
/// However it stops, the reason is kept for the devices that ask, with
/// whether the server refused the file: a refusal is not sent again, while
/// a file cut off, gone quiet or not stored may be.
async fn upload_page_file(
    pool: &sqlx::PgPool,
    drive: &crate::api::drive::DriveConfig,
    id: String,
    mut multipart: axum::extract::Multipart,
    quiet: std::time::Duration,
) -> Response {
    use crate::api::media;

    if !media::is_page_file_id(&id) {
        return error_response(crate::error::Error::InvalidInput(format!(
            "{id} isn't an address a page's file can have."
        )));
    }
    let mut filename: Option<String> = None;
    let mut total: Option<u64> = None;
    // A stop the sending device sees as a refusal is one it does not try
    // again (`UploadError.refused` in the web client): a 4xx other than 408
    // and 429. A connection cut off answers nobody, so it is no refusal
    // whatever its status says. `upload`: this request's bytes, once they
    // began.
    let stopped = |upload: Option<media::PageFileUpload>, filename: &str, status: StatusCode, reason: String, refused: bool| {
        media::page_file_failed(&id, filename, &reason, refused, upload);
        tracing::warn!(id = %id, filename, %reason, refused, "a page's file stopped uploading");
        (status, Json(serde_json::json!({ "error": reason }))).into_response()
    };
    loop {
        let mut field = match tokio::time::timeout(quiet, multipart.next_field()).await {
            Ok(Ok(Some(field))) => field,
            Ok(Ok(None)) => break,
            Ok(Err(_)) => {
                let name = filename.clone().unwrap_or_else(|| "The file".into());
                return stopped(None, &name, StatusCode::BAD_REQUEST, cut_off(&name), false);
            }
            Err(_) => {
                let name = filename.clone().unwrap_or_else(|| "The file".into());
                return stopped(None, &name, StatusCode::REQUEST_TIMEOUT, media::stalled(&name), false);
            }
        };
        match field.name().unwrap_or("") {
            "filename" => filename = field.text().await.ok(),
            "size" => total = field.text().await.ok().and_then(|t| t.trim().parse().ok()),
            "file" => {
                let name = filename.clone().or_else(|| field.file_name().map(str::to_string)).unwrap_or_else(|| "unnamed".into());
                let mime_type = field.content_type().map(str::to_string);
                let upload = media::page_file_arriving(&id, &name, total);
                // A stop from here is this request's bytes' stop.
                let stopped = |filename: &str, status: StatusCode, reason: String, refused: bool| {
                    stopped(Some(upload), filename, status, reason, refused)
                };
                // From here every way out says how the upload ended, but one:
                // hyper drops this future when its connection or stream is
                // reset, and nothing after the await it was in runs.
                let mut unanswered = Unanswered { id: &id, name: &name, upload, armed: true };
                let mut bytes = Vec::with_capacity(total.unwrap_or(0).min(media::MAX_FILE_SIZE as u64) as usize);
                loop {
                    match tokio::time::timeout(quiet, field.chunk()).await {
                        Ok(Ok(Some(chunk))) => {
                            if bytes.len() + chunk.len() > media::MAX_FILE_SIZE {
                                unanswered.armed = false;
                                return stopped(&name, StatusCode::PAYLOAD_TOO_LARGE, media::too_large(&name), true);
                            }
                            bytes.extend_from_slice(&chunk);
                            media::page_file_received(&id, upload, bytes.len() as u64);
                        }
                        Ok(Ok(None)) => break,
                        Ok(Err(_)) => {
                            unanswered.armed = false;
                            return stopped(&name, StatusCode::BAD_REQUEST, cut_off(&name), false);
                        }
                        Err(_) => {
                            unanswered.armed = false;
                            return stopped(&name, StatusCode::REQUEST_TIMEOUT, media::stalled(&name), false);
                        }
                    }
                }
                let stored = media::store_page_file(
                    pool,
                    drive,
                    &id,
                    &name,
                    mime_type,
                    axum::body::Bytes::from(bytes),
                )
                .await;
                unanswered.armed = false;
                return match stored {
                    Ok(media::PageFileStored::Stored(file)) => {
                        media::page_file_stored(&id);
                        (StatusCode::CREATED, Json(file)).into_response()
                    }
                    Ok(media::PageFileStored::Taken) => stopped(
                        &name,
                        StatusCode::CONFLICT,
                        format!("Your server already holds another file at this address, so it didn't store {name}. Add it to the page again."),
                        true,
                    ),
                    // A 5xx: the sending device tries again, and Try again
                    // stores it once there is room.
                    Ok(media::PageFileStored::NoRoom) => {
                        stopped(&name, StatusCode::INSUFFICIENT_STORAGE, media::no_room(&name), false)
                    }
                    Err(e) => {
                        let status = StatusCode::from_u16(e.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                        let refused = status.is_client_error()
                            && status != StatusCode::REQUEST_TIMEOUT
                            && status != StatusCode::TOO_MANY_REQUESTS;
                        let reason = match &e {
                            // Already a sentence for the person: too large, an id of another shape.
                            crate::error::Error::InvalidInput(message) => message.clone(),
                            _ => {
                                tracing::error!(id = %id, error = %e, "a page's file could not be stored");
                                format!("Your server couldn't store {name}. Try again.")
                            }
                        };
                        stopped(&name, status, reason, refused)
                    }
                };
            }
            _ => {}
        }
    }
    let name = filename.unwrap_or_else(|| "The file".into());
    stopped(None, &name, StatusCode::BAD_REQUEST, format!("{name} never arrived, so your server stored nothing."), true)
}

/// A page file's upload as it arrives, said to have been cut off when the
/// handler is dropped before it answers: a connection or stream reset drops
/// the request's future, and with nothing said the upload read as arriving
/// until it went quiet, then as stopped from then on. Every answer the
/// handler gives says how the upload ended itself, and disarms this first.
struct Unanswered<'a> {
    id: &'a str,
    name: &'a str,
    upload: crate::api::media::PageFileUpload,
    armed: bool,
}

impl Drop for Unanswered<'_> {
    fn drop(&mut self) {
        if self.armed {
            crate::api::media::page_file_failed(self.id, self.name, &cut_off(self.name), false, Some(self.upload));
            tracing::warn!(id = %self.id, filename = self.name, "a page's file stopped uploading: its request was dropped");
        }
    }
}

/// Why an upload cut off on its way stopped.
fn cut_off(name: &str) -> String {
    format!("{name} stopped before it finished uploading.")
}

/// The most ids one `GET /api/media/uploads` asks about.
const PAGE_FILES_AT_ONCE: usize = 100;

#[derive(Debug, Deserialize)]
pub struct PageFileStatesQuery {
    /// Drive ids, comma-separated.
    pub ids: String,
}

/// GET /api/media/uploads?ids=a,b - how each block page's file stands:
/// `done`, `uploading` (with `received` and `total`), `failed` (with
/// `reason`) or `missing`.
pub async fn page_file_states_handler(
    State(state): State<AppState>,
    Query(query): Query<PageFileStatesQuery>,
) -> Response {
    let ids: Vec<String> = query
        .ids
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect();
    if ids.len() > PAGE_FILES_AT_ONCE {
        return error_response(crate::error::Error::InvalidInput(format!(
            "Ask about at most {PAGE_FILES_AT_ONCE} files at once."
        )));
    }
    api_response(crate::api::media::page_file_states(state.db.pool(), &ids).await)
}

/// One file a device says is coming (`POST /api/media/uploads`).
#[derive(Debug, Deserialize)]
pub struct ComingFile {
    pub id: String,
    pub filename: String,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct ComingFiles {
    pub files: Vec<ComingFile>,
}

/// POST /api/media/uploads - the page files a device holds and is about to
/// send: each reads as arriving from now (`media::page_file_announced`), so a
/// device asking, and a share of the page, wait for it while the browser
/// holds its request. At most [`PAGE_FILES_AT_ONCE`]; an id of another shape
/// is refused.
pub async fn page_files_coming_handler(Json(coming): Json<ComingFiles>) -> Response {
    if coming.files.len() > PAGE_FILES_AT_ONCE {
        return error_response(crate::error::Error::InvalidInput(format!(
            "Say at most {PAGE_FILES_AT_ONCE} files at once."
        )));
    }
    if let Some(bad) = coming.files.iter().find(|f| !crate::api::media::is_page_file_id(&f.id)) {
        return error_response(crate::error::Error::InvalidInput(format!(
            "{} isn't an address a page's file can have.",
            bad.id
        )));
    }
    for file in &coming.files {
        crate::api::media::page_file_announced(&file.id, &file.filename, file.size);
    }
    StatusCode::NO_CONTENT.into_response()
}

/// GET /api/media/uploads/named?ids=a,b - whether any page names each file
/// (`media::page_files_named`), as `{ id: bool }`.
pub async fn page_files_named_handler(
    State(state): State<AppState>,
    Query(query): Query<PageFileStatesQuery>,
) -> Response {
    let ids: Vec<String> = query
        .ids
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect();
    if ids.len() > PAGE_FILES_AT_ONCE {
        return error_response(crate::error::Error::InvalidInput(format!(
            "Ask about at most {PAGE_FILES_AT_ONCE} files at once."
        )));
    }
    api_response(crate::api::media::page_files_named(&state.yjs_state, &ids).await)
}

/// GET /api/media/:id - Get media file metadata
pub async fn get_media_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    api_response(crate::api::media::get_media(state.db.pool(), &file_id).await)
}

#[cfg(test)]
mod page_file_tests {
    //! A block page's file: stored under the Drive id its block already
    //! holds, and every device able to ask how it stands.

    use super::{receive_media, receive_media_within};
    use crate::api::drive::DriveConfig;
    use crate::api::media::{page_file_states, PageFileState};
    use axum::{body::Body, http::Request, routing::post, Router};
    use sqlx::PgPool;
    use std::sync::Arc;
    use tower::Service;

    const ID: &str = "file_00c0ffee00c0ffee";
    const BOUNDARY: &str = "page-file-boundary";

    fn scratch_drive() -> (tempfile::TempDir, Arc<DriveConfig>) {
        let dir = tempfile::tempdir().unwrap();
        let storage = crate::storage::Storage::file(dir.path().to_string_lossy().into_owned()).unwrap();
        (dir, Arc::new(DriveConfig::new(Arc::new(storage))))
    }

    fn app(pool: &PgPool, drive: &Arc<DriveConfig>) -> Router {
        let (pool, drive) = (pool.clone(), drive.clone());
        Router::new().route(
            "/api/media/upload",
            post(move |multipart: axum::extract::Multipart| async move { receive_media(&pool, &drive, multipart).await }),
        )
    }

    /// [`app`], an upload stopped once nothing has arrived for `quiet`.
    fn app_within(pool: &PgPool, drive: &Arc<DriveConfig>, quiet: std::time::Duration) -> Router {
        let (pool, drive) = (pool.clone(), drive.clone());
        Router::new().route(
            "/api/media/upload",
            post(move |multipart: axum::extract::Multipart| async move {
                receive_media_within(&pool, &drive, multipart, quiet).await
            }),
        )
    }

    /// A multipart form's fields in order: `(name, file name, bytes)`.
    fn form(fields: &[(&str, Option<&str>, &[u8])]) -> Vec<u8> {
        let mut body = vec![];
        for (name, file, bytes) in fields {
            body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
            match file {
                Some(file) => body.extend_from_slice(
                    format!("Content-Disposition: form-data; name=\"{name}\"; filename=\"{file}\"\r\nContent-Type: image/png\r\n\r\n").as_bytes(),
                ),
                None => body.extend_from_slice(format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes()),
            }
            body.extend_from_slice(bytes);
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
        body
    }

    fn page_file(id: &str, bytes: &[u8]) -> Vec<u8> {
        form(&[
            ("id", None, id.as_bytes()),
            ("filename", None, b"harbour.png"),
            ("size", None, bytes.len().to_string().as_bytes()),
            ("file", Some("harbour.png"), bytes),
        ])
    }

    async fn send(app: &Router, body: impl Into<Body>) -> (u16, serde_json::Value) {
        let request = Request::post("/api/media/upload")
            .header("content-type", format!("multipart/form-data; boundary={BOUNDARY}"))
            .body(body.into())
            .unwrap();
        let response = app.clone().call(request).await.unwrap();
        let status = response.status().as_u16();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
    }

    async fn state(pool: &PgPool, id: &str) -> PageFileState {
        page_file_states(pool, &[id.to_string()]).await.unwrap().remove(id).unwrap()
    }

    #[sqlx::test]
    async fn a_page_file_is_stored_under_the_id_its_block_holds(pool: PgPool) {
        let (_dir, drive) = scratch_drive();
        let app = app(&pool, &drive);
        assert_eq!(state(&pool, ID).await, PageFileState::Missing);

        let (status, body) = send(&app, page_file(ID, b"one picture")).await;
        assert_eq!(status, 201, "{body}");
        assert_eq!(body["id"], ID);
        let file = crate::api::drive::get_file_metadata(&pool, ID).await.unwrap();
        assert_eq!(file.path, format!(".media/uploads/{ID}.png"));
        let (_, bytes) = crate::api::drive::download_file(&pool, &drive, ID).await.unwrap();
        assert_eq!(&bytes[..], b"one picture");
        assert_eq!(state(&pool, ID).await, PageFileState::Done);

        // A retry after a lost answer: the same bytes answer with the file stored.
        let (status, body) = send(&app, page_file(ID, b"one picture")).await;
        assert_eq!(status, 201, "{body}");
        assert_eq!(body["id"], ID);
    }

    #[sqlx::test]
    async fn an_id_another_file_has_is_refused_and_that_file_left_as_it_is(pool: PgPool) {
        let (_dir, drive) = scratch_drive();
        let app = app(&pool, &drive);
        // Its own id: what an upload stopped for is kept for every device, across tests.
        let id = "file_0dd0dd0dd0dd0dd0";
        assert_eq!(send(&app, page_file(id, b"one picture")).await.0, 201);
        let (status, body) = send(&app, page_file(id, b"another picture")).await;
        assert_eq!(status, 409, "{body}");
        assert!(body["error"].as_str().unwrap().contains("harbour.png"), "{body}");
        let (_, bytes) = crate::api::drive::download_file(&pool, &drive, id).await.unwrap();
        assert_eq!(&bytes[..], b"one picture");

        // A Drive file of the person's, whatever its id's shape.
        let theirs = "file_0123456789abcdef";
        sqlx::query(
            "INSERT INTO app_drive_files (id, path, filename, mime_type, size_bytes) \
             VALUES ($1, 'notes.txt', 'notes.txt', 'text/plain', 5)",
        )
        .bind(theirs)
        .execute(&pool)
        .await
        .unwrap();
        let (status, _) = send(&app, page_file(theirs, b"a picture")).await;
        assert_eq!(status, 409);
        let row = crate::api::drive::get_file_metadata(&pool, theirs).await.unwrap();
        assert_eq!((row.path.as_str(), row.size_bytes), ("notes.txt", 5));
    }

    #[sqlx::test]
    async fn an_id_of_another_shape_is_refused(pool: PgPool) {
        let (_dir, drive) = scratch_drive();
        let app = app(&pool, &drive);
        for id in ["file_00C0FFEE00C0FFEE", "file_00c0ffee", "../file_00c0ffee00c0ffee", "mu_00c0ffee00c0ffee"] {
            let (status, body) = send(&app, page_file(id, b"x")).await;
            assert_eq!(status, 400, "{id}: {body}");
        }
    }

    /// While the bytes arrive, every device asking learns how far; an upload
    /// cut off says so, with its reason, and the file is not stored.
    #[sqlx::test]
    async fn how_an_upload_stands_while_it_arrives_and_after_it_stops(pool: PgPool) {
        let (_dir, drive) = scratch_drive();
        let app = app(&pool, &drive);
        let id = "file_feedfacefeedface";
        let body = page_file(id, &[7u8; 64 * 1024]);
        // Everything but the file's last bytes and the closing boundary, then nothing.
        let cut = body.len() - 1024;
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, std::io::Error>>(4);
        tx.send(Ok(body[..cut].to_vec())).await.unwrap();
        let upload = tokio::spawn({
            let app = app.clone();
            async move { send(&app, Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx))).await }
        });
        let arriving = loop {
            match state(&pool, id).await {
                PageFileState::Uploading { received, .. } if received > 0 => break state(&pool, id).await,
                _ => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
            }
        };
        assert!(
            matches!(&arriving, PageFileState::Uploading { filename, total: Some(total), .. }
                if filename == "harbour.png" && *total == 64 * 1024),
            "{arriving:?}"
        );
        tx.send(Err(std::io::Error::other("the connection dropped"))).await.unwrap();
        drop(tx);
        let (status, answer) = upload.await.unwrap();
        assert_eq!(status, 400, "{answer}");
        match state(&pool, id).await {
            PageFileState::Failed { filename, reason, refused, .. } => {
                assert_eq!(filename, "harbour.png");
                assert!(reason.contains("stopped before it finished"), "{reason}");
                assert!(!refused, "the device that holds it tries again after a cut-off");
            }
            other => panic!("{other:?}"),
        }
        assert!(crate::api::drive::get_file_metadata(&pool, id).await.is_err());
    }

    /// An upload whose request is dropped mid-way (a reset connection or
    /// stream: hyper drops the handler's future) is said to have stopped at
    /// once, short of a refusal, and how long ago. Nothing said it: it read
    /// as arriving, then as gone quiet, never as cut off.
    #[sqlx::test]
    async fn an_upload_whose_request_is_dropped_says_it_stopped(pool: PgPool) {
        let (_dir, drive) = scratch_drive();
        let app = app(&pool, &drive);
        let id = "file_00000000decade00";
        let body = page_file(id, &[7u8; 64 * 1024]);
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, std::io::Error>>(4);
        tx.send(Ok(body[..body.len() - 1024].to_vec())).await.unwrap();
        let upload = tokio::spawn({
            let app = app.clone();
            async move { send(&app, Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx))).await }
        });
        while !matches!(state(&pool, id).await, PageFileState::Uploading { received, .. } if received > 0) {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        upload.abort();
        assert!(upload.await.unwrap_err().is_cancelled());
        match state(&pool, id).await {
            PageFileState::Failed { filename, reason, refused, stopped_ms_ago } => {
                assert_eq!(filename, "harbour.png");
                assert!(reason.contains("stopped before it finished"), "{reason}");
                assert!(!refused, "the device that holds it may send it again");
                assert!(stopped_ms_ago < 60_000, "{stopped_ms_ago}");
            }
            other => panic!("{other:?}"),
        }
        drop(tx);
    }

    /// A file sent again while the server still holds the request that
    /// carried it before (a phone that moved from Wi-Fi to cellular: its
    /// first request errored at once on the phone, and the server heard
    /// nothing until it went quiet): the older request's stop is no stop of
    /// the newer, which goes on arriving and is stored. Every device read
    /// the file as stopped, then "didn't finish uploading" with Remove,
    /// while its bytes still arrived.
    #[sqlx::test]
    async fn an_older_requests_stop_is_no_stop_of_the_file_sent_again(pool: PgPool) {
        let (_dir, drive) = scratch_drive();
        let app = app(&pool, &drive);
        let id = "file_0a1d0a1d0a1d0a1d";
        let start = |body: Vec<u8>, at: usize| {
            let (tx, rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, std::io::Error>>(4);
            let sent = tx.try_send(Ok(body[..at].to_vec()));
            assert!(sent.is_ok());
            let app = app.clone();
            let upload = tokio::spawn(async move {
                send(&app, Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx))).await
            });
            (tx, upload)
        };
        let arriving_of = |total: u64| {
            let pool = pool.clone();
            async move {
                loop {
                    match state(&pool, id).await {
                        PageFileState::Uploading { received, total: Some(t), .. } if received > 0 && t == total => {
                            return
                        }
                        _ => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
                    }
                }
            }
        };
        let first = page_file(id, &[7u8; 64 * 1024]);
        let (first_tx, first_upload) = start(first.clone(), first.len() - 1024);
        arriving_of(64 * 1024).await;

        let again = page_file(id, &[7u8; 32 * 1024]);
        let (again_tx, again_upload) = start(again.clone(), again.len() - 1024);
        arriving_of(32 * 1024).await;

        // The older request is dropped, as the server lets go of it.
        first_upload.abort();
        assert!(first_upload.await.unwrap_err().is_cancelled());
        drop(first_tx);
        assert!(
            matches!(state(&pool, id).await, PageFileState::Uploading { total: Some(t), .. } if t == 32 * 1024),
            "{:?}",
            state(&pool, id).await
        );

        again_tx.send(Ok(again[again.len() - 1024..].to_vec())).await.unwrap();
        drop(again_tx);
        let (status, answer) = again_upload.await.unwrap();
        assert_eq!(status, 201, "{answer}");
        assert_eq!(state(&pool, id).await, PageFileState::Done);
    }

    /// A device that goes quiet mid-upload without closing the connection
    /// (asleep, out of range) stops the upload once nothing has arrived for
    /// the quiet spell: the server stops reading, answers, and every device
    /// asking hears it stopped and that it may be sent again, never that it
    /// is still arriving.
    #[sqlx::test]
    async fn an_upload_gone_quiet_stops_and_says_so(pool: PgPool) {
        let (_dir, drive) = scratch_drive();
        let app = app_within(&pool, &drive, std::time::Duration::from_millis(300));
        let id = "file_0000feed0000feed";
        let body = page_file(id, &[7u8; 64 * 1024]);
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, std::io::Error>>(4);
        tx.send(Ok(body[..body.len() - 1024].to_vec())).await.unwrap();
        // The connection stays open, and nothing more comes.
        let upload = send(&app, Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx)));
        let (status, answer) = tokio::time::timeout(std::time::Duration::from_secs(10), upload)
            .await
            .expect("the server stops a quiet upload rather than wait on it");
        assert_eq!(status, 408, "{answer}");
        match state(&pool, id).await {
            PageFileState::Failed { filename, reason, refused, .. } => {
                assert_eq!(filename, "harbour.png");
                assert_eq!(reason, crate::api::media::stalled("harbour.png"));
                assert!(!refused);
            }
            other => panic!("{other:?}"),
        }
        assert!(crate::api::drive::get_file_metadata(&pool, id).await.is_err());
        drop(tx);
    }

    /// A disk with room for as many checks as `room` holds (`ROOM`: room
    /// always, `0`: none); the files are a real store's.
    struct Disk {
        store: crate::storage::Storage,
        room: std::sync::atomic::AtomicUsize,
    }

    const ROOM: usize = usize::MAX;

    impl Disk {
        fn with_room(dir: &tempfile::TempDir, room: usize) -> Arc<Self> {
            Arc::new(Disk {
                store: crate::storage::Storage::file(dir.path().to_string_lossy().into_owned()).unwrap(),
                room: std::sync::atomic::AtomicUsize::new(room),
            })
        }

        fn drive(self: &Arc<Self>) -> Arc<DriveConfig> {
            let storage = crate::storage::Storage::from_backend(self.clone());
            Arc::new(DriveConfig::new(Arc::new(storage)))
        }
    }

    #[async_trait::async_trait]
    impl crate::storage::StorageBackend for Disk {
        async fn initialize(&self) -> crate::error::Result<()> {
            self.store.initialize().await
        }
        async fn upload(&self, key: &str, data: Vec<u8>) -> crate::error::Result<()> {
            self.store.upload(key, data).await
        }
        async fn upload_from_file(&self, key: &str, src: &std::path::Path) -> crate::error::Result<()> {
            self.store.upload_from_file(key, src).await
        }
        async fn download(&self, key: &str) -> crate::error::Result<Vec<u8>> {
            self.store.download(key).await
        }
        async fn read_stream(
            &self,
            key: &str,
            range: Option<(u64, u64)>,
        ) -> crate::error::Result<crate::storage::ObjectStream> {
            self.store.read_stream(key, range).await
        }
        async fn staging_dir(&self) -> crate::error::Result<std::path::PathBuf> {
            self.store.staging_dir().await
        }
        async fn disk_stats(&self) -> crate::error::Result<(u64, u64)> {
            use std::sync::atomic::Ordering::SeqCst;
            let had_room = self
                .room
                .fetch_update(SeqCst, SeqCst, |left| match left {
                    0 => None,
                    ROOM => Some(ROOM),
                    left => Some(left - 1),
                })
                .is_ok();
            if had_room {
                Ok((u64::MAX / 2, u64::MAX / 4))
            } else {
                Ok((u64::MAX / 2, 0))
            }
        }
        async fn delete(&self, key: &str) -> crate::error::Result<()> {
            self.store.delete(key).await
        }
        async fn list(&self, prefix: &str) -> crate::error::Result<Vec<String>> {
            self.store.list(prefix).await
        }
        async fn list_with_pagination(
            &self,
            prefix: &str,
            max_keys: Option<i32>,
            continuation_token: Option<String>,
        ) -> crate::error::Result<crate::storage::ListResult> {
            self.store.list_with_pagination(prefix, max_keys, continuation_token).await
        }
        async fn health_check(&self) -> crate::error::Result<crate::storage::HealthStatus> {
            self.store.health_check().await
        }
    }

    /// A server out of space stops the upload short of a refusal: emptying
    /// the trash makes room, and the same file sent again is stored. Said as
    /// a refusal, every other device stopped asking for good, and kept
    /// saying the file never finished after it was stored.
    #[sqlx::test]
    async fn an_upload_with_no_room_stops_short_of_a_refusal_and_is_stored_once_there_is(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        let disk = Disk::with_room(&dir, 0);
        let app = app(&pool, &disk.drive());
        let id = "file_00000000f00df00d";

        let (status, body) = send(&app, page_file(id, b"one picture")).await;
        assert_eq!(status, 507, "{body}");
        assert!(body["error"].as_str().unwrap().contains("out of space"), "{body}");
        match state(&pool, id).await {
            PageFileState::Failed { refused, .. } => assert!(!refused, "every other device goes on asking"),
            other => panic!("{other:?}"),
        }

        disk.room.store(ROOM, std::sync::atomic::Ordering::SeqCst);
        let (status, body) = send(&app, page_file(id, b"one picture")).await;
        assert_eq!(status, 201, "{body}");
        assert_eq!(state(&pool, id).await, PageFileState::Done);
    }

    /// A retry of a file already stored, its answer lost on the way, takes
    /// no room: it answers with the file stored however full the box is
    /// now. Refused for want of room, the sending device gave up on a file
    /// every other device draws.
    #[sqlx::test]
    async fn a_retry_of_a_stored_file_is_answered_stored_on_a_full_box(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        let disk = Disk::with_room(&dir, ROOM);
        let app = app(&pool, &disk.drive());
        let id = "file_00000000f11ef11e";

        assert_eq!(send(&app, page_file(id, b"one picture")).await.0, 201);
        disk.room.store(0, std::sync::atomic::Ordering::SeqCst);
        let (status, body) = send(&app, page_file(id, b"one picture")).await;
        assert_eq!(status, 201, "{body}");
        assert_eq!(body["id"], id);
        assert_eq!(state(&pool, id).await, PageFileState::Done);
    }

    /// Room is judged once: an upload that finds the last of it taken
    /// between two checks (another upload landing first) stops short of a
    /// refusal, or is stored, never refused.
    #[sqlx::test]
    async fn an_upload_that_loses_the_last_room_is_never_refused(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        let disk = Disk::with_room(&dir, 1);
        let app = app(&pool, &disk.drive());
        let id = "file_00000000ba5eba5e";

        let (status, body) = send(&app, page_file(id, b"one picture")).await;
        assert!(status == 201 || status == 507, "{status} {body}");
        assert!(!matches!(state(&pool, id).await, PageFileState::Failed { refused: true, .. }));
    }

    /// A refusal is said as one, so devices asking stop asking: the device
    /// that holds the file does not send it again.
    #[sqlx::test]
    async fn a_refused_upload_says_it_was_refused(pool: PgPool) {
        let (_dir, drive) = scratch_drive();
        let app = app(&pool, &drive);
        let id = "file_0000beef0000beef";
        let (status, _) = send(&app, form(&[("id", None, id.as_bytes()), ("filename", None, b"harbour.png")])).await;
        assert_eq!(status, 400);
        assert!(matches!(state(&pool, id).await, PageFileState::Failed { refused: true, .. }));
    }
}

//! REST API handlers.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

use super::webhook::AppState;
use crate::error::Error;

mod applets;
pub use applets::*;
mod settings;
pub use settings::*;
mod wiki;
pub use wiki::*;
mod chat;
pub use chat::*;
mod drive;
pub use drive::*;

/// Sanitize a filename for use in Content-Disposition headers.
/// Removes characters that could cause header injection or parsing issues.
fn sanitize_content_disposition(filename: &str) -> String {
    filename
        .replace('"', "'")
        .replace('\\', "_")
        .replace('\r', "")
        .replace('\n', "")
}

/// Helper to convert Result to Response with proper status code
fn api_response<T: Serialize>(result: crate::error::Result<T>) -> Response {
    result.map(Json).into_response()
}

/// Helper to convert Error to Response with appropriate status code
fn error_response(error: Error) -> Response {
    error.into_response()
}

/// Helper to create a success message response
fn success_message(message: &str) -> Response {
    (
        StatusCode::OK,
        Json(serde_json::json!({ "message": message })),
    )
        .into_response()
}

// ============================================================================
// Pages Handlers
// ============================================================================

/// Query params for pages list
#[derive(Debug, Deserialize)]
pub struct ListPagesQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub workspace_id: Option<String>,
}

/// GET /api/pages - List all pages
pub async fn list_pages_handler(
    State(state): State<AppState>,
    Query(query): Query<ListPagesQuery>,
) -> Response {
    // Note: workspace_id filter removed - views handle filtering now
    api_response(crate::api::list_pages(state.db.pool(), query.limit, query.offset).await)
}

/// GET /api/pages/:id - Get a single page
pub async fn get_page_handler(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    api_response(crate::api::get_page(state.db.pool(), &id).await)
}

/// GET /api/records/:ontology/:record_id - fetch one raw life-graph record.
pub async fn get_record_handler(
    State(state): State<AppState>,
    Path((ontology, record_id)): Path<(String, String)>,
) -> Response {
    api_response(crate::api::records::get_record(state.db.pool(), &ontology, &record_id).await)
}

/// POST /api/pages - Create a new page
pub async fn create_page_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreatePageRequest>,
) -> Response {
    match crate::api::create_page(state.db.pool(), request).await {
        Ok(page) => (StatusCode::CREATED, Json(page)).into_response(),
        Err(e) => error_response(e),
    }
}

/// PUT /api/pages/:id - Update a page
pub async fn update_page_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::UpdatePageRequest>,
) -> Response {
    api_response(crate::api::update_page(state.db.pool(), &id, request).await)
}

/// DELETE /api/pages/:id - Delete a page
pub async fn delete_page_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::delete_page(state.db.pool(), &id).await {
        Ok(_) => success_message("Page deleted successfully"),
        Err(e) => error_response(e),
    }
}

/// POST /api/pages/:id/append — append a markdown block to a page THROUGH Yjs.
///
/// The synthesis-bridge write path (researcher-plan D4): sending a highlight
/// into a page must not clobber an editor that's currently open on it, so this
/// goes through the authoritative Yjs doc (transaction + broadcast) rather than
/// a REST content replace.
#[derive(Debug, Deserialize)]
pub struct AppendPageRequest {
    pub markdown: String,
}

pub async fn append_page_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<AppendPageRequest>,
) -> Response {
    if req.markdown.trim().is_empty() {
        return error_response(crate::error::Error::InvalidInput(
            "markdown cannot be empty".into(),
        ));
    }
    match state.yjs_state.append_markdown(&id, &req.markdown).await {
        Ok(content) => api_response(Ok(serde_json::json!({ "content": content }))),
        Err(e) => error_response(crate::error::Error::Other(e)),
    }
}

/// GET /api/pages/:id/backlinks - Get inbound references (pages linking here)
pub async fn get_page_backlinks_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::get_page_backlinks(state.db.pool(), &id).await)
}

/// Query params for entity search
#[derive(Debug, Deserialize)]
pub struct EntitySearchQuery {
    pub q: String,
}

/// GET /api/pages/search/refs - Search entities for autocomplete
pub async fn search_refs_handler(
    State(state): State<AppState>,
    Query(query): Query<EntitySearchQuery>,
) -> Response {
    api_response(crate::api::search_refs(state.db.pool(), &query.q).await)
}

// ============================================================================
// Page Sharing Handlers
// ============================================================================

/// POST /api/pages/:id/share - Create or replace a share link for a page
pub async fn create_page_share_handler(
    State(state): State<AppState>,
    Path(page_id): Path<String>,
) -> Response {
    match crate::api::create_page_share(state.db.pool(), &page_id).await {
        Ok(share) => (StatusCode::CREATED, Json(share)).into_response(),
        Err(e) => error_response(e),
    }
}

/// GET /api/pages/:id/share - Get the active share for a page
pub async fn get_page_share_handler(
    State(state): State<AppState>,
    Path(page_id): Path<String>,
) -> Response {
    api_response(crate::api::get_page_share(state.db.pool(), &page_id).await)
}

/// DELETE /api/pages/:id/share - Revoke the share for a page
pub async fn delete_page_share_handler(
    State(state): State<AppState>,
    Path(page_id): Path<String>,
) -> Response {
    match crate::api::delete_page_share(state.db.pool(), &page_id).await {
        Ok(_) => success_message("Share revoked"),
        Err(e) => error_response(e),
    }
}

/// GET /api/s/:token - Get a shared page (public, no auth)
pub async fn get_shared_page_handler(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Response {
    api_response(crate::api::get_shared_page(state.db.pool(), &token).await)
}

/// GET /api/s/:token/files/:file_id - Download a file from a shared page (public, no auth)
/// Validates that the file is referenced by the shared page's content
pub async fn shared_file_download_handler(
    State(state): State<AppState>,
    Path((token, file_id)): Path<(String, String)>,
) -> Response {
    // Validate the share token and that this file belongs to the shared page
    if let Err(e) = crate::api::validate_shared_file(state.db.pool(), &token, &file_id).await {
        return error_response(e);
    }

    // Lake objects use in-memory download
    if crate::api::is_lake_object_id(&file_id) {
        let result =
            crate::api::download_lake_object(state.db.pool(), &state.storage, &file_id).await;
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
                            format!("inline; filename=\"{}\"", filename),
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

    // Regular drive files: stream from storage
    let result = crate::api::download_drive_file_stream(
        state.db.pool(),
        &state.drive_config,
        &file_id,
        None,
    )
    .await;
    match result {
        Ok((file, total, stream)) => {
            let content_type = file
                .mime_type
                .unwrap_or_else(|| "application/octet-stream".to_string());
            let filename = sanitize_content_disposition(&file.filename);
            (
                [
                    (axum::http::header::CONTENT_TYPE, content_type),
                    (
                        axum::http::header::CONTENT_DISPOSITION,
                        format!("inline; filename=\"{}\"", filename),
                    ),
                    (axum::http::header::CONTENT_LENGTH, total.to_string()),
                ],
                axum::body::Body::from_stream(stream),
            )
                .into_response()
        }
        Err(e) => error_response(e),
    }
}

// ============================================================================
// Page Versions Handlers
// ============================================================================

/// Query params for versions list
#[derive(Debug, Deserialize)]
pub struct ListVersionsQuery {
    pub limit: Option<i64>,
}

/// GET /api/pages/:id/versions - List versions for a page
pub async fn list_page_versions_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ListVersionsQuery>,
) -> Response {
    api_response(crate::api::list_versions(state.db.pool(), &id, query.limit).await)
}

/// POST /api/pages/:id/versions - Create a new version snapshot
pub async fn create_page_version_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::CreateVersionRequest>,
) -> Response {
    match crate::api::create_version(state.db.pool(), &id, request).await {
        Ok(version) => (StatusCode::CREATED, Json(version)).into_response(),
        Err(e) => error_response(e),
    }
}

/// GET /api/pages/versions/:version_id - Get a single version (with snapshot for restore)
pub async fn get_page_version_handler(
    State(state): State<AppState>,
    Path(version_id): Path<String>,
) -> Response {
    api_response(crate::api::get_version(state.db.pool(), &version_id).await)
}

// ============================================================================
// Local content search (⌘K)
// ============================================================================

/// POST /api/search/local — content hits for the command palette.
///
/// POST rather than GET because the query is user text: a GET would put what
/// someone is searching their own life for into the URL, where it lands in
/// history and any access log. Same reason the rest of the search surface
/// posts.
pub async fn search_local_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::LocalSearchRequest>,
) -> Response {
    api_response(crate::api::search_local(state.db.pool(), request).await)
}

// ============================================================================
// Bookmarks Handlers (saved web content — data_content_bookmark)
// ============================================================================

/// GET /api/bookmarks — one page of saved bookmarks, newest first.
/// GET /api/bookmarks/:id — one bookmark, for its detail view.
pub async fn get_bookmark_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::get_bookmark(state.db.pool(), &id).await)
}

/// PATCH /api/bookmarks/:id/note — write the user's marginalia.
pub async fn update_bookmark_note_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<crate::api::UpdateNoteRequest>,
) -> Response {
    api_response(crate::api::update_note(state.db.pool(), &id, req).await)
}

pub async fn list_bookmarks_handler(
    State(state): State<AppState>,
    Query(query): Query<crate::api::ListBookmarksQuery>,
) -> Response {
    api_response(crate::api::list_bookmarks(state.db.pool(), query).await)
}

/// POST /api/bookmarks — save a URL (idempotent on canonical URL). The manual
/// capture door; enrichment backfills titles/extraction later.
pub async fn save_bookmark_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::SaveBookmarkRequest>,
) -> Response {
    match crate::api::save_bookmark(state.db.pool(), request).await {
        Ok(saved) => (StatusCode::CREATED, Json(saved)).into_response(),
        Err(e) => error_response(e),
    }
}

// ============================================================================
// Pins Handlers (sidebar pinned URLs)
// ============================================================================

/// GET /api/pins — list all pins, ordered by `sort_order`.
pub async fn list_pins_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_pins(state.db.pool()).await)
}

/// POST /api/pins — pin a URL (idempotent on URL).
pub async fn create_pin_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreatePinRequest>,
) -> Response {
    match crate::api::create_pin(state.db.pool(), request).await {
        Ok(pin) => (StatusCode::CREATED, Json(pin)).into_response(),
        Err(e) => error_response(e),
    }
}

/// PATCH /api/pins/:id — update label / icon / sort_order.
pub async fn update_pin_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::UpdatePinRequest>,
) -> Response {
    api_response(crate::api::update_pin(state.db.pool(), &id, request).await)
}

/// DELETE /api/pins/:id — unpin.
pub async fn delete_pin_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::delete_pin(state.db.pool(), &id).await {
        Ok(_) => success_message("Pin removed"),
        Err(e) => error_response(e),
    }
}

#[derive(serde::Deserialize)]
pub struct ReorderPinsRequest {
    pub urls: Vec<String>,
}

/// PUT /api/pins/reorder — reorder all pins to match the supplied URL list.
pub async fn reorder_pins_handler(
    State(state): State<AppState>,
    Json(request): Json<ReorderPinsRequest>,
) -> Response {
    match crate::api::reorder_pins(state.db.pool(), &request.urls).await {
        Ok(_) => success_message("Pins reordered"),
        Err(e) => error_response(e),
    }
}

// ============================================================================
// Projects Handlers
// ============================================================================

/// GET /api/projects - List all projects
pub async fn list_projects_handler(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<ListProjectsQuery>,
) -> Response {
    api_response(
        crate::api::projects::list_projects(state.db.pool(), q.include_archived.unwrap_or(false))
            .await,
    )
}

#[derive(Deserialize)]
pub struct ListProjectsQuery {
    /// `?include_archived=true` — the projects page, which folds them.
    pub include_archived: Option<bool>,
}

/// POST /api/projects/:id/archive — close a project (kept, out of the working view)
pub async fn archive_project_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::projects::archive_project(state.db.pool(), &id).await {
        Ok(()) => success_message("Project archived"),
        Err(e) => error_response(e),
    }
}

/// POST /api/projects/:id/unarchive
pub async fn unarchive_project_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::projects::unarchive_project(state.db.pool(), &id).await {
        Ok(()) => success_message("Project reopened"),
        Err(e) => error_response(e),
    }
}

/// GET /api/projects/:id - Get a single project with its members
pub async fn get_project_handler(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    api_response(crate::api::projects::get_project(state.db.pool(), &id).await)
}

/// POST /api/projects - Create a project
pub async fn create_project_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::projects::CreateProjectRequest>,
) -> Response {
    match crate::api::projects::create_project(state.db.pool(), request).await {
        Ok(project) => (StatusCode::CREATED, Json(project)).into_response(),
        Err(e) => error_response(e),
    }
}

/// PUT /api/projects/:id - Update a project
pub async fn update_project_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::projects::UpdateProjectRequest>,
) -> Response {
    api_response(crate::api::projects::update_project(state.db.pool(), &id, request).await)
}

/// DELETE /api/projects/:id - Delete a project
pub async fn delete_project_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::projects::delete_project(state.db.pool(), &id).await {
        Ok(_) => success_message("Project deleted"),
        Err(e) => error_response(e),
    }
}

/// POST /api/projects/:id/items - Add a member URL to a project
pub async fn add_project_item_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::projects::AddProjectItemRequest>,
) -> Response {
    match crate::api::projects::add_project_item(state.db.pool(), &id, request).await {
        Ok(item) => (StatusCode::CREATED, Json(item)).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Debug, Deserialize)]
pub struct RemoveProjectItemRequest {
    pub url: String,
}

/// DELETE /api/projects/:id/items - Remove a member URL from a project
pub async fn remove_project_item_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<RemoveProjectItemRequest>,
) -> Response {
    match crate::api::projects::remove_project_item(state.db.pool(), &id, &request.url).await {
        Ok(_) => success_message("Item removed from project"),
        Err(e) => error_response(e),
    }
}

/// PUT /api/projects/:id/items/reorder - Reorder project members
pub async fn reorder_project_items_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::projects::ReorderProjectItemsRequest>,
) -> Response {
    match crate::api::projects::reorder_project_items(state.db.pool(), &id, request).await {
        Ok(_) => success_message("Project items reordered"),
        Err(e) => error_response(e),
    }
}

/// PUT /api/projects/:id/items/role - Set a member's role
pub async fn set_project_item_role_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::projects::SetProjectItemRoleRequest>,
) -> Response {
    match crate::api::projects::set_project_item_role(state.db.pool(), &id, request).await {
        Ok(item) => Json(item).into_response(),
        Err(e) => error_response(e),
    }
}

/// GET /api/projects/:id/graph - Entities referenced across the members
pub async fn project_graph_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::projects::project_graph(state.db.pool(), &id).await {
        Ok(graph) => Json(graph).into_response(),
        Err(e) => error_response(e),
    }
}


// ============================================================================
// Lake API handlers
// ============================================================================

/// GET /api/lake/summary - Get lake summary statistics
pub async fn get_lake_summary_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::lake::get_lake_summary(state.db.pool()).await)
}

/// GET /api/lake/streams - List all streams in the lake
pub async fn list_lake_streams_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::lake::list_lake_streams(state.db.pool()).await)
}

// ============================================================================
// Recently deleted — the trash for chats, pages and projects (`api::trash`)
// ============================================================================

/// GET /api/trash — everything in the trash, all kinds, newest deletion first.
pub async fn list_trash_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::trash::list_trash(state.db.pool()).await)
}

/// POST /api/trash/:kind/:id/restore
pub async fn restore_trash_handler(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
) -> Response {
    let kind = match crate::api::trash::TrashKind::parse(&kind) {
        Ok(k) => k,
        Err(e) => return error_response(e),
    };
    match crate::api::trash::restore(state.db.pool(), kind, &id).await {
        Ok(()) => success_message("Restored"),
        Err(e) => error_response(e),
    }
}

/// DELETE /api/trash/:kind/:id — delete forever. Refuses anything not in the
/// trash; the 30 days cannot be skipped from here.
pub async fn purge_trash_handler(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
) -> Response {
    let kind = match crate::api::trash::TrashKind::parse(&kind) {
        Ok(k) => k,
        Err(e) => return error_response(e),
    };
    match crate::api::trash::purge_trashed(state.db.pool(), kind, &id).await {
        Ok(()) => success_message("Deleted forever"),
        Err(e) => error_response(e),
    }
}

/// POST /api/trash/empty
pub async fn empty_trash_handler(State(state): State<AppState>) -> Response {
    api_response(
        crate::api::trash::empty_trash(state.db.pool())
            .await
            .map(|deleted_count| serde_json::json!({ "deleted_count": deleted_count })),
    )
}

// ============================================================================
// Visits — the frecency log behind ⌘K (`api::visits`)
// ============================================================================

/// POST /api/visits — the owner opened a chat, page or project themselves.
pub async fn record_visit_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::api::visits::RecordVisitRequest>,
) -> Response {
    match crate::api::visits::record_visit(state.db.pool(), req).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => error_response(e),
    }
}

/// GET /api/visits/frecency — every visited record's score, highest first.
pub async fn frecency_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::visits::frecency(state.db.pool()).await)
}

#[cfg(test)]
mod range_tests {
    use super::drive::{resolve_range, RangeOutcome};

    #[test]
    fn no_header_serves_full() {
        assert_eq!(resolve_range(None, 100), RangeOutcome::Full);
    }

    #[test]
    fn bounded_range() {
        assert_eq!(
            resolve_range(Some("bytes=0-49"), 100),
            RangeOutcome::Partial(0, 50)
        );
        assert_eq!(
            resolve_range(Some("bytes=10-19"), 100),
            RangeOutcome::Partial(10, 10)
        );
        // End past EOF clamps
        assert_eq!(
            resolve_range(Some("bytes=90-199"), 100),
            RangeOutcome::Partial(90, 10)
        );
    }

    #[test]
    fn open_ended_range() {
        assert_eq!(
            resolve_range(Some("bytes=40-"), 100),
            RangeOutcome::Partial(40, 60)
        );
    }

    #[test]
    fn suffix_range() {
        assert_eq!(
            resolve_range(Some("bytes=-10"), 100),
            RangeOutcome::Partial(90, 10)
        );
        // Suffix longer than the object serves the whole object
        assert_eq!(
            resolve_range(Some("bytes=-500"), 100),
            RangeOutcome::Partial(0, 100)
        );
    }

    #[test]
    fn unsatisfiable_ranges() {
        assert_eq!(
            resolve_range(Some("bytes=100-"), 100),
            RangeOutcome::Unsatisfiable
        );
        assert_eq!(
            resolve_range(Some("bytes=200-300"), 100),
            RangeOutcome::Unsatisfiable
        );
        assert_eq!(
            resolve_range(Some("bytes=-0"), 100),
            RangeOutcome::Unsatisfiable
        );
        // Any range against an empty object is unsatisfiable
        assert_eq!(
            resolve_range(Some("bytes=0-10"), 0),
            RangeOutcome::Unsatisfiable
        );
        assert_eq!(
            resolve_range(Some("bytes=-5"), 0),
            RangeOutcome::Unsatisfiable
        );
    }

    #[test]
    fn ignored_forms_serve_full() {
        // Multi-range: permitted to ignore, serve 200
        assert_eq!(resolve_range(Some("bytes=0-1,5-9"), 100), RangeOutcome::Full);
        // Malformed
        assert_eq!(resolve_range(Some("bytes=abc-def"), 100), RangeOutcome::Full);
        assert_eq!(resolve_range(Some("bytes=50-10"), 100), RangeOutcome::Full);
        assert_eq!(resolve_range(Some("bytes=-"), 100), RangeOutcome::Full);
        assert_eq!(resolve_range(Some("items=0-10"), 100), RangeOutcome::Full);
    }
}

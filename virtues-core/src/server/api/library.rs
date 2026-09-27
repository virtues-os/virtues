//! The library: bookmarks, pins, projects, the lake, recently deleted and
//! the visits log.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;

use super::{api_response, error_response, success_message};
use crate::server::AppState;

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

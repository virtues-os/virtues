//! Pages: the documents, sharing, versions, raw records and local search.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json,
    Router,
};
use serde::Deserialize;

use super::{api_response, error_response, success_message};
use crate::server::AppState;

/// This area's authenticated routes. Merged into the protected router, whose
/// `route_layer` requires a resolved `AuthUser`.
pub fn routes() -> Router<AppState> {
    Router::new()
        // Local content search — the ⌘K palette. Never leaves the box.
        .route("/api/search/local", post(search_local_handler))
        // Pages API
        .route(
            "/api/pages",
            get(list_pages_handler).post(create_page_handler),
        )
        .route(
            "/api/pages/search/refs",
            get(search_refs_handler),
        )
        .route(
            "/api/pages/:id",
            get(get_page_handler)
                .put(update_page_handler)
                .delete(delete_page_handler),
        )
        // Raw record viewer — one life-graph row by (ontology, id)
        .route(
            "/api/records/:ontology/:record_id",
            get(get_record_handler),
        )
        // Page References (backlinks) API
        .route(
            "/api/pages/:id/backlinks",
            get(get_page_backlinks_handler),
        )
        // Append a markdown block through Yjs (safe with an open editor) — the
        // synthesis bridge's write path.
        .route("/api/pages/:id/append", post(append_page_handler))
        // Page Versions API
        .route(
            "/api/pages/:id/versions",
            get(list_page_versions_handler).post(create_page_version_handler),
        )
        .route(
            "/api/pages/versions/:version_id",
            get(get_page_version_handler),
        )
        // Yjs WebSocket (real-time collaborative editing)
        .route("/ws/yjs/:page_id", get(crate::server::yjs::yjs_websocket_handler))
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
    api_response(crate::api::pages::list_pages(state.db.pool(), query.limit, query.offset).await)
}

/// GET /api/pages/:id - Get a single page
pub async fn get_page_handler(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    api_response(crate::api::pages::get_page(state.db.pool(), &id).await)
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
    Json(request): Json<crate::api::pages::CreatePageRequest>,
) -> Response {
    match crate::api::pages::create_page(state.db.pool(), request).await {
        Ok(page) => (StatusCode::CREATED, Json(page)).into_response(),
        Err(e) => error_response(e),
    }
}

/// PUT /api/pages/:id - Update a page
pub async fn update_page_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::pages::UpdatePageRequest>,
) -> Response {
    api_response(crate::api::pages::update_page(state.db.pool(), &id, request).await)
}

/// DELETE /api/pages/:id - Delete a page
pub async fn delete_page_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::pages::delete_page(state.db.pool(), &id).await {
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
    api_response(crate::api::pages::get_page_backlinks(state.db.pool(), &id).await)
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
    api_response(crate::api::pages::search_refs(state.db.pool(), &query.q).await)
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
    api_response(crate::api::pages::list_versions(state.db.pool(), &id, query.limit).await)
}

/// POST /api/pages/:id/versions - Create a new version snapshot
pub async fn create_page_version_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::pages::CreateVersionRequest>,
) -> Response {
    match crate::api::pages::create_version(state.db.pool(), &id, request).await {
        Ok(version) => (StatusCode::CREATED, Json(version)).into_response(),
        Err(e) => error_response(e),
    }
}

/// GET /api/pages/versions/:version_id - Get a single version (with snapshot for restore)
pub async fn get_page_version_handler(
    State(state): State<AppState>,
    Path(version_id): Path<String>,
) -> Response {
    api_response(crate::api::pages::get_version(state.db.pool(), &version_id).await)
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
    Json(request): Json<crate::api::search_local::LocalSearchRequest>,
) -> Response {
    api_response(crate::api::search_local::search_local(state.db.pool(), request).await)
}

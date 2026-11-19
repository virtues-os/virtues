//! Deleting ingested records (`crate::api::deletions`): the routes the chat's
//! deletion card and the table viewer call.
//!
//! Allowing a deletion and deleting rows in the table viewer both need the
//! app's owner proof, as the admin applet's commands do: any process on a
//! paired Mac reaches the server as that Mac, and only the app can sign.
//! Reading a deletion, setting one aside and lifting its tombstones do not.

use axum::{
    extract::{OriginalUri, Path, State},
    http::HeaderMap,
    response::Response,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;

use super::api_response;
use crate::api::deletions;
use crate::middleware::auth::AuthUser;
use crate::server::AppState;

/// This area's authenticated routes. Merged into the protected router, whose
/// `route_layer` requires a resolved `AuthUser`.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/deletions/:id", get(get_handler))
        .route("/api/deletions/:id/allow", post(allow_handler))
        .route("/api/deletions/:id/cancel", post(cancel_handler))
        .route("/api/deletions/:id/tombstones", delete(lift_handler))
        .route("/api/developer/rows/delete", post(delete_rows_handler))
}

/// GET /api/deletions/:id — a deletion as it stands: what it would remove
/// while it waits, the receipt once it has run.
async fn get_handler(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    api_response(deletions::get(state.db.pool(), &id).await)
}

/// POST /api/deletions/:id/allow — the owner allows it, and it runs.
async fn allow_handler(
    State(state): State<AppState>,
    user: AuthUser,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Err(refused) = crate::api::owner_proof::check(state.db.pool(), &user, "POST", uri.path(), &headers).await {
        return refused;
    }
    api_response(deletions::approve(state.db.pool(), &id).await)
}

/// POST /api/deletions/:id/cancel — the owner sets it aside.
async fn cancel_handler(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    api_response(deletions::cancel(state.db.pool(), &id).await)
}

/// DELETE /api/deletions/:id/tombstones — let its records be collected again.
async fn lift_handler(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    api_response(
        deletions::lift(state.db.pool(), &id)
            .await
            .map(|lifted| serde_json::json!({ "lifted": lifted })),
    )
}

#[derive(Debug, Deserialize)]
struct DeleteRows {
    table: String,
    ids: Vec<String>,
}

/// POST /api/developer/rows/delete — delete and tombstone rows of one data
/// table, picked by hand in the table viewer.
async fn delete_rows_handler(
    State(state): State<AppState>,
    user: AuthUser,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Json(request): Json<DeleteRows>,
) -> Response {
    if let Err(refused) = crate::api::owner_proof::check(state.db.pool(), &user, "POST", uri.path(), &headers).await {
        return refused;
    }
    api_response(
        deletions::delete_rows(state.db.pool(), &request.table, &request.ids)
            .await
            .map(|removed| removed.receipt()),
    )
}

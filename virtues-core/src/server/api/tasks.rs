//! Tasks: the shared to-do list (`crate::api::tasks`). Every change made
//! here is signed by the owner; the assistant writes through its tools.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use super::{api_response, error_response};
use crate::api::tasks::{self, ListFilter, NewTask, Origin, TaskPatch, Writer};
use crate::server::AppState;

/// This area's authenticated routes. Merged into the protected router, whose
/// `route_layer` requires a resolved `AuthUser`.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/tasks", get(list_tasks_handler).post(create_task_handler))
        .route("/api/tasks/:id", get(get_task_handler).patch(update_task_handler))
        .route("/api/tasks/:id/notes", post(add_note_handler))
        .route("/api/tasks/:id/events", get(list_events_handler))
}

/// GET /api/tasks?status=open|closed|all&assignee=&project_id=
async fn list_tasks_handler(
    State(state): State<AppState>,
    Query(filter): Query<ListFilter>,
) -> Response {
    api_response(tasks::list_tasks(state.db.pool(), &filter).await)
}

/// POST /api/tasks — one task, typed into the list.
async fn create_task_handler(
    State(state): State<AppState>,
    Json(request): Json<NewTask>,
) -> Response {
    match tasks::create_tasks(state.db.pool(), vec![request], Origin::Owner, &Writer::owner()).await
    {
        Ok(mut created) => (StatusCode::CREATED, Json(created.remove(0))).into_response(),
        Err(e) => error_response(e),
    }
}

async fn get_task_handler(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    api_response(tasks::get_task(state.db.pool(), &id).await)
}

/// PATCH /api/tasks/:id — any field, including status.
async fn update_task_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(patch): Json<TaskPatch>,
) -> Response {
    api_response(tasks::update_task(state.db.pool(), &id, patch, &Writer::owner()).await)
}

#[derive(Debug, Deserialize)]
struct NoteRequest {
    text: String,
}

async fn add_note_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<NoteRequest>,
) -> Response {
    match tasks::add_note(state.db.pool(), &id, &request.text, &Writer::owner()).await {
        Ok(event) => (StatusCode::CREATED, Json(event)).into_response(),
        Err(e) => error_response(e),
    }
}

async fn list_events_handler(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    api_response(tasks::list_events(state.db.pool(), &id).await)
}

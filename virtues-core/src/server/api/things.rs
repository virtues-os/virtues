//! Things: what goes on in a life and the person named (`crate::api::things`).
//!
//! What sits under a thing is read through the routes that own it: the
//! projects list carries each project's `thing_id`, and
//! `GET /api/tasks?thing_id=` lists a thing's tasks.

use axum::{
    extract::{Path, State},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use super::{api_response, error_response, success_message};
use crate::api::things::{self, ThingFields};
use crate::server::AppState;

/// This area's authenticated routes. Merged into the protected router, whose
/// `route_layer` requires a resolved `AuthUser`.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/wiki/things", get(list_things_handler).post(create_thing_handler))
        .route(
            "/api/wiki/thing/:id",
            get(get_thing_handler).put(update_thing_handler).delete(delete_thing_handler),
        )
        .route("/api/wiki/thing/:id/article", post(start_thing_article_handler))
}

async fn list_things_handler(State(state): State<AppState>) -> Response {
    api_response(things::list_things(state.db.pool()).await)
}

/// A thing and its article's prose, for its page.
async fn get_thing_handler(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    api_response(things::get_thing_detail(state.db.pool(), &id).await)
}

#[derive(Debug, Deserialize)]
struct NewThing {
    title: String,
}

/// Name a thing. Only the person may: the editor's constitution forbids it
/// from creating a subject.
async fn create_thing_handler(
    State(state): State<AppState>,
    Json(new): Json<NewThing>,
) -> Response {
    api_response(things::create_thing(state.db.pool(), &new.title).await)
}

async fn update_thing_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(fields): Json<ThingFields>,
) -> Response {
    api_response(things::update_thing(state.db.pool(), &id, &fields).await)
}

async fn delete_thing_handler(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match things::delete_thing(state.db.pool(), &id).await {
        Ok(()) => success_message("Removed"),
        Err(e) => error_response(e),
    }
}

/// Give a thing a page, seeded with the person's own sentence for the editor
/// to fill by searching.
async fn start_thing_article_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(things::start_article(state.db.pool(), &id).await)
}

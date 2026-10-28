//! Connections to other services' MCP servers: list, search the registry,
//! sign in, sign out (`tools/connection.rs`, agents/plan/mcp-plan.md part 1).
//!
//! A sign-in starts here. The service sends the owner's browser back to the
//! OAuth proxy, and the box collects the code from there
//! (`tools/connection.rs`).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;

use crate::server::AppState;
use crate::tools::connection::{self, Connection};
use crate::tools::connection_registry::{self as registry, Listing};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/connections", get(list_handler))
        .route("/api/connections/search", get(search_handler))
        .route("/api/connections/connect", post(connect_handler))
        .route("/api/connections/:id", delete(disconnect_handler))
}

/// What is connected, and the featured servers. A registry that cannot be
/// reached leaves `featured` empty and says why in `registry_error`; the
/// connected list does not depend on it.
async fn list_handler(State(pool): State<PgPool>) -> Response {
    let connected = match connection::connected(&pool).await {
        Ok(c) => c,
        Err(e) => return refuse(StatusCode::SERVICE_UNAVAILABLE, &format!("Could not read the connections: {e}")),
    };
    let (featured, registry_error) = match registry::featured().await {
        Ok(f) => (offered(f, &connected), None),
        Err(e) => (Vec::new(), Some(format!("Could not reach the MCP Registry: {e:#}"))),
    };
    Json(json!({ "connected": connected, "featured": featured, "registry_error": registry_error })).into_response()
}

/// Listings with whether each is already connected.
fn offered(listings: Vec<Listing>, connected: &[Connection]) -> Vec<serde_json::Value> {
    listings
        .into_iter()
        .map(|l| {
            let is_connected = connected.iter().any(|c| c.url == l.url);
            let mut v = json!(l);
            v["is_connected"] = json!(is_connected);
            v
        })
        .collect()
}

#[derive(Deserialize)]
struct Search {
    q: String,
}

async fn search_handler(State(pool): State<PgPool>, Query(search): Query<Search>) -> Response {
    if search.q.trim().is_empty() {
        return Json(json!({ "results": [] })).into_response();
    }
    let connected = match connection::connected(&pool).await {
        Ok(c) => c,
        Err(e) => return refuse(StatusCode::SERVICE_UNAVAILABLE, &format!("Could not read the connections: {e}")),
    };
    match registry::search(&search.q).await {
        Ok(found) => Json(json!({ "results": offered(found, &connected) })).into_response(),
        Err(e) => refuse(StatusCode::BAD_GATEWAY, &format!("Could not search the MCP Registry: {e:#}")),
    }
}

/// An app that predates the proxy return also sends `origin`, which is
/// ignored.
#[derive(Deserialize)]
struct Connect {
    /// A registry name, such as `app.linear/linear`.
    registry: Option<String>,
    /// Or a server address the owner pasted.
    url: Option<String>,
}

async fn connect_handler(State(pool): State<PgPool>, Json(ask): Json<Connect>) -> Response {
    let taken = match connection::connected(&pool).await {
        Ok(c) => c,
        Err(e) => return refuse(StatusCode::SERVICE_UNAVAILABLE, &format!("Could not read the connections: {e}")),
    };
    let c = match (ask.registry.as_deref(), ask.url.as_deref()) {
        (Some(name), _) => match registry::lookup(name).await {
            Ok(Some(listing)) => Connection::listed(listing, &taken),
            Ok(None) => {
                return refuse(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    &format!("{name} needs more than a sign-in, such as an API key, so it cannot be connected yet."),
                )
            }
            Err(e) => return refuse(StatusCode::BAD_GATEWAY, &format!("Could not look up {name}: {e:#}")),
        },
        (None, Some(url)) => match Connection::pasted(url, &taken) {
            Ok(c) => c,
            Err(e) => return refuse(StatusCode::BAD_REQUEST, &format!("{e} Check the server address and try again.")),
        },
        (None, None) => return refuse(StatusCode::BAD_REQUEST, "Name a registry server or give a server address."),
    };
    let name = c.name.clone();
    match connection::start(&pool, c).await {
        Ok(auth_url) => Json(json!({ "auth_url": auth_url })).into_response(),
        Err(e) => refuse(StatusCode::BAD_GATEWAY, &format!("Could not start the sign-in with {name}: {e:#}")),
    }
}

async fn disconnect_handler(State(pool): State<PgPool>, Path(id): Path<String>) -> Response {
    match connection::disconnect(&pool, &id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => refuse(StatusCode::SERVICE_UNAVAILABLE, &format!("Could not forget the sign-in: {e}")),
    }
}

fn refuse(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

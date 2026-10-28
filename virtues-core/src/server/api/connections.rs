//! Connections to other services' MCP servers: list, search the registry,
//! sign in, sign out (`tools/connection.rs`, agents/plan/mcp-plan.md part 1).
//!
//! A sign-in starts here and comes back here: the owner's app sends the
//! origin it reaches the box by, the box registers that origin's
//! [`CALLBACK_PATH`] with the service, and the service sends the owner's
//! browser back to it with the code.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;

use crate::server::AppState;
use crate::tools::connection::{self, Connection, CALLBACK_PATH};
use crate::tools::connection_registry::{self as registry, Listing};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/connections", get(list_handler))
        .route("/api/connections/search", get(search_handler))
        .route("/api/connections/connect", post(connect_handler))
        .route("/api/connections/:id", delete(disconnect_handler))
        .route(CALLBACK_PATH, get(callback_handler))
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

#[derive(Deserialize)]
struct Connect {
    /// The origin the owner's app reaches the box by, such as
    /// `http://127.0.0.1:7117`.
    origin: String,
    /// A registry name, such as `app.linear/linear`.
    registry: Option<String>,
    /// Or a server address the owner pasted.
    url: Option<String>,
}

async fn connect_handler(State(pool): State<PgPool>, Json(ask): Json<Connect>) -> Response {
    let Some(origin) = origin(&ask.origin) else {
        return refuse(StatusCode::BAD_REQUEST, "The origin must be an http or https address with no path.");
    };
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
    match connection::start(&pool, c, &format!("{origin}{CALLBACK_PATH}")).await {
        Ok(auth_url) => Json(json!({ "auth_url": auth_url })).into_response(),
        Err(e) => refuse(StatusCode::BAD_GATEWAY, &format!("Could not start the sign-in with {name}: {e:#}")),
    }
}

/// `origin` as a bare origin, or `None` when it is not one.
fn origin(origin: &str) -> Option<String> {
    let url = url::Url::parse(origin.trim()).ok()?;
    let bare = matches!(url.scheme(), "http" | "https")
        && url.host().is_some()
        && matches!(url.path(), "" | "/")
        && url.query().is_none()
        && url.fragment().is_none();
    bare.then(|| url.origin().ascii_serialization())
}

async fn disconnect_handler(State(pool): State<PgPool>, Path(id): Path<String>) -> Response {
    match connection::disconnect(&pool, &id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => refuse(StatusCode::SERVICE_UNAVAILABLE, &format!("Could not forget the sign-in: {e}")),
    }
}

#[derive(Deserialize)]
struct Callback {
    code: Option<String>,
    state: Option<String>,
    iss: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

async fn callback_handler(Query(back): Query<Callback>) -> Response {
    if let Some(error) = back.error {
        let why = back.error_description.unwrap_or(error);
        return page(StatusCode::BAD_REQUEST, &format!("The sign-in did not finish: {why}. Close this tab and try again."));
    }
    let (Some(code), Some(state)) = (back.code, back.state) else {
        return page(StatusCode::BAD_REQUEST, "This address needs the code the service sends back. Start the sign-in again.");
    };
    match connection::finish(&code, &state, back.iss.as_deref()).await {
        Ok(c) => page(StatusCode::OK, &format!("{} is connected. You can close this tab.", c.name)),
        Err(e) => page(StatusCode::BAD_GATEWAY, &format!("The sign-in did not finish: {e:#}")),
    }
}

/// The page the owner's browser lands on after a sign-in.
fn page(status: StatusCode, message: &str) -> Response {
    let message = message.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let html = format!(
        "<!doctype html><meta charset=utf-8><meta name=viewport content=\"width=device-width\">\
         <title>Virtues</title><body style=\"font:16px system-ui;margin:3rem auto;max-width:32rem;padding:0 1rem\">\
         <p>{message}</p></body>"
    );
    (status, Html(html)).into_response()
}

fn refuse(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_bare_web_origin_is_taken() {
        assert_eq!(origin("http://127.0.0.1:7117").as_deref(), Some("http://127.0.0.1:7117"));
        assert_eq!(origin("http://localhost:5173/").as_deref(), Some("http://localhost:5173"));
        assert_eq!(origin("http://localhost:5173/x"), None);
        assert_eq!(origin("virtues://localhost"), None);
        assert_eq!(origin("javascript:alert(1)"), None);
    }
}

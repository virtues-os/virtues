//! Connections to other services' MCP servers: list, sign in, sign out
//! (`tools/connection.rs`, agents/plan/mcp-plan.md part 1).
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
use crate::tools::connection::{self, CALLBACK_PATH, CATALOG};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/connections", get(list_handler))
        .route("/api/connections/:id/connect", post(connect_handler))
        .route("/api/connections/:id", delete(disconnect_handler))
        .route(CALLBACK_PATH, get(callback_handler))
}

async fn list_handler(State(pool): State<PgPool>) -> Response {
    let mut out = Vec::new();
    for c in CATALOG {
        match connection::is_connected(&pool, c).await {
            Ok(connected) => out.push(json!({ "id": c.id, "name": c.name, "about": c.about, "connected": connected })),
            Err(e) => return refuse(StatusCode::SERVICE_UNAVAILABLE, &format!("Could not read the connections: {e}")),
        }
    }
    Json(json!({ "connections": out })).into_response()
}

#[derive(Deserialize)]
struct Connect {
    /// The origin the owner's app reaches the box by, such as
    /// `http://127.0.0.1:7117`.
    origin: String,
}

async fn connect_handler(State(pool): State<PgPool>, Path(id): Path<String>, Json(ask): Json<Connect>) -> Response {
    let Some(c) = connection::find(&id) else {
        return refuse(StatusCode::NOT_FOUND, &format!("There is no connection called {id}."));
    };
    let Some(origin) = origin(&ask.origin) else {
        return refuse(StatusCode::BAD_REQUEST, "The origin must be an http or https address with no path.");
    };
    match connection::start(&pool, c, &format!("{origin}{CALLBACK_PATH}")).await {
        Ok(auth_url) => Json(json!({ "auth_url": auth_url })).into_response(),
        Err(e) => refuse(StatusCode::BAD_GATEWAY, &format!("Could not start the sign-in with {}: {e:#}", c.name)),
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
    let Some(c) = connection::find(&id) else {
        return refuse(StatusCode::NOT_FOUND, &format!("There is no connection called {id}."));
    };
    match connection::disconnect(&pool, c).await {
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

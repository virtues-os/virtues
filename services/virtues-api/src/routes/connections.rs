//! The return leg of a box's MCP connection sign-in (the box side is
//! `virtues-core/src/tools/connection.rs`).
//!
//!   GET  /connections/callback       — the service sends the owner's browser
//!                                       here with `code` and `state`; keep
//!                                       them and tell the owner to go back,
//!                                       or close the iOS app's sign-in sheet
//!   POST /connections/claim/{state}  — the box collects them, once
//!
//! The box cannot be the redirect URI itself. On a phone the app reaches the
//! box through a listener inside the app, and Safari cannot reach that, so a
//! sign-in that tried to return there failed. Every box registers this one
//! address instead.
//!
//! Nothing here can finish a sign-in. The box signs in with PKCE as a public
//! client, so the code is worth nothing without the verifier the box kept.
//! `state` is the box's random sign-in id, so only the box that started the
//! sign-in knows what to claim.

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::AppState;

/// How long a code waits to be claimed. The box gives a sign-in ten minutes.
const RETURN_TTL_SECS: f64 = 600.0;

/// Sending the iOS app's sign-in sheet here closes it
/// (`apps/web/plugins/location-probe/ios/Sources/SignInSheet.swift`). Only the
/// sheet listens for this scheme, so it carries nothing.
const CLOSE_SHEET: &str = "virtues-signin://done";

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/connections/callback", get(callback))
        .route("/connections/claim/:state", post(claim))
}

#[derive(Deserialize)]
struct Back {
    code: Option<String>,
    state: Option<String>,
    iss: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
    /// `1` when the box registered this address for a sign-in the iOS app
    /// opened in its sign-in sheet.
    sheet: Option<String>,
}

async fn callback(State(state): State<Arc<AppState>>, Query(back): Query<Back>) -> axum::response::Response {
    let Some(sign_in) = back.state.filter(|s| !s.is_empty() && s.len() <= 512) else {
        return page(StatusCode::BAD_REQUEST, "This address needs the sign-in the service sends back. Start the sign-in again in Virtues.");
    };
    let error = back.error.map(|e| back.error_description.unwrap_or(e));
    if back.code.is_none() && error.is_none() {
        return page(StatusCode::BAD_REQUEST, "The service sent no code. Start the sign-in again in Virtues.");
    }

    // The callback is the only inserter, so it sweeps what expired.
    if let Err(e) = sqlx::query("DELETE FROM connection_return WHERE expires_at < now()")
        .execute(&state.db)
        .await
    {
        tracing::warn!(error = %e, "connection_return sweep failed");
    }
    // A second arrival for one sign-in (a reload) keeps the first.
    let stored = sqlx::query(
        "INSERT INTO connection_return (state, code, issuer, error, expires_at)
         VALUES ($1, $2, $3, $4, now() + make_interval(secs => $5))
         ON CONFLICT (state) DO NOTHING",
    )
    .bind(&sign_in)
    .bind(&back.code)
    .bind(&back.iss)
    .bind(&error)
    .bind(RETURN_TTL_SECS)
    .execute(&state.db)
    .await;
    if let Err(e) = stored {
        tracing::error!(error = %e, "connection_return insert failed");
        return page(StatusCode::INTERNAL_SERVER_ERROR, "Virtues could not pass the sign-in to your server. Start it again in Virtues.");
    }

    // The app reloads its list when the sheet closes. A refused sign-in
    // leaves the service unconnected there.
    if back.sheet.as_deref() == Some("1") {
        return Redirect::to(CLOSE_SHEET).into_response();
    }
    match error {
        Some(why) => page(StatusCode::OK, &format!("The sign-in did not finish: {why}. Go back to Virtues and try again.")),
        None => page(StatusCode::OK, "You're signed in. Go back to Virtues; your server is finishing the connection."),
    }
}

/// The box collects what the service sent, once. 404 means nothing has
/// arrived yet, or it expired, and the box asks again.
async fn claim(State(state): State<Arc<AppState>>, Path(sign_in): Path<String>) -> axum::response::Response {
    let row: Result<Option<(Option<String>, Option<String>, Option<String>)>, _> = sqlx::query_as(
        "DELETE FROM connection_return WHERE state = $1 AND expires_at > now()
         RETURNING code, issuer, error",
    )
    .bind(&sign_in)
    .fetch_optional(&state.db)
    .await;
    match row {
        Ok(Some((code, issuer, error))) => Json(json!({ "code": code, "iss": issuer, "error": error })).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({ "error": "nothing to claim yet" }))).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "connection_return claim failed");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "claim failed" }))).into_response()
        }
    }
}

/// The page the owner's browser lands on.
fn page(status: StatusCode, message: &str) -> axum::response::Response {
    let message = message.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let html = format!(
        "<!doctype html><html lang=en><meta charset=utf-8>\
         <meta name=viewport content=\"width=device-width,initial-scale=1\">\
         <meta name=referrer content=no-referrer><title>Virtues</title>\
         <body style=\"font:16px system-ui;margin:3rem auto;max-width:32rem;padding:0 1rem\">\
         <p>{message}</p></body></html>"
    );
    (status, [(header::CONTENT_TYPE, "text/html; charset=utf-8")], html).into_response()
}

#[cfg(test)]
mod tests {
    /// The proxy's providers sit at `/{provider}/…`, so these routes must
    /// merge beside them; axum panics at startup if two routes conflict.
    #[test]
    fn the_return_routes_sit_beside_the_oauth_providers() {
        let _ = super::super::oauth::router().merge(super::router());
    }
}

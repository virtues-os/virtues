//! Session probe — "who am I?" for the web UI.
//!
//! There is no signin/signout and no password/email auth. Authentication happens
//! at the transport/credential layer (the `AuthUser` extractor: the proven iroh
//! EndpointId, loopback console, or the dev fallback). This endpoint only reports
//! whether the caller is authenticated so the UI can render the app shell vs. a
//! pair prompt.

use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;

use crate::middleware::auth::AuthUser;

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub user: Option<SessionUser>,
}

#[derive(Debug, Serialize)]
pub struct SessionUser {
    pub id: String,
    pub device_id: String,
    pub device_label: String,
}

/// `GET /auth/session` — "who am I?" probe used by the web UI to decide whether
/// to render the app shell or the pair prompt. Authenticated via the `AuthUser`
/// extractor (iroh key / loopback / dev); returns the caller or `null`.
pub async fn session_handler(user: Option<AuthUser>) -> impl IntoResponse {
    let user = user.map(|u| SessionUser {
        id: u.id,
        device_id: u.device_id,
        device_label: u.device_label,
    });
    (StatusCode::OK, Json(SessionResponse { user }))
}

//! Setup-wizard endpoints (agents/build/onboarding.md) — the web ports of the CLI's
//! account flows.
//!
//! The wizard runs in a phone/laptop browser after the pair-token claim, so
//! every handler here takes `AuthUser` (session cookie). The underlying
//! device-link machinery (`virtues_api::link`) was built for exactly this:
//! `start` seals the secret `device_code` as a box secret, so each poll can
//! be an independent HTTP request — no server-side wizard session.
//!
//! Endpoints:
//!   POST /api/setup/login/start      → {email}: magic link that links this
//!                                      box to that address's account (a new
//!                                      address creates one)
//!   POST /api/setup/link/poll        → one poll tick; on `ready` the billing
//!                                      api_key is stored
//!
//! The box keeps its default `virtues.local` name — there is no rename
//! endpoint: the name is cosmetic and reachability is WireGuard/SPKI +
//! localhost, not mDNS.
//!
//! The wizard reads overall progress from the public `/api/setup/state`
//! (box_status.rs) — these endpoints only *drive* transitions.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::json;

use crate::middleware::auth::AuthUser;
use crate::server::webhook::AppState;
use crate::virtues_api::link::{self, LinkStatus, LoginStart};

fn atlas_url() -> String {
    crate::virtues_api::atlas_url()
}

#[derive(Deserialize)]
pub struct LoginStartRequest {
    pub email: String,
}

/// `POST /api/setup/login/start` — begin the existing-account branch: send a
/// magic link to `email`. Ensures a device link is in flight first (the email's
/// confirm page approves that same link, and the shared poll collects it).
///
/// Answers with the link's `user_code`, which the email names too, so the page
/// can show the owner what to match.
pub async fn login_start_handler(
    _user: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<LoginStartRequest>,
) -> impl IntoResponse {
    let email = body.email.trim().to_string();
    if !email.contains('@') || !email.contains('.') {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid_email"})),
        )
            .into_response();
    }

    let pool = state.db.pool();
    let http = crate::http_client::virtues_api_client();
    let atlas = atlas_url();

    // The login call binds to an in-flight device link; mint one if absent.
    // (Idempotent from the wizard's perspective — re-starting just rotates
    // the pending link.)
    let user_code = match link::start(pool, &http, &atlas).await {
        Ok(started) => started.user_code,
        Err(e) => {
            tracing::warn!(error = %e, "setup login: link start failed");
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "atlas_unreachable", "detail": e.to_string()})),
            )
                .into_response();
        }
    };

    match link::login(pool, &http, &atlas, &email).await {
        Ok(LoginStart::Sent) => {
            (StatusCode::OK, Json(json!({"status": "sent", "user_code": user_code}))).into_response()
        }
        Ok(LoginStart::RateLimited) => {
            (StatusCode::OK, Json(json!({"status": "rate_limited"}))).into_response()
        }
        Err(e) => {
            tracing::warn!(error = %e, "setup login start failed");
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "login_failed", "detail": e.to_string()})),
            )
                .into_response()
        }
    }
}

/// `POST /api/setup/link/poll` — one poll tick for whichever branch is in
/// flight. On `ready` the api_key is stored; the
/// page sees `ready` and advances.
pub async fn link_poll_handler(
    _user: AuthUser,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let http = crate::http_client::virtues_api_client();
    match link::poll(state.db.pool(), &http, &atlas_url()).await {
        Ok(status) => {
            let s = match status {
                LinkStatus::Pending => "pending",
                LinkStatus::Ready => "ready",
                LinkStatus::Expired => "expired",
                LinkStatus::None => "none",
            };
            (StatusCode::OK, Json(json!({"status": s}))).into_response()
        }
        Err(e) => {
            tracing::warn!(error = %e, "setup link poll failed");
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "poll_failed", "detail": e.to_string()})),
            )
                .into_response()
        }
    }
}

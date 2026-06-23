//! Setup-wizard endpoints (docs/onboarding.md) — the web ports of the CLI's
//! account flows.
//!
//! The wizard runs in a phone/laptop browser after the pair-token claim, so
//! every handler here takes `AuthUser` (session cookie). The underlying
//! device-link machinery (`virtues_api::link`) was built for exactly this:
//! `start` seals the secret `device_code` in `box_secrets`, so each poll can
//! be an independent HTTP request — no server-side wizard session.
//!
//! Endpoints:
//!   POST /api/setup/subscribe/start  → start a device link, return the
//!                                      Stripe-checkout URL bits (create-new)
//!   POST /api/setup/login/start      → {email}: magic-link to an existing
//!                                      subscription (reuses the same link)
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

/// `POST /api/setup/subscribe/start` — begin the create-new-account branch.
/// Returns the user-facing checkout bits; the secret device_code stays sealed
/// box-side. The page then polls `/api/setup/link/poll`.
pub async fn subscribe_start_handler(
    _user: AuthUser,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let http = crate::http_client::virtues_api_client();
    match link::start(state.db.pool(), &http, &atlas_url()).await {
        Ok(start) => (StatusCode::OK, Json(json!(start))).into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "setup subscribe start failed");
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "atlas_unreachable", "detail": e.to_string()})),
            )
                .into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct LoginStartRequest {
    pub email: String,
}

/// `POST /api/setup/login/start` — begin the existing-account branch: send a
/// magic link to `email`. Ensures a device link is in flight first (the email
/// click flips that same link to `ready`, picked up by the shared poll).
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
    if let Err(e) = link::start(pool, &http, &atlas).await {
        tracing::warn!(error = %e, "setup login: link start failed");
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "atlas_unreachable", "detail": e.to_string()})),
        )
            .into_response();
    }

    match link::login(pool, &http, &atlas, &email).await {
        Ok(LoginStart::Sent) => (StatusCode::OK, Json(json!({"status": "sent"}))).into_response(),
        Ok(LoginStart::NoAccount) => {
            (StatusCode::OK, Json(json!({"status": "no_account"}))).into_response()
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
/// flight. On `ready` the api_key is stored (atlas funds the wallet); the
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

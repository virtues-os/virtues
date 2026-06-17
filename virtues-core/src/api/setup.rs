//! Setup-wizard endpoints (docs/onboarding.md) — the web ports of the CLI's
//! account flows, plus the box-rename step.
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
//!                                      token is stored + first bearer minted
//!   POST /api/setup/name             → {name}: validate + rename the box
//!                                      (hostnamectl + avahi via the sudoers
//!                                      seam the installer writes)
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
    std::env::var("VIRTUES_ATLAS_URL").unwrap_or_else(|_| "http://localhost:9100".to_string())
}

fn api_url() -> String {
    std::env::var("VIRTUES_API_URL").unwrap_or_else(|_| "http://localhost:9002".to_string())
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
/// flight. On `ready` the billing token is stored and the first bearer is
/// minted (inside `link::poll`); the page sees `ready` and advances.
pub async fn link_poll_handler(
    _user: AuthUser,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let http = crate::http_client::virtues_api_client();
    match link::poll(state.db.pool(), &http, &atlas_url(), &api_url()).await {
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

// ─── Box rename ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct NameRequest {
    pub name: String,
}

/// Validate a user-chosen box name into a safe mDNS hostname. Pure, so the
/// rules are unit-tested: 2–32 chars, lowercase ASCII letters / digits /
/// hyphens, no leading/trailing hyphen. (Uppercase input is folded rather
/// than rejected — phones love to capitalize.)
pub fn validate_hostname(name: &str) -> Result<String, &'static str> {
    let n = name.trim().to_ascii_lowercase();
    if n.len() < 2 || n.len() > 32 {
        return Err("name must be 2–32 characters");
    }
    if !n
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("only letters, digits, and hyphens");
    }
    if n.starts_with('-') || n.ends_with('-') {
        return Err("can't start or end with a hyphen");
    }
    Ok(n)
}

/// `POST /api/setup/name` — the wizard's "name your box" step. Sets the
/// system hostname (which IS the mDNS name — avahi publishes `<hostname>
/// .local`) and reloads avahi.
///
/// The server runs as the unprivileged `virtues` user; `hostnamectl` needs
/// root. The installer writes a sudoers rule scoped to exactly these two
/// commands (`/etc/sudoers.d/virtues-setup`), and we invoke with `sudo -n`
/// so a missing rule fails immediately with a clear 501 instead of hanging
/// on a password prompt. The state machine derives `named` from the live
/// hostname, so no DB write happens here.
pub async fn name_handler(
    _user: AuthUser,
    State(_state): State<AppState>,
    Json(body): Json<NameRequest>,
) -> impl IntoResponse {
    let name = match validate_hostname(&body.name) {
        Ok(n) => n,
        Err(why) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "invalid_name", "detail": why})),
            )
                .into_response()
        }
    };

    if !cfg!(target_os = "linux") {
        return (
            StatusCode::NOT_IMPLEMENTED,
            Json(json!({"error": "unsupported_platform", "detail": "box rename is Linux-only"})),
        )
            .into_response();
    }

    let set = tokio::process::Command::new("sudo")
        .args(["-n", "hostnamectl", "set-hostname", &name])
        .output()
        .await;
    match set {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            tracing::warn!(error = %err, "hostnamectl set-hostname failed");
            // `sudo -n` failing means we couldn't rename non-interactively.
            // The common cause on older boxes is a missing sudoers rule (the
            // installer writes /etc/sudoers.d/virtues-setup), but it can also
            // be a container/read-only host. Surface the real stderr so the
            // cause is visible instead of always blaming a stale install, and
            // always offer the manual escape hatch.
            let hint = if err.contains("a password is required")
                || err.contains("not allowed")
                || err.contains("no tty")
            {
                "the box is missing its rename permission (older install) — "
            } else {
                ""
            };
            return (
                StatusCode::NOT_IMPLEMENTED,
                Json(json!({
                    "error": "rename_unavailable",
                    "detail": format!(
                        "couldn't rename the box: {hint}run \
                         `sudo hostnamectl set-hostname <name>` on the box directly. \
                         (details: {err})"
                    ),
                })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "rename_failed", "detail": e.to_string()})),
            )
                .into_response()
        }
    }

    // Re-announce on mDNS under the new name. Best-effort: the hostname is
    // already set (the step's source of truth) so we still return success, but
    // log a refused reload instead of swallowing it — if the sudoers rule only
    // whitelists hostnamectl (not systemctl), the `.local` name lags until
    // avahi notices on its own, and the operator should be able to see why.
    match tokio::process::Command::new("sudo")
        .args(["-n", "systemctl", "reload-or-restart", "avahi-daemon"])
        .output()
        .await
    {
        Ok(out) if !out.status.success() => {
            tracing::warn!(
                stderr = %String::from_utf8_lossy(&out.stderr).trim(),
                "avahi reload refused after rename; mDNS name will catch up on its own"
            );
        }
        Err(e) => tracing::warn!(error = %e, "could not invoke avahi reload after rename"),
        Ok(_) => {}
    }

    (
        StatusCode::OK,
        Json(json!({"ok": true, "hostname": name, "mdns": format!("{name}.local")})),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::validate_hostname;

    #[test]
    fn accepts_normal_names() {
        assert_eq!(validate_hostname("adam-jace"), Ok("adam-jace".to_string()));
        assert_eq!(validate_hostname("box2"), Ok("box2".to_string()));
        // Folded, not rejected — phones capitalize.
        assert_eq!(validate_hostname(" Adam-Jace "), Ok("adam-jace".to_string()));
    }

    #[test]
    fn rejects_bad_names() {
        assert!(validate_hostname("a").is_err()); // too short
        assert!(validate_hostname(&"x".repeat(33)).is_err()); // too long
        assert!(validate_hostname("adam jace").is_err()); // space
        assert!(validate_hostname("adam.jace").is_err()); // dot
        assert!(validate_hostname("-adam").is_err()); // leading hyphen
        assert!(validate_hostname("adam-").is_err()); // trailing hyphen
        assert!(validate_hostname("ädam").is_err()); // non-ascii
    }
}

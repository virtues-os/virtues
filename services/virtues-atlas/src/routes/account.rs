//! Account sessions for the app — the minimum that lets the browser leave the
//! onboarding flow.
//!
//! ```text
//!   app  POST /account/login        {email}         -> code emailed
//!   app  POST /account/login/verify {email, code}   -> { token, entitled }
//! ```
//!
//! **Why this is small on purpose.** The box is already an authenticated client
//! of atlas, so anything the app wants *after* a box exists — usage, wallet,
//! billing portal — it asks the box, and the box asks atlas with its own api
//! key. A user session is not a second door to the same data. It exists for one
//! window: before a box is linked, to pay in-app and to vouch for a link.
//!
//! **Codes, not magic links.** A link opens a browser, which is the exact hop
//! this deletes. A six-digit code is typed into the app and nobody leaves it.
//!
//! **Opaque tokens, not JWT.** Instant revocation (lost device → kill the
//! session) matters here; statelessness does not.

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use chrono::{Duration, Utc};
use rand::RngCore;
use serde::Deserialize;
use serde_json::json;

use super::claim::sha256;
use crate::routes::AppState;

/// How long an emailed code is good for. Short: it is typed immediately, from
/// the same room, into an app that is already open.
const CODE_TTL_MINUTES: i64 = 10;
/// Guesses before a code is burned. Six digits is a million possibilities; five
/// tries makes online guessing pointless without punishing a typo.
const MAX_CODE_ATTEMPTS: i32 = 5;
/// Sends per email per hour.
const MAX_SENDS_PER_HOUR: i64 = 5;
/// Session lifetime. Long, because re-authenticating a phone monthly is a tax
/// with no security benefit — revocation is what actually protects a lost
/// device, and that is a row update away.
const SESSION_TTL_DAYS: i64 = 180;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/account/login", post(login))
        .route("/account/login/verify", post(verify))
        .route("/account/session", post(session_info))
        // The airlock's inline sign-in calls these from the app's webview
        // origin — see `app_cors` for why the policy is a wildcard.
        .layer(super::app_cors())
}

// ─── POST /account/login ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct LoginBody {
    email: String,
}

/// Email a six-digit code.
///
/// **Answers identically whether or not the address has an account.** Anything
/// else turns this into an oracle for who our customers are, and the app does
/// not need to know: a person without an account is signing *up*, and the code
/// works the same either way.
async fn login(State(state): State<AppState>, Json(body): Json<LoginBody>) -> impl IntoResponse {
    let email = body.email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return err(StatusCode::BAD_REQUEST, "bad_email", "invalid email");
    }

    // Do NOT swallow this with `.unwrap_or(0)`: a read error would make the
    // count zero and lift the send cap entirely — a broken query becoming an
    // open relay for OTP email (CLAUDE.md, "Do not swallow a query error").
    // Fail CLOSED: a rate-limit read we can't trust refuses the send.
    let recent: i64 = match sqlx::query_scalar(
        "SELECT count(*) FROM login_code WHERE email = $1 AND created_at > now() - interval '1 hour'",
    )
    .bind(&email)
    .fetch_one(&state.pool)
    .await
    {
        Ok(n) => n,
        Err(e) => {
            tracing::warn!("login rate-limit read failed: {e:#}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "could not send a code");
        }
    };
    if recent >= MAX_SENDS_PER_HOUR {
        return err(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "too many codes for this email — try again in an hour",
        );
    }

    let code = gen_code();
    // Salt the hash with the email so a stolen code row cannot be replayed
    // against a different address, and so two identical codes in flight for
    // different people do not collide on the primary key.
    let code_hash = sha256(format!("{email}:{code}").as_bytes());
    let expires_at = Utc::now() + Duration::minutes(CODE_TTL_MINUTES);

    if let Err(e) = sqlx::query(
        "INSERT INTO login_code (code_hash, email, expires_at) VALUES ($1, $2, $3)",
    )
    .bind(&code_hash[..])
    .bind(&email)
    .bind(expires_at)
    .execute(&state.pool)
    .await
    {
        tracing::warn!("login_code insert failed: {e:#}");
        return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "could not send a code");
    }

    let from = std::env::var("VIRTUES_LOGIN_FROM")
        .unwrap_or_else(|_| "login@virtues.com".to_string());
    if let Err(e) =
        crate::email::send_login_code(&state.resend_api_key, &from, &email, &code, CODE_TTL_MINUTES)
            .await
    {
        // The row stays: a send failure is usually Resend being unhappy, and the
        // rate limit should still count the attempt.
        tracing::warn!(error = %format!("{e:#}"), "login code email failed");
        return err(StatusCode::BAD_GATEWAY, "email_failed", "could not send the email");
    }

    (StatusCode::OK, Json(json!({ "sent": true, "expires_in": CODE_TTL_MINUTES * 60 })))
        .into_response()
}

// ─── POST /account/login/verify ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct VerifyBody {
    email: String,
    code: String,
}

/// Exchange a code for a session token.
async fn verify(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<VerifyBody>,
) -> impl IntoResponse {
    let email = body.email.trim().to_lowercase();
    // Accept the code however a human types it: spaces, dashes, whatever. The
    // digits are the secret; the punctuation is not.
    let code: String = body.code.chars().filter(|c| c.is_ascii_digit()).collect();
    if email.is_empty() || code.len() != 6 {
        return err(StatusCode::BAD_REQUEST, "bad_code", "that code doesn't look right");
    }

    let code_hash = sha256(format!("{email}:{code}").as_bytes());
    let row: Option<(i32,)> = sqlx::query_as(
        "SELECT attempts FROM login_code
         WHERE code_hash = $1 AND consumed_at IS NULL AND expires_at > now()",
    )
    .bind(&code_hash[..])
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some((attempts,)) = row else {
        // Count the miss against every live code for this email, so guessing
        // burns the real one rather than being free.
        let _ = sqlx::query(
            "UPDATE login_code SET attempts = attempts + 1
             WHERE email = $1 AND consumed_at IS NULL AND expires_at > now()",
        )
        .bind(&email)
        .execute(&state.pool)
        .await;
        let _ = sqlx::query(
            "UPDATE login_code SET consumed_at = now()
             WHERE email = $1 AND attempts >= $2 AND consumed_at IS NULL",
        )
        .bind(&email)
        .bind(MAX_CODE_ATTEMPTS)
        .execute(&state.pool)
        .await;
        return err(StatusCode::UNAUTHORIZED, "bad_code", "that code didn't match");
    };
    if attempts >= MAX_CODE_ATTEMPTS {
        return err(StatusCode::UNAUTHORIZED, "bad_code", "that code is no longer valid");
    }

    // Single use.
    let _ = sqlx::query("UPDATE login_code SET consumed_at = now() WHERE code_hash = $1")
        .bind(&code_hash[..])
        .execute(&state.pool)
        .await;

    // A session may exist before any payment does — sign in, then buy, then
    // link. So a missing customer is not an error here.
    let customer: Option<(String,)> =
        sqlx::query_as("SELECT stripe_customer_id FROM customers WHERE email = $1 LIMIT 1")
            .bind(&email)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);
    let customer_id = customer.map(|c| c.0);

    let token = random_hex(32);
    let token_hash = sha256(token.as_bytes());
    let ua = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(200).collect::<String>());

    if let Err(e) = sqlx::query(
        "INSERT INTO account_session (token_hash, email, stripe_customer_id, user_agent, expires_at)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&token_hash[..])
    .bind(&email)
    .bind(&customer_id)
    .bind(&ua)
    .bind(Utc::now() + Duration::days(SESSION_TTL_DAYS))
    .execute(&state.pool)
    .await
    {
        tracing::warn!("account_session insert failed: {e:#}");
        return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "could not sign in");
    }

    // Advisory here — the app re-checks entitlement server-side at /init/approve
    // before anything is granted, so a read error at sign-in defaulting to
    // `false` costs at most a needless "you have no subscription" glance, never
    // a wrong grant. Named explicitly rather than swallowed silently.
    let entitled = is_entitled(&state, customer_id.as_deref()).await.unwrap_or_else(|e| {
        tracing::warn!("entitlement check at sign-in failed (defaulting to not-entitled): {e:#}");
        false
    });
    (
        StatusCode::OK,
        Json(json!({ "token": token, "email": email, "entitled": entitled })),
    )
        .into_response()
}

// ─── POST /account/session ──────────────────────────────────────────────────

/// Who am I, and do I owe money? The app calls this at launch to decide whether
/// the link step needs a payment sheet at all.
async fn session_info(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let Some(sess) = authed(&state, &headers).await else {
        return err(StatusCode::UNAUTHORIZED, "unauthorized", "sign in again");
    };
    // Advisory (same as at sign-in): the grant path re-checks. A read error
    // defaults to not-entitled with a log, never a wrong grant.
    let entitled = is_entitled(&state, sess.customer_id.as_deref()).await.unwrap_or_else(|e| {
        tracing::warn!("entitlement check at session_info failed (defaulting to not-entitled): {e:#}");
        false
    });
    (StatusCode::OK, Json(json!({ "email": sess.email, "entitled": entitled }))).into_response()
}

// ─── POST /init/grant — NOT YET, and the reason matters ────────────────────
//
// The endpoint that would remove the browser from onboarding: a signed-in app
// asks for a pre-approved `device_code` and carries it to the box over
// Bluetooth. It is deliberately absent, because minting a device credential
// today does:
//
//     INSERT INTO customers … ON CONFLICT DO UPDATE SET api_key_hash = $3
//
// — one api key per CUSTOMER (claim.rs). So linking a second box rotates the
// first box's credential and silently kills it. Building the grant on that
// would bake the single-box limit into the very feature meant to remove it,
// and would do it invisibly, at the moment a household adds their second box.
//
// The fix is per-box keys — a `device_keys`-shaped table keyed by the box's
// EndpointId, with `customers.api_key_hash` retired — and it wants its own
// change with its own migration. See docs/onboarding-plan.md, "Billing
// correctness".
//
// `POST /init/approve` (link.rs) is NOT this endpoint, though it looks
// adjacent: it approves the box's own in-flight link — the same attach
// `login_verify` already performs on a magic-link click, with a session
// bearer as the proof instead of a clicked email. It inherits the
// single-key rotation exactly as the magic link does; it does not extend it.
//
// ─── shared ─────────────────────────────────────────────────────────────────

pub(super) struct Session {
    pub(super) email: String,
    pub(super) customer_id: Option<String>,
}

/// Resolve a `Authorization: Bearer <token>` into a live session, refreshing
/// `last_seen_at` so the account page can show honest device activity.
pub(super) async fn authed(state: &AppState, headers: &HeaderMap) -> Option<Session> {
    let raw = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())?
        .strip_prefix("Bearer ")?
        .trim();
    if raw.is_empty() {
        return None;
    }
    let token_hash = sha256(raw.as_bytes());
    let row: Option<(String, Option<String>)> = sqlx::query_as(
        "UPDATE account_session SET last_seen_at = now()
         WHERE token_hash = $1 AND revoked_at IS NULL AND expires_at > now()
         RETURNING email, stripe_customer_id",
    )
    .bind(&token_hash[..])
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();
    row.map(|(email, customer_id)| Session { email, customer_id })
}

/// Does this customer have an active subscription right now?
///
/// Returns `Result` rather than a bare bool BECAUSE the answer gates money:
/// `/init/approve` routes a `false` to browser checkout, so a swallowed query
/// error (`.unwrap_or(false)`) would send a paying customer to pay again — the
/// exact "turn a broken query into a plausible value" failure CLAUDE.md bans.
/// A `None` customer_id is a real, non-error `false`: no Stripe customer means
/// nothing is subscribed.
pub(super) async fn is_entitled(
    state: &AppState,
    customer_id: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let Some(cid) = customer_id else { return Ok(false) };
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM subscriptions
         WHERE stripe_customer_id = $1 AND status = 'active')",
    )
    .bind(cid)
    .fetch_one(&state.pool)
    .await
}

/// Six digits. Uniform over 000000..=999999 — no modulo bias, and leading zeros
/// preserved, because a code that renders as five characters confuses people.
fn gen_code() -> String {
    let mut rng = rand::rng();
    let mut n;
    loop {
        n = rng.next_u32();
        // Reject the short tail so every value is equally likely.
        if n < u32::MAX - (u32::MAX % 1_000_000) {
            break;
        }
    }
    format!("{:06}", n % 1_000_000)
}

fn random_hex(n: usize) -> String {
    let mut bytes = vec![0u8; n];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn err(status: StatusCode, code: &str, message: &str) -> axum::response::Response {
    (status, Json(json!({ "error": { "code": code, "message": message } }))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_six_digits_including_leading_zeros() {
        for _ in 0..500 {
            let c = gen_code();
            assert_eq!(c.len(), 6, "not six characters: {c}");
            assert!(c.chars().all(|ch| ch.is_ascii_digit()), "not digits: {c}");
        }
    }

    #[test]
    fn code_hash_is_bound_to_the_email() {
        // A code row must not be replayable against a different address.
        let a = sha256(b"adam@example.com:123456");
        let b = sha256(b"eve@example.com:123456");
        assert_ne!(a, b);
    }
}

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
//! window: before a box is linked, to vouch for a link.
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
use super::link::page;
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
        // Where Stripe sends the browser after a checkout opened by
        // `POST /billing/checkout/sessions` (the box-key door). It attaches
        // the subscription to the account stamped into the session.
        .route("/account/checkout/done", axum::routing::get(account_checkout_done))
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
    // link. So a missing customer is not an error here. The customer comes
    // from the account this code just proved, never from `customers.email`:
    // that is the address typed at Stripe, which nobody verified.
    let customer: Option<(String,)> = match sqlx::query_as(
        "SELECT stripe_customer_id FROM accounts \
         WHERE email = lower($1) AND stripe_customer_id IS NOT NULL",
    )
    .bind(&email)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(c) => c,
        // Not "no customer": that would sign a paying owner in as unpaid.
        Err(e) => {
            tracing::warn!("sign-in customer lookup failed: {e:#}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "could not sign in");
        }
    };
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

    // The account exists from the first sign-in (0017) — identity needs no
    // payment. Best-effort here: the grant/approve paths ensure it again, so
    // a transient failure costs nothing but this log line.
    if let Err(e) = ensure_account(&state.pool, &email).await {
        tracing::warn!("ensure_account at sign-in failed (grant path will retry): {e:#}");
    }

    // Advisory: `entitled` still reports subscription state (Settings and
    // billing surfaces read it) but no linking door is gated on it anymore.
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

// ─── GET /account/checkout/done ─────────────────────────────────────────────

/// The browser lands here after paying. Attach the subscription to the
/// account stamped into the session at creation: upsert the customer (no key
/// minted here), upsert the subscription, label the account's keys and
/// sessions with the customer (that is what flips `entitled` for the app's
/// poll), and fund the first month's wallet.
async fn account_checkout_done(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> axum::response::Response {
    let session_id = q.get("session_id").cloned().unwrap_or_default();
    if session_id.is_empty() || session_id == "cancelled" {
        return page(
            "Checkout cancelled",
            "Nothing was charged. Go back to the Virtues app — you can try again there, \
             or skip and run your server LAN-only.",
        );
    }
    let paid = match super::claim::verify_and_claim_session(&state, &session_id).await {
        Ok(p) => p,
        Err(e) if e.code == "session_already_claimed" => {
            // A refresh of the success page. The work is done; say so.
            return page(
                "Subscription active",
                "This payment was already processed. Go back to the Virtues app — it \
                 continues on its own.",
            );
        }
        Err(e) => {
            tracing::warn!("account checkout finalize failed: {} {}", e.code, e.message);
            super::claim::release_session_claim(&state, &session_id).await;
            return page(
                "Payment not confirmed",
                "We couldn't confirm the payment yet. If you completed checkout, wait a \
                 moment and refresh this page; nothing is lost.",
            );
        }
    };

    // The account comes from the session's stamp, written when an
    // authenticated caller opened this checkout. It must still exist: an
    // account deleted mid-checkout has nowhere to attach a subscription.
    let ensured_account_id = match sqlx::query_scalar::<_, String>(
        "SELECT account_id FROM accounts WHERE account_id = $1",
    )
    .bind(&paid.account_id)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(Some(id)) => id,
        Ok(None) => {
            tracing::warn!(account_id = %paid.account_id, "account checkout: stamped account does not exist");
            return page(
                "Something went wrong",
                "Payment went through but we couldn't find the account it was for. \
                 Email support@virtues.com and we'll sort it out.",
            );
        }
        Err(e) => {
            tracing::warn!("account checkout account lookup failed: {e:#}");
            super::claim::release_session_claim(&state, &session_id).await;
            return page("Something went wrong", "Payment went through but we couldn't finish. Refresh this page to retry.");
        }
    };
    let account_id: Result<(String,), _> = sqlx::query_as(
        r#"
        INSERT INTO customers (stripe_customer_id, email, account_id)
        VALUES ($1, $2, $3)
        ON CONFLICT (stripe_customer_id) DO UPDATE SET email = $2, account_id = $3
        RETURNING account_id
        "#,
    )
    .bind(&paid.stripe_customer_id)
    .bind(&paid.email)
    .bind(&ensured_account_id)
    .fetch_one(&state.pool)
    .await;
    let (account_id,) = match account_id {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("account checkout customer upsert failed: {e:#}");
            super::claim::release_session_claim(&state, &session_id).await;
            return page("Something went wrong", "Payment went through but we couldn't finish. Refresh this page to retry.");
        }
    };
    // Best-effort by design: the payment is captured and the subscription row
    // lands below regardless; a transient failure here heals on the next
    // attach-touching call, and the warn is the operator's signal.
    if let Err(e) =
        attach_stripe_to_account(&state.pool, &ensured_account_id, &paid.stripe_customer_id).await
    {
        tracing::warn!("account checkout stripe attach failed: {e:#}");
    }
    if let Err(e) = sqlx::query(
        r#"
        INSERT INTO subscriptions (stripe_subscription_id, stripe_customer_id, status, current_period_end)
        VALUES ($1, $2, 'active', $3)
        ON CONFLICT (stripe_subscription_id)
        DO UPDATE SET status = 'active', current_period_end = $3
        "#,
    )
    .bind(&paid.stripe_subscription_id)
    .bind(&paid.stripe_customer_id)
    .bind(paid.period_end)
    .execute(&state.pool)
    .await
    {
        tracing::warn!("account checkout subscription upsert failed: {e:#}");
        super::claim::release_session_claim(&state, &session_id).await;
        return page("Something went wrong", "Payment went through but we couldn't finish. Refresh this page to retry.");
    }
    // (Session repoint rides in attach_stripe_to_account above.)
    // First month's wallet. Best-effort: `invoice.paid` re-sets it monthly,
    // and virtues-api's credit upserts the account row itself.
    if let Err(e) = state
        .virtues_api
        .credit(&crate::virtues_api_client::Credit {
            account_id,
            amount_micros: state.credit.renewal_micros,
            mode: "set",
            reference: Some(format!("account-checkout:{session_id}")),
        })
        .await
    {
        tracing::warn!("account checkout initial credit failed (invoice.paid will cover it): {e:#}");
    }
    page(
        "Subscription active ✓",
        "Go back to the Virtues app — it notices on its own and carries on with your \
         server's setup.",
    )
}

// ─── POST /account/session ──────────────────────────────────────────────────

/// Who am I, and is a subscription active? `entitled` is advisory billing
/// state for Settings/billing surfaces — since 0017 no linking or setup step
/// is gated on it, and the airlock ignores it.
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

// ─── POST /init/grant — NOT YET, but no longer blocked ─────────────────────
//
// The endpoint that would remove the browser from onboarding: a signed-in app
// asks for a pre-approved `device_code` and carries it to the box over its
// paired connection. It was deliberately absent
// while atlas held ONE api key per customer — a second box linking rotated
// the first box's credential and silently killed it, and building the grant
// on that would have baked the single-box limit into the very feature meant
// to remove it.
//
// That blocker fell 2026-08-24: migration 0015 introduced per-box keys
// (`box_key`, scoped by the box's self-reported endpoint_id;
// claim.rs::mint_box_key / customer_id_by_key_hash). What remains is the
// grant itself — see agents/record/one-wire-plan.md, Phase 2.
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

/// Mint-or-fetch the account for an email — the identity root since 0017.
///
/// Read-first: this runs on every authed linking call, and the upsert's
/// conflict arm is a WRITE (row version + row lock) even when nothing
/// changes — a per-request churn the plain SELECT avoids. The upsert only
/// runs on a genuine first sign-in (or the read/insert race, which the
/// conflict arm absorbs). No Stripe anywhere in it — that is the point.
pub(crate) async fn ensure_account(
    pool: &sqlx::PgPool,
    email: &str,
) -> Result<String, sqlx::Error> {
    if let Some(id) = sqlx::query_scalar::<_, String>(
        "SELECT account_id FROM accounts WHERE email = lower($1)",
    )
    .bind(email)
    .fetch_optional(pool)
    .await?
    {
        return Ok(id);
    }
    let candidate = super::claim::new_account_id();
    sqlx::query_scalar(
        r#"
        INSERT INTO accounts (account_id, email)
        VALUES ($1, lower($2))
        ON CONFLICT (email) DO UPDATE SET email = excluded.email
        RETURNING account_id
        "#,
    )
    .bind(&candidate)
    .bind(email)
    .fetch_one(pool)
    .await
}

/// First payment: attach the Stripe customer to the account, and heal
/// everything that was minted while the account was free. The `IS NULL`
/// guard keeps the first attachment: a re-subscribe under a new Stripe
/// customer leaves it alone, and subscriptions still resolve through
/// `customers`.
///
/// The healing matters as much as the attach: a box linked before payment
/// carries `box_key.stripe_customer_id = NULL` and no `customers.api_key_hash`
/// mirror, so without this the now-paying customer's box answers
/// `invalid_api_key` on every billing door until a manual re-link.
pub(crate) async fn attach_stripe_to_account(
    pool: &sqlx::PgPool,
    account_id: &str,
    stripe_customer_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE accounts SET stripe_customer_id = $1 \
         WHERE account_id = $2 AND stripe_customer_id IS NULL",
    )
    .bind(stripe_customer_id)
    .bind(account_id)
    .execute(pool)
    .await?;
    // Label the free-era keys with their customer so customer-keyed doors
    // (credits, settings, billing portal) resolve them.
    sqlx::query(
        "UPDATE box_key SET stripe_customer_id = $1 \
         WHERE account_id = $2 AND stripe_customer_id IS NULL",
    )
    .bind(stripe_customer_id)
    .bind(account_id)
    .execute(pool)
    .await?;
    // Mirror the newest key into customers.api_key_hash (most-recent-key
    // semantics, matching mint_box_key) so a rolled-back binary authenticates
    // boxes that linked while the account was free.
    let newest: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT api_key_hash FROM box_key WHERE account_id = $1 \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await?;
    if let Some(hash) = newest {
        sqlx::query("UPDATE customers SET api_key_hash = $2 WHERE stripe_customer_id = $1")
            .bind(stripe_customer_id)
            .bind(&hash)
            .execute(pool)
            .await?;
    }
    // Re-point keyless sessions signed in as this account's email (sessions
    // store lowercased emails) — this is what flips `entitled` for billing
    // surfaces.
    sqlx::query(
        "UPDATE account_session s SET stripe_customer_id = $1 \
         FROM accounts a \
         WHERE a.account_id = $2 AND s.email = a.email AND s.stripe_customer_id IS NULL",
    )
    .bind(stripe_customer_id)
    .bind(account_id)
    .execute(pool)
    .await?;
    Ok(())
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

#[cfg(test)]
mod attach_tests {
    use super::attach_stripe_to_account;

    async fn account(pool: &sqlx::PgPool, id: &str, email: &str, customer: Option<&str>) {
        sqlx::query("INSERT INTO accounts (account_id, email, stripe_customer_id) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(email)
            .bind(customer)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO box_key (api_key_hash, account_id, stripe_customer_id) VALUES ($1, $2, $3)",
        )
        .bind(id.as_bytes())
        .bind(id)
        .bind(customer)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO account_session (token_hash, email, stripe_customer_id, expires_at) \
             VALUES ($1, $2, $3, now() + interval '1 day')",
        )
        .bind(id.as_bytes())
        .bind(email)
        .bind(customer)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn customer_of(pool: &sqlx::PgPool, table: &str, id: &str) -> Option<String> {
        let sql = match table {
            "accounts" => "SELECT stripe_customer_id FROM accounts WHERE account_id = $1",
            "box_key" => "SELECT stripe_customer_id FROM box_key WHERE account_id = $1",
            _ => "SELECT s.stripe_customer_id FROM account_session s \
                  JOIN accounts a ON a.email = s.email WHERE a.account_id = $1",
        };
        sqlx::query_scalar(sql).bind(id).fetch_one(pool).await.unwrap()
    }

    /// The payment attaches to the account it was opened for and nowhere
    /// else: another account, paying or free, keeps its customer, its keys
    /// and its sessions exactly as they were. Before the session stamp, the
    /// account came from the email typed at Stripe, so a payer could type
    /// another owner's address and land on their account.
    #[sqlx::test(migrations = "./migrations")]
    async fn first_payment_touches_only_its_own_account(pool: sqlx::PgPool) {
        account(&pool, "acct_payer", "payer@example.com", None).await;
        account(&pool, "acct_free", "free@example.com", None).await;
        account(&pool, "acct_paying", "paying@example.com", Some("cus_paying")).await;

        attach_stripe_to_account(&pool, "acct_payer", "cus_new").await.unwrap();

        for t in ["accounts", "box_key", "session"] {
            assert_eq!(customer_of(&pool, t, "acct_payer").await.as_deref(), Some("cus_new"), "{t}");
            assert_eq!(customer_of(&pool, t, "acct_free").await, None, "{t}");
            assert_eq!(customer_of(&pool, t, "acct_paying").await.as_deref(), Some("cus_paying"), "{t}");
        }
    }

    /// A re-subscribe under a new Stripe customer keeps the first attachment.
    #[sqlx::test(migrations = "./migrations")]
    async fn keeps_an_existing_attachment(pool: sqlx::PgPool) {
        account(&pool, "acct_paying", "paying@example.com", Some("cus_first")).await;
        attach_stripe_to_account(&pool, "acct_paying", "cus_second").await.unwrap();
        assert_eq!(customer_of(&pool, "accounts", "acct_paying").await.as_deref(), Some("cus_first"));
    }
}

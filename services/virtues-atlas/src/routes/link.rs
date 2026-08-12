//! Device-authorization link flow (RFC 8628 shape).
//!
//! Connects a box to a paid subscription without the box ever holding a Stripe
//! key. The box never sees a checkout page or a `customer_id` — it only starts
//! a link and polls for the resulting api_key.
//!
//! ```text
//!   box  POST /init/start            -> { device_code, user_code, verification_uri… }
//!   user opens  GET /link?code=…     -> 302 to Stripe Checkout
//!   Stripe success -> GET /init/done -> finalize: mint api_key, mark ready
//!   box  POST /init/poll {device_code} (loop) -> { status:"ready", api_key }
//! ```
//!
//! The `device_code` (secret) is the poll capability; the `user_code` (short,
//! human) is what the customer types. They map to the same `device_link` row.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json, Redirect},
    routing::{get, post},
    Router,
};
use chrono::{Duration, Utc};
use rand::RngCore;
use serde::Deserialize;
use serde_json::json;

use super::claim::{finalize_paid_session, sha256};
use crate::routes::AppState;

/// How long a started link stays claimable before the box must restart it.
const LINK_TTL_MINUTES: i64 = 15;
/// Suggested poll interval (seconds) the box should honor.
const POLL_INTERVAL_SECS: u64 = 5;
/// Unambiguous alphabet for the human code (no I/O/0/1).
const USER_CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/init/start", post(start))
        .route("/init/poll", post(poll))
        .route("/init", get(verify))
        .route("/init/done", get(done))
        // ─── [1] Log in flow ──────────────────────────────────────────
        // Box-callable login start: takes a device_code + email, looks
        // up the matching customer, sends a magic link via Resend. The
        // verify_login GET endpoint clicks → marks the device_link ready.
        .route("/init/login", post(login_start))
        .route("/init/login/verify", get(login_verify))
}

// ─── POST /init/start ───────────────────────────────────────────────────────

async fn start(State(state): State<AppState>) -> axum::response::Response {
    let device_code = random_hex(32);
    let user_code = gen_user_code();
    let device_code_hash = sha256(device_code.as_bytes());
    let expires_at = Utc::now() + Duration::minutes(LINK_TTL_MINUTES);

    let res = sqlx::query(
        r#"
        INSERT INTO device_link (device_code_hash, user_code, status, expires_at)
        VALUES ($1, $2, 'pending', $3)
        "#,
    )
    .bind(&device_code_hash[..])
    .bind(&user_code)
    .bind(expires_at)
    .execute(&state.pool)
    .await;
    if let Err(e) = res {
        tracing::warn!("link start insert failed: {e:#}");
        return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "could not start link");
    }

    let base = state.public_url.trim_end_matches('/');
    (
        StatusCode::OK,
        Json(json!({
            "device_code": device_code,
            "user_code": user_code,
            "verification_uri": format!("{base}/init"),
            "verification_uri_complete": format!("{base}/init?code={user_code}"),
            "interval": POLL_INTERVAL_SECS,
            "expires_in": LINK_TTL_MINUTES * 60,
        })),
    )
        .into_response()
}

// ─── POST /init/poll ──────────────────────────────────────────────────────--

#[derive(Debug, Deserialize)]
struct PollBody {
    device_code: String,
}

async fn poll(State(state): State<AppState>, Json(body): Json<PollBody>) -> axum::response::Response {
    let hash = sha256(body.device_code.as_bytes());

    let row: Option<(String, Option<String>, chrono::DateTime<Utc>)> = match sqlx::query_as(
        "SELECT status, api_key, expires_at FROM device_link WHERE device_code_hash = $1",
    )
    .bind(&hash[..])
    .fetch_optional(&state.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("link poll query failed: {e:#}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "poll failed");
        }
    };

    let Some((status, api_key, expires_at)) = row else {
        return Json(json!({ "status": "expired" })).into_response();
    };
    if Utc::now() > expires_at && status == "pending" {
        return Json(json!({ "status": "expired" })).into_response();
    }

    match status.as_str() {
        "ready" => {
            // One-time delivery: hand the api_key over, then clear it.
            let _ = sqlx::query(
                "UPDATE device_link SET status = 'claimed', api_key = NULL \
                 WHERE device_code_hash = $1 AND status = 'ready'",
            )
            .bind(&hash[..])
            .execute(&state.pool)
            .await;
            Json(json!({ "status": "ready", "api_key": api_key })).into_response()
        }
        "claimed" => Json(json!({ "status": "claimed" })).into_response(),
        "denied" => Json(json!({ "status": "denied" })).into_response(),
        _ => Json(json!({ "status": "pending" })).into_response(),
    }
}

// ─── GET /link?code=… (customer-facing verification → Stripe Checkout) ───────

#[derive(Debug, Deserialize)]
struct VerifyQuery {
    code: Option<String>,
}

async fn verify(State(state): State<AppState>, Query(q): Query<VerifyQuery>) -> axum::response::Response {
    let Some(code) = q.code.map(|c| c.trim().to_uppercase()).filter(|c| !c.is_empty()) else {
        // No code in the URL — render a form so the user can type the pairing
        // code shown on their box. (The box's copy says "enter the code here";
        // previously this page had no input field, a dead end.) The form GETs
        // back to this same handler with `?code=…`.
        return connect_page();
    };

    // Must be a live pending link.
    let row: Option<(chrono::DateTime<Utc>,)> = sqlx::query_as(
        "SELECT expires_at FROM device_link WHERE user_code = $1 AND status = 'pending'",
    )
    .bind(&code)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);
    let Some((expires_at,)) = row else {
        return page("Link not found", "That code is invalid or already used. Start again from your box.");
    };
    if Utc::now() > expires_at {
        return page("Link expired", "This code expired. Start again from your box.");
    }

    if !state.stripe.is_configured() || state.stripe_price_id.is_empty() {
        return page("Checkout unavailable", "Billing isn't configured on this server yet.");
    }

    let base = state.public_url.trim_end_matches('/');
    // Stripe substitutes {CHECKOUT_SESSION_ID} into the success URL.
    let success_url = format!("{base}/init/done?code={code}&session_id={{CHECKOUT_SESSION_ID}}");
    let cancel_url = format!("{base}/init?code={code}");

    match state
        .stripe
        .create_checkout_session(
            &state.stripe_price_id,
            &success_url,
            &cancel_url,
            &code,
            state.allow_promotion_codes,
        )
        .await
    {
        Ok(session) => {
            let _ = sqlx::query("UPDATE device_link SET stripe_session_id = $2 WHERE user_code = $1")
                .bind(&code)
                .bind(&session.id)
                .execute(&state.pool)
                .await;
            Redirect::to(&session.url).into_response()
        }
        Err(e) => {
            // The link is fine — it's live and pending (we got here past the
            // expiry/not-found checks above). Stripe itself refused to open a
            // checkout session (bad/missing price id, key mismatch, or a Stripe
            // outage). Don't send the customer back to the box: the box can't
            // fix a billing-server problem, and restarting just mints a new link
            // that hits the same wall. Offer an in-place retry instead.
            tracing::warn!("link checkout create failed: {e:#}");
            let retry = format!("{base}/init?code={code}");
            let reason = html_escape(&e.to_string());
            page(
                "Checkout couldn’t start",
                &format!(
                    "Our billing server couldn’t open a Stripe checkout session. This is on \
                     our end — not a problem with your link or your box. \
                     <a href=\"{retry}\">Try again</a> in a moment; if it keeps failing, \
                     email support@virtues.com.<br><br>\
                     <small style=\"color:#888\">Details: {reason}</small>",
                ),
            )
        }
    }
}

// ─── GET /init/done (Stripe success URL → finalize + mark ready) ─────────────

#[derive(Debug, Deserialize)]
struct DoneQuery {
    code: String,
    session_id: String,
}

async fn done(State(state): State<AppState>, Query(q): Query<DoneQuery>) -> axum::response::Response {
    let code = q.code.trim().to_uppercase();
    let finalized = match finalize_paid_session(&state, &q.session_id).await {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!("link finalize failed: {} {}", e.code, e.message);
            return page("Payment not confirmed", "We couldn't confirm the payment yet. If you completed checkout, return to your box — it will keep trying.");
        }
    };

    // C2: bind the paid session to *this* code in two ways.
    //   (a) the user_code we stamped into Stripe metadata at creation must
    //       match the URL `code` — a session created for code A can't be
    //       applied to code B by swapping the URL param;
    //   (b) the device_link row must already carry this session_id (verify()
    //       writes it just before redirecting to Stripe), so even with a
    //       matching code, only the row that started this checkout finalizes.
    // Without either, /init/done was a free api_key write keyed on a
    // 40-bit brute-forceable user_code with no rate limit.
    if finalized.metadata_user_code.as_deref() != Some(code.as_str()) {
        tracing::warn!(code = %code, "link/done: session metadata.user_code mismatch");
        return page("Link mismatch", "That payment is for a different link code. Restart from your box.");
    }

    let res = sqlx::query(
        "UPDATE device_link SET status = 'ready', api_key = $2 \
         WHERE user_code = $1 AND stripe_session_id = $3 AND status = 'pending'",
    )
    .bind(&code)
    .bind(&finalized.api_key)
    .bind(&finalized.session_id)
    .execute(&state.pool)
    .await;
    match res {
        Ok(r) if r.rows_affected() == 1 => {
            page("Box linked ✓", "Your Virtues box is now linked to your subscription. Return to it — it'll pick this up automatically.")
        }
        Ok(_) => page("Already linked", "This link was already completed, or this session doesn't match. Return to your box."),
        Err(e) => {
            tracing::warn!("link done update failed: {e:#}");
            page("Something went wrong", "Payment went through but linking hit an error. Contact support.")
        }
    }
}

// ─── helpers ─────────────────────────────────────────────────────────────--

fn random_hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    rand::rng().fill_bytes(&mut b);
    hex::encode(b)
}

/// 8 chars from an unambiguous alphabet, formatted `XXXX-XXXX`. The negligible
/// modulo bias is irrelevant for a short-lived, single-use human code.
fn gen_user_code() -> String {
    let mut bytes = [0u8; 8];
    rand::rng().fill_bytes(&mut bytes);
    let mut s = String::with_capacity(9);
    for (i, b) in bytes.iter().enumerate() {
        if i == 4 {
            s.push('-');
        }
        s.push(USER_CODE_ALPHABET[(*b as usize) % USER_CODE_ALPHABET.len()] as char);
    }
    s
}

/// Minimal HTML escaping for untrusted text interpolated into a `page()` body
/// (e.g. a Stripe error message). Covers the characters that could break out of
/// text context; the messages we render are server-originated, this is hygiene.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn page(title: &str, body: &str) -> axum::response::Response {
    Html(format!(
        "<!doctype html><html><head><meta charset=utf-8>\
         <meta name=viewport content='width=device-width,initial-scale=1'>\
         <title>{title}</title>\
         <style>body{{font-family:system-ui,sans-serif;max-width:32rem;margin:4rem auto;padding:0 1rem;line-height:1.5}}\
         h1{{font-size:1.4rem}}</style></head>\
         <body><h1>{title}</h1><p>{body}</p></body></html>"
    ))
    .into_response()
}

/// The `/init` landing page when no `?code=` is present: a form that lets the
/// user type the pairing code shown on their box. Submitting GETs `/init?code=…`,
/// which the same `verify` handler then turns into a Stripe checkout. (The static
/// version of this page had no input — the box told users to "enter the code
/// here" with nowhere to enter it.) `method=get` keeps the code in the URL the
/// handler already reads, and uppercases on submit to match the stored code.
fn connect_page() -> axum::response::Response {
    Html(
        "<!doctype html><html><head><meta charset=utf-8>\
         <meta name=viewport content='width=device-width,initial-scale=1'>\
         <title>Connect your Virtues box</title>\
         <style>body{font-family:system-ui,sans-serif;max-width:32rem;margin:4rem auto;padding:0 1rem;line-height:1.5}\
         h1{font-size:1.4rem}\
         form{display:flex;gap:.5rem;margin-top:1.5rem}\
         input{flex:1;font:inherit;font-size:1.1rem;letter-spacing:.08em;text-transform:uppercase;\
         padding:.6rem .75rem;border:1px solid #ccc;border-radius:8px}\
         button{font:inherit;padding:.6rem 1.1rem;border:0;border-radius:8px;background:#111;color:#fff;cursor:pointer}\
         </style></head>\
         <body>\
         <h1>Connect your Virtues box</h1>\
         <p>Enter the pairing code shown on your box to continue to checkout.</p>\
         <form method=get action=/init>\
         <input name=code placeholder='XXXX-XXXX' autocomplete=off autocapitalize=characters \
         spellcheck=false autofocus required aria-label='Pairing code'>\
         <button type=submit>Continue &rarr;</button>\
         </form>\
         <p style='margin-top:1rem;color:#666;font-size:.9rem'>Or just open the full link shown on your box.</p>\
         </body></html>"
            .to_string(),
    )
    .into_response()
}

fn err(status: StatusCode, code: &str, message: &str) -> axum::response::Response {
    (status, Json(json!({ "error": { "code": code, "message": message } }))).into_response()
}

// ─────────────────────────────────────────────────────────────────────────
// [1] Log in flow — Resend magic-link verification of an existing customer
// ─────────────────────────────────────────────────────────────────────────
//
// Pattern:
//   box  POST /init/login {device_code, email}
//        └→ atlas looks up customers.email → stripe_customer_id
//           - found:   insert login_attempt(token_hash, …), send magic link via Resend
//           - missing: return {status:"no_account"} (box surfaces "subscribe?" prompt)
//   user opens magic link
//        └→ GET /init/login/verify?token=…
//           - hash + lookup login_attempt
//           - mark used; flip the bound device_link → status='ready' with the api_key
//           - render success HTML ("return to your terminal")
//   box  POST /init/poll {device_code}
//        └→ existing handler picks up status='ready' + api_key
//
// Tokens are 32 random bytes; stored only as sha256. Sender rate-limit:
// no more than 3 active attempts per email per hour (anti-spam).

const LOGIN_TTL_MINUTES: i64 = 15;
const LOGIN_FROM_DEFAULT: &str = "login@virtues.com";

#[derive(Debug, Deserialize)]
struct LoginStartBody {
    device_code: String,
    email: String,
}

async fn login_start(
    State(state): State<AppState>,
    Json(body): Json<LoginStartBody>,
) -> axum::response::Response {
    // Normalize the email lightly. Stripe stores them lowercased; we match.
    let email = body.email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return err(StatusCode::BAD_REQUEST, "bad_email", "invalid email");
    }

    // The box must already have an active /init/start in flight.
    let device_code_hash = sha256(body.device_code.as_bytes());
    let device_link_exists: Option<(String,)> = sqlx::query_as(
        "SELECT status FROM device_link WHERE device_code_hash = $1 AND expires_at > now()",
    )
    .bind(&device_code_hash[..])
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);
    if device_link_exists.is_none() {
        return err(
            StatusCode::BAD_REQUEST,
            "no_device_link",
            "device_code not found or expired; restart /init/start first",
        );
    }

    // Rate limit: max 3 send attempts per email per hour.
    let recent: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM login_attempt \
         WHERE email = $1 AND created_at > now() - interval '1 hour'",
    )
    .bind(&email)
    .fetch_one(&state.pool)
    .await
    .unwrap_or((0,));
    if recent.0 >= 3 {
        return err(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "too many login attempts for this email — try again in an hour",
        );
    }

    // Look up the customer in atlas's local table (mirrors Stripe customers).
    let customer: Option<(String,)> = sqlx::query_as(
        "SELECT stripe_customer_id FROM customers WHERE email = $1 LIMIT 1",
    )
    .bind(&email)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some((customer_id,)) = customer else {
        // Don't send an email when there's no account — would be a spam
        // vector AND wouldn't help anyway. Box surfaces "subscribe?" CTA.
        return (
            StatusCode::OK,
            Json(json!({ "status": "no_account" })),
        )
            .into_response();
    };

    // Mint the magic-link token (32 random bytes, hex-encoded).
    let token = random_hex(32);
    let token_hash = sha256(token.as_bytes());
    let expires_at = Utc::now() + Duration::minutes(LOGIN_TTL_MINUTES);

    let ins = sqlx::query(
        r#"
        INSERT INTO login_attempt
            (token_hash, email, customer_id, device_code_hash, expires_at)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(&token_hash[..])
    .bind(&email)
    .bind(&customer_id)
    .bind(&device_code_hash[..])
    .bind(expires_at)
    .execute(&state.pool)
    .await;
    if let Err(e) = ins {
        tracing::warn!("login_attempt insert failed: {e:#}");
        return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "could not start login");
    }

    let base = state.public_url.trim_end_matches('/');
    let link = format!("{base}/init/login/verify?token={token}");
    let from = std::env::var("VIRTUES_LOGIN_FROM")
        .unwrap_or_else(|_| LOGIN_FROM_DEFAULT.to_string());

    match crate::email::send_login_magic_link(&state.resend_api_key, &from, &email, &link).await {
        Ok(_) => (
            StatusCode::OK,
            Json(json!({ "status": "sent" })),
        )
            .into_response(),
        Err(e) => {
            tracing::warn!("magic link send failed: {e:#}");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "email_send_failed",
                "could not send login email",
            )
        }
    }
}

#[derive(Debug, Deserialize)]
struct LoginVerifyQuery {
    token: String,
}

async fn login_verify(
    State(state): State<AppState>,
    Query(q): Query<LoginVerifyQuery>,
) -> axum::response::Response {
    let token_hash = sha256(q.token.as_bytes());

    // Atomic: claim the login_attempt + mark used in one shot. Concurrent
    // clicks lose the race and see "already used".
    let row: Option<(String, Vec<u8>, chrono::DateTime<Utc>, String)> = sqlx::query_as(
        r#"
        UPDATE login_attempt
        SET status = 'used', used_at = now()
        WHERE token_hash = $1
          AND status = 'pending'
          AND expires_at > now()
        RETURNING email, device_code_hash, expires_at, customer_id
        "#,
    )
    .bind(&token_hash[..])
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some((_email, device_code_hash, _exp, customer_id)) = row else {
        return page(
            "Link expired or already used",
            "This login link is no longer valid. Restart the flow from your box's terminal to get a fresh link.",
        );
    };

    // Re-link recovery: mint a fresh api_key and re-point the device to the
    // existing customer. We trust our own customers table (looked up by email +
    // verified via the magic link), so no Stripe call. We re-point to the SAME
    // `account_id` and do NOT re-credit — the wallet is preserved (the recovery
    // win).
    //
    // Ordering matters: register the new key with virtues-api FIRST, and only
    // rotate `customers.api_key_hash` once that succeeds. If register fails, the
    // customers row keeps the OLD hash — which still matches what virtues-api
    // holds — so the box's old key stays consistent across the proxy AND atlas
    // billing-auth, and the user can just retry. (The opposite order would
    // leave a split-brain: atlas on the new hash, virtues-api on the old.)
    let api_key = super::claim::random_token();
    let api_key_hash = sha256(api_key.as_bytes());

    let lookup: Result<(String,), _> = sqlx::query_as(
        "SELECT account_id FROM customers WHERE stripe_customer_id = $1",
    )
    .bind(&customer_id)
    .fetch_one(&state.pool)
    .await;
    let (account_id,) = match lookup {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("customers lookup failed: {e:#}");
            return page(
                "Something went wrong",
                "We verified your link but couldn't finish attaching the box. Try again, or reach out to support@virtues.com.",
            );
        }
    };

    if let Err(e) = state
        .virtues_api
        .register_device(&crate::virtues_api_client::RegisterDevice {
            // TODO(per-box keys): atlas does not yet know which box is
            // registering here — the box's EndpointId reaches atlas via
            // `/iroh/register`, which is a separate call. Until they are
            // joined up this stays None and rotation keeps its historical
            // whole-account behaviour. The virtues-api side is ready.
            box_id: None,
            api_key_hash: hex::encode(&api_key_hash),
            account_id,
        })
        .await
    {
        tracing::warn!("re-link register_device failed: {e:#}");
        return page(
            "Something went wrong",
            "We verified your link but couldn't finish attaching the box. Try again, or reach out to support@virtues.com.",
        );
    }

    // Device registered — now rotate the stored hash to match.
    if let Err(e) = sqlx::query("UPDATE customers SET api_key_hash = $2 WHERE stripe_customer_id = $1")
        .bind(&customer_id)
        .bind(&api_key_hash[..])
        .execute(&state.pool)
        .await
    {
        tracing::warn!("customers api_key_hash update failed: {e:#}");
        return page(
            "Something went wrong",
            "We verified your link but couldn't finish attaching the box. Try again, or reach out to support@virtues.com.",
        );
    }

    // Flip the bound device_link to ready with the api_key so the box's
    // existing poll handler picks it up on the next /init/poll.
    let flip = sqlx::query(
        "UPDATE device_link SET status = 'ready', api_key = $2 \
         WHERE device_code_hash = $1 AND status = 'pending'",
    )
    .bind(&device_code_hash[..])
    .bind(&api_key)
    .execute(&state.pool)
    .await;
    match flip {
        Ok(r) if r.rows_affected() == 1 => page(
            "✓ Box attached",
            "Your Virtues box is now attached to your subscription. Return to your terminal — the install will continue automatically.",
        ),
        Ok(_) => page(
            "Link expired",
            "The device_link this email was bound to is no longer pending. Restart from your terminal to get a fresh link.",
        ),
        Err(e) => {
            tracing::warn!("device_link flip failed: {e:#}");
            page(
                "Something went wrong",
                "We verified your link but couldn't attach the box. Try again, or reach out to support@virtues.com.",
            )
        }
    }
}

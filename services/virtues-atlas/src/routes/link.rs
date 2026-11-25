//! Device-authorization link flow (RFC 8628 shape).
//!
//! Attaches a box to an account. The box never sees a sign-in page or a
//! Stripe key; it starts a link and polls for the resulting api_key.
//!
//! ```text
//!   box  POST /init/start            -> { device_code, user_code, verification_uri… }
//!   user opens  GET /init?code=…     -> sign-in page (email)
//!   POST /init/login-web             -> magic link emailed to that address
//!   user clicks GET /init/login/verify -> confirm page; its POST approves the link
//!   box  POST /init/poll {device_code} (loop) -> { status:"ready", api_key }
//! ```
//!
//! The api_key is created by the poll that collects it
//! (`redeem_granted_link`) and returned in that response. It is never stored
//! here, only its hash in `box_key`.
//!
//! Paying is a separate step that starts from the linked box
//! (`POST /billing/checkout/sessions`). The link flow has no checkout of its
//! own: a checkout here would have to choose an account from the email typed
//! at Stripe, which proves nothing about who is paying.
//!
//! The `device_code` (secret) is the poll capability; the `user_code` (short,
//! human) is what the customer types. They map to the same `device_link` row.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use chrono::{Duration, Utc};
use rand::RngCore;
use serde::Deserialize;
use serde_json::json;

use super::claim::sha256;
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
        // Stripe's success URL for checkouts opened before checkouts were
        // bound to an account. Kept so those payers see what to do instead
        // of a 404; nothing here attaches anything.
        .route("/init/done", get(legacy_done))
        // ─── [1] Log in flow ──────────────────────────────────────────
        // Box-callable login start: takes a device_code + email, looks
        // up the matching customer, sends a magic link via Resend. The
        // verify_login GET endpoint clicks → marks the device_link ready.
        .route("/init/login", post(login_start))
        // GET shows what the link will do; only the POST from that page
        // attaches. Opening an emailed link must not be enough on its own:
        // anyone holding a link code can have the email sent to any address,
        // and mail scanners open links before people do.
        .route("/init/login/verify", get(login_confirm).post(login_verify))
        // The same login, reachable from the PAGE. `/init/login` takes the
        // box's secret device_code, which a browser cannot know — so without
        // this an owner with an existing account had no way through the web
        // flow at all and was pushed into a second subscription.
        .route("/init/login-web", post(login_web))
        // The same sign-in page as `/init`, kept addressable because boxes
        // and old emails link to it.
        .route("/init/signin", get(signin))
        // The app's inline sign-in: a session-authed approve of the box's
        // in-flight link, keyed on the USER code. Same attach as a clicked
        // magic link; only the proof of identity differs. Called from the
        // airlock webview, hence the CORS layer — the browser pages above
        // need none.
        .route("/init/approve", post(approve).layer(crate::routes::app_cors()))
        // THE KEYSTONE (one-wire-plan Phase 2): a signed-in app
        // asks for a pre-approved device_code and writes it to the box over
        // BLE (0x82); the box redeems it through its ordinary /init/poll the
        // moment it is online. Attach happens AT REDEMPTION, when the box can
        // say which endpoint it is. Airlock-called, hence CORS.
        .route("/init/grant", post(grant).layer(crate::routes::app_cors()))
}

// ─── POST /init/start ───────────────────────────────────────────────────────

/// The identity blob the box has been sending all along ("every field is
/// advisory: atlas tolerates its absence — older boxes send no body",
/// virtues-core/src/virtues_api/link.rs). Until migration 0015 atlas never
/// read it; now `endpoint_id` scopes the eventual per-box key. It remains a
/// LABEL, never an authorization input — this call is unauthenticated.
#[derive(Debug, Default, Deserialize)]
struct StartBody {
    #[serde(default)]
    r#box: StartBoxIdentity,
}

#[derive(Debug, Default, Deserialize)]
struct StartBoxIdentity {
    #[serde(default)]
    endpoint_id: Option<String>,
}

async fn start(
    State(state): State<AppState>,
    // Option<Json<…>>: an older box POSTs no body at all, and a bare Json
    // extractor would answer it 415 — breaking every deployed box in one
    // deploy. Missing/invalid body simply reads as "no identity".
    body: Option<Json<StartBody>>,
) -> axum::response::Response {
    let endpoint_id = body.and_then(|Json(b)| b.r#box.endpoint_id).filter(|s| !s.is_empty());
    // TODO(2026-11-25): remove this and the 'ready' arm of `poll` a day after
    // this image is deployed, then drop `device_link.api_key`. Nothing writes
    // 'ready' or a stored key any more; these serve links approved by the
    // image before it.
    if let Err(e) = sqlx::query(
        "UPDATE device_link SET status = 'expired', api_key = NULL \
         WHERE status = 'ready' AND expires_at < now() - interval '1 day'",
    )
    .execute(&state.pool)
    .await
    {
        tracing::warn!("clearing uncollected link keys failed: {e:#}");
    }
    let device_code = random_hex(32);
    let user_code = gen_user_code();
    let device_code_hash = sha256(device_code.as_bytes());
    let expires_at = Utc::now() + Duration::minutes(LINK_TTL_MINUTES);

    let res = sqlx::query(
        r#"
        INSERT INTO device_link (device_code_hash, user_code, status, expires_at, endpoint_id)
        VALUES ($1, $2, 'pending', $3, $4)
        "#,
    )
    .bind(&device_code_hash[..])
    .bind(&user_code)
    .bind(expires_at)
    .bind(&endpoint_id)
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
    /// The box's self-reported iroh EndpointId — a rotation-scoping label for
    /// the per-box key minted at grant redemption, never an authorization
    /// input. Absent from older boxes' polls.
    #[serde(default)]
    endpoint_id: Option<String>,
}

async fn poll(State(state): State<AppState>, Json(body): Json<PollBody>) -> axum::response::Response {
    let hash = sha256(body.device_code.as_bytes());

    let row: Option<(String, chrono::DateTime<Utc>)> = match sqlx::query_as(
        "SELECT status, expires_at FROM device_link WHERE device_code_hash = $1",
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

    let Some((status, expires_at)) = row else {
        return Json(json!({ "status": "expired" })).into_response();
    };
    if Utc::now() > expires_at && (status == "pending" || status == "granted") {
        return Json(json!({ "status": "expired" })).into_response();
    }

    // An approved link redeems ON THIS POLL: the key is created, registered
    // and returned here, so it is never stored. Every door that approves a
    // link (the emailed confirm page, `/init/approve`, `/init/grant`) leaves
    // the row 'granted' for this.
    if status == "granted" {
        return redeem_granted_link(&state, &hash[..], body.endpoint_id.as_deref()).await;
    }

    match status.as_str() {
        "ready" => {
            // A link approved by an older image, which stored the key on the
            // row (see the TODO in `start`). One-time delivery, atomic: only
            // the poll that flips the row from 'ready' gets the key, and the
            // key leaves the table with it.
            let taken: Option<Option<String>> = match sqlx::query_scalar(
                "WITH taken AS ( \
                   SELECT device_code_hash, api_key FROM device_link \
                   WHERE device_code_hash = $1 AND status = 'ready' FOR UPDATE) \
                 UPDATE device_link d SET status = 'claimed', api_key = NULL \
                 FROM taken WHERE d.device_code_hash = taken.device_code_hash \
                 RETURNING taken.api_key",
            )
            .bind(&hash[..])
            .fetch_optional(&state.pool)
            .await
            {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("link key delivery failed: {e:#}");
                    return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "poll failed");
                }
            };
            match taken.flatten() {
                Some(key) => Json(json!({ "status": "ready", "api_key": key })).into_response(),
                // Another poll took it first.
                None => Json(json!({ "status": "claimed" })).into_response(),
            }
        }
        "claimed" => Json(json!({ "status": "claimed" })).into_response(),
        "denied" => Json(json!({ "status": "denied" })).into_response(),
        _ => Json(json!({ "status": "pending" })).into_response(),
    }
}

// ─── GET /init?code=… (customer-facing verification → sign-in) ─────────────

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
    // A DB error must not render as "code not found" — that tells the owner
    // to fetch a fresh code and retry a DB outage forever (CLAUDE.md,
    // "Do not swallow a query error").
    let row: Option<(chrono::DateTime<Utc>,)> = match sqlx::query_as(
        "SELECT expires_at FROM device_link WHERE user_code = $1 AND status = 'pending'",
    )
    .bind(&code)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("device_link lookup failed: {e:#}");
            return page(
                "Something went wrong",
                "We couldn't look that code up just now. Wait a moment and reload this page.",
            );
        }
    };
    let Some((expires_at,)) = row else {
        return page(
            "Link not found",
            "That code is invalid, already used, or has been replaced by a newer one. \
             Open the Virtues app and start the link again.",
        );
    };
    if Utc::now() > expires_at {
        return page(
            "Link expired",
            "Your box has already replaced this code with a fresh one &mdash; nothing is wrong \
             with it. Open the Virtues app and start the link again.",
        );
    }

    signin_page(&code)
}

#[derive(Debug, Deserialize)]
struct LegacyDoneQuery {
    #[serde(default)]
    session_id: String,
}

async fn legacy_done(Query(q): Query<LegacyDoneQuery>) -> axum::response::Response {
    tracing::warn!(session_id = %q.session_id, "legacy /init/done: unbound payment needs support");
    super::account::unbound_payment_page(&q.session_id)
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

/// HTML escaping for untrusted text interpolated into a page, in text or in a
/// quoted attribute. Quotes matter: the sign-in error page puts the
/// caller-supplied code inside `href='…'`.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub(super) fn page(title: &str, body: &str) -> axum::response::Response {
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
/// which the same `verify` handler then turns into the sign-in page. (The static
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
         <p>Enter the pairing code shown on your box to continue.</p>\
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

/// `GET /init/signin?code=…` — the same sign-in page `/init` renders.
/// Validates the code the same way, because this is a URL someone can
/// bookmark or come back to.
async fn signin(State(state): State<AppState>, Query(q): Query<VerifyQuery>) -> axum::response::Response {
    let Some(code) = q.code.map(|c| c.trim().to_uppercase()).filter(|c| !c.is_empty()) else {
        return connect_page();
    };
    // A DB error must not render as "code not found" — that tells the owner
    // to fetch a fresh code and retry a DB outage forever (CLAUDE.md,
    // "Do not swallow a query error").
    let row: Option<(chrono::DateTime<Utc>,)> = match sqlx::query_as(
        "SELECT expires_at FROM device_link WHERE user_code = $1 AND status = 'pending'",
    )
    .bind(&code)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("device_link lookup failed: {e:#}");
            return page(
                "Something went wrong",
                "We couldn't look that code up just now. Wait a moment and reload this page.",
            );
        }
    };
    let Some((expires_at,)) = row else {
        return page(
            "Link not found",
            "That code is invalid, already used, or has been replaced by a newer one. \
             Open the Virtues app and start the link again.",
        );
    };
    if Utc::now() > expires_at {
        return page(
            "Link expired",
            "Your box has already replaced this code with a fresh one &mdash; nothing is wrong \
             with it. Open the Virtues app and start the link again.",
        );
    }
    signin_page(&code)
}

/// The one sign-in card. A new address is a sign-up: the emailed link
/// creates the account when it is clicked (see `begin_login`). Paying comes
/// after, from the app, once the box is attached.
fn signin_page(code: &str) -> axum::response::Response {
    Html(format!(
        "<!doctype html><html><head><meta charset=utf-8>\
         <meta name=viewport content='width=device-width,initial-scale=1'>\
         <title>Sign in to link your box</title>\
         <style>body{{font-family:system-ui,sans-serif;max-width:32rem;margin:4rem auto;padding:0 1rem;line-height:1.5}}\
         h1{{font-size:1.4rem}}\
         .code{{font-family:ui-monospace,Menlo,monospace;letter-spacing:.08em}}\
         .card{{border:1px solid #ddd;border-radius:12px;padding:1.1rem 1.2rem;margin-top:1.1rem}}\
         p.sub{{margin:0 0 .8rem;color:#555;font-size:.92rem}}\
         form{{display:flex;gap:.5rem}}\
         input{{flex:1;font:inherit;padding:.55rem .7rem;border:1px solid #ccc;border-radius:8px}}\
         button{{font:inherit;padding:.55rem 1.05rem;border:0;border-radius:8px;background:#111;color:#fff;\
         cursor:pointer}}\
         </style></head>\
         <body>\
         <h1>Sign in to link your box</h1>\
         <p>Code <span class=code>{code}</span>. This box is waiting to be linked.</p>\
         <div class=card>\
         <p class=sub>Enter your email and we'll send you a link. Opening it attaches this box \
         to your Virtues account. New here? The same link creates your account.</p>\
         <form method=post action=/init/login-web>\
         <input type=hidden name=code value='{code}'>\
         <input name=email type=email placeholder='you@example.com' autocomplete=email required \
         autofocus aria-label='Email address'>\
         <button type=submit>Email me a link</button>\
         </form>\
         </div>\
         </body></html>"
    ))
    .into_response()
}

#[derive(Debug, Deserialize)]
struct LoginWebBody {
    code: String,
    email: String,
}

/// `POST /init/login-web` — the existing-account door, keyed on the USER code.
///
/// `/init/login` needs the secret `device_code` because the BOX calls it. A
/// browser only ever has the short user code, so this resolves one to the other
/// through `device_link` and then does exactly what `login_start` does.
///
/// Accepting the user code here grants nothing by itself: the box is only
/// attached once the emailed link is opened, which proves the address.
async fn login_web(
    State(state): State<AppState>,
    axum::extract::Form(body): axum::extract::Form<LoginWebBody>,
) -> axum::response::Response {
    let code = body.code.trim().to_uppercase();
    let email = body.email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return page(
            "Check the address",
            &format!(
                "That doesn't look like an email address. \
                 <a href='/init/signin?code={}'>Try again</a>.",
                html_escape(&code)
            ),
        );
    }

    let row: Option<(Vec<u8>,)> = match sqlx::query_as(
        "SELECT device_code_hash FROM device_link \
         WHERE user_code = $1 AND status = 'pending' AND expires_at > now()",
    )
    .bind(&code)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("device_link lookup failed: {e:#}");
            return page(
                "Something went wrong",
                "We couldn't look that code up just now. Wait a moment and resubmit.",
            );
        }
    };
    let Some((device_code_hash,)) = row else {
        return page(
            "Link not found",
            "That code is invalid, used, or has been replaced by a newer one. Open the \
             Virtues app and start the link again.",
        );
    };

    // EVERY outcome carries a way onward. These used to be flat statements —
    // "go back and choose I'm new" named a choice on a page the reader had
    // already left, with no link on the one they were looking at. A browser
    // that autofills the wrong saved address (the common case: the address
    // that pays is not always the address that signs in) put an owner in a
    // dead end they could only leave by restarting from the box (2026-08-13).
    // The address is echoed for the same reason: autofill is silent, so the
    // one fact needed to understand the failure was the one not shown.
    let who = html_escape(&email);
    let back = format!("<a href='/init/signin?code={code}'>try another address</a>");
    match begin_login(&state, &device_code_hash, &email).await {
        LoginOutcome::Sent => page(
            "Check your email",
            &format!(
                "We sent a link to <b>{who}</b>. Open it on any device and choose Attach box. Your box links \
                 itself within a few seconds.<br><br>Wrong address? You can {back}."
            ),
        ),
        LoginOutcome::RateLimited => page(
            "Too many attempts",
            &format!(
                "Too many login emails for <b>{who}</b> in the last hour. Wait an hour, or \
                 {back}."
            ),
        ),
        LoginOutcome::Failed => page(
            "Something went wrong",
            &format!("We couldn't send that email. You can {back}."),
        ),
    }
}

// ─── POST /init/approve (the app's inline sign-in) ──────────────────────────

#[derive(Debug, Deserialize)]
struct ApproveBody {
    user_code: String,
}

/// Session-authed approve of an in-flight link, keyed on the short user code.
///
/// The airlock signs in with the existing `/account/login` + `/verify` (email
/// OTP), then calls this with the code it read off the box over BLE (0x84).
/// From atlas's side it is `login_verify` with a different proof: an OTP
/// session instead of a clicked magic link — the attach itself is the shared
/// `attach_link_to_customer`, so the two doors cannot drift.
///
/// Error codes are part of the airlock contract (linking-plan.md):
/// `link_not_found` / `link_expired` tell it to re-fetch the code and retry —
/// the session stays good, so neither costs a second email round-trip.
/// (`no_subscription` left the contract with 0017: linking is identity, not
/// billing — see agents/record/open-relay.md.)
/// Approve calls per account per hour. Generous for a legitimate owner (one
/// approve, maybe a couple of retries after a code rotation); tight enough that
/// the endpoint cannot be ground as an enumeration oracle.
const MAX_APPROVE_ATTEMPTS_PER_HOUR: i64 = 10;

async fn approve(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<ApproveBody>,
) -> axum::response::Response {
    let Some(sess) = super::account::authed(&state, &headers).await else {
        return err(StatusCode::UNAUTHORIZED, "unauthorized", "sign in again");
    };
    // No entitlement gate (0017): linking is identity, not billing. An unpaid
    // account's api_key funds nothing — the wallet is empty and virtues-api
    // refuses spend — so the key is safe to mint; what it buys is reachability
    // and ownership, which are not for sale. The account row is ensured here
    // because the session may predate the accounts table.
    let account_id = match super::account::ensure_account(&state.pool, &sess.email).await {
        Ok(id) => id,
        Err(e) => {
            tracing::warn!("approve ensure_account failed: {e:#}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "couldn't resolve the account");
        }
    };

    // Attempt budget, keyed on the authenticated account's email — bounds an
    // entitled session's guess rate regardless of which codes it tries, and
    // makes a guessing campaign both capped and (via the miss log below)
    // visible. A DB error here is fail-CLOSED (deny), the opposite of the
    // login-send counter's fail-open bug: refusing a legitimate retry is
    // recoverable; silently lifting the guard on the attach door is not.
    let recent: i64 = match sqlx::query_scalar(
        "SELECT count(*) FROM approve_attempt WHERE email = $1 AND created_at > now() - interval '1 hour'",
    )
    .bind(&sess.email)
    .fetch_one(&state.pool)
    .await
    {
        Ok(n) => n,
        Err(e) => {
            tracing::warn!("approve rate-limit read failed: {e:#}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "couldn't verify the request");
        }
    };
    if recent >= MAX_APPROVE_ATTEMPTS_PER_HOUR {
        tracing::warn!(email = %sess.email, "approve rate limit hit — possible code guessing");
        return err(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "too many link attempts — wait a few minutes and try again",
        );
    }
    // Record this attempt before doing the work, so a burst in flight still
    // counts against the cap. Best-effort: a failed insert must not block a
    // legitimate approve, and the count query above is the real guard.
    let _ = sqlx::query("INSERT INTO approve_attempt (email) VALUES ($1)")
        .bind(&sess.email)
        .execute(&state.pool)
        .await;

    let code = body.user_code.trim().to_uppercase();
    if code.is_empty() {
        return err(StatusCode::BAD_REQUEST, "bad_code", "missing user_code");
    }
    // Do NOT swallow this query error (CLAUDE.md): a broken query answered as
    // link_not_found tells the airlock the code rotated, so it re-fetches and
    // retries a DB outage forever with nothing in the logs. Absent row → the
    // code really is gone (invalid/used/replaced); Err → surface it.
    let row: Option<(Vec<u8>,)> = match sqlx::query_as(
        "SELECT device_code_hash FROM device_link \
         WHERE user_code = $1 AND status = 'pending' AND expires_at > now()",
    )
    .bind(&code)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("approve device_link lookup failed: {e:#}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "couldn't look up the link");
        }
    };
    let Some((device_code_hash,)) = row else {
        // A genuine miss. Logged at info (not warn) so the rate-limit warn
        // above is the signal that stands out; a lone miss is ordinary (a
        // rotated code the app hasn't re-fetched yet).
        tracing::info!(email = %sess.email, "approve: no pending link for code");
        return err(
            StatusCode::NOT_FOUND,
            "link_not_found",
            "that code is invalid, used, or replaced — fetch a fresh one and retry",
        );
    };

    match attach_link_to_account(&state.pool, &device_code_hash, &account_id).await {
        AttachOutcome::Attached => {
            (StatusCode::OK, Json(json!({ "approved": true }))).into_response()
        }
        AttachOutcome::LinkGone => err(
            StatusCode::GONE,
            "link_expired",
            "your server has moved on to a fresh code — fetch it and retry",
        ),
        AttachOutcome::Failed => {
            err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "couldn't attach the box")
        }
    }
}

// ─── POST /init/grant (the 0x82 keystone) ───────────────────────────────────

/// How long a grant stays redeemable. Generous on purpose: the box may sit
/// offline while the owner finishes setup, and the grant is pre-authorized to
/// one account and delivered over a proven line-of-sight channel — the real
/// guard is single-use claiming, not this clock.
const GRANT_TTL_HOURS: i64 = 24;

async fn grant(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> axum::response::Response {
    let Some(sess) = super::account::authed(&state, &headers).await else {
        return err(StatusCode::UNAUTHORIZED, "unauthorized", "sign in again");
    };
    // No entitlement gate (0017) — same reasoning as approve: the key an
    // unpaid account earns funds nothing, and linking is not for sale.
    let account_id = match super::account::ensure_account(&state.pool, &sess.email).await {
        Ok(id) => id,
        Err(e) => {
            tracing::warn!("grant ensure_account failed: {e:#}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "couldn't resolve the account");
        }
    };
    // Same attempt budget as approve — one shared table, one shared story
    // about how fast a session may mint box-shaped things. Fail closed.
    let recent: i64 = match sqlx::query_scalar(
        "SELECT count(*) FROM approve_attempt WHERE email = $1 AND created_at > now() - interval '1 hour'",
    )
    .bind(&sess.email)
    .fetch_one(&state.pool)
    .await
    {
        Ok(n) => n,
        Err(e) => {
            tracing::warn!("grant rate-limit read failed: {e:#}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "couldn't verify the request");
        }
    };
    if recent >= MAX_APPROVE_ATTEMPTS_PER_HOUR {
        tracing::warn!(email = %sess.email, "grant rate limit hit");
        return err(StatusCode::TOO_MANY_REQUESTS, "rate_limited", "too many link attempts — wait a few minutes and try again");
    }
    let _ = sqlx::query("INSERT INTO approve_attempt (email) VALUES ($1)")
        .bind(&sess.email)
        .execute(&state.pool)
        .await;

    let device_code = random_hex(32);
    let user_code = gen_user_code();
    let device_code_hash = sha256(device_code.as_bytes());
    let expires_at = Utc::now() + Duration::hours(GRANT_TTL_HOURS);
    if let Err(e) = sqlx::query(
        r#"
        INSERT INTO device_link (device_code_hash, user_code, status, expires_at, account_id, stripe_customer_id)
        VALUES ($1, $2, 'granted', $3, $4, $5)
        "#,
    )
    .bind(&device_code_hash[..])
    .bind(&user_code)
    .bind(expires_at)
    .bind(&account_id)
    .bind(sess.customer_id.as_deref())
    .execute(&state.pool)
    .await
    {
        tracing::warn!("grant insert failed: {e:#}");
        return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "could not mint a grant");
    }
    (
        StatusCode::OK,
        Json(json!({ "grant": device_code, "expires_in": GRANT_TTL_HOURS * 3600 })),
    )
        .into_response()
}

/// Redeem an approved link: create the box's key and hand it over. Returns
/// the poll response to send.
///
/// The row is claimed first (granted → linking), so a double poll cannot
/// create two keys, and so nothing is rotated for a link that lapsed:
/// `register_device` and `mint_box_key` both replace the key with this label,
/// which would strand a box still holding the old one. Then register with
/// virtues-api, record the key, answer ready. Failures after the claim revert
/// to 'granted' and answer pending — the box's poll loop IS the retry.
/// (The entitlement re-check left with 0017: a refund empties the wallet,
/// which is where non-payment actually bites; it no longer unlinks a box.)
async fn redeem_granted_link(
    state: &AppState,
    device_code_hash: &[u8],
    poll_endpoint_id: Option<&str>,
) -> axum::response::Response {
    let claim: Result<Option<(Option<String>, Option<String>, Option<String>)>, _> = sqlx::query_as(
        "UPDATE device_link SET status = 'linking' \
         WHERE device_code_hash = $1 AND status = 'granted' AND expires_at > now() \
         RETURNING account_id, stripe_customer_id, endpoint_id",
    )
    .bind(device_code_hash)
    .fetch_optional(&state.pool)
    .await;
    let (row_account, customer_id, row_endpoint) = match claim {
        Ok(Some(v)) => v,
        // Lost the race to a concurrent poll (it is doing the work), or the
        // grant lapsed. The generic poll flow already answered the caller
        // correctly for both on the next round.
        Ok(None) => return Json(json!({ "status": "pending" })).into_response(),
        Err(e) => {
            tracing::warn!("granted link claim failed: {e:#}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "poll failed");
        }
    };
    let release = || async {
        let _ = sqlx::query(
            "UPDATE device_link SET status = 'granted' \
             WHERE device_code_hash = $1 AND status = 'linking'",
        )
        .bind(device_code_hash)
        .execute(&state.pool)
        .await;
    };

    // The box self-reports its endpoint, at `/init/start` (on the row) or at
    // redemption (the poll body, the only source for an app-made grant).
    // virtues-api replaces only the key with this label, so a second box
    // linking does not rotate the first box's key. Label, never an
    // authorization input.
    let endpoint_id = poll_endpoint_id
        .map(str::to_string)
        .or(row_endpoint)
        .filter(|s| !s.is_empty());

    // account_id is on the row for grants minted since 0017; a grant minted
    // by an older binary carries only the customer, so resolve through it.
    // A row with neither is a bug, not a state; deny — and park the row at
    // 'denied' rather than leaving it 'linking', where every later poll
    // reads as pending and the box's link loop wedges silently.
    let deny = || async {
        let _ = sqlx::query(
            "UPDATE device_link SET status = 'denied' \
             WHERE device_code_hash = $1 AND status = 'linking'",
        )
        .bind(device_code_hash)
        .execute(&state.pool)
        .await;
        Json(json!({ "status": "denied" })).into_response()
    };
    let account_id: String = match row_account {
        Some(a) => a,
        None => {
            let Some(cid) = customer_id.as_deref() else {
                tracing::warn!("granted link with no account and no customer — denying");
                return deny().await;
            };
            match sqlx::query_scalar(
                "SELECT account_id FROM customers WHERE stripe_customer_id = $1",
            )
            .bind(cid)
            .fetch_one(&state.pool)
            .await
            {
                Ok(a) => a,
                Err(e) => {
                    tracing::warn!("redemption account lookup failed: {e:#}");
                    release().await;
                    return Json(json!({ "status": "pending" })).into_response();
                }
            }
        }
    };

    let label = super::claim::key_label(endpoint_id.as_deref());
    let api_key = super::claim::random_token();
    let api_key_hash = sha256(api_key.as_bytes());
    if let Err(e) = state
        .virtues_api
        .register_device(&crate::virtues_api_client::RegisterDevice {
            box_id: Some(label.clone()),
            api_key_hash: hex::encode(&api_key_hash),
            account_id: account_id.clone(),
        })
        .await
    {
        tracing::warn!("redemption register_device failed: {e:#}");
        release().await;
        return Json(json!({ "status": "pending" })).into_response();
    }
    if let Err(e) = super::claim::mint_box_key(
        &state.pool,
        &account_id,
        &label,
        &api_key_hash[..],
    )
    .await
    {
        // Release for the next poll: it registers a fresh key under the same
        // label, which replaces this undelivered one in both systems.
        tracing::warn!("redemption mint_box_key failed, releasing for a retry: {e:#}");
        release().await;
        return Json(json!({ "status": "pending" })).into_response();
    }
    // One-shot delivery, same as the ready path: mark claimed and hand the
    // key over in this response.
    let _ = sqlx::query(
        "UPDATE device_link SET status = 'claimed' WHERE device_code_hash = $1",
    )
    .bind(device_code_hash)
    .execute(&state.pool)
    .await;
    tracing::info!("approved link redeemed: box attached");
    Json(json!({ "status": "ready", "api_key": api_key })).into_response()
}

/// What `begin_login` decided — shared by the box-callable and web doors so the
/// two can never drift into different rules about accounts or rate limits.
enum LoginOutcome {
    Sent,
    RateLimited,
    Failed,
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
//        └→ atlas inserts login_attempt(token_hash, …), sends a magic link via
//           Resend for ANY address (0017: an unknown email is a sign-up; the
//           account is minted at verify, when the click proves the address)
//   user opens magic link
//        └→ GET /init/login/verify?token=…
//           - hash + lookup login_attempt
//           - show the confirm page; its POST marks the attempt used and
//             approves the bound device_link for the account
//   box  POST /init/poll {device_code}
//        └→ redeems the approved link: creates the api_key and returns it
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
    let device_link_exists: Option<(String,)> = match sqlx::query_as(
        "SELECT status FROM device_link WHERE device_code_hash = $1 AND expires_at > now()",
    )
    .bind(&device_code_hash[..])
    .fetch_optional(&state.pool)
    .await
    {
        Ok(r) => r,
        // A DB error must not answer as "no such link" — the box would
        // restart the whole flow against an outage (CLAUDE.md, "Do not
        // swallow a query error").
        Err(e) => {
            tracing::warn!("login_start device_link lookup failed: {e:#}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "could not check the link");
        }
    };
    if device_link_exists.is_none() {
        return err(
            StatusCode::BAD_REQUEST,
            "no_device_link",
            "device_code not found or expired; restart /init/start first",
        );
    }

    match begin_login(&state, &device_code_hash, &email).await {
        LoginOutcome::Sent => (StatusCode::OK, Json(json!({ "status": "sent" }))).into_response(),
        LoginOutcome::RateLimited => err(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "too many login attempts for this email — try again in an hour",
        ),
        LoginOutcome::Failed => err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "email_send_failed",
            "could not send login email",
        ),
    }
}

/// Rate-limit, ensure the account, mint a magic-link token, send it.
///
/// The one implementation behind both doors — the box's `/init/login` and the
/// page's `/init/login-web`. They differ only in how they learned the
/// `device_code_hash` (the box has the secret; the page looks it up from the
/// short code), and everything after that must be identical or the two drift
/// into different rules about who gets an email and how often.
async fn begin_login(
    state: &AppState,
    device_code_hash: &[u8],
    email: &str,
) -> LoginOutcome {
    // Max 3 send attempts per email per hour.
    // Fail closed: a DB error must not lift the cap.
    let recent: (i64,) = match sqlx::query_as(
        "SELECT COUNT(*) FROM login_attempt \
         WHERE email = $1 AND created_at > now() - interval '1 hour'",
    )
    .bind(email)
    .fetch_one(&state.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("login send-cap lookup failed: {e:#}");
            return LoginOutcome::Failed;
        }
    };
    if recent.0 >= 3 {
        return LoginOutcome::RateLimited;
    }

    // Account-first (0017): an unknown email is someone signing UP, not an
    // error — send the link, exactly as the app's /account/login door already
    // does for any address. The account is minted at VERIFY, not here: the
    // click is the proof of the address, and minting at send would let anyone
    // holding one device_code create a permanent accounts row for every email
    // they can type (the per-email send cap bounds mail rate, not distinct
    // addresses). No customer resolution here either: mint_box_key derives the
    // key's customer from `accounts`.
    let token = random_hex(32);
    let token_hash = sha256(token.as_bytes());
    let expires_at = Utc::now() + Duration::minutes(LOGIN_TTL_MINUTES);

    let ins = sqlx::query(
        r#"
        INSERT INTO login_attempt
            (token_hash, email, device_code_hash, expires_at)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(&token_hash[..])
    .bind(email)
    .bind(device_code_hash)
    .bind(expires_at)
    .execute(&state.pool)
    .await;
    if let Err(e) = ins {
        tracing::warn!("login_attempt insert failed: {e:#}");
        return LoginOutcome::Failed;
    }

    // The email names the link code, so the owner can match it against the
    // code where they started linking before they open the link at all.
    let user_code: String = match sqlx::query_scalar("SELECT user_code FROM device_link WHERE device_code_hash = $1")
        .bind(device_code_hash)
        .fetch_one(&state.pool)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("login link code lookup failed: {e:#}");
            return LoginOutcome::Failed;
        }
    };

    let base = state.public_url.trim_end_matches('/');
    let link = format!("{base}/init/login/verify?token={token}");
    let from =
        std::env::var("VIRTUES_LOGIN_FROM").unwrap_or_else(|_| LOGIN_FROM_DEFAULT.to_string());

    match crate::email::send_login_magic_link(&state.resend_api_key, &from, email, &link, &user_code).await {
        Ok(_) => LoginOutcome::Sent,
        Err(e) => {
            tracing::warn!("magic link send failed: {e:#}");
            LoginOutcome::Failed
        }
    }
}

#[derive(Debug, Deserialize)]
struct LoginVerifyQuery {
    token: String,
}

/// `GET /init/login/verify?token=…` — the page an emailed link opens. It
/// checks the token without using it, shows the link code and what attaching
/// does, and offers one button that POSTs the token back.
async fn login_confirm(
    State(state): State<AppState>,
    Query(q): Query<LoginVerifyQuery>,
) -> axum::response::Response {
    let token_hash = sha256(q.token.as_bytes());
    let row: Option<(String, String)> = match sqlx::query_as(
        r#"
        SELECT la.email, dl.user_code
        FROM login_attempt la
        JOIN device_link dl ON dl.device_code_hash = la.device_code_hash
        WHERE la.token_hash = $1
          AND la.status = 'pending'
          AND la.expires_at > now()
        "#,
    )
    .bind(&token_hash[..])
    .fetch_optional(&state.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("login_attempt lookup failed: {e:#}");
            return page(
                "Something went wrong",
                "We couldn't check this link just now. Wait a moment and open it again. It is still valid.",
            );
        }
    };
    let Some((email, user_code)) = row else {
        return page(
            "Link expired or already used",
            "This link is no longer valid. Open the Virtues app and start the link again \
             &mdash; it fetches a fresh code from your box. (Set the box up from a terminal? \
             Re-run the link command there.)",
        );
    };
    confirm_page(&q.token, &html_escape(&email), &html_escape(&user_code))
}

/// The one-button confirmation behind an emailed link.
fn confirm_page(token: &str, email: &str, user_code: &str) -> axum::response::Response {
    // The token is 64 hex characters from `random_hex`; anything else is not
    // a token this server minted, so it is not echoed into the page.
    let token = if token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit()) {
        token
    } else {
        ""
    };
    Html(format!(
        "<!doctype html><html><head><meta charset=utf-8>\
         <meta name=viewport content='width=device-width,initial-scale=1'>\
         <title>Attach this box?</title>\
         <style>body{{font-family:system-ui,sans-serif;max-width:32rem;margin:4rem auto;padding:0 1rem;line-height:1.5}}\
         h1{{font-size:1.4rem}}\
         .code{{font-family:ui-monospace,Menlo,monospace;letter-spacing:.08em}}\
         p.sub{{color:#555;font-size:.92rem}}\
         button{{font:inherit;padding:.6rem 1.1rem;border:0;border-radius:8px;background:#111;color:#fff;cursor:pointer}}\
         </style></head>\
         <body>\
         <h1>Attach this box to your account?</h1>\
         <p>A box showing link code <span class=code>{user_code}</span> asked to join the \
         Virtues account for <b>{email}</b>. Attaching it lets that box use your account, \
         including its AI credit.</p>\
         <p class=sub>Only attach it if you started linking a box just now. If you typed \
         your email on a page that showed a link code, check that it matches this one. If you \
         didn't start this, close this page. Nothing changes.</p>\
         <form method=post action=/init/login/verify>\
         <input type=hidden name=token value='{token}'>\
         <button type=submit>Attach box</button>\
         </form>\
         </body></html>"
    ))
    .into_response()
}

/// `POST /init/login/verify` — the confirmation page's button. Uses the
/// token and attaches the box.
async fn login_verify(
    State(state): State<AppState>,
    axum::extract::Form(q): axum::extract::Form<LoginVerifyQuery>,
) -> axum::response::Response {
    let token_hash = sha256(q.token.as_bytes());

    // Atomic: claim the login_attempt + mark used in one shot. Concurrent
    // clicks lose the race and see "already used".
    // A DB error here must NOT render as "link expired" — that page tells the
    // owner to restart the whole flow for an outage that will pass. Surface it
    // as its own page and keep the token unclaimed (the UPDATE didn't run).
    let row: Option<(String, Vec<u8>, chrono::DateTime<Utc>)> = match sqlx::query_as(
        r#"
        UPDATE login_attempt
        SET status = 'used', used_at = now()
        WHERE token_hash = $1
          AND status = 'pending'
          AND expires_at > now()
        RETURNING email, device_code_hash, expires_at
        "#,
    )
    .bind(&token_hash[..])
    .fetch_optional(&state.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("login_attempt claim failed: {e:#}");
            return page(
                "Something went wrong",
                "We couldn't check this link just now. Wait a moment and open it again — it is still valid.",
            );
        }
    };

    let Some((email, device_code_hash, _exp)) = row else {
        return page(
            "Link expired or already used",
            "This login link is no longer valid. Open the Virtues app and start the link again \
             &mdash; it fetches a fresh code from your box. (Set the box up from a terminal? \
             Re-run the link command there.)",
        );
    };

    // The click proved control of the address — THIS is where the account is
    // minted (or fetched) for the magic-link door; see begin_login for why
    // not at send time.
    let account_id: String = match super::account::ensure_account(&state.pool, &email).await {
        Ok(a) => a,
        Err(e) => {
            tracing::warn!("magic-link account resolve failed: {e:#}");
            return page(
                "Something went wrong",
                "We verified your link but couldn't finish attaching the box. Try again, or reach out to support@virtues.com.",
            );
        }
    };

    match attach_link_to_account(&state.pool, &device_code_hash, &account_id).await {
        AttachOutcome::Attached => attached_page(&state, &account_id).await,
        AttachOutcome::LinkGone => page(
            "Link expired",
            "This link took too long, and your box has already moved on to a fresh code. \
             Nothing is wrong with it. Open the Virtues app and start the link again.",
        ),
        AttachOutcome::Failed => page(
            "Something went wrong",
            "We verified your link but couldn't finish attaching the box. Try again, or reach out to support@virtues.com.",
        ),
    }
}

/// The page after a confirmed attach. An account without a subscription is
/// offered a checkout for that account right here: the owner just proved the
/// address, so the session can be bound to it. This is also what keeps Setup
/// working on apps that open this page expecting to reach checkout.
async fn attached_page(state: &AppState, account_id: &str) -> axum::response::Response {
    use super::billing_checkout::{open_account_checkout, AccountCheckout};
    const DONE: &str = "Your Virtues box is now attached to your account. Go back to the \
                        Virtues app. It continues on its own within a few seconds.";
    match open_account_checkout(state, account_id).await {
        Ok(AccountCheckout::Url(url)) => {
            let url = html_escape(&url);
            page(
                "✓ Box attached",
                &format!(
                    "Your Virtues box is now attached to your account. To use Virtues AI it \
                     needs a subscription.<br><br>\
                     <a href=\"{url}\">Continue to checkout &rarr;</a><br><br>\
                     <small>Using your own AI instead? Go back to the Virtues app and choose \
                     Your own AI.</small>"
                ),
            )
        }
        Ok(AccountCheckout::Entitled) => page("✓ Box attached", DONE),
        // The attach is done; a checkout that couldn't open is offered again
        // from the app (Settings, Billing).
        Err(e) => {
            tracing::warn!("post-attach checkout failed: {}", e.code);
            page(
                "✓ Box attached",
                "Your Virtues box is now attached to your account. We couldn't open checkout \
                 just now. You can subscribe from the Virtues app in Settings, Billing.",
            )
        }
    }
}

/// What `attach_link_to_account` decided — shared by the magic-link click and
/// the app's `/init/approve`, so the two proofs of identity can never drift
/// into different attach rules.
enum AttachOutcome {
    Attached,
    /// The device_link lapsed or was already taken; the box has moved on to a
    /// fresh code. Retrying with a re-fetched code is the fix.
    LinkGone,
    Failed,
}

/// How long past its own expiry an approved link can still be collected. The
/// box polls every few seconds, so this only covers an approval that lands
/// in a link's last moments.
const APPROVED_COLLECT_GRACE_SECS: f64 = 60.0;

/// Approve an in-flight device_link for an account. The caller has already
/// verified control of the account's email (the emailed confirm page, or an
/// OTP session).
///
/// Nothing is created here. The row goes `pending → granted` and the box's
/// next poll creates the key (`redeem_granted_link`). So the key is never
/// stored, and a link the box never collects rotates nothing: creating the
/// key replaces the one already under this box's label, which would strand a
/// box still holding it.
///
/// `expires_at > now()` matters: once its own link lapses the box starts a
/// new one with a new device_code, and approving the abandoned row would
/// tell the owner "attached" while their box polls a different code.
async fn attach_link_to_account(
    pool: &sqlx::PgPool,
    device_code_hash: &[u8],
    account_id: &str,
) -> AttachOutcome {
    let approved = sqlx::query(
        "UPDATE device_link SET status = 'granted', account_id = $2, \
                expires_at = GREATEST(expires_at, now() + make_interval(secs => $3)) \
         WHERE device_code_hash = $1 AND status = 'pending' AND expires_at > now()",
    )
    .bind(device_code_hash)
    .bind(account_id)
    .bind(APPROVED_COLLECT_GRACE_SECS)
    .execute(pool)
    .await;
    match approved {
        Ok(r) if r.rows_affected() == 1 => AttachOutcome::Attached,
        // The link lapsed, or another door approved it first.
        Ok(_) => AttachOutcome::LinkGone,
        Err(e) => {
            tracing::warn!("device_link approve failed: {e:#}");
            AttachOutcome::Failed
        }
    }
}

#[cfg(test)]
mod approve_tests {
    use super::{attach_link_to_account, AttachOutcome};
    use sqlx::PgPool;

    async fn pending_link(pool: &PgPool, code: &[u8], expires_in_secs: f64) {
        sqlx::query(
            "INSERT INTO device_link (device_code_hash, user_code, status, expires_at) \
             VALUES ($1, 'ABCD-EFGH', 'pending', now() + make_interval(secs => $2))",
        )
        .bind(code)
        .bind(expires_in_secs)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn row(pool: &PgPool, code: &[u8]) -> (String, Option<String>, Option<String>, bool) {
        sqlx::query_as(
            "SELECT status, account_id, api_key, expires_at > now() + interval '30 seconds' \
             FROM device_link WHERE device_code_hash = $1",
        )
        .bind(code)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// Approving stores no key: the row waits for the box's poll, which
    /// creates one.
    #[sqlx::test(migrations = "./migrations")]
    async fn approving_a_link_stores_no_key(pool: PgPool) {
        pending_link(&pool, b"code-1", 600.0).await;
        assert!(matches!(attach_link_to_account(&pool, b"code-1", "acct_a").await, AttachOutcome::Attached));
        let (status, account, key, _) = row(&pool, b"code-1").await;
        assert_eq!((status.as_str(), account.as_deref(), key), ("granted", Some("acct_a"), None));
        let keys: i64 = sqlx::query_scalar("SELECT count(*) FROM box_key").fetch_one(&pool).await.unwrap();
        assert_eq!(keys, 0);
        // A second approval, by any door, finds nothing to approve.
        assert!(matches!(attach_link_to_account(&pool, b"code-1", "acct_b").await, AttachOutcome::LinkGone));
        assert_eq!(row(&pool, b"code-1").await.1.as_deref(), Some("acct_a"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_lapsed_link_is_not_approved(pool: PgPool) {
        pending_link(&pool, b"code-2", -1.0).await;
        assert!(matches!(attach_link_to_account(&pool, b"code-2", "acct_a").await, AttachOutcome::LinkGone));
        assert_eq!(row(&pool, b"code-2").await.0, "pending");
    }

    /// An approval in a link's last seconds still leaves the box time to
    /// collect.
    #[sqlx::test(migrations = "./migrations")]
    async fn a_late_approval_leaves_time_to_collect(pool: PgPool) {
        pending_link(&pool, b"code-3", 2.0).await;
        assert!(matches!(attach_link_to_account(&pool, b"code-3", "acct_a").await, AttachOutcome::Attached));
        assert!(row(&pool, b"code-3").await.3);
    }
}

#[cfg(test)]
mod start_body_tests {
    use super::StartBody;

    /// The EXACT shape virtues-core sends (virtues_api/link.rs `start`) —
    /// extra advisory fields and all. A silent deserialization failure here
    /// would not error anywhere: `Option<Json<StartBody>>` reads a failed
    /// parse as "no body", and every box's key would quietly lose its
    /// endpoint label. This test is what makes that failure loud.
    #[test]
    fn parses_the_real_box_identity_payload() {
        let real = r#"{"box":{"name":"Virtues-Quaint-Tern","label":"Quaint Tern",
            "model":"Dragon Q6A","endpoint_id":"ep_abc123","version":"0.1.2"}}"#;
        let b: StartBody = serde_json::from_str(real).expect("real payload must parse");
        assert_eq!(b.r#box.endpoint_id.as_deref(), Some("ep_abc123"));
    }

    #[test]
    fn tolerates_null_endpoint_and_missing_box() {
        // The pre-bind race sends endpoint_id: null; hypothetical callers may
        // send an empty object. Both must parse to None, not error.
        let null_ep = r#"{"box":{"name":"x","endpoint_id":null}}"#;
        let b: StartBody = serde_json::from_str(null_ep).unwrap();
        assert_eq!(b.r#box.endpoint_id, None);
        let empty: StartBody = serde_json::from_str("{}").unwrap();
        assert_eq!(empty.r#box.endpoint_id, None);
    }
}

#[cfg(test)]
mod confirm_page_tests {
    use super::confirm_page;

    async fn body(r: axum::response::Response) -> String {
        let b = axum::body::to_bytes(r.into_body(), 1 << 20).await.unwrap();
        String::from_utf8(b.to_vec()).unwrap()
    }

    /// The emailed link lands on a page that attaches nothing by itself: the
    /// token only goes back through the form's POST.
    #[tokio::test]
    async fn the_page_posts_the_token_back() {
        let token = "a".repeat(64);
        let html = body(confirm_page(&token, "owner@example.com", "ABCD-EFGH")).await;
        assert!(html.contains("method=post action=/init/login/verify"));
        assert!(html.contains(&format!("value='{token}'")));
        assert!(html.contains("ABCD-EFGH"));
    }

    #[tokio::test]
    async fn a_token_it_did_not_mint_is_not_echoed() {
        let html = body(confirm_page("x'><script>", "owner@example.com", "ABCD-EFGH")).await;
        assert!(!html.contains("<script>"));
        assert!(html.contains("value=''"));
    }
}

#[cfg(test)]
mod escape_tests {
    /// The sign-in error page puts a caller-supplied code inside
    /// `href='…'`, so quotes must not survive.
    #[test]
    fn quotes_cannot_leave_an_attribute() {
        let out = super::html_escape("' autofocus onfocus='x\"<b>");
        assert!(!out.contains('\'') && !out.contains('"') && !out.contains('<'));
    }
}

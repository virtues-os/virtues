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
        // The same login, reachable from the PAGE. `/init/login` takes the
        // box's secret device_code, which a browser cannot know — so without
        // this an owner with an existing account had no way through the web
        // flow at all and was pushed into a second subscription.
        .route("/init/login-web", post(login_web))
        .route("/init/checkout", get(checkout))
        // The existing-account door, addressable on its own. The app asks
        // "new or existing?" itself now, so sending an owner who already
        // answered to a page that asks again is a wasted step.
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
    if Utc::now() > expires_at && (status == "pending" || status == "granted") {
        return Json(json!({ "status": "expired" })).into_response();
    }

    // A granted link redeems ON THIS POLL: entitlement re-check, per-box key
    // mint, register — the attach deferred to the moment the box can say who
    // it is. Everything else is the classic RFC-8628 read.
    if status == "granted" {
        return redeem_granted_link(&state, &hash[..], body.endpoint_id.as_deref()).await;
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

    // TWO DOORS. This used to 302 straight to Stripe, which meant an owner who
    // already pays for Virtues could only ever buy a SECOND subscription —
    // there was no way to attach a new box to the account they have. The
    // magic-link half already existed (`/init/login`); nothing on the web could
    // reach it. A household adding its second box is the common case, not the
    // exotic one.
    return choice_page(&code);
}

/// `GET /init/checkout?code=…` — the "new subscription" door.
///
/// Split out of `verify` when that page gained a choice. Re-validates the code
/// rather than trusting the referrer: this is a link an owner can bookmark,
/// re-open an hour later, or land on after a cancelled Stripe session.
async fn checkout(State(state): State<AppState>, Query(q): Query<VerifyQuery>) -> axum::response::Response {
    let Some(code) = q.code.map(|c| c.trim().to_uppercase()).filter(|c| !c.is_empty()) else {
        return connect_page();
    };
    let row: Option<(chrono::DateTime<Utc>,)> = sqlx::query_as(
        "SELECT expires_at FROM device_link WHERE user_code = $1 AND status = 'pending'",
    )
    .bind(&code)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);
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

/// `GET /init/signin?code=…` — the "existing account" door, on its own URL.
///
/// The chooser at `/init` still exists for anyone who arrives cold. But the app
/// asks which door you want BEFORE it opens a browser, so an owner who already
/// said "I have an account" was being asked the same question twice. Validates
/// the code the same way `checkout` does, and for the same reason: this is a URL
/// someone can bookmark or come back to.
async fn signin(State(state): State<AppState>, Query(q): Query<VerifyQuery>) -> axum::response::Response {
    let Some(code) = q.code.map(|c| c.trim().to_uppercase()).filter(|c| !c.is_empty()) else {
        return connect_page();
    };
    let row: Option<(chrono::DateTime<Utc>,)> = sqlx::query_as(
        "SELECT expires_at FROM device_link WHERE user_code = $1 AND status = 'pending'",
    )
    .bind(&code)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);
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

/// The existing-account card alone. Keeps a way over to checkout: picking the
/// wrong door in the app must not be a dead end.
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
         p.alt{{margin-top:1.6rem;font-size:.85rem;color:#666}}\
         </style></head>\
         <body>\
         <h1>Sign in to link your box</h1>\
         <p>Code <span class=code>{code}</span> &mdash; this box is waiting to be linked.</p>\
         <div class=card>\
         <p class=sub>We'll email you a link. Clicking it attaches this box to your \
         subscription &mdash; no new charge.</p>\
         <form method=post action=/init/login-web>\
         <input type=hidden name=code value='{code}'>\
         <input name=email type=email placeholder='you@example.com' autocomplete=email required \
         autofocus aria-label='Email address'>\
         <button type=submit>Email me a link</button>\
         </form>\
         </div>\
         <p class=alt>Don't have an account yet? \
         <a href='/init/checkout?code={code}'>Start a subscription</a>.</p>\
         </body></html>"
    ))
    .into_response()
}

/// `GET /init?code=…` — the two doors, once the code is known good.
///
/// Deliberately a page and not a redirect. The owner is standing at a box that
/// just told them to come here; this is the moment they decide whether this box
/// joins an account they already pay for or starts a new subscription, and a
/// 302 made that decision for them — always wrongly for an existing customer.
fn choice_page(code: &str) -> axum::response::Response {
    Html(format!(
        "<!doctype html><html><head><meta charset=utf-8>\
         <meta name=viewport content='width=device-width,initial-scale=1'>\
         <title>Connect your Virtues box</title>\
         <style>body{{font-family:system-ui,sans-serif;max-width:32rem;margin:4rem auto;padding:0 1rem;line-height:1.5}}\
         h1{{font-size:1.4rem}}\
         .code{{font-family:ui-monospace,Menlo,monospace;letter-spacing:.08em}}\
         .card{{border:1px solid #ddd;border-radius:12px;padding:1.1rem 1.2rem;margin-top:1.1rem}}\
         h2{{font-size:1rem;margin:0 0 .35rem}}\
         p.sub{{margin:0 0 .8rem;color:#555;font-size:.92rem}}\
         form{{display:flex;gap:.5rem}}\
         input{{flex:1;font:inherit;padding:.55rem .7rem;border:1px solid #ccc;border-radius:8px}}\
         button,a.btn{{font:inherit;padding:.55rem 1.05rem;border:0;border-radius:8px;background:#111;color:#fff;\
         cursor:pointer;text-decoration:none;display:inline-block}}\
         </style></head>\
         <body>\
         <h1>Connect your Virtues box</h1>\
         <p>Code <span class=code>{code}</span> &mdash; this box is waiting to be linked.</p>\
         <div class=card>\
         <h2>I already have a Virtues account</h2>\
         <p class=sub>We'll email you a link. Clicking it attaches this box to your subscription \
         &mdash; no new charge.</p>\
         <form method=post action=/init/login-web>\
         <input type=hidden name=code value='{code}'>\
         <input name=email type=email placeholder='you@example.com' autocomplete=email required \
         autofocus aria-label='Email address'>\
         <button type=submit>Email me a link</button>\
         </form>\
         </div>\
         <div class=card>\
         <h2>I'm new</h2>\
         <p class=sub>Start a subscription for this box.</p>\
         <a class=btn href='/init/checkout?code={code}'>Continue to checkout &rarr;</a>\
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
/// Accepting the user code here grants no more than the page already granted:
/// the same code, one click away, could create a subscription and link this box.
/// Reading it still requires standing in front of the box.
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

    let row: Option<(Vec<u8>,)> = sqlx::query_as(
        "SELECT device_code_hash FROM device_link \
         WHERE user_code = $1 AND status = 'pending' AND expires_at > now()",
    )
    .bind(&code)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);
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
                "We sent a link to <b>{who}</b>. Open it on any device &mdash; your box links \
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
/// billing — see open-relay-plan §Work 1b.)
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

    match attach_link_to_account(&state, &device_code_hash, &account_id).await {
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

/// Redeem a granted link: the attach, deferred to the moment the box shows up
/// with its endpoint. Returns the poll response to send.
///
/// Ordering inside mirrors `attach_link_to_account`: claim the row first
/// (granted → linking, so a double poll can't double-mint), register with
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

    // The box self-reports its endpoint at redemption (the poll body); the
    // grant-time row has none (the app minted it before the box was even
    // online). Label, never an authorization input.
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

    let api_key = super::claim::random_token();
    let api_key_hash = sha256(api_key.as_bytes());
    if let Err(e) = state
        .virtues_api
        .register_device(&crate::virtues_api_client::RegisterDevice {
            box_id: endpoint_id.clone(),
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
        endpoint_id.as_deref(),
        &api_key_hash[..],
    )
    .await
    {
        // Key registered upstream; do NOT release (a retry would re-register
        // destructively). Park at linking; the next poll answers pending and
        // an operator sees the warn.
        tracing::warn!("redemption mint_box_key failed: {e:#}");
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
    tracing::info!("granted link redeemed — box attached at redemption");
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
    let recent: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM login_attempt \
         WHERE email = $1 AND created_at > now() - interval '1 hour'",
    )
    .bind(email)
    .fetch_one(&state.pool)
    .await
    .unwrap_or((0,));
    if recent.0 >= 3 {
        return LoginOutcome::RateLimited;
    }

    // Account-first (0017): an unknown email is someone signing UP, not an
    // error — send the link, exactly as the app's /account/login door already
    // does for any address. The account is minted at VERIFY, not here: the
    // click is the proof of the address, and minting at send would let anyone
    // holding one device_code create a permanent accounts row for every email
    // they can type (the per-email send cap bounds mail rate, not distinct
    // addresses). No customer resolution here either — the key mirror derives
    // its customer from `accounts` at mint time.
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

    let base = state.public_url.trim_end_matches('/');
    let link = format!("{base}/init/login/verify?token={token}");
    let from =
        std::env::var("VIRTUES_LOGIN_FROM").unwrap_or_else(|_| LOGIN_FROM_DEFAULT.to_string());

    match crate::email::send_login_magic_link(&state.resend_api_key, &from, email, &link).await {
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

async fn login_verify(
    State(state): State<AppState>,
    Query(q): Query<LoginVerifyQuery>,
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

    match attach_link_to_account(&state, &device_code_hash, &account_id).await {
        AttachOutcome::Attached => page(
            "✓ Box attached",
            "Your Virtues box is now attached to your account. Go back to the Virtues app — it continues on its own within a few seconds.",
        ),
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

/// Attach an in-flight device_link to an account: mint a fresh api_key,
/// register it with virtues-api, rotate the stored hash, flip the link to
/// ready for the box's poll. A free account attaches identically (0017); the
/// legacy key mirror derives its customer from `accounts` inside
/// `mint_box_key`.
///
/// Re-link recovery semantics: the caller has already verified control of the
/// email (magic-link click or OTP session), so no Stripe call. We re-point to
/// the SAME `account_id` and do NOT re-credit — the wallet is preserved (the
/// recovery win).
///
/// ## Why the link is CLAIMED before anything is rotated
///
/// `register_device` (box_id=None) DELETEs the account's whole key set and
/// inserts the new one, and rotating `customers.api_key_hash` retires the old
/// hash for atlas's own billing-auth. Both are destructive to any box already
/// on the account. The link flip used to be LAST — so a lost race (the code
/// rotated, or the magic-link door and this one both fired) rotated the
/// account key and then flipped ZERO rows, leaving an existing box holding a
/// key neither virtues-api nor atlas would accept any more. Permanent, silent.
/// Codes rotate every 15 minutes and the app offers one-tap retry, so that
/// race is ordinary, not exotic.
///
/// So we CLAIM the row first — an atomic `pending → linking` guarded on
/// expiry — and only touch virtues-api and `customers` once the claim is ours.
/// A lost race now costs nothing. `linking` reads as "keep polling" to the box
/// (its poll handler maps every unknown status to pending), and no other door
/// can re-claim it (both flip `WHERE status = 'pending'`).
///
/// Register-before-rotate still holds inside the claim: register the new key
/// with virtues-api FIRST, then rotate `customers.api_key_hash`. If register
/// fails, nothing in virtues-api changed and we release the claim back to
/// pending so a retry (or the other door) can proceed cleanly.
async fn attach_link_to_account(
    state: &AppState,
    device_code_hash: &[u8],
    account_id: &str,
) -> AttachOutcome {
    let api_key = super::claim::random_token();
    let api_key_hash = sha256(api_key.as_bytes());

    // CLAIM the link before any destructive write. `expires_at > now()` is
    // load-bearing: once its own link lapses the box STARTS A NEW ONE with a
    // new device_code, abandoning this row — claiming an abandoned row would
    // render "attached" at someone whose box is polling a different code
    // entirely (2026-08-13). 0 rows → the link is gone or already taken, and
    // crucially NOTHING has been rotated yet, so an existing box is untouched.
    // RETURNING endpoint_id: the box that started this link identified itself
    // at /init/start (0015), and that label is what scopes the key we mint —
    // a second box linking must not rotate the first box's credential.
    let claim: Result<Option<Option<String>>, _> = sqlx::query_scalar(
        "UPDATE device_link SET status = 'linking' \
         WHERE device_code_hash = $1 AND status = 'pending' AND expires_at > now() \
         RETURNING endpoint_id",
    )
    .bind(device_code_hash)
    .fetch_optional(&state.pool)
    .await;
    let endpoint_id: Option<String> = match claim {
        Ok(Some(ep)) => ep,
        Ok(None) => return AttachOutcome::LinkGone,
        Err(e) => {
            tracing::warn!("device_link claim failed: {e:#}");
            return AttachOutcome::Failed;
        }
    };
    // From here the row is 'linking' and ours. Any early return that is not a
    // completed attach must release it back to 'pending' so the box (and a
    // retry) can use it again — EXCEPT after register has already dropped the
    // old key, where releasing would invite a second destructive register.
    let release = || async {
        let _ = sqlx::query(
            "UPDATE device_link SET status = 'pending' \
             WHERE device_code_hash = $1 AND status = 'linking'",
        )
        .bind(device_code_hash)
        .execute(&state.pool)
        .await;
    };

    if let Err(e) = state
        .virtues_api
        .register_device(&crate::virtues_api_client::RegisterDevice {
            // The endpoint_id the box self-reported at /init/start, carried on
            // the device_link row (0015). Some → virtues-api replaces only
            // THIS box's key; None (older box) → the historical whole-account
            // rotation. Label, never an authorization input.
            box_id: endpoint_id.clone(),
            api_key_hash: hex::encode(&api_key_hash),
            account_id: account_id.to_string(),
        })
        .await
    {
        // register_device runs in a transaction, so a failure changed nothing
        // in virtues-api — safe to release the claim and let the user retry.
        tracing::warn!("re-link register_device failed: {e:#}");
        release().await;
        return AttachOutcome::Failed;
    }

    // Device registered (old key now dropped in virtues-api) — record the key
    // atlas-side with the same scoping (box_key + the legacy customers
    // mirror), via the shared mint_box_key so the two systems can never
    // disagree about rotation scope. Do NOT release the claim past this
    // point: the old key is already gone upstream, so the row must reach
    // 'ready' with the new key, and a retry resumes from 'linking' (finalize
    // below is idempotent on it) rather than re-registering.
    if let Err(e) = super::claim::mint_box_key(
        &state.pool,
        account_id,
        endpoint_id.as_deref(),
        &api_key_hash[..],
    )
    .await
    {
        tracing::warn!("mint_box_key failed: {e:#}");
        return AttachOutcome::Failed;
    }

    // Finalize: publish the api_key on the claimed row so the box's poll
    // collects it. Guarded on our own 'linking' claim, so this is a no-op if a
    // retry already finalized.
    let finalize = sqlx::query(
        "UPDATE device_link SET status = 'ready', api_key = $2 \
         WHERE device_code_hash = $1 AND status = 'linking'",
    )
    .bind(device_code_hash)
    .bind(&api_key)
    .execute(&state.pool)
    .await;
    match finalize {
        Ok(r) if r.rows_affected() == 1 => AttachOutcome::Attached,
        // 0 rows: the claim vanished under us (a concurrent finalize, or an
        // admin reset). The key is registered and rotated, so the account is
        // consistent; report Attached rather than sending the user to retry a
        // link that is effectively done.
        Ok(_) => AttachOutcome::Attached,
        Err(e) => {
            tracing::warn!("device_link finalize failed: {e:#}");
            AttachOutcome::Failed
        }
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
        let real = r#"{"box":{"name":"Virtues-Honest-Kestrel","label":"Honest Kestrel",
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

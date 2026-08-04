//! OAuth proxy routes — the Rust port of the Node `apps/oauth-proxy` (WS-4).
//!
//! Folds the GitHub-less proxy (google / notion / strava / plaid) into
//! virtues-api. Contract is unchanged, so the home server just repoints
//! `VIRTUES_OAUTH_PROXY_URL` at virtues-api:
//!
//!   GET  /{provider}/start            — kick off; redirect to the provider
//!   GET  /{provider}/callback         — exchange the code, sign a short-lived
//!                                       exchange_token, bounce to return_url
//!   POST /{provider}/exchange/{token} — home server pulls {secrets, metadata,
//!                                       expires_in, scopes} server-to-server
//!   POST /{provider}/refresh          — refresh access_token (no-op for
//!                                       notion/plaid; their tokens are permanent)
//!
//! Secrets never touch the home server's browser except inside the signed,
//! 5-minute exchange_token. The exchange-token HMAC lives in
//! `virtues_helpers::crypto` (the Lint-3 home), keyed by `OAUTH_PROXY_EXCHANGE_SECRET`.

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Json, Router,
};
use base64::Engine;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/:provider/start", get(start))
        .route("/:provider/callback", get(callback))
        .route("/:provider/exchange/:token", post(exchange))
        .route("/:provider/refresh", post(refresh))
}

// ─────────────────────────────────────────────────────────────────────────
// Provider config (from env, mirroring the Node oauthConfigs)
// ─────────────────────────────────────────────────────────────────────────

struct ProviderCfg {
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    scopes: Vec<String>,
    auth_url: String,
    token_url: String,
}

fn env(k: &str) -> String {
    std::env::var(k).unwrap_or_default()
}
fn env_or(k: &str, default: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| default.to_string())
}

fn provider_cfg(p: &str) -> Option<ProviderCfg> {
    match p {
        "google" => Some(ProviderCfg {
            client_id: env("GOOGLE_CLIENT_ID"),
            client_secret: env("GOOGLE_CLIENT_SECRET"),
            redirect_uri: env_or("GOOGLE_REDIRECT_URI", "https://auth.virtues.com/google/callback"),
            scopes: vec![
                "https://www.googleapis.com/auth/calendar.readonly".into(),
                "https://www.googleapis.com/auth/gmail.readonly".into(),
                "https://www.googleapis.com/auth/drive.readonly".into(),
            ],
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token_url: "https://oauth2.googleapis.com/token".into(),
        }),
        "notion" => Some(ProviderCfg {
            client_id: env("NOTION_CLIENT_ID"),
            client_secret: env("NOTION_CLIENT_SECRET"),
            redirect_uri: env_or("NOTION_REDIRECT_URI", "https://auth.virtues.com/notion/callback"),
            scopes: vec![],
            auth_url: "https://api.notion.com/v1/oauth/authorize".into(),
            token_url: "https://api.notion.com/v1/oauth/token".into(),
        }),
        "strava" => Some(ProviderCfg {
            client_id: env("STRAVA_CLIENT_ID"),
            client_secret: env("STRAVA_CLIENT_SECRET"),
            redirect_uri: env_or("STRAVA_REDIRECT_URI", "https://auth.virtues.com/strava/callback"),
            scopes: vec!["read,activity:read_all".into()], // Strava: comma-separated
            auth_url: "https://www.strava.com/oauth/authorize".into(),
            token_url: "https://www.strava.com/oauth/token".into(),
        }),
        "plaid" => Some(ProviderCfg {
            client_id: env("PLAID_CLIENT_ID"),
            client_secret: env("PLAID_SECRET"),
            redirect_uri: env_or("PLAID_REDIRECT_URI", "https://auth.virtues.com/plaid/callback"),
            scopes: vec![],
            auth_url: String::new(), // Plaid uses Hosted Link, not an authorize URL
            token_url: String::new(),
        }),
        _ => None,
    }
}

fn exchange_secret() -> Result<String, String> {
    let s = env("OAUTH_PROXY_EXCHANGE_SECRET");
    if s.len() < 32 {
        return Err("OAUTH_PROXY_EXCHANGE_SECRET must be set to a >= 32 char value".into());
    }
    Ok(s)
}

// ─────────────────────────────────────────────────────────────────────────
// Helpers: state, return-url validation, redirects, errors
// ─────────────────────────────────────────────────────────────────────────

/// `base64(json({return_url, rust_state}))` — round-trips through the provider's
/// `state` param so the callback can recover where to bounce the browser back to.
fn encode_state(return_url: &str, rust_state: &str) -> String {
    let j = json!({ "return_url": return_url, "rust_state": rust_state });
    base64::engine::general_purpose::STANDARD.encode(j.to_string())
}
fn decode_state(s: &str) -> Option<(String, String)> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(s).ok()?;
    let v: Value = serde_json::from_slice(&bytes).ok()?;
    let return_url = v.get("return_url")?.as_str()?.to_string();
    let rust_state = v
        .get("rust_state")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    Some((return_url, rust_state))
}

/// Open-redirect guard (port of url-validator.ts).
///
/// This is the only control deciding where an `exchange_token` gets delivered,
/// and `/exchange/{token}` has no auth — whoever receives that redirect can
/// trade it for the user's provider tokens. So the guard is load-bearing.
///
/// It matches on the *shape* of an address rather than on the identity of the
/// box that started the flow, which is a weaker thing than it looks: the box
/// signs `state` with a key the proxy doesn't have, so the proxy cannot bind a
/// return_url to whoever asked for it. The durable fix is a session row keyed to
/// an authenticated box (the shape `plaid_link_session` already has, for a
/// different reason). Until then this list is what we have — so it should be
/// exactly as wide as the deployments we actually ship, and no wider.
///
/// Private IPv4/IPv6 is in the list because the box hands those out itself:
/// `qr_pair_url` puts the raw LAN IP in the onboarding QR on purpose (phones
/// fumble mDNS), and `virtues link` prints it as the `.local` fallback. Without
/// this, every OAuth connect from a phone that scanned the QR fails with a 400
/// raised on a host the box operator cannot see.
///
/// Public addresses stay rejected, which is what keeps an attacker's own server
/// out. Note the redirect is issued to the *browser* — the proxy never dials a
/// private address itself, so widening this opens no SSRF surface.
fn is_valid_return_url(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };

    // An IP literal is never a domain, so test it first. `host_str` brackets
    // IPv6; strip them before parsing. Userinfo tricks (`http://192.168.1.1@evil
    // .com`) never reach here as an IP — the parser already resolved the host to
    // `evil.com`, which then falls through to the domain arm and is rejected.
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(v4) = bare.parse::<std::net::Ipv4Addr>() {
        // RFC 1918 (10/8, 172.16/12, 192.168/16) plus loopback.
        return v4.is_private() || v4.is_loopback();
    }
    if let Ok(v6) = bare.parse::<std::net::Ipv6Addr>() {
        return is_private_v6(&v6);
    }

    host == "localhost"
        || host.ends_with(".virtues.com")
        || host.ends_with(".local")
        || host.ends_with(".localhost")
}

/// IPv6 counterpart to `Ipv4Addr::is_private`: unique-local (`fc00::/7`) and
/// link-local unicast (`fe80::/10`), plus loopback. Hand-rolled because
/// `is_unique_local` / `is_unicast_link_local` are still unstable in std.
fn is_private_v6(ip: &std::net::Ipv6Addr) -> bool {
    let first = ip.segments()[0];
    ip.is_loopback() || (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
}

/// Whether this return_url is reachable only from the user's own network — an
/// mDNS name or a private address. Drives the "continue on your home network"
/// interstitial in `redirect_back`.
///
/// Loopback is deliberately excluded: `localhost` always resolves, so a box
/// serving its own browser needs the seamless 302, not a page telling it to go
/// somewhere it already is.
fn is_lan_only_host(u: &reqwest::Url) -> bool {
    let Some(host) = u.host_str() else {
        return false;
    };
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(v4) = bare.parse::<std::net::Ipv4Addr>() {
        return v4.is_private();
    }
    if let Ok(v6) = bare.parse::<std::net::Ipv6Addr>() {
        return !v6.is_loopback() && is_private_v6(&v6);
    }
    host.ends_with(".local")
}

fn err(status: StatusCode, msg: &str) -> axum::response::Response {
    (status, Json(json!({ "error": msg }))).into_response()
}

/// Bounce the browser back to the home server with the rust state + either the
/// exchange_token (success) or an error code.
fn redirect_back(return_url: &str, rust_state: &str, key: &str, val: &str) -> axum::response::Response {
    let mut u = match reqwest::Url::parse(return_url) {
        Ok(u) => u,
        Err(_) => return err(StatusCode::BAD_REQUEST, "invalid return_url"),
    };
    u.query_pairs_mut()
        .append_pair("state", rust_state)
        .append_pair(key, val);
    let target = u.as_str().to_string();

    // For LAN-hosted boxes, the final hop redirects the browser to an address
    // that only resolves — or only routes — on the user's home WiFi. If the
    // user is currently on a different network we can't deliver them to the
    // box, but we can stop dumping them into a blank page. Show an explanatory
    // "click to continue on your home network" page first; the click itself
    // still fails off-LAN, but the failure mode is explicit instead of
    // mysterious. For everything else (dev, `*.virtues.com`, future
    // remote-access shapes) we keep the seamless 302.
    //
    // A private IP needs this at least as much as `.local` does: off-network it
    // hangs on a TCP timeout rather than failing fast on DNS, so the unexplained
    // version of it is the slower and more confusing one.
    if is_lan_only_host(&u) {
        lan_continue_page(&target).into_response()
    } else {
        Redirect::to(&target).into_response()
    }
}

/// HTML intermediary page rendered between the provider callback and the
/// final `.local` redirect. Pure HTML, no JS dependencies — the page works
/// in any browser, including ones with strict CSP. Inline minimal styling
/// so this is one file with zero asset deps.
fn lan_continue_page(target: &str) -> axum::response::Response {
    use axum::http::{header, StatusCode};
    use axum::response::IntoResponse;

    // Escape the target href; it's already URL-encoded but we still need
    // to avoid injecting markup. Replacing the four characters that matter
    // in an attribute value is enough here.
    let safe_target = target
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");

    let body = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <meta name="referrer" content="no-referrer">
  <title>Almost done — Virtues</title>
  <style>
    body {{
      font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto,
                   "Helvetica Neue", Arial, sans-serif;
      max-width: 540px;
      margin: 0 auto;
      padding: 48px 24px;
      color: #1f2937;
      background: #f9fafb;
      line-height: 1.5;
    }}
    h1 {{ font-size: 22px; margin: 0 0 12px; }}
    p  {{ font-size: 15px; color: #4b5563; }}
    a.btn {{
      display: inline-block;
      margin-top: 18px;
      padding: 10px 18px;
      background: #111827;
      color: #ffffff;
      text-decoration: none;
      border-radius: 8px;
      font-weight: 500;
    }}
    .hint {{
      margin-top: 28px;
      padding: 12px 14px;
      background: #fff;
      border: 1px solid #e5e7eb;
      border-radius: 8px;
      font-size: 13px;
      color: #4b5563;
    }}
    code {{
      background: #f3f4f6;
      padding: 1px 5px;
      border-radius: 4px;
      font-size: 12px;
    }}
  </style>
</head>
<body>
  <h1>Almost done</h1>
  <p>
    Click below to finish connecting on your Virtues box. This only works
    when you're on the same home network as the box.
  </p>
  <p>
    <a class="btn" href="{safe_target}">Continue on my home network</a>
  </p>
  <div class="hint">
    If the button takes you to a blank page or a
    <code>DNS_PROBE_FINISHED_NXDOMAIN</code> error, your laptop is not on
    the home network where your box lives. Open this same email or browser
    tab from a device on your home WiFi and click again.
  </div>
</body>
</html>"#
    );

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        body,
    )
        .into_response()
}

// ─────────────────────────────────────────────────────────────────────────
// GET /{provider}/start
// ─────────────────────────────────────────────────────────────────────────

async fn start(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> axum::response::Response {
    let Some(cfg) = provider_cfg(&provider) else {
        return err(StatusCode::NOT_FOUND, "unknown provider");
    };
    let (Some(return_url), Some(rust_state)) = (q.get("return_url"), q.get("state")) else {
        return err(StatusCode::BAD_REQUEST, "missing return_url or state");
    };
    if !is_valid_return_url(return_url) {
        return err(StatusCode::BAD_REQUEST, "invalid return_url");
    }
    // Plaid carries its session server-side (see the Hosted Link note below),
    // so it never gets a `state` blob to round-trip.
    if provider == "plaid" {
        return plaid_start(&state, &cfg, return_url, rust_state).await;
    }
    let proxy_state = encode_state(return_url, rust_state);

    // Standard OAuth authorize redirect.
    let mut url = match reqwest::Url::parse(&cfg.auth_url) {
        Ok(u) => u,
        Err(_) => return err(StatusCode::INTERNAL_SERVER_ERROR, "bad auth_url"),
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("client_id", &cfg.client_id)
            .append_pair("redirect_uri", &cfg.redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("state", &proxy_state);
        if !cfg.scopes.is_empty() {
            qp.append_pair("scope", &cfg.scopes.join(" "));
        }
        match provider.as_str() {
            "google" => {
                qp.append_pair("access_type", "offline")
                    .append_pair("prompt", "consent");
            }
            "strava" => {
                qp.append_pair("approval_prompt", "auto");
            }
            "notion" => {
                qp.append_pair("owner", "user");
            }
            _ => {}
        }
    }
    Redirect::to(url.as_str()).into_response()
}

// ─────────────────────────────────────────────────────────────────────────
// Plaid Hosted Link
//
// Plaid is not an OAuth provider, so it does not fit the code/authorize shape
// the other three share. We use **Hosted Link**: Plaid hosts the whole Link
// flow (including each bank's own OAuth round-trip) at a URL it gives us, then
// redirects the browser to our `completion_redirect_uri` when the session ends.
//
// Two properties of that contract drive the design here:
//
//  1. The completion redirect carries NO result — not the public_token, not
//     even success-vs-exit. You learn the outcome by calling `/link/token/get`
//     with the *link_token*, which means the callback needs a link_token that
//     only `/plaid/start` ever saw.
//  2. The completion URI must be registered in the Plaid dashboard and is
//     matched exactly, so it cannot carry a per-session query param.
//
// Hence `plaid_link_session`: `/plaid/start` parks {link_token, return_url,
// rust_state} and hands the browser a first-party session cookie; the callback
// reads the cookie, polls Plaid, and bounces back to the box exactly like the
// OAuth providers do. Plaid owns the bank-OAuth leg end to end, so there is no
// `receivedRedirectUri` / `oauth_state_id` resume dance on our side at all.
// ─────────────────────────────────────────────────────────────────────────

/// Hosted Link URL lifetime, and therefore the session row + cookie lifetime.
/// Plaid's own default when it isn't delivering the URL itself is 30 minutes;
/// matching it keeps all three clocks the same.
const PLAID_LINK_TTL_SECS: i64 = 1800;

/// Session cookie name. Scoped to `/plaid` so it is never sent to the other
/// providers' routes.
const PLAID_SESSION_COOKIE: &str = "virtues_plaid_session";

/// `/link/token/create` body for a Hosted Link session. Pure so the shape is
/// unit-testable without a Plaid account.
fn plaid_link_token_body(cfg: &ProviderCfg, completion_redirect_uri: &str) -> Value {
    json!({
        "client_id": cfg.client_id,
        "secret": cfg.client_secret,
        "client_name": "Virtues",
        "user": { "client_user_id": "virtues-user" },
        "products": ["transactions"],
        // NOTE: `optional_products: ["investments", "liabilities"]` was removed
        // because the Plaid account isn't enabled for those products — Plaid
        // rejects the whole `link/token/create` with INVALID_PRODUCT, breaking
        // every connect. Re-add once the account is enabled for them (and then
        // the plaid_investments_sync / plaid_liabilities_sync actions light up;
        // both no-op cleanly on PRODUCTS_NOT_SUPPORTED until they do).
        "country_codes": ["US"],
        "language": "en",
        // No top-level `redirect_uri`: that is the *self-hosted* Link OAuth
        // contract, where our own page has to relaunch Link with
        // `receivedRedirectUri` after the bank bounces back. Hosted Link keeps
        // that leg inside Plaid, and mixing the two is what hung the flow
        // before. `completion_redirect_uri` is the only redirect we own.
        "hosted_link": {
            "completion_redirect_uri": completion_redirect_uri,
            "url_lifetime_seconds": PLAID_LINK_TTL_SECS,
        },
    })
}

async fn plaid_start(
    state: &AppState,
    cfg: &ProviderCfg,
    return_url: &str,
    rust_state: &str,
) -> axum::response::Response {
    let body = plaid_link_token_body(cfg, &cfg.redirect_uri);
    let resp = state
        .http_client
        .post(format!("{}/link/token/create", state.config.plaid_base_url))
        .json(&body)
        .send()
        .await;
    let v = match json_ok(resp).await {
        Ok(v) => v,
        Err(e) => return err(StatusCode::BAD_GATEWAY, &format!("plaid link_token failed: {e}")),
    };
    let (Some(link_token), Some(hosted_link_url)) = (
        v.get("link_token").and_then(|x| x.as_str()),
        v.get("hosted_link_url").and_then(|x| x.as_str()),
    ) else {
        // A link_token without a hosted_link_url means the `hosted_link` block
        // was rejected or ignored — fail loudly rather than falling back to the
        // self-hosted webview shape, which cannot complete (see module note).
        return err(
            StatusCode::BAD_GATEWAY,
            "plaid link/token/create returned no hosted_link_url",
        );
    };

    let session_id = uuid::Uuid::new_v4().to_string();
    if let Err(e) = put_link_session(&state.db, &session_id, link_token, return_url, rust_state).await
    {
        tracing::error!(error = %e, "plaid link session store failed");
        return err(StatusCode::INTERNAL_SERVER_ERROR, "could not start plaid session");
    }

    let mut headers = HeaderMap::new();
    headers.insert(header::LOCATION, match hosted_link_url.parse() {
        Ok(v) => v,
        Err(_) => return err(StatusCode::BAD_GATEWAY, "plaid returned an unusable hosted_link_url"),
    });
    if let Ok(c) = session_cookie(&session_id, &cfg.redirect_uri).parse() {
        headers.insert(header::SET_COOKIE, c);
    }
    (StatusCode::FOUND, headers).into_response()
}

/// Session cookie for the Hosted Link round-trip. `SameSite=Lax` is what makes
/// this work: Plaid's completion redirect is a top-level cross-site GET
/// navigation, which Lax still sends the cookie on (Strict would not).
/// `Secure` is conditional so a plain-http dev proxy can still round-trip.
fn session_cookie(session_id: &str, completion_uri: &str) -> String {
    let mut c = format!(
        "{PLAID_SESSION_COOKIE}={session_id}; Path=/plaid; Max-Age={PLAID_LINK_TTL_SECS}; HttpOnly; SameSite=Lax"
    );
    if completion_uri.starts_with("https://") {
        c.push_str("; Secure");
    }
    c
}

fn read_session_cookie(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    raw.split(';')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| k.trim() == PLAID_SESSION_COOKIE)
        .map(|(_, v)| v.trim().to_string())
}

async fn put_link_session(
    db: &sqlx::PgPool,
    session_id: &str,
    link_token: &str,
    return_url: &str,
    rust_state: &str,
) -> Result<(), sqlx::Error> {
    // Opportunistic sweep: this table's only writer is this function, so
    // expired rows are collected here instead of by a cron nobody would own.
    let _ = sqlx::query("DELETE FROM plaid_link_session WHERE expires_at < now()")
        .execute(db)
        .await;
    sqlx::query(
        "INSERT INTO plaid_link_session (session_id, link_token, return_url, rust_state, expires_at)
         VALUES ($1, $2, $3, $4, now() + make_interval(secs => $5))",
    )
    .bind(session_id)
    .bind(link_token)
    .bind(return_url)
    .bind(rust_state)
    .bind(PLAID_LINK_TTL_SECS as f64)
    .execute(db)
    .await
    .map(|_| ())
}

/// Single-use read: the row is deleted as it is returned, so a replayed
/// completion redirect can't re-exchange the session.
async fn take_link_session(
    db: &sqlx::PgPool,
    session_id: &str,
) -> Result<Option<(String, String, String)>, sqlx::Error> {
    sqlx::query_as(
        "DELETE FROM plaid_link_session
          WHERE session_id = $1 AND expires_at > now()
          RETURNING link_token, return_url, rust_state",
    )
    .bind(session_id)
    .fetch_optional(db)
    .await
}

// ─────────────────────────────────────────────────────────────────────────
// GET /{provider}/callback
// ─────────────────────────────────────────────────────────────────────────

async fn callback(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> axum::response::Response {
    let Some(cfg) = provider_cfg(&provider) else {
        return err(StatusCode::NOT_FOUND, "unknown provider");
    };

    // Plaid's completion redirect carries nothing at all — no code, no state,
    // no outcome. Everything comes from the session cookie + `/link/token/get`.
    if provider == "plaid" {
        return plaid_callback(&state, &cfg, &headers).await;
    }

    let Some((return_url, rust_state)) = q.get("state").and_then(|s| decode_state(s)) else {
        return err(StatusCode::BAD_REQUEST, "missing or invalid state");
    };
    if !is_valid_return_url(&return_url) {
        return err(StatusCode::BAD_REQUEST, "invalid return_url in state");
    }
    if q.get("error").is_some() {
        return redirect_back(&return_url, &rust_state, "error", "provider_error");
    }

    // Exchange the provider's code → {secrets, metadata, expires_in, scopes}.
    // (Plaid returned above; it has no authorization code to exchange.)
    let exchanged = match provider.as_str() {
        "google" | "strava" | "notion" => exchange_oauth_code(&state, &provider, &cfg, &q).await,
        _ => Err("unknown provider".into()),
    };
    let payload = match exchanged {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("oauth {provider} callback exchange failed: {e}");
            return redirect_back(&return_url, &rust_state, "error", "token_exchange_failed");
        }
    };

    let secret = match exchange_secret() {
        Ok(s) => s,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    };
    match virtues_helpers::crypto::sign_exchange_token(
        &secret,
        &provider,
        payload.secrets,
        payload.metadata,
        payload.expires_in,
        payload.scopes,
    ) {
        Ok(token) => redirect_back(&return_url, &rust_state, "exchange_token", &token),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &format!("sign failed: {e}")),
    }
}

/// Normalized payload before signing / on exchange-out.
struct Normalized {
    secrets: Value,
    metadata: Value,
    expires_in: Option<i64>,
    scopes: Option<Vec<String>>,
}

async fn exchange_oauth_code(
    state: &AppState,
    provider: &str,
    cfg: &ProviderCfg,
    q: &HashMap<String, String>,
) -> Result<Normalized, String> {
    let code = q.get("code").ok_or("missing code")?;

    // Build the token request (Notion uses Basic auth + omits client creds in body;
    // Google includes redirect_uri; Strava omits it).
    let mut form: Vec<(&str, &str)> = vec![("grant_type", "authorization_code"), ("code", code)];
    let mut req = state.http_client.post(&cfg.token_url);
    if provider == "notion" {
        let basic = base64::engine::general_purpose::STANDARD
            .encode(format!("{}:{}", cfg.client_id, cfg.client_secret));
        req = req.header(header::AUTHORIZATION, format!("Basic {basic}"));
        form.push(("redirect_uri", &cfg.redirect_uri));
    } else {
        form.push(("client_id", &cfg.client_id));
        form.push(("client_secret", &cfg.client_secret));
        if provider == "google" {
            form.push(("redirect_uri", &cfg.redirect_uri));
        }
    }
    let v = send_form(req, &form).await?;

    let access = v.get("access_token").and_then(|x| x.as_str()).ok_or("no access_token")?.to_string();
    match provider {
        "google" => {
            let scope = v.get("scope").and_then(|x| x.as_str()).map(String::from);
            Ok(Normalized {
                secrets: json!({ "access_token": access, "refresh_token": v.get("refresh_token") }),
                metadata: json!({ "granted_scopes": scope }),
                expires_in: v.get("expires_in").and_then(|x| x.as_i64()),
                scopes: scope.map(|s| s.split(' ').map(String::from).collect()),
            })
        }
        "strava" => Ok(Normalized {
            secrets: json!({
                "access_token": access,
                "refresh_token": v.get("refresh_token"),
                "expires_at": v.get("expires_at"),
            }),
            metadata: json!({ "athlete": v.get("athlete") }),
            expires_in: v.get("expires_in").and_then(|x| x.as_i64()),
            scopes: None,
        }),
        "notion" => Ok(Normalized {
            secrets: json!({
                "access_token": access,
                "bot_id": v.get("bot_id"),
                "workspace_id": v.get("workspace_id"),
            }),
            metadata: json!({
                "workspace_name": v.get("workspace_name"),
                "workspace_icon": v.get("workspace_icon"),
                "owner": v.get("owner"),
            }),
            expires_in: None,
            scopes: None,
        }),
        _ => Err("unknown provider".into()),
    }
}

/// The Hosted Link return leg. Reached with an empty query string, so the whole
/// session comes from the cookie: link_token to ask Plaid what happened, plus
/// the box's return_url + state to bounce back to.
async fn plaid_callback(
    state: &AppState,
    cfg: &ProviderCfg,
    headers: &HeaderMap,
) -> axum::response::Response {
    let session = match read_session_cookie(headers) {
        Some(id) => match take_link_session(&state.db, &id).await {
            Ok(session) => session,
            Err(e) => {
                tracing::error!(error = %e, "plaid link session lookup failed");
                return err(StatusCode::INTERNAL_SERVER_ERROR, "plaid session lookup failed");
            }
        },
        None => None,
    };
    // No cookie or no live row: we don't know which box started this, so there
    // is nowhere to bounce back to. Explain it instead of 400-ing into a blank
    // page — the user is sitting in front of this.
    let Some((link_token, return_url, rust_state)) = session else {
        return plaid_session_lost_page();
    };
    if !is_valid_return_url(&return_url) {
        return err(StatusCode::BAD_REQUEST, "invalid return_url in session");
    }

    // Hosted Link reports the outcome only through `/link/token/get`.
    let body = json!({
        "client_id": cfg.client_id,
        "secret": cfg.client_secret,
        "link_token": link_token,
    });
    let resp = state
        .http_client
        .post(format!("{}/link/token/get", state.config.plaid_base_url))
        .json(&body)
        .send()
        .await;
    let session_info = match json_ok(resp).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("plaid link/token/get failed: {e}");
            return finish_plaid(&return_url, &rust_state, "error", "token_exchange_failed", cfg);
        }
    };

    // Plaid fires the completion redirect whether the user linked an account or
    // backed out, so "no public_token" is the ordinary cancel path — not an
    // error to show. It is ALSO what a response-shape drift would look like,
    // though, and those two must not be indistinguishable: log the session's
    // key set (keys only — these payloads carry tokens) so a first live run can
    // tell "the user cancelled" from "Plaid renamed a field on us".
    let Some(public_token) = extract_public_token(&session_info) else {
        tracing::info!(
            sessions = session_info
                .get("link_sessions")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0),
            shape = %describe_session_shape(&session_info),
            "plaid hosted link finished without a public_token"
        );
        return finish_plaid(&return_url, &rust_state, "error", "connect_cancelled", cfg);
    };

    let exchanged = exchange_plaid_public_token(
        state,
        cfg,
        &public_token,
        extract_institution(&session_info),
    )
    .await;
    let payload = match exchanged {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("plaid public_token exchange failed: {e}");
            return finish_plaid(&return_url, &rust_state, "error", "token_exchange_failed", cfg);
        }
    };

    let secret = match exchange_secret() {
        Ok(s) => s,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    };
    match virtues_helpers::crypto::sign_exchange_token(
        &secret,
        "plaid",
        payload.secrets,
        payload.metadata,
        payload.expires_in,
        payload.scopes,
    ) {
        Ok(token) => finish_plaid(&return_url, &rust_state, "exchange_token", &token, cfg),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &format!("sign failed: {e}")),
    }
}

/// `redirect_back` plus expiry of the session cookie — the row is already gone,
/// so leaving the cookie behind would only produce a confusing second attempt.
fn finish_plaid(
    return_url: &str,
    rust_state: &str,
    key: &str,
    val: &str,
    cfg: &ProviderCfg,
) -> axum::response::Response {
    let mut resp = redirect_back(return_url, rust_state, key, val);
    let cleared = format!(
        "{PLAID_SESSION_COOKIE}=; Path=/plaid; Max-Age=0; HttpOnly; SameSite=Lax{}",
        if cfg.redirect_uri.starts_with("https://") { "; Secure" } else { "" }
    );
    if let Ok(v) = cleared.parse() {
        resp.headers_mut().insert(header::SET_COOKIE, v);
    }
    resp
}

/// Pull the public_token out of a `/link/token/get` response.
///
/// `results.item_add_results[].public_token` is the current shape;
/// `on_success.public_token` is the legacy one and still populated for
/// single-Item sessions. Sessions are ordered oldest-first, so scan from the
/// back: a user who exited once and retried in the same Hosted Link URL has
/// several, and the last successful one is the one they meant.
fn extract_public_token(session_info: &Value) -> Option<String> {
    let sessions = session_info.get("link_sessions")?.as_array()?;
    sessions.iter().rev().find_map(|s| {
        let from_results = s
            .get("results")
            .and_then(|r| r.get("item_add_results"))
            .and_then(|r| r.as_array())
            .and_then(|arr| {
                arr.iter()
                    .find_map(|i| i.get("public_token").and_then(|t| t.as_str()))
            });
        from_results
            .or_else(|| {
                s.get("on_success")
                    .and_then(|o| o.get("public_token"))
                    .and_then(|t| t.as_str())
            })
            .map(String::from)
    })
}

/// Key names only, never values — a `/link/token/get` response carries
/// public_tokens. Enough to tell a genuine user-exit (`exit` populated,
/// `item_add_results` empty) from a schema that moved under us (neither key
/// present at all).
fn describe_session_shape(session_info: &Value) -> String {
    let Some(sessions) = session_info.get("link_sessions").and_then(|v| v.as_array()) else {
        let top: Vec<&str> = session_info
            .as_object()
            .map(|o| o.keys().map(String::as_str).collect())
            .unwrap_or_default();
        return format!("no link_sessions; top-level keys: {top:?}");
    };
    sessions
        .iter()
        .map(|s| {
            let keys: Vec<&str> = s
                .as_object()
                .map(|o| o.keys().map(String::as_str).collect())
                .unwrap_or_default();
            let adds = s
                .get("results")
                .and_then(|r| r.get("item_add_results"))
                .and_then(|r| r.as_array())
                .map(|a| a.len());
            format!("{{keys: {keys:?}, item_add_results: {adds:?}}}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Institution `(id, name)` from the same response, when Plaid included it.
/// Best-effort: it is a display label, and `plaid_accounts_sync` falls back to
/// "Unknown" without it.
fn extract_institution(session_info: &Value) -> Option<(String, String)> {
    let sessions = session_info.get("link_sessions")?.as_array()?;
    sessions.iter().rev().find_map(|s| {
        let inst = s
            .get("results")
            .and_then(|r| r.get("item_add_results"))
            .and_then(|r| r.as_array())
            .and_then(|arr| arr.iter().find_map(|i| i.get("institution")))
            .or_else(|| {
                s.get("on_success")
                    .and_then(|o| o.get("metadata"))
                    .and_then(|m| m.get("institution"))
            })?;
        let name = inst.get("name").and_then(|v| v.as_str())?.to_string();
        let id = inst
            .get("institution_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        Some((id, name))
    })
}

async fn exchange_plaid_public_token(
    state: &AppState,
    cfg: &ProviderCfg,
    public_token: &str,
    institution: Option<(String, String)>,
) -> Result<Normalized, String> {
    let body = json!({
        "client_id": cfg.client_id,
        "secret": cfg.client_secret,
        "public_token": public_token,
    });
    let resp = state
        .http_client
        .post(format!(
            "{}/item/public_token/exchange",
            state.config.plaid_base_url
        ))
        .json(&body)
        .send()
        .await;
    let v = json_ok(resp).await?;
    let access = v.get("access_token").and_then(|x| x.as_str()).ok_or("no access_token")?;

    // `institution_name` is what `plaid_accounts_sync` stamps on every account
    // row and what names the credential in the UI; without it every connected
    // bank reads "Unknown". Prefer what the Link session already told us, and
    // only fall back to the two-call lookup if it didn't.
    let institution = match institution {
        Some(i) => Some(i),
        None => plaid_lookup_institution(state, cfg, access).await,
    };
    let (institution_id, institution_name) = match institution {
        Some((id, name)) => (Value::String(id), Value::String(name)),
        None => (Value::Null, Value::Null),
    };

    Ok(Normalized {
        secrets: json!({ "access_token": access }),
        metadata: json!({
            "item_id": v.get("item_id"),
            "institution_id": institution_id,
            // `institution_name` is the Plaid-shaped key `plaid_accounts_sync`
            // reads; `display_name` is the provider-agnostic one the box uses to
            // title a credential, so core never has to know what an institution
            // is. Same string, two audiences.
            "institution_name": institution_name.clone(),
            "display_name": institution_name,
        }),
        expires_in: None,
        scopes: None,
    })
}

/// `/item/get` → institution_id → `/institutions/get_by_id` → name. Two extra
/// calls, once per connect, and entirely best-effort: any failure just leaves
/// the label unset rather than failing a connect that otherwise succeeded.
async fn plaid_lookup_institution(
    state: &AppState,
    cfg: &ProviderCfg,
    access_token: &str,
) -> Option<(String, String)> {
    let item = json_ok(
        state
            .http_client
            .post(format!("{}/item/get", state.config.plaid_base_url))
            .json(&json!({
                "client_id": cfg.client_id,
                "secret": cfg.client_secret,
                "access_token": access_token,
            }))
            .send()
            .await,
    )
    .await
    .ok()?;
    let institution_id = item
        .get("item")
        .and_then(|i| i.get("institution_id"))
        .and_then(|v| v.as_str())?
        .to_string();

    let inst = json_ok(
        state
            .http_client
            .post(format!("{}/institutions/get_by_id", state.config.plaid_base_url))
            .json(&json!({
                "client_id": cfg.client_id,
                "secret": cfg.client_secret,
                "institution_id": institution_id,
                "country_codes": ["US"],
            }))
            .send()
            .await,
    )
    .await
    .ok()?;
    let name = inst
        .get("institution")
        .and_then(|i| i.get("name"))
        .and_then(|v| v.as_str())?
        .to_string();
    Some((institution_id, name))
}

/// Shown when the completion redirect arrives without a usable session — the
/// cookie was cleared, blocked, or the 30-minute window lapsed. We genuinely
/// cannot route the user onward here (the return_url lived in that row), so the
/// only honest move is to say what happened and send them back to the app.
fn plaid_session_lost_page() -> axum::response::Response {
    let body = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <meta name="referrer" content="no-referrer">
  <title>Session expired — Virtues</title>
  <style>
    body { font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto,
           "Helvetica Neue", Arial, sans-serif; max-width: 480px; margin: 0 auto;
           padding: 64px 24px; color: #1f2937; background: #f9fafb; line-height: 1.5;
           text-align: center; }
    h1 { font-size: 22px; margin: 0 0 8px; }
    p  { font-size: 15px; color: #4b5563; margin: 0; }
  </style>
</head>
<body>
  <h1>This connection attempt expired</h1>
  <p>Nothing was connected. Open Virtues and start the bank connection again.</p>
</body>
</html>"#;
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        body,
    )
        .into_response()
}

// ─────────────────────────────────────────────────────────────────────────
// POST /{provider}/exchange/{token}
// ─────────────────────────────────────────────────────────────────────────

async fn exchange(Path((provider, token)): Path<(String, String)>) -> axum::response::Response {
    if provider_cfg(&provider).is_none() {
        return err(StatusCode::NOT_FOUND, "unknown provider");
    }
    let secret = match exchange_secret() {
        Ok(s) => s,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    };
    match virtues_helpers::crypto::verify_exchange_token(&secret, &token, &provider) {
        Ok(c) => Json(json!({
            "secrets": c.secrets,
            "metadata": c.metadata,
            "expires_in": c.expires_in,
            "scopes": c.scopes,
        }))
        .into_response(),
        Err(e) => err(StatusCode::BAD_REQUEST, &format!("invalid exchange_token: {e}")),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// POST /{provider}/refresh
// ─────────────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct RefreshBody {
    refresh_token: Option<String>,
}

async fn refresh(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
    Json(body): Json<RefreshBody>,
) -> axum::response::Response {
    let Some(cfg) = provider_cfg(&provider) else {
        return err(StatusCode::NOT_FOUND, "unknown provider");
    };
    let Some(refresh_token) = body.refresh_token.filter(|s| !s.is_empty()) else {
        return err(StatusCode::BAD_REQUEST, "missing refresh_token");
    };

    // Notion + Plaid tokens are permanent — echo back in the canonical shape so
    // the credential-refresh cron is a no-op success.
    if provider == "notion" || provider == "plaid" {
        return Json(json!({
            "secrets": { "access_token": refresh_token },
            "metadata": {},
            "expires_in": Value::Null,
            "scopes": Value::Null,
        }))
        .into_response();
    }

    // Google / Strava: real refresh-token grant.
    let form: Vec<(&str, &str)> = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", &refresh_token),
        ("client_id", &cfg.client_id),
        ("client_secret", &cfg.client_secret),
    ];
    let v = match send_form(state.http_client.post(&cfg.token_url), &form).await {
        Ok(v) => v,
        Err(e) => {
            let status = if e.contains("invalid_grant") {
                StatusCode::UNAUTHORIZED
            } else {
                StatusCode::BAD_GATEWAY
            };
            return err(status, &format!("refresh failed: {e}"));
        }
    };
    let Some(access) = v.get("access_token").and_then(|x| x.as_str()) else {
        return err(StatusCode::BAD_GATEWAY, "no access_token in refresh response");
    };
    // Google may omit a new refresh_token — preserve the old one.
    let new_refresh = v
        .get("refresh_token")
        .and_then(|x| x.as_str())
        .unwrap_or(&refresh_token);

    let (secrets, expires_in, scopes) = if provider == "strava" {
        (
            json!({ "access_token": access, "refresh_token": new_refresh, "expires_at": v.get("expires_at") }),
            v.get("expires_in").and_then(|x| x.as_i64()).or(Some(21600)),
            Value::Null,
        )
    } else {
        let scope = v.get("scope").and_then(|x| x.as_str());
        (
            json!({ "access_token": access, "refresh_token": new_refresh }),
            v.get("expires_in").and_then(|x| x.as_i64()),
            scope.map(|s| json!(s.split(' ').collect::<Vec<_>>())).unwrap_or(Value::Null),
        )
    };
    let metadata = if provider == "strava" {
        json!({})
    } else {
        json!({ "granted_scopes": v.get("scope") })
    };
    Json(json!({ "secrets": secrets, "metadata": metadata, "expires_in": expires_in, "scopes": scopes }))
        .into_response()
}

// ─────────────────────────────────────────────────────────────────────────
// Small HTTP helpers
// ─────────────────────────────────────────────────────────────────────────

async fn send_form(
    req: reqwest::RequestBuilder,
    form: &[(&str, &str)],
) -> Result<Value, String> {
    let resp = req.form(form).send().await;
    json_ok(resp).await
}

async fn json_ok(resp: reqwest::Result<reqwest::Response>) -> Result<Value, String> {
    let resp = resp.map_err(|e| e.to_string())?;
    let status = resp.status();
    if !status.is_success() {
        // F5: discard the upstream body on error. OAuth/Plaid error responses
        // can echo back caller-supplied secrets (notably Plaid `public_token`
        // on exchange failures), and this error string flows into
        // `tracing::warn!` at the call site. Status only; never the body.
        let _ = resp.text().await;
        return Err(format!("upstream status {}", status.as_u16()));
    }
    let body = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str::<Value>(&body).map_err(|e| format!("bad json: {e}"))
}

// (`parse_field` lived here to pull `link_token` out of the old single-field
// Plaid response; Hosted Link needs two fields at once, so plaid_start reads
// the object directly and this had no callers left.)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_addresses_the_box_hands_out() {
        // `qr_pair_url` / `virtues link` print these; a connect started from one
        // must not die on the proxy.
        for url in [
            "http://192.168.1.40:7117/oauth/callback",
            "http://10.0.0.7:7117/oauth/callback",
            "http://172.16.5.2:7117/oauth/callback",
            "http://172.31.255.254:7117/oauth/callback",
            "http://virtues.local:7117/oauth/callback",
            "http://localhost:7117/oauth/callback",
            "http://127.0.0.1:7117/oauth/callback",
            "https://app.virtues.com/oauth/callback",
        ] {
            assert!(is_valid_return_url(url), "should accept {url}");
        }
    }

    #[test]
    fn rejects_public_and_near_miss_addresses() {
        for url in [
            // An attacker's own server is the whole threat model.
            "https://evil.com/oauth/callback",
            "http://8.8.8.8/oauth/callback",
            // 172.15 and 172.32 sit just outside RFC 1918's 172.16/12.
            "http://172.15.0.1/oauth/callback",
            "http://172.32.0.1/oauth/callback",
            // 169.254/16 is link-local, not a LAN the box is served on.
            "http://169.254.1.1/oauth/callback",
            // Suffix matching is not substring matching.
            "https://virtues.com.evil.com/oauth/callback",
            "https://notvirtues.com/oauth/callback",
            "not a url",
        ] {
            assert!(!is_valid_return_url(url), "should reject {url}");
        }
    }

    #[test]
    fn ip_encoding_tricks_do_not_bypass_the_guard() {
        // Userinfo: the host is `evil.com`, not the private-looking prefix.
        assert!(!is_valid_return_url("http://192.168.1.1@evil.com/oauth/callback"));
        // Integer-encoded IPv4 normalizes to 192.168.1.1 — an alias for an
        // address we deliberately allow, so accepting it is correct, not a hole.
        assert!(is_valid_return_url("http://3232235777/oauth/callback"));
        // ...and the same encoding of a public address stays rejected.
        assert!(!is_valid_return_url("http://134744072/oauth/callback")); // 8.8.8.8
    }

    #[test]
    fn lan_only_hosts_get_the_interstitial() {
        let lan = |s: &str| is_lan_only_host(&reqwest::Url::parse(s).unwrap());
        // Off-network these hang or fail to resolve; explain rather than 302.
        assert!(lan("http://virtues.local:7117/oauth/callback"));
        assert!(lan("http://192.168.1.40:7117/oauth/callback"));
        assert!(lan("http://[fd00::1]:7117/oauth/callback"));
        // Loopback and public hosts always work where the browser already is.
        assert!(!lan("http://localhost:7117/oauth/callback"));
        assert!(!lan("http://127.0.0.1:7117/oauth/callback"));
        assert!(!lan("https://app.virtues.com/oauth/callback"));
    }

    #[test]
    fn ipv6_private_ranges_only() {
        assert!(is_valid_return_url("http://[::1]:7117/oauth/callback"));
        assert!(is_valid_return_url("http://[fd00::1]:7117/oauth/callback")); // unique-local
        assert!(is_valid_return_url("http://[fe80::1]:7117/oauth/callback")); // link-local
        assert!(!is_valid_return_url("http://[2001:4860:4860::8888]/oauth/callback"));
    }

    fn plaid_cfg() -> ProviderCfg {
        ProviderCfg {
            client_id: "cid".into(),
            client_secret: "sec".into(),
            redirect_uri: "https://auth.virtues.com/plaid/callback".into(),
            scopes: vec![],
            auth_url: String::new(),
            token_url: String::new(),
        }
    }

    #[test]
    fn link_token_body_requests_hosted_link_and_no_redirect_uri() {
        let body = plaid_link_token_body(&plaid_cfg(), "https://auth.virtues.com/plaid/callback");
        assert_eq!(
            body["hosted_link"]["completion_redirect_uri"],
            json!("https://auth.virtues.com/plaid/callback")
        );
        assert_eq!(body["products"], json!(["transactions"]));
        // A top-level `redirect_uri` is the self-hosted-Link contract. Setting it
        // alongside hosted_link is what put the old flow into an OAuth-resume
        // state it could never satisfy, so it must stay absent.
        assert!(body.get("redirect_uri").is_none());
        // Likewise: optional_products triggers INVALID_PRODUCT on an account that
        // isn't enabled for them, which kills the whole connect.
        assert!(body.get("optional_products").is_none());
    }

    #[test]
    fn session_cookie_survives_plaids_cross_site_redirect() {
        let c = session_cookie("sess-1", "https://auth.virtues.com/plaid/callback");
        // Lax (not Strict) is the load-bearing bit: Plaid's completion redirect
        // is a top-level cross-site navigation.
        assert!(c.contains("SameSite=Lax"));
        assert!(c.contains("HttpOnly"));
        assert!(c.contains("Secure"));
        assert!(c.contains("Path=/plaid"));
        // http dev proxy: Secure would make the cookie undeliverable.
        assert!(!session_cookie("s", "http://localhost:8080/plaid/callback").contains("Secure"));
    }

    #[test]
    fn reads_session_cookie_among_others() {
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            "other=1; virtues_plaid_session=sess-42; another=2".parse().unwrap(),
        );
        assert_eq!(read_session_cookie(&h).as_deref(), Some("sess-42"));
        assert_eq!(read_session_cookie(&HeaderMap::new()), None);
    }

    #[test]
    fn extracts_public_token_from_item_add_results() {
        let v = json!({
            "link_sessions": [{
                "results": { "item_add_results": [{
                    "public_token": "public-sandbox-1",
                    "institution": { "institution_id": "ins_1", "name": "Chase" }
                }]}
            }]
        });
        assert_eq!(extract_public_token(&v).as_deref(), Some("public-sandbox-1"));
        assert_eq!(
            extract_institution(&v),
            Some(("ins_1".to_string(), "Chase".to_string()))
        );
    }

    #[test]
    fn extracts_public_token_from_legacy_on_success() {
        let v = json!({
            "link_sessions": [{
                "on_success": {
                    "public_token": "public-legacy",
                    "metadata": { "institution": { "institution_id": "ins_2", "name": "Ally" } }
                }
            }]
        });
        assert_eq!(extract_public_token(&v).as_deref(), Some("public-legacy"));
        assert_eq!(
            extract_institution(&v),
            Some(("ins_2".to_string(), "Ally".to_string()))
        );
    }

    #[test]
    fn prefers_the_last_successful_session() {
        // Exited once, retried on the same Hosted Link URL: take the retry.
        let v = json!({
            "link_sessions": [
                { "exit": { "error": { "error_code": "INVALID_CREDENTIALS" } } },
                { "results": { "item_add_results": [{ "public_token": "public-second" }] } }
            ]
        });
        assert_eq!(extract_public_token(&v).as_deref(), Some("public-second"));
    }

    #[test]
    fn user_exit_yields_no_public_token() {
        // Plaid fires the completion redirect on exit too — this is the cancel
        // path, and it must be distinguishable from a failure.
        let v = json!({
            "link_sessions": [{
                "exit": { "institution": null, "error": null },
                "results": { "item_add_results": [] }
            }]
        });
        assert_eq!(extract_public_token(&v), None);
        assert_eq!(extract_institution(&v), None);
    }

    #[test]
    fn session_shape_distinguishes_cancel_from_drift() {
        // Genuine exit: the keys we expect are there, the results are just empty.
        let cancelled = json!({
            "link_sessions": [{ "exit": {}, "results": { "item_add_results": [] } }]
        });
        let s = describe_session_shape(&cancelled);
        assert!(s.contains("exit"), "{s}");
        assert!(s.contains("item_add_results: Some(0)"), "{s}");

        // Drift: Plaid answered, but nothing we know how to read is present.
        let drifted = json!({ "sessions": [{ "token": "p" }] });
        let s = describe_session_shape(&drifted);
        assert!(s.contains("no link_sessions"), "{s}");
        assert!(s.contains("sessions"), "{s}");

        // Never leak a token into the log line.
        let with_token = json!({
            "link_sessions": [{ "results": { "item_add_results": [{ "public_token": "public-secret" }] } }]
        });
        assert!(!describe_session_shape(&with_token).contains("public-secret"));
    }

    #[test]
    fn missing_institution_is_tolerated() {
        let v = json!({
            "link_sessions": [{ "results": { "item_add_results": [{ "public_token": "p" }] } }]
        });
        assert_eq!(extract_public_token(&v).as_deref(), Some("p"));
        assert_eq!(extract_institution(&v), None);
    }
}

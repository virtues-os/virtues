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
    http::{header, StatusCode},
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
fn is_valid_return_url(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    host == "localhost"
        || host == "127.0.0.1"
        || host.ends_with(".virtues.com")
        || host.ends_with(".local")
        || host.ends_with(".localhost")
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

    // For LAN-hosted boxes (`.local`), the final hop redirects the browser
    // to a hostname that only resolves on the user's home WiFi. If the user
    // is currently on a different network we can't deliver them to the
    // box — but we can stop dumping them into a blank-page DNS error.
    // Show an explanatory "click to continue on your home network" page
    // first; the click itself still fails off-LAN, but the failure mode is
    // explicit instead of mysterious. For non-`.local` return URLs (dev /
    // future remote-access shapes) we keep the seamless 302.
    let is_local_host = u
        .host_str()
        .map(|h| h.eq_ignore_ascii_case("virtues.local") || h.ends_with(".local"))
        .unwrap_or(false);
    if is_local_host {
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
    let proxy_state = encode_state(return_url, rust_state);

    if provider == "plaid" {
        return plaid_start(&state, &cfg, &proxy_state).await;
    }

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

async fn plaid_start(
    state: &AppState,
    cfg: &ProviderCfg,
    proxy_state: &str,
) -> axum::response::Response {
    let body = json!({
        "client_id": cfg.client_id,
        "secret": cfg.client_secret,
        "client_name": "Virtues",
        "user": { "client_user_id": "virtues-user" },
        "products": ["transactions"],
        // NOTE: `optional_products: ["investments", "liabilities"]` was removed
        // because the Plaid account isn't enabled for those products — Plaid
        // rejects the whole `link/token/create` with INVALID_PRODUCT, breaking
        // every connect. Re-add once the account is enabled for them (and then
        // the plaid_investments_sync / plaid_liabilities_sync actions light up).
        "country_codes": ["US"],
        "language": "en",
        "redirect_uri": cfg.redirect_uri,
    });
    let resp = state
        .http_client
        .post(format!("{}/link/token/create", state.config.plaid_base_url))
        .json(&body)
        .send()
        .await;
    let link_token = match parse_field(resp, "link_token").await {
        Ok(t) => t,
        Err(e) => return err(StatusCode::BAD_GATEWAY, &format!("plaid link_token failed: {e}")),
    };
    // NB: do NOT set `receivedRedirectUri` on the *initial* Hosted Link launch.
    // That param is Plaid's OAuth-resume signal — present, it makes Link try to
    // resume a nonexistent OAuth session and hang on the spinner. The
    // `redirect_uri` is already baked into the link_token via link/token/create
    // above; it belongs only on the post-bank-OAuth resume leg (see the plaid
    // branch in `callback`), carrying the *full* received URL with its params.
    let mut hosted = reqwest::Url::parse("https://cdn.plaid.com/link/v2/stable/link.html").unwrap();
    hosted
        .query_pairs_mut()
        .append_pair("isWebview", "true")
        .append_pair("token", &link_token)
        .append_pair("state", proxy_state);
    Redirect::to(hosted.as_str()).into_response()
}

// ─────────────────────────────────────────────────────────────────────────
// GET /{provider}/callback
// ─────────────────────────────────────────────────────────────────────────

async fn callback(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> axum::response::Response {
    let Some(cfg) = provider_cfg(&provider) else {
        return err(StatusCode::NOT_FOUND, "unknown provider");
    };
    let Some((return_url, rust_state)) = q.get("state").and_then(|s| decode_state(s)) else {
        return err(StatusCode::BAD_REQUEST, "missing or invalid state");
    };
    if !is_valid_return_url(&return_url) {
        return err(StatusCode::BAD_REQUEST, "invalid return_url in state");
    }
    if q.get("error").is_some() {
        return redirect_back(&return_url, &rust_state, "error", "provider_error");
    }

    // Exchange the provider's code/public_token → {secrets, metadata, expires_in, scopes}.
    let exchanged = match provider.as_str() {
        "google" | "strava" | "notion" => exchange_oauth_code(&state, &provider, &cfg, &q).await,
        "plaid" => exchange_plaid_public_token(&state, &cfg, &q).await,
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

async fn exchange_plaid_public_token(
    state: &AppState,
    cfg: &ProviderCfg,
    q: &HashMap<String, String>,
) -> Result<Normalized, String> {
    let public_token = q.get("public_token").ok_or("missing public_token")?;
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
    Ok(Normalized {
        secrets: json!({ "access_token": access }),
        metadata: json!({ "item_id": v.get("item_id") }),
        expires_in: None,
        scopes: None,
    })
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

async fn parse_field(resp: reqwest::Result<reqwest::Response>, field: &str) -> Result<String, String> {
    let v = json_ok(resp).await?;
    v.get(field)
        .and_then(|x| x.as_str())
        .map(String::from)
        .ok_or_else(|| format!("missing {field}"))
}

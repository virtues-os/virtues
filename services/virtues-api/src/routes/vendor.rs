//! Mail and calendar for accounts signed in through a vendor's verified app
//! we do not run yet: Google's and Microsoft's.
//!
//! Google asks for a yearly security assessment before an app may read mail
//! for the public, and Microsoft asks a work account's admin to approve an
//! unverified app. Until Virtues has its own approved apps, a Google or
//! Outlook account can be connected through Truto's instead. Truto keeps the
//! grant; this module is the only thing that talks to Truto.
//!
//! Each provider in [`PROVIDERS`] gets the same routes under its own name:
//!
//!   POST   /v1/<provider>/connect                    — a sign-in link for a new account
//!   GET    /v1/<provider>/connections                — this account's connections
//!   POST   /v1/<provider>/connections/:id/reconnect  — a sign-in link to repair one
//!   DELETE /v1/<provider>/connections/:id            — remove one, at Truto too
//!   ANY    /v1/<provider>/c/:id/*path                — one call to the vendor's API
//!
//! The box sees a route that behaves like the vendor's API: `*path` is the
//! vendor's own path (`gmail/v1/users/me/history`, `v1.0/me/calendars`), the
//! query string and body pass through, and the answer is the vendor's body
//! with the vendor's status, so a 404 for a forgotten Gmail history id and a
//! 410 for an expired sync token mean what they mean to the sync code. The
//! box never learns that Truto exists, which keeps replacing it a change to
//! this file.
//!
//! Three rules hold here:
//!
//! * **A box reaches only its own connections.** The Truto account a call
//!   goes to is looked up by the connection id, the provider AND the caller's
//!   account. An id the caller does not own is a 404.
//! * **Only the calls Virtues makes are forwarded** (each provider's
//!   `allowed`). Truto's proxy will also send mail and set up forwarding,
//!   delegates and filters; a box, or whoever has taken one over, cannot
//!   reach those through here.
//! * **Nothing is logged or stored.** Mail and calendar bodies pass through
//!   in flight. The request log carries this route's pattern, never the path
//!   or query, which hold calendar ids (addresses) and message ids.
//!
//! Truto's token can reach every connected account, so it lives in this
//! service's environment (`TRUTO_API_TOKEN`) and never goes to a box. Without
//! it these routes answer 503. A box then signs in to Google directly, as
//! before, and cannot connect Outlook.

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, RawQuery, State},
    http::{header, HeaderMap, HeaderName, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{any, delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::bearer_auth::BearerAuth;
use crate::AppState;

/// One vendor whose sign-in goes through Truto.
pub struct Provider {
    /// The route segment.
    name: &'static str,
    /// The table that says which Virtues account owns each Truto connection.
    /// One per provider, so the image before a provider existed cannot touch
    /// its rows after a rollback (migration 0018). Always a constant here,
    /// never input, because it is written into the SQL.
    table: &'static str,
    /// How a message to a person names the vendor's sign-in.
    label: &'static str,
    /// Truto's integration. Naming it on the connect page skips Truto's list.
    integration: &'static str,
    /// What a sign-in asks the vendor for.
    scopes: &'static [&'static str],
    /// Every call a box may make, as (method, path). `*` is one segment. Add
    /// a line here when the box learns a new call; nothing else is forwarded.
    allowed: &'static [(&'static str, &'static str)],
}

/// Truto's Google app serves Gmail and Calendar, and `www.googleapis.com`
/// answers both APIs, so one connection covers both. It asks for reading and
/// changing mail, the calendar list and calendar events. Truto's own default
/// also asks for contacts.
const GOOGLE: Provider = Provider {
    name: "google",
    table: "google_connections",
    label: "Google",
    integration: "googlecalendar",
    scopes: &[
        "https://www.googleapis.com/auth/calendar.calendarlist",
        "https://www.googleapis.com/auth/calendar.events",
        "https://www.googleapis.com/auth/gmail.modify",
        "https://www.googleapis.com/auth/userinfo.email",
        "openid",
    ],
    allowed: &[
        // Mail sync.
        ("GET", "gmail/v1/users/me/profile"),
        ("GET", "gmail/v1/users/me/messages"),
        ("GET", "gmail/v1/users/me/messages/*"),
        ("GET", "gmail/v1/users/me/history"),
        // Archive, label and draft.
        ("POST", "gmail/v1/users/me/threads/*/modify"),
        ("GET", "gmail/v1/users/me/labels"),
        ("POST", "gmail/v1/users/me/labels"),
        ("POST", "gmail/v1/users/me/drafts"),
        ("GET", "gmail/v1/users/me/drafts/*"),
        ("PUT", "gmail/v1/users/me/drafts/*"),
        // Calendar sync and its watches.
        ("GET", "calendar/v3/users/me/calendarList"),
        ("GET", "calendar/v3/calendars/*/events"),
        ("POST", "calendar/v3/calendars/*/events/watch"),
        ("POST", "calendar/v3/channels/stop"),
        // Calendar events the assistant adds, changes or removes.
        ("POST", "calendar/v3/calendars/*/events"),
        ("GET", "calendar/v3/calendars/*/events/*"),
        ("PATCH", "calendar/v3/calendars/*/events/*"),
        ("DELETE", "calendar/v3/calendars/*/events/*"),
    ],
};

/// Truto's Outlook mail and Outlook calendar integrations share one Microsoft
/// app and both call `graph.microsoft.com`, so one connection made through
/// the mail integration covers both when the sign-in asks for both. Reading
/// only: the assistant has no Outlook tools yet.
const OUTLOOK: Provider = Provider {
    name: "outlook",
    table: "outlook_connections",
    label: "Outlook",
    integration: "outlook",
    scopes: &[
        "https://graph.microsoft.com/Mail.Read",
        "https://graph.microsoft.com/Calendars.Read",
        "https://graph.microsoft.com/User.Read",
        "offline_access",
        "openid",
        "email",
        "profile",
    ],
    allowed: &[
        // Whose mailbox it is.
        ("GET", "v1.0/me"),
        // Mail sync: one change feed per folder.
        ("GET", "v1.0/me/mailFolders/*/messages/delta"),
        // Calendar sync.
        ("GET", "v1.0/me/calendars"),
        ("GET", "v1.0/me/calendars/*/calendarView"),
    ],
};

pub const PROVIDERS: [&Provider; 2] = [&GOOGLE, &OUTLOOK];

/// The page a person signs in on.
const CONNECT_PAGE: &str = "https://app.truto.one/connect-account";
/// Truto's edge refuses requests that carry no ordinary user agent.
const USER_AGENT: &str = "virtues-api";
/// A draft with attachments is the largest thing a box sends.
const MAX_REQUEST_BYTES: usize = 30 * 1024 * 1024;
/// Graph reads `Prefer` for the body format, the time zone of event times and
/// the page size. It is the only request header besides the body's type that
/// goes on to the vendor.
const PREFER: HeaderName = HeaderName::from_static("prefer");

pub fn router() -> Router<Arc<AppState>> {
    let mut router = Router::new();
    for p in PROVIDERS {
        router = router
            .route(
                &format!("/v1/{}/connect", p.name),
                post(move |s: State<Arc<AppState>>, a: BearerAuth, b: Option<Json<LinkRequest>>| connect(p, s, a, b)),
            )
            .route(&format!("/v1/{}/connections", p.name), get(move |s: State<Arc<AppState>>, a: BearerAuth| list(p, s, a)))
            .route(
                &format!("/v1/{}/connections/:id/reconnect", p.name),
                post(move |s: State<Arc<AppState>>, a: BearerAuth, id: Path<String>, b: Option<Json<LinkRequest>>| {
                    reconnect(p, s, a, id, b)
                }),
            )
            .route(
                &format!("/v1/{}/connections/:id", p.name),
                delete(move |s: State<Arc<AppState>>, a: BearerAuth, id: Path<String>| disconnect(p, s, a, id)),
            )
            .route(
                &format!("/v1/{}/c/:id/*path", p.name),
                any(
                    move |s: State<Arc<AppState>>,
                          a: BearerAuth,
                          m: Method,
                          path: Path<(String, String)>,
                          q: RawQuery,
                          h: HeaderMap,
                          body: Bytes| call(p, s, a, m, path, q, h, body),
                ),
            );
    }
    router.layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
}

fn err(status: StatusCode, code: &str, message: &str) -> Response {
    (status, Json(json!({ "error": { "code": code, "message": message } }))).into_response()
}

fn not_configured(p: &Provider) -> Response {
    err(
        StatusCode::SERVICE_UNAVAILABLE,
        "service_not_configured",
        &format!("{} through Truto is not set up on this server", p.label),
    )
}

/// One request to Truto. `path_and_query` starts with `/`.
async fn truto(
    state: &AppState,
    token: &str,
    method: Method,
    path_and_query: &str,
    prefer: Option<&str>,
    body: Option<(Option<&str>, Bytes)>,
) -> Result<(u16, Option<String>, Bytes), reqwest::Error> {
    let url = format!("{}{}", state.config.truto_api_url.trim_end_matches('/'), path_and_query);
    let mut req = state
        .http_client
        .request(method, url)
        .bearer_auth(token)
        .header(header::USER_AGENT, USER_AGENT)
        .header(header::ACCEPT, "application/json");
    if let Some(v) = prefer {
        req = req.header(PREFER, v);
    }
    if let Some((content_type, bytes)) = body {
        req = req.header(header::CONTENT_TYPE, content_type.unwrap_or("application/json")).body(bytes);
    }
    let resp = req.send().await?;
    let status = resp.status().as_u16();
    let retry_after = resp.headers().get(header::RETRY_AFTER).and_then(|v| v.to_str().ok()).map(String::from);
    Ok((status, retry_after, resp.bytes().await?))
}

async fn truto_json(state: &AppState, token: &str, method: Method, path_and_query: &str, body: Option<&Value>) -> Result<(u16, Value), Response> {
    let bytes = body.map(|b| (None, Bytes::from(b.to_string())));
    match truto(state, token, method, path_and_query, None, bytes).await {
        Ok((status, _, raw)) => Ok((status, serde_json::from_slice(&raw).unwrap_or(Value::Null))),
        Err(e) => {
            tracing::warn!(error = %e.without_url(), "truto request failed");
            Err(err(StatusCode::BAD_GATEWAY, "upstream_error", "could not reach the sign-in service"))
        }
    }
}

#[derive(Deserialize, Default)]
struct LinkRequest {
    /// Where the browser goes when the sign-in is done: the box's own page.
    redirect_uri: Option<String>,
}

/// Ask Truto for a one-time sign-in link.
async fn link(p: &Provider, state: &AppState, token: &str, mut body: Value, redirect_uri: Option<String>) -> Response {
    if let Some(uri) = redirect_uri {
        // The same rule as the OAuth proxy: only a box or a Virtues page.
        if !super::oauth::is_valid_return_url(&uri) {
            return err(StatusCode::BAD_REQUEST, "invalid_redirect", "redirect_uri must be a private address or a virtues.com page");
        }
        body["redirect_uri"] = Value::from(uri);
    }
    body["scopes"] = json!(p.scopes);
    let (status, answer) = match truto_json(state, token, Method::POST, "/link-token", Some(&body)).await {
        Ok(a) => a,
        Err(resp) => return resp,
    };
    match answer.get("link_token").and_then(|v| v.as_str()) {
        Some(link_token) if (200..300).contains(&status) => {
            Json(json!({ "url": format!("{CONNECT_PAGE}/{}?link_token={link_token}", p.integration) })).into_response()
        }
        _ => {
            tracing::warn!(status, provider = p.name, "truto refused a link token");
            err(StatusCode::BAD_GATEWAY, "upstream_error", "the sign-in service did not give a link")
        }
    }
}

/// A sign-in link for an account this Virtues account has not connected.
/// Truto files it under the account's opaque id, with no name or address.
async fn connect(p: &Provider, State(state): State<Arc<AppState>>, BearerAuth(wallet): BearerAuth, body: Option<Json<LinkRequest>>) -> Response {
    let Some(token) = state.config.truto_api_token.clone() else { return not_configured(p) };
    let redirect_uri = body.and_then(|Json(b)| b.redirect_uri);
    link(p, &state, &token, json!({ "tenant_id": wallet.account_id }), redirect_uri).await
}

/// This account's connections to one provider, read from Truto. A connection
/// made since the last call gets its id here, so a box calls this after a
/// sign-in to learn what to store.
async fn list(p: &Provider, State(state): State<Arc<AppState>>, BearerAuth(wallet): BearerAuth) -> Response {
    let Some(token) = state.config.truto_api_token.clone() else { return not_configured(p) };
    let mut found: Vec<Value> = Vec::new();
    let mut cursor: Option<String> = None;
    // A person connects a handful of accounts; five pages is far past that.
    for _ in 0..5 {
        let mut path = format!("/integrated-account?tenant_id={}", encode(&wallet.account_id));
        if let Some(c) = &cursor {
            path.push_str(&format!("&next_cursor={}", encode(c)));
        }
        let (status, page) = match truto_json(&state, &token, Method::GET, &path, None).await {
            Ok(a) => a,
            Err(resp) => return resp,
        };
        if !(200..300).contains(&status) {
            tracing::warn!(status, "truto refused the connection list");
            return err(StatusCode::BAD_GATEWAY, "upstream_error", "the sign-in service did not list connections");
        }
        found.extend(page.get("result").and_then(|v| v.as_array()).cloned().unwrap_or_default());
        cursor = page.get("next_cursor").and_then(|v| v.as_str()).map(String::from);
        if cursor.is_none() {
            break;
        }
    }

    let mut connections = Vec::new();
    let mut live: Vec<String> = Vec::new();
    for account in found.iter().filter(|a| integration_of(a) == Some(p.integration)) {
        // Truto filtered by tenant; check again, since this decides ownership.
        if account.get("tenant_id").and_then(|v| v.as_str()) != Some(wallet.account_id.as_str()) {
            continue;
        }
        let Some(truto_id) = account.get("id").and_then(|v| v.as_str()) else { continue };
        let id = match connection_id(&state.db, p, &wallet.account_id, truto_id).await {
            Ok(Some(id)) => id,
            Ok(None) => continue,
            Err(e) => {
                tracing::error!(error = %e, provider = p.name, "vendor connection lookup failed");
                return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "could not record the connection");
            }
        };
        live.push(truto_id.to_string());
        let context = account.get("context");
        connections.push(json!({
            "id": id,
            // Read from Truto each time and never stored here.
            "email": context.and_then(context_email),
            "status": account.get("status"),
            "scope": context.and_then(|c| c.get("scope")),
        }));
    }
    // A connection removed at Truto has nothing left to route to.
    if let Err(e) = sqlx::query(&format!(
        "DELETE FROM {} WHERE account_id = $1 AND NOT (truto_account_id = ANY($2))",
        p.table
    ))
    .bind(&wallet.account_id)
    .bind(&live)
    .execute(&state.db)
    .await
    {
        tracing::warn!(error = %e, provider = p.name, "vendor connection sweep failed");
    }
    Json(json!({ "connections": connections })).into_response()
}

fn integration_of(account: &Value) -> Option<&str> {
    let integration = account.get("integration")?;
    integration.as_str().or_else(|| integration.get("name").and_then(|v| v.as_str()))
}

/// The account's address from what Truto recorded at sign-in. Google's
/// connections carry `email`; a Microsoft account may only carry its mail
/// address or its sign-in name.
fn context_email(context: &Value) -> Option<&Value> {
    ["email", "mail", "userPrincipalName"]
        .iter()
        .filter_map(|k| context.get(*k))
        .find(|v| v.as_str().is_some_and(|s| !s.is_empty()))
}

/// The id a box uses for a Truto account, made on first sight. `None` when
/// the Truto account is already recorded for another Virtues account.
async fn connection_id(db: &sqlx::PgPool, p: &Provider, account_id: &str, truto_account_id: &str) -> sqlx::Result<Option<String>> {
    sqlx::query(&format!(
        "INSERT INTO {} (id, account_id, truto_account_id) VALUES ($1, $2, $3) ON CONFLICT (truto_account_id) DO NOTHING",
        p.table
    ))
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(account_id)
    .bind(truto_account_id)
    .execute(db)
    .await?;
    sqlx::query_scalar(&format!("SELECT id FROM {} WHERE truto_account_id = $1 AND account_id = $2", p.table))
        .bind(truto_account_id)
        .bind(account_id)
        .fetch_optional(db)
        .await
}

/// The Truto account behind a connection, if the caller owns it and it
/// belongs to this provider.
async fn owned(db: &sqlx::PgPool, p: &Provider, account_id: &str, connection: &str) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar(&format!("SELECT truto_account_id FROM {} WHERE id = $1 AND account_id = $2", p.table))
        .bind(connection)
        .bind(account_id)
        .fetch_optional(db)
        .await
}

async fn owned_or_refuse(p: &Provider, state: &AppState, account_id: &str, connection: &str) -> Result<String, Response> {
    match owned(&state.db, p, account_id, connection).await {
        Ok(Some(id)) => Ok(id),
        Ok(None) => Err(err(
            StatusCode::NOT_FOUND,
            "unknown_connection",
            &format!("no such {} connection on this account", p.label),
        )),
        Err(e) => {
            tracing::error!(error = %e, provider = p.name, "vendor connection lookup failed");
            Err(err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "could not look up the connection"))
        }
    }
}

/// A sign-in link that repairs a connection whose sign-in ended.
async fn reconnect(
    p: &Provider,
    State(state): State<Arc<AppState>>,
    BearerAuth(wallet): BearerAuth,
    Path(connection): Path<String>,
    body: Option<Json<LinkRequest>>,
) -> Response {
    let Some(token) = state.config.truto_api_token.clone() else { return not_configured(p) };
    let truto_id = match owned_or_refuse(p, &state, &wallet.account_id, &connection).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    let redirect_uri = body.and_then(|Json(b)| b.redirect_uri);
    link(p, &state, &token, json!({ "integrated_account_id": truto_id, "persist_previous_context": true }), redirect_uri).await
}

/// Remove a connection. Truto drops the vendor's grant and stops charging for it.
async fn disconnect(p: &Provider, State(state): State<Arc<AppState>>, BearerAuth(wallet): BearerAuth, Path(connection): Path<String>) -> Response {
    let Some(token) = state.config.truto_api_token.clone() else { return not_configured(p) };
    let truto_id = match owned_or_refuse(p, &state, &wallet.account_id, &connection).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    let (status, _) = match truto_json(&state, &token, Method::DELETE, &format!("/integrated-account/{}", encode(&truto_id)), None).await {
        Ok(a) => a,
        Err(resp) => return resp,
    };
    // Already gone at Truto is the outcome asked for.
    if !(200..300).contains(&status) && status != 404 {
        tracing::warn!(status, provider = p.name, "truto refused to delete a connection");
        return err(StatusCode::BAD_GATEWAY, "upstream_error", "the sign-in service did not remove the connection");
    }
    if let Err(e) = sqlx::query(&format!("DELETE FROM {} WHERE id = $1 AND account_id = $2", p.table))
        .bind(&connection)
        .bind(&wallet.account_id)
        .execute(&state.db)
        .await
    {
        tracing::error!(error = %e, provider = p.name, "vendor connection delete failed");
        return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "could not forget the connection");
    }
    StatusCode::NO_CONTENT.into_response()
}

/// One vendor API call for a connection the caller owns.
#[allow(clippy::too_many_arguments)]
async fn call(
    p: &Provider,
    State(state): State<Arc<AppState>>,
    BearerAuth(wallet): BearerAuth,
    method: Method,
    Path((connection, path)): Path<(String, String)>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(token) = state.config.truto_api_token.clone() else { return not_configured(p) };
    let Some(segments) = allowed(p, method.as_str(), &path) else {
        return err(
            StatusCode::FORBIDDEN,
            "call_not_allowed",
            &format!("this {} call is not one Virtues makes", p.label),
        );
    };
    let truto_id = match owned_or_refuse(p, &state, &wallet.account_id, &connection).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    let upstream = upstream_path(&segments, &truto_id, query.as_deref());
    let content_type = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok());
    let prefer = headers.get(PREFER).and_then(|v| v.to_str().ok());
    let sent = (!body.is_empty()).then_some((content_type, body));
    match truto(&state, &token, method, &upstream, prefer, sent).await {
        Ok((status, retry_after, raw)) => {
            let (status, out) = translate(p, status, &raw);
            let mut resp = (status, [(header::CONTENT_TYPE, "application/json")], out).into_response();
            if let Some(v) = retry_after.and_then(|v| v.parse().ok()) {
                resp.headers_mut().insert(header::RETRY_AFTER, v);
            }
            resp
        }
        Err(e) => {
            tracing::warn!(error = %e.without_url(), provider = p.name, "truto request failed");
            err(StatusCode::BAD_GATEWAY, "upstream_error", &format!("could not reach {}", p.label))
        }
    }
}

/// The path's segments when (method, path) is a call Virtues makes. Literal
/// segments match in any letter case: Graph writes `mailfolders` in the links
/// it hands back, and both vendors read their own paths that way or refuse
/// them.
fn allowed<'a>(p: &Provider, method: &str, path: &'a str) -> Option<Vec<&'a str>> {
    let segments: Vec<&str> = path.split('/').collect();
    if segments.iter().any(|s| s.is_empty() || *s == "." || *s == "..") {
        return None;
    }
    p.allowed
        .iter()
        .any(|(m, pattern)| {
            let want: Vec<&str> = pattern.split('/').collect();
            *m == method && want.len() == segments.len() && want.iter().zip(&segments).all(|(w, s)| *w == "*" || w.eq_ignore_ascii_case(s))
        })
        .then_some(segments)
}

/// Truto's Custom API path for a vendor path. Each segment is encoded again
/// (the router decoded it, and a calendar id holds `@`, `#` or `=`), the
/// caller's query follows, and the account is ours to name: any the caller
/// sent is dropped.
fn upstream_path(segments: &[&str], truto_account_id: &str, query: Option<&str>) -> String {
    let path: Vec<String> = segments.iter().map(|s| encode(s)).collect();
    let mut out = format!("/custom/{}?integrated_account_id={}", path.join("/"), encode(truto_account_id));
    for pair in query.unwrap_or("").split('&').filter(|p| !p.is_empty()) {
        // Compare the decoded key, so `integrated%5Faccount%5Fid` cannot name
        // another account either.
        let key = pair.split('=').next().unwrap_or("");
        let decoded = reqwest::Url::parse(&format!("http://x/?{key}"))
            .ok()
            .and_then(|u| u.query_pairs().next().map(|(k, _)| k.into_owned()));
        if decoded.as_deref() != Some("integrated_account_id") {
            out.push('&');
            out.push_str(pair);
        }
    }
    out
}

fn encode(s: &str) -> String {
    reqwest::Url::parse("http://x/").map(|mut u| {
        u.path_segments_mut().expect("http has segments").push(s);
        u.path()[1..].to_string()
    })
    .unwrap_or_default()
    // `push` leaves these as they are, and each means something in a query.
    .replace('&', "%26")
    .replace('=', "%3D")
    .replace('+', "%2B")
}

/// Turn Truto's answer into the one the vendor would have given.
///
/// Truto wraps a success as `{"result": <the vendor's body>}`, and a refusal
/// from the vendor as `{"truto_is_remote_error": true, "raw_response": <the
/// vendor's body>}` under the vendor's status. Anything else is Truto's own
/// trouble: a sign-in that ended is told apart, because the box must ask for
/// a new one instead of trying again, and the rest is a 502.
fn translate(p: &Provider, status: u16, raw: &[u8]) -> (StatusCode, Vec<u8>) {
    let code = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    let parsed: Option<Value> = serde_json::from_slice(raw).ok();
    if code.is_success() {
        return match parsed {
            Some(Value::Object(mut o)) if o.contains_key("result") => {
                (code, serde_json::to_vec(&o.remove("result").unwrap_or(Value::Null)).unwrap_or_default())
            }
            // Google answers some calls (stopping a watch) with no body.
            _ => (code, raw.to_vec()),
        };
    }
    let body = parsed.unwrap_or(Value::Null);
    if body.get("truto_is_remote_error").and_then(|v| v.as_bool()) == Some(true) {
        if let Some(vendor) = body.get("raw_response").filter(|v| v.is_object()) {
            return (code, serde_json::to_vec(vendor).unwrap_or_default());
        }
    }
    let truto_code = body.get("truto_error_code").and_then(|v| v.as_str()).unwrap_or("");
    if truto_code.contains("needs_reauth") {
        let out = json!({
            "code": "reauth_required",
            "error": {
                "code": 401,
                "status": "UNAUTHENTICATED",
                "message": format!("The {} sign-in has ended. Connect the account again.", p.label),
            },
        });
        return (StatusCode::UNAUTHORIZED, out.to_string().into_bytes());
    }
    if code == StatusCode::TOO_MANY_REQUESTS {
        let out = json!({ "error": { "code": 429, "status": "RESOURCE_EXHAUSTED", "message": "Too many requests. Try again shortly." } });
        return (code, out.to_string().into_bytes());
    }
    let out = json!({ "code": "upstream_error", "upstream_status": status });
    (StatusCode::BAD_GATEWAY, out.to_string().into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vendor_routes_sit_beside_the_oauth_providers() {
        let _ = super::super::oauth::router().merge(router());
    }

    #[test]
    fn only_the_google_calls_virtues_makes_are_allowed() {
        for (method, path) in [
            ("GET", "gmail/v1/users/me/history"),
            ("GET", "gmail/v1/users/me/messages/18c0ffee"),
            ("POST", "gmail/v1/users/me/threads/18c0ffee/modify"),
            ("PUT", "gmail/v1/users/me/drafts/r-1"),
            ("GET", "calendar/v3/calendars/nick@example.com/events"),
            ("POST", "calendar/v3/calendars/nick@example.com/events/watch"),
            ("PATCH", "calendar/v3/calendars/primary/events/abc"),
            ("POST", "calendar/v3/channels/stop"),
        ] {
            assert!(allowed(&GOOGLE, method, path).is_some(), "{method} {path}");
        }
        for (method, path) in [
            // Sending, and the settings a mailbox can be taken over through.
            ("POST", "gmail/v1/users/me/messages/send"),
            ("POST", "gmail/v1/users/me/drafts/send"),
            ("POST", "gmail/v1/users/me/settings/forwardingAddresses"),
            ("PUT", "gmail/v1/users/me/settings/autoForwarding"),
            ("POST", "gmail/v1/users/me/settings/delegates"),
            ("POST", "gmail/v1/users/me/settings/filters"),
            ("DELETE", "gmail/v1/users/me/messages/18c0ffee"),
            ("POST", "gmail/v1/users/me/messages/18c0ffee/trash"),
            // Another mailbox, another Google API, and paths that climb.
            ("GET", "gmail/v1/users/david@example.com/messages"),
            ("GET", "drive/v3/files"),
            ("GET", "gmail/v1/users/me/messages/../settings/forwardingAddresses"),
            ("GET", "gmail/v1/users/me//history"),
            ("DELETE", "calendar/v3/calendars/primary"),
            // An Outlook call on the Google route.
            ("GET", "v1.0/me"),
        ] {
            assert!(allowed(&GOOGLE, method, path).is_none(), "{method} {path}");
        }
    }

    #[test]
    fn outlook_is_read_only() {
        for (method, path) in [
            ("GET", "v1.0/me"),
            ("GET", "v1.0/me/mailFolders/inbox/messages/delta"),
            ("GET", "v1.0/me/calendars"),
            ("GET", "v1.0/me/calendars/AAMkAD-1=/calendarView"),
            // The form of Graph's own next and delta links.
            ("GET", "v1.0/me/mailfolders/AAMkAD-2=/messages/delta"),
        ] {
            assert!(allowed(&OUTLOOK, method, path).is_some(), "{method} {path}");
        }
        for (method, path) in [
            // Sending, forwarding rules and changes.
            ("POST", "v1.0/me/sendMail"),
            ("POST", "v1.0/me/messages/AAMk1/send"),
            ("POST", "v1.0/me/messages/AAMk1/forward"),
            ("POST", "v1.0/me/mailFolders/inbox/messageRules"),
            ("PATCH", "v1.0/me/mailboxSettings"),
            ("DELETE", "v1.0/me/messages/AAMk1"),
            ("POST", "v1.0/me/calendars/AAMk1/events"),
            // Someone else's mailbox, the beta API, other Graph data.
            ("GET", "v1.0/users/david@example.com/messages"),
            ("GET", "beta/me/messages"),
            ("GET", "v1.0/me/drive/root/children"),
            ("GET", "v1.0/me/mailFolders/inbox/../../drive"),
            // A Google call on the Outlook route.
            ("GET", "gmail/v1/users/me/history"),
        ] {
            assert!(allowed(&OUTLOOK, method, path).is_none(), "{method} {path}");
        }
    }

    #[test]
    fn every_provider_has_its_own_routes() {
        let names: Vec<&str> = PROVIDERS.iter().map(|p| p.name).collect();
        assert_eq!(names, ["google", "outlook"]);
        assert!(PROVIDERS.iter().all(|p| !p.allowed.is_empty() && !p.scopes.is_empty()));
    }

    #[test]
    fn the_upstream_path_names_our_account_and_encodes_the_calendar() {
        let segments = allowed(&GOOGLE, "GET", "calendar/v3/calendars/en.usa#holiday@group.v.calendar.google.com/events").unwrap();
        let out = upstream_path(&segments, "acct-1", Some("syncToken=abc%3D&integrated_account_id=someone-else&singleEvents=true"));
        assert_eq!(
            out,
            "/custom/calendar/v3/calendars/en.usa%23holiday@group.v.calendar.google.com/events\
             ?integrated_account_id=acct-1&syncToken=abc%3D&singleEvents=true"
        );
        assert_eq!(encode("a b&c=d/e?f"), "a%20b%26c%3Dd%2Fe%3Ff");
        let smuggled = upstream_path(&segments, "acct-1", Some("integrated%5Faccount%5Fid=other&singleEvents=true"));
        assert!(!smuggled.contains("other"), "{smuggled}");

        let segments = allowed(&OUTLOOK, "GET", "v1.0/me/calendars/AAMkAD-1=/calendarView").unwrap();
        let out = upstream_path(&segments, "acct-2", Some("startDateTime=2030-01-01T00:00:00Z&$top=100"));
        assert_eq!(
            out,
            "/custom/v1.0/me/calendars/AAMkAD-1%3D/calendarView?integrated_account_id=acct-2\
             &startDateTime=2030-01-01T00:00:00Z&$top=100"
        );
    }

    #[test]
    fn an_outlook_address_comes_from_whichever_field_truto_kept() {
        assert_eq!(context_email(&json!({ "email": "nick@example.com" })), Some(&json!("nick@example.com")));
        assert_eq!(context_email(&json!({ "email": "", "mail": "nick@example.com" })), Some(&json!("nick@example.com")));
        assert_eq!(context_email(&json!({ "userPrincipalName": "nick@example.com" })), Some(&json!("nick@example.com")));
        assert_eq!(context_email(&json!({ "scope": "Mail.Read" })), None);
    }

    #[test]
    fn a_success_is_unwrapped_to_the_vendors_body() {
        let (status, body) = translate(&GOOGLE, 200, br#"{"result":{"historyId":"42","history":[]}}"#);
        assert_eq!(status, StatusCode::OK);
        assert_eq!(serde_json::from_slice::<Value>(&body).unwrap(), json!({ "historyId": "42", "history": [] }));
        // Stopping a watch: Google sends nothing.
        assert_eq!(translate(&GOOGLE, 200, b""), (StatusCode::OK, Vec::new()));
    }

    #[test]
    fn the_vendors_refusal_keeps_its_status_and_body() {
        let gone = br#"{"message":"Gone","statusCode":410,"truto_is_remote_error":true,"raw_response":{"error":{"code":410,"message":"Sync token is no longer valid"}}}"#;
        let (status, body) = translate(&GOOGLE, 410, gone);
        assert_eq!(status, StatusCode::GONE);
        assert_eq!(serde_json::from_slice::<Value>(&body).unwrap()["error"]["code"], 410);
    }

    #[test]
    fn an_ended_sign_in_is_told_apart_from_other_failures() {
        let (status, body) = translate(&OUTLOOK, 401, br#"{"statusCode":401,"truto_error_code":"integrated_account_needs_reauth"}"#);
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["code"], "reauth_required");
        assert_eq!(body["error"]["message"], "The Outlook sign-in has ended. Connect the account again.");

        let (status, body) = translate(&GOOGLE, 500, br#"{"message":"boom"}"#);
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(serde_json::from_slice::<Value>(&body).unwrap(), json!({ "code": "upstream_error", "upstream_status": 500 }));
        assert_eq!(translate(&GOOGLE, 429, b"{}").0, StatusCode::TOO_MANY_REQUESTS);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_connection_belongs_to_the_account_and_provider_that_made_it(db: sqlx::PgPool) {
        let id = connection_id(&db, &GOOGLE, "acct_a", "truto-1").await.unwrap().expect("recorded");
        assert_eq!(
            connection_id(&db, &GOOGLE, "acct_a", "truto-1").await.unwrap().as_deref(),
            Some(id.as_str()),
            "one id per connection"
        );
        // The same Truto account cannot be claimed by a second Virtues account.
        assert_eq!(connection_id(&db, &GOOGLE, "acct_b", "truto-1").await.unwrap(), None);

        assert_eq!(owned(&db, &GOOGLE, "acct_a", &id).await.unwrap().as_deref(), Some("truto-1"));
        assert_eq!(owned(&db, &GOOGLE, "acct_b", &id).await.unwrap(), None);
        assert_eq!(owned(&db, &OUTLOOK, "acct_a", &id).await.unwrap(), None, "a Google id on the Outlook route");
        assert_eq!(owned(&db, &GOOGLE, "acct_a", "not-an-id").await.unwrap(), None);
    }

    /// The image before Outlook lists an account's Google connections and
    /// deletes every other row of `google_connections` for that account,
    /// whatever its provider. An Outlook connection must survive that.
    #[sqlx::test(migrations = "./migrations")]
    async fn the_image_before_outlook_cannot_delete_an_outlook_connection(db: sqlx::PgPool) {
        let google = connection_id(&db, &GOOGLE, "acct_a", "truto-g").await.unwrap().expect("google");
        let outlook = connection_id(&db, &OUTLOOK, "acct_a", "truto-o").await.unwrap().expect("outlook");
        // The older image's sweep, verbatim.
        sqlx::query("DELETE FROM google_connections WHERE account_id = $1 AND NOT (truto_account_id = ANY($2))")
            .bind("acct_a")
            .bind(vec!["truto-g".to_string()])
            .execute(&db)
            .await
            .unwrap();
        assert_eq!(owned(&db, &GOOGLE, "acct_a", &google).await.unwrap().as_deref(), Some("truto-g"));
        assert_eq!(owned(&db, &OUTLOOK, "acct_a", &outlook).await.unwrap().as_deref(), Some("truto-o"));
        assert_eq!(owned(&db, &GOOGLE, "acct_a", &outlook).await.unwrap(), None, "an Outlook id on the Google route");
    }
}

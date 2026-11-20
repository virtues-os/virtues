//! Gmail and Google Calendar for accounts signed in through a verified
//! Google app we do not run yet.
//!
//! Google asks for a yearly security assessment before an app may read mail
//! for the public. Until Virtues has passed its own, a Google account can be
//! connected through Truto's verified app instead. Truto keeps the Google
//! grant; this module is the only thing that talks to Truto.
//!
//!   POST   /v1/google/connect                    — a sign-in link for a new account
//!   GET    /v1/google/connections                — this account's connections
//!   POST   /v1/google/connections/:id/reconnect  — a sign-in link to repair one
//!   DELETE /v1/google/connections/:id            — remove one, at Truto too
//!   ANY    /v1/google/c/:id/*path                — one Google API call
//!
//! The box sees a route that behaves like Google's: `*path` is Google's own
//! path (`gmail/v1/users/me/history`, `calendar/v3/calendars/{id}/events`),
//! the query string and body pass through, and the answer is Google's body
//! with Google's status, so a 404 for a forgotten history id and a 410 for an
//! expired sync token mean what they mean to the sync code. The box never
//! learns that Truto exists, which keeps replacing it a change to this file.
//!
//! Three rules hold here:
//!
//! * **A box reaches only its own connections.** The Truto account a call
//!   goes to is looked up by the connection id AND the caller's account. An
//!   id the caller does not own is a 404.
//! * **Only the calls Virtues makes are forwarded** (`ALLOWED`). Truto's
//!   proxy will also send mail and set up forwarding, delegates and filters;
//!   a box, or whoever has taken one over, cannot reach those through here.
//! * **Nothing is logged or stored.** Mail and calendar bodies pass through
//!   in flight. The request log carries this route's pattern, never the path
//!   or query, which hold calendar ids (addresses) and message ids.
//!
//! Truto's token can reach every connected account, so it lives in this
//! service's environment (`TRUTO_API_TOKEN`) and never goes to a box. Without
//! it these routes answer 503 and the box signs in to Google directly, as
//! before.

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, RawQuery, State},
    http::{header, HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{any, delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::bearer_auth::BearerAuth;
use crate::AppState;

/// Truto's integration for this. Its Google app serves Gmail and Calendar,
/// and `www.googleapis.com` answers both APIs, so one connection covers both.
const INTEGRATION: &str = "googlecalendar";
/// What a sign-in asks Google for: reading and changing mail, the calendar
/// list, and calendar events. Truto's own default also asks for contacts.
const SCOPES: [&str; 5] = [
    "https://www.googleapis.com/auth/calendar.calendarlist",
    "https://www.googleapis.com/auth/calendar.events",
    "https://www.googleapis.com/auth/gmail.modify",
    "https://www.googleapis.com/auth/userinfo.email",
    "openid",
];
/// The page a person signs in on. Naming the integration skips Truto's list.
const CONNECT_PAGE: &str = "https://app.truto.one/connect-account";
/// Truto's edge refuses requests that carry no ordinary user agent.
const USER_AGENT: &str = "virtues-api";
/// A draft with attachments is the largest thing a box sends.
const MAX_REQUEST_BYTES: usize = 30 * 1024 * 1024;

/// Every Google call a box may make, as (method, path). `*` is one segment.
/// Add a line here when the box learns a new call; nothing else is forwarded.
const ALLOWED: &[(&str, &str)] = &[
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
];

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/google/connect", post(connect))
        .route("/v1/google/connections", get(list))
        .route("/v1/google/connections/:id/reconnect", post(reconnect))
        .route("/v1/google/connections/:id", delete(disconnect))
        .route("/v1/google/c/:id/*path", any(call))
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
}

fn err(status: StatusCode, code: &str, message: &str) -> Response {
    (status, Json(json!({ "error": { "code": code, "message": message } }))).into_response()
}

fn not_configured() -> Response {
    err(StatusCode::SERVICE_UNAVAILABLE, "service_not_configured", "Google through Truto is not set up on this server")
}

/// One request to Truto. `path_and_query` starts with `/`.
async fn truto(
    state: &AppState,
    token: &str,
    method: Method,
    path_and_query: &str,
    body: Option<(Option<&str>, Bytes)>,
) -> Result<(u16, Option<String>, Bytes), reqwest::Error> {
    let url = format!("{}{}", state.config.truto_api_url.trim_end_matches('/'), path_and_query);
    let mut req = state
        .http_client
        .request(method, url)
        .bearer_auth(token)
        .header(header::USER_AGENT, USER_AGENT)
        .header(header::ACCEPT, "application/json");
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
    match truto(state, token, method, path_and_query, bytes).await {
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
async fn link(state: &AppState, token: &str, mut body: Value, redirect_uri: Option<String>) -> Response {
    if let Some(uri) = redirect_uri {
        // The same rule as the OAuth proxy: only a box or a Virtues page.
        if !super::oauth::is_valid_return_url(&uri) {
            return err(StatusCode::BAD_REQUEST, "invalid_redirect", "redirect_uri must be a private address or a virtues.com page");
        }
        body["redirect_uri"] = Value::from(uri);
    }
    body["scopes"] = json!(SCOPES);
    let (status, answer) = match truto_json(state, token, Method::POST, "/link-token", Some(&body)).await {
        Ok(a) => a,
        Err(resp) => return resp,
    };
    match answer.get("link_token").and_then(|v| v.as_str()) {
        Some(link_token) if (200..300).contains(&status) => {
            Json(json!({ "url": format!("{CONNECT_PAGE}/{INTEGRATION}?link_token={link_token}") })).into_response()
        }
        _ => {
            tracing::warn!(status, "truto refused a link token");
            err(StatusCode::BAD_GATEWAY, "upstream_error", "the sign-in service did not give a link")
        }
    }
}

/// A sign-in link for a Google account this Virtues account has not connected.
/// Truto files it under the account's opaque id, with no name or address.
async fn connect(State(state): State<Arc<AppState>>, BearerAuth(wallet): BearerAuth, body: Option<Json<LinkRequest>>) -> Response {
    let Some(token) = state.config.truto_api_token.clone() else { return not_configured() };
    let redirect_uri = body.and_then(|Json(b)| b.redirect_uri);
    link(&state, &token, json!({ "tenant_id": wallet.account_id }), redirect_uri).await
}

/// This account's connections, read from Truto. A connection made since the
/// last call gets its id here, so a box calls this after a sign-in to learn
/// what to store.
async fn list(State(state): State<Arc<AppState>>, BearerAuth(wallet): BearerAuth) -> Response {
    let Some(token) = state.config.truto_api_token.clone() else { return not_configured() };
    let mut found: Vec<Value> = Vec::new();
    let mut cursor: Option<String> = None;
    // A person connects a handful of Google accounts; five pages is far past that.
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
    for account in found.iter().filter(|a| integration_of(a) == Some(INTEGRATION)) {
        // Truto filtered by tenant; check again, since this decides ownership.
        if account.get("tenant_id").and_then(|v| v.as_str()) != Some(wallet.account_id.as_str()) {
            continue;
        }
        let Some(truto_id) = account.get("id").and_then(|v| v.as_str()) else { continue };
        let id = match connection_id(&state.db, &wallet.account_id, truto_id).await {
            Ok(Some(id)) => id,
            Ok(None) => continue,
            Err(e) => {
                tracing::error!(error = %e, "google connection lookup failed");
                return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "could not record the connection");
            }
        };
        live.push(truto_id.to_string());
        let context = account.get("context");
        connections.push(json!({
            "id": id,
            // Read from Truto each time and never stored here.
            "email": context.and_then(|c| c.get("email")),
            "status": account.get("status"),
            "scope": context.and_then(|c| c.get("scope")),
        }));
    }
    // A connection removed at Truto has nothing left to route to.
    if let Err(e) = sqlx::query("DELETE FROM google_connections WHERE account_id = $1 AND NOT (truto_account_id = ANY($2))")
        .bind(&wallet.account_id)
        .bind(&live)
        .execute(&state.db)
        .await
    {
        tracing::warn!(error = %e, "google connection sweep failed");
    }
    Json(json!({ "connections": connections })).into_response()
}

fn integration_of(account: &Value) -> Option<&str> {
    let integration = account.get("integration")?;
    integration.as_str().or_else(|| integration.get("name").and_then(|v| v.as_str()))
}

/// The id a box uses for a Truto account, made on first sight. `None` when
/// the Truto account is already recorded for another Virtues account.
async fn connection_id(db: &sqlx::PgPool, account_id: &str, truto_account_id: &str) -> sqlx::Result<Option<String>> {
    sqlx::query(
        "INSERT INTO google_connections (id, account_id, truto_account_id) VALUES ($1, $2, $3)
         ON CONFLICT (truto_account_id) DO NOTHING",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(account_id)
    .bind(truto_account_id)
    .execute(db)
    .await?;
    sqlx::query_scalar("SELECT id FROM google_connections WHERE truto_account_id = $1 AND account_id = $2")
        .bind(truto_account_id)
        .bind(account_id)
        .fetch_optional(db)
        .await
}

/// The Truto account behind a connection, if the caller owns it.
async fn owned(db: &sqlx::PgPool, account_id: &str, connection: &str) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("SELECT truto_account_id FROM google_connections WHERE id = $1 AND account_id = $2")
        .bind(connection)
        .bind(account_id)
        .fetch_optional(db)
        .await
}

async fn owned_or_refuse(state: &AppState, account_id: &str, connection: &str) -> Result<String, Response> {
    match owned(&state.db, account_id, connection).await {
        Ok(Some(id)) => Ok(id),
        Ok(None) => Err(err(StatusCode::NOT_FOUND, "unknown_connection", "no such Google connection on this account")),
        Err(e) => {
            tracing::error!(error = %e, "google connection lookup failed");
            Err(err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "could not look up the connection"))
        }
    }
}

/// A sign-in link that repairs a connection whose Google sign-in ended.
async fn reconnect(
    State(state): State<Arc<AppState>>,
    BearerAuth(wallet): BearerAuth,
    Path(connection): Path<String>,
    body: Option<Json<LinkRequest>>,
) -> Response {
    let Some(token) = state.config.truto_api_token.clone() else { return not_configured() };
    let truto_id = match owned_or_refuse(&state, &wallet.account_id, &connection).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    let redirect_uri = body.and_then(|Json(b)| b.redirect_uri);
    link(&state, &token, json!({ "integrated_account_id": truto_id, "persist_previous_context": true }), redirect_uri).await
}

/// Remove a connection. Truto drops the Google grant and stops charging for it.
async fn disconnect(State(state): State<Arc<AppState>>, BearerAuth(wallet): BearerAuth, Path(connection): Path<String>) -> Response {
    let Some(token) = state.config.truto_api_token.clone() else { return not_configured() };
    let truto_id = match owned_or_refuse(&state, &wallet.account_id, &connection).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    let (status, _) = match truto_json(&state, &token, Method::DELETE, &format!("/integrated-account/{}", encode(&truto_id)), None).await {
        Ok(a) => a,
        Err(resp) => return resp,
    };
    // Already gone at Truto is the outcome asked for.
    if !(200..300).contains(&status) && status != 404 {
        tracing::warn!(status, "truto refused to delete a connection");
        return err(StatusCode::BAD_GATEWAY, "upstream_error", "the sign-in service did not remove the connection");
    }
    if let Err(e) = sqlx::query("DELETE FROM google_connections WHERE id = $1 AND account_id = $2")
        .bind(&connection)
        .bind(&wallet.account_id)
        .execute(&state.db)
        .await
    {
        tracing::error!(error = %e, "google connection delete failed");
        return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "could not forget the connection");
    }
    StatusCode::NO_CONTENT.into_response()
}

/// One Google API call for a connection the caller owns.
async fn call(
    State(state): State<Arc<AppState>>,
    BearerAuth(wallet): BearerAuth,
    method: Method,
    Path((connection, path)): Path<(String, String)>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(token) = state.config.truto_api_token.clone() else { return not_configured() };
    let Some(segments) = allowed(method.as_str(), &path) else {
        return err(StatusCode::FORBIDDEN, "call_not_allowed", "this Google call is not one Virtues makes");
    };
    let truto_id = match owned_or_refuse(&state, &wallet.account_id, &connection).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    let upstream = upstream_path(&segments, &truto_id, query.as_deref());
    let content_type = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok());
    let sent = (!body.is_empty()).then_some((content_type, body));
    match truto(&state, &token, method, &upstream, sent).await {
        Ok((status, retry_after, raw)) => {
            let (status, out) = translate(status, &raw);
            let mut resp = (status, [(header::CONTENT_TYPE, "application/json")], out).into_response();
            if let Some(v) = retry_after.and_then(|v| v.parse().ok()) {
                resp.headers_mut().insert(header::RETRY_AFTER, v);
            }
            resp
        }
        Err(e) => {
            tracing::warn!(error = %e.without_url(), "truto request failed");
            err(StatusCode::BAD_GATEWAY, "upstream_error", "could not reach Google")
        }
    }
}

/// The path's segments when (method, path) is a call Virtues makes.
fn allowed<'a>(method: &str, path: &'a str) -> Option<Vec<&'a str>> {
    let segments: Vec<&str> = path.split('/').collect();
    if segments.iter().any(|s| s.is_empty() || *s == "." || *s == "..") {
        return None;
    }
    ALLOWED
        .iter()
        .any(|(m, pattern)| {
            let want: Vec<&str> = pattern.split('/').collect();
            *m == method && want.len() == segments.len() && want.iter().zip(&segments).all(|(w, s)| *w == "*" || w == s)
        })
        .then_some(segments)
}

/// Truto's Custom API path for a Google path. Each segment is encoded again
/// (the router decoded it, and a calendar id holds `@` and `#`), the caller's
/// query follows, and the account is ours to name: any the caller sent is dropped.
fn upstream_path(segments: &[&str], truto_account_id: &str, query: Option<&str>) -> String {
    let path: Vec<String> = segments.iter().map(|s| encode(s)).collect();
    let mut out = format!("/custom/{}?integrated_account_id={}", path.join("/"), encode(truto_account_id));
    for pair in query.unwrap_or("").split('&').filter(|p| !p.is_empty()) {
        if pair.split('=').next() != Some("integrated_account_id") {
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

/// Turn Truto's answer into the one Google would have given.
///
/// Truto wraps a success as `{"result": <Google's body>}`, and a refusal from
/// Google as `{"truto_is_remote_error": true, "raw_response": <Google's
/// body>}` under Google's status. Anything else is Truto's own trouble: a
/// sign-in that ended is told apart, because the box must ask for a new one
/// instead of trying again, and the rest is a 502.
fn translate(status: u16, raw: &[u8]) -> (StatusCode, Vec<u8>) {
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
        if let Some(google) = body.get("raw_response").filter(|v| v.is_object()) {
            return (code, serde_json::to_vec(google).unwrap_or_default());
        }
    }
    let truto_code = body.get("truto_error_code").and_then(|v| v.as_str()).unwrap_or("");
    if truto_code.contains("needs_reauth") {
        let out = json!({
            "code": "reauth_required",
            "error": { "code": 401, "status": "UNAUTHENTICATED", "message": "The Google sign-in has ended. Connect the account again." },
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
    fn the_google_routes_sit_beside_the_oauth_providers() {
        let _ = super::super::oauth::router().merge(router());
    }

    #[test]
    fn only_the_calls_virtues_makes_are_allowed() {
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
            assert!(allowed(method, path).is_some(), "{method} {path}");
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
        ] {
            assert!(allowed(method, path).is_none(), "{method} {path}");
        }
    }

    #[test]
    fn the_upstream_path_names_our_account_and_encodes_the_calendar() {
        let segments = allowed("GET", "calendar/v3/calendars/en.usa#holiday@group.v.calendar.google.com/events").unwrap();
        let out = upstream_path(&segments, "acct-1", Some("syncToken=abc%3D&integrated_account_id=someone-else&singleEvents=true"));
        assert_eq!(
            out,
            "/custom/calendar/v3/calendars/en.usa%23holiday@group.v.calendar.google.com/events\
             ?integrated_account_id=acct-1&syncToken=abc%3D&singleEvents=true"
        );
        assert_eq!(encode("a b&c=d/e?f"), "a%20b%26c%3Dd%2Fe%3Ff");
    }

    #[test]
    fn a_success_is_unwrapped_to_googles_body() {
        let (status, body) = translate(200, br#"{"result":{"historyId":"42","history":[]}}"#);
        assert_eq!(status, StatusCode::OK);
        assert_eq!(serde_json::from_slice::<Value>(&body).unwrap(), json!({ "historyId": "42", "history": [] }));
        // Stopping a watch: Google sends nothing.
        assert_eq!(translate(200, b""), (StatusCode::OK, Vec::new()));
    }

    #[test]
    fn googles_refusal_keeps_its_status_and_body() {
        let gone = br#"{"message":"Gone","statusCode":410,"truto_is_remote_error":true,"raw_response":{"error":{"code":410,"message":"Sync token is no longer valid"}}}"#;
        let (status, body) = translate(410, gone);
        assert_eq!(status, StatusCode::GONE);
        assert_eq!(serde_json::from_slice::<Value>(&body).unwrap()["error"]["code"], 410);
    }

    #[test]
    fn an_ended_sign_in_is_told_apart_from_other_failures() {
        let (status, body) = translate(401, br#"{"statusCode":401,"truto_error_code":"integrated_account_needs_reauth"}"#);
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(serde_json::from_slice::<Value>(&body).unwrap()["code"], "reauth_required");

        let (status, body) = translate(500, br#"{"message":"boom"}"#);
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(serde_json::from_slice::<Value>(&body).unwrap(), json!({ "code": "upstream_error", "upstream_status": 500 }));
        assert_eq!(translate(429, b"{}").0, StatusCode::TOO_MANY_REQUESTS);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_connection_belongs_to_the_account_that_made_it(db: sqlx::PgPool) {
        let id = connection_id(&db, "acct_a", "truto-1").await.unwrap().expect("recorded");
        assert_eq!(connection_id(&db, "acct_a", "truto-1").await.unwrap().as_deref(), Some(id.as_str()), "one id per connection");
        // The same Truto account cannot be claimed by a second Virtues account.
        assert_eq!(connection_id(&db, "acct_b", "truto-1").await.unwrap(), None);

        assert_eq!(owned(&db, "acct_a", &id).await.unwrap().as_deref(), Some("truto-1"));
        assert_eq!(owned(&db, "acct_b", &id).await.unwrap(), None);
        assert_eq!(owned(&db, "acct_a", "not-an-id").await.unwrap(), None);
    }
}

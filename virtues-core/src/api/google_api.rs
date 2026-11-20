//! Google sign-ins that go through virtues-api.
//!
//! A Google account is connected in one of two ways. The older way keeps
//! Google's own tokens in the credential, and the box calls Google directly.
//! The other way is used when virtues-api offers it: the sign-in is made
//! through a verified Google app that virtues-api reaches (its
//! `routes/google.rs` says which and why), the credential holds no Google
//! token at all, and every Google call goes to a virtues-api route that
//! answers as Google would.
//!
//! This module is the box's side of that. A credential made this way carries
//! `api_connection_id` in its metadata; [`Route`] turns either kind of
//! credential into where to send a Gmail or Calendar call and what to send as
//! the bearer, so the sync applets and the assistant's Google tools make the
//! same requests either way.

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use sqlx::PgPool;

/// The metadata key that marks a credential as one virtues-api serves.
pub const CONNECTION_KEY: &str = "api_connection_id";

const GMAIL_DIRECT: &str = "https://gmail.googleapis.com";
const CALENDAR_DIRECT: &str = "https://www.googleapis.com";

/// Where one Google sign-in's calls go.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    /// Sent as `Authorization: Bearer`: Google's access token, or this box's
    /// Virtues key when the call goes to virtues-api.
    pub token: String,
    /// Gmail's API for the signed-in user, up to and including `users/me`.
    pub gmail: String,
    /// Calendar's API root, up to and including `calendar/v3`.
    pub calendar: String,
}

impl Route {
    /// The route for a credential that holds Google's own tokens.
    pub fn direct(access_token: &str) -> Route {
        Route::at(access_token, GMAIL_DIRECT, CALENDAR_DIRECT)
    }

    /// The route for a connection virtues-api serves.
    fn through_api(box_key: &str, api_url: &str, connection_id: &str) -> Route {
        let base = api_base(api_url, connection_id);
        Route::at(box_key, &base, &base)
    }

    fn at(token: &str, gmail_host: &str, calendar_host: &str) -> Route {
        Route {
            token: token.to_string(),
            gmail: format!("{gmail_host}/gmail/v1/users/me"),
            calendar: format!("{calendar_host}/calendar/v3"),
        }
    }
}

/// The virtues-api address that stands in for Google's hosts for one connection.
fn api_base(api_url: &str, connection_id: &str) -> String {
    format!("{}/v1/google/c/{}", api_url.trim_end_matches('/'), urlencoding::encode(connection_id))
}

/// The connection id of a credential virtues-api serves, from its metadata.
pub fn connection_id(metadata: &Value) -> Option<&str> {
    metadata.get(CONNECTION_KEY).and_then(|v| v.as_str()).filter(|s| !s.is_empty())
}

/// This box's Virtues key: the dev override, else the linked key.
async fn box_key(pool: &PgPool) -> Result<String> {
    crate::virtues_api::box_key(pool)
        .await?
        .ok_or_else(|| anyhow!("this server is not linked to a Virtues account"))
}

/// Where a credential's Google calls go, from its metadata and its secrets.
pub async fn route(pool: &PgPool, metadata: &Value, secrets: &Value) -> Result<Route> {
    if let Some(id) = connection_id(metadata) {
        return Ok(Route::through_api(&box_key(pool).await?, &crate::virtues_api::api_url(), id));
    }
    secrets
        .get("access_token")
        .and_then(|v| v.as_str())
        .map(Route::direct)
        .ok_or_else(|| anyhow!("the Google sign-in has no access token"))
}

/// The secrets a sync applet is given for a credential virtues-api serves:
/// the bearer to send and the address that stands in for Google's hosts.
/// `None` for a credential that holds Google's own tokens, which an applet
/// is given as they are.
pub async fn applet_secrets(pool: &PgPool, metadata: &Value) -> Result<Option<Value>> {
    let Some(id) = connection_id(metadata) else { return Ok(None) };
    Ok(Some(json!({
        "access_token": box_key(pool).await?,
        "api_base": api_base(&crate::virtues_api::api_url(), id),
    })))
}

/// One connection as virtues-api lists it.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Connection {
    pub id: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
}

async fn send(pool: &PgPool, method: reqwest::Method, path: &str, body: Option<&Value>) -> Result<(u16, Value)> {
    let key = box_key(pool).await?;
    let url = format!("{}{}", crate::virtues_api::api_url().trim_end_matches('/'), path);
    let mut req = crate::http_client::virtues_api_client().request(method, url).bearer_auth(key);
    if let Some(b) = body {
        req = req.json(b);
    }
    let resp = req.send().await.map_err(|e| anyhow!("virtues-api request failed: {}", e.without_url()))?;
    let status = resp.status().as_u16();
    Ok((status, resp.json().await.unwrap_or(Value::Null)))
}

/// A sign-in link from virtues-api, for a new account or (with `reconnecting`,
/// a connection id) to repair one. `None` when virtues-api does not offer
/// this: it has no such route, the route is switched off, or this box has no
/// key or no plan for it. The caller then starts the direct sign-in.
pub async fn sign_in_link(pool: &PgPool, redirect_uri: &str, reconnecting: Option<&str>) -> Option<String> {
    let path = match reconnecting {
        Some(id) => format!("/v1/google/connections/{}/reconnect", urlencoding::encode(id)),
        None => "/v1/google/connect".to_string(),
    };
    match send(pool, reqwest::Method::POST, &path, Some(&json!({ "redirect_uri": redirect_uri }))).await {
        Ok((200, body)) => body.get("url").and_then(|v| v.as_str()).map(String::from),
        Ok((status, _)) => {
            tracing::info!(status, "virtues-api offers no Google sign-in; using the direct one");
            None
        }
        Err(e) => {
            tracing::info!(error = %e, "could not ask virtues-api for a Google sign-in; using the direct one");
            None
        }
    }
}

/// This account's connections at virtues-api.
pub async fn connections(pool: &PgPool) -> Result<Vec<Connection>> {
    let (status, body) = send(pool, reqwest::Method::GET, "/v1/google/connections", None).await?;
    if status != 200 {
        return Err(anyhow!("virtues-api answered {status} for the Google connections"));
    }
    Ok(serde_json::from_value(body.get("connections").cloned().unwrap_or_else(|| json!([])))?)
}

/// After a sync applet fails: if the credential is one virtues-api serves and
/// its Google sign-in has ended there, mark it `reauth_required`. The box
/// holds no token for such a credential, so nothing else on it can notice.
/// `credentials` is what the applet was given (`id`, `source_id`, `metadata`).
pub async fn mark_if_ended(pool: &PgPool, credentials: &Value) {
    if credentials.get("source_id").and_then(|v| v.as_str()) != Some("google") {
        return;
    }
    let (Some(credential_id), Some(connection)) = (
        credentials.get("id").and_then(|v| v.as_str()),
        credentials.get("metadata").and_then(connection_id),
    ) else {
        return;
    };
    let listed = match connections(pool).await {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!(credential_id, error = %e, "could not ask virtues-api whether the Google sign-in ended");
            return;
        }
    };
    if !sign_in_ended(&listed, connection) {
        return;
    }
    match virtues_helpers::auth::mark_credential_status(
        pool,
        credential_id,
        virtues_helpers::auth::CredentialStatus::ReauthRequired,
        Some("the Google sign-in ended"),
    )
    .await
    {
        Ok(()) => tracing::info!(credential_id, "the Google sign-in ended; marked reauth_required"),
        Err(e) => tracing::warn!(credential_id, error = %e, "could not mark the Google sign-in as ended"),
    }
}

/// Whether a connection's sign-in has ended: it is listed and not active, or
/// it is no longer listed at all.
fn sign_in_ended(listed: &[Connection], connection: &str) -> bool {
    match listed.iter().find(|c| c.id == connection) {
        Some(c) => c.status.as_deref() != Some("active"),
        None => true,
    }
}

/// Remove a connection at virtues-api, which removes the Google grant where
/// it is kept. A connection already gone is the outcome asked for.
pub async fn disconnect(pool: &PgPool, connection_id: &str) -> Result<()> {
    let path = format!("/v1/google/connections/{}", urlencoding::encode(connection_id));
    match send(pool, reqwest::Method::DELETE, &path, None).await? {
        (200..=299, _) | (404, _) => Ok(()),
        (status, _) => Err(anyhow!("virtues-api answered {status} when removing the Google connection")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_direct_route_names_googles_own_hosts() {
        let r = Route::direct("ya29.token");
        assert_eq!(r.token, "ya29.token");
        assert_eq!(r.gmail, "https://gmail.googleapis.com/gmail/v1/users/me");
        assert_eq!(r.calendar, "https://www.googleapis.com/calendar/v3");
    }

    #[test]
    fn an_api_route_puts_both_apis_under_the_connection() {
        let r = Route::through_api("box-key", "https://api.example.com/", "c-1");
        assert_eq!(r.token, "box-key");
        assert_eq!(r.gmail, "https://api.example.com/v1/google/c/c-1/gmail/v1/users/me");
        assert_eq!(r.calendar, "https://api.example.com/v1/google/c/c-1/calendar/v3");
    }

    #[test]
    fn a_sign_in_has_ended_when_it_is_not_active_or_not_listed() {
        let c = |id: &str, status: &str| Connection { id: id.into(), email: None, status: Some(status.into()), scope: None };
        let listed = [c("c-1", "active"), c("c-2", "needs_reauth")];
        assert!(!sign_in_ended(&listed, "c-1"));
        assert!(sign_in_ended(&listed, "c-2"));
        assert!(sign_in_ended(&listed, "c-3"));
    }

    #[test]
    fn only_a_named_connection_marks_a_credential() {
        assert_eq!(connection_id(&json!({ "api_connection_id": "c-1" })), Some("c-1"));
        assert_eq!(connection_id(&json!({ "api_connection_id": "" })), None);
        assert_eq!(connection_id(&json!({ "email": "nick@example.com" })), None);
    }
}

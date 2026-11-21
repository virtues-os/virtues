//! Shared helpers for the action subprocess binaries.
//!
//! Every action is a `[[bin]]` in this crate; this lib holds the boilerplate
//! they all repeated: tracing init, an HTTP client, `AppletInput`
//! credential/config accessors, and the send→error_for_status→json fetch dance.

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use std::time::Duration;
use virtues_helpers::AppletInput;

/// Initialize stderr tracing with an `info` default (overridable via `RUST_LOG`),
/// and load the box environment — the same files, in the same order, as the
/// server (`main.rs`).
///
/// Actions used to load NOTHING. On a box that was invisible, because systemd
/// hands them an `EnvironmentFile`; in dev they simply ran with different
/// configuration than the server, and nobody noticed because nothing important
/// depended on it.
///
/// Something does now. The embedding model's width and prompt formats live in the
/// environment (they are facts about a model, not about Virtues), so an action
/// that cannot see them computes a different vector geometry than the server —
/// and the index guard, correctly, refuses to let it write. The config drift was
/// always there; embeddings are just the first thing sharp enough to cut on it.
pub fn init_tracing() {
    let _ = dotenv::from_path("/var/lib/virtues/virtues.env");
    if dotenv::dotenv().is_err() {
        let _ = dotenv::from_path("../.env");
    }

    // Auto, like every other binary: JSON into the pipe the runner reads,
    // text when a person runs an applet by hand at a terminal.
    //
    // This was briefly forced to Text, to stop the runner wrapping our JSON
    // inside more JSON. That fixed the reading and broke the filtering — an
    // applet's `kind` became prose inside the runner's message, and applet
    // events turned into the one class of event the box could not query. The
    // runner now UNWRAPS a structured line instead of quoting it (see
    // `emit_subprocess_stderr`), so structure here is what it wants.
    virtues::observe::init("info");
}

/// A reqwest client with a generous default timeout for action HTTP calls
/// (the bare `Client::new()` they used had no timeout at all).
pub fn http_client() -> reqwest::Client {
    ensure_crypto_provider();
    reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .expect("failed to build reqwest client")
}

/// `http_client` that returns a redirect instead of following it. A sync that
/// replays a logged-in web session needs this: when the session ends, the site
/// redirects to its login page, and a client that follows the redirect gets the
/// login page's HTML with a 200 instead of a status the applet can recognize.
pub fn http_client_no_redirects() -> reqwest::Client {
    ensure_crypto_provider();
    reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("failed to build reqwest client")
}

/// Install the ring crypto provider once (reqwest here is rustls-tls-no-provider,
/// so direct external HTTPS panics with "No provider set" without this).
fn ensure_crypto_provider() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Read `credentials.secrets.<key>` as a string, erroring if absent.
pub fn secret<'a>(input: &'a AppletInput, key: &str) -> Result<&'a str> {
    input
        .credentials
        .as_ref()
        .and_then(|c| c.get("secrets"))
        .and_then(|s| s.get(key))
        .and_then(|v| v.as_str())
        .with_context(|| format!("credentials missing secrets.{key}"))
}

/// Where a Google applet sends its calls.
pub struct GoogleHosts {
    /// Gmail's API for the signed-in user, up to and including `users/me`.
    pub gmail: String,
    /// Calendar's API root, up to and including `calendar/v3`.
    pub calendar: String,
}

/// Google's own hosts, or the address the runner gave in `secrets.api_base`
/// for a sign-in made through virtues-api. That address answers as Google
/// does, and `secrets.access_token` is then this server's Virtues key, so
/// an applet makes the same requests either way.
pub fn google_hosts(input: &AppletInput) -> GoogleHosts {
    let base = input
        .credentials
        .as_ref()
        .and_then(|c| c.get("secrets"))
        .and_then(|s| s.get("api_base"))
        .and_then(|v| v.as_str())
        .filter(|b| !b.is_empty())
        .map(|b| b.trim_end_matches('/').to_string());
    match base {
        Some(b) => GoogleHosts { gmail: format!("{b}/gmail/v1/users/me"), calendar: format!("{b}/calendar/v3") },
        None => GoogleHosts {
            gmail: "https://gmail.googleapis.com/gmail/v1/users/me".to_string(),
            calendar: "https://www.googleapis.com/calendar/v3".to_string(),
        },
    }
}

/// The Microsoft Graph root an Outlook applet calls: `{secrets.api_base}/v1.0`.
///
/// Outlook connects only through virtues-api, which answers at `api_base`
/// exactly as Graph does, so there is no Microsoft host to fall back to. A
/// credential without `api_base` is an error.
pub fn outlook_graph(input: &AppletInput) -> Result<String> {
    let base = secret(input, "api_base")?.trim().trim_end_matches('/');
    if base.is_empty() {
        anyhow::bail!("credentials have an empty secrets.api_base; reconnect Outlook");
    }
    Ok(format!("{base}/v1.0"))
}

/// The request a Graph paging link (`@odata.nextLink`, `@odata.deltaLink`)
/// names, rebuilt under `graph` (from [`outlook_graph`]).
///
/// Graph returns absolute `https://graph.microsoft.com/v1.0/...` links, often
/// with keys in OData's `segment('X')` form, such as `mailFolders('inbox')`.
/// virtues-api forwards only plain `segment/X` paths, so each key is moved
/// into its own path segment. The query string, which carries Graph's paging
/// tokens, is kept byte for byte. Only the path after `/v1.0/` is used, so the
/// token is never sent to whatever host a link names.
pub fn graph_link(graph: &str, link: &str) -> Result<String> {
    let (path, query) = match link.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (link, None),
    };
    let at = path.find("/v1.0/").with_context(|| format!("not a Graph v1.0 link: {link}"))?;
    let rest = &path[at + "/v1.0/".len()..];
    let segments: Vec<String> = rest.split('/').filter(|s| !s.is_empty()).map(key_segment).collect();
    let mut out = format!("{}/{}", graph.trim_end_matches('/'), segments.join("/"));
    if let Some(q) = query {
        out.push('?');
        out.push_str(q);
    }
    Ok(out)
}

/// `name('key')`, `name(%27key%27)` or `name(key)` as `name/key`; any other
/// segment unchanged.
fn key_segment(seg: &str) -> String {
    let Some(open) = seg.find('(') else { return seg.to_string() };
    if open == 0 || !seg.ends_with(')') {
        return seg.to_string();
    }
    let key = &seg[open + 1..seg.len() - 1];
    let key = key
        .strip_prefix('\'')
        .and_then(|k| k.strip_suffix('\''))
        .or_else(|| key.strip_prefix("%27").and_then(|k| k.strip_suffix("%27")))
        .unwrap_or(key);
    format!("{}/{key}", &seg[..open])
}

/// A non-2xx answer from Graph, or from virtues-api standing in for it.
#[derive(Debug)]
pub struct GraphError {
    pub status: u16,
    /// Graph's `error.code`, or virtues-api's top-level `code`.
    pub code: Option<String>,
    pub message: String,
}

impl GraphError {
    /// Parse an error body. Graph sends `{"error":{"code","message"}}`;
    /// virtues-api sends its own refusals as `{"code","message"}`.
    pub fn from_body(status: u16, body: &str) -> GraphError {
        let json: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
        let field = |k: &str| {
            json.get("error")
                .and_then(|e| e.get(k))
                .or_else(|| json.get(k))
                .and_then(|v| v.as_str())
                .map(String::from)
        };
        let message = field("message").unwrap_or_else(|| body.chars().take(300).collect());
        GraphError { status, code: field("code"), message }
    }

    /// The Outlook sign-in ended. Retrying cannot fix it; the owner has to
    /// reconnect.
    pub fn is_reauth(&self) -> bool {
        self.status == 401 && self.code.as_deref() == Some("reauth_required")
    }
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.code {
            Some(code) => write!(f, "Graph answered {} {code}: {}", self.status, self.message),
            None => write!(f, "Graph answered {}: {}", self.status, self.message),
        }
    }
}

impl std::error::Error for GraphError {}

/// The [`GraphError`] in an error's chain, if one is there.
pub fn graph_error(err: &anyhow::Error) -> Option<&GraphError> {
    err.chain().find_map(|c| c.downcast_ref::<GraphError>())
}

/// How many times one Graph request waits out a "too many requests" answer.
const GRAPH_RATE_LIMIT_TRIES: u32 = 4;

/// GET a Graph URL and return the JSON body. `prefer` is sent as the `Prefer`
/// header. A 429 or 503 is waited out a few times, as `Retry-After` asks. Any
/// other non-2xx is returned as a [`GraphError`], so a caller can act on the
/// status or code.
pub async fn graph_get(
    client: &reqwest::Client,
    token: &str,
    url: &str,
    prefer: Option<&str>,
) -> Result<serde_json::Value> {
    let mut tries = 0;
    loop {
        let mut req = client.get(url).bearer_auth(token);
        if let Some(p) = prefer {
            req = req.header("Prefer", p);
        }
        let resp = req.send().await.context("Graph request failed")?;
        let status = resp.status();
        if status.is_success() {
            return resp.json().await.context("Graph answer was not JSON");
        }
        tries += 1;
        let busy = status == reqwest::StatusCode::TOO_MANY_REQUESTS || status == reqwest::StatusCode::SERVICE_UNAVAILABLE;
        if busy && tries < GRAPH_RATE_LIMIT_TRIES {
            tokio::time::sleep(rate_limit_wait(resp.headers().get(reqwest::header::RETRY_AFTER))).await;
            continue;
        }
        // The body only explains the failure; one that cannot be read leaves
        // the status to say what happened.
        let body = resp.text().await.unwrap_or_default();
        let err = GraphError::from_body(status.as_u16(), &body);
        if err.is_reauth() {
            return Err(anyhow::Error::new(err).context("the Outlook sign-in ended; reconnect Outlook on the Sources page"));
        }
        return Err(err.into());
    }
}

/// The longest single wait after "too many requests", whatever `Retry-After`
/// says.
pub const MAX_RATE_LIMIT_WAIT_SECS: u64 = 15;

/// How long to wait after "too many requests": what `Retry-After` asks for in
/// seconds, within [`MAX_RATE_LIMIT_WAIT_SECS`], or two seconds when it says
/// nothing usable.
pub fn rate_limit_wait(retry_after: Option<&reqwest::header::HeaderValue>) -> Duration {
    let secs = retry_after
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(2)
        .clamp(1, MAX_RATE_LIMIT_WAIT_SECS);
    Duration::from_secs(secs)
}

/// The credential's browser session (`secrets.jar`), if a sign-in through the
/// app stored one. Each entry has at least `name` and `value`.
fn jar(input: &AppletInput) -> Option<&Vec<serde_json::Value>> {
    input.credentials.as_ref()?.get("secrets")?.get("jar")?.as_array()
}

/// One session cookie's value: from `secrets.jar`, else from the named field
/// a sign-in older than the jar stored (`secrets.<name>`).
pub fn cookie<'a>(input: &'a AppletInput, name: &str) -> Result<&'a str> {
    let from_jar = jar(input).and_then(|j| {
        j.iter()
            .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(name))
            .and_then(|c| c.get("value"))
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
    });
    match from_jar {
        Some(v) => Ok(v),
        None => secret(input, name),
    }
}

/// The whole session as a `Cookie:` header: built from `secrets.jar`, else
/// the header string a sign-in older than the jar stored in `secrets.<field>`.
pub fn cookie_header(input: &AppletInput, field: &str) -> Result<String> {
    if let Some(j) = jar(input).filter(|j| !j.is_empty()) {
        let pairs: Vec<String> = j
            .iter()
            .filter_map(|c| Some(format!("{}={}", c.get("name")?.as_str()?, c.get("value")?.as_str()?)))
            .collect();
        return Ok(pairs.join("; "));
    }
    secret(input, field).map(str::to_string)
}

/// The user agent a session sync sends when the sign-in does not say which
/// browser made it: a sign-in whose cookies the owner pasted, or one handed
/// over by an app older than `secrets.user_agent`.
const PASTED_SESSION_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
     AppleWebKit/537.36 (KHTML, like Gecko) Chrome/153.0.0.0 Safari/537.36";

/// The user agent to replay a browser session with: the one the session was
/// made in (`secrets.user_agent`, sent by the app's sign-in), so the site sees
/// one browser. A site can end a session that turns up from a second browser.
/// A value that is empty or could not be a header is ignored.
pub fn session_user_agent(input: &AppletInput) -> &str {
    input
        .credentials
        .as_ref()
        .and_then(|c| c.get("secrets"))
        .and_then(|s| s.get("user_agent"))
        .and_then(|v| v.as_str())
        .filter(|ua| !ua.trim().is_empty() && ua.chars().all(|c| c == ' ' || c.is_ascii_graphic()))
        .unwrap_or(PASTED_SESSION_USER_AGENT)
}

/// Mark this run's credential as needing the owner to reconnect, with the
/// sentence the Sources page shows beside its Reconnect button. For a session
/// the site ended: retrying cannot fix it, and a run that just fails looks the
/// same as an outage. Best effort; the caller still fails the run.
pub async fn needs_reconnect(pool: &sqlx::PgPool, input: &AppletInput, why: &str) {
    let Some(id) = input.credentials.as_ref().and_then(|c| c.get("id")).and_then(|v| v.as_str()) else {
        return;
    };
    if let Err(e) = virtues_helpers::auth::vault::mark_credential_status(
        pool,
        id,
        virtues_helpers::auth::vault::CredentialStatus::ReauthRequired,
        Some(why),
    )
    .await
    {
        tracing::warn!(credential_id = id, error = %e, "could not mark the credential for reconnecting");
    }
}

/// Read `config.<key>` as a string, if present.
pub fn config_str<'a>(input: &'a AppletInput, key: &str) -> Option<&'a str> {
    input.config.get(key).and_then(|v| v.as_str())
}

/// Send a request and deserialize a 2xx JSON body, with context on each failure.
pub async fn fetch_json<T: DeserializeOwned>(req: reqwest::RequestBuilder) -> Result<T> {
    let resp = req.send().await.context("request failed")?;
    let resp = resp.error_for_status().context("non-2xx response")?;
    resp.json::<T>().await.context("failed to parse JSON response")
}

/// Call a third-party data endpoint through virtues-api's `/v1/services/*`
/// proxy.
///
/// The box holds only the per-user token; virtues-api injects the master
/// application credentials server-side and forwards upstream, so the vendor's
/// app secret never lives on the box. `service` names the proxied vendor as
/// virtues-api routes it (`"plaid"`); `path` is the endpoint tail (e.g.
/// `"transactions/sync"`); `body` carries the per-user token and any params —
/// never the master credentials. Returns the vendor's response JSON verbatim,
/// so callers archive and transform it exactly as they would a direct call.
///
/// **Takes the service rather than naming one.** This was `plaid_proxy`, with
/// the vendor and its route welded into a library shared by every applet — so
/// the generic layer knew one integration by name, and the second vendor to use
/// the same proxy would have arrived as a copy of it. `/v1/services/{service}/`
/// is already a namespace on the virtues-api side; this now matches it.
pub async fn service_proxy(
    pool: &sqlx::PgPool,
    service: &str,
    path: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value> {
    let (status, resp_body) = service_proxy_raw(pool, service, path, body).await?;
    if !(200..300).contains(&status) {
        anyhow::bail!("{service} proxy /v1/services/{service}/{path} returned {status}: {resp_body}");
    }
    Ok(resp_body)
}

/// Like [`service_proxy`] but returns the raw `(status, body)` without erroring
/// on a non-2xx. For endpoints where a non-2xx is expected and benign (Plaid's
/// `liabilities/get` returns 400 for accounts that do not carry that product).
pub async fn service_proxy_raw(
    pool: &sqlx::PgPool,
    service: &str,
    path: &str,
    body: &serde_json::Value,
) -> Result<(u16, serde_json::Value)> {
    use virtues::virtues_api::client::{BearerClient, Purpose};

    let route = format!("/v1/services/{service}/{path}");
    let resp = BearerClient::from_env(pool.clone())
        .with_purpose(Purpose::System)
        .post_json(&route, body)
        .await
        .with_context(|| format!("{service} proxy call to {route} failed"))?;
    Ok((resp.status, resp.body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn input(secrets: serde_json::Value) -> AppletInput {
        serde_json::from_value(json!({ "config": {}, "credentials": { "id": "cred_t", "secrets": secrets } })).unwrap()
    }

    #[test]
    fn the_graph_root_is_the_api_base_plus_the_version() {
        let app = input(json!({ "api_base": "https://api.example.com/v1/outlook/c/conn_1/" }));
        assert_eq!(outlook_graph(&app).unwrap(), "https://api.example.com/v1/outlook/c/conn_1/v1.0");
        assert!(outlook_graph(&input(json!({ "access_token": "t" }))).is_err(), "no direct sign-in to fall back to");
        assert!(outlook_graph(&input(json!({ "api_base": " " }))).is_err());
    }

    #[test]
    fn a_graph_link_is_rebuilt_under_the_api_base() {
        let graph = "https://api.example.com/v1/outlook/c/conn_1/v1.0";
        let next = "https://graph.microsoft.com/v1.0/me/mailFolders('inbox')/messages/delta?$skiptoken=AbC-1_2%3D%3D";
        assert_eq!(
            graph_link(graph, next).unwrap(),
            "https://api.example.com/v1/outlook/c/conn_1/v1.0/me/mailFolders/inbox/messages/delta?$skiptoken=AbC-1_2%3D%3D"
        );
        let view = "https://graph.microsoft.com/v1.0/me/calendars('AAMkAGI2T_x-y=')/calendarView?startDateTime=2026-08-01T00%3A00%3A00Z&$skip=100";
        assert_eq!(
            graph_link(graph, view).unwrap(),
            "https://api.example.com/v1/outlook/c/conn_1/v1.0/me/calendars/AAMkAGI2T_x-y=/calendarView?startDateTime=2026-08-01T00%3A00%3A00Z&$skip=100"
        );
        // Plain paths, encoded quotes and a trailing slash on the base.
        let plain = "https://graph.microsoft.com/v1.0/me/calendars/AAMk%3D/calendarView?$skip=200";
        assert_eq!(
            graph_link(&format!("{graph}/"), plain).unwrap(),
            "https://api.example.com/v1/outlook/c/conn_1/v1.0/me/calendars/AAMk%3D/calendarView?$skip=200"
        );
        let encoded = "https://graph.microsoft.com/v1.0/me/mailFolders(%27sentitems%27)/messages/delta?$deltatoken=x";
        assert_eq!(
            graph_link(graph, encoded).unwrap(),
            "https://api.example.com/v1/outlook/c/conn_1/v1.0/me/mailFolders/sentitems/messages/delta?$deltatoken=x"
        );
        // Another host's link still goes to the api base, never to that host.
        let elsewhere = "https://graph.example.net/v1.0/me/calendars";
        assert_eq!(graph_link(graph, elsewhere).unwrap(), format!("{graph}/me/calendars"));
        assert!(graph_link(graph, "https://graph.microsoft.com/beta/me").is_err());
    }

    #[test]
    fn graph_errors_carry_the_code_from_either_body_shape() {
        let graph = GraphError::from_body(410, r#"{"error":{"code":"syncStateNotFound","message":"Sync state expired."}}"#);
        assert_eq!(graph.code.as_deref(), Some("syncStateNotFound"));
        assert_eq!(graph.message, "Sync state expired.");
        let api = GraphError::from_body(401, r#"{"code":"reauth_required","message":"Sign in again."}"#);
        assert!(api.is_reauth());
        let plain = GraphError::from_body(401, "Unauthorized");
        assert!(!plain.is_reauth());
        assert_eq!(plain.message, "Unauthorized");
    }

    #[test]
    fn a_rate_limit_wait_follows_retry_after_within_bounds() {
        let header = |v: &'static str| reqwest::header::HeaderValue::from_static(v);
        assert_eq!(rate_limit_wait(Some(&header("5"))).as_secs(), 5);
        assert_eq!(rate_limit_wait(Some(&header("600"))).as_secs(), MAX_RATE_LIMIT_WAIT_SECS);
        assert_eq!(rate_limit_wait(Some(&header("0"))).as_secs(), 1);
        assert_eq!(rate_limit_wait(Some(&header("Wed, 21 Oct 2026 07:28:00 GMT"))).as_secs(), 2);
        assert_eq!(rate_limit_wait(None).as_secs(), 2);
    }

    /// A stand-in for virtues-api: each request takes the next canned answer.
    async fn canned(answers: Vec<(&'static str, &'static str, &'static str)>) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = seen.clone();
        tokio::spawn(async move {
            for (status, extra, body) in answers {
                let Ok((mut socket, _)) = listener.accept().await else { return };
                let mut buf = vec![0u8; 4096];
                let n = socket.read(&mut buf).await.unwrap_or(0);
                log.lock().unwrap().push(String::from_utf8_lossy(&buf[..n]).to_string());
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });
        (base, seen)
    }

    #[tokio::test]
    async fn a_graph_get_waits_out_too_many_requests_and_sends_prefer() {
        let (base, seen) = canned(vec![
            ("429 Too Many Requests", "Retry-After: 1\r\n", "{}"),
            ("200 OK", "", r#"{"value":[]}"#),
        ])
        .await;
        let got = graph_get(&reqwest::Client::new(), "tok", &format!("{base}/v1.0/me"), Some("odata.maxpagesize=50"))
            .await
            .unwrap();
        assert_eq!(got, json!({ "value": [] }));
        let requests = seen.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests[1].to_ascii_lowercase().contains("prefer: odata.maxpagesize=50"));
        assert!(requests[1].to_ascii_lowercase().contains("authorization: bearer tok"));
    }

    #[tokio::test]
    async fn a_graph_get_fails_when_the_sign_in_ended() {
        let (base, _) = canned(vec![("401 Unauthorized", "", r#"{"code":"reauth_required"}"#)]).await;
        let err = graph_get(&reqwest::Client::new(), "tok", &format!("{base}/v1.0/me"), None).await.unwrap_err();
        assert!(graph_error(&err).is_some_and(GraphError::is_reauth));
        assert!(err.to_string().contains("reconnect Outlook"));
    }

    #[test]
    fn a_session_is_replayed_as_the_browser_that_made_it() {
        let app = input(json!({ "user_agent": "Mozilla/5.0 Example/1.0" }));
        assert_eq!(session_user_agent(&app), "Mozilla/5.0 Example/1.0");
        // A pasted sign-in, an empty value and one that could not be a header
        // all fall back to the default.
        for secrets in [json!({}), json!({ "user_agent": " " }), json!({ "user_agent": "a\r\nx: y" })] {
            assert_eq!(session_user_agent(&input(secrets)), PASTED_SESSION_USER_AGENT);
        }
    }

    #[test]
    fn cookies_come_from_the_jar_before_the_old_fields() {
        let both = input(json!({
            "auth_token": "old", "cookies": "a=old",
            "jar": [{"name": "auth_token", "value": "new", "domain": ".example.com"},
                    {"name": "ct0", "value": "csrf"}]
        }));
        assert_eq!(cookie(&both, "auth_token").unwrap(), "new");
        assert_eq!(cookie(&both, "ct0").unwrap(), "csrf");
        assert_eq!(cookie_header(&both, "cookies").unwrap(), "auth_token=new; ct0=csrf");
    }

    #[test]
    fn a_credential_without_a_jar_reads_its_old_fields() {
        let old = input(json!({ "auth_token": "old", "cookies": "a=1; b=2" }));
        assert_eq!(cookie(&old, "auth_token").unwrap(), "old");
        assert_eq!(cookie_header(&old, "cookies").unwrap(), "a=1; b=2");
        assert!(cookie(&old, "ct0").is_err());
    }
}

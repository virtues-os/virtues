//! The public MCP Registry (registry.modelcontextprotocol.io), searched from
//! the box so the owner can connect any listed server (agents/plan/mcp-plan.md
//! part 1). The registry is a list of servers and their addresses; it says
//! nothing about how often a server is used, so the order the owner sees is
//! [`FEATURED`] first and then the registry's own.
//!
//! Only servers the box can connect with one sign-in are listed: a remote over
//! streamable HTTP, at a fixed address, that needs no API key in a header.
//! The owner's search text goes to the registry; nothing else does.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::Mutex;

const REGISTRY: &str = "https://registry.modelcontextprotocol.io";

/// Shown first, in this order: servers that the service itself publishes,
/// for things people use day to day. Registry names, so each one's address
/// and description come from the registry. Zapier is the way to Gmail and
/// Google Calendar, since Google publishes no server for either.
///
/// Each one starts a sign-in with the box registering itself. GitHub, Figma
/// and Zoom are left out because they refuse that registration; they need a
/// client document hosted on the web (agents/plan/mcp-plan.md).
pub const FEATURED: &[&str] = &[
    "com.zapier/mcp",
    "com.notion/mcp",
    "net.todoist/mcp",
    "app.linear/linear",
    "com.airtable/mcp",
    "com.canva.mcp/mcp",
    "com.atlassian/atlassian-mcp-server",
    "com.monday/monday.com",
    "com.stripe/mcp",
];

/// How long the featured list is kept before it is read again.
const FEATURED_FOR: Duration = Duration::from_secs(6 * 60 * 60);
const TIMEOUT: Duration = Duration::from_secs(10);
const SEARCH_LIMIT: usize = 50;

/// One server the owner can connect.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Listing {
    /// Its registry name, such as `app.linear/linear`.
    pub registry: String,
    pub title: String,
    pub about: String,
    pub url: String,
    pub icon: Option<String>,
    pub website: Option<String>,
}

/// A registry entry as a [`Listing`], or `None` when the box cannot connect
/// it with a sign-in alone.
fn listing(entry: &Value) -> Option<Listing> {
    let server = entry.get("server")?;
    let status = entry.pointer("/_meta/io.modelcontextprotocol.registry~1official/status");
    if status.is_some_and(|s| s != "active") {
        return None;
    }
    let remote = server.get("remotes")?.as_array()?.iter().find(|r| {
        let needs_key = r
            .get("headers")
            .and_then(Value::as_array)
            .is_some_and(|h| h.iter().any(|h| h.get("isRequired") == Some(&Value::Bool(true))));
        r.get("type") == Some(&Value::from("streamable-http"))
            && r.get("url").and_then(Value::as_str).is_some_and(|u| u.starts_with("https://") && !u.contains('{'))
            && !needs_key
    })?;
    let registry = server.get("name")?.as_str()?.to_string();
    let text = |key: &str| server.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
    let title = text("title")
        .map(|t| t.trim_end_matches(" MCP Server").trim_end_matches(" MCP").to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| fallback_title(&registry));
    let icon = server
        .get("icons")
        .and_then(Value::as_array)
        .and_then(|icons| icons.iter().filter_map(|i| i.get("src")?.as_str()).find(|s| s.starts_with("https://")))
        .map(str::to_string);
    Some(Listing {
        about: text("description").unwrap_or("").to_string(),
        url: remote.get("url")?.as_str()?.to_string(),
        website: text("websiteUrl").filter(|u| u.starts_with("https://")).map(str::to_string),
        registry,
        title,
        icon,
    })
}

/// A title for an entry that has none: the server's name without "mcp", or
/// its publisher's when that leaves nothing. `io.github.getsentry/sentry-mcp`
/// is `Sentry` and `com.notion/mcp` is `Notion`.
fn fallback_title(registry: &str) -> String {
    let (publisher, server) = registry.split_once('/').unwrap_or((registry, registry));
    let server = server.trim_end_matches("-mcp").trim_start_matches("mcp-");
    let word = if !server.is_empty() && server != "mcp" {
        server
    } else {
        publisher
            .rsplit('.')
            .find(|w| !matches!(*w, "com" | "app" | "net" | "io" | "ai" | "dev" | "mcp" | "github"))
            .unwrap_or(publisher)
    };
    let mut chars = word.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| reqwest::Client::builder().timeout(TIMEOUT).build().unwrap_or_default())
}

async fn get(url: url::Url) -> Result<Value> {
    let response = http().get(url).send().await.context("reach the MCP Registry")?;
    if !response.status().is_success() {
        bail!("the MCP Registry answered {}", response.status());
    }
    response.json().await.context("read the MCP Registry's answer")
}

/// The newest version of one server, or `None` when the registry has it but
/// the box cannot connect it.
pub async fn lookup(registry: &str) -> Result<Option<Listing>> {
    let mut url = url::Url::parse(REGISTRY)?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("registry address"))?
        .extend(["v0", "servers", registry, "versions", "latest"]);
    Ok(listing(&get(url).await?))
}

/// [`FEATURED`], newest versions, read once every few hours.
pub async fn featured() -> Result<Vec<Listing>> {
    static CACHE: OnceLock<Mutex<Option<(Instant, Vec<Listing>)>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().await;
    if let Some((read, listings)) = cache.as_ref() {
        if read.elapsed() < FEATURED_FOR {
            return Ok(listings.clone());
        }
    }
    let found = futures::future::join_all(FEATURED.iter().map(|name| lookup(name))).await;
    let mut listings = Vec::new();
    let mut last_error = None;
    for (name, result) in FEATURED.iter().zip(found) {
        match result {
            Ok(Some(l)) => listings.push(l),
            Ok(None) => tracing::warn!(server = name, "featured MCP server needs more than a sign-in"),
            Err(e) => {
                tracing::warn!(server = name, error = %format!("{e:#}"), "featured MCP server not read");
                last_error = Some(e);
            }
        }
    }
    if listings.is_empty() {
        if let Some(e) = last_error {
            return Err(e);
        }
    }
    *cache = Some((Instant::now(), listings.clone()));
    Ok(listings)
}

/// Servers matching `query`, featured ones first.
pub async fn search(query: &str) -> Result<Vec<Listing>> {
    let mut url = url::Url::parse(REGISTRY)?.join("/v0/servers")?;
    url.query_pairs_mut()
        .append_pair("search", query.trim())
        .append_pair("version", "latest")
        .append_pair("limit", &SEARCH_LIMIT.to_string());
    let body = get(url).await?;
    let mut listings: Vec<Listing> = body
        .get("servers")
        .and_then(Value::as_array)
        .map(|servers| servers.iter().filter_map(listing).collect())
        .unwrap_or_default();
    listings.sort_by_key(|l| FEATURED.iter().position(|f| *f == l.registry).unwrap_or(FEATURED.len()));
    Ok(listings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry(remotes: Value) -> Value {
        json!({
            "server": { "name": "com.notion/mcp", "description": "Official Notion MCP server", "remotes": remotes },
            "_meta": { "io.modelcontextprotocol.registry/official": { "status": "active" } }
        })
    }

    #[test]
    fn a_streamable_remote_with_no_key_is_listed() {
        let l = listing(&entry(json!([
            { "type": "sse", "url": "https://mcp.notion.com/sse" },
            { "type": "streamable-http", "url": "https://mcp.notion.com/mcp" }
        ])))
        .unwrap();
        assert_eq!(l.url, "https://mcp.notion.com/mcp");
        assert_eq!(l.title, "Notion");
        assert_eq!(l.about, "Official Notion MCP server");
    }

    #[test]
    fn a_server_that_needs_a_key_or_a_filled_address_is_left_out() {
        let key = json!([{ "type": "streamable-http", "url": "https://example.com/mcp",
            "headers": [{ "name": "Authorization", "isRequired": true, "isSecret": true }] }]);
        assert!(listing(&entry(key)).is_none());
        let template = json!([{ "type": "streamable-http", "url": "https://example.com/{tenant}/mcp" }]);
        assert!(listing(&entry(template)).is_none());
        assert!(listing(&entry(json!([{ "type": "sse", "url": "https://example.com/sse" }]))).is_none());
        assert!(listing(&entry(json!([{ "type": "streamable-http", "url": "http://example.com/mcp" }]))).is_none());
    }

    #[test]
    fn a_retired_entry_is_left_out() {
        let mut e = entry(json!([{ "type": "streamable-http", "url": "https://mcp.notion.com/mcp" }]));
        e["_meta"]["io.modelcontextprotocol.registry/official"]["status"] = json!("deprecated");
        assert!(listing(&e).is_none());
    }

    #[test]
    fn an_untitled_entry_is_named_for_its_publisher() {
        assert_eq!(fallback_title("com.notion/mcp"), "Notion");
        assert_eq!(fallback_title("io.github.getsentry/sentry-mcp"), "Sentry");
        assert_eq!(fallback_title("net.todoist/mcp"), "Todoist");
    }
}

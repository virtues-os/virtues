//! Fetch a saved URL and reduce it to readable text.
//!
//! The capability bookmarks needed and the box did not have: given a URL a
//! person kept, get back a title, a description, a lead image, and the body
//! text — cheaply, and without trusting the URL (see [`guard`]).
//!
//! This is deliberately NOT a search product. `tools/web_search.rs` answers
//! "find me pages about X"; this answers "read the one page I already have".
//! They are different jobs and share no code.
//!
//! Native-first by design. The box has a residential IP, which is the whole
//! reason YouTube caption tracks and paywall-lite pages resolve from here at
//! all, and a fetch that stays on the box tells no third party what the user
//! saved. When this path fails — JS-rendered SPAs, bot walls — the escalation
//! is Parallel Extract, opt-in per source (docs/bookmarks-plan.md step 2).

pub mod article;
pub mod guard;

use std::time::Duration;

use crate::error::{Error, Result};
use crate::http_client::base_builder;

pub use article::Article;

/// Bytes of HTML we are willing to read. Well past any real article, well short
/// of a page that would hurt.
const MAX_BODY_BYTES: usize = 5 * 1024 * 1024;

/// A page that does not answer in this long is not worth an enrichment slot.
/// Shorter than the 60s used for LLM calls on purpose — those we wait for.
const FETCH_TIMEOUT_SECS: u64 = 20;

/// Redirect hops followed before giving up. Each hop is re-vetted.
const MAX_REDIRECTS: usize = 5;

/// Identify honestly. A site that would rather not be read by a machine can
/// see us coming and say no, which is the correct relationship to be in.
const USER_AGENT: &str = concat!(
    "virtues/",
    env!("CARGO_PKG_VERSION"),
    " (+https://virtues.com; personal bookmark archiver)"
);

/// A page as fetched: where we ended up, and what was on it.
#[derive(Debug, Clone)]
pub struct FetchedPage {
    /// The URL after redirects — what should be stored and cited.
    pub final_url: String,
    pub article: Article,
}

/// The shared client.
///
/// Built once: `base_builder()` reads the OS trust store on every call, which
/// is not something to repeat per fetch during an enrichment sweep.
///
/// **Redirects are disabled deliberately.** reqwest would follow them
/// internally, and every hop it followed on its own would skip the address
/// guard — a public URL that 302s to `http://169.254.169.254/` is the entire
/// attack. Following them by hand is what makes the guard hold.
fn client() -> Result<&'static reqwest::Client> {
    static CLIENT: std::sync::OnceLock<std::result::Result<reqwest::Client, String>> =
        std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            base_builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(FETCH_TIMEOUT_SECS))
                .user_agent(USER_AGENT)
                .build()
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| Error::Network(format!("cannot build fetch client: {e}")))
}

/// Fetch `url` and extract what is worth keeping.
///
/// Refuses anything that is not http(s), anything resolving to a non-public
/// address, and any response that is not HTML or plain text — a bookmark
/// pointing at a 200MB video should cost one HEAD-shaped round trip, not a
/// download.
pub async fn fetch_page(url: &str) -> Result<FetchedPage> {
    let mut current = normalize(url)?;

    for _hop in 0..=MAX_REDIRECTS {
        vet(&current).await?;

        let response = client()?
            .get(current.as_str())
            .send()
            .await
            .map_err(|e| Error::Network(format!("fetching {current}: {e}")))?;

        let status = response.status();
        if status.is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| {
                    Error::Http(format!("{current} returned {status} with no Location"))
                })?;
            // Relative Locations are legal and common; resolve against the URL
            // we actually requested.
            current = current
                .join(location)
                .map_err(|e| Error::InvalidInput(format!("bad redirect from {current}: {e}")))?;
            if !matches!(current.scheme(), "http" | "https") {
                return Err(Error::InvalidInput(format!(
                    "refusing redirect to non-http scheme: {current}"
                )));
            }
            continue;
        }

        if !status.is_success() {
            return Err(Error::Http(format!("{current} returned {status}")));
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !is_readable_type(&content_type) {
            return Err(Error::InvalidInput(format!(
                "{current} is {}, which this path does not read",
                if content_type.is_empty() {
                    "of unknown type"
                } else {
                    &content_type
                }
            )));
        }

        let final_url = response.url().to_string();
        let body = read_capped(response).await?;
        let html = String::from_utf8_lossy(&body);

        return Ok(FetchedPage {
            final_url,
            article: article::parse(&html),
        });
    }

    Err(Error::Http(format!(
        "{url} still redirecting after {MAX_REDIRECTS} hops"
    )))
}

/// Parse and require an http(s) URL with a host.
fn normalize(raw: &str) -> Result<url::Url> {
    let parsed = url::Url::parse(raw.trim())
        .map_err(|e| Error::InvalidInput(format!("not a URL: {raw} ({e})")))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(Error::InvalidInput(format!(
            "refusing to fetch {} URL: {parsed}",
            parsed.scheme()
        )));
    }
    if parsed.host_str().is_none() {
        return Err(Error::InvalidInput(format!("URL has no host: {parsed}")));
    }
    Ok(parsed)
}

/// Resolve this hop's host and refuse non-public destinations.
async fn vet(u: &url::Url) -> Result<()> {
    let host = u
        .host_str()
        .ok_or_else(|| Error::InvalidInput(format!("URL has no host: {u}")))?;
    let port = u
        .port_or_known_default()
        .ok_or_else(|| Error::InvalidInput(format!("URL has no port: {u}")))?;
    guard::resolve_public(host, port).await.map(|_| ())
}

fn is_readable_type(content_type: &str) -> bool {
    let base = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim();
    matches!(
        base,
        "text/html" | "application/xhtml+xml" | "text/plain" | "application/xml" | "text/xml"
    )
}

/// Read the body, stopping at [`MAX_BODY_BYTES`].
///
/// Streamed rather than `bytes()` so a server advertising a small page and then
/// sending gigabytes is cut off at the cap instead of buffering all of it. A
/// truncated page is still worth parsing — the useful part of an article is at
/// the top.
async fn read_capped(mut response: reqwest::Response) -> Result<Vec<u8>> {
    let mut body = Vec::with_capacity(64 * 1024);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| Error::Network(format!("reading body: {e}")))?
    {
        let room = MAX_BODY_BYTES.saturating_sub(body.len());
        if room == 0 {
            break;
        }
        let take = room.min(chunk.len());
        body.extend_from_slice(&chunk[..take]);
        if take < chunk.len() {
            break;
        }
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn refuses_non_http_schemes() {
        for bad in ["file:///etc/passwd", "ftp://example.com/x", "data:text/html,x"] {
            let err = fetch_page(bad).await.unwrap_err();
            assert!(
                err.to_string().contains("refusing") || err.to_string().contains("not a URL"),
                "{bad} gave: {err}"
            );
        }
    }

    #[tokio::test]
    async fn refuses_loopback_and_metadata() {
        // The two that matter: our own services, and cloud metadata.
        for bad in [
            "http://127.0.0.1:18181/embed",
            "http://localhost:5432/",
            "http://169.254.169.254/latest/meta-data/",
        ] {
            let err = fetch_page(bad).await.unwrap_err();
            assert!(
                err.to_string().contains("non-public"),
                "{bad} was not refused as non-public: {err}"
            );
        }
    }

    #[test]
    fn content_type_gate() {
        assert!(is_readable_type("text/html; charset=utf-8"));
        assert!(is_readable_type("text/plain"));
        assert!(!is_readable_type("application/pdf"));
        assert!(!is_readable_type("video/mp4"));
        assert!(!is_readable_type("image/jpeg"));
        assert!(!is_readable_type(""));
    }

    /// Against the live internet (needs network; run explicitly):
    ///
    ///     cargo test -p virtues --lib -- --ignored fetches_a_real_page
    ///
    /// The unit tests above prove the policy; only this proves the thing works.
    #[tokio::test]
    #[ignore]
    async fn fetches_a_real_page() {
        let page = fetch_page("https://example.com").await.expect("fetch");
        assert!(page.final_url.starts_with("https://example.com"));
        assert!(
            page.article.title.is_some(),
            "no title extracted from example.com"
        );
        assert!(
            page.article.text.to_lowercase().contains("example domain"),
            "body text missing, got: {:?}",
            page.article.text
        );
    }

    #[test]
    fn normalize_requires_a_host() {
        assert!(normalize("https://example.com/a").is_ok());
        assert!(normalize("http://").is_err());
        assert!(normalize("nonsense").is_err());
    }
}


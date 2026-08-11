//! Shared HTTP Client Configuration
//!
//! Provides pre-configured HTTP clients with appropriate timeouts for
//! different use cases (regular requests vs streaming).
//!
//! All clients going to virtues-api should use these to ensure consistent
//! timeout behavior and connection pooling.

use std::sync::Arc;
use std::time::Duration;

/// Install the ring CryptoProvider as the process default if nobody has yet.
///
/// reqwest-with-rustls panics with "No provider set" if a client is built
/// before a provider is installed. `main.rs` installs it for the `virtues`
/// binary, but other entry points (tests, mcp-server, seed bins) build
/// clients directly — so the constructors below are self-sufficient instead
/// of relying on every caller remembering. Idempotent: a second install
/// attempt errors harmlessly and is ignored.
pub(crate) fn ensure_crypto_provider() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// DNS resolver that returns only A (IPv4) records.
///
/// reqwest (via rustls) tries AAAA first when DNS returns both A and AAAA.
/// On boxes with no global IPv6 routing, the connect gets ENETUNREACH
/// instantly and reqwest does NOT fall back to IPv4 — it just fails. This
/// resolver strips AAAA records so the connector never attempts IPv6.
#[derive(Clone, Debug)]
struct Ipv4OnlyResolver;

impl reqwest::dns::Resolve for Ipv4OnlyResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let addrs: Vec<std::net::SocketAddr> =
                tokio::net::lookup_host(format!("{}:0", host))
                    .await
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
                    .filter(std::net::SocketAddr::is_ipv4)
                    .collect();
            Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// Load the OS native CA certificate store.
///
/// Uses rustls-native-certs so our TLS verification tracks the system trust
/// store rather than the compiled-in webpki-roots bundle. This means Let's
/// Encrypt intermediate rotation (e.g. R3→R11, E1→E8) never breaks outbound
/// HTTPS, because the OS CA store already trusts ISRG Root X1 and X2.
fn native_root_certs() -> Vec<reqwest::Certificate> {
    let result = rustls_native_certs::load_native_certs();
    if !result.errors.is_empty() {
        tracing::warn!(
            count = result.errors.len(),
            "some native CA certs failed to load; continuing with those that did"
        );
    }
    result
        .certs
        .iter()
        .filter_map(|cert| reqwest::Certificate::from_der(cert).ok())
        .collect()
}

/// Connect timeout in seconds (time to establish TCP connection)
pub const CONNECT_TIMEOUT_SECS: u64 = 10;

/// Request timeout for regular (non-streaming) requests in seconds
pub const REQUEST_TIMEOUT_SECS: u64 = 60;

/// Request timeout for streaming requests in seconds (longer for SSE)
pub const STREAMING_TIMEOUT_SECS: u64 = 300;

/// Request timeout for non-streaming AI completions in seconds.
///
/// A non-streaming completion delivers nothing until the whole generation
/// finishes, so its wall time is the model's full thinking-plus-writing time
/// — a 4000-token structured extraction routinely runs 45–90s, and reasoning
/// models (Grok reasons on every turn, untunably) push past that. At 60s the
/// nightly day-summary segmentation died at exactly the timeout three days
/// running (2026-08-09..11), after months of 46–60s near-misses. Matches
/// STREAMING_TIMEOUT_SECS: the same generation over SSE is already allowed
/// this long.
pub const AI_COMPLETION_TIMEOUT_SECS: u64 = 300;

/// Shared rooted builder: installs the rustls crypto provider and loads the OS
/// native CA store, with an IPv4-only resolver. `reqwest` is built with
/// `rustls-tls-no-provider` (no bundled provider/roots), so a bare
/// `reqwest::Client::builder()` has an EMPTY trust store and every HTTPS
/// handshake fails ("error sending request") — every outbound client must start
/// here. Exposed `pub(crate)` so the CLI (`virtues upgrade`) shares the exact
/// same TLS setup instead of rolling a rootless client.
pub(crate) fn base_builder() -> reqwest::ClientBuilder {
    ensure_crypto_provider();
    let mut builder = reqwest::Client::builder()
        .tls_built_in_root_certs(false) // use native roots only
        .dns_resolver(Arc::new(Ipv4OnlyResolver));
    for cert in native_root_certs() {
        builder = builder.add_root_certificate(cert);
    }
    builder
}

/// Create an HTTP client for regular virtues-api requests (non-streaming)
///
/// Uses moderate timeouts suitable for synchronous LLM calls.
pub fn virtues_api_client() -> reqwest::Client {
    base_builder()
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .expect("Failed to build HTTP client")
}

/// Create an HTTP client for non-streaming AI completions (`/v1/ai/*`).
///
/// Same connect timeout as the regular client, but the request timeout covers
/// a full unstreamed generation — see [`AI_COMPLETION_TIMEOUT_SECS`].
pub fn virtues_api_completion_client() -> reqwest::Client {
    base_builder()
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(AI_COMPLETION_TIMEOUT_SECS))
        .build()
        .expect("Failed to build completion HTTP client")
}

/// Create an HTTP client for streaming virtues-api requests (SSE)
///
/// Uses longer timeouts to accommodate streaming responses that
/// may take several minutes to complete.
pub fn virtues_api_streaming_client() -> reqwest::Client {
    base_builder()
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(STREAMING_TIMEOUT_SECS))
        .build()
        .expect("Failed to build streaming HTTP client")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtues_api_client_creation() {
        let client = virtues_api_client();
        drop(client);
    }

    #[test]
    fn test_completion_client_creation() {
        let client = virtues_api_completion_client();
        drop(client);
    }

    #[test]
    fn test_streaming_client_creation() {
        let client = virtues_api_streaming_client();
        drop(client);
    }
}

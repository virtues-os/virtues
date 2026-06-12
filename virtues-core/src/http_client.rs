//! Shared HTTP Client Configuration
//!
//! Provides pre-configured HTTP clients with appropriate timeouts for
//! different use cases (regular requests vs streaming).
//!
//! All clients going to virtues-api should use these to ensure consistent
//! timeout behavior and connection pooling.

use std::time::Duration;

/// Install the ring CryptoProvider as the process default if nobody has yet.
///
/// reqwest-with-rustls panics with "No provider set" if a client is built
/// before a provider is installed. `main.rs` installs it for the `virtues`
/// binary, but other entry points (tests, mcp-server, seed bins) build
/// clients directly — so the constructors below are self-sufficient instead
/// of relying on every caller remembering. Idempotent: a second install
/// attempt errors harmlessly and is ignored.
fn ensure_crypto_provider() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Connect timeout in seconds (time to establish TCP connection)
pub const CONNECT_TIMEOUT_SECS: u64 = 10;

/// Request timeout for regular (non-streaming) requests in seconds
pub const REQUEST_TIMEOUT_SECS: u64 = 60;

/// Request timeout for streaming requests in seconds (longer for SSE)
pub const STREAMING_TIMEOUT_SECS: u64 = 300;

/// Create an HTTP client for regular virtues-api requests (non-streaming)
///
/// Uses moderate timeouts suitable for synchronous LLM calls.
pub fn virtues_api_client() -> reqwest::Client {
    ensure_crypto_provider();
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .expect("Failed to build HTTP client")
}

/// Create an HTTP client for streaming virtues-api requests (SSE)
///
/// Uses longer timeouts to accommodate streaming responses that
/// may take several minutes to complete.
pub fn virtues_api_streaming_client() -> reqwest::Client {
    ensure_crypto_provider();
    reqwest::Client::builder()
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
        // Just verify it creates without panicking
        drop(client);
    }

    #[test]
    fn test_streaming_client_creation() {
        let client = virtues_api_streaming_client();
        // Just verify it creates without panicking
        drop(client);
    }
}

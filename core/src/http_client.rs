//! Shared HTTP Client Configuration
//!
//! Provides pre-configured HTTP clients with appropriate timeouts for
//! different use cases (regular requests vs streaming).
//!
//! All clients going to Tollbooth should use these to ensure consistent
//! timeout behavior and connection pooling.

use std::time::Duration;

/// Connect timeout in seconds (time to establish TCP connection)
pub const CONNECT_TIMEOUT_SECS: u64 = 10;

/// Request timeout for regular (non-streaming) requests in seconds
pub const REQUEST_TIMEOUT_SECS: u64 = 60;

/// Request timeout for streaming requests in seconds (longer for SSE)
pub const STREAMING_TIMEOUT_SECS: u64 = 300;

/// Create an HTTP client for regular Tollbooth requests (non-streaming)
///
/// Uses moderate timeouts suitable for synchronous LLM calls.
pub fn tollbooth_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .expect("Failed to build HTTP client")
}

/// Create an HTTP client for streaming Tollbooth requests (SSE)
///
/// Uses longer timeouts to accommodate streaming responses that
/// may take several minutes to complete.
pub fn tollbooth_streaming_client() -> reqwest::Client {
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
    fn test_tollbooth_client_creation() {
        let client = tollbooth_client();
        // Just verify it creates without panicking
        drop(client);
    }

    #[test]
    fn test_streaming_client_creation() {
        let client = tollbooth_streaming_client();
        // Just verify it creates without panicking
        drop(client);
    }
}

//! Proxy Error Types
//!
//! CRITICAL: virtues-api is a privacy-preserving proxy.
//! We do NOT:
//! - Log request bodies (prompts)
//! - Log response bodies (completions)
//! - Store any content for training
//! - Analyze or inspect payloads
//!
//! We ONLY:
//! - Check budget (in routes, before calling providers)
//! - Extract usage metadata from responses for billing
//!
//! This code is open source so you can verify these guarantees.

use virtues_helpers::error::StructuredError;

/// Proxy error types
#[derive(Debug)]
pub enum ProxyError {
    UpstreamError { status: u16, message: String },
    NetworkError { message: String },
}

impl ProxyError {
    /// Map an upstream HTTP status to a (code, hint) pair.
    fn classify(status: u16) -> (&'static str, &'static str) {
        match status {
            401 | 403 => (
                "llm_provider_auth_failed",
                "Check your LLM provider API key (OPENAI_API_KEY, ANTHROPIC_API_KEY, etc.)",
            ),
            429 => ("rate_limited", "LLM provider rate limit exceeded. Wait and retry."),
            500..=599 => (
                "provider_error",
                "LLM provider service error. Try again or use a different model.",
            ),
            _ => ("upstream_error", "Error communicating with LLM provider."),
        }
    }
}

impl StructuredError for ProxyError {
    fn status(&self) -> u16 {
        match self {
            ProxyError::UpstreamError { status, .. } => *status,
            ProxyError::NetworkError { .. } => 502,
        }
    }
    fn code(&self) -> &str {
        match self {
            ProxyError::UpstreamError { status, .. } => Self::classify(*status).0,
            ProxyError::NetworkError { .. } => "network_error",
        }
    }
    fn message(&self) -> String {
        match self {
            ProxyError::UpstreamError { status, message } => {
                format!("[{}] {}", Self::classify(*status).0, message)
            }
            ProxyError::NetworkError { message } => message.clone(),
        }
    }
    fn extra(&self) -> serde_json::Value {
        match self {
            ProxyError::UpstreamError { status, .. } => {
                let (ty, hint) = Self::classify(*status);
                serde_json::json!({ "type": ty, "hint": hint, "upstream_status": status })
            }
            ProxyError::NetworkError { .. } => serde_json::json!({ "type": "network_error" }),
        }
    }
}
virtues_helpers::impl_into_response!(ProxyError);

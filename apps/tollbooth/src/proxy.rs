//! Proxy Error Types
//!
//! CRITICAL: Tollbooth is a privacy-preserving proxy.
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

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

/// Proxy error types
#[derive(Debug)]
pub enum ProxyError {
    InsufficientBudget { balance: f64 },
    SubscriptionExpired { status: String },
    UpstreamError { status: u16, message: String },
    NetworkError { message: String },
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        let (status, body) = match self {
            ProxyError::InsufficientBudget { balance } => (
                StatusCode::PAYMENT_REQUIRED,
                serde_json::json!({
                    "error": {
                        "message": format!("Insufficient budget. Current balance: ${:.2}", balance),
                        "type": "insufficient_quota",
                        "code": "insufficient_budget"
                    }
                }),
            ),
            ProxyError::SubscriptionExpired { status } => (
                StatusCode::PAYMENT_REQUIRED,
                serde_json::json!({
                    "error": {
                        "message": "Subscription expired",
                        "type": "subscription_expired",
                        "code": "subscription_expired",
                        "status": status
                    }
                }),
            ),
            ProxyError::UpstreamError { status, message } => {
                // Provide more specific error messages for common upstream errors
                let (error_type, hint) = match status {
                    401 | 403 => (
                        "llm_provider_auth_failed",
                        "Check your LLM provider API key (OPENAI_API_KEY, ANTHROPIC_API_KEY, etc.)"
                    ),
                    429 => (
                        "rate_limited",
                        "LLM provider rate limit exceeded. Wait and retry."
                    ),
                    500..=599 => (
                        "provider_error",
                        "LLM provider service error. Try again or use a different model."
                    ),
                    _ => (
                        "upstream_error",
                        "Error communicating with LLM provider."
                    ),
                };

                (
                    StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
                    serde_json::json!({
                        "error": {
                            "message": format!("[{}] {}", error_type, message),
                            "type": error_type,
                            "code": error_type,
                            "hint": hint,
                            "upstream_status": status
                        }
                    }),
                )
            }
            ProxyError::NetworkError { message } => (
                StatusCode::BAD_GATEWAY,
                serde_json::json!({
                    "error": {
                        "message": message,
                        "type": "network_error",
                        "code": "network_error"
                    }
                }),
            ),
        };

        (status, axum::Json(body)).into_response()
    }
}

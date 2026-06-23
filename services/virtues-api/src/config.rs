//! Configuration for virtues-api
//!
//! All secrets are injected via environment variables at runtime.
//! The source code contains no secrets.
//!
//! Budget state lives in Postgres (accounts + ledger); the pool is required at
//! boot. Atlas pushes credits + device registrations in via `/internal/*`;
//! virtues-api never calls back — there is no hydration mode.

use anyhow::{bail, Context, Result};

/// Minimum secret length for security (256 bits = 32 bytes)
/// Weak secrets enable brute-force attacks
pub const MIN_SECRET_LENGTH: usize = 32;

#[derive(Clone)]
pub struct Config {
    /// Port to listen on (default: 9002).
    pub port: u16,

    /// Internal secret for validating requests from Core backend
    /// Sent via X-Internal-Secret header
    pub internal_secret: String,

    // =========================================================================
    // Vercel AI Gateway (Single unified LLM provider)
    // =========================================================================
    /// Vercel AI Gateway API key
    /// Get from: https://vercel.com/ai-gateway
    pub ai_gateway_api_key: String,

    /// Vercel AI Gateway URL (default: https://ai-gateway.vercel.sh)
    pub ai_gateway_url: String,

    // =========================================================================
    // External Service API Keys (All billable services proxied through virtues-api)
    // =========================================================================
    /// Exa API key (for web search)
    pub exa_api_key: Option<String>,

    /// Google API key (for Places autocomplete)
    pub google_api_key: Option<String>,

    /// Unsplash API key (for cover image search)
    pub unsplash_access_key: Option<String>,
    // Plaid/OAuth provider credentials are read directly from the environment
    // in `routes/oauth.rs` (the OAuth proxy), not carried on this struct.
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            port: std::env::var("VIRTUES_API_PORT")
                .unwrap_or_else(|_| "9002".to_string())
                .parse()
                .context("Invalid VIRTUES_API_PORT")?,

            internal_secret: {
                let secret = std::env::var("VIRTUES_API_INTERNAL_SECRET")
                    .context("VIRTUES_API_INTERNAL_SECRET is required")?;
                if secret.len() < MIN_SECRET_LENGTH {
                    bail!(
                        "VIRTUES_API_INTERNAL_SECRET must be at least {} characters (got {})",
                        MIN_SECRET_LENGTH,
                        secret.len()
                    );
                }
                secret
            },

            // Atlas integration (optional)

            // Vercel AI Gateway
            ai_gateway_api_key: std::env::var("AI_GATEWAY_API_KEY")
                .context("AI_GATEWAY_API_KEY is required")?,
            ai_gateway_url: std::env::var("AI_GATEWAY_URL")
                .unwrap_or_else(|_| "https://ai-gateway.vercel.sh".to_string()),

            // External service API keys
            exa_api_key: std::env::var("EXA_API_KEY").ok(),
            google_api_key: std::env::var("GOOGLE_API_KEY").ok(),
            unsplash_access_key: std::env::var("UNSPLASH_ACCESS_KEY").ok(),
        })
    }

    /// Check if LLM provider (AI Gateway) is configured
    pub fn has_llm_provider(&self) -> bool {
        !self.ai_gateway_api_key.is_empty()
    }
}

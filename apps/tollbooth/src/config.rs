//! Configuration for Tollbooth
//!
//! All secrets are injected via environment variables at runtime.
//! The source code contains no secrets.
//!
//! Tollbooth runs in two modes:
//! - Standalone: RAM-only budget tracking, default budget for all users
//! - Production: Hydrates from Atlas on startup, reports usage to Atlas

use anyhow::{bail, Context, Result};

/// Minimum secret length for security (256 bits = 32 bytes)
/// Weak secrets enable brute-force attacks
pub const MIN_SECRET_LENGTH: usize = 32;

#[derive(Clone)]
pub struct Config {
    /// Port to listen on (default: 9002 to avoid MinIO conflict on 9000)
    pub port: u16,

    /// Internal secret for validating requests from Core backend
    /// Sent via X-Internal-Secret header
    pub internal_secret: String,

    /// Default budget for new users in USD (default: 5.00)
    /// In standalone mode, all users get this budget
    pub default_budget_usd: f64,

    // =========================================================================
    // Atlas Integration (Optional - for production with orchestrator)
    // =========================================================================
    /// Atlas API URL for hydrating budgets on startup
    /// If not set, runs in standalone mode with default budgets
    pub atlas_url: Option<String>,

    /// Shared secret for authenticating with Atlas internal API
    pub atlas_secret: Option<String>,

    /// Subdomain identifying this tenant (used when calling Atlas endpoints)
    pub subdomain: Option<String>,

    /// Interval in seconds for reporting usage to Atlas (default: 30)
    pub atlas_report_interval_secs: u64,

    /// Interval in seconds for re-hydrating budgets/tiers/subscriptions from Atlas (default: 900 = 15 min)
    /// Catches subscription changes, trial expirations, balance top-ups, and plan upgrades
    /// that happen while Tollbooth is running.
    pub atlas_rehydrate_interval_secs: u64,

    // =========================================================================
    // Vercel AI Gateway (Single unified LLM provider)
    // =========================================================================
    /// Vercel AI Gateway API key
    /// Get from: https://vercel.com/ai-gateway
    pub ai_gateway_api_key: String,

    /// Vercel AI Gateway URL (default: https://ai-gateway.vercel.sh)
    pub ai_gateway_url: String,

    // =========================================================================
    // External Service API Keys (All billable services proxied through Tollbooth)
    // =========================================================================
    /// Exa API key (for web search)
    pub exa_api_key: Option<String>,

    /// Google API key (for Places autocomplete)
    pub google_api_key: Option<String>,

    /// Unsplash API key (for cover image search)
    pub unsplash_access_key: Option<String>,

    /// Plaid Client ID
    pub plaid_client_id: Option<String>,

    /// Plaid Secret
    pub plaid_secret: Option<String>,

    /// Plaid Environment (sandbox, development, production)
    pub plaid_env: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            port: std::env::var("TOLLBOOTH_PORT")
                .unwrap_or_else(|_| "9002".to_string())
                .parse()
                .context("Invalid TOLLBOOTH_PORT")?,

            internal_secret: {
                let secret = std::env::var("TOLLBOOTH_INTERNAL_SECRET")
                    .context("TOLLBOOTH_INTERNAL_SECRET is required")?;
                if secret.len() < MIN_SECRET_LENGTH {
                    bail!(
                        "TOLLBOOTH_INTERNAL_SECRET must be at least {} characters (got {})",
                        MIN_SECRET_LENGTH,
                        secret.len()
                    );
                }
                secret
            },

            default_budget_usd: std::env::var("TOLLBOOTH_DEFAULT_BUDGET")
                .unwrap_or_else(|_| "5.0".to_string())
                .parse()
                .context("Invalid TOLLBOOTH_DEFAULT_BUDGET")?,

            // Atlas integration (optional)
            atlas_url: std::env::var("ATLAS_URL").ok(),
            atlas_secret: std::env::var("ATLAS_SECRET").ok(),
            subdomain: std::env::var("SUBDOMAIN").ok(),
            atlas_report_interval_secs: std::env::var("TOLLBOOTH_REPORT_INTERVAL")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .context("Invalid TOLLBOOTH_REPORT_INTERVAL")?,
            atlas_rehydrate_interval_secs: std::env::var("TOLLBOOTH_REHYDRATE_INTERVAL")
                .unwrap_or_else(|_| "900".to_string())
                .parse()
                .context("Invalid TOLLBOOTH_REHYDRATE_INTERVAL")?,

            // Vercel AI Gateway
            ai_gateway_api_key: std::env::var("AI_GATEWAY_API_KEY")
                .context("AI_GATEWAY_API_KEY is required")?,
            ai_gateway_url: std::env::var("AI_GATEWAY_URL")
                .unwrap_or_else(|_| "https://ai-gateway.vercel.sh".to_string()),

            // External service API keys
            exa_api_key: std::env::var("EXA_API_KEY").ok(),
            google_api_key: std::env::var("GOOGLE_API_KEY").ok(),
            unsplash_access_key: std::env::var("UNSPLASH_ACCESS_KEY").ok(),

            // Plaid
            plaid_client_id: std::env::var("PLAID_CLIENT_ID").ok(),
            plaid_secret: std::env::var("PLAID_SECRET").ok(),
            plaid_env: std::env::var("PLAID_ENV").unwrap_or_else(|_| "sandbox".to_string()),
        })
    }

    /// Check if LLM provider (AI Gateway) is configured
    pub fn has_llm_provider(&self) -> bool {
        !self.ai_gateway_api_key.is_empty()
    }

    /// Check if Atlas integration is configured
    pub fn has_atlas(&self) -> bool {
        self.atlas_url.is_some() && self.atlas_secret.is_some()
    }

    /// Check if Plaid is configured
    pub fn has_plaid(&self) -> bool {
        self.plaid_client_id.is_some() && self.plaid_secret.is_some()
    }
}

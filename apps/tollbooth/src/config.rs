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

    /// Interval in seconds for reporting usage to Atlas (default: 30)
    pub atlas_report_interval_secs: u64,

    // =========================================================================
    // LLM Provider API Keys (Direct accounts, no Bedrock)
    // =========================================================================

    /// OpenAI API key (for GPT-4o - Smart mode)
    pub openai_api_key: Option<String>,

    /// Anthropic API key (for Claude 3.5 Sonnet - Smart mode)
    pub anthropic_api_key: Option<String>,

    /// Cerebras API key (for Llama - Instant mode)
    pub cerebras_api_key: Option<String>,

    /// Google Cloud Project ID (for Vertex AI / Gemini models)
    /// Required for Vertex AI - get from: https://console.cloud.google.com
    pub google_cloud_project: Option<String>,

    /// Google Cloud Region for Vertex AI (default: us-central1)
    /// Available regions: us-central1, europe-west4, asia-northeast1, etc.
    pub google_cloud_region: String,

    /// xAI API key (for Grok models)
    /// Get from: https://console.x.ai
    pub xai_api_key: Option<String>,

    // =========================================================================
    // Model Routing Configuration
    // =========================================================================

    /// Default model for "smart" requests (default: gpt-4o)
    pub default_smart_model: String,

    /// Default model for "instant" requests (default: cerebras/gpt-oss-120b)
    pub default_instant_model: String,

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
            atlas_report_interval_secs: std::env::var("TOLLBOOTH_REPORT_INTERVAL")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .context("Invalid TOLLBOOTH_REPORT_INTERVAL")?,

            // LLM Provider Keys
            openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
            anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
            cerebras_api_key: std::env::var("CEREBRAS_API_KEY").ok(),
            // Vertex AI uses GOOGLE_APPLICATION_CREDENTIALS for auth (read by gcp_auth)
            google_cloud_project: std::env::var("GOOGLE_CLOUD_PROJECT").ok(),
            google_cloud_region: std::env::var("GOOGLE_CLOUD_REGION")
                .unwrap_or_else(|_| "us-central1".to_string()),
            xai_api_key: std::env::var("XAI_API_KEY").ok(),

            // Model routing
            default_smart_model: std::env::var("DEFAULT_SMART_MODEL")
                .unwrap_or_else(|_| "openai/gpt-4o".to_string()),
            default_instant_model: std::env::var("DEFAULT_INSTANT_MODEL")
                .unwrap_or_else(|_| "cerebras/gpt-oss-120b".to_string()),

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

    /// Check if at least one LLM provider is configured
    pub fn has_llm_provider(&self) -> bool {
        self.openai_api_key.is_some()
            || self.anthropic_api_key.is_some()
            || self.cerebras_api_key.is_some()
            || self.google_cloud_project.is_some()
            || self.xai_api_key.is_some()
    }

    /// Check if Vertex AI (Google Cloud) is configured
    pub fn has_vertex_ai(&self) -> bool {
        self.google_cloud_project.is_some()
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

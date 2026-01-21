//! Provider Configuration and Routing
//!
//! Unified provider handling for all LLM requests (streaming and non-streaming).
//! Each provider has specific endpoint, authentication, and format requirements.
//!
//! Supported Providers:
//! - OpenAI: Bearer token, OpenAI format
//! - Anthropic: x-api-key header, Messages API format (requires transform)
//! - Cerebras: Bearer token, OpenAI-compatible
//! - Vertex AI (Google): OAuth2 Bearer, OpenAI-compatible
//! - xAI (Grok): Bearer token, OpenAI-compatible

use gcp_auth::TokenProvider;
use std::sync::Arc;
use tokio::sync::OnceCell;

use crate::{config::Config, proxy::ProxyError};

/// Cached GCP token provider for Vertex AI
static GCP_AUTH: OnceCell<Arc<dyn TokenProvider>> = OnceCell::const_new();

/// Provider configuration for making LLM requests
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// API endpoint URL
    pub endpoint: String,
    /// API key (None for Vertex AI which uses OAuth2)
    pub api_key: Option<String>,
    /// Whether this is Anthropic (requires different request/response format)
    pub is_anthropic: bool,
    /// Whether this is Vertex AI (requires OAuth2 token)
    pub is_vertex_ai: bool,
    /// Model name to send to the provider (may differ from requested model)
    pub model_name: String,
}

/// Get or initialize the GCP token provider
async fn get_gcp_provider() -> Result<Arc<dyn TokenProvider>, ProxyError> {
    GCP_AUTH
        .get_or_try_init(|| async {
            gcp_auth::provider()
                .await
                .map_err(|e| ProxyError::UpstreamError {
                    status: 500,
                    message: format!("Failed to initialize GCP auth: {}", e),
                })
        })
        .await
        .cloned()
}

/// Get Vertex AI OAuth2 access token
///
/// Tokens are cached and refreshed automatically by gcp_auth.
/// Token lifetime is ~1 hour.
pub async fn get_vertex_ai_token() -> Result<String, ProxyError> {
    let provider = get_gcp_provider().await?;
    let scopes = &["https://www.googleapis.com/auth/cloud-platform"];
    let token = provider
        .token(scopes)
        .await
        .map_err(|e| ProxyError::UpstreamError {
            status: 500,
            message: format!("Failed to get Vertex AI token: {}", e),
        })?;
    Ok(token.as_str().to_string())
}

/// Get provider configuration based on model name
///
/// Routes models to the appropriate provider based on prefix or name:
/// - `anthropic/` or contains `claude` → Anthropic
/// - `cerebras/` or contains `llama` → Cerebras
/// - `google/` or contains `gemini` → Vertex AI
/// - `xai/` or contains `grok` → xAI
/// - Default → OpenAI
pub fn get_provider_config(model: &str, config: &Config) -> Option<ProviderConfig> {
    let model_lower = model.to_lowercase();

    if model_lower.starts_with("anthropic/") || model_lower.contains("claude") {
        // Anthropic Claude models
        config.anthropic_api_key.as_ref().map(|key| ProviderConfig {
            endpoint: "https://api.anthropic.com/v1/messages".to_string(),
            api_key: Some(key.clone()),
            is_anthropic: true,
            is_vertex_ai: false,
            model_name: model.trim_start_matches("anthropic/").to_string(),
        })
    } else if model_lower.starts_with("cerebras/") || model_lower.contains("llama") {
        // Cerebras (Llama models)
        config.cerebras_api_key.as_ref().map(|key| ProviderConfig {
            endpoint: "https://api.cerebras.ai/v1/chat/completions".to_string(),
            api_key: Some(key.clone()),
            is_anthropic: false,
            is_vertex_ai: false,
            model_name: model.trim_start_matches("cerebras/").to_string(),
        })
    } else if model_lower.starts_with("google/") || model_lower.contains("gemini") {
        // Vertex AI (Google Gemini models)
        config.google_cloud_project.as_ref().map(|project| {
            // Gemini 3 models require global region
            let is_gemini_3 = model_lower.contains("gemini-3");
            let region = if is_gemini_3 {
                "global"
            } else {
                &config.google_cloud_region
            };

            // Vertex AI expects model in "google/model-name" format
            let model_name = if model_lower.starts_with("google/") {
                model.to_string()
            } else {
                format!("google/{}", model)
            };

            // Global endpoint has different URL format (no region prefix)
            let endpoint = if is_gemini_3 {
                format!(
                    "https://aiplatform.googleapis.com/v1/projects/{}/locations/global/endpoints/openapi/chat/completions",
                    project
                )
            } else {
                format!(
                    "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/endpoints/openapi/chat/completions",
                    region, project, region
                )
            };

            ProviderConfig {
                endpoint,
                api_key: None, // Vertex AI uses OAuth2, not API keys
                is_anthropic: false,
                is_vertex_ai: true,
                model_name,
            }
        })
    } else if model_lower.starts_with("xai/") || model_lower.contains("grok") {
        // xAI (Grok models)
        config.xai_api_key.as_ref().map(|key| ProviderConfig {
            endpoint: "https://api.x.ai/v1/chat/completions".to_string(),
            api_key: Some(key.clone()),
            is_anthropic: false,
            is_vertex_ai: false,
            model_name: model.trim_start_matches("xai/").to_string(),
        })
    } else {
        // Default to OpenAI
        config.openai_api_key.as_ref().map(|key| ProviderConfig {
            endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            api_key: Some(key.clone()),
            is_anthropic: false,
            is_vertex_ai: false,
            model_name: model.trim_start_matches("openai/").to_string(),
        })
    }
}

/// Calculate cost from usage data based on model pricing
///
/// Pricing per 1K tokens (approximate)
pub fn calculate_cost(model: &str, prompt_tokens: u32, completion_tokens: u32) -> f64 {
    let model_lower = model.to_lowercase();

    let (input_cost_per_1k, output_cost_per_1k) = if model_lower.contains("llama") {
        // Cerebras Llama pricing (very cheap)
        (0.0001, 0.0001)
    } else if model_lower.contains("claude-sonnet-4") || model_lower.contains("claude-3-5-sonnet") {
        // Claude Sonnet 4 / 3.5 ($3/$15 per million)
        (0.003, 0.015)
    } else if model_lower.contains("claude-opus-4") || model_lower.contains("claude-3-opus") {
        // Claude Opus 4 ($15/$75 per million)
        (0.015, 0.075)
    } else if model_lower.contains("claude-haiku-4") || model_lower.contains("claude-3-haiku") {
        // Claude Haiku 4.5 / 3.5 ($1/$5 per million)
        (0.001, 0.005)
    } else if model_lower.contains("gpt-4o-mini") {
        // GPT-4o mini
        (0.00015, 0.0006)
    } else if model_lower.contains("gpt-4o") {
        // GPT-4o
        (0.005, 0.015)
    } else if model_lower.contains("gpt-4-turbo") {
        // GPT-4 Turbo
        (0.01, 0.03)
    } else if model_lower.contains("gpt-4") {
        // GPT-4
        (0.03, 0.06)
    } else if model_lower.contains("gpt-3.5") {
        // GPT-3.5 Turbo
        (0.0005, 0.0015)
    } else if model_lower.contains("gemini") {
        // Google Gemini (approximate)
        (0.00025, 0.0005)
    } else if model_lower.contains("grok") {
        // xAI Grok (approximate)
        (0.005, 0.015)
    } else {
        // Default fallback (conservative estimate)
        (0.005, 0.015)
    };

    let input_cost = (prompt_tokens as f64 / 1000.0) * input_cost_per_1k;
    let output_cost = (completion_tokens as f64 / 1000.0) * output_cost_per_1k;

    input_cost + output_cost
}

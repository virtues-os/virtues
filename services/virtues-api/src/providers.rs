//! Provider Configuration
//!
//! Simplified provider handling - all requests go through Vercel AI Gateway.
//! The gateway handles routing to providers (OpenAI, Anthropic, Google, etc.)
//! based on the model name prefix (e.g., "anthropic/claude-sonnet-4.5").

use crate::config::Config;

/// Provider configuration for making LLM requests
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// API endpoint URL (Vercel AI Gateway)
    pub endpoint: String,
    /// API key for the gateway
    pub api_key: String,
    /// Model name to send (passed through as-is)
    pub model_name: String,
}

/// Get provider configuration - always routes to Vercel AI Gateway
///
/// Model names should be in provider/model format:
/// - `anthropic/claude-sonnet-4.5`
/// - `openai/gpt-4o`
/// - `google/gemini-2.5-pro`
/// - `xai/grok-3`
pub fn get_provider_config(model: &str, config: &Config) -> ProviderConfig {
    ProviderConfig {
        endpoint: format!("{}/v1/chat/completions", config.ai_gateway_url),
        api_key: config.ai_gateway_api_key.clone(),
        model_name: model.to_string(),
    }
}

/// Get embeddings endpoint configuration
pub fn get_embeddings_config(config: &Config) -> ProviderConfig {
    ProviderConfig {
        endpoint: format!("{}/v1/embeddings", config.ai_gateway_url),
        api_key: config.ai_gateway_api_key.clone(),
        model_name: String::new(),
    }
}

/// Calculate cost from token usage. FALLBACK ONLY.
///
/// The gateway's `usage.cost` is authoritative and covers nearly every call
/// (see `routes/ai.rs::extract_cost_micros`). We land here only when it's
/// absent — older endpoints, non-Vercel upstreams, some embeddings responses.
///
/// Prices come from the live gateway catalog. If the catalog is cold (a fresh
/// process that has never reached the gateway) we use `FALLBACK_PRICING`, which
/// is deliberately expensive: in a blind spot, over-charge visibly rather than
/// under-charge silently.
///
/// There is no per-model price table here anymore, and there must never be one
/// again — the last one under-billed image generation by 13× because nobody
/// remembered to add a row. See `catalog.rs`.
pub fn calculate_cost(
    catalog: &crate::catalog::Catalog,
    model: &str,
    prompt_tokens: u32,
    completion_tokens: u32,
) -> f64 {
    let (input_cost_per_1k, output_cost_per_1k) = catalog.pricing(model).unwrap_or_else(|| {
        tracing::warn!(
            model,
            "no catalog pricing — billing at the fallback floor (catalog cold or model unknown)"
        );
        virtues_registry::models::FALLBACK_PRICING
    });

    let input_cost = (prompt_tokens as f64 / 1000.0) * input_cost_per_1k;
    let output_cost = (completion_tokens as f64 / 1000.0) * output_cost_per_1k;

    input_cost + output_cost
}

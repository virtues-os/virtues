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

/// Calculate cost from usage data based on model pricing.
///
/// This is the FALLBACK only — Vercel AI Gateway's `usage.cost` field is the
/// authoritative source for billing (see `routes/ai.rs::extract_cost_micros`).
/// We reach here just when the gateway omits `cost`. Pricing comes from the
/// single shared registry (`virtues_registry::models::get_model_pricing`),
/// which returns rates per 1K tokens — no second table to drift out of sync.
pub fn calculate_cost(model: &str, prompt_tokens: u32, completion_tokens: u32) -> f64 {
    let (input_cost_per_1k, output_cost_per_1k) =
        virtues_registry::models::get_model_pricing(model);

    let input_cost = (prompt_tokens as f64 / 1000.0) * input_cost_per_1k;
    let output_cost = (completion_tokens as f64 / 1000.0) * output_cost_per_1k;

    input_cost + output_cost
}

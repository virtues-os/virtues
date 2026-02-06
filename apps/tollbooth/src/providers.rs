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

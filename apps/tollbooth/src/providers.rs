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
/// Pricing sourced from Vercel AI Gateway /v1/models endpoint (Feb 2026).
/// Rates are per 1K tokens. More specific patterns must come before general ones.
pub fn calculate_cost(model: &str, prompt_tokens: u32, completion_tokens: u32) -> f64 {
    let model_lower = model.to_lowercase();

    let (input_cost_per_1k, output_cost_per_1k) =
        // ── Anthropic ──────────────────────────────────────────────
        if model_lower.contains("claude-opus-4.5") || model_lower.contains("claude-opus-4.6") {
            // Claude Opus 4.5/4.6 ($5/$25 per million)
            (0.005, 0.025)
        } else if model_lower.contains("claude-opus-4") || model_lower.contains("claude-3-opus") {
            // Claude Opus 4/4.1, 3-Opus ($15/$75 per million)
            (0.015, 0.075)
        } else if model_lower.contains("claude-sonnet") || model_lower.contains("claude-3-5-sonnet")
            || model_lower.contains("claude-3.5-sonnet") || model_lower.contains("claude-3.7-sonnet")
        {
            // Claude Sonnet 4/4.5, 3.5/3.7 ($3/$15 per million)
            (0.003, 0.015)
        } else if model_lower.contains("claude-haiku-4") {
            // Claude Haiku 4.5 ($1/$5 per million)
            (0.001, 0.005)
        } else if model_lower.contains("claude-3.5-haiku") {
            // Claude 3.5 Haiku ($0.80/$4 per million)
            (0.0008, 0.004)
        } else if model_lower.contains("claude-3-haiku") {
            // Claude 3 Haiku ($0.25/$1.25 per million)
            (0.00025, 0.00125)
        }

        // ── OpenAI ─────────────────────────────────────────────────
        else if model_lower.contains("gpt-4.1-nano") {
            // GPT-4.1 Nano ($0.10/$0.40 per million)
            (0.0001, 0.0004)
        } else if model_lower.contains("gpt-4.1-mini") {
            // GPT-4.1 Mini ($0.40/$1.60 per million)
            (0.0004, 0.0016)
        } else if model_lower.contains("gpt-4.1") {
            // GPT-4.1 ($2/$8 per million)
            (0.002, 0.008)
        } else if model_lower.contains("gpt-4o-mini") {
            // GPT-4o Mini ($0.15/$0.60 per million)
            (0.00015, 0.0006)
        } else if model_lower.contains("gpt-4o") {
            // GPT-4o ($2.50/$10 per million)
            (0.0025, 0.01)
        } else if model_lower.contains("gpt-4-turbo") {
            // GPT-4 Turbo ($10/$30 per million)
            (0.01, 0.03)
        } else if model_lower.contains("gpt-3.5") {
            // GPT-3.5 Turbo ($0.50/$1.50 per million)
            (0.0005, 0.0015)
        }

        // ── Google ─────────────────────────────────────────────────
        else if model_lower.contains("gemini-3-flash") {
            // Gemini 3 Flash ($0.50/$3 per million)
            (0.0005, 0.003)
        } else if model_lower.contains("gemini-3-pro") {
            // Gemini 3 Pro ($2/$12 per million)
            (0.002, 0.012)
        } else if model_lower.contains("gemini-2.5-flash-lite") {
            // Gemini 2.5 Flash-Lite ($0.10/$0.40 per million)
            (0.0001, 0.0004)
        } else if model_lower.contains("gemini-2.5-flash") {
            // Gemini 2.5 Flash ($0.30/$2.50 per million)
            (0.0003, 0.0025)
        } else if model_lower.contains("gemini-2.5-pro") {
            // Gemini 2.5 Pro ($1.25/$10 per million)
            (0.00125, 0.01)
        } else if model_lower.contains("gemini-2.0-flash-lite") {
            // Gemini 2.0 Flash-Lite ($0.075/$0.30 per million)
            (0.000075, 0.0003)
        } else if model_lower.contains("gemini") {
            // Other Gemini (fallback to 2.5 Flash pricing)
            (0.0003, 0.0025)
        }

        // ── xAI ────────────────────────────────────────────────────
        else if model_lower.contains("grok-3-mini") || model_lower.contains("grok-4-fast") {
            // Grok 3 Mini / Grok 4 Fast ($0.30/$0.50 per million)
            (0.0003, 0.0005)
        } else if model_lower.contains("grok") {
            // Grok 3/4 ($3/$15 per million)
            (0.003, 0.015)
        }

        // ── DeepSeek ───────────────────────────────────────────────
        else if model_lower.contains("deepseek-r1") {
            // DeepSeek R1 ($0.50/$2.15 per million)
            (0.0005, 0.00215)
        } else if model_lower.contains("deepseek") {
            // DeepSeek V3.x ($0.27/$0.40 per million)
            (0.00027, 0.0004)
        }

        // ── Mistral ────────────────────────────────────────────────
        else if model_lower.contains("mistral-large") || model_lower.contains("pixtral-large") {
            // Mistral/Pixtral Large ($0.50/$1.50 per million)
            (0.0005, 0.0015)
        } else if model_lower.contains("mistral-small") || model_lower.contains("codestral") {
            // Mistral Small / Codestral ($0.10/$0.30 per million)
            (0.0001, 0.0003)
        } else if model_lower.contains("mistral") {
            // Other Mistral (fallback to medium pricing)
            (0.0004, 0.002)
        }

        // ── Meta Llama ─────────────────────────────────────────────
        else if model_lower.contains("llama-4-scout") {
            // Llama 4 Scout ($0.08/$0.30 per million)
            (0.00008, 0.0003)
        } else if model_lower.contains("llama-4-maverick") {
            // Llama 4 Maverick ($0.15/$0.60 per million)
            (0.00015, 0.0006)
        } else if model_lower.contains("llama") {
            // Other Llama ($0.40/$0.40 per million)
            (0.0004, 0.0004)
        }

        // ── Default ────────────────────────────────────────────────
        else {
            // Unknown model — conservative fallback ($3/$15 per million)
            (0.003, 0.015)
        };

    let input_cost = (prompt_tokens as f64 / 1000.0) * input_cost_per_1k;
    let output_cost = (completion_tokens as f64 / 1000.0) * output_cost_per_1k;

    input_cost + output_cost
}

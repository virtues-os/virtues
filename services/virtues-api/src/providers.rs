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

/// The body sent upstream, built in exactly one place for the streaming and
/// non-streaming paths (they used to be two near-copies that drifted).
///
/// What this does NOT do, on purpose:
/// - It never invents `max_tokens`. For three months this filled in 4096 when
///   the caller sent none, which put a hard ceiling on every live chat turn
///   the box had deliberately left uncapped, on a model that counts its
///   thinking inside that ceiling. Absent means the model's own window.
/// - It never forwards `thought_signature`. See the wire crate.
///
/// What it still does, for now: default `temperature` to 0.7. The box sends
/// none on chat turns and has run at 0.7 since the proxy existed; dropping
/// the default here would move every chat to the provider's 1.0 in a cloud
/// deploy nobody can see from the box. The box starts sending its own
/// temperature in plan phase 2d; this default goes in the deploy after that.
pub fn upstream_body(
    req: &virtues_ai_wire::ChatCompletionRequest,
    upstream_model: &str,
    stream: bool,
    enforce_zdr: bool,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": upstream_model,
        "messages": req.messages,
        "temperature": req.temperature.unwrap_or(0.7),
    });
    if let Some(mt) = req.max_tokens {
        body["max_tokens"] = serde_json::json!(mt);
    }
    if stream {
        body["stream"] = serde_json::json!(true);
        body["stream_options"] = serde_json::json!({ "include_usage": true });
    }
    if let Some(ref effort) = req.reasoning_effort {
        body["reasoning_effort"] = serde_json::json!(effort);
    }
    if let Some(ref reasoning) = req.reasoning {
        body["reasoning"] = serde_json::to_value(reasoning).unwrap_or_default();
    }
    // Only include tools if present and non-empty (providers reject null/empty arrays)
    if let Some(ref tools) = req.tools {
        if !tools.is_empty() {
            body["tools"] = serde_json::json!(tools);
            if let Some(ref choice) = req.tool_choice {
                body["tool_choice"] = choice.clone();
            }
        }
    }
    // The caller's provider options, with zero-retention pinned over them
    // wherever the catalog says the model has ZDR routes. See
    // Catalog::enforce_zdr for why this is per-request and never a setting.
    if let Some(po) = virtues_ai_wire::merge_provider_options(req.provider_options.clone(), enforce_zdr) {
        body["providerOptions"] = po;
    }
    body
}

/// Calculate cost from token usage. FALLBACK ONLY. `None` means we do not know.
///
/// The gateway's `usage.cost` is authoritative and is present on every call, on
/// both the streaming and non-streaming paths (verified 2026-07-28 across both
/// providers). We land here only when it's somehow absent — an older endpoint,
/// a non-Vercel upstream, a BYOK response.
///
/// Prices come from the live gateway catalog. If the catalog is cold (a fresh
/// process that has never reached the gateway) or the model is unknown, this
/// returns `None` and **the caller must not charge**.
///
/// There used to be a `FALLBACK_PRICING` floor here — a deliberately expensive
/// invented rate, on the reasoning that over-charging is visible and refundable
/// while under-charging is silent. That is margin logic, and it is the wrong
/// trade on a consumer product: a customer billed a number we made up has no
/// way to know, and "refundable" only helps the ones who check. We always know
/// the tokens and the model, so the rate is knowable — just not yet. Eat the
/// cost of the blind spot rather than guessing at the user's expense, and log
/// loudly so the blind spot doesn't stay quiet.
///
/// There is no per-model price table here, and there must never be one again —
/// the last one under-billed image generation by 13× because nobody remembered
/// to add a row. See `catalog.rs`.
pub fn calculate_cost(
    catalog: &crate::catalog::Catalog,
    model: &str,
    prompt_tokens: u32,
    completion_tokens: u32,
) -> Option<f64> {
    let Some((input_cost_per_1k, output_cost_per_1k)) = catalog.pricing(model) else {
        tracing::error!(
            model,
            prompt_tokens,
            completion_tokens,
            catalog_cold = catalog.is_cold(),
            "no gateway cost and no catalog price — serving this call UNBILLED. \
             We eat it rather than invent a rate."
        );
        return None;
    };

    let input_cost = (prompt_tokens as f64 / 1000.0) * input_cost_per_1k;
    let output_cost = (completion_tokens as f64 / 1000.0) * output_cost_per_1k;

    Some(input_cost + output_cost)
}

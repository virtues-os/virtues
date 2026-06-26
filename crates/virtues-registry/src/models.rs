//! Model registry - LLM providers and their capabilities
//!
//! Models are static configuration - users cannot add new LLM providers.
//! They can only enable/disable models via user preferences.

use serde::{Deserialize, Serialize};

/// Model configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelConfig {
    /// Unique model identifier (e.g., "google/gemini-3-flash")
    pub model_id: String,
    /// Human-readable display name
    pub display_name: String,
    /// Provider name (e.g., "Anthropic", "Google", "OpenAI")
    pub provider: String,
    /// Sort order for UI display
    pub sort_order: i32,
    /// Whether this model is enabled
    pub enabled: bool,
    /// Context window size in tokens
    pub context_window: i32,
    /// Maximum output tokens
    pub max_output_tokens: i32,
    /// Whether the model supports tool/function calling
    pub supports_tools: bool,
    /// Whether the model can accept image inputs (vision)
    #[serde(default)]
    pub supports_vision: bool,
    /// Whether the model can read PDF / document inputs
    #[serde(default)]
    pub supports_pdf: bool,
    /// Whether the model can accept audio inputs
    #[serde(default)]
    pub supports_audio: bool,
    /// Whether this is the default model
    #[serde(default)]
    pub is_default: bool,
    /// Pricing per 1K input tokens (for virtues-api billing)
    #[serde(default)]
    pub input_cost_per_1k: Option<f64>,
    /// Pricing per 1K output tokens (for virtues-api billing)
    #[serde(default)]
    pub output_cost_per_1k: Option<f64>,
}

/// Model slot types for user preferences
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelSlot {
    /// Default chat model - used for general conversations
    Chat,
    /// Fast/lite model - used for titles, summaries, background jobs
    Lite,
    /// Reasoning model - used for complex analysis and thinking
    Reasoning,
    /// Coding model - used for code generation and technical tasks
    Coding,
}

impl ModelSlot {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelSlot::Chat => "chat",
            ModelSlot::Lite => "lite",
            ModelSlot::Reasoning => "reasoning",
            ModelSlot::Coding => "coding",
        }
    }
}

/// Get default model configurations
/// These are the 4 slot defaults available via Vercel AI Gateway
pub fn default_models() -> Vec<ModelConfig> {
    vec![
        // CHAT: Default conversational model.
        // Claude Opus over GLM-5: GLM-5 is a reasoning model that runs a
        // (non-streamed, ~6s) chain-of-thought before every turn, which stacks
        // across the agent's tool-call rounds into 20s+ stalls in chat. Opus
        // answers directly — no reasoning pass, streams immediately — and
        // handles parallel tool calls cleanly. (Gemini 3 stays out: via the
        // gateway's OpenAI-compatible endpoint it 400s on parallel tool calls,
        // needing a thought_signature the gateway doesn't pass through; see
        // vercel/ai #11590/#10344. GLM 5 / 5.1 remain available as the Lite/
        // Reasoning slots for users who want them.)
        ModelConfig {
            model_id: "anthropic/claude-opus-4.8".to_string(),
            display_name: "Claude Opus 4.8".to_string(),
            provider: "Anthropic".to_string(),
            sort_order: 1,
            enabled: true,
            context_window: 200000,
            max_output_tokens: 32000,
            supports_tools: true,
            supports_vision: true,
            supports_pdf: true,
            supports_audio: false,
            is_default: true,
            // Advisory only — Vercel AI Gateway's `usage.cost` is authoritative
            // for billing (see virtues-api's ai.rs). Kept for picker display.
            input_cost_per_1k: Some(0.015),
            output_cost_per_1k: Some(0.075),
        },
        // LITE: Fast model for background tasks (titles, summaries)
        ModelConfig {
            model_id: "zai/glm-4.7-flash".to_string(),
            display_name: "GLM 4.7 Flash".to_string(),
            provider: "Z.AI".to_string(),
            sort_order: 2,
            enabled: true,
            context_window: 203000,
            max_output_tokens: 131000,
            supports_tools: true,
            supports_vision: false,
            supports_pdf: false,
            supports_audio: false,
            is_default: false,
            input_cost_per_1k: Some(0.0003),
            output_cost_per_1k: Some(0.001),
        },
        // REASONING: Complex analysis and thinking
        ModelConfig {
            model_id: "zai/glm-5.1".to_string(),
            display_name: "GLM 5.1".to_string(),
            provider: "Z.AI".to_string(),
            sort_order: 3,
            enabled: true,
            context_window: 203000,
            max_output_tokens: 131000,
            supports_tools: true,
            supports_vision: false,
            supports_pdf: false,
            supports_audio: false,
            is_default: false,
            input_cost_per_1k: Some(0.0012),
            output_cost_per_1k: Some(0.0035),
        },
        // (CODING slot also defaults to Opus 4.8 — see default_model_for_slot();
        // no separate catalog entry, since the picker is a flat list of distinct
        // models and a duplicate model_id breaks its keyed render.)
        // ───────────────────────────────────────────────────────────────────
        // Additional curated models (selectable in the picker; not slot defaults).
        // Capability flags are hand-maintained here — the long-term plan is to
        // enrich these from the Vercel AI Gateway model catalog (which carries
        // modality + pricing metadata) so they stay current without a release.
        //
        // NOTE: model_ids must match the gateway's catalog exactly or requests
        // 404 — verify any newly added id against the live gateway.
        // ───────────────────────────────────────────────────────────────────
        // Anthropic Sonnet — faster / cheaper than Opus, same vision + PDF.
        ModelConfig {
            model_id: "anthropic/claude-sonnet-4.6".to_string(),
            display_name: "Claude Sonnet 4.6".to_string(),
            provider: "Anthropic".to_string(),
            sort_order: 5,
            enabled: true,
            context_window: 200000,
            max_output_tokens: 64000,
            supports_tools: true,
            supports_vision: true,
            supports_pdf: true,
            supports_audio: false,
            is_default: false,
            input_cost_per_1k: Some(0.003),
            output_cost_per_1k: Some(0.015),
        },
        // Gemini 2.5 Pro — full multimodal in (image + PDF + AUDIO); tool calls
        // work via the gateway (unlike Gemini 3, below). Closes the audio gap.
        ModelConfig {
            model_id: "google/gemini-2.5-pro".to_string(),
            display_name: "Gemini 2.5 Pro".to_string(),
            provider: "Google".to_string(),
            sort_order: 6,
            enabled: true,
            context_window: 1000000,
            max_output_tokens: 65000,
            supports_tools: true,
            supports_vision: true,
            supports_pdf: true,
            supports_audio: true,
            is_default: false,
            input_cost_per_1k: Some(0.00125),
            output_cost_per_1k: Some(0.01),
        },
        // Gemini 2.5 Flash — fast, cheap, full multimodal in.
        ModelConfig {
            model_id: "google/gemini-2.5-flash".to_string(),
            display_name: "Gemini 2.5 Flash".to_string(),
            provider: "Google".to_string(),
            sort_order: 7,
            enabled: true,
            context_window: 1000000,
            max_output_tokens: 65000,
            supports_tools: true,
            supports_vision: true,
            supports_pdf: true,
            supports_audio: true,
            is_default: false,
            input_cost_per_1k: Some(0.0003),
            output_cost_per_1k: Some(0.0025),
        },
        // Gemini 3 Pro — newest Google flagship, full multimodal in. CAVEAT: via
        // the gateway's OpenAI-compatible endpoint it 400s on parallel tool calls
        // (needs a thought_signature the gateway doesn't pass; vercel/ai
        // #11590/#10344) — fine for multimodal Q&A, shaky for tool-heavy agent
        // turns. Enabled but not a slot default.
        ModelConfig {
            model_id: "google/gemini-3-pro".to_string(),
            display_name: "Gemini 3 Pro".to_string(),
            provider: "Google".to_string(),
            sort_order: 8,
            enabled: true,
            context_window: 1000000,
            max_output_tokens: 65000,
            supports_tools: true,
            supports_vision: true,
            supports_pdf: true,
            supports_audio: true,
            is_default: false,
            input_cost_per_1k: Some(0.002),
            output_cost_per_1k: Some(0.012),
        },
        // OpenAI GPT-5.1 — strong general model, vision + PDF in. (Audio input is
        // a separate realtime surface, not this chat path.)
        ModelConfig {
            model_id: "openai/gpt-5.1".to_string(),
            display_name: "GPT-5.1".to_string(),
            provider: "OpenAI".to_string(),
            sort_order: 9,
            enabled: true,
            context_window: 400000,
            max_output_tokens: 128000,
            supports_tools: true,
            supports_vision: true,
            supports_pdf: true,
            supports_audio: false,
            is_default: false,
            input_cost_per_1k: Some(0.00125),
            output_cost_per_1k: Some(0.01),
        },
    ]
}

/// Get the default model ID for a given slot
pub fn default_model_for_slot(slot: ModelSlot) -> &'static str {
    match slot {
        ModelSlot::Chat => "anthropic/claude-opus-4.8",
        ModelSlot::Lite => "zai/glm-4.7-flash",
        ModelSlot::Reasoning => "zai/glm-5.1",
        ModelSlot::Coding => "anthropic/claude-opus-4.8",
    }
}

/// Pricing for a model by ID, as `(input_cost_per_1k, output_cost_per_1k)` USD.
///
/// THE single source of model pricing across the system:
///   - `virtues-api` uses it as the billing fallback when the Vercel AI Gateway
///     omits the authoritative `usage.cost` field (see `providers::calculate_cost`).
///   - the box uses it for on-box cost estimation (`chat_usage`, `rate_limit`).
///
/// Resolution order: exact match on a curated default model (its displayed
/// price) → per-family pattern match (most-specific first) → conservative
/// fallback ($5/$15 per 1M, i.e. never under-charge on an unknown model).
pub fn get_model_pricing(model_id: &str) -> (f64, f64) {
    // 1. Curated default models keep their exact (displayed) price.
    if let Some(model) = default_models().iter().find(|m| m.model_id == model_id) {
        return (
            model.input_cost_per_1k.unwrap_or(0.005),
            model.output_cost_per_1k.unwrap_or(0.015),
        );
    }

    // 2. Per-family pattern table. Rates per 1K tokens; most-specific first
    //    (e.g. gpt-4o-mini before gpt-4o before gpt-4).
    let m = model_id.to_lowercase();
    if m.contains("gemini-2.5") {
        (0.00125, 0.010) // Gemini 2.5 Pro
    } else if m.contains("gemini-3-flash") || m.contains("gemini-3.5") {
        (0.000075, 0.0003) // Gemini 3/3.5 Flash
    } else if m.contains("gemini") {
        (0.00015, 0.0006) // other Gemini
    } else if m.contains("claude-opus") {
        (0.015, 0.075) // Claude Opus
    } else if m.contains("claude-3-5-haiku") || m.contains("claude-haiku") {
        (0.0008, 0.004) // Claude Haiku
    } else if m.contains("claude") {
        (0.003, 0.015) // Claude Sonnet / other Claude
    } else if m.contains("gpt-4o-mini") {
        (0.00015, 0.0006)
    } else if m.contains("gpt-4o") {
        (0.0025, 0.010)
    } else if m.contains("gpt-4-turbo") {
        (0.010, 0.030)
    } else if m.contains("gpt-4") {
        (0.030, 0.060)
    } else if m.contains("o1") {
        (0.015, 0.060) // o1 reasoning
    } else if m.contains("gpt-3.5") {
        (0.0005, 0.0015)
    } else {
        (0.005, 0.015) // conservative fallback ($5/$15 per 1M)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_models() {
        let models = default_models();
        assert!(!models.is_empty(), "Models should not be empty");

        // Verify all models have context windows
        for model in &models {
            assert!(
                model.context_window > 0,
                "Model {} should have context_window",
                model.model_id
            );
            assert!(
                model.max_output_tokens > 0,
                "Model {} should have max_output_tokens",
                model.model_id
            );
        }

        // Verify exactly one default model
        let default_count = models.iter().filter(|m| m.is_default).count();
        assert_eq!(default_count, 1, "Should have exactly one default model");
    }

    #[test]
    fn test_all_models_have_pricing() {
        let models = default_models();
        for model in &models {
            assert!(
                model.input_cost_per_1k.is_some(),
                "Model {} should have input pricing",
                model.model_id
            );
            assert!(
                model.output_cost_per_1k.is_some(),
                "Model {} should have output pricing",
                model.model_id
            );
        }
    }

    #[test]
    fn test_get_model_pricing() {
        // Curated default model — exact match wins (displayed price).
        assert_eq!(get_model_pricing("google/gemini-3-flash"), (0.0001, 0.0004));

        // Unknown model — conservative fallback.
        assert_eq!(get_model_pricing("unknown/model"), (0.005, 0.015));
    }

    #[test]
    fn test_get_model_pricing_patterns() {
        // Per-family pattern table (rates per 1K tokens), most-specific first.
        assert_eq!(get_model_pricing("google/gemini-2.5-pro"), (0.00125, 0.010));
        assert_eq!(get_model_pricing("anthropic/claude-opus-4.1"), (0.015, 0.075));
        assert_eq!(get_model_pricing("anthropic/claude-3-5-haiku"), (0.0008, 0.004));
        assert_eq!(get_model_pricing("anthropic/claude-sonnet-4"), (0.003, 0.015));
        // gpt-4o-mini must win over gpt-4o / gpt-4.
        assert_eq!(get_model_pricing("openai/gpt-4o-mini"), (0.00015, 0.0006));
        assert_eq!(get_model_pricing("openai/gpt-4o"), (0.0025, 0.010));
        assert_eq!(get_model_pricing("openai/gpt-4-turbo"), (0.010, 0.030));
    }
}

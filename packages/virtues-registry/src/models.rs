//! Model registry - LLM providers and their capabilities
//!
//! Models are static configuration - users cannot add new LLM providers.
//! They can only enable/disable models via user preferences.

use serde::{Deserialize, Serialize};

/// Model configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelConfig {
    /// Unique model identifier (e.g., "anthropic/claude-sonnet-4-20250514")
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
    /// Whether this is the default model
    #[serde(default)]
    pub is_default: bool,
    /// Pricing per 1K input tokens (for Tollbooth billing)
    #[serde(default)]
    pub input_cost_per_1k: Option<f64>,
    /// Pricing per 1K output tokens (for Tollbooth billing)
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
        // CHAT: Default conversational model
        ModelConfig {
            model_id: "google/gemini-3-flash".to_string(),
            display_name: "Gemini 3 Flash".to_string(),
            provider: "Google".to_string(),
            sort_order: 1,
            enabled: true,
            context_window: 1000000,
            max_output_tokens: 8192,
            supports_tools: true,
            is_default: true,
            input_cost_per_1k: Some(0.0001),
            output_cost_per_1k: Some(0.0004),
        },
        // LITE: Fast model for background tasks
        ModelConfig {
            model_id: "zai/glm-4.7-flashx".to_string(),
            display_name: "GLM 4.7 FlashX".to_string(),
            provider: "Zhipu".to_string(),
            sort_order: 2,
            enabled: true,
            context_window: 128000,
            max_output_tokens: 8192,
            supports_tools: true,
            is_default: false,
            input_cost_per_1k: Some(0.0001),
            output_cost_per_1k: Some(0.0004),
        },
        // REASONING: Complex analysis and thinking
        ModelConfig {
            model_id: "google/gemini-3-pro-preview".to_string(),
            display_name: "Gemini 3 Pro".to_string(),
            provider: "Google".to_string(),
            sort_order: 3,
            enabled: true,
            context_window: 1000000,
            max_output_tokens: 65536,
            supports_tools: true,
            is_default: false,
            input_cost_per_1k: Some(0.00125),
            output_cost_per_1k: Some(0.005),
        },
        // CODING: Code generation and technical tasks
        ModelConfig {
            model_id: "anthropic/claude-opus-4.5".to_string(),
            display_name: "Claude Opus 4.5".to_string(),
            provider: "Anthropic".to_string(),
            sort_order: 4,
            enabled: true,
            context_window: 200000,
            max_output_tokens: 32000,
            supports_tools: true,
            is_default: false,
            input_cost_per_1k: Some(0.015),
            output_cost_per_1k: Some(0.075),
        },
    ]
}

/// Get the default model ID for a given slot
pub fn default_model_for_slot(slot: ModelSlot) -> &'static str {
    match slot {
        ModelSlot::Chat => "google/gemini-3-flash",
        ModelSlot::Lite => "zai/glm-4.7-flashx",
        ModelSlot::Reasoning => "google/gemini-3-pro-preview",
        ModelSlot::Coding => "anthropic/claude-opus-4.5",
    }
}

/// Get pricing for a model by ID
/// Returns (input_cost_per_1k, output_cost_per_1k)
/// Falls back to default pricing if model not found
pub fn get_model_pricing(model_id: &str) -> (f64, f64) {
    let models = default_models();

    // Try exact match first
    if let Some(model) = models.iter().find(|m| m.model_id == model_id) {
        return (
            model.input_cost_per_1k.unwrap_or(0.005),
            model.output_cost_per_1k.unwrap_or(0.015),
        );
    }

    // Try to match by model name (without provider prefix)
    let model_lower = model_id.to_lowercase();
    for model in &models {
        if model_lower.contains(&model.model_id.to_lowercase())
            || model.model_id.to_lowercase().contains(&model_lower)
        {
            return (
                model.input_cost_per_1k.unwrap_or(0.005),
                model.output_cost_per_1k.unwrap_or(0.015),
            );
        }
    }

    // Default fallback pricing
    (0.005, 0.015)
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
        let (input, output) = get_model_pricing("anthropic/claude-sonnet-4-20250514");
        assert!(input > 0.0);
        assert!(output > 0.0);

        // Test fallback
        let (input, output) = get_model_pricing("unknown/model");
        assert_eq!(input, 0.005);
        assert_eq!(output, 0.015);
    }
}

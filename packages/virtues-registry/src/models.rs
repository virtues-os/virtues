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

/// Get default model configurations
pub fn default_models() -> Vec<ModelConfig> {
    vec![
        // Anthropic models - DISABLED (thought_signature issues with tool calls)
        ModelConfig {
            model_id: "anthropic/claude-sonnet-4-20250514".to_string(),
            display_name: "Claude Sonnet 4".to_string(),
            provider: "Anthropic".to_string(),
            sort_order: 2,
            enabled: false,
            context_window: 200000,
            max_output_tokens: 8192,
            supports_tools: true,
            is_default: false,
            input_cost_per_1k: Some(0.003),
            output_cost_per_1k: Some(0.015),
        },
        ModelConfig {
            model_id: "anthropic/claude-opus-4-20250514".to_string(),
            display_name: "Claude Opus 4".to_string(),
            provider: "Anthropic".to_string(),
            sort_order: 3,
            enabled: false,
            context_window: 200000,
            max_output_tokens: 8192,
            supports_tools: true,
            is_default: false,
            input_cost_per_1k: Some(0.015),
            output_cost_per_1k: Some(0.075),
        },
        ModelConfig {
            model_id: "anthropic/claude-haiku-4-5-20251001".to_string(),
            display_name: "Claude Haiku 4.5".to_string(),
            provider: "Anthropic".to_string(),
            sort_order: 4,
            enabled: false,
            context_window: 200000,
            max_output_tokens: 8192,
            supports_tools: true,
            is_default: false,
            input_cost_per_1k: Some(0.0008),
            output_cost_per_1k: Some(0.004),
        },
        // Google models - DISABLED (thought_signature issues with tool calls)
        ModelConfig {
            model_id: "google/gemini-2.5-pro-preview-05-06".to_string(),
            display_name: "Gemini 2.5 Pro".to_string(),
            provider: "Google".to_string(),
            sort_order: 5,
            enabled: false,
            context_window: 1000000,
            max_output_tokens: 8192,
            supports_tools: true,
            is_default: false,
            input_cost_per_1k: Some(0.00125),
            output_cost_per_1k: Some(0.005),
        },
        ModelConfig {
            model_id: "google/gemini-2.5-flash".to_string(),
            display_name: "Gemini 2.5 Flash".to_string(),
            provider: "Google".to_string(),
            sort_order: 1,
            enabled: false,
            context_window: 1000000,
            max_output_tokens: 8192,
            supports_tools: true,
            is_default: false,
            input_cost_per_1k: Some(0.000075),
            output_cost_per_1k: Some(0.0003),
        },
        // OpenAI models - DISABLED
        ModelConfig {
            model_id: "openai/gpt-4o".to_string(),
            display_name: "GPT-4o".to_string(),
            provider: "OpenAI".to_string(),
            sort_order: 7,
            enabled: false,
            context_window: 128000,
            max_output_tokens: 16384,
            supports_tools: true,
            is_default: false,
            input_cost_per_1k: Some(0.0025),
            output_cost_per_1k: Some(0.01),
        },
        ModelConfig {
            model_id: "openai/gpt-4o-mini".to_string(),
            display_name: "GPT-4o Mini".to_string(),
            provider: "OpenAI".to_string(),
            sort_order: 8,
            enabled: false,
            context_window: 128000,
            max_output_tokens: 16384,
            supports_tools: true,
            is_default: false,
            input_cost_per_1k: Some(0.00015),
            output_cost_per_1k: Some(0.0006),
        },
        // xAI models - DISABLED
        ModelConfig {
            model_id: "xai/grok-3".to_string(),
            display_name: "Grok 3".to_string(),
            provider: "xAI".to_string(),
            sort_order: 9,
            enabled: false,
            context_window: 128000,
            max_output_tokens: 16384,
            supports_tools: true,
            is_default: false,
            input_cost_per_1k: Some(0.003),
            output_cost_per_1k: Some(0.015),
        },
        // Cerebras models - ENABLED (fast inference, no thought_signature issues)
        ModelConfig {
            model_id: "cerebras/gpt-oss-120b".to_string(),
            display_name: "GPT-OSS 120B (Cerebras)".to_string(),
            provider: "Cerebras".to_string(),
            sort_order: 1,
            enabled: true,
            context_window: 32768,
            max_output_tokens: 8192,
            supports_tools: true,
            is_default: true,
            input_cost_per_1k: Some(0.0006),
            output_cost_per_1k: Some(0.0006),
        },
        ModelConfig {
            model_id: "cerebras/zai-glm-4.7".to_string(),
            display_name: "ZAI GLM 4.7 (Cerebras)".to_string(),
            provider: "Cerebras".to_string(),
            sort_order: 2,
            enabled: true,
            context_window: 32768,
            max_output_tokens: 8192,
            supports_tools: true,
            is_default: false,
            input_cost_per_1k: Some(0.0006),
            output_cost_per_1k: Some(0.0006),
        },
    ]
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

//! Model registry - uses shared virtues-registry crate
//!
//! Single source of truth for available models across Tollbooth.
//! Models are loaded from the shared virtues-registry crate.

use std::collections::HashMap;
use std::sync::OnceLock;

// Re-export ModelConfig from shared registry
pub use virtues_registry::models::ModelConfig;

/// Global model registry
static MODELS: OnceLock<ModelRegistry> = OnceLock::new();

/// Local model entry with additional Tollbooth-specific fields
#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub model_id: String,
    pub display_name: String,
    pub provider: String,
    pub sort_order: i32,
    pub enabled: bool,
    pub context_window: i32,
    pub max_output_tokens: i32,
    pub supports_tools: bool,
    pub is_default: bool,
    pub input_cost_per_1k: Option<f64>,
    pub output_cost_per_1k: Option<f64>,
}

impl From<ModelConfig> for ModelEntry {
    fn from(config: ModelConfig) -> Self {
        Self {
            model_id: config.model_id,
            display_name: config.display_name,
            provider: config.provider,
            sort_order: config.sort_order,
            enabled: config.enabled,
            context_window: config.context_window,
            max_output_tokens: config.max_output_tokens,
            supports_tools: config.supports_tools,
            is_default: config.is_default,
            input_cost_per_1k: config.input_cost_per_1k,
            output_cost_per_1k: config.output_cost_per_1k,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelRegistry {
    pub models: Vec<ModelEntry>,
    pub by_id: HashMap<String, ModelEntry>,
}

impl ModelRegistry {
    /// Build registry from shared virtues-registry crate
    pub fn from_registry() -> Self {
        let models: Vec<ModelEntry> = virtues_registry::models::default_models()
            .into_iter()
            .map(ModelEntry::from)
            .collect();

        let by_id: HashMap<String, ModelEntry> = models
            .iter()
            .cloned()
            .map(|m| (m.model_id.clone(), m))
            .collect();

        Self { models, by_id }
    }

    /// Get enabled models filtered by provider availability
    pub fn get_enabled_models(&self, config: &crate::config::Config) -> Vec<&ModelEntry> {
        self.models
            .iter()
            .filter(|m| m.enabled)
            .filter(|m| {
                let provider = m.provider.to_lowercase();
                match provider.as_str() {
                    "google" => config.google_cloud_project.is_some(),
                    "anthropic" => config.anthropic_api_key.is_some(),
                    "openai" => config.openai_api_key.is_some(),
                    "cerebras" => config.cerebras_api_key.is_some(),
                    "xai" => config.xai_api_key.is_some(),
                    _ => false, // Unknown provider, skip
                }
            })
            .collect()
    }

    /// Get pricing for a model (input_cost_per_1k, output_cost_per_1k)
    pub fn get_pricing(&self, model_id: &str) -> (f64, f64) {
        // Try exact match first
        if let Some(model) = self.by_id.get(model_id) {
            return (
                model.input_cost_per_1k.unwrap_or(0.005),
                model.output_cost_per_1k.unwrap_or(0.015),
            );
        }

        // Try to match by model name (without provider prefix)
        let model_lower = model_id.to_lowercase();
        for model in &self.models {
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
}

/// Initialize the global model registry
pub fn init(_config_path: Option<&str>) -> anyhow::Result<()> {
    // Config path is now ignored - always use shared registry
    let registry = ModelRegistry::from_registry();

    tracing::info!("Loaded {} models from virtues-registry", registry.models.len());

    MODELS
        .set(registry)
        .map_err(|_| anyhow::anyhow!("Model registry already initialized"))?;

    Ok(())
}

/// Get the global model registry
pub fn get() -> &'static ModelRegistry {
    MODELS.get().expect("Model registry not initialized - call init() first")
}

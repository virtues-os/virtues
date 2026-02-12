//! Model registry - uses shared virtues-registry crate
//!
//! Single source of truth for available models across Tollbooth.
//! Models are loaded from the shared virtues-registry crate.

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
    pub enabled: bool,
    pub context_window: i32,
    pub max_output_tokens: i32,
    pub supports_tools: bool,
    pub input_cost_per_1k: Option<f64>,
    pub output_cost_per_1k: Option<f64>,
}

impl From<ModelConfig> for ModelEntry {
    fn from(config: ModelConfig) -> Self {
        Self {
            model_id: config.model_id,
            display_name: config.display_name,
            provider: config.provider,
            enabled: config.enabled,
            context_window: config.context_window,
            max_output_tokens: config.max_output_tokens,
            supports_tools: config.supports_tools,
            input_cost_per_1k: config.input_cost_per_1k,
            output_cost_per_1k: config.output_cost_per_1k,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelRegistry {
    pub models: Vec<ModelEntry>,
}

impl ModelRegistry {
    /// Build registry from shared virtues-registry crate
    pub fn from_registry() -> Self {
        let models: Vec<ModelEntry> = virtues_registry::models::default_models()
            .into_iter()
            .map(ModelEntry::from)
            .collect();

        Self { models }
    }

    /// Get enabled models - all are available via AI Gateway
    pub fn get_enabled_models(&self, config: &crate::config::Config) -> Vec<&ModelEntry> {
        // With Vercel AI Gateway, all enabled models are available if the gateway is configured
        if config.has_llm_provider() {
            self.models.iter().filter(|m| m.enabled).collect()
        } else {
            Vec::new()
        }
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

//! Model registry loaded from config/seeds/models.json
//!
//! Single source of truth for available models across the app.
//! The JSON config is embedded at compile time to ensure consistency.

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::sync::OnceLock;

/// Embedded models config from config/seeds/models.json (compile-time)
const EMBEDDED_MODELS_JSON: &str = include_str!("../../../config/seeds/models.json");

/// Global model registry
static MODELS: OnceLock<ModelRegistry> = OnceLock::new();

#[derive(Debug, Clone, Deserialize)]
pub struct ModelsConfig {
    pub version: String,
    pub models: Vec<ModelEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelEntry {
    pub model_id: String,
    pub display_name: String,
    pub provider: String,
    pub sort_order: u32,
    pub enabled: bool,
    pub context_window: u32,
    pub max_output_tokens: u32,
    pub supports_tools: bool,
    #[serde(default)]
    pub is_default: bool,
    /// Pricing per 1K input tokens (optional, uses defaults if not set)
    #[serde(default)]
    pub input_cost_per_1k: Option<f64>,
    /// Pricing per 1K output tokens (optional, uses defaults if not set)
    #[serde(default)]
    pub output_cost_per_1k: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct ModelRegistry {
    pub models: Vec<ModelEntry>,
    pub by_id: HashMap<String, ModelEntry>,
}

impl ModelRegistry {
    /// Load models from JSON file (allows runtime override of embedded config)
    pub fn load_from_file(path: &str) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;
        Self::load_from_str(&content)
    }

    /// Get default registry from embedded JSON (compile-time single source of truth)
    pub fn default() -> Self {
        Self::load_from_str(EMBEDDED_MODELS_JSON)
            .expect("Embedded models.json is invalid - this is a build-time error")
    }

    /// Load models from a JSON string
    fn load_from_str(json: &str) -> anyhow::Result<Self> {
        let config: ModelsConfig = serde_json::from_str(json)?;

        let by_id: HashMap<String, ModelEntry> = config
            .models
            .iter()
            .cloned()
            .map(|m| (m.model_id.clone(), m))
            .collect();

        Ok(Self {
            models: config.models,
            by_id,
        })
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
pub fn init(config_path: Option<&str>) -> anyhow::Result<()> {
    let registry = match config_path {
        Some(path) => {
            tracing::info!("Loading models from: {}", path);
            ModelRegistry::load_from_file(path)?
        }
        None => {
            tracing::info!("Using default model registry");
            ModelRegistry::default()
        }
    };

    tracing::info!("Loaded {} models", registry.models.len());

    MODELS
        .set(registry)
        .map_err(|_| anyhow::anyhow!("Model registry already initialized"))?;

    Ok(())
}

/// Get the global model registry
pub fn get() -> &'static ModelRegistry {
    MODELS.get().expect("Model registry not initialized - call init() first")
}

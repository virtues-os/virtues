//! API functions for model management
//!
//! Models are read directly from the shared virtues-registry crate.
//! No SQLite tables needed for static model configuration.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Model information returned by API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub model_id: String,
    pub display_name: String,
    pub provider: String,
    pub enabled: bool,
    pub sort_order: i32,
    pub context_window: Option<i32>,
    pub max_output_tokens: Option<i32>,
    pub supports_tools: Option<bool>,
    pub is_default: Option<bool>,
}

impl From<virtues_registry::models::ModelConfig> for ModelInfo {
    fn from(config: virtues_registry::models::ModelConfig) -> Self {
        Self {
            model_id: config.model_id,
            display_name: config.display_name,
            provider: config.provider,
            enabled: config.enabled,
            sort_order: config.sort_order,
            context_window: Some(config.context_window),
            max_output_tokens: Some(config.max_output_tokens),
            supports_tools: Some(config.supports_tools),
            is_default: Some(config.is_default),
        }
    }
}

/// List all enabled models from the registry
pub async fn list_models() -> Result<Vec<ModelInfo>> {
    let models: Vec<ModelInfo> = virtues_registry::models::default_models()
        .into_iter()
        .filter(|m| m.enabled)
        .map(ModelInfo::from)
        .collect();

    Ok(models)
}

/// Get the default model (is_default = true)
pub async fn get_default_model() -> Result<ModelInfo> {
    let model = virtues_registry::models::default_models()
        .into_iter()
        .find(|m| m.enabled && m.is_default)
        .map(ModelInfo::from)
        .ok_or_else(|| Error::NotFound("No default model configured".to_string()))?;

    Ok(model)
}

/// Get a specific model by ID
pub async fn get_model(model_id: &str) -> Result<ModelInfo> {
    let model = virtues_registry::models::default_models()
        .into_iter()
        .find(|m| m.model_id == model_id)
        .map(ModelInfo::from)
        .ok_or_else(|| Error::NotFound(format!("Model not found: {}", model_id)))?;

    Ok(model)
}

/// Recommended models response with slot mappings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendedModelsResponse {
    pub data: Vec<ModelInfoWithSlot>,
    pub slots: SlotDefaults,
}

/// Model info with slot assignment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfoWithSlot {
    #[serde(flatten)]
    pub model: ModelInfo,
    pub slot: String,
    pub input_cost_per_1k: Option<f64>,
    pub output_cost_per_1k: Option<f64>,
}

/// Default model IDs for each slot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotDefaults {
    pub chat: String,
    pub lite: String,
    pub reasoning: String,
    pub coding: String,
}

/// List recommended models with slot assignments
pub async fn list_recommended_models() -> Result<RecommendedModelsResponse> {
    use virtues_registry::models::{default_model_for_slot, ModelSlot};

    let models: Vec<ModelInfoWithSlot> = virtues_registry::models::default_models()
        .into_iter()
        .map(|m| {
            // Determine which slot this model belongs to
            let slot = if m.model_id == default_model_for_slot(ModelSlot::Chat) {
                "chat"
            } else if m.model_id == default_model_for_slot(ModelSlot::Lite) {
                "lite"
            } else if m.model_id == default_model_for_slot(ModelSlot::Reasoning) {
                "reasoning"
            } else if m.model_id == default_model_for_slot(ModelSlot::Coding) {
                "coding"
            } else {
                "other"
            };

            ModelInfoWithSlot {
                input_cost_per_1k: m.input_cost_per_1k,
                output_cost_per_1k: m.output_cost_per_1k,
                model: ModelInfo::from(m),
                slot: slot.to_string(),
            }
        })
        .collect();

    let slots = SlotDefaults {
        chat: default_model_for_slot(ModelSlot::Chat).to_string(),
        lite: default_model_for_slot(ModelSlot::Lite).to_string(),
        reasoning: default_model_for_slot(ModelSlot::Reasoning).to_string(),
        coding: default_model_for_slot(ModelSlot::Coding).to_string(),
    };

    Ok(RecommendedModelsResponse { data: models, slots })
}

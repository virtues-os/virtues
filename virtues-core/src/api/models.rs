//! Model list served to the web app.
//!
//! Facts (prices, context windows, which ids actually exist) come from the live
//! Vercel AI Gateway catalog, fetched from virtues-api and cached by
//! `api::model_catalog`. Taste (which models we surface, which one fills each
//! slot) comes from `virtues-registry`. Nothing here is a hand-maintained
//! mirror — see `api::model_catalog` for what happened the last time it was.

use serde::{Deserialize, Serialize};

use crate::api::model_catalog::{self, CatalogModel, SlotMap};
use crate::error::{Error, Result};

/// Model information returned by API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub model_id: String,
    pub display_name: String,
    pub provider: String,
    pub enabled: bool,
    pub sort_order: i32,
    pub context_window: Option<i64>,
    pub max_output_tokens: Option<i64>,
    pub supports_tools: Option<bool>,
    pub supports_vision: Option<bool>,
    pub supports_pdf: Option<bool>,
    pub supports_audio: Option<bool>,
    pub is_default: Option<bool>,
    /// Live catalog price. `None` means we genuinely don't know (this box has
    /// never reached the cloud) — render blank, never `$0.00`.
    pub input_cost_per_1k: Option<f64>,
    pub output_cost_per_1k: Option<f64>,
}

impl From<CatalogModel> for ModelInfo {
    fn from(m: CatalogModel) -> Self {
        Self {
            model_id: m.model_id,
            display_name: m.display_name,
            provider: m.provider,
            // The catalog only ever carries models we curate AND the gateway
            // still serves — anything else has already been filtered out.
            enabled: true,
            sort_order: m.sort_order,
            context_window: Some(m.context_window),
            max_output_tokens: Some(m.max_output_tokens),
            supports_tools: Some(m.supports_tools),
            supports_vision: Some(m.supports_vision),
            supports_pdf: Some(m.supports_pdf),
            supports_audio: Some(m.supports_audio),
            is_default: Some(m.is_default),
            input_cost_per_1k: m.input_cost_per_1k,
            output_cost_per_1k: m.output_cost_per_1k,
        }
    }
}

/// The picker, plus what each slot currently resolves to.
///
/// `slots` powers the "Virtues default · Claude Opus 4.8" option: the user
/// picks *the slot*, not the model, and thereafter rides whatever we choose —
/// a cloud-side swap, no box release. Pinning a model in
/// `app_assistant_profile` always overrides it. Their choice wins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsResponse {
    pub data: Vec<ModelInfo>,
    pub slots: SlotMap,
}

/// List the curated models, hydrated with live catalog facts.
///
/// Dedupes by `model_id`: one model can fill several slots (Opus is both chat
/// and coding) and the picker's keyed `{#each}` throws on duplicate keys.
pub async fn list_models() -> Result<Vec<ModelInfo>> {
    let mut seen = std::collections::HashSet::new();
    Ok(model_catalog::models()
        .into_iter()
        .filter(|m| seen.insert(m.model_id.clone()))
        .map(ModelInfo::from)
        .collect())
}

/// The picker plus the slot map — what the model settings UI needs in one call.
pub async fn list_models_with_slots() -> Result<ModelsResponse> {
    Ok(ModelsResponse {
        data: list_models().await?,
        slots: model_catalog::slots(),
    })
}

/// Whatever the Chat slot currently resolves to. This is what "Virtues default"
/// means at this moment.
pub async fn get_default_model() -> Result<ModelInfo> {
    let chat = model_catalog::slots().chat;
    get_model(&chat).await
}

/// Get a specific model by ID
pub async fn get_model(model_id: &str) -> Result<ModelInfo> {
    model_catalog::models()
        .into_iter()
        .find(|m| m.model_id == model_id)
        .map(ModelInfo::from)
        .ok_or_else(|| Error::NotFound(format!("Model not found: {model_id}")))
}

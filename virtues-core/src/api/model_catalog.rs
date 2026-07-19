//! The box's view of the model catalog — fetched from virtues-api, never
//! compiled in.
//!
//! virtues-api keeps a live mirror of the Vercel AI Gateway catalog (prices,
//! context windows, which ids actually exist) and serves the curated subset at
//! `GET /v1/ai/models`, together with the slot map. This module caches that
//! response and is the box's ONLY source of model facts.
//!
//! # Slot resolution
//!
//!   1. the user's `app_assistant_profile` override — their choice always wins
//!   2. the slot map served here — so swapping the Lite model is a cloud
//!      change every box picks up within the refresh window, not a release
//!   3. `virtues_registry::models::default_model_for_slot` — the compiled floor,
//!      so a box that has never reached the cloud still boots with a model
//!
//! # Why not compile the prices in
//!
//! We did, and every one of them had rotted: Opus 3× over, GLM 4× over, image
//! generation 13× *under*, and two models in the picker that did not exist on
//! the gateway at all. Model ids churn faster than we ship boxes, and Vercel
//! publishes no deprecation notice. Never store a fact you can fetch.

use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::virtues_api::client::BearerClient;

/// The box refreshes 6-hourly. virtues-api itself polls the gateway hourly and
/// has already dropped anything that vanished, so this only bounds how long a
/// *box* lags a cloud-side model swap.
const REFRESH_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

/// One curated model, as virtues-api hydrated it: our taste, the gateway's
/// facts. Prices are `None` only when virtues-api's own catalog is cold.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogModel {
    pub model_id: String,
    pub display_name: String,
    pub provider: String,
    pub sort_order: i32,
    pub context_window: i64,
    pub max_output_tokens: i64,
    pub supports_tools: bool,
    #[serde(default)]
    pub supports_vision: bool,
    #[serde(default)]
    pub supports_pdf: bool,
    #[serde(default)]
    pub supports_audio: bool,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub input_cost_per_1k: Option<f64>,
    #[serde(default)]
    pub output_cost_per_1k: Option<f64>,
}

/// Which model fills each slot, per the cloud. Ids only — the models
/// themselves are in `data`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotMap {
    pub chat: String,
    pub lite: String,
    pub coding: String,
    pub image: String,
}

impl Default for SlotMap {
    /// The compiled floor. Used until the first successful fetch — a box with
    /// no cloud reach still has a working model for every slot.
    fn default() -> Self {
        use virtues_registry::models::{default_model_for_slot, ModelSlot};
        Self {
            chat: default_model_for_slot(ModelSlot::Chat).to_string(),
            lite: default_model_for_slot(ModelSlot::Lite).to_string(),
            coding: default_model_for_slot(ModelSlot::Coding).to_string(),
            image: default_model_for_slot(ModelSlot::Image).to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct CatalogResponse {
    data: Vec<CatalogModel>,
    #[serde(default)]
    slots: Option<SlotMap>,
}

#[derive(Default)]
struct Snapshot {
    models: Vec<CatalogModel>,
    slots: SlotMap,
    #[allow(dead_code)]
    fetched_at: Option<Instant>,
}

/// Process-wide cache. One box, one catalog.
static CACHE: std::sync::OnceLock<Arc<RwLock<Snapshot>>> = std::sync::OnceLock::new();

fn cache() -> &'static Arc<RwLock<Snapshot>> {
    CACHE.get_or_init(|| Arc::new(RwLock::new(Snapshot::default())))
}

/// The curated picker. Falls back to the compiled registry list — unhydrated,
/// no prices — if we have never reached the cloud.
pub fn models() -> Vec<CatalogModel> {
    if let Ok(s) = cache().read() {
        if !s.models.is_empty() {
            return s.models.clone();
        }
    }
    virtues_registry::models::default_models()
        .into_iter()
        .filter(|m| m.enabled)
        .map(|m| CatalogModel {
            model_id: m.model_id,
            display_name: m.display_name,
            provider: m.provider,
            sort_order: m.sort_order,
            context_window: m.context_window as i64,
            max_output_tokens: m.max_output_tokens as i64,
            supports_tools: m.supports_tools,
            supports_vision: m.supports_vision,
            supports_pdf: m.supports_pdf,
            supports_audio: m.supports_audio,
            is_default: m.is_default,
            // Honest: we do not know. A blank beats a confident lie.
            input_cost_per_1k: None,
            output_cost_per_1k: None,
        })
        .collect()
}

/// The slot map — cloud-served if we have it, compiled floor otherwise.
pub fn slots() -> SlotMap {
    cache()
        .read()
        .ok()
        .map(|s| s.slots.clone())
        .unwrap_or_default()
}

/// Resolve a slot to a model id. This is layer 2 of the resolution order — the
/// caller checks the user's override first.
pub fn model_for_slot(slot: virtues_registry::models::ModelSlot) -> String {
    use virtues_registry::models::{default_model_for_slot, ModelSlot};
    let s = slots();
    match slot {
        ModelSlot::Chat => s.chat,
        ModelSlot::Lite => s.lite,
        ModelSlot::Coding => s.coding,
        ModelSlot::Image => s.image,
        // Omni (audio transcription) is a fixed system model, not cloud- or
        // user-overridable — it must stay audio-capable — so it resolves from
        // the compiled registry floor rather than the cloud SlotMap.
        ModelSlot::Omni => default_model_for_slot(ModelSlot::Omni).to_string(),
    }
}

/// `(input_per_1k, output_per_1k)` from the live catalog, or None when we are
/// cold or the model is not curated. Callers must NOT substitute zero.
pub fn pricing(model_id: &str) -> Option<(f64, f64)> {
    let snap = cache().read().ok()?;
    let m = snap.models.iter().find(|m| m.model_id == model_id)?;
    Some((m.input_cost_per_1k?, m.output_cost_per_1k?))
}

async fn fetch(pool: &PgPool) -> crate::Result<CatalogResponse> {
    let client = BearerClient::from_env(pool.clone());
    let resp = client.get_json("/v1/ai/models").await?;
    serde_json::from_value(resp.body)
        .map_err(|e| crate::Error::Configuration(format!("model catalog parse: {e}")))
}

fn store(resp: CatalogResponse) {
    if let Ok(mut guard) = cache().write() {
        guard.models = resp.data;
        if let Some(s) = resp.slots {
            guard.slots = s;
        }
        guard.fetched_at = Some(Instant::now());
    }
}

/// Boot-time fetch + 6-hourly refresh.
///
/// A failed fetch is never fatal and never clears the cache: an unreachable
/// cloud must not empty the model picker. The box keeps the last snapshot it
/// saw, and falls back to the compiled registry list only if it has never seen
/// one at all.
pub fn spawn(pool: PgPool) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(REFRESH_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            // First tick fires immediately — that's the boot fetch.
            interval.tick().await;
            match fetch(&pool).await {
                Ok(resp) => {
                    tracing::debug!(count = resp.data.len(), "model catalog refreshed");
                    store(resp);
                }
                Err(e) => tracing::warn!(
                    "model catalog fetch failed: {e} — keeping last snapshot \
                     (picker falls back to the compiled list if we've never fetched)"
                ),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cold_cache_still_yields_a_picker_and_every_slot() {
        // A box that has never reached the cloud must still be usable.
        assert!(!models().is_empty());
        let s = slots();
        assert!(!s.chat.is_empty() && !s.lite.is_empty());
        assert!(!s.coding.is_empty() && !s.image.is_empty());
    }

    #[test]
    fn cold_cache_reports_no_pricing_rather_than_zero() {
        // Zero would be a free-money bug on the usage tab; None renders blank.
        assert!(pricing("anthropic/claude-opus-4.8").is_none());
        assert!(models().iter().all(|m| m.input_cost_per_1k.is_none()));
    }
}

//! The box's view of the model catalog — fetched from virtues-api, never
//! compiled in.
//!
//! virtues-api keeps a live mirror of the Vercel AI Gateway catalog (prices,
//! context windows, which ids actually exist) and serves the picker at
//! `GET /v1/ai/models` — every priced language model the gateway carries, with
//! the five slot models flagged `recommended: true` — together with the slot
//! map. This module caches that response and is the box's ONLY source of model
//! facts.
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

/// One picker entry, as virtues-api derived it from the gateway. Every field
/// here is the gateway's, not ours. Prices are `None` only when virtues-api's
/// own catalog is cold.
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
    /// `true` when this model fills one of our five slots — the entire set we
    /// vouch for. `false` for everything else the gateway carries, which is the
    /// BYO path: selectable, but its capability flags are the provider's own
    /// claim. The picker sections on this. Absent on older responses → `false`.
    #[serde(default)]
    pub recommended: bool,
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

/// The picker, as served by virtues-api. When we have never reached the cloud
/// this degrades to the five slot ids and nothing else.
///
/// That degradation is deliberate. We know which model we'd pick — that is the
/// compiled floor — but we know nothing else about it: no price, no context
/// window, no verified capabilities. Rendering the id with blank facts is
/// honest; rendering a hand-written description is how seven stale entries
/// survived four model generations. A blank beats a confident lie.
pub fn models() -> Vec<CatalogModel> {
    if let Ok(s) = cache().read() {
        if !s.models.is_empty() {
            return s.models.clone();
        }
    }
    use virtues_registry::models::{default_model_for_slot, ModelSlot};
    let chat = default_model_for_slot(ModelSlot::Chat);

    // The chat-facing slots only. Image and Omni are system slots — their
    // models are never picker options (Omni's is a Gemini 3 model, which 400s
    // on parallel tool calls and must not be offered for chat).
    let mut ids: Vec<String> = Vec::new();
    for slot in [ModelSlot::Chat, ModelSlot::Lite, ModelSlot::Coding] {
        let id = default_model_for_slot(slot).to_string();
        if !ids.contains(&id) {
            ids.push(id);
        }
    }

    ids.into_iter()
        .enumerate()
        .map(|(i, id)| {
            // `provider/model` — the only structure we can rely on offline.
            let (provider, name) = id.split_once('/').unwrap_or(("", id.as_str()));
            CatalogModel {
                is_default: id == chat,
                display_name: name.to_string(),
                provider: provider.to_string(),
                sort_order: i as i32,
                model_id: id,
                // Unknown, every one of them. Not zero — unknown.
                context_window: 0,
                max_output_tokens: 0,
                supports_tools: false,
                supports_vision: false,
                supports_pdf: false,
                supports_audio: false,
                input_cost_per_1k: None,
                output_cost_per_1k: None,
                // These are the slot models, which is what `recommended` means.
                recommended: true,
            }
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

/// Which slot, if any, currently resolves to this model id — the inverse of
/// [`model_for_slot`].
///
/// Exists for the BYO fork. A model id is an *address on a specific gateway*,
/// not a name: `xai/grok-4.5` is where our gateway keeps the chat model, and
/// means nothing on someone else's. Callers build bodies with our address, so
/// the fork has to turn it back into the role it stands for before it can look
/// up the user's address for that role.
///
/// `None` when the id is not a slot default — which is the case for a user who
/// pinned an arbitrary model from the picker. Nothing to translate then: their
/// choice is passed through, and a route that does not carry it fails loudly.
pub fn slot_for_model(model_id: &str) -> Option<virtues_registry::models::ModelSlot> {
    use virtues_registry::models::ModelSlot;
    ModelSlot::all()
        .into_iter()
        .find(|slot| model_for_slot(*slot) == model_id)
}

/// Whether a model can read images, per the live catalog. `None` when we are
/// cold or the model is unknown.
///
/// `None` is not `false`, for the same reason `pricing` refuses to answer zero.
/// The compiled floor sets every capability flag to `false` because it knows
/// nothing, and a caller that reads that as a real "no" would silently
/// downgrade a vision-capable model on any box that has not reached the cloud
/// yet. Callers must distinguish "cannot" from "do not know" and say which.
pub fn supports_vision(model_id: &str) -> Option<bool> {
    let snap = cache().read().ok()?;
    if snap.models.is_empty() {
        return None;
    }
    let m = snap.models.iter().find(|m| m.model_id == model_id)?;
    Some(m.supports_vision)
}

/// `(input_per_1k, output_per_1k)` from the live catalog, or None when we are
/// cold or the model is unknown. Callers must NOT substitute zero.
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

/// Live check of the one fact the tool-attachment path depends on.
///   cargo test -p virtues --lib api::model_catalog::live -- --ignored --nocapture
#[cfg(test)]
mod live {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn the_chat_slot_model_can_actually_read_images() {
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://virtues:virtues@localhost:5432/virtues".to_string());
        let pool = PgPool::connect(&url).await.expect("dev database");

        let resp = match fetch(&pool).await {
            Ok(r) => r,
            Err(e) => {
                println!("catalog unreachable ({e}) — cannot verify the vision gate here");
                return;
            }
        };
        println!("catalog: {} models", resp.data.len());
        store(resp);

        let chat = model_for_slot(virtues_registry::models::ModelSlot::Chat);
        println!("chat slot: {chat}");
        for m in models() {
            println!(
                "  {:<45} vision={:<5} pdf={:<5} audio={:<5} tools={}",
                m.model_id, m.supports_vision, m.supports_pdf, m.supports_audio, m.supports_tools
            );
        }

        match supports_vision(&chat) {
            Some(true) => println!("\nOK: read_asset attachments will reach the model"),
            Some(false) => panic!("chat slot {chat} cannot read images — read_asset is inert"),
            None => panic!("chat slot {chat} is absent from the catalog — gate reads as cannot"),
        }
    }
}

//! Model registry — the models we *choose*, not the models that *exist*.
//!
//! # The line
//!
//!   FACTS come from the gateway.   Does this model exist? What does it cost?
//!                                  Context window, max output. These change
//!                                  without us and we must never mirror them.
//!                                  → `GET {gateway}/v1/models`, refreshed
//!                                    hourly by virtues-api (`catalog.rs`).
//!
//!   TASTE comes from here.         Which models we surface in the picker,
//!                                  which one fills each slot, and the
//!                                  capability caveats we learned the hard way
//!                                  (see the Gemini-3 note below — the
//!                                  gateway's own tags won't tell you that).
//!
//!   MONEY comes from `usage.cost`. The gateway reports the authoritative cost
//!                                  of every call. We bill that, plus markup.
//!                                  The catalog is only the fallback when the
//!                                  field is absent; `FALLBACK_PRICING` below
//!                                  is the last resort when even that is cold.
//!
//! The rule that falls out: **never store a fact you can fetch.** This file
//! previously carried a hand-copied price table, and every entry in it had
//! drifted — Opus was 3× over, the image model 13× *under*, and two models in
//! the picker (`google/gemini-3-pro`, `openai/gpt-5.1`) did not exist on the
//! gateway at all. That failure mode is now structurally impossible: we don't
//! store prices, and `curated ⊆ catalog` is enforced at refresh and in CI.
//!
//! Vercel publishes no deprecation notice — a model id can simply stop
//! existing (precedent: Cohere Command R/R+). The intersection check is the
//! only warning we get, so it is a *runtime* invariant, not just a test.

use serde::{Deserialize, Serialize};

/// A model we have chosen to surface, and what we know about it that the
/// gateway's catalog does not say.
///
/// Deliberately carries NO pricing. Prices live in the gateway catalog; see
/// the module docs. Capability flags stay here because they encode our own
/// testing (e.g. "works via the gateway's OpenAI-compatible endpoint"), which
/// is a different claim than the provider's marketing.
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
}

/// Model slot types for user preferences
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelSlot {
    /// Default chat model - used for general conversations
    Chat,
    /// Fast/lite model - used for titles, summaries, background jobs
    Lite,
    /// Coding model - used for code generation and technical tasks
    Coding,
    /// Image model - text-to-image generation (the `generate_image` tool)
    Image,
}

impl ModelSlot {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelSlot::Chat => "chat",
            ModelSlot::Lite => "lite",
            ModelSlot::Coding => "coding",
            ModelSlot::Image => "image",
        }
    }
}

/// The curated picker — the models we have actually tried, in the order we
/// want them seen. Not a catalog: the gateway lists 300+ models, and this is
/// the handful we vouch for.
///
/// Invariant: every `model_id` here MUST exist in the gateway catalog. It is
/// checked at each catalog refresh (virtues-api drops + logs any that vanish)
/// and asserted in CI. Two entries once rotted here unnoticed —
/// `google/gemini-3-pro` (real id is `-preview`) and `openai/gpt-5.1` (never
/// existed; only `-codex`/`-instant`/`-thinking` ever did) — both 404'd for
/// anyone who selected them.
///
/// No pricing lives here. See the module docs.
pub fn default_models() -> Vec<ModelConfig> {
    vec![
        // CHAT + CODING: Default conversational model.
        // Claude Opus over GLM-5: GLM-5 is a reasoning model that runs a
        // (non-streamed, ~6s) chain-of-thought before every turn, which stacks
        // across the agent's tool-call rounds into 20s+ stalls in chat. Opus
        // answers directly — no reasoning pass, streams immediately — and
        // handles parallel tool calls cleanly.
        //
        // Gemini 3 stays OUT of the picker entirely: via the gateway's
        // OpenAI-compatible endpoint it 400s on parallel tool calls, needing a
        // thought_signature the gateway doesn't pass through (vercel/ai
        // #11590/#10344). Gemini 2.5 Pro covers Google multimodal — including
        // audio — and its tool calls actually work.
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
        },
        // LITE: Fast model for background tasks (titles, summaries, extraction).
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
        },
        // Selectable, not a slot default. Reasoning-heavy; slow first token.
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
        },
        // Anthropic Sonnet — faster / cheaper than Opus, same vision + PDF.
        ModelConfig {
            model_id: "anthropic/claude-sonnet-4.6".to_string(),
            display_name: "Claude Sonnet 4.6".to_string(),
            provider: "Anthropic".to_string(),
            sort_order: 4,
            enabled: true,
            context_window: 200000,
            max_output_tokens: 64000,
            supports_tools: true,
            supports_vision: true,
            supports_pdf: true,
            supports_audio: false,
            is_default: false,
        },
        // Gemini 2.5 Pro — full multimodal in (image + PDF + AUDIO); tool calls
        // work via the gateway (unlike Gemini 3). This is the audio path.
        ModelConfig {
            model_id: "google/gemini-2.5-pro".to_string(),
            display_name: "Gemini 2.5 Pro".to_string(),
            provider: "Google".to_string(),
            sort_order: 5,
            enabled: true,
            context_window: 1000000,
            max_output_tokens: 65000,
            supports_tools: true,
            supports_vision: true,
            supports_pdf: true,
            supports_audio: true,
            is_default: false,
        },
        // Gemini 2.5 Flash — fast, cheap, full multimodal in.
        ModelConfig {
            model_id: "google/gemini-2.5-flash".to_string(),
            display_name: "Gemini 2.5 Flash".to_string(),
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
        },
    ]
}

/// Get the default model ID for a given slot
pub fn default_model_for_slot(slot: ModelSlot) -> &'static str {
    match slot {
        ModelSlot::Chat => "anthropic/claude-opus-4.8",
        ModelSlot::Lite => "zai/glm-4.7-flash",
        ModelSlot::Coding => "anthropic/claude-opus-4.8",
        ModelSlot::Image => "google/gemini-3-pro-image",
    }
}

/// Every model id this build depends on: the curated picker plus the slot
/// defaults (which include ids the picker never shows, e.g. the Image slot).
///
/// This is the set that must exist in the gateway catalog. virtues-api checks
/// it on every refresh; CI checks it against the live gateway.
pub fn required_model_ids() -> Vec<String> {
    let mut ids: Vec<String> = default_models().into_iter().map(|m| m.model_id).collect();
    for slot in [
        ModelSlot::Chat,
        ModelSlot::Lite,
        ModelSlot::Coding,
        ModelSlot::Image,
    ] {
        let id = default_model_for_slot(slot).to_string();
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

/// Last-resort pricing, as `(input_cost_per_1k, output_cost_per_1k)` USD.
///
/// This is NOT a price table — it is a floor, and it is deliberately
/// expensive. It applies only when BOTH authoritative sources are unavailable:
/// the gateway omitted `usage.cost` on the response *and* the cached catalog
/// is cold (a fresh virtues-api that has never reached the gateway).
///
/// Set high on purpose: in that blind spot we would rather over-charge a user
/// by a few cents — visible, refundable — than silently under-charge and eat
/// unbounded cost. The old per-family pattern table that lived here did the
/// opposite: it under-charged image generation by 13× because nobody
/// remembered to add a row for the image model.
///
/// $5 / $15 per 1M tokens.
pub const FALLBACK_PRICING: (f64, f64) = (0.005, 0.015);

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

    /// Every slot default must be reachable — including ids the picker never
    /// shows (the Image slot has no ModelConfig entry, which is exactly how it
    /// went unpriced and under-billed by 13× for so long).
    #[test]
    fn required_ids_cover_every_slot() {
        let required = required_model_ids();
        for slot in [
            ModelSlot::Chat,
            ModelSlot::Lite,
            ModelSlot::Coding,
            ModelSlot::Image,
        ] {
            let id = default_model_for_slot(slot);
            assert!(
                required.contains(&id.to_string()),
                "slot {} default `{id}` missing from required_model_ids()",
                slot.as_str()
            );
        }
    }

    #[test]
    fn fallback_pricing_never_undercharges() {
        // The floor must sit at or above the most expensive model we curate,
        // so a cold-catalog blind spot can only ever over-charge.
        let (input, output) = FALLBACK_PRICING;
        assert!(input >= 0.005 && output >= 0.015);
    }

    /// The `curated ⊆ catalog` invariant — the one that would have caught
    /// `google/gemini-3-pro` and `openai/gpt-5.1` sitting in the picker,
    /// 404ing, for who knows how long — needs an HTTP client, and this crate is
    /// deliberately serde-only. It lives with the fetcher instead:
    /// `services/virtues-api/src/catalog.rs::curated_models_all_exist_on_the_gateway`.
    /// virtues-api also enforces it at runtime on every hourly refresh, which
    /// matters because Vercel ships no deprecation notice at all.
    #[test]
    fn no_pricing_is_stored_in_this_crate() {
        // A guard against the mirror growing back. If you find yourself wanting
        // to add a price here: don't. Fetch it. See the module docs.
        //
        // The needle is assembled at runtime so this test does not match its
        // own source, and only `default_models()` is scanned so `FALLBACK_PRICING`
        // (a floor, not a price) is exempt.
        let needle = format!("cost_per_1k{}", ": Some(");
        let src = include_str!("models.rs");
        let body = src
            .split("pub fn default_models()")
            .nth(1)
            .and_then(|s| s.split("pub fn required_model_ids()").next())
            .expect("default_models() not found");

        let offenders: Vec<&str> = body
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .filter(|l| l.contains(&needle))
            .collect();

        assert!(
            offenders.is_empty(),
            "hardcoded model pricing reintroduced into default_models(): {offenders:?}"
        );
    }
}

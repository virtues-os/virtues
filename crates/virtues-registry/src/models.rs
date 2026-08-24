//! Model slots — the five decisions we make on the user's behalf.
//!
//! # The line
//!
//!   FACTS come from the gateway.   Which models exist, what they cost, context
//!                                  windows, capabilities, display names — all
//!                                  of it, for every model. Fetched hourly by
//!                                  virtues-api (`catalog.rs`), never mirrored.
//!
//!   CURATION is five model ids.    One per slot, below. That is the entire
//!                                  surface on which we express taste. The
//!                                  picker is the gateway's list, unedited: a
//!                                  user who picks their own model has left the
//!                                  recommended path and is on the BYO path by
//!                                  definition.
//!
//!   MONEY comes from `usage.cost`. The gateway reports the authoritative cost
//!                                  of every call on both the streaming and
//!                                  non-streaming paths. We bill that, plus
//!                                  markup. Nothing here invents a price.
//!
//! # Why there is no model list here anymore
//!
//! This file used to carry a hand-written `Vec<ModelConfig>` describing seven
//! models: display name, provider, context window, max output, and four
//! capability booleans each. Every one of those is a gateway fact, and
//! `catalog.rs::all_selectable()` already derives all of them automatically for
//! the ~250 models we did *not* curate. Describing seven by hand bought nothing
//! and rotted: by 2026-07-28 every curated entry was at least a generation
//! behind — we listed `gemini-2.5-flash` while the gateway carried `3.6-flash`,
//! and `claude-opus-4.8` while it carried `opus-5`. The `curated ⊆ catalog`
//! guard stayed green throughout, because it checks existence, not currency.
//!
//! The lesson isn't "maintain the list better", it's that curation is the thing
//! that decays, so we curate less. What survives is the part that genuinely
//! cannot be fetched.
//!
//! # Slot resolution
//!
//!   1. the user's `app_assistant_profile` override — their choice always wins
//!   2. the cloud slot map served on `/v1/ai/models` — a swap without a *box*
//!      release; boxes pick it up within their 6-hourly refresh
//!   3. the compiled floor below — so a box with no cloud reach still boots
//!
//! Layer 3 stays compiled rather than seeded into SQL on purpose: a seeded row
//! is written once and then diverges forever, while a constant ships fresh with
//! every box upgrade. For a value whose whole job is to still be safe years
//! later, that difference is the point.
//!
//! **Layers 2 and 3 currently share this source.** virtues-api builds the slot
//! map it serves by calling `default_model_for_slot` below, so changing a slot
//! is a virtues-api deploy — no box release, but not a config toggle either.
//! Giving layer 2 its own store (a table in virtues-api) was considered and
//! deferred: it is a migration plus an admin surface to move five strings that
//! change a few times a year, and the lesson this file exists to record is that
//! unused machinery rots faster than it earns. Revisit if slot changes ever
//! become frequent enough to feel the deploy.

use serde::{Deserialize, Serialize};

/// Model slot types for user preferences.
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
    /// Omni model — audio-native multimodal UNDERSTANDING (the transcription
    /// pipeline): a verbatim transcript PLUS scene/mood/music/entities from raw
    /// audio. This is NOT plain speech-to-text: Whisper and `*-transcribe`
    /// endpoints emit words only and MUST NOT be assigned here — the model has
    /// to accept audio input and reason over it. Like Image, it's a system
    /// slot, not a user-facing picker entry.
    Omni,
}

impl ModelSlot {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelSlot::Chat => "chat",
            ModelSlot::Lite => "lite",
            ModelSlot::Coding => "coding",
            ModelSlot::Image => "image",
            ModelSlot::Omni => "omni",
        }
    }

    /// Every slot. Iterate this rather than re-listing the variants.
    pub fn all() -> [ModelSlot; 5] {
        [
            ModelSlot::Chat,
            ModelSlot::Lite,
            ModelSlot::Coding,
            ModelSlot::Image,
            ModelSlot::Omni,
        ]
    }
}

/// The compiled floor: which model fills each slot when we have no cloud reach.
///
/// These five ids are the whole of our curation. Promoting a model into a slot
/// is a real decision with a real failure mode, so verify it against the
/// gateway's OpenAI-compatible endpoint FIRST — the gateway's `tool-use` tag
/// describes the model, not the shim it is reached through. Run:
///
/// ```text
/// cargo test -p virtues --lib slot_model_smoke -- --ignored
/// ```
///
/// which drives the candidate with our real tool set: tool selection, valid
/// tool names, parseable arguments, and parallel calls in one turn.
///
/// The precedent is Gemini 3, kept out of the chat slots because it 400'd on
/// parallel tool calls — it wanted a `thought_signature` the gateway did not
/// forward (vercel/ai #11590/#10344). **That may no longer be true**: on
/// 2026-07-28 `gemini-3-flash` emitted parallel calls cleanly against our tool
/// set. The exclusion is now tracked by
/// `report_whether_the_gemini_3_exclusion_still_holds` rather than asserted
/// here, and wants re-evaluating before the next slot decision.
pub fn default_model_for_slot(slot: ModelSlot) -> &'static str {
    match slot {
        // Verified against the shim on 2026-07-28: all four legs pass, and it
        // beat Opus 4.8 on every one (2.1s vs 2.3s on the parallel-call turn).
        //
        // Accepted cost: Grok reasons on every turn (~180-200 tokens, measured
        // over 3 runs) and NO reasoning_effort value reduces it — none/minimal/
        // low/high all land in the same range. A fixed per-round tax we cannot
        // tune, taken because it is small and Grok is ~4x cheaper on output
        // than Opus. GLM-5.1 is the counterexample and why it is never a slot
        // default: ~300-460 reasoning tokens per turn, equally uncontrollable,
        // which stacks across an agent's tool rounds into 20s+ stalls in chat.
        //
        // Same model, new address since 2026-08-24: the gateway renamed the
        // provider slug `xai/` -> `spacexai/`, and the old id vanished from
        // the catalog rather than aliasing — requests to it 404.
        ModelSlot::Chat => "spacexai/grok-4.5",
        ModelSlot::Coding => "spacexai/grok-4.5",
        // Titles, summaries, background jobs. Measured 0 reasoning tokens at
        // every effort level — no thinking tax on the high-volume slot.
        ModelSlot::Lite => "zai/glm-4.7-flash",
        ModelSlot::Image => "google/gemini-3-pro-image",
        // The audio-native model that won a controlled 5-clip bench. Stays out
        // of the Gemini-3 parallel-tool-call problem entirely: transcription
        // uses no tools. Every audio-in model besides Gemini rejects audio on
        // the gateway, so this slot is effectively Gemini-only.
        ModelSlot::Omni => "google/gemini-3-flash",
    }
}

/// Every model id this build depends on — the five slot defaults, deduplicated.
///
/// This is the set that must exist in the gateway catalog: a slot default that
/// 404s is an outage, not a cosmetic problem. virtues-api checks it on every
/// hourly refresh; CI checks it against the live gateway. Nothing else needs
/// checking, because nothing else is ours.
pub fn required_model_ids() -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for slot in ModelSlot::all() {
        let id = default_model_for_slot(slot).to_string();
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every slot default must be a plausible gateway id. The live-existence
    /// check needs an HTTP client and lives with the fetcher:
    /// `services/virtues-api/src/catalog.rs::curated_models_all_exist_on_the_gateway`.
    #[test]
    fn every_slot_has_a_qualified_id() {
        for slot in ModelSlot::all() {
            let id = default_model_for_slot(slot);
            assert!(
                id.contains('/') && !id.starts_with('/') && !id.ends_with('/'),
                "slot {} default `{id}` is not a `provider/model` id",
                slot.as_str()
            );
        }
    }

    #[test]
    fn required_ids_cover_every_slot() {
        let required = required_model_ids();
        for slot in ModelSlot::all() {
            let id = default_model_for_slot(slot);
            assert!(
                required.contains(&id.to_string()),
                "slot {} default `{id}` missing from required_model_ids()",
                slot.as_str()
            );
        }
    }

    /// A guard against the mirror growing back. If you find yourself wanting to
    /// add a model list, a price, a context window, or a capability flag to
    /// this file: don't. Fetch it. See the module docs.
    #[test]
    fn no_model_facts_are_stored_in_this_crate() {
        let src = include_str!("models.rs");
        let body = src
            .split("mod tests")
            .next()
            .expect("test module marker not found");

        for needle in [
            "context_window",
            "max_output_tokens",
            "cost_per_1k",
            "supports_tools",
            "supports_vision",
            "display_name",
        ] {
            let offenders: Vec<&str> = body
                .lines()
                .filter(|l| !l.trim_start().starts_with("//!"))
                .filter(|l| !l.trim_start().starts_with("//"))
                .filter(|l| l.contains(needle))
                .collect();
            assert!(
                offenders.is_empty(),
                "gateway fact `{needle}` reintroduced into the registry: {offenders:?}"
            );
        }
    }
}

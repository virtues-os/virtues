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
    /// The cheap tier: titles, summaries, background jobs. Never a chat
    /// choice.
    Lite,
    /// The everyday tier: every conversation that does not ask for more, and
    /// the day pipeline's writers.
    ///
    /// Spelled `chat` before 2026-10-05. That name is still read where it was
    /// persisted or sent by an older peer — a BYO credential's model map, an
    /// applet manifest, the cloud slot map — via [`ModelSlot::legacy_name`].
    Standard,
    /// The strong tier: the rare, slow, high-stakes turns (deep research, a
    /// skill that declares `model: deep`). Picked by the mode, never guessed
    /// per turn — switching models mid-chat throws away the prompt cache that
    /// most of a chat's input tokens are served from.
    Deep,
    /// Image model - text-to-image generation (the `generate_image` tool)
    Image,
    /// Omni model — audio-native multimodal UNDERSTANDING (the transcription
    /// pipeline): a verbatim transcript PLUS scene/mood/music/entities from raw
    /// audio. This is NOT plain speech-to-text: Whisper and `*-transcribe`
    /// endpoints emit words only and MUST NOT be assigned here — the model has
    /// to accept audio input and reason over it. It must accept IMAGES too: the
    /// bookmark image pass (`bookmark_enrichment::enrich_image`) sends pixels
    /// here, so an audio-only model in this slot fails every saved screenshot.
    /// Like Image, it's a system slot, not a user-facing picker entry.
    Omni,
}

impl ModelSlot {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelSlot::Lite => "lite",
            ModelSlot::Standard => "standard",
            ModelSlot::Deep => "deep",
            ModelSlot::Image => "image",
            ModelSlot::Omni => "omni",
        }
    }

    /// The name this slot went by before the 2026-10-05 rename, where it had
    /// one. Read it wherever an older peer or a stored value may still use it;
    /// never write it.
    pub fn legacy_name(&self) -> Option<&'static str> {
        match self {
            ModelSlot::Standard => Some("chat"),
            _ => None,
        }
    }

    /// Parse a slot name, accepting the legacy spelling. `None` for anything
    /// else, including the retired `coding` slot.
    pub fn from_name(name: &str) -> Option<ModelSlot> {
        let name = name.trim().to_ascii_lowercase();
        ModelSlot::all()
            .into_iter()
            .find(|s| s.as_str() == name || s.legacy_name() == Some(name.as_str()))
    }

    /// Every slot. Iterate this rather than re-listing the variants.
    pub fn all() -> [ModelSlot; 5] {
        [
            ModelSlot::Lite,
            ModelSlot::Standard,
            ModelSlot::Deep,
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
/// SMOKE_MODEL=<candidate> cargo test -p virtues --lib slot_model_smoke -- --ignored
/// ```
///
/// `SMOKE_MODEL` is what lets you run it on a model that is not in this file
/// yet — without it the gate could only ever test the id already promoted,
/// which is the one moment the answer is not in question.
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
        // DeepSeek V4.1 Flash 1017, chosen 2026-10-22 on cost at about the
        // same quality as the models it was measured against. Chat spend is
        // almost all input (a 26-turn chat billed 2.0M input tokens against
        // 25k output), and this model's input is $0.07-0.30/M depending on
        // the route, against $2.00/M for grok-4.7 and Sonnet 5.5.
        //
        // Measured on a real box, the same questions against Sonnet 5.5:
        // $0.015 a turn against $0.052, 30s median against 21s, 5.4 steps a
        // turn against 3.0. It queries again where Sonnet answers, so it
        // sends about twice the input and is slower than its token rate
        // suggests. The Artificial Analysis index puts it at 45, grok-4.7 at
        // 46 and Sonnet 5.5 at 47-56.
        //
        // It wrote broken citation links until `agent::citations` gave the
        // model short handles to cite: 52 of 55 broken before, every link
        // resolving after. Before promoting another model to this slot, check
        // its links against real tool output, not only `slot_model_smoke`.
        //
        // `zdr: some`: virtues-api pins every request to its zero-retention
        // routes (`Catalog::enforce_zdr`). If that enforcement is ever
        // removed, re-check this slot first.
        //
        // Measured and not chosen:
        //   grok-4.7            the gateway now bills $2.00/M input and
        //                       $0.50/M cached, so on cached chats it costs
        //                       more than Sonnet 5.5 for the same quality.
        //   deepseek-v4-pro     index 36; fast only because it barely reasons.
        //   kimi-k2.6           its one zero-retention route runs ~47 tok/s.
        //   glm-5.3             3k reasoning tokens over 36s on a judgment turn.
        //   v4.1-flash-*-fast   Baseten is its only route.
        ModelSlot::Standard => "deepseek/deepseek-v4.1-flash-1017",
        // Opus 5.5, chosen 2026-10-05 for the slot's first callers, deep
        // research. `zdr: all` and `no_training: all`, $4/M input
        // and $20/M output — cheaper per token than the opus-5 that was turned
        // down for the Standard slot at "$55.01/mo, 9.3x the incumbent". That
        // verdict was about every unpinned turn; this slot runs only when a
        // mode asks for it. Reads PDFs, 1M context, and exposes effort
        // (low..max), so a caller can spend less thinking than the default.
        ModelSlot::Deep => "anthropic/claude-opus-5.5",
        // Titles, summaries, bookmark extraction, the query the web-search
        // tool composes, entity article drafts, compaction, and the day
        // article's per-sentence check. High volume, and some of its output
        // lands in the UI as a bare string, so output hygiene matters as much
        // as judgment.
        //
        // gpt-6-luna because the slot now carries judgment work, not only
        // strings. As the day article's checker, on one real day's draft:
        //
        //   glm-4.7-flash        22/38 sentences kept (deleted true ones)  14s
        //   gpt-6-luna, none     31/38                                      4s
        //   gpt-6-luna, low      33-36/38                                  17s
        //
        // and it still writes a clean title with thinking off, ~1.3s against
        // glm's 0.7s, at the same fraction of a cent. It exposes a reasoning
        // toggle and `none`..`max` effort, so each caller's `Thinking` decides
        // what it pays: a title asks for none, a check asks for low.
        //
        // `zdr: some` — zero-retention in practice because virtues-api sets
        // `zeroDataRetention` per request (Catalog::enforce_zdr). If that
        // enforcement is ever removed, re-check this slot's posture first.
        ModelSlot::Lite => "openai/gpt-6-luna",
        // Nano Banana 2 (the 3.1 generation), replacing gemini-3-pro-image on
        // 2026-09-15. Newer, and a quarter the price on the gateway's catalog
        // ($0.0005/$0.003 per 1k against $0.002/$0.012). Compared head to head
        // on the same painterly prompt: 3.1-flash lays paint on thickly —
        // palette-knife ridges, saturated — where 3-pro is subtler and more
        // atmospheric but holds a "leave this area quiet" instruction better.
        // The thick-paint character is what we want for the plates.
        ModelSlot::Image => "google/gemini-3.1-flash-image",
        // The audio-native model that won a controlled 5-clip bench. Stays out
        // of the Gemini-3 parallel-tool-call problem entirely: transcription
        // uses no tools. Every audio-in model besides Gemini rejects audio on
        // the gateway, so this slot is effectively Gemini-only.
        //
        // RE-VALIDATED 2026-08-27 on four real 5-minute recordings pulled from
        // a box, using the applet's own prompt and `reasoning_effort: "low"`.
        // It stays, and the freshness report will keep saying it is nine
        // versions behind, which is the wrong reading of these numbers:
        //
        //   gemini-3-flash    8.5s   0 reasoning    348 words   json 4/4
        //   gemini-3.5-flash  8.3s   0 reasoning    336 words   json 4/4
        //   gemini-3.6-flash 20.5s  1869 reasoning  340 words   json 4/4
        //   gemini-3.7-flash  8.2s    19 reasoning  241 words   json 4/4
        //
        // 3.6 is the one to keep out, and the gateway says why before you spend
        // a cent: its `reasoning_options` is null, so the `reasoning_effort`
        // the applet sends is silently ignored and it thinks 1,869 tokens per
        // clip at 2.4x the latency. That is the "only tier whose thinking
        // budget the gateway honors" line from the original bench, still true —
        // check `reasoning_options` before benching an Omni candidate.
        //
        // 3.7 honors the budget but transcribed ~30% FEWER words off the same
        // audio. The prompt says omissions are safe and fabrications are not,
        // which is right about the danger but does not make dropped speech
        // free: this slot is the sole record of a moment nobody re-listens to.
        //
        // 3.5 is a lateral move — no reason to churn the slot for it.
        ModelSlot::Omni => "google/gemini-3-flash",
    }
}

/// Every model id this build depends on — the five slot defaults, deduplicated.
///
/// This is the set that must exist in the gateway catalog: a slot default that
/// 404s is an outage, not a cosmetic problem. virtues-api checks it on every
/// hourly refresh; CI checks it against the live gateway. Nothing else needs
/// checking, because nothing else is ours.
/// What the catalog says about a model's thinking. Derived by virtues-api
/// from the gateway's `tags` and `reasoning_options`, served to boxes on the
/// picker, and read by the box's completion helper to turn a thinking mode
/// into a request.
///
/// This is the SHAPE of a fact, never a fact: every value is fetched, none is
/// written here (see the module docs and the guard test below). It lives in
/// this crate because it is the one crate both the box and the proxy already
/// compile against, and a picker entry has to deserialize on the box exactly
/// as it serialized on the proxy.
///
/// Every field is the gateway's claim about the model, not a measurement.
/// `can_disable` in particular comes from a `toggle` entry that the catalog
/// lists for Claude Fable 5, a model the gateway's own docs say cannot turn
/// thinking off. So it decides whether to *try* `enabled: false`, never
/// whether an output ceiling is safe.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ReasoningFacts {
    /// The model reasons at all: tagged `reasoning`, or lists any control.
    pub thinks: bool,
    /// A `toggle` control is listed, so `reasoning.enabled: false` may work.
    pub can_disable: bool,
    /// Allowed `effort` values, in the gateway's order. Empty: no lever.
    #[serde(default)]
    pub effort_values: Vec<String>,
    /// The `provider_options` that ask this model to RETURN its thinking
    /// text, computed by the proxy per provider family. Empty object when
    /// the family has no such switch.
    #[serde(default)]
    pub display_options: serde_json::Value,
}

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

    /// `chat` was the Standard slot's name until 2026-10-05, and it is still
    /// stored on boxes (BYO model maps, applet manifests). It must keep
    /// parsing, and `coding` — a slot nothing ever called — must not.
    #[test]
    fn slot_names_parse_with_the_legacy_spelling() {
        for slot in ModelSlot::all() {
            assert_eq!(ModelSlot::from_name(slot.as_str()), Some(slot));
        }
        assert_eq!(ModelSlot::from_name("chat"), Some(ModelSlot::Standard));
        assert_eq!(ModelSlot::from_name(" Standard "), Some(ModelSlot::Standard));
        assert_eq!(ModelSlot::from_name("coding"), None);
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

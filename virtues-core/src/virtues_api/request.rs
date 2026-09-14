//! The chat completion request as the box builds it.
//!
//! One struct for every site that posts to `/v1/ai/chat/completions`
//! (`agent/stream.rs`, `completion.rs`, `ai_complete.rs`), so the box has one
//! spelling of each field rather than five hand-built JSON bodies. Before
//! this, the sites disagreed with each other and with the proxy, and the
//! proxy dropped `provider_options` on every chat turn for three months.
//!
//! The proxy does not share this type, on purpose. It forwards the body as
//! opaque JSON and touches a handful of keys (`model`, `providerOptions`,
//! `stream_options`, `temperature`, the empty-tools guard); anything else the
//! box sends reaches the gateway untouched, so a field added here needs no
//! matching edit anywhere else. The gateway, not the proxy, is the judge of
//! what it accepts. See `services/virtues-api/src/providers.rs`.
//!
//! Everything optional is `skip_serializing_if` so an unset field is absent
//! on the wire rather than `null`: a `null` `max_tokens` is "no ceiling" to
//! one endpoint and a 400 to another.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A chat completion, as the box asks for it. Field names are the box's,
/// snake_case; the proxy spells `providerOptions` the gateway's way.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<Value>,
    /// A hard output ceiling. Almost never right to set: on a model that
    /// thinks, the thinking is counted inside it, and a ceiling sized for the
    /// answer alone returns nothing. Absent means the model's own window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    /// The gateway's effort alias (`low` | `medium` | `high` | …). Kept
    /// because it is also what a BYO OpenAI-compatible endpoint understands;
    /// [`Self::reasoning`] is the richer, gateway-only shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    /// The gateway's reasoning object. A gateway extension: the proxy
    /// forwards it, a BYO endpoint must never see it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Reasoning>,
    /// Provider-specific options in the gateway's `providerOptions` shape,
    /// keyed by provider (`anthropic`, `google`, `gateway`, …). The proxy
    /// merges its own `gateway.zeroDataRetention` into this; the caller
    /// cannot switch that off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_options: Option<Value>,
}

/// What a job wants from the model's reasoning. Never a token count: the
/// helper turns this plus the catalog's facts about the model into the
/// request, and the model's own window is the only ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Thinking {
    /// Arrangement, extraction, titles, summaries. Asks for thinking off
    /// where the model can be told; the lowest effort it lists otherwise.
    Off,
    /// Adjudication (the day detective). The `low` effort, or the lowest
    /// the model lists.
    Low,
    /// Leave the model's default. What live chat asks.
    Default,
    /// The `high` effort, or the highest the model lists.
    High,
}

/// The reasoning fields for a request: the gateway's object, and the
/// `reasoning_effort` alias a BYO endpoint understands.
///
/// `facts` is what the catalog says about the model, `None` when the box has
/// never reached the cloud or the proxy predates the field. `byo` means the
/// request goes to the owner's own endpoint, which never sees the gateway's
/// object. The rules, in order:
///
/// - BYO: the alias only, and only for `Low` and `High`. An Ollama or OpenAI
///   endpoint ignores what it does not know; the object is not sent to it.
/// - No facts: the alias only, same as BYO. Unknown means "cannot be told
///   off", which is the reading that cannot lose an answer.
/// - A model that does not think: nothing.
/// - `Off`: `enabled: false` when the catalog lists a toggle (a claim, not a
///   guarantee; Fable 5 lists one and ignores it), else the lowest effort
///   listed, else nothing.
/// - `Low` / `High`: the named effort when listed, else the lowest/highest
///   listed, else nothing. A toggle-only model keeps its default.
/// - `Default`: nothing.
pub fn reasoning_for(
    thinking: Thinking,
    facts: Option<&virtues_registry::ReasoningFacts>,
    byo: bool,
) -> (Option<Reasoning>, Option<String>) {
    let alias = |level: &str| (None, Some(level.to_string()));
    let facts = match (byo, facts) {
        (true, _) | (false, None) => {
            return match thinking {
                Thinking::Low => alias("low"),
                Thinking::High => alias("high"),
                Thinking::Off | Thinking::Default => (None, None),
            }
        }
        (false, Some(f)) => f,
    };
    if !facts.thinks {
        return (None, None);
    }
    let effort = |wanted: &str, fallback: Option<&String>| -> (Option<Reasoning>, Option<String>) {
        let pick = if facts.effort_values.iter().any(|v| v == wanted) {
            Some(wanted.to_string())
        } else {
            fallback.cloned()
        };
        match pick {
            Some(level) => (Some(Reasoning { effort: Some(level), ..Default::default() }), None),
            None => (None, None),
        }
    };
    match thinking {
        Thinking::Off if facts.can_disable => {
            (Some(Reasoning { enabled: Some(false), ..Default::default() }), None)
        }
        Thinking::Off => effort("__none__", facts.effort_values.first()),
        Thinking::Low => effort("low", facts.effort_values.first()),
        Thinking::High => effort("high", facts.effort_values.last()),
        Thinking::Default => (None, None),
    }
}

/// The gateway's per-request reasoning control (Chat Completions extension).
/// `effort` and `max_tokens` cannot be combined; the gateway rejects both.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Reasoning {
    /// Off when `Some(false)`. Some models cannot turn thinking off and
    /// ignore this; a false here is a request, never a guarantee.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// A thinking-token budget, for models whose catalog lists one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Omit reasoning text from the response. Does not stop the thinking.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unset_fields_are_absent_on_the_wire() {
        let req = ChatCompletionRequest {
            model: "provider/model".into(),
            messages: vec![json!({"role": "user", "content": "hi"})],
            ..Default::default()
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v.as_object().unwrap().keys().collect::<Vec<_>>(), vec!["messages", "model"]);
    }

    fn facts(thinks: bool, can_disable: bool, efforts: &[&str]) -> virtues_registry::ReasoningFacts {
        virtues_registry::ReasoningFacts {
            thinks,
            can_disable,
            effort_values: efforts.iter().map(|s| s.to_string()).collect(),
            display_options: json!({}),
        }
    }

    /// The four live catalog shapes (2026-09-08) against every mode.
    #[test]
    fn a_thinking_mode_becomes_what_the_model_can_take() {
        let sonnet = facts(true, true, &["low", "medium", "high", "xhigh"]);
        let glm = facts(true, true, &[]);
        let gemini = facts(true, false, &["minimal", "low", "medium", "high"]);
        let qwen = facts(false, false, &[]);
        let off = |f| reasoning_for(Thinking::Off, Some(f), false);
        let low = |f| reasoning_for(Thinking::Low, Some(f), false);
        let high = |f| reasoning_for(Thinking::High, Some(f), false);

        assert_eq!(off(&sonnet).0.unwrap().enabled, Some(false));
        assert_eq!(low(&sonnet).0.unwrap().effort.as_deref(), Some("low"));
        assert_eq!(high(&sonnet).0.unwrap().effort.as_deref(), Some("high"));
        assert_eq!(reasoning_for(Thinking::Default, Some(&sonnet), false), (None, None));

        assert_eq!(off(&glm).0.unwrap().enabled, Some(false));
        assert_eq!(low(&glm), (None, None), "toggle-only: Low keeps the default");

        assert_eq!(off(&gemini).0.unwrap().effort.as_deref(), Some("minimal"), "no toggle: lowest effort");
        assert_eq!(low(&gemini).0.unwrap().effort.as_deref(), Some("low"));

        assert_eq!(off(&qwen), (None, None));
        assert_eq!(high(&qwen), (None, None));
    }

    /// Without facts, or on the owner's own endpoint, only the alias goes
    /// out, and only when there is a level to name.
    #[test]
    fn no_facts_or_byo_means_the_alias_only() {
        assert_eq!(reasoning_for(Thinking::Off, None, false), (None, None));
        assert_eq!(reasoning_for(Thinking::Low, None, false), (None, Some("low".into())));
        let sonnet = facts(true, true, &["low", "high"]);
        assert_eq!(reasoning_for(Thinking::Off, Some(&sonnet), true), (None, None));
        assert_eq!(reasoning_for(Thinking::High, Some(&sonnet), true), (None, Some("high".into())));
    }

    #[test]
    fn reasoning_object_round_trips() {
        let req = ChatCompletionRequest {
            model: "m".into(),
            reasoning: Some(Reasoning { enabled: Some(false), ..Default::default() }),
            ..Default::default()
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["reasoning"], json!({"enabled": false}));
        let back: ChatCompletionRequest = serde_json::from_value(v).unwrap();
        assert_eq!(back, req);
    }
}

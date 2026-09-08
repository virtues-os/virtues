//! The wire between a box and virtues-api's AI routes.
//!
//! One `ChatCompletionRequest`, built by the box and deserialized by the
//! proxy. Before this crate the proxy kept two hand-mirrored request structs
//! and the box built JSON by hand at five sites, and they drifted for three
//! months without anyone noticing: the box sent `provider_options` and
//! `thought_signature` on every chat turn and the proxy, which had neither
//! field, dropped both. A reasoning budget the box thought it was setting
//! never reached a model.
//!
//! The rule this crate makes mechanical: **a field the box sends either
//! reaches the proxy's builder or does not compile.** The proxy still
//! tolerates fields it does not know (it serves every released box at once,
//! and a 400 on an old field is an outage, not a contract), but it collects
//! them into `extra` and logs the keys, so drift is loud instead of silent.
//!
//! Two other shared shapes live here because both sides read them:
//! [`Reasoning`], the gateway's per-request reasoning object, and
//! [`ReasoningFacts`], what the catalog knows about whether a model thinks.
//! Model FACTS are never stored here (see the registry crate's rule); facts
//! are fetched, and this is only their shape.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// A chat completion, as the box asks for it.
///
/// Field names are the box's, snake_case; the proxy translates to the
/// gateway's spelling (`providerOptions`) in one place. Everything optional
/// is `skip_serializing_if` so an unset field is absent on the wire rather
/// than `null`, which older proxies read as "sent".
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
    /// Deprecated and never forwarded. Landed 2026-06-08 for Gemini 3's
    /// function-calling continuity and has never reached a model, because
    /// the proxy never had the field. Replaced by echoing the gateway's
    /// `reasoning_details` (see agents/plan/ai-door-plan.md, 2d). Stays on
    /// the wire until that lands so released boxes keep parsing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
    /// Anything the sender put on the wire that this type does not name.
    /// Empty from a box built against this crate. The proxy logs the keys.
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
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

/// What the catalog says about a model's thinking. Derived by the proxy from
/// the gateway's `tags` and `reasoning_options`, served to boxes on the
/// picker, and read by the box's completion helper to turn a thinking mode
/// into a request.
///
/// Every field is the gateway's claim about the model, not a measurement.
/// `can_disable` in particular comes from a `toggle` entry that the catalog
/// lists for Claude Fable 5, a model the gateway's own docs say cannot turn
/// thinking off. So it decides whether to *try* `enabled: false`, never
/// whether an output ceiling is safe. (Nothing sets ceilings any more.)
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
    /// text. Claude 5 omits it unless asked; Gemini needs `includeThoughts`.
    /// Empty object when the family has no such switch.
    #[serde(default)]
    pub display_options: Value,
}

impl ReasoningFacts {
    /// The provider options that make a model family return its thinking
    /// text, keyed on the catalog's `owned_by`. Both Google keys are set
    /// because the gateway may serve a Gemini model from either provider and
    /// applies whichever entry matches the one it picked.
    pub fn display_options_for(owner: &str) -> Value {
        match owner {
            "anthropic" => serde_json::json!({
                "anthropic": { "thinking": { "type": "adaptive", "display": "summarized" } }
            }),
            "google" => serde_json::json!({
                "google": { "thinkingConfig": { "includeThoughts": true } },
                "vertex": { "thinkingConfig": { "includeThoughts": true } }
            }),
            _ => serde_json::json!({}),
        }
    }
}

/// The proxy's one rule for `providerOptions`: the caller's object, with
/// `gateway.zeroDataRetention: true` written over it when the catalog says
/// this model must be pinned to zero-retention routes. Other keys under
/// `gateway` (routing order, BYOK) survive; a caller's own
/// `zeroDataRetention` does not. A caller value that is not an object is
/// discarded rather than merged into, because there is nothing to merge.
pub fn merge_provider_options(caller: Option<Value>, enforce_zdr: bool) -> Option<Value> {
    if !enforce_zdr {
        return caller;
    }
    let mut root = match caller {
        Some(Value::Object(m)) => m,
        _ => serde_json::Map::new(),
    };
    let gateway = root
        .entry("gateway")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if !gateway.is_object() {
        *gateway = Value::Object(serde_json::Map::new());
    }
    gateway["zeroDataRetention"] = Value::Bool(true);
    Some(Value::Object(root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unset_fields_are_absent_on_the_wire() {
        let req = ChatCompletionRequest {
            model: "anthropic/claude-sonnet-5".into(),
            messages: vec![json!({"role": "user", "content": "hi"})],
            ..Default::default()
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v.as_object().unwrap().keys().collect::<Vec<_>>(), vec!["messages", "model"]);
    }

    /// The drift this crate exists to end: a field the sender knows and the
    /// receiver does not is collected, not dropped, and round-trips.
    #[test]
    fn unknown_fields_are_kept_and_named() {
        let req: ChatCompletionRequest = serde_json::from_value(json!({
            "model": "m", "messages": [], "some_future_field": 1, "thought_signature": "sig"
        }))
        .unwrap();
        assert_eq!(req.extra.keys().collect::<Vec<_>>(), vec!["some_future_field"]);
        assert_eq!(req.thought_signature.as_deref(), Some("sig"));
    }

    #[test]
    fn reasoning_object_round_trips() {
        let req: ChatCompletionRequest = serde_json::from_value(json!({
            "model": "m", "messages": [], "reasoning": {"enabled": false}
        }))
        .unwrap();
        assert_eq!(req.reasoning, Some(Reasoning { enabled: Some(false), ..Default::default() }));
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["reasoning"], json!({"enabled": false}));
    }

    #[test]
    fn zdr_is_merged_over_the_callers_options_and_cannot_be_switched_off() {
        let merged = merge_provider_options(
            Some(json!({
                "anthropic": {"thinking": {"type": "adaptive"}},
                "gateway": {"order": ["anthropic"], "zeroDataRetention": false}
            })),
            true,
        )
        .unwrap();
        assert_eq!(merged["anthropic"]["thinking"]["type"], "adaptive");
        assert_eq!(merged["gateway"]["order"], json!(["anthropic"]));
        assert_eq!(merged["gateway"]["zeroDataRetention"], true);
    }

    #[test]
    fn no_zdr_means_the_callers_options_pass_untouched() {
        assert_eq!(merge_provider_options(None, false), None);
        let own = json!({"google": {"thinkingConfig": {"includeThoughts": true}}});
        assert_eq!(merge_provider_options(Some(own.clone()), false), Some(own));
        assert_eq!(
            merge_provider_options(None, true),
            Some(json!({"gateway": {"zeroDataRetention": true}}))
        );
    }
}

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
    /// Deprecated; the proxy strips it and it never reaches a model. Landed
    /// 2026-06-08 for Gemini 3's function-calling continuity. Replaced by
    /// echoing the gateway's `reasoning_details` (agents/plan/ai-door-plan.md,
    /// 2d); the field leaves with that change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
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

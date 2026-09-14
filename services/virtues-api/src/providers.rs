//! Provider Configuration
//!
//! Simplified provider handling - all requests go through Vercel AI Gateway.
//! The gateway handles routing to providers (OpenAI, Anthropic, Google, etc.)
//! based on the model name prefix (e.g., "anthropic/claude-sonnet-4.5").

use crate::config::Config;

/// Provider configuration for making LLM requests
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// API endpoint URL (Vercel AI Gateway)
    pub endpoint: String,
    /// API key for the gateway
    pub api_key: String,
    /// Model name to send (passed through as-is)
    pub model_name: String,
}

/// Get provider configuration - always routes to Vercel AI Gateway
///
/// Model names should be in provider/model format:
/// - `anthropic/claude-sonnet-4.5`
/// - `openai/gpt-4o`
/// - `google/gemini-2.5-pro`
/// - `xai/grok-3`
pub fn get_provider_config(model: &str, config: &Config) -> ProviderConfig {
    ProviderConfig {
        endpoint: format!("{}/v1/chat/completions", config.ai_gateway_url),
        api_key: config.ai_gateway_api_key.clone(),
        model_name: model.to_string(),
    }
}

/// The body sent upstream: the caller's body, passed through, with the few
/// keys the proxy owns rewritten. One function for the streaming and
/// non-streaming paths.
///
/// This is a pass-through and not a re-typed request, on purpose. The proxy
/// used to deserialize the body into its own struct and rebuild the outgoing
/// JSON field by field, which meant every field it did not name was dropped:
/// the box sent `provider_options` on every chat turn for three months and
/// no model ever saw it. Sharing the struct between box and proxy only moved
/// that allowlist one function over. Forwarding the body opaquely removes the
/// allowlist: a field the box adds reaches the gateway with no proxy edit,
/// and a field the gateway rejects comes back as its 400, which is loud,
/// instead of a silent drop. The gateway is the judge of the request shape;
/// the proxy's job is authentication, billing, and zero-retention.
///
/// What the proxy rewrites, and nothing else:
/// - `model`: the upstream id.
/// - `stream` + `stream_options.include_usage`: so the final chunk carries
///   the usage the charge is settled on.
/// - `temperature`: 0.7 when absent. The box sends none on chat turns and has
///   run at 0.7 since the proxy existed; dropping the default would move
///   every chat to the provider's 1.0 in a cloud deploy nobody can see from
///   the box. The box starts sending its own in plan phase 2d; the default
///   goes in the deploy after that.
/// - `provider_options` (the box's spelling) becomes `providerOptions` (the
///   gateway's), with zero-retention merged over it where the catalog says
///   the model must be pinned. See [`merge_provider_options`].
/// - `tools`/`tool_choice` when `tools` is empty: providers reject an empty
///   array.
/// - `thought_signature`: stripped. It landed 2026-06-08 for Gemini 3's
///   function-calling continuity and has never reached a model; the gateway
///   has no such field. Released boxes still send it. Plan 2d replaces it.
///
/// What it never does: invent `max_tokens`. For three months this filled in
/// 4096 when the caller sent none, which put a hard ceiling on every live
/// chat turn the box had deliberately left uncapped, on a model that counts
/// its thinking inside that ceiling. Absent means the model's own window.
pub fn upstream_body(
    body: serde_json::Value,
    upstream_model: &str,
    stream: bool,
    enforce_zdr: bool,
) -> serde_json::Value {
    let mut obj = match body {
        serde_json::Value::Object(m) => m,
        _ => serde_json::Map::new(),
    };
    obj.insert("model".into(), serde_json::json!(upstream_model));
    if stream {
        obj.insert("stream".into(), serde_json::json!(true));
        obj.insert("stream_options".into(), serde_json::json!({ "include_usage": true }));
    }
    obj.entry("temperature").or_insert_with(|| serde_json::json!(0.7));

    // The box spells it snake_case; the gateway reads camelCase. A caller
    // that already sent the gateway's spelling is left alone unless the box
    // spelling is also present, in which case the box's wins.
    let callers = obj
        .remove("provider_options")
        .or_else(|| obj.remove("providerOptions"));
    if let Some(po) = merge_provider_options(callers, enforce_zdr) {
        obj.insert("providerOptions".into(), po);
    }

    let tools_empty = obj
        .get("tools")
        .map(|t| t.as_array().map(|a| a.is_empty()).unwrap_or(true))
        .unwrap_or(true);
    if tools_empty {
        obj.remove("tools");
        obj.remove("tool_choice");
    }
    obj.remove("thought_signature");
    serde_json::Value::Object(obj)
}

/// The proxy's one rule for `providerOptions`: the caller's object, with
/// `gateway.zeroDataRetention: true` written over it when the catalog says
/// this model must be pinned to zero-retention routes. Other keys under
/// `gateway` (routing order, BYOK) survive; a caller's own
/// `zeroDataRetention` does not. A caller value that is not an object is
/// discarded rather than merged into, because there is nothing to merge.
pub fn merge_provider_options(
    caller: Option<serde_json::Value>,
    enforce_zdr: bool,
) -> Option<serde_json::Value> {
    use serde_json::Value;
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

/// Calculate cost from token usage. FALLBACK ONLY. `None` means we do not know.
///
/// The gateway's `usage.cost` is authoritative and is present on every call, on
/// both the streaming and non-streaming paths (verified 2026-07-28 across both
/// providers). We land here only when it's somehow absent — an older endpoint,
/// a non-Vercel upstream, a BYOK response.
///
/// Prices come from the live gateway catalog. If the catalog is cold (a fresh
/// process that has never reached the gateway) or the model is unknown, this
/// returns `None` and **the caller must not charge**.
///
/// There used to be a `FALLBACK_PRICING` floor here — a deliberately expensive
/// invented rate, on the reasoning that over-charging is visible and refundable
/// while under-charging is silent. That is margin logic, and it is the wrong
/// trade on a consumer product: a customer billed a number we made up has no
/// way to know, and "refundable" only helps the ones who check. We always know
/// the tokens and the model, so the rate is knowable — just not yet. Eat the
/// cost of the blind spot rather than guessing at the user's expense, and log
/// loudly so the blind spot doesn't stay quiet.
///
/// There is no per-model price table here, and there must never be one again —
/// the last one under-billed image generation by 13× because nobody remembered
/// to add a row. See `catalog.rs`.
pub fn calculate_cost(
    catalog: &crate::catalog::Catalog,
    model: &str,
    prompt_tokens: u32,
    completion_tokens: u32,
) -> Option<f64> {
    let Some((input_cost_per_1k, output_cost_per_1k)) = catalog.pricing(model) else {
        tracing::error!(
            model,
            prompt_tokens,
            completion_tokens,
            catalog_cold = catalog.is_cold(),
            "no gateway cost and no catalog price — serving this call UNBILLED. \
             We eat it rather than invent a rate."
        );
        return None;
    };

    let input_cost = (prompt_tokens as f64 / 1000.0) * input_cost_per_1k;
    let output_cost = (completion_tokens as f64 / 1000.0) * output_cost_per_1k;

    Some(input_cost + output_cost)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The drift this pass-through exists to end: a field the proxy has never
    /// heard of reaches the gateway, and the caller's ceiling is neither
    /// invented nor removed.
    #[test]
    fn unknown_fields_pass_through_and_no_ceiling_is_invented() {
        let body = upstream_body(
            json!({"model": "m", "messages": [], "some_future_field": 1, "reasoning": {"effort": "low"}}),
            "provider/m",
            false,
            false,
        );
        assert_eq!(body["some_future_field"], 1);
        assert_eq!(body["reasoning"]["effort"], "low");
        assert_eq!(body["model"], "provider/m");
        assert!(body.get("max_tokens").is_none());
        assert!(body.get("stream").is_none());
        assert_eq!(body["temperature"], 0.7);
    }

    #[test]
    fn the_proxy_owned_keys_are_rewritten() {
        let body = upstream_body(
            json!({
                "model": "m", "messages": [], "temperature": 0.2,
                "provider_options": {"anthropic": {"thinking": {"type": "adaptive"}}},
                "thought_signature": "sig",
                "tools": [], "tool_choice": "auto"
            }),
            "provider/m",
            true,
            true,
        );
        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"]["include_usage"], true);
        assert_eq!(body["temperature"], 0.2);
        assert!(body.get("provider_options").is_none());
        assert_eq!(body["providerOptions"]["anthropic"]["thinking"]["type"], "adaptive");
        assert_eq!(body["providerOptions"]["gateway"]["zeroDataRetention"], true);
        assert!(body.get("thought_signature").is_none());
        assert!(body.get("tools").is_none());
        assert!(body.get("tool_choice").is_none());
    }

    #[test]
    fn non_empty_tools_survive() {
        let body = upstream_body(
            json!({"model": "m", "messages": [], "tools": [{"type": "function"}], "tool_choice": "auto"}),
            "m",
            false,
            false,
        );
        assert_eq!(body["tools"].as_array().unwrap().len(), 1);
        assert_eq!(body["tool_choice"], "auto");
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

//! The pieces of one agent turn that do not stream: what the request carries
//! besides the conversation, and what the conversation gains between steps.
//! `AgentLoop::run` holds the control flow and the `yield`s; these hold the
//! decisions, so each can be tested without a model on the other end.

use serde_json::Value;

use super::executor::{self, ToolExecutionResult};
use super::stream::LlmStreamResult;

/// A note from the loop to the model, sent as a user message.
pub(super) fn system_note(text: &str) -> Value {
    serde_json::json!({ "role": "user", "content": format!("[System: {text}]") })
}

/// The `provider_options` every step of the turn sends.
///
/// Asks the model to RETURN its thinking — Claude 5 omits the text unless
/// told `display: summarized`, Gemini needs `includeThoughts`; the catalog
/// carries the right options per family (`ReasoningFacts::display_options`).
///
/// And asks the gateway to place the cache markers. Our own marker sits on
/// the system prompt only; `caching: auto` adds one on the last message, so
/// each step reads the previous step's prompt from cache, plus one before the
/// last user message. On a model that caches implicitly the gateway changes
/// nothing.
///
/// A BYO endpoint never sees the gateway's providerOptions, so it gets none.
pub(super) fn gateway_options(
    facts: Option<&virtues_registry::ReasoningFacts>,
    byo: bool,
) -> Option<Value> {
    if byo {
        return None;
    }
    let mut options = facts
        .map(|f| f.display_options.clone())
        .filter(|v| v.is_object())
        .unwrap_or_else(|| serde_json::json!({}));
    let gateway = options
        .as_object_mut()
        .expect("filtered to an object above")
        .entry("gateway")
        .or_insert_with(|| serde_json::json!({}));
    if let Some(gateway) = gateway.as_object_mut() {
        gateway.insert("caching".into(), serde_json::json!("auto"));
    }
    Some(options)
}

/// Extend the conversation with a tool step, ready for the next model call:
/// the assistant's tool-call message, one result message per call, any media
/// the tools returned, and a warning when few steps remain.
///
/// `vision` is the catalog's word on whether `model` reads images; `None`
/// means the catalog does not know, treated as cannot, because guessing wrong
/// fails the whole request rather than one attachment.
pub(super) fn append_next_step(
    messages: &mut Vec<Value>,
    model: &str,
    result: &LlmStreamResult,
    tool_results: &[ToolExecutionResult],
    vision: Option<bool>,
    steps_remaining: u32,
) {
    messages.push(executor::build_assistant_tool_message(
        &result.content,
        &result.tool_calls,
        &result.reasoning_details,
    ));

    for tool_result in tool_results {
        messages.push(executor::build_tool_result_message(
            &tool_result.tool_call_id,
            &tool_result.to_llm_content(),
        ));
    }

    // Media the tools returned, so a file the model found counts for as much
    // as one the user pasted.
    let attachments: Vec<crate::tools::ToolAttachment> = tool_results
        .iter()
        .filter_map(|tr| tr.result.as_ref().ok())
        .flat_map(|r| r.attachments.iter().cloned())
        .collect();
    if !attachments.is_empty() {
        match vision {
            Some(true) => {
                if let Some(msg) = executor::build_attachment_message(&attachments) {
                    tracing::info!(count = attachments.len(), "Attaching tool media to next turn");
                    messages.push(msg);
                }
            }
            // Said in the transcript rather than dropped silently, so the
            // model tells the user it cannot see instead of reporting an
            // absence.
            other => {
                let why = if other == Some(false) {
                    format!("{model} cannot read images")
                } else {
                    format!("image support for {model} is unknown on this box")
                };
                tracing::warn!(model = %model, "Dropping tool media: {}", why);
                messages.push(system_note(&format!(
                    "the tool returned {} file(s) to look at, but they were not attached because {}. Tell the user you cannot see the file rather than guessing at its contents.",
                    attachments.len(),
                    why
                )));
            }
        }
    }

    // "Steps" — model calls — not "tool calls": one step can carry several
    // calls. At one step left the last step's own note says it.
    if steps_remaining <= 3 && steps_remaining > 1 {
        messages.push(system_note(&format!(
            "{} step{} (model call{}) remaining in this turn. Finish, or say where you got to and what is left.",
            steps_remaining,
            if steps_remaining == 1 { "" } else { "s" },
            if steps_remaining == 1 { "" } else { "s" }
        )));
        tracing::debug!(steps_remaining, "Injected turn limit warning");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::protocol::StepReason;
    use crate::agent::stream::ToolCall;
    use crate::tools::{ToolAttachment, ToolResult};
    use serde_json::json;

    fn facts(display_options: Value) -> virtues_registry::ReasoningFacts {
        virtues_registry::ReasoningFacts {
            thinks: true,
            can_disable: false,
            effort_values: vec![],
            display_options,
        }
    }

    #[test]
    fn byo_gets_no_provider_options() {
        assert_eq!(gateway_options(Some(&facts(json!({"anthropic": {}}))), true), None);
        assert_eq!(gateway_options(None, true), None);
    }

    #[test]
    fn an_unknown_model_still_asks_for_caching() {
        assert_eq!(gateway_options(None, false), Some(json!({"gateway": {"caching": "auto"}})));
        // A non-object display option is the family having no switch.
        assert_eq!(
            gateway_options(Some(&facts(Value::Null)), false),
            Some(json!({"gateway": {"caching": "auto"}}))
        );
    }

    #[test]
    fn display_options_keep_their_fields_beside_caching() {
        let got = gateway_options(
            Some(&facts(json!({
                "anthropic": {"thinking": {"display": "summarized"}},
                "gateway": {"order": ["a"]}
            }))),
            false,
        )
        .unwrap();
        assert_eq!(got["anthropic"]["thinking"]["display"], "summarized");
        assert_eq!(got["gateway"]["order"], json!(["a"]));
        assert_eq!(got["gateway"]["caching"], "auto");
    }

    fn step(tool_calls: Vec<ToolCall>) -> LlmStreamResult {
        LlmStreamResult {
            content: "Looking.".into(),
            reasoning_details: vec![],
            tool_calls,
            finish_reason: StepReason::ToolCalls,
            usage: None,
        }
    }

    fn call(id: &str) -> ToolCall {
        ToolCall { id: id.into(), name: "read_file".into(), arguments: json!({"path": id}) }
    }

    fn done(id: &str, attachments: Vec<ToolAttachment>) -> ToolExecutionResult {
        let mut result = ToolResult::success(json!({"ok": id}));
        result.attachments = attachments;
        ToolExecutionResult { tool_call_id: id.into(), tool_name: "read_file".into(), result: Ok(result) }
    }

    fn png() -> ToolAttachment {
        ToolAttachment {
            media_type: "image/png".into(),
            data_url: "data:image/png;base64,AAAA".into(),
            filename: "a.png".into(),
        }
    }

    #[test]
    fn a_tool_step_appends_the_call_then_each_result_in_order() {
        let mut messages = vec![];
        append_next_step(
            &mut messages,
            "m",
            &step(vec![call("1"), call("2")]),
            &[done("1", vec![]), done("2", vec![])],
            Some(true),
            10,
        );
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[0]["tool_calls"].as_array().unwrap().len(), 2);
        assert_eq!(messages[1]["role"], "tool");
        assert_eq!(messages[1]["tool_call_id"], "1");
        assert_eq!(messages[2]["tool_call_id"], "2");
    }

    #[test]
    fn media_is_attached_when_the_model_reads_images() {
        let mut messages = vec![];
        append_next_step(&mut messages, "m", &step(vec![call("1")]), &[done("1", vec![png()])], Some(true), 10);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[2]["role"], "user");
        assert!(messages[2]["content"].is_array(), "{}", messages[2]);
    }

    #[test]
    fn media_is_named_not_attached_when_the_model_cannot_read_it() {
        let mut messages = vec![];
        append_next_step(&mut messages, "m", &step(vec![call("1")]), &[done("1", vec![png()])], Some(false), 10);
        assert_eq!(messages.len(), 3);
        let note = messages[2]["content"].as_str().unwrap();
        assert!(note.starts_with("[System: the tool returned 1 file(s)"), "{note}");
        assert!(note.contains("m cannot read images"), "{note}");
    }

    #[test]
    fn media_is_named_not_attached_when_vision_is_unknown() {
        let mut messages = vec![];
        append_next_step(&mut messages, "m", &step(vec![call("1")]), &[done("1", vec![png()])], None, 10);
        let note = messages[2]["content"].as_str().unwrap();
        assert!(note.contains("image support for m is unknown on this box"), "{note}");
    }

    #[test]
    fn a_failed_result_carries_no_media() {
        let failed = ToolExecutionResult {
            tool_call_id: "1".into(),
            tool_name: "read_file".into(),
            result: Err(crate::agent::ToolExecutionError::Timeout(std::time::Duration::from_secs(30))),
        };
        let mut messages = vec![];
        append_next_step(&mut messages, "m", &step(vec![call("1")]), &[failed], None, 10);
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn the_steps_warning_runs_from_three_to_two() {
        let warned = |remaining| {
            let mut messages = vec![];
            append_next_step(&mut messages, "m", &step(vec![call("1")]), &[done("1", vec![])], Some(true), remaining);
            messages.last().unwrap()["content"].as_str().map(str::to_string)
        };
        assert_eq!(
            warned(3).as_deref(),
            Some("[System: 3 steps (model calls) remaining in this turn. Finish, or say where you got to and what is left.]")
        );
        assert!(warned(2).unwrap().starts_with("[System: 2 steps"));
        // One left is the last step, whose own note says it.
        assert!(!warned(1).unwrap_or_default().starts_with("[System"));
        assert!(!warned(4).unwrap_or_default().starts_with("[System"));
    }
}

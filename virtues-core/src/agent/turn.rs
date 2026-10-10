//! The pieces of one agent turn that do not stream: what the request carries
//! besides the conversation, and what the conversation gains between steps.
//! `AgentLoop::run` holds the control flow and the `yield`s; these hold the
//! decisions, so each can be tested without a model on the other end.

use std::time::Instant;

use serde_json::Value;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::JoinHandle;

use super::executor::{self, ExecutorConfig, ToolExecutionError, ToolExecutionResult};
use super::protocol::{AgentEvent, ErrorCode, FinishReason, StepReason};
use super::stream::{self, LlmConfig, LlmStreamResult, StepOptions, StreamError, ToolCall};
use super::{caps, guard, TurnBudget};
use crate::tools::{ToolContext, ToolExecutor};

/// What one turn has done so far: the step it is on, what it has spent, and
/// what its tools have been allowed and refused.
pub(super) struct TurnState {
    pub step: u32,
    started: Instant,
    spent_micros: i64,
    /// Which tools have failed, and how — see `guard`.
    repeat_guard: guard::RepeatGuard,
    /// Calls spent against each capped tool — see `caps`.
    tool_caps: caps::ToolCaps,
}

impl TurnState {
    pub fn new(budget: &TurnBudget) -> Self {
        Self {
            step: 0,
            started: Instant::now(),
            spent_micros: 0,
            repeat_guard: guard::RepeatGuard::new(),
            tool_caps: caps::ToolCaps::new(budget.tool_caps),
        }
    }

    /// Why the current step is the turn's last, if it is.
    ///
    /// On the last step the ceiling allows, or the first step over a budget,
    /// the model is told to answer and sent `tool_choice: none`, and the turn
    /// ends after this step whatever it returns — so a turn never ends on a
    /// tool result with no reply. The step can take the spend one step past a
    /// budget; a cap that ended turns with nothing said cost the whole turn
    /// instead.
    pub fn last_step(&self, max_steps: u32, budget: &TurnBudget) -> Option<FinishReason> {
        let over_cost = budget.max_cost_micros.is_some_and(|cap| self.spent_micros >= cap);
        let over_clock = budget.max_wall_clock.is_some_and(|cap| self.started.elapsed() >= cap);
        let reason = last_step_reason(self.step, max_steps, over_cost || over_clock);
        if let Some(reason) = reason {
            tracing::warn!(
                step = self.step,
                ?reason,
                spent_micros = self.spent_micros,
                elapsed_secs = self.started.elapsed().as_secs(),
                "last step: asking for the answer, tools off"
            );
        }
        reason
    }

    /// Count a model call against the budget, and log it: one line per call,
    /// because the usage table sums a chat's calls into one row and cannot
    /// say which call cost what. Counts and ids only, never content.
    ///
    /// `reusable_tokens` is the most the cache could have served (see
    /// `cache_watch`); `cache_read_tokens` well under it is the provider's
    /// miss, and a non-empty `diverged_at` is ours.
    pub fn record_usage(
        &mut self,
        result: &LlmStreamResult,
        model: &str,
        chat_id: Option<&str>,
        signature: super::cache_watch::Signature,
    ) {
        let Some(usage) = result.usage.as_ref() else { return };
        if let Some(cost) = usage.cost_micros {
            self.spent_micros += cost;
        }
        let reading = chat_id.map(|chat| super::cache_watch::record(
            chat,
            signature,
            self.step,
            usage.prompt_tokens,
            usage.cache_read_tokens.unwrap_or(0),
        ));
        tracing::info!(
            step = self.step,
            model = %model,
            chat_id = chat_id.unwrap_or(""),
            prompt_tokens = usage.prompt_tokens,
            cache_read_tokens = usage.cache_read_tokens.unwrap_or(0),
            reusable_tokens = reading.as_ref().map_or(0, |r| r.reusable_tokens),
            diverged_at = reading.as_ref().map_or("", |r| r.diverged_at.as_str()),
            completion_tokens = usage.completion_tokens,
            reasoning_tokens = usage.reasoning_tokens.unwrap_or(0),
            cost_micros = usage.cost_micros.unwrap_or(0),
            tool_calls = result.tool_calls.len(),
            "model call usage"
        );
    }

    /// Run a step's tool calls through the caps and the guard, returning one
    /// result per call: the ones that ran, then the guard's refusals, then
    /// the calls over their cap.
    ///
    /// The guard records the calls within caps — ran and refused alike, so an
    /// identical retry walks toward the tool's close. A call over its cap did
    /// not fail and is not recorded.
    pub async fn run_tools(
        &mut self,
        tool_executor: &ToolExecutor,
        calls: Vec<ToolCall>,
        context: &ToolContext,
        executor_config: &ExecutorConfig,
    ) -> Vec<ToolExecutionResult> {
        tracing::info!(count = calls.len(), "Executing tool calls");
        let (within_caps, over_caps) = self.tool_caps.admit(calls);
        let (admitted, refused) = self.repeat_guard.admit(&within_caps);
        let mut results =
            executor::execute_tools(tool_executor, &admitted, context, executor_config).await;
        results.extend(refused);
        self.repeat_guard.record(&within_caps, &results);
        results.extend(over_caps);
        results
    }
}

/// Why this step is the turn's last, if it is: a budget ran out, or it is the
/// last step the ceiling allows.
fn last_step_reason(step: u32, max_steps: u32, over_budget: bool) -> Option<FinishReason> {
    if over_budget {
        Some(FinishReason::BudgetExceeded)
    } else if step >= max_steps {
        Some(FinishReason::MaxSteps)
    } else {
        None
    }
}

/// How a turn ends on a step that called no tools: cut off by the output
/// limit, cut short by the step ceiling or a budget, or simply finished.
pub(super) fn answered(step_reason: StepReason, last_step: Option<FinishReason>) -> FinishReason {
    if step_reason == StepReason::MaxTokens {
        FinishReason::OutputLimit
    } else {
        last_step.unwrap_or(FinishReason::EndTurn)
    }
}

/// The error code for a model call that failed. An interrupted stream is not
/// an LLM error: the model was mid-sentence when the bytes stopped. Nor is a
/// provider that refused the call while it was down: the request was fine.
pub(super) fn stream_error_code(e: &StreamError) -> ErrorCode {
    match e {
        StreamError::Interrupted(_) => ErrorCode::Interrupted,
        StreamError::LlmError { status, .. } if stream::is_transient_status(*status) => {
            ErrorCode::ProviderUnavailable
        }
        _ => ErrorCode::LlmError,
    }
}

/// What the client is told about a failed model call. An outage says so up
/// front: the generic "LLM error" text read as a fault in the request. The
/// status stays in the text, because the web client classifies by it.
pub(super) fn stream_error_text(e: &StreamError) -> String {
    match e {
        StreamError::LlmError { status, message } if stream::is_transient_status(*status) => format!(
            "provider unavailable (status {status}): inference is down at the model's provider, \
             which refused the call twice: {message}"
        ),
        _ => e.to_string(),
    }
}

/// A call that was running when Stop was pressed, recorded as stopped so the
/// transcript and the next turn show it ended rather than hung.
/// The web client matches this text to draw the call as stopped rather than
/// failed (`toolPresentation.ts` TOOL_STOPPED).
pub(super) fn stopped(call: &ToolCall) -> ToolExecutionResult {
    ToolExecutionResult {
        tool_call_id: call.id.clone(),
        tool_name: call.name.clone(),
        result: Err(ToolExecutionError::ExecutionFailed("stopped by the owner before it finished".into())),
    }
}

/// Whether a tool result asks the person to act (binding a page, allowing a
/// sudo write) before the turn can go on.
pub(super) fn needs_user(result: &ToolExecutionResult) -> bool {
    result.result.as_ref().is_ok_and(|r| {
        let flag = |k: &str| r.data.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
        // `awaiting_owner`: a sudo write waiting for Allow (`tools::sudo_gate`).
        flag("needs_binding") || flag("awaiting_owner")
    })
}

/// Start one model call on its own task, its events arriving on the returned
/// channel as they stream, so each delta reaches the client when it arrives
/// and aborting the handle drops the provider connection.
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_step(
    llm_config: &LlmConfig,
    model: &str,
    messages: &[Value],
    tools: &[Value],
    provider_options: Option<Value>,
    step_options: StepOptions,
    session_affinity: Option<String>,
) -> (JoinHandle<Result<LlmStreamResult, StreamError>>, UnboundedReceiver<AgentEvent>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<AgentEvent>();
    let llm_config = llm_config.clone();
    let model = model.to_string();
    let messages = messages.to_vec();
    let tools = tools.to_vec();
    let handle = tokio::spawn(async move {
        stream::stream_llm_response(
            &llm_config,
            &model,
            &messages,
            &tools,
            provider_options,
            Some(0.7), // the temperature chat has always run at
            None,      // no fixed output cap
            step_options,
            session_affinity.as_deref(),
            |event| {
                let _ = tx.send(event);
            },
        )
        .await
    });
    (handle, rx)
}

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
    use crate::tools::{ToolAttachment, ToolResult};
    use serde_json::json;

    #[test]
    fn the_last_step_is_the_ceiling_or_the_first_over_budget() {
        assert_eq!(last_step_reason(1, 20, false), None);
        assert_eq!(last_step_reason(19, 20, false), None);
        assert_eq!(last_step_reason(20, 20, false), Some(FinishReason::MaxSteps));
        // A budget names itself even on the ceiling step.
        assert_eq!(last_step_reason(20, 20, true), Some(FinishReason::BudgetExceeded));
        assert_eq!(last_step_reason(3, 20, true), Some(FinishReason::BudgetExceeded));
    }

    fn usage(cost_micros: Option<i64>) -> LlmStreamResult {
        LlmStreamResult {
            content: String::new(),
            reasoning_details: vec![],
            tool_calls: vec![],
            finish_reason: StepReason::EndTurn,
            usage: Some(stream::TokenUsage { cost_micros, ..Default::default() }),
        }
    }

    #[test]
    fn spend_reaching_the_cost_budget_makes_the_next_step_the_last() {
        let budget = TurnBudget { max_cost_micros: Some(1_000), ..Default::default() };
        let mut turn = TurnState::new(&budget);
        turn.step = 1;
        assert_eq!(turn.last_step(20, &budget), None);
        turn.record_usage(&usage(Some(600)), "m", None, super::super::cache_watch::sign("m", &[], &[]));
        turn.record_usage(&usage(None), "m", None, super::super::cache_watch::sign("m", &[], &[])); // a BYO call is free here
        assert_eq!(turn.last_step(20, &budget), None);
        turn.record_usage(&usage(Some(400)), "m", None, super::super::cache_watch::sign("m", &[], &[]));
        assert_eq!(turn.last_step(20, &budget), Some(FinishReason::BudgetExceeded));
    }

    #[test]
    fn a_spent_clock_makes_the_step_the_last() {
        let budget = TurnBudget {
            max_wall_clock: Some(std::time::Duration::ZERO),
            ..Default::default()
        };
        let mut turn = TurnState::new(&budget);
        turn.step = 1;
        assert_eq!(turn.last_step(20, &budget), Some(FinishReason::BudgetExceeded));
        // No budget: only the ceiling.
        let none = TurnBudget::default();
        assert_eq!(turn.last_step(20, &none), None);
        turn.step = 20;
        assert_eq!(turn.last_step(20, &none), Some(FinishReason::MaxSteps));
    }

    #[test]
    fn a_toolless_step_ends_on_the_output_limit_first() {
        assert_eq!(answered(StepReason::EndTurn, None), FinishReason::EndTurn);
        assert_eq!(answered(StepReason::ContentFilter, None), FinishReason::EndTurn);
        assert_eq!(answered(StepReason::EndTurn, Some(FinishReason::MaxSteps)), FinishReason::MaxSteps);
        assert_eq!(
            answered(StepReason::MaxTokens, Some(FinishReason::BudgetExceeded)),
            FinishReason::OutputLimit
        );
    }

    #[test]
    fn interruptions_and_outages_are_not_llm_errors() {
        assert_eq!(stream_error_code(&StreamError::Interrupted("x".into())), ErrorCode::Interrupted);
        assert_eq!(stream_error_code(&StreamError::Connection("x".into())), ErrorCode::LlmError);
        assert_eq!(
            stream_error_code(&StreamError::LlmError { status: 500, message: "x".into() }),
            ErrorCode::LlmError
        );
        assert_eq!(
            stream_error_code(&StreamError::LlmError { status: 503, message: "x".into() }),
            ErrorCode::ProviderUnavailable
        );
    }

    #[test]
    fn an_outage_names_itself_and_keeps_its_status() {
        let text = stream_error_text(&StreamError::LlmError { status: 503, message: "busy".into() });
        assert!(text.starts_with("provider unavailable (status 503)"), "{text}");
        let other = stream_error_text(&StreamError::LlmError { status: 400, message: "bad".into() });
        assert!(other.starts_with("LLM error (status 400)"), "{other}");
    }

    #[test]
    fn a_result_asking_for_a_binding_needs_the_user() {
        let with = |data| ToolExecutionResult {
            tool_call_id: "1".into(),
            tool_name: "edit_page".into(),
            result: Ok(ToolResult::success(data)),
        };
        assert!(needs_user(&with(json!({"needs_binding": true}))));
        assert!(!needs_user(&with(json!({"needs_binding": false}))));
        assert!(!needs_user(&with(json!(["needs_binding"]))));
    }

    /// A call whose arguments did not parse fails in the executor before
    /// any tool runs, so these exercise `run_tools` with no database or
    /// network behind the executor.
    fn broken(id: &str, name: &str, raw: &str) -> ToolCall {
        ToolCall {
            id: id.into(),
            name: name.into(),
            arguments: json!({ stream::UNPARSEABLE_ARGUMENTS_KEY: { "error": "bad", "raw": raw } }),
        }
    }

    #[tokio::test]
    async fn run_tools_orders_ran_then_refused_then_over_cap_and_records_refusals() {
        static CAPS: &[(&str, u32)] = &[("web_search", 1)];
        let budget = TurnBudget { tool_caps: CAPS, ..Default::default() };
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://nobody@127.0.0.1:1/none")
            .unwrap();
        let exec = ToolExecutor::new(pool);
        let ctx = ToolContext::default();
        let cfg = ExecutorConfig::default();
        let mut turn = TurnState::new(&budget);
        let ids = |rs: &[ToolExecutionResult]| rs.iter().map(|r| r.tool_call_id.clone()).collect::<Vec<_>>();

        // Consecutive failures of sql_query: 1.
        let rs = turn.run_tools(&exec, vec![broken("a", "sql_query", "X")], &ctx, &cfg).await;
        assert_eq!(ids(&rs), ["a"]);

        let rs = turn
            .run_tools(
                &exec,
                vec![
                    broken("b", "sql_query", "X"),  // identical to a: refused
                    broken("c", "sql_query", "Y"),  // runs, fails
                    broken("d", "web_search", "P"), // within its cap
                    broken("e", "web_search", "Q"), // over its cap
                ],
                &ctx,
                &cfg,
            )
            .await;
        assert_eq!(ids(&rs), ["c", "d", "b", "e"]);
        assert!(rs[2].to_llm_content().contains("already failed this turn"), "{}", rs[2].to_llm_content());
        assert!(rs[3].to_llm_content().contains("budget for this turn is spent"), "{}", rs[3].to_llm_content());

        // The refusal counted: b and c make three, this fourth closes the tool.
        let rs = turn.run_tools(&exec, vec![broken("f", "sql_query", "Z")], &ctx, &cfg).await;
        assert!(rs[0].to_llm_content().contains("not valid JSON"), "{}", rs[0].to_llm_content());
        let rs = turn.run_tools(&exec, vec![broken("g", "sql_query", "W")], &ctx, &cfg).await;
        assert!(rs[0].to_llm_content().contains("closed for the rest of it"), "{}", rs[0].to_llm_content());
    }

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

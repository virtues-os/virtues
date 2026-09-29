//! One assistant turn, as the agent loop tells it and as the chat keeps it.
//!
//! The recorder turns each `AgentEvent` into the UI-message-stream events the
//! client reads, and collects what the row needs: the text, each run of it,
//! the tool calls in the order they interleaved with that text, the thinking,
//! the usage, and how the turn ended. It does no I/O; the chat stream feeds
//! it, yields what it returns, and persists what it holds at the end.

use std::collections::HashSet;

use chrono::Utc;

use crate::agent::{AgentEvent, FinishReason, StepReason};
use crate::api::chat::{StreamEvent, UIPart};
use crate::api::chats::{ChatMessage, ToolCall};
use crate::types::Timestamp;

/// Where one piece of a turn sat in time.
///
/// The row stores the turn's text (`content`) and its tool calls
/// (`tool_calls`), and neither records the ORDER they interleaved in — so a
/// reload could show what was said and what was called, never which was said
/// before which call. `parts` is built from this at save time.
enum TurnSlot {
    /// An index into the turn's text segments.
    Text(usize),
    /// A tool call id, resolved against the turn's tool calls for its
    /// input/output.
    Tool(String),
}

/// A turn's token and cost totals, summed across its steps (and any Deep
/// Research workers it dispatched).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TurnUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub reasoning_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_write_tokens: u32,
    /// Authoritative spend: the sum of the gateway-reported `usage.cost`
    /// across every step. 0 when nothing reported one.
    pub cost_micros: i64,
}

pub struct TurnRecorder {
    msg_id: String,
    full_content: String,
    reasoning: String,
    in_reasoning: bool,
    text_open: bool,
    /// The row stores the turn's text as one string, so text that resumes
    /// after a tool call gets a paragraph break IN THE ROW ("…exact
    /// text.The earlier edit…" otherwise). On the wire each step's text is
    /// its own part and needs none.
    needs_text_break: bool,
    /// ONE PART PER TEXT RUN, and a distinct id for each (`{msg_id}:t{n}`).
    ///
    /// The AI SDK keys a part by its id, so a second `text-start` with the id
    /// it just closed REOPENS the first part and appends to it. The line the
    /// model writes before reaching for a tool ("Checking what he sent in
    /// August") is scaffolding worth reading while it happens; the text at
    /// the end is the reply. A distinct id per run is what lets the view put
    /// the first in the thinking block and leave the second in the transcript.
    text_seq: usize,
    text_part_id: String,
    text_segments: Vec<String>,
    /// The turn in order, so the stored `parts` can interleave text with the
    /// tool calls it ran between — a reload then sees what the stream saw.
    turn_slots: Vec<TurnSlot>,
    tool_calls: Vec<ToolCall>,
    /// Which calls failed. `{"error": …}` in a result is not the same claim —
    /// a tool may answer with an `error` key of its own — so the ids are kept.
    failed_tools: HashSet<String>,
    /// The gateway's reasoning blocks across the turn's steps, for the row.
    reasoning_details: Vec<serde_json::Value>,
    usage: TurnUsage,
    /// Set by any error event mid-turn: the reply on screen is partial, and
    /// the row must say so or a reload shows the stub as the answer.
    interrupted: bool,
    /// How the last LLM step ended, and how the loop ended: together they are
    /// the `finish` event's reason and the row's subject.
    last_step_reason: Option<StepReason>,
    loop_finish: Option<FinishReason>,
}

impl TurnRecorder {
    pub fn new(msg_id: String) -> Self {
        Self {
            text_part_id: msg_id.clone(),
            msg_id,
            full_content: String::new(),
            reasoning: String::new(),
            in_reasoning: false,
            text_open: false,
            needs_text_break: false,
            text_seq: 0,
            text_segments: Vec::new(),
            turn_slots: Vec::new(),
            tool_calls: Vec::new(),
            failed_tools: HashSet::new(),
            reasoning_details: Vec::new(),
            usage: TurnUsage::default(),
            interrupted: false,
            last_step_reason: None,
            loop_finish: None,
        }
    }

    pub fn usage(&self) -> TurnUsage {
        self.usage
    }

    /// One event from the agent loop, and what the client is told about it.
    ///
    /// Text and reasoning parts open lazily inside a step and close with it:
    /// the SDK forgets its open parts at every finish-step, so a part that
    /// spans steps is a delta with no home.
    pub fn on_event(&mut self, event: AgentEvent) -> Vec<StreamEvent> {
        let mut out = Vec::new();
        match event {
            AgentEvent::TextDelta { content } => {
                if self.in_reasoning {
                    self.in_reasoning = false;
                    out.push(StreamEvent::ReasoningEnd { id: self.msg_id.clone() });
                }
                if !self.text_open {
                    self.text_open = true;
                    self.text_seq += 1;
                    self.text_part_id = format!("{}:t{}", self.msg_id, self.text_seq);
                    self.turn_slots.push(TurnSlot::Text(self.text_segments.len()));
                    self.text_segments.push(String::new());
                    out.push(StreamEvent::TextStart { id: self.text_part_id.clone() });
                }
                // Text resuming after a tool call: break the paragraph in the
                // stored string so it doesn't butt against the previous
                // segment's final sentence.
                if self.needs_text_break && !self.full_content.is_empty() && !self.full_content.ends_with('\n') {
                    self.full_content.push_str("\n\n");
                }
                self.needs_text_break = false;
                self.full_content.push_str(&content);
                if let Some(seg) = self.text_segments.last_mut() {
                    seg.push_str(&content);
                }
                out.push(StreamEvent::TextDelta { id: self.text_part_id.clone(), delta: content });
            }

            AgentEvent::ReasoningDelta { content } => {
                if !self.in_reasoning {
                    self.in_reasoning = true;
                    out.push(StreamEvent::ReasoningStart { id: self.msg_id.clone() });
                }
                self.reasoning.push_str(&content);
                out.push(StreamEvent::ReasoningDelta { id: self.msg_id.clone(), delta: content });
            }

            AgentEvent::ToolCallStart { id, name, args } => {
                // Any text that resumes after this tool call starts a new
                // paragraph (see needs_text_break).
                self.needs_text_break = true;
                self.turn_slots.push(TurnSlot::Tool(id.clone()));
                self.tool_calls.push(ToolCall {
                    tool_name: name.clone(),
                    tool_call_id: Some(id.clone()),
                    arguments: args.unwrap_or(serde_json::Value::Null),
                    // Filled in by ToolCallResult.
                    result: None,
                    timestamp: Utc::now().to_rfc3339(),
                });
                out.push(StreamEvent::ToolInputStart { tool_call_id: id, tool_name: name });
            }

            AgentEvent::ToolCallArgsPartial { id, args_delta } => {
                out.push(StreamEvent::ToolInputDelta { tool_call_id: id, input_text_delta: args_delta });
            }

            AgentEvent::ToolCallArgsComplete { id, args } => {
                // This is where the arguments become known: ToolCallStart
                // fires as soon as the tool has a name, while the args are
                // still streaming, so the tracked call is holding `Null`.
                // Writing them back here is what puts them in the persisted
                // row, and in the turn replayed to the model later.
                let tool_name = self
                    .tool_call_mut(&id)
                    .map(|tc| {
                        tc.arguments = args.clone();
                        tc.tool_name.clone()
                    })
                    .unwrap_or_default();
                out.push(StreamEvent::ToolInputAvailable { tool_call_id: id, tool_name, input: args });
            }

            AgentEvent::ToolCallResult { id, result, success: false, error } => {
                // A failed tool is a tool error on the wire, not an output
                // with an error inside it. The model still sees the failure
                // text (executor::to_llm_content); the row keeps it as the
                // result so a reload shows the same, beside whatever evidence
                // the tool returned with it — a code run's traceback.
                let error_text = error
                    .or_else(|| result.get("error").and_then(|e| e.as_str()).map(str::to_string))
                    .unwrap_or_else(|| "the tool reported a failure".to_string());
                if let Some(tc) = self.tool_call_mut(&id) {
                    let mut row = match result {
                        serde_json::Value::Object(map) => map,
                        _ => serde_json::Map::new(),
                    };
                    row.insert("error".into(), serde_json::Value::String(error_text.clone()));
                    tc.result = Some(serde_json::Value::Object(row));
                }
                self.failed_tools.insert(id.clone());
                out.push(StreamEvent::ToolOutputError { tool_call_id: id, error_text });
            }

            AgentEvent::ToolCallResult { id, result, success: true, error: _ } => {
                // The CLIENT gets the value whole — that is how a generated
                // image reaches the chat. The ROW does not: a data URL is a
                // megabyte of base64 that the model cannot read and that would
                // be replayed into every later turn from `parts`.
                let mut narrative_page = None;
                if let Some(tc) = self.tool_call_mut(&id) {
                    let mut stored = result.clone();
                    redact_inline_data(&mut stored);
                    tc.result = Some(stored);
                    // The interview's finisher names the page the client
                    // should open beside the chat (see NarrativeDocumentReady).
                    if tc.tool_name == "write_it_up" {
                        narrative_page =
                            result.get("document_page_id").and_then(|v| v.as_str()).map(str::to_string);
                    }
                }
                // Bill nested Deep Research worker tokens to this chat's
                // usage, and their cost with them: the dispatch result carries
                // aggregate worker counts that the orchestrator's own Usage
                // events don't include.
                if let Some(usage) = result.get("usage") {
                    self.usage.input_tokens += usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    self.usage.output_tokens += usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    self.usage.cost_micros += usage.get("cost_micros").and_then(|v| v.as_i64()).unwrap_or(0);
                }
                out.push(StreamEvent::ToolOutputAvailable { tool_call_id: id, output: result });
                if let Some(page_id) = narrative_page {
                    out.push(StreamEvent::NarrativeDocumentReady { page_id });
                }
            }

            AgentEvent::Usage {
                prompt_tokens,
                completion_tokens,
                total_tokens: _,
                reasoning_tokens,
                cache_read_tokens,
                cache_write_tokens,
                cost_micros,
            } => {
                self.usage.input_tokens += prompt_tokens;
                self.usage.output_tokens += completion_tokens;
                self.usage.reasoning_tokens += reasoning_tokens.unwrap_or(0);
                self.usage.cache_read_tokens += cache_read_tokens.unwrap_or(0);
                self.usage.cache_write_tokens += cache_write_tokens.unwrap_or(0);
                self.usage.cost_micros += cost_micros.unwrap_or(0);
            }

            AgentEvent::ReasoningDetails { details } => {
                self.reasoning_details.extend(details);
            }

            AgentEvent::Error { message, code: _, recoverable: _ } => {
                self.interrupted = true;
                out.push(StreamEvent::Error { error_text: message });
            }

            // A step ended. Close it on the wire; when the model asked for
            // tools, the next LLM call is a new step and opens one.
            AgentEvent::StepComplete { reason, .. } => {
                self.last_step_reason = Some(reason);
                if self.in_reasoning {
                    self.in_reasoning = false;
                    out.push(StreamEvent::ReasoningEnd { id: self.msg_id.clone() });
                }
                if self.text_open {
                    self.text_open = false;
                    out.push(StreamEvent::TextEnd { id: self.text_part_id.clone() });
                }
                out.push(StreamEvent::FinishStep);
                if reason == StepReason::ToolCalls {
                    out.push(StreamEvent::StartStep);
                }
            }

            AgentEvent::Done { finish_reason, .. } => {
                self.loop_finish = Some(finish_reason);
            }

            AgentEvent::LoopStarted { .. } => {}
        }
        out
    }

    /// The turn's last events: close whatever part a step left open (an
    /// error or a stop mid-step), then say how it ended.
    ///
    /// `cancelled` is the turn's token; `unattended` is whether the cap, not
    /// the person, cancelled it. A stop is an abort, not a finish. Otherwise
    /// the loop's verdict wins over the last step's, and the last step's over
    /// "stop".
    pub fn close(&mut self, cancelled: bool, unattended: bool) -> Vec<StreamEvent> {
        let mut out = Vec::new();
        if self.in_reasoning {
            self.in_reasoning = false;
            out.push(StreamEvent::ReasoningEnd { id: self.msg_id.clone() });
        }
        if self.text_open {
            self.text_open = false;
            out.push(StreamEvent::TextEnd { id: self.text_part_id.clone() });
        }
        if cancelled || self.loop_finish == Some(FinishReason::Cancelled) {
            let reason = if unattended { "unattended" } else { "stopped" };
            out.push(StreamEvent::Abort { reason: Some(reason.to_string()) });
        } else {
            let finish_reason = match (self.loop_finish, self.last_step_reason) {
                (Some(FinishReason::Error), _) => "error",
                (Some(FinishReason::MaxSteps), _)
                | (Some(FinishReason::BudgetExceeded), _)
                | (Some(FinishReason::AwaitingUser), _) => "other",
                (Some(FinishReason::OutputLimit), _) | (_, Some(StepReason::MaxTokens)) => "length",
                (_, Some(StepReason::ContentFilter)) => "content-filter",
                (_, Some(StepReason::ToolCalls)) => "tool-calls",
                _ if self.interrupted => "error",
                _ => "stop",
            };
            out.push(StreamEvent::Finish { finish_reason: finish_reason.to_string() });
        }
        out
    }

    /// How the turn ended, as the row's subject, so a reload shows the same
    /// notice under the reply. A stop wins over everything, and the cap's
    /// stop is told apart from the person's: telling someone they stopped a
    /// reply they never touched is a lie about who did what. "interrupted"
    /// is the stream or the model stopping before the reply was finished
    /// (VIR-334).
    ///
    /// `cancelled` here is the turn's token alone — unlike `close`, a loop
    /// that reported `Cancelled` on an uncancelled token is not a stop.
    pub fn subject(&self, cancelled: bool, unattended: bool) -> Option<&'static str> {
        let cut_short = self.loop_finish == Some(FinishReason::OutputLimit)
            || self.last_step_reason == Some(StepReason::MaxTokens);
        if unattended {
            Some("unattended")
        } else if cancelled {
            Some("cancelled")
        } else if cut_short {
            Some("length")
        } else if self.loop_finish == Some(FinishReason::MaxSteps) {
            Some("max_steps")
        } else if self.loop_finish == Some(FinishReason::BudgetExceeded) {
            Some("budget")
        } else if self.interrupted {
            Some("interrupted")
        } else {
            None
        }
    }

    /// The turn's text, every run joined, as the row's `content` holds it.
    pub fn content(&self) -> &str {
        &self.full_content
    }

    /// The assistant row for this turn, or None if it produced neither text
    /// nor a tool call.
    ///
    /// Text is not the only thing a turn produces: one that called tools and
    /// was stopped — or hit the step ceiling — before it wrote a word still
    /// has calls the person watched run, and a notice to hang on them.
    pub fn into_message(self, model: &str, agent_id: String, subject: Option<&str>) -> Option<ChatMessage> {
        if self.full_content.is_empty() && self.tool_calls.is_empty() {
            return None;
        }
        let parts = build_turn_parts(
            &self.turn_slots,
            &self.text_segments,
            &self.tool_calls,
            &self.failed_tools,
            &self.reasoning,
        );
        Some(ChatMessage {
            id: None,
            role: "assistant".to_string(),
            // The whole turn joined: what the model is shown as its own
            // history, and what a row without `parts` falls back to.
            content: self.full_content,
            timestamp: Timestamp::now(),
            model: Some(model.to_string()),
            provider: Some(model.split('/').next().unwrap_or("unknown").to_string()),
            agent_id: Some(agent_id),
            tool_calls: if self.tool_calls.is_empty() { None } else { Some(self.tool_calls) },
            reasoning: if self.reasoning.is_empty() { None } else { Some(self.reasoning) },
            intent: None,
            subject: subject.map(str::to_string),
            reasoning_details: if self.reasoning_details.is_empty() {
                None
            } else {
                Some(serde_json::Value::Array(self.reasoning_details))
            },
            // The turn in order: what lets a reopened chat tell the model's
            // "checking his messages now" from its actual answer.
            parts: if parts.is_empty() { None } else { Some(parts) },
        })
    }

    fn tool_call_mut(&mut self, id: &str) -> Option<&mut ToolCall> {
        self.tool_calls.iter_mut().find(|tc| tc.tool_call_id.as_deref() == Some(id))
    }
}

/// Replace embedded data URLs with a note about what was there.
///
/// A `data:` URL is for the browser. Kept in a tool result it is carried to the
/// model, stored in `parts`, and replayed on every subsequent turn — none of
/// which it survives usefully, and all of which it fills.
fn redact_inline_data(value: &mut serde_json::Value) {
    /// Long enough that a small inline icon survives; short enough that no
    /// real payload does.
    const INLINE_LIMIT: usize = 2048;
    match value {
        serde_json::Value::String(s) if s.starts_with("data:") && s.len() > INLINE_LIMIT => {
            let kind = s
                .split_once(';')
                .map(|(head, _)| head.trim_start_matches("data:"))
                .filter(|k| !k.is_empty())
                .unwrap_or("file");
            *s = format!("[{kind}, {} KB, shown in the chat]", s.len() / 1024);
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(redact_inline_data),
        serde_json::Value::Object(map) => map.values_mut().for_each(redact_inline_data),
        _ => {}
    }
}

/// Materialize a turn's ordered `parts` from the pieces the stream collected.
///
/// Text runs keep their order relative to the tool calls that ran between them,
/// which is the whole point: the LAST text run is the reply, and everything
/// before it is the model narrating its way there.
///
/// A tool id in `turn_slots` with no matching entry in `tool_calls` is skipped
/// rather than written as an empty invocation — that only happens if the stream
/// ended between a tool starting and being recorded, and a part with no name or
/// input tells a reader nothing except that something is missing.
fn build_turn_parts(
    turn_slots: &[TurnSlot],
    text_segments: &[String],
    tool_calls: &[ToolCall],
    failed_tools: &HashSet<String>,
    reasoning: &str,
) -> Vec<UIPart> {
    let mut parts = Vec::with_capacity(turn_slots.len() + 1);
    // The thinking comes before the turn it produced. It has its own column
    // too, but the client returns `parts` verbatim when there are any, so a
    // turn whose thinking is only in the column reloads with an empty
    // thinking block.
    if !reasoning.trim().is_empty() {
        parts.push(UIPart::Reasoning { text: reasoning.to_string() });
    }
    for slot in turn_slots {
        match slot {
            TurnSlot::Text(i) => {
                let Some(text) = text_segments.get(*i) else { continue };
                // A run that produced nothing is not a paragraph of silence.
                if text.trim().is_empty() {
                    continue;
                }
                parts.push(UIPart::Text { text: text.clone() });
            }
            TurnSlot::Tool(id) => {
                let Some(tc) = tool_calls
                    .iter()
                    .find(|tc| tc.tool_call_id.as_deref() == Some(id.as_str()))
                else {
                    continue;
                };
                // The turn is over by the time this runs, so anything that
                // returned has its output and anything that did not, did not.
                // A tool that FAILED is its own state, not `output-available`
                // with the reason buried inside `output`: otherwise the red
                // block never renders on reload and the replay hands the model
                // an error object as though it were an answer.
                let failed = failed_tools.contains(id);
                let error_text = failed.then(|| {
                    tc.result
                        .as_ref()
                        .and_then(|r| r.get("error"))
                        .and_then(|e| e.as_str())
                        .unwrap_or("the tool reported a failure")
                        .to_string()
                });
                let state = match (failed, tc.result.is_some()) {
                    (true, _) => "output-error",
                    (false, true) => "output-available",
                    (false, false) => "input-available",
                };
                parts.push(UIPart::ToolInvocation {
                    tool_call_id: id.clone(),
                    tool_name: tc.tool_name.clone(),
                    input: tc.arguments.clone(),
                    state: state.to_string(),
                    output: tc.result.clone(),
                    error_text,
                });
            }
        }
    }
    parts
}

/// What the stream sends and what the row keeps, pinned: the exact wire
/// lines for each kind of turn, and the message, parts and subject it saves.
/// The client parses these lines and reloads from that row, so a change here
/// is a change to both.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::ErrorCode;
    use crate::api::chat::serialize_event;
    use serde_json::json;

    fn feed(r: &mut TurnRecorder, events: Vec<AgentEvent>) -> Vec<String> {
        events.into_iter().flat_map(|e| r.on_event(e)).map(|e| serialize_event(&e)).collect()
    }

    fn wire(events: Vec<StreamEvent>) -> Vec<String> {
        events.iter().map(serialize_event).collect()
    }

    fn text(s: &str) -> AgentEvent {
        AgentEvent::TextDelta { content: s.into() }
    }

    fn step(reason: StepReason) -> AgentEvent {
        AgentEvent::StepComplete { step: 1, reason }
    }

    fn done(finish_reason: FinishReason) -> AgentEvent {
        AgentEvent::Done { total_steps: 1, finish_reason }
    }

    fn tool_start(id: &str, name: &str) -> AgentEvent {
        AgentEvent::ToolCallStart { id: id.into(), name: name.into(), args: None }
    }

    fn tool_ok(id: &str, result: serde_json::Value) -> AgentEvent {
        AgentEvent::ToolCallResult { id: id.into(), result, success: true, error: None }
    }

    fn kinds(parts: &[UIPart]) -> Vec<String> {
        parts
            .iter()
            .map(|p| match p {
                UIPart::Text { text } => format!("text:{text}"),
                UIPart::Reasoning { text } => format!("reasoning:{text}"),
                UIPart::ToolInvocation { tool_name, state, .. } => format!("tool:{tool_name}:{state}"),
                _ => "other".to_string(),
            })
            .collect()
    }

    /// How a turn ends on the wire and on the row, for a given ending.
    fn ending(events: Vec<AgentEvent>, cancelled: bool, unattended: bool) -> (Vec<String>, Option<&'static str>) {
        let mut r = TurnRecorder::new("m".into());
        feed(&mut r, events);
        let subject = r.subject(cancelled, unattended);
        (wire(r.close(cancelled, unattended)), subject)
    }

    #[test]
    fn a_plain_reply_is_one_text_part() {
        let mut r = TurnRecorder::new("m".into());
        let mut lines = feed(&mut r, vec![text("Hel"), text("lo"), step(StepReason::EndTurn), done(FinishReason::EndTurn)]);
        lines.extend(wire(r.close(false, false)));
        assert_eq!(
            lines,
            [
                r#"{"type":"text-start","id":"m:t1"}"#,
                r#"{"type":"text-delta","id":"m:t1","delta":"Hel"}"#,
                r#"{"type":"text-delta","id":"m:t1","delta":"lo"}"#,
                r#"{"type":"text-end","id":"m:t1"}"#,
                r#"{"type":"finish-step"}"#,
                r#"{"type":"finish","finishReason":"stop"}"#,
            ]
        );
        assert_eq!(r.subject(false, false), None);
        let msg = r.into_message("anthropic/claude", "auto".into(), None).expect("a row");
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, "Hello");
        assert_eq!(msg.model.as_deref(), Some("anthropic/claude"));
        assert_eq!(msg.provider.as_deref(), Some("anthropic"));
        assert_eq!(msg.agent_id.as_deref(), Some("auto"));
        assert!(msg.tool_calls.is_none() && msg.reasoning.is_none() && msg.reasoning_details.is_none());
        assert_eq!(kinds(msg.parts.as_deref().unwrap()), ["text:Hello"]);
    }

    #[test]
    fn reasoning_closes_when_text_begins_and_is_kept_first() {
        let mut r = TurnRecorder::new("m".into());
        let lines = feed(
            &mut r,
            vec![
                AgentEvent::ReasoningDelta { content: "weigh".into() },
                AgentEvent::ReasoningDelta { content: "ing".into() },
                AgentEvent::ReasoningDetails { details: vec![json!({"type": "reasoning.text", "text": "weighing"})] },
                text("Yes."),
                step(StepReason::EndTurn),
            ],
        );
        assert_eq!(
            lines,
            [
                r#"{"type":"reasoning-start","id":"m"}"#,
                r#"{"type":"reasoning-delta","id":"m","delta":"weigh"}"#,
                r#"{"type":"reasoning-delta","id":"m","delta":"ing"}"#,
                r#"{"type":"reasoning-end","id":"m"}"#,
                r#"{"type":"text-start","id":"m:t1"}"#,
                r#"{"type":"text-delta","id":"m:t1","delta":"Yes."}"#,
                r#"{"type":"text-end","id":"m:t1"}"#,
                r#"{"type":"finish-step"}"#,
            ]
        );
        let msg = r.into_message("m1", "auto".into(), None).unwrap();
        assert_eq!(msg.reasoning.as_deref(), Some("weighing"));
        assert_eq!(msg.reasoning_details, Some(json!([{"type": "reasoning.text", "text": "weighing"}])));
        assert_eq!(kinds(msg.parts.as_deref().unwrap()), ["reasoning:weighing", "text:Yes."]);
    }

    /// Reasoning still open when the turn ends is closed before the finish.
    #[test]
    fn close_ends_an_open_reasoning_part() {
        let (lines, _) = ending(vec![AgentEvent::ReasoningDelta { content: "hm".into() }], false, false);
        assert_eq!(lines, [r#"{"type":"reasoning-end","id":"m"}"#, r#"{"type":"finish","finishReason":"stop"}"#]);
    }

    /// Each run of text has its own part id; text after a tool is a new
    /// paragraph in the row but not on the wire; the args that arrive after
    /// the tool starts are the ones stored.
    #[test]
    fn text_tool_text_keeps_its_order_and_its_part_ids() {
        let mut r = TurnRecorder::new("m".into());
        let mut lines = feed(
            &mut r,
            vec![
                text("Checking."),
                tool_start("c1", "sql_query"),
                AgentEvent::ToolCallArgsPartial { id: "c1".into(), args_delta: "{\"q\"".into() },
                AgentEvent::ToolCallArgsComplete { id: "c1".into(), args: json!({"q": 1}) },
                step(StepReason::ToolCalls),
                tool_ok("c1", json!({"rows": []})),
                text("Found it."),
                step(StepReason::EndTurn),
                done(FinishReason::EndTurn),
            ],
        );
        lines.extend(wire(r.close(false, false)));
        assert_eq!(
            lines,
            [
                r#"{"type":"text-start","id":"m:t1"}"#,
                r#"{"type":"text-delta","id":"m:t1","delta":"Checking."}"#,
                r#"{"type":"tool-input-start","toolCallId":"c1","toolName":"sql_query"}"#,
                r#"{"type":"tool-input-delta","toolCallId":"c1","inputTextDelta":"{\"q\""}"#,
                r#"{"type":"tool-input-available","toolCallId":"c1","toolName":"sql_query","input":{"q":1}}"#,
                r#"{"type":"text-end","id":"m:t1"}"#,
                r#"{"type":"finish-step"}"#,
                r#"{"type":"start-step"}"#,
                r#"{"type":"tool-output-available","toolCallId":"c1","output":{"rows":[]}}"#,
                r#"{"type":"text-start","id":"m:t2"}"#,
                r#"{"type":"text-delta","id":"m:t2","delta":"Found it."}"#,
                r#"{"type":"text-end","id":"m:t2"}"#,
                r#"{"type":"finish-step"}"#,
                r#"{"type":"finish","finishReason":"stop"}"#,
            ]
        );
        let msg = r.into_message("m1", "auto".into(), None).unwrap();
        assert_eq!(msg.content, "Checking.\n\nFound it.");
        let calls = msg.tool_calls.as_ref().unwrap();
        assert_eq!(calls[0].arguments, json!({"q": 1}));
        assert_eq!(calls[0].result, Some(json!({"rows": []})));
        assert_eq!(
            kinds(msg.parts.as_deref().unwrap()),
            ["text:Checking.", "tool:sql_query:output-available", "text:Found it."]
        );
    }

    /// No break is added after text that already ends a line, and none
    /// before the first text of a turn that opened with a tool.
    #[test]
    fn the_paragraph_break_is_only_added_where_text_would_butt() {
        let mut r = TurnRecorder::new("m".into());
        feed(&mut r, vec![tool_start("c0", "a"), text("One\n"), tool_start("c1", "b"), text("Two")]);
        assert_eq!(r.content(), "One\nTwo");
        // A tool that starts before its step ends still breaks the row's
        // paragraph, while the wire keeps the text in the part still open.
        let mut r = TurnRecorder::new("m".into());
        let lines = feed(&mut r, vec![text("A."), tool_start("c1", "b"), text("B.")]);
        assert_eq!(r.content(), "A.\n\nB.");
        assert!(
            !lines.iter().any(|l| l.contains(r#""id":"m:t2""#)),
            "no step ended, so the second delta is still in the first part: {lines:?}"
        );
    }

    #[test]
    fn a_failed_code_run_keeps_its_traceback_in_the_row() {
        let mut r = TurnRecorder::new("m".into());
        feed(
            &mut r,
            vec![
                tool_start("c1", "code_interpreter"),
                AgentEvent::ToolCallResult {
                    id: "c1".into(),
                    result: json!({"stderr": "Traceback …\nZeroDivisionError: division by zero", "exit_code": 1}),
                    success: false,
                    error: Some("Code execution failed: ZeroDivisionError: division by zero".into()),
                },
            ],
        );
        let msg = r.into_message("m1", "auto".into(), None).unwrap();
        let row = msg.tool_calls.as_ref().unwrap()[0].result.clone().unwrap();
        assert_eq!(row["error"], "Code execution failed: ZeroDivisionError: division by zero");
        assert!(row["stderr"].as_str().unwrap().starts_with("Traceback"));
    }

    #[test]
    fn a_failed_tool_is_an_error_on_the_wire_and_in_the_row() {
        let mut r = TurnRecorder::new("m".into());
        let lines = feed(
            &mut r,
            vec![
                tool_start("c1", "create_page"),
                AgentEvent::ToolCallResult {
                    id: "c1".into(),
                    result: json!({"error": "the page already exists"}),
                    success: false,
                    error: None,
                },
                tool_start("c2", "create_page"),
                AgentEvent::ToolCallResult {
                    id: "c2".into(),
                    result: json!({"error": "ignored"}),
                    success: false,
                    error: Some("timed out".into()),
                },
                tool_start("c3", "create_page"),
                AgentEvent::ToolCallResult { id: "c3".into(), result: json!({}), success: false, error: None },
            ],
        );
        assert_eq!(
            lines,
            [
                r#"{"type":"tool-input-start","toolCallId":"c1","toolName":"create_page"}"#,
                r#"{"type":"tool-output-error","toolCallId":"c1","errorText":"the page already exists"}"#,
                r#"{"type":"tool-input-start","toolCallId":"c2","toolName":"create_page"}"#,
                r#"{"type":"tool-output-error","toolCallId":"c2","errorText":"timed out"}"#,
                r#"{"type":"tool-input-start","toolCallId":"c3","toolName":"create_page"}"#,
                r#"{"type":"tool-output-error","toolCallId":"c3","errorText":"the tool reported a failure"}"#,
            ]
        );
        // No text, but calls: still a row.
        let msg = r.into_message("m1", "auto".into(), None).expect("a turn of only tools is kept");
        assert_eq!(msg.content, "");
        assert_eq!(msg.tool_calls.as_ref().unwrap()[1].result, Some(json!({"error": "timed out"})));
        let parts = msg.parts.unwrap();
        assert_eq!(kinds(&parts), ["tool:create_page:output-error"; 3]);
        match &parts[1] {
            UIPart::ToolInvocation { error_text, .. } => assert_eq!(error_text.as_deref(), Some("timed out")),
            other => panic!("expected a tool part, got {other:?}"),
        }
    }

    /// The client gets the image; the row gets a note about it.
    #[test]
    fn a_data_url_reaches_the_client_whole_and_the_row_redacted() {
        let big = format!("data:image/png;base64,{}", "A".repeat(4096));
        let mut r = TurnRecorder::new("m".into());
        let lines = feed(&mut r, vec![tool_start("c1", "generate_image"), tool_ok("c1", json!({"url": big}))]);
        assert!(lines[1].contains(&big));
        let msg = r.into_message("m1", "auto".into(), None).unwrap();
        assert_eq!(
            msg.tool_calls.unwrap()[0].result,
            Some(json!({"url": "[image/png, 4 KB, shown in the chat]"}))
        );
    }

    #[test]
    fn the_interview_finisher_names_its_page() {
        let mut r = TurnRecorder::new("m".into());
        let lines = feed(
            &mut r,
            vec![
                tool_start("c1", "write_it_up"),
                tool_ok("c1", json!({"document_page_id": "page_1"})),
                tool_start("c2", "create_page"),
                tool_ok("c2", json!({"document_page_id": "page_2"})),
            ],
        );
        assert_eq!(
            lines,
            [
                r#"{"type":"tool-input-start","toolCallId":"c1","toolName":"write_it_up"}"#,
                r#"{"type":"tool-output-available","toolCallId":"c1","output":{"document_page_id":"page_1"}}"#,
                r#"{"type":"data-narrative-document","data":{"pageId":"page_1"},"transient":true}"#,
                r#"{"type":"tool-input-start","toolCallId":"c2","toolName":"create_page"}"#,
                r#"{"type":"tool-output-available","toolCallId":"c2","output":{"document_page_id":"page_2"}}"#,
            ]
        );
    }

    #[test]
    fn usage_sums_steps_and_dispatched_workers() {
        let mut r = TurnRecorder::new("m".into());
        let lines = feed(
            &mut r,
            vec![
                AgentEvent::Usage {
                    prompt_tokens: 100,
                    completion_tokens: 10,
                    total_tokens: Some(110),
                    reasoning_tokens: Some(4),
                    cache_read_tokens: Some(50),
                    cache_write_tokens: None,
                    cost_micros: Some(1_000),
                },
                AgentEvent::Usage {
                    prompt_tokens: 200,
                    completion_tokens: 20,
                    total_tokens: None,
                    reasoning_tokens: None,
                    cache_read_tokens: None,
                    cache_write_tokens: Some(7),
                    cost_micros: None,
                },
                tool_start("c1", "dispatch_subagents"),
                tool_ok("c1", json!({"usage": {"input_tokens": 1000, "output_tokens": 300, "cost_micros": 5_000}})),
            ],
        );
        assert_eq!(lines.len(), 2, "usage is never on the wire");
        assert_eq!(
            r.usage(),
            TurnUsage {
                input_tokens: 1300,
                output_tokens: 330,
                reasoning_tokens: 4,
                cache_read_tokens: 50,
                cache_write_tokens: 7,
                cost_micros: 6_000,
            }
        );
    }

    /// A turn that only thought is billed, but is not a row.
    #[test]
    fn a_turn_with_no_text_and_no_tools_is_not_a_row() {
        let mut r = TurnRecorder::new("m".into());
        feed(
            &mut r,
            vec![
                AgentEvent::ReasoningDelta { content: "…".into() },
                AgentEvent::Usage {
                    prompt_tokens: 5,
                    completion_tokens: 5,
                    total_tokens: None,
                    reasoning_tokens: None,
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                    cost_micros: Some(9),
                },
            ],
        );
        assert_eq!(r.usage().cost_micros, 9);
        assert!(r.into_message("m1", "auto".into(), None).is_none());
    }

    #[test]
    fn an_error_mid_turn_closes_the_part_and_marks_the_row() {
        let mut r = TurnRecorder::new("m".into());
        let mut lines = feed(
            &mut r,
            vec![
                text("Partial"),
                AgentEvent::Error {
                    message: "Your server lost the reply midway.".into(),
                    code: Some(ErrorCode::Interrupted),
                    recoverable: false,
                },
            ],
        );
        lines.extend(wire(r.close(false, false)));
        assert_eq!(
            lines,
            [
                r#"{"type":"text-start","id":"m:t1"}"#,
                r#"{"type":"text-delta","id":"m:t1","delta":"Partial"}"#,
                r#"{"type":"error","errorText":"Your server lost the reply midway."}"#,
                r#"{"type":"text-end","id":"m:t1"}"#,
                r#"{"type":"finish","finishReason":"error"}"#,
            ]
        );
        assert_eq!(r.subject(false, false), Some("interrupted"));
    }

    /// The recorded stopped-turn fixture the web client's vitest parses is
    /// exactly what a stopped turn produces.
    #[test]
    fn a_stopped_turn_matches_the_recorded_fixture() {
        let mut r = TurnRecorder::new("msg_fixture".into());
        let mut events = vec![
            StreamEvent::Start { message_id: "msg_fixture".into() },
            StreamEvent::StartStep,
        ];
        events.extend(r.on_event(text("Partial")));
        events.extend(r.close(true, false));
        let have: String = events.iter().map(|e| serialize_event(e) + "\n").collect();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../apps/web/src/lib/ai/fixtures/box-ui-stream-abort.jsonl");
        assert_eq!(have, std::fs::read_to_string(path).expect("the fixture"));
        assert_eq!(r.subject(true, false), Some("cancelled"));
    }

    #[test]
    fn every_ending_has_its_finish_reason_and_subject() {
        use FinishReason as F;
        use StepReason as S;
        let abort = |reason: &str| format!(r#"{{"type":"abort","reason":"{reason}"}}"#);
        let finish = |reason: &str| format!(r#"{{"type":"finish","finishReason":"{reason}"}}"#);
        let cases: Vec<(&str, Vec<AgentEvent>, bool, bool, String, Option<&str>)> = vec![
            ("stopped", vec![], true, false, abort("stopped"), Some("cancelled")),
            ("unattended", vec![], true, true, abort("unattended"), Some("unattended")),
            // The loop saying Cancelled without the token is an abort on the
            // wire, but the row does not claim a stop.
            ("loop cancelled", vec![done(F::Cancelled)], false, false, abort("stopped"), None),
            ("stop wins over a length cut", vec![done(F::OutputLimit)], true, false, abort("stopped"), Some("cancelled")),
            ("loop error", vec![done(F::Error)], false, false, finish("error"), None),
            ("max steps", vec![step(S::ToolCalls), done(F::MaxSteps)], false, false, finish("other"), Some("max_steps")),
            ("budget", vec![done(F::BudgetExceeded)], false, false, finish("other"), Some("budget")),
            ("awaiting user", vec![done(F::AwaitingUser)], false, false, finish("other"), None),
            ("output limit", vec![done(F::OutputLimit)], false, false, finish("length"), Some("length")),
            ("max tokens step", vec![step(S::MaxTokens), done(F::EndTurn)], false, false, finish("length"), Some("length")),
            ("content filter", vec![step(S::ContentFilter), done(F::EndTurn)], false, false, finish("content-filter"), None),
            ("ended on tools", vec![step(S::ToolCalls)], false, false, finish("tool-calls"), None),
            ("clean", vec![step(S::EndTurn), done(F::EndTurn)], false, false, finish("stop"), None),
            ("nothing at all", vec![], false, false, finish("stop"), None),
        ];
        for (name, events, cancelled, unattended, want_line, want_subject) in cases {
            let (lines, subject) = ending(events, cancelled, unattended);
            assert_eq!(lines.last(), Some(&want_line), "{name}");
            assert_eq!(subject, want_subject, "{name}");
        }
    }

    /// Subagent status rides beside the recorder, straight from the tool's
    /// side-channel.
    #[test]
    fn a_subagent_update_is_a_data_event() {
        let update = crate::tools::SubagentUpdate {
            dispatch_id: 9,
            id: 2,
            title: "Prices".into(),
            model: "m".into(),
            status: crate::tools::SubagentStatus::Done,
            tokens: 12,
        };
        assert_eq!(
            serialize_event(&StreamEvent::from(update)),
            r#"{"type":"data-subagent","data":{"dispatchId":9,"subagentId":2,"title":"Prices","model":"m","status":"done","tokens":12},"transient":true}"#
        );
    }
}

/// `parts` built from a turn's pieces, directly.
#[cfg(test)]
mod parts_tests {
    use super::*;

    fn tool(id: &str, name: &str, result: Option<serde_json::Value>) -> ToolCall {
        ToolCall {
            tool_name: name.to_string(),
            tool_call_id: Some(id.to_string()),
            arguments: serde_json::json!({}),
            result,
            timestamp: "2026-09-16T00:00:00Z".to_string(),
        }
    }

    fn kinds(parts: &[UIPart]) -> Vec<String> {
        parts
            .iter()
            .map(|p| match p {
                UIPart::Text { text } => format!("text:{}", text.trim()),
                UIPart::ToolInvocation { tool_name, state, .. } => {
                    format!("tool:{tool_name}:{state}")
                }
                _ => "other".to_string(),
            })
            .collect()
    }

    /// The order is the whole point. Without it a reload can say what was said
    /// and what was called, but never which was said BEFORE which call — and
    /// that distinction is what separates the model narrating its way to an
    /// answer from the answer itself.
    #[test]
    fn a_turn_keeps_the_order_its_text_and_tools_happened_in() {
        let slots = vec![
            TurnSlot::Text(0),
            TurnSlot::Tool("c1".into()),
            TurnSlot::Text(1),
            TurnSlot::Tool("c2".into()),
            TurnSlot::Text(2),
        ];
        let segments = vec![
            "Checking his messages.".to_string(),
            "Nothing in August. Looking at September.".to_string(),
            "He sent it on the 3rd.".to_string(),
        ];
        let calls = vec![
            tool("c1", "sql_query", Some(serde_json::json!({"rows": []}))),
            tool("c2", "semantic_search", Some(serde_json::json!({"rows": []}))),
        ];

        assert_eq!(
            kinds(&build_turn_parts(&slots, &segments, &calls, &Default::default(), "")),
            [
                "text:Checking his messages.",
                "tool:sql_query:output-available",
                "text:Nothing in August. Looking at September.",
                "tool:semantic_search:output-available",
                "text:He sent it on the 3rd.",
            ]
        );
    }

    /// A turn that ends on a tool call has no reply yet. The view reads "text
    /// with a tool after it" as narration, so an empty trailing run must not be
    /// written — it would present itself as an answer of nothing.
    #[test]
    fn empty_runs_and_unrecorded_tools_are_left_out() {
        let slots = vec![
            TurnSlot::Text(0),
            TurnSlot::Tool("c1".into()),
            TurnSlot::Text(1),
            // Started, never recorded: the stream ended in between.
            TurnSlot::Tool("c_ghost".into()),
        ];
        let segments = vec!["Looking it up.".to_string(), "   \n ".to_string()];
        let calls = vec![tool("c1", "sql_query", None)];

        assert_eq!(
            kinds(&build_turn_parts(&slots, &segments, &calls, &Default::default(), "")),
            ["text:Looking it up.", "tool:sql_query:input-available"],
            "a tool that never returned still shows, as awaiting output"
        );
    }

    /// The overwhelmingly common turn: a question, an answer, no tools. It must
    /// come out as one text part, or every plain reply would look like narration.
    #[test]
    fn a_turn_with_no_tools_is_all_reply() {
        let parts = build_turn_parts(&[TurnSlot::Text(0)], &["Yes.".to_string()], &[], &Default::default(), "");
        assert_eq!(kinds(&parts), ["text:Yes."]);
    }

    /// A failed tool is its own state, or the red block never renders and the
    /// replay hands the model an error object as though it were an answer.
    #[test]
    fn a_failed_tool_keeps_its_failure() {
        let mut failed = HashSet::new();
        failed.insert("call-0".to_string());
        let calls = vec![ToolCall {
            tool_name: "create_page".to_string(),
            tool_call_id: Some("call-0".to_string()),
            arguments: serde_json::json!({}),
            result: Some(serde_json::json!({ "error": "the page already exists" })),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        }];
        let parts = build_turn_parts(
            &[TurnSlot::Tool("call-0".to_string())],
            &[],
            &calls,
            &failed,
            "",
        );
        match &parts[0] {
            UIPart::ToolInvocation { state, error_text, .. } => {
                assert_eq!(state, "output-error");
                assert_eq!(error_text.as_deref(), Some("the page already exists"));
            }
            other => panic!("expected a tool part, got {other:?}"),
        }
    }

    /// The thinking has its own column, but the client stopped reading it the
    /// moment `parts` existed.
    #[test]
    fn a_turn_that_thought_keeps_its_thinking() {
        let parts = build_turn_parts(
            &[TurnSlot::Text(0)],
            &["Yes.".to_string()],
            &[],
            &Default::default(),
            "Weighing it up.",
        );
        assert!(matches!(&parts[0], UIPart::Reasoning { text } if text == "Weighing it up."));
        assert!(matches!(&parts[1], UIPart::Text { .. }));
    }
}

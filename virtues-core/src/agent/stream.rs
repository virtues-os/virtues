//! LLM Streaming
//!
//! Handles streaming responses from the LLM (via virtues-api) and parsing
//! the OpenAI-format SSE stream into structured data.

use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

use super::protocol::{AgentEvent, StepReason};

/// Configuration for the LLM client.
///
/// Streams through the device api_key (`BearerClient`); a 402 on an empty
/// wallet triggers one auto-top-up-and-retry. The client itself is cheap to
/// clone (it wraps an `Arc`-backed `reqwest::Client` + `PgPool`).
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub client: crate::virtues_api::client::BearerClient,
}

/// A parsed tool call from the LLM response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// The one key an argument object carries when the model's argument text
/// did not parse. Set at the parse site below, read by
/// `executor::execute_single`, never by a tool.
pub const UNPARSEABLE_ARGUMENTS_KEY: &str = "__unparseable_arguments";

/// Result of streaming an LLM response
#[derive(Debug)]
pub struct LlmStreamResult {
    /// The accumulated text content
    pub content: String,
    /// The gateway's `reasoning_details` for this step, merged by index:
    /// each block's text joined across deltas, the last signature kept.
    pub reasoning_details: Vec<Value>,
    /// Tool calls requested by the LLM
    pub tool_calls: Vec<ToolCall>,
    /// Why the LLM stopped
    pub finish_reason: StepReason,
    /// Token usage
    pub usage: Option<TokenUsage>,
}

/// Token usage information
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub reasoning_tokens: Option<u32>,
    /// Prompt tokens the provider served from its cache, when it says so.
    pub cache_read_tokens: Option<u32>,
    /// Prompt tokens the provider wrote INTO its cache, when it says so. Always
    /// `None` today: see the parse site for why no field is guessed at.
    pub cache_write_tokens: Option<u32>,
    /// Authoritative cost in micros-USD, from the gateway's `usage.cost` (the
    /// real upstream price for this call). `None` if the gateway didn't report
    /// it. Captured into `app_ai_calls` rather than re-estimated locally.
    pub cost_micros: Option<i64>,
}

/// Per-request settings beyond the conversation. The default is what every
/// call sent before these existed: the model's own reasoning default, and
/// `tool_choice: auto` whenever tools are present.
#[derive(Debug, Clone, Default)]
pub struct StepOptions {
    /// From `virtues_api::request::reasoning_for` — the gateway object, and
    /// the effort alias a BYO endpoint reads. At most one is ever set.
    pub reasoning: Option<crate::virtues_api::request::Reasoning>,
    pub reasoning_effort: Option<String>,
    /// Replaces the `auto` sent with tools. The loop sends `"none"` on a
    /// turn's last step so it ends in an answer rather than another call.
    /// The tools stay in the request either way: a conversation holding tool
    /// calls must still declare them, and the provider's cache keys on them.
    pub tool_choice: Option<Value>,
}

/// Post one streaming chat completion and read it through an
/// `SseAccumulator`, emitting each delta as it arrives and returning the step.
pub async fn stream_llm_response<F>(
    config: &LlmConfig,
    model: &str,
    messages: &[Value],
    tools: &[Value],
    provider_options: Option<Value>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    options: StepOptions,
    session_affinity: Option<&str>,
    mut emit: F,
) -> Result<LlmStreamResult, StreamError>
where
    F: FnMut(AgentEvent),
{
    // One builder for every site that posts a chat completion; the proxy
    // forwards the body opaquely, so what is set here is what the gateway
    // sees.
    let request = crate::virtues_api::request::ChatCompletionRequest {
        model: model.to_string(),
        messages: messages.to_vec(),
        stream: Some(true),
        max_tokens,
        tools: if tools.is_empty() { None } else { Some(tools.to_vec()) },
        tool_choice: if tools.is_empty() {
            None
        } else {
            Some(options.tool_choice.unwrap_or_else(|| serde_json::json!("auto")))
        },
        reasoning: options.reasoning,
        reasoning_effort: options.reasoning_effort,
        provider_options,
        temperature,
        ..Default::default()
    };
    let body = serde_json::to_value(&request)
        .map_err(|e| StreamError::ParseError(format!("encode chat request: {e}")))?;

    // Stream through the device api_key. Any auto-top-up-and-retry on a 402
    // happens before the body opens; mid-stream top-up is impossible.
    let response = match config
        .client
        .stream_affine("/v1/ai/chat/completions", &body, session_affinity)
        .await
        .map_err(|e| StreamError::Connection(e.to_string()))?
    {
        crate::virtues_api::client::StreamOutcome::Stream(resp) => resp,
        crate::virtues_api::client::StreamOutcome::Error { status, body } => {
            return Err(StreamError::LlmError {
                status,
                message: body,
            });
        }
    };

    let mut bytes_stream = response.bytes_stream();
    let mut sse = SseAccumulator::new();
    loop {
        match bytes_stream.next().await {
            Some(Ok(chunk)) => {
                if sse.feed(&chunk, &mut emit)? == Flow::Done {
                    break;
                }
            }
            Some(Err(e)) => {
                tracing::error!("Stream error: {}", e);
                return Err(StreamError::Interrupted(format!(
                    "the connection dropped mid-reply: {e}"
                )));
            }
            None => {
                sse.eof(&mut emit)?;
                break;
            }
        }
    }
    sse.finish(&mut emit)
}

/// Whether to keep reading the stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Continue,
    /// `[DONE]` arrived; nothing after it is read.
    Done,
}

/// The state of one streamed chat completion: the SSE line buffer and
/// everything accumulated from the frames so far. Fed bytes as they arrive,
/// it emits each delta as an `AgentEvent` and, at the end, yields the step.
pub struct SseAccumulator {
    buffer: String,
    content: String,
    reasoning_details: Vec<Value>,
    /// Keyed by the provider's call index and ordered by it, so the calls
    /// come out in the order the model made them: the echoed assistant
    /// message is the next step's cache prefix, and a per-tool cap refuses
    /// by order.
    tool_calls: BTreeMap<i64, (String, String, String)>,
    tool_calls_started: HashSet<i64>,
    usage: TokenUsage,
    finish_reason: StepReason,
    /// The model said it was done: a `finish_reason` on a choice, or the
    /// `[DONE]` sentinel. A stream that ends without either did not finish —
    /// the gateway's upstream broke, a proxy gave up, the idle timeout fired.
    ended_cleanly: bool,
}

impl Default for SseAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

impl SseAccumulator {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            content: String::new(),
            reasoning_details: Vec::new(),
            tool_calls: BTreeMap::new(),
            tool_calls_started: HashSet::new(),
            usage: TokenUsage::default(),
            finish_reason: StepReason::EndTurn,
            ended_cleanly: false,
        }
    }

    /// Take one chunk of the body and process every complete line in it.
    pub fn feed(
        &mut self,
        bytes: &[u8],
        emit: &mut impl FnMut(AgentEvent),
    ) -> Result<Flow, StreamError> {
        self.buffer.push_str(&String::from_utf8_lossy(bytes));
        self.drain_lines(emit)
    }

    /// The body closed. A final line that arrived without its newline is
    /// still a line — left in the buffer it would turn a `[DONE]` into a
    /// false interruption.
    pub fn eof(&mut self, emit: &mut impl FnMut(AgentEvent)) -> Result<Flow, StreamError> {
        if !self.buffer.trim().is_empty() && !self.buffer.ends_with('\n') {
            self.buffer.push('\n');
        }
        self.drain_lines(emit)
    }

    fn drain_lines(&mut self, emit: &mut impl FnMut(AgentEvent)) -> Result<Flow, StreamError> {
        while let Some(line_end) = self.buffer.find('\n') {
            let line = self.buffer[..line_end].trim().to_string();
            self.buffer = self.buffer[line_end + 1..].to_string();

            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };
            if data == "[DONE]" {
                self.ended_cleanly = true;
                return Ok(Flow::Done);
            }
            if let Ok(json) = serde_json::from_str::<Value>(data) {
                self.frame(&json, emit)?;
            }
        }
        Ok(Flow::Continue)
    }

    fn frame(&mut self, json: &Value, emit: &mut impl FnMut(AgentEvent)) -> Result<(), StreamError> {
        // The gateway's own frame for an upstream that broke mid-stream
        // (`routes/streaming.rs`): a chat-completions stream has no standard
        // error shape, so this is the one the proxy and the box agree on.
        // Never a model chunk, so nothing is lost by stopping here. A bare
        // `"error": null` is not a report.
        if let Some(err) = json.get("error").filter(|v| !v.is_null()) {
            let message = err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("the gateway reported an error mid-reply")
                .to_string();
            return Err(StreamError::Interrupted(message));
        }

        if let Some(choice) = json.get("choices").and_then(|c| c.as_array()).and_then(|c| c.first()) {
            if let Some(delta) = choice.get("delta") {
                self.delta(delta, emit);
            }
            if let Some(reason) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                self.ended_cleanly = true;
                self.finish_reason = match reason {
                    "tool_calls" => StepReason::ToolCalls,
                    "length" => StepReason::MaxTokens,
                    "content_filter" => StepReason::ContentFilter,
                    _ => StepReason::EndTurn,
                };
            }
        }

        // Null-guarded like the error frame: many OpenAI-compatible streams
        // carry `"usage": null` on every chunk, which must not zero counts an
        // earlier chunk set. Each field is taken only when present, for the
        // same reason.
        if let Some(usage) = json.get("usage").filter(|v| !v.is_null()) {
            self.read_usage(usage);
        }
        Ok(())
    }

    fn delta(&mut self, delta: &Value, emit: &mut impl FnMut(AgentEvent)) {
        if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
            if !content.is_empty() {
                self.content.push_str(content);
                emit(AgentEvent::text(content));
            }
        }

        // The gateway streams reasoning as `delta.reasoning`;
        // `reasoning_content` is the DeepSeek-style spelling a BYO endpoint
        // may use.
        let reasoning = delta
            .get("reasoning")
            .or_else(|| delta.get("reasoning_content"))
            .and_then(|r| r.as_str());
        if let Some(reasoning) = reasoning {
            if !reasoning.is_empty() {
                emit(AgentEvent::reasoning(reasoning));
            }
        }

        // The gateway's structured reasoning blocks, to be echoed back on
        // this step's assistant message.
        if let Some(details) = delta.get("reasoning_details").and_then(|d| d.as_array()) {
            for d in details {
                merge_reasoning_detail(&mut self.reasoning_details, d.clone());
            }
        }

        let Some(tool_calls) = delta.get("tool_calls").and_then(|t| t.as_array()) else {
            return;
        };
        for tool_call in tool_calls {
            let idx = tool_call.get("index").and_then(|i| i.as_i64()).unwrap_or(0);
            let tc_id = tool_call.get("id").and_then(|i| i.as_str()).unwrap_or("");
            let Some(function) = tool_call.get("function") else {
                continue;
            };
            let name = function.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let args = function.get("arguments").and_then(|a| a.as_str()).unwrap_or("");

            let entry = self
                .tool_calls
                .entry(idx)
                .or_insert_with(|| (tc_id.to_string(), String::new(), String::new()));
            if !tc_id.is_empty() {
                entry.0 = tc_id.to_string();
            }
            if !name.is_empty() {
                entry.1 = name.to_string();
            }
            entry.2.push_str(args);

            // Started off the ACCUMULATED id and name, not this chunk's: a
            // provider may send the id in one frame and the name in the next.
            if !entry.0.is_empty() && !entry.1.is_empty() && !self.tool_calls_started.contains(&idx) {
                self.tool_calls_started.insert(idx);
                emit(AgentEvent::tool_start(entry.0.clone(), entry.1.clone()));
            }
            if !args.is_empty() && self.tool_calls_started.contains(&idx) {
                emit(AgentEvent::ToolCallArgsPartial {
                    id: entry.0.clone(),
                    args_delta: args.to_string(),
                });
            }
        }
    }

    fn read_usage(&mut self, usage_obj: &Value) {
        tracing::debug!(usage = %usage_obj, "raw gateway usage object");
        let usage = &mut self.usage;
        if let Some(t) = usage_obj.get("prompt_tokens").and_then(|t| t.as_u64()) {
            usage.prompt_tokens = t as u32;
        }
        if let Some(t) = usage_obj.get("completion_tokens").and_then(|t| t.as_u64()) {
            usage.completion_tokens = t as u32;
        }
        if let Some(details) = usage_obj.get("prompt_tokens_details") {
            if let Some(t) = details.get("cached_tokens").and_then(|t| t.as_u64()) {
                usage.cache_read_tokens = Some(t as u32);
            }
            // Cache WRITES have no OpenAI-compatible spelling, and what the
            // gateway calls them here is not known, so `cache_write_tokens`
            // stays None rather than a guessed field. The keys we do not read
            // are logged so one real payload can name it.
            if let Some(map) = details.as_object() {
                let unread: Vec<&String> =
                    map.keys().filter(|k| k.as_str() != "cached_tokens").collect();
                if !unread.is_empty() {
                    tracing::debug!(
                        keys = ?unread,
                        details = %details,
                        "gateway prompt_tokens_details carries fields we do not read"
                    );
                }
            }
        }
        if let Some(details) = usage_obj.get("completion_tokens_details") {
            usage.reasoning_tokens = details
                .get("reasoning_tokens")
                .and_then(|t| t.as_u64())
                .map(|t| t as u32);
        }
        // The gateway's authoritative cost (USD float → micros), kept rather
        // than re-estimated from a local price table.
        if let Some(cost) = usage_obj.get("cost").and_then(|c| c.as_f64()) {
            usage.cost_micros = Some((cost * 1_000_000.0).round() as i64);
        }
    }

    /// The step as streamed: refused as `Interrupted` if the model never said
    /// it was done, since everything streamed so far already reached the
    /// caller and only the claim that it was the whole reply is withheld.
    pub fn finish(self, emit: &mut impl FnMut(AgentEvent)) -> Result<LlmStreamResult, StreamError> {
        if !self.ended_cleanly {
            return Err(StreamError::Interrupted(
                "the reply stopped before the model finished".to_string(),
            ));
        }

        let tool_calls: Vec<ToolCall> = self
            .tool_calls
            .into_values()
            .filter(|(id, name, _)| !id.is_empty() && !name.is_empty())
            .map(|(id, name, args_str)| {
                // No arguments at all is a call to a tool that takes none —
                // some providers send `""` rather than `{}`. Arguments that
                // arrived and do not parse carry the marker, which the
                // executor turns into a failure the model can act on.
                let arguments = if args_str.trim().is_empty() {
                    Value::Object(Default::default())
                } else {
                    match serde_json::from_str::<Value>(&args_str) {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::warn!(tool_call_id = %id, tool_name = %name, error = %e, "tool call arguments were not JSON");
                            serde_json::json!({
                                UNPARSEABLE_ARGUMENTS_KEY: {
                                    "error": e.to_string(),
                                    "raw": args_str.chars().take(400).collect::<String>(),
                                }
                            })
                        }
                    }
                };
                emit(AgentEvent::ToolCallArgsComplete {
                    id: id.clone(),
                    args: arguments.clone(),
                });
                ToolCall { id, name, arguments }
            })
            .collect();

        let usage = self.usage;
        if usage.prompt_tokens > 0 || usage.completion_tokens > 0 {
            emit(AgentEvent::Usage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: Some(usage.prompt_tokens + usage.completion_tokens),
                reasoning_tokens: usage.reasoning_tokens,
                cache_read_tokens: usage.cache_read_tokens,
                cache_write_tokens: usage.cache_write_tokens,
                cost_micros: usage.cost_micros,
            });
        }

        Ok(LlmStreamResult {
            content: self.content,
            reasoning_details: self.reasoning_details,
            tool_calls,
            finish_reason: self.finish_reason,
            usage: Some(usage),
        })
    }
}

/// Fold one streamed `reasoning_details` entry into the step's list.
///
/// A block arrives as many deltas sharing an `index`: text fragments to be
/// joined, and a `signature` (Anthropic) or encrypted `data` (OpenAI) that
/// the last fragment carries whole. Joined text plus the latest scalar
/// fields is the block as the provider wants it back. An entry with no
/// index is its own block.
fn merge_reasoning_detail(blocks: &mut Vec<Value>, incoming: Value) {
    let idx = incoming.get("index").and_then(|i| i.as_i64());
    let slot = idx.and_then(|i| {
        blocks
            .iter()
            .position(|b| b.get("index").and_then(|x| x.as_i64()) == Some(i))
    });
    match slot {
        Some(pos) => {
            let existing = &mut blocks[pos];
            if let (Some(have), Some(more)) = (
                existing.get("text").and_then(|t| t.as_str()).map(str::to_string),
                incoming.get("text").and_then(|t| t.as_str()),
            ) {
                existing["text"] = Value::String(have + more);
            }
            if let (Some(obj), Some(new)) = (existing.as_object_mut(), incoming.as_object()) {
                for (k, v) in new {
                    if k != "text" && !v.is_null() {
                        obj.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        None => blocks.push(incoming),
    }
}

/// Errors that can occur during streaming
#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    #[error("Connection failed: {0}")]
    Connection(String),

    #[error("LLM error (status {status}): {message}")]
    LlmError { status: u16, message: String },

    #[error("Parse error: {0}")]
    ParseError(String),

    /// The stream ended before the model finished: a dropped connection, the
    /// idle timeout, the gateway's error frame, or bytes that simply stopped
    /// with no `finish_reason` and no `[DONE]`. What streamed before the cut
    /// has already been emitted; the message says why the rest never came.
    #[error("Stream interrupted: {0}")]
    Interrupted(String),
}

#[cfg(test)]
mod reasoning_detail_tests {
    use super::*;
    use serde_json::json;

    /// The gateway's streaming shape: one block, many deltas, the signature
    /// on the last. Echoing it back means one block with the whole text.
    #[test]
    fn deltas_of_one_block_fold_into_one_entry() {
        let mut blocks = Vec::new();
        merge_reasoning_detail(&mut blocks, json!({"type": "reasoning.text", "text": "Let me ", "index": 0, "format": "anthropic-claude-v1"}));
        merge_reasoning_detail(&mut blocks, json!({"type": "reasoning.text", "text": "think.", "index": 0, "signature": "sig-xyz"}));
        merge_reasoning_detail(&mut blocks, json!({"type": "reasoning.encrypted", "data": "enc", "index": 1}));
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["text"], "Let me think.");
        assert_eq!(blocks[0]["signature"], "sig-xyz");
        assert_eq!(blocks[0]["format"], "anthropic-claude-v1");
        assert_eq!(blocks[1]["data"], "enc");
    }
}

#[cfg(test)]
mod sse_tests {
    use super::*;
    use serde_json::json;

    /// Feed each string as one network chunk, close the body, finish.
    fn run(chunks: &[&str]) -> (Vec<Value>, Result<LlmStreamResult, StreamError>) {
        let mut events = Vec::new();
        let mut emit = |e: AgentEvent| events.push(serde_json::to_value(e).unwrap());
        let mut sse = SseAccumulator::new();
        let mut read = || -> Result<LlmStreamResult, StreamError> {
            for chunk in chunks {
                if sse.feed(chunk.as_bytes(), &mut emit)? == Flow::Done {
                    return std::mem::take(&mut sse).finish(&mut emit);
                }
            }
            sse.eof(&mut emit)?;
            std::mem::take(&mut sse).finish(&mut emit)
        };
        let result = read();
        (events, result)
    }

    fn data(v: Value) -> String {
        format!("data: {v}\n\n")
    }

    fn delta(d: Value) -> String {
        data(json!({"choices": [{"delta": d}]}))
    }

    fn stop(reason: &str) -> String {
        data(json!({"choices": [{"delta": {}, "finish_reason": reason}]}))
    }

    fn of_type<'a>(events: &'a [Value], ty: &str) -> Vec<&'a Value> {
        events.iter().filter(|e| e["type"] == ty).collect()
    }

    #[test]
    fn text_deltas_stream_and_accumulate() {
        let (events, r) = run(&[&delta(json!({"content": "Hel"})), &delta(json!({"content": ""})), &delta(json!({"content": "lo"})), &stop("stop")]);
        let r = r.unwrap();
        assert_eq!(r.content, "Hello");
        assert_eq!(r.finish_reason, StepReason::EndTurn);
        let texts: Vec<_> = of_type(&events, "text_delta").iter().map(|e| e["content"].clone()).collect();
        assert_eq!(texts, [json!("Hel"), json!("lo")]);
    }

    #[test]
    fn a_line_split_across_chunks_is_one_line() {
        let frame = delta(json!({"content": "whole"}));
        let (head, tail) = frame.split_at(10);
        let (_, r) = run(&[head, tail, &stop("stop")]);
        assert_eq!(r.unwrap().content, "whole");
    }

    #[test]
    fn reasoning_arrives_under_either_spelling() {
        let (events, r) = run(&[
            &delta(json!({"reasoning": "gateway "})),
            &delta(json!({"reasoning_content": "byo"})),
            &delta(json!({"reasoning": ""})),
            &stop("stop"),
        ]);
        r.unwrap();
        let said: Vec<_> = of_type(&events, "reasoning_delta").iter().map(|e| e["content"].clone()).collect();
        assert_eq!(said, [json!("gateway "), json!("byo")]);
    }

    #[test]
    fn reasoning_details_merge_by_index() {
        let (_, r) = run(&[
            &delta(json!({"reasoning_details": [{"type": "reasoning.text", "text": "a", "index": 0}]})),
            &delta(json!({"reasoning_details": [{"type": "reasoning.text", "text": "b", "index": 0, "signature": "s"}]})),
            &stop("stop"),
        ]);
        let details = r.unwrap().reasoning_details;
        assert_eq!(details.len(), 1);
        assert_eq!(details[0]["text"], "ab");
        assert_eq!(details[0]["signature"], "s");
    }

    #[test]
    fn a_tool_call_with_id_and_name_in_separate_frames_starts_once_both_are_known() {
        let (events, r) = run(&[
            &delta(json!({"tool_calls": [{"index": 0, "id": "call_1", "function": {"arguments": ""}}]})),
            &delta(json!({"tool_calls": [{"index": 0, "function": {"name": "sql_query", "arguments": "{\"sql\":"}}]})),
            &delta(json!({"tool_calls": [{"index": 0, "function": {"arguments": "\"SELECT 1\"}"}}]})),
            &stop("tool_calls"),
        ]);
        let r = r.unwrap();
        assert_eq!(r.finish_reason, StepReason::ToolCalls);
        assert_eq!(r.tool_calls.len(), 1);
        assert_eq!(r.tool_calls[0].id, "call_1");
        assert_eq!(r.tool_calls[0].name, "sql_query");
        assert_eq!(r.tool_calls[0].arguments, json!({"sql": "SELECT 1"}));

        let starts = of_type(&events, "tool_call_start");
        assert_eq!(starts.len(), 1);
        assert_eq!(starts[0]["id"], "call_1");
        assert_eq!(starts[0]["name"], "sql_query");
        let partials: Vec<_> = of_type(&events, "tool_call_args_partial").iter().map(|e| e["args_delta"].clone()).collect();
        assert_eq!(partials, [json!("{\"sql\":"), json!("\"SELECT 1\"}")]);
        let complete = of_type(&events, "tool_call_args_complete");
        assert_eq!(complete.len(), 1);
        assert_eq!(complete[0]["args"], json!({"sql": "SELECT 1"}));
    }

    #[test]
    fn parallel_tool_calls_come_out_in_index_order() {
        let (_, r) = run(&[
            &delta(json!({"tool_calls": [{"index": 1, "id": "b", "function": {"name": "two", "arguments": "{}"}}]})),
            &delta(json!({"tool_calls": [{"index": 0, "id": "a", "function": {"name": "one", "arguments": ""}}]})),
            &stop("tool_calls"),
        ]);
        let calls = r.unwrap().tool_calls;
        let ids: Vec<_> = calls.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["a", "b"]);
        // No arguments at all is a call to a tool that takes none.
        assert_eq!(calls[0].arguments, json!({}));
    }

    #[test]
    fn a_call_missing_its_name_is_dropped() {
        let (_, r) = run(&[
            &delta(json!({"tool_calls": [{"index": 0, "id": "a", "function": {"arguments": "{}"}}]})),
            &stop("tool_calls"),
        ]);
        assert!(r.unwrap().tool_calls.is_empty());
    }

    #[test]
    fn unparseable_arguments_carry_the_marker() {
        let (events, r) = run(&[
            &delta(json!({"tool_calls": [{"index": 0, "id": "a", "function": {"name": "sql_query", "arguments": "{\"sql\": SELECT"}}]})),
            &stop("tool_calls"),
        ]);
        let args = &r.unwrap().tool_calls[0].arguments;
        let bad = &args[UNPARSEABLE_ARGUMENTS_KEY];
        assert!(bad["error"].as_str().is_some_and(|e| !e.is_empty()));
        assert_eq!(bad["raw"], "{\"sql\": SELECT");
        assert_eq!(of_type(&events, "tool_call_args_complete")[0]["args"], *args);
    }

    #[test]
    fn a_null_usage_after_real_usage_does_not_wipe_it() {
        let (events, r) = run(&[
            &data(json!({"choices": [{"delta": {"content": "x"}}], "usage": null})),
            &data(json!({"choices": [], "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 20,
                "prompt_tokens_details": {"cached_tokens": 80},
                "completion_tokens_details": {"reasoning_tokens": 5},
                "cost": 0.001234
            }})),
            &data(json!({"choices": [{"delta": {}, "finish_reason": "stop"}], "usage": null})),
        ]);
        let usage = r.unwrap().usage.unwrap();
        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 20);
        assert_eq!(usage.cache_read_tokens, Some(80));
        assert_eq!(usage.reasoning_tokens, Some(5));
        assert_eq!(usage.cache_write_tokens, None);
        assert_eq!(usage.cost_micros, Some(1234));
        let reported = of_type(&events, "usage");
        assert_eq!(reported.len(), 1);
        assert_eq!(reported[0]["total_tokens"], 120);
        assert_eq!(reported[0]["cost_micros"], 1234);
    }

    #[test]
    fn no_usage_means_no_usage_event() {
        let (events, r) = run(&[&delta(json!({"content": "x"})), &stop("stop")]);
        assert!(of_type(&events, "usage").is_empty());
        let usage = r.unwrap().usage.unwrap();
        assert_eq!((usage.prompt_tokens, usage.cost_micros), (0, None));
    }

    #[test]
    fn done_without_a_trailing_newline_ends_cleanly() {
        let (_, r) = run(&[&delta(json!({"content": "x"})), "data: [DONE]"]);
        assert_eq!(r.unwrap().content, "x");
    }

    #[test]
    fn nothing_after_done_is_read() {
        let (events, r) = run(&[&delta(json!({"content": "x"})), "data: [DONE]\n\n", &delta(json!({"content": "late"}))]);
        assert_eq!(r.unwrap().content, "x");
        assert_eq!(of_type(&events, "text_delta").len(), 1);
    }

    #[test]
    fn a_gateway_error_frame_interrupts_with_its_message() {
        let (events, r) = run(&[
            &delta(json!({"content": "half"})),
            &data(json!({"error": {"message": "upstream went away"}})),
            &stop("stop"),
        ]);
        match r {
            Err(StreamError::Interrupted(m)) => assert_eq!(m, "upstream went away"),
            other => panic!("expected Interrupted, got {other:?}"),
        }
        assert_eq!(of_type(&events, "text_delta").len(), 1);
    }

    #[test]
    fn a_null_error_is_not_a_report() {
        let (_, r) = run(&[&data(json!({"error": null, "choices": [{"delta": {"content": "x"}, "finish_reason": "stop"}]}))]);
        assert_eq!(r.unwrap().content, "x");
    }

    #[test]
    fn a_stream_that_stops_without_finishing_is_interrupted() {
        let (events, r) = run(&[&delta(json!({"content": "half a sent"}))]);
        assert!(matches!(r, Err(StreamError::Interrupted(_))), "{r:?}");
        assert_eq!(of_type(&events, "text_delta").len(), 1);
    }

    #[test]
    fn finish_reasons_map_to_step_reasons() {
        for (wire, want) in [
            ("length", StepReason::MaxTokens),
            ("content_filter", StepReason::ContentFilter),
            ("tool_calls", StepReason::ToolCalls),
            ("stop", StepReason::EndTurn),
            ("something_new", StepReason::EndTurn),
        ] {
            let (_, r) = run(&[&stop(wire)]);
            assert_eq!(r.unwrap().finish_reason, want, "{wire}");
        }
    }

    #[test]
    fn lines_that_are_not_data_are_skipped() {
        let (_, r) = run(&[": keep-alive\n\nevent: ping\n", "data: not json\n", &delta(json!({"content": "x"})), &stop("stop")]);
        assert_eq!(r.unwrap().content, "x");
    }
}

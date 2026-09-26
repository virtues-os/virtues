//! LLM Streaming
//!
//! Handles streaming responses from the LLM (via virtues-api) and parsing
//! the OpenAI-format SSE stream into structured data.

use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

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
    /// The accumulated reasoning content (if any)
    pub reasoning: String,
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

/// Stream an LLM response and emit events
///
/// This function:
/// 1. Makes a streaming request to the LLM
/// 2. Parses the SSE stream
/// 3. Emits AgentEvents for each chunk
/// 4. Returns the accumulated result
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
    // sees. (The hand-built JSON this replaced sent `provider_options` to a
    // proxy that re-typed the body without that field, for three months,
    // silently.)
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

    // Process the stream
    let mut bytes_stream = response.bytes_stream();
    let mut buffer = String::new();
    
    // Accumulated content
    let mut full_content = String::new();
    let mut reasoning_content = String::new();
    let mut reasoning_details: Vec<Value> = Vec::new();
    let mut in_reasoning = false;
    
    // Tool call tracking
    let mut tool_calls_map: HashMap<i64, (String, String, String)> = HashMap::new();
    let mut tool_calls_started: HashSet<i64> = HashSet::new();
    
    // Token usage
    let mut usage = TokenUsage::default();
    let mut finish_reason = StepReason::EndTurn;
    // The model said it was done: a `finish_reason` on a choice, or the
    // `[DONE]` sentinel. A stream that ends without either did not finish —
    // the gateway's upstream broke, a proxy gave up, the idle timeout fired.
    // This used to be indistinguishable from a normal end: the loop broke,
    // `finish_reason` kept its `EndTurn` default, and the half-reply was
    // saved as the answer with nothing on screen to say so (VIR-334).
    let mut ended_cleanly = false;

    let mut upstream_open = true;
    'read: while upstream_open {
        match bytes_stream.next().await {
            Some(Ok(chunk)) => buffer.push_str(&String::from_utf8_lossy(&chunk)),
            Some(Err(e)) => {
                tracing::error!("Stream error: {}", e);
                return Err(StreamError::Interrupted(format!(
                    "the connection dropped mid-reply: {e}"
                )));
            }
            None => {
                // The stream closed. A final line that arrived without its
                // newline is still a line — left in the buffer it would turn
                // a `[DONE]` into a false interruption.
                upstream_open = false;
                if !buffer.trim().is_empty() && !buffer.ends_with('\n') {
                    buffer.push('\n');
                }
            }
        }

        // Process complete SSE lines
        while let Some(line_end) = buffer.find('\n') {
            let line = buffer[..line_end].trim().to_string();
            buffer = buffer[line_end + 1..].to_string();

            if line.is_empty() || !line.starts_with("data: ") {
                continue;
            }

            let data = &line[6..]; // Strip "data: " prefix

            if data == "[DONE]" {
                ended_cleanly = true;
                break 'read;
            }

            // Parse the SSE data as JSON
            if let Ok(json) = serde_json::from_str::<Value>(data) {
                // The gateway's own frame for an upstream that broke mid-stream
                // (`routes/streaming.rs`): a chat-completions stream has no
                // standard error shape, so this is the one the proxy and the
                // box agree on. Never a model chunk, so nothing is lost by
                // stopping here. A bare `"error": null` is not a report.
                if let Some(err) = json.get("error").filter(|v| !v.is_null()) {
                    let message = err
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("the gateway reported an error mid-reply")
                        .to_string();
                    return Err(StreamError::Interrupted(message));
                }

                if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
                    if let Some(choice) = choices.first() {
                        if let Some(delta) = choice.get("delta") {
                            // Handle content delta
                            if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                if !content.is_empty() {
                                    full_content.push_str(content);
                                    emit(AgentEvent::text(content));
                                }
                            }

                            // Handle reasoning delta. The gateway streams it
                            // as `delta.reasoning`; `reasoning_content` is the
                            // DeepSeek-style spelling a BYO endpoint may use.
                            // This read only the second for three months, so
                            // the thinking block stayed empty on the gateway.
                            let reasoning = delta
                                .get("reasoning")
                                .or_else(|| delta.get("reasoning_content"))
                                .and_then(|r| r.as_str());
                            if let Some(reasoning) = reasoning {
                                if !reasoning.is_empty() {
                                    if !in_reasoning {
                                        in_reasoning = true;
                                    }
                                    reasoning_content.push_str(reasoning);
                                    emit(AgentEvent::reasoning(reasoning));
                                }
                            }

                            // The gateway's structured reasoning blocks, to be
                            // echoed back on this step's assistant message.
                            if let Some(details) = delta.get("reasoning_details").and_then(|d| d.as_array()) {
                                for d in details {
                                    merge_reasoning_detail(&mut reasoning_details, d.clone());
                                }
                            }

                            // Handle tool call streaming
                            if let Some(tool_calls) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                                for tool_call in tool_calls {
                                    let idx = tool_call.get("index").and_then(|i| i.as_i64()).unwrap_or(0);
                                    let tc_id = tool_call.get("id").and_then(|i| i.as_str()).unwrap_or("");
                                    
                                    if let Some(function) = tool_call.get("function") {
                                        let name = function.get("name").and_then(|n| n.as_str()).unwrap_or("");
                                        let args = function.get("arguments").and_then(|a| a.as_str()).unwrap_or("");
                                        
                                        // Track or update this tool call
                                        let entry = tool_calls_map.entry(idx).or_insert_with(|| {
                                            (tc_id.to_string(), String::new(), String::new())
                                        });
                                        
                                        if !tc_id.is_empty() {
                                            entry.0 = tc_id.to_string();
                                        }
                                        if !name.is_empty() {
                                            entry.1 = name.to_string();
                                        }
                                        entry.2.push_str(args);
                                        
                                        // Emit start event on first encounter.
                                        //
                                        // Off the ACCUMULATED id and name, not
                                        // this chunk's: a provider that sends
                                        // the id in one frame and the name in
                                        // the next never satisfied both at
                                        // once, so the start never fired, every
                                        // argument delta was suppressed with
                                        // it, and the call never made it into
                                        // the turn's record at all — no name,
                                        // no result, absent from the saved
                                        // parts.
                                        if !entry.0.is_empty() && !entry.1.is_empty() && !tool_calls_started.contains(&idx) {
                                            tool_calls_started.insert(idx);
                                            emit(AgentEvent::tool_start(entry.0.clone(), entry.1.clone()));
                                        }

                                        // Emit delta for arguments
                                        if !args.is_empty() && tool_calls_started.contains(&idx) {
                                            emit(AgentEvent::ToolCallArgsPartial {
                                                id: entry.0.clone(),
                                                args_delta: args.to_string(),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        
                        // Check for finish_reason
                        if let Some(reason) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                            ended_cleanly = true;
                            finish_reason = match reason {
                                "tool_calls" => StepReason::ToolCalls,
                                "length" => StepReason::MaxTokens,
                                "content_filter" => StepReason::ContentFilter,
                                _ => StepReason::EndTurn,
                            };
                        }
                    }
                }
                
                // Extract token usage.
                //
                // Null-guarded like the error frame above: many
                // OpenAI-compatible streams carry `"usage": null` on every
                // chunk, and an unguarded `get` on one of those ASSIGNED zero
                // over counts a previous chunk had already set. Each field is
                // taken only when present, for the same reason.
                if let Some(usage_obj) = json.get("usage").filter(|v| !v.is_null()) {
                    tracing::debug!(usage = %usage_obj, "raw gateway usage object");
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
                        // Cache WRITES have no OpenAI-compatible spelling — they
                        // are an Anthropic concept, and what (if anything) the
                        // gateway calls them through this surface is not known
                        // from the repo. So: no guess. Log the keys we did not
                        // read, once per turn that carries any, and let one real
                        // payload name the field instead of a plausible-looking
                        // constant that silently records nothing.
                        //
                        // Until then `cache_write_tokens` stays None and lands
                        // as 0 — but as "nobody reported it", not as a literal
                        // typed into the insert (which is what it was).
                        if let Some(map) = details.as_object() {
                            let unread: Vec<&String> = map
                                .keys()
                                .filter(|k| k.as_str() != "cached_tokens")
                                .collect();
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

                    // Authoritative cost from the gateway (USD float → micros).
                    // Keep it instead of re-estimating from a local price table.
                    if let Some(cost) = usage_obj.get("cost").and_then(|c| c.as_f64()) {
                        usage.cost_micros = Some((cost * 1_000_000.0).round() as i64);
                    }
                }
            }
        }
    }

    if !ended_cleanly {
        // The bytes stopped before the model said it was done. Everything
        // streamed so far already reached the caller through `emit`; what is
        // refused here is the claim that it was the whole reply.
        return Err(StreamError::Interrupted(
            "the reply stopped before the model finished".to_string(),
        ));
    }

    // Parse accumulated tool calls
    let tool_calls: Vec<ToolCall> = tool_calls_map
        .into_values()
        .filter(|(id, name, _)| !id.is_empty() && !name.is_empty())
        .map(|(id, name, args_str)| {
            // No arguments at all is a legitimate call to a tool that takes
            // none — some providers send `""` rather than `{}` for it.
            // Arguments that ARRIVED and do not parse are a different thing:
            // they used to become `{}` here, silently, and the tool then
            // failed on a missing field. The marker carries the parse error
            // and the head of the text to the executor, which turns them
            // into the failure the model can act on.
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
            
            // Emit args complete event
            emit(AgentEvent::ToolCallArgsComplete {
                id: id.clone(),
                args: arguments.clone(),
            });
            
            ToolCall { id, name, arguments }
        })
        .collect();

    // Emit usage if available
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
        content: full_content,
        reasoning: reasoning_content,
        reasoning_details,
        tool_calls,
        finish_reason,
        usage: Some(usage),
    })
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

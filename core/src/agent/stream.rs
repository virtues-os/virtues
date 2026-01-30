//! LLM Streaming
//!
//! Handles streaming responses from the LLM (via Tollbooth) and parsing
//! the OpenAI-format SSE stream into structured data.

use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use super::protocol::{AgentEvent, StepReason};

/// Configuration for the LLM client
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub tollbooth_url: String,
    pub tollbooth_user_id: String,
    pub tollbooth_secret: String,
}

/// A parsed tool call from the LLM response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// Result of streaming an LLM response
#[derive(Debug)]
pub struct LlmStreamResult {
    /// The accumulated text content
    pub content: String,
    /// The accumulated reasoning content (if any)
    pub reasoning: String,
    /// Gemini thought signature (if any)
    pub thought_signature: Option<String>,
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
    thought_signature: Option<String>,
    mut emit: F,
) -> Result<LlmStreamResult, StreamError>
where
    F: FnMut(AgentEvent),
{
    // Build request body
    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": true
    });

    if !tools.is_empty() {
        body["tools"] = serde_json::json!(tools);
        body["tool_choice"] = serde_json::json!("auto");
    }

    if let Some(opts) = provider_options {
        body["provider_options"] = opts;
    }

    if let Some(sig) = thought_signature {
        body["thought_signature"] = serde_json::json!(sig);
    }

    // Make streaming request to Tollbooth
    let client = crate::http_client::tollbooth_streaming_client();
    let response = crate::tollbooth::with_tollbooth_auth(
        client.post(format!("{}/v1/chat/completions", config.tollbooth_url)),
        &config.tollbooth_user_id,
        &config.tollbooth_secret,
    )
    .header("Content-Type", "application/json")
    .json(&body)
    .send()
    .await
    .map_err(|e| StreamError::Connection(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let error_body = response.text().await.unwrap_or_default();
        return Err(StreamError::LlmError {
            status,
            message: error_body,
        });
    }

    // Process the stream
    let mut bytes_stream = response.bytes_stream();
    let mut buffer = String::new();
    
    // Accumulated content
    let mut full_content = String::new();
    let mut reasoning_content = String::new();
    let mut thought_signature: Option<String> = None;
    let mut in_reasoning = false;
    
    // Tool call tracking
    let mut tool_calls_map: HashMap<i64, (String, String, String)> = HashMap::new();
    let mut tool_calls_started: HashSet<i64> = HashSet::new();
    
    // Token usage
    let mut usage = TokenUsage::default();
    let mut finish_reason = StepReason::EndTurn;

    while let Some(chunk) = bytes_stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Stream error: {}", e);
                break;
            }
        };

        buffer.push_str(&String::from_utf8_lossy(&chunk));

        // Process complete SSE lines
        while let Some(line_end) = buffer.find('\n') {
            let line = buffer[..line_end].trim().to_string();
            buffer = buffer[line_end + 1..].to_string();

            if line.is_empty() || !line.starts_with("data: ") {
                continue;
            }

            let data = &line[6..]; // Strip "data: " prefix

            if data == "[DONE]" {
                break;
            }

            // Parse the SSE data as JSON
            if let Ok(json) = serde_json::from_str::<Value>(data) {
                // Extract thought signature if present (check top-level and choices)
                if let Some(sig) = json.get("thought_signature").and_then(|s| s.as_str()) {
                    thought_signature = Some(sig.to_string());
                    emit(AgentEvent::ThoughtSignature { signature: sig.to_string() });
                } else if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
                    if let Some(choice) = choices.first() {
                        if let Some(sig) = choice.get("thought_signature").and_then(|s| s.as_str()) {
                            thought_signature = Some(sig.to_string());
                            emit(AgentEvent::ThoughtSignature { signature: sig.to_string() });
                        }
                    }
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

                            // Handle reasoning delta
                            if let Some(reasoning) = delta.get("reasoning_content").and_then(|r| r.as_str()) {
                                if !reasoning.is_empty() {
                                    if !in_reasoning {
                                        in_reasoning = true;
                                    }
                                    reasoning_content.push_str(reasoning);
                                    emit(AgentEvent::reasoning(reasoning));
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
                                        
                                        // Emit start event on first encounter
                                        if !tc_id.is_empty() && !name.is_empty() && !tool_calls_started.contains(&idx) {
                                            tool_calls_started.insert(idx);
                                            emit(AgentEvent::tool_start(tc_id, name));
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
                            finish_reason = match reason {
                                "tool_calls" => StepReason::ToolCalls,
                                "length" => StepReason::MaxTokens,
                                "content_filter" => StepReason::ContentFilter,
                                _ => StepReason::EndTurn,
                            };
                        }
                    }
                }
                
                // Extract token usage
                if let Some(usage_obj) = json.get("usage") {
                    usage.prompt_tokens = usage_obj
                        .get("prompt_tokens")
                        .and_then(|t| t.as_u64())
                        .unwrap_or(0) as u32;
                    usage.completion_tokens = usage_obj
                        .get("completion_tokens")
                        .and_then(|t| t.as_u64())
                        .unwrap_or(0) as u32;
                    
                    if let Some(details) = usage_obj.get("completion_tokens_details") {
                        usage.reasoning_tokens = details
                            .get("reasoning_tokens")
                            .and_then(|t| t.as_u64())
                            .map(|t| t as u32);
                    }
                }
            }
        }
    }

    // Parse accumulated tool calls
    let tool_calls: Vec<ToolCall> = tool_calls_map
        .into_values()
        .filter(|(id, name, _)| !id.is_empty() && !name.is_empty())
        .map(|(id, name, args_str)| {
            let arguments = serde_json::from_str(&args_str).unwrap_or(Value::Object(Default::default()));
            
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
        });
    }

    Ok(LlmStreamResult {
        content: full_content,
        reasoning: reasoning_content,
        thought_signature,
        tool_calls,
        finish_reason,
        usage: Some(usage),
    })
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

    #[error("Stream interrupted")]
    Interrupted,
}

/// Build provider options for reasoning models
pub fn build_provider_options(model: &str) -> Option<Value> {
    // Check if model supports extended thinking (Claude 3.5+ with thinking, DeepSeek)
    let supports_thinking = model.contains("claude-3")
        || model.contains("deepseek")
        || model.contains("o1")
        || model.contains("o3");

    if supports_thinking {
        Some(serde_json::json!({
            "anthropic": {
                "thinking": {
                    "type": "enabled",
                    "budget_tokens": 10000
                }
            }
        }))
    } else {
        None
    }
}

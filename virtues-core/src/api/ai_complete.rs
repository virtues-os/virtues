//! POST /api/ai/complete — lean inline AI completion for the page editor.
//!
//! Unlike `/api/chat` (full agent loop + tools + chat persistence), this streams
//! a single-turn prose completion for the "live AI cursor": rewrite a selection,
//! or continue/generate at the cursor. Output is plain text streamed as SSE so
//! the browser can insert it into the Yjs document token-by-token (origin 'ai').
//!
//! Minimal SSE contract (NOT the AI SDK v6 protocol — the client is bespoke):
//!   data: {"type":"delta","text":"…"}
//!   data: {"type":"done"}
//!   data: {"type":"error","message":"…"}
//!
//! Cancellation rides the dropped connection: when the browser aborts the fetch,
//! axum drops the response body, the stream future is dropped, and the upstream
//! `stream_llm_response` request is dropped with it.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response, Sse},
    Json,
};
use futures::stream::Stream;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::convert::Infallible;
use std::pin::Pin;

use crate::agent::{stream::stream_llm_response, AgentEvent, LlmConfig};
use crate::middleware::auth::AuthUser;
use crate::virtues_api::client::BearerClient;

type SseEvent = axum::response::sse::Event;

#[derive(Debug, Deserialize)]
pub struct AiCompleteRequest {
    pub model: String,
    /// "rewrite" | "continue" | "generate"
    pub intent: String,
    #[serde(default)]
    pub instruction: String,
    #[serde(default)]
    pub selection: Option<String>,
    #[serde(default)]
    pub context_before: Option<String>,
    #[serde(default)]
    pub context_after: Option<String>,
    #[serde(default)]
    pub page_title: Option<String>,
}

fn sse(value: Value) -> SseEvent {
    SseEvent::default().data(value.to_string())
}

fn build_messages(req: &AiCompleteRequest) -> Vec<Value> {
    let title = req.page_title.clone().unwrap_or_default();
    let before = req.context_before.clone().unwrap_or_default();
    let after = req.context_after.clone().unwrap_or_default();

    let system = match req.intent.as_str() {
        "rewrite" => "You are a writing assistant editing prose inside a user's Markdown document. \
Rewrite the provided passage according to the instruction. Output ONLY the rewritten passage as \
Markdown. Do NOT add any preamble or explanation, do NOT wrap your output in triple backticks or a \
code fence, and do NOT surround it with quotation marks. Return only the replacement text itself.",
        _ => "You are a writing assistant continuing prose inside a user's Markdown document. \
Continue naturally and concisely from where the text leaves off, matching the existing voice and \
tone. Output ONLY the new text to insert. Do NOT add any preamble or explanation, and do NOT wrap \
your output in triple backticks or a code fence. Return only the text itself.",
    };

    let user = match req.intent.as_str() {
        "rewrite" => {
            let selection = req.selection.clone().unwrap_or_default();
            format!(
                "Document title: {title}\n\nInstruction: {instruction}\n\n\
Passage to rewrite:\n{selection}\n\n\
(Context before, for tone — do not repeat:)\n{before}\n\n\
(Context after, for tone — do not repeat:)\n{after}",
                instruction = req.instruction,
            )
        }
        _ => {
            let instruction = if req.instruction.trim().is_empty() {
                "Continue the writing.".to_string()
            } else {
                req.instruction.clone()
            };
            format!(
                "Document title: {title}\n\nInstruction: {instruction}\n\n\
Text so far (continue from the end):\n{before}\n\n\
(Text that follows, for context:)\n{after}",
            )
        }
    };

    vec![
        json!({ "role": "system", "content": system }),
        json!({ "role": "user", "content": user }),
    ]
}

/// POST /api/ai/complete
pub async fn ai_complete_handler(
    State(pool): State<PgPool>,
    _user: AuthUser,
    Json(request): Json<AiCompleteRequest>,
) -> Response {
    // Validate the model against the registry (mirrors chat_handler).
    match crate::api::models::list_models().await {
        Ok(models) => {
            if !models.iter().any(|m| m.model_id == request.model) {
                return (StatusCode::BAD_REQUEST, "Invalid model").into_response();
            }
        }
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to load models").into_response();
        }
    }

    let messages = build_messages(&request);
    let model = request.model.clone();

    let stream: Pin<Box<dyn Stream<Item = Result<SseEvent, Infallible>> + Send>> =
        Box::pin(async_stream::stream! {
            let config = LlmConfig { client: BearerClient::from_env(pool) };
            // Bridge the synchronous `emit` callback to the async SSE stream.
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

            let fut = stream_llm_response(
                &config,
                &model,
                &messages,
                &[],   // no tools — pure prose
                None,  // no provider options (no reasoning)
                None,  // no thought signature
                Some(512), // cap output so a misread prompt can't fill the doc
                move |event| {
                    if let AgentEvent::TextDelta { content } = event {
                        let _ = tx.send(content);
                    }
                },
            );
            tokio::pin!(fut);

            loop {
                tokio::select! {
                    biased;
                    // Forward each token chunk as it arrives.
                    Some(text) = rx.recv() => {
                        yield Ok(sse(json!({ "type": "delta", "text": text })));
                    }
                    // The LLM stream finished (or errored).
                    result = &mut fut => {
                        while let Ok(text) = rx.try_recv() {
                            yield Ok(sse(json!({ "type": "delta", "text": text })));
                        }
                        match result {
                            Ok(_) => yield Ok(sse(json!({ "type": "done" }))),
                            Err(e) => {
                                yield Ok(sse(json!({ "type": "error", "message": e.to_string() })));
                            }
                        }
                        break;
                    }
                }
            }
        });

    Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::new())
        .into_response()
}

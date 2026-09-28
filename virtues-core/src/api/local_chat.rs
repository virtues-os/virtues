//! Local mode's turn, and its model's status and one-time download.
//!
//! The chat handler hands a `ChatMode::Local` turn here before any cloud
//! step. The turn runs through the same live-turn registry and cancellation as
//! every other turn, so Stop and rejoin work, and emits the same UI message
//! stream through the same `TurnRecorder`, so the reply and its reasoning
//! render as they do in chat. What it never does is the rest: no model choice,
//! no gateway, no tools, and no row. A local chat is not stored, so nothing
//! downstream (titles, day summaries, the wiki, embeddings, backups) can read
//! it and send it anywhere.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

use crate::agent::protocol::{AgentEvent, StepReason};
use crate::api::chat::{
    generate_id, serialize_event, spawn_turn_driver, ui_stream_response, ChatCancellationState,
    ChatError, ChatRequest, StreamEvent,
};
use crate::api::live_turn::{self, LiveTurns};
use crate::api::turn_recorder::TurnRecorder;
use crate::local_model::{self, Message, Piece, TurnEvent};
use crate::middleware::auth::AuthUser;

/// Run one local turn and return its stream.
pub(crate) fn run(request: ChatRequest, live_turns: LiveTurns, cancel_state: ChatCancellationState) -> Response {
    let chat_id = request.chat_id.clone();
    // Same refusal as a cloud turn: starting a second turn would take over the
    // running one's live-turn slot and cancellation.
    if live_turns.get(&chat_id).is_some() && !cancel_state.is_cancelled(&chat_id) {
        return (
            StatusCode::CONFLICT,
            Json(ChatError {
                error: "turn_in_progress".to_string(),
                details: Some("The local model is still writing a reply in this chat. Stop it or wait for it to finish.".into()),
            }),
        )
            .into_response();
    }
    let msg_id = request.message_id.clone().unwrap_or_else(|| format!("msg_{}", generate_id()));
    // A local chat has no row, so the transcript is the request's, whole.
    let messages: Vec<Message> = request
        .messages
        .iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .map(|m| Message { role: m.role.clone(), text: m.text() })
        .filter(|m| !m.text.trim().is_empty())
        .collect();
    let think = request.think;

    let turn = live_turns.start(&chat_id);
    let token = cancel_state.register(&chat_id);
    let stream = {
        let token = token.clone();
        let cancel_state = cancel_state.clone();
        let chat_id = chat_id.clone();
        Box::pin(async_stream::stream! {
            yield serialize_event(&StreamEvent::Start { message_id: msg_id.clone() });
            yield serialize_event(&StreamEvent::StartStep);

            let mut recorder = TurnRecorder::new(msg_id.clone());
            let mut stats = None;
            match local_model::run_turn(messages, think, token.clone()).await {
                Err(refusal) => {
                    yield serialize_event(&StreamEvent::Error { error_text: refusal.to_string() });
                }
                Ok(mut rx) => {
                    while let Some(event) = rx.recv().await {
                        let agent_event = match event {
                            Ok(TurnEvent::Piece(Piece::Reasoning(content))) => AgentEvent::ReasoningDelta { content },
                            Ok(TurnEvent::Piece(Piece::Text(content))) => AgentEvent::TextDelta { content },
                            Ok(TurnEvent::Done(s)) => {
                                stats = Some(s);
                                continue;
                            }
                            Err(e) => {
                                tracing::warn!(error = %format!("{e:#}"), "local turn failed");
                                yield serialize_event(&StreamEvent::Error {
                                    error_text: "The local model stopped before it finished. Your chat is still here, so send your message again.".into(),
                                });
                                break;
                            }
                        };
                        for ev in recorder.on_event(agent_event) {
                            yield serialize_event(&ev);
                        }
                    }
                    for ev in recorder.on_event(AgentEvent::StepComplete { step: 1, reason: StepReason::EndTurn }) {
                        yield serialize_event(&ev);
                    }
                }
            }
            let cancelled = token.is_cancelled();
            for ev in recorder.close(cancelled, false) {
                yield serialize_event(&ev);
            }
            if let Some(s) = stats {
                yield serialize_event(&StreamEvent::LocalStats(s));
            }
            yield "[DONE]".to_string();
            cancel_state.remove(&chat_id, &token);
        })
    };
    spawn_turn_driver(stream, turn.clone(), token, live_turns, cancel_state, chat_id);
    ui_stream_response(live_turn::watch(turn))
}

/// GET /api/local-model — whether this box offers local mode, and whether the
/// model is on it. The client offers the mode only when `supported` is true,
/// so a box without this route never sees the mode requested. An older box
/// would otherwise read `"local"` as ordinary chat and answer from the cloud.
pub async fn status_handler(_user: AuthUser) -> Response {
    Json(local_model::status()).into_response()
}

/// POST /api/local-model — start the one-time download (about 0.7 GB). The
/// client polls the GET for progress.
pub async fn download_handler(_user: AuthUser) -> Response {
    match local_model::start_download() {
        Ok(()) => (StatusCode::ACCEPTED, Json(local_model::status())).into_response(),
        Err(e) => (StatusCode::CONFLICT, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The app reads this part by name and shape for the line under the reply.
    #[test]
    fn local_stats_are_a_transient_data_part() {
        let line = serialize_event(&StreamEvent::LocalStats(local_model::Stats {
            prompt_tokens: 23,
            generated_tokens: 36,
            tokens_per_second: 7.5,
            seconds_to_first_token: 1.3,
        }));
        let v: serde_json::Value = serde_json::from_str(&line).expect("json");
        assert_eq!(v["type"], "data-local-stats");
        assert_eq!(v["transient"], true);
        assert_eq!(v["data"]["generatedTokens"], 36);
        assert_eq!(v["data"]["tokensPerSecond"], 7.5);
    }
}

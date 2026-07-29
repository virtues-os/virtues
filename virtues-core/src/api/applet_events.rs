//! Applet Event Broadcasting
//!
//! Provides a persistent SSE channel for real-time action event updates.
//! Follows the same pattern as ChatCancellationState — per-chat broadcast channels
//! stored in an Arc<RwLock<HashMap>>.
//!
//! Architecture:
//! - `AppletBroadcastState` holds broadcast::Sender per chat_id
//! - `run_applet()` sends AgentEvents to the broadcast channel
//! - `GET /api/chats/{id}/action/events` subscribes to the channel via SSE

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    response::{
        sse::{Event as SseEvent, KeepAlive},
        Sse,
    },
};
use futures::stream::Stream;
use tokio::sync::broadcast;

use crate::agent::protocol::AgentEvent;
use crate::middleware::auth::AuthUser;

/// Broadcast channel capacity per chat. Events are dropped if subscriber falls behind.
const BROADCAST_CAPACITY: usize = 256;

/// Shared state for broadcasting action events to SSE subscribers.
///
/// Each chat_id can have one broadcast::Sender. Multiple subscribers (browser tabs)
/// can listen to the same channel. Channels are created lazily and cleaned up
/// when the last sender is dropped (no active action run).
#[derive(Clone, Default)]
pub struct AppletBroadcastState {
    channels: Arc<std::sync::RwLock<HashMap<String, broadcast::Sender<AgentEvent>>>>,
}

impl AppletBroadcastState {
    pub fn new() -> Self {
        Self {
            channels: Arc::new(std::sync::RwLock::new(HashMap::new())),
        }
    }

    /// Get or create a broadcast sender for a chat.
    /// Called by `run_applet()` to broadcast events.
    pub fn get_or_create(&self, chat_id: &str) -> broadcast::Sender<AgentEvent> {
        // Try read lock first
        {
            let channels = self.channels.read().unwrap();
            if let Some(sender) = channels.get(chat_id) {
                return sender.clone();
            }
        }

        // Need to create — take write lock
        let mut channels = self.channels.write().unwrap();
        // Double-check after acquiring write lock
        if let Some(sender) = channels.get(chat_id) {
            return sender.clone();
        }

        let (sender, _) = broadcast::channel(BROADCAST_CAPACITY);
        channels.insert(chat_id.to_string(), sender.clone());
        sender
    }

    /// Subscribe to events for a chat. Returns a broadcast::Receiver.
    /// If no channel exists yet, creates one (subscriber will wait for events).
    pub fn subscribe(&self, chat_id: &str) -> broadcast::Receiver<AgentEvent> {
        self.get_or_create(chat_id).subscribe()
    }

    /// Clean up a channel if there are no more subscribers or senders.
    /// Called after an action run completes.
    pub fn cleanup(&self, chat_id: &str) {
        let mut channels = self.channels.write().unwrap();
        if let Some(sender) = channels.get(chat_id) {
            if sender.receiver_count() == 0 {
                channels.remove(chat_id);
            }
        }
    }

    /// Broadcast an event to all subscribers of a chat.
    pub fn broadcast(&self, chat_id: &str, event: AgentEvent) -> Result<usize, ()> {
        let channels = self.channels.read().unwrap();
        if let Some(sender) = channels.get(chat_id) {
            match sender.send(event) {
                Ok(count) => Ok(count),
                Err(_) => Ok(0),
            }
        } else {
            Err(())
        }
    }
}

/// SSE handler: `GET /api/chats/:id/action/events`
///
/// Subscribes to the action event broadcast channel for a chat and streams
/// events as SSE. The connection stays open until the client disconnects
/// or the action run completes (Done/Error event).
pub async fn subscribe_applet_events(
    State(broadcast_state): State<AppletBroadcastState>,
    Path(chat_id): Path<String>,
    _user: AuthUser,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let mut rx = broadcast_state.subscribe(&chat_id);

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let data = event.to_sse_data();
                    let is_terminal = matches!(
                        event,
                        AgentEvent::Done { .. } | AgentEvent::Error { .. }
                    );

                    yield Ok(SseEvent::default()
                        .event("action_event")
                        .data(data));

                    if is_terminal {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(count)) => {
                    let warning = serde_json::json!({
                        "type": "warning",
                        "message": format!("Missed {} events due to slow consumption", count),
                    });
                    yield Ok(SseEvent::default()
                        .event("action_event")
                        .data(warning.to_string()));
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

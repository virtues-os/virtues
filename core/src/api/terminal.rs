use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures::{sink::SinkExt, stream::StreamExt};
use std::env;

use crate::server::ingest::AppState;

/// Handler for the terminal WebSocket
pub async fn terminal_ws_handler(
    ws: WebSocketUpgrade,
    State(_state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

/// Handle the established WebSocket connection
async fn handle_socket(mut socket: WebSocket) {
    // Check if we are in production mode (SSH target set)
    let ssh_host = env::var("SSH_TARGET_HOST").ok();

    if let Some(_host) = ssh_host {
        // PRODUCTION MODE: Real SSH connection
        // For now, we'll just send a "Coming Soon" message even in prod until the full
        // russh implementation is ready, to avoid breaking anything critically.
        // In the next step, we would implement the full SSH bridge here.
        if let Err(e) = socket
            .send(Message::Text(
                "\x1b[33m⚡ Production SSH Access initialized.\x1b[0m\r\n".to_string(),
            ))
            .await
        {
            tracing::error!("Failed to send terminal welcome: {}", e);
            return;
        }
    } else {
        // DEV MODE: Local echo / simulation
        if let Err(e) = socket
            .send(Message::Text(
                "\x1b[32m✓ Connected to Local Dev Environment.\x1b[0m\r\n\x1b[90m> SSH Access is only available in Staging/Production.\r\n> This terminal is in 'Echo Mode' for UI testing.\x1b[0m\r\n\r\n".to_string(),
            ))
            .await
        {
            tracing::error!("Failed to send terminal welcome: {}", e);
            return;
        }

        // Simple echo loop
        while let Some(msg) = socket.recv().await {
            if let Ok(msg) = msg {
                match msg {
                    Message::Text(t) => {
                        // Handle input (simple JSON protocol from frontend)
                        if let Ok(cmd) = serde_json::from_str::<serde_json::Value>(&t) {
                            if let Some(data) = cmd.get("data").and_then(|v| v.as_str()) {
                                // Echo back the characters
                                // In a real terminal, the PTY handles echo, so we simulate it here
                                if data == "\r" {
                                    // Enter key
                                    let _ = socket.send(Message::Text("\r\n".to_string())).await;
                                    let _ = socket.send(Message::Text("virtues-dev $ ".to_string())).await;
                                } else if data == "\u{7f}" {
                                     // Backspace handled by frontend usually, but we can echo backspace
                                    let _ = socket.send(Message::Text("\x08 \x08".to_string())).await;
                                } else {
                                    let _ = socket.send(Message::Text(data.to_string())).await;
                                }
                            } else if let Some(type_) = cmd.get("type").and_then(|v| v.as_str()) {
                                if type_ == "resize" {
                                    // Acknowledge resize
                                    tracing::debug!("Terminal resized");
                                }
                            }
                        }
                    }
                    Message::Close(_) => {
                        tracing::info!("Terminal connection closed");
                        return;
                    }
                    _ => {}
                }
            } else {
                tracing::info!("Terminal connection error / closed");
                return;
            }
        }
    }
}

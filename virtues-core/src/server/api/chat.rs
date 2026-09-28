//! Chats: the list, the streaming turn, usage and compaction, edit
//! permissions, and the session probe.

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json,
    Router,
};
use serde::Deserialize;

use super::{api_response, error_response, success_message};
use crate::server::AppState;

/// This area's authenticated routes. Merged into the protected router, whose
/// `route_layer` requires a resolved `AuthUser`.
pub fn routes() -> Router<AppState> {
    Router::new()
        // Chats API
        .route(
            "/api/chats",
            get(list_chats_handler).post(create_chat_handler),
        )
        .route(
            "/api/chats/:id",
            get(get_chat_handler)
                .patch(update_chat_handler)
                .delete(delete_chat_handler),
        )
        .route("/api/chats/title", post(generate_chat_title_handler))
        // Chat Usage & Compaction API
        .route("/api/chats/:id/usage", get(get_chat_usage_handler))
        .route("/api/chats/:id/compact", post(compact_chat_handler))
        // Chat API (streaming)
        .route("/api/chat", post(chat_handler))
        .route("/api/chat/cancel", post(cancel_chat_handler))
        .route("/api/chat/:id/stream", get(live_turn_stream_handler))
        .route("/api/ai/complete", post(ai_complete_handler))
        // Chat Edit Permissions API
        .route(
            "/api/chats/:id/permissions",
            get(list_chat_permissions_handler).post(add_chat_permission_handler),
        )
        .route(
            "/api/chats/:id/permissions/:entity_id",
            delete(remove_chat_permission_handler),
        )
}


// =============================================================================
// Chat Usage & Compaction API Handlers
// =============================================================================

/// Get token usage for a chat
pub async fn get_chat_usage_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
) -> Response {
    api_response(crate::api::chat_usage::get_chat_usage(state.db.pool(), chat_id).await)
}

/// Compact a chat (summarize older messages)
pub async fn compact_chat_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    Json(request): Json<Option<CompactChatRequest>>,
) -> Response {
    let options = request.unwrap_or_default().into();
    api_response(crate::api::compaction::compact_chat(state.db.pool(), chat_id, options).await)
}

/// Request body for compaction
#[derive(Debug, Deserialize, Default)]
pub struct CompactChatRequest {
    /// Number of recent exchanges to keep verbatim (default: 4)
    pub keep_recent_exchanges: Option<usize>,
    /// Force compaction even if under threshold
    #[serde(default)]
    pub force: bool,
}

impl From<CompactChatRequest> for crate::api::compaction::CompactionOptions {
    fn from(req: CompactChatRequest) -> Self {
        let default_opts = crate::api::compaction::CompactionOptions::default();
        Self {
            keep_recent_exchanges: req
                .keep_recent_exchanges
                .unwrap_or(default_opts.keep_recent_exchanges),
            force: req.force,
            model_id: None, // API compaction uses default model context window
        }
    }
}

// =============================================================================
// Chats API Handlers
// =============================================================================

/// List chats
pub async fn list_chats_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::chats::list_chats(state.db.pool(), 25).await)
}

/// Create a new chat with initial messages
pub async fn create_chat_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::chats::CreateChatRequest>,
) -> Response {
    api_response(crate::api::chats::create_chat_from_request(state.db.pool(), request).await)
}

/// Get a chat by ID
pub async fn get_chat_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
) -> Response {
    api_response(crate::api::chats::get_chat(state.db.pool(), chat_id).await)
}

/// Update a chat (title and/or icon)
pub async fn update_chat_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    Json(request): Json<crate::api::chats::UpdateChatRequest>,
) -> Response {
    api_response(
        crate::api::chats::update_chat(state.db.pool(), chat_id, &request).await,
    )
}

/// Delete a chat
pub async fn delete_chat_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
) -> Response {
    api_response(crate::api::chats::delete_chat(state.db.pool(), chat_id).await)
}

/// Generate a title for a chat
pub async fn generate_chat_title_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::chats::GenerateTitleRequest>,
) -> Response {
    api_response(
        crate::api::chats::generate_title(state.db.pool(), request.chat_id, &request.messages)
            .await,
    )
}

// =============================================================================
// Chat API Handler
// =============================================================================

/// POST /api/chat - Stream chat completion (requires authentication)
pub async fn chat_handler(
    State(state): State<AppState>,
    user: crate::middleware::auth::AuthUser,
    Json(request): Json<crate::api::chat::ChatRequest>,
) -> Response {
    crate::api::chat::chat_handler(
        axum::extract::State(state.db.pool().clone()),
        axum::extract::State(state.yjs_state.clone()),
        axum::extract::State(state.chat_cancel_state.clone()),
        axum::extract::State(state.live_turns.clone()),
        axum::extract::State(state.ghost_permissions.clone()),
        user,
        Json(request),
    )
    .await
}

/// GET /api/chat/:id/stream - Rejoin the turn still running for a chat (VIR-323)
pub async fn live_turn_stream_handler(
    State(state): State<AppState>,
    user: crate::middleware::auth::AuthUser,
    axum::extract::Path(chat_id): axum::extract::Path<String>,
) -> Response {
    crate::api::chat::live_turn_stream_handler(
        axum::extract::State(state.live_turns.clone()),
        user,
        axum::extract::Path(chat_id),
    )
    .await
}

/// POST /api/ai/complete - Lean inline AI completion (live AI cursor)
pub async fn ai_complete_handler(
    State(state): State<AppState>,
    user: crate::middleware::auth::AuthUser,
    Json(request): Json<crate::api::ai_complete::AiCompleteRequest>,
) -> Response {
    crate::api::ai_complete::ai_complete_handler(
        axum::extract::State(state.db.pool().clone()),
        user,
        Json(request),
    )
    .await
}

/// POST /api/chat/cancel - Cancel an in-progress chat request
pub async fn cancel_chat_handler(
    State(state): State<AppState>,
    user: crate::middleware::auth::AuthUser,
    Json(request): Json<crate::api::chat::CancelChatRequest>,
) -> impl IntoResponse {
    crate::api::chat::cancel_chat_handler(
        axum::extract::State(state.chat_cancel_state.clone()),
        user,
        Json(request),
    )
    .await
}

// =============================================================================
// Chat Edit Permissions API Handlers
// =============================================================================

/// GET /api/chats/:id/permissions - List edit permissions for a chat
pub async fn list_chat_permissions_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
) -> Response {
    api_response(crate::api::chat_permissions::list_permissions(state.db.pool(), &chat_id).await)
}

/// POST /api/chats/:id/permissions - Add an edit permission
pub async fn add_chat_permission_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    Json(request): Json<crate::api::chat_permissions::AddPermissionRequest>,
) -> Response {
    api_response(
        crate::api::chat_permissions::add_permission(
            state.db.pool(),
            &state.ghost_permissions,
            &chat_id,
            request,
        )
        .await,
    )
}

/// DELETE /api/chats/:id/permissions/:entity_id - Remove an edit permission
pub async fn remove_chat_permission_handler(
    State(state): State<AppState>,
    Path((chat_id, entity_id)): Path<(String, String)>,
) -> Response {
    match crate::api::chat_permissions::remove_permission(state.db.pool(), &chat_id, &entity_id)
        .await
    {
        Ok(_) => success_message("Permission removed"),
        Err(e) => error_response(e),
    }
}

// =============================================================================
// Auth API Handlers
// =============================================================================

/// GET /auth/session — current session (or null if not paired). Authenticated by
/// the `AuthUser` extractor (proven iroh key / loopback console / dev fallback);
/// there is no cookie/signout — the credential is the device's iroh key.
pub async fn auth_session_handler(user: Option<crate::middleware::auth::AuthUser>) -> Response {
    crate::api::auth::session_handler(user).await.into_response()
}

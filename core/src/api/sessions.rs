//! Chat Sessions API
//!
//! CRUD operations for chat sessions stored in app.chat_sessions.
//! Sessions contain conversation history with messages stored as JSONB.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::Result;

// ============================================================================
// Types
// ============================================================================

/// Chat message structure stored in JSONB array
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "user" | "assistant" | "system"
    pub content: String,
    pub timestamp: String, // ISO 8601 timestamp

    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    #[serde(rename = "agentId", skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<IntentMetadata>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
}

/// Tool call structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool_name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,

    pub arguments: serde_json::Value,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,

    pub timestamp: String,
}

/// Intent classification metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentMetadata {
    #[serde(rename = "type")]
    pub intent_type: String,
    pub confidence: f64,
    pub reasoning: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub entities: Option<Vec<String>>,

    #[serde(rename = "timeRange", skip_serializing_if = "Option::is_none")]
    pub time_range: Option<TimeRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRange {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
}

/// Chat session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: Uuid,
    pub title: String,
    pub messages: Vec<ChatMessage>,
    pub message_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Session list item (without messages for list queries)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionListItem {
    pub conversation_id: Uuid,
    pub title: String,
    pub message_count: i32,
    pub first_message_at: String,
    pub last_updated: String,
}

/// Response for session list
#[derive(Debug, Serialize)]
pub struct SessionListResponse {
    pub conversations: Vec<SessionListItem>,
    pub source: String,
}

/// Response for session detail
#[derive(Debug, Serialize)]
pub struct SessionDetailResponse {
    pub conversation: ConversationMeta,
    pub messages: Vec<MessageResponse>,
}

#[derive(Debug, Serialize)]
pub struct ConversationMeta {
    pub conversation_id: Uuid,
    pub title: String,
    pub first_message_at: String,
    pub last_message_at: String,
    pub message_count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
}

/// Request to update session title
#[derive(Debug, Deserialize)]
pub struct UpdateTitleRequest {
    pub title: String,
}

/// Request to create a new session with initial messages
#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub title: String,
    pub messages: Vec<ChatMessage>,
}

/// Response after creating a session
#[derive(Debug, Serialize)]
pub struct CreateSessionResponse {
    pub id: Uuid,
    pub title: String,
    pub message_count: i32,
    pub created_at: String,
}

/// Response after updating session
#[derive(Debug, Serialize)]
pub struct UpdateSessionResponse {
    pub conversation_id: Uuid,
    pub title: String,
    pub updated_at: String,
}

/// Response after deleting session
#[derive(Debug, Serialize)]
pub struct DeleteSessionResponse {
    pub success: bool,
    pub conversation_id: Uuid,
}

/// Request to generate title
#[derive(Debug, Deserialize)]
pub struct GenerateTitleRequest {
    #[serde(rename = "sessionId")]
    pub session_id: Uuid,
    pub messages: Vec<TitleMessage>,
}

#[derive(Debug, Deserialize)]
pub struct TitleMessage {
    pub role: String,
    pub content: String,
}

/// Response after generating title
#[derive(Debug, Serialize)]
pub struct GenerateTitleResponse {
    pub session_id: Uuid,
    pub title: String,
}

// ============================================================================
// Functions
// ============================================================================

/// List recent chat sessions (without messages)
pub async fn list_sessions(pool: &SqlitePool, limit: i64) -> Result<SessionListResponse> {
    let rows = sqlx::query!(
        r#"
        SELECT
            id,
            title,
            message_count,
            created_at,
            updated_at
        FROM app_chat_sessions
        ORDER BY updated_at DESC
        LIMIT $1
        "#,
        limit
    )
    .fetch_all(pool)
    .await?;

    let conversations = rows
        .into_iter()
        .filter_map(|row| {
            let id = row.id.as_ref().and_then(|s| Uuid::parse_str(s).ok())?;
            Some(SessionListItem {
                conversation_id: id,
                title: row.title.clone(),
                message_count: row.message_count as i32,
                first_message_at: row.created_at.clone(),
                last_updated: row.updated_at.clone(),
            })
        })
        .collect();

    Ok(SessionListResponse {
        conversations,
        source: "app_schema".to_string(),
    })
}

/// Get a single session with all messages
pub async fn get_session(pool: &SqlitePool, session_id: Uuid) -> Result<SessionDetailResponse> {
    let session_id_str = session_id.to_string();
    let row = sqlx::query!(
        r#"
        SELECT
            id,
            title,
            messages,
            message_count,
            created_at,
            updated_at
        FROM app_chat_sessions
        WHERE id = $1
        "#,
        session_id_str
    )
    .fetch_optional(pool)
    .await?;

    let row = row.ok_or_else(|| crate::Error::NotFound("Session not found".into()))?;

    // Parse ID
    let id = row
        .id
        .as_ref()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| crate::Error::Database("Invalid session ID".into()))?;
    let id_str = id.to_string();

    // Parse messages from JSON string
    let messages: Vec<ChatMessage> = serde_json::from_str(&row.messages).map_err(|e| {
        tracing::error!("Corrupted messages JSON in session {}: {}", id, e);
        crate::Error::Database(format!("Corrupted message data: {}", e))
    })?;

    // Get last message for model/provider info
    let last_message = messages.last();

    let conversation = ConversationMeta {
        conversation_id: id,
        title: row.title.clone(),
        first_message_at: row.created_at.clone(),
        last_message_at: row.updated_at.clone(),
        message_count: row.message_count as i32,
        model: last_message.and_then(|m| m.model.clone()),
        provider: last_message.and_then(|m| m.provider.clone()),
    };

    // Format messages for response
    let messages_response: Vec<MessageResponse> = messages
        .iter()
        .enumerate()
        .map(|(idx, msg)| MessageResponse {
            id: format!("{}_{}", id_str, idx),
            role: msg.role.clone(),
            content: msg.content.clone(),
            timestamp: msg.timestamp.clone(),
            model: msg.model.clone(),
            tool_calls: msg.tool_calls.clone(),
            reasoning: msg.reasoning.clone(),
            subject: msg.subject.clone(),
        })
        .collect();

    Ok(SessionDetailResponse {
        conversation,
        messages: messages_response,
    })
}

/// Create a new chat session
pub async fn create_session(
    pool: &SqlitePool,
    title: &str,
    messages: Vec<ChatMessage>,
) -> Result<ChatSession> {
    // Serialize messages to JSON string for SQLite
    let id = Uuid::new_v4().to_string();
    let messages_json = serde_json::to_string(&messages)?;
    let message_count = messages.len() as i32;

    let row = sqlx::query!(
        r#"
        INSERT INTO app_chat_sessions (id, title, messages, message_count)
        VALUES ($1, $2, $3, $4)
        RETURNING id, title, messages, message_count, created_at, updated_at
        "#,
        id,
        title,
        messages_json,
        message_count
    )
    .fetch_one(pool)
    .await?;

    // Parse ID
    let id = row
        .id
        .as_ref()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| crate::Error::Database("Invalid session ID".into()))?;

    // Parse messages from JSON string
    let messages: Vec<ChatMessage> = serde_json::from_str(&row.messages).map_err(|e| {
        tracing::error!("Corrupted messages JSON in session {}: {}", id, e);
        crate::Error::Database(format!("Corrupted message data: {}", e))
    })?;

    // Parse timestamps
    let created_at = chrono::DateTime::parse_from_rfc3339(&row.created_at)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    let updated_at = chrono::DateTime::parse_from_rfc3339(&row.updated_at)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    Ok(ChatSession {
        id,
        title: row.title.clone(),
        messages,
        message_count: row.message_count as i32,
        created_at,
        updated_at,
    })
}

/// Create a new chat session with initial messages (public API)
pub async fn create_session_from_request(
    pool: &SqlitePool,
    request: CreateSessionRequest,
) -> Result<CreateSessionResponse> {
    let session = create_session(pool, &request.title, request.messages).await?;

    Ok(CreateSessionResponse {
        id: session.id,
        title: session.title,
        message_count: session.message_count,
        created_at: session.created_at.to_rfc3339(),
    })
}

/// Update session title
pub async fn update_session_title(
    pool: &SqlitePool,
    session_id: Uuid,
    title: &str,
) -> Result<UpdateSessionResponse> {
    let session_id_str = session_id.to_string();
    let row = sqlx::query!(
        r#"
        UPDATE app_chat_sessions
        SET title = $1, updated_at = datetime('now')
        WHERE id = $2
        RETURNING id, title, updated_at
        "#,
        title,
        session_id_str
    )
    .fetch_optional(pool)
    .await?;

    let row = row.ok_or_else(|| crate::Error::NotFound("Session not found".into()))?;

    // Parse ID
    let id = row
        .id
        .as_ref()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| crate::Error::Database("Invalid session ID".into()))?;

    Ok(UpdateSessionResponse {
        conversation_id: id,
        title: row.title.clone(),
        updated_at: row.updated_at.clone(),
    })
}

/// Append a message to a session
pub async fn append_message(
    pool: &SqlitePool,
    session_id: Uuid,
    message: ChatMessage,
) -> Result<()> {
    let session_id_str = session_id.to_string();

    // SQLite doesn't support JSONB concatenation like PostgreSQL
    // We need to read the existing messages, append, and write back
    let row = sqlx::query!(
        "SELECT messages FROM app_chat_sessions WHERE id = $1",
        session_id_str
    )
    .fetch_optional(pool)
    .await?;

    let row = row.ok_or_else(|| crate::Error::NotFound("Session not found".into()))?;

    // Parse existing messages and append new one
    let mut messages: Vec<ChatMessage> = serde_json::from_str(&row.messages).map_err(|e| {
        tracing::error!("Corrupted messages JSON in session {}: {}", session_id, e);
        crate::Error::Database(format!("Corrupted message data: {}", e))
    })?;
    messages.push(message);
    let messages_json = serde_json::to_string(&messages)?;
    let message_count = messages.len() as i32;

    sqlx::query!(
        r#"
        UPDATE app_chat_sessions
        SET
            messages = $1,
            message_count = $2,
            updated_at = datetime('now')
        WHERE id = $3
        "#,
        messages_json,
        message_count,
        session_id_str
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Update messages in a session (replace all messages)
pub async fn update_messages(
    pool: &SqlitePool,
    session_id: Uuid,
    messages: Vec<ChatMessage>,
) -> Result<()> {
    let session_id_str = session_id.to_string();
    let messages_json = serde_json::to_string(&messages)?;
    let message_count = messages.len() as i32;

    let result = sqlx::query!(
        r#"
        UPDATE app_chat_sessions
        SET
            messages = $1,
            message_count = $2,
            updated_at = datetime('now')
        WHERE id = $3
        "#,
        messages_json,
        message_count,
        session_id_str
    )
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(crate::Error::NotFound("Session not found".into()));
    }

    Ok(())
}

/// Delete a chat session
pub async fn delete_session(pool: &SqlitePool, session_id: Uuid) -> Result<DeleteSessionResponse> {
    let session_id_str = session_id.to_string();
    let result = sqlx::query!(
        r#"
        DELETE FROM app_chat_sessions
        WHERE id = $1
        RETURNING id
        "#,
        session_id_str
    )
    .fetch_optional(pool)
    .await?;

    let row = result.ok_or_else(|| crate::Error::NotFound("Session not found".into()))?;

    // Parse ID
    let id = row
        .id
        .as_ref()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| crate::Error::Database("Invalid session ID".into()))?;

    Ok(DeleteSessionResponse {
        success: true,
        conversation_id: id,
    })
}

/// Generate a title for a session using AI
///
/// Uses Tollbooth with system user (no specific user context for background operations)
pub async fn generate_title(
    pool: &SqlitePool,
    session_id: Uuid,
    messages: &[TitleMessage],
) -> Result<GenerateTitleResponse> {
    // Get background model from assistant profile
    let background_model = crate::api::assistant_profile::get_background_model(pool).await?;

    // Get Tollbooth configuration
    let tollbooth_url = std::env::var("TOLLBOOTH_URL").unwrap_or_else(|_| {
        tracing::warn!("TOLLBOOTH_URL not set, using default localhost:9002");
        "http://localhost:9002".into()
    });
    let secret = std::env::var("TOLLBOOTH_INTERNAL_SECRET")
        .map_err(|_| crate::Error::Configuration("TOLLBOOTH_INTERNAL_SECRET not set".into()))?;

    // Build conversation summary (first few messages)
    let messages_to_include: Vec<&TitleMessage> =
        messages.iter().take(6.min(messages.len())).collect();
    let conversation_summary: String = messages_to_include
        .iter()
        .map(|m| format!("{}: {}", m.role, &m.content[..200.min(m.content.len())]))
        .collect::<Vec<_>>()
        .join("\n\n");

    let prompt = format!(
        r#"Based on this conversation, generate a very short title (3-6 words maximum) that captures the main topic or theme. Only return the title, nothing else.

Conversation:
{}"#,
        conversation_summary
    );

    // Call Tollbooth using shared client with timeouts
    let client = crate::http_client::tollbooth_client();
    let response = crate::tollbooth::with_system_auth(
        client.post(format!("{}/v1/chat/completions", tollbooth_url)),
        &secret,
    )
    .json(&serde_json::json!({
        "model": background_model,
        "messages": [
            {"role": "user", "content": prompt}
        ],
        "max_tokens": 50
    }))
    .send()
    .await?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let error_text = response.text().await.unwrap_or_default();
        // Provide user-friendly message for budget errors
        let error_msg = match status {
            402 => "Usage limit reached for title generation".to_string(),
            429 => "Rate limited. Please try again later.".to_string(),
            _ => format!("Tollbooth error: {}", error_text),
        };
        return Err(crate::Error::ExternalApi(error_msg));
    }

    let response_json: serde_json::Value = response.json().await?;
    let mut title = response_json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("New Chat")
        .trim()
        .to_string();

    // Remove quotes if present
    title = title.trim_matches(|c| c == '"' || c == '\'').to_string();

    // Truncate if too long
    if title.len() > 60 {
        title = format!("{}...", &title[..57]);
    }

    // Update session title in database
    sqlx::query!(
        r#"
        UPDATE app_chat_sessions
        SET title = $1, updated_at = datetime('now')
        WHERE id = $2
        "#,
        title,
        session_id
    )
    .execute(pool)
    .await?;

    Ok(GenerateTitleResponse { session_id, title })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message_serialization() {
        let message = ChatMessage {
            role: "user".to_string(),
            content: "Hello".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            model: None,
            provider: None,
            agent_id: None,
            tool_calls: None,
            reasoning: None,
            intent: None,
            subject: None,
        };

        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"content\":\"Hello\""));
        // Optional fields should not be present when None
        assert!(!json.contains("\"model\""));
    }
}

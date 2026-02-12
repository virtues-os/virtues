//! Chats API
//!
//! CRUD operations for chats stored in the chats table.
//! Messages are stored in a normalized chat_messages table for
//! performance, proper indexing, and race-condition-free appends.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::api::chat::UIPart;
use crate::error::Result;
use crate::types::Timestamp;

// ============================================================================
// Helper Functions
// ============================================================================

/// Get the next sequence number for a chat
async fn get_next_sequence_num(pool: &SqlitePool, chat_id: &str) -> Result<i32> {
    let row = sqlx::query_scalar!(
        r#"SELECT COALESCE(MAX(sequence_num), 0) as "seq!: i64" FROM app_chat_messages WHERE chat_id = $1"#,
        chat_id
    )
    .fetch_one(pool)
    .await?;

    Ok((row as i32) + 1)
}

// ============================================================================
// Types
// ============================================================================

/// Chat message structure stored in chat_messages table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Unique message ID (stable, persisted)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    pub role: String, // "user" | "assistant" | "system"
    pub content: String,
    pub timestamp: Timestamp,

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
    #[serde(rename = "thoughtSignature", skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
    pub parts: Option<Vec<UIPart>>,
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

/// Chat conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chat {
    pub id: String,
    pub title: String,
    pub messages: Vec<ChatMessage>,
    pub message_count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Chat list item (without messages for list queries)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatListItem {
    pub conversation_id: String,
    pub title: String,
    pub message_count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub first_message_at: Timestamp,
    pub last_updated: Timestamp,
}

/// Response for chat list
#[derive(Debug, Serialize)]
pub struct ChatListResponse {
    pub conversations: Vec<ChatListItem>,
    pub source: String,
}

/// Response for chat detail
#[derive(Debug, Serialize)]
pub struct ChatDetailResponse {
    pub conversation: ConversationMeta,
    pub messages: Vec<MessageResponse>,
}

#[derive(Debug, Serialize)]
pub struct ConversationMeta {
    pub conversation_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub first_message_at: Timestamp,
    pub last_message_at: Timestamp,
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
    pub timestamp: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(rename = "thoughtSignature", skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
    pub parts: Option<Vec<UIPart>>,
}

/// Request to update chat metadata (title and/or icon)
#[derive(Debug, Deserialize)]
pub struct UpdateChatRequest {
    pub title: Option<String>,
    pub icon: Option<Option<String>>,
}

// System space ID - chats created here don't get auto-added
const SYSTEM_SPACE_ID: &str = "space_system";

/// Request to create a new chat with initial messages
#[derive(Debug, Deserialize)]
pub struct CreateChatRequest {
    pub title: String,
    pub messages: Vec<ChatMessage>,
    #[serde(rename = "spaceId")]
    pub space_id: Option<String>, // For auto-add to space_items (not stored on chat)
}

/// Response after creating a chat
#[derive(Debug, Serialize)]
pub struct CreateChatResponse {
    pub id: String,
    pub title: String,
    pub message_count: i32,
    pub created_at: Timestamp,
}

/// Response after updating chat
#[derive(Debug, Serialize)]
pub struct UpdateChatResponse {
    pub conversation_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub updated_at: Timestamp,
}

/// Response after deleting chat
#[derive(Debug, Serialize)]
pub struct DeleteChatResponse {
    pub success: bool,
    pub conversation_id: String,
}

/// Request to generate title
#[derive(Debug, Deserialize)]
pub struct GenerateTitleRequest {
    #[serde(rename = "chatId")]
    pub chat_id: String,
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
    pub chat_id: String,
    pub title: String,
}

// ============================================================================
// Functions
// ============================================================================

/// List recent chats (without messages)
pub async fn list_chats(pool: &SqlitePool, limit: i64) -> Result<ChatListResponse> {
    let rows = sqlx::query(
        r#"
        SELECT
            id,
            title,
            icon,
            message_count,
            created_at,
            updated_at
        FROM app_chats
        ORDER BY updated_at DESC
        LIMIT ?
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let conversations = rows
        .into_iter()
        .filter_map(|row| {
            use sqlx::Row;
            let id: String = row.get("id");
            let title: String = row.get("title");
            let icon: Option<String> = row.get("icon");
            let message_count: i32 = row.get("message_count");
            let created_at: String = row.get("created_at");
            let updated_at: String = row.get("updated_at");

            let first_message_at = created_at.parse::<Timestamp>().ok()?;
            let last_updated = updated_at.parse::<Timestamp>().ok()?;
            Some(ChatListItem {
                conversation_id: id,
                title,
                message_count,
                icon,
                first_message_at,
                last_updated,
            })
        })
        .collect();

    Ok(ChatListResponse {
        conversations,
        source: "app_schema".to_string(),
    })
}

/// Get a single chat with all messages
pub async fn get_chat(pool: &SqlitePool, chat_id: String) -> Result<ChatDetailResponse> {
    let chat_id_str = chat_id.clone();

    // Get chat metadata
    let row = sqlx::query(
        r#"
        SELECT
            id,
            title,
            icon,
            message_count,
            created_at,
            updated_at
        FROM app_chats
        WHERE id = ?
        "#,
    )
    .bind(&chat_id_str)
    .fetch_optional(pool)
    .await?;

    let row = row.ok_or_else(|| crate::Error::NotFound("Chat not found".into()))?;

    // Parse ID
    use sqlx::Row;
    let id: String = row.get("id");
    let title: String = row.get("title");
    let icon: Option<String> = row.get("icon");
    let message_count: i32 = row.get("message_count");
    let created_at: String = row.get("created_at");
    let updated_at: String = row.get("updated_at");

        // Query messages from normalized table
        let message_rows = sqlx::query(
            r#"
            SELECT
                id, role, content, model, provider, agent_id,
                reasoning, tool_calls, intent, subject, thought_signature, created_at, parts
            FROM app_chat_messages
            WHERE chat_id = ?
            ORDER BY sequence_num ASC
            "#,
        )
        .bind(&chat_id_str)
        .fetch_all(pool)
        .await?;

        // Convert to response format
        let messages_response: Vec<MessageResponse> = message_rows
            .into_iter()
            .map(|row| {
                use sqlx::Row;
                let id: String = row.get("id");
                let role: String = row.get("role");
                let content: String = row.get("content");
                let model: Option<String> = row.get("model");
                let _provider: Option<String> = row.get("provider");
                let _agent_id: Option<String> = row.get("agent_id");
                let reasoning: Option<String> = row.get("reasoning");
                let tool_calls_raw: Option<String> = row.get("tool_calls");
                let subject: Option<String> = row.get("subject");
                let thought_signature: Option<String> = row.get("thought_signature");
                let timestamp: Timestamp = row.get("created_at");
                let parts_raw: Option<String> = row.get("parts");

                // Parse tool_calls from JSON if present
                let tool_calls: Option<Vec<ToolCall>> = tool_calls_raw
                    .and_then(|tc| serde_json::from_str(&tc).ok());
                let parts: Option<Vec<UIPart>> = parts_raw
                    .and_then(|p| serde_json::from_str(&p).ok());

                MessageResponse {
                    id,
                    role,
                    content,
                    timestamp,
                    model,
                    tool_calls,
                    reasoning,
                    subject,
                    thought_signature,
                    parts,
                }
            })
            .collect();

    // Get last message for model/provider info
    let last_message = messages_response.last();

    let first_message_at = created_at.parse::<Timestamp>()
        .map_err(|_| crate::Error::Database("Invalid created_at timestamp".into()))?;
    let last_message_at = updated_at.parse::<Timestamp>()
        .map_err(|_| crate::Error::Database("Invalid updated_at timestamp".into()))?;

    let conversation = ConversationMeta {
        conversation_id: id,
        title,
        icon,
        first_message_at,
        last_message_at,
        message_count,
        model: last_message.and_then(|m| m.model.clone()),
        provider: None, // Provider not stored in MessageResponse
    };

    Ok(ChatDetailResponse {
        conversation,
        messages: messages_response,
    })
}

/// Create a new chat
pub async fn create_chat(
    pool: &SqlitePool,
    title: &str,
    messages: Vec<ChatMessage>,
) -> Result<Chat> {
    let timestamp = Utc::now().to_rfc3339();
    let id = crate::ids::generate_id(crate::ids::CHAT_PREFIX, &[title, &timestamp]);
    let message_count = messages.len() as i32;

    // Create chat record (no JSON blob for messages anymore!)
    let row = sqlx::query(
        r#"
        INSERT INTO app_chats (id, title, message_count)
        VALUES (?, ?, ?)
        RETURNING id, title, message_count, created_at, updated_at
        "#,
    )
    .bind(&id)
    .bind(title)
    .bind(message_count)
    .fetch_one(pool)
    .await?;

    // Parse ID
    use sqlx::Row;
    let chat_id: String = row.get("id");
    let chat_title: String = row.get("title");
    let chat_message_count: i32 = row.get("message_count");
    let chat_created_at: String = row.get("created_at");
    let chat_updated_at: String = row.get("updated_at");

    // Insert messages into normalized table
    let mut inserted_messages = Vec::new();
    for (idx, mut msg) in messages.into_iter().enumerate() {
        let msg_id = msg.id.clone().unwrap_or_else(|| {
            crate::ids::generate_id(crate::ids::MESSAGE_PREFIX, &[&chat_id, &uuid::Uuid::new_v4().to_string()])
        });
        msg.id = Some(msg_id.clone());

        let tool_calls_json = msg.tool_calls
            .as_ref()
            .map(|tc| serde_json::to_string(tc))
            .transpose()?;
        let intent_json = msg.intent
            .as_ref()
            .map(|i| serde_json::to_string(i))
            .transpose()?;
        let parts_json = msg.parts
            .as_ref()
            .map(|p| serde_json::to_string(p))
            .transpose()?;

        let sequence_num = (idx + 1) as i32;

        sqlx::query(
            r#"
            INSERT INTO app_chat_messages (
                id, chat_id, role, content, model, provider, agent_id,
                reasoning, tool_calls, intent, subject, thought_signature, sequence_num, created_at, parts
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&msg_id)
        .bind(&chat_id)
        .bind(&msg.role)
        .bind(&msg.content)
        .bind(&msg.model)
        .bind(&msg.provider)
        .bind(&msg.agent_id)
        .bind(&msg.reasoning)
        .bind(&tool_calls_json)
        .bind(&intent_json)
        .bind(&msg.subject)
        .bind(&msg.thought_signature)
        .bind(sequence_num)
        .bind(&msg.timestamp)
        .bind(&parts_json)
        .execute(pool)
        .await?;

        inserted_messages.push(msg);
    }

    // Parse timestamps (handles both SQLite "YYYY-MM-DD HH:MM:SS" and RFC 3339 formats)
    let created_at = chat_created_at.parse::<Timestamp>()
        .map(|ts| ts.into_inner())
        .unwrap_or_else(|_| Utc::now());
    let updated_at = chat_updated_at.parse::<Timestamp>()
        .map(|ts| ts.into_inner())
        .unwrap_or_else(|_| Utc::now());

    Ok(Chat {
        id: chat_id,
        title: chat_title,
        messages: inserted_messages,
        message_count: chat_message_count,
        icon: None,
        created_at,
        updated_at,
    })
}

/// Create a new chat with initial messages (public API)
/// If space_id is provided and not the system space, auto-adds to space_items
pub async fn create_chat_from_request(
    pool: &SqlitePool,
    request: CreateChatRequest,
) -> Result<CreateChatResponse> {
    let chat = create_chat(pool, &request.title, request.messages).await?;

    // Auto-add to space_items if space_id provided and not system space
    if let Some(space_id) = &request.space_id {
        if space_id != SYSTEM_SPACE_ID {
            let url = format!("/chat/{}", chat.id);
            if let Err(e) = crate::api::views::add_space_item(pool, space_id, &url).await {
                tracing::warn!("Failed to auto-add chat to space {}: {}", space_id, e);
                // Don't fail chat creation if auto-add fails
            }
        }
    }

    Ok(CreateChatResponse {
        id: chat.id,
        title: chat.title,
        message_count: chat.message_count,
        created_at: Timestamp::from(chat.created_at),
    })
}

/// Update chat metadata (title and/or icon)
pub async fn update_chat(
    pool: &SqlitePool,
    chat_id: String,
    request: &UpdateChatRequest,
) -> Result<UpdateChatResponse> {
    // Build dynamic SET clauses
    let mut set_clauses = vec!["updated_at = datetime('now')".to_string()];
    let mut binds: Vec<Option<String>> = Vec::new();

    if let Some(ref title) = request.title {
        set_clauses.push("title = ?".to_string());
        binds.push(Some(title.clone()));
    }

    if let Some(ref icon) = request.icon {
        set_clauses.push("icon = ?".to_string());
        binds.push(icon.clone());
    }

    let sql = format!(
        "UPDATE app_chats SET {} WHERE id = ? RETURNING id, title, icon, updated_at",
        set_clauses.join(", ")
    );

    let mut query = sqlx::query(&sql);
    for bind in &binds {
        query = query.bind(bind);
    }
    query = query.bind(&chat_id);

    let row = query.fetch_optional(pool).await?;
    let row = row.ok_or_else(|| crate::Error::NotFound("Chat not found".into()))?;

    use sqlx::Row;
    let id: String = row.get("id");
    let title: String = row.get("title");
    let icon: Option<String> = row.get("icon");
    let updated_at_raw: String = row.get("updated_at");

    let updated_at = updated_at_raw.parse::<Timestamp>()
        .map_err(|_| crate::Error::Database("Invalid updated_at timestamp".into()))?;

    Ok(UpdateChatResponse {
        conversation_id: id,
        title,
        icon,
        updated_at,
    })
}

/// Append a message to a chat (atomic INSERT - no race conditions!)
///
/// Returns the generated message ID for the newly inserted message.
pub async fn append_message(
    pool: &SqlitePool,
    chat_id: String,
    message: ChatMessage,
) -> Result<String> {
    let chat_id_str = chat_id.clone();

    // Generate stable message ID
    let msg_id = message.id.clone().unwrap_or_else(|| {
        crate::ids::generate_id(crate::ids::MESSAGE_PREFIX, &[&chat_id_str, &uuid::Uuid::new_v4().to_string()])
    });

    // Get next sequence number atomically
    let sequence_num = get_next_sequence_num(pool, &chat_id_str).await?;

    // Serialize tool_calls and intent to JSON if present
    let tool_calls_json = message.tool_calls
        .as_ref()
        .map(|tc| serde_json::to_string(tc))
        .transpose()?;
    let intent_json = message.intent
        .as_ref()
        .map(|i| serde_json::to_string(i))
        .transpose()?;

    // Insert into normalized table (atomic, idempotent with INSERT OR IGNORE)
    // If a message with this ID already exists, silently ignore the duplicate
    let parts_json = message.parts
        .as_ref()
        .map(|p| serde_json::to_string(p))
        .transpose()?;

    let result = sqlx::query(
        r#"
        INSERT OR IGNORE INTO app_chat_messages (
            id, chat_id, role, content, model, provider, agent_id,
            reasoning, tool_calls, intent, subject, thought_signature, sequence_num, created_at, parts
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&msg_id)
    .bind(&chat_id_str)
    .bind(&message.role)
    .bind(&message.content)
    .bind(&message.model)
    .bind(&message.provider)
    .bind(&message.agent_id)
    .bind(&message.reasoning)
    .bind(&tool_calls_json)
    .bind(&intent_json)
    .bind(&message.subject)
    .bind(&message.thought_signature)
    .bind(sequence_num)
    .bind(&message.timestamp)
    .bind(&parts_json)
    .execute(pool)
    .await?;

    // Only update chat metadata if we actually inserted a new row
    if result.rows_affected() > 0 {
        // Update chat metadata (message count and updated_at)
        sqlx::query(
            r#"
            UPDATE app_chats
            SET message_count = message_count + 1, updated_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(&chat_id_str)
        .execute(pool)
        .await?;
    }

    Ok(msg_id)
}

/// Update messages in a chat (replace all messages)
///
/// Deletes all existing messages and re-inserts the new set.
/// Used for editing messages or regenerating responses.
pub async fn update_messages(
    pool: &SqlitePool,
    chat_id: String,
    messages: Vec<ChatMessage>,
) -> Result<()> {
    let chat_id_str = chat_id.clone();
    let message_count = messages.len() as i32;

    // Delete all existing messages for this chat
    sqlx::query("DELETE FROM app_chat_messages WHERE chat_id = ?")
        .bind(&chat_id_str)
        .execute(pool)
        .await?;

    // Re-insert all messages with new sequence numbers
    for (idx, msg) in messages.into_iter().enumerate() {
        let msg_id = msg.id.clone().unwrap_or_else(|| {
            crate::ids::generate_id(crate::ids::MESSAGE_PREFIX, &[&chat_id_str, &uuid::Uuid::new_v4().to_string()])
        });

        let tool_calls_json = msg.tool_calls
            .as_ref()
            .map(|tc| serde_json::to_string(tc))
            .transpose()?;
        let intent_json = msg.intent
            .as_ref()
            .map(|i| serde_json::to_string(i))
            .transpose()?;

        let sequence_num = (idx + 1) as i32;

        sqlx::query(
            r#"
            INSERT INTO app_chat_messages (
                id, chat_id, role, content, model, provider, agent_id,
                reasoning, tool_calls, intent, subject, thought_signature, sequence_num, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&msg_id)
        .bind(&chat_id_str)
        .bind(&msg.role)
        .bind(&msg.content)
        .bind(&msg.model)
        .bind(&msg.provider)
        .bind(&msg.agent_id)
        .bind(&msg.reasoning)
        .bind(&tool_calls_json)
        .bind(&intent_json)
        .bind(&msg.subject)
        .bind(&msg.thought_signature)
        .bind(sequence_num)
        .bind(&msg.timestamp)
        .execute(pool)
        .await?;
    }

    // Update chat metadata
    sqlx::query(
        r#"
        UPDATE app_chats
        SET message_count = ?, updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(message_count)
    .bind(&chat_id_str)
    .execute(pool)
    .await?;

    Ok(())
}

/// Delete a chat
/// Also cleans up all space_items references (orphan cleanup)
pub async fn delete_chat(pool: &SqlitePool, chat_id: String) -> Result<DeleteChatResponse> {
    let chat_id_str = chat_id;
    let result = sqlx::query(
        r#"
        DELETE FROM app_chats
        WHERE id = ?
        RETURNING id
        "#,
    )
    .bind(&chat_id_str)
    .fetch_optional(pool)
    .await?;

    let row = result.ok_or_else(|| crate::Error::NotFound("Chat not found".into()))?;

    // Parse ID
    use sqlx::Row;
    let id: String = row.get("id");

    // Clean up all space_items references
    let url = format!("/chat/{}", id);
    if let Err(e) = crate::api::views::remove_items_by_url(pool, &url).await {
        tracing::warn!("Failed to clean up space_items for chat {}: {}", id, e);
        // Don't fail deletion if cleanup fails
    }

    Ok(DeleteChatResponse {
        success: true,
        conversation_id: id,
    })
}

/// Generate a title for a chat using AI
///
/// Uses Tollbooth with system user (no specific user context for background operations)
pub async fn generate_title(
    pool: &SqlitePool,
    chat_id: String,
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

    // Update chat title in database
    sqlx::query(
        r#"
        UPDATE app_chats
        SET title = ?, updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(&title)
    .bind(&chat_id)
    .execute(pool)
    .await?;

    Ok(GenerateTitleResponse { chat_id, title })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message_serialization() {
        let message = ChatMessage {
            id: None,
            role: "user".to_string(),
            content: "Hello".to_string(),
            timestamp: Timestamp::parse("2024-01-01T00:00:00Z").unwrap(),
            model: None,
            provider: None,
            agent_id: None,
            tool_calls: None,
            reasoning: None,
            intent: None,
            subject: None,
            thought_signature: None,
            parts: None,
        };

        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"content\":\"Hello\""));
        // Optional fields should not be present when None
        assert!(!json.contains("\"model\""));
    }
}

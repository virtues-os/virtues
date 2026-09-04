//! Chats API
//!
//! CRUD operations for chats stored in the chats table.
//! Messages are stored in a normalized chat_messages table for
//! performance, proper indexing, and race-condition-free appends.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::api::chat::UIPart;
use crate::error::{Error, Result};
use crate::types::Timestamp;

// ============================================================================
// Helper Functions
// ============================================================================

/// Get the next sequence number for a chat
async fn get_next_sequence_num(pool: &PgPool, chat_id: &str) -> Result<i32> {
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
    /// `--cat-*` token key, never a hex. See migration 0079.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notebook_id: Option<String>,
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
    #[serde(default)]
    pub icon_color: Option<Option<String>>,
    /// Tri-state: absent = leave, null = detach from Notebook, value = set Notebook.
    /// Routed through `notebooks::set_chat_notebook` (also folds chat into membership).
    #[serde(default, rename = "notebookId")]
    pub notebook_id: Option<Option<String>>,
}

/// Request to create a new chat with initial messages
#[derive(Debug, Deserialize)]
pub struct CreateChatRequest {
    pub title: String,
    pub messages: Vec<ChatMessage>,
    #[serde(rename = "notebookId")]
    pub notebook_id: Option<String>, // For auto-add to notebook_items (not stored on chat)
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_color: Option<String>,
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
pub async fn list_chats(pool: &PgPool, limit: i64) -> Result<ChatListResponse> {
    let rows = sqlx::query(
        r#"
        SELECT
            id,
            title,
            icon,
            icon_color,
            notebook_id,
            message_count,
            created_at,
            updated_at
        FROM app_chats
        ORDER BY updated_at DESC
        LIMIT $1
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
            let icon_color: Option<String> = row.get("icon_color");
            let notebook_id: Option<String> = row.get("notebook_id");
            let message_count: i64 = row.get("message_count");
            let first_message_at: Timestamp = row.get("created_at");
            let last_updated: Timestamp = row.get("updated_at");
            Some(ChatListItem {
                conversation_id: id,
                title,
                message_count: message_count as i32,
                icon,
                icon_color,
                notebook_id,
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
pub async fn get_chat(pool: &PgPool, chat_id: String) -> Result<ChatDetailResponse> {
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
        WHERE id = $1
        "#,
    )
    .bind(&chat_id_str)
    .fetch_optional(pool)
    .await?;

    let row = row.ok_or_else(|| crate::Error::NotFound("Chat not found".into()))?;

    use sqlx::Row;
    let id: String = row.get("id");
    let title: String = row.get("title");
    let icon: Option<String> = row.get("icon");
    let message_count: i64 = row.get("message_count");
    let created_at: Timestamp = row.get("created_at");
    let updated_at: Timestamp = row.get("updated_at");

        // Query messages from normalized table
        // Filter out onboarding synthetic triggers (subject='onboarding_synthetic')
        // so the user only sees the AI's opening message on revisit
        let message_rows = sqlx::query(
            r#"
            SELECT
                id, role, content, model, provider, agent_id,
                reasoning, tool_calls, intent, subject, thought_signature, created_at, parts
            FROM app_chat_messages
            WHERE chat_id = $1
              AND (subject IS NULL OR subject != 'onboarding_synthetic')
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
                let tool_calls_raw: Option<serde_json::Value> = row.get("tool_calls");
                let subject: Option<String> = row.get("subject");
                let thought_signature: Option<String> = row.get("thought_signature");
                let timestamp: Timestamp = row.get("created_at");
                let parts_raw: Option<serde_json::Value> = row.get("parts");

                let tool_calls: Option<Vec<ToolCall>> = tool_calls_raw
                    .and_then(|tc| serde_json::from_value(tc).ok());
                let parts: Option<Vec<UIPart>> = parts_raw
                    .and_then(|p| serde_json::from_value(p).ok());

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

    let first_message_at = created_at;
    let last_message_at = updated_at;

    let conversation = ConversationMeta {
        conversation_id: id,
        title,
        icon,
        first_message_at,
        last_message_at,
        message_count: message_count as i32,
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
    pool: &PgPool,
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
        VALUES ($1, $2, $3)
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
    let chat_message_count: i64 = row.get("message_count");
    let chat_created_at: Timestamp = row.get("created_at");
    let chat_updated_at: Timestamp = row.get("updated_at");

    // Insert messages into normalized table
    let mut inserted_messages = Vec::new();
    for (idx, mut msg) in messages.into_iter().enumerate() {
        let msg_id = msg.id.clone().unwrap_or_else(|| {
            crate::ids::generate_id(crate::ids::MESSAGE_PREFIX, &[&chat_id, &uuid::Uuid::new_v4().to_string()])
        });
        msg.id = Some(msg_id.clone());

        let tool_calls_json: Option<serde_json::Value> = msg.tool_calls
            .as_ref()
            .map(serde_json::to_value)
            .transpose()?;
        let intent_json: Option<serde_json::Value> = msg.intent
            .as_ref()
            .map(serde_json::to_value)
            .transpose()?;
        let parts_json: Option<serde_json::Value> = msg.parts
            .as_ref()
            .map(serde_json::to_value)
            .transpose()?;

        let sequence_num = (idx + 1) as i32;

        sqlx::query(
            r#"
            INSERT INTO app_chat_messages (
                id, chat_id, role, content, model, provider, agent_id,
                reasoning, tool_calls, intent, subject, thought_signature, sequence_num, created_at, parts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
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

    Ok(Chat {
        id: chat_id,
        title: chat_title,
        messages: inserted_messages,
        message_count: chat_message_count as i32,
        icon: None,
        created_at: chat_created_at.into_inner(),
        updated_at: chat_updated_at.into_inner(),
    })
}

/// Create a new chat with initial messages (public API)
/// If notebook_id is provided and not the system notebook, auto-adds to notebook_items
pub async fn create_chat_from_request(
    pool: &PgPool,
    request: CreateChatRequest,
) -> Result<CreateChatResponse> {
    let chat = create_chat(pool, &request.title, request.messages).await?;

    // Bind the chat to its Notebook (stores notebook_id + folds into membership).
    if let Err(e) = crate::api::notebooks::set_chat_notebook(pool, &chat.id, request.notebook_id.as_deref()).await {
        tracing::warn!("Failed to set chat notebook: {}", e);
        // Don't fail chat creation if the binding fails
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
    pool: &PgPool,
    chat_id: String,
    request: &UpdateChatRequest,
) -> Result<UpdateChatResponse> {
    // Build dynamic SET clauses with sequentially-numbered Postgres
    // placeholders ($1, $2, ...). WHERE id binds the last index.
    let mut set_clauses = vec!["updated_at = now()".to_string()];
    let mut binds: Vec<Option<String>> = Vec::new();

    if let Some(ref title) = request.title {
        binds.push(Some(title.clone()));
        set_clauses.push(format!("title = ${}", binds.len()));
    }

    if let Some(ref icon) = request.icon {
        binds.push(icon.clone());
        set_clauses.push(format!("icon = ${}", binds.len()));
    }

    if let Some(ref icon_color) = request.icon_color {
        binds.push(icon_color.clone());
        set_clauses.push(format!("icon_color = ${}", binds.len()));
    }

    let sql = format!(
        "UPDATE app_chats SET {} WHERE id = ${} RETURNING id, title, icon, icon_color, updated_at",
        set_clauses.join(", "),
        binds.len() + 1
    );

    let mut query = sqlx::query(&sql);
    for bind in &binds {
        query = query.bind(bind);
    }
    query = query.bind(&chat_id);

    let row = query.fetch_optional(pool).await?;
    let row = row.ok_or_else(|| crate::Error::NotFound("Chat not found".into()))?;

    // Bind/unbind the chat's Notebook if the field was provided.
    if let Some(ref notebook_id) = request.notebook_id {
        crate::api::notebooks::set_chat_notebook(pool, &chat_id, notebook_id.as_deref()).await?;
    }

    use sqlx::Row;
    let id: String = row.get("id");
    let title: String = row.get("title");
    let icon: Option<String> = row.get("icon");
    let icon_color: Option<String> = row.get("icon_color");
    let updated_at: Timestamp = row.get("updated_at");

    Ok(UpdateChatResponse {
        conversation_id: id,
        title,
        icon,
        icon_color,
        updated_at,
    })
}

/// Append a message to a chat (atomic INSERT - no race conditions!)
///
/// Returns the generated message ID for the newly inserted message.
pub async fn append_message(
    pool: &PgPool,
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

    // Serialize tool_calls/intent/parts to serde_json::Value so sqlx binds
    // them as jsonb (the columns are jsonb, not text — binding a String would
    // raise `expression is of type text`).
    let tool_calls_json: Option<serde_json::Value> = message.tool_calls
        .as_ref()
        .map(serde_json::to_value)
        .transpose()?;
    let intent_json: Option<serde_json::Value> = message.intent
        .as_ref()
        .map(serde_json::to_value)
        .transpose()?;
    let parts_json: Option<serde_json::Value> = message.parts
        .as_ref()
        .map(serde_json::to_value)
        .transpose()?;

    let result = sqlx::query(
        r#"
        INSERT INTO app_chat_messages (
            id, chat_id, role, content, model, provider, agent_id,
            reasoning, tool_calls, intent, subject, thought_signature, sequence_num, created_at, parts
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
        ON CONFLICT (id) DO NOTHING
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
            SET message_count = message_count + 1, updated_at = now()
            WHERE id = $1
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
    pool: &PgPool,
    chat_id: String,
    messages: Vec<ChatMessage>,
) -> Result<()> {
    let chat_id_str = chat_id.clone();
    let message_count = messages.len() as i32;

    // Delete all existing messages for this chat
    sqlx::query("DELETE FROM app_chat_messages WHERE chat_id = $1")
        .bind(&chat_id_str)
        .execute(pool)
        .await?;

    // Re-insert all messages with new sequence numbers
    for (idx, msg) in messages.into_iter().enumerate() {
        let msg_id = msg.id.clone().unwrap_or_else(|| {
            crate::ids::generate_id(crate::ids::MESSAGE_PREFIX, &[&chat_id_str, &uuid::Uuid::new_v4().to_string()])
        });

        let tool_calls_json: Option<serde_json::Value> = msg.tool_calls
            .as_ref()
            .map(serde_json::to_value)
            .transpose()?;
        let intent_json: Option<serde_json::Value> = msg.intent
            .as_ref()
            .map(serde_json::to_value)
            .transpose()?;

        let sequence_num = (idx + 1) as i32;

        sqlx::query(
            r#"
            INSERT INTO app_chat_messages (
                id, chat_id, role, content, model, provider, agent_id,
                reasoning, tool_calls, intent, subject, thought_signature, sequence_num, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
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
        SET message_count = $1, updated_at = now()
        WHERE id = $2
        "#,
    )
    .bind(message_count)
    .bind(&chat_id_str)
    .execute(pool)
    .await?;

    Ok(())
}

/// Delete a chat
/// Also cleans up all notebook_items references (orphan cleanup)
pub async fn delete_chat(pool: &PgPool, chat_id: String) -> Result<DeleteChatResponse> {
    // The narrative interview is undeletable, decided by the id like every
    // other property of that room (mode, title). Boot would re-seed the CHAT
    // row, but the transcript — the most private data on the box and the
    // drafter's only source — would be gone on one misclick, unrecoverably.
    if chat_id == crate::api::narrative_draft::INTERVIEW_CHAT_ID {
        return Err(crate::Error::InvalidInput(
            "The interview conversation can't be deleted — it is the source of \"In your own words\".".into(),
        ));
    }
    let chat_id_str = chat_id;
    let result = sqlx::query(
        r#"
        DELETE FROM app_chats
        WHERE id = $1
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

    // Clean up all notebook_items references
    let url = format!("/chat/{}", id);
    if let Err(e) = crate::api::notebooks::remove_items_by_url(pool, &url).await {
        tracing::warn!("Failed to clean up notebook_items for chat {}: {}", id, e);
        // Don't fail deletion if cleanup fails
    }

    Ok(DeleteChatResponse {
        success: true,
        conversation_id: id,
    })
}

/// Generate a title for a chat using AI
///
/// Uses virtues-api with system user (no specific user context for background operations)
pub async fn generate_title(
    pool: &PgPool,
    chat_id: String,
    messages: &[TitleMessage],
) -> Result<GenerateTitleResponse> {
    // The narrative interview keeps its seeded name, and the id decides that
    // — never the client (same doctrine as interview mode itself, see
    // chat_handler). A generated title would ship the most private transcript
    // on the box to a model to be summarised, and then print the summary in
    // the sidebar: this chat had already renamed itself after the person's
    // own childhood.
    if chat_id == crate::api::narrative_draft::INTERVIEW_CHAT_ID {
        let title: String = sqlx::query_scalar("SELECT title FROM app_chats WHERE id = $1")
            .bind(&chat_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| Error::Database(format!("read interview title: {e}")))?
            // absent-ok: the error path is handled by the `?` above; this
            // default only covers a chat that has no title yet.
            .unwrap_or_else(|| "In your own words".to_string());
        return Ok(GenerateTitleResponse { chat_id, title });
    }
    // Get background model from assistant profile
    let background_model = crate::api::assistant_profile::get_background_model(pool).await?;

    // Build conversation summary (first few messages)
    let messages_to_include: Vec<&TitleMessage> =
        messages.iter().take(6.min(messages.len())).collect();
    let conversation_summary: String = messages_to_include
        .iter()
        // Chars, not bytes — `&content[..200]` panics if byte 200 lands mid
        // character, and the first message of a chat is arbitrary user text.
        .map(|m| {
            let head: String = m.content.chars().take(200).collect();
            format!("{}: {}", m.role, head)
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    // "Plain text" spelled out, because models reach for emphasis when asked
    // for a title — they were returning `**The Definition and Meaning of
    // Life**`, and a tab label has no markdown renderer, so the asterisks
    // showed. The stripping below is the belt to this braces: the instruction
    // fixes the common case, the parse fixes the rest.
    let prompt = format!(
        r#"Based on this conversation, generate a very short title (3-6 words maximum) that captures the main topic or theme.

Return the title as plain text only. No markdown, no asterisks, no bold, no quotation marks, no trailing punctuation, no preamble — just the words of the title.

Conversation:
{}"#,
        conversation_summary
    );

    // Call virtues-api with the device bearer (auto-renews on 402 expiry).
    let client = crate::virtues_api::client::BearerClient::from_env(pool.clone());
    let response = client
        .post_json(
            "/v1/ai/chat/completions",
            &serde_json::json!({
                "model": background_model,
                "messages": [
                    {"role": "user", "content": prompt}
                ],
                "max_tokens": 50
            }),
        )
        .await
        .map_err(|e| crate::Error::Network(format!("virtues-api request failed: {e}")))?;

    if !response.is_success() {
        // Provide user-friendly message for budget errors
        let error_msg = match response.status {
            402 => crate::virtues_api::client::payment_required_message(&response.body, "title generation"),
            429 => "Rate limited. Please try again later.".to_string(),
            _ => format!("virtues-api error: {}", response.body),
        };
        return Err(crate::Error::ExternalApi(error_msg));
    }

    let response_json = response.body;
    let mut title = response_json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("New Chat")
        .trim()
        .to_string();

    // Strip the costume the model put on the title: markdown emphasis, a
    // leading heading marker, quotes. Trimmed as a set and repeatedly, because
    // they nest — `**"Title"**` arrives with two layers, and one pass would
    // leave the inner one. Cheap, and the alternative is asterisks in a tab.
    loop {
        let before = title.clone();
        title = title
            .trim()
            .trim_start_matches('#')
            .trim_matches(|c| c == '"' || c == '\'' || c == '*' || c == '_')
            .trim()
            .to_string();
        if title == before {
            break;
        }
    }

    // Truncate if too long. By CHARS, not bytes: `&title[..57]` panics when
    // byte 57 lands inside a multi-byte character, and titles are arbitrary
    // user-topic text — an accented word or an emoji was a crash waiting for
    // the right conversation.
    if title.chars().count() > 60 {
        let head: String = title.chars().take(57).collect();
        title = format!("{}...", head);
    }

    // Update chat title in database
    sqlx::query(
        r#"
        UPDATE app_chats
        SET title = $1, updated_at = now()
        WHERE id = $2
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

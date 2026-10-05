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
    /// The gateway's normalized reasoning blocks for this assistant turn
    /// (text plus provider signatures), as an array. Stored so the turn can
    /// be resent with its thinking intact; today only the loop within a turn
    /// echoes them, because history rebuilds assistant rows as text only.
    #[serde(rename = "reasoningDetails", skip_serializing_if = "Option::is_none")]
    pub reasoning_details: Option<serde_json::Value>,
    #[serde(default, with = "crate::api::chat::wire_parts")]
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
    pub project_id: Option<String>,
    pub first_message_at: Timestamp,
    /// When anything on the row last changed: a rename, a filing, an icon.
    pub last_updated: Timestamp,
    /// When the conversation last moved: its newest message, or its creation
    /// for a chat with none yet. The list is ordered by this and the Home
    /// panel groups by it, so filing or renaming an old chat does not make it
    /// one you talked in today.
    pub last_message_at: Timestamp,
    /// A reply landed after the person last had this chat on screen
    /// (`app_chat_seen`). The sidebar's dot.
    pub unread: bool,
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
    /// The project this chat is filed in. On the chat itself because the chat
    /// list holds only the most recent chats, and an older chat opened from a
    /// project could not otherwise say where it lives.
    pub project_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Both of these were read off the row and dropped on the floor: the
    /// client asks for `provider` and `agentId` when it rebuilds a message's
    /// metadata, and got `undefined` for every message on every reload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(rename = "agentId", skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(rename = "reasoningDetails", skip_serializing_if = "Option::is_none")]
    pub reasoning_details: Option<serde_json::Value>,
    #[serde(default, with = "crate::api::chat::wire_parts")]
    pub parts: Option<Vec<UIPart>>,
    /// The span of the turn that wrote an assistant row (migration 0040),
    /// shown as "Worked for". Absent where it was never recorded.
    #[serde(rename = "startedAt", skip_serializing_if = "Option::is_none")]
    pub started_at: Option<Timestamp>,
    #[serde(rename = "endedAt", skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<Timestamp>,
}

/// Request to update chat metadata (title and/or icon)
#[derive(Debug, Deserialize)]
pub struct UpdateChatRequest {
    pub title: Option<String>,
    pub icon: Option<Option<String>>,
    #[serde(default)]
    pub icon_color: Option<Option<String>>,
    /// Tri-state: absent = leave, null = detach from Project, value = set Project.
    /// Routed through `projects::set_chat_project` (also folds chat into membership).
    #[serde(default, rename = "projectId", alias = "notebookId")]
    pub project_id: Option<Option<String>>,
}

/// Request to create a new chat with initial messages
#[derive(Debug, Deserialize)]
pub struct CreateChatRequest {
    pub title: String,
    pub messages: Vec<ChatMessage>,
    #[serde(rename = "projectId", alias = "notebookId")]
    pub project_id: Option<String>, // For auto-add to project_items (not stored on chat)
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
    /// No automatic title will ever be written to this chat again: the person
    /// named it, or the one re-title has happened. The client stops asking.
    pub done: bool,
}

/// User turns a chat needs before its generated title is replaced, once, by
/// one that has seen the conversation rather than its opening.
const SETTLE_AFTER_USER_TURNS: i64 = 6;

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
            project_id,
            message_count,
            created_at,
            updated_at,
            COALESCE(
                (SELECT MAX(m.created_at) FROM app_chat_messages m WHERE m.chat_id = c.id),
                created_at
            ) AS last_message_at,
            EXISTS (
                SELECT 1 FROM app_chat_messages m
                 WHERE m.chat_id = c.id AND m.role = 'assistant'
                   AND m.created_at > COALESCE(s.seen_at, '-infinity'::timestamptz)
            ) AS unread
        FROM app_chats c
        LEFT JOIN app_chat_seen s ON s.chat_id = c.id
        WHERE deleted_at IS NULL
        ORDER BY last_message_at DESC
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
            let project_id: Option<String> = row.get("project_id");
            let message_count: i64 = row.get("message_count");
            let first_message_at: Timestamp = row.get("created_at");
            let last_updated: Timestamp = row.get("updated_at");
            let last_message_at: Timestamp = row.get("last_message_at");
            let unread: bool = row.get("unread");
            Some(ChatListItem {
                conversation_id: id,
                title,
                message_count: message_count as i32,
                icon,
                icon_color,
                project_id,
                first_message_at,
                last_updated,
                last_message_at,
                unread,
            })
        })
        .collect();

    Ok(ChatListResponse {
        conversations,
        source: "app_schema".to_string(),
    })
}

/// The person has this chat on screen as of now: every reply in it so far is
/// read. Upserted, so the first open of a chat an applet started makes its row;
/// an id with no chat behind it (a new chat not yet sent) is a no-op.
pub async fn mark_seen(pool: &PgPool, chat_id: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO app_chat_seen (chat_id, seen_at) SELECT id, now() FROM app_chats WHERE id = $1 \
         ON CONFLICT (chat_id) DO UPDATE SET seen_at = GREATEST(app_chat_seen.seen_at, now())",
    )
    .bind(chat_id)
    .execute(pool)
    .await?;
    Ok(())
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
            project_id,
            message_count,
            created_at,
            updated_at
        FROM app_chats
        WHERE id = $1 AND deleted_at IS NULL
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
    let project_id: Option<String> = row.get("project_id");
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
                reasoning, tool_calls, intent, subject, reasoning_details, created_at, parts,
                started_at, ended_at
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
                let provider: Option<String> = row.get("provider");
                let agent_id: Option<String> = row.get("agent_id");
                let reasoning: Option<String> = row.get("reasoning");
                let tool_calls_raw: Option<serde_json::Value> = row.get("tool_calls");
                let subject: Option<String> = row.get("subject");
                let reasoning_details: Option<serde_json::Value> = row.get("reasoning_details");
                let timestamp: Timestamp = row.get("created_at");
                let parts_raw: Option<serde_json::Value> = row.get("parts");
                let started_at: Option<Timestamp> = row.get("started_at");
                let ended_at: Option<Timestamp> = row.get("ended_at");

                let tool_calls: Option<Vec<ToolCall>> = tool_calls_raw.and_then(|tc| {
                    serde_json::from_value(tc)
                        .map_err(|e| tracing::warn!(msg_id = %id, error = %e, "tool_calls did not parse"))
                        .ok()
                });
                let parts: Option<Vec<UIPart>> = parts_raw
                    .and_then(|p| crate::api::chat::parts_from_jsonb(p, &id));

                MessageResponse {
                    id,
                    role,
                    content,
                    timestamp,
                    model,
                    provider,
                    agent_id,
                    tool_calls,
                    reasoning,
                    subject,
                    reasoning_details,
                    parts,
                    started_at,
                    ended_at,
                }
            })
            .collect();

    // Get last message for model/provider info
    let last_message = messages_response.last();

    let first_message_at = created_at;
    // The newest message, as the chat list reports it; `updated_at` moves on
    // any change to the row, a filing or a rename included.
    let last_message_at = last_message.map(|m| m.timestamp.clone()).unwrap_or(updated_at);

    let conversation = ConversationMeta {
        conversation_id: id,
        title,
        icon,
        first_message_at,
        last_message_at,
        message_count: message_count as i32,
        model: last_message.and_then(|m| m.model.clone()),
        provider: None, // Provider not stored in MessageResponse
        project_id,
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
        let parts_json: Option<serde_json::Value> =
            msg.parts.as_deref().map(crate::api::chat::parts_to_jsonb);

        let sequence_num = (idx + 1) as i32;

        sqlx::query(
            r#"
            INSERT INTO app_chat_messages (
                id, chat_id, role, content, model, provider, agent_id,
                reasoning, tool_calls, intent, subject, reasoning_details, sequence_num, created_at, parts
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
        .bind(&msg.reasoning_details)
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
/// If project_id is provided and not the system project, auto-adds to project_items
pub async fn create_chat_from_request(
    pool: &PgPool,
    request: CreateChatRequest,
) -> Result<CreateChatResponse> {
    let chat = create_chat(pool, &request.title, request.messages).await?;

    // Bind the chat to its Project (stores project_id + folds into membership).
    if let Err(e) = crate::api::projects::set_chat_project(pool, &chat.id, request.project_id.as_deref()).await {
        tracing::warn!("Failed to set chat project: {}", e);
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

    // A title through this door is a person naming the chat, so the
    // generator never writes over it (see `generate_title`).
    if let Some(ref title) = request.title {
        binds.push(Some(title.clone()));
        set_clauses.push(format!("title = ${}", binds.len()));
        set_clauses.push("title_source = 'user'".to_string());
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

    // Bind/unbind the chat's Project if the field was provided.
    if let Some(ref project_id) = request.project_id {
        crate::api::projects::set_chat_project(pool, &chat_id, project_id.as_deref()).await?;
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

/// A chat's whole transcript, in order, as the model and the context gauge
/// read it: every row, checkpoints included, with the jsonb columns decoded.
///
/// The transcript the person sees (`get_chat`) is not this — it hides the
/// onboarding trigger rows and answers in its own response shape.
pub async fn load_messages(pool: &PgPool, chat_id: &str) -> Result<Vec<ChatMessage>> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"
        SELECT
            id, role, content, created_at,
            model, provider, agent_id, reasoning, tool_calls, intent, subject, reasoning_details, parts
        FROM app_chat_messages
        WHERE chat_id = $1
        ORDER BY sequence_num ASC
        "#,
    )
    .bind(chat_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let id: String = row.get("id");
            // Columns are jsonb; read as serde_json::Value, not String.
            let tool_calls_raw: Option<serde_json::Value> = row.get("tool_calls");
            let intent_raw: Option<serde_json::Value> = row.get("intent");
            // `parts` carries the attachments and the turn's order; the
            // gauge needs the first to see a PDF, the model the second.
            let parts_raw: Option<serde_json::Value> = row.get("parts");

            // A shape these no longer understand is a turn that silently
            // loses its tool calls or its order and falls back to flat text —
            // which reads as "this message had no tools" rather than as a
            // bug, so it says so out loud.
            let tool_calls = tool_calls_raw.and_then(|tc| {
                serde_json::from_value(tc)
                    .map_err(|e| tracing::warn!(msg_id = %id, error = %e, "tool_calls did not parse"))
                    .ok()
            });
            let intent = intent_raw.and_then(|i| serde_json::from_value(i).ok());
            let parts = parts_raw.and_then(|p| crate::api::chat::parts_from_jsonb(p, &id));

            ChatMessage {
                id: Some(id),
                role: row.get("role"),
                content: row.get("content"),
                timestamp: row.get("created_at"),
                model: row.get("model"),
                provider: row.get("provider"),
                agent_id: row.get("agent_id"),
                reasoning: row.get("reasoning"),
                tool_calls,
                intent,
                subject: row.get("subject"),
                reasoning_details: row.get("reasoning_details"),
                parts,
            }
        })
        .collect())
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
    let parts_json: Option<serde_json::Value> =
        message.parts.as_deref().map(crate::api::chat::parts_to_jsonb);

    // The sequence number is chosen INSIDE the insert. It used to be a
    // `SELECT MAX + 1` followed by a separate statement, described in its own
    // comment as atomic; `(chat_id, sequence_num)` is UNIQUE, so two appends
    // that read the same maximum — a turn finishing while the
    // getting-started room's poll narrates, or one chat open on two devices —
    // raced, and whichever lost was simply never written. If that was the
    // assistant row, the reply the person had just watched was gone on reload.
    let result = sqlx::query(
        r#"
        INSERT INTO app_chat_messages (
            id, chat_id, role, content, model, provider, agent_id,
            reasoning, tool_calls, intent, subject, reasoning_details, sequence_num, created_at, parts
        )
        SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
               COALESCE(MAX(m.sequence_num), 0) + 1, $13, $14
        FROM app_chat_messages m
        WHERE m.chat_id = $2
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
    .bind(&message.reasoning_details)
    .bind(&message.timestamp)
    .bind(&parts_json)
    .execute(pool)
    .await?;

    // Only update chat metadata if we actually inserted a new row
    if result.rows_affected() > 0 {
        // Counted, not incremented. `+ 1` has no inverse, and rows are deleted
        // in two places that never decremented — regenerate, and the room's own
        // sweep — so the count drifted upward for good and everything derived
        // from it (`message_count - messages_summarized`, shown in the context
        // panel) drifted with it, into negative numbers after an edit.
        sqlx::query(
            r#"
            UPDATE app_chats
            SET message_count = (
                    SELECT count(*) FROM app_chat_messages WHERE chat_id = $1
                ),
                updated_at = now()
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
        // `parts` is the turn's order — what was said before which call. This
        // insert was written before the column existed and never grew it, so
        // every message in the chat came back flattened. The other two insert
        // sites carry it; a column that only two of three writers know about
        // is how a rewrite silently becomes a loss.
        let parts_json: Option<serde_json::Value> =
            msg.parts.as_deref().map(crate::api::chat::parts_to_jsonb);

        let sequence_num = (idx + 1) as i32;

        sqlx::query(
            r#"
            INSERT INTO app_chat_messages (
                id, chat_id, role, content, model, provider, agent_id,
                reasoning, tool_calls, intent, subject, reasoning_details, sequence_num, created_at, parts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
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
        .bind(&msg.reasoning_details)
        .bind(sequence_num)
        .bind(&msg.timestamp)
        .bind(&parts_json)
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

/// Delete a chat — into the trash, not for good. The row is stamped
/// `deleted_at` and leaves every listing and the search index; Recently
/// deleted holds it for `trash::TRASH_RETENTION_DAYS` with a restore, and the
/// hard delete is `trash::purge`, reachable only from there and the sweeper.
/// Messages and project membership stay with the row so a restore is whole.
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
    // Same for getting started: the room is seeded, and the sidebar's way
    // back to any open step is this chat.
    if chat_id == crate::api::getting_started::GETTING_STARTED_CHAT_ID {
        return Err(crate::Error::InvalidInput(
            "You can't delete the getting-started conversation.".into(),
        ));
    }
    crate::api::trash::trash(pool, crate::api::trash::TrashKind::Chat, &chat_id).await?;

    Ok(DeleteChatResponse {
        success: true,
        conversation_id: chat_id,
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
    if chat_id == crate::api::narrative_draft::INTERVIEW_CHAT_ID
        || chat_id == crate::api::getting_started::GETTING_STARTED_CHAT_ID
    {
        let title: String = sqlx::query_scalar("SELECT title FROM app_chats WHERE id = $1")
            .bind(&chat_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| Error::Database(format!("read interview title: {e}")))?
            // absent-ok: the error path is handled by the `?` above; this
            // default only covers a chat that has no title yet.
            .unwrap_or_else(|| {
                if chat_id == crate::api::getting_started::GETTING_STARTED_CHAT_ID {
                    "Getting started".to_string()
                } else {
                    "In your own words".to_string()
                }
            });
        return Ok(GenerateTitleResponse { chat_id, title, done: true });
    }

    // Who wrote the current title decides whether to write another. The
    // client asks after every send and cannot know; the row can. A chat is
    // titled at most twice: once after the first exchange, and once more
    // when it has run long enough that the opening no longer describes it.
    // A hand rename ends it.
    let Some((current, source, user_turns)) = sqlx::query_as::<_, (String, String, i64)>(
        "SELECT c.title, c.title_source, \
                (SELECT count(*) FROM app_chat_messages m \
                  WHERE m.chat_id = c.id AND m.role = 'user') \
           FROM app_chats c WHERE c.id = $1",
    )
    .bind(&chat_id)
    .fetch_optional(pool)
    .await?
    else {
        return Err(Error::NotFound("Chat not found".into()));
    };
    let next_source = match source.as_str() {
        "seed" => "generated",
        "generated" if user_turns >= SETTLE_AFTER_USER_TURNS => "settled",
        "generated" => {
            return Ok(GenerateTitleResponse { chat_id, title: current, done: false });
        }
        _ => return Ok(GenerateTitleResponse { chat_id, title: current, done: true }),
    };

    // The first title reads the opening. The re-title reads the opening and
    // where the chat is now, since that drift is why it re-titles at all.
    let messages_to_include: Vec<&TitleMessage> = if next_source == "settled" && messages.len() > 8 {
        messages.iter().take(2).chain(messages.iter().skip(messages.len() - 6)).collect()
    } else {
        messages.iter().take(6).collect()
    };
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

    // Through the shared background helper: the Lite slot via the owner's
    // background pin, thinking off, no cap. The prompt bounds the title; the
    // 50-token cap that sat here was spent thinking on a model that thinks
    // and returned no title at all.
    let mut title = crate::virtues_api::completion::system_completion(
        pool,
        virtues_registry::models::ModelSlot::Lite,
        "chat_title",
        "",
        &prompt,
        crate::virtues_api::request::Thinking::Off,
        0.7,
    )
    .await
    .map_err(|e| match e {
        // The helper's messages are already the user-facing ones (wallet,
        // rate limit); only the bucket name changes.
        crate::Error::ExternalApi(m) => crate::Error::ExternalApi(m),
        other => crate::Error::Network(format!("title generation failed: {other}")),
    })?
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

    // An empty answer keeps the title there is and retries next turn.
    // Writing a placeholder over it would replace the first message's words
    // with "New Chat".
    if title.is_empty() {
        return Ok(GenerateTitleResponse { chat_id, title: current, done: false });
    }

    // Only if the title is still the one this call read: a rename that
    // landed while the model was answering is the person's, and wins.
    let written: Option<String> = sqlx::query_scalar(
        "UPDATE app_chats SET title = $1, title_source = $2 \
          WHERE id = $3 AND title_source = $4 RETURNING title",
    )
    .bind(&title)
    .bind(next_source)
    .bind(&chat_id)
    .bind(&source)
    .fetch_optional(pool)
    .await?;
    match written {
        Some(title) => Ok(GenerateTitleResponse { chat_id, title, done: next_source == "settled" }),
        None => {
            let (title, source): (String, String) =
                sqlx::query_as("SELECT title, title_source FROM app_chats WHERE id = $1")
                    .bind(&chat_id)
                    .fetch_one(pool)
                    .await?;
            let done = matches!(source.as_str(), "user" | "settled");
            Ok(GenerateTitleResponse { chat_id, title, done })
        }
    }
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
            reasoning_details: None,
            parts: None,
        };

        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"content\":\"Hello\""));
        // Optional fields should not be present when None
        assert!(!json.contains("\"model\""));
    }

    /// The model's view of a chat: every row in order, jsonb decoded.
    #[sqlx::test]
    async fn load_messages_reads_every_row_in_order(pool: PgPool) {
        sqlx::query("INSERT INTO app_chats (id, title, message_count) VALUES ('chat_lm', 't', 0)")
            .execute(&pool)
            .await
            .unwrap();
        let mut user = ChatMessage {
            id: Some("m_user".into()),
            role: "user".into(),
            content: "hi".into(),
            timestamp: Timestamp::now(),
            model: None,
            provider: None,
            agent_id: None,
            tool_calls: None,
            reasoning: None,
            intent: None,
            subject: None,
            reasoning_details: None,
            parts: Some(vec![UIPart::Text { text: "hi".into() }]),
        };
        append_message(&pool, "chat_lm".into(), user.clone()).await.unwrap();
        user.id = Some("m_reply".into());
        user.role = "assistant".into();
        user.content = "hello".into();
        user.subject = Some("interrupted".into());
        user.tool_calls = Some(vec![ToolCall {
            tool_name: "sql_query".into(),
            tool_call_id: Some("c1".into()),
            arguments: serde_json::json!({"q": 1}),
            result: None,
            timestamp: "2026-09-28T00:00:00Z".into(),
        }]);
        append_message(&pool, "chat_lm".into(), user).await.unwrap();

        let got = load_messages(&pool, "chat_lm").await.unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].id.as_deref(), Some("m_user"));
        assert!(matches!(got[0].parts.as_deref(), Some([UIPart::Text { .. }])));
        assert_eq!(got[1].role, "assistant");
        assert_eq!(got[1].subject.as_deref(), Some("interrupted"));
        assert_eq!(got[1].tool_calls.as_ref().map(|t| t[0].tool_name.as_str()), Some("sql_query"));
    }

    /// A chat whose title the generator must leave alone never reaches the
    /// model: these return before `system_completion`, so they run offline.
    #[sqlx::test(migrations = "./migrations")]
    async fn generate_title_leaves_named_and_young_chats_alone(pool: PgPool) {
        for (id, source) in [("chat_named", "user"), ("chat_settled", "settled"), ("chat_young", "generated")] {
            sqlx::query("INSERT INTO app_chats (id, title, message_count, title_source) VALUES ($1, 'Kept', 0, $2)")
                .bind(id)
                .bind(source)
                .execute(&pool)
                .await
                .unwrap();
        }
        let msgs = [TitleMessage { role: "user".into(), content: "hi".into() }];

        for id in ["chat_named", "chat_settled"] {
            let r = generate_title(&pool, id.into(), &msgs).await.unwrap();
            assert_eq!((r.title.as_str(), r.done), ("Kept", true), "{id}");
        }
        // Under the settle threshold: kept, and the client should keep asking.
        let r = generate_title(&pool, "chat_young".into(), &msgs).await.unwrap();
        assert_eq!((r.title.as_str(), r.done), ("Kept", false));
    }

    /// A reply after the last look is unread; looking clears it; a chat never
    /// opened counts every reply; an unknown id is a no-op.
    #[sqlx::test(migrations = "./migrations")]
    async fn unread_follows_replies_after_the_last_look(pool: PgPool) {
        for id in ["chat_seen", "chat_never"] {
            sqlx::query("INSERT INTO app_chats (id, title, message_count) VALUES ($1, 't', 0)")
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
        }
        mark_seen(&pool, "chat_seen").await.unwrap();
        mark_seen(&pool, "chat_nowhere").await.unwrap();
        let unread = |pool: PgPool| async move {
            let list = list_chats(&pool, 10).await.unwrap();
            let mut v: Vec<(String, bool)> =
                list.conversations.into_iter().map(|c| (c.conversation_id, c.unread)).collect();
            v.sort();
            v
        };
        assert_eq!(unread(pool.clone()).await, [("chat_never".into(), false), ("chat_seen".into(), false)]);

        for (i, id) in ["chat_seen", "chat_never"].into_iter().enumerate() {
            sqlx::query(
                "INSERT INTO app_chat_messages (id, chat_id, role, content, sequence_num, created_at) \
                 VALUES ($1, $2, 'assistant', 'hi', 0, now() + interval '1 second')",
            )
            .bind(format!("m{i}"))
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        }
        assert_eq!(unread(pool.clone()).await, [("chat_never".into(), true), ("chat_seen".into(), true)]);

        sqlx::query("UPDATE app_chat_seen SET seen_at = now() + interval '2 seconds'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(unread(pool.clone()).await, [("chat_never".into(), true), ("chat_seen".into(), false)]);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn renaming_a_chat_marks_the_title_as_the_persons(pool: PgPool) {
        sqlx::query("INSERT INTO app_chats (id, title, message_count) VALUES ('chat_r', 'Seeded', 0)")
            .execute(&pool)
            .await
            .unwrap();
        let req = UpdateChatRequest { title: Some("Mine".into()), icon: None, icon_color: None, project_id: None };
        update_chat(&pool, "chat_r".into(), &req).await.unwrap();
        let source: String = sqlx::query_scalar("SELECT title_source FROM app_chats WHERE id = 'chat_r'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(source, "user");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn filing_an_old_chat_does_not_make_it_recent(pool: sqlx::PgPool) {
        for (id, at) in [("chat_old", "2026-01-01T00:00:00Z"), ("chat_new", "2026-06-01T00:00:00Z")] {
            sqlx::query("INSERT INTO app_chats (id, title, message_count, created_at) VALUES ($1, 'Chat', 1, $2::timestamptz)")
                .bind(id)
                .bind(at)
                .execute(&pool)
                .await
                .expect("seed chat");
            sqlx::query(
                "INSERT INTO app_chat_messages (id, chat_id, role, content, sequence_num, created_at) \
                 VALUES ($1, $2, 'user', 'Hi', 0, $3::timestamptz)",
            )
            .bind(format!("msg_{id}"))
            .bind(id)
            .bind(at)
            .execute(&pool)
            .await
            .expect("seed message");
        }
        // Any update touches updated_at through the table's trigger.
        sqlx::query("UPDATE app_chats SET title = 'Renamed' WHERE id = 'chat_old'")
            .execute(&pool)
            .await
            .expect("touch the old chat");

        let listed = list_chats(&pool, 10).await.expect("list");
        let ids: Vec<&str> = listed.conversations.iter().map(|c| c.conversation_id.as_str()).collect();
        assert_eq!(ids, vec!["chat_new", "chat_old"]);
    }
}

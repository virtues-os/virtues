//! AI chat messages → `data_content_conversation`.
//!
//! Shared by every producer of other providers' chat history — the one-time
//! `chat_import` of an export file and the live syncs (`grok_chats_sync`) — so
//! the row shape and the dedup key live in one place. A message's identity is
//! [`stream_id`] over the provider's own conversation and message ids, so an
//! export imported after a live sync (or the reverse) adds only what the other
//! did not already write.
//!
//! Insert-or-ignore: a message, once written, is never rewritten. Chat sites
//! keep a message's text fixed once it is sent; an edit or a retry makes a new
//! message with a new id.

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::dedup::{build_batch_insert_query, BATCH_SIZE};

/// One normalized conversation message, ready for insert.
#[derive(Debug, Clone)]
pub struct ConversationMsg {
    pub stream_id: String,
    pub conversation_id: String,
    pub message_id: String,
    /// `user` | `assistant` | `system` — the table's CHECK.
    pub role: String,
    pub content: String,
    /// The chat site: `claude`, `chatgpt`, `gemini`, `grok`.
    pub provider: String,
    /// When the message was sent, from the provider. Never the wall clock for
    /// a live sync: a guessed time would land the message on the wrong day.
    pub timestamp: DateTime<Utc>,
    /// The applet that wrote the row (`chat_import`, `grok_chats`).
    pub source_table: String,
}

/// The dedup key for a message: `chatimport:{provider}:{conversation}:{message}`.
/// The `chatimport` prefix predates the live syncs and stays, because every
/// imported row already carries it.
pub fn stream_id(provider: &str, conversation_id: &str, message_id: &str) -> String {
    format!("chatimport:{provider}:{conversation_id}:{message_id}")
}

/// Insert messages in batches, skipping any whose `stream_id` is already
/// stored. Returns the number of new rows.
pub async fn write_messages(pool: &PgPool, messages: &[ConversationMsg]) -> Result<usize> {
    let mut written = 0usize;
    for chunk in messages.chunks(BATCH_SIZE) {
        written += flush(pool, chunk).await?;
    }
    Ok(written)
}

async fn flush(pool: &PgPool, records: &[ConversationMsg]) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }
    let sql = build_batch_insert_query(
        "data_content_conversation",
        &[
            "id",
            "conversation_id",
            "message_id",
            "role",
            "content",
            "provider",
            "occurred_at",
            "source_stream_id",
            "source_table",
            "source_provider",
        ],
        "source_stream_id",
        records.len(),
    );

    let mut q = sqlx::query(&sql);
    for r in records {
        q = q
            .bind(&r.stream_id) // id (the deterministic stream id doubles as the PK)
            .bind(&r.conversation_id)
            .bind(&r.message_id)
            .bind(&r.role)
            .bind(&r.content)
            .bind(&r.provider)
            .bind(r.timestamp)
            .bind(&r.stream_id)
            .bind(&r.source_table)
            .bind(&r.provider);
    }
    let result = q.execute(pool).await?;
    Ok(result.rows_affected() as usize)
}

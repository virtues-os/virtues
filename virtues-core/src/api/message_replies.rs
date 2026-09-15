//! Drafted replies to message threads (`app_message_replies`).
//!
//! The owner asks for a draft; the `message_reply` applet reads that thread
//! and writes one row here. The device that asked opens it, shows the draft,
//! and reports what happened: sent (with the text that actually went out,
//! which may be an edit), or dismissed. Nothing writes a row here unless it
//! was asked for.
//!
//! This module is the device-facing half only. Drafting lives in the applet;
//! this never calls a model.

use crate::error::{Error, Result};
use crate::types::Timestamp;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MessageReply {
    pub id: String,
    /// The chat GUID from chat.db — also what macOS Messages accepts as a
    /// `chat id` when told to send, so the row is sufficient to act on.
    pub thread_id: String,
    pub ask_stream_id: String,
    pub ask_text: String,
    pub from_handle: String,
    pub from_name: Option<String>,
    pub is_group: bool,
    pub draft: String,
    pub rationale: Option<String>,
    pub status: String,
    pub sent_text: Option<String>,
    pub sent_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

const COLUMNS: &str = "id, thread_id, ask_stream_id, ask_text, from_handle, from_name, is_group, \
                       draft, rationale, status, sent_text, sent_at, created_at, updated_at";

/// Pending drafts, newest first. A draft the owner never acted on is not
/// pending forever: after a day the ask is stale and answering it late would
/// be worse than not at all, so it is expired here on read rather than by a
/// separate sweeper.
pub async fn list_pending_message_replies(db: &PgPool) -> Result<Vec<MessageReply>> {
    sqlx::query(
        "UPDATE app_message_replies SET status = 'expired', updated_at = now() \
         WHERE status = 'pending' AND created_at < now() - interval '24 hours'",
    )
    .execute(db)
    .await?;

    let rows = sqlx::query_as::<_, MessageReply>(&format!(
        "SELECT {COLUMNS} FROM app_message_replies \
         WHERE status = 'pending' ORDER BY created_at DESC"
    ))
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn get_message_reply(db: &PgPool, id: &str) -> Result<MessageReply> {
    sqlx::query_as::<_, MessageReply>(&format!(
        "SELECT {COLUMNS} FROM app_message_replies WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| Error::NotFound(format!("message reply {id}")))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkSentRequest {
    /// What actually went out. Differs from `draft` when the owner edited it.
    pub sent_text: String,
}

/// The device sent it. Only a pending row can be marked sent; anything else
/// is a stale device acting on a row the owner already resolved elsewhere.
pub async fn mark_message_reply_sent(
    db: &PgPool,
    id: &str,
    req: MarkSentRequest,
) -> Result<MessageReply> {
    let text = req.sent_text.trim();
    if text.is_empty() {
        return Err(Error::InvalidInput("sent_text is empty".into()));
    }
    let updated = sqlx::query(
        "UPDATE app_message_replies \
         SET status = 'sent', sent_text = $2, sent_at = now(), updated_at = now() \
         WHERE id = $1 AND status = 'pending'",
    )
    .bind(id)
    .bind(text)
    .execute(db)
    .await?
    .rows_affected();
    if updated == 0 {
        // Either unknown or already resolved — tell the caller which.
        let existing = get_message_reply(db, id).await?;
        return Err(Error::InvalidInput(format!(
            "message reply {id} is {}, not pending",
            existing.status
        )));
    }
    get_message_reply(db, id).await
}

pub async fn dismiss_message_reply(db: &PgPool, id: &str) -> Result<MessageReply> {
    sqlx::query(
        "UPDATE app_message_replies SET status = 'dismissed', updated_at = now() \
         WHERE id = $1 AND status = 'pending'",
    )
    .bind(id)
    .execute(db)
    .await?;
    get_message_reply(db, id).await
}

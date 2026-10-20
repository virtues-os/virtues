//! Messages the assistant sends as the owner, from the moment it decides to
//! until chat.db says whether they arrived.
//!
//! `send_imessage` does not send. It writes a row here with a `send_at`: five
//! seconds out by default, which is the owner's window to cancel from the
//! chat, or the time they asked for. This worker sends each row when it is
//! due, through the Mac app (`browser::call`), and then watches the owner's
//! own messages as the collector syncs them back, to settle the row as
//! delivered or failed.
//!
//! States: `pending` (waiting to go, cancellable) → `sending` → `sent` (handed
//! to Messages) → `delivered`, or `failed`, or `cancelled` from `pending`.
//! The card in the chat reads them through `GET /api/messages/outbox/:id`.

use std::sync::OnceLock;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use sqlx::PgPool;
use tokio::sync::Notify;

/// How long a send waits before it goes, so the owner can cancel it.
pub const UNDO_WINDOW: Duration = Duration::from_secs(5);

/// A send whose Mac is away this long past its time fails instead of going
/// late. "Happy birthday" at 3am the next day is worse than nothing.
const LATE_LIMIT: chrono::Duration = chrono::Duration::minutes(10);

/// How long a sent message is watched for its delivery. Past this the card
/// keeps saying "Sent".
const DELIVERY_WATCH: chrono::Duration = chrono::Duration::hours(1);

/// The longest the worker sleeps with nothing due: the delivery check's pace.
const IDLE: Duration = Duration::from_secs(30);

/// Messages may be cold: the app's script launches it and retries the thread
/// for about four seconds before it falls back to the handle.
const SEND_WAIT: Duration = Duration::from_secs(30);

fn wake() -> &'static Notify {
    static WAKE: OnceLock<Notify> = OnceLock::new();
    WAKE.get_or_init(Notify::new)
}

/// One outbox row, as the card reads it.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Outgoing {
    pub id: String,
    pub person_id: Option<String>,
    pub recipient: String,
    pub handle: String,
    pub thread_id: Option<String>,
    pub body: String,
    pub send_at: DateTime<Utc>,
    pub state: String,
    pub error: Option<String>,
    pub sent_at: Option<DateTime<Utc>>,
    pub delivered_at: Option<DateTime<Utc>>,
}

const COLUMNS: &str =
    "id, person_id, recipient, handle, thread_id, body, send_at, state, error, sent_at, delivered_at";

pub struct NewMessage<'a> {
    pub chat_id: Option<&'a str>,
    pub person_id: Option<&'a str>,
    pub recipient: &'a str,
    pub handle: &'a str,
    pub thread_id: Option<&'a str>,
    pub body: &'a str,
    pub send_at: DateTime<Utc>,
}

/// Queue a message, and wake the worker so it learns the new due time.
pub async fn enqueue(pool: &PgPool, m: NewMessage<'_>) -> sqlx::Result<Outgoing> {
    let id = crate::ids::generate_id("outbox", &[&uuid::Uuid::new_v4().to_string()]);
    let row = sqlx::query_as::<_, Outgoing>(&format!(
        "INSERT INTO app_message_outbox (id, chat_id, person_id, recipient, handle, thread_id, body, send_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING {COLUMNS}"
    ))
    .bind(&id)
    .bind(m.chat_id)
    .bind(m.person_id)
    .bind(m.recipient)
    .bind(m.handle)
    .bind(m.thread_id)
    .bind(m.body)
    .bind(m.send_at)
    .fetch_one(pool)
    .await?;
    wake().notify_one();
    Ok(row)
}

pub async fn get(pool: &PgPool, id: &str) -> sqlx::Result<Option<Outgoing>> {
    sqlx::query_as::<_, Outgoing>(&format!("SELECT {COLUMNS} FROM app_message_outbox WHERE id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// Cancel a message that has not gone yet. `None` when there is no such
/// message; otherwise the row, whose state says whether the cancel landed (a
/// message already sending cannot be called back).
pub async fn cancel(pool: &PgPool, id: &str) -> sqlx::Result<Option<Outgoing>> {
    sqlx::query("UPDATE app_message_outbox SET state = 'cancelled', updated_at = now() WHERE id = $1 AND state = 'pending'")
        .bind(id)
        .execute(pool)
        .await?;
    get(pool, id).await
}

pub fn spawn(pool: PgPool) {
    tokio::spawn(async move {
        // A row left `sending` was mid-send when the box stopped, and nothing
        // can say whether Messages took it. Say so; the delivery check below
        // can still find it in chat.db and settle it.
        if let Err(e) = sqlx::query(
            "UPDATE app_message_outbox
                SET state = 'failed', updated_at = now(),
                    error = 'Your server restarted while sending this. Check Messages to see whether it went.'
              WHERE state = 'sending'",
        )
        .execute(&pool)
        .await
        {
            tracing::error!("outbox: could not settle interrupted sends: {e}");
        }
        loop {
            if let Err(e) = send_due(&pool).await {
                tracing::error!("outbox: sending failed: {e}");
            }
            if let Err(e) = settle_delivered(&pool).await {
                tracing::error!("outbox: delivery check failed: {e}");
            }
            let pause = match next_due(&pool).await {
                Ok(Some(at)) => (at - Utc::now()).to_std().unwrap_or(Duration::ZERO).min(IDLE),
                Ok(None) => IDLE,
                Err(e) => {
                    tracing::error!("outbox: could not read the next send: {e}");
                    IDLE
                }
            };
            let _ = tokio::time::timeout(pause, wake().notified()).await;
        }
    });
}

async fn next_due(pool: &PgPool) -> sqlx::Result<Option<DateTime<Utc>>> {
    sqlx::query_scalar("SELECT min(send_at) FROM app_message_outbox WHERE state = 'pending'")
        .fetch_one(pool)
        .await
}

/// Send everything due, one at a time and oldest first.
async fn send_due(pool: &PgPool) -> sqlx::Result<()> {
    loop {
        if !crate::browser::connected() {
            // Nothing can go. Leave each row pending until it is too late.
            let late = Utc::now() - LATE_LIMIT;
            sqlx::query(
                "UPDATE app_message_outbox
                    SET state = 'failed', updated_at = now(),
                        error = 'Your Mac wasn''t connected when this was due, so it wasn''t sent.'
                  WHERE state = 'pending' AND send_at < $1",
            )
            .bind(late)
            .execute(pool)
            .await?;
            return Ok(());
        }
        // Claim one: the UPDATE is the lock, so a cancel that lands first wins.
        let Some(m) = sqlx::query_as::<_, Outgoing>(&format!(
            "UPDATE app_message_outbox SET state = 'sending', updated_at = now()
              WHERE id = (SELECT id FROM app_message_outbox
                           WHERE state = 'pending' AND send_at <= now()
                           ORDER BY send_at LIMIT 1
                           FOR UPDATE SKIP LOCKED)
              RETURNING {COLUMNS}"
        ))
        .fetch_optional(pool)
        .await?
        else {
            return Ok(());
        };

        // absent-ok: no thread means a first message, sent to the handle.
        let args = json!({ "chat_id": m.thread_id.clone().unwrap_or_default(), "handle": m.handle, "text": m.body });
        let outcome = crate::browser::call("imessage_send", args, SEND_WAIT).await;
        match outcome {
            Ok(_) => {
                sqlx::query("UPDATE app_message_outbox SET state = 'sent', sent_at = now(), updated_at = now() WHERE id = $1")
                    .bind(&m.id)
                    .execute(pool)
                    .await?
            }
            Err(why) => {
                tracing::warn!(id = %m.id, "outbox: Messages refused a send: {why}");
                sqlx::query("UPDATE app_message_outbox SET state = 'failed', error = $2, updated_at = now() WHERE id = $1")
                    .bind(&m.id)
                    .bind(why)
                    .execute(pool)
                    .await?
            }
        };
    }
}

/// Match each sent message to the owner's own message in chat.db, once the
/// collector has synced it, and settle it by what chat.db says.
///
/// The match is the thread (or, for a first message, a thread ending in the
/// handle), the exact text, and a time after the send. A failed send that
/// shows up anyway is settled too: chat.db is the better witness.
async fn settle_delivered(pool: &PgPool) -> sqlx::Result<()> {
    let since = Utc::now() - DELIVERY_WATCH;
    let rows: Vec<(String, String, Option<bool>, Option<DateTime<Utc>>, Option<i64>)> = sqlx::query_as(
        "SELECT DISTINCT ON (o.id)
                o.id, m.id,
                (m.metadata->>'is_delivered')::boolean,
                (m.metadata->>'date_delivered')::timestamptz,
                (m.metadata->>'send_error')::bigint
           FROM app_message_outbox o
           JOIN data_communication_message m
             ON (m.thread_id = o.thread_id
                 OR (o.thread_id IS NULL AND m.thread_id LIKE '%;' || o.handle))
            AND m.body = o.body
            AND (m.metadata->>'is_from_me')::boolean
            AND m.occurred_at >= COALESCE(o.sent_at, o.send_at) - interval '2 minutes'
            AND m.occurred_at >= $1
          WHERE o.state IN ('sent', 'failed')
            AND o.send_at >= $1
            AND (o.state = 'sent' OR o.message_id IS NULL)
          ORDER BY o.id, m.occurred_at",
    )
    .bind(since)
    .fetch_all(pool)
    .await?;

    for (outbox_id, message_id, delivered, delivered_at, send_error) in rows {
        // chat.db writes 0 for "not yet", which reads as 2001-01-01.
        let delivered_at = delivered_at.filter(|at| at.timestamp() > 978_393_600);
        // absent-ok: chat.db writes no error for a message that went.
        let (state, error): (&str, Option<&str>) = if send_error.unwrap_or(0) != 0 {
            ("failed", Some("Messages couldn't deliver this. Open it in Messages to try again."))
        } else if delivered == Some(true) {
            ("delivered", None)
        } else {
            ("sent", None)
        };
        sqlx::query(
            "UPDATE app_message_outbox
                SET state = $2, error = COALESCE($3, CASE WHEN $2 = 'failed' THEN error END),
                    message_id = $4,
                    delivered_at = CASE WHEN $2 = 'delivered' THEN COALESCE($5, now()) END,
                    sent_at = COALESCE(sent_at, now()),
                    updated_at = now()
              WHERE id = $1",
        )
        .bind(&outbox_id)
        .bind(state)
        .bind(error)
        .bind(&message_id)
        .bind(delivered_at)
        .execute(pool)
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message<'a>(send_at: DateTime<Utc>) -> NewMessage<'a> {
        NewMessage {
            chat_id: None,
            person_id: None,
            recipient: "Nick Example",
            handle: "+15125550101",
            thread_id: Some("any;-;+15125550101"),
            body: "Running ten minutes late",
            send_at,
        }
    }

    #[sqlx::test]
    async fn a_cancel_lands_only_before_the_send(pool: PgPool) {
        let m = enqueue(&pool, message(Utc::now() + chrono::Duration::seconds(5))).await.unwrap();
        assert_eq!(m.state, "pending");
        assert_eq!(cancel(&pool, &m.id).await.unwrap().unwrap().state, "cancelled");

        let n = enqueue(&pool, message(Utc::now())).await.unwrap();
        sqlx::query("UPDATE app_message_outbox SET state = 'sent' WHERE id = $1")
            .bind(&n.id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(cancel(&pool, &n.id).await.unwrap().unwrap().state, "sent");
        assert!(cancel(&pool, "outbox_none").await.unwrap().is_none());
    }

    #[sqlx::test]
    async fn the_owners_own_message_settles_the_send(pool: PgPool) {
        let m = enqueue(&pool, message(Utc::now() - chrono::Duration::seconds(30))).await.unwrap();
        sqlx::query("UPDATE app_message_outbox SET state = 'sent', sent_at = now() - interval '20 seconds' WHERE id = $1")
            .bind(&m.id)
            .execute(&pool)
            .await
            .unwrap();
        // Not synced back yet: still sent.
        settle_delivered(&pool).await.unwrap();
        assert_eq!(get(&pool, &m.id).await.unwrap().unwrap().state, "sent");

        sqlx::query(
            "INSERT INTO data_communication_message
               (id, message_id, thread_id, channel, body, from_identifier, from_handle,
                occurred_at, source_stream_id, source_table, source_provider, metadata)
             VALUES ('m1', 'm1', 'any;-;+15125550101', 'imessage', 'Running ten minutes late', 'me', '',
                     now() - interval '10 seconds', 'm1', 'messages', 'mac',
                     '{\"is_from_me\": true, \"is_delivered\": true}')",
        )
        .execute(&pool)
        .await
        .unwrap();
        settle_delivered(&pool).await.unwrap();
        let settled = get(&pool, &m.id).await.unwrap().unwrap();
        assert_eq!(settled.state, "delivered");
        assert!(settled.delivered_at.is_some());
    }

    #[sqlx::test]
    async fn a_message_chat_db_could_not_deliver_fails(pool: PgPool) {
        let m = enqueue(&pool, message(Utc::now() - chrono::Duration::seconds(30))).await.unwrap();
        sqlx::query("UPDATE app_message_outbox SET state = 'sent', sent_at = now() - interval '20 seconds' WHERE id = $1")
            .bind(&m.id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO data_communication_message
               (id, message_id, thread_id, channel, body, from_identifier, from_handle,
                occurred_at, source_stream_id, source_table, source_provider, metadata)
             VALUES ('m2', 'm2', 'any;-;+15125550101', 'imessage', 'Running ten minutes late', 'me', '',
                     now() - interval '10 seconds', 'm2', 'messages', 'mac',
                     '{\"is_from_me\": true, \"is_delivered\": false, \"send_error\": 22}')",
        )
        .execute(&pool)
        .await
        .unwrap();
        settle_delivered(&pool).await.unwrap();
        let settled = get(&pool, &m.id).await.unwrap().unwrap();
        assert_eq!(settled.state, "failed");
        assert!(settled.error.unwrap().contains("couldn't deliver"));
    }
}

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

/// The op the Mac app sends a message with. An app names it when it connects
/// if it can (`browser::supports`).
pub const SEND_OP: &str = "imessage_send";

/// Why a due send failed on an app that cannot send.
const APP_TOO_OLD: &str = "Your Mac app needs an update to send messages.";

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
    /// `[{ref, filename, mime_type, size_bytes}]`, as [`attachment`] made them.
    pub attachments: serde_json::Value,
}

const COLUMNS: &str =
    "id, person_id, recipient, handle, thread_id, body, send_at, state, error, sent_at, delivered_at, attachments";

/// The most a message can carry in files, all told. They cross to the Mac as
/// base64 in one websocket frame.
pub const MAX_ATTACHMENT_BYTES: usize = 25 * 1024 * 1024;

pub struct NewMessage<'a> {
    pub chat_id: Option<&'a str>,
    pub person_id: Option<&'a str>,
    pub recipient: &'a str,
    pub handle: &'a str,
    pub thread_id: Option<&'a str>,
    pub body: &'a str,
    pub send_at: DateTime<Utc>,
    pub attachments: serde_json::Value,
}

/// Queue a message, and wake the worker so it learns the new due time.
pub async fn enqueue(pool: &PgPool, m: NewMessage<'_>) -> sqlx::Result<Outgoing> {
    let id = crate::ids::generate_id("outbox", &[&uuid::Uuid::new_v4().to_string()]);
    let row = sqlx::query_as::<_, Outgoing>(&format!(
        "INSERT INTO app_message_outbox (id, chat_id, person_id, recipient, handle, thread_id, body, send_at, attachments)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
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
    .bind(&m.attachments)
    .fetch_one(pool)
    .await?;
    wake().notify_one();
    Ok(row)
}

/// A file the owner has, by the reference the model passes: a drive file
/// (`/drive/<id>` or the bare id) or an iMessage attachment in the record
/// (`/api/messages/<id>/attachments/<n>`). `Err` is a sentence for the model.
pub async fn fetch_attachment(pool: &PgPool, raw: &str) -> Result<(String, String, Vec<u8>), String> {
    let raw = raw.trim();
    let path = raw.strip_prefix("/api").unwrap_or(raw);
    let storage = crate::storage::Storage::file(crate::storage::lake::lake_root().to_string_lossy().into_owned())
        .map_err(|e| format!("Your server's storage is unavailable: {e}"))?;

    if let Some(rest) = path.strip_prefix("/messages/") {
        let (message_id, index) = rest
            .split_once("/attachments/")
            .and_then(|(id, n)| Some((id, n.parse::<usize>().ok()?)))
            .ok_or_else(|| format!("{raw} is not a message attachment. Use /api/messages/<id>/attachments/<n>."))?;
        let (bytes, mime) = crate::storage::message_media::load_attachment(pool, &storage, message_id, index)
            .await
            .map_err(|e| format!("Could not read {raw}: {e}"))?;
        let filename = message_attachment_name(pool, message_id, index, &mime).await;
        return Ok((filename, mime, bytes));
    }

    let id = crate::api::refs::split_ref(path).map(|(_, id)| id).unwrap_or(path);
    let config = crate::api::DriveConfig::new(std::sync::Arc::new(storage));
    let (file, bytes) = crate::api::drive::download_file(pool, &config, id)
        .await
        .map_err(|e| format!("Could not read {raw}: {e}"))?;
    let mime = file.mime_type.clone().unwrap_or_else(|| "application/octet-stream".into());
    Ok((file.filename, mime, bytes))
}

/// The name chat.db gave the file, or one made from its type.
async fn message_attachment_name(pool: &PgPool, message_id: &str, index: usize, mime: &str) -> String {
    let named: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT metadata->'attachments'->$2::int->>'filename' FROM data_communication_message WHERE id = $1",
    )
    .bind(message_id)
    .bind(index as i32)
    .fetch_optional(pool)
    .await
    // absent-ok: a missing name only costs the file its original name.
    .unwrap_or(None);
    named
        .and_then(|(n,)| n)
        .and_then(|n| std::path::Path::new(&n).file_name().and_then(|f| f.to_str()).map(str::to_string))
        .unwrap_or_else(|| format!("attachment.{}", mime.rsplit('/').next().unwrap_or("bin")))
}

/// Read and check one file, for the outbox row: everything but the bytes.
pub async fn attachment(pool: &PgPool, raw: &str) -> Result<serde_json::Value, String> {
    let (filename, mime, bytes) = fetch_attachment(pool, raw).await?;
    Ok(json!({ "ref": raw.trim(), "filename": filename, "mime_type": mime, "size_bytes": bytes.len() }))
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
        if !crate::browser::supports(SEND_OP) {
            // An older app refuses the op as unknown. Don't hand it one.
            fail_due_unsendable(pool).await?;
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

        let files = match files_for(pool, &m.attachments).await {
            Ok(f) => f,
            Err(why) => {
                sqlx::query("UPDATE app_message_outbox SET state = 'failed', error = $2, updated_at = now() WHERE id = $1")
                    .bind(&m.id)
                    .bind(why)
                    .execute(pool)
                    .await?;
                continue;
            }
        };
        // absent-ok: no thread means a first message, sent to the handle.
        let args = json!({ "chat_id": m.thread_id.clone().unwrap_or_default(), "handle": m.handle, "text": m.body, "files": files });
        // Each file is its own hand-off to Messages.
        let wait = SEND_WAIT + Duration::from_secs(15) * m.attachments.as_array().map_or(0, |a| a.len() as u32);
        let outcome = crate::browser::call(SEND_OP, args, wait).await;
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

/// Fail every due send: the connected app is too old to send any of them.
async fn fail_due_unsendable(pool: &PgPool) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE app_message_outbox SET state = 'failed', error = $1, updated_at = now()
          WHERE state = 'pending' AND send_at <= now()",
    )
    .bind(APP_TOO_OLD)
    .execute(pool)
    .await?;
    Ok(())
}

/// The files to send, read now: `[{filename, data_base64}]`.
async fn files_for(pool: &PgPool, attachments: &serde_json::Value) -> Result<Vec<serde_json::Value>, String> {
    use base64::Engine;
    let mut files = Vec::new();
    let mut total = 0;
    for a in attachments.as_array().into_iter().flatten() {
        let raw = a.get("ref").and_then(|r| r.as_str()).unwrap_or("");
        let (filename, _, bytes) =
            fetch_attachment(pool, raw).await.map_err(|why| format!("{why} Nothing was sent."))?;
        total += bytes.len();
        if total > MAX_ATTACHMENT_BYTES {
            return Err("The files are larger than 25 MB together. Nothing was sent.".into());
        }
        files.push(json!({ "filename": filename, "data_base64": base64::engine::general_purpose::STANDARD.encode(bytes) }));
    }
    Ok(files)
}

/// Match each sent message to the owner's own messages in chat.db, once the
/// collector has synced them, and settle it by what chat.db says.
///
/// Messages makes one message of the text and one of each file. The text is
/// matched by thread (or, for a first message, a thread ending in the
/// handle), exact words, and a time after the send; the files are the
/// owner's next messages in that thread that carry one. The send is
/// delivered only when every part is, and failed when any part is: the text
/// arriving says nothing about the photo after it. A failed send that shows
/// up anyway is settled too: chat.db is the better witness.
async fn settle_delivered(pool: &PgPool) -> sqlx::Result<()> {
    type Row = (
        String,
        bool,
        Option<String>,
        Option<bool>,
        Option<DateTime<Utc>>,
        Option<i64>,
        i32,
        i64,
        i64,
        Option<String>,
        Option<DateTime<Utc>>,
    );
    let since = Utc::now() - DELIVERY_WATCH;
    let rows: Vec<Row> = sqlx::query_as(
        "WITH open AS (
             SELECT * FROM app_message_outbox
              WHERE state IN ('sent', 'failed') AND send_at >= $1
                AND (state = 'sent' OR message_id IS NULL)
         ), mine AS (
             SELECT o.id AS oid, m.id AS mid, m.body, m.has_attachments, m.occurred_at,
                    COALESCE((m.metadata->>'is_delivered')::boolean, false) AS delivered,
                    (m.metadata->>'date_delivered')::timestamptz AS delivered_at,
                    COALESCE((m.metadata->>'send_error')::bigint, 0) AS err
               FROM open o
               JOIN data_communication_message m
                 ON (m.thread_id = o.thread_id
                     OR (o.thread_id IS NULL AND m.thread_id LIKE '%;' || o.handle))
                AND (m.metadata->>'is_from_me')::boolean
                AND m.occurred_at >= COALESCE(o.sent_at, o.send_at) - interval '2 minutes'
                AND m.occurred_at >= $1
         )
         SELECT o.id, o.body <> '', t.mid, t.delivered, t.delivered_at, t.err,
                jsonb_array_length(o.attachments),
                COALESCE(f.delivered_n, 0), COALESCE(f.failed_n, 0), f.first_mid, f.last_delivered_at
           FROM open o
           LEFT JOIN LATERAL (
                SELECT * FROM mine
                 WHERE mine.oid = o.id AND o.body <> '' AND mine.body = o.body
                 ORDER BY occurred_at LIMIT 1
           ) t ON true
           LEFT JOIN LATERAL (
                SELECT count(*) FILTER (WHERE delivered) AS delivered_n,
                       count(*) FILTER (WHERE err <> 0) AS failed_n,
                       min(mid) AS first_mid,
                       max(delivered_at) AS last_delivered_at
                  FROM (SELECT * FROM mine
                         WHERE mine.oid = o.id AND mine.has_attachments
                         ORDER BY occurred_at
                         LIMIT jsonb_array_length(o.attachments)) x
           ) f ON true
          WHERE t.mid IS NOT NULL OR f.first_mid IS NOT NULL",
    )
    .bind(since)
    .fetch_all(pool)
    .await?;

    for (outbox_id, has_text, text_id, text_delivered, text_at, text_err, files, files_delivered, files_failed, file_id, file_at) in rows {
        // absent-ok: chat.db writes no error for a message that went.
        let (state, error): (&str, Option<&str>) = if text_err.unwrap_or(0) != 0 {
            ("failed", Some("Messages couldn't deliver this. Open it in Messages to try again."))
        } else if files_failed > 0 {
            ("failed", Some("Messages couldn't deliver the attachment. Open it in Messages to try again."))
        } else if (!has_text || text_delivered == Some(true)) && files_delivered >= i64::from(files) {
            ("delivered", None)
        } else {
            ("sent", None)
        };
        // chat.db writes 0 for "not yet", which reads as 2001-01-01.
        let delivered_at = [text_at, file_at]
            .into_iter()
            .flatten()
            .filter(|at| at.timestamp() > 978_393_600)
            .max();
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
        .bind(text_id.or(file_id))
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
            attachments: json!([]),
        }
    }

    #[sqlx::test]
    async fn an_app_that_cannot_send_fails_only_what_is_due(pool: PgPool) {
        let due = enqueue(&pool, message(Utc::now() - chrono::Duration::seconds(1))).await.unwrap();
        let later = enqueue(&pool, message(Utc::now() + chrono::Duration::minutes(5))).await.unwrap();
        fail_due_unsendable(&pool).await.unwrap();

        let due = get(&pool, &due.id).await.unwrap().unwrap();
        assert_eq!(due.state, "failed");
        assert_eq!(due.error.as_deref(), Some(APP_TOO_OLD));
        assert_eq!(get(&pool, &later.id).await.unwrap().unwrap().state, "pending");
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
    async fn a_message_of_files_alone_settles_on_its_attachment(pool: PgPool) {
        let mut only_files = message(Utc::now() - chrono::Duration::seconds(30));
        only_files.body = "";
        only_files.attachments = json!([{ "ref": "/drive/dr_x", "filename": "a.png", "mime_type": "image/png", "size_bytes": 1 }]);
        let m = enqueue(&pool, only_files).await.unwrap();
        sqlx::query("UPDATE app_message_outbox SET state = 'sent', sent_at = now() - interval '20 seconds' WHERE id = $1")
            .bind(&m.id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO data_communication_message
               (id, message_id, thread_id, channel, body, from_identifier, from_handle, has_attachments,
                occurred_at, source_stream_id, source_table, source_provider, metadata)
             VALUES ('m3', 'm3', 'any;-;+15125550101', 'imessage', '[Photo]', 'me', '', true,
                     now() - interval '10 seconds', 'm3', 'messages', 'mac',
                     '{\"is_from_me\": true, \"is_delivered\": true}')",
        )
        .execute(&pool)
        .await
        .unwrap();
        settle_delivered(&pool).await.unwrap();
        assert_eq!(get(&pool, &m.id).await.unwrap().unwrap().state, "delivered");
    }

    #[sqlx::test]
    async fn the_text_arriving_says_nothing_about_the_photo(pool: PgPool) {
        let mut with_photo = message(Utc::now() - chrono::Duration::seconds(30));
        with_photo.attachments = json!([{ "ref": "/drive/dr_x", "filename": "a.png", "mime_type": "image/png", "size_bytes": 1 }]);
        let m = enqueue(&pool, with_photo).await.unwrap();
        sqlx::query("UPDATE app_message_outbox SET state = 'sent', sent_at = now() - interval '20 seconds' WHERE id = $1")
            .bind(&m.id)
            .execute(&pool)
            .await
            .unwrap();
        let own = |id: &str, body: &str, files: bool, meta: &str| {
            format!(
                "INSERT INTO data_communication_message
                   (id, message_id, thread_id, channel, body, from_identifier, from_handle, has_attachments,
                    occurred_at, source_stream_id, source_table, source_provider, metadata)
                 VALUES ('{id}', '{id}', 'any;-;+15125550101', 'imessage', '{body}', 'me', '', {files},
                         now() - interval '10 seconds', '{id}', 'messages', 'mac', '{meta}')"
            )
        };
        sqlx::query(&own("t1", "Running ten minutes late", false, r#"{"is_from_me": true, "is_delivered": true}"#))
            .execute(&pool)
            .await
            .unwrap();
        // The text alone: still waiting on the photo.
        settle_delivered(&pool).await.unwrap();
        assert_eq!(get(&pool, &m.id).await.unwrap().unwrap().state, "sent");

        sqlx::query(&own("f1", "[Photo]", true, r#"{"is_from_me": true, "is_delivered": false, "send_error": 25}"#))
            .execute(&pool)
            .await
            .unwrap();
        settle_delivered(&pool).await.unwrap();
        let settled = get(&pool, &m.id).await.unwrap().unwrap();
        assert_eq!(settled.state, "failed");
        assert!(settled.error.unwrap().contains("attachment"));
    }

    #[sqlx::test]
    async fn a_reference_to_nothing_is_refused_before_anything_queues(pool: PgPool) {
        assert!(attachment(&pool, "/drive/dr_none").await.is_err());
        assert!(attachment(&pool, "/api/messages/none/attachments/0").await.is_err());
        assert!(attachment(&pool, "/api/messages/none/attachments/x").await.is_err());
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

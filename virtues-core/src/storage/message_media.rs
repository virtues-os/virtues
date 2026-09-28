//! Message attachments: the bytes a message carried, kept in the lake and linked
//! from the message row.
//!
//! The Mac collector sends an iMessage/SMS attachment only after the message it
//! belongs to has reached the box, so the message row is the anchor: bytes land
//! as a `kind='media'` lake object (the microphone stream's convention, see
//! [`super::lake::put_media`]) and the row records where under
//! `metadata.attachment_media`, keyed by the attachment's chat.db GUID:
//!
//! ```json
//! "attachment_media": {
//!   "<attachment guid>": {"key": "media/mac/imessage_attachments/<guid>.jpg",
//!                         "mime_type": "image/jpeg", "size_bytes": 412345,
//!                         "index": 0}
//! }
//! ```
//!
//! # Why a key of its own, not `metadata.attachments[i]`
//!
//! The message upsert merges metadata SHALLOWLY (`metadata || EXCLUDED.metadata`,
//! see `virtues_helpers::dedup`). `attachments` is a key the collector sends on
//! every re-read of the message, so a reference written inside that array would
//! be erased the next time the same message was upserted. `attachment_media` is
//! a key the transform never writes, so the merge keeps it.
//!
//! # Deletion
//!
//! When the collector reports that a message is gone from chat.db,
//! [`purge_deleted_messages`] stamps `deleted_at_source` and removes the stored
//! bytes and their lake rows. The row stays as a tombstone, like every other
//! `deleted_at_source` in the system; the picture does not.

use serde_json::{json, Value};
use sqlx::PgPool;

use super::Storage;
use crate::{Error, Result};

/// The lake stream the bytes live under: `media/mac/imessage_attachments/…`.
pub const STREAM: &str = "imessage_attachments";
const PROVIDER: &str = "mac";

/// The message table's own name for Mac iMessage rows (`source_table`).
const SOURCE_TABLE: &str = "mac_imessage";

/// Largest attachment the box will keep. The collector applies the same cap
/// before sending; this is the backstop for anything that did not.
pub const MAX_ATTACHMENT_BYTES: usize = 10 * 1024 * 1024;

/// Whether a declared type is one the box keeps. Video and audio are refused:
/// they are the bulk of any Messages library and nothing reads them yet.
pub fn is_keepable_mime(mime: &str) -> bool {
    let m = mime.trim().to_ascii_lowercase();
    !(m.starts_with("video/") || m.starts_with("audio/"))
}

/// A filename for the lake key, built from the attachment GUID.
///
/// The GUID arrives in a device's JSON payload, so it is not trusted: anything
/// outside `[A-Za-z0-9_-]` becomes `_`. (`Storage` also refuses keys that
/// escape the lake; this keeps the key legible as well as safe.) Returns `None`
/// for a GUID with nothing usable in it.
pub fn media_filename(attachment_guid: &str, mime: &str) -> Option<String> {
    let stem: String = attachment_guid
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if stem.trim_matches('_').is_empty() {
        return None;
    }
    Some(format!("{stem}.{}", extension_for(mime)))
}

/// File extension for a stored attachment, from its (post-transcode) type.
pub fn extension_for(mime: &str) -> &'static str {
    match mime.trim().to_ascii_lowercase().as_str() {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/heic" | "image/heif" => "heic",
        "image/tiff" => "tiff",
        "application/pdf" => "pdf",
        "text/plain" => "txt",
        _ => "bin",
    }
}

/// The stored reference for attachment `index` of a message, given the row's
/// `metadata`. `attachments[index].guid` names it; a ref whose own `index`
/// matches is the fallback for an attachment that arrived without a GUID.
pub fn media_ref_for_index(metadata: &Value, index: usize) -> Option<&Value> {
    let media = metadata.get("attachment_media")?.as_object()?;
    if let Some(guid) = metadata
        .get("attachments")
        .and_then(|a| a.get(index))
        .and_then(|a| a.get("guid"))
        .and_then(Value::as_str)
    {
        if let Some(r) = media.get(guid) {
            return Some(r);
        }
    }
    media
        .values()
        .find(|r| r.get("index").and_then(Value::as_u64) == Some(index as u64))
}

/// Every lake key a message's metadata points at.
pub fn media_keys(metadata: &Value) -> Vec<String> {
    metadata
        .get("attachment_media")
        .and_then(Value::as_object)
        .map(|m| {
            m.values()
                .filter_map(|r| r.get("key").and_then(Value::as_str).map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Is there a live (not deleted-at-source) Mac message with this GUID?
pub async fn message_is_live(pool: &PgPool, message_guid: &str) -> Result<bool> {
    let row: Option<(bool,)> = sqlx::query_as(
        "SELECT deleted_at_source IS NULL FROM data_communication_message
          WHERE source_stream_id = $1 AND source_table = $2",
    )
    .bind(message_guid)
    .bind(SOURCE_TABLE)
    .fetch_optional(pool)
    .await?;
    Ok(matches!(row, Some((true,))))
}

/// Point a message row at an already-stored attachment. Idempotent: the entry
/// for this GUID is replaced, never duplicated. Returns whether a live row was
/// updated.
pub async fn link_attachment(
    pool: &PgPool,
    message_guid: &str,
    attachment_guid: &str,
    key: &str,
    index: Option<u64>,
    mime: &str,
    size_bytes: usize,
) -> Result<bool> {
    let entry = json!({
        "key": key,
        "mime_type": mime,
        "size_bytes": size_bytes,
        "index": index,
    });
    let done = sqlx::query(
        "UPDATE data_communication_message
            SET metadata = jsonb_set(
                    COALESCE(metadata, '{}'::jsonb),
                    '{attachment_media}',
                    COALESCE(metadata -> 'attachment_media', '{}'::jsonb)
                        || jsonb_build_object($2::text, $3::jsonb),
                    true),
                updated_at = now()
          WHERE source_stream_id = $1
            AND source_table = $4
            AND deleted_at_source IS NULL",
    )
    .bind(message_guid)
    .bind(attachment_guid)
    .bind(&entry)
    .bind(SOURCE_TABLE)
    .execute(pool)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Messages the source no longer has: tombstone each row and delete its stored
/// attachment bytes. Returns `(messages tombstoned, blobs removed)`.
///
/// A GUID the box never had is not an error — the collector only knows which
/// of ITS attachments vanished, not whether the box kept the message.
pub async fn purge_deleted_messages(
    pool: &PgPool,
    storage: &Storage,
    message_guids: &[String],
) -> Result<(u64, u64)> {
    let mut tombstoned = 0u64;
    let mut removed = 0u64;
    for guid in message_guids.iter().filter(|g| !g.is_empty()) {
        let row: Option<(Value,)> = sqlx::query_as(
            "SELECT metadata FROM data_communication_message
              WHERE source_stream_id = $1 AND source_table = $2",
        )
        .bind(guid)
        .bind(SOURCE_TABLE)
        .fetch_optional(pool)
        .await?;
        let Some((metadata,)) = row else { continue };

        // Bytes first, row second: if a delete fails the ref is still on the
        // row, so the next report of this deletion finds and retries it. The
        // other order would leave bytes nothing points at.
        for key in media_keys(&metadata) {
            // Only keys this module writes. The key came back out of a JSONB
            // column that devices can influence, so do not delete anything a
            // stray value could name.
            if !key.starts_with(&format!("media/{PROVIDER}/{STREAM}/")) {
                tracing::warn!(key, "attachment ref outside the attachment prefix — not deleting");
                continue;
            }
            if let Err(e) = storage.delete(&key).await {
                // Already gone is fine; anything else is retried by the device.
                if !is_not_found(&e) {
                    return Err(e);
                }
            }
            sqlx::query("DELETE FROM lake_objects WHERE storage_key = $1 AND kind = 'media'")
                .bind(&key)
                .execute(pool)
                .await?;
            removed += 1;
        }

        let done = sqlx::query(
            "UPDATE data_communication_message
                SET deleted_at_source = COALESCE(deleted_at_source, now()),
                    metadata = metadata - 'attachment_media',
                    updated_at = now()
              WHERE source_stream_id = $1 AND source_table = $2",
        )
        .bind(guid)
        .bind(SOURCE_TABLE)
        .execute(pool)
        .await?;
        tombstoned += done.rows_affected();
    }
    Ok((tombstoned, removed))
}

/// Attachment `index` of message `message_id` (the row's `id`): its bytes and
/// type. `NotFound` when the message, the attachment, or its bytes are absent —
/// including an attachment the Mac has not managed to send yet.
pub async fn load_attachment(
    pool: &PgPool,
    storage: &Storage,
    message_id: &str,
    index: usize,
) -> Result<(Vec<u8>, String)> {
    let row: Option<(Value,)> = sqlx::query_as(
        "SELECT metadata FROM data_communication_message
          WHERE id = $1 AND deleted_at_source IS NULL",
    )
    .bind(message_id)
    .fetch_optional(pool)
    .await?;
    let Some((metadata,)) = row else {
        return Err(Error::NotFound(format!("message {message_id}")));
    };
    let Some(r) = media_ref_for_index(&metadata, index) else {
        return Err(Error::NotFound(format!(
            "attachment {index} of message {message_id} is not stored"
        )));
    };
    let key = r
        .get("key")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::NotFound(format!("attachment {index} has no key")))?;
    let mime = r
        .get("mime_type")
        .and_then(Value::as_str)
        .unwrap_or("application/octet-stream")
        .to_string();
    let bytes = storage.download(key).await.map_err(|e| {
        if is_not_found(&e) {
            Error::NotFound(format!("attachment {index} of message {message_id}: bytes missing"))
        } else {
            e
        }
    })?;
    Ok((bytes, mime))
}

fn is_not_found(e: &Error) -> bool {
    match e {
        Error::NotFound(_) => true,
        Error::Io(io) => io.kind() == std::io::ErrorKind::NotFound,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_and_audio_are_refused_everything_else_kept() {
        assert!(!is_keepable_mime("video/quicktime"));
        assert!(!is_keepable_mime("Audio/x-m4a"));
        assert!(is_keepable_mime("image/heic"));
        assert!(is_keepable_mime("application/pdf"));
        assert!(is_keepable_mime(""));
    }

    #[test]
    fn filenames_cannot_escape_or_carry_separators() {
        assert_eq!(
            media_filename("at_0_ABC-123", "image/jpeg").as_deref(),
            Some("at_0_ABC-123.jpg")
        );
        assert_eq!(
            media_filename("../../etc/passwd", "image/png").as_deref(),
            Some("______etc_passwd.png")
        );
        assert_eq!(media_filename("../", "image/png"), None);
        assert_eq!(media_filename("", "image/png"), None);
        assert_eq!(media_filename("x", "application/x-unknown").as_deref(), Some("x.bin"));
    }

    #[test]
    fn index_resolves_through_the_attachment_guid() {
        let md = json!({
            "attachments": [{"guid": "A"}, {"guid": "B"}],
            "attachment_media": {
                "B": {"key": "media/mac/imessage_attachments/B.jpg", "index": 1},
                "A": {"key": "media/mac/imessage_attachments/A.png", "index": 0},
            }
        });
        assert_eq!(
            media_ref_for_index(&md, 1).unwrap()["key"],
            "media/mac/imessage_attachments/B.jpg"
        );
        assert_eq!(
            media_ref_for_index(&md, 0).unwrap()["key"],
            "media/mac/imessage_attachments/A.png"
        );
        assert!(media_ref_for_index(&md, 2).is_none());
    }

    #[test]
    fn index_falls_back_to_the_refs_own_index() {
        // An attachment row without a GUID in `attachments` still resolves.
        let md = json!({
            "attachments": [{"mime_type": "image/jpeg"}],
            "attachment_media": {"X": {"key": "k", "index": 0}}
        });
        assert_eq!(media_ref_for_index(&md, 0).unwrap()["key"], "k");
        // No media at all: nothing, not a panic.
        assert!(media_ref_for_index(&json!({"attachments": []}), 0).is_none());
    }

    async fn insert_message(pool: &PgPool, id: &str, guid: &str) {
        sqlx::query(
            "INSERT INTO data_communication_message
                 (id, message_id, channel, body, from_identifier, occurred_at,
                  source_stream_id, source_table, source_provider, metadata)
             VALUES ($1, $2, 'imessage', 'This one is a home', '+15125550100', now(),
                     $2, 'mac_imessage', 'mac', $3)",
        )
        .bind(id)
        .bind(guid)
        .bind(json!({"attachments": [{"guid": "AT1", "mime_type": "image/heic"}]}))
        .execute(pool)
        .await
        .unwrap();
    }

    /// The whole life of one attachment against the real schema: stored,
    /// linked, surviving a re-upsert of its message, served by index, then
    /// removed when the source deletes the message.
    #[sqlx::test]
    async fn an_attachment_is_linked_survives_reupsert_and_goes_with_its_message(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::file(dir.path().to_string_lossy().into_owned()).unwrap();
        insert_message(&pool, "msg-1", "MSG-GUID-1").await;
        assert!(message_is_live(&pool, "MSG-GUID-1").await.unwrap());
        assert!(!message_is_live(&pool, "NOT-ON-BOX").await.unwrap());

        let filename = media_filename("AT1", "image/jpeg").unwrap();
        let key = super::super::lake::put_media(&pool, &storage, PROVIDER, STREAM, &filename, b"jpeg")
            .await
            .unwrap();
        assert!(link_attachment(&pool, "MSG-GUID-1", "AT1", &key, Some(0), "image/jpeg", 4)
            .await
            .unwrap());

        // The transform's upsert merges metadata shallowly and resends
        // `attachments`; the link must survive it.
        sqlx::query(
            "UPDATE data_communication_message SET metadata = metadata || $2 WHERE id = $1",
        )
        .bind("msg-1")
        .bind(json!({"attachments": [{"guid": "AT1", "mime_type": "image/heic"}]}))
        .execute(&pool)
        .await
        .unwrap();

        let (bytes, mime) = load_attachment(&pool, &storage, "msg-1", 0).await.unwrap();
        assert_eq!(bytes, b"jpeg");
        assert_eq!(mime, "image/jpeg");
        assert!(matches!(
            load_attachment(&pool, &storage, "msg-1", 1).await,
            Err(Error::NotFound(_))
        ));

        let (tombstoned, removed) =
            purge_deleted_messages(&pool, &storage, &["MSG-GUID-1".into(), "UNKNOWN".into()])
                .await
                .unwrap();
        assert_eq!((tombstoned, removed), (1, 1));
        assert!(!dir.path().join(&key).exists(), "the bytes are gone");
        let (n,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM lake_objects WHERE storage_key = $1")
                .bind(&key)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(n, 0);
        assert!(!message_is_live(&pool, "MSG-GUID-1").await.unwrap());
        assert!(matches!(
            load_attachment(&pool, &storage, "msg-1", 0).await,
            Err(Error::NotFound(_))
        ));
        // A second report of the same deletion is harmless.
        purge_deleted_messages(&pool, &storage, &["MSG-GUID-1".into()]).await.unwrap();
    }

    #[test]
    fn media_keys_lists_every_stored_blob() {
        let md = json!({"attachment_media": {"A": {"key": "a"}, "B": {"key": "b"}, "C": {}}});
        let mut keys = media_keys(&md);
        keys.sort();
        assert_eq!(keys, vec!["a", "b"]);
        assert!(media_keys(&json!({})).is_empty());
    }
}

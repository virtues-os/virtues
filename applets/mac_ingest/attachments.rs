//! iMessage/SMS attachment bytes and source deletions.
//!
//! Two payload keys, both sent by the Mac collector on their own posts after
//! the messages they refer to have already been delivered:
//!
//! ```json
//! "imessage_attachments": [{
//!   "message_guid": "…", "attachment_guid": "at_0_…", "index": 0,
//!   "mime_type": "image/jpeg",            // of the bytes sent (HEIC arrives as JPEG)
//!   "original_mime_type": "image/heic",   // what chat.db said
//!   "filename": "IMG_0001.HEIC",
//!   "data": "<base64>"
//! }],
//! "imessage_deletions": [{"message_guid": "…"}]
//! ```
//!
//! The bytes follow the microphone stream's road: [`externalize`] takes `data`
//! out of each record and keeps it as a lake media object BEFORE the records are
//! archived, so the lake holds a record with a `media_key`, never a second
//! base64 copy. [`link`] then points the message row at it — and does the same
//! for a replayed record, which already carries its `media_key`.
//!
//! Failure is split by whether a retry could help, as in
//! `ios_ingest::bookmark`: an attachment that will not decode, is too large, is
//! a video, or belongs to a message the box does not have is dropped and the
//! post still succeeds. The disk or the database failing is an error, so the
//! Mac keeps the attachment and sends it again.

use anyhow::{Context, Result};
use base64::Engine;
use serde_json::Value;
use sqlx::PgPool;
use virtues::storage::message_media::{self, MAX_ATTACHMENT_BYTES};
use virtues::storage::Storage;

/// What becomes of one attachment record, decided without touching storage.
#[derive(Debug, PartialEq)]
enum Decoded {
    /// Bytes to keep.
    Keep { bytes: Vec<u8> },
    /// Already externalized (a replay): nothing to store, only to link.
    AlreadyStored,
    /// Never keepable; retrying would not change that.
    Drop(String),
}

fn str_field<'a>(rec: &'a Value, key: &str) -> Option<&'a str> {
    rec.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// Take the base64 OUT of the record (whatever becomes of it) and judge it.
fn take_bytes(rec: &mut Value) -> Decoded {
    let raw = rec.as_object_mut().and_then(|o| o.remove("data"));
    let Some(raw) = raw else {
        return if str_field(rec, "media_key").is_some() {
            Decoded::AlreadyStored
        } else {
            Decoded::Drop("no data and no media_key".into())
        };
    };
    if str_field(rec, "message_guid").is_none() || str_field(rec, "attachment_guid").is_none() {
        return Decoded::Drop("missing message_guid or attachment_guid".into());
    }
    let mime = str_field(rec, "mime_type").unwrap_or("");
    if !message_media::is_keepable_mime(mime) {
        return Decoded::Drop(format!("{mime} is not kept"));
    }
    let Some(b64) = raw.as_str() else {
        return Decoded::Drop("data was not a string".into());
    };
    // base64 is 4/3 the size of its bytes; refuse an oversize one before
    // allocating for it.
    if b64.len() / 4 * 3 > MAX_ATTACHMENT_BYTES + 3 {
        return Decoded::Drop("over the size cap".into());
    }
    match base64::engine::general_purpose::STANDARD.decode(b64.trim()) {
        Ok(bytes) if bytes.is_empty() => Decoded::Drop("data was empty".into()),
        Ok(bytes) if bytes.len() > MAX_ATTACHMENT_BYTES => {
            Decoded::Drop("over the size cap".into())
        }
        Ok(bytes) => Decoded::Keep { bytes },
        Err(e) => Decoded::Drop(format!("data did not decode: {e}")),
    }
}

/// Store each attachment's bytes, returning the records with `data` replaced
/// by `media_key` (or by `dropped`, saying why nothing was kept).
///
/// Runs BEFORE the records are archived.
pub async fn externalize(
    db: &PgPool,
    storage: &Storage,
    records: &[Value],
) -> Result<(Vec<Value>, usize)> {
    let mut out = Vec::with_capacity(records.len());
    let mut dropped = 0usize;
    for record in records {
        let mut rec = record.clone();
        match take_bytes(&mut rec) {
            Decoded::AlreadyStored => {}
            Decoded::Drop(why) => {
                tracing::warn!(
                    attachment = str_field(&rec, "attachment_guid").unwrap_or("?"),
                    why,
                    "iMessage attachment not kept"
                );
                rec["dropped"] = Value::String(why);
                dropped += 1;
            }
            Decoded::Keep { bytes } => {
                let message_guid = str_field(&rec, "message_guid").unwrap_or_default().to_string();
                // Only for messages the box already has: bytes nothing points
                // at are bytes nothing will ever delete.
                if !message_media::message_is_live(db, &message_guid).await? {
                    tracing::warn!(
                        message_guid,
                        "attachment for a message the box does not have (or that was deleted) — not kept"
                    );
                    rec["dropped"] = Value::String("message not on the box".into());
                    dropped += 1;
                } else {
                    let attachment_guid =
                        str_field(&rec, "attachment_guid").unwrap_or_default().to_string();
                    let mime = str_field(&rec, "mime_type").unwrap_or("application/octet-stream");
                    let filename = message_media::media_filename(&attachment_guid, mime)
                        .context("attachment guid has no usable characters")?;
                    let key = virtues::storage::lake::put_media(
                        db,
                        storage,
                        "mac",
                        message_media::STREAM,
                        &filename,
                        &bytes,
                    )
                    .await
                    .with_context(|| format!("failed to store attachment {attachment_guid}"))?;
                    rec["media_key"] = Value::String(key);
                    rec["size_bytes"] = Value::from(bytes.len());
                }
            }
        }
        out.push(rec);
    }
    Ok((out, dropped))
}

/// Point each message row at its stored attachment. Returns how many linked.
pub async fn link(db: &PgPool, records: &[Value]) -> Result<usize> {
    let mut linked = 0;
    for rec in records {
        let (Some(message_guid), Some(attachment_guid), Some(key)) = (
            str_field(rec, "message_guid"),
            str_field(rec, "attachment_guid"),
            str_field(rec, "media_key"),
        ) else {
            continue;
        };
        let mime = str_field(rec, "mime_type").unwrap_or("application/octet-stream");
        let size = rec.get("size_bytes").and_then(Value::as_u64).unwrap_or(0) as usize;
        let index = rec.get("index").and_then(Value::as_u64);
        if message_media::link_attachment(db, message_guid, attachment_guid, key, index, mime, size)
            .await?
        {
            linked += 1;
        }
    }
    Ok(linked)
}

/// The message GUIDs a deletions payload names.
pub fn deleted_guids(records: &[Value]) -> Vec<String> {
    records
        .iter()
        .filter_map(|r| {
            r.get("message_guid")
                .and_then(Value::as_str)
                .or_else(|| r.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn b64(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    fn rec(data: Value, mime: &str) -> Value {
        json!({"message_guid": "M", "attachment_guid": "A", "index": 0,
               "mime_type": mime, "data": data})
    }

    #[test]
    fn an_image_is_kept_and_the_base64_leaves_the_record() {
        let mut r = rec(json!(b64(b"\xFF\xD8\xFFjpeg")), "image/jpeg");
        assert_eq!(
            take_bytes(&mut r),
            Decoded::Keep { bytes: b"\xFF\xD8\xFFjpeg".to_vec() }
        );
        assert!(r.get("data").is_none(), "base64 must not be archived");
    }

    #[test]
    fn video_audio_oversize_and_garbage_are_dropped_not_errors() {
        let mut v = rec(json!(b64(b"x")), "video/quicktime");
        assert!(matches!(take_bytes(&mut v), Decoded::Drop(_)));
        assert!(v.get("data").is_none());

        let mut a = rec(json!(b64(b"x")), "audio/x-m4a");
        assert!(matches!(take_bytes(&mut a), Decoded::Drop(_)));

        let big = vec![0u8; MAX_ATTACHMENT_BYTES + 1];
        let mut o = rec(json!(b64(&big)), "image/png");
        assert!(matches!(take_bytes(&mut o), Decoded::Drop(_)));

        let mut g = rec(json!("not base64!!"), "image/png");
        assert!(matches!(take_bytes(&mut g), Decoded::Drop(_)));

        let mut e = rec(json!(""), "image/png");
        assert!(matches!(take_bytes(&mut e), Decoded::Drop(_)));
    }

    #[test]
    fn exactly_the_cap_is_kept() {
        let at_cap = vec![1u8; MAX_ATTACHMENT_BYTES];
        let mut r = rec(json!(b64(&at_cap)), "application/pdf");
        assert!(matches!(take_bytes(&mut r), Decoded::Keep { .. }));
    }

    #[test]
    fn a_replayed_record_is_linked_not_restored() {
        let mut r = json!({"message_guid": "M", "attachment_guid": "A",
                           "media_key": "media/mac/imessage_attachments/A.jpg"});
        assert_eq!(take_bytes(&mut r), Decoded::AlreadyStored);
    }

    #[test]
    fn a_record_without_ids_is_dropped() {
        let mut r = json!({"attachment_guid": "A", "mime_type": "image/png", "data": b64(b"x")});
        assert!(matches!(take_bytes(&mut r), Decoded::Drop(_)));
    }

    #[test]
    fn deletions_accept_objects_or_bare_strings() {
        let recs = vec![json!({"message_guid": "G1"}), json!("G2"), json!({"message_guid": ""})];
        assert_eq!(deleted_guids(&recs), vec!["G1", "G2"]);
    }
}

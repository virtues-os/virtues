//! Mac activity → ontology transforms.
//!
//! The Mac client app posts batches of four record kinds in one webhook:
//!   - `app_events` → aggregated into app sessions in `data_activity_app_session`
//!   - `browser_history` → `data_activity_web_browsing`
//!   - `imessages` → `data_communication_message`
//!   - `bookmarks` → `data_content_bookmark` (per-browser snapshots, reconciled)
//!
//! The expected payload shape (matches the deleted `core/src/sources/mac/transform.rs`
//! aggregation behaviour):
//! ```json
//! {
//!   "app_events": [{"timestamp": "...", "bundle_id": "...", "app_name": "...", "window_title": "..."}, ...],
//!   "browser_history": [{"url": "...", "title": "...", "timestamp": "..."}, ...],
//!   "imessages": [{"guid": "...", "text": "...", "timestamp": "...", "from_handle": "...", "is_from_me": true}, ...]
//! }
//! ```

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;
use virtues_helpers::bookmarks::{self, BookmarkRow};
use virtues_helpers::dedup::{
    build_batch_insert_query, build_batch_upsert_query, dedup_refs_keep_last, BATCH_SIZE,
};

const PROVIDER: &str = "mac";

/// A record's timestamp, or `None` if it has none we can parse.
///
/// Deliberately NOT `unwrap_or_else(Utc::now)`. Every one of these transforms folds
/// the timestamp into either its dedup key or the row's place in the timeline, so a
/// wall-clock fallback doesn't "recover" a bad record — it silently mints a new
/// identity on every retry (duplicating the row) or files the record at ingest time
/// (wrong forever, since ON CONFLICT DO NOTHING never corrects it). Callers skip
/// records this returns `None` for; the raw record is still in the lake either way.
fn event_time(record: &Value) -> Option<DateTime<Utc>> {
    record
        .get("timestamp")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
}

// NOTE: app-event aggregation used to live here. It grouped events into sessions
// WITHIN a single upload batch, which is structurally incapable of recording a
// session longer than the upload interval — a 40-minute focus produced no row at
// all, while backlog batches fabricated enormous ones. It now lives in
// `sessionize.rs`, which holds sessions open across batches against the DB.

// ─────────────────────────────────────────────────────────────────────────────
// Browser history → data_activity_web_browsing
// ─────────────────────────────────────────────────────────────────────────────

pub async fn write_browser_history(db: &PgPool, visits: &[Value]) -> Result<usize> {
    let mut pending: Vec<(
        String,
        String,
        String,
        Option<String>,
        DateTime<Utc>,
        String,
        Value,
    )> = Vec::new();
    let mut written = 0;

    for visit in visits {
        let url = visit.get("url").and_then(|v| v.as_str()).unwrap_or("");
        if url.is_empty() {
            continue;
        }
        let domain = extract_domain(url).unwrap_or_else(|| "unknown".to_string());
        let title = visit
            .get("title")
            .and_then(|v| v.as_str())
            .map(String::from);
        // NO Utc::now() fallback. The timestamp goes straight into the dedup key
        // below, so defaulting to the wall clock means a retry of the SAME visit
        // computes a DIFFERENT source_stream_id and inserts a duplicate — every
        // 5 minutes, for as long as the device keeps retrying. A visit we cannot
        // place in time is not a visit we can dedup; skip it.
        let Some(ts) = event_time(visit) else {
            tracing::warn!(url, "browser visit has no parseable timestamp — skipping");
            continue;
        };

        // HASH the dedup key rather than embedding the URL in it.
        //
        // `{url}:{ts}` looks fine until someone visits a URL with a page of tracking
        // parameters (or a `data:` URI). source_stream_id is UNIQUE, and a btree
        // index row cannot exceed ~2704 bytes — so one long URL doesn't just drop
        // that visit, it fails the INSERT, which 500s the webhook, which poisons the
        // whole batch: app sessions and iMessages die with it, and the device retries
        // the same doomed payload every 5 minutes. A UUIDv5 is 36 bytes no matter how
        // deranged the URL, and it is just as deterministic.
        let stream_id = Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("mac:browse:{}:{}", url, ts.timestamp_millis()).as_bytes(),
        )
        .to_string();
        let id = Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("mac:browse:{stream_id}").as_bytes(),
        )
        .to_string();

        pending.push((
            id,
            url.to_string(),
            domain,
            title,
            ts,
            stream_id,
            serde_json::json!({"browser": visit.get("browser")}),
        ));

        if pending.len() >= BATCH_SIZE {
            written += flush_browser(db, &pending).await?;
            pending.clear();
        }
    }
    if !pending.is_empty() {
        written += flush_browser(db, &pending).await?;
    }
    Ok(written)
}

fn extract_domain(url: &str) -> Option<String> {
    let after_scheme = url.split("://").nth(1)?;
    let host = after_scheme.split('/').next()?;
    Some(host.trim_start_matches("www.").to_string())
}

async fn flush_browser(
    db: &PgPool,
    rows: &[(
        String,
        String,
        String,
        Option<String>,
        DateTime<Utc>,
        String,
        Value,
    )],
) -> Result<usize> {
    if rows.is_empty() {
        return Ok(0);
    }
    let sql = build_batch_insert_query(
        "data_activity_web_browsing",
        &[
            "id",
            "url",
            "domain",
            "page_title",
            "timestamp",
            "source_stream_id",
            "source_table",
            "source_provider",
            "metadata",
        ],
        "source_stream_id",
        rows.len(),
    );
    let mut q = sqlx::query(&sql);
    for r in rows {
        q = q
            .bind(&r.0)
            .bind(&r.1)
            .bind(&r.2)
            .bind(&r.3)
            .bind(r.4)
            .bind(&r.5)
            .bind("mac_browser")
            .bind(PROVIDER)
            .bind(&r.6);
    }
    Ok(q.execute(db).await?.rows_affected() as usize)
}

// ─────────────────────────────────────────────────────────────────────────────
// iMessage → data_communication_message
// ─────────────────────────────────────────────────────────────────────────────

/// iMessage's stand-in for an attachment: U+FFFC, OBJECT REPLACEMENT CHARACTER.
///
/// It renders as an invisible box. A message whose whole content is a photo therefore
/// arrives as a message that appears to say nothing at all — which is why 614 of the
/// first 9,000 messages synced were blank, and why a thread reads as though someone
/// went quiet mid-conversation when in fact they sent you a picture.
const OBJECT_REPLACEMENT: char = '\u{FFFC}';

/// What to call an attachment inside the body text.
///
/// Not an attempt to be clever — the point is that "[Photo]" is legible to a person,
/// to search, and to a model reading the thread back, and an invisible control
/// character is legible to none of them. We store no bytes; this is the whole of what
/// a reader gets, so it has to carry its own weight.
fn attachment_label(att: &Value) -> String {
    let mime = att.get("mime_type").and_then(|v| v.as_str()).unwrap_or("");
    let name = att.get("filename").and_then(|v| v.as_str()).unwrap_or("");
    let is_sticker = att
        .get("is_sticker")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if is_sticker {
        return "[Sticker]".to_string();
    }

    let kind = match mime.split('/').next().unwrap_or("") {
        // Media is named by its TYPE. Its filename is camera noise ("IMG_4821.HEIC")
        // and tells a reader nothing they don't already get from "[Photo]".
        "image" if mime == "image/gif" => "GIF",
        "image" => "Photo",
        "video" => "Video",
        "audio" => "Audio message",
        _ => {
            // Documents are the other way round: the NAME is the signal.
            // "[File: Cars.com.pdf]" says they sent you a car listing. "[PDF]" says
            // nothing at all, and this is a real message from the archive.
            if !name.is_empty() {
                return format!("[File: {name}]");
            }
            match mime {
                "text/vcard" | "text/x-vcard" => "Contact",
                "application/pdf" => "PDF",
                _ => "Attachment",
            }
        }
    };
    format!("[{kind}]")
}

/// Replace each U+FFFC in the body with a label for the attachment it stands for.
///
/// The placeholders are positional: the Nth U+FFFC is the Nth attachment. Messages
/// that are *only* an attachment usually carry no text at all — no placeholder either —
/// so any attachments left over are appended, otherwise the body stays empty and the
/// message still reads as if nothing was sent.
///
/// This edits the *projection*, not the evidence: the raw payload, U+FFFC and all, is
/// already in the lake. `data_*` is what people and models read, and it should read.
fn render_attachments(body: &str, attachments: &[Value]) -> String {
    if attachments.is_empty() {
        return body.to_string();
    }

    let mut next = 0usize;
    let mut out = String::with_capacity(body.len() + attachments.len() * 8);
    for ch in body.chars() {
        if ch == OBJECT_REPLACEMENT {
            out.push_str(
                &attachments
                    .get(next)
                    .map(attachment_label)
                    .unwrap_or_else(|| "[Attachment]".to_string()),
            );
            next += 1;
        } else {
            out.push(ch);
        }
    }

    for att in attachments.iter().skip(next) {
        if !out.is_empty() && !out.ends_with(' ') {
            out.push(' ');
        }
        out.push_str(&attachment_label(att));
    }

    out.trim().to_string()
}

/// One message, ready to insert. A struct rather than a tuple because this grew to
/// eleven fields and a mis-ordered bind is exactly the kind of bug that ships
/// silently — the columns all take text.
struct Msg {
    id: String,
    body: String,
    from_identifier: String,
    /// The same identifier in the one normal form (see `virtues_helpers::handles`),
    /// so the sender can be *joined* to `wiki_people.handles` instead of looked up one
    /// message at a time. `""` means "not a person" — a short code, or ourselves —
    /// and is what stops the resolver re-asking about every 2FA robot forever.
    from_handle: String,
    thread_id: Option<String>,
    timestamp: DateTime<Utc>,
    guid: String,
    metadata: Value,
    is_read: bool,
    has_attachments: bool,
    is_group: bool,
    reply_to: Option<String>,
}

pub async fn write_imessages(db: &PgPool, messages: &[Value]) -> Result<usize> {
    let mut pending: Vec<Msg> = Vec::new();
    let mut written = 0;

    for m in messages {
        let guid = m.get("guid").and_then(|v| v.as_str()).unwrap_or("");
        if guid.is_empty() {
            continue;
        }
        let text = m
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let is_from_me = m
            .get("is_from_me")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        // from_identifier is NOT NULL. In chat.db the handle identifies the *other*
        // party even on messages we sent, so is_from_me has to win — otherwise our
        // own messages get attributed to the recipient.
        let from_handle = if is_from_me {
            "me".to_string()
        } else {
            m.get("from_handle")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };
        // NOTE: `direction` ('sent'/'received') is a column on
        // data_communication_*EMAIL*, NOT on data_communication_message — inserting
        // it here failed every iMessage batch with "column \"direction\" ... does not
        // exist" (and, because app_events ride the same webhook batch, took those
        // down too). Sent-vs-received is preserved in `metadata.is_from_me` below.

        // The collector sends `chat_id`; this read `chat_guid`. They never matched, so
        // thread_id was NULL on EVERY message ever ingested — no conversation grouping
        // at all, and no way to ask "what did I talk about with X". Accept both.
        let chat_guid = m
            .get("chat_id")
            .or_else(|| m.get("chat_guid"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from);

        // Everything below was already on the wire and being thrown away on arrival:
        // every tapback, every read receipt, every "they sent a photo".
        let is_read = m.get("is_read").and_then(|v| v.as_bool()).unwrap_or(false);

        // Metadata only, by design — no image bytes. What we keep is enough to *say*
        // what was sent, plus the on-disk `path`, which is what makes a v2 backfill of
        // the images themselves a backfill rather than archaeology.
        let attachments: Vec<Value> = m
            .get("attachment_info")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let text = render_attachments(&text, &attachments);

        let has_attachments = !attachments.is_empty()
            || m.get("cache_has_attachments")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            || m.get("attachment_count")
                .and_then(|v| v.as_i64())
                .is_some_and(|n| n > 0);
        let group_title = m.get("group_title").and_then(|v| v.as_str());

        // The chat GUID encodes group-ness, and it's the only reliable signal:
        // `group_title` is empty for most group chats (people don't name them), so
        // keying on it found zero groups.
        //
        //   any;-;+15125550164            1:1
        //   any;+;chat100000000000000001  group
        let is_group = chat_guid.as_deref().is_some_and(|g| g.contains(";+;"));

        // A tapback IS a message row in chat.db: `associated_message_type` says which
        // reaction (2000-3005), and `associated_message_guid` points at the message it
        // reacts TO. That target is exactly what reply_to_message_id is for.
        //
        // ZERO means "not a reaction" — it is not NULL. Storing it verbatim tagged
        // every ordinary message as a reaction of type 0, which is how you end up with
        // "500 of 500 messages have reactions".
        let reaction_type = m
            .get("associated_message_type")
            .and_then(|v| v.as_i64())
            .filter(|t| *t > 0);
        let reply_to = m
            .get("associated_message_guid")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            // chat.db prefixes these ("p:0/GUID", "bp:GUID") — the bare guid is the
            // one that joins back to message_id.
            .map(|s| s.rsplit('/').next().unwrap_or(s).to_string());
        // The GUID keys the dedup, so an unparseable timestamp wouldn't duplicate the
        // row — it would silently file the message at INGEST time instead, landing a
        // months-old message in today's timeline and then never correcting it
        // (ON CONFLICT DO NOTHING). A message we can't place in time is worse than
        // no message.
        let Some(ts) = event_time(m) else {
            tracing::warn!(guid, "iMessage has no parseable timestamp — skipping");
            continue;
        };

        let id = Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("mac:imessage:{guid}").as_bytes(),
        )
        .to_string();

        pending.push(Msg {
            id,
            body: text,
            from_handle: virtues_helpers::handles::normalize_handle(&from_handle)
                .unwrap_or_default(),
            from_identifier: from_handle,
            thread_id: chat_guid,
            timestamp: ts,
            guid: guid.to_string(),
            metadata: serde_json::json!({
                "is_from_me": is_from_me,
                "service": m.get("service"),
                "group_title": group_title,
                "attachment_count": m.get("attachment_count"),
                "date_read": m.get("date_read"),
                // The reaction KIND (liked/loved/laughed/…) — reply_to_message_id says
                // what it reacted to, this says what it was.
                "reaction_type": reaction_type,
                "expressive_send_style": m.get("expressive_send_style_id"),
                // guid / mime_type / filename / size_bytes / uti / is_sticker / path.
                // `path` is the pointer: chat.db keeps the file under
                // ~/Library/Messages/Attachments/ indefinitely, so v2 can fetch the
                // bytes later without needing the message thread to still make sense.
                "attachments": attachments,
            }),
            is_read,
            has_attachments,
            is_group,
            reply_to,
        });

        if pending.len() >= BATCH_SIZE {
            written += flush_imessage(db, &pending).await?;
            pending.clear();
        }
    }
    if !pending.is_empty() {
        written += flush_imessage(db, &pending).await?;
    }
    Ok(written)
}

async fn flush_imessage(db: &PgPool, rows: &[Msg]) -> Result<usize> {
    if rows.is_empty() {
        return Ok(0);
    }
    // The GUID is the conflict key (source_stream_id). A chat.db read can surface the
    // same GUID twice in one batch; collapse to the last so the ON CONFLICT DO UPDATE
    // doesn't abort the whole flush ("cannot affect row a second time").
    let rows = dedup_refs_keep_last(rows, |r| &r.guid);
    // UPSERT, not DO NOTHING — the one stream where that matters.
    //
    // There is no upstream to re-fetch a message from: chat.db is the only copy, and a
    // transform bug therefore has to be corrected on the rows we already hold. Under
    // DO NOTHING, adding attachment metadata would have rendered "[Photo]" on new
    // messages and left 845 historical ones as a blank invisible box forever.
    //
    // Note what is NOT in `update`: `from_name` (owned by the entity resolver — the
    // transform doesn't know who anyone is and must not overwrite an answer it didn't
    // compute), `timestamp` and `message_id` (identity), and `metadata`, which is
    // merged rather than replaced by the builder.
    let columns: &[&str] = &[
        "id",
        // The provider's native message id (the iMessage GUID). NOT NULL — omitting
        // it is what broke ingest after the `direction` fix.
        "message_id",
        "body",
        "from_identifier",
        // The normal form of the above, so resolution is a join, not an N+1.
        "from_handle",
        // `channel` is what the registry reads for a message's source_type
        // ("message:" || channel), so name it rather than leaving it "unknown".
        "channel",
        "thread_id",
        "timestamp",
        // These four were on the wire and thrown away on arrival — every tapback,
        // every read receipt, every "they sent a photo", and every group chat.
        "is_read",
        "has_attachments",
        "is_group_message",
        "reply_to_message_id",
        "source_stream_id",
        "source_table",
        "source_provider",
        "metadata",
    ];
    let sql = build_batch_upsert_query(
        "data_communication_message",
        columns,
        "source_stream_id",
        // Everything a fixed transform can legitimately correct on a message we already
        // have. All of these are pure functions of the chat.db row, so re-deriving them
        // can only make an existing row more right.
        &[
            "body",
            "from_handle",
            "thread_id",
            "is_read",
            "has_attachments",
            "is_group_message",
            "reply_to_message_id",
            "metadata",
        ],
        rows.len(),
    );
    let mut q = sqlx::query_as::<_, (Option<bool>,)>(&sql);
    for r in rows {
        // The GUID serves as both the native message_id and the dedup key.
        q = q
            .bind(&r.id)
            .bind(&r.guid)
            .bind(&r.body)
            .bind(&r.from_identifier)
            .bind(&r.from_handle)
            .bind("imessage")
            .bind(&r.thread_id)
            .bind(r.timestamp)
            .bind(r.is_read)
            .bind(r.has_attachments)
            .bind(r.is_group)
            .bind(&r.reply_to)
            .bind(&r.guid)
            .bind("mac_imessage")
            .bind(PROVIDER)
            .bind(&r.metadata);
    }

    // `xmax = 0` distinguishes a genuinely new row from a corrected one. Without it a
    // backfill reports "845 written" whether it fixed 845 messages or zero.
    let results = q.fetch_all(db).await?;
    let inserted = results
        .iter()
        .filter(|(is_insert,)| is_insert.unwrap_or(true))
        .count();
    let corrected = results.len() - inserted;
    if corrected > 0 {
        tracing::info!(inserted, corrected, "iMessages written");
    }
    Ok(results.len())
}

// ─────────────────────────────────────────────────────────────────────────────
// Browser bookmarks → data_content_bookmark
// ─────────────────────────────────────────────────────────────────────────────

/// Bookmark files are SNAPSHOTS, not event logs — the payload is one browser's
/// complete current state, so this is a reconcile, not an append: everything
/// present upserts, and anything of ours missing from the snapshot is
/// tombstoned (`deleted_at_source`), never deleted. The collector only sends a
/// browser's entry when its file changed, so most batches carry no `bookmarks`
/// key at all.
///
/// Payload shape (one entry per browser, each a COMPLETE snapshot):
/// ```json
/// "bookmarks": [{
///   "browser": "safari",
///   "records": [{"guid": "...", "url": "...", "title": "...",
///                "folder_path": ["Favorites", "Design"],
///                "date_added": "2024-05-01T12:00:00Z",
///                "kind": "bookmark" | "reading_list", "preview": "..."}]
/// }]
/// ```
///
/// The reconcile scope is `mac:{device}:{browser}:` — two Macs, or Safari vs
/// Chrome on one Mac, can never tombstone each other's rows.
pub async fn write_bookmarks(
    db: &PgPool,
    device_id: &str,
    snapshots: &[Value],
) -> Result<(usize, u64)> {
    let mut written = 0usize;
    let mut tombstoned = 0u64;

    for snap in snapshots {
        let Some(browser) = snap
            .get("browser")
            .and_then(|v| v.as_str())
            .filter(|b| !b.is_empty())
        else {
            tracing::warn!("bookmark snapshot missing `browser` — skipping entry");
            continue;
        };
        // A missing/malformed `records` key is NOT an empty snapshot. An empty
        // ARRAY legitimately means "this browser now has zero bookmarks —
        // tombstone them all"; an absent key means the payload is broken, and
        // reading it as empty would turn a collector bug into a mass delete.
        let Some(records) = snap.get("records").and_then(|v| v.as_array()).cloned() else {
            tracing::warn!(
                browser,
                "bookmark snapshot has no `records` array — skipping entry"
            );
            continue;
        };

        let prefix = format!("mac:{device_id}:{browser}:");
        let rows = snapshot_rows(device_id, browser, &records);

        let present: Vec<String> = rows.iter().map(|r| r.source_stream_id.clone()).collect();
        written += bookmarks::upsert_bookmarks(db, &rows).await?;
        tombstoned += bookmarks::tombstone_absent(db, &prefix, &present).await?;
    }

    Ok((written, tombstoned))
}

/// One browser snapshot's records → normalized rows. Pure so the identity and
/// sentinel rules below are testable.
fn snapshot_rows(device_id: &str, browser: &str, records: &[Value]) -> Vec<BookmarkRow> {
    let prefix = format!("mac:{device_id}:{browser}:");
    let mut rows: Vec<BookmarkRow> = Vec::with_capacity(records.len());

    for rec in records {
        let Some(url) = rec
            .get("url")
            .and_then(|v| v.as_str())
            .filter(|u| !u.is_empty())
        else {
            continue; // folders arrive as path context on leaves, not as records
        };
        // GUID is the identity (stable across renames/moves in both plist
        // and Chromium JSON). A record without one cannot be reconciled —
        // a synthetic id would tombstone+recreate it on every snapshot.
        let Some(guid) = rec
            .get("guid")
            .and_then(|v| v.as_str())
            .filter(|g| !g.is_empty())
        else {
            tracing::warn!(url, browser, "bookmark record has no guid — skipping");
            continue;
        };

        // Safari's plist stores no date for plain bookmarks. The sentinel
        // is epoch-0, NOT the wall clock: `timestamp` is in the upsert's
        // update set, so a wall-clock fallback would re-file every undated
        // bookmark at each snapshot push. Epoch-0 is stable across syncs
        // and keeps undated bookmarks off the day timeline instead of
        // pretending they were added today.
        let (ts, date_known) = match rec
            .get("date_added")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        {
            Some(t) => (t, true),
            None => (DateTime::<Utc>::UNIX_EPOCH, false),
        };

        let folder_path: Vec<String> = rec
            .get("folder_path")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let kind = rec
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("bookmark");

        rows.push(BookmarkRow {
            url: url.to_string(),
            title: rec
                .get("title")
                .and_then(|v| v.as_str())
                .filter(|t| !t.is_empty())
                .map(String::from),
            // Reading List items carry a WebKit-generated preview blurb.
            description: rec
                .get("preview")
                .and_then(|v| v.as_str())
                .filter(|p| !p.is_empty())
                .map(String::from),
            source_platform: Some(browser.to_string()),
            bookmark_type: Some(kind.to_string()),
            author: None,
            // The folder path is the user's own taxonomy — the container
            // whisper (docs/bookmarks-plan.md). Harvested as tags.
            tags: (!folder_path.is_empty()).then(|| serde_json::json!(folder_path)),
            thumbnail_url: None,
            timestamp: ts,
            source_stream_id: format!("{prefix}{guid}"),
            source_table: "mac_bookmarks".to_string(),
            source_provider: PROVIDER.to_string(),
            metadata: serde_json::json!({
                "device_id": device_id,
                "browser": browser,
                "folder_path": folder_path,
                "date_known": date_known,
            }),
        });
    }

    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bookmark_identity_is_the_guid_and_guidless_records_are_skipped() {
        let recs = vec![
            json!({"guid": "ABC-123", "url": "https://example.com", "title": "Example",
                   "folder_path": ["Favorites", "Design"],
                   "date_added": "2024-05-01T12:00:00Z", "kind": "bookmark"}),
            // No guid: cannot be reconciled — a synthetic id would tombstone
            // and recreate it on every snapshot. Must be skipped, not invented.
            json!({"url": "https://no-guid.example", "title": "nope"}),
            // Folders/separators arrive without urls; also skipped.
            json!({"guid": "DEF-456", "title": "a folder node"}),
        ];
        let rows = snapshot_rows("dev1", "safari", &recs);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source_stream_id, "mac:dev1:safari:ABC-123");
        assert_eq!(rows[0].tags, Some(json!(["Favorites", "Design"])));
        assert_eq!(rows[0].bookmark_type.as_deref(), Some("bookmark"));
    }

    #[test]
    fn undated_bookmarks_get_the_epoch_sentinel_not_the_wall_clock() {
        // Safari's plist stores no date for plain bookmarks. `timestamp` is in
        // the upsert's update set, so a wall-clock fallback would re-file the
        // bookmark at every snapshot push; the sentinel is stable across syncs
        // and keeps undated bookmarks off the day timeline.
        let recs = vec![json!({"guid": "G", "url": "https://example.com"})];
        let rows = snapshot_rows("dev1", "safari", &recs);
        assert_eq!(rows[0].timestamp, DateTime::<Utc>::UNIX_EPOCH);
        assert_eq!(rows[0].metadata["date_known"], json!(false));
    }

    fn photo() -> Value {
        json!({"mime_type": "image/heic", "filename": "IMG_4821.HEIC"})
    }

    #[test]
    fn a_photo_only_message_is_no_longer_blank() {
        // The 614-message case: chat.db gives us a body that is one invisible box.
        assert_eq!(render_attachments("\u{FFFC}", &[photo()]), "[Photo]");
    }

    #[test]
    fn placeholders_are_positional() {
        let atts = vec![photo(), json!({"mime_type": "video/quicktime"})];
        assert_eq!(
            render_attachments("look \u{FFFC} and \u{FFFC}", &atts),
            "look [Photo] and [Video]"
        );
    }

    #[test]
    fn attachments_without_a_placeholder_are_still_named() {
        // A bare photo often carries no text and no U+FFFC at all. Appending is the
        // difference between "[Photo]" and a message that reads as if nothing was sent.
        assert_eq!(render_attachments("", &[photo()]), "[Photo]");
        assert_eq!(render_attachments("here", &[photo()]), "here [Photo]");
    }

    #[test]
    fn a_named_file_says_more_than_its_type() {
        let doc = json!({"mime_type": "application/vnd.ms-excel", "filename": "rent.xlsx"});
        assert_eq!(render_attachments("\u{FFFC}", &[doc]), "[File: rent.xlsx]");
        // A REAL message from the archive: the whole body was one invisible box, and the
        // attachment was a car listing. "[PDF]" would have thrown away the only part of
        // it that means anything.
        let pdf = json!({"mime_type": "application/pdf", "filename": "Cars.com.pdf"});
        assert_eq!(
            render_attachments("\u{FFFC}", &[pdf]),
            "[File: Cars.com.pdf]"
        );
        // ...but a photo is named by its type: IMG_4821.HEIC tells a reader nothing.
        let photo = json!({"mime_type": "image/heic", "filename": "IMG_4821.HEIC"});
        assert_eq!(render_attachments("\u{FFFC}", &[photo]), "[Photo]");
    }

    #[test]
    fn stickers_and_gifs_are_not_photos() {
        let sticker = json!({"mime_type": "image/png", "is_sticker": true});
        assert_eq!(render_attachments("\u{FFFC}", &[sticker]), "[Sticker]");
        let gif = json!({"mime_type": "image/gif"});
        assert_eq!(render_attachments("\u{FFFC}", &[gif]), "[GIF]");
    }

    #[test]
    fn a_message_with_no_attachments_is_untouched() {
        assert_eq!(render_attachments("hello", &[]), "hello");
        // Including one that somehow still holds a placeholder — we must not invent an
        // attachment that the metadata does not vouch for.
        assert_eq!(render_attachments("hi \u{FFFC}", &[]), "hi \u{FFFC}");
    }
}

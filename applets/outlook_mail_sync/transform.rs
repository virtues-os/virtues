//! Outlook messages from Microsoft Graph → `data_communication_email`.
//!
//! Same table and columns as the Gmail transform. A message is keyed by the
//! mailbox's own address and its `internetMessageId`:
//! `outlook:{mailbox}:{internetMessageId}`. The message id stays the same when
//! the message moves between folders; Graph's own `id` changes on every move,
//! so it is only the fallback. The mailbox is in the key because two connected
//! accounts can receive the same mail, and each keeps its own row with its own
//! read state, flag and folder.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;
use virtues_helpers::dedup::{build_batch_insert_query, dedup_refs_keep_last, BATCH_SIZE};

const SOURCE_TABLE: &str = "outlook_mail";
const SOURCE_PROVIDER: &str = "outlook";
/// The well-known folder whose messages are always the owner's own.
pub const SENT_FOLDER: &str = "sentitems";

/// One `data_communication_email` row.
#[derive(Debug, Clone, PartialEq)]
pub struct EmailRow {
    pub id: String,
    pub message_id: String,
    pub conversation_id: Option<String>,
    pub subject: Option<String>,
    pub body: Option<String>,
    pub body_preview: Option<String>,
    pub from_email: String,
    pub from_name: Option<String>,
    pub to_emails: Value,
    pub to_names: Value,
    pub cc_emails: Value,
    pub bcc_emails: Value,
    pub direction: &'static str,
    pub is_read: bool,
    pub is_starred: bool,
    pub has_attachments: bool,
    pub labels: Value,
    pub occurred_at: DateTime<Utc>,
    pub source_stream_id: String,
    pub metadata: Value,
}

/// What one page did to the table.
#[derive(Debug, Default, Clone, Copy)]
pub struct Written {
    /// Messages stored for the first time.
    pub added: usize,
    /// Stored messages whose read state, flag, categories or folder changed.
    pub changed: u64,
}

/// Store one page of a folder's delta. A message already stored (seen in an
/// earlier run or another folder) is not inserted again; its read state,
/// flag and labels are brought up to date instead.
pub async fn write_messages(db: &PgPool, user_email: &str, folder: &str, messages: &[Value]) -> Result<Written> {
    let rows: Vec<EmailRow> = messages.iter().filter_map(|m| shape_one(m, user_email, folder)).collect();
    // A page can carry the same message twice when it changed during paging.
    // The last copy is the current one.
    let rows: Vec<&EmailRow> = dedup_refs_keep_last(&rows, |r| r.source_stream_id.clone());
    let mut written = Written::default();
    for chunk in rows.chunks(BATCH_SIZE) {
        written.added += insert(db, chunk).await?;
        written.changed += update_state(db, chunk).await?;
    }
    Ok(written)
}

/// Shape one Graph message into a row. `None` for a removal entry, a draft,
/// or a message with no id or time.
pub fn shape_one(msg: &Value, user_email: &str, folder: &str) -> Option<EmailRow> {
    // In a folder's delta, a removal almost always means the message moved to
    // another folder, where it shows up as an addition. Nothing is deleted.
    if msg.get("@removed").is_some() {
        return None;
    }
    if msg.get("isDraft").and_then(Value::as_bool).unwrap_or(false) {
        return None;
    }
    let graph_id = str_field(msg, "id")?;
    let internet_id = str_field(msg, "internetMessageId");
    let message_id = internet_id.clone().unwrap_or_else(|| graph_id.clone());
    let key = stream_key(user_email, &message_id);

    let (from_email, from_name) = msg.get("from").map(address).unwrap_or_default();
    let (to_emails, to_names) = recipients(msg.get("toRecipients"));
    let (cc_emails, _) = recipients(msg.get("ccRecipients"));
    let (bcc_emails, _) = recipients(msg.get("bccRecipients"));

    let sent = folder == SENT_FOLDER || (!user_email.is_empty() && from_email.eq_ignore_ascii_case(user_email));
    let direction = if sent { "sent" } else { "received" };

    let received_at = time_field(msg, "receivedDateTime");
    let sent_at = time_field(msg, "sentDateTime");
    let occurred_at = if sent { sent_at.or(received_at) } else { received_at.or(sent_at) }?;

    let content_type = msg.pointer("/body/contentType").and_then(Value::as_str).unwrap_or("text");
    let body = msg
        .pointer("/body/content")
        .and_then(Value::as_str)
        .map(|c| if content_type.eq_ignore_ascii_case("html") { html_to_text(c) } else { c.trim().to_string() })
        .filter(|c| !c.is_empty());

    let categories: Vec<String> = msg
        .get("categories")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|c| c.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let mut labels = categories.clone();
    labels.push(folder.to_string());

    let conversation_id = str_field(msg, "conversationId");
    let metadata = serde_json::json!({
        "graph_id": graph_id,
        "internet_message_id": internet_id,
        "conversation_id": conversation_id,
        "folder": folder,
        "categories": categories,
        "received_at": received_at,
        "sent_at": sent_at,
        "body_content_type": content_type,
    });

    Some(EmailRow {
        id: Uuid::new_v5(&Uuid::NAMESPACE_OID, format!("outlook:mail:{key}").as_bytes()).to_string(),
        message_id,
        conversation_id,
        subject: str_field(msg, "subject"),
        body,
        body_preview: str_field(msg, "bodyPreview").filter(|p| !p.is_empty()),
        from_email,
        from_name,
        to_emails: serde_json::json!(to_emails),
        to_names: serde_json::json!(to_names),
        cc_emails: serde_json::json!(cc_emails),
        bcc_emails: serde_json::json!(bcc_emails),
        direction,
        is_read: msg.get("isRead").and_then(Value::as_bool).unwrap_or(true),
        is_starred: msg.pointer("/flag/flagStatus").and_then(Value::as_str) == Some("flagged"),
        has_attachments: msg.get("hasAttachments").and_then(Value::as_bool).unwrap_or(false),
        labels: serde_json::json!(labels),
        occurred_at,
        source_stream_id: key,
        metadata,
    })
}

/// `outlook:{mailbox address, lowercased}:{message id}`. Insert and the
/// state update both match on this.
fn stream_key(user_email: &str, message_id: &str) -> String {
    format!("outlook:{}:{message_id}", user_email.trim().to_lowercase())
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(String::from)
}

fn time_field(v: &Value, key: &str) -> Option<DateTime<Utc>> {
    v.get(key).and_then(Value::as_str).and_then(|s| s.parse::<DateTime<Utc>>().ok())
}

/// A Graph recipient, `{"emailAddress": {"address", "name"}}`, as an address
/// and an optional name.
fn address(r: &Value) -> (String, Option<String>) {
    let email = r.pointer("/emailAddress/address").and_then(Value::as_str).unwrap_or("").to_string();
    let name = r
        .pointer("/emailAddress/name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|n| !n.is_empty() && !n.eq_ignore_ascii_case(&email))
        .map(String::from);
    (email, name)
}

fn recipients(v: Option<&Value>) -> (Vec<String>, Vec<Option<String>>) {
    v.and_then(Value::as_array)
        .map(|a| a.iter().map(address).filter(|(e, _)| !e.is_empty()).unzip())
        .unwrap_or_default()
}

/// Readable text from an HTML mail body. Graph is asked for text bodies, but
/// does not always honor it. Block elements become line breaks; styles,
/// scripts, the head and comments are dropped; entities are decoded.
pub fn html_to_text(html: &str) -> String {
    // ASCII lowercasing keeps every byte offset, so `lower` indexes `html`.
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut i = 0;
    while let Some(off) = html[i..].find('<') {
        // Line breaks in HTML source are only spaces.
        out.push_str(&html[i..i + off].replace(['\r', '\n'], " "));
        let start = i + off;
        if lower[start..].starts_with("<!--") {
            i = lower[start..].find("-->").map_or(html.len(), |e| start + e + 3);
            continue;
        }
        let Some(close) = html[start..].find('>') else {
            i = html.len();
            break;
        };
        let end = start + close + 1;
        let inner = &lower[start + 1..end - 1];
        let closing = inner.starts_with('/');
        let name: String = inner.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
        if !closing && matches!(name.as_str(), "style" | "script" | "head" | "title") {
            // Skip to the end of the element. Without a closing tag, drop
            // only the opening one.
            let close_tag = format!("</{name}");
            i = lower[end..]
                .find(&close_tag)
                .and_then(|c| lower[end + c..].find('>').map(|e| end + c + e + 1))
                .unwrap_or(end);
            continue;
        }
        match name.as_str() {
            "br" => out.push('\n'),
            "div" | "tr" | "li" => out.push(LINE),
            "p" | "ul" | "ol" | "table" | "blockquote" | "hr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                out.push(PARAGRAPH)
            }
            "td" | "th" => out.push(' '),
            _ => {}
        }
        i = end;
    }
    if i < html.len() {
        out.push_str(&html[i..].replace(['\r', '\n'], " "));
    }
    tidy(&decode_entities(&out))
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let decoded = rest[1..].find(';').filter(|&n| n > 0 && n <= 10).and_then(|n| {
            let name = &rest[1..1 + n];
            entity(name).map(|c| (c, n + 2))
        });
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<char> {
    if let Some(num) = name.strip_prefix('#') {
        let code = match num.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => num.parse().ok()?,
        };
        // A non-breaking space reads as a space in plain text.
        return if code == 160 { Some(' ') } else { char::from_u32(code) };
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        "rsquo" => '\u{2019}',
        "lsquo" => '\u{2018}',
        "rdquo" => '\u{201D}',
        "ldquo" => '\u{201C}',
        "mdash" => '\u{2014}',
        "ndash" => '\u{2013}',
        "hellip" => '\u{2026}',
        "copy" => '\u{00A9}',
        "reg" => '\u{00AE}',
        "trade" => '\u{2122}',
        "bull" => '\u{2022}',
        "middot" => '\u{00B7}',
        _ => return None,
    })
}

/// Marks where a block element starts or ends a line.
const LINE: char = '\u{1}';
/// Marks where a paragraph-like element leaves a blank line.
const PARAGRAPH: char = '\u{2}';

/// Turn the break marks into text. Any run of whitespace and marks between
/// two words becomes one space, one line break, or one blank line: a blank
/// line for a paragraph mark or two `<br>`s, a line break for a line mark or
/// one `<br>`. Nothing is added before the first word or after the last.
fn tidy(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    // 0: none, 1: line break, 2: blank line.
    let mut brk = 0u8;
    let mut hard = 0u8;
    let mut space = false;
    for c in s.chars() {
        match c {
            LINE => brk = brk.max(1),
            PARAGRAPH => brk = 2,
            '\n' => {
                hard = hard.saturating_add(1);
                brk = brk.max(hard.min(2));
            }
            c if c.is_whitespace() => space = true,
            c => {
                if !out.is_empty() {
                    match brk {
                        2 => out.push_str("\n\n"),
                        1 => out.push('\n'),
                        _ if space => out.push(' '),
                        _ => {}
                    }
                }
                out.push(c);
                (brk, hard, space) = (0, 0, false);
            }
        }
    }
    out
}

async fn insert(db: &PgPool, rows: &[&EmailRow]) -> Result<usize> {
    if rows.is_empty() {
        return Ok(0);
    }
    let sql = build_batch_insert_query(
        "data_communication_email",
        &[
            "id",
            "message_id",
            "conversation_id",
            "subject",
            "body",
            "body_preview",
            "from_email",
            "from_name",
            "to_emails",
            "to_names",
            "cc_emails",
            "bcc_emails",
            "direction",
            "is_read",
            "is_starred",
            "has_attachments",
            "labels",
            "occurred_at",
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
            .bind(&r.id)
            .bind(&r.message_id)
            .bind(&r.conversation_id)
            .bind(&r.subject)
            .bind(&r.body)
            .bind(&r.body_preview)
            .bind(&r.from_email)
            .bind(&r.from_name)
            .bind(&r.to_emails)
            .bind(&r.to_names)
            .bind(&r.cc_emails)
            .bind(&r.bcc_emails)
            .bind(r.direction)
            .bind(r.is_read)
            .bind(r.is_starred)
            .bind(r.has_attachments)
            .bind(&r.labels)
            .bind(r.occurred_at)
            .bind(&r.source_stream_id)
            .bind(SOURCE_TABLE)
            .bind(SOURCE_PROVIDER)
            .bind(&r.metadata);
    }
    Ok(q.execute(db).await?.rows_affected() as usize)
}

/// Bring stored Outlook messages' read state, flag and labels up to date.
/// Graph's delta reports a message again when any of these change. Rows that
/// already match are not touched, so a message inserted a moment ago counts
/// only once.
async fn update_state(db: &PgPool, rows: &[&EmailRow]) -> Result<u64> {
    if rows.is_empty() {
        return Ok(0);
    }
    let keys: Vec<&str> = rows.iter().map(|r| r.source_stream_id.as_str()).collect();
    let is_read: Vec<bool> = rows.iter().map(|r| r.is_read).collect();
    let is_starred: Vec<bool> = rows.iter().map(|r| r.is_starred).collect();
    let labels: Vec<String> = rows.iter().map(|r| r.labels.to_string()).collect();
    let res = sqlx::query(
        "UPDATE data_communication_email e \
         SET is_read = t.is_read, is_starred = t.is_starred, labels = t.labels::jsonb \
         FROM UNNEST($1::text[], $2::bool[], $3::bool[], $4::text[]) AS t(key, is_read, is_starred, labels) \
         WHERE e.source_stream_id = t.key AND e.source_table = 'outlook_mail' \
           AND (e.is_read IS DISTINCT FROM t.is_read OR e.is_starred IS DISTINCT FROM t.is_starred \
                OR e.labels IS DISTINCT FROM t.labels::jsonb)",
    )
    .bind(&keys)
    .bind(&is_read)
    .bind(&is_starred)
    .bind(&labels)
    .execute(db)
    .await?;
    Ok(res.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn message() -> Value {
        json!({
            "id": "AAMkAGraphId1",
            "internetMessageId": "<lunch-plans-1@example.com>",
            "conversationId": "AAQkConv1",
            "subject": "Lunch on Thursday?",
            "bodyPreview": "Are you free at noon?",
            "body": { "contentType": "text", "content": "Are you free at noon?\r\n\r\nNick\r\n" },
            "from": { "emailAddress": { "name": "Nick", "address": "nick@example.com" } },
            "toRecipients": [
                { "emailAddress": { "name": "David Okafor", "address": "david@example.com" } },
                { "emailAddress": { "name": "", "address": "team@example.com" } }
            ],
            "ccRecipients": [ { "emailAddress": { "name": "Ana", "address": "ana@example.com" } } ],
            "bccRecipients": [],
            "isRead": false,
            "flag": { "flagStatus": "flagged" },
            "hasAttachments": true,
            "categories": ["Friends"],
            "receivedDateTime": "2026-11-19T17:02:11Z",
            "sentDateTime": "2026-11-19T17:02:05Z",
            "isDraft": false
        })
    }

    #[test]
    fn a_graph_message_becomes_an_email_row() {
        let row = shape_one(&message(), "david@example.com", "inbox").unwrap();
        assert_eq!(row.source_stream_id, "outlook:david@example.com:<lunch-plans-1@example.com>");
        assert_eq!(row.message_id, "<lunch-plans-1@example.com>");
        assert_eq!(row.conversation_id.as_deref(), Some("AAQkConv1"));
        assert_eq!(row.subject.as_deref(), Some("Lunch on Thursday?"));
        assert_eq!(row.body.as_deref(), Some("Are you free at noon?\r\n\r\nNick"));
        assert_eq!(row.from_email, "nick@example.com");
        assert_eq!(row.from_name.as_deref(), Some("Nick"));
        assert_eq!(row.to_emails, json!(["david@example.com", "team@example.com"]));
        assert_eq!(row.to_names, json!(["David Okafor", null]));
        assert_eq!(row.cc_emails, json!(["ana@example.com"]));
        assert_eq!(row.direction, "received");
        assert!(!row.is_read);
        assert!(row.is_starred);
        assert!(row.has_attachments);
        assert_eq!(row.labels, json!(["Friends", "inbox"]));
        assert_eq!(row.occurred_at, "2026-11-19T17:02:11Z".parse::<DateTime<Utc>>().unwrap());
        assert_eq!(row.metadata["graph_id"], "AAMkAGraphId1");
        // The id is stable for the message, so a move does not make a second row.
        let mut moved = message();
        moved["id"] = json!("AAMkNewId");
        let moved = shape_one(&moved, "david@example.com", "archive").unwrap();
        assert_eq!(moved.id, row.id);
        assert_eq!(moved.labels, json!(["Friends", "archive"]));
    }

    #[test]
    fn sent_mail_is_known_by_the_folder_or_the_owner_address() {
        let from_owner = shape_one(&message(), "NICK@example.com", "archive").unwrap();
        assert_eq!(from_owner.direction, "sent");
        assert_eq!(from_owner.occurred_at, "2026-11-19T17:02:05Z".parse::<DateTime<Utc>>().unwrap());
        let in_sent = shape_one(&message(), "", SENT_FOLDER).unwrap();
        assert_eq!(in_sent.direction, "sent");
    }

    #[test]
    fn removals_drafts_and_messages_without_a_time_are_skipped() {
        let removed = json!({ "id": "AAMkGone", "@removed": { "reason": "changed" } });
        assert!(shape_one(&removed, "david@example.com", "inbox").is_none());
        let mut draft = message();
        draft["isDraft"] = json!(true);
        assert!(shape_one(&draft, "david@example.com", "inbox").is_none());
        let mut timeless = message();
        timeless.as_object_mut().unwrap().remove("receivedDateTime");
        timeless.as_object_mut().unwrap().remove("sentDateTime");
        assert!(shape_one(&timeless, "david@example.com", "inbox").is_none());
    }

    #[test]
    fn graph_id_is_the_key_when_there_is_no_internet_message_id() {
        let mut m = message();
        m.as_object_mut().unwrap().remove("internetMessageId");
        let row = shape_one(&m, "david@example.com", "inbox").unwrap();
        assert_eq!(row.source_stream_id, "outlook:david@example.com:AAMkAGraphId1");
        assert_eq!(row.message_id, "AAMkAGraphId1");
    }

    #[test]
    fn the_key_names_the_mailbox_in_lowercase() {
        let a = shape_one(&message(), "David@Example.com", "inbox").unwrap();
        let b = shape_one(&message(), "team@example.com", "inbox").unwrap();
        assert_eq!(a.source_stream_id, "outlook:david@example.com:<lunch-plans-1@example.com>");
        assert_ne!(a.source_stream_id, b.source_stream_id);
        assert_ne!(a.id, b.id);
        assert_eq!(a.message_id, b.message_id);
    }

    #[test]
    fn an_html_body_is_stored_as_text() {
        let mut m = message();
        m["body"] = json!({
            "contentType": "html",
            "content": "<html><head><style>p { color: red; }</style><title>x</title></head>\r\n<body><div>Hi David,</div>\r\n<p>The plan for <b>Thursday</b>:<br>noon &amp; the café?</p><!-- tracking --><ul><li>Bring&nbsp;the map</li><li>Tom&#39;s idea &#x2014; bikes</li></ul></body></html>"
        });
        let row = shape_one(&m, "david@example.com", "inbox").unwrap();
        assert_eq!(
            row.body.as_deref(),
            Some("Hi David,\n\nThe plan for Thursday:\nnoon & the café?\n\nBring the map\nTom's idea \u{2014} bikes")
        );
    }

    /// A message seen twice in one page and then again in another folder is
    /// stored once, and the later sighting updates its read state and labels.
    #[sqlx::test(migrations = "../virtues-core/migrations")]
    async fn a_message_seen_again_is_updated_not_stored_twice(pool: PgPool) {
        let w = write_messages(&pool, "david@example.com", "inbox", &[message(), message()]).await.unwrap();
        assert_eq!((w.added, w.changed), (1, 0));

        let mut moved = message();
        moved["id"] = json!("AAMkNewId");
        moved["isRead"] = json!(true);
        let w = write_messages(&pool, "david@example.com", "archive", &[moved.clone()]).await.unwrap();
        assert_eq!((w.added, w.changed), (0, 1));
        let w = write_messages(&pool, "david@example.com", "archive", &[moved]).await.unwrap();
        assert_eq!((w.added, w.changed), (0, 0), "nothing changed the second time");

        let got: Vec<(bool, Value, String, String)> = sqlx::query_as(
            "SELECT is_read, labels, source_table, source_provider FROM data_communication_email",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(got, vec![(true, json!(["Friends", "archive"]), "outlook_mail".into(), "outlook".into())]);
    }

    /// Two connected mailboxes that both received one message keep a row
    /// each, and one mailbox reading it leaves the other's row unread.
    #[sqlx::test(migrations = "../virtues-core/migrations")]
    async fn two_mailboxes_keep_their_own_copy_of_one_message(pool: PgPool) {
        let w = write_messages(&pool, "david@example.com", "inbox", &[message()]).await.unwrap();
        assert_eq!((w.added, w.changed), (1, 0));
        let w = write_messages(&pool, "team@example.com", "inbox", &[message()]).await.unwrap();
        assert_eq!((w.added, w.changed), (1, 0));

        let mut read = message();
        read["isRead"] = json!(true);
        let w = write_messages(&pool, "team@example.com", "inbox", &[read]).await.unwrap();
        assert_eq!((w.added, w.changed), (0, 1));

        let got: Vec<(String, bool)> =
            sqlx::query_as("SELECT source_stream_id, is_read FROM data_communication_email ORDER BY source_stream_id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            got,
            vec![
                ("outlook:david@example.com:<lunch-plans-1@example.com>".into(), false),
                ("outlook:team@example.com:<lunch-plans-1@example.com>".into(), true),
            ]
        );
    }

    #[test]
    fn html_to_text_keeps_paragraphs_and_drops_markup() {
        assert_eq!(html_to_text("<p>One</p>\n\n<p>Two</p>"), "One\n\nTwo");
        assert_eq!(html_to_text("<div>A</div><div>B</div>"), "A\nB");
        assert_eq!(html_to_text("<br><p>Lead</p><br>"), "Lead");
        assert_eq!(html_to_text("a<br/><br/><br/>b"), "a\n\nb");
        assert_eq!(html_to_text("<script>alert(1)</script>Kept &lt;tag&gt;"), "Kept <tag>");
        assert_eq!(html_to_text("<STYLE type=x>a{}</STYLE>Upper"), "Upper");
        assert_eq!(html_to_text("<td>a</td><td>b</td>"), "a b");
        assert_eq!(html_to_text("unterminated <b"), "unterminated");
        assert_eq!(html_to_text("AT&T and 5 &lt 6"), "AT&T and 5 &lt 6");
    }
}

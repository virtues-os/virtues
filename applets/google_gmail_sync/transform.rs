//! Gmail messages → `data_communication_email` transform.
//!
//! Adapted from `core/src/sources/google/gmail/transform.rs`. The fetch path
//! is in `main.rs` (list message ids → batch fetch full messages); this
//! module just shapes a fully-fetched message into the email row.

use anyhow::Result;
use base64::Engine;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;
use virtues_helpers::dedup::{build_batch_insert_query, BATCH_SIZE};

#[allow(clippy::type_complexity)]
type EmailRow = (
    String,                 // id
    String,                 // message_id
    Option<String>,         // thread_id
    Option<String>,         // subject
    Option<String>,         // body
    Option<String>,         // body_preview
    String,                 // from_email
    Option<String>,         // from_name
    Value,                  // to_emails (JSONB)
    Value,                  // to_names (JSONB)
    Value,                  // cc_emails (JSONB)
    Value,                  // bcc_emails (JSONB)
    String,                 // direction
    bool,                   // is_read
    bool,                   // is_starred
    bool,                   // has_attachments
    Value,                  // labels (JSONB)
    DateTime<Utc>,          // timestamp
    String,                 // source_stream_id
    Value,                  // metadata
);

pub async fn write_messages(
    db: &PgPool,
    user_email: &str,
    messages: &[Value],
) -> Result<usize> {
    let mut pending: Vec<EmailRow> = Vec::new();
    let mut written = 0;

    for msg in messages {
        let Some(row) = shape_one(msg, user_email) else {
            continue;
        };
        pending.push(row);
        if pending.len() >= BATCH_SIZE {
            written += flush(db, &pending).await?;
            pending.clear();
        }
    }
    if !pending.is_empty() {
        written += flush(db, &pending).await?;
    }
    Ok(written)
}

fn shape_one(msg: &Value, user_email: &str) -> Option<EmailRow> {
    let gid = msg.get("id").and_then(|v| v.as_str())?;
    let thread_id = msg.get("threadId").and_then(|v| v.as_str()).map(String::from);

    let payload = msg.get("payload")?;
    let headers = payload
        .get("headers")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let header = |name: &str| -> Option<String> {
        headers.iter().find_map(|h| {
            if h.get("name").and_then(|v| v.as_str()).map(|s| s.eq_ignore_ascii_case(name))
                == Some(true)
            {
                h.get("value").and_then(|v| v.as_str()).map(String::from)
            } else {
                None
            }
        })
    };

    let subject = header("Subject");
    let from_raw = header("From").unwrap_or_default();
    let to_raw = header("To").unwrap_or_default();
    let cc_raw = header("Cc").unwrap_or_default();
    let bcc_raw = header("Bcc").unwrap_or_default();

    let (from_email, from_name) = parse_address(&from_raw);
    let (to_emails, to_names) = parse_address_list(&to_raw);
    let (cc_emails, _) = parse_address_list(&cc_raw);
    let (bcc_emails, _) = parse_address_list(&bcc_raw);

    let direction = if from_email.eq_ignore_ascii_case(user_email) {
        "sent"
    } else {
        "received"
    };

    let label_ids: Vec<String> = msg
        .get("labelIds")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let is_read = !label_ids.iter().any(|l| l == "UNREAD");
    let is_starred = label_ids.iter().any(|l| l == "STARRED");

    let body = extract_body(payload);
    let body_preview = msg
        .get("snippet")
        .and_then(|v| v.as_str())
        .map(String::from);

    let has_attachments = has_attachments(payload);

    let timestamp = msg
        .get("internalDate")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok())
        .and_then(|ms| chrono::DateTime::<Utc>::from_timestamp_millis(ms))
        .unwrap_or_else(Utc::now);

    let metadata = serde_json::json!({
        "gmail_id": gid,
        "thread_id": thread_id,
        "label_ids": label_ids,
        "size_estimate": msg.get("sizeEstimate"),
    });

    let id = Uuid::new_v5(
        &Uuid::NAMESPACE_OID,
        format!("google:gmail:{gid}").as_bytes(),
    )
    .to_string();

    Some((
        id,
        gid.to_string(),
        thread_id,
        subject,
        body,
        body_preview,
        from_email,
        from_name,
        serde_json::json!(to_emails),
        serde_json::json!(to_names),
        serde_json::json!(cc_emails),
        serde_json::json!(bcc_emails),
        direction.to_string(),
        is_read,
        is_starred,
        has_attachments,
        serde_json::json!(label_ids),
        timestamp,
        gid.to_string(),
        metadata,
    ))
}

fn parse_address(raw: &str) -> (String, Option<String>) {
    // Patterns: `"Name" <email@x>`, `Name <email@x>`, or just `email@x`.
    let raw = raw.trim();
    if let (Some(lt), Some(gt)) = (raw.find('<'), raw.rfind('>')) {
        if gt > lt {
            let email = raw[lt + 1..gt].trim().to_string();
            let name = raw[..lt].trim().trim_matches('"').trim();
            let name = if name.is_empty() { None } else { Some(name.to_string()) };
            return (email, name);
        }
    }
    (raw.to_string(), None)
}

fn parse_address_list(raw: &str) -> (Vec<String>, Vec<Option<String>>) {
    let mut emails = Vec::new();
    let mut names = Vec::new();
    for part in raw.split(',') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        let (e, n) = parse_address(p);
        emails.push(e);
        names.push(n);
    }
    (emails, names)
}

fn extract_body(payload: &Value) -> Option<String> {
    if let Some(b) = payload
        .get("body")
        .and_then(|b| b.get("data"))
        .and_then(|v| v.as_str())
    {
        return decode_b64url(b);
    }
    if let Some(parts) = payload.get("parts").and_then(|v| v.as_array()) {
        for p in parts {
            let mime = p.get("mimeType").and_then(|v| v.as_str()).unwrap_or("");
            if mime == "text/plain" {
                if let Some(d) = p.get("body").and_then(|b| b.get("data")).and_then(|v| v.as_str()) {
                    if let Some(decoded) = decode_b64url(d) {
                        return Some(decoded);
                    }
                }
            }
        }
        // Recurse one level for multipart/alternative.
        for p in parts {
            if let Some(b) = extract_body(p) {
                return Some(b);
            }
        }
    }
    None
}

fn decode_b64url(input: &str) -> Option<String> {
    base64::engine::general_purpose::URL_SAFE
        .decode(input)
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
}

fn has_attachments(payload: &Value) -> bool {
    if payload
        .get("filename")
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false)
    {
        return true;
    }
    if let Some(parts) = payload.get("parts").and_then(|v| v.as_array()) {
        for p in parts {
            if has_attachments(p) {
                return true;
            }
        }
    }
    false
}

async fn flush(db: &PgPool, records: &[EmailRow]) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }
    let sql = build_batch_insert_query(
        "data_communication_email",
        &[
            "id",
            "message_id",
            "thread_id",
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
            "timestamp",
            "source_stream_id",
            "source_table",
            "source_provider",
            "metadata",
        ],
        "source_stream_id",
        records.len(),
    );

    let mut q = sqlx::query(&sql);
    for r in records {
        q = q
            .bind(&r.0)
            .bind(&r.1)
            .bind(&r.2)
            .bind(&r.3)
            .bind(&r.4)
            .bind(&r.5)
            .bind(&r.6)
            .bind(&r.7)
            .bind(&r.8)
            .bind(&r.9)
            .bind(&r.10)
            .bind(&r.11)
            .bind(&r.12)
            .bind(r.13)
            .bind(r.14)
            .bind(r.15)
            .bind(&r.16)
            .bind(r.17)
            .bind(&r.18)
            .bind("google_gmail")
            .bind("google")
            .bind(&r.19);
    }
    let result = q.execute(db).await?;
    Ok(result.rows_affected() as usize)
}

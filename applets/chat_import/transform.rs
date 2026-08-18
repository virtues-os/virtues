//! Parsers for the three common chat-export shapes → `data_content_conversation`.
//!
//! Each parser is defensive: it navigates `serde_json::Value` and skips entries
//! it can't read rather than failing the whole import (exports drift over time
//! and partial success beats none). Unknown providers fall back to trying the
//! ChatGPT shape, then the Claude shape.

use anyhow::Result;
use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;
use sqlx::PgPool;
use virtues_helpers::dedup::{build_batch_insert_query, BATCH_SIZE};

/// One normalized conversation message ready for insert.
pub struct Msg {
    pub stream_id: String,
    pub conversation_id: String,
    pub message_id: String,
    pub role: String,
    pub content: String,
    pub provider: String,
    pub timestamp: DateTime<Utc>,
}

/// Map a provider's sender label to the ontology's role CHECK
/// (`user` | `assistant` | `system`). Returns `None` for roles we drop
/// (e.g. ChatGPT `tool`), which the caller skips.
fn normalize_role(raw: &str) -> Option<&'static str> {
    match raw.to_ascii_lowercase().as_str() {
        "user" | "human" => Some("user"),
        "assistant" | "model" | "bot" => Some("assistant"),
        "system" => Some("system"),
        _ => None,
    }
}

fn stream_id(provider: &str, conversation_id: &str, message_id: &str) -> String {
    format!("chatimport:{provider}:{conversation_id}:{message_id}")
}

pub fn parse(provider: &str, json: &Value) -> Vec<Msg> {
    match provider {
        "chatgpt" => parse_chatgpt(json),
        "claude" => parse_claude(json),
        "gemini" => parse_gemini(json),
        _ => {
            // Best-effort: try the ChatGPT mapping shape, then Claude.
            let mut m = parse_chatgpt(json);
            if m.is_empty() {
                m = parse_claude(json);
            }
            m
        }
    }
}

/// Claude `conversations.json`: an array of conversations, each with `uuid`,
/// `name`, and `chat_messages[]` whose entries carry `uuid`, `sender`
/// (`human`/`assistant`), `created_at` (RFC3339), and `text` (or `content[]`).
fn parse_claude(json: &Value) -> Vec<Msg> {
    let mut out = Vec::new();
    let Some(convos) = json.as_array() else {
        return out;
    };
    for convo in convos {
        let conversation_id = first_str(convo, &["uuid", "conversation_id", "id"]);
        let Some(conversation_id) = conversation_id else {
            continue;
        };
        let Some(msgs) = convo.get("chat_messages").and_then(|v| v.as_array()) else {
            continue;
        };
        for m in msgs {
            let Some(role) = m
                .get("sender")
                .and_then(|v| v.as_str())
                .and_then(normalize_role)
            else {
                continue;
            };
            let message_id = first_str(m, &["uuid", "id"]).unwrap_or_else(|| {
                format!("{conversation_id}:{}", out.len())
            });
            let content = m
                .get("text")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty())
                .or_else(|| join_content_parts(m.get("content")))
                .unwrap_or_default();
            if content.is_empty() {
                continue;
            }
            let timestamp = parse_ts(m.get("created_at"));
            out.push(Msg {
                stream_id: stream_id("claude", &conversation_id, &message_id),
                conversation_id: conversation_id.clone(),
                message_id,
                role: role.to_string(),
                content,
                provider: "claude".into(),
                timestamp,
            });
        }
    }
    out
}

/// ChatGPT `conversations.json`: an array of conversations, each with a
/// `mapping` object of nodes; each node's `message` has `id`, `author.role`,
/// `create_time` (epoch seconds), and `content.parts[]`.
fn parse_chatgpt(json: &Value) -> Vec<Msg> {
    let mut out = Vec::new();
    let Some(convos) = json.as_array() else {
        return out;
    };
    for convo in convos {
        let conversation_id =
            first_str(convo, &["conversation_id", "id"]).unwrap_or_else(|| "unknown".into());
        let Some(mapping) = convo.get("mapping").and_then(|v| v.as_object()) else {
            continue;
        };
        for node in mapping.values() {
            let Some(message) = node.get("message").filter(|m| !m.is_null()) else {
                continue;
            };
            let Some(role) = message
                .get("author")
                .and_then(|a| a.get("role"))
                .and_then(|v| v.as_str())
                .and_then(normalize_role)
            else {
                continue;
            };
            let message_id = first_str(message, &["id"])
                .unwrap_or_else(|| format!("{conversation_id}:{}", out.len()));
            let content = message
                .get("content")
                .and_then(|c| c.get("parts"))
                .and_then(|p| p.as_array())
                .map(|parts| {
                    parts
                        .iter()
                        .filter_map(|p| p.as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default();
            if content.trim().is_empty() {
                continue;
            }
            let timestamp = parse_ts(message.get("create_time"));
            out.push(Msg {
                stream_id: stream_id("chatgpt", &conversation_id, &message_id),
                conversation_id: conversation_id.clone(),
                message_id,
                role: role.to_string(),
                content,
                provider: "chatgpt".into(),
                timestamp,
            });
        }
    }
    out
}

/// Gemini / Google Takeout exports vary widely (and are often HTML). Handle the
/// JSON case best-effort: an array of records with a text + role + timestamp.
fn parse_gemini(json: &Value) -> Vec<Msg> {
    let mut out = Vec::new();
    let Some(items) = json.as_array() else {
        return out;
    };
    for (i, item) in items.iter().enumerate() {
        let role = item
            .get("role")
            .and_then(|v| v.as_str())
            .and_then(normalize_role)
            .unwrap_or("user");
        let content = first_str(item, &["text", "content", "message"]).unwrap_or_default();
        if content.trim().is_empty() {
            continue;
        }
        let conversation_id = first_str(item, &["conversation_id", "thread_id"])
            .unwrap_or_else(|| "gemini".into());
        let message_id = first_str(item, &["id", "message_id"]).unwrap_or_else(|| i.to_string());
        let timestamp = parse_ts(item.get("timestamp").or_else(|| item.get("time")));
        out.push(Msg {
            stream_id: stream_id("gemini", &conversation_id, &message_id),
            conversation_id,
            message_id,
            role: role.to_string(),
            content,
            provider: "gemini".into(),
            timestamp,
        });
    }
    out
}

// ── helpers ──────────────────────────────────────────────────────────────

fn first_str(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| v.get(*k).and_then(|x| x.as_str()))
        .map(|s| s.to_string())
}

/// Join a Claude `content: [{type:"text", text:"..."}]` array into one string.
fn join_content_parts(v: Option<&Value>) -> Option<String> {
    let arr = v?.as_array()?;
    let joined = arr
        .iter()
        .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
        .collect::<Vec<_>>()
        .join("\n");
    (!joined.is_empty()).then_some(joined)
}

/// Parse a timestamp that may be an RFC3339 string or an epoch-seconds number.
fn parse_ts(v: Option<&Value>) -> DateTime<Utc> {
    match v {
        Some(Value::String(s)) => DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        Some(Value::Number(n)) => n
            .as_f64()
            .and_then(|secs| Utc.timestamp_opt(secs as i64, 0).single())
            .unwrap_or_else(Utc::now),
        _ => Utc::now(),
    }
}

/// Dedup-insert messages into `data_content_conversation` in batches.
pub async fn write_messages(db: &PgPool, messages: &[Msg]) -> Result<usize> {
    let mut written = 0usize;
    for chunk in messages.chunks(BATCH_SIZE) {
        written += flush(db, chunk).await?;
    }
    Ok(written)
}

async fn flush(db: &PgPool, records: &[Msg]) -> Result<usize> {
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
            .bind(&r.stream_id) // id (reuse the deterministic stream id as PK)
            .bind(&r.conversation_id)
            .bind(&r.message_id)
            .bind(&r.role)
            .bind(&r.content)
            .bind(&r.provider)
            .bind(r.timestamp)
            .bind(&r.stream_id)
            .bind("chat_import")
            .bind(&r.provider);
    }
    let result = q.execute(db).await?;
    Ok(result.rows_affected() as usize)
}

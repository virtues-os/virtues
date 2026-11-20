//! Parsers for chat exports → `data_content_conversation`.
//!
//! The export is read one conversation at a time (see `main.rs`), so every
//! parser here takes a single top-level element of the export's array. The
//! provider comes from the element's shape: a ChatGPT conversation carries a
//! `mapping` of nodes, a Claude one carries `chat_messages`. The upload's
//! provider hint is used only for a shape neither of those matches.
//!
//! Each parser is defensive: it skips messages it can't read rather than
//! failing the import, because exports drift over time. A message with no
//! readable time of its own takes its conversation's time, and is skipped
//! when that is missing too: the wall clock would land it on the wrong day.

use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;
use std::collections::HashSet;
use virtues_helpers::conversations::{stream_id, ConversationMsg};

/// The `source_table` every imported row carries.
const SOURCE_TABLE: &str = "chat_import";

/// One parsed message as a row, with the fields every import shares.
fn msg(
    provider: &str,
    conversation_id: &str,
    message_id: String,
    role: &str,
    content: String,
    timestamp: DateTime<Utc>,
) -> ConversationMsg {
    ConversationMsg {
        stream_id: stream_id(provider, conversation_id, &message_id),
        conversation_id: conversation_id.to_string(),
        message_id,
        role: role.to_string(),
        content,
        provider: provider.to_string(),
        timestamp,
        source_table: SOURCE_TABLE.to_string(),
    }
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

/// The provider an element's shape belongs to, if it has a known one.
pub fn detect(element: &Value) -> Option<&'static str> {
    if element.get("mapping").is_some_and(Value::is_object) {
        Some("chatgpt")
    } else if element.get("chat_messages").is_some_and(Value::is_array) {
        Some("claude")
    } else {
        None
    }
}

/// Parse one top-level element of an export. `index` is the element's place
/// in the export, used only by Gemini records that carry no id of their own.
pub fn parse_element(hint: Option<&str>, index: usize, element: &Value) -> Vec<ConversationMsg> {
    match detect(element).or(hint) {
        Some("chatgpt") => parse_chatgpt(element),
        Some("claude") => parse_claude(element),
        Some("gemini") => parse_gemini(index, element).into_iter().collect(),
        _ => Vec::new(),
    }
}

/// A Claude conversation: `uuid`, `created_at`, and `chat_messages[]` whose
/// entries carry `uuid`, `sender` (`human`/`assistant`), `created_at`
/// (RFC3339), and `text` (or `content[]`).
fn parse_claude(convo: &Value) -> Vec<ConversationMsg> {
    let mut out = Vec::new();
    let Some(conversation_id) = first_str(convo, &["uuid", "conversation_id", "id"]) else {
        return out;
    };
    let convo_ts = parse_ts(convo.get("created_at"));
    let Some(msgs) = convo.get("chat_messages").and_then(Value::as_array) else {
        return out;
    };
    for (i, m) in msgs.iter().enumerate() {
        let Some(role) = m.get("sender").and_then(Value::as_str).and_then(normalize_role) else {
            continue;
        };
        let content = m
            .get("text")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .or_else(|| join_text_parts(m.get("content")))
            .unwrap_or_default();
        if content.trim().is_empty() {
            continue;
        }
        let Some(timestamp) = parse_ts(m.get("created_at")).or(convo_ts) else {
            continue;
        };
        let message_id =
            first_str(m, &["uuid", "id"]).unwrap_or_else(|| format!("{conversation_id}:{i}"));
        out.push(msg("claude", &conversation_id, message_id, role, content, timestamp));
    }
    out
}

/// A ChatGPT conversation: `id`, `create_time`, `current_node`, and a
/// `mapping` of nodes (`parent`, `message`). The mapping holds every branch,
/// edits and regenerations included; the version the person sees is the
/// chain from `current_node` back through `parent`. Only that chain is kept.
/// A conversation without `current_node` keeps every node.
fn parse_chatgpt(convo: &Value) -> Vec<ConversationMsg> {
    let mut out = Vec::new();
    let Some(conversation_id) = first_str(convo, &["conversation_id", "id"]) else {
        return out;
    };
    let Some(mapping) = convo.get("mapping").and_then(Value::as_object) else {
        return out;
    };
    let convo_ts = parse_ts(convo.get("create_time"));

    let mut nodes: Vec<&Value> = Vec::new();
    match convo.get("current_node").and_then(Value::as_str) {
        Some(current) => {
            // Walk back to the root; the `seen` set stops a malformed cycle.
            let mut seen = HashSet::new();
            let mut cursor = Some(current.to_string());
            while let Some(id) = cursor {
                if !seen.insert(id.clone()) {
                    break;
                }
                let Some(node) = mapping.get(&id) else { break };
                nodes.push(node);
                cursor = node.get("parent").and_then(Value::as_str).map(str::to_string);
            }
            nodes.reverse();
        }
        None => nodes.extend(mapping.values()),
    }

    for node in nodes {
        let Some(message) = node.get("message").filter(|m| !m.is_null()) else {
            continue;
        };
        let Some(role) = message
            .get("author")
            .and_then(|a| a.get("role"))
            .and_then(Value::as_str)
            .and_then(normalize_role)
        else {
            continue;
        };
        // `text` and `multimodal_text` are what was said. `code`, `thoughts`,
        // `reasoning_recap` and the custom-instructions context are dropped.
        let content_type = message
            .get("content")
            .and_then(|c| c.get("content_type"))
            .and_then(Value::as_str)
            .unwrap_or("text");
        if !matches!(content_type, "text" | "multimodal_text") {
            continue;
        }
        let content = message
            .get("content")
            .and_then(|c| c.get("parts"))
            .and_then(Value::as_array)
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        if content.trim().is_empty() {
            continue;
        }
        let Some(timestamp) = parse_ts(message.get("create_time")).or(convo_ts) else {
            continue;
        };
        let Some(message_id) = first_str(message, &["id"]) else {
            continue;
        };
        out.push(msg("chatgpt", &conversation_id, message_id, role, content, timestamp));
    }
    out
}

/// A Gemini record: text + role + timestamp. The shape is a guess and has not
/// been checked against a real Google Takeout export.
fn parse_gemini(index: usize, item: &Value) -> Option<ConversationMsg> {
    let role = item
        .get("role")
        .and_then(Value::as_str)
        .and_then(normalize_role)
        .unwrap_or("user");
    let content = first_str(item, &["text", "content", "message"])?;
    if content.trim().is_empty() {
        return None;
    }
    let timestamp = parse_ts(item.get("timestamp").or_else(|| item.get("time")))?;
    let conversation_id =
        first_str(item, &["conversation_id", "thread_id"]).unwrap_or_else(|| "gemini".into());
    let message_id = first_str(item, &["id", "message_id"]).unwrap_or_else(|| index.to_string());
    Some(msg("gemini", &conversation_id, message_id, role, content, timestamp))
}

// ── helpers ──────────────────────────────────────────────────────────────

fn first_str(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| v.get(*k).and_then(Value::as_str))
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Join the `text` parts of a Claude `content: [{type:"text", text:"..."}]`
/// array. Thinking and tool parts are left out.
fn join_text_parts(v: Option<&Value>) -> Option<String> {
    let arr = v?.as_array()?;
    let joined = arr
        .iter()
        .filter(|p| p.get("type").and_then(Value::as_str).is_none_or(|t| t == "text"))
        .filter_map(|p| p.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    (!joined.trim().is_empty()).then_some(joined)
}

/// Parse a timestamp that may be an RFC3339 string or an epoch-seconds number.
fn parse_ts(v: Option<&Value>) -> Option<DateTime<Utc>> {
    match v? {
        Value::String(s) => DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc)),
        Value::Number(n) => {
            let secs = n.as_f64()?;
            let millis = (secs * 1000.0).round() as i64;
            Utc.timestamp_millis_opt(millis).single()
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn chatgpt_keeps_only_the_branch_on_screen() {
        // "a2" was regenerated as "a2b"; current_node points down the b branch.
        let convo = json!({
            "id": "c-1",
            "create_time": 1_700_000_000.0,
            "current_node": "a2b",
            "mapping": {
                "root": { "parent": null, "message": null },
                "u1": { "parent": "root", "message": {
                    "id": "u1", "author": { "role": "user" }, "create_time": 1_700_000_001.5,
                    "content": { "content_type": "text", "parts": ["How do I fix a squeaky door?"] } } },
                "a2": { "parent": "u1", "message": {
                    "id": "a2", "author": { "role": "assistant" }, "create_time": 1_700_000_002.0,
                    "content": { "content_type": "text", "parts": ["First answer"] } } },
                "a2b": { "parent": "u1", "message": {
                    "id": "a2b", "author": { "role": "assistant" }, "create_time": 1_700_000_003.0,
                    "content": { "content_type": "text", "parts": ["Oil the hinge."] } } }
            }
        });
        assert_eq!(detect(&convo), Some("chatgpt"));
        let rows = parse_element(None, 0, &convo);
        let ids: Vec<_> = rows.iter().map(|r| r.message_id.as_str()).collect();
        assert_eq!(ids, ["u1", "a2b"]);
        assert_eq!(rows[0].stream_id, "chatimport:chatgpt:c-1:u1");
        assert_eq!(rows[0].timestamp.timestamp_millis(), 1_700_000_001_500);
    }

    #[test]
    fn chatgpt_drops_thoughts_and_tool_output() {
        let convo = json!({
            "id": "c-2",
            "current_node": "t",
            "mapping": {
                "u": { "parent": null, "message": {
                    "id": "u", "author": { "role": "user" }, "create_time": 1_700_000_000,
                    "content": { "content_type": "text", "parts": ["hi"] } } },
                "th": { "parent": "u", "message": {
                    "id": "th", "author": { "role": "assistant" }, "create_time": 1_700_000_001,
                    "content": { "content_type": "thoughts", "thoughts": [] } } },
                "t": { "parent": "th", "message": {
                    "id": "t", "author": { "role": "tool" }, "create_time": 1_700_000_002,
                    "content": { "content_type": "text", "parts": ["result"] } } }
            }
        });
        let rows = parse_element(None, 0, &convo);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].content, "hi");
    }

    #[test]
    fn claude_reads_text_or_text_parts_and_falls_back_to_the_conversation_time() {
        let convo = json!({
            "uuid": "k-1",
            "created_at": "2024-05-01T10:00:00Z",
            "chat_messages": [
                { "uuid": "m1", "sender": "human", "text": "Plan a picnic", "created_at": "2024-05-01T10:00:05Z" },
                { "uuid": "m2", "sender": "assistant", "text": "", "content": [
                    { "type": "thinking", "thinking": "hidden" },
                    { "type": "text", "text": "Bring a blanket." }
                ] }
            ]
        });
        assert_eq!(detect(&convo), Some("claude"));
        let rows = parse_element(Some("chatgpt"), 0, &convo);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].role, "user");
        assert_eq!(rows[1].content, "Bring a blanket.");
        assert_eq!(rows[1].timestamp.to_rfc3339(), "2024-05-01T10:00:00+00:00");
    }

    #[test]
    fn a_message_with_no_time_anywhere_is_skipped() {
        let convo = json!({
            "uuid": "k-2",
            "chat_messages": [ { "uuid": "m1", "sender": "human", "text": "hello" } ]
        });
        assert!(parse_element(None, 0, &convo).is_empty());
    }

    #[test]
    fn an_unknown_shape_without_a_hint_parses_nothing() {
        assert!(parse_element(None, 0, &json!({ "title": "x" })).is_empty());
    }
}

//! Freezing a chat for sharing: the visible conversation, read-only, as one
//! standalone HTML file the door serves (`api::publications`).
//!
//! A chat carries far more of the owner's record than it shows: tool calls
//! and their raw results (database rows, search hits over messages and mail),
//! the model's reasoning, citations into records, attachments. None of that
//! leaves. What does:
//!
//! - **The words**: each user and assistant message's text parts, in order,
//!   rendered from Markdown. The rows the transcript hides (the onboarding
//!   trigger) stay hidden here too.
//! - **Names, not records**, as on a page (`api::publish_page`).
//!
//! Attachments are left out and counted, so the Share modal can say so.

use sqlx::{PgPool, Row};

use crate::api::publish_page::{escape, render, strip_internal_links};
use crate::error::{Error, Result};

pub struct FrozenChat {
    pub title: String,
    pub html: String,
    pub names: Vec<String>,
    pub message_count: usize,
    pub attachments_left_out: usize,
}

/// One message as it is shared: who said it and the words.
struct Said {
    role: String,
    text: String,
}

/// The text a message shows, and how many attachments it carries. Messages
/// written before `parts` existed fall back to their `content`.
fn said(role: &str, content: &str, parts: Option<&serde_json::Value>) -> (String, usize) {
    let Some(parts) = parts.and_then(|p| p.as_array()) else {
        return (content.to_string(), 0);
    };
    let mut text = Vec::new();
    let mut files = 0;
    for part in parts {
        match part.get("type").and_then(|t| t.as_str()) {
            Some("text") => {
                if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                    text.push(t.to_string());
                }
            }
            Some("file") => files += 1,
            // Reasoning, tool calls and their results, sources: kept.
            _ => {}
        }
    }
    if text.is_empty() && files == 0 && role == "user" {
        return (content.to_string(), 0);
    }
    (text.join("\n\n"), files)
}

const CHAT_CSS: &str = r#"
:root { color-scheme: light dark; --fg: #1d1f1e; --muted: #6a706d; --bg: #fdfcfa; --line: #e6e3dd; --you: #efece6; --code: #f2f0ec; }
@media (prefers-color-scheme: dark) { :root { --fg: #e9ebe9; --muted: #9aa19e; --bg: #151716; --line: #2c302e; --you: #222624; --code: #1f2321; } }
html, body { margin: 0; background: var(--bg); color: var(--fg); }
body { font: 17px/1.6 -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif; }
main { max-width: 720px; margin: 0 auto; padding: 32px 20px 80px; }
h1.title { font: normal 1.6em/1.25 Georgia, "Times New Roman", serif; margin: 0 0 6px; }
.meta { color: var(--muted); font-size: 0.85em; margin: 0 0 28px; }
.msg { margin: 0 0 22px; }
.who { font-size: 0.75em; letter-spacing: 0.04em; text-transform: uppercase; color: var(--muted); margin-bottom: 4px; }
.user .body { background: var(--you); border-radius: 12px; padding: 10px 14px; }
.body > :first-child { margin-top: 0; }
.body > :last-child { margin-bottom: 0; }
.body p, .body ul, .body ol, .body pre, .body table, .body blockquote { margin: 0 0 0.8em; }
.body h1, .body h2, .body h3 { font-weight: 600; line-height: 1.3; margin: 1.2em 0 0.4em; }
.body h1 { font-size: 1.3em; } .body h2 { font-size: 1.15em; } .body h3 { font-size: 1em; }
code { font: 0.85em ui-monospace, Menlo, monospace; background: var(--code); padding: 0.1em 0.3em; border-radius: 4px; }
pre { background: var(--code); padding: 12px 14px; border-radius: 8px; overflow-x: auto; }
pre code { background: none; padding: 0; }
table { border-collapse: collapse; width: 100%; font-size: 0.9em; }
th, td { border-bottom: 1px solid var(--line); padding: 6px 8px; text-align: left; }
blockquote { border-left: 3px solid var(--line); padding-left: 14px; color: var(--muted); }
a { color: inherit; }
li:has(> input[type="checkbox"]) { list-style: none; margin-left: -1.3em; }
"#;

/// The chat's visible conversation as one standalone file.
pub async fn freeze_chat(pool: &PgPool, chat_id: &str) -> Result<FrozenChat> {
    let title: Option<String> =
        sqlx::query_scalar("SELECT title FROM app_chats WHERE id = $1 AND deleted_at IS NULL")
            .bind(chat_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| Error::Database(format!("read chat: {e}")))?;
    let title = title.ok_or_else(|| Error::NotFound(format!("no chat {chat_id:?}")))?;

    // The same rows the person sees in the transcript (`api::chats::get_chat`).
    let rows = sqlx::query(
        "SELECT role, content, parts FROM app_chat_messages \
         WHERE chat_id = $1 AND (subject IS NULL OR subject != 'onboarding_synthetic') \
           AND role IN ('user', 'assistant') \
         ORDER BY sequence_num ASC",
    )
    .bind(chat_id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("read chat messages: {e}")))?;

    let mut messages = Vec::new();
    let mut attachments_left_out = 0;
    let mut all_names = Vec::new();
    for row in rows {
        let role: String = row.get("role");
        let content: String = row.get("content");
        let parts: Option<serde_json::Value> = row.get("parts");
        let (text, files) = said(&role, &content, parts.as_ref());
        attachments_left_out += files;
        if text.trim().is_empty() {
            continue;
        }
        let (text, names) = strip_internal_links(&text);
        all_names.extend(names);
        messages.push(Said { role, text });
    }
    if messages.is_empty() {
        return Err(Error::InvalidInput("this chat has no messages to share yet".into()));
    }
    all_names.sort();
    all_names.dedup();

    let mut body = String::new();
    for m in &messages {
        // The sharer's own messages sit in a shaded bubble and carry no label:
        // to the reader they are neither "you" nor "them".
        let (class, who) = if m.role == "user" {
            ("user", String::new())
        } else {
            ("assistant", "<div class=\"who\">Assistant</div>".to_string())
        };
        body.push_str(&format!(
            "<section class=\"msg {class}\">{who}<div class=\"body\">{}</div></section>\n",
            render(&m.text)
        ));
    }
    let shown_title = escape(&title);
    let count = messages.len();
    let html = format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{shown_title}</title>\n<style>{CHAT_CSS}</style>\n</head>\n<body>\n<main>\n\
         <h1 class=\"title\">{shown_title}</h1>\n<p class=\"meta\">A conversation with a virtues assistant, {count} {}.</p>\n\
         {body}</main>\n</body>\n</html>\n",
        if count == 1 { "message" } else { "messages" }
    );
    Ok(FrozenChat { title, html, names: all_names, message_count: count, attachments_left_out })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_words_leave() {
        let parts = serde_json::json!([
            { "type": "reasoning", "text": "the owner's heart rate was 51 last night" },
            { "type": "tool-sql_query", "input": { "sql": "select * from data_health_heart_rate" }, "output": [{ "bpm": 51 }] },
            { "type": "text", "text": "You slept well." },
            { "type": "file", "mediaType": "image/png", "url": "/api/drive/files/df_1/download" }
        ]);
        let (text, files) = said("assistant", "ignored", Some(&parts));
        assert_eq!(text, "You slept well.");
        assert_eq!(files, 1);
        assert!(!text.contains("51"));
    }

    #[test]
    fn older_messages_fall_back_to_their_content() {
        assert_eq!(said("user", "hello", None), ("hello".to_string(), 0));
        assert_eq!(said("user", "hello", Some(&serde_json::json!([]))), ("hello".to_string(), 0));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_chat_freezes_to_its_visible_words(pool: PgPool) {
        sqlx::query("INSERT INTO app_chats (id, title, message_count) VALUES ('chat_t', 'Double dates', 3)")
            .execute(&pool)
            .await
            .unwrap();
        let msgs = [
            ("m1", "user", "any fun double date ideas?", serde_json::json!([{ "type": "text", "text": "any fun double date ideas?" }]), None),
            ("m2", "assistant", "", serde_json::json!([
                { "type": "tool-sql_query", "output": [{ "phone": "+15125550142" }] },
                { "type": "text", "text": "Try a cooking class with [⟦Nick⟧](/person/person_devnick)." }
            ]), None),
            ("m3", "user", "start", serde_json::json!([{ "type": "text", "text": "start" }]), Some("onboarding_synthetic")),
        ];
        for (i, (id, role, content, parts, subject)) in msgs.iter().enumerate() {
            sqlx::query(
                "INSERT INTO app_chat_messages (id, chat_id, role, content, parts, subject, sequence_num) \
                 VALUES ($1, 'chat_t', $2, $3, $4, $5, $6)",
            )
            .bind(id)
            .bind(role)
            .bind(content)
            .bind(parts)
            .bind(subject)
            .bind(i as i64)
            .execute(&pool)
            .await
            .unwrap();
        }
        let frozen = freeze_chat(&pool, "chat_t").await.unwrap();
        assert_eq!(frozen.message_count, 2, "the onboarding trigger stays hidden");
        assert!(frozen.html.contains("cooking class with Nick"));
        assert!(!frozen.html.contains("5550142"), "tool results never leave");
        assert!(!frozen.html.contains("/person/"));
        assert_eq!(frozen.names, vec!["Nick".to_string()]);
    }
}

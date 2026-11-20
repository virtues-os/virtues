//! Change the owner's Gmail and Google Calendar from the chat
//! (agents/plan/google-write-access-plan.md, slice 4).
//!
//! Four tools act through the owner's Google sign-in, the `source` credential
//! with key `google`:
//!
//! - `email_archive` takes a conversation out of the inbox, or puts it back.
//!   The reason goes into the synced rows' metadata, so every archive says why.
//! - `email_label` adds or removes the owner's labels on a conversation.
//! - `email_draft` writes a Gmail draft, or replaces one this tool wrote.
//! - `calendar_event` creates, changes or deletes an event. A delete asks the
//!   owner first (`ToolExecutor::PERMISSION_REQUIRED`).
//!
//! There is no send tool. The owner sends from Gmail, so an email that carries
//! instructions cannot get the box to send mail as them. Trash and spam are
//! refused for the same reason: every change these tools make can be undone.
//!
//! An owner can have more than one Google account signed in. The synced mail
//! rows do not say which account a message belongs to, so a call tries each
//! account until one has the conversation. A grant made before these scopes
//! existed fails with a message that says to reconnect Google in Sources.

use std::future::Future;

use base64::Engine;
use chrono::{Duration, NaiveDate, NaiveDateTime};
use serde_json::{json, Value};
use sqlx::PgPool;

use super::{ToolContext, ToolResult};

pub const ARCHIVE_TOOL: &str = "email_archive";
pub const LABEL_TOOL: &str = "email_label";
pub const DRAFT_TOOL: &str = "email_draft";
pub const CALENDAR_TOOL: &str = "calendar_event";

const GMAIL: &str = "https://gmail.googleapis.com/gmail/v1/users/me";
const CALENDAR: &str = "https://www.googleapis.com/calendar/v3/calendars";

/// The private extended property that marks an event this tool created.
const CREATED_BY: &str = "virtues";
/// The header that marks a draft this tool wrote, so a replace never
/// overwrites a draft the owner started.
const DRAFT_HEADER: &str = "X-Virtues-Draft";

/// Gmail labels the tools never add: both are a step toward deleting mail.
const REFUSED_LABELS: &[&str] = &["TRASH", "SPAM"];
/// How many conversations one call may change.
const MAX_EMAILS: usize = 50;
/// Longer than any email a person writes by hand.
const MAX_BODY_CHARS: usize = 20_000;

/// Whether a delete needs the owner's approval: the executor gates only this.
pub fn is_delete(arguments: &Value) -> bool {
    arguments.get("action").and_then(|v| v.as_str()) == Some("delete")
}

// ─── Accounts and calls ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Account {
    credential_id: String,
    email: Option<String>,
}

/// Why a Google call failed.
#[derive(Debug)]
enum Failure {
    /// This account does not have the thing asked for; try the next one.
    NotFound,
    /// The grant lacks the scope. Reconnecting adds it.
    Scope,
    Other(String),
}

/// Which permission a missing scope is, in the owner's words.
#[derive(Clone, Copy)]
enum Area {
    Mail,
    Calendar,
}

fn scope_message(area: Area) -> String {
    let what = match area {
        Area::Mail => "change your mail",
        Area::Calendar => "change your calendar",
    };
    format!(
        "Your Google sign-in does not let Virtues {what} yet. Tell the owner to open Sources, \
         choose Google, choose Reconnect, and allow the new permission. Nothing was changed."
    )
}

/// The owner's active Google sign-ins, oldest first. With `account`, only
/// the one with that address.
async fn accounts(pool: &PgPool, account: Option<&str>) -> Result<Vec<Account>, String> {
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT id, metadata->>'email' FROM credentials \
          WHERE kind = 'source' AND key = 'google' AND status = 'active' \
          ORDER BY created_at",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Could not read your Google sign-ins: {e}"))?;
    let all: Vec<Account> = rows
        .into_iter()
        .map(|(credential_id, email)| Account { credential_id, email })
        .collect();
    if all.is_empty() {
        return Err("Google is not connected. Tell the owner to connect Google in Sources.".into());
    }
    let Some(wanted) = account.map(str::trim).filter(|a| !a.is_empty()) else {
        return Ok(all);
    };
    let matched: Vec<Account> = all
        .iter()
        .filter(|a| a.email.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(wanted)))
        .cloned()
        .collect();
    if matched.is_empty() {
        let known: Vec<&str> = all.iter().filter_map(|a| a.email.as_deref()).collect();
        return Err(format!(
            "No Google account {wanted} is connected. Connected: {}.",
            if known.is_empty() { "accounts with no address on file".to_string() } else { known.join(", ") }
        ));
    }
    Ok(matched)
}

/// A current access token for one sign-in, refreshed first if it is about
/// to expire.
async fn token(pool: &PgPool, account: &Account) -> Result<String, Failure> {
    virtues_helpers::auth::ensure_fresh(pool, &account.credential_id)
        .await
        .map_err(|e| Failure::Other(format!("Could not refresh the Google sign-in: {e}")))?;
    let secrets = virtues_helpers::auth::vault::read_credential_secrets(pool, &account.credential_id)
        .await
        .map_err(|e| Failure::Other(format!("Could not read the Google sign-in: {e}")))?;
    secrets
        .get("access_token")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| Failure::Other("The Google sign-in has no access token. Reconnect Google in Sources.".into()))
}

fn client() -> Result<&'static reqwest::Client, Failure> {
    static CLIENT: std::sync::OnceLock<Option<reqwest::Client>> = std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            crate::http_client::base_builder()
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .map_err(|e| tracing::warn!(error = %e, "could not build the Google client"))
                .ok()
        })
        .as_ref()
        .ok_or_else(|| Failure::Other("Could not start an HTTPS client.".into()))
}

/// Send one request to Google and read the JSON answer. An empty body (a
/// delete) reads as `null`.
async fn send(request: reqwest::RequestBuilder) -> Result<Value, Failure> {
    let response = request
        .send()
        .await
        .map_err(|e| Failure::Other(format!("Could not reach Google: {e}")))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| Failure::Other(format!("Could not read Google's answer: {e}")))?;
    if status.is_success() {
        if body.trim().is_empty() {
            return Ok(Value::Null);
        }
        return serde_json::from_str(&body)
            .map_err(|e| Failure::Other(format!("Google answered with something other than JSON: {e}")));
    }
    Err(classify(status.as_u16(), &body))
}

/// What a failed Google answer means for the caller.
fn classify(status: u16, body: &str) -> Failure {
    let lower = body.to_ascii_lowercase();
    if status == 403 && (lower.contains("insufficient") || lower.contains("scope")) {
        return Failure::Scope;
    }
    if status == 404 {
        return Failure::NotFound;
    }
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.pointer("/error/message").and_then(|m| m.as_str()).map(String::from))
        .unwrap_or_else(|| body.chars().take(300).collect());
    if status == 401 {
        return Failure::Other(format!(
            "Google refused the sign-in ({message}). Tell the owner to reconnect Google in Sources."
        ));
    }
    Failure::Other(format!("Google answered {status}: {message}"))
}

/// Run `op` with each account's token until one has what it needs. Returns
/// the answer and the account that gave it.
async fn on_each_account<F, Fut>(
    pool: &PgPool,
    accounts: &[Account],
    area: Area,
    not_found: &str,
    op: F,
) -> Result<(Value, Account), String>
where
    F: Fn(String) -> Fut,
    Fut: Future<Output = Result<Value, Failure>>,
{
    for account in accounts {
        let token = match token(pool, account).await {
            Ok(t) => t,
            Err(Failure::Other(why)) => return Err(why),
            Err(Failure::Scope) => return Err(scope_message(area)),
            Err(Failure::NotFound) => continue,
        };
        match op(token).await {
            Ok(v) => return Ok((v, account.clone())),
            Err(Failure::NotFound) => continue,
            Err(Failure::Scope) => return Err(scope_message(area)),
            Err(Failure::Other(why)) => return Err(why),
        }
    }
    Err(not_found.to_string())
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty())
}

fn list_arg(args: &Value, key: &str) -> Vec<String> {
    match args.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect(),
        Some(Value::String(s)) if !s.trim().is_empty() => vec![s.trim().to_string()],
        _ => Vec::new(),
    }
}

// ─── Mail ─────────────────────────────────────────────────────────────────

/// One conversation the call names, from the synced rows.
#[derive(Debug, Clone, PartialEq)]
struct Conversation {
    conversation_id: String,
    subject: Option<String>,
}

/// The conversations `refs` name. A ref is a row id from
/// data_communication_email, a Gmail message id, or a Gmail thread id.
/// Unknown refs come back in the second list.
async fn conversations(pool: &PgPool, refs: &[String]) -> Result<(Vec<Conversation>, Vec<String>), String> {
    let rows: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT id, message_id, conversation_id, subject FROM data_communication_email \
          WHERE source_provider = 'google' \
            AND (id = ANY($1) OR message_id = ANY($1) OR conversation_id = ANY($1))",
    )
    .bind(refs)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Could not read your mail: {e}"))?;

    let mut found: Vec<Conversation> = Vec::new();
    let mut missing = Vec::new();
    for r in refs {
        let hit = rows.iter().find(|(id, message_id, conversation_id, _)| {
            id == r || message_id == r || conversation_id.as_deref() == Some(r.as_str())
        });
        match hit {
            Some((_, message_id, conversation_id, subject)) => {
                // A message with no thread id is its own conversation.
                let conversation_id = conversation_id.clone().unwrap_or_else(|| message_id.clone());
                if !found.iter().any(|c| c.conversation_id == conversation_id) {
                    found.push(Conversation { conversation_id, subject: subject.clone() });
                }
            }
            None => missing.push(r.clone()),
        }
    }
    Ok((found, missing))
}

async fn modify_thread(token: String, thread: String, add: Vec<String>, remove: Vec<String>) -> Result<Value, Failure> {
    let client = client()?;
    send(
        client
            .post(format!("{GMAIL}/threads/{thread}/modify"))
            .bearer_auth(token)
            .json(&json!({ "addLabelIds": add, "removeLabelIds": remove })),
    )
    .await
}

pub async fn archive(pool: &PgPool, args: &Value, _context: &ToolContext) -> ToolResult {
    let refs = list_arg(args, "emails");
    if refs.is_empty() {
        return ToolResult::error("Name the emails in `emails`: ids from data_communication_email.");
    }
    if refs.len() > MAX_EMAILS {
        return ToolResult::error(format!("At most {MAX_EMAILS} emails per call."));
    }
    let undo = args.get("undo").and_then(|v| v.as_bool()).unwrap_or(false);
    let Some(reason) = str_arg(args, "reason") else {
        return ToolResult::error("Say why in `reason`, in a few words. It is kept with the email.");
    };
    let accounts = match accounts(pool, str_arg(args, "account")).await {
        Ok(a) => a,
        Err(why) => return ToolResult::error(why),
    };
    let (found, missing) = match conversations(pool, &refs).await {
        Ok(x) => x,
        Err(why) => return ToolResult::error(why),
    };
    if found.is_empty() {
        return ToolResult::error(format!("None of these are synced emails: {}.", missing.join(", ")));
    }

    let (add, remove) = if undo {
        (vec!["INBOX".to_string()], Vec::new())
    } else {
        (Vec::new(), vec!["INBOX".to_string()])
    };
    let mut changed = Vec::new();
    let mut failed = Vec::new();
    for conversation in &found {
        let thread = conversation.conversation_id.clone();
        let result = on_each_account(pool, &accounts, Area::Mail, "Gmail has no such conversation.", |token| {
            modify_thread(token, thread.clone(), add.clone(), remove.clone())
        })
        .await;
        match result {
            Ok(_) => {
                if let Err(e) = record_archive(pool, &conversation.conversation_id, undo, reason).await {
                    tracing::warn!(error = %e, "archived in Gmail but could not update the synced rows");
                }
                changed.push(json!({ "conversation_id": conversation.conversation_id, "subject": conversation.subject }));
            }
            Err(why) if why == scope_message(Area::Mail) => return ToolResult::error(why),
            Err(why) => failed.push(json!({ "conversation_id": conversation.conversation_id, "error": why })),
        }
    }

    let note = if undo {
        "Back in the inbox. Say so in a few words."
    } else {
        "Archived: out of the inbox, still in Gmail. The owner can ask you to put it back (call again with undo: true). Say so in a few words."
    };
    ToolResult::success(json!({
        "archived": !undo,
        "conversations": changed,
        "failed": failed,
        "not_found": missing,
        "note": note,
    }))
}

/// Mirror an archive in the synced rows, with why and when.
async fn record_archive(pool: &PgPool, conversation_id: &str, undo: bool, reason: &str) -> Result<(), sqlx::Error> {
    let entry = json!({
        "assistant_archive": {
            "archived": !undo,
            "reason": reason,
            "at": chrono::Utc::now().to_rfc3339(),
        }
    });
    sqlx::query(
        "UPDATE data_communication_email \
            SET labels = CASE WHEN $2 THEN \
                    (CASE WHEN labels ? 'INBOX' THEN labels ELSE labels || '[\"INBOX\"]'::jsonb END) \
                  ELSE labels - 'INBOX' END, \
                is_archived = NOT $2, \
                metadata = metadata || $3, \
                updated_at = now() \
          WHERE source_provider = 'google' \
            AND (conversation_id = $1 OR (conversation_id IS NULL AND message_id = $1))",
    )
    .bind(conversation_id)
    .bind(undo)
    .bind(entry)
    .execute(pool)
    .await?;
    Ok(())
}

/// A label name resolved to Gmail's id. `create` makes a user label that
/// does not exist yet.
async fn label_ids(token: &str, names: &[String], create: bool) -> Result<Vec<String>, Failure> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let client = client()?;
    let listed = send(client.get(format!("{GMAIL}/labels")).bearer_auth(token)).await?;
    let labels = listed.get("labels").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let mut ids = Vec::new();
    for name in names {
        let hit = labels.iter().find(|l| {
            let id = l.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let label = l.get("name").and_then(|v| v.as_str()).unwrap_or("");
            id.eq_ignore_ascii_case(name) || label.eq_ignore_ascii_case(name)
        });
        match hit.and_then(|l| l.get("id").and_then(|v| v.as_str())) {
            Some(id) => ids.push(id.to_string()),
            None if create => {
                let made = send(
                    client
                        .post(format!("{GMAIL}/labels"))
                        .bearer_auth(token)
                        .json(&json!({ "name": name, "labelListVisibility": "labelShow", "messageListVisibility": "show" })),
                )
                .await?;
                match made.get("id").and_then(|v| v.as_str()) {
                    Some(id) => ids.push(id.to_string()),
                    None => return Err(Failure::Other(format!("Gmail did not create the label {name}."))),
                }
            }
            None => return Err(Failure::Other(format!("There is no label named {name} in Gmail."))),
        }
    }
    Ok(ids)
}

fn refused_label(names: &[String]) -> Option<&String> {
    names.iter().find(|n| REFUSED_LABELS.iter().any(|r| r.eq_ignore_ascii_case(n)))
}

pub async fn label(pool: &PgPool, args: &Value, _context: &ToolContext) -> ToolResult {
    let refs = list_arg(args, "emails");
    let add = list_arg(args, "add");
    let remove = list_arg(args, "remove");
    if refs.is_empty() {
        return ToolResult::error("Name the emails in `emails`: ids from data_communication_email.");
    }
    if refs.len() > MAX_EMAILS {
        return ToolResult::error(format!("At most {MAX_EMAILS} emails per call."));
    }
    if add.is_empty() && remove.is_empty() {
        return ToolResult::error("Name labels to `add` or `remove`.");
    }
    if let Some(name) = refused_label(&add) {
        return ToolResult::error(format!("{name} is not a label this tool adds. Archive instead, which can be undone."));
    }
    if add.iter().chain(&remove).any(|n| n.eq_ignore_ascii_case("INBOX")) {
        return ToolResult::error("Use email_archive to take mail out of the inbox or put it back.");
    }
    let accounts = match accounts(pool, str_arg(args, "account")).await {
        Ok(a) => a,
        Err(why) => return ToolResult::error(why),
    };
    let (found, missing) = match conversations(pool, &refs).await {
        Ok(x) => x,
        Err(why) => return ToolResult::error(why),
    };
    if found.is_empty() {
        return ToolResult::error(format!("None of these are synced emails: {}.", missing.join(", ")));
    }

    let mut changed = Vec::new();
    let mut failed = Vec::new();
    for conversation in &found {
        let thread = conversation.conversation_id.clone();
        let result = on_each_account(pool, &accounts, Area::Mail, "Gmail has no such conversation.", |token| {
            let (thread, add, remove) = (thread.clone(), add.clone(), remove.clone());
            async move {
                let add_ids = label_ids(&token, &add, true).await?;
                let remove_ids = label_ids(&token, &remove, false).await?;
                modify_thread(token, thread, add_ids.clone(), remove_ids.clone()).await?;
                Ok(json!({ "add": add_ids, "remove": remove_ids }))
            }
        })
        .await;
        match result {
            Ok((ids, _)) => {
                if let Err(e) = record_labels(pool, &conversation.conversation_id, &ids["add"], &ids["remove"]).await {
                    tracing::warn!(error = %e, "labeled in Gmail but could not update the synced rows");
                }
                changed.push(json!({ "conversation_id": conversation.conversation_id, "subject": conversation.subject }));
            }
            Err(why) if why == scope_message(Area::Mail) => return ToolResult::error(why),
            Err(why) => failed.push(json!({ "conversation_id": conversation.conversation_id, "error": why })),
        }
    }
    ToolResult::success(json!({
        "added": add,
        "removed": remove,
        "conversations": changed,
        "failed": failed,
        "not_found": missing,
        "note": "Done in Gmail. The reverse call undoes it. Say what changed in a few words.",
    }))
}

/// Mirror a label change in the synced rows.
async fn record_labels(pool: &PgPool, conversation_id: &str, add: &Value, remove: &Value) -> Result<(), sqlx::Error> {
    let remove: Vec<String> = remove
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    sqlx::query(
        "UPDATE data_communication_email \
            SET labels = (SELECT COALESCE(jsonb_agg(DISTINCT l), '[]'::jsonb) \
                            FROM jsonb_array_elements_text((labels - $3::text[]) || $2) AS l), \
                updated_at = now() \
          WHERE source_provider = 'google' \
            AND (conversation_id = $1 OR (conversation_id IS NULL AND message_id = $1))",
    )
    .bind(conversation_id)
    .bind(add)
    .bind(&remove)
    .execute(pool)
    .await?;
    Ok(())
}

/// What a draft needs, after the reply's headers are filled in.
#[derive(Debug, Default, PartialEq)]
struct Draft {
    to: Vec<String>,
    cc: Vec<String>,
    subject: String,
    body: String,
    in_reply_to: Option<String>,
    references: Option<String>,
}

/// One header value with no line breaks, so nothing the model writes can
/// add a header of its own.
fn header_value(s: &str) -> String {
    s.replace(['\r', '\n'], " ").trim().to_string()
}

/// RFC 2047 encoding for a header with non-ASCII text.
fn encode_header(s: &str) -> String {
    let s = header_value(s);
    if s.is_ascii() {
        return s;
    }
    format!("=?UTF-8?B?{}?=", base64::engine::general_purpose::STANDARD.encode(s.as_bytes()))
}

/// The draft as an RFC 2822 message, base64url-encoded the way Gmail's
/// `raw` field wants it.
fn raw_message(draft: &Draft) -> String {
    let mut out = String::new();
    let list = |v: &[String]| v.iter().map(|a| header_value(a)).collect::<Vec<_>>().join(", ");
    if !draft.to.is_empty() {
        out.push_str(&format!("To: {}\r\n", list(&draft.to)));
    }
    if !draft.cc.is_empty() {
        out.push_str(&format!("Cc: {}\r\n", list(&draft.cc)));
    }
    out.push_str(&format!("Subject: {}\r\n", encode_header(&draft.subject)));
    if let Some(id) = &draft.in_reply_to {
        out.push_str(&format!("In-Reply-To: {}\r\n", header_value(id)));
    }
    if let Some(refs) = &draft.references {
        out.push_str(&format!("References: {}\r\n", header_value(refs)));
    }
    out.push_str(&format!("{DRAFT_HEADER}: 1\r\n"));
    out.push_str("MIME-Version: 1.0\r\n");
    out.push_str("Content-Type: text/plain; charset=\"UTF-8\"\r\n");
    out.push_str("Content-Transfer-Encoding: base64\r\n\r\n");
    let body = base64::engine::general_purpose::STANDARD.encode(draft.body.replace("\r\n", "\n").replace('\n', "\r\n"));
    for chunk in body.as_bytes().chunks(76) {
        out.push_str(std::str::from_utf8(chunk).unwrap_or_default());
        out.push_str("\r\n");
    }
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(out.as_bytes())
}

fn header_of(message: &Value, name: &str) -> Option<String> {
    message
        .pointer("/payload/headers")
        .and_then(|h| h.as_array())?
        .iter()
        .find(|h| h.get("name").and_then(|n| n.as_str()).is_some_and(|n| n.eq_ignore_ascii_case(name)))
        .and_then(|h| h.get("value").and_then(|v| v.as_str()))
        .map(String::from)
}

/// The reply's subject: the original's with one "Re: " in front.
fn reply_subject(original: &str) -> String {
    let trimmed = original.trim();
    if trimmed.len() >= 3 && trimmed[..3].eq_ignore_ascii_case("re:") {
        trimmed.to_string()
    } else {
        format!("Re: {trimmed}")
    }
}

/// The Gmail message id a `reply_to` ref names: a row id or a message id.
async fn reply_message_id(pool: &PgPool, reference: &str) -> Result<String, String> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT message_id FROM data_communication_email \
          WHERE source_provider = 'google' AND (id = $1 OR message_id = $1) LIMIT 1",
    )
    .bind(reference)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Could not read your mail: {e}"))?;
    // A ref that is not a synced row may still be a Gmail id; Gmail decides.
    Ok(match row {
        Some((id,)) => id,
        None => reference.to_string(),
    })
}

pub async fn draft(pool: &PgPool, args: &Value, _context: &ToolContext) -> ToolResult {
    let body = args.get("body").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if body.trim().is_empty() {
        return ToolResult::error("Write the draft in `body`.");
    }
    if body.chars().count() > MAX_BODY_CHARS {
        return ToolResult::error(format!("That is longer than {MAX_BODY_CHARS} characters. Shorten it."));
    }
    let reply_to = match str_arg(args, "reply_to") {
        Some(r) => match reply_message_id(pool, r).await {
            Ok(id) => Some(id),
            Err(why) => return ToolResult::error(why),
        },
        None => None,
    };
    let to = list_arg(args, "to");
    if to.is_empty() && reply_to.is_none() {
        return ToolResult::error("Say who the draft is to in `to`, or name the email it answers in `reply_to`.");
    }
    let replace = str_arg(args, "draft_id").map(String::from);
    let accounts = match accounts(pool, str_arg(args, "account")).await {
        Ok(a) => a,
        Err(why) => return ToolResult::error(why),
    };
    let cc = list_arg(args, "cc");
    let subject = str_arg(args, "subject").map(String::from);

    let result = on_each_account(pool, &accounts, Area::Mail, "Gmail has no such email or draft.", |token| {
        let (reply_to, replace, to, cc, subject, body) =
            (reply_to.clone(), replace.clone(), to.clone(), cc.clone(), subject.clone(), body.clone());
        async move {
            let client = client()?;
            let mut draft = Draft { to, cc, subject: subject.clone().unwrap_or_default(), body, ..Draft::default() };
            let mut thread_id = None;
            if let Some(id) = &reply_to {
                let original = send(
                    client
                        .get(format!("{GMAIL}/messages/{id}"))
                        .bearer_auth(&token)
                        .query(&[
                            ("format", "metadata"),
                            ("metadataHeaders", "Message-ID"),
                            ("metadataHeaders", "References"),
                            ("metadataHeaders", "Subject"),
                            ("metadataHeaders", "From"),
                            ("metadataHeaders", "Reply-To"),
                        ]),
                )
                .await?;
                thread_id = original.get("threadId").and_then(|v| v.as_str()).map(String::from);
                let message_id = header_of(&original, "Message-ID");
                if draft.to.is_empty() {
                    if let Some(addr) = header_of(&original, "Reply-To").or_else(|| header_of(&original, "From")) {
                        draft.to = vec![addr];
                    }
                }
                if subject.is_none() {
                    draft.subject = reply_subject(&header_of(&original, "Subject").unwrap_or_default());
                }
                draft.references = match (header_of(&original, "References"), &message_id) {
                    (Some(refs), Some(id)) => Some(format!("{refs} {id}")),
                    (None, Some(id)) => Some(id.clone()),
                    (refs, None) => refs,
                };
                draft.in_reply_to = message_id;
            }
            let mut message = json!({ "raw": raw_message(&draft) });
            if let Some(t) = &thread_id {
                message["threadId"] = json!(t);
            }
            let request = match &replace {
                Some(id) => {
                    let existing = send(
                        client
                            .get(format!("{GMAIL}/drafts/{id}"))
                            .bearer_auth(&token)
                            .query(&[("format", "metadata"), ("metadataHeaders", DRAFT_HEADER)]),
                    )
                    .await?;
                    let ours = existing
                        .get("message")
                        .map(|m| header_of(m, DRAFT_HEADER).is_some())
                        .unwrap_or(false);
                    if !ours {
                        return Err(Failure::Other(
                            "That draft was not written by Virtues, so it stays as it is. Write a new draft instead.".into(),
                        ));
                    }
                    client.put(format!("{GMAIL}/drafts/{id}")).json(&json!({ "id": id, "message": message }))
                }
                None => client.post(format!("{GMAIL}/drafts")).json(&json!({ "message": message })),
            };
            let saved = send(request.bearer_auth(&token)).await?;
            Ok(json!({
                "draft_id": saved.get("id"),
                "conversation_id": saved.pointer("/message/threadId"),
                "to": draft.to,
                "cc": draft.cc,
                "subject": draft.subject,
            }))
        }
    })
    .await;

    match result {
        Ok((mut saved, account)) => {
            saved["account"] = json!(account.email);
            saved["replaced"] = json!(replace.is_some());
            saved["note"] = json!(
                "Saved as a draft in Gmail. Nothing was sent: the owner reviews and sends it from Gmail. \
                 Say so in a few words; do not repeat the draft."
            );
            ToolResult::success(saved)
        }
        Err(why) => ToolResult::error(why),
    }
}

// ─── Calendar ─────────────────────────────────────────────────────────────

/// An event time as Google's API takes it: a date for an all-day event, or a
/// local date and time with the owner's timezone.
fn event_time(raw: &str, all_day: bool, timezone: &str) -> Result<Value, String> {
    let raw = raw.trim();
    if all_day {
        let date = NaiveDate::parse_from_str(raw.get(..10).unwrap_or(raw), "%Y-%m-%d")
            .map_err(|_| format!("{raw} is not a date. Use 2026-11-20."))?;
        return Ok(json!({ "date": date.format("%Y-%m-%d").to_string() }));
    }
    if let Ok(at) = chrono::DateTime::parse_from_rfc3339(raw) {
        return Ok(json!({ "dateTime": at.to_rfc3339(), "timeZone": timezone }));
    }
    let local = ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M"]
        .iter()
        .find_map(|f| NaiveDateTime::parse_from_str(raw, f).ok())
        .ok_or_else(|| format!("{raw} is not a date and time. Use 2026-11-20T15:00, in the owner's time."))?;
    Ok(json!({ "dateTime": local.format("%Y-%m-%dT%H:%M:%S").to_string(), "timeZone": timezone }))
}

/// The end an event gets when the call names only a start: an hour later,
/// or the next day for an all-day event.
fn default_end(start: &Value, timezone: &str) -> Option<Value> {
    if let Some(date) = start.get("date").and_then(|d| d.as_str()) {
        let day = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()? + Duration::days(1);
        return Some(json!({ "date": day.format("%Y-%m-%d").to_string() }));
    }
    let at = start.get("dateTime").and_then(|d| d.as_str())?;
    let end = if let Ok(t) = chrono::DateTime::parse_from_rfc3339(at) {
        (t + Duration::hours(1)).to_rfc3339()
    } else {
        let t = NaiveDateTime::parse_from_str(at, "%Y-%m-%dT%H:%M:%S").ok()?;
        (t + Duration::hours(1)).format("%Y-%m-%dT%H:%M:%S").to_string()
    };
    Some(json!({ "dateTime": end, "timeZone": timezone }))
}

/// The Google event and calendar an `event` ref names: a row id from
/// data_calendar_event, or a Google event id on `calendar`.
async fn event_target(pool: &PgPool, reference: &str, calendar: &str) -> Result<(String, String), String> {
    let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT metadata->>'google_event_id', metadata->>'calendar_id' FROM data_calendar_event \
          WHERE source_provider = 'google' AND (id = $1 OR metadata->>'google_event_id' = $1) LIMIT 1",
    )
    .bind(reference)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Could not read your calendar: {e}"))?;
    Ok(match row {
        Some((Some(event), Some(cal))) => (event, cal),
        Some((Some(event), None)) => (event, calendar.to_string()),
        // Not synced yet, such as one this tool just made: Google decides.
        _ => (reference.to_string(), calendar.to_string()),
    })
}

fn encode_path(s: &str) -> String {
    urlencoding::encode(s).into_owned()
}

pub async fn calendar_event(pool: &PgPool, args: &Value, context: &ToolContext) -> ToolResult {
    let action = str_arg(args, "action").unwrap_or("create");
    let calendar = str_arg(args, "calendar_id").unwrap_or("primary").to_string();
    let timezone = context.timezone.clone().unwrap_or_else(|| "UTC".to_string());
    let all_day = args.get("all_day").and_then(|v| v.as_bool()).unwrap_or(false);
    let accounts = match accounts(pool, str_arg(args, "account")).await {
        Ok(a) => a,
        Err(why) => return ToolResult::error(why),
    };

    // The fields a create or update sets.
    let mut fields = serde_json::Map::new();
    if let Some(title) = str_arg(args, "title") {
        fields.insert("summary".into(), json!(title));
    }
    if let Some(location) = str_arg(args, "location") {
        fields.insert("location".into(), json!(location));
    }
    if let Some(description) = args.get("description").and_then(|v| v.as_str()) {
        fields.insert("description".into(), json!(description));
    }
    let start = match str_arg(args, "start").map(|s| event_time(s, all_day, &timezone)).transpose() {
        Ok(s) => s,
        Err(why) => return ToolResult::error(why),
    };
    let end = match str_arg(args, "end").map(|s| event_time(s, all_day, &timezone)).transpose() {
        Ok(e) => e,
        Err(why) => return ToolResult::error(why),
    };
    if let Some(s) = &start {
        fields.insert("start".into(), s.clone());
    }
    match (&end, &start) {
        (Some(e), _) => {
            fields.insert("end".into(), e.clone());
        }
        (None, Some(s)) => {
            if let Some(e) = default_end(s, &timezone) {
                fields.insert("end".into(), e);
            }
        }
        (None, None) => {}
    }

    let result = match action {
        "create" => {
            if !fields.contains_key("summary") || start.is_none() {
                return ToolResult::error("A new event needs `title` and `start`.");
            }
            fields.insert("extendedProperties".into(), json!({ "private": { "created_by": CREATED_BY } }));
            let body = Value::Object(fields);
            let url = format!("{CALENDAR}/{}/events", encode_path(&calendar));
            on_each_account(pool, &accounts[..1], Area::Calendar, "Google has no such calendar.", |token| {
                let (url, body) = (url.clone(), body.clone());
                async move {
                    // `sendUpdates=none`: no one is emailed about the event.
                    send(client()?.post(url).bearer_auth(token).query(&[("sendUpdates", "none")]).json(&body)).await
                }
            })
            .await
        }
        "update" | "delete" => {
            let Some(reference) = str_arg(args, "event") else {
                return ToolResult::error("Name the event in `event`: an id from data_calendar_event.");
            };
            let (event, calendar) = match event_target(pool, reference, &calendar).await {
                Ok(t) => t,
                Err(why) => return ToolResult::error(why),
            };
            let url = format!("{CALENDAR}/{}/events/{}", encode_path(&calendar), encode_path(&event));
            if action == "update" {
                if fields.is_empty() {
                    return ToolResult::error("Say what to change: title, start, end, location or description.");
                }
                let body = Value::Object(fields);
                on_each_account(pool, &accounts, Area::Calendar, "Google has no such event.", |token| {
                    let (url, body) = (url.clone(), body.clone());
                    async move {
                        send(client()?.patch(url).bearer_auth(token).query(&[("sendUpdates", "none")]).json(&body)).await
                    }
                })
                .await
            } else {
                on_each_account(pool, &accounts, Area::Calendar, "Google has no such event.", |token| {
                    let url = url.clone();
                    async move {
                        send(client()?.delete(&url).bearer_auth(token).query(&[("sendUpdates", "none")])).await?;
                        Ok(json!({ "id": event_target_id(&url) }))
                    }
                })
                .await
            }
        }
        other => return ToolResult::error(format!("`action` is create, update or delete, not {other}.")),
    };

    match result {
        Ok((event, account)) => {
            let note = match action {
                "create" => "Added to the owner's Google Calendar. No one was invited or emailed. To undo, delete it with this tool.",
                "update" => "Changed in the owner's Google Calendar. No one was emailed.",
                _ => "Deleted from the owner's Google Calendar. No one was emailed.",
            };
            ToolResult::success(json!({
                "action": action,
                "event_id": event.get("id"),
                "title": event.get("summary"),
                "start": event.get("start"),
                "end": event.get("end"),
                "link": event.get("htmlLink"),
                "account": account.email,
                "note": note,
            }))
        }
        Err(why) => ToolResult::error(why),
    }
}

/// The event id at the end of an events URL, for a delete's answer.
fn event_target_id(url: &str) -> String {
    url.rsplit('/').next().map(|s| urlencoding::decode(s).map(|d| d.into_owned()).unwrap_or_else(|_| s.to_string())).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(raw: &str) -> String {
        String::from_utf8(base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(raw).unwrap()).unwrap()
    }

    #[test]
    fn a_draft_cannot_add_a_header_of_its_own() {
        let draft = Draft {
            to: vec!["nick@example.com\r\nBcc: someone@example.com".into()],
            subject: "Lunch\nBcc: someone@example.com".into(),
            body: "See you at noon.".into(),
            ..Draft::default()
        };
        let message = decode(&raw_message(&draft));
        let headers = message.split("\r\n\r\n").next().unwrap();
        assert!(!headers.lines().any(|l| l.starts_with("Bcc:")), "{headers}");
        assert!(headers.contains(&format!("{DRAFT_HEADER}: 1")));
    }

    #[test]
    fn a_reply_keeps_the_thread_headers_and_encodes_non_ascii() {
        let draft = Draft {
            to: vec!["David Okafor <david@example.com>".into()],
            subject: "Re: Café on Friday".into(),
            body: "Yes, Friday works.".into(),
            in_reply_to: Some("<abc@example.com>".into()),
            references: Some("<root@example.com> <abc@example.com>".into()),
            ..Draft::default()
        };
        let message = decode(&raw_message(&draft));
        assert!(message.contains("In-Reply-To: <abc@example.com>\r\n"));
        assert!(message.contains("References: <root@example.com> <abc@example.com>\r\n"));
        assert!(message.contains("Subject: =?UTF-8?B?"));
    }

    #[test]
    fn reply_subjects_get_one_re() {
        assert_eq!(reply_subject("Lunch"), "Re: Lunch");
        assert_eq!(reply_subject("RE: Lunch"), "RE: Lunch");
    }

    #[test]
    fn trash_and_spam_are_refused() {
        assert!(refused_label(&["trash".into()]).is_some());
        assert!(refused_label(&["Receipts".into()]).is_none());
    }

    #[test]
    fn event_times_take_the_owners_timezone_and_default_ends() {
        let start = event_time("2026-11-20T15:00", false, "America/Chicago").unwrap();
        assert_eq!(start, json!({ "dateTime": "2026-11-20T15:00:00", "timeZone": "America/Chicago" }));
        assert_eq!(
            default_end(&start, "America/Chicago").unwrap(),
            json!({ "dateTime": "2026-11-20T16:00:00", "timeZone": "America/Chicago" })
        );
        let day = event_time("2026-11-20", true, "UTC").unwrap();
        assert_eq!(default_end(&day, "UTC").unwrap(), json!({ "date": "2026-11-21" }));
        assert!(event_time("tomorrow", false, "UTC").is_err());
    }

    #[test]
    fn a_missing_scope_is_told_apart_from_a_missing_thing() {
        let body = r#"{"error":{"code":403,"message":"Request had insufficient authentication scopes.","status":"PERMISSION_DENIED"}}"#;
        assert!(matches!(classify(403, body), Failure::Scope));
        assert!(matches!(classify(404, "{}"), Failure::NotFound));
        assert!(matches!(classify(500, "{}"), Failure::Other(_)));
    }

    #[test]
    fn only_a_delete_is_gated() {
        assert!(is_delete(&json!({ "action": "delete" })));
        assert!(!is_delete(&json!({ "action": "create" })));
        assert!(!is_delete(&json!({})));
    }
}

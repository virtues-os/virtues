//! Send an iMessage as the owner, from the chat.
//!
//! The box holds the record and the Mac holds Messages. This tool works out
//! who `to` names and which thread the owner already has with them, then asks
//! the Mac app to send `text` into it, over the socket the app's browser
//! already keeps open (`browser::call`). It is offered only while that app is
//! connected (`ChatMode::tools`).
//!
//! It does not ask first: the owner's call, 2026-10-20. A send cannot be
//! taken back, so instead it never guesses who. A name that fits two people,
//! or nobody, comes back as an error naming who it could be, and the model
//! asks the owner.

use std::time::Duration;

use serde_json::{json, Value};
use sqlx::PgPool;

use super::ToolResult;

pub const TOOL: &str = "send_imessage";

/// Messages may be cold: the app's script launches it and retries the thread
/// for about four seconds before it falls back to the handle.
const SEND_WAIT: Duration = Duration::from_secs(30);

/// Longer than any text message a person writes; a model that produces more
/// has gone wrong.
const MAX_CHARS: usize = 4_000;

/// Who the message goes to, and where.
#[derive(Debug, PartialEq)]
struct Target {
    person_id: Option<String>,
    name: Option<String>,
    /// A phone in E.164 or an email: the fallback when no thread resolves.
    handle: String,
    /// The chat.db GUID of the latest one-to-one thread, as chat.db wrote it.
    chat_id: Option<String>,
}

pub async fn run(pool: &PgPool, args: &Value) -> ToolResult {
    let to = args.get("to").and_then(|v| v.as_str()).unwrap_or("").trim();
    let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("").trim();
    if to.is_empty() {
        return ToolResult::error("Say who the message is for in `to`.");
    }
    if text.is_empty() {
        return ToolResult::error("There is nothing to send: `text` is empty.");
    }
    if text.chars().count() > MAX_CHARS {
        return ToolResult::error(format!("That is longer than {MAX_CHARS} characters. Shorten it."));
    }

    let target = match resolve(pool, to).await {
        Ok(Ok(t)) => t,
        Ok(Err(why)) => return ToolResult::error(why),
        Err(e) => return ToolResult::error(format!("Could not read your contacts: {e}")),
    };

    let sent = crate::browser::call(
        "imessage_send",
        json!({ "chat_id": target.chat_id.clone().unwrap_or_default(), "handle": target.handle, "text": text }),
        SEND_WAIT,
    )
    .await;
    if let Err(e) = sent {
        return ToolResult::error(e);
    }

    ToolResult::success(json!({
        "sent": true,
        "to": target.name.clone().unwrap_or_else(|| target.handle.clone()),
        "person_id": target.person_id,
        "handle": target.handle,
        "text": text,
        "sent_at": chrono::Utc::now().to_rfc3339(),
        "note": "The chat shows the owner what was sent. Confirm in a few words; do not repeat the message.",
    }))
}

/// `Ok(Err(..))` is an answer for the model: nobody, or more than one person.
async fn resolve(pool: &PgPool, to: &str) -> sqlx::Result<Result<Target, String>> {
    // A phone number or an email: send to it, naming the person if it is theirs.
    if let Some(handle) = virtues_helpers::handles::normalize_handle(to) {
        let owner: Option<(String, String)> = sqlx::query_as(
            "SELECT id, name FROM wiki_people WHERE handles ? $1 ORDER BY updated_at DESC LIMIT 1",
        )
        .bind(&handle)
        .fetch_optional(pool)
        .await?;
        let chat_id = latest_thread(pool, std::slice::from_ref(&handle), owner.as_ref().map(|o| o.0.as_str()))
            .await?
            .map(|(chat, _)| chat);
        let (person_id, name) = owner.unzip();
        return Ok(Ok(Target { person_id, name, handle, chat_id }));
    }

    let people = matching_people(pool, to).await?;
    let person = match people.as_slice() {
        [] => {
            return Ok(Err(format!(
                "Nobody in the owner's people is called \"{to}\". Ask the owner who they mean, \
                 or pass a phone number or email."
            )))
        }
        [one] => one,
        many => {
            let list: Vec<String> = many.iter().map(|p| format!("{} ({})", p.name, p.id)).collect();
            return Ok(Err(format!(
                "\"{to}\" could be {}. Ask the owner which one, then pass that person's id.",
                list.join(", ")
            )));
        }
    };
    if let Some(died) = person.died_on {
        return Ok(Err(format!("{} died on {died}. Nothing was sent.", person.name)));
    }
    if person.handles.is_empty() {
        return Ok(Err(format!(
            "There is no phone number or email for {}. Ask the owner for one.",
            person.name
        )));
    }

    let thread = latest_thread(pool, &person.handles, Some(&person.id)).await?;
    let (chat_id, thread_handle) = thread.unzip();
    let handle = thread_handle.flatten().unwrap_or_else(|| fallback_handle(&person.handles));
    Ok(Ok(Target { person_id: Some(person.id.clone()), name: Some(person.name.clone()), handle, chat_id }))
}

struct Person {
    id: String,
    name: String,
    handles: Vec<String>,
    died_on: Option<chrono::NaiveDate>,
}

/// An exact name, nickname or alias first. A first name alone counts only
/// when it belongs to one person, so "message Sam" never picks a Sam.
async fn matching_people(pool: &PgPool, to: &str) -> sqlx::Result<Vec<Person>> {
    type Row = (String, String, Value, Option<chrono::NaiveDate>);
    let rows = |rows: Vec<Row>| -> Vec<Person> {
        rows.into_iter()
            .map(|(id, name, handles, died_on)| Person {
                id,
                name,
                handles: handles
                    .as_array()
                    .map(|a| a.iter().filter_map(|h| h.as_str().map(str::to_string)).collect())
                    .unwrap_or_default(),
                died_on,
            })
            .collect()
    };

    if to.starts_with(&format!("{}_", crate::ids::WIKI_PERSON_PREFIX)) {
        let by_id: Vec<Row> = sqlx::query_as("SELECT id, name, handles, died_on FROM wiki_people WHERE id = $1")
            .bind(to)
            .fetch_all(pool)
            .await?;
        return Ok(rows(by_id));
    }

    let exact: Vec<Row> = sqlx::query_as(
        "SELECT id, name, handles, died_on FROM wiki_people
          WHERE lower(name) = lower($1)
             OR lower(nickname) = lower($1)
             OR EXISTS (SELECT 1 FROM jsonb_array_elements_text(aliases) a WHERE lower(a) = lower($1))
          ORDER BY name, id",
    )
    .bind(to)
    .fetch_all(pool)
    .await?;
    if !exact.is_empty() {
        return Ok(rows(exact));
    }

    let first: Vec<Row> = sqlx::query_as(
        "SELECT id, name, handles, died_on FROM wiki_people
          WHERE split_part(lower(name), ' ', 1) = lower($1)
          ORDER BY name, id",
    )
    .bind(to)
    .fetch_all(pool)
    .await?;
    Ok(rows(first))
}

/// The most recent one-to-one thread with any of these handles: theirs to the
/// owner, or (through the recipient refs) the owner's to them that went
/// unanswered. Returns the thread's GUID and the handle it was seen under.
async fn latest_thread(
    pool: &PgPool,
    handles: &[String],
    person_id: Option<&str>,
) -> sqlx::Result<Option<(String, Option<String>)>> {
    sqlx::query_as(
        "SELECT thread_id, handle FROM (
             SELECT m.thread_id, m.from_handle AS handle, m.occurred_at
               FROM data_communication_message m
              WHERE m.from_handle = ANY($1)
                AND NOT m.is_group_message
                AND m.thread_id <> ''
             UNION ALL
             SELECT m.thread_id, NULL, m.occurred_at
               FROM wiki_refs r
               JOIN data_communication_message m ON m.id = r.source_id
              WHERE r.entity_id = $2
                AND r.source_table = 'data_communication_message'
                AND r.role = 'recipient'
                AND NOT m.is_group_message
                AND m.thread_id <> ''
         ) t
         ORDER BY occurred_at DESC
         LIMIT 1",
    )
    .bind(handles)
    .bind(person_id)
    .fetch_optional(pool)
    .await
}

/// With no thread to go by, a phone before an email: iMessage and SMS both
/// reach a phone.
fn fallback_handle(handles: &[String]) -> String {
    handles.iter().find(|h| h.starts_with('+')).or_else(|| handles.first()).cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn person(pool: &PgPool, id: &str, name: &str, handles: &[&str]) {
        sqlx::query("INSERT INTO wiki_people (id, name, handles) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(name)
            .bind(json!(handles))
            .execute(pool)
            .await
            .unwrap();
    }

    async fn message(pool: &PgPool, id: &str, thread: &str, handle: &str, minutes_ago: i64) {
        sqlx::query(
            "INSERT INTO data_communication_message
               (id, message_id, thread_id, channel, body, from_identifier, from_handle,
                occurred_at, source_stream_id, source_table, source_provider)
             VALUES ($1, $1, $2, 'imessage', 'hi', $3, $3,
                     now() - make_interval(mins => $4::int), $1, 'messages', 'mac')",
        )
        .bind(id)
        .bind(thread)
        .bind(handle)
        .bind(minutes_ago as i32)
        .execute(pool)
        .await
        .unwrap();
    }

    #[test]
    fn a_phone_goes_before_an_email() {
        let hs = vec!["nick@example.com".to_string(), "+15125550101".to_string()];
        assert_eq!(fallback_handle(&hs), "+15125550101");
        assert_eq!(fallback_handle(&["nick@example.com".to_string()]), "nick@example.com");
    }

    #[sqlx::test]
    async fn a_name_resolves_to_the_latest_thread(pool: PgPool) {
        person(&pool, "person_nick", "Nick Example", &["+15125550101", "nick@example.com"]).await;
        message(&pool, "m1", "any;-;nick@example.com", "nick@example.com", 60).await;
        message(&pool, "m2", "any;-;+15125550101", "+15125550101", 5).await;

        let t = resolve(&pool, "nick").await.unwrap().unwrap();
        assert_eq!(t.person_id.as_deref(), Some("person_nick"));
        assert_eq!(t.chat_id.as_deref(), Some("any;-;+15125550101"));
        assert_eq!(t.handle, "+15125550101");
    }

    #[sqlx::test]
    async fn a_shared_first_name_is_never_guessed(pool: PgPool) {
        person(&pool, "person_a", "Sam Okafor", &["+15125550102"]).await;
        person(&pool, "person_b", "Sam Example", &["+15125550103"]).await;

        let why = resolve(&pool, "sam").await.unwrap().unwrap_err();
        assert!(why.contains("person_a") && why.contains("person_b"), "{why}");
        // The full name is not ambiguous.
        let t = resolve(&pool, "Sam Okafor").await.unwrap().unwrap();
        assert_eq!(t.person_id.as_deref(), Some("person_a"));
        // No thread yet: the handle is the way in.
        assert_eq!(t.chat_id, None);
        assert_eq!(t.handle, "+15125550102");
    }

    #[sqlx::test]
    async fn a_number_needs_no_person(pool: PgPool) {
        let t = resolve(&pool, "(512) 555-0104").await.unwrap().unwrap();
        assert_eq!(t, Target { person_id: None, name: None, handle: "+15125550104".into(), chat_id: None });
        assert!(resolve(&pool, "Nobody Here").await.unwrap().is_err());
    }
}

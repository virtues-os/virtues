//! Send an iMessage as the owner, from the chat.
//!
//! The box holds the record and the Mac holds Messages. This tool works out
//! who `to` names and which thread the owner already has with them, and puts
//! the message in the outbox (`outbox.rs`), which sends it through the Mac
//! app after the undo window or at the time asked for. It is offered only
//! while that app is connected (`ChatMode::tools`).
//!
//! It does not ask first: the owner's call, 2026-10-20. The card in the chat
//! can cancel it for five seconds. And it never guesses who: a name that fits
//! two people, or nobody, comes back as an error naming who it could be, and
//! the model asks the owner.

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use serde_json::{json, Value};
use sqlx::PgPool;

use super::{ToolContext, ToolResult};

pub const TOOL: &str = "send_imessage";

/// How far ahead a message can be scheduled.
const MAX_AHEAD: chrono::Duration = chrono::Duration::days(30);

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

pub async fn run(pool: &PgPool, args: &Value, context: &ToolContext) -> ToolResult {
    let to = args.get("to").and_then(|v| v.as_str()).unwrap_or("").trim();
    let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("").trim();
    if to.is_empty() {
        return ToolResult::error("Say who the message is for in `to`.");
    }
    let refs: Vec<&str> = args
        .get("attachments")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|r| r.as_str()).filter(|r| !r.trim().is_empty()).collect())
        .unwrap_or_default();
    if text.is_empty() && refs.is_empty() {
        return ToolResult::error("There is nothing to send: `text` is empty and there are no attachments.");
    }
    if text.chars().count() > MAX_CHARS {
        return ToolResult::error(format!("That is longer than {MAX_CHARS} characters. Shorten it."));
    }

    let now = Utc::now();
    let scheduled = match args.get("at").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()) {
        None => None,
        Some(raw) => match parse_at(raw, context.timezone.as_deref(), now) {
            Ok(at) => Some(at),
            Err(why) => return ToolResult::error(why),
        },
    };

    let target = match resolve(pool, to).await {
        Ok(Ok(t)) => t,
        Ok(Err(why)) => return ToolResult::error(why),
        Err(e) => return ToolResult::error(format!("Could not read your contacts: {e}")),
    };

    // Read every file now, so a reference that resolves to nothing is the
    // model's to fix, not a failed send later.
    let mut attachments = Vec::new();
    let mut total = 0;
    for r in &refs {
        match crate::outbox::attachment(pool, r).await {
            Ok(a) => {
                total += a["size_bytes"].as_u64().unwrap_or(0) as usize;
                attachments.push(a);
            }
            Err(why) => return ToolResult::error(why),
        }
    }
    if total > crate::outbox::MAX_ATTACHMENT_BYTES {
        return ToolResult::error("The files are larger than 25 MB together. Send fewer, or smaller ones.");
    }

    let recipient = target.name.clone().unwrap_or_else(|| target.handle.clone());
    let send_at = scheduled.unwrap_or_else(|| now + crate::outbox::UNDO_WINDOW);
    let queued = crate::outbox::enqueue(
        pool,
        crate::outbox::NewMessage {
            chat_id: context.chat_id.as_deref(),
            person_id: target.person_id.as_deref(),
            recipient: &recipient,
            handle: &target.handle,
            thread_id: target.chat_id.as_deref(),
            body: text,
            send_at,
            attachments: json!(attachments),
        },
    )
    .await;
    let queued = match queued {
        Ok(q) => q,
        Err(e) => return ToolResult::error(format!("Could not queue the message: {e}")),
    };

    let note = if scheduled.is_some() {
        "Scheduled. The chat shows the owner the message and when it goes, with a way to cancel. \
         Confirm the time in a few words; do not repeat the message."
    } else {
        "It goes in five seconds unless the owner cancels it on the card the chat shows them. \
         Say it is on its way, not that it was sent or delivered, in a few words; do not repeat \
         the message."
    };
    ToolResult::success(json!({
        "outbox_id": queued.id,
        "state": queued.state,
        "to": recipient,
        "person_id": target.person_id,
        "handle": target.handle,
        "text": text,
        "attachments": queued.attachments,
        "send_at": queued.send_at.to_rfc3339(),
        "scheduled": scheduled.is_some(),
        "note": note,
    }))
}

/// A time to send at: RFC 3339 with an offset, or a local date and time in the
/// owner's timezone. Within a minute of now counts as now.
fn parse_at(raw: &str, tz: Option<&str>, now: DateTime<Utc>) -> Result<DateTime<Utc>, String> {
    let at = if let Ok(at) = DateTime::parse_from_rfc3339(raw) {
        at.with_timezone(&Utc)
    } else {
        let local = ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M"]
            .iter()
            .find_map(|f| NaiveDateTime::parse_from_str(raw, f).ok())
            .ok_or_else(|| format!("\"{raw}\" is not a time. Pass `at` as 2026-10-21T09:00 in the owner's time."))?;
        let tz: chrono_tz::Tz = tz.and_then(|t| t.parse().ok()).unwrap_or(chrono_tz::UTC);
        tz.from_local_datetime(&local)
            .earliest()
            .ok_or_else(|| format!("{raw} does not exist in {tz} (the clocks skip it). Pick another time."))?
            .with_timezone(&Utc)
    };
    if at < now - chrono::Duration::minutes(1) {
        return Err(format!("{} has already passed. Ask the owner when to send it.", at.to_rfc3339()));
    }
    if at > now + MAX_AHEAD {
        return Err("That is more than 30 days away. Messages can be scheduled up to 30 days ahead.".into());
    }
    Ok(at.max(now))
}

/// `Ok(Err(..))` is an answer for the model: nobody, or more than one person.
async fn resolve(pool: &PgPool, to: &str) -> sqlx::Result<Result<Target, String>> {
    // A thread id from the record, which is how an unnamed group is reached:
    // the model finds the thread by who is in it, then passes its id.
    if to.contains(";-;") || to.contains(";+;") {
        let title: Option<(Option<String>,)> = sqlx::query_as(
            "SELECT metadata->>'group_title' FROM data_communication_message
              WHERE thread_id = $1 ORDER BY occurred_at DESC LIMIT 1",
        )
        .bind(to)
        .fetch_optional(pool)
        .await?;
        let Some((title,)) = title else {
            return Ok(Err(format!("There is no thread {to} in the owner's messages.")));
        };
        // absent-ok: most groups have no name; the thread was found, so it exists.
        let name = title.filter(|t| !t.is_empty()).unwrap_or_else(|| "the group".into());
        return Ok(Ok(Target { person_id: None, name: Some(name), handle: String::new(), chat_id: Some(to.into()) }));
    }

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
            if let Some(group) = named_group(pool, to).await? {
                return Ok(Ok(group));
            }
            return Ok(Err(format!(
                "Nobody in the owner's people, and no group chat, is called \"{to}\". Ask the owner who \
                 they mean, or pass a phone number or email. For a group with no name, find its thread_id \
                 in data_communication_message (is_group_message) by who is in it, and pass that."
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

/// A group chat by the name someone gave it in Messages, newest first. Most
/// groups have no name; those are reached by thread id.
async fn named_group(pool: &PgPool, to: &str) -> sqlx::Result<Option<Target>> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT thread_id, metadata->>'group_title' FROM data_communication_message
          WHERE is_group_message AND lower(metadata->>'group_title') = lower($1)
          ORDER BY occurred_at DESC LIMIT 1",
    )
    .bind(to)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(thread, title)| Target { person_id: None, name: Some(title), handle: String::new(), chat_id: Some(thread) }))
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
    fn a_local_time_is_the_owners() {
        let now = Utc.with_ymd_and_hms(2026, 10, 20, 12, 0, 0).unwrap();
        let at = parse_at("2026-10-21T09:00", Some("America/Chicago"), now).unwrap();
        assert_eq!(at, Utc.with_ymd_and_hms(2026, 10, 21, 14, 0, 0).unwrap());
        let at = parse_at("2026-10-21T09:00:00-05:00", None, now).unwrap();
        assert_eq!(at, Utc.with_ymd_and_hms(2026, 10, 21, 14, 0, 0).unwrap());
        assert!(parse_at("2026-10-19T09:00", Some("America/Chicago"), now).is_err());
        assert!(parse_at("2027-01-01T09:00", None, now).is_err());
        assert!(parse_at("tomorrow at nine", None, now).is_err());
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
    async fn a_group_is_reached_by_its_name_or_its_thread(pool: PgPool) {
        sqlx::query(
            "INSERT INTO data_communication_message
               (id, message_id, thread_id, channel, body, from_identifier, from_handle, is_group_message,
                occurred_at, source_stream_id, source_table, source_provider, metadata)
             VALUES ('g1', 'g1', 'any;+;chat100', 'imessage', 'hi', '+15125550101', '+15125550101', true,
                     now(), 'g1', 'messages', 'mac', '{\"group_title\": \"Dinner Club\"}'),
                    ('g2', 'g2', 'any;+;chat200', 'imessage', 'hi', '+15125550102', '+15125550102', true,
                     now(), 'g2', 'messages', 'mac', '{}')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let t = resolve(&pool, "dinner club").await.unwrap().unwrap();
        assert_eq!((t.name.as_deref(), t.chat_id.as_deref()), (Some("Dinner Club"), Some("any;+;chat100")));
        let t = resolve(&pool, "any;+;chat200").await.unwrap().unwrap();
        assert_eq!((t.name.as_deref(), t.handle.as_str()), (Some("the group"), ""));
        assert!(resolve(&pool, "any;+;chat999").await.unwrap().is_err());
    }

    #[sqlx::test]
    async fn a_number_needs_no_person(pool: PgPool) {
        let t = resolve(&pool, "(512) 555-0104").await.unwrap().unwrap();
        assert_eq!(t, Target { person_id: None, name: None, handle: "+15125550104".into(), chat_id: None });
        assert!(resolve(&pool, "Nobody Here").await.unwrap().is_err());
    }
}

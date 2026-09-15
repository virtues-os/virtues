//! Reply from the record.
//!
//! Given a message thread, decide whether its latest message is asking the
//! owner for something the record can answer — an address, a phone number, a
//! link they sent once, when they are free — and if so, draft the reply in
//! the owner's own voice and park it in `app_message_replies` for a device to
//! show, send, or dismiss. This module never sends anything.
//!
//! It is channel-neutral: a thread is rows of `data_communication_message`
//! sharing a `thread_id`, whoever ingested them. iMessage from the Mac is the
//! first channel; email is the same shape.
//!
//! **One model call, and only when asked.** There is no watcher here and no
//! gate. An earlier build ran a cheap model over every arriving message to
//! decide whether to offer a draft; it was deleted. A box that reasons about
//! every message as it lands is doing work nobody requested — the cost is the
//! smaller half of the objection, and the larger half is that the owner never
//! asked and cannot tell it to stop. Pressing the button IS the decision, so
//! there is nothing left to decide.
//!
//! Retrieval is deterministic, not a tool loop: the ask is matched by name
//! against the record's places and people, and run once through hybrid
//! search. Far more predictable than letting the model call tools, and the
//! day summary already established the pattern.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use virtues::search::query::SearchOptions;
use virtues::search::SemanticSearchEngine;
use virtues::virtues_api::completion::system_completion;
use virtues::virtues_api::request::Thinking;
use virtues_registry::models::ModelSlot;

/// How much of a thread the model sees. Twenty is enough to know what "it"
/// refers to; a hundred is noise and cost.
const THREAD_WINDOW: i64 = 20;
/// The owner's own recent messages in this thread, as the voice sample.
const VOICE_SAMPLE: i64 = 12;
const SEARCH_HITS: i64 = 8;
const ARTICLE_CAP: usize = 1500;
const PREVIEW_CAP: usize = 400;

#[derive(Debug, Default, Serialize)]
pub struct Outcome {
    pub considered: usize,
    pub drafted: usize,
    pub skipped: usize,
    pub skipped_why: Vec<String>,
}

#[derive(Debug, Clone)]
struct ThreadMessage {
    message_id: String,
    body: String,
    from_handle: String,
    from_name: Option<String>,
    is_from_me: bool,
    is_group: bool,
    occurred_at: DateTime<Utc>,
}

#[derive(Debug, Default)]
struct Owner {
    name: Option<String>,
    full_name: Option<String>,
    occupation: Option<String>,
    employer: Option<String>,
    home_name: Option<String>,
    home_address: Option<String>,
    timezone: Option<String>,
}

#[derive(Debug, Default)]
struct Counterpart {
    name: Option<String>,
    relationship: Option<String>,
    notes: Option<String>,
    article: Option<String>,
}

/// Consider every thread in the list. Errors on one thread are logged and
/// counted, never fatal: a batch of five threads with one bad row still
/// drafts the other four.
pub async fn consider_threads(pool: &PgPool, thread_ids: &[String]) -> Result<Outcome> {
    let mut out = Outcome::default();
    for thread_id in thread_ids {
        out.considered += 1;
        match consider_thread(pool, thread_id).await {
            Ok(Some(id)) => {
                tracing::info!(thread_id, reply_id = %id, "drafted a reply");
                out.drafted += 1;
            }
            Ok(None) => out.skipped += 1,
            Err(e) => {
                tracing::warn!(thread_id, error = %e, "could not consider thread");
                out.skipped += 1;
                out.skipped_why.push(format!("{thread_id}: {e:#}"));
            }
        }
    }
    Ok(out)
}

/// The thread whose most recent inbound message is newest — what "take care
/// of this" means when the owner presses it without naming a thread. Scoped
/// to a channel: the Mac's button means Messages, and an email thread handed
/// back here could not be sent from there.
pub async fn latest_inbound_thread(pool: &PgPool, channel: &str) -> Result<Option<String>> {
    let row = sqlx::query(
        "SELECT thread_id FROM data_communication_message \
         WHERE thread_id IS NOT NULL AND thread_id <> '' \
           AND channel = $1 \
           AND reply_to_message_id IS NULL \
           AND COALESCE((metadata->>'is_from_me')::boolean, from_identifier = 'me') = false \
           AND COALESCE(body, '') <> '' \
         ORDER BY occurred_at DESC LIMIT 1",
    )
    .bind(channel)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.get::<String, _>("thread_id")))
}

/// The closed loop. An outbound message in a thread with a pending draft
/// answers the ask, whoever typed it: `sent` when the text is the draft (or
/// the edit the Mac reported), `answered` when the owner said something else.
/// Either way the row keeps what actually went out, which is how the drafter
/// gets measured against real replies. Called from ingest, so a reply typed
/// on the phone settles the row just as one sent from the Mac does.
///
/// Returns how many rows were resolved.
pub async fn resolve_from_outbound(
    pool: &PgPool,
    outbound: &[(String, String, DateTime<Utc>)],
) -> Result<u64> {
    let mut resolved = 0u64;
    for (thread_id, body, occurred_at) in outbound {
        let body = body.trim();
        if body.is_empty() {
            continue;
        }
        let n = sqlx::query(
            "UPDATE app_message_replies \
             SET status = CASE WHEN btrim($2) = btrim(COALESCE(sent_text, draft)) \
                               THEN 'sent' ELSE 'answered' END, \
                 sent_text = $2, \
                 sent_at = COALESCE(sent_at, $3), \
                 updated_at = now() \
             WHERE thread_id = $1 AND status = 'pending' AND created_at <= $3",
        )
        .bind(thread_id)
        .bind(body)
        .bind(occurred_at)
        .execute(pool)
        .await?
        .rows_affected();
        resolved += n;
    }
    Ok(resolved)
}

/// One drafter per thread at a time. Ingest now flushes per message, so two
/// batches seconds apart each start a drafter for the same thread; without
/// this both pay the model and the second overwrites a draft the Mac may
/// already be showing. A session lock on a dedicated connection, held for
/// the whole consideration; the loser returns at once and the winner's row
/// is what the loser would have seen had it waited.
async fn consider_thread(pool: &PgPool, thread_id: &str) -> Result<Option<String>> {
    let mut lock_conn = pool.acquire().await?;
    let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtext($1))")
        .bind(format!("message_reply|{thread_id}"))
        .fetch_one(&mut *lock_conn)
        .await?;
    if !locked {
        tracing::info!(thread_id, "another drafter holds this thread");
        return Ok(None);
    }
    let result = consider_thread_locked(pool, thread_id).await;
    // Unlock on the same connection, whatever happened; dropping the
    // connection back to the pool would release it too, but explicitly.
    let _ = sqlx::query("SELECT pg_advisory_unlock(hashtext($1))")
        .bind(format!("message_reply|{thread_id}"))
        .execute(&mut *lock_conn)
        .await;
    result
}

async fn consider_thread_locked(pool: &PgPool, thread_id: &str) -> Result<Option<String>> {
    let thread = load_thread(pool, thread_id).await?;
    let Some(latest) = thread.first() else {
        tracing::info!(thread_id, "empty thread");
        return Ok(None);
    };
    if latest.is_from_me {
        // The owner had the last word. Nothing is waiting on them.
        tracing::info!(thread_id, "latest message is the owner's — nothing pending");
        return Ok(None);
    }
    if latest.body.trim().is_empty() {
        tracing::info!(thread_id, "latest message has no text");
        return Ok(None);
    }

    // Asking again for a thread already answered is not a mistake to correct —
    // the owner pressed the button twice, so draft again. The one exception is
    // a draft already sent: that conversation moved on, and re-answering the
    // same message would be a second reply to it.
    let already: Option<String> = sqlx::query_scalar(
        "SELECT status FROM app_message_replies WHERE thread_id = $1 AND ask_stream_id = $2",
    )
    .bind(thread_id)
    .bind(&latest.message_id)
    .fetch_optional(pool)
    .await?;
    if already.as_deref() == Some("sent") {
        tracing::info!(thread_id, "this message was already answered");
        return Ok(None);
    }

    // `?`, not a fallback: a failed query here is a bug against the schema,
    // and a draft written without the owner or the counterpart would look
    // plausible and be wrong. Absence is already `None` from fetch_optional.
    let owner = load_owner(pool).await?;
    let counterpart = load_counterpart(pool, latest).await?;
    let transcript = render_transcript(&thread, &owner);

    let voice = load_voice_sample(pool, thread_id).await?;
    let mut hits = named_facts(pool, &latest.body).await?;
    hits.extend(search_record(pool, &latest.body, &counterpart).await);

    let drafted = draft(pool, &owner, &counterpart, &transcript, latest, &voice, &hits).await?;
    let Some(reply) = drafted.reply.filter(|r| is_a_message(r)) else {
        tracing::info!(thread_id, why = ?drafted.rationale, "drafter: nothing to add");
        return Ok(None);
    };

    let id = virtues_helpers::ids::generate_id("reply", &[thread_id, &latest.message_id]);
    sqlx::query(
        "INSERT INTO app_message_replies \
           (id, thread_id, ask_stream_id, ask_text, from_handle, from_name, is_group, \
            draft, rationale, status) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'pending') \
         ON CONFLICT (thread_id, ask_stream_id) DO UPDATE \
           SET draft = EXCLUDED.draft, rationale = EXCLUDED.rationale, \
               status = 'pending', sent_text = NULL, sent_at = NULL, updated_at = now()",
    )
    .bind(&id)
    .bind(thread_id)
    .bind(&latest.message_id)
    .bind(latest.body.trim())
    .bind(&latest.from_handle)
    .bind(counterpart.name.as_deref().or(latest.from_name.as_deref()))
    .bind(latest.is_group)
    .bind(reply.trim())
    .bind(drafted.rationale.as_deref())
    .execute(pool)
    .await?;
    Ok(Some(id))
}

// ── Loading ──────────────────────────────────────────────────────────────────

/// Newest first.
async fn load_thread(pool: &PgPool, thread_id: &str) -> Result<Vec<ThreadMessage>> {
    let rows = sqlx::query(
        "SELECT message_id, COALESCE(body, '') AS body, COALESCE(from_handle, '') AS from_handle, \
                from_name, is_group_message, occurred_at, \
                COALESCE((metadata->>'is_from_me')::boolean, from_identifier = 'me') AS is_from_me \
         FROM data_communication_message \
         WHERE thread_id = $1 AND reply_to_message_id IS NULL \
         ORDER BY occurred_at DESC LIMIT $2",
    )
    .bind(thread_id)
    .bind(THREAD_WINDOW)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ThreadMessage {
            message_id: r.get("message_id"),
            body: r.get("body"),
            from_handle: r.get("from_handle"),
            from_name: r.get("from_name"),
            is_from_me: r.get("is_from_me"),
            is_group: r.get("is_group_message"),
            occurred_at: r.get("occurred_at"),
        })
        .collect())
}

async fn load_owner(pool: &PgPool) -> Result<Owner> {
    let row = sqlx::query(
        "SELECT p.preferred_name, p.full_name, p.occupation, p.employer, p.home_timezone, \
                h.name AS home_name, h.address AS home_address \
         FROM app_user_profile p \
         LEFT JOIN wiki_places h ON h.id = p.home_place_id \
         LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    let Some(r) = row else { return Ok(Owner::default()) };
    Ok(Owner {
        name: r.get("preferred_name"),
        full_name: r.get("full_name"),
        occupation: r.get("occupation"),
        employer: r.get("employer"),
        home_name: r.get("home_name"),
        home_address: r.get("home_address"),
        timezone: r.get("home_timezone"),
    })
}

async fn load_counterpart(pool: &PgPool, latest: &ThreadMessage) -> Result<Counterpart> {
    if latest.from_handle.is_empty() {
        return Ok(Counterpart { name: latest.from_name.clone(), ..Default::default() });
    }
    // `name`, not `canonical_name` — renamed in migration 0002; the initial
    // schema file still shows the old spelling.
    let row = sqlx::query(
        "SELECT name, relationship_category, notes, article \
         FROM wiki_people \
         WHERE handles ? $1 \
         ORDER BY seen_count DESC LIMIT 1",
    )
    .bind(&latest.from_handle)
    .fetch_optional(pool)
    .await?;
    let Some(r) = row else {
        return Ok(Counterpart { name: latest.from_name.clone(), ..Default::default() });
    };
    Ok(Counterpart {
        name: Some(r.get::<String, _>("name")),
        relationship: r.get("relationship_category"),
        notes: r.get("notes"),
        article: r.get::<Option<String>, _>("article").map(|a| cap(&a, ARTICLE_CAP)),
    })
}

async fn load_voice_sample(pool: &PgPool, thread_id: &str) -> Result<Vec<String>> {
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT body FROM data_communication_message \
         WHERE thread_id = $1 AND reply_to_message_id IS NULL \
           AND COALESCE((metadata->>'is_from_me')::boolean, from_identifier = 'me') = true \
           AND length(btrim(COALESCE(body, ''))) >= 8 \
         ORDER BY occurred_at DESC LIMIT $2",
    )
    .bind(thread_id)
    .bind(VOICE_SAMPLE)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Places and people the ask NAMES, looked up by name rather than by vector.
///
/// The asks people actually text are mostly about named things — "what's the
/// address of that ramen place", "do you have Nick's number" — and the answer
/// is a structured row, not a passage. Hybrid search finds those only where
/// the box has an index; a fresh box, or one mid-reindex, has none, and the
/// drafter would answer "I don't have that" about a row sitting in plain
/// sight. This pass costs one indexed query and works on any box.
///
/// Only entities the record already holds are matched, so this cannot invent
/// a place; it can only remember one.
async fn named_facts(pool: &PgPool, ask: &str) -> Result<Vec<String>> {
    // Words the ask might be naming. Short words and the common furniture of a
    // question match everything and mean nothing.
    const STOPWORDS: &[&str] = &[
        "what", "whats", "where", "when", "which", "that", "this", "your", "yours", "you",
        "the", "there", "here", "have", "with", "from", "about", "again", "please", "send",
        "address", "number", "phone", "email", "time", "place", "thing", "know", "tell",
        "took", "went", "give", "just", "like", "they", "them", "were", "was", "for",
    ];
    let words: Vec<String> = ask
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .map(|w| w.trim_matches('\'').to_lowercase())
        .filter(|w| w.chars().count() >= 4 && !STOPWORDS.contains(&w.as_str()))
        .take(12)
        .collect();
    if words.is_empty() {
        return Ok(Vec::new());
    }
    let patterns: Vec<String> = words.iter().map(|w| format!("%{w}%")).collect();

    let mut out = Vec::new();

    let places = sqlx::query(
        "SELECT name, address, category FROM wiki_places \
         WHERE name ILIKE ANY($1) OR aliases::text ILIKE ANY($1) \
         ORDER BY seen_count DESC LIMIT 4",
    )
    .bind(&patterns)
    .fetch_all(pool)
    .await?;
    for r in places {
        let name: String = r.get("name");
        let address: Option<String> = r.get("address");
        let category: Option<String> = r.get("category");
        out.push(format!(
            "[place] {name}{}{}",
            category.map(|c| format!(" ({c})")).unwrap_or_default(),
            address.map(|a| format!(" — {a}")).unwrap_or_default()
        ));
    }

    let people = sqlx::query(
        "SELECT name, relationship_category, handles, emails, phones FROM wiki_people \
         WHERE name ILIKE ANY($1) OR aliases::text ILIKE ANY($1) \
         ORDER BY seen_count DESC LIMIT 4",
    )
    .bind(&patterns)
    .fetch_all(pool)
    .await?;
    for r in people {
        let name: String = r.get("name");
        let rel: Option<String> = r.get("relationship_category");
        let contacts = [
            r.get::<Value, _>("emails"),
            r.get::<Value, _>("phones"),
            r.get::<Value, _>("handles"),
        ]
        .iter()
        .filter_map(|v| v.as_array())
        .flatten()
        .filter_map(Value::as_str)
        .take(4)
        .collect::<Vec<_>>()
        .join(", ");
        out.push(format!(
            "[person] {name}{}{}",
            rel.map(|c| format!(" ({c})")).unwrap_or_default(),
            if contacts.is_empty() { String::new() } else { format!(" — {contacts}") }
        ));
    }

    Ok(out)
}

/// Hybrid search over the record with the ask as the query. Search being down
/// (no embedder, an index mid-rebuild) must not stop a draft that the thread
/// alone can answer, so this degrades to no hits with a warning.
async fn search_record(pool: &PgPool, ask: &str, who: &Counterpart) -> Vec<String> {
    let mut queries = vec![ask.trim().to_string()];
    if let Some(name) = &who.name {
        queries.push(format!("{name} {}", ask.trim()));
    }
    let engine = SemanticSearchEngine::new(Arc::new(pool.clone()));
    let opts = SearchOptions { limit: Some(SEARCH_HITS), ..Default::default() };
    match engine.search_multi(&queries, &opts).await {
        Ok(results) => results
            .into_iter()
            .filter(|r| r.ontology != "communication_message")
            .map(|r| {
                let when = r.timestamp.as_deref().unwrap_or("");
                let title = r.title.as_deref().unwrap_or("");
                let text = r
                    .content
                    .as_deref()
                    .or(r.preview.as_deref())
                    .unwrap_or("");
                format!("[{}] {} {} — {}", r.ontology, when, title, cap(text, PREVIEW_CAP))
            })
            .collect(),
        Err(e) => {
            tracing::warn!(error = %e, "record search unavailable — drafting from the thread alone");
            Vec::new()
        }
    }
}

// ── Model calls ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct Draft {
    #[serde(default)]
    reply: Option<String>,
    #[serde(default)]
    rationale: Option<String>,
}

async fn draft(
    pool: &PgPool,
    owner: &Owner,
    who: &Counterpart,
    transcript: &str,
    latest: &ThreadMessage,
    voice: &[String],
    hits: &[String],
) -> Result<Draft> {
    let owner_name = owner.name.as_deref().or(owner.full_name.as_deref()).unwrap_or("the owner");
    let system = format!(
        "\
You write ONE text message on behalf of {owner_name}, in their voice, answering the latest \
message in a thread. You are not an assistant talking to them; you are them, replying to the \
other person. The owner will read it and press send, or not.

Rules:
- Answer the latest ask and nothing else. One message, plain text, no greeting, no \
sign-off, no markdown, no emoji unless the owner's own messages use them.
- Never volunteer a detail nobody asked for. Asked for one thing, give that one thing. An \
address, an email, a phone number or a plan that was not requested must not appear in the \
reply — adding one is worse than saying nothing, because the owner is about to send it.
- Every fact must appear, in those words, in the material below. If it is not written there \
you do not know it: return null rather than something plausible. This binds hardest on \
addresses, emails, phone numbers, times and dates — a guessed one looks exactly like a real \
one and is sent as if true.
- Sound like the owner's own messages in this thread: their length, punctuation, \
capitalization, warmth. Short beats complete.
- The owner's own everyday details are theirs to give out when asked for them: their home \
address, their email, their phone number, where they work, when they are free. Give them to \
someone the record knows — a name, a relationship, a history of talking. That is the ordinary \
case, not an exception.
- Withhold those details when the counterpart is a stranger to the record, or when the \
thread reads like a stranger, a scam, or a pretext. Then return null.
- Never share, whoever is asking: bank, card or account numbers; passwords or verification \
codes; health details; another person's private information; or a history of where the owner \
has been. For those, return null.
- Do not answer for anyone else in a group.

Reply with JSON only: {{\"reply\": \"the message\" | null, \"rationale\": \"one short line on \
what you used, or why not\"}}"
    );

    let mut user = String::new();
    user.push_str("## Owner\n");
    push_kv(&mut user, "Name", owner.full_name.as_deref().or(owner.name.as_deref()));
    push_kv(&mut user, "Goes by", owner.name.as_deref());
    push_kv(&mut user, "Occupation", owner.occupation.as_deref());
    push_kv(&mut user, "Employer", owner.employer.as_deref());
    push_kv(&mut user, "Home", owner.home_name.as_deref());
    push_kv(&mut user, "Home address", owner.home_address.as_deref());
    push_kv(&mut user, "Time zone", owner.timezone.as_deref());
    let now = match owner.timezone.as_deref().and_then(|tz| tz.parse::<chrono_tz::Tz>().ok()) {
        Some(tz) => Utc::now().with_timezone(&tz).format("%A %Y-%m-%d %H:%M").to_string(),
        None => Utc::now().format("%A %Y-%m-%d %H:%M UTC").to_string(),
    };
    push_kv(&mut user, "Now", Some(&now));

    user.push_str("\n## Counterpart\n");
    push_kv(&mut user, "Name", who.name.as_deref().or(latest.from_name.as_deref()));
    push_kv(&mut user, "Handle", Some(&latest.from_handle));
    push_kv(&mut user, "Relationship", who.relationship.as_deref());
    push_kv(&mut user, "Notes", who.notes.as_deref());
    if let Some(article) = &who.article {
        user.push_str("About them:\n");
        user.push_str(article);
        user.push('\n');
    }
    push_kv(&mut user, "Group thread", Some(if latest.is_group { "yes" } else { "no" }));

    user.push_str("\n## Thread (oldest first, latest last)\n");
    user.push_str(transcript);

    if !voice.is_empty() {
        user.push_str("\n## How the owner writes to this person (their recent messages)\n");
        for v in voice.iter().rev() {
            user.push_str("- ");
            user.push_str(v.trim());
            user.push('\n');
        }
    }

    user.push_str("\n## From the owner's records\n");
    if hits.is_empty() {
        user.push_str("(none)\n");
    } else {
        for h in hits {
            user.push_str("- ");
            user.push_str(h);
            user.push('\n');
        }
    }

    user.push_str("\n## Latest message to answer\n");
    user.push_str(latest.body.trim());
    user.push('\n');

    let raw = system_completion(pool, ModelSlot::Chat, "message_reply_draft", &system, &user, Thinking::Low, 0.4)
        .await
        .context("draft completion")?;
    parse_json::<Draft>(&raw).context("drafter returned no JSON")
}

// ── Rendering + parsing ──────────────────────────────────────────────────────

fn render_transcript(thread: &[ThreadMessage], owner: &Owner) -> String {
    let me = owner.name.as_deref().unwrap_or("Me");
    let mut lines = Vec::with_capacity(thread.len());
    for m in thread.iter().rev() {
        let who = if m.is_from_me {
            me.to_string()
        } else {
            m.from_name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| m.from_handle.clone())
        };
        let body = if m.body.trim().is_empty() { "(no text)" } else { m.body.trim() };
        lines.push(format!("[{}] {}: {}", m.occurred_at.format("%Y-%m-%d %H:%M"), who, body));
    }
    lines.join("\n")
}

fn push_kv(out: &mut String, key: &str, value: Option<&str>) {
    if let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) {
        out.push_str(key);
        out.push_str(": ");
        out.push_str(v);
        out.push('\n');
    }
}

/// "Nothing to add" reaches us two ways: a JSON `null`, and — because models
/// asked for text often answer in text — the WORD null. A draft of "null"
/// once made it into a pending row, ready to send to someone. Anything that
/// is only a way of saying nothing is nothing.
fn is_a_message(reply: &str) -> bool {
    let t = reply.trim();
    if t.is_empty() {
        return false;
    }
    let bare = t.trim_matches(|c: char| c == '"' || c == '\'' || c == '.').to_ascii_lowercase();
    !matches!(
        bare.as_str(),
        "null" | "none" | "nil" | "n/a" | "na" | "nothing" | "no reply" | "no response" | "undefined"
    )
}

fn cap(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let cut: String = s.chars().take(n).collect();
    format!("{cut}…")
}

/// Models fence JSON, preface it, or trail it with a remark. Take the first
/// balanced object in the text.
fn parse_json<T: for<'de> Deserialize<'de>>(raw: &str) -> Result<T> {
    let text = raw.trim();
    if let Ok(v) = serde_json::from_str::<T>(text) {
        return Ok(v);
    }
    let start = text.find('{').context("no '{' in model output")?;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, c) in text[start..].char_indices() {
        if in_str {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let slice = &text[start..start + i + c.len_utf8()];
                    return serde_json::from_str::<T>(slice).context("model JSON did not parse");
                }
            }
            _ => {}
        }
    }
    anyhow::bail!("unbalanced JSON in model output")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_word_null_is_not_a_message() {
        for nothing in ["null", "NULL", "\"null\"", " none ", "N/A", "nothing", "", "   "] {
            assert!(!is_a_message(nothing), "{nothing:?} should not be sendable");
        }
        for real in ["it's Mueller, Austin, TX", "no", "nothing much, you?", "yes"] {
            assert!(is_a_message(real), "{real:?} should be sendable");
        }
    }

    #[test]
    fn json_is_taken_from_a_fenced_or_prefaced_answer() {
        #[derive(Debug, Deserialize)]
        struct R {
            reply: Option<String>,
        }
        let raw = "Here you go:\n```json\n{\"reply\": \"sure, 3pm\"}\n```\nhope that helps";
        assert_eq!(parse_json::<R>(raw).unwrap().reply.as_deref(), Some("sure, 3pm"));
    }

    #[test]
    fn only_the_owners_own_messages_settle_a_draft() {
        let batch = vec![
            // theirs: not an answer to anything
            json!({"guid": "a", "chat_id": "t1", "text": "what time?", "is_from_me": false}),
            // the owner's tapback: a reaction, not an answer
            json!({"guid": "b", "chat_id": "t1", "text": "Loved \u{201c}ok\u{201d}",
                   "is_from_me": true, "associated_message_type": 2000}),
            // the owner actually replying
            json!({"guid": "c", "chat_id": "t1", "text": "7pm", "is_from_me": true}),
        ];
        let out = outbound_in_batch(&batch);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].1, "7pm");
    }
}

/// The messages in a batch that the OWNER sent, for settling drafts.
///
/// Inbound messages are deliberately NOT collected here any more: nothing is
/// triggered by a message arriving.
pub fn outbound_in_batch(imessages: &[Value]) -> Vec<(String, String, DateTime<Utc>)> {
    let mut outbound = Vec::new();
    for m in imessages {
        let Some(thread) = m
            .get("chat_id")
            .or_else(|| m.get("chat_guid"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let body = m.get("text").and_then(Value::as_str).unwrap_or("").trim();
        if body.is_empty() || !m.get("is_from_me").and_then(Value::as_bool).unwrap_or(false) {
            continue;
        }
        // A tapback ("Loved …") is a reaction, not an answer to anything.
        let reaction = m
            .get("associated_message_type")
            .and_then(Value::as_i64)
            .is_some_and(|t| t > 0)
            || m.get("associated_message_guid")
                .and_then(Value::as_str)
                .is_some_and(|g| !g.is_empty());
        if reaction {
            continue;
        }
        let when = m
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);
        outbound.push((thread.to_string(), body.to_string(), when));
    }
    outbound
}

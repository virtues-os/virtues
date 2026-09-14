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
//! Two model calls, deliberately split. The gate runs on the lite slot for
//! every inbound message and only asks "is there an open, answerable ask
//! here?" — cheap enough to run on every "lol". The draft runs on the chat
//! slot and only when the gate said yes. A manual request skips the gate:
//! the owner pressed the button, so they want a draft even for a thread the
//! gate would have passed over.
//!
//! Retrieval is deterministic, not a tool loop: the ask is run through the
//! record's hybrid search once and the hits are pasted into the prompt. Far
//! more predictable than letting the model call tools, and the day summary
//! already established the pattern.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use virtues::search::query::SearchOptions;
use virtues::search::SemanticSearchEngine;
use virtues::virtues_api::completion::system_completion;
use virtues::virtues_api::request::Thinking;
use virtues_registry::models::ModelSlot;

pub const TRIGGER_AUTO: &str = "auto";
pub const TRIGGER_MANUAL: &str = "manual";

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
pub async fn consider_threads(pool: &PgPool, thread_ids: &[String], trigger: &str) -> Result<Outcome> {
    let mut out = Outcome::default();
    for thread_id in thread_ids {
        out.considered += 1;
        match consider_thread(pool, thread_id, trigger).await {
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
async fn consider_thread(pool: &PgPool, thread_id: &str, trigger: &str) -> Result<Option<String>> {
    let mut lock_conn = pool.acquire().await?;
    let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtext($1))")
        .bind(format!("message_reply|{thread_id}"))
        .fetch_one(&mut *lock_conn)
        .await?;
    if !locked {
        tracing::info!(thread_id, "another drafter holds this thread");
        return Ok(None);
    }
    let result = consider_thread_locked(pool, thread_id, trigger).await;
    // Unlock on the same connection, whatever happened; dropping the
    // connection back to the pool would release it too, but explicitly.
    let _ = sqlx::query("SELECT pg_advisory_unlock(hashtext($1))")
        .bind(format!("message_reply|{thread_id}"))
        .execute(&mut *lock_conn)
        .await;
    result
}

async fn consider_thread_locked(pool: &PgPool, thread_id: &str, trigger: &str) -> Result<Option<String>> {
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

    let already: Option<String> = sqlx::query_scalar(
        "SELECT status FROM app_message_replies WHERE thread_id = $1 AND ask_stream_id = $2",
    )
    .bind(thread_id)
    .bind(&latest.message_id)
    .fetch_optional(pool)
    .await?;
    if let Some(status) = &already {
        // Auto never reconsiders an ask: the owner already saw it, or the gate
        // already passed. Manual is the owner asking again, which overrides
        // a dismissal or an expiry but not a send.
        let manual = trigger == TRIGGER_MANUAL;
        if !manual || status == "sent" || status == "pending" {
            tracing::info!(thread_id, status, "ask already considered");
            return Ok(None);
        }
    }

    // `?`, not a fallback: a failed query here is a bug against the schema,
    // and a draft written without the owner or the counterpart would look
    // plausible and be wrong. Absence is already `None` from fetch_optional.
    let owner = load_owner(pool).await?;
    let counterpart = load_counterpart(pool, latest).await?;
    let transcript = render_transcript(&thread, &owner);

    if trigger != TRIGGER_MANUAL {
        let gate = gate(pool, &transcript, latest, &counterpart).await?;
        if !gate.answerable {
            tracing::info!(thread_id, why = %gate.why, "gate: not answerable");
            return Ok(None);
        }
        tracing::info!(thread_id, ask = %gate.ask, "gate: answerable");
    }

    let voice = load_voice_sample(pool, thread_id).await?;
    let hits = search_record(pool, &latest.body, &counterpart).await;

    let drafted = draft(pool, &owner, &counterpart, &transcript, latest, &voice, &hits).await?;
    let Some(reply) = drafted.reply.filter(|r| !r.trim().is_empty()) else {
        tracing::info!(thread_id, why = ?drafted.rationale, "drafter: nothing to add");
        return Ok(None);
    };

    let id = virtues_helpers::ids::generate_id("reply", &[thread_id, &latest.message_id]);
    sqlx::query(
        "INSERT INTO app_message_replies \
           (id, thread_id, ask_stream_id, ask_text, from_handle, from_name, is_group, \
            draft, rationale, trigger, status) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'pending') \
         ON CONFLICT (thread_id, ask_stream_id) DO UPDATE \
           SET draft = EXCLUDED.draft, rationale = EXCLUDED.rationale, \
               trigger = EXCLUDED.trigger, status = 'pending', \
               sent_text = NULL, sent_at = NULL, updated_at = now()",
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
    .bind(trigger)
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
struct Gate {
    answerable: bool,
    #[serde(default)]
    ask: String,
    #[serde(default)]
    why: String,
}

async fn gate(
    pool: &PgPool,
    transcript: &str,
    latest: &ThreadMessage,
    who: &Counterpart,
) -> Result<Gate> {
    let system = "\
You read the tail of a text-message thread and decide one thing: does the LATEST message \
ask the owner of this phone for something concrete that could be answered from the owner's \
own records — an address, a phone number or email, a link or file they once sent, a date, a \
time, whether they are free, a name, a place, a fact about their own past or plans?

Answer YES only when all of these hold:
- the latest message is a request or question aimed at the owner (in a group, it must be \
addressed to them, not to someone else);
- it wants information, not an action in the world or an opinion or a feeling;
- the owner has not already answered it later in the thread.

Answer NO for small talk, reactions, statements, jokes, questions of taste or feeling, \
requests to do something physical, and anything the owner already answered.

Reply with JSON only: {\"answerable\": true|false, \"ask\": \"the ask in one short line\", \
\"why\": \"one short line\"}";
    let user = format!(
        "Counterpart: {}\nGroup thread: {}\n\nThread (oldest first, latest last):\n{}\n\nLatest message: {}",
        who.name.as_deref().unwrap_or("unknown"),
        if latest.is_group { "yes" } else { "no" },
        transcript,
        latest.body.trim()
    );
    let raw = system_completion(pool, ModelSlot::Lite, "message_reply_gate", system, &user, Thinking::Off, 0.0)
        .await
        .context("gate completion")?;
    parse_json::<Gate>(&raw).context("gate returned no JSON")
}

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
- Answer only the latest open ask. One message, plain text, no greeting, no sign-off, no \
markdown, no emoji unless the owner's own messages use them.
- Sound like the owner's own messages in this thread: their length, punctuation, \
capitalization, warmth. Short beats complete.
- Use only facts present below — the owner's profile, the record hits, the thread itself. \
Never invent a detail. If the records do not contain the answer, return null and say why.
- Never share: bank or card numbers, passwords or codes, health details, anyone else's \
private information, or the owner's location history. If the ask is for one of those, \
return null.
- If the ask is for the owner's address or phone number and the counterpart is a stranger or \
unknown, return null.
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

    user.push_str("\n## From the owner's records (search hits for the ask)\n");
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

// ── Detached spawn (used by ingest) ─────────────────────────────────────────

/// Start the `message_reply` binary on these threads and return at once.
///
/// Ingest runs inline under the collector's upload request and under the
/// runner's per-applet concurrency gate, so model calls cannot live there: a
/// ten-second draft would hold the Mac's HTTP request open and 409 the next
/// batch. The child gets its own process group so the runner's `kill_on_drop`
/// on the ingest process cannot reach it, and inherits the environment the
/// runner built (DATABASE_URL and the rest).
pub fn spawn_detached(thread_ids: &[String], trigger: &str) -> Result<()> {
    use std::io::Write;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    if thread_ids.is_empty() {
        return Ok(());
    }
    let exe = locate_sibling("message_reply")?;
    let payload = json!({ "thread_ids": thread_ids, "trigger": trigger });
    let input = json!({ "config": {}, "payload": payload });

    let mut child = Command::new(&exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .with_context(|| format!("spawn {}", exe.display()))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.to_string().as_bytes())?;
    }
    // Not waited on, on purpose. The child outlives this process.
    std::mem::forget(child);
    tracing::info!(threads = thread_ids.len(), exe = %exe.display(), "message_reply started");
    Ok(())
}

/// Applet binaries ship side by side, and the runner resolved this one from
/// the same directory it would resolve the sibling from.
fn locate_sibling(name: &str) -> Result<std::path::PathBuf> {
    if let Ok(dir) = std::env::var("VIRTUES_APPLETS_BIN_DIR") {
        let p = std::path::Path::new(&dir).join(name);
        if p.exists() {
            return Ok(p);
        }
    }
    let me = std::env::current_exe().context("current_exe")?;
    let p = me.parent().context("exe has no parent")?.join(name);
    if p.exists() {
        return Ok(p);
    }
    anyhow::bail!("no `{name}` binary beside {}", me.display())
}

/// Inbound and outbound messages of one collector batch, split for the two
/// things ingest does with them: outbound settles pending drafts, inbound
/// starts new ones. `is_from_me` decides which; a message with no thread is
/// neither.
pub fn split_batch(imessages: &[Value]) -> (Vec<String>, Vec<(String, String, DateTime<Utc>)>) {
    let mut inbound_threads: Vec<String> = Vec::new();
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
        if body.is_empty() {
            continue;
        }
        // A tapback ("Loved …") is a reaction, not a message: it neither asks
        // anything nor answers anything.
        let reaction = m
            .get("associated_message_type")
            .and_then(Value::as_i64)
            .is_some_and(|t| t > 0)
            || m
                .get("associated_message_guid")
                .and_then(Value::as_str)
                .is_some_and(|g| !g.is_empty());
        if reaction {
            continue;
        }
        let from_me = m.get("is_from_me").and_then(Value::as_bool).unwrap_or(false);
        if from_me {
            let when = m
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                .map(|t| t.with_timezone(&Utc))
                .unwrap_or_else(Utc::now);
            outbound.push((thread.to_string(), body.to_string(), when));
        } else if !inbound_threads.iter().any(|t| t == thread) {
            inbound_threads.push(thread.to_string());
        }
    }
    (inbound_threads, outbound)
}

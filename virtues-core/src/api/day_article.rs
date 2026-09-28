//! The day's article, written from the day itself.
//!
//! The narrator used to read the segmenter's event summaries: transcript →
//! five-minute summary → a capped session rollup → a one-to-three sentence
//! event summary. Four lossy hops, each told to be factual, so each removed
//! what made the day matter, and the article described the machine ("desk,
//! apps, messages") instead of the day.
//!
//! Significance is the model's judgment, not a rubric computed here. What this
//! module owns is the shape of what the writer reads and the checks on what it
//! writes:
//!
//! 1. **Assemble** the day: full transcripts (silence dropped) grouped into
//!    conversations, messages by thread, the owner's own words, their chats
//!    and page edits, the day's people as the wiki knows them, their narrative
//!    identity, the pages for the days before, and the hours with no record.
//! 2. **Write** on the Chat slot. Every sentence ends with the time of the
//!    evidence it rests on.
//! 3. **Check** each sentence on the Lite slot against exactly that evidence
//!    (plus the rest of its conversation). Unsupported sentences are deleted,
//!    never repaired: repair and strictness turned pages into inventories.
//! 4. **Names**: a person named on the page must be in the day's people or in
//!    the day's record, or the sentence goes.
//! 5. **Render** markdown: the Abstract first, sections after, evidence as
//!    footnotes the page draws in its margin (`[^ev-N]`), section time spans as
//!    context footnotes (`[^cx-N]`).
//!
//! The record of why, and what the spike measured, is
//! `agents/plan/day-article-plan.md`.

use std::collections::{BTreeSet, HashMap};

use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Tz;
use sqlx::{PgPool, Row};

use crate::error::Result;
use crate::virtues_api::request::Thinking;
use virtues_registry::models::ModelSlot;

/// A transcript chunk shorter than this is a cough or a word; it carries no day.
const MIN_CHUNK_CHARS: usize = 40;
/// Chunks further apart than this belong to different conversations.
const CONVERSATION_GAP_MIN: i64 = 10;
/// A silence in the recording longer than this is a gap worth telling the writer.
const GAP_MINUTES: i64 = 30;
/// A group thread where the owner sent fewer than this is shown as a count, not
/// as its chatter.
const LURK_THRESHOLD: usize = 3;
/// Message bodies are cut here; the writer needs what a message was about.
const MESSAGE_CHARS: usize = 280;
/// Three previous pages carry a thread forward; more is prompt weight.
const RECENT_PAGES: i64 = 3;

// ── What the writer reads ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub(crate) struct Chunk {
    pub id: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub hm: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub(crate) struct Msg {
    pub id: String,
    pub hm: String,
    pub who: String,
    pub body: String,
    pub from_me: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Person {
    pub id: String,
    pub name: String,
}

pub(crate) struct DayInput {
    pub user_prompt: String,
    pub chunks: Vec<Chunk>,
    pub messages: Vec<Msg>,
    pub people: Vec<Person>,
    pub gaps_text: String,
}

impl DayInput {
    /// Enough of the day to write about: a real conversation, or the owner
    /// writing to someone. Below this the page would be padding.
    pub fn has_substance(&self) -> bool {
        self.chunks.iter().any(|c| c.text.len() >= 400) || self.messages.iter().filter(|m| m.from_me).count() >= 3
    }
}

/// Drop emoji and decoration from a display name ("Nick 🌷" → "Nick").
pub(crate) fn clean_name(s: &str) -> String {
    let kept: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '\'' | '’' | '-' | '.' | ' ') { c } else { ' ' })
        .collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn hm(t: &DateTime<Utc>, tz: Option<&Tz>) -> String {
    match tz {
        Some(z) => t.with_timezone(z).format("%H:%M").to_string(),
        None => t.format("%H:%M").to_string(),
    }
}

fn cut(s: &str, n: usize) -> String {
    let s = s.replace('\n', " ");
    let s = s.trim();
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

/// Reactions ("Loved an image") are acknowledgements, not words.
fn is_reaction(body: &str) -> bool {
    const VERBS: [&str; 6] = ["Loved ", "Liked ", "Emphasized ", "Laughed at ", "Disliked ", "Questioned "];
    VERBS.iter().any(|v| body.starts_with(v))
}

/// "[Photo][Photo][Photo]" → "[3 photos]".
fn fold_photos(body: &str) -> String {
    let n = body.matches("[Photo]").count();
    if n < 2 {
        return body.to_string();
    }
    let rest = body.replace("[Photo]", "");
    format!("{}[{n} photos]", rest.trim_end())
}

/// Assemble everything the writer reads for one day.
pub(crate) async fn assemble(
    pool: &PgPool,
    date: NaiveDate,
    tz: Option<&Tz>,
    start: &str,
    end: &str,
) -> Result<DayInput> {
    // The owner, so they are not listed as someone in their own day.
    let self_id: Option<String> =
        sqlx::query_scalar("SELECT self_person_id FROM app_user_profile LIMIT 1")
            .fetch_optional(pool)
            .await?
            .flatten();

    // ── transcripts ──
    let rows = sqlx::query(
        "SELECT id, text, started_at, COALESCE(ended_at, started_at + interval '5 minutes') AS ended_at \
         FROM data_communication_transcription \
         WHERE started_at >= $1::timestamptz AND started_at < $2::timestamptz \
         ORDER BY started_at",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?;
    let mut recorded: Vec<(DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    let mut chunks = Vec::new();
    for r in &rows {
        let s: DateTime<Utc> = r.try_get("started_at")?;
        let e: DateTime<Utc> = r.try_get("ended_at")?;
        recorded.push((s, e));
        let text: String = r.try_get("text")?;
        let text = text.trim().to_string();
        if text.chars().count() < MIN_CHUNK_CHARS {
            continue; // silence: counted as coverage above, carries no words
        }
        chunks.push(Chunk { id: r.try_get("id")?, start: s, end: e, hm: hm(&s, tz), text });
    }

    // ── people ──
    let people_rows = sqlx::query(
        "SELECT p.id, p.name, p.metadata->'ios_relations' AS relations, \
                (SELECT left(pg.content, 900) FROM wiki_articles a JOIN app_pages pg ON pg.id = a.page_id \
                  WHERE a.subject_type = 'person' AND a.subject_id = p.id) AS article \
         FROM wiki_people p \
         WHERE p.id IN (SELECT DISTINCT entity_id FROM wiki_refs \
                        WHERE entity_type = 'person' AND occurred_at >= $1::timestamptz AND occurred_at < $2::timestamptz) \
           AND (p.handles <> '[]'::jsonb OR EXISTS (SELECT 1 FROM wiki_articles a \
                 WHERE a.subject_type = 'person' AND a.subject_id = p.id)) \
         ORDER BY p.name",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?;
    let mut people = Vec::new();
    let mut people_block = String::new();
    for r in &people_rows {
        let id: String = r.try_get("id")?;
        if Some(&id) == self_id.as_ref() {
            continue;
        }
        let name = clean_name(&r.try_get::<String, _>("name")?);
        if name.is_empty() {
            continue;
        }
        people_block.push_str(&format!("- {name} (href: /person/{id})"));
        if let Some(rel) = r.try_get::<Option<serde_json::Value>, _>("relations")? {
            people_block.push_str(&format!("; related names on their contact card: {rel}"));
        }
        if let Some(a) = r.try_get::<Option<String>, _>("article")? {
            if !a.trim().is_empty() {
                people_block.push_str(&format!("\n  From their wiki page: {}", cut(&a, 900)));
            }
        }
        people_block.push('\n');
        people.push(Person { id, name });
    }

    // ── messages, by thread ──
    let rows = sqlx::query(
        "SELECT DISTINCT ON (m.id) m.id, m.thread_id, m.body, m.occurred_at, m.from_name, m.from_identifier, \
                m.is_group_message, COALESCE((m.metadata->>'is_from_me')::boolean, false) AS from_me, \
                pe.name AS resolved_name \
         FROM data_communication_message m \
         LEFT JOIN wiki_refs er ON er.source_table = 'data_communication_message' AND er.source_id = m.id \
              AND er.entity_type = 'person' AND er.role = 'sender' \
         LEFT JOIN wiki_people pe ON pe.id = er.entity_id \
         WHERE m.occurred_at >= $1::timestamptz AND m.occurred_at < $2::timestamptz \
         ORDER BY m.id, (pe.name IS NULL)",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?;
    let participants: Vec<(String, String)> = sqlx::query_as(
        "SELECT DISTINCT m.thread_id, p.name FROM data_communication_message m \
         JOIN wiki_refs r ON r.source_table = 'data_communication_message' AND r.source_id = m.id AND r.entity_type = 'person' \
         JOIN wiki_people p ON p.id = r.entity_id \
         WHERE m.occurred_at >= $1::timestamptz AND m.occurred_at < $2::timestamptz AND m.thread_id IS NOT NULL \
           AND ($3::text IS NULL OR p.id <> $3)",
    )
    .bind(start)
    .bind(end)
    .bind(self_id.as_deref())
    .fetch_all(pool)
    .await?;
    let mut thread_names: HashMap<String, BTreeSet<String>> = HashMap::new();
    for (t, n) in participants {
        thread_names.entry(t).or_default().insert(clean_name(&n));
    }

    struct Raw {
        id: String,
        thread: String,
        at: DateTime<Utc>,
        who: String,
        body: String,
        from_me: bool,
        group: bool,
    }
    let mut raws = Vec::new();
    for r in &rows {
        let from_me: bool = r.try_get("from_me")?;
        let who = if from_me {
            "you".to_string()
        } else {
            let resolved: Option<String> = r.try_get("resolved_name")?;
            let from_name: Option<String> = r.try_get("from_name")?;
            let ident: String = r.try_get("from_identifier")?;
            clean_name(&resolved.or(from_name).unwrap_or(ident))
        };
        raws.push(Raw {
            id: r.try_get("id")?,
            thread: r.try_get::<Option<String>, _>("thread_id")?.unwrap_or_default(),
            at: r.try_get("occurred_at")?,
            who,
            body: r.try_get::<Option<String>, _>("body")?.unwrap_or_default(),
            from_me,
            group: r.try_get("is_group_message")?,
        });
    }
    raws.sort_by_key(|m| m.at);

    let mut by_thread: Vec<(String, Vec<&Raw>)> = Vec::new();
    for m in &raws {
        match by_thread.iter_mut().find(|(t, _)| *t == m.thread) {
            Some((_, v)) => v.push(m),
            None => by_thread.push((m.thread.clone(), vec![m])),
        }
    }
    let mut messages = Vec::new();
    let mut threads_block = String::new();
    let mut your_words = String::new();
    let mut automated = 0usize;
    for (thread, ms) in &by_thread {
        let names: Vec<String> = thread_names.get(thread).map(|s| s.iter().cloned().collect()).unwrap_or_default();
        if names.is_empty() && ms.iter().all(|m| !m.from_me) {
            automated += ms.len(); // codes, notices, a number nobody knows
            continue;
        }
        let group = ms.iter().any(|m| m.group) || names.len() > 1;
        let mut reactions = 0usize;
        let mut lines = Vec::new();
        for m in ms {
            if is_reaction(&m.body) {
                reactions += 1;
                continue;
            }
            let body = cut(&fold_photos(&m.body), MESSAGE_CHARS);
            let t = hm(&m.at, tz);
            if m.from_me {
                your_words.push_str(&format!("[msg {t}] to {}: {body}\n", if names.is_empty() { "unknown".into() } else { names.join(", ") }));
            }
            messages.push(Msg { id: m.id.clone(), hm: t.clone(), who: m.who.clone(), body: body.clone(), from_me: m.from_me });
            lines.push((t, m.who.clone(), body, m.from_me));
        }
        let sent = lines.iter().filter(|l| l.3).count();
        let with = if names.is_empty() { "unknown".to_string() } else { names.join(", ") };
        threads_block.push_str(&format!(
            "### {} with {with}: {} messages, you sent {sent}{}",
            if group { "Group" } else { "1:1" },
            ms.len(),
            if reactions > 0 { format!(", {reactions} reactions") } else { String::new() }
        ));
        if group && sent < LURK_THRESHOLD {
            // A group the owner barely spoke in: its size and their part, not its chatter.
            threads_block.push_str(" (you mostly read; only your messages shown)\n");
            for (t, w, b, me) in &lines {
                if *me {
                    threads_block.push_str(&format!("[msg {t}] {w}: {b}\n"));
                }
            }
        } else {
            threads_block.push('\n');
            for (t, w, b, _) in &lines {
                threads_block.push_str(&format!("[msg {t}] {w}: {b}\n"));
            }
        }
        threads_block.push('\n');
    }

    // ── the owner's chats with Virtues, and pages they edited ──
    let chat_rows = sqlx::query(
        "SELECT c.title, c.created_at, \
                (SELECT string_agg(left(m.content, 300), ' / ' ORDER BY m.sequence_num) FROM \
                   (SELECT content, sequence_num FROM app_chat_messages WHERE chat_id = c.id AND role = 'user' \
                    ORDER BY sequence_num LIMIT 3) m) AS asked \
         FROM app_chats c \
         WHERE c.created_at >= $1::timestamptz AND c.created_at < $2::timestamptz AND c.deleted_at IS NULL \
         ORDER BY c.created_at LIMIT 12",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?;
    let mut chats_block = String::new();
    for r in &chat_rows {
        let at: DateTime<Utc> = r.try_get("created_at")?;
        let title: Option<String> = r.try_get("title")?;
        let asked: Option<String> = r.try_get("asked")?;
        chats_block.push_str(&format!(
            "[chat {}] \"{}\": {}\n",
            hm(&at, tz),
            title.unwrap_or_else(|| "untitled".into()),
            cut(&asked.unwrap_or_default(), 600)
        ));
    }

    // ── the days before ──
    let recent: Vec<(NaiveDate, String)> = sqlx::query_as(
        "SELECT d.date, pg.content FROM wiki_days d \
         JOIN wiki_articles a ON a.subject_type = 'day' AND a.subject_id = d.id \
         JOIN app_pages pg ON pg.id = a.page_id \
         WHERE d.date < $1 AND d.date >= $1 - 7 AND pg.content <> '' \
         ORDER BY d.date DESC LIMIT $2",
    )
    .bind(date)
    .bind(RECENT_PAGES)
    .fetch_all(pool)
    .await?;
    let recent_block: String = recent
        .iter()
        .map(|(d, c)| format!("#### {}\n{}\n", d.format("%A, %B %-d"), strip_footnotes(c)))
        .collect();

    // ── what the record does not hold ──
    let gaps_text = gaps(&recorded, tz);

    // ── identity ──
    let identity = crate::api::wiki::get_narrative_identity(pool).await?.content;

    // ── the prompt ──
    let mut p = format!("# {}\n\n", date.format("%A, %B %-d, %Y"));
    if !identity.trim().is_empty() {
        p.push_str(&format!("<identity>\n{}\n</identity>\n\n", identity.trim()));
    }
    p.push_str(&format!("<people>\n{people_block}</people>\n\n"));
    if !recent_block.is_empty() {
        p.push_str(&format!("<recent_days>\nThe pages for the days just before, newest first.\n{recent_block}</recent_days>\n\n"));
    }
    p.push_str(&format!("<gaps>\n{gaps_text}</gaps>\n\n"));
    p.push_str(&format!("<your_words>\nEverything you wrote to anyone this day.\n{your_words}</your_words>\n\n"));
    if !chats_block.is_empty() {
        p.push_str(&format!("<your_chats>\nWhat you asked Virtues this day.\n{chats_block}</your_chats>\n\n"));
    }
    p.push_str("<transcripts>\n");
    p.push_str(&conversations_block(&chunks));
    p.push_str("</transcripts>\n\n");
    p.push_str(&format!("<messages>\n{threads_block}</messages>\n"));
    if automated > 0 {
        p.push_str(&format!("\n({automated} automated texts left out.)\n"));
    }

    Ok(DayInput { user_prompt: p, chunks, messages, people, gaps_text })
}

/// Earlier pages feed the writer as prose; their footnotes are page furniture.
fn strip_footnotes(md: &str) -> String {
    let body: Vec<&str> = md.lines().filter(|l| !l.trim_start().starts_with("[^")).collect();
    let joined = body.join("\n");
    let re = regex::Regex::new(r"\[\^[a-z]{2}-\d+\]").expect("static regex");
    re.replace_all(&joined, "").to_string()
}

fn conversations_block(chunks: &[Chunk]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < chunks.len() {
        let mut j = i;
        while j + 1 < chunks.len() && (chunks[j + 1].start - chunks[j].end).num_minutes() <= CONVERSATION_GAP_MIN {
            j += 1;
        }
        out.push_str(&format!("## Conversation {}–{}\n", chunks[i].hm, chunks[j].hm));
        for c in &chunks[i..=j] {
            out.push_str(&format!("### [{}]\n{}\n\n", c.hm, c.text));
        }
        i = j + 1;
    }
    out
}

fn gaps(recorded: &[(DateTime<Utc>, DateTime<Utc>)], tz: Option<&Tz>) -> String {
    if recorded.is_empty() {
        return "- Nothing was recorded this day.\n".into();
    }
    let mut out = format!("- Nothing was recorded before {}.\n", hm(&recorded[0].0, tz));
    for w in recorded.windows(2) {
        if (w[1].0 - w[0].1).num_minutes() > GAP_MINUTES {
            out.push_str(&format!("- Nothing was recorded between {} and {}.\n", hm(&w[0].1, tz), hm(&w[1].0, tz)));
        }
    }
    out
}

// ── The writer ─────────────────────────────────────────────────────────────

pub(crate) const WRITER_PROMPT: &str = r#"You write one day's page in a person's private journal: the page they will open weeks or years from now to get this day back.

You are given the day itself: the conversations the microphone caught, in full; their message threads; their own words that day and what they asked Virtues; the people in it, as their own wiki knows them; the pages for the days just before; and a list of hours with no recording. You are also given their own account of who they are.

WHAT TO DECIDE
Before writing, decide the one thing this day was. Not what filled the most hours: what they would want back. A long conversation where something real was said outweighs eight hours at a desk. Continuity matters: if the days before left something in motion (an appointment tomorrow, a trip, a conversation to finish), and today is that day, that is very likely the thread. Their own words are the strongest signal of what mattered to them.

<identity> is their own account of who they are. Read it for temperament and for what they care about. It shapes your judgment and never appears on the page: do not quote it, echo it, or explain the day through it.

THE PAGE
1. **Abstract**: one to three plain sentences. Who the day was with, what it was, and the thread that ran through it. Precise, not poetic.
2. Then the body: up to three sections under `## ` headings. A heading is three to six words naming a thing or a moment ("The waiting room at the clinic", "The walk home"), never a bare proper noun. This is the fuller account. Include the specific details that carry the day: the corner, the question asked, the thing on the shelf. Leave out details that do not serve the day's thread: app names, background TV, logistics, group-chat chatter.
3. Where the day holds a list worth keeping (questions asked, things bought, songs played), write it as a small markdown table of two columns, with a one-line lead-in sentence above it. Put the table's evidence tags alone on the line after it.
Walk the day in order. The body runs 300 to 600 words on a full day; a thin day gets an Abstract and a few lines.

EVERY SENTENCE CARRIES ITS SOURCE
End every sentence with the time of the evidence it rests on, in square brackets: `[17:59]` for a transcript chunk, `[msg 19:40]` for a message, `[chat 09:18]` for a question they asked Virtues. Several are fine: `[17:14, 17:19]`. A sentence you cannot tag is a sentence you must not write. Each sentence will be checked against exactly the evidence it cites, and unsupported sentences are deleted.

WHAT COUNTS AS EVIDENCE
- Only what is in the transcripts, messages and chats. A detail is written only if it is there; a descriptive word ("well-worn", "lopsided") only if the record uses it.
- Who someone is to them (partner, friend, roommate, mother) comes only from <people>. Never infer a relationship from context.
- Speaker labels in transcripts are unlabeled and unstable: "Speaker 1" can be a different person in the next chunk. Decide who said something only when a name, a reply, or the content makes it certain. When it could be either of them, write it as shared ("between you, it came to...").
- Transcription mishears names. A name that sounds like someone in <people> and fits is them; always call people by their <people> name, without emoji.
- Plans are not events. A plan for later is not evidence it happened.
- Keep the order the record shows. Never join two things as cause and effect unless the record shows the link.
- Hours listed in <gaps> have no recording. Never fill them with who was probably there or what probably happened. If a gap is longer than two hours, say once, plainly: "Nothing was recorded between 8:40 and 5." Tag that sentence `[gap]`.

HARD CONVERSATIONS
They belong on the page, with discretion. The owner was there; give them a door back into the moment, not its contents. Name what it turned on in a word or two ("an old injury", "family", "a job left behind") and how it moved: it got awkward, someone apologized, it was set down and the evening went on. No body parts, procedures, ages, or third parties named inside it. Feelings someone said aloud are evidence; feelings nobody expressed are not yours to assign. Never quote another person; paraphrase.

VOICE
Second person, past tense. Plain and warm, the voice of a friend who was paying attention. No verdicts on the day, no inner states nobody voiced.

LINKS
Link a person's first mention as [Name](href) using the href in <people>. Never invent a link.

OUTPUT
Exactly this, and nothing else:

Abstract: <one to three sentences, each tagged>

<sections>
"#;

/// Write the day. `None` when the day has no substance to write from.
pub(crate) async fn write_day(pool: &PgPool, date: NaiveDate, tz: Option<&Tz>, start: &str, end: &str) -> Result<Option<String>> {
    let input = assemble(pool, date, tz, start, end).await?;
    if !input.has_substance() {
        tracing::info!(date = %date, "not enough of the day to write from - no model call");
        return Ok(None);
    }
    let draft = crate::virtues_api::completion::system_completion(
        pool,
        ModelSlot::Chat,
        "day_article",
        WRITER_PROMPT,
        &input.user_prompt,
        Thinking::Low,
        0.7,
    )
    .await?;
    let items = split_items(&draft);
    let verdicts = check(pool, &items, &input).await?;
    let names = known_names(&input);
    let md = render(&items, &verdicts, &input, &names);
    tracing::info!(
        date = %date,
        sentences = items.iter().filter(|i| matches!(i, Item::Sentence { .. })).count(),
        kept = verdicts.values().filter(|v| **v).count(),
        "day article written"
    );
    Ok(Some(md))
}

// ── Parsing the draft ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Item {
    Abstract(String),
    Heading(String),
    /// One sentence and the evidence tags it ended with.
    Sentence { para: usize, text: String, tags: Vec<String> },
    /// A table block with the tags on the line after it.
    Table { text: String, tags: Vec<String> },
}

fn tag_re() -> regex::Regex {
    regex::Regex::new(r"\[((?:msg |chat )?\d{1,2}:\d{2}(?:\s*,\s*(?:msg |chat )?\d{1,2}:\d{2})*|gap)\]").expect("static regex")
}

fn parse_tags(inner: &str) -> Vec<String> {
    inner.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect()
}

/// Split the draft into the Abstract, headings, tagged sentences and tables.
pub(crate) fn split_items(draft: &str) -> Vec<Item> {
    let re = tag_re();
    let mut items = Vec::new();
    let blocks: Vec<&str> = draft.split("\n\n").map(str::trim).filter(|b| !b.is_empty()).collect();
    let mut para = 0usize;
    let mut k = 0;
    while k < blocks.len() {
        let b = blocks[k];
        if let Some(h) = b.strip_prefix("## ") {
            items.push(Item::Heading(h.trim().to_string()));
        } else if let Some(a) = b.strip_prefix("Abstract:") {
            items.push(Item::Abstract(a.trim().to_string()));
        } else if b.starts_with('|') {
            // The table's lines, then its tags on their own line (same block or the next).
            let (table, tail): (Vec<&str>, Vec<&str>) = b.lines().partition(|l| l.trim_start().starts_with('|'));
            let mut tags: Vec<String> = tail.iter().flat_map(|l| re.captures_iter(l).map(|c| parse_tags(&c[1])).collect::<Vec<_>>()).flatten().collect();
            if tags.is_empty() {
                if let Some(next) = blocks.get(k + 1) {
                    if re.is_match(next) && re.replace_all(next, "").trim().is_empty() {
                        tags = re.captures_iter(next).flat_map(|c| parse_tags(&c[1])).collect();
                        k += 1;
                    }
                }
            }
            items.push(Item::Table { text: table.join("\n"), tags });
        } else {
            para += 1;
            let mut pos = 0;
            for m in re.find_iter(b) {
                // The sentence runs to the tag, plus any closing punctuation after it.
                let mut end = m.end();
                while let Some(ch) = b[end..].chars().next() {
                    if matches!(ch, '.' | '!' | '?' | '"' | '”' | '’' | ')') {
                        end += ch.len_utf8();
                    } else {
                        break;
                    }
                }
                let text = b[pos..end].trim().to_string();
                let tags = re.captures(&b[m.start()..m.end()]).map(|c| parse_tags(&c[1])).unwrap_or_default();
                if !text.is_empty() {
                    items.push(Item::Sentence { para, text, tags });
                }
                pos = end;
            }
            let rest = b[pos..].trim();
            if !rest.is_empty() {
                items.push(Item::Sentence { para, text: rest.to_string(), tags: vec![] });
            }
        }
        k += 1;
    }
    items
}

/// The text of a sentence without its tags.
fn untagged(text: &str) -> String {
    let t = tag_re().replace_all(text, "");
    regex::Regex::new(r"\s+([.,;:!?])").expect("static regex").replace_all(t.trim(), "$1").to_string()
}

// ── The check ──────────────────────────────────────────────────────────────

const CHECK_PROMPT: &str = r#"You check one diary page, sentence by sentence, against the evidence each sentence cites. For each numbered item, answer whether its evidence supports it.

- "supported": everything the sentence claims is in its cited evidence or in the rest of that same conversation (paraphrase is fine; the owner is "you"; a name said anywhere in the conversation identifies who was there; people are listed below).
- "unsupported": the sentence claims something its evidence does not contain (an invented detail, a descriptive word the evidence lacks, a relationship, a cause, a place, a person being present).
- "wrong_person": the evidence shows the thing, but assigned to the other person, or speaker labels make who-said-it uncertain while the sentence states it as certain.
- "untagged": the item cites no evidence.
Be strict about facts (invented details, relationships, causes, presence during a gap, who said what) and relaxed about wording ("your alarm was set for 6:30" supports "you set an alarm for 6:30").

Return ONLY a JSON array: [{"i": 1, "verdict": "..."}]

ITEMS:
"#;

fn evidence_for(tags: &[String], input: &DayInput) -> String {
    let mut ev = Vec::new();
    for tag in tags {
        if tag == "gap" {
            ev.push(format!("GAPS:\n{}", input.gaps_text));
        } else if let Some(t) = tag.strip_prefix("msg ") {
            let hits: Vec<String> = input.messages.iter().filter(|m| m.hm == t).map(|m| format!("{} {}: {}", m.hm, m.who, m.body)).collect();
            ev.push(format!("MESSAGES AT {t}:\n{}", if hits.is_empty() { "(none)".into() } else { hits.join("\n") }));
        } else if tag.starts_with("chat ") {
            // Chats are in the prompt only as titles and the owner's questions;
            // a sentence about them is checked against that same text.
            ev.push(format!("CHAT {tag}: (the owner's own question to Virtues, as given to the writer)"));
        } else if let Some(i) = input.chunks.iter().position(|c| c.hm == *tag) {
            ev.push(format!("CITED TRANSCRIPT {tag}:\n{}", input.chunks[i].text));
            let lo = i.saturating_sub(2);
            let hi = (i + 2).min(input.chunks.len() - 1);
            let ctx: Vec<String> = (lo..=hi)
                .filter(|&j| j != i && ((input.chunks[j].start - input.chunks[i].end).num_minutes().abs() <= CONVERSATION_GAP_MIN * 3))
                .map(|j| format!("[{}] {}", input.chunks[j].hm, cut(&input.chunks[j].text, 1500)))
                .collect();
            if !ctx.is_empty() {
                ev.push(format!("THE REST OF THAT CONVERSATION (context only):\n{}", ctx.join("\n")));
            }
        } else {
            ev.push(format!("TRANSCRIPT {tag}: (no chunk at this time)"));
        }
    }
    ev.join("\n\n")
}

/// Verdicts keyed by item index: `true` keeps the item.
async fn check(pool: &PgPool, items: &[Item], input: &DayInput) -> Result<HashMap<usize, bool>> {
    let mut numbered = String::new();
    let mut index = Vec::new();
    for (k, item) in items.iter().enumerate() {
        let (text, tags) = match item {
            Item::Sentence { text, tags, .. } => (text, tags),
            _ => continue,
        };
        index.push(k);
        numbered.push_str(&format!("#{}\nSENTENCE: {}\nEVIDENCE:\n{}\n\n", index.len(), text, if tags.is_empty() { "(none)".into() } else { evidence_for(tags, input) }));
    }
    let mut verdicts: HashMap<usize, bool> = HashMap::new();
    if index.is_empty() {
        return Ok(verdicts);
    }
    let people: String = input.people.iter().map(|p| format!("- {}\n", p.name)).collect();
    let raw = crate::virtues_api::completion::system_completion(
        pool,
        ModelSlot::Lite,
        "day_article_check",
        "",
        &format!("{CHECK_PROMPT}{numbered}\nPEOPLE:\n{people}"),
        Thinking::Low,
        0.0,
    )
    .await?;
    let parsed: Vec<serde_json::Value> = raw
        .find('[')
        .and_then(|s| raw.rfind(']').map(|e| &raw[s..=e]))
        .and_then(|j| serde_json::from_str(j).ok())
        .unwrap_or_default();
    if parsed.is_empty() {
        // A check that answered nothing checked nothing: keep the draft rather
        // than delete every sentence over a malformed reply, and say so.
        tracing::warn!("day article check returned no verdicts - keeping the draft unchecked");
        for k in index {
            verdicts.insert(k, true);
        }
        return Ok(verdicts);
    }
    for v in parsed {
        let n = v.get("i").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
        let ok = v.get("verdict").and_then(|s| s.as_str()) == Some("supported");
        if n >= 1 && n <= index.len() {
            verdicts.insert(index[n - 1], ok);
        }
    }
    Ok(verdicts)
}

// ── Names ──────────────────────────────────────────────────────────────────

/// Every capitalized word a person's name on the page may use: the day's
/// people, and every capitalized word in the day's own record.
fn known_names(input: &DayInput) -> BTreeSet<String> {
    let word = regex::Regex::new(r"[A-Z][\w'’\-]+").expect("static regex");
    let mut known = BTreeSet::new();
    for p in &input.people {
        known.extend(word.find_iter(&p.name).map(|m| m.as_str().to_string()));
    }
    for c in &input.chunks {
        known.extend(word.find_iter(&c.text).map(|m| m.as_str().to_string()));
    }
    for m in &input.messages {
        known.extend(word.find_iter(&m.body).map(|m| m.as_str().to_string()));
        known.extend(word.find_iter(&m.who).map(|m| m.as_str().to_string()));
    }
    known
}

/// A person's name the record never contains: the writer made it up.
///
/// Only linked names and capitalized pairs ("Firstname Lastname") are
/// examined — a single capitalized word is usually a place, a product or the
/// start of a clause, and the model checker already judges those.
fn invented_name(sentence: &str, input: &DayInput, known: &BTreeSet<String>) -> Option<String> {
    let link = regex::Regex::new(r"\[([^\]]+)\]\(/person/([^)]+)\)").expect("static regex");
    for c in link.captures_iter(sentence) {
        if !input.people.iter().any(|p| p.id == c[2]) {
            return Some(c[1].to_string());
        }
    }
    let pair = regex::Regex::new(r"\b([A-Z][a-z]+) ([A-Z][a-z]+)\b").expect("static regex");
    let plain = link.replace_all(sentence, "$1");
    for c in pair.captures_iter(&plain) {
        if !known.contains(&c[1].to_string()) || !known.contains(&c[2].to_string()) {
            return Some(format!("{} {}", &c[1], &c[2]));
        }
    }
    None
}

// ── Markdown ───────────────────────────────────────────────────────────────

fn evidence_label(tag: &str, input: &DayInput) -> Option<(String, String)> {
    if let Some(t) = tag.strip_prefix("msg ") {
        let m = input.messages.iter().find(|m| m.hm == t)?;
        Some((format!("Message · {}", twelve_hour(t)), format!("data_communication_message:{}", m.id)))
    } else if let Some(t) = tag.strip_prefix("chat ") {
        Some((format!("Chat · {}", twelve_hour(t)), String::new()))
    } else if tag == "gap" {
        None
    } else {
        let c = input.chunks.iter().find(|c| c.hm == tag)?;
        Some((format!("Recording · {}", twelve_hour(tag)), format!("data_communication_transcription:{}", c.id)))
    }
}

fn twelve_hour(hm: &str) -> String {
    let mut parts = hm.split(':');
    let h: u32 = parts.next().and_then(|h| h.parse().ok()).unwrap_or(0);
    let m = parts.next().unwrap_or("00");
    let (h12, ap) = match h {
        0 => (12, "AM"),
        1..=11 => (h, "AM"),
        12 => (12, "PM"),
        _ => (h - 12, "PM"),
    };
    format!("{h12}:{m} {ap}")
}

/// The finished page: Abstract, sections, tables, and the footnotes the page
/// draws in its margin. Evidence footnotes are `ev-N`; the section time spans
/// are `cx-N`.
pub(crate) fn render(items: &[Item], keep: &HashMap<usize, bool>, input: &DayInput, known: &BTreeSet<String>) -> String {
    let mut out = String::new();
    let mut notes: Vec<String> = Vec::new();
    let mut ev_n = 0usize;
    let mut cx_n = 0usize;
    let mut para: Option<usize> = None;
    let mut current = String::new();

    // Section time spans from the kept sentences' transcript tags.
    let mut heading_spans: HashMap<usize, (String, String)> = HashMap::new();
    let mut open: Option<usize> = None;
    for (k, item) in items.iter().enumerate() {
        match item {
            Item::Heading(_) => open = Some(k),
            Item::Sentence { tags, .. } | Item::Table { tags, .. } if keep.get(&k).copied().unwrap_or(matches!(item, Item::Table { .. })) => {
                if let Some(h) = open {
                    for t in tags.iter().filter(|t| t.len() == 5 && t.as_bytes()[2] == b':') {
                        let e = heading_spans.entry(h).or_insert((t.clone(), t.clone()));
                        if *t < e.0 { e.0 = t.clone(); }
                        if *t > e.1 { e.1 = t.clone(); }
                    }
                }
            }
            _ => {}
        }
    }

    let flush = |out: &mut String, current: &mut String| {
        if !current.trim().is_empty() {
            out.push_str(current.trim());
            out.push_str("\n\n");
        }
        current.clear();
    };

    for (k, item) in items.iter().enumerate() {
        match item {
            Item::Abstract(a) => {
                flush(&mut out, &mut current);
                out.push_str(&untagged(a));
                out.push_str("\n\n");
            }
            Item::Heading(h) => {
                flush(&mut out, &mut current);
                para = None;
                match heading_spans.get(&k) {
                    Some((a, b)) => {
                        cx_n += 1;
                        let span = if a == b { twelve_hour(a) } else { format!("{}–{}", twelve_hour(a), twelve_hour(b)) };
                        out.push_str(&format!("## {h}[^cx-{cx_n}]\n\n"));
                        notes.push(format!("[^cx-{cx_n}]: {span}"));
                    }
                    None => out.push_str(&format!("## {h}\n\n")),
                }
            }
            Item::Table { text, tags } => {
                flush(&mut out, &mut current);
                para = None;
                out.push_str(text);
                out.push('\n');
                if let Some((label, r)) = tags.iter().find_map(|t| evidence_label(t, input)) {
                    ev_n += 1;
                    out.push_str(&format!("\n[^ev-{ev_n}]\n"));
                    notes.push(format!("[^ev-{ev_n}]: {label} · {r}"));
                }
                out.push('\n');
            }
            Item::Sentence { para: p, text, tags } => {
                if !keep.get(&k).copied().unwrap_or(false) || invented_name(text, input, known).is_some() {
                    continue;
                }
                if para != Some(*p) {
                    flush(&mut out, &mut current);
                    para = Some(*p);
                }
                let mut s = untagged(text);
                if let Some((label, r)) = tags.iter().find_map(|t| evidence_label(t, input)) {
                    ev_n += 1;
                    s.push_str(&format!("[^ev-{ev_n}]"));
                    notes.push(format!("[^ev-{ev_n}]: {label}{}", if r.is_empty() { String::new() } else { format!(" · {r}") }));
                }
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(&s);
            }
        }
    }
    flush(&mut out, &mut current);
    // A heading whose every sentence was deleted is a heading over nothing.
    let lines: Vec<&str> = out.lines().collect();
    let mut cleaned = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if l.starts_with("## ") && lines[i + 1..].iter().find(|x| !x.trim().is_empty()).map_or(true, |n| n.starts_with("## ")) {
            continue;
        }
        cleaned.push(*l);
    }
    let mut md = cleaned.join("\n").trim().to_string();
    if !notes.is_empty() {
        md.push_str("\n\n");
        md.push_str(&notes.join("\n"));
    }
    md.push('\n');
    md
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn input() -> DayInput {
        let t = |h, m| Utc.with_ymd_and_hms(2026, 1, 7, h, m, 0).unwrap();
        DayInput {
            user_prompt: String::new(),
            chunks: vec![
                Chunk { id: "tr_a".into(), start: t(17, 14), end: t(17, 19), hm: "17:14".into(), text: "[Speaker 1]: cross Fifth and Main, Nick".into() },
                Chunk { id: "tr_b".into(), start: t(17, 39), end: t(17, 44), hm: "17:39".into(), text: "[Speaker 2]: the most romantic thing?".into() },
            ],
            messages: vec![Msg { id: "msg_1".into(), hm: "19:40".into(), who: "you".into(), body: "home, walked the dog".into(), from_me: true }],
            people: vec![Person { id: "person_n".into(), name: "Nick".into() }],
            gaps_text: "- Nothing was recorded between 08:41 and 16:59.\n".into(),
        }
    }

    #[test]
    fn splits_the_draft_into_its_parts() {
        let draft = "Abstract: The day was [Nick](/person/person_n) and a long walk. [17:14]\n\n## The walk home\n\nYou crossed Fifth and Main [17:14]. You got home. [msg 19:40]\n\n| Question | Answer |\n|---|---|\n| Romantic? | Rain |\n\n[17:39]";
        let items = split_items(draft);
        assert!(matches!(&items[0], Item::Abstract(a) if a.contains("long walk")));
        assert_eq!(items[1], Item::Heading("The walk home".into()));
        assert!(matches!(&items[2], Item::Sentence { tags, .. } if tags == &vec!["17:14".to_string()]));
        assert!(matches!(&items[3], Item::Sentence { tags, .. } if tags == &vec!["msg 19:40".to_string()]));
        assert!(matches!(&items[4], Item::Table { tags, .. } if tags == &vec!["17:39".to_string()]));
    }

    #[test]
    fn renders_evidence_and_section_spans_as_footnotes() {
        let items = split_items("Abstract: A walk. [17:14]\n\n## The walk home\n\nYou crossed Fifth and Main. [17:14] You got home. [msg 19:40]");
        let keep: HashMap<usize, bool> = (0..items.len()).map(|k| (k, true)).collect();
        let inp = input();
        let md = render(&items, &keep, &inp, &known_names(&inp));
        assert!(md.starts_with("A walk.\n\n## The walk home[^cx-1]"), "{md}");
        assert!(md.contains("You crossed Fifth and Main.[^ev-1] You got home.[^ev-2]"), "{md}");
        assert!(md.contains("[^ev-1]: Recording · 5:14 PM · data_communication_transcription:tr_a"), "{md}");
        assert!(md.contains("[^ev-2]: Message · 7:40 PM · data_communication_message:msg_1"), "{md}");
        assert!(md.contains("[^cx-1]: 5:14 PM"), "{md}");
    }

    #[test]
    fn deleted_sentences_and_empty_sections_leave_no_trace() {
        let items = split_items("Abstract: A day. [17:14]\n\n## The shop\n\nYou bought a hat. [17:39]\n\n## The walk\n\nYou crossed Fifth and Main. [17:14]");
        let mut keep: HashMap<usize, bool> = (0..items.len()).map(|k| (k, true)).collect();
        keep.insert(2, false); // "You bought a hat." failed its check
        let inp = input();
        let md = render(&items, &keep, &inp, &known_names(&inp));
        assert!(!md.contains("The shop"), "{md}");
        assert!(!md.contains("hat"), "{md}");
        assert!(md.contains("## The walk"), "{md}");
    }

    #[test]
    fn an_invented_full_name_removes_its_sentence() {
        let inp = input();
        let known = known_names(&inp);
        assert_eq!(invented_name("You met Nick at Fifth.", &inp, &known), None);
        assert_eq!(invented_name("You met Margaret Thornbury.", &inp, &known).as_deref(), Some("Margaret Thornbury"));
        assert!(invented_name("You texted [Dave](/person/person_x).", &inp, &known).is_some());
    }

    #[test]
    fn gaps_name_the_silences() {
        let t = |h, m| Utc.with_ymd_and_hms(2026, 1, 7, h, m, 0).unwrap();
        let g = gaps(&[(t(7, 46), t(8, 41)), (t(16, 59), t(17, 4))], None);
        assert!(g.contains("between 08:41 and 16:59"));
        assert!(g.starts_with("- Nothing was recorded before 07:46."));
    }

    #[test]
    fn folds_photos_and_skips_reactions() {
        assert_eq!(fold_photos("[Photo][Photo][Photo]"), "[3 photos]");
        assert!(is_reaction("Loved an image"));
        assert_eq!(clean_name("Nick 🌷"), "Nick");
    }
}

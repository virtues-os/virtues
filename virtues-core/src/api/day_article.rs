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
//!    identity, the pages for the days before, what earlier pages say about
//!    this day's people (`day_memory`), and the day's timeline.
//! 2. **Scout** on the Lite slot: read all of it and choose the day's shape,
//!    its lede, and the moments worth keeping, each named by where it is in
//!    the record. Code attaches those chunks and messages word for word.
//! 3. **Write** on the Standard slot from that brief. Every sentence ends with
//!    the time of the evidence it rests on.
//! 4. **Check** each sentence on the Lite slot against exactly that evidence
//!    (plus the rest of its conversation). Unsupported sentences are deleted,
//!    never repaired: repair and strictness turned pages into inventories.
//! 5. **Names**: a person named on the page must be in the day's people or in
//!    the day's record, or the sentence goes.
//! 6. **Figures**: the writer may end with one quote or exchange from a light
//!    moment; code keeps it only if the record holds it word for word, or as
//!    messages in a row from one thread. Code adds the day's route when the
//!    day travelled. Each is a fenced `figure` block at the end of the section
//!    that holds its time.
//! 7. **Render** markdown: the Abstract first, sections after, evidence as
//!    footnotes that close the sentence they support (`[^ev-N]`, opened in a
//!    card when the sentence is clicked), section time spans as context
//!    footnotes the page draws in its margin (`[^cx-N]`).
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
/// A silence in the recording longer than this is worth telling the scout, so
/// the day's shape says where the record goes quiet.
const GAP_MINUTES: i64 = 30;
/// A group thread where the owner sent fewer than this is shown as a count, not
/// as its chatter.
const LURK_THRESHOLD: usize = 3;
/// Message bodies are cut here; the writer needs what a message was about.
const MESSAGE_CHARS: usize = 280;
/// A timeline line's summary is cut here: it says where and when, the
/// transcripts say what.
const TIMELINE_SUMMARY_CHARS: usize = 220;
/// The moments the writer gets, and the passages each carries. Beyond these
/// the brief is the whole day again, which is what the scout is for.
const MAX_MOMENTS: usize = 10;
const MAX_PASSAGES: usize = 6;
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
    /// Its thread, so an exchange set as a figure is proven to be one conversation.
    pub thread: String,
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
    /// What the writer reads besides the brief: the date, who "you" is, their
    /// identity, the day's people, the pages just before, and what earlier
    /// pages say.
    pub writer_context: String,
    /// The owner's names (full name first), so the check knows who "you" is.
    pub owner: Vec<String>,
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
        let text = crate::transcript::flatten_speaker_tags(text.trim()).into_owned();
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
            messages.push(Msg {
                id: m.id.clone(),
                thread: thread.clone(),
                hm: t.clone(),
                who: m.who.clone(),
                body: body.clone(),
                from_me: m.from_me,
            });
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

    // ── what the record already says about these people, and the weeks before ──
    let ids: Vec<String> = people.iter().map(|p| p.id.clone()).collect();
    let in_full: Vec<NaiveDate> = recent.iter().map(|(d, _)| *d).collect();
    let earlier = crate::api::day_memory::for_writer(pool, &ids, date, start, end, &in_full).await?;

    // ── what the record does not hold ──
    let gaps_text = gaps(&recorded, tz);

    // ── identity ──
    let identity = crate::api::wiki::get_narrative_identity(pool).await?.content;

    // ── who "you" is ──
    let owner = match &self_id {
        Some(id) => owner_names(pool, id).await?,
        None => Vec::new(),
    };

    // ── the timeline: where the day went, as segmentation cut it ──
    let events: Vec<(DateTime<Utc>, DateTime<Utc>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT e.started_at, e.ended_at, COALESCE(e.user_label, e.auto_label), e.event_summary \
         FROM wiki_events e JOIN wiki_days d ON d.id = e.day_id \
         WHERE d.date = $1 ORDER BY e.started_at, e.id",
    )
    .bind(date)
    .fetch_all(pool)
    .await?;
    let timeline: String = events
        .iter()
        .map(|(s, e, label, summary)| {
            format!(
                "[{}–{}] {}{}\n",
                hm(s, tz),
                hm(e, tz),
                label.as_deref().unwrap_or("Unknown"),
                summary.as_deref().map(|x| format!(": {}", cut(x, TIMELINE_SUMMARY_CHARS))).unwrap_or_default()
            )
        })
        .collect();

    // ── what the writer reads besides the brief ──
    let mut w = format!("# {}\n\n", date.format("%A, %B %-d, %Y"));
    w.push_str(&you_block(&owner));
    if !identity.trim().is_empty() {
        w.push_str(&format!("<identity>\n{}\n</identity>\n\n", identity.trim()));
    }
    w.push_str(&format!("<people>\n{people_block}</people>\n\n"));
    if !recent_block.is_empty() {
        w.push_str(&format!("<recent_days>\nThe pages for the days just before, newest first.\n{recent_block}</recent_days>\n\n"));
    }
    if !earlier.is_empty() {
        w.push_str(&format!("<earlier>\nWhat their earlier days' pages already say, by each page's opening paragraph: the people this day was most with and the days that link them, then the rest of the last two weeks. Read this day against it.\n{earlier}</earlier>\n\n"));
    }

    // ── what the scout reads: all of that, and the whole day ──
    let mut p = w.clone();
    if !timeline.is_empty() {
        p.push_str(&format!("<timeline>\nMachine-written; use it for where and when, not for what mattered.\n{timeline}</timeline>\n\n"));
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

    Ok(DayInput { user_prompt: p, writer_context: w, chunks, messages, people, owner })
}

/// The owner's names: their person record's name, its first word, their
/// nickname and aliases, in that order, without repeats.
pub(crate) async fn owner_names(pool: &PgPool, self_id: &str) -> Result<Vec<String>> {
    let row: Option<(String, Option<String>, serde_json::Value)> =
        sqlx::query_as("SELECT name, nickname, aliases FROM wiki_people WHERE id = $1")
            .bind(self_id)
            .fetch_optional(pool)
            .await?;
    let Some((name, nickname, aliases)) = row else { return Ok(Vec::new()) };
    let full = clean_name(&name);
    let mut names = vec![full.clone()];
    names.extend(full.split_whitespace().next().map(str::to_string));
    names.extend(nickname.map(|n| clean_name(&n)));
    if let Some(list) = aliases.as_array() {
        names.extend(list.iter().filter_map(|a| a.as_str()).map(clean_name));
    }
    let mut seen = std::collections::HashSet::new();
    names.retain(|n| !n.is_empty() && seen.insert(n.to_lowercase()));
    Ok(names)
}

/// Who "you" is, for the scout and the writer. The page is in the second
/// person, and the recordings never say which voice is the owner's, so the
/// one fact that settles it is given outright: the owner's name. Someone who
/// says it is someone else, speaking to or about them.
pub(crate) fn you_block(names: &[String]) -> String {
    let Some(full) = names.first() else { return String::new() };
    let others: Vec<&str> = names[1..].iter().map(String::as_str).collect();
    let called = if others.is_empty() { String::new() } else { format!(" (also {})", others.join(", ")) };
    let first = names.get(1).unwrap_or(full);
    format!(
        "<you>\nThis is {full}'s day{called}, and the page calls them \"you\". They are one of the voices in the recordings and \"me\" in their messages. A speaker who says \"{first}\" is someone else, speaking to or about them: what that person says to or about {first} is said to or about you.\n</you>\n\n"
    )
}

/// Earlier pages feed the writer as prose; their footnotes and figures are
/// page furniture.
fn strip_footnotes(md: &str) -> String {
    let mut in_figure = false;
    let body: Vec<&str> = md
        .lines()
        .filter(|l| {
            let t = l.trim();
            if in_figure {
                in_figure = t != "```";
                return false;
            }
            if t == FIGURE_FENCE {
                in_figure = true;
                return false;
            }
            !t.starts_with("[^")
        })
        .collect();
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

/// Lite's job: read the whole day against what the earlier pages say, and
/// choose. It names the moments by where they are in the record; code attaches
/// those chunks and messages word for word, so nothing the writer reads was
/// paraphrased on the way.
pub(crate) const SCOUT_PROMPT: &str = r#"You know this person's life better than anyone who reads their journal. Tonight you prepare one day of it for the writer of their journal page. The writer reads only what you hand over, so your choices decide the page.

You have the whole day: the day's timeline (where they were and roughly what was happening, written by a machine and often wrong about what mattered), every recorded conversation, their message threads, what they asked Virtues, the people in it, the pages for the days just before, and what their earlier pages say about the people in this day and the weeks before it.

Read all of it, then hand over the brief as JSON, and nothing else:

{
  "shape": [
    {"from": "16:45", "to": "17:24", "where": "the barber", "with": "a stylist", "what": "a haircut"}
  ],
  "lede": "the one thing that happened today they would tell first, as an event in a plain sentence; one thing, never a list",
  "moments": [
    {"title": "a few words", "why": "why they would want this back", "at": ["20:29", "20:34", "msg 20:21"]}
  ]
}

SHAPE is the day in the world: where they were, with whom, for how long, in order. "With" is only someone the recordings show taking part; a name heard or read aloud is not someone there. Build it from the timeline and the record together. Plain place words ("the office", "home", "a bar downtown"), never coordinates or app names. Merge desk hours into one line.

THE LEDE is what they would tell first if a friend asked about their day. Read the day against what was already going on: a death that had been coming, a relationship days old, a trip that was planned. The biggest turn in something already in motion usually leads.

MOMENTS are the five to ten things they would want back a year from now: what happened between people, what was said that was real, what was new or funny or hard, what they did with their body and their time. Mostly things that happened in person, in the recordings. A text message is a moment only when the text itself is the event (news arriving, something said that could only be said in writing); ordinary back-and-forth texting is not a moment. Order moments by the time they happened.

AT lists where each moment is in the record: the times of the transcript chunks (as they appear in the transcripts, "20:29") and messages ("msg 20:21") it lives in. The writer receives those chunks and messages in full, word for word, and nothing else from the day, so list every chunk the moment runs through, usually two to six. A moment with no times cannot be written."#;

/// Chat's job: tell the day from the brief.
pub(crate) const WRITER_PROMPT: &str = r#"You write one day's page in a person's private journal: the page they will open a year from now to get this day back.

Someone who knows their life has read the whole day and brought you a brief: where the day went (the shape), the moments worth keeping as exact passages from the recordings and messages, and a guess at the lede, which you may overrule. You also have the people in it, as their own wiki knows them, the pages for the days just before, what their earlier pages say about the people in this day, and their own account of who they are.

THE PAGE
Begin with the Abstract: the day's one event, said the way they would tell a friend who asked, and what made it that day rather than any day like it: who it was with, why they were there, or how it went, as the passages show it. One or two sentences about one thing. If two things happened, the one they would tell first leads and the other waits for the body.

Then the body: two to four sections, each one moment, in the order the moments happened. A section's `## ` heading is three to six words naming its place or its moment ("Upstairs at the Driskill"). Everything in a section belongs to that moment; nothing from earlier or later in the day is folded in.

Write each moment as a scene: where they were, who was there, what was said and done, and how it went. Give it room: a full day runs 400 to 700 words, and a moment worth keeping is worth a full paragraph or two. A text message belongs only where it carries something the moment turned on, told as part of the scene, not as a transcript of the exchange.

Use what was already going on only where it changes what a moment means (a death that had been coming for days, a relationship days old), in a clause inside the scene. Most of it never appears; it is there so you understand the day.

When a conversation ran through a list (questions and answers, options weighed, an order placed), write that part as a small two-column markdown table with a one-line lead-in, its evidence tags alone on the line after it. At most one table.

A quiet day gets the Abstract and a few lines. Start a new paragraph when the scene changes, and keep paragraphs to five sentences or fewer.

SOURCES
End every sentence with the time of the passage it rests on in square brackets: `[17:59]` for a recording, `[msg 19:40]` for a message, `[chat 09:18]` for a question they asked Virtues. Several are fine: `[17:14, 17:19]`. Every sentence is checked against what it cites and deleted if unsupported, so write only what the passages say.

- A detail goes on the page only if a passage holds it.
- Who someone is to them comes only from <people>.
- `[Speaker]:` marks a change of voice and nothing else. Say who spoke only when the passage names them or they answer someone by name; otherwise write it as shared between them ("between you, it came to…"). What was said is never a reason to guess who said it.
- Someone was there only if a passage shows them taking part: they speak, or are spoken to. A name heard in a recording (read from a screen, mentioned in passing, someone talked about) is not that person in the room, and <people> lists everyone in the day's messages, not who was present.
- Always use a person's <people> name.
- A plan is not an event. Never join two things as cause and effect unless the passages do.

HARD CONVERSATIONS
They belong on the page, with discretion: a door back into the moment, not its contents. Name what it turned on in a word or two and how it moved. No body parts, procedures, ages, or third parties named inside it. Feelings someone said aloud are evidence; others are not yours to assign. Paraphrase other people; never quote them, and never take the figure from one.

Other people's private lives stay off the page unless they were the moment: someone's family, health, money, marriage or past, and drugs or drinking beyond a plain mention of a night out. The page is about this person's day, and it will be read with others nearby.

THE VEIL
Wrap every name of a person or a place, and every phrase that names a hard or intimate matter, in ⟦ ⟧: "you drove ⟦Nick⟧ to ⟦the clinic⟧". Inside a link, wrap the link text: [⟦Nick⟧](href).

VOICE
Second person, past tense. Plain and warm, a friend who was there and paying attention. No verdicts on the day, no inner states nobody voiced. No dashes as punctuation. Link a person's first mention as [Name](href) from <people>; never invent a link.

<identity> is their own account of who they are. It shapes your judgment and never appears on the page.

THE FIGURE
The page may carry one figure, set apart from the prose: the day in its own words. Take it from a light moment (something funny, warm or plainly theirs), never from a hard conversation. Their own words are best. Leave it out when nothing light stands out. It is one line at the very end, in one of two forms:
- A quote, under 25 words, copied exactly as one passage has them, cited by that passage's time: `Quote: "<the words, exactly>" [17:59]`. From a recording you may leave out fillers ("um", "uh", "like") and false starts; every word you keep is one they said, in their order.
- An exchange, two to four messages in a row from one thread, cited by their times: `Exchange: [msg 21:44] [msg 21:45]`
A quote that differs from its passage by a word is dropped, and so is an exchange from more than one thread.

OUTPUT
Exactly this, and nothing else:

Abstract: <one or two sentences, each tagged>

<sections>

<the figure line, if any>
"#;

// ── The brief ─────────────────────────────────────────────────────────────

/// What the scout hands the writer. Anything it gets wrong in shape is
/// dropped, not guessed at: a moment with no times the record holds has
/// nothing for the writer to stand on.
#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct Brief {
    #[serde(default)]
    pub shape: Vec<Stretch>,
    #[serde(default)]
    pub lede: String,
    #[serde(default)]
    pub moments: Vec<Moment>,
}

#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct Stretch {
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
    #[serde(default, rename = "where")]
    pub place: String,
    #[serde(default)]
    pub with: String,
    #[serde(default)]
    pub what: String,
}

#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct Moment {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub why: String,
    #[serde(default)]
    pub at: Vec<String>,
}

/// The scout's JSON, from the first `{` to the last `}`: models wrap it in
/// fences and prose often enough that a strict parse fails good briefs.
pub(crate) fn parse_brief(raw: &str) -> Option<Brief> {
    let s = raw.find('{')?;
    let e = raw.rfind('}')?;
    serde_json::from_str(raw.get(s..=e)?).ok()
}

/// The writer's prompt: its context, the shape and lede, and each moment with
/// the record it lives in, word for word. Times the record does not hold are
/// dropped, and so is a moment left with nothing.
pub(crate) fn writer_prompt(brief: &Brief, input: &DayInput) -> String {
    let mut p = input.writer_context.clone();
    let shape: String = brief
        .shape
        .iter()
        .map(|s| {
            let with = if s.with.trim().is_empty() { String::new() } else { format!(" · with {}", s.with.trim()) };
            format!("- {}–{} · {}{with} · {}\n", s.from, s.to, s.place, s.what)
        })
        .collect();
    if !shape.is_empty() {
        p.push_str(&format!("<shape>\nWhere the day went.\n{shape}</shape>\n\n"));
    }
    if !brief.lede.trim().is_empty() {
        p.push_str(&format!("<lede>\n{}\n</lede>\n\n", brief.lede.trim()));
    }
    p.push_str("<moments>\nThe moments worth keeping, in order, each with the passages it lives in, word for word.\n");
    let mut n = 0;
    for m in brief.moments.iter().take(MAX_MOMENTS) {
        let mut seen: Vec<&str> = Vec::new();
        let mut passages = String::new();
        for at in m.at.iter().map(|a| a.trim()) {
            if seen.len() >= MAX_PASSAGES || seen.contains(&at) {
                continue;
            }
            let text = if let Some(t) = at.strip_prefix("msg ") {
                let lines: Vec<String> = input.messages.iter().filter(|x| x.hm == t).map(|x| format!("{}: {}", x.who, x.body)).collect();
                (!lines.is_empty()).then(|| lines.join("\n"))
            } else {
                input.chunks.iter().find(|c| c.hm == at).map(|c| c.text.clone())
            };
            if let Some(text) = text {
                seen.push(at);
                passages.push_str(&format!("[{at}] {text}\n\n"));
            }
        }
        if passages.is_empty() {
            continue;
        }
        n += 1;
        p.push_str(&format!("### {n}. {}\n(why: {})\n{passages}", m.title.trim(), m.why.trim()));
    }
    p.push_str("</moments>\n");
    p
}

/// Write the day. `None` when the day has no substance to write from.
///
/// Two calls, so neither has to hold the whole day and the page at once. The
/// scout (Lite) reads everything and chooses; the writer (Chat) reads the
/// choice and the record it points at, and writes. One call doing both took
/// long enough on a full day to outlast the completion timeout.
pub(crate) async fn write_day(pool: &PgPool, date: NaiveDate, tz: Option<&Tz>, start: &str, end: &str) -> Result<Option<String>> {
    let input = assemble(pool, date, tz, start, end).await?;
    if !input.has_substance() {
        tracing::info!(date = %date, "not enough of the day to write from - no model call");
        return Ok(None);
    }
    let raw = crate::virtues_api::completion::system_completion(
        pool,
        ModelSlot::Lite,
        "day_article_scout",
        SCOUT_PROMPT,
        &input.user_prompt,
        Thinking::Low,
        0.3,
    )
    .await?;
    let brief = parse_brief(&raw).filter(|b| !b.moments.is_empty()).ok_or_else(|| {
        crate::error::Error::Other(format!("day article scout returned no moments for {date}"))
    })?;
    let draft = crate::virtues_api::completion::system_completion(
        pool,
        ModelSlot::Standard,
        "day_article",
        WRITER_PROMPT,
        &writer_prompt(&brief, &input),
        Thinking::Low,
        0.7,
    )
    .await?;
    let (prose, chosen) = take_figure(&draft);
    let items = split_items(&prose);
    let verdicts = check(pool, &items, &input).await?;
    let names = known_names(&input);
    let mut figures: Vec<Figure> = chosen.as_ref().and_then(|f| verify_figure(f, &input)).into_iter().collect();
    if chosen.is_some() && figures.is_empty() {
        tracing::debug!(figure = ?chosen, "day article figure did not match the record - dropped");
    }
    if let Some(t) = route_moment(&day_fixes(pool, start, end).await?) {
        figures.push(Figure::route(hm(&t, tz)));
    }
    let md = render(&items, &verdicts, &input, &names, &figures);
    tracing::info!(
        date = %date,
        moments = brief.moments.len(),
        sentences = items.iter().filter(|i| matches!(i, Item::Sentence { .. })).count(),
        kept = verdicts.values().filter(|v| **v).count(),
        figures = figures.len(),
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
    regex::Regex::new(r"\[((?:msg |chat )?\d{1,2}:\d{2}(?:\s*,\s*(?:msg |chat )?\d{1,2}:\d{2})*)\]").expect("static regex")
}

fn parse_tags(inner: &str) -> Vec<String> {
    inner.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect()
}

/// A block's sentences, each with the tags it ended on. A sentence runs to its
/// tag, plus any closing punctuation after it; trailing text with no tag is a
/// sentence with none, which the check deletes.
fn tagged_sentences(b: &str, re: &regex::Regex) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    let mut pos = 0;
    for m in re.find_iter(b) {
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
            out.push((text, tags));
        }
        pos = end;
    }
    let rest = b[pos..].trim();
    if !rest.is_empty() {
        out.push((rest.to_string(), vec![]));
    }
    out
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
            // One item per sentence, so the check judges each like the body's.
            for (text, _) in tagged_sentences(a.trim(), &re) {
                items.push(Item::Abstract(text));
            }
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
            for (text, tags) in tagged_sentences(b, &re) {
                items.push(Item::Sentence { para, text, tags });
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

// ── The figure ─────────────────────────────────────────────────────────────

/// The fence a figure's block opens with. Its body is `key: value` lines,
/// which the page reads to draw the figure.
pub(crate) const FIGURE_FENCE: &str = "```figure";
/// A quote shorter than this is a word, not a voice; longer is a passage.
const QUOTE_MIN_WORDS: usize = 3;
const QUOTE_MAX_WORDS: usize = 35;
/// An exchange is a few messages in a row.
const EXCHANGE_MIN: usize = 2;
const EXCHANGE_MAX: usize = 4;

/// The figure the writer chose, as it wrote it on the last line of its output.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FigureDraft {
    Quote { text: String, tags: Vec<String> },
    Exchange { tags: Vec<String> },
}

/// A figure the page carries: the local time that places it in a section, and
/// its block's fields in order.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Figure {
    pub at: String,
    pub fields: Vec<(&'static str, String)>,
}

impl Figure {
    /// The day's route, placed at its longest movement.
    fn route(at: String) -> Self {
        Figure { at, fields: vec![("kind", "route".into())] }
    }

    fn markdown(&self) -> String {
        figure_block(self.fields.iter().map(|(k, v)| (*k, v.as_str())))
    }
}

/// A fenced `figure` block, each field on one line (a value's line breaks
/// would end its field).
pub(crate) fn figure_block<'a>(fields: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    let mut s = format!("{FIGURE_FENCE}\n");
    for (k, v) in fields {
        s.push_str(&format!("{k}: {}\n", v.split_whitespace().collect::<Vec<_>>().join(" ")));
    }
    s.push_str("```");
    s
}

/// The draft without its figure line, and the figure that line names. Every
/// figure line leaves the prose whether it reads or not: it is never a
/// sentence. The first one that reads is the figure.
pub(crate) fn take_figure(draft: &str) -> (String, Option<FigureDraft>) {
    let re = tag_re();
    let emphasis = |c: char| c == '*' || c == '_';
    let mut prose = Vec::new();
    let mut figure = None;
    for line in draft.lines() {
        let t = line.trim().trim_start_matches(emphasis);
        let read = if let Some(rest) = t.strip_prefix("Quote:") {
            Some(read_quote(rest.trim_start_matches(emphasis), &re))
        } else if let Some(rest) = t.strip_prefix("Exchange:") {
            Some(read_exchange(rest, &re))
        } else {
            None
        };
        match read {
            Some(f) => figure = figure.or(f),
            None => prose.push(line),
        }
    }
    (prose.join("\n"), figure)
}

/// `"<words>" [17:59]`: the words, and the passage's time from the last tag.
fn read_quote(rest: &str, re: &regex::Regex) -> Option<FigureDraft> {
    let m = re.find_iter(rest).last()?;
    let tags = re.captures(m.as_str()).map(|c| parse_tags(&c[1]))?;
    let text = rest[..m.start()].trim();
    let text = text.strip_prefix(|c: char| c == '"' || c == '“').unwrap_or(text);
    let text = text.strip_suffix(|c: char| c == '"' || c == '”').unwrap_or(text).trim();
    (!text.is_empty() && !tags.is_empty()).then(|| FigureDraft::Quote { text: text.to_string(), tags })
}

/// `[msg 21:44] [msg 21:45]`: message times only, each once.
fn read_exchange(rest: &str, re: &regex::Regex) -> Option<FigureDraft> {
    let mut tags: Vec<String> = Vec::new();
    for t in re.captures_iter(rest).flat_map(|c| parse_tags(&c[1])) {
        if !tags.contains(&t) {
            tags.push(t);
        }
    }
    (!tags.is_empty() && tags.iter().all(|t| t.starts_with("msg "))).then_some(FigureDraft::Exchange { tags })
}

/// What "exactly as the passage has it" compares: the words with the
/// writer's veil marks dropped, curly quotes straightened, and every run of
/// space made one.
fn verbatim(s: &str) -> String {
    let s: String = s
        .chars()
        .filter(|c| !matches!(c, '⟦' | '⟧'))
        .map(|c| match c {
            '‘' | '’' | 'ʼ' => '\'',
            '“' | '”' => '"',
            c => c,
        })
        .collect();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Spoken words a quote may leave out: they carry no meaning of the
/// speaker's, only the sound of talking.
const FILLERS: &[&str] = &["um", "uh", "uhm", "er", "erm", "ah", "like"];
/// How many spoken words one quote may leave out in all.
const MAX_LEFT_OUT: usize = 8;

/// Whether `quote` is what a recording said, cleaned the way a transcript
/// is for print: its words appear in the passage in order, one run, with
/// only fillers ("um", "like") and false starts left out between them. A
/// false start is a word the speaker repeats within a few words ("it's a,
/// it's like a modern take"). Every word the quote keeps was said, in that
/// order, in one voice's turn; nothing is reworded.
fn said_cleanly(quote: &str, passage: &str) -> bool {
    let turn_mark = regex::Regex::new(r"\[[^\]]*\]:").expect("static regex");
    let said = turn_mark.split(passage).any(|turn| said_in_turn(quote, turn));
    said
}

fn said_in_turn(quote: &str, passage: &str) -> bool {
    let tokens = |s: &str| -> Vec<String> {
        s.to_lowercase()
            .replace(['’', '‘', 'ʼ'], "'")
            .split(|c: char| !(c.is_alphanumeric() || c == '\''))
            .filter(|w| !w.is_empty())
            .map(str::to_string)
            .collect()
    };
    let q = tokens(quote);
    let p = tokens(passage);
    if q.is_empty() {
        return false;
    }
    (0..p.len()).filter(|&start| p[start] == q[0]).any(|start| {
        let (mut i, mut j, mut left_out) = (start, 0, 0);
        while j < q.len() && i < p.len() {
            if p[i] == q[j] {
                j += 1;
            } else {
                let repeated = p[i.saturating_sub(3)..i].contains(&p[i]);
                if !(FILLERS.contains(&p[i].as_str()) || repeated) || left_out == MAX_LEFT_OUT {
                    return false;
                }
                left_out += 1;
            }
            i += 1;
        }
        j == q.len()
    })
}

/// The figure, proven against the record and ready for the page, or `None`
/// when the record does not bear it out: a quote stands word for word in a
/// passage it cites, an exchange is messages in a row from one thread.
pub(crate) fn verify_figure(draft: &FigureDraft, input: &DayInput) -> Option<Figure> {
    match draft {
        FigureDraft::Quote { text, tags } => verify_quote(text, tags, input),
        FigureDraft::Exchange { tags } => verify_exchange(tags, input),
    }
}

fn verify_quote(text: &str, tags: &[String], input: &DayInput) -> Option<Figure> {
    let words = verbatim(text);
    if !(QUOTE_MIN_WORDS..=QUOTE_MAX_WORDS).contains(&words.split_whitespace().count()) {
        return None;
    }
    let quote = |at: &str, who: String, r: String| Figure {
        at: at.to_string(),
        fields: vec![
            ("kind", "quote".into()),
            ("text", text.to_string()),
            ("who", who),
            ("ref", r),
            ("time", twelve_hour(at)),
        ],
    };
    for tag in tags {
        if let Some(t) = tag.strip_prefix("msg ") {
            if let Some(m) = input.messages.iter().find(|m| m.hm == t && verbatim(&m.body).contains(&words)) {
                let who = if m.from_me {
                    "You".to_string()
                } else if input.people.iter().any(|p| p.name == m.who) {
                    m.who.clone()
                } else {
                    // A sender the wiki has no name for is a number or a handle.
                    "In a message".to_string()
                };
                return Some(quote(t, who, format!("data_communication_message:{}", m.id)));
            }
        } else if let Some(c) =
            input.chunks.iter().find(|c| c.hm == *tag && (verbatim(&c.text).contains(&words) || said_cleanly(&words, &c.text)))
        {
            // The recordings never say which voice is the owner's.
            return Some(quote(&c.hm, "In a recording".into(), format!("data_communication_transcription:{}", c.id)));
        }
    }
    None
}

fn verify_exchange(tags: &[String], input: &DayInput) -> Option<Figure> {
    let times: Vec<&str> = tags.iter().filter_map(|t| t.strip_prefix("msg ")).collect();
    if !(EXCHANGE_MIN..=EXCHANGE_MAX).contains(&times.len()) {
        return None;
    }
    // The thread with a message at every cited time.
    let thread = input
        .messages
        .iter()
        .filter(|m| m.hm == times[0] && !m.thread.is_empty())
        .map(|m| m.thread.as_str())
        .find(|th| times.iter().all(|t| input.messages.iter().any(|m| m.thread == *th && m.hm == *t)))?;
    let in_thread: Vec<&Msg> = input.messages.iter().filter(|m| m.thread == thread).collect();
    let picked: Vec<usize> = (0..in_thread.len()).filter(|&i| times.contains(&in_thread[i].hm.as_str())).collect();
    let (first, last) = (*picked.first()?, *picked.last()?);
    // In a row: no message of the thread between them is left out.
    if !(EXCHANGE_MIN..=EXCHANGE_MAX).contains(&picked.len()) || last - first + 1 != picked.len() {
        return None;
    }
    let refs: Vec<String> = picked.iter().map(|&i| format!("data_communication_message:{}", in_thread[i].id)).collect();
    Some(Figure { at: in_thread[first].hm.clone(), fields: vec![("kind", "thread".into()), ("refs", refs.join(", "))] })
}

// ── The route ──────────────────────────────────────────────────────────────

/// A day that moved this far in all has a route worth drawing...
const ROUTE_MIN_TOTAL_M: f64 = 20_000.0;
/// ...and so does one whose fixes lie this far apart: a flight, a long drive.
const ROUTE_MIN_SPAN_M: f64 = 200_000.0;
/// A fix the phone rates at or worse than this is a cell tower's guess, not
/// GPS: the Timeline's rule (`apps/web/src/lib/timeline/track.ts`).
const GPS_ACCURACY_MAX_M: f64 = 100.0;
/// A fix this far from both its neighbours, while they sit together, is a
/// spike: the Timeline's rule too.
const SPIKE_M: f64 = 300.0;
/// Distance counts in hops of at least this, so a still phone's jitter adds
/// nothing.
const HOP_M: f64 = 200.0;
/// Hops further apart in time than this are two movements with a stay between.
const MOVEMENT_GAP_MIN: i64 = 10;
/// A hop slower than this, in metres a second, is a phone waking up somewhere
/// new rather than travel.
const MOVING_MIN_MPS: f64 = 0.5;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Fix {
    pub at: DateTime<Utc>,
    pub lat: f64,
    pub lon: f64,
}

fn metres(a: &Fix, b: &Fix) -> f64 {
    crate::geo::haversine_distance(a.lat, a.lon, b.lat, b.lon)
}

/// The day's GPS fixes, in time order.
async fn day_fixes(pool: &PgPool, start: &str, end: &str) -> Result<Vec<Fix>> {
    let rows: Vec<(DateTime<Utc>, f64, f64)> = sqlx::query_as(
        "SELECT occurred_at, latitude, longitude FROM data_location_point \
         WHERE occurred_at >= $1::timestamptz AND occurred_at < $2::timestamptz \
           AND deleted_at_source IS NULL AND NOT is_archived \
           AND (horizontal_accuracy IS NULL OR horizontal_accuracy < $3) \
         ORDER BY occurred_at",
    )
    .bind(start)
    .bind(end)
    .bind(GPS_ACCURACY_MAX_M)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(at, lat, lon)| Fix { at, lat, lon }).collect())
}

/// The middle of the day's longest movement, when the day went far enough to
/// draw: 20 km in all, or fixes 200 km apart. `None` otherwise.
pub(crate) fn route_moment(fixes: &[Fix]) -> Option<DateTime<Utc>> {
    let fixes: Vec<Fix> = fixes
        .iter()
        .enumerate()
        .filter(|&(i, f)| match (i.checked_sub(1).map(|j| &fixes[j]), fixes.get(i + 1)) {
            (Some(p), Some(n)) => !(metres(p, f) > SPIKE_M && metres(f, n) > SPIKE_M && metres(p, n) < SPIKE_M),
            _ => true,
        })
        .map(|(_, f)| *f)
        .collect();
    let first = *fixes.first()?;

    // How far apart the fixes lie: the farthest from the first, then the
    // farthest from that.
    let farthest = |from: &Fix| fixes.iter().copied().max_by(|a, b| metres(from, a).total_cmp(&metres(from, b)));
    let a = farthest(&first)?;
    let span = farthest(&a).map_or(0.0, |b| metres(&a, &b));

    // Hops from an anchor to the first fix HOP_M away. A hop starts at the
    // last fix before it, so a stay is never inside the hop that leaves it.
    let mut hops: Vec<(f64, DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    let mut anchor = first;
    for w in fixes.windows(2) {
        let m = metres(&anchor, &w[1]);
        if m >= HOP_M {
            hops.push((m, w[0].at, w[1].at));
            anchor = w[1];
        }
    }
    let total: f64 = hops.iter().map(|h| h.0).sum();
    if total < ROUTE_MIN_TOTAL_M && span < ROUTE_MIN_SPAN_M {
        return None;
    }

    // Movements: moving hops with no stay between them.
    let mut moves: Vec<(f64, DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    let mut open = false;
    for &(m, from, to) in &hops {
        let secs = (to - from).num_seconds().max(1) as f64;
        if m / secs < MOVING_MIN_MPS {
            open = false;
            continue;
        }
        match moves.last_mut() {
            Some(mv) if open && (from - mv.2).num_minutes() <= MOVEMENT_GAP_MIN => {
                mv.0 += m;
                mv.2 = to;
            }
            _ => moves.push((m, from, to)),
        }
        open = true;
    }
    let longest = |v: &[(f64, DateTime<Utc>, DateTime<Utc>)]| v.iter().copied().max_by(|a, b| a.0.total_cmp(&b.0));
    let (_, s, e) = longest(&moves).or_else(|| longest(&hops))?;
    Some(s + (e - s) / 2)
}

/// The section a moment belongs to: the one whose span holds it, else the
/// last to begin before it, else the first. `None` when there are none.
/// Sections are `(heading item, (first time, last time))`, times "HH:MM".
fn section_for(at: &str, sections: &[(usize, (String, String))]) -> Option<usize> {
    sections
        .iter()
        .find(|(_, (a, b))| a.as_str() <= at && at <= b.as_str())
        .or_else(|| sections.iter().filter(|(_, (a, _))| a.as_str() <= at).last())
        .or(sections.first())
        .map(|(k, _)| *k)
}

// ── The check ──────────────────────────────────────────────────────────────

const CHECK_PROMPT: &str = r#"You check one diary page, sentence by sentence, against the evidence each sentence cites. For each numbered item, answer whether its evidence supports it.

- "supported": everything the sentence claims is in its cited evidence or in the rest of that same conversation (paraphrase is fine; the owner is "you"; people are listed below).
  A sentence that puts a named person somewhere ("with Nick", "Nick came over") needs the evidence to show them taking part; a name only read aloud or mentioned in passing does not put them there. This applies to that claim alone, not to everything else the sentence says.
- "unsupported": the sentence claims something its evidence does not contain (an invented detail, a descriptive word the evidence lacks, a relationship, a cause, a place, a person being present).
- "wrong_person": the sentence says who said or did something, and the evidence does not show it was them. Transcripts mark every change of voice as [Speaker] with no name, so a sentence may say who spoke only when the evidence names them or they answer someone by name. A sentence that keeps it shared ("between you", "one of you") is not wrong_person. A sentence that names the owner as someone else (they are always "you") is wrong_person.
- "untagged": the item cites no evidence.
Be strict about facts (invented details, relationships, causes, presence) and relaxed about wording ("your alarm was set for 6:30" supports "you set an alarm for 6:30").

Return ONLY a JSON array: [{"i": 1, "verdict": "..."}]

ITEMS:
"#;

fn evidence_for(tags: &[String], input: &DayInput) -> String {
    let mut ev = Vec::new();
    for tag in tags {
        if let Some(t) = tag.strip_prefix("msg ") {
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
    // Each body sentence against exactly what it cites. The Abstract is not
    // sent: a summary fails its few cited chunks on any day worth summing up,
    // and a model judging summaries deleted true ones run to run. What keeps
    // the Abstract honest is the writer's own rule about who was there, and
    // render's invented-name check, sentence by sentence.
    let body: Vec<usize> = (0..items.len()).filter(|&k| matches!(items[k], Item::Sentence { .. })).collect();
    check_batch(pool, items, &body, input, |k| match &items[k] {
        Item::Sentence { tags, .. } if !tags.is_empty() => evidence_for(tags, input),
        _ => "(none)".into(),
    })
    .await
}

/// One check call over some of the page's items. `evidence` gives each item's
/// evidence by its index.
async fn check_batch(
    pool: &PgPool,
    items: &[Item],
    idxs: &[usize],
    input: &DayInput,
    evidence: impl Fn(usize) -> String,
) -> Result<HashMap<usize, bool>> {
    let mut verdicts: HashMap<usize, bool> = HashMap::new();
    if idxs.is_empty() {
        return Ok(verdicts);
    }
    let text_of = |k: usize| match &items[k] {
        Item::Sentence { text, .. } | Item::Abstract(text) => text.as_str(),
        _ => "",
    };
    let mut numbered = String::new();
    for (n, &k) in idxs.iter().enumerate() {
        numbered.push_str(&format!("#{}\nSENTENCE: {}\nEVIDENCE:\n{}\n\n", n + 1, text_of(k), evidence(k)));
    }
    let people: String = input.people.iter().map(|p| format!("- {}\n", p.name)).collect();
    let owner = if input.owner.is_empty() {
        String::new()
    } else {
        format!("\nTHE OWNER, always \"you\" on the page: {}\n", input.owner.join(", "))
    };
    let raw = crate::virtues_api::completion::system_completion(
        pool,
        ModelSlot::Lite,
        "day_article_check",
        "",
        &format!("{CHECK_PROMPT}{numbered}\nPEOPLE:\n{people}{owner}"),
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
        for &k in idxs {
            verdicts.insert(k, true);
        }
        return Ok(verdicts);
    }
    for v in parsed {
        let n = v.get("i").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
        let verdict = v.get("verdict").and_then(|s| s.as_str()).unwrap_or("");
        if n >= 1 && n <= idxs.len() {
            let k = idxs[n - 1];
            let ok = verdict == "supported";
            if !ok {
                // The deleted sentence stays on the box, in its own log; it is
                // how a page that came back thin gets explained.
                tracing::debug!(verdict, sentence = text_of(k), "day article check removed a sentence");
            }
            verdicts.insert(k, ok);
        }
    }
    Ok(verdicts)
}

// ── Names ──────────────────────────────────────────────────────────────────

/// Every word a person's name on the page may use, lowercased: the day's
/// people, and every word in the day's own record.
fn known_names(input: &DayInput) -> BTreeSet<String> {
    // Every word, lowercased: a writer capitalizing what a message wrote in
    // lowercase ("the divine mercy chaplet") has not invented anyone.
    let word = regex::Regex::new(r"[\w'’\-]+").expect("static regex");
    let mut known = BTreeSet::new();
    let texts = input.people.iter().map(|p| p.name.as_str())
        .chain(input.chunks.iter().map(|c| c.text.as_str()))
        .chain(input.messages.iter().flat_map(|m| [m.body.as_str(), m.who.as_str()]));
    for t in texts {
        known.extend(word.find_iter(t).map(|m| m.as_str().to_lowercase()));
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
        if !known.contains(&c[1].to_lowercase()) || !known.contains(&c[2].to_lowercase()) {
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

/// The finished page: Abstract, sections, tables, figures, and their
/// footnotes. Evidence footnotes (`ev-N`) close the sentence they support;
/// the section time spans (`cx-N`) are drawn in the margin; each figure closes
/// the section that holds its time.
pub(crate) fn render(
    items: &[Item],
    keep: &HashMap<usize, bool>,
    input: &DayInput,
    known: &BTreeSet<String>,
    figures: &[Figure],
) -> String {
    let mut out = String::new();
    let mut notes: Vec<String> = Vec::new();
    let mut ev_n = 0usize;
    let mut cx_n = 0usize;
    let mut para: Option<usize> = None;
    let mut current = String::new();

    // Section time spans from the kept sentences' transcript tags, and the
    // wider bounds, messages included, that place a figure.
    let mut heading_spans: HashMap<usize, (String, String)> = HashMap::new();
    let mut bounds: Vec<(usize, (String, String))> = Vec::new();
    let mut open: Option<usize> = None;
    let widen = |span: &mut (String, String), t: &str| {
        if t < span.0.as_str() { span.0 = t.to_string(); }
        if t > span.1.as_str() { span.1 = t.to_string(); }
    };
    for (k, item) in items.iter().enumerate() {
        match item {
            Item::Heading(_) => open = Some(k),
            Item::Sentence { tags, .. } | Item::Table { tags, .. } if keep.get(&k).copied().unwrap_or(matches!(item, Item::Table { .. })) => {
                if let Some(h) = open {
                    for t in tags.iter().filter(|t| t.len() == 5 && t.as_bytes()[2] == b':') {
                        widen(heading_spans.entry(h).or_insert((t.clone(), t.clone())), t);
                    }
                    for t in tags.iter().map(|t| t.strip_prefix("msg ").unwrap_or(t)).filter(|t| t.len() == 5 && t.as_bytes()[2] == b':') {
                        match bounds.last_mut() {
                            Some((at, span)) if *at == h => widen(span, t),
                            _ => bounds.push((h, (t.to_string(), t.to_string()))),
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let mut placed: HashMap<Option<usize>, Vec<&Figure>> = HashMap::new();
    for f in figures {
        placed.entry(section_for(&f.at, &bounds)).or_default().push(f);
    }
    for v in placed.values_mut() {
        v.sort_by(|a, b| a.at.cmp(&b.at));
    }
    let close_section = |out: &mut String, section: Option<usize>| {
        for f in placed.get(&section).into_iter().flatten() {
            out.push_str(&f.markdown());
            out.push_str("\n\n");
        }
    };

    let flush = |out: &mut String, current: &mut String| {
        if !current.trim().is_empty() {
            out.push_str(current.trim());
            out.push_str("\n\n");
        }
        current.clear();
    };

    let mut section: Option<usize> = None;
    for (k, item) in items.iter().enumerate() {
        match item {
            // The Abstract's kept sentences make one paragraph, without
            // footnotes: it is the line read most and stays plain.
            Item::Abstract(a) => {
                if invented_name(a, input, known).is_some() {
                    continue;
                }
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(&untagged(a));
                if !matches!(items.get(k + 1), Some(Item::Abstract(_))) {
                    flush(&mut out, &mut current);
                }
            }
            Item::Heading(h) => {
                flush(&mut out, &mut current);
                if section.is_some() {
                    close_section(&mut out, section);
                }
                section = Some(k);
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
    close_section(&mut out, section);
    if section.is_some() {
        // Sections whose sentences cite no time leave their figures unplaced.
        close_section(&mut out, None);
    }
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

// ── The dateline's facts ───────────────────────────────────────────────────

/// The day's facts for the dateline above its title. Deterministic only: an
/// absent fact is `None` or empty, and the page omits it rather than guessing.
#[derive(Debug, serde::Serialize)]
pub struct DayFacts {
    pub temperature_high_c: Option<f64>,
    pub temperature_low_c: Option<f64>,
    /// Minutes the microphone was recording, silence included.
    pub recorded_minutes: i64,
    /// Recorded stretches, merged, as `[start, end]` instants.
    pub coverage: Vec<[DateTime<Utc>; 2]>,
    /// How long the day was: 1440 minutes, or 1380/1500 on a daylight-saving change.
    pub day_minutes: i64,
}

/// Recorded spans closer than this are one stretch on the strip.
const COVERAGE_MERGE_MIN: i64 = 10;

pub async fn day_facts(pool: &PgPool, date: NaiveDate) -> Result<DayFacts> {
    let day_tz = crate::timezone::day_timezone(pool, date).await?;
    let (start, end) = super::day_summary::day_boundaries_utc(date, Some(&day_tz));
    let (day_start, day_end) = super::day_summary::day_bounds(date, Some(&day_tz));
    let day_minutes = (day_end - day_start).num_minutes();

    let (high, low): (Option<f64>, Option<f64>) = sqlx::query_as(
        "SELECT max(temperature_c)::float8, min(temperature_c)::float8 FROM data_environment_weather \
         WHERE occurred_at >= $1::timestamptz AND occurred_at < $2::timestamptz AND NOT is_forecast",
    )
    .bind(&start)
    .bind(&end)
    .fetch_one(pool)
    .await?;

    let spans: Vec<(DateTime<Utc>, DateTime<Utc>)> = sqlx::query_as(
        "SELECT started_at, COALESCE(ended_at, started_at + interval '5 minutes') \
         FROM data_communication_transcription \
         WHERE started_at >= $1::timestamptz AND started_at < $2::timestamptz ORDER BY started_at",
    )
    .bind(&start)
    .bind(&end)
    .fetch_all(pool)
    .await?;
    let mut coverage: Vec<[DateTime<Utc>; 2]> = Vec::new();
    for (s, e) in spans {
        match coverage.last_mut() {
            Some(last) if (s - last[1]).num_minutes() <= COVERAGE_MERGE_MIN => last[1] = last[1].max(e),
            _ => coverage.push([s, e]),
        }
    }
    let recorded_minutes = coverage.iter().map(|[s, e]| (*e - *s).num_minutes()).sum();

    Ok(DayFacts { temperature_high_c: high, temperature_low_c: low, recorded_minutes, coverage, day_minutes })
}

// ── Similar days ───────────────────────────────────────────────────────────

/// A day whose page reads like this one's.
#[derive(Debug, serde::Serialize)]
pub struct SimilarDay {
    pub date: NaiveDate,
    /// The page's first paragraph (its Abstract), markdown.
    pub abstract_md: String,
    pub similarity: f64,
}

/// Days whose pages sit nearest this one's, by the embedding of each page's
/// opening chunk — the Abstract and the start of the first section, which is
/// where a page says what its day was. The days on either side are left out:
/// the page already links them.
pub async fn similar_days(pool: &PgPool, date: NaiveDate, limit: i64) -> Result<Vec<SimilarDay>> {
    let rows: Vec<(NaiveDate, String, f64)> = sqlx::query_as(
        "WITH me AS ( \
             SELECT v.embedding FROM wiki_days d \
             JOIN wiki_articles a ON a.subject_type = 'day' AND a.subject_id = d.id \
             JOIN search_embeddings e ON e.source_table = 'app_pages' AND e.record_id = a.page_id AND e.chunk_index = 0 \
             JOIN search_vectors v ON v.embedding_id = e.id \
             WHERE d.date = $1 LIMIT 1) \
         SELECT d.date, pg.content, (1 - (v.embedding <=> me.embedding))::float8 \
         FROM me, search_embeddings e \
         JOIN search_vectors v ON v.embedding_id = e.id \
         JOIN wiki_articles a ON a.page_id = e.record_id AND a.subject_type = 'day' \
         JOIN wiki_days d ON d.id = a.subject_id \
         JOIN app_pages pg ON pg.id = a.page_id \
         WHERE e.source_table = 'app_pages' AND e.chunk_index = 0 \
           AND d.date NOT BETWEEN $1 - 1 AND $1 + 1 \
         ORDER BY v.embedding <=> me.embedding \
         LIMIT $2",
    )
    .bind(date)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(date, content, similarity)| SimilarDay {
            date,
            abstract_md: crate::api::day_memory::abstract_of(&content).to_string(),
            similarity,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Timelike};

    fn input() -> DayInput {
        let t = |h, m| Utc.with_ymd_and_hms(2026, 1, 7, h, m, 0).unwrap();
        DayInput {
            user_prompt: String::new(),
            chunks: vec![
                Chunk { id: "tr_a".into(), start: t(17, 14), end: t(17, 19), hm: "17:14".into(), text: "[Speaker 1]: cross Fifth and Main, Nick".into() },
                Chunk { id: "tr_b".into(), start: t(17, 39), end: t(17, 44), hm: "17:39".into(), text: "[Speaker 2]: the most romantic thing?".into() },
            ],
            messages: vec![
                msg("msg_1", "th_dad", "19:40", "you", "home, walked the dog", true),
                msg("msg_2", "th_nick", "21:44", "Nick", "Did you get home safe?", false),
                msg("msg_3", "th_nick", "21:45", "you", "Yes, thank you", true),
                msg("msg_4", "th_nick", "21:47", "Nick", "Good. Sleep well", false),
                msg("msg_5", "th_group", "21:45", "Ana Ruiz", "who has the keys", false),
            ],
            people: vec![Person { id: "person_n".into(), name: "Nick".into() }],
            writer_context: String::new(),
            owner: vec!["David Okafor".into(), "David".into()],
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
        let md = render(&items, &keep, &inp, &known_names(&inp), &[]);
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
        let md = render(&items, &keep, &inp, &known_names(&inp), &[]);
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
        // The record wrote it in lowercase ("home, walked the dog"); capitals are not a new person.
        assert_eq!(invented_name("You walked the Dog Home.", &inp, &known), None);
        assert!(invented_name("You texted [Dave](/person/person_x).", &inp, &known).is_some());
    }

    #[test]
    fn an_invented_name_in_the_abstract_loses_only_its_sentence() {
        let items = split_items("Abstract: You walked home with [Nick](/person/person_n). [17:14] You met Margaret Thornbury. [17:39] It rained. [17:14]\n\n## The walk\n\nYou crossed Fifth and Main. [17:14]");
        assert!(matches!(&items[1], Item::Abstract(a) if a.contains("Margaret")));
        let keep: HashMap<usize, bool> = (0..items.len()).map(|k| (k, true)).collect();
        let inp = input();
        let md = render(&items, &keep, &inp, &known_names(&inp), &[]);
        assert!(md.starts_with("You walked home with [Nick](/person/person_n). It rained.\n\n## The walk"), "{md}");
    }

    #[test]
    fn a_brief_survives_fences_and_prose_around_it() {
        let raw = "Here it is:\n```json\n{\"lede\": \"A walk.\", \"moments\": [{\"title\": \"Home\", \"why\": \"w\", \"at\": [\"17:14\"]}]}\n```";
        let b = parse_brief(raw).expect("parses");
        assert_eq!(b.lede, "A walk.");
        assert_eq!(b.moments[0].at, vec!["17:14".to_string()]);
        assert!(parse_brief("no json at all").is_none());
    }

    #[test]
    fn the_writer_gets_each_moment_word_for_word_and_nothing_invented() {
        let mut inp = input();
        inp.writer_context = "# Wednesday\n\n".into();
        let brief = Brief {
            lede: "A walk.".into(),
            moments: vec![
                Moment { title: "The corner".into(), why: "w".into(), at: vec!["17:14".into(), "17:14".into(), "msg 19:40".into(), "03:00".into()] },
                Moment { title: "Nothing there".into(), why: "w".into(), at: vec!["04:00".into()] },
            ],
            ..Default::default()
        };
        let p = writer_prompt(&brief, &inp);
        assert!(p.starts_with("# Wednesday"), "{p}");
        assert!(p.contains("[17:14] [Speaker 1]: cross Fifth and Main, Nick"), "{p}");
        assert!(p.contains("[msg 19:40] you: home, walked the dog"), "{p}");
        assert_eq!(p.matches("[17:14]").count(), 1, "a repeated time is one passage: {p}");
        assert!(!p.contains("03:00") && !p.contains("Nothing there"), "times the record lacks, and a moment left empty, are dropped: {p}");
    }

    #[test]
    fn gaps_name_the_silences() {
        let t = |h, m| Utc.with_ymd_and_hms(2026, 1, 7, h, m, 0).unwrap();
        let g = gaps(&[(t(7, 46), t(8, 41)), (t(16, 59), t(17, 4))], None);
        assert!(g.contains("between 08:41 and 16:59"));
        assert!(g.starts_with("- Nothing was recorded before 07:46."));
    }

    fn msg(id: &str, thread: &str, hm: &str, who: &str, body: &str, from_me: bool) -> Msg {
        Msg { id: id.into(), thread: thread.into(), hm: hm.into(), who: who.into(), body: body.into(), from_me }
    }

    fn field<'a>(f: &'a Figure, key: &str) -> &'a str {
        f.fields.iter().find(|(k, _)| *k == key).map(|(_, v)| v.as_str()).unwrap_or("")
    }

    fn quote(text: &str, tag: &str) -> FigureDraft {
        FigureDraft::Quote { text: text.into(), tags: vec![tag.into()] }
    }

    fn exchange(tags: &[&str]) -> FigureDraft {
        FigureDraft::Exchange { tags: tags.iter().map(|t| t.to_string()).collect() }
    }

    /// The input with one recorded conversation at the table.
    fn with_table_talk() -> DayInput {
        let t = |h, m| Utc.with_ymd_and_hms(2026, 1, 7, h, m, 0).unwrap();
        let mut inp = input();
        inp.chunks.push(Chunk {
            id: "tr_c".into(),
            start: t(13, 17),
            end: t(13, 22),
            hm: "13:17".into(),
            text: "[Speaker]: We've all been to the Vatican.  How have you not\ngone? [Speaker]: Not yet.".into(),
        });
        inp
    }

    #[test]
    fn the_figure_line_leaves_the_prose() {
        let (prose, fig) = take_figure("Abstract: A walk. [17:14]\n\n## The walk\n\nYou crossed. [17:14]\n\nQuote: “We’ve all been to the Vatican.” [13:17]");
        assert!(!prose.contains("Quote") && prose.ends_with("You crossed. [17:14]\n"), "{prose}");
        assert_eq!(fig, Some(quote("We’ve all been to the Vatican.", "13:17")));
        let (_, ex) = take_figure("Abstract: A walk. [17:14]\n\n**Exchange:** [msg 21:44] [msg 21:45, msg 21:44]");
        assert_eq!(ex, Some(exchange(&["msg 21:44", "msg 21:45"])), "each time once, in order");
        // A figure line that doesn't read still leaves the prose, and is no figure.
        let (prose, none) = take_figure("Abstract: A walk. [17:14]\n\nExchange: [17:14] [msg 19:40]");
        assert_eq!(none, None);
        assert!(!prose.contains("Exchange"), "{prose}");
    }

    #[test]
    fn a_quote_is_kept_only_word_for_word() {
        let inp = with_table_talk();
        // Curly quotes, the writer's veil marks and the record's line break are not words.
        let f = verify_figure(&quote("We’ve all been to the ⟦Vatican⟧. How have you not gone?", "13:17"), &inp).expect("verbatim");
        assert_eq!(f.at, "13:17");
        assert_eq!(field(&f, "text"), "We’ve all been to the ⟦Vatican⟧. How have you not gone?");
        assert_eq!(field(&f, "who"), "In a recording");
        assert_eq!(field(&f, "ref"), "data_communication_transcription:tr_c");
        assert_eq!(field(&f, "time"), "1:17 PM");
        // One word changed.
        assert!(verify_figure(&quote("We've all been to Rome. How have you not gone?", "13:17"), &inp).is_none());
        // The right words, cited to the wrong passage.
        assert!(verify_figure(&quote("We've all been to the Vatican.", "17:14"), &inp).is_none());
        // Across a change of voice: the record never said it in one breath.
        assert!(verify_figure(&quote("How have you not gone? Not yet.", "13:17"), &inp).is_none());
        // Too short to be a voice.
        assert!(verify_figure(&quote("Not yet.", "13:17"), &inp).is_none());
    }

    #[test]
    fn a_quoted_message_says_who_sent_it() {
        let inp = input();
        let you = verify_figure(&quote("Yes, thank you", "msg 21:45"), &inp).expect("your message");
        assert_eq!((field(&you, "who"), field(&you, "ref")), ("You", "data_communication_message:msg_3"));
        let nick = verify_figure(&quote("Did you get home safe?", "msg 21:44"), &inp).expect("Nick's message");
        assert_eq!(field(&nick, "who"), "Nick");
        // A sender the day's people don't name is not named.
        let other = verify_figure(&quote("who has the keys", "msg 21:45"), &inp).expect("the group's message");
        assert_eq!((field(&other, "who"), field(&other, "ref")), ("In a message", "data_communication_message:msg_5"));
    }

    #[test]
    fn an_exchange_is_messages_in_a_row_from_one_thread() {
        let inp = input();
        let f = verify_figure(&exchange(&["msg 21:44", "msg 21:45"]), &inp).expect("one thread, in a row");
        assert_eq!(f.at, "21:44");
        assert_eq!(field(&f, "kind"), "thread");
        assert_eq!(field(&f, "refs"), "data_communication_message:msg_2, data_communication_message:msg_3");
        // 21:45 has a message in the group too; only Nick's thread holds both times.
        let f = verify_figure(&exchange(&["msg 21:45", "msg 21:44"]), &inp).expect("the thread with both");
        assert_eq!(field(&f, "refs"), "data_communication_message:msg_2, data_communication_message:msg_3");
        // No one thread holds 19:40 and 21:45.
        assert!(verify_figure(&exchange(&["msg 19:40", "msg 21:45"]), &inp).is_none(), "cross-thread");
        // 21:44 and 21:47 leave out the reply between them.
        assert!(verify_figure(&exchange(&["msg 21:44", "msg 21:47"]), &inp).is_none(), "not in a row");
        assert!(verify_figure(&exchange(&["msg 21:44"]), &inp).is_none(), "one message is no exchange");
    }

    #[test]
    fn figures_close_the_section_that_holds_their_time() {
        let draft = "Abstract: A day. [17:14]\n\n## The corner\n\nYou crossed Fifth and Main. [17:14]\n\n## Home\n\nYou got home. [msg 19:40] Nick asked how it went. [msg 21:44]\n\nQuote: \"Did you get home safe?\" [msg 21:44]";
        let (prose, chosen) = take_figure(draft);
        let items = split_items(&prose);
        let keep: HashMap<usize, bool> = (0..items.len()).map(|k| (k, true)).collect();
        let inp = input();
        let quote = verify_figure(&chosen.expect("a figure"), &inp).expect("verbatim");
        let md = render(&items, &keep, &inp, &known_names(&inp), &[quote, Figure::route("17:30".into())]);
        let at = |s: &str| md.find(s).unwrap_or_else(|| panic!("{s} missing: {md}"));
        // 17:30 is after the corner's last time and before home began: the corner holds it.
        assert!(at("## The corner") < at("kind: route") && at("kind: route") < at("## Home"), "{md}");
        assert!(at("## Home") < at("kind: quote") && at("kind: quote") < at("[^ev-1]:"), "{md}");
        assert!(
            md.contains("Nick asked how it went.[^ev-3]\n\n```figure\nkind: quote\ntext: Did you get home safe?\nwho: Nick\nref: data_communication_message:msg_2\ntime: 9:44 PM\n```\n\n[^cx-1]: 5:14 PM\n[^ev-1]:"),
            "{md}"
        );
        assert!(md.contains("You crossed Fifth and Main.[^ev-1]\n\n```figure\nkind: route\n```\n\n## Home"), "{md}");
    }

    #[test]
    fn a_page_without_sections_ends_its_body_with_the_figure() {
        let items = split_items("Abstract: A walk. [17:14]\n\nYou walked the dog. [msg 19:40]");
        let keep: HashMap<usize, bool> = (0..items.len()).map(|k| (k, true)).collect();
        let inp = input();
        let md = render(&items, &keep, &inp, &known_names(&inp), &[Figure::route("12:00".into())]);
        assert!(md.contains("You walked the dog.[^ev-1]\n\n```figure\nkind: route\n```\n\n[^ev-1]: Message"), "{md}");
    }

    #[test]
    fn earlier_pages_reach_the_writer_without_their_figures() {
        let page = "A day.\n\n## Home\n\nYou got home.[^ev-1]\n\n```figure\nkind: quote\ntext: Did you get home safe?\n```\n\nMore.\n\n[^ev-1]: Message · 9:44 PM";
        let s = strip_footnotes(page);
        assert!(!s.contains("figure") && !s.contains("safe") && !s.contains("[^"), "{s}");
        assert!(s.contains("You got home.") && s.contains("More."), "{s}");
    }

    fn fix(h: u32, m: u32, lat: f64, lon: f64) -> Fix {
        Fix { at: Utc.with_ymd_and_hms(2026, 1, 7, h, m, 0).unwrap(), lat, lon }
    }

    /// A still phone, one fix every three minutes from midnight, jittering
    /// some 20 m around one spot.
    fn home_day() -> Vec<Fix> {
        (0..480u32).map(|i| fix(i * 3 / 60, i * 3 % 60, 41.9 + 0.0001 * ((i % 5) as f64 - 2.0), -87.63)).collect()
    }

    #[test]
    fn a_day_at_home_has_no_route() {
        assert_eq!(route_moment(&home_day()), None);
        assert_eq!(route_moment(&[]), None);
        // Spikes out and back (bad fixes the phone rated as good) are no trips,
        // though ten of them would add 40 km.
        let mut spiky = home_day();
        for k in 0..10 {
            spiky[40 * k + 20].lat += 0.02;
        }
        assert_eq!(route_moment(&spiky), None);
    }

    #[test]
    fn a_drive_across_town_is_a_route_and_a_short_one_is_not() {
        let drive = |km: u32| {
            let mut day: Vec<Fix> = home_day().into_iter().filter(|f| f.at.hour() < 14).collect();
            // One kilometre a minute, north from home, from 14:00.
            day.extend((1..=km).map(|k| fix(14 + k / 60, k % 60, 41.9 + 0.009 * k as f64, -87.63)));
            day
        };
        assert_eq!(route_moment(&drive(8)), None, "8 km in all");
        let mid = route_moment(&drive(25)).expect("25 km");
        // From the last fix at home (13:57) to the last on the road (14:25).
        assert_eq!((mid.hour(), mid.minute()), (14, 11), "the middle of the drive: {mid}");
    }

    #[test]
    fn a_flight_puts_the_route_in_the_middle_of_the_long_move() {
        let mut day: Vec<Fix> = home_day().into_iter().filter(|f| f.at.hour() < 10).collect();
        // A 3 km walk in the morning, then the afternoon at the airport.
        day.extend((1..=20).map(|k| fix(10, 2 * k, 41.9 + 0.0013 * k as f64, -87.63)));
        day.extend((0..=16).map(|k| fix(17 + 5 * k / 60, 5 * k % 60, 39.861, -104.673)));
        // Nothing in the air; the phone wakes up a long way off, then drives home.
        day.push(fix(20, 45, 42.365, -71.010));
        day.extend((1..=15u32).map(|k| fix(20 + (45 + k) / 60, (45 + k) % 60, 42.365 + 0.009 * k as f64, -71.010)));
        let mid = route_moment(&day).expect("a flight");
        // From the last fix at the airport (18:20) to the end of the drive (21:00).
        assert_eq!((mid.hour(), mid.minute()), (19, 40), "{mid}");
    }

    #[test]
    fn folds_photos_and_skips_reactions() {
        assert_eq!(fold_photos("[Photo][Photo][Photo]"), "[3 photos]");
        assert!(is_reaction("Loved an image"));
        assert_eq!(clean_name("Nick 🌷"), "Nick");
    }

    /// The writer can only say "you" for the right voice if it knows the
    /// owner's name: someone who says it is someone else.
    #[test]
    fn the_owner_is_named_for_the_writer() {
        let b = you_block(&["David Okafor".into(), "David".into()]);
        assert!(b.contains("This is David Okafor's day (also David)"), "{b}");
        assert!(b.contains("A speaker who says \"David\" is someone else"), "{b}");
        assert_eq!(you_block(&[]), "", "no owner on record: nothing to say");
    }

    #[test]
    fn a_spoken_quote_may_drop_fillers_and_false_starts_but_nothing_else() {
        let said = "[Speaker]: It's, I like it a lot because it's a, it's like a modern take on like the Catholic schoolgirl.";
        assert!(said_cleanly("I like it a lot because it's a modern take on the Catholic schoolgirl.", said));
        // A word nobody said, or the words out of order, is not a quote.
        assert!(!said_cleanly("I love it because it's a modern take on the Catholic schoolgirl.", said));
        assert!(!said_cleanly("A modern take on the schoolgirl, I like it a lot.", said));
        // Leaving out a word that carried meaning is rewording.
        assert!(!said_cleanly("I like it because it's a modern take on the Catholic schoolgirl.", said));
    }

}

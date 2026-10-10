//! The assembly seam for the system prompt.
//!
//! One ordered list of named blocks, rendered in list order — the registry
//! the formula doc (docs/narrative-identity.md, "Its place in the system
//! prompt") builds toward. Slice 1 made the seam (byte-identical wrap of the
//! old push_str chain); slice 2 made the first formula moves: rules render
//! LAST (constraint recency), and the precedence ladder is stated as text the
//! model can cite. <circumstances>, <coverage> and the cache breakpoint are
//! built (the breakpoint falls before the first per-turn block — see
//! `assemble`); the first two are held per chat (`held_per_chat`), and the
//! current time rides on each user message rather than in this prompt.
//! Still to come as one-list edits: the head split
//! (<character>/<narrative_identity>/<tools>) and per-block budgets.
//!
//! Error policy: a block that fails renders nothing and says so in the log —
//! never a default, never fabricated bytes (the house swallowed-query rule,
//! made structural). Today every wrapped builder still handles its own
//! errors internally, exactly as it did before this seam existed; new blocks
//! get the policy for free by returning `None` on failure and logging.
//!
//! Determinism: every query inside a block must end in a total ORDER BY, and
//! no block may call `Utc::now()` more than once per assemble — unstable
//! bytes silently kill provider prompt caching. Enforced by review until the
//! cadence test lands with the reorder slice.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, Utc};
use futures::future::BoxFuture;

/// Who authors a block's content. Rendered into nothing today; the
/// precedence preamble renders from this once the reorder slice lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Author {
    /// Us — the product's own voice (persona, tool guidance).
    System,
    /// The person — authored, ratified words (narrative identity, rules).
    User,
    /// The machine's accumulated notes (memory).
    Machine,
    /// Deterministic computation over the record (clock, user context).
    Computed,
    /// The UI's live state (open project, open page).
    Ui,
}

/// Declarative blocks describe; imperative blocks bind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Mood {
    Declarative,
    Imperative,
}

/// How often the rendered bytes change — cache metadata. Once the reorder
/// slice lands, registry order must be non-decreasing in cadence (stable
/// prefix first), with the one deliberate exception of rules-last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[allow(dead_code)]
pub enum Cadence {
    /// Never changes for a given box (persona, tool guidance).
    Static,
    /// Months–years (narrative identity, rules).
    Slow,
    /// Changes within a session but not per turn (memory, project, and the
    /// present, held per chat).
    Session,
    /// Changes every turn (the open page's live content).
    PerTurn,
}

/// One named section of the prompt.
pub struct BlockMeta {
    /// The XML tag (and audit key). For the fused head block — base + persona
    /// + narrative identity + tools, still one string from
    /// `build_personalized_prompt` — this is `"base"` until the reorder slice
    /// splits it.
    pub tag: &'static str,
    #[allow(dead_code)]
    pub author: Author,
    #[allow(dead_code)]
    pub mood: Mood,
    /// Precedence rung: higher outranks lower when sections conflict.
    #[allow(dead_code)]
    pub rung: u8,
    #[allow(dead_code)]
    pub cadence: Cadence,
}

/// A block: metadata plus the future that renders it. `None` = legitimately
/// absent — the section renders nothing at all (an empty `<rules>` block
/// would teach the model the section is usually noise).
pub struct Block<'a> {
    pub meta: BlockMeta,
    pub body: BoxFuture<'a, Option<String>>,
}

/// One rendered block, for audits: which tag produced how many chars.
#[derive(Debug)]
pub struct RenderedBlock {
    pub tag: &'static str,
    #[allow(dead_code)]
    pub chars: usize,
}

/// Render the blocks: fan the bodies out concurrently, concatenate strictly
/// in list order. Blocks own their separators (each body starts with the
/// `\n\n` its section always carried), so assembly is pure concatenation and
/// the output is byte-identical to the old push_str chain.
pub async fn assemble(blocks: Vec<Block<'_>>) -> (String, String, Vec<RenderedBlock>) {
    let (metas, bodies): (Vec<_>, Vec<_>) =
        blocks.into_iter().map(|b| (b.meta, b.body)).unzip();
    let results = futures::future::join_all(bodies).await;

    let mut stable = String::new();
    let mut volatile = String::new();
    let mut in_tail = false;
    let mut rendered = Vec::new();
    for (meta, body) in metas.into_iter().zip(results) {
        // A cache breakpoint marks a PREFIX, so the split is the FIRST block
        // that changes every turn — not every volatile block. `rules` is Slow
        // but deliberately sits last, behind the per-turn tail, so it falls on
        // the uncached side too; that is exactly what its own placement note
        // in the registry describes, and it is a few hundred tokens.
        //
        // The boundary is fixed by cadence, not by whether the block rendered
        // anything this turn. An `active_context` that renders nothing when no
        // page is open would otherwise move `rules` in and out of the cached
        // prefix as pages are bound, which busts the cache on a transition
        // that has no business touching it.
        if meta.cadence == Cadence::PerTurn {
            in_tail = true;
        }
        if let Some(body) = body {
            rendered.push(RenderedBlock { tag: meta.tag, chars: body.chars().count() });
            if in_tail {
                volatile.push_str(&body);
            } else {
                stable.push_str(&body);
            }
        }
    }
    (stable, volatile, rendered)
}

/// How long a chat keeps the present it was shown.
///
/// Grok caches whole requests: the next request reads from cache only where
/// an earlier one ended, so one changed byte anywhere in the system message
/// costs the prompt and the whole history behind it. Measured on grok-4.7 on
/// 2026-10-09, editing one line of `<circumstances>` read 1,152 of 9,535
/// tokens from cache (xAI's own preamble) where the unchanged request read
/// 9,472. That block moves with every ingest and its clock moved every quarter
/// hour, so on the main box 57% of turns in the same quarter hour, and 88% of
/// those that crossed one, started from nothing.
const HELD_FOR: std::time::Duration = std::time::Duration::from_secs(60 * 60);

struct Held {
    zone: Option<String>,
    date: chrono::NaiveDate,
    taken_at: DateTime<Utc>,
    body: Option<String>,
}

type HeldKey = (String, &'static str);

fn held() -> &'static Mutex<HashMap<HeldKey, Held>> {
    static HELD: OnceLock<Mutex<HashMap<HeldKey, Held>>> = OnceLock::new();
    HELD.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A block's body as this chat was first shown it, rendered again only once it
/// is `HELD_FOR` old, the date has turned where they are, or their zone has
/// changed. `render` gets the instant to render as of. Without a chat (audits,
/// tests) every call renders. Held in memory: a restart costs each open chat
/// one cache miss.
pub async fn held_per_chat<F, Fut>(
    chat_id: Option<&str>,
    tag: &'static str,
    timezone: Option<&str>,
    render: F,
) -> Option<String>
where
    F: FnOnce(DateTime<Utc>) -> Fut,
    Fut: Future<Output = Option<String>>,
{
    held_as_of(Utc::now(), chat_id, tag, timezone, render).await
}

async fn held_as_of<F, Fut>(
    now: DateTime<Utc>,
    chat_id: Option<&str>,
    tag: &'static str,
    timezone: Option<&str>,
    render: F,
) -> Option<String>
where
    F: FnOnce(DateTime<Utc>) -> Fut,
    Fut: Future<Output = Option<String>>,
{
    let Some(chat_id) = chat_id else {
        return render(now).await;
    };
    let zone = timezone.map(str::to_string);
    let date = match timezone.and_then(|t| t.parse::<chrono_tz::Tz>().ok()) {
        Some(tz) => now.with_timezone(&tz).date_naive(),
        None => now.date_naive(),
    };
    let key = (chat_id.to_string(), tag);
    let fresh = |h: &Held| {
        h.zone == zone
            && h.date == date
            && (now - h.taken_at).to_std().is_ok_and(|age| age < HELD_FOR)
    };
    if let Some(h) = held().lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        if fresh(h) {
            return h.body.clone();
        }
    }

    let body = render(now).await;
    let mut map = held().lock().unwrap_or_else(|e| e.into_inner());
    map.retain(|_, h| (now - h.taken_at).to_std().is_ok_and(|age| age < HELD_FOR));
    map.insert(key, Held { zone, date, taken_at: now, body: body.clone() });
    body
}

/// The precedence ladder, stated once near the head of the prompt. Rendered
/// text rather than an emergent property of block order, so the model can
/// cite the ranking instead of inferring it. Keep in sync with the registry's
/// `rung` values — the reorder slice's audit test asserts both exist.
pub fn precedence_line() -> &'static str {
    "\n\n<precedence>\nWhen sections of this prompt conflict: <rules> outrank everything and are absolute; <narrative_identity> outranks the machine's own <memory>; both outrank house guidance; <circumstances> is situational fact, not instruction. Declarative sections describe — only <rules> command.\n</precedence>"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    async fn render_counted(
        now: DateTime<Utc>,
        chat: Option<&str>,
        zone: Option<&str>,
        calls: &AtomicUsize,
    ) -> Option<String> {
        held_as_of(now, chat, "test_block", zone, |at| async move {
            let n = calls.fetch_add(1, Ordering::SeqCst);
            Some(format!("render {n} as of {at}"))
        })
        .await
    }

    /// A chat keeps the bytes it was first shown until the hold runs out, the
    /// day turns where they are, or their zone changes.
    #[tokio::test]
    async fn a_chat_keeps_its_present_until_an_hour_or_a_day_turns() {
        let calls = AtomicUsize::new(0);
        let chat = Some("chat_held_test");
        let zone = Some("America/Chicago");
        // 14:00 in Chicago.
        let t0 = DateTime::parse_from_rfc3339("2026-10-09T19:00:00Z").unwrap().with_timezone(&Utc);
        let minutes = |m: i64| t0 + chrono::TimeDelta::minutes(m);

        let first = render_counted(t0, chat, zone, &calls).await;
        assert_eq!(render_counted(minutes(59), chat, zone, &calls).await, first, "held within the hour");
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        let other_chat = render_counted(minutes(1), Some("chat_held_other"), zone, &calls).await;
        assert_ne!(other_chat, first, "each chat holds its own");

        let after_hour = render_counted(minutes(61), chat, zone, &calls).await;
        assert_ne!(after_hour, first, "read again once an hour old");

        let moved = render_counted(minutes(62), chat, Some("Europe/London"), &calls).await;
        assert_ne!(moved, after_hour, "read again when their zone changes");

        // 23:30 → 00:10 in London: a new day inside the hour.
        let late = DateTime::parse_from_rfc3339("2026-10-09T22:30:00Z").unwrap().with_timezone(&Utc);
        let before_midnight = render_counted(late, chat, Some("Europe/London"), &calls).await;
        let after_midnight =
            render_counted(late + chrono::TimeDelta::minutes(40), chat, Some("Europe/London"), &calls).await;
        assert_ne!(after_midnight, before_midnight, "read again when the day turns");

        let a = render_counted(t0, None, zone, &calls).await;
        let b = render_counted(t0, None, zone, &calls).await;
        assert_ne!(a, b, "without a chat nothing is held");
    }
}

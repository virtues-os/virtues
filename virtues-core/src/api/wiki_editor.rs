//! The wiki editor: one role, one constitution, one brief per subject kind.
//!
//! Design: `agents/record/article-resolution.md`. How the prompts are
//! organized and why: `agents/build/wiki-editor.md`.
//!
//! This module holds the parts that are pure functions of text — prompt
//! assembly, the lede, provenance, and the invariant the server enforces on
//! every machine edit. They are here rather than inside the applet because
//! they are the half of article resolution that can be tested without a model,
//! a CRDT or a scheduler, and because the invariant is the thing that makes
//! two pens on one article safe.

use crate::error::{Error, Result};

/// The rules that never vary, shared by every article. Compiled in, so the
/// file that is reviewed is the text that runs.
pub const CONSTITUTION: &str = include_str!("../../prompts/wiki/constitution.md");

/// What one KIND of page is. The constitution says how to write; a brief says
/// what this page is for.
pub const YEAR_BRIEF: &str = include_str!("../../prompts/wiki/year.md");
pub const ENTITY_BRIEF: &str = include_str!("../../prompts/wiki/entity.md");
pub const STORY_BRIEF: &str = include_str!("../../prompts/wiki/story.md");
pub const CHAPTER_BRIEF: &str = include_str!("../../prompts/wiki/chapter.md");

/// Which brief a subject type gets, from the registry.
///
/// Two absences are refusals rather than gaps, and both are structural on
/// purpose — a prompt rule is something a model weighs against its other
/// rules, and a missing brief is not. Which two, and why, is recorded on the
/// rows themselves in [`crate::api::subjects::SUBJECTS`].
pub fn brief_for(subject_type: &str) -> Option<&'static str> {
    crate::api::subjects::by_kind(subject_type)?.brief
}

/// The editor's system prompt: constitution, then brief, then the person's
/// standing rules.
///
/// The rules ride here for the same reason they ride in chat
/// (`api::chat`): without them the year article can name the thing the
/// assistant is forbidden to raise, which is worse than the assistant naming
/// it — prose persists, and a person meets it again every time they open the
/// page.
pub fn system_prompt(subject_type: &str, rules: &[String]) -> Result<String> {
    let brief = brief_for(subject_type).ok_or_else(|| {
        Error::InvalidInput(format!("No wiki brief for subject type {subject_type}"))
    })?;
    let mut p = String::with_capacity(CONSTITUTION.len() + brief.len() + 512);
    p.push_str(CONSTITUTION);
    p.push_str("\n\n---\n\n");
    p.push_str(brief);
    if !rules.is_empty() {
        p.push_str("\n\n---\n\n## Standing rules from the owner\n\n");
        p.push_str(
            "These are instructions, not context. They are absolute, they \
             outrank anything you infer from the record, and they apply to \
             this article as they apply to every other surface.\n\n",
        );
        for r in rules {
            p.push_str("- ");
            p.push_str(r.trim());
            p.push('\n');
        }
    }
    Ok(p)
}

/// The LEDE: an article's opening paragraph, which is the short form every
/// rung above it reads.
///
/// The first block that is neither blank nor a markdown heading. The narrate
/// prompt requires an article to open with a lede carrying no heading; the
/// skip exists because a human edit that adds a heading above it must not turn
/// that heading into the summary.
///
/// **One rule, three languages.** [`crate::api::wiki_editor::lede_sql`] is the SQL
/// spelling, for list queries that must not fetch whole articles; `lede()` in
/// `apps/web/src/lib/wiki/lede.ts` is the client's. This one and the SQL one
/// are checked against each other by `lede_and_lede_sql_agree`, because three
/// hand-written copies of a rule is how two spellings of it start disagreeing
/// — which they had: the chronicle's copy returned a heading as the lede.
pub fn lede(article: &str) -> Option<&str> {
    article
        .split("\n\n")
        .map(str::trim)
        .find(|b| !b.is_empty() && !b.starts_with('#'))
}

/// THE LEDE, in SQL — an article's opening paragraph, which is the short form
/// every rung above it reads.
///
/// `text_expr` is any SQL expression yielding article prose:
/// `wiki_day_prose.prose` for a day, `app_pages.content` for every other
/// subject. The rule is not day-specific and never was.
///
/// The first block that is neither blank nor a markdown heading. Every brief
/// requires an article to open with a lede carrying no heading; the skip
/// exists because a human edit that adds a heading above it must not turn that
/// heading into the summary.
///
/// **This is one of two implementations of one rule** — the other is
/// [`crate::api::wiki_editor::lede`], for callers that already hold the text —
/// and `lede_and_lede_sql_agree` is the test that keeps them from drifting.
/// The third, in TypeScript, is `lede()` in `apps/web/src/lib/wiki/lede.ts`,
/// and it is one function rather than a copy per component for the same
/// reason.
pub fn lede_sql(text_expr: &str) -> String {
    format!(
        "(SELECT btrim(b) FROM unnest(string_to_array({text_expr}, E'\\n\\n')) AS b \
          WHERE btrim(b) <> '' AND left(btrim(b), 1) <> '#' LIMIT 1)"
    )
}

/// Split prose into sentences, for provenance.
///
/// Deliberately crude, and biased toward splitting too little rather than too
/// much: a sentence that is really two costs nothing (it is compared whole),
/// while a sentence wrongly split at "Dr." or "3 p.m." would make the
/// invariant fail on text nobody touched. A known abbreviation or a one-letter
/// token therefore never ends a sentence, even when what follows looks like the
/// start of one.
pub fn sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        cur.push(c);
        if matches!(c, '.' | '!' | '?') {
            // A terminator only ends a sentence when whitespace and something
            // that looks like a new sentence follow it — and when the word it
            // closes is not an abbreviation. "Met Dr. Vance" is one sentence;
            // so is "at 3.30 p.m. The train was quiet", because a period after
            // a one-letter token is ambiguous even to a person and the safe
            // reading is the one that does not split.
            let word: String = cur
                .trim_end_matches(['.', '!', '?'])
                .chars()
                .rev()
                .take_while(|c| c.is_alphanumeric())
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            const ABBREV: &[&str] = &[
                "dr", "mr", "mrs", "ms", "prof", "st", "ave", "rd", "no", "vs",
                "jan", "feb", "mar", "apr", "jun", "jul", "aug", "sep", "sept",
                "oct", "nov", "dec", "approx", "etc", "eg", "ie",
            ];
            let abbrev = c == '.'
                && (word.chars().count() <= 1 || ABBREV.contains(&word.to_lowercase().as_str()));
            let mut look = chars.clone();
            let ws = matches!(look.peek(), Some(c) if c.is_whitespace());
            if ws && !abbrev {
                while matches!(look.peek(), Some(c) if c.is_whitespace()) {
                    look.next();
                }
                let starts_new = look
                    .peek()
                    .is_none_or(|n| n.is_uppercase() || n.is_ascii_digit() || *n == '[' || *n == '#');
                if starts_new {
                    let s = cur.trim().to_string();
                    if !s.is_empty() {
                        out.push(s);
                    }
                    cur.clear();
                }
            }
        }
    }
    let s = cur.trim().to_string();
    if !s.is_empty() {
        out.push(s);
    }
    out
}

/// What the person has done to an article since the editor last left it.
///
/// `machine_text` is the article exactly as the editor wrote it. Anything in
/// the live text that is not in it, they added; anything in it that is no
/// longer in the live text, they removed. That is the whole of provenance, and
/// it needs no per-character attribution and no version history.
///
/// Returns `(added, removed)` as sentences.
pub fn provenance(machine_text: &str, live_text: &str) -> (Vec<String>, Vec<String>) {
    let before = sentences(machine_text);
    let after = sentences(live_text);
    let added = after
        .iter()
        .filter(|s| !before.contains(s))
        .cloned()
        .collect();
    let removed = before
        .iter()
        .filter(|s| !after.contains(s))
        .cloned()
        .collect();
    (added, removed)
}

/// Fold a new observation of the person's edits into the stored sets.
///
/// Both sets are pruned against the live text so they cannot grow without
/// bound: a sentence they wrote and later deleted stops being theirs and
/// becomes a removal; a removal they typed back in themselves stops being a
/// removal.
pub fn fold_provenance(
    theirs: &[String],
    removed: &[String],
    added_now: &[String],
    removed_now: &[String],
    live_text: &str,
) -> (Vec<String>, Vec<String>) {
    let live = sentences(live_text);
    let mut t: Vec<String> = theirs
        .iter()
        .chain(added_now.iter())
        .filter(|s| live.contains(s))
        .cloned()
        .collect();
    t.dedup();
    let mut r: Vec<String> = removed
        .iter()
        .chain(removed_now.iter())
        .filter(|s| !live.contains(s))
        .cloned()
        .collect();
    r.dedup();
    (t, r)
}

/// Why a proposed edit was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum Violation {
    /// A sentence the person wrote is not present verbatim in the new text.
    LostTheirs(String),
    /// A sentence the person deleted has come back.
    RestoredRemoved(String),
    /// The article no longer opens with a paragraph.
    NoLede,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Violation::LostTheirs(s) => {
                write!(f, "a sentence the owner wrote did not survive: {s:?}")
            }
            Violation::RestoredRemoved(s) => {
                write!(f, "a sentence the owner deleted was restored: {s:?}")
            }
            Violation::NoLede => write!(f, "the article no longer opens with a paragraph"),
        }
    }
}

/// The invariant, checked on a scratch copy before anything reaches the live
/// document.
///
/// This is what makes two pens on one article safe, and it is the reason the
/// claim flip could be removed. The constitution ASKS the editor to leave
/// their words alone; this REFUSES the edit if it did not. A rule a prompt
/// merely states is a rule that holds most of the time.
pub fn check_edit(new_text: &str, theirs: &[String], removed: &[String]) -> Result<()> {
    if lede(new_text).is_none() {
        return Err(Error::InvalidInput(Violation::NoLede.to_string()));
    }
    for s in theirs {
        if !new_text.contains(s.as_str()) {
            return Err(Error::InvalidInput(
                Violation::LostTheirs(s.clone()).to_string(),
            ));
        }
    }
    let after = sentences(new_text);
    for s in removed {
        if after.contains(s) {
            return Err(Error::InvalidInput(
                Violation::RestoredRemoved(s.clone()).to_string(),
            ));
        }
    }
    Ok(())
}

/// Every subject a piece of prose links to, as `(label, route, id)`.
///
/// Links are written as `[label](/person/person_ab12)`, so the shape is fixed
/// and a regex would be overkill.
///
/// The LABEL is carried because a link can be wrong while pointing at
/// something that exists — see [`label_fits`].
pub fn linked_refs(text: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for (i, _) in text.match_indices("](/") {
        let rest = &text[i + 3..];
        let Some(end) = rest.find(')') else { continue };
        let target = &rest[..end];
        let mut parts = target.splitn(2, '/');
        let (Some(kind), Some(id)) = (parts.next(), parts.next()) else { continue };
        if kind.is_empty() || id.is_empty() || id.contains('/') {
            continue;
        }
        // Walk back over `[label]` to the bracket that opened it. A label may
        // contain no `]`, which is what makes this findable at all.
        let label = text[..i]
            .rfind('[')
            .map(|open| text[open + 1..i].to_string())
            .unwrap_or_default();
        out.push((label, kind.to_string(), id.to_string()));
    }
    out
}

/// Every subject a piece of prose links to, as `(type, id)`.
pub fn linked_subjects(text: &str) -> Vec<(String, String)> {
    linked_refs(text)
        .into_iter()
        .map(|(_, kind, id)| (kind, id))
        .collect()
}

/// Is `label` a fair way to write a subject whose known names are `names`?
///
/// Exactly equal, or one is a subset of the other's words: "Soph" fits "Soph
/// Auciello", and "Nick" fits an alias of "Nick". What does NOT fit is the
/// observed failure — a name with no word in common with the thing it points
/// at, "Theo Kovac" over the id belonging to Anton Fenwick.
///
/// Deliberately generous. A false positive here silently deletes a correct
/// link (first-draft path) or refuses a legitimate edit (editor path), and a
/// shortened or affectionate name for someone is the normal way to write about
/// them. The failure being caught is not subtle and does not need a subtle
/// test.
pub fn label_fits(label: &str, names: &[String]) -> bool {
    fn words(s: &str) -> std::collections::HashSet<String> {
        s.split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(|w| w.to_lowercase())
            .collect()
    }
    let l = words(label);
    if l.is_empty() {
        // Nothing to disagree with. An empty label is a different problem.
        return true;
    }
    names.iter().any(|n| {
        let w = words(n);
        !w.is_empty() && (l.is_subset(&w) || w.is_subset(&l))
    })
}

/// Every name a subject answers to: its own, its nickname, its aliases.
///
/// `aliases` is jsonb on all three entity tables and `nickname` exists only on
/// people, which is why this is per-table rather than one query.
async fn known_names(
    pool: &sqlx::PgPool,
    table: &str,
    id: &str,
) -> Result<Vec<String>> {
    let row: Option<(String, Option<serde_json::Value>)> =
        sqlx::query_as(&format!("SELECT name, aliases FROM {table} WHERE id = $1"))
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to read {table} names: {e}")))?;
    let Some((name, aliases)) = row else {
        return Ok(Vec::new());
    };
    let mut names = vec![name];
    if let Some(serde_json::Value::Array(items)) = aliases {
        names.extend(items.into_iter().filter_map(|v| match v {
            serde_json::Value::String(s) => Some(s),
            _ => None,
        }));
    }
    if table == "wiki_people" {
        if let Ok(Some(n)) = sqlx::query_scalar::<_, Option<String>>(
            "SELECT nickname FROM wiki_people WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        {
            names.extend(n);
        }
    }
    Ok(names.into_iter().filter(|n| !n.trim().is_empty()).collect())
}

/// Every link in a piece of prose that does not point at a real subject, each
/// described in the words the writer needs to hear.
///
/// The constitution says never invent a link, and the very first article the
/// editor wrote invented one anyway: it linked the subject as
/// `/person/person_abc1` — the id from the EXAMPLE in its own prompt. Examples
/// leak into output, which is a property of models rather than a bug in this
/// one, so the rule needs a check behind it and not just a sentence.
///
/// Separate from the policy on purpose. An article is a stored artifact, so a
/// bad link there is REFUSED before anyone sees it (`check_links`). A chat
/// reply has already streamed past the person by the time anything could
/// object, so there the same finding is reported and not enforced.
pub async fn dead_links(pool: &sqlx::PgPool, text: &str) -> Result<Vec<String>> {
    let mut found = Vec::new();
    for (label, route, id) in linked_refs(text) {
        // Resolve through the ID, not the route segment. A route is not a
        // subject_type (`/org/…` is an organization), and keying on the
        // segment meant a hand-written match that named five kinds and let
        // every other link fall through unchecked — including chapters and
        // stories, which a chapter article is the likeliest thing to write.
        let Some(subject) = crate::api::subjects::by_id(&id) else {
            // Not a subject at all: an external URL, a page, a chat. Those are
            // not this check's business.
            continue;
        };

        // A subject with no route of its own cannot be linked, however real it
        // is. The client has no page to open, so the link renders as an anchor
        // that goes nowhere — which is the failure this function exists to
        // prevent, arrived at from the other direction.
        let Some(expected) = subject.route else {
            found.push(format!(
                "links to /{route}/{id}, and a {} has no page to open — \
                 name it in the prose instead of linking it",
                subject.kind
            ));
            continue;
        };
        if route != expected {
            found.push(format!(
                "links to /{route}/{id}, but {id} is a {} and lives at /{expected}/{id}",
                subject.kind
            ));
            continue;
        }

        let Some(table) = subject.table else { continue };
        let exists: bool = sqlx::query_scalar(&format!(
            "SELECT EXISTS (SELECT 1 FROM {table} WHERE id = $1)"
        ))
        .bind(&id)
        .fetch_one(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to check a link: {e}")))?;
        if !exists {
            found.push(format!(
                "links to /{route}/{id}, which does not exist — link only \
                 the exact ids you were given, and never an id from an example"
            ));
            continue;
        }

        // The id is real. Is it the one the SENTENCE is about?
        //
        // Existence was the whole check until 2026-09-22, and it passes the
        // failure that actually happens: an audit of 24 generated articles
        // found person links 0 correct and 3 mismatched, every mismatched one
        // pointing at an id that exists perfectly well and belongs to somebody
        // else. One article labelled its own subject with another person's id.
        //
        // Only the three entity kinds are checked. A day, a year or a chapter
        // is labelled with a rendering of what its id already says — "September
        // 2, 2026", "September 2" — and there is no name to disagree with.
        if matches!(subject.kind, "person" | "place" | "organization") {
            let names = known_names(pool, table, &id).await?;
            if !names.is_empty() && !label_fits(&label, &names) {
                found.push(format!(
                    "calls /{route}/{id} \"{label}\", but that id is {} — \
                     a link has to point at the one you named, or say the name \
                     without linking it",
                    names[0]
                ));
            }
        }
    }
    Ok(found)
}

/// Refuse prose that links to something that does not exist.
///
/// A dead link in a wiki is worse than no link: it looks like a page that
/// exists and is one click from proving the record wrong about itself.
pub async fn check_links(pool: &sqlx::PgPool, text: &str) -> Result<()> {
    match dead_links(pool, text).await?.into_iter().next() {
        Some(problem) => Err(Error::InvalidInput(format!("the article {problem}"))),
        None => Ok(()),
    }
}

/// The mechanical half of an edit summary: what changed, counted rather than
/// described.
///
/// The editor writes the "why" in its own words, and a caption can be wrong or
/// flattering about its own work ("improved the article" over a rewrite). This
/// line cannot be. Both go on the version, and the person reads them together.
pub fn change_line(before: &str, after: &str) -> String {
    let (sb, sa) = (sentences(before), sentences(after));
    let added = sa.iter().filter(|s| !sb.contains(s)).count();
    let cut = sb.iter().filter(|s| !sa.contains(s)).count();
    let para = |t: &str| t.split("\n\n").filter(|b| !b.trim().is_empty()).count() as i64;
    let dp = para(after) - para(before);
    let mut parts = Vec::new();
    if added > 0 {
        parts.push(format!("+{added} sentence{}", if added == 1 { "" } else { "s" }));
    }
    if cut > 0 {
        parts.push(format!("−{cut} sentence{}", if cut == 1 { "" } else { "s" }));
    }
    if dp != 0 {
        parts.push(format!("{dp:+} paragraph{}", if dp.abs() == 1 { "" } else { "s" }));
    }
    if parts.is_empty() {
        "no change".to_string()
    } else {
        parts.join(", ")
    }
}

/// An article the editor should look at on this run.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DueArticle {
    pub id: String,
    pub subject_type: String,
    pub subject_id: String,
    pub page_id: String,
    /// The article exactly as the editor last left it (§ provenance).
    pub machine_text: Option<String>,
    pub input_fingerprint: Option<String>,
}

/// At most this many articles per run. The ceiling is the cost control: there
/// is no per-day spend limit anywhere in the system, so the editor must carry
/// its own.
pub const MAX_PER_RUN: i64 = 3;

/// Articles whose rest is over, oldest edition first.
///
/// Three gates, and this is only the cheap two. **Eligible**: the person has
/// not switched maintenance off, and nobody has been editing it in the last
/// six hours — safe under the CRDT, still jarring to watch a paragraph change
/// under your cursor. **Ready**: the minimum interval has passed, or they
/// pressed update now.
///
/// The third gate — **drift**, whether the article's inputs actually moved —
/// is not here. It needs the fold input built, which costs a query per
/// article, so it is checked per candidate by the caller. An article whose
/// inputs have not changed is not rewritten; it just gets its fingerprint
/// stored and waits.
///
/// Note this selects from articles that EXIST. Whether an entity deserves one
/// in the first place is a separate question with a separate budget, because
/// "every entity on the box" is 226 articles nobody asked for on a five-month
/// record.
pub async fn due_articles(pool: &sqlx::PgPool, limit: i64) -> Result<Vec<DueArticle>> {
    sqlx::query_as::<_, DueArticle>(
        r#"
        SELECT id, subject_type, subject_id, page_id, machine_text, input_fingerprint
        FROM wiki_articles
        WHERE maintenance <> 'never'
          AND (last_human_edit_at IS NULL OR last_human_edit_at < now() - interval '6 hours')
          AND (
                update_requested_at IS NOT NULL
             OR last_written_at IS NULL
             OR last_written_at < now() - make_interval(days =>
                  CASE WHEN subject_type IN ('year', 'story') THEN 7 ELSE 30 END)
          )
        ORDER BY update_requested_at NULLS LAST, last_written_at NULLS FIRST
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to select due articles: {e}")))
}

/// A hash of what an article rests on.
///
/// The third gate. `due_articles` answers "has it rested long enough"; this
/// answers "did anything actually change". An article whose evidence is
/// identical to its last edition is not rewritten, which is what lets the
/// schedule be hourly without the cost being hourly.
///
/// Counts and latest timestamps rather than content: the same shape
/// `wiki_days.sources_fingerprint` has used to gate re-segmentation since it
/// shipped, and cheap enough to run on every candidate every hour. It moves
/// when a ref is added, a note is accepted, or the person edits an authored
/// field — which is exactly the set of things that should earn a new edition.
pub async fn evidence_fingerprint(
    pool: &sqlx::PgPool,
    article: &DueArticle,
) -> Result<String> {
    use sha2::Digest;

    // What the subject rests on, which differs by KIND. An entity rests on the
    // records that reference it; a year rests on the days inside it, and has no
    // refs of its own — hashing refs for a year would produce a constant, and
    // the year would never be revised no matter how much was written beneath
    // it.
    let (refs, last_ref): (i64, Option<chrono::DateTime<chrono::Utc>>) =
        if article.subject_type == "story" {
            // A story has no refs and usually no dates, so there is no cheap
            // signal that its material grew: finding out would mean running the
            // agent's own searches, which is the expensive thing the gate
            // exists to avoid. So a story is revised when the person asks or
            // when they accept a note — both explicit, both theirs. The whole
            // record moving underneath it is not, by itself, a reason.
            (0, None)
        } else if article.subject_type == "year" {
            let year: i32 = article
                .subject_id
                .strip_prefix("year_")
                .and_then(|y| y.parse().ok())
                .ok_or_else(|| {
                    Error::InvalidInput(format!("not a year id: {}", article.subject_id))
                })?;
            sqlx::query_as(
                "SELECT count(*) FILTER (WHERE narrated_at IS NOT NULL), max(narrated_at) \
                 FROM wiki_days WHERE EXTRACT(YEAR FROM date) = $1",
            )
            .bind(year)
            .fetch_one(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to count the year's days: {e}")))?
        } else if article.subject_type == "chapter" {
            // A chapter rests on the days inside the era the person drew, the
            // same shape as a year over a longer span. `ended_at` IS NULL is
            // the running chapter and means "through today" rather than "no
            // days" — the chapter most likely to be revised is exactly the one
            // that has not ended, so an inner join on a NULL end would freeze
            // the current era forever.
            sqlx::query_as(
                "SELECT count(*) FILTER (WHERE d.narrated_at IS NOT NULL), max(d.narrated_at) \
                 FROM wiki_days d \
                 JOIN wiki_chapters c ON c.id = $1 \
                 WHERE d.date >= c.started_at \
                   AND (c.ended_at IS NULL OR d.date < c.ended_at)",
            )
            .bind(&article.subject_id)
            .fetch_one(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to count the chapter's days: {e}")))?
        } else {
            sqlx::query_as("SELECT count(*), max(occurred_at) FROM wiki_refs WHERE entity_id = $1")
                .bind(&article.subject_id)
                .fetch_one(pool)
                .await
                .map_err(|e| Error::Database(format!("Failed to count refs: {e}")))?
        };

    let (notes, last_note): (i64, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT count(*), max(resolved_at) FROM wiki_notes \
         WHERE subject_type = $1 AND subject_id = $2 AND resolved_at IS NOT NULL",
    )
    .bind(&article.subject_type)
    .bind(&article.subject_id)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to count notes: {e}")))?;

    // What the person wrote about the subject moves the hash as well. A title
    // or summary they added is new evidence in the only sense that matters: the
    // article should be revisited because of it.
// Which kinds carry fields the person writes, and which table holds them, is
    // the registry's answer — this used to be a second subject-to-table map
    // forty lines below `check_links`'s, in the same file.
    //
    // A chapter's title, summary and changepoint are the spine of its article,
    // so moving one is the strongest reason there is to rewrite it. The same
    // goes for a year's name and a story's sentence.
    let subject = crate::api::subjects::by_kind(&article.subject_type);
    let authored: Option<chrono::DateTime<chrono::Utc>> = match subject {
        Some(s) if s.authored_fields => {
            let table = s.table.expect("a subject with authored fields has a table");
            sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(&format!(
                "SELECT updated_at FROM {table} WHERE id = $1"
            ))
            .bind(&article.subject_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to read the {}: {e}", s.kind)))?
            .flatten()
        }
        _ => None,
    };

    // MICROSECONDS, not seconds. A whole-second stamp makes any change landing
    // in the same second as the last fingerprint invisible — and invisible
    // forever, because the fingerprint is then stored as if that second had
    // been accounted for. The window is small and the consequence is not: the
    // edit never earns an edition. Postgres stores timestamptz to the
    // microsecond, so the hash reads what the column actually holds.
    let mut h = sha2::Sha256::new();
    h.update(
        format!(
            "refs:{refs}:{};notes:{notes}:{};authored:{};",
            last_ref.map(|t| t.timestamp_micros()).unwrap_or(0),
            last_note.map(|t| t.timestamp_micros()).unwrap_or(0),
            authored.map(|t| t.timestamp_micros()).unwrap_or(0),
        )
        .as_bytes(),
    );
    Ok(format!("{:x}", h.finalize()))
}

/// Record that the editor has looked at an article.
///
/// Called on EVERY outcome, including a refusal: an article whose edit failed
/// its checks must still store the fingerprint it was refused for, or it comes
/// back next hour, fails the same way, and burns a model call an hour until
/// its interval elapses.
pub async fn record_pass(
    pool: &sqlx::PgPool,
    article_id: &str,
    fingerprint: &str,
    wrote: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "UPDATE wiki_articles \
         SET input_fingerprint = $2, \
             update_requested_at = NULL, \
             machine_text = COALESCE($3, machine_text), \
             last_written_at = CASE WHEN $3 IS NULL THEN last_written_at ELSE now() END, \
             updated_at = now() \
         WHERE id = $1",
    )
    .bind(article_id)
    .bind(fingerprint)
    .bind(wrote)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to record editor pass: {e}")))?;
    Ok(())
}

/// Record that a new edition exists: what the editor wrote, and when.
///
/// Deliberately does NOT touch `input_fingerprint`. The fingerprint is stored
/// by the scheduler before the agent runs, so that a revision which fails does
/// not come back next hour to fail the same way; writing it again here would
/// be harmless, but writing an empty one — which is all this function knows —
/// would erase the very thing that stops the loop.
pub async fn record_edition(
    pool: &sqlx::PgPool,
    article_id: &str,
    machine_text: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE wiki_articles \
         SET machine_text = $2, last_written_at = now(), update_requested_at = NULL, \
             updated_at = now() \
         WHERE id = $1",
    )
    .bind(article_id)
    .bind(machine_text)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to record edition: {e}")))?;
    Ok(())
}

/// Store what the person has done to an article since the editor last wrote.
pub async fn record_provenance(
    pool: &sqlx::PgPool,
    article_id: &str,
    theirs: &[String],
    removed: &[String],
) -> Result<()> {
    sqlx::query("UPDATE wiki_articles SET theirs = $2, removed = $3 WHERE id = $1")
        .bind(article_id)
        .bind(serde_json::json!(theirs))
        .bind(serde_json::json!(removed))
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to record provenance: {e}")))?;
    Ok(())
}

/// Put a named version's text back, as a NEW version.
///
/// History is never rewound. Reverting to v12 leaves v12 where it is and adds
/// v18 saying the same thing, so the act of reverting is itself in the record
/// and can be undone in turn.
///
/// This deliberately does NOT run `check_edit`. That invariant protects the
/// person from the machine; a revert is the person, and they are allowed to
/// drop a sentence of their own if that is what going back means.
///
/// It also deliberately leaves `machine_text` alone, which looks like an
/// omission and is the whole trick. `machine_text` means "what the editor last
/// wrote", and the editor did not write this. Left as it is, the next pass
/// diffs the reverted text against it and reaches the right conclusions on its
/// own: sentences the revert brought back that the editor never wrote are
/// theirs, and sentences the editor wrote that the revert removed are not to
/// be restored. Setting it to the reverted text would tell the editor it had
/// authored every word of it, and it would feel free to rewrite them all.
pub async fn revert_article(
    pool: &sqlx::PgPool,
    yjs: &crate::server::yjs::YjsState,
    subject_type: &str,
    subject_id: &str,
    version_number: i64,
) -> Result<String> {
    let article = crate::api::wiki_articles::get_article(pool, subject_type, subject_id)
        .await?
        .ok_or_else(|| Error::NotFound(format!("No article for {subject_type} {subject_id}")))?;

    let snapshot: Vec<u8> = sqlx::query_scalar(
        "SELECT yjs_snapshot FROM app_page_versions \
         WHERE page_id = $1 AND version_number = $2",
    )
    .bind(&article.page_id)
    .bind(version_number)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to load that version: {e}")))?
    .ok_or_else(|| Error::NotFound(format!("No version {version_number} of this article")))?;

    let text = crate::server::yjs::extract_text_content(&snapshot);
    if text.trim().is_empty() {
        return Err(Error::InvalidInput(
            "that version has no readable text to restore".into(),
        ));
    }

    let live = yjs
        .read_text(&article.page_id)
        .await
        .map_err(|e| Error::Other(format!("could not read the article: {e}")))?;
    if live == text {
        return Ok("that version is already what the page says".into());
    }

    let applied = yjs
        .apply_text_diff(&article.page_id, &live, &text)
        .await
        .map_err(Error::Other)?;

    let change = change_line(&live, &applied);
    if let Ok(state) = yjs.encoded_state(&article.page_id).await {
        let _ = crate::api::pages::create_version_from_snapshot(
            pool,
            &article.page_id,
            &state,
            &applied,
            "user",
            Some(&format!("reverted to v{version_number} — {change}")),
        )
        .await;
    }
    Ok(change)
}

/// The maintenance state of one article, as the editor needs it.
#[derive(Debug, Clone, sqlx::FromRow)]
struct EditorArticle {
    id: String,
    page_id: String,
    maintenance: String,
    machine_text: Option<String>,
    theirs: serde_json::Value,
    removed: serde_json::Value,
}

impl EditorArticle {
    fn provenance_sets(&self) -> (Vec<String>, Vec<String>) {
        let as_vec = |v: &serde_json::Value| -> Vec<String> {
            v.as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };
        (as_vec(&self.theirs), as_vec(&self.removed))
    }
}

/// Apply the editor's revision of one article.
///
/// This is the whole write path, and it runs on the server rather than in the
/// agent because every step after "here is the new text" is a guarantee the
/// agent must not be able to skip:
///
/// 1. Read the live document. If the person has edited it since the editor
///    last wrote, cut a `user` version first — otherwise their change would
///    sit invisibly inside the machine's next version — and fold what they did
///    into `theirs` / `removed`.
/// 2. Check the proposed text against those sets. A refusal returns the reason
///    rather than raising: the caller is an agent loop, and the reason is
///    something it can act on in the same turn.
/// 3. Apply only what changed, through the CRDT, with the staleness guard.
/// 4. Cut the machine's version, carrying the editor's own summary and a
///    mechanical count that cannot flatter itself.
///
/// Returns the line the agent should report.
pub async fn revise_article(
    pool: &sqlx::PgPool,
    yjs: &crate::server::yjs::YjsState,
    subject_type: &str,
    subject_id: &str,
    new_text: &str,
    summary: &str,
) -> Result<String> {
    // The editor's own columns, read directly: `wiki_articles::get_article`
    // is a compile-time-checked macro over a shared struct, and widening that
    // for one caller would put maintenance state on every reader of articles.
    let article: EditorArticle = sqlx::query_as(
        "SELECT id, page_id, maintenance, machine_text, theirs, removed \
         FROM wiki_articles WHERE subject_type = $1 AND subject_id = $2",
    )
    .bind(subject_type)
    .bind(subject_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to load article: {e}")))?
    .ok_or_else(|| Error::NotFound(format!("No article for {subject_type} {subject_id}")))?;

    if article.maintenance == "never" {
        return Err(Error::InvalidInput(
            "the owner has turned maintenance off for this article".into(),
        ));
    }
    let (theirs_stored, removed_stored) = article.provenance_sets();

    let live = yjs
        .read_text(&article.page_id)
        .await
        .map_err(|e| Error::Other(format!("could not read the article: {e}")))?;

    // ── 1. What the person has done since the editor last wrote ──
    //
    // No `machine_text` means the editor has never recorded an edition of this
    // article, so the difference between it and the live text is UNKNOWN — not
    // "all of it is theirs". Inferring provenance from an empty string would
    // mark every sentence, including the machine's own first draft, as the
    // person's, and the invariant would then forbid the editor from ever
    // touching the article again. Absent is absent.
    let (theirs, removed) = match article.machine_text.as_deref() {
        Some(machine_text) => {
            let (added, removed_now) = provenance(machine_text, &live);
            fold_provenance(&theirs_stored, &removed_stored, &added, &removed_now, &live)
        }
        None => (theirs_stored.clone(), removed_stored.clone()),
    };
    let machine_text = article.machine_text.clone().unwrap_or_default();
    if !machine_text.is_empty() && live != machine_text {
        // Their edit becomes its own version, before the machine's, so the
        // history reads in the order the writing happened.
        if let Ok(state) = yjs.encoded_state(&article.page_id).await {
            let _ = crate::api::pages::create_version_from_snapshot(
                pool, &article.page_id, &state, &live, "user", Some("edited"),
            )
            .await;
        }
        record_provenance(pool, &article.id, &theirs, &removed).await?;
    }

    // ── 2. The invariants, before anything reaches the document ──
    check_edit(new_text, &theirs, &removed)?;
    check_links(pool, new_text).await?;

    // ── 3. Only what changed ──
    let applied = yjs
        .apply_text_diff(&article.page_id, &live, new_text)
        .await
        .map_err(Error::Other)?;

    // ── 4. The machine's version, with a summary that cannot flatter itself ──
    let change = change_line(&live, &applied);
    let description = format!("{} — {}", summary.trim(), change);
    if let Ok(state) = yjs.encoded_state(&article.page_id).await {
        let _ = crate::api::pages::create_version_from_snapshot(
            pool,
            &article.page_id,
            &state,
            &applied,
            "ai",
            Some(&description),
        )
        .await;
    }
    record_provenance(pool, &article.id, &theirs, &removed).await?;
    record_edition(pool, &article.id, &applied).await?;
    Ok(change)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_prompt_is_constitution_then_brief_then_rules() {
        let p = system_prompt("year", &["Never mention my father unless I do.".into()]).unwrap();
        let c = p.find("observe, never infer").expect("constitution present");
        let b = p.find("ONE YEAR of this person's life").expect("year brief");
        let r = p.find("Never mention my father").expect("rules");
        assert!(c < b && b < r, "constitution, then brief, then rules");
        assert!(
            p.contains("They are absolute"),
            "a rule must not read as context the model may weigh"
        );
    }

    #[test]
    fn a_subject_without_a_brief_is_refused_rather_than_written_blind() {
        // The day is the case that matters: its narrator is released and tuned,
        // and must not be silently handed the generic door.
        assert!(brief_for("day").is_none());
        assert!(system_prompt("day", &[]).is_err());

        // The life page is the harder one, because refusing it is a promise
        // rather than a scheduling detail: it is written by the person, in the
        // first person, and the editor may not touch it. Structural, so that
        // it is not a rule in a prompt that a model can weigh against another
        // rule.
        assert!(brief_for("narrative_identity").is_none());
        assert!(system_prompt("narrative_identity", &[]).is_err());
    }

    #[test]
    fn every_other_subject_the_schema_allows_has_a_brief() {
        // The vocabulary is `wiki_articles_subject_type_check` (migration
        // 0022). A kind the schema allows, the UI gives a room, and the editor
        // has no brief for is an article nothing will ever write — which is
        // how chapters sat with a seeded page and no second sentence.
        for kind in ["year", "story", "chapter", "person", "place", "organization"] {
            assert!(brief_for(kind).is_some(), "{kind} has no brief");
            assert!(system_prompt(kind, &[]).is_ok(), "{kind} has no prompt");
        }
        let p = system_prompt("chapter", &[]).unwrap();
        assert!(p.contains("an era of this person's life"), "the chapter brief");
        assert!(
            p.contains("Do not name it for them"),
            "an unnamed chapter is an answer, and the brief has to say so — a \
             model handed a titleless era will supply a title"
        );
    }

    /// A chapter that has not ended must still notice its days.
    ///
    /// `ended_at IS NULL` is the running era — the one chapter most likely to
    /// be revised, because it is the one still accumulating. An inner join on
    /// a NULL end matches nothing, which would freeze the current chapter's
    /// article permanently while every finished chapter kept updating: a bug
    /// that looks like "it works" right up until the only page anyone is
    /// watching is the one that never changes.
    #[sqlx::test]
    async fn a_running_chapter_still_sees_the_days_inside_it(pool: sqlx::PgPool) {
        sqlx::query(
            "INSERT INTO wiki_chapters (id, title, started_at, ended_at) \
             VALUES ('chap_now', 'Out on my own', '2026-01-01', NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let article = crate::api::wiki_articles::create_article(
            &pool,
            "chapter",
            "chap_now",
            "Out on my own",
            "Theirs.",
        )
        .await
        .unwrap();
        let due = DueArticle {
            id: article.id.clone(),
            subject_type: "chapter".into(),
            subject_id: "chap_now".into(),
            page_id: article.page_id.clone(),
            machine_text: None,
            input_fingerprint: None,
        };

        let empty = evidence_fingerprint(&pool, &due).await.unwrap();

        sqlx::query(
            "INSERT INTO wiki_days (id, date, narrated_at) \
             VALUES ('day_2026-02-02', '2026-02-02', now())",
        )
        .execute(&pool)
        .await
        .unwrap();
        let with_a_day = evidence_fingerprint(&pool, &due).await.unwrap();
        assert_ne!(
            empty, with_a_day,
            "a day written up inside a running chapter is exactly the evidence \
             that should earn the era a new edition"
        );

        // And a day outside the era is not the era's evidence.
        sqlx::query(
            "INSERT INTO wiki_days (id, date, narrated_at) \
             VALUES ('day_2020-05-05', '2020-05-05', now())",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            with_a_day,
            evidence_fingerprint(&pool, &due).await.unwrap(),
            "a day before the chapter started is another chapter's evidence"
        );

        // Their own three fields are the spine of the page, so moving one is
        // the strongest reason there is to rewrite it.
        sqlx::query("UPDATE wiki_chapters SET changepoint = 'the lease ran out' WHERE id = 'chap_now'")
            .execute(&pool)
            .await
            .unwrap();
        assert_ne!(
            with_a_day,
            evidence_fingerprint(&pool, &due).await.unwrap(),
            "what they say ended an era says more about it than its name does"
        );
    }

    #[test]
    fn the_lede_skips_a_heading_the_person_added_above_it() {
        assert_eq!(lede("The day began early.\n\nThen more."), Some("The day began early."));
        assert_eq!(
            lede("# My own title\n\nThe day began early."),
            Some("The day began early."),
            "a heading a human typed above the lede must not become the summary"
        );
        assert_eq!(lede("   \n\n  "), None);
    }

    #[test]
    fn sentences_do_not_split_on_abbreviations_or_decimals() {
        let s = sentences("Met Dr. Vance at 3.30 p.m. The train was quiet.");
        assert_eq!(
            s,
            vec!["Met Dr. Vance at 3.30 p.m. The train was quiet."],
            "splitting too little is safe; splitting wrongly fails the invariant on \
             text nobody touched"
        );
        let two = sentences("The train was quiet. You walked from the station.");
        assert_eq!(two.len(), 2);
    }

    #[test]
    fn provenance_is_the_difference_from_what_the_editor_last_wrote() {
        let machine = "You went to the coast. The water was cold.";
        let live = "You went to the coast. I proposed there.";
        let (added, removed) = provenance(machine, live);
        assert_eq!(added, vec!["I proposed there."]);
        assert_eq!(removed, vec!["The water was cold."]);
    }

    #[test]
    fn folding_drops_a_sentence_they_later_deleted_and_a_removal_they_typed_back() {
        // Their own sentence, later deleted by them: stops being theirs, and
        // becomes something the editor must not restore.
        let (t, r) = fold_provenance(
            &["I proposed there.".into()],
            &[],
            &[],
            &["I proposed there.".into()],
            "You went to the coast.",
        );
        assert!(t.is_empty());
        assert_eq!(r, vec!["I proposed there."]);

        // A removal they typed back in themselves is no longer a removal.
        let (t2, r2) = fold_provenance(
            &[],
            &["The water was cold.".into()],
            &[],
            &[],
            "You went to the coast. The water was cold.",
        );
        assert!(r2.is_empty());
        assert!(t2.is_empty());
    }

    #[test]
    fn an_edit_that_loses_their_sentence_is_refused() {
        let theirs = vec!["I proposed there.".to_string()];
        assert!(check_edit("You went to the coast. I proposed there.", &theirs, &[]).is_ok());

        let err = check_edit("You went to the coast.", &theirs, &[])
            .unwrap_err()
            .to_string();
        assert!(err.contains("did not survive"), "{err}");

        // Paraphrase is loss. This is the whole point: the constitution asks,
        // and this refuses.
        let err = check_edit("You went to the coast. You proposed there.", &theirs, &[])
            .unwrap_err()
            .to_string();
        assert!(err.contains("did not survive"), "{err}");
    }

    #[test]
    fn an_edit_that_restores_what_they_deleted_is_refused() {
        let removed = vec!["The water was cold.".to_string()];
        let err = check_edit("You went to the coast. The water was cold.", &[], &removed)
            .unwrap_err()
            .to_string();
        assert!(err.contains("was restored"), "{err}");
        assert!(check_edit("You went to the coast.", &[], &removed).is_ok());
    }

    /// Seed an article with a page, and set the maintenance columns directly.
    #[cfg(test)]
    async fn seed(
        pool: &sqlx::PgPool,
        subject_id: &str,
        maintenance: &str,
        written_days_ago: Option<i32>,
    ) -> String {
        let a = crate::api::wiki_articles::create_article(
            pool, "person", subject_id, subject_id, "A first draft.",
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE wiki_articles SET maintenance = $2, \
             last_written_at = CASE WHEN $3::int IS NULL THEN NULL \
                               ELSE now() - make_interval(days => $3::int) END \
             WHERE id = $1",
        )
        .bind(&a.id)
        .bind(maintenance)
        .bind(written_days_ago)
        .execute(pool)
        .await
        .unwrap();
        a.id
    }

    /// A person, an article, and the live document layer — everything
    /// `revise_article` touches except the model, which is exactly the half
    /// worth testing: the model's output is an argument here, so every
    /// guarantee after it can be proven without spending a call.
    #[cfg(test)]
    async fn article_fixture(
        pool: &sqlx::PgPool,
        first_draft: &str,
    ) -> (crate::server::yjs::YjsState, String, String) {
        sqlx::query("INSERT INTO wiki_people (id, name) VALUES ('person_z', 'Zoe')")
            .execute(pool)
            .await
            .unwrap();
        let a = crate::api::wiki_articles::create_article(
            pool, "person", "person_z", "Zoe", first_draft,
        )
        .await
        .unwrap();
        // The editor knows what it last wrote; without that there is no
        // provenance, because provenance IS the difference from it.
        record_pass(pool, &a.id, "fp-0", Some(first_draft)).await.unwrap();
        (
            crate::server::yjs::YjsState::new(pool.clone()),
            a.id,
            a.page_id,
        )
    }

    #[sqlx::test]
    async fn a_revision_lands_as_a_small_diff_with_a_summary_that_counts(pool: sqlx::PgPool) {
        let (yjs, _id, page_id) =
            article_fixture(&pool, "You met Zoe at the shop. She fixes bicycles.").await;

        let change = revise_article(
            &pool,
            &yjs,
            "person",
            "person_z",
            "You met Zoe at the shop. She fixes bicycles. You now ride together on Sundays.",
            "added the Sunday rides",
        )
        .await
        .unwrap();
        assert!(change.contains("+1 sentence"), "{change}");

        let live = yjs.read_text(&page_id).await.unwrap();
        assert!(live.contains("ride together on Sundays"));
        assert!(
            live.starts_with("You met Zoe at the shop."),
            "what was already true is left exactly as it was"
        );

        let (by, desc): (String, Option<String>) = sqlx::query_as(
            "SELECT created_by, description FROM app_page_versions \
             WHERE page_id = $1 ORDER BY version_number DESC LIMIT 1",
        )
        .bind(&page_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(by, "ai");
        let desc = desc.unwrap();
        assert!(desc.contains("added the Sunday rides"), "the editor's why: {desc}");
        assert!(
            desc.contains("+1 sentence"),
            "and a count it cannot flatter itself with: {desc}"
        );
    }

    #[sqlx::test]
    async fn the_owners_sentence_survives_a_revision_that_tried_to_reword_it(
        pool: sqlx::PgPool,
    ) {
        let (yjs, id, page_id) = article_fixture(&pool, "You met Zoe at the shop.").await;

        // The person adds a line of their own, the way they actually would.
        yjs.apply_text_diff(
            &page_id,
            "You met Zoe at the shop.",
            "You met Zoe at the shop. She taught me to ride again.",
        )
        .await
        .unwrap();

        // The editor's next pass tries to smooth their sentence into its own
        // voice. This is the whole reason the invariant exists.
        let refused = revise_article(
            &pool,
            &yjs,
            "person",
            "person_z",
            "You met Zoe at the shop. She taught you to ride again, after some years away.",
            "tidied",
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(refused.contains("did not survive"), "{refused}");

        let live = yjs.read_text(&page_id).await.unwrap();
        assert!(
            live.contains("She taught me to ride again."),
            "a refused edit must leave the document exactly as it was"
        );

        // Their words were recorded on the way through, so the next attempt is
        // told what it may not touch.
        let theirs: serde_json::Value =
            sqlx::query_scalar("SELECT theirs FROM wiki_articles WHERE id = $1")
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(theirs[0], "She taught me to ride again.");

        // And a version carrying THEIR edit was cut before the machine's, so
        // the history reads in the order the writing happened.
        let authors: Vec<String> = sqlx::query_scalar(
            "SELECT created_by FROM app_page_versions WHERE page_id = $1 ORDER BY version_number",
        )
        .bind(&page_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(authors.contains(&"user".to_string()), "{authors:?}");
    }

    #[test]
    fn links_are_read_out_of_prose_by_shape() {
        let found = linked_subjects(
            "You met [Nick](/person/person_nick01) on [3 March](/day/day_2026-03-03), \
             and see [the site](https://example.com).",
        );
        assert_eq!(
            found,
            vec![
                ("person".into(), "person_nick01".into()),
                ("day".into(), "day_2026-03-03".into())
            ],
            "an external URL is not a subject link"
        );
    }

    #[test]
    fn labels_are_read_back_with_their_links() {
        let found = linked_refs(
            "You met [Nick](/person/person_nick01) on [3 March](/day/day_2026-03-03), \
             and see [the site](https://example.com).",
        );
        assert_eq!(
            found,
            vec![
                ("Nick".into(), "person".into(), "person_nick01".into()),
                ("3 March".into(), "day".into(), "day_2026-03-03".into())
            ]
        );
    }

    #[test]
    fn a_shortened_or_aliased_name_fits_and_a_stranger_does_not() {
        let names = vec!["Soph Auciello".to_string(), "Soph".to_string()];
        assert!(label_fits("Soph Auciello", &names));
        assert!(label_fits("soph", &names), "case does not matter");
        assert!(label_fits("Soph", &names), "a first name is how people write");
        assert!(
            !label_fits("David Okafor", &names),
            "no word in common is the failure being caught"
        );
        assert!(label_fits("", &names), "an empty label is a different problem");
    }

    /// The failure an existence check cannot see: the id is real, and it is
    /// somebody else's. Reproduced from an audit of 24 generated articles in
    /// which person links were 0 correct and 3 mismatched.
    #[sqlx::test]
    async fn a_real_id_under_the_wrong_name_is_reported(pool: sqlx::PgPool) {
        sqlx::query(
            "INSERT INTO wiki_people (id, name, nickname) \
             VALUES ('person_a', 'Nick', NULL), ('person_b', 'David Okafor', 'Dave')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let problems = dead_links(&pool, "You met [Nick](/person/person_b) for lunch.")
            .await
            .unwrap();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("David Okafor"), "{}", problems[0]);

        // And the honest versions of the same sentence, including the nickname
        // and a first name, are left alone.
        for text in [
            "You met [David Okafor](/person/person_b) for lunch.",
            "You met [Dave](/person/person_b) for lunch.",
            "You met [David](/person/person_b) for lunch.",
            "You met [Nick](/person/person_a) for lunch.",
        ] {
            let clean = dead_links(&pool, text).await.unwrap();
            assert!(clean.is_empty(), "refused an honest link: {text} -> {clean:?}");
        }
    }

    /// A day is labelled with a rendering of the date its id already carries,
    /// so there is no name to disagree with and the check must not invent one.
    #[sqlx::test]
    async fn a_day_link_is_not_held_to_a_name(pool: sqlx::PgPool) {
        sqlx::query("INSERT INTO wiki_days (id, date) VALUES ('day_2026-03-03', '2026-03-03')")
            .execute(&pool)
            .await
            .unwrap();
        for label in ["March 3, 2026", "March 3", "that Tuesday"] {
            let clean = dead_links(&pool, &format!("on [{label}](/day/day_2026-03-03)"))
                .await
                .unwrap();
            assert!(clean.is_empty(), "{label} -> {clean:?}");
        }
    }

    /// The finding is separate from the policy: the wiki refuses on the first
    /// bad link, chat reports every one of them and rewrites nothing.
    #[sqlx::test]
    async fn every_bad_link_is_reported_not_just_the_first(pool: sqlx::PgPool) {
        sqlx::query("INSERT INTO wiki_people (id, name) VALUES ('person_real', 'Nick')")
            .execute(&pool)
            .await
            .unwrap();

        let problems = dead_links(
            &pool,
            "I read it in [a message](/person/person_abc1) and again from \
             [Nick](/person/person_real), then [here](/person/person_xyz9).",
        )
        .await
        .unwrap();

        assert_eq!(problems.len(), 2, "the real one is not a problem: {problems:?}");
        assert!(problems[0].contains("person_abc1"));
        assert!(problems[1].contains("person_xyz9"));

        let clean = dead_links(&pool, "Only [Nick](/person/person_real) here.")
            .await
            .unwrap();
        assert!(clean.is_empty());
    }

    #[sqlx::test]
    async fn a_link_to_a_subject_that_does_not_exist_is_refused(pool: sqlx::PgPool) {
        sqlx::query("INSERT INTO wiki_people (id, name) VALUES ('person_real', 'Nick')")
            .execute(&pool)
            .await
            .unwrap();
        check_links(&pool, "You met [Nick](/person/person_real).")
            .await
            .unwrap();

        // The id from the example in the editor's own prompt. The first
        // article ever written by this editor linked exactly this.
        let err = check_links(&pool, "You met [Nick](/person/person_abc1).")
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("does not exist"), "{err}");
        assert!(err.contains("never an id from an example"), "{err}");
    }

    #[sqlx::test]
    async fn reverting_adds_a_version_rather_than_rewinding_history(pool: sqlx::PgPool) {
        let (yjs, id, page_id) = article_fixture(&pool, "You met Zoe at the shop.").await;
        // Two editions, so there is an earlier one to go back to. A version's
        // snapshot is the state AFTER the edit it records, so v1 is the first
        // revision's result — not the empty page before it.
        revise_article(
            &pool,
            &yjs,
            "person",
            "person_z",
            "You met Zoe at the shop. She fixes bicycles.",
            "added the shop",
        )
        .await
        .unwrap();
        let v1: i64 = sqlx::query_scalar(
            "SELECT max(version_number) FROM app_page_versions WHERE page_id = $1",
        )
        .bind(&page_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        revise_article(
            &pool,
            &yjs,
            "person",
            "person_z",
            "You met Zoe at the shop. She fixes bicycles. You ride together on Sundays.",
            "added the Sunday rides",
        )
        .await
        .unwrap();

        revert_article(&pool, &yjs, "person", "person_z", v1)
            .await
            .unwrap();

        let live = yjs.read_text(&page_id).await.unwrap();
        assert!(!live.contains("Sundays"), "the later edit is undone: {live}");
        assert!(live.contains("fixes bicycles"), "and the earlier one is kept");

        let rows: Vec<(i64, String, Option<String>)> = sqlx::query_as(
            "SELECT version_number, created_by, description FROM app_page_versions \
             WHERE page_id = $1 ORDER BY version_number",
        )
        .bind(&page_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(
            rows.iter().any(|(n, _, _)| *n == v1),
            "the version reverted TO is still there — history is never rewound"
        );
        let last = rows.last().unwrap();
        assert_eq!(last.1, "user", "a revert is the person's act, not the machine's");
        assert!(last.2.as_deref().unwrap().contains(&format!("reverted to v{v1}")));

        // The editor's idea of what IT last wrote is deliberately untouched, so
        // the next pass reads the revert correctly instead of claiming it.
        let machine: Option<String> =
            sqlx::query_scalar("SELECT machine_text FROM wiki_articles WHERE id = $1")
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(
            machine.unwrap().contains("Sundays"),
            "machine_text still says what the EDITOR last wrote, so its next pass \
             reads the revert as the person's doing instead of claiming it"
        );
    }

    #[sqlx::test]
    async fn an_article_the_owner_switched_off_is_not_revised(pool: sqlx::PgPool) {
        let (yjs, id, _page) = article_fixture(&pool, "You met Zoe at the shop.").await;
        sqlx::query("UPDATE wiki_articles SET maintenance = 'never' WHERE id = $1")
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();
        let err = revise_article(&pool, &yjs, "person", "person_z", "Anything.", "why")
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("maintenance off"), "{err}");
    }

    #[sqlx::test]
    async fn due_skips_what_is_switched_off_and_what_is_still_resting(pool: sqlx::PgPool) {
        for (id, name) in [("person_a", "Ada"), ("person_b", "Bo"), ("person_c", "Cy")] {
            sqlx::query("INSERT INTO wiki_people (id, name) VALUES ($1, $2)")
                .bind(id)
                .bind(name)
                .execute(&pool)
                .await
                .unwrap();
        }
        let off = seed(&pool, "person_a", "never", Some(400)).await;
        let resting = seed(&pool, "person_b", "auto", Some(3)).await;
        let due = seed(&pool, "person_c", "auto", Some(90)).await;

        let got: Vec<String> = due_articles(&pool, 10)
            .await
            .unwrap()
            .into_iter()
            .map(|a| a.id)
            .collect();
        assert!(!got.contains(&off), "maintenance 'never' means never");
        assert!(!got.contains(&resting), "inside its interval");
        assert!(got.contains(&due));
    }

    #[sqlx::test]
    async fn update_now_beats_the_interval_and_a_recent_human_edit_defers(pool: sqlx::PgPool) {
        sqlx::query("INSERT INTO wiki_people (id, name) VALUES ('person_d', 'Di')")
            .execute(&pool)
            .await
            .unwrap();
        let id = seed(&pool, "person_d", "auto", Some(1)).await;
        assert!(due_articles(&pool, 10).await.unwrap().is_empty());

        sqlx::query("UPDATE wiki_articles SET update_requested_at = now() WHERE id = $1")
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(due_articles(&pool, 10).await.unwrap().len(), 1, "asked for");

        // Somebody is in the page right now. Even "update now" waits: the CRDT
        // makes it safe, not unsurprising.
        sqlx::query("UPDATE wiki_articles SET last_human_edit_at = now() WHERE id = $1")
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(due_articles(&pool, 10).await.unwrap().is_empty());
    }

    #[sqlx::test]
    async fn a_refused_pass_still_stores_its_fingerprint(pool: sqlx::PgPool) {
        sqlx::query("INSERT INTO wiki_people (id, name) VALUES ('person_e', 'Eli')")
            .execute(&pool)
            .await
            .unwrap();
        let id = seed(&pool, "person_e", "auto", Some(90)).await;

        let before: Option<chrono::DateTime<chrono::Utc>> =
            sqlx::query_scalar("SELECT last_written_at FROM wiki_articles WHERE id = $1")
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap();

        // A pass that wrote nothing: the article must not come back next hour
        // to fail the same way and burn a model call an hour.
        record_pass(&pool, &id, "fp-1", None).await.unwrap();
        let (fp, written): (Option<String>, Option<chrono::DateTime<chrono::Utc>>) =
            sqlx::query_as("SELECT input_fingerprint, last_written_at FROM wiki_articles WHERE id = $1")
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(fp.as_deref(), Some("fp-1"), "the refusal is remembered");
        assert_eq!(
            written, before,
            "nothing was written, so the edition date must not move — \
             otherwise a refusal reads as an edit and resets the interval"
        );

        record_pass(&pool, &id, "fp-2", Some("The new text.")).await.unwrap();
        let (mt, written): (Option<String>, Option<chrono::DateTime<chrono::Utc>>) =
            sqlx::query_as("SELECT machine_text, last_written_at FROM wiki_articles WHERE id = $1")
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(mt.as_deref(), Some("The new text."));
        assert!(written.is_some());
    }

    #[test]
    fn the_change_line_counts_rather_than_describes() {
        assert_eq!(
            change_line("A thing happened.", "A thing happened.\n\nAnother thing did."),
            "+1 sentence, +1 paragraph"
        );
        assert_eq!(change_line("A thing happened.", "A thing happened."), "no change");
    }
    /// The two spellings of the lede rule must return the same block.
    ///
    /// There is no way to share one implementation across Rust and SQL — the
    /// SQL exists precisely so a list query need not fetch whole articles —
    /// so what keeps them honest is this: every case that has ever been
    /// ambiguous, run through both. The TypeScript copy is the third, and its
    /// divergence is what prompted the test: the chronicle's hand-written
    /// version returned `# A heading` as the lede.
    #[sqlx::test]
    async fn lede_and_lede_sql_agree(pool: sqlx::PgPool) {
        let cases = [
            "Plain opening paragraph.\n\n## A thread\n\nMore.",
            "# A title above it\n\nThe real lede.\n\n## Thread",
            "## Straight into a heading\n\nThen prose.",
            "\n\n   \n\nLeading blank blocks.\n\nSecond.",
            "Only one paragraph.",
            "#not a heading, no space\n\nSecond block.",
            "",
            "   ",
            "###### Deep heading\n\nProse under it.",
            "Trailing whitespace on the block.   \n\n## Next",
        ];

        for case in cases {
            let from_sql: Option<String> =
                sqlx::query_scalar(&format!("SELECT {}", lede_sql("$1")))
                    .bind(case)
                    .fetch_one(&pool)
                    .await
                    .expect("the lede SQL runs");
            let from_rust = crate::api::wiki_editor::lede(case).map(str::to_string);
            assert_eq!(
                from_sql, from_rust,
                "the two spellings of the lede rule disagreed on {case:?}"
            );
        }
    }

}

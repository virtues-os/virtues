//! Moving a markdown page to blocks, and the comparisons that keep an
//! article its owner's once it holds blocks.
//!
//! Slice 6 of `agents/plan/document-contract-plan.md`. User pages move in a
//! run (`api::page_convert`); an article moves when the server first builds
//! its live document (`DocCache::get_or_create`), or in the run once opened.
//! Both read the page through [`convert`], the one way markdown becomes a
//! page's blocks for a move:
//!
//! - **An article is read as an article** (`Dialect::Article`): its
//!   footnotes, figures and veils become the contract's own nodes, and an
//!   article whose blocks its readers would read otherwise stays markdown
//!   (`ArticleCounts::holds`).
//! - **Its edition moves with it** ([`rebase`]): `machine_text`, what the
//!   writers last wrote, becomes what this server's export makes of it, and
//!   each sentence of the owner's (`theirs`) becomes the export's spelling of
//!   it, in the flip's own transaction ([`record_rebase`]). A sentence of
//!   theirs the export cannot place keeps the article in markdown.
//! - **Every later comparison re-projects both sides** ([`as_compared`]): an
//!   article's text and its edition are compared as this server would write
//!   them, so a release that writes an escape differently does not make
//!   every article read as its owner's.
//! - **A note beside a sentence keeps its place** ([`anchors_lost`]): an
//!   article whose move would leave an anchored note's words behind stays
//!   markdown.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use virtues_document::migrate::{ArticleCounts, Dialect, TextLoss};
use virtues_document::{Node, Problem};

use crate::api::pages::{self, PageFormat};
use crate::api::wiki_editor::{sentence_key, sentences};
use crate::error::Result;
use crate::server::yjs::Written;

/// `VIRTUES_TREE_ARTICLES=1`, with block pages on (`VIRTUES_TREE_PAGES=1`),
/// read once: an article nobody has opened becomes blocks when the server
/// first builds its document. Gates the move only: an article that holds
/// blocks keeps opening, editing and saving with it off.
pub fn tree_articles_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        pages::tree_pages_enabled()
            && std::env::var("VIRTUES_TREE_ARTICLES").is_ok_and(|v| v.trim() == "1")
    })
}

// ── Converting a page for its move ───────────────────────────────────────────

/// A page's markdown converted, judged and written into a new document.
#[derive(Debug, Clone)]
pub struct Converted {
    /// The new document: its export, full state and tree.
    pub blocks: Written,
    /// Each step the converter took that changed something on the way in.
    pub notes: Vec<Problem>,
    /// Where the blocks' text first departs from the markdown's.
    pub text_lost: Option<TextLoss>,
    /// Addresses of links and embeds the blocks drop with no note saying so.
    pub addresses_lost: Vec<String>,
    /// Where the export reads back to other blocks than the page holds.
    pub export_differs: Option<String>,
    /// Pending suggestions carried over as suggestion marks.
    pub suggestions: usize,
    /// Pending proposals kept as their text, each holding what a suggestion
    /// cannot (a mention, an image, a line break).
    pub proposals_as_text: usize,
    /// An article's syntax as its markdown and its blocks hold it; `None`
    /// for a user page.
    pub article: Option<ArticleCounts>,
}

/// Why a page whose conversion loses text stays markdown.
pub const LOSES_TEXT: &str =
    "Converting the page would lose some of its text, so your server left it in markdown. The report shows where.";

/// Why a page with a pending suggestion its blocks cannot hold stays
/// markdown: in blocks it would be text no one can accept or reject.
pub const SUGGESTION_HELD: &str = "A pending suggestion on the page covers a mention, an image or a line break, so your \
     server left the page in markdown. A block page's suggestion holds only text: accept or reject this one, then convert \
     the page again.";

/// Why an article whose sources would move stays markdown.
pub const SOURCES_MOVED: &str = "Converting the article would change which sentences its sources sit beside, so your \
     server left it in markdown.";

/// Why an article whose figure would change stays markdown.
pub const FIGURE_CHANGED: &str =
    "Converting the article would change one of its figures, so your server left it in markdown.";

/// Why an article with a source note on more than one line stays markdown:
/// the day page reads a note's first line only.
pub const NOTE_RUNS_ON: &str = "A source note in the article runs onto a second line, which the day page doesn't \
     read, so your server left the article in markdown.";

/// Why an article with two notes under one label stays markdown.
pub const NOTE_TWICE: &str =
    "Two source notes in the article share one label, so your server left the article in markdown.";

/// Why an article whose pending suggestion covers more than text stays
/// markdown: in blocks a suggestion holds only text, so accepting or
/// rejecting it would leave that source, mention or line break behind, or
/// keep it alone.
pub const ARTICLE_SUGGESTION_HELD: &str = "A pending suggestion in the article covers a source, a mention or a \
     line break, so your server left the article in markdown. A block page's suggestion holds only text: accept or \
     reject this one, and the article moves the next time it's opened.";

impl Converted {
    /// Whether the blocks would lose what the page says: a character of its
    /// text, or a link's or an embed's address with no note saying so.
    pub fn loses(&self) -> bool {
        self.text_lost.is_some() || !self.addresses_lost.is_empty()
    }

    /// Why the page stays markdown, or `None` when it moves: its blocks would
    /// lose what it says, hold a pending suggestion as its text, or, for an
    /// article, hold its sources, figures or notes other than its readers
    /// read them, or a pending suggestion only in part
    /// (`ArticleCounts::holds`).
    pub fn held_back(&self) -> Option<&'static str> {
        if self.loses() {
            return Some(LOSES_TEXT);
        }
        if self.proposals_as_text > 0 {
            return Some(SUGGESTION_HELD);
        }
        let counts = self.article.as_ref()?;
        if counts.holds() {
            return None;
        }
        Some(if counts.suggestions_over_nodes > 0 {
            ARTICLE_SUGGESTION_HELD
        } else if counts.notes_continued > 0 {
            NOTE_RUNS_ON
        } else if counts.notes_redefined > 0 {
            NOTE_TWICE
        } else if counts.figures_in != counts.figures_out + counts.figures_as_code {
            FIGURE_CHANGED
        } else {
            SOURCES_MOVED
        })
    }
}

/// Convert `markdown` as `dialect` reads it, judge the conversion, and build
/// the document. A user page is read as its markdown editor showed it
/// (`Dialect::Page`), an article (`Dialect::Article`) as its writers wrote
/// it; either way its pending suggestions are its owner's and stay
/// suggestions, and one its blocks can hold only in part holds the page
/// back (`Converted::held_back`). `Err` names why the markdown makes no
/// page. A conversion: run it through `pages::convert_off_workers`.
pub fn convert(markdown: &str, dialect: Dialect) -> std::result::Result<Converted, String> {
    let measured = virtues_document::migrate::measure_as(virtues_document::contract(), markdown, dialect);
    let outcome = measured.outcome;
    if !outcome.errors.is_empty() {
        return Err(format!(
            "Your server couldn't turn the page's markdown into blocks ({}), so it left the page in markdown.",
            crate::server::yjs::problems_summary(&outcome.errors)
        ));
    }
    let blocks = pages::new_tree_document(outcome.nodes);
    let tree = blocks.tree.as_deref().unwrap_or_default();
    let refused = virtues_document::validate(tree);
    if !refused.is_empty() {
        return Err(format!(
            "The blocks your server made from the page break the document contract ({}), so it left the page in markdown.",
            crate::server::yjs::problems_summary(&refused)
        ));
    }
    let suggestions = suggestions_in(tree);
    Ok(Converted {
        notes: outcome.notes,
        text_lost: measured.text_lost,
        addresses_lost: measured.addresses_lost,
        export_differs: measured.export_diff,
        suggestions,
        proposals_as_text: measured.proposals_as_text,
        article: measured.article,
        blocks,
    })
}

/// How many suggestions `tree` holds: proposals counted by their id, so a
/// change (a deletion and its insertion) is one.
fn suggestions_in(tree: &[Node]) -> usize {
    let mut ids = HashSet::new();
    for n in tree {
        n.walk(&mut |n| {
            for m in &n.marks {
                if matches!(m.kind.as_str(), "proposedDeletion" | "proposedInsertion") {
                    ids.insert(m.attrs.get("proposal").map(|v| v.to_string()).unwrap_or_default());
                }
            }
        });
    }
    ids.len()
}

// ── Comparing an article's texts ──────────────────────────────────────────

/// A page's markdown as this server compares it: an article that holds
/// blocks as its export would write it (`virtues_document::edition_of`),
/// any other page as it is. Both sides of every "has the owner changed it?"
/// go through here, so a release that writes the export differently (an
/// escape, a list marker) changes neither the answer nor what an article's
/// writers may touch. Markdown that makes no page compares as written. A
/// conversion, for a block article: run it off the async workers.
pub fn as_compared(format: PageFormat, is_article: bool, md: &str) -> String {
    if format == PageFormat::Tree && is_article {
        virtues_document::edition_of(md).unwrap_or_else(|_| md.to_string())
    } else {
        md.to_string()
    }
}

/// Whether two texts of one page read the same as this server compares
/// them ([`as_compared`]), worked out off the async workers. Texts equal as
/// written are the same without a conversion; a conversion that doesn't get
/// its turn in time compares them as written.
pub async fn reads_the_same(format: PageFormat, is_article: bool, a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if format != PageFormat::Tree || !is_article {
        return false;
    }
    let (a, b) = (a.to_string(), b.to_string());
    let (a2, b2) = (a.clone(), b.clone());
    match pages::convert_off_workers(move || as_compared(format, is_article, &a2) == as_compared(format, is_article, &b2))
        .await
    {
        Ok(same) => same,
        Err(e) => {
            tracing::warn!(error = %e, "an article's texts were compared as written: the conversion didn't run");
            a == b
        }
    }
}

/// Each of `texts`, of one page, as this server compares it
/// ([`as_compared`]), worked out off the async workers. Only a block
/// article converts; a conversion that doesn't get its turn in time leaves
/// the texts as written.
pub async fn compared(format: PageFormat, is_article: bool, texts: Vec<String>) -> Vec<String> {
    if format != PageFormat::Tree || !is_article {
        return texts;
    }
    let kept = texts.clone();
    match pages::convert_off_workers(move || {
        texts
            .iter()
            .map(|t| as_compared(format, is_article, t))
            .collect::<Vec<_>>()
    })
    .await
    {
        Ok(compared) => compared,
        Err(e) => {
            tracing::warn!(error = %e, "an article's texts were compared as written: the conversion didn't run");
            kept
        }
    }
}

// ── The edition, moved with the article ─────────────────────────────────────

/// What an article's provenance becomes when it moves to blocks: written in
/// the flip's own transaction ([`record_rebase`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArticleRebase {
    pub article_id: String,
    /// `machine_text` as it was read: the write is guarded on it.
    pub was: Option<String>,
    /// The edition as the export writes it: the export itself when the
    /// article was still exactly the edition, else the edition converted.
    /// `None` stays `None`: an article with no edition has none after.
    pub machine_text: Option<String>,
    /// Each sentence of the owner's, as the export spells it.
    pub theirs: Vec<String>,
    /// What the owner removed, less what the export says again.
    pub removed: Vec<String>,
}

/// Why an article stays markdown: what its move found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaysMarkdown(pub String);

impl std::fmt::Display for StaysMarkdown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Why an article's edition did not move ([`rebase`]): the article's own
/// reason, which holds until the article changes, or the database or the
/// converter failing this time, which the next open tries again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unrebased {
    /// Its last edition makes no page, or a sentence of its owner's has no
    /// place in the export ([`rebase_of`]).
    Stays(StaysMarkdown),
    /// Reading its provenance failed, or the converter was busy or too slow.
    ForNow(String),
}

impl std::fmt::Display for Unrebased {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stays(why) => why.fmt(f),
            Self::ForNow(why) => f.write_str(why),
        }
    }
}

/// Why an article whose owner's sentence the export cannot place stays
/// markdown.
pub const THEIRS_UNPLACED: &str =
    "A sentence you wrote in this article reads differently as blocks, so your server left it in markdown.";

/// Why an article whose last edition makes no page stays markdown.
pub const EDITION_UNREAD: &str =
    "Your server couldn't read what it last wrote in this article as blocks, so it left the article in markdown.";

/// Why an article whose move would unplace a note stays markdown.
pub const ANCHOR_LOST: &str =
    "A note you wrote beside this article would lose its place in blocks, so your server left the article in markdown.";

/// An article's provenance as stored.
#[derive(Debug, Clone, Default, sqlx::FromRow)]
pub struct Stored {
    pub machine_text: Option<String>,
    pub theirs: serde_json::Value,
    pub removed: serde_json::Value,
}

fn strings(v: &serde_json::Value) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

/// What the article `article_id`'s provenance becomes when its markdown
/// `source` moves to blocks exporting as `export`. Read from the database,
/// worked out off the async workers. A database or a converter that fails
/// is `ForNow`, never the article's own reason.
pub async fn rebase(
    pool: &PgPool,
    article_id: &str,
    source: &str,
    export: &str,
) -> std::result::Result<ArticleRebase, Unrebased> {
    let stored: Option<Stored> =
        sqlx::query_as("SELECT machine_text, theirs, removed FROM wiki_articles WHERE id = $1")
            .bind(article_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| {
                Unrebased::ForNow(format!(
                    "Your server couldn't read what the article's writers last wrote ({e}), so it left the article in markdown for now."
                ))
            })?;
    let Some(stored) = stored else {
        return Ok(ArticleRebase {
            article_id: article_id.to_string(),
            was: None,
            machine_text: None,
            theirs: vec![],
            removed: vec![],
        });
    };
    let (id, source, export) = (article_id.to_string(), source.to_string(), export.to_string());
    pages::convert_off_workers(move || rebase_of(&id, &stored, &source, &export))
        .await
        .map_err(|e| Unrebased::ForNow(e.to_string()))?
        .map_err(Unrebased::Stays)
}

/// [`rebase`] of what is stored, with nothing to wait on.
///
/// - `machine_text`: `None` stays `None`; the source itself (nobody changed
///   the article since its writers wrote it) becomes the export; any other
///   edition becomes what the export makes of it.
/// - `theirs`: each sentence still in the source becomes the export's
///   sentence that says it, matched by its key ([`sentence_key`]: escapes
///   and character references read), else by its plain words (its text as
///   the converter reads it). One that matches neither keeps the article
///   markdown. One no longer a sentence of the source is dropped, as the
///   next revision drops it (`wiki_editor::fold_provenance`): its words
///   inside a longer sentence (the owner wrote "Then I ran." over "I
///   ran.") are not it.
/// - `removed`: what the export says again is no longer removed.
pub fn rebase_of(
    article_id: &str,
    stored: &Stored,
    source: &str,
    export: &str,
) -> std::result::Result<ArticleRebase, StaysMarkdown> {
    let machine_text = match &stored.machine_text {
        None => None,
        Some(m) if m == source => Some(export.to_string()),
        Some(m) => Some(virtues_document::edition_of(m).map_err(|_| StaysMarkdown(EDITION_UNREAD.into()))?),
    };

    let said = sentences(export);
    let keys: Vec<String> = said.iter().map(|s| sentence_key(s)).collect();
    let in_source: HashSet<String> = sentences(source).iter().map(|s| sentence_key(s)).collect();
    let mut plain: Option<Vec<String>> = None;
    let mut theirs = vec![];
    for s in strings(&stored.theirs) {
        let key = sentence_key(&s);
        if let Some(i) = keys.iter().position(|k| *k == key) {
            theirs.push(said[i].clone());
            continue;
        }
        // Gone from the page: the next revision would drop it too.
        if !in_source.contains(&key) {
            continue;
        }
        let words = plain_words(&s);
        let plain = plain.get_or_insert_with(|| said.iter().map(|s| plain_words(s)).collect());
        match plain.iter().position(|p| !words.is_empty() && *p == words) {
            Some(i) => theirs.push(said[i].clone()),
            None => return Err(StaysMarkdown(THEIRS_UNPLACED.into())),
        }
    }
    let mut seen = HashSet::new();
    theirs.retain(|s| seen.insert(sentence_key(s)));

    let said_keys: HashSet<&str> = keys.iter().map(String::as_str).collect();
    let removed = strings(&stored.removed)
        .into_iter()
        .filter(|r| !said_keys.contains(sentence_key(r).as_str()))
        .collect();

    Ok(ArticleRebase {
        article_id: article_id.to_string(),
        was: stored.machine_text.clone(),
        machine_text,
        theirs,
        removed,
    })
}

/// A sentence's words as the converter reads them, a pending suggestion
/// in them still one: its blocks' text, whitespace collapsed.
fn plain_words(sentence: &str) -> String {
    let o = virtues_document::parse_own_article_markdown(sentence);
    let text: Vec<String> = o.nodes.iter().map(Node::text_content).collect();
    text.join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Write `rebase` inside the flip's transaction, unless the article's
/// edition changed since it was read (a writer recorded one meanwhile).
/// Returns whether the edition was still the one read; when it was not, the
/// flip rolls back and the move reads the page again. Neither
/// `last_written_at` nor `last_human_edit_at` moves: a move is no one's
/// edit. A rebase that changes nothing (an article with no edition and no
/// sentence of its owner's) leaves the row unwritten, so its `updated_at`
/// stays too.
pub async fn record_rebase(tx: &mut sqlx::PgConnection, rebase: &ArticleRebase) -> std::result::Result<bool, sqlx::Error> {
    let (theirs, removed) = (serde_json::json!(rebase.theirs), serde_json::json!(rebase.removed));
    // Locked to the flip's commit, so no writer records an edition between
    // this check and the commit.
    let read: Option<(bool, bool)> = sqlx::query_as(
        "SELECT machine_text IS NOT DISTINCT FROM $2, \
                (machine_text, theirs, removed) IS NOT DISTINCT FROM ($3, $4, $5) \
         FROM wiki_articles WHERE id = $1 FOR UPDATE",
    )
    .bind(&rebase.article_id)
    .bind(&rebase.was)
    .bind(&rebase.machine_text)
    .bind(&theirs)
    .bind(&removed)
    .fetch_optional(&mut *tx)
    .await?;
    match read {
        Some((true, true)) => Ok(true),
        Some((true, false)) => {
            sqlx::query("UPDATE wiki_articles SET machine_text = $2, theirs = $3, removed = $4 WHERE id = $1")
                .bind(&rebase.article_id)
                .bind(&rebase.machine_text)
                .bind(&theirs)
                .bind(&removed)
                .execute(&mut *tx)
                .await?;
            Ok(true)
        }
        Some((false, _)) | None => Ok(false),
    }
}

// ── Notes beside a sentence ──────────────────────────────────────────────────

/// A sentence's words with its markup gone, exactly as the day page's
/// `plainSentence` (`lib/wiki/dayNotes.ts`) reads them, for telling whether
/// a note's quote still lands: its inline markdown read by
/// `inlineMarkdown.tokenize`'s rules ([`plain_text`]), veil brackets gone,
/// whitespace collapsed. Nothing looser: no case folded, no delimiter
/// dropped where the page draws it (`~tired~`, `__done__`), an `@` kept on
/// the label of a link that names no ref, so a move that changes what the
/// page draws reads as the note's sentence gone. `plain-sentences.json`,
/// beside `dayNotes.ts`, holds both to the same answers.
pub fn plain_sentence(markdown: &str) -> String {
    let text: String = plain_text(markdown).chars().filter(|c| !matches!(c, '⟦' | '⟧')).collect();
    text.split(js_space).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ")
}

/// What JavaScript's `\s` matches, which `plainSentence` collapses.
fn js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}'
            | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}

/// What JavaScript's `.` does not match.
fn js_line_end(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// What JavaScript's `\w` matches.
fn js_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The route bases a ref names (`ROUTE_TO_TYPE` in `lib/utils/refRoutes.ts`):
/// the editor's `![@Name](/person/id)` is a ref only to one of them.
const ENTITY_ROUTES: &[&str] = &[
    "/person", "/place", "/org", "/day", "/year", "/drive", "/page", "/chat", "/project", "/notebook", "/sources",
    "/source", "/record",
];

/// `parseEntityRoute` in `lib/utils/refRoutes.ts`: whether `url` names a ref.
fn entity_route(url: &str) -> bool {
    ENTITY_ROUTES
        .iter()
        .any(|base| url.strip_prefix(base).and_then(|r| r.strip_prefix('/')).is_some_and(|id| !id.is_empty()))
}

/// The words of inline markdown, its markup gone, as the day page reads
/// them: `plainText` over `tokenize` in `lib/wiki/inlineMarkdown.ts`, its
/// one regular expression read here by hand, alternative for alternative,
/// leftmost match first and the first alternative to match there: an
/// escape, a character reference, code, a link, strong, strike, emphasis
/// with `*`, emphasis with `_`.
fn plain_text(md: &str) -> String {
    let s: Vec<char> = md.chars().collect();
    let mut out = String::with_capacity(md.len());
    let (mut i, mut from) = (0, 0);
    while i < s.len() {
        match inline_at(&s, i) {
            Some((end, words)) => {
                out.extend(&s[from..i]);
                out.push_str(&words);
                i = end;
                from = end;
            }
            None => i += 1,
        }
    }
    out.extend(&s[from..]);
    out
}

/// The match of `tokenize`'s expression at `s[i]`, if any: where it ends,
/// and the words it reads as.
fn inline_at(s: &[char], i: usize) -> Option<(usize, String)> {
    let at = |k: usize| s.get(k).copied();
    let text = |a: usize, b: usize| s[a..b].iter().collect::<String>();
    match s[i] {
        // `\` and ASCII punctuation or a line's end: that character.
        '\\' => at(i + 1)
            .filter(|c| c.is_ascii_punctuation() || *c == '\n')
            .map(|c| (i + 2, c.to_string())),
        '&' => reference_at(s, i),
        '`' => {
            let close = (i + 1..s.len()).find(|&k| s[k] == '`')?;
            (close > i + 1).then(|| (close + 1, text(i + 1, close)))
        }
        '!' | '[' => link_at(s, i),
        '*' if at(i + 1) == Some('*') => wrapped_at(s, i, '*').or_else(|| emphasis_at(s, i, '*')),
        '~' if at(i + 1) == Some('~') => wrapped_at(s, i, '~'),
        '*' | '_' => emphasis_at(s, i, s[i]),
        _ => None,
    }
}

/// `&…;` CommonMark reads in text, as the character it names: a decimal or
/// hexadecimal one, or `amp`, `lt`, `gt`, `quot`, `apos`, `nbsp`. A number
/// past Unicode, a surrogate or zero is U+FFFD.
fn reference_at(s: &[char], i: usize) -> Option<(usize, String)> {
    let run = |from: usize, ok: fn(char) -> bool, most: usize| {
        let n = s[from.min(s.len())..].iter().take_while(|c| ok(**c)).count().min(most);
        (n > 0 && s.get(from + n) == Some(&';')).then_some(n)
    };
    let number = |digits: &[char], radix: u32| -> String {
        let code = u32::from_str_radix(&digits.iter().collect::<String>(), radix).ok();
        match code.filter(|&c| c != 0).and_then(char::from_u32) {
            Some(c) => c.to_string(),
            None => '\u{fffd}'.to_string(),
        }
    };
    if s.get(i + 1) == Some(&'#') {
        if let Some(n) = run(i + 2, |c| c.is_ascii_digit(), 7) {
            return Some((i + 3 + n, number(&s[i + 2..i + 2 + n], 10)));
        }
        if matches!(s.get(i + 2), Some('x' | 'X')) {
            if let Some(n) = run(i + 3, |c| c.is_ascii_hexdigit(), 6) {
                return Some((i + 4 + n, number(&s[i + 3..i + 3 + n], 16)));
            }
        }
        return None;
    }
    for (name, c) in [("amp", "&"), ("lt", "<"), ("gt", ">"), ("quot", "\""), ("apos", "'"), ("nbsp", "\u{a0}")] {
        let end = i + 1 + name.len();
        if s.get(i + 1..end).is_some_and(|w| w.iter().copied().eq(name.chars())) && s.get(end) == Some(&';') {
            return Some((end + 1, c.to_string()));
        }
    }
    None
}

/// `[label](url)`, or `![label](url)`: the label's words, escapes and
/// references read. Either is a ref when its address names one, drawn
/// without the label's `@`: the editor wrote a ref both ways, and a block
/// page exports the one mention as the link. Any other image is the text
/// it was written as.
fn link_at(s: &[char], i: usize) -> Option<(usize, String)> {
    let image = s[i] == '!';
    let open = if image { i + 1 } else { i };
    if s.get(open) != Some(&'[') {
        return None;
    }
    // The label: escapes, or anything but `]` and `\`.
    let mut p = open + 1;
    while p < s.len() && s[p] != ']' {
        if s[p] == '\\' {
            if s.get(p + 1).is_none_or(|c| js_line_end(*c)) {
                return None;
            }
            p += 2;
        } else {
            p += 1;
        }
    }
    if p == open + 1 || s.get(p) != Some(&']') || s.get(p + 1) != Some(&'(') {
        return None;
    }
    let label: String = s[open + 1..p].iter().collect();
    let q = p + 2;
    let (raw, end) = angled_url(s, q).or_else(|| bare_url(s, q))?;
    let url = unescaped(raw.strip_prefix('<').and_then(|r| r.strip_suffix('>')).unwrap_or(&raw));
    let label = unescaped(&label);
    let is_ref = entity_route(&url);
    if image && !is_ref {
        return Some((end, s[i..end].iter().collect()));
    }
    let shown = if is_ref { label.strip_prefix('@').unwrap_or(&label).to_string() } else { label };
    Some((end, shown))
}

/// `<…>)` from `s[q]`, with no `<`, `>` or line feed inside: the address
/// with its brackets, and where the link ends.
fn angled_url(s: &[char], q: usize) -> Option<(String, usize)> {
    if s.get(q) != Some(&'<') {
        return None;
    }
    let close = (q + 1..s.len()).find(|&k| matches!(s[k], '<' | '>' | '\n'))?;
    (s[close] == '>' && s.get(close + 1) == Some(&')'))
        .then(|| (s[q..=close].iter().collect(), close + 2))
}

/// An address from `s[q]` of anything but parentheses and whitespace, or a
/// pair of parentheses holding neither, then `)`.
fn bare_url(s: &[char], q: usize) -> Option<(String, usize)> {
    let plain = |c: char| c != '(' && c != ')' && !js_space(c);
    let mut p = q;
    loop {
        match s.get(p) {
            Some(&c) if plain(c) => p += 1,
            Some('(') => {
                let inner = s[p + 1..].iter().take_while(|c| plain(**c)).count();
                if s.get(p + 1 + inner) != Some(&')') {
                    break;
                }
                p += inner + 2;
            }
            _ => break,
        }
    }
    (p > q && s.get(p) == Some(&')')).then(|| (s[q..p].iter().collect(), p + 1))
}

/// Strong (`**`) or strike (`~~`) from `s[i]`, `mark` its character: a
/// delimiter hugging its words, the shortest that closes, on one line, and
/// never closing after a backslash. Its words are read again.
fn wrapped_at(s: &[char], i: usize, mark: char) -> Option<(usize, String)> {
    let first = *s.get(i + 2)?;
    if js_space(first) {
        return None;
    }
    let mut e = i + 3;
    while e <= s.len() {
        let last = s[e - 1];
        if js_line_end(last) {
            return None;
        }
        if !js_space(last) && last != '\\' && s.get(e) == Some(&mark) && s.get(e + 1) == Some(&mark) {
            return Some((e + 2, plain_text(&s[i + 2..e].iter().collect::<String>())));
        }
        e += 1;
    }
    None
}

/// Emphasis with a single `*` or `_` from `s[i]`: never inside a word,
/// hugging its words, the shortest that closes, holding no bare delimiter
/// of its own and never closing after a backslash. Its words are read again.
fn emphasis_at(s: &[char], i: usize, mark: char) -> Option<(usize, String)> {
    let joined = |c: Option<char>| c.is_some_and(|c| js_word(c) || c == mark);
    if i > 0 && joined(Some(s[i - 1])) {
        return None;
    }
    if s.get(i + 1).is_none_or(|c| js_space(*c)) {
        return None;
    }
    let mut p = i + 1;
    loop {
        // One more unit of its words: an escape, or anything but the mark and `\`.
        match s.get(p) {
            Some('\\') if s.get(p + 1).is_some_and(|c| !js_line_end(*c)) => p += 2,
            Some(&c) if c != mark && c != '\\' => p += 1,
            _ => return None,
        }
        let last = s[p - 1];
        if s.get(p) == Some(&mark) && !js_space(last) && last != '\\' && !joined(s.get(p + 1).copied()) {
            return Some((p + 1, plain_text(&s[i + 1..p].iter().collect::<String>())));
        }
    }
}

/// Markdown text with its escapes, hard line breaks and character
/// references read: `unescapeMarkdown` in `lib/wiki/inlineMarkdown.ts`.
fn unescaped(md: &str) -> String {
    let s: Vec<char> = md.chars().collect();
    let mut out = String::with_capacity(md.len());
    let mut i = 0;
    while i < s.len() {
        let read = match s[i] {
            '\\' => s.get(i + 1).filter(|c| c.is_ascii_punctuation() || **c == '\n').map(|c| (i + 2, c.to_string())),
            '&' => reference_at(&s, i),
            _ => None,
        };
        match read {
            Some((end, c)) => {
                out.push_str(&c);
                i = end;
            }
            None => {
                out.push(s[i]);
                i += 1;
            }
        }
    }
    out
}

/// The sentences a note's quote may land beside: each paragraph's, as the
/// day page reads them ([`day_sentences`]), as plain words
/// ([`plain_sentence`]). Headings, figures, tables, lists and quotes, and the
/// footnote definitions, hold none: a note sits beside a paragraph's
/// sentence (`dayNotes.placeNotes`).
fn passages(md: &str) -> Vec<String> {
    let mut out = vec![];
    let mut block: Vec<&str> = vec![];
    let mut fence = false;
    let mut part = |block: &mut Vec<&str>| {
        let text = block.join("\n");
        if is_paragraph(text.trim()) {
            out.extend(day_sentences(&text).iter().map(|s| plain_sentence(s)).filter(|s| !s.is_empty()));
        }
        block.clear();
    };
    for line in md.lines() {
        let t = line.trim_start();
        if t.starts_with("```") {
            part(&mut block);
            fence = !fence;
            continue;
        }
        if fence || is_definition(t) {
            continue;
        }
        if t.is_empty() {
            part(&mut block);
        } else if is_heading(line) {
            part(&mut block);
        } else {
            block.push(line);
        }
    }
    part(&mut block);
    out
}

/// Whether a line is a heading, which the day page reads as a block of its
/// own wherever it stands.
fn is_heading(line: &str) -> bool {
    let t = line.trim_start_matches(' ');
    line.len() - t.len() <= 3 && {
        let hashes = t.len() - t.trim_start_matches('#').len();
        (1..=6).contains(&hashes) && t[hashes..].chars().next().is_none_or(|c| c == ' ' || c == '\t')
    }
}

/// Whether a block of the page is a paragraph to the day page (`kindOf` in
/// `lib/wiki/dayArticle.ts`): not a table of rows under a rule, nor a list,
/// a quote, a fence or an image.
fn is_paragraph(block: &str) -> bool {
    use std::sync::LazyLock;
    static OTHER: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^(```|>|-\s|\*\s|\d+[.)]\s|!\[)").expect("the block pattern"));
    static RULE_ROW: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"^\s*\|?\s*:?-+:?\s*(?:\|\s*:?-+:?\s*)*\|?\s*$").expect("the rule pattern")
    });
    if block.is_empty() || OTHER.is_match(block) {
        return false;
    }
    let lines: Vec<&str> = block.lines().collect();
    !(lines.len() > 1 && lines.iter().all(|l| l.trim().starts_with('|')) && RULE_ROW.is_match(lines[1]))
}

/// A paragraph's sentences as the day page splits them (`sentencesOf` in
/// `lib/wiki/dayArticle.ts`): hard line breaks and lines read as spaces,
/// each run of footnote markers closing a sentence, and between them a
/// stop followed by space and what starts a sentence ending one, but after
/// an abbreviation or an initial, inside a link, a veil or bold, or when
/// the stop is escaped.
pub fn day_sentences(paragraph: &str) -> Vec<String> {
    use std::sync::LazyLock;
    static MARK_RUN: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"(?:\[\^(?:ev|cx)-\d+\])+").expect("the marker pattern"));
    let joined = hard_breaks_read(paragraph);
    let text = joined.split('\n').map(str::trim).collect::<Vec<_>>().join(" ");
    let mut out = vec![];
    let mut from = 0;
    for run in MARK_RUN.find_iter(&text) {
        out.extend(split_sentences(text[from..run.start()].trim()));
        from = run.end();
    }
    out.extend(split_sentences(text[from..].trim()));
    out.retain(|s: &String| !s.trim().is_empty());
    out
}

/// Markdown with each hard line break's backslash read: the line's end
/// alone. A backslash that is itself escaped stays.
fn hard_breaks_read(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    let mut chars = md.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some('\\') => {
                    out.push('\\');
                    out.push(chars.next().unwrap_or('\\'));
                    continue;
                }
                Some('\n') => continue,
                _ => {}
            }
        }
        out.push(c);
    }
    out
}

/// One piece of a paragraph between runs of markers, cut into sentences
/// (`splitSentences` in `lib/wiki/dayArticle.ts`).
fn split_sentences(chunk: &str) -> Vec<String> {
    let chars: Vec<char> = chunk.chars().collect();
    let mut parts: Vec<String> = vec![];
    let mut start = 0;
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < chars.len() && chars[j].is_whitespace() {
            j += 1;
        }
        if ends_sentence(&chars, i) && starts_sentence(&chars, j) {
            parts.push(chars[start..i].iter().collect());
            start = j;
        }
        i = j;
    }
    parts.push(chars[start..].iter().collect());
    let mut merged: Vec<String> = vec![];
    for p in parts {
        match merged.last_mut() {
            Some(last) if abbreviated(last) || unclosed(last) => {
                last.push(' ');
                last.push_str(&p);
            }
            _ => merged.push(p),
        }
    }
    merged
}

/// Whether the text before `chars[at]` ends with a stop, a closing quote or
/// parenthesis after it at most, the stop not escaped.
fn ends_sentence(chars: &[char], at: usize) -> bool {
    let mut k = at;
    if k > 0 && matches!(chars[k - 1], '"' | '”' | '’' | ')') {
        k -= 1;
    }
    k > 0 && matches!(chars[k - 1], '.' | '!' | '?') && (k < 2 || chars[k - 2] != '\\')
}

/// Whether what starts at `chars[at]` starts a sentence: a capital, a veil,
/// a link, bold or a quote, or one of the last three escaped.
fn starts_sentence(chars: &[char], at: usize) -> bool {
    match chars.get(at) {
        Some(c) if c.is_ascii_uppercase() || matches!(c, '⟦' | '[' | '*' | '"' | '“') => true,
        Some('\\') => matches!(chars.get(at + 1), Some('[' | '*' | '"')),
        _ => false,
    }
}

/// Whether a piece ends in a stop that ends no sentence: "St. John", an
/// initial ("David O. Okafor").
fn abbreviated(piece: &str) -> bool {
    let Some(body) = piece.strip_suffix('.') else {
        return false;
    };
    const ABBR: &[&str] = &["Dr", "Mr", "Mrs", "Ms", "St", "Mt", "Ft", "Jr", "Sr", "vs", "e.g", "i.e"];
    ABBR.iter().any(|a| {
        body.strip_suffix(a)
            .is_some_and(|before| !before.chars().last().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_'))
    }) || {
        let mut tail = body.chars().rev();
        tail.next().is_some_and(|c| c.is_ascii_uppercase())
            && !tail.next().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
    }
}

/// Whether a link, a veil or bold is still open at a piece's end, escaped
/// brackets and stars opening nothing.
fn unclosed(piece: &str) -> bool {
    let mut s = String::with_capacity(piece.len());
    let mut chars = piece.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek().is_some_and(|n| n.is_ascii_punctuation()) {
            chars.next();
            continue;
        }
        s.push(c);
    }
    let count = |c: char| s.matches(c).count();
    count('[') > count(']') || count('⟦') > count('⟧') || count('(') > count(')') || s.matches("**").count() % 2 == 1
}

/// Whether a line is a source note the day page lifts out of the text
/// (`DEF` in `lib/wiki/dayArticle.ts`): `[^ev-N]:` or `[^cx-N]:` and its
/// words. It draws any other definition (`[^note]: …`) as a paragraph.
fn is_definition(line: &str) -> bool {
    let Some((label, note)) = line.trim().strip_prefix("[^").and_then(|rest| rest.split_once("]:")) else {
        return false;
    };
    virtues_document::migrate::lifted_by_the_day_page(label) && !note.trim_start().is_empty()
}

/// The quotes among `quotes` that land in `source` and would not in
/// `export`: each a whole sentence of a paragraph, as the day page places
/// a note beside one.
pub fn quotes_lost(quotes: &[String], source: &str, export: &str) -> Vec<String> {
    let (before, after) = (passages(source), passages(export));
    let lands = |sentences: &[String], want: &str| sentences.iter().any(|p| p == want);
    quotes
        .iter()
        .filter(|q| {
            let want = plain_sentence(q);
            !want.is_empty() && lands(&before, &want) && !lands(&after, &want)
        })
        .cloned()
        .collect()
}

/// The open notes on `subject` (its type and id) anchored beside a sentence
/// whose words land in `source` and would not land in `export`: notes the
/// move would leave beside the Abstract. A false alarm only keeps an
/// article markdown.
pub async fn anchors_lost(pool: &PgPool, subject: (&str, &str), source: &str, export: &str) -> Result<Vec<String>> {
    let quotes: Vec<Option<String>> = sqlx::query_scalar(
        "SELECT anchor->>'quote' FROM wiki_notes \
         WHERE subject_type = $1 AND subject_id = $2 AND resolved_at IS NULL AND anchor IS NOT NULL",
    )
    .bind(subject.0)
    .bind(subject.1)
    .fetch_all(pool)
    .await
    .map_err(|e| crate::Error::Database(format!("Failed to read the notes beside the article: {e}")))?;
    let quotes: Vec<String> = quotes.into_iter().flatten().collect();
    if quotes.is_empty() {
        return Ok(vec![]);
    }
    Ok(quotes_lost(&quotes, source, export))
}

// ── An article's first open ───────────────────────────────────────────────────

/// What a markdown article nobody had opened came to when the server first
/// built its document ([`first_open`]).
#[derive(Debug)]
pub(crate) enum FirstOpen {
    /// The page holds blocks now: read it again.
    Moved,
    /// The page stays markdown, for `reason`. `settled` when the reason is
    /// the page's own, not a database or a busy converter: it holds until
    /// the page changes.
    StaysMarkdown { reason: String, settled: bool },
    /// The page changed since it was read: read it again.
    Again,
}

fn stays(reason: impl Into<String>, settled: bool) -> FirstOpen {
    FirstOpen::StaysMarkdown {
        reason: reason.into(),
        settled,
    }
}

/// Move the markdown article `page_id`, read as `content` at `read_at`, to
/// blocks, as the server first builds its document:
///
/// 1. Convert and judge it as an article ([`convert`]); its rebased
///    edition ([`rebase`]); and the notes beside it ([`anchors_lost`]). Any
///    of them holding it back leaves it markdown.
/// 2. Keep it as it was ("Before blocks", `pages::keep_before_blocks`), as
///    the run keeps a page.
/// 3. Flip the row (`yjs::move_row`): state, export, format, a fresh
///    lineage and the rebase, in one transaction, guarded on `read_at`.
///    Missed (narration or another first open wrote meanwhile), the version
///    step 2 cut is dropped and the page is read again.
/// 4. Version the blocks ("Moved to blocks"). History reads neither version
///    as an edit (`pages::is_move_version`).
pub(crate) async fn first_open(pool: &PgPool, page_id: &str, content: &str, read_at: DateTime<Utc>) -> FirstOpen {
    let markdown = content.to_string();
    let converted = match pages::convert_off_workers(move || convert(&markdown, Dialect::Article)).await {
        Ok(Ok(converted)) => converted,
        Ok(Err(reason)) => return stays(reason, true),
        Err(e) => return stays(e.to_string(), false),
    };
    if let Some(why) = converted.held_back() {
        return stays(why, true);
    }
    let export = converted.blocks.text.as_str();

    let article: Option<(String, String, String)> = match sqlx::query_as(
        "SELECT id, subject_type, subject_id FROM wiki_articles WHERE page_id = $1",
    )
    .bind(page_id)
    .fetch_optional(pool)
    .await
    {
        Ok(article) => article,
        Err(e) => return stays(format!("Your server couldn't read the article: {e}"), false),
    };
    let rebase = match &article {
        Some((id, ..)) => match rebase(pool, id, content, export).await {
            Ok(rebase) => Some(rebase),
            Err(Unrebased::Stays(StaysMarkdown(reason))) => return stays(reason, true),
            Err(Unrebased::ForNow(reason)) => return stays(reason, false),
        },
        None => None,
    };
    if let Some((_, subject_type, subject_id)) = &article {
        match anchors_lost(pool, (subject_type, subject_id), content, export).await {
            Ok(lost) if !lost.is_empty() => return stays(ANCHOR_LOST, true),
            Ok(_) => {}
            Err(e) => return stays(e.to_string(), false),
        }
    }

    let before = Written {
        text: content.to_string(),
        state: crate::server::yjs::state_from_text(content),
        tree: None,
    };
    let kept = match pages::keep_before_blocks(pool, page_id, before).await {
        Ok(kept) => kept,
        Err(e) => {
            return stays(
                format!("Your server couldn't keep a copy of the article as it was, so it left it in markdown: {e}"),
                false,
            )
        }
    };
    let blocks = &converted.blocks;
    let moved = crate::server::yjs::move_row(pool, page_id, read_at, &blocks.state, &blocks.text, rebase.as_ref()).await;
    match moved {
        Ok(true) => {}
        Ok(false) => {
            if kept.cut {
                if let Err(e) = pages::drop_before_blocks(pool, page_id, kept.point.version_number).await {
                    tracing::warn!(page_id, error = %e, "an article's copy from before blocks stays, for a move that did not happen");
                }
            }
            return FirstOpen::Again;
        }
        Err(e) => {
            if kept.cut {
                if let Err(e) = pages::drop_before_blocks(pool, page_id, kept.point.version_number).await {
                    tracing::warn!(page_id, error = %e, "an article's copy from before blocks stays, for a move that did not happen");
                }
            }
            return stays(format!("Your server couldn't write the article as blocks: {e}"), false);
        }
    }
    pages::cut_version(pool, page_id, blocks, "auto", Some(crate::api::page_convert::MOVED_TO_BLOCKS)).await;
    tracing::info!(page_id, notes = converted.notes.len(), "an article moved to blocks as it was first opened");
    FirstOpen::Moved
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored(machine_text: Option<&str>, theirs: &[&str], removed: &[&str]) -> Stored {
        Stored {
            machine_text: machine_text.map(str::to_string),
            theirs: serde_json::json!(theirs),
            removed: serde_json::json!(removed),
        }
    }

    #[test]
    fn an_untouched_edition_becomes_the_export() {
        let source = "You met Nick at the R&B night.\n";
        let export = "You met Nick at the R\\&B night.\n";
        let r = rebase_of("a_1", &stored(Some(source), &[], &[]), source, export).unwrap();
        assert_eq!(r.machine_text.as_deref(), Some(export));
        assert_eq!(r.was.as_deref(), Some(source));
        let none = rebase_of("a_1", &stored(None, &[], &[]), source, export).unwrap();
        assert_eq!(none.machine_text, None, "no edition stays none");
    }

    #[test]
    fn an_edited_articles_edition_is_converted_on_its_own() {
        let edition = "You met Nick at the R&B night.\n";
        let source = "You met Nick at the R&B night.\n\nI stayed late.\n";
        let export = "You met Nick at the R\\&B night.\n\nI stayed late.\n";
        let r = rebase_of("a_1", &stored(Some(edition), &["I stayed late."], &[]), source, export).unwrap();
        assert_eq!(r.machine_text.as_deref(), Some("You met Nick at the R\\&B night.\n"));
        assert_eq!(r.theirs, ["I stayed late."]);
    }

    #[test]
    fn their_sentence_takes_the_exports_spelling() {
        let source = "A day.\n\nI paid for user_name & co.\n";
        let export = "A day.\n\nI paid for user\\_name \\& co.\n";
        let r = rebase_of("a_1", &stored(Some("A day.\n"), &["I paid for user_name & co."], &[]), source, export)
            .unwrap();
        assert_eq!(r.theirs, ["I paid for user\\_name \\& co."]);
    }

    #[test]
    fn their_sentence_in_other_markup_matches_by_its_words() {
        let source = "A day.\n\nIt was *very* late.\n";
        let export = "A day.\n\nIt was _very_ late.\n";
        let r = rebase_of("a_1", &stored(Some("A day.\n"), &["It was *very* late."], &[]), source, export).unwrap();
        assert_eq!(r.theirs, ["It was _very_ late."]);
    }

    #[test]
    fn a_sentence_of_theirs_the_export_cannot_place_keeps_the_article_markdown() {
        let source = "A day.\n\nI rowed alone.\n";
        let export = "A day.\n\nI rowed with Nick.\n";
        let refused = rebase_of("a_1", &stored(Some("A day.\n"), &["I rowed alone."], &[]), source, export);
        assert_eq!(refused, Err(StaysMarkdown(THEIRS_UNPLACED.into())));
    }

    #[test]
    fn a_sentence_of_theirs_no_longer_on_the_page_is_dropped() {
        let source = "A day.\n";
        let r = rebase_of("a_1", &stored(Some("A day.\n"), &["I deleted this one."], &[]), source, source).unwrap();
        assert!(r.theirs.is_empty());
    }

    /// The owner's sentence whose words now stand inside a longer one (they
    /// wrote "Then I ran." over "I ran.") is no longer a sentence of the
    /// page: dropped, as the next revision drops it, not a sentence the
    /// export cannot place, which kept the article markdown for good.
    #[test]
    fn a_sentence_of_theirs_now_inside_a_longer_one_is_dropped() {
        for source in ["A day.\n\nThen I ran.\n", "A day.\n\nI ran. then I left.\n"] {
            let theirs = ["I ran.".to_string()];
            let r = rebase_of("a_1", &stored(Some("A day.\n"), &["I ran."], &[]), source, source);
            assert_eq!(r.map(|r| r.theirs), Ok(vec![]), "{source}");
            let (folded, _) = crate::api::wiki_editor::fold_provenance(&theirs, &[], &[], &[], source);
            assert!(folded.is_empty(), "{source}: the next revision drops it too");
        }
    }

    #[test]
    fn a_removal_the_export_says_again_is_no_longer_one() {
        let source = "A day.\n\nYou slept & woke.\n";
        let export = "A day.\n\nYou slept \\& woke.\n";
        let r = rebase_of("a_1", &stored(Some(source), &[], &["You slept & woke.", "Gone for good."]), source, export)
            .unwrap();
        assert_eq!(r.removed, ["Gone for good."]);
    }

    /// The day page's own answers (`plain-sentences.json`, which
    /// `dayNotes.test.ts` holds `plainSentence` to): every sentence reads
    /// here as the page reads it.
    #[test]
    fn plain_sentences_read_as_the_day_page_reads_them() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../../apps/web/src/lib/wiki/plain-sentences.json")).unwrap();
        assert!(cases.len() > 10);
        for case in cases {
            let (markdown, plain) = (case["markdown"].as_str().unwrap(), case["plain"].as_str().unwrap());
            assert_eq!(plain_sentence(markdown), plain, "{markdown:?}");
        }
    }

    /// A move that changes what the day page draws of a sentence loses the
    /// note beside it, however little the change: a single `~` the export
    /// reads as a strike, `__` as bold.
    #[test]
    fn a_note_on_a_sentence_the_move_redraws_is_lost() {
        for (sentence, moved) in [
            ("You were ~tired~ by noon.", "You were ~~tired~~ by noon."),
            ("You felt __done__ today.", "You felt **done** today."),
        ] {
            let source = format!("A day.\n\n## Morning\n\n{sentence}[^ev-1]\n\n[^ev-1]: Message · 9:00 AM\n");
            let export = format!("A day.\n\n## Morning\n\n{moved}[^ev-1]\n\n[^ev-1]: Message · 9:00 AM\n");
            let quotes = vec![plain_sentence(sentence)];
            assert_eq!(quotes_lost(&quotes, &source, &export), quotes, "{sentence}");
            assert!(quotes_lost(&quotes, &source, &source).is_empty(), "{sentence}");
        }
    }

    /// A ref the editor wrote as an image, `![@Nick](/person/id)`, is the
    /// mention the export writes as a link, `[@Nick](/person/id)`, and the
    /// day page draws both as the name: the note beside its sentence stays.
    /// Drawn with its `@` as a link, every such sentence read differently
    /// once its article moved.
    #[test]
    fn a_ref_the_export_writes_as_a_link_reads_as_it_did() {
        let sentence = "You walked with ![@Nick](/person/person_1) by the river.";
        let moved = "You walked with [@Nick](/person/person_1) by the river.";
        assert_eq!(plain_sentence(moved), plain_sentence(sentence));
        let source = format!("A day.\n\n## Morning\n\n{sentence}[^ev-1]\n\n[^ev-1]: Message · 9:00 AM\n");
        let export = format!("A day.\n\n## Morning\n\n{moved}[^ev-1]\n\n[^ev-1]: Message · 9:00 AM\n");
        let quotes = vec![plain_sentence(sentence)];
        assert!(quotes_lost(&quotes, &source, &export).is_empty());
    }

    #[test]
    fn a_quote_that_lands_before_and_not_after_is_lost() {
        let source = "You walked by the river.\n\n## Lunch[^cx-1]\n\nYou ate with Nick.[^ev-1]\n\n[^ev-1]: Message · 1:00 PM · data_communication_message:m_1\n[^cx-1]: 1:00 PM–2:00 PM\n";
        let export = "You walked by the river.\n\n## Lunch[^cx-1]\n\nYou ate alone.[^ev-1]\n\n[^ev-1]: Message · 1:00 PM · data_communication_message:m_1\n[^cx-1]: 1:00 PM–2:00 PM\n";
        let quotes = vec!["You ate with Nick.".to_string(), "You walked by the river.".to_string()];
        assert_eq!(quotes_lost(&quotes, source, export), ["You ate with Nick."]);
        assert!(quotes_lost(&quotes, source, source).is_empty());
        let nowhere = vec!["Never on the page.".to_string()];
        assert!(quotes_lost(&nowhere, source, export).is_empty(), "a quote that never landed is not lost by the move");
    }

    /// A note sits beside a whole sentence of a paragraph (`placeNotes`):
    /// words that stand inside a longer sentence after the move are a note
    /// that lost its place, whatever passage still holds them.
    #[test]
    fn a_quote_lands_as_a_whole_sentence() {
        let quotes = vec!["You read.".to_string()];
        let source = "A day.\n\nYou read. It rained.\n";
        assert_eq!(quotes_lost(&quotes, source, "A day.\n\nLater you read. It rained.\n"), quotes);
        assert!(quotes_lost(&quotes, source, "A day.\n\nIt rained. You read.\n").is_empty());
        // A heading, a list or a figure holds no sentence a note sits by.
        assert!(quotes_lost(&quotes, "## You read.\n\n- You read.\n", "A day.\n").is_empty());
    }

    /// The export writes a hard line break as a backslash ending the line,
    /// where the writer wrote two spaces or `<br>`. Read as the day page
    /// reads it, the sentences either side stay apart, and a note beside
    /// either keeps its place.
    #[test]
    fn a_note_after_a_hard_break_keeps_its_place() {
        let source = "Lead.\n\nYou cooked dinner.[^ev-2]  \nThen you read a book. It was late.\n\n[^ev-2]: Location · 7:00 PM\n";
        let export = "Lead.\n\nYou cooked dinner.[^ev-2]\\\nThen you read a book. It was late.\n\n[^ev-2]: Location · 7:00 PM\n";
        let quotes = vec!["Then you read a book.".to_string(), "You cooked dinner.".to_string()];
        assert!(quotes_lost(&quotes, source, export).is_empty(), "{:?}", passages(export));
        assert_eq!(
            day_sentences("You cooked dinner.\\\nThen you read a book. It was late."),
            ["You cooked dinner.", "Then you read a book.", "It was late."]
        );
        assert_eq!(plain_sentence("dinner.\\\nThen"), "dinner. Then");
    }

    /// The day page's sentences: a stop then a capital ends one, but after
    /// an abbreviation or an initial, inside a link, or when escaped; each
    /// run of markers closes the sentence before it.
    #[test]
    fn sentences_split_as_the_day_page_splits_them() {
        assert_eq!(
            day_sentences("You met Dr. Vance and David O. Okafor at [St. Paul. Church](/place/p_1). It rained.[^ev-1][^ev-2] Then home"),
            ["You met Dr. Vance and David O. Okafor at [St. Paul. Church](/place/p_1).", "It rained.", "Then home"]
        );
        assert_eq!(day_sentences("In 2019\\. We went. \\[Late\\] again."), ["In 2019\\. We went.", "\\[Late\\] again."]);
        assert_eq!(day_sentences("e.g. This stays."), ["e.g. This stays."]);
    }

    #[test]
    fn a_source_note_is_no_passage() {
        let md = "A day.\n\n[^ev-1]: Message · 1:00 PM · data_communication_message:m_1\n";
        assert_eq!(passages(md), ["A day."]);
        assert!(is_definition("[^cx-2]: 9:00 AM"));
        assert!(!is_definition("[^]: x"));
        assert!(!is_definition("[^ev-1] is not one"));
        // The day page draws any other label's definition, and an empty
        // one, as a paragraph.
        assert!(!is_definition("[^note]: Remember the bank."));
        assert!(!is_definition("[^ev-1]:"));
        assert_eq!(passages("A day.\n\n[^note]: Remember the bank.\n"), ["A day.", "[^note]: Remember the bank."]);
    }

    #[test]
    fn a_user_page_and_a_markdown_article_compare_as_written() {
        let md = "R&B.\n";
        assert_eq!(as_compared(PageFormat::Markdown, true, md), md);
        assert_eq!(as_compared(PageFormat::Tree, false, md), md);
        assert_eq!(as_compared(PageFormat::Tree, true, md), "R\\&B.\n");
        assert_eq!(
            as_compared(PageFormat::Tree, true, "R\\&B.\n"),
            as_compared(PageFormat::Tree, true, md),
            "two spellings of one article compare the same"
        );
    }
}

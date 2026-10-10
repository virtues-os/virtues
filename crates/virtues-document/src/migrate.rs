//! Markdown → document tree: existing pages once, and markdown that writers
//! still send.
//!
//! Our markdown has a dialect on top of GFM: `==highlight==`, `<u>`,
//! `[@Label](/person/id)` references, `![alt|600](url)` image widths,
//! audio/video/file embeds written as images, CriticMarkup proposals
//! (`{--old--}{++new++}`) and GitHub's alerts (`> [!TIP]`). This maps the
//! dialect onto the contract, renders to HTML with pulldown-cmark, and
//! ingests it in migration mode, which reports every step that changed
//! something.
//!
//! [`measure`] checks one document the way a conversion is judged: was any
//! text lost, and does the markdown export read back to the same tree.

use crate::contract::{is_route, normalize_url, url_scheme, Contract};
use crate::ingest::{parse, Mode, Outcome, MAX_INPUT_BYTES};
use crate::model::{new_id, Node, Problem};
use crate::render;
use pulldown_cmark::{html::write_html, CowStr, Event, LinkType, Options, Parser, Tag, TagEnd};
use serde::Serialize;
use std::collections::HashMap;
use std::ops::Range;

/// The extensions each kind takes: the lists the browser's `kindOfLink`
/// holds (`apps/web/src/lib/document/media-kind.ts`), which both page
/// editors draw links by.
const IMAGE_EXT: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico", "avif", "heic", "heif", "tif", "tiff",
];
const AUDIO_EXT: &[&str] = &["mp3", "wav", "ogg", "m4a", "aac", "flac", "opus", "wma"];
const VIDEO_EXT: &[&str] = &["mp4", "webm", "mov", "avi", "mkv", "m4v", "ogv"];

/// What a media embed (`![alt](src)`) is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Image,
    Audio,
    Video,
    File,
}

impl MediaKind {
    /// The contract node an embed of this kind becomes.
    pub fn node(self) -> &'static str {
        match self {
            MediaKind::Image => "image",
            MediaKind::Audio => "audio",
            MediaKind::Video => "video",
            MediaKind::File => "file",
        }
    }
}

/// A file name's or URL path's extension, lowercase: what follows the last
/// `.` of its last segment, a query or fragment left out. Empty when it has
/// none.
fn extension(s: &str) -> String {
    let path = s.split(['?', '#']).next().unwrap_or("");
    let last = path.rsplit('/').next().unwrap_or("");
    match last.rsplit_once('.') {
        Some((stem, ext))
            if !stem.is_empty()
                && !ext.is_empty()
                && ext.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            ext.to_ascii_lowercase()
        }
        _ => String::new(),
    }
}

/// What an embed is, read as the browser's `kindOfLink` reads a link, in
/// both page editors: an image extension on the URL or the alt text makes an
/// image, then an audio one audio and a video one video. Drive's URLs carry no extension and its
/// embeds name the file in the alt text, so the alt decides for them. Past
/// that, a web URL with no extension is an image (image hosts serve them
/// so), and anything else, a Drive file named without one included, is a
/// file.
pub fn media_kind(src: &str, alt: &str) -> MediaKind {
    let (on_src, on_alt) = (extension(src), extension(alt));
    let has = |list: &[&str]| list.contains(&on_src.as_str()) || list.contains(&on_alt.as_str());
    if has(IMAGE_EXT) {
        return MediaKind::Image;
    }
    if has(AUDIO_EXT) {
        return MediaKind::Audio;
    }
    if has(VIDEO_EXT) {
        return MediaKind::Video;
    }
    let web = matches!(
        url_scheme(&normalize_url(src)).as_deref(),
        Some("http" | "https")
    );
    if web && on_src.is_empty() {
        MediaKind::Image
    } else {
        MediaKind::File
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn plain_text(events: &[Event]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Text(t) | Event::Code(t) => Some(t.to_string()),
            _ => None,
        })
        .collect()
}

/// `==highlight==` inside one text run, as `<mark>`; text without it as is.
fn highlight<'a>(t: CowStr<'a>, out: &mut Vec<Event<'a>>) {
    if !t.contains("==") {
        out.push(Event::Text(t));
        return;
    }
    let s = t.to_string();
    let mut rest = s.as_str();
    while let Some(start) = rest.find("==") {
        let after = &rest[start + 2..];
        match after.find("==") {
            Some(end)
                if end > 0 && !after[..end].starts_with(' ') && !after[..end].ends_with(' ') =>
            {
                out.push(Event::Text(CowStr::from(rest[..start].to_string())));
                out.push(Event::InlineHtml(CowStr::from("<mark>")));
                out.push(Event::Text(CowStr::from(after[..end].to_string())));
                out.push(Event::InlineHtml(CowStr::from("</mark>")));
                rest = &after[end + 2..];
            }
            _ => break,
        }
    }
    out.push(Event::Text(CowStr::from(rest.to_string())));
}

/// Whether the last `|` in an image's alt text was written as a `|`, not
/// as a character reference (`&#124;`, which the export writes for a pipe
/// that is part of the alt): only a `|` starts a width. `events` are the
/// alt's, `ranges` where each is in `md`. A backslash before it does not
/// count, because a table cell escapes every pipe in it that way, a width's
/// included.
fn last_pipe_is_bare(md: &str, events: &[Event], ranges: &[Range<usize>]) -> bool {
    for (e, r) in events.iter().zip(ranges).rev() {
        let Event::Text(t) = e else { continue };
        if t.contains('|') {
            // Text that differs from its source came through a reference.
            return md.get(r.clone()) == Some(t.as_ref());
        }
    }
    false
}

/// A list item that opens with a bare `[ ]` or `[x]` and holds nothing
/// more in its first block: a task with no text yet. GFM reads the marker
/// only when text follows it, so pulldown-cmark leaves `- [ ]` as the text
/// "[ ]", and drops `- [ ] ` (with its space) to an empty item. The page's
/// own export writes an empty task as `- [ ]`, so both are read here as
/// task markers. Only the marker as written counts: `\[ \]` is text.
fn bare_task_markers<'a>(
    md: &str,
    events: Vec<(Event<'a>, Range<usize>)>,
) -> Vec<(Event<'a>, Range<usize>)> {
    fn marker(s: &str) -> Option<bool> {
        match s {
            "[ ]" => Some(false),
            "[x]" | "[X]" => Some(true),
            _ => None,
        }
    }
    let mut out = Vec::with_capacity(events.len());
    let mut i = 0;
    while i < events.len() {
        let (ev, range) = &events[i];
        out.push((ev.clone(), range.clone()));
        i += 1;
        if !matches!(ev, Event::Start(Tag::Item)) {
            continue;
        }
        // `- [ ] `, which the parser empties: read the item's source.
        if matches!(events.get(i), Some((Event::End(TagEnd::Item), _))) {
            let src = md.get(range.clone()).unwrap_or("").trim_start();
            let after_bullet = src
                .strip_prefix(['-', '*', '+'])
                .or_else(|| {
                    let digits =
                        src.len() - src.trim_start_matches(|c: char| c.is_ascii_digit()).len();
                    (digits > 0)
                        .then(|| src[digits..].strip_prefix(['.', ')']))
                        .flatten()
                })
                .unwrap_or("");
            if let Some(checked) = marker(after_bullet.trim()) {
                out.push((Event::TaskListMarker(checked), range.clone()));
            }
            continue;
        }
        // `- [ ]`, read as text: the item's first block holds only it.
        let start = match events.get(i) {
            Some((Event::Start(Tag::Paragraph), _)) => i + 1,
            _ => i,
        };
        let mut end = start;
        while matches!(events.get(end), Some((Event::Text(_), _))) {
            end += 1;
        }
        let (Some(first), Some(last)) = (
            events.get(start),
            end.checked_sub(1).and_then(|e| events.get(e)),
        ) else {
            continue;
        };
        if end == start {
            continue;
        }
        let ends_block = matches!(
            events.get(end),
            Some((
                Event::End(TagEnd::Item | TagEnd::Paragraph) | Event::Start(_),
                _
            ))
        );
        let written = md.get(first.1.start..last.1.end).and_then(marker);
        if let (true, Some(checked)) = (ends_block, written) {
            if start > i {
                out.push(events[i].clone());
            }
            out.push((Event::TaskListMarker(checked), first.1.start..last.1.end));
            i = end;
        }
    }
    out
}

/// "an audio", "a video", "a file": for notes.
fn article(kind: MediaKind) -> String {
    match kind {
        MediaKind::Audio | MediaKind::Image => format!("an {}", kind.node()),
        _ => format!("a {}", kind.node()),
    }
}

type Events<'a> = Vec<(Event<'a>, Range<usize>)>;

/// Every HTML element a page's markdown may carry as markup: what ingest
/// reads, unwraps or drops as HTML. A raw tag named otherwise is not HTML.
const HTML_ELEMENTS: &[&str] = &[
    "a", "abbr", "address", "area", "article", "aside", "audio", "b", "base", "bdi", "bdo",
    "blockquote", "body", "br", "button", "canvas", "caption", "center", "cite", "code", "col",
    "colgroup", "data", "datalist", "dd", "del", "details", "dfn", "dialog", "div", "dl", "dt",
    "em", "embed", "fieldset", "figcaption", "figure", "font", "footer", "form", "h1", "h2",
    "h3", "h4", "h5", "h6", "head", "header", "hgroup", "hr", "html", "i", "iframe", "img",
    "input", "ins", "kbd", "label", "legend", "li", "link", "main", "map", "mark", "menu",
    "meta", "meter", "nav", "noscript", "object", "ol", "optgroup", "option", "output", "p",
    "param", "picture", "pre", "progress", "q", "rp", "rt", "ruby", "s", "samp", "script",
    "search", "section", "select", "slot", "small", "source", "span", "strike", "strong",
    "style", "sub", "summary", "sup", "svg", "table", "tbody", "td", "template", "textarea",
    "tfoot", "th", "thead", "time", "title", "tr", "track", "tt", "u", "ul", "var", "video",
    "wbr",
];

/// The tag names a raw HTML run opens or closes, as written.
fn tag_names(html: &str) -> impl Iterator<Item = &str> {
    let b = html.as_bytes();
    let mut i = 0;
    std::iter::from_fn(move || {
        while i < b.len() {
            if b[i] == b'<' {
                let mut start = i + 1;
                if start < b.len() && b[start] == b'/' {
                    start += 1;
                }
                let mut end = start;
                while end < b.len() && (b[end].is_ascii_alphanumeric() || b[end] == b'-') {
                    end += 1;
                }
                i = end.max(i + 1);
                if end > start && b[start].is_ascii_alphabetic() {
                    return Some(&html[start..end]);
                }
            } else {
                i += 1;
            }
        }
        None
    })
}

/// Whether a raw tag named `name` is markup: a lowercase HTML element or a
/// tag of the contract. `<Login/>` and `Vec<String>` are text.
fn is_markup_tag(name: &str) -> bool {
    if name.bytes().any(|b| b.is_ascii_uppercase()) {
        return false;
    }
    HTML_ELEMENTS.contains(&name)
        || !crate::contract().node_rules_for_tag(name).is_empty()
        || crate::contract().mark_rule_for_tag(name).is_some()
}

/// Raw HTML in a paste that names a tag no page reads, as the text it is:
/// an inline run as text, an HTML block as a paragraph of its lines.
fn literal_tags(events: Events<'_>) -> Events<'_> {
    let markup = |html: &str| tag_names(html).all(is_markup_tag);
    let mut out: Events = Vec::with_capacity(events.len());
    // An HTML block being read: where it starts, and its events.
    let mut block: Option<(Range<usize>, Events)> = None;
    for (ev, range) in events {
        match (ev, block.take()) {
            (Event::End(TagEnd::HtmlBlock), Some((start, held))) => {
                let text: String = held
                    .iter()
                    .filter_map(|(e, _)| match e {
                        Event::Html(h) | Event::Text(h) => Some(h.as_ref()),
                        _ => None,
                    })
                    .collect();
                if markup(&text) {
                    out.push((Event::Start(Tag::HtmlBlock), start));
                    out.extend(held);
                    out.push((Event::End(TagEnd::HtmlBlock), range));
                    continue;
                }
                out.push((Event::Start(Tag::Paragraph), start.clone()));
                for (i, line) in text.trim_end_matches('\n').split('\n').enumerate() {
                    if i > 0 {
                        out.push((Event::HardBreak, start.clone()));
                    }
                    out.push((Event::Text(CowStr::from(line.to_string())), start.clone()));
                }
                out.push((Event::End(TagEnd::Paragraph), range));
            }
            (ev, Some((start, mut held))) => {
                held.push((ev, range));
                block = Some((start, held));
            }
            (Event::Start(Tag::HtmlBlock), None) => block = Some((range, vec![])),
            (Event::InlineHtml(h), None) if !markup(&h) => out.push((Event::Text(h), range)),
            (other, None) => out.push((other, range)),
        }
    }
    out
}

/// Bytes to cut from text events, by event, each with the HTML that stands
/// where they were.
type Cuts = HashMap<usize, Vec<(Range<usize>, Option<String>)>>;

/// Whether the character at `at` in `md` was written after a backslash that
/// escapes it: an odd run of backslashes before it. pulldown-cmark leaves an
/// escaping backslash out of every event's range, so an escaped character
/// starts a text event whose range begins just past one.
fn escaped_at(md: &str, at: usize) -> bool {
    md[..at].bytes().rev().take_while(|&b| b == b'\\').count() % 2 == 1
}

/// GitHub's alerts: a blockquote whose first line is `[!NOTE]`, `[!TIP]`,
/// `[!WARNING]`, `[!IMPORTANT]` or `[!CAUTION]` alone, any case, is a
/// callout, the last two as the nearest tone the contract has, noted.
/// pulldown-cmark 0.10 reads them as quotes, so the marker is read from the
/// quote's first text and its source.
fn alerts<'a>(md: &str, events: Events<'a>, notes: &mut Vec<Problem>) -> Events<'a> {
    let mut out: Events<'a> = Vec::with_capacity(events.len());
    // For each quote open, whether it is an alert.
    let mut quotes: Vec<bool> = vec![];
    let mut i = 0;
    while i < events.len() {
        let (ev, range) = &events[i];
        match ev {
            Event::Start(Tag::BlockQuote) => match alert_at(md, &events, i) {
                Some((kind, tone, kept_para, resume)) => {
                    if !matches!(kind.as_str(), "NOTE" | "TIP" | "WARNING") {
                        notes.push(Problem::new(
                            "markdown",
                            format!("a [!{kind}] alert became a callout of tone `{tone}`"),
                        ));
                    }
                    out.push((
                        Event::Html(CowStr::from(format!("<aside data-tone=\"{tone}\">"))),
                        range.clone(),
                    ));
                    if kept_para {
                        out.push(events[i + 1].clone());
                    }
                    quotes.push(true);
                    i = resume;
                    continue;
                }
                None => quotes.push(false),
            },
            Event::End(TagEnd::BlockQuote) if quotes.pop() == Some(true) => {
                out.push((Event::Html(CowStr::from("</aside>\n")), range.clone()));
                i += 1;
                continue;
            }
            _ => {}
        }
        out.push(events[i].clone());
        i += 1;
    }
    out
}

/// The kind (uppercase) and tone of the alert whose quote starts at
/// `events[i]`, whether the quote's first paragraph goes on past the
/// marker's line (and is kept), and the event to go on from: the one after
/// the marker's line.
fn alert_at(md: &str, events: &Events, i: usize) -> Option<(String, &'static str, bool, usize)> {
    if !matches!(events.get(i + 1), Some((Event::Start(Tag::Paragraph), _))) {
        return None;
    }
    let first = i + 2;
    let mut end = first;
    let mut text = String::new();
    while let Some((Event::Text(t), _)) = events.get(end) {
        text.push_str(t);
        end += 1;
        if text.len() > "[!IMPORTANT]".len() {
            return None;
        }
    }
    if end == first {
        return None;
    }
    let kept_para = match events.get(end) {
        Some((Event::SoftBreak, _)) => true,
        Some((Event::End(TagEnd::Paragraph), _)) => false,
        _ => return None,
    };
    let (from, to) = (events[first].1.start, events[end - 1].1.end);
    // As written: no escape, entity or reference in it.
    if md.get(from..to) != Some(text.as_str()) || escaped_at(md, from) {
        return None;
    }
    let kind = text
        .strip_prefix("[!")?
        .strip_suffix(']')?
        .to_ascii_uppercase();
    let tone = match kind.as_str() {
        "NOTE" | "IMPORTANT" => "note",
        "TIP" => "tip",
        "WARNING" | "CAUTION" => "warning",
        _ => return None,
    };
    Some((kind, tone, kept_para, end + 1))
}

/// One unit of a block's inline content, as the CriticMarkup pass reads it.
/// Offsets fit in 32 bits: one call reads at most [`MAX_INPUT_BYTES`].
enum Unit {
    /// A character CriticMarkup's delimiters are made of ([`CRITIC`], each
    /// one byte): the text event, where in its text, whether it was written
    /// as itself (not through an escape or a character reference), and
    /// where it is in the source.
    Ch {
        ev: u32,
        at: u32,
        ch: u8,
        bare: bool,
        src: u32,
    },
    /// Formatting or a link opening; `strike` for a strikethrough.
    Open {
        strike: bool,
    },
    Close,
    /// Anything else inline: other text, a code span, a break, an image,
    /// inline HTML. A run of them is one.
    Leaf,
}

/// The characters CriticMarkup's delimiters are made of.
const CRITIC: &[u8] = b"{}-+<>~";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Critic {
    Deletion,
    Insertion,
    Comment,
}

impl Critic {
    fn closer(self) -> [char; 3] {
        match self {
            Critic::Deletion => ['-', '-', '}'],
            Critic::Insertion => ['+', '+', '}'],
            Critic::Comment => ['<', '<', '}'],
        }
    }

    fn tag(self) -> &'static str {
        match self {
            Critic::Deletion => "virtues-del",
            _ => "virtues-ins",
        }
    }
}

/// What the CriticMarkup pass left as text, counted for one note each.
#[derive(Default)]
struct Kept {
    comments: usize,
    substitutions: usize,
    unclosed: usize,
    misnested: usize,
}

/// CriticMarkup proposals, as the editor wrote them into a page's text:
/// `{--old--}` and `{++new++}` written bare (not escaped) inside one block
/// become proposal marks, and a deletion straight followed by an insertion
/// is one proposal, a change, sharing one id. What they hold is read as
/// markdown, so the escapes the editor wrote into them (`--\}`) read as the
/// text they stand for. A span that crosses blocks, or opens and closes
/// inside different formatting, stays text, and so do comments
/// (`{>>…<<}`) and substitutions (`{~~…~>…~~}`), which a page cannot hold;
/// each is noted.
///
/// Each proposal is rewritten as six cuts and two tags, so the cuts count
/// against [`MAX_EVENTS`], and markdown with more is refused before they
/// are made.
fn critic_markup<'a>(
    md: &str,
    events: Events<'a>,
    notes: &mut Vec<Problem>,
) -> Result<Events<'a>, Problem> {
    let mut cuts = Cuts::new();
    let mut cut_count = 0;
    let mut kept = Kept::default();
    let mut units: Vec<Unit> = vec![];
    // Inside an image's alt text, a code block or an HTML block: how deep,
    // and whether it is an image (a leaf in its block).
    let mut skip = (0usize, false);
    for (i, (ev, range)) in events.iter().enumerate() {
        if skip.0 > 0 {
            match ev {
                Event::Start(_) => skip.0 += 1,
                Event::End(_) => skip.0 -= 1,
                _ => {}
            }
            if skip.0 == 0 && skip.1 {
                units.push(Unit::Leaf);
            }
            continue;
        }
        match ev {
            Event::Start(Tag::Image { .. }) => skip = (1, true),
            Event::Start(Tag::CodeBlock(_) | Tag::HtmlBlock) => {
                cut_count += proposals_in_block(&units, &mut cuts, &mut kept, MAX_EVENTS - cut_count)
                    .ok_or_else(too_complex)?;
                units.clear();
                skip = (1, false);
            }
            Event::Start(Tag::Emphasis | Tag::Strong | Tag::Link { .. }) => {
                units.push(Unit::Open { strike: false })
            }
            Event::Start(Tag::Strikethrough) => units.push(Unit::Open { strike: true }),
            Event::End(
                TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link,
            ) => units.push(Unit::Close),
            Event::Start(_) | Event::End(_) => {
                cut_count += proposals_in_block(&units, &mut cuts, &mut kept, MAX_EVENTS - cut_count)
                    .ok_or_else(too_complex)?;
                units.clear();
            }
            Event::Text(t) => {
                let literal = md.get(range.clone()) == Some(t.as_ref());
                let escaped = literal && escaped_at(md, range.start);
                for (at, b) in t.bytes().enumerate() {
                    if CRITIC.contains(&b) {
                        units.push(Unit::Ch {
                            ev: i as u32,
                            at: at as u32,
                            ch: b,
                            bare: literal && !(at == 0 && escaped),
                            src: (range.start + at) as u32,
                        });
                    } else if !matches!(units.last(), Some(Unit::Leaf)) {
                        units.push(Unit::Leaf);
                    }
                }
            }
            _ if matches!(units.last(), Some(Unit::Leaf)) => {}
            _ => units.push(Unit::Leaf),
        }
    }
    proposals_in_block(&units, &mut cuts, &mut kept, MAX_EVENTS - cut_count)
        .ok_or_else(too_complex)?;

    let count = |n: usize, one: &str, many: &str| match n {
        1 => one.to_string(),
        n => many.replace("{n}", &n.to_string()),
    };
    if kept.comments > 0 {
        notes.push(Problem::new(
            "markdown",
            count(
                kept.comments,
                "kept a comment {>>…<<} as text: a page holds no comments",
                "kept {n} comments {>>…<<} as text: a page holds no comments",
            ),
        ));
    }
    if kept.substitutions > 0 {
        notes.push(Problem::new(
            "markdown",
            count(
                kept.substitutions,
                "kept a substitution {~~…~>…~~} as text: a page proposes a change as {--old--}{++new++}",
                "kept {n} substitutions {~~…~>…~~} as text: a page proposes a change as {--old--}{++new++}",
            ),
        ));
    }
    if kept.unclosed > 0 {
        notes.push(Problem::new(
            "markdown",
            count(
                kept.unclosed,
                "kept a proposal's {-- or {++ as text: a proposal ends in the block it starts in",
                "kept {n} proposals' {-- or {++ as text: a proposal ends in the block it starts in",
            ),
        ));
    }
    if kept.misnested > 0 {
        notes.push(Problem::new(
            "markdown",
            count(
                kept.misnested,
                "kept a proposal as text: it opens and closes inside different formatting",
                "kept {n} proposals as text: each opens and closes inside different formatting",
            ),
        ));
    }
    if cuts.is_empty() {
        return Ok(events);
    }

    let mut out: Events<'a> = Vec::with_capacity(events.len() + cuts.len() * 2);
    for (i, (ev, range)) in events.into_iter().enumerate() {
        let (Some(mut cs), Event::Text(t)) = (cuts.remove(&i), &ev) else {
            out.push((ev, range));
            continue;
        };
        cs.sort_by_key(|(bytes, _)| bytes.start);
        let piece = |from: usize, to: usize| range.start + from..range.start + to;
        let mut at = 0;
        for (bytes, html) in cs {
            if bytes.start > at {
                out.push((
                    Event::Text(CowStr::from(t[at..bytes.start].to_string())),
                    piece(at, bytes.start),
                ));
            }
            if let Some(html) = html {
                out.push((
                    Event::InlineHtml(CowStr::from(html)),
                    piece(bytes.start, bytes.end),
                ));
            }
            at = bytes.end;
        }
        if at < t.len() {
            out.push((
                Event::Text(CowStr::from(t[at..].to_string())),
                piece(at, t.len()),
            ));
        }
    }
    Ok(out)
}

/// The proposals in one block's inline content, as cuts: the three
/// characters of each opener and closer cut out, the opening and closing
/// tags standing where they were. Returns how many cuts it made, or `None`,
/// having made none, when that would be more than `budget`.
fn proposals_in_block(
    units: &[Unit],
    cuts: &mut Cuts,
    kept: &mut Kept,
    budget: usize,
) -> Option<usize> {
    // Three characters written as themselves, in a row, from `k`.
    let three = |k: usize| -> Option<[char; 3]> {
        let mut out = ['\0'; 3];
        for (n, slot) in out.iter_mut().enumerate() {
            match units.get(k + n) {
                Some(Unit::Ch { ch, bare: true, .. }) => *slot = char::from(*ch),
                _ => return None,
            }
        }
        Some(out)
    };
    // The formatting open, each by when it opened: a span opens and closes
    // inside the same. Each opening has its own number, so the stack is
    // named by its depth and the number on top, and a span compares those
    // two rather than the whole stack, which would cost depth times spans.
    let mut open: Vec<usize> = vec![];
    let mut opened = 0usize;
    let mut pending: Option<(Critic, usize, (usize, Option<usize>))> = None;
    let mut spans: Vec<(Critic, usize, usize)> = vec![];
    let mut k = 0;
    while k < units.len() {
        match &units[k] {
            Unit::Open { .. } => {
                opened += 1;
                open.push(opened);
            }
            Unit::Close => {
                open.pop();
            }
            Unit::Leaf => {}
            Unit::Ch { ch, bare, .. } => match &pending {
                None => {
                    let kind = match three(k) {
                        Some(['{', '-', '-']) => Some(Critic::Deletion),
                        Some(['{', '+', '+']) => Some(Critic::Insertion),
                        Some(['{', '>', '>']) => Some(Critic::Comment),
                        _ => None,
                    };
                    if let Some(kind) = kind {
                        pending = Some((kind, k, (open.len(), open.last().copied())));
                        k += 3;
                        continue;
                    }
                    let strike_after =
                        matches!(units.get(k + 1), Some(Unit::Open { strike: true }));
                    if *bare && *ch == b'{' && (three(k) == Some(['{', '~', '~']) || strike_after) {
                        kept.substitutions += 1;
                    }
                }
                Some((kind, from, at)) => {
                    if three(k) == Some(kind.closer()) {
                        match kind {
                            Critic::Comment => kept.comments += 1,
                            _ if *at == (open.len(), open.last().copied()) => {
                                spans.push((*kind, *from, k))
                            }
                            _ => kept.misnested += 1,
                        }
                        pending = None;
                        k += 3;
                        continue;
                    }
                }
            },
        }
        k += 1;
    }
    if let Some((kind, ..)) = pending {
        if kind != Critic::Comment {
            kept.unclosed += 1;
        }
    }

    let src = |k: usize| match &units[k] {
        Unit::Ch { src, .. } => *src,
        _ => u32::MAX,
    };
    let mut cut = |k: usize, html: Option<String>| {
        if let Unit::Ch { ev, at, .. } = &units[k] {
            let at = *at as usize;
            cuts.entry(*ev as usize)
                .or_default()
                .push((at..at + 1, html));
        }
    };
    let made = spans.len() * 6;
    if made > budget {
        return None;
    }
    let mut last: Option<(Critic, u32, String)> = None;
    for (kind, from, to) in spans {
        // A change: a deletion and the insertion written straight after it.
        let id = match &last {
            Some((Critic::Deletion, end, id))
                if kind == Critic::Insertion && src(from) == end.saturating_add(1) =>
            {
                id.clone()
            }
            _ => new_id(),
        };
        let tag = kind.tag();
        cut(from, Some(format!("<{tag} proposal=\"{id}\">")));
        cut(from + 1, None);
        cut(from + 2, None);
        cut(to, None);
        cut(to + 1, None);
        cut(to + 2, Some(format!("</{tag}>")));
        last = Some((kind, src(to + 2), id));
    }
    Some(made)
}

/// The most parser events one conversion holds. Markdown can make an event
/// for every few bytes (`*a ` repeated makes one for every three), and each
/// is held several times over on the way to HTML and then in the tree, so a
/// conversion is bounded by its events as well as its input: refused, a
/// conversion holds about 250 MB at most, and taken, about 550 MB. Formatted
/// prose makes about one event for every nine bytes, so its HTML passes
/// [`MAX_INPUT_BYTES`] first; a table of short cells makes about one for
/// every two, so about 800 KB of one is the most that converts.
pub const MAX_EVENTS: usize = 1 << 19;

/// Refusing markdown past [`MAX_EVENTS`].
pub fn too_complex() -> Problem {
    Problem::new(
        "markdown",
        format!(
            "the markdown makes more than {MAX_EVENTS} parts to convert, as a table with many \
             columns does; convert it in smaller pieces"
        ),
    )
}

/// Refusing markdown whose HTML passes [`MAX_INPUT_BYTES`].
pub fn too_much_html() -> Problem {
    Problem::new(
        "markdown",
        format!(
            "the markdown makes more than {} MiB of HTML; one write holds at most that",
            MAX_INPUT_BYTES >> 20
        ),
    )
}

/// Whether a refusal is for more markdown than one conversion holds
/// ([`too_complex`], [`too_much_html`]), not for what it says: the answer
/// to it is less markdown at a time.
pub fn past_one_conversion(problem: &Problem) -> bool {
    *problem == too_complex() || *problem == too_much_html()
}

/// Front matter: `---` on the first line, then lines up to the next `---`
/// or `...`, as YAML front matter is written. Where the markdown after it
/// starts, or `None` when it has none. Only at the very start: anywhere
/// else those lines are a rule and text.
fn front_matter_end(md: &str) -> Option<usize> {
    fn line_end(s: &str, from: usize) -> usize {
        s[from..].find('\n').map_or(s.len(), |i| from + i + 1)
    }
    let first = line_end(md, 0);
    if md[..first].trim_end() != "---" {
        return None;
    }
    let mut at = first;
    let mut first_line = true;
    while at < md.len() {
        let end = line_end(md, at);
        let line = md[at..end].trim_end_matches(['\n', '\r']);
        let closes = matches!(line.trim_end_matches(' '), "---" | "...");
        // The first line holds something, or this is a rule, not front matter.
        if first_line && (closes || line.trim().is_empty()) {
            return None;
        }
        if closes {
            return Some(end);
        }
        first_line = false;
        at = end;
    }
    None
}

/// Writes HTML up to a limit, then refuses the rest, so a page whose HTML
/// would pass the limit is not rendered whole first.
struct Capped {
    html: Vec<u8>,
    limit: usize,
}

impl std::io::Write for Capped {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.html.len() + buf.len() > self.limit {
            return Err(std::io::Error::other("past the limit"));
        }
        self.html.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// How markdown is read: as written for a page (a writer's markdown, an
/// existing page), or as a person's paste.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// Our dialect in full: raw HTML is markup, CriticMarkup a proposal.
    Written,
    /// Text a person copied. A raw tag that is no HTML element and no tag
    /// of the contract (`<Login/>`, `Vec<String>`, `<placeholder>`) is the
    /// text it reads as, not a tag to unwrap, and so is CriticMarkup, which
    /// a paste cannot make into a suggestion: nothing written is lost.
    Paste,
}

/// Our markdown → HTML, with the dialect mapped onto contract tags. Returns
/// the HTML and notes for dialect features the contract has no node for, or
/// the refusal of markdown that makes more than [`MAX_EVENTS`] events or
/// HTML past [`MAX_INPUT_BYTES`], which ingest would refuse.
pub fn to_html(md: &str) -> Result<(String, Vec<Problem>), Problem> {
    to_html_as(md, Dialect::Written)
}

/// [`to_html`], read as `dialect`.
pub fn to_html_as(md: &str, dialect: Dialect) -> Result<(String, Vec<Problem>), Problem> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    let mut notes = vec![];
    // Read here, not by the parser: its own reading of front matter never
    // returns on some markdown that only looks like it (an indented `---`
    // with a `---` line further on).
    let md = match front_matter_end(md) {
        Some(end) => {
            notes.push(Problem::new(
                "markdown",
                "front matter is not document content; left out",
            ));
            &md[end..]
        }
        None => md,
    };
    let mut out: Vec<Event> = vec![];
    let mut in_code = 0;
    // A link or image being collected until its end, to rewrite it whole,
    // with where each of its events is in the source.
    let mut buffer: Option<(Tag, Vec<Event>, Vec<Range<usize>>)> = None;

    let mut parsed = vec![];
    for event in Parser::new_ext(md, opts).into_offset_iter() {
        if parsed.len() == MAX_EVENTS {
            return Err(too_complex());
        }
        parsed.push(event);
    }
    let parsed = match dialect {
        Dialect::Written => parsed,
        Dialect::Paste => literal_tags(parsed),
    };
    let events = bare_task_markers(md, parsed);
    let events = alerts(md, events, &mut notes);
    let events = match dialect {
        Dialect::Written => critic_markup(md, events, &mut notes)?,
        Dialect::Paste => events,
    };
    for (ev, range) in events {
        if let Some((tag, buf, ranges)) = buffer.as_mut() {
            match (&ev, &*tag) {
                (
                    Event::End(TagEnd::Link),
                    Tag::Link {
                        dest_url, title, ..
                    },
                ) => {
                    let text = plain_text(buf);
                    let plain = buf.iter().all(|e| matches!(e, Event::Text(_)));
                    if text.starts_with('@') {
                        if is_route(dest_url) && plain {
                            out.push(Event::InlineHtml(CowStr::from(format!(
                                "<virtues-mention to=\"{}\" label=\"{}\"></virtues-mention>",
                                esc(dest_url),
                                esc(text.trim_start_matches('@'))
                            ))));
                            buffer = None;
                            continue;
                        }
                        let why = if plain {
                            format!("`{dest_url}` is not a ref")
                        } else {
                            "its label carries formatting".to_string()
                        };
                        notes.push(Problem::new(
                            "markdown",
                            format!("kept [{text}]({dest_url}) as a link, not a mention: {why}"),
                        ));
                    }
                    // The link as HTML written here, not by pulldown-cmark,
                    // which percent-encodes the URL: `"` would come back as
                    // `%22`, and the URL would not be the one written.
                    if !title.is_empty() {
                        notes.push(Problem::new(
                            "markdown",
                            format!("dropped the title \"{title}\" of a link to {dest_url}"),
                        ));
                    }
                    out.push(Event::InlineHtml(CowStr::from(format!(
                        "<a href=\"{}\">",
                        esc(dest_url)
                    ))));
                    if let Some((_, buf, _)) = buffer.take() {
                        for e in buf {
                            match e {
                                Event::Text(t) => highlight(t, &mut out),
                                other => out.push(other),
                            }
                        }
                    }
                    out.push(Event::InlineHtml(CowStr::from("</a>")));
                    continue;
                }
                (Event::End(TagEnd::Image), Tag::Image { dest_url, .. }) => {
                    let alt = plain_text(buf);
                    let plain = buf.iter().all(|e| matches!(e, Event::Text(_)));
                    // `![@Label](/kind/id)`: a ref embedded on a line of its
                    // own, which the editor drew as the ref.
                    if alt.starts_with('@') && is_route(dest_url) && plain {
                        out.push(Event::InlineHtml(CowStr::from(format!(
                            "<virtues-mention to=\"{}\" label=\"{}\"></virtues-mention>",
                            esc(dest_url),
                            esc(alt.trim_start_matches('@'))
                        ))));
                        buffer = None;
                        continue;
                    }
                    // `![alt|600](…)`: the last `|` starts a width, unless it
                    // was written as `&#124;`, which keeps it in the alt.
                    let (alt, width) = match alt.rsplit_once('|') {
                        Some((a, w))
                            if w.trim().parse::<u32>().is_ok()
                                && last_pipe_is_bare(md, buf, ranges) =>
                        {
                            (a.to_string(), Some(w.trim().to_string()))
                        }
                        _ => (alt, None),
                    };
                    let kind = media_kind(dest_url, &alt);
                    let html = if kind == MediaKind::Image {
                        let w = width.map(|w| format!(" width=\"{w}\"")).unwrap_or_default();
                        let alt_attr = if alt.is_empty() {
                            String::new()
                        } else {
                            format!(" alt=\"{}\"", esc(&alt))
                        };
                        format!("<img src=\"{}\"{alt_attr}{w}>", esc(dest_url))
                    } else {
                        if let Some(w) = width {
                            notes.push(Problem::new(
                                "markdown",
                                format!(
                                    "dropped the width {w} of {} embed, which has none",
                                    article(kind)
                                ),
                            ));
                        }
                        let tag = match kind {
                            MediaKind::Audio => "audio",
                            MediaKind::Video => "video",
                            _ => "virtues-file",
                        };
                        let name = if alt.is_empty() {
                            String::new()
                        } else {
                            format!(" data-name=\"{}\"", esc(&alt))
                        };
                        format!("<{tag} src=\"{}\"{name}></{tag}>", esc(dest_url))
                    };
                    out.push(Event::InlineHtml(CowStr::from(html)));
                    buffer = None;
                    continue;
                }
                _ => {
                    buf.push(ev);
                    ranges.push(range);
                    continue;
                }
            }
        }
        match ev {
            Event::Start(Tag::CodeBlock(k)) => {
                in_code += 1;
                out.push(Event::Start(Tag::CodeBlock(k)));
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code -= 1;
                out.push(Event::End(TagEnd::CodeBlock));
            }
            Event::Start(
                t @ Tag::Link {
                    link_type:
                        LinkType::Inline
                        | LinkType::Reference
                        | LinkType::Collapsed
                        | LinkType::Shortcut,
                    ..
                },
            ) => {
                buffer = Some((t, vec![], vec![]));
            }
            Event::Start(t @ Tag::Image { .. }) => buffer = Some((t, vec![], vec![])),
            Event::Text(t) if in_code == 0 => highlight(t, &mut out),
            other => out.push(other),
        }
    }
    if out.len() > MAX_EVENTS {
        return Err(too_complex());
    }
    let mut html = Capped {
        html: vec![],
        limit: MAX_INPUT_BYTES,
    };
    if write_html(&mut html, out.into_iter()).is_err() {
        return Err(too_much_html());
    }
    let html = String::from_utf8(html.html).map_err(|e| Problem::new("markdown", e.to_string()))?;
    Ok((html, notes))
}

/// Our markdown as a document tree, in migration mode: what the contract
/// cannot hold is unwrapped or dropped and noted, never silently. The nodes
/// carry no block ids, not even one the markdown's HTML wrote; writing them
/// to a document gives them ids ([`crate::write_nodes`]).
pub fn from_markdown(c: &Contract, md: &str) -> Outcome {
    from_markdown_as(c, md, Dialect::Written)
}

/// [`from_markdown`], read as `dialect`.
pub fn from_markdown_as(c: &Contract, md: &str, dialect: Dialect) -> Outcome {
    if md.len() > MAX_INPUT_BYTES {
        return Outcome {
            errors: vec![Problem::new(
                "markdown",
                format!(
                    "the markdown is {} bytes; one write holds at most {} MiB",
                    md.len(),
                    MAX_INPUT_BYTES >> 20
                ),
            )],
            ..Outcome::default()
        };
    }
    let (html, mut notes) = match to_html_as(md, dialect) {
        Ok(converted) => converted,
        Err(refused) => {
            return Outcome {
                errors: vec![refused],
                ..Outcome::default()
            }
        }
    };
    let mut o = parse(c, &html, &c.fragment, Mode::Migrate);
    notes.append(&mut o.notes);
    if o.errors.is_empty() {
        drop_empty_header_rows(&mut o.nodes, &mut notes);
    }
    o.notes = notes;
    o
}

/// GFM has no table without a header row, so markdown writes an empty one
/// for a table that has none (the export does, and people do by hand). A
/// first row of header cells holding nothing is that stand-in, and is
/// dropped, with a note.
fn drop_empty_header_rows(nodes: &mut [Node], notes: &mut Vec<Problem>) {
    for n in nodes {
        if n.kind == "table" && n.content.len() > 1 {
            let stand_in = n.content[0].content.iter().all(|cell| {
                cell.kind == "tableHeader"
                    && cell
                        .content
                        .iter()
                        .all(|b| b.kind == "paragraph" && b.content.is_empty())
            });
            if stand_in && !n.content[0].content.is_empty() {
                n.content.remove(0);
                notes.push(Problem::new(
                    "markdown",
                    "dropped a table's empty header row, which markdown needs and the page does not",
                ));
            }
        }
        drop_empty_header_rows(&mut n.content, notes);
    }
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Visible text of the HTML the markdown renders to.
fn html_text(html: &str) -> String {
    let doc = scraper::Html::parse_fragment(html);
    let mut s = String::new();
    for n in doc.root_element().descendants() {
        match n.value() {
            scraper::Node::Text(t) => {
                s.push_str(t);
                s.push(' ');
            }
            scraper::Node::Element(e) if e.name() == "virtues-mention" => {
                s.push('@');
                s.push_str(e.attr("label").unwrap_or(""));
                s.push(' ');
            }
            _ => {}
        }
    }
    collapse(&s)
}

fn tree_text(nodes: &[Node]) -> String {
    let mut s = String::new();
    for n in nodes {
        n.walk(&mut |n| {
            if let Some(t) = &n.text {
                s.push_str(t);
                s.push(' ');
            } else if n.kind == "mention" {
                s.push('@');
                s.push_str(n.attrs.get("label").and_then(|v| v.as_str()).unwrap_or(""));
                s.push(' ');
            } else if n.kind == "hardBreak" {
                s.push(' ');
            }
        });
        s.push(' ');
    }
    collapse(&s)
}

fn strip_ids(c: &Contract, nodes: &mut [Node]) {
    for n in nodes {
        n.walk_mut(&mut |n| {
            n.attrs.remove(&c.id.attr);
        });
    }
}

/// Proposal ids renumbered `1`, `2`, … by first appearance: what the
/// markdown export keeps of them. CriticMarkup carries no id, so the
/// converter gives each proposal a new one; which runs share one is what
/// reads back.
pub fn number_proposals(nodes: &mut [Node]) {
    let mut seen: HashMap<String, String> = HashMap::new();
    for n in nodes {
        n.walk_mut(&mut |n| {
            for m in &mut n.marks {
                if !matches!(m.kind.as_str(), "proposedDeletion" | "proposedInsertion") {
                    continue;
                }
                if let Some(serde_json::Value::String(id)) = m.attrs.get_mut("proposal") {
                    let next = (seen.len() + 1).to_string();
                    *id = seen.entry(id.clone()).or_insert(next).clone();
                }
            }
        });
    }
}

/// First place two trees differ, as a short path.
fn first_diff(a: &[Node], b: &[Node], path: &str) -> Option<String> {
    for i in 0..a.len().max(b.len()) {
        match (a.get(i), b.get(i)) {
            (Some(x), Some(y)) if x == y => continue,
            (Some(x), Some(y)) => {
                let here = format!("{path}/{}", x.kind);
                if x.kind != y.kind {
                    return Some(format!("{here} became {}", y.kind));
                }
                if x.attrs != y.attrs {
                    return Some(format!(
                        "{here} attrs {} → {}",
                        serde_json::Value::Object(x.attrs.clone()),
                        serde_json::Value::Object(y.attrs.clone())
                    ));
                }
                if x.text != y.text || x.marks != y.marks {
                    return Some(format!(
                        "{here} text {:?}{:?} → {:?}{:?}",
                        x.text,
                        x.marks.iter().map(|m| &m.kind).collect::<Vec<_>>(),
                        y.text,
                        y.marks.iter().map(|m| &m.kind).collect::<Vec<_>>()
                    ));
                }
                return first_diff(&x.content, &y.content, &here)
                    .or(Some(format!("{here} content length")));
            }
            (Some(x), None) => return Some(format!("{path}/{} missing after export", x.kind)),
            (None, Some(y)) => return Some(format!("{path}/{} added by export", y.kind)),
            (None, None) => {}
        }
    }
    None
}

/// A note's kind, for counting notes across documents.
pub fn category(msg: &str) -> String {
    if let Some(i) = msg.find("; got `") {
        return format!(
            "attribute dropped: {}",
            msg[i + 7..].split('`').next().unwrap_or("")
        );
    }
    if msg.starts_with("dropped style") {
        return "style dropped".to_string();
    }
    if let Some(rest) = msg.strip_prefix("dropped the ") {
        if let Some((mark, _)) = rest.split_once(':') {
            return format!("{mark} dropped");
        }
    }
    if msg.starts_with("unwrapped <") || msg.starts_with("dropped <") || msg.starts_with("moved <")
    {
        return msg.split('>').next().unwrap_or(msg).to_string() + ">";
    }
    if msg.contains("cannot be combined") {
        return format!("mark clash: {}", msg.split(';').next().unwrap_or(msg));
    }
    if msg.ends_with("more notes left out") {
        return "notes left out".to_string();
    }
    msg.to_string()
}

/// Where the tree's text first departs from the text the markdown renders to.
#[derive(Debug, Clone, Serialize)]
pub struct TextLoss {
    pub at_char: usize,
    pub html_text_len: usize,
    pub tree_text_len: usize,
    pub html_around: String,
    pub tree_around: String,
}

/// One document converted and judged.
#[derive(Debug, Serialize)]
pub struct Measured {
    pub outcome: Outcome,
    /// Set when a character of the rendered markdown is missing from the tree.
    pub text_lost: Option<TextLoss>,
    /// The first difference between the tree and its markdown export read
    /// back in; `None` when they are the same tree.
    pub export_diff: Option<String>,
}

/// Convert `md` and check the conversion: no text lost, and the export reads
/// back to the same tree. A refused document is returned with neither check.
pub fn measure(c: &Contract, md: &str) -> Measured {
    let outcome = from_markdown(c, md);
    if !outcome.errors.is_empty() {
        return Measured {
            outcome,
            text_lost: None,
            export_diff: None,
        };
    }
    // The same markdown converted just now, so it renders the same.
    let Ok((html, _)) = to_html(md) else {
        return Measured {
            outcome,
            text_lost: None,
            export_diff: None,
        };
    };
    let before = html_text(&html);
    let after = tree_text(&outcome.nodes);
    // Spacing between inline runs is the measure's own noise; a lost
    // character is not.
    let squeeze = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let text_lost = (squeeze(&before) != squeeze(&after)).then(|| {
        let common = before
            .chars()
            .zip(after.chars())
            .take_while(|(a, b)| a == b)
            .count();
        let around = |t: &str| {
            t.chars()
                .skip(common.saturating_sub(40))
                .take(80)
                .collect::<String>()
        };
        TextLoss {
            at_char: common,
            html_text_len: before.chars().count(),
            tree_text_len: after.chars().count(),
            html_around: around(&before),
            tree_around: around(&after),
        }
    });
    let exported = render::markdown(c, &outcome.nodes);
    let back = from_markdown(c, &exported);
    let mut a = outcome.nodes.clone();
    let mut b = back.nodes.clone();
    strip_ids(c, &mut a);
    strip_ids(c, &mut b);
    number_proposals(&mut a);
    number_proposals(&mut b);
    let export_diff = if back.errors.is_empty() {
        first_diff(&a, &b, "")
    } else {
        Some("export failed to re-ingest".into())
    };
    Measured {
        outcome,
        text_lost,
        export_diff,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    /// A span is matched against the formatting open where it began by that
    /// formatting's depth and newest opening, so a span costs the same
    /// however deep the formatting around it is: the whole stack copied and
    /// compared per span took a minute on a paste under the size limit.
    #[test]
    fn spans_cost_the_same_under_formatting_of_any_depth() {
        const SPANS: usize = 2_000;
        let units = |depth: usize| -> Vec<Unit> {
            let ch = |c: u8| Unit::Ch {
                ev: 0,
                at: 0,
                ch: c,
                bare: true,
                src: 0,
            };
            let mut units: Vec<Unit> = (0..depth).map(|_| Unit::Open { strike: false }).collect();
            for _ in 0..SPANS {
                units.extend(b"{----}".iter().map(|&c| ch(c)));
            }
            units.extend((0..depth).map(|_| Unit::Close));
            units
        };
        let time = |depth: usize| {
            let units = units(depth);
            (0..5)
                .map(|_| {
                    let (mut cuts, mut kept) = (Cuts::new(), Kept::default());
                    let start = std::time::Instant::now();
                    proposals_in_block(&units, &mut cuts, &mut kept, usize::MAX);
                    let elapsed = start.elapsed();
                    assert_eq!(cuts[&0].len(), 6 * SPANS);
                    assert_eq!(kept.misnested, 0);
                    elapsed
                })
                .min()
                .unwrap()
        };
        // A thousand times the depth: a little more time for the openings
        // and closings when a span compares two numbers, a hundred times
        // more or worse when it copies and compares the stack.
        let (s, l) = (time(200), time(200_000));
        let ratio = l.as_secs_f64() / s.as_secs_f64().max(1e-6);
        assert!(
            ratio < 8.0,
            "200 deep {s:?}, 200,000 deep {l:?} ({ratio:.1}x)"
        );
    }

    #[test]
    fn dialect_maps_onto_the_contract() {
        let c = Contract::load();
        let md = "# Title\n\nLunch with [@Nick](/person/p_1), ==important== and <u>under</u>.\n\n\
                  ![A cat|600](/api/drive/files/1/cat.png)\n\n- [x] done\n- [ ] todo\n";
        let o = from_markdown(&c, md);
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        let kinds: Vec<&str> = o.nodes.iter().map(|n| n.kind.as_str()).collect();
        assert_eq!(kinds, ["heading", "paragraph", "image", "taskList"]);
        let p = &o.nodes[1];
        assert!(p
            .content
            .iter()
            .any(|n| n.kind == "mention" && n.attrs["label"] == json!("Nick")));
        assert!(p
            .content
            .iter()
            .any(|n| n.marks.iter().any(|m| m.kind == "highlight")));
        assert!(p
            .content
            .iter()
            .any(|n| n.marks.iter().any(|m| m.kind == "underline")));
        assert_eq!(o.nodes[2].attrs["width"], json!(600));
        assert_eq!(o.nodes[3].content[0].attrs["checked"], json!(true));
    }

    fn mention_targets(o: &Outcome) -> Vec<String> {
        let mut out = vec![];
        for n in &o.nodes {
            n.walk(&mut |n| {
                if n.kind == "mention" {
                    out.push(n.attrs["to"].as_str().unwrap_or("").to_string());
                }
            });
        }
        out
    }

    #[test]
    fn any_ref_the_grammar_reads_is_a_mention() {
        let c = Contract::load();
        let md = "[@Nick](/person/p_1) [@Plan](/project/nb_1) [@Old](/notebook/nb_1) \
                  [@Row](/record/r_1) [@Src](/sources/s_1) [@Page](/page/pg_1?x=1#h)\n";
        let o = from_markdown(&c, md);
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        assert_eq!(o.notes, []);
        assert_eq!(
            mention_targets(&o),
            [
                "/person/p_1",
                "/project/nb_1",
                "/notebook/nb_1",
                "/record/r_1",
                "/sources/s_1",
                "/page/pg_1?x=1#h"
            ]
        );
        // Written back byte for byte.
        let m = measure(&c, md);
        assert_eq!(m.export_diff, None);
        assert_eq!(render::markdown(&c, &m.outcome.nodes), md);
    }

    #[test]
    fn an_at_link_kept_as_a_link_is_noted() {
        let c = Contract::load();
        for md in [
            "[@Nick](https://example.com/nick)\n",
            "[@Nick](/person)\n",
            "[@**Nick**](/person/p_1)\n",
        ] {
            let o = from_markdown(&c, md);
            assert!(o.errors.is_empty(), "{:?}", o.errors);
            assert_eq!(mention_targets(&o), Vec::<String>::new(), "{md}");
            assert!(
                o.notes.iter().any(|n| n.message.contains("not a mention")),
                "{md}: {:?}",
                o.notes
            );
        }
        // A link whose text does not start with `@` is just a link.
        let o = from_markdown(&c, "[Nick](/person/p_1)\n");
        assert_eq!(o.notes, []);
    }

    #[test]
    fn a_highlight_inside_a_link_is_a_highlight() {
        let c = Contract::load();
        let m = measure(&c, "[==x== and *y*](/y)\n");
        assert!(m.outcome.errors.is_empty(), "{:?}", m.outcome.errors);
        let p = &m.outcome.nodes[0];
        assert!(p.content.iter().any(|n| n.text.as_deref() == Some("x")
            && n.marks.iter().any(|m| m.kind == "highlight")
            && n.marks.iter().any(|m| m.kind == "link")));
        assert_eq!(m.export_diff, None);
    }

    #[test]
    fn a_link_keeps_its_url_as_written() {
        let c = Contract::load();
        let m = measure(&c, "[q](/search?q=\"x\"&y=1) and [r](</a b>)\n");
        let hrefs: Vec<&str> = m.outcome.nodes[0]
            .content
            .iter()
            .flat_map(|n| n.marks.iter())
            .filter_map(|m| m.attrs.get("href").and_then(|v| v.as_str()))
            .collect();
        assert!(hrefs.contains(&"/search?q=\"x\"&y=1"), "{hrefs:?}");
        assert!(hrefs.contains(&"/a b"), "{hrefs:?}");
        assert_eq!(m.export_diff, None);
        // A title the contract has no place for is noted.
        let o = from_markdown(&c, "[a](/x \"Title\")\n");
        assert!(
            o.notes.iter().any(|n| n.message.contains("title")),
            "{:?}",
            o.notes
        );
    }

    #[test]
    fn a_link_to_a_scheme_the_page_refuses_keeps_its_text() {
        let c = Contract::load();
        let o = from_markdown(&c, "Open [the item](zotero://select/items/1) now.\n");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        assert_eq!(o.nodes[0].text_content(), "Open the item now.");
        assert!(o.nodes[0].content.iter().all(|n| n.marks.is_empty()));
        let cats: Vec<String> = o.notes.iter().map(|n| category(&n.message)).collect();
        assert_eq!(cats, ["link dropped"], "{:?}", o.notes);
        // On the model's path the same link is refused.
        let strict = crate::ingest::parse(
            &c,
            "<p><a href=\"zotero://select/items/1\">x</a></p>",
            "doc",
            Mode::Strict,
        );
        assert_eq!(strict.errors.len(), 1, "{:?}", strict.errors);
    }

    #[test]
    fn an_empty_header_row_is_markdowns_not_the_pages() {
        let c = Contract::load();
        let o = from_markdown(&c, "|  |  |\n| --- | --- |\n| 1 | 2 |\n");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        let rows = &o.nodes[0].content;
        assert_eq!(rows.len(), 1);
        assert!(rows[0].content.iter().all(|cell| cell.kind == "tableCell"));
        assert!(o
            .notes
            .iter()
            .any(|n| n.message.contains("empty header row")));
        // A header row with anything in it stays.
        let o = from_markdown(&c, "| A |  |\n| --- | --- |\n| 1 | 2 |\n");
        assert_eq!(o.nodes[0].content.len(), 2);
        assert_eq!(o.notes, []);
    }

    #[test]
    fn deep_headings_convert_without_notes() {
        let c = Contract::load();
        let m = measure(&c, "#### Four\n\n##### Five\n\n###### Six\n");
        assert!(m.outcome.notes.is_empty(), "{:?}", m.outcome.notes);
        let levels: Vec<_> = m
            .outcome
            .nodes
            .iter()
            .map(|n| n.attrs["level"].clone())
            .collect();
        assert_eq!(levels, [json!(4), json!(5), json!(6)]);
        assert!(m.text_lost.is_none());
        assert_eq!(m.export_diff, None);
    }

    #[test]
    fn table_alignment_survives_conversion_and_export() {
        let c = Contract::load();
        let md = "| L | C | R | N |\n| :--- | :---: | ---: | --- |\n| 1 | 2 | 3 | 4 |\n";
        let m = measure(&c, md);
        assert!(m.outcome.errors.is_empty(), "{:?}", m.outcome.errors);
        assert!(m.outcome.notes.is_empty(), "{:?}", m.outcome.notes);
        let rows = &m.outcome.nodes[0].content;
        for row in rows {
            let align: Vec<_> = row
                .content
                .iter()
                .map(|cell| cell.attrs["align"].clone())
                .collect();
            assert_eq!(
                align,
                [
                    json!("left"),
                    json!("center"),
                    json!("right"),
                    serde_json::Value::Null
                ]
            );
        }
        assert_eq!(render::markdown(&c, &m.outcome.nodes), md);
        assert_eq!(m.export_diff, None);
    }

    fn kinds(o: &Outcome) -> Vec<&str> {
        o.nodes.iter().map(|n| n.kind.as_str()).collect()
    }

    #[test]
    fn a_list_mixing_tasks_and_other_items_is_split_not_refused() {
        let c = Contract::load();
        for (md, want) in [
            ("- [x] done\n- plain item\n", vec!["taskList", "bulletList"]),
            ("- plain\n- [ ] todo\n", vec!["bulletList", "taskList"]),
            (
                "- a\n- [ ] b\n- [x] c\n- d\n",
                vec!["bulletList", "taskList", "bulletList"],
            ),
        ] {
            let m = measure(&c, md);
            assert!(
                m.outcome.errors.is_empty(),
                "{md:?}: {:?}",
                m.outcome.errors
            );
            assert_eq!(kinds(&m.outcome), want, "{md:?}");
            assert!(
                m.outcome
                    .notes
                    .iter()
                    .any(|n| n.message.contains("split a list")),
                "{md:?}: {:?}",
                m.outcome.notes
            );
            assert!(m.text_lost.is_none(), "{md:?}");
            assert_eq!(m.export_diff, None, "{md:?}");
        }
    }

    #[test]
    fn an_ordered_list_of_tasks_becomes_a_task_list() {
        let c = Contract::load();
        let m = measure(&c, "1. [ ] one\n2. [x] two\n");
        assert!(m.outcome.errors.is_empty(), "{:?}", m.outcome.errors);
        assert_eq!(kinds(&m.outcome), ["taskList"]);
        assert_eq!(m.outcome.nodes[0].content[1].attrs["checked"], json!(true));
        assert!(m
            .outcome
            .notes
            .iter()
            .any(|n| n.message.contains("without numbers")));
        assert_eq!(m.export_diff, None);
        // Mixed, the other items keep their numbers.
        let o = from_markdown(&c, "3. a\n4. [ ] b\n5. c\n");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        assert_eq!(kinds(&o), ["orderedList", "taskList", "orderedList"]);
        assert_eq!(o.nodes[0].attrs["start"], json!(3));
        assert_eq!(o.nodes[2].attrs["start"], json!(5));
    }

    #[test]
    fn an_empty_task_reads_back_as_one() {
        let c = Contract::load();
        for (md, checked) in [
            ("- [ ]\n", vec![false]),
            ("- [x]\n", vec![true]),
            ("- [ ] \n", vec![false]),
            ("- [ ] a\n- [ ]\n", vec![false, false]),
            ("- [x]\n- [ ] a\n", vec![true, false]),
        ] {
            let o = from_markdown(&c, md);
            assert!(o.errors.is_empty(), "{md:?}: {:?}", o.errors);
            assert_eq!(kinds(&o), ["taskList"], "{md:?}");
            let got: Vec<bool> = o.nodes[0]
                .content
                .iter()
                .map(|i| i.attrs["checked"].as_bool().unwrap())
                .collect();
            assert_eq!(got, checked, "{md:?}");
            assert!(
                o.nodes[0]
                    .content
                    .iter()
                    .all(|i| !i.text_content().contains('[')),
                "{md:?}"
            );
        }
        // With a list nested under it, and loose.
        let o = from_markdown(&c, "- [ ]\n  - nested\n- [ ] b\n");
        assert_eq!(kinds(&o), ["taskList"], "{:?}", o.errors);
        let kids: Vec<&str> = o.nodes[0].content[0]
            .content
            .iter()
            .map(|n| n.kind.as_str())
            .collect();
        assert_eq!(kids, ["paragraph", "bulletList"]);
        // Written as text, the brackets are text.
        let o = from_markdown(&c, "- \\[ \\]\n");
        assert_eq!(kinds(&o), ["bulletList"]);
        assert_eq!(o.nodes[0].text_content(), "[ ]");
    }

    #[test]
    fn markdown_never_brings_block_ids() {
        let c = Contract::load();
        let o = from_markdown(
            &c,
            "Appended note.\n\n<p data-id=\"pg1a2b3c\">Quoted block</p>\n",
        );
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        let mut ids = vec![];
        for n in &o.nodes {
            n.walk(&mut |n| ids.push(n.id().map(str::to_string)));
        }
        assert!(ids.iter().all(Option::is_none), "{ids:?}");
        assert!(o.notes.iter().any(|n| n.message.contains("data-id")));

        // Written into a page that has the id already, through the documented
        // path, every block's id is the page's only one.
        let page = crate::doc_from_nodes(
            crate::parse_html("<p data-id=\"pg1a2b3c\">Original</p>", "doc").nodes,
        );
        let mut nodes = crate::parse_html("<p data-id=\"pg1a2b3c\">Copied</p>", "doc").nodes;
        nodes.extend(o.nodes);
        {
            use yrs::{Transact, WriteTxn, XmlFragment};
            let mut txn = page.transact_mut();
            let frag = txn.get_or_insert_xml_fragment("doc");
            let at = frag.len(&txn);
            let written = crate::write_nodes(&mut txn, &frag, at, &nodes);
            assert_ne!(written[0].id(), Some("pg1a2b3c"));
        }
        let tree = crate::read_doc(&yrs::Transact::transact(&page));
        let ids: Vec<&str> = tree.iter().filter_map(Node::id).collect();
        let unique: std::collections::HashSet<&&str> = ids.iter().collect();
        assert_eq!(ids.len(), 4);
        assert_eq!(unique.len(), 4, "{ids:?}");
        assert_eq!(crate::validate_doc(&yrs::Transact::transact(&page)), []);
    }

    #[test]
    fn an_embed_the_contract_refuses_keeps_its_alt_text() {
        let c = Contract::load();
        for (md, cat, kept) in [
            // No image extension on either side: read as a file, as
            // `kindOfLink` reads it.
            (
                "Intro\n\n![chart](data:image/png;base64,iVBORw0KGgo=)\n\nMore text.\n",
                "file dropped",
                "kept its name",
            ),
            (
                "Intro\n\n![chart](file:///Users/x/shot.png)\n\nMore text.\n",
                "image dropped",
                "kept its alt text",
            ),
            (
                "Intro\n\n![chart.mp3](javascript:alert(1))\n\nMore text.\n",
                "audio dropped",
                "kept its name",
            ),
        ] {
            let o = from_markdown(&c, md);
            assert!(o.errors.is_empty(), "{md:?}: {:?}", o.errors);
            let text: Vec<String> = o.nodes.iter().map(Node::text_content).collect();
            assert_eq!(text[0], "Intro", "{md:?}");
            assert!(text[1].starts_with("chart"), "{md:?}: {text:?}");
            assert_eq!(text[2], "More text.", "{md:?}");
            let cats: Vec<String> = o.notes.iter().map(|n| category(&n.message)).collect();
            assert_eq!(cats, [cat], "{md:?}: {:?}", o.notes);
            assert!(o.notes[0].message.contains(kept), "{:?}", o.notes);
        }
        // Inside a paragraph, the alt text stays in its sentence.
        let o = from_markdown(&c, "See ![the chart](data:x) here.\n");
        assert_eq!(o.nodes.len(), 1, "{:?}", o.nodes);
        assert_eq!(o.nodes[0].text_content(), "See the chart here.");
    }

    #[test]
    fn a_value_the_contract_refuses_is_dropped_and_noted() {
        let c = Contract::load();
        let o = from_markdown(
            &c,
            "<ol start=\"abc\"><li>a</li></ol>\n\n<aside data-tone=\"loud\"><p>x</p></aside>\n",
        );
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        assert_eq!(kinds(&o), ["orderedList", "callout"]);
        assert_eq!(o.nodes[0].attrs["start"], json!(1));
        assert_eq!(o.nodes[1].attrs["tone"], json!("note"));
        let notes: Vec<&str> = o.notes.iter().map(|n| n.message.as_str()).collect();
        assert!(notes.iter().any(|m| m.contains("`start`")), "{notes:?}");
        assert!(notes.iter().any(|m| m.contains("`tone`")), "{notes:?}");
        // On the model's path a refused image is one error, not two.
        let strict = crate::ingest::parse(
            &c,
            "<img src=\"data:image/png;base64,x\" alt=\"a\">",
            "doc",
            Mode::Strict,
        );
        assert_eq!(strict.errors.len(), 1, "{:?}", strict.errors);
    }

    /// One block of `md`, judged: converted without error, no text lost, and
    /// the export reading back to the same tree.
    fn judged(md: &str) -> Outcome {
        let c = Contract::load();
        let m = measure(&c, md);
        assert!(
            m.outcome.errors.is_empty(),
            "{md:?}: {:?}",
            m.outcome.errors
        );
        assert!(m.text_lost.is_none(), "{md:?}: {:?}", m.text_lost);
        assert_eq!(m.export_diff, None, "{md:?}");
        m.outcome
    }

    #[test]
    fn embeds_are_told_apart_by_extension() {
        for (md, kind, name) in [
            (
                "![report.pdf](/api/drive/files/df_1/download)\n",
                "file",
                json!("report.pdf"),
            ),
            (
                "![clip.mp4](/api/drive/files/df_2/download)\n",
                "video",
                json!("clip.mp4"),
            ),
            ("![voice.m4a](/drive/df_3)\n", "audio", json!("voice.m4a")),
            ("![notes](/drive/df_5)\n", "file", json!("notes")),
            ("![walk.MOV](/drive/df_7)\n", "video", json!("walk.MOV")),
        ] {
            let o = judged(md);
            assert_eq!(kinds(&o), [kind], "{md:?}");
            assert_eq!(o.nodes[0].attrs["name"], name, "{md:?}");
            assert!(
                o.notes.iter().all(|n| n.message.starts_with("moved <")),
                "{md:?}: {:?}",
                o.notes
            );
        }
        // With no name, the export names it by its URL's last segment, which
        // reads back as its name: a projection's limit.
        for (md, kind, named) in [
            ("![](/drive/df_6)\n", "file", "![df\\_6](/drive/df_6)\n"),
            (
                "![](https://cdn.example.com/a/b.ogg?x=1)\n",
                "audio",
                "![b.ogg](https://cdn.example.com/a/b.ogg?x=1)\n",
            ),
        ] {
            let c = Contract::load();
            let o = from_markdown(&c, md);
            assert_eq!(kinds(&o), [kind], "{md:?}");
            assert_eq!(o.nodes[0].attrs["name"], Value::Null, "{md:?}");
            assert_eq!(render::markdown(&c, &o.nodes), named);
            assert_eq!(kinds(&from_markdown(&c, named)), [kind], "{named:?}");
        }
        for (md, alt) in [
            (
                "![](https://images.example.com/photo-1?w=400)\n",
                Value::Null,
            ),
            ("![A cat|600](/api/drive/files/1/cat.png)\n", json!("A cat")),
            ("![cat.png](/drive/df_8)\n", json!("cat.png")),
            ("![shot](/a.webp)\n", json!("shot")),
        ] {
            let o = judged(md);
            assert_eq!(kinds(&o), ["image"], "{md:?}");
            assert_eq!(o.nodes[0].attrs["alt"], alt, "{md:?}");
        }
        // A width is an image's: on any other embed it is dropped, noted.
        let o = from_markdown(&Contract::load(), "![memo.m4a|300](/drive/df_3)\n");
        assert_eq!(kinds(&o), ["audio"]);
        assert_eq!(o.nodes[0].attrs["name"], json!("memo.m4a"));
        assert!(
            o.notes.iter().any(|n| n.message.contains("width 300")),
            "{:?}",
            o.notes
        );
        // A ref embedded with `!` is the ref.
        let o = judged("![@Nick](/person/person_1)\n");
        assert_eq!(kinds(&o), ["paragraph"]);
        assert_eq!(o.nodes[0].content[0].kind, "mention");
        assert_eq!(o.nodes[0].content[0].attrs["to"], json!("/person/person_1"));
        assert_eq!(o.nodes[0].content[0].attrs["label"], json!("Nick"));
    }

    /// The cases the browser's `kindOfLink` reads too (`editor.test.ts`,
    /// and CodeMirror's `media-widgets.test.ts`), so a link turned into an
    /// embed or drawn in either editor is the block this converter makes.
    #[test]
    fn media_kind_reads_as_kind_of_link_does() {
        #[derive(serde::Deserialize)]
        struct Case {
            src: String,
            name: String,
            kind: String,
        }
        let cases: Vec<Case> =
            serde_json::from_str(include_str!("../tests/corpus/media-kinds.json")).unwrap();
        assert!(cases.len() > 10);
        for c in cases {
            assert_eq!(
                media_kind(&c.src, &c.name).node(),
                c.kind,
                "{} {}",
                c.src,
                c.name
            );
        }
    }

    fn proposals(o: &Outcome) -> Vec<(String, String, String)> {
        let mut out = vec![];
        for n in &o.nodes {
            n.walk(&mut |n| {
                for m in &n.marks {
                    if m.kind.starts_with("proposed") {
                        out.push((
                            m.kind.clone(),
                            m.attrs["proposal"].as_str().unwrap_or("").to_string(),
                            n.text.clone().unwrap_or_default(),
                        ));
                    }
                }
            });
        }
        out
    }

    #[test]
    fn critic_markup_becomes_proposals() {
        // A deletion straight followed by an insertion is one change.
        let o = judged("Lunch {--at noon--}{++on Friday++}.\n");
        let p = proposals(&o);
        assert_eq!(p.len(), 2, "{p:?}");
        assert_eq!(
            (p[0].0.as_str(), p[0].2.as_str()),
            ("proposedDeletion", "at noon")
        );
        assert_eq!(
            (p[1].0.as_str(), p[1].2.as_str()),
            ("proposedInsertion", "on Friday")
        );
        assert_eq!(p[0].1, p[1].1);
        assert_eq!(p[0].1.len(), 8);
        assert_eq!(o.nodes[0].text_content(), "Lunch at noonon Friday.");
        // Apart, or the other way round, each is its own.
        for md in [
            "{--a--} {++b++}\n",
            "{++b++}{--a--}\n",
            "{++b++}\n\n{--a--}\n",
        ] {
            let p = proposals(&judged(md));
            assert_eq!(p.len(), 2, "{md:?}");
            assert_ne!(p[0].1, p[1].1, "{md:?}");
        }
        // What a proposal holds is markdown.
        let o = judged("{++**bold** and [the plan](/page/page_abc)++} {--`code`--}\n");
        let p = proposals(&o);
        assert!(p
            .iter()
            .all(|(k, _, _)| k == "proposedInsertion" || k == "proposedDeletion"));
        assert_eq!(
            p.iter().map(|x| x.2.as_str()).collect::<Vec<_>>(),
            ["bold", " and ", "the plan", "code"]
        );
        // In a heading, a table cell and a task.
        let o = judged(
            "## {--Old--} title\n\n| A |\n| --- |\n| {++new++} |\n\n- [ ] {++call Nick++}\n",
        );
        assert_eq!(proposals(&o).len(), 3);
    }

    #[test]
    fn critic_markup_that_is_text_stays_text() {
        let c = Contract::load();
        for md in [
            "Write \\{--old--} to propose.\n",
            "Write \\{++new++} to propose.\n",
            "Write `{--old--}` to propose.\n",
            "```\n{--old--}\n```\n",
            "A brace {- -} and {+ +}.\n",
        ] {
            let o = judged(md);
            assert_eq!(proposals(&o), vec![], "{md:?}");
            assert_eq!(o.notes, [], "{md:?}");
        }
        // An escaped backslash before the opener leaves it an opener.
        assert_eq!(proposals(&judged("a \\\\{--b--}\n")).len(), 1);
        // Across blocks, mis-nested, a comment, a substitution: text, noted.
        for (md, note, text) in [
            ("{--one\n\ntwo--}\n", "ends in the block", "{--one"),
            ("*{--a*--}\n", "different formatting", "{--a"),
            (
                "Ask {>>is this right?<<} first.\n",
                "no comments",
                "{>>is this right?<<}",
            ),
            ("{~~old~>new~~}\n", "substitution", "{"),
        ] {
            let o = from_markdown(&c, md);
            assert!(o.errors.is_empty(), "{md:?}: {:?}", o.errors);
            assert_eq!(proposals(&o), vec![], "{md:?}");
            assert!(
                o.notes.iter().any(|n| n.message.contains(note)),
                "{md:?}: {:?}",
                o.notes
            );
            let all: String = o.nodes.iter().map(Node::text_content).collect();
            assert!(all.contains(text), "{md:?}: {all:?}");
            // Kept as text, it stays text through the export.
            let m = measure(&c, md);
            assert!(m.text_lost.is_none(), "{md:?}: {:?}", m.text_lost);
            assert_eq!(m.export_diff, None, "{md:?}");
        }
    }

    /// `escapeReviewBody` in the editor's `review-marks.ts`: how the
    /// CodeMirror editor wrote a proposal's text.
    fn escape_review_body(text: &str) -> String {
        [
            ("\\", "\\\\"),
            ("{--", "\\{--"),
            ("{++", "\\{++"),
            ("{>>", "\\{>>"),
            ("--}", "--\\}"),
            ("++}", "++\\}"),
            ("<<}", "<<\\}"),
        ]
        .iter()
        .fold(text.to_string(), |t, (from, to)| t.replace(from, to))
    }

    #[test]
    fn what_the_editor_escaped_in_a_proposal_reads_back_as_its_text() {
        for body in [
            "plain words",
            "a --} b",
            "a ++} b",
            "a <<} b",
            "x {-- y",
            "x {++ y",
            "x {>> y",
            "back\\slash",
            "two \\\\ backslashes",
            "C:\\dir\\{--x",
            "ends in a backslash \\",
            "mixed {--a--} and {++b++}",
        ] {
            for (open, close, kind) in [
                ("{--", "--}", "proposedDeletion"),
                ("{++", "++}", "proposedInsertion"),
            ] {
                let md = format!("Before {open}{}{close} after.\n", escape_review_body(body));
                let o = judged(&md);
                let p = proposals(&o);
                assert_eq!(p.len(), 1, "{md:?}: {p:?}");
                assert_eq!((p[0].0.as_str(), p[0].2.as_str()), (kind, body), "{md:?}");
            }
        }
    }

    #[test]
    fn alerts_become_callouts() {
        for (md, tone, body) in [
            ("> [!TIP]\n> Book early.\n", "tip", "Book early."),
            ("> [!NOTE]\n> Seen.\n", "note", "Seen."),
            ("> [!warning]\n> Careful.\n", "warning", "Careful."),
            ("> [!WARNING]\n>\n> - a\n> - b\n", "warning", "ab"),
        ] {
            let o = judged(md);
            assert_eq!(kinds(&o), ["callout"], "{md:?}");
            assert_eq!(o.nodes[0].attrs["tone"], json!(tone), "{md:?}");
            assert_eq!(o.nodes[0].text_content(), body, "{md:?}");
            assert_eq!(o.notes, [], "{md:?}");
        }
        for (md, tone) in [
            ("> [!CAUTION]\n> Hot.\n", "warning"),
            ("> [!IMPORTANT]\n> Read.\n", "note"),
        ] {
            let o = from_markdown(&Contract::load(), md);
            assert_eq!(kinds(&o), ["callout"], "{md:?}");
            assert_eq!(o.nodes[0].attrs["tone"], json!(tone), "{md:?}");
            assert!(
                o.notes
                    .iter()
                    .any(|n| n.message.contains("became a callout")),
                "{:?}",
                o.notes
            );
        }
        // In a list item, and holding only its marker.
        let o = judged("- a\n\n  > [!TIP]\n  > tip\n");
        assert_eq!(o.nodes[0].content[0].content[1].kind, "callout");
        let o = from_markdown(&Contract::load(), "> [!TIP]\n");
        assert_eq!(kinds(&o), ["callout"]);
        // Not alerts: escaped, with text on the marker's line, an unknown kind.
        for md in [
            "> \\[!TIP]\n> x\n",
            "> [!TIP] x\n",
            "> [!DANGER]\n> x\n",
            "> **[!TIP]**\n> x\n",
        ] {
            assert_eq!(kinds(&judged(md)), ["blockquote"], "{md:?}");
        }
    }

    #[test]
    fn what_the_contract_lacks_is_noted_and_no_text_is_lost() {
        let c = Contract::load();
        let m = measure(
            &c,
            "---\ntitle: x\n---\n\nSee <kbd>Ctrl</kbd> and ![clip](/drive/a.mp3).\n",
        );
        assert!(m.outcome.errors.is_empty(), "{:?}", m.outcome.errors);
        let cats: Vec<String> = m
            .outcome
            .notes
            .iter()
            .map(|n| category(&n.message))
            .collect();
        assert!(cats.iter().any(|c| c.contains("front matter")), "{cats:?}");
        assert!(cats.iter().any(|c| c == "unwrapped <kbd>"), "{cats:?}");
        // The embed is a block of its own, moved out of the sentence.
        assert!(cats.iter().any(|c| c == "moved <audio>"), "{cats:?}");
        assert_eq!(kinds(&m.outcome), ["paragraph", "audio", "paragraph"]);
        assert!(m.text_lost.is_none(), "{:?}", m.text_lost);
    }
}

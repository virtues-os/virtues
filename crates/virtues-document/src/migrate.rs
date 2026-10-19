//! Markdown → document tree: existing pages once, and markdown that writers
//! still send.
//!
//! Our markdown has a dialect on top of GFM: `==highlight==`, `<u>`,
//! `[@Label](/person/id)` references, `![alt|600](url)` image widths,
//! audio/video/file embeds written as images, CriticMarkup proposals
//! (`{--old--}{++new++}`) and GitHub's alerts (`> [!TIP]`). An article's
//! markdown adds footnotes (`[^ev-1]`), figure fences and veils (`⟦…⟧`),
//! read for articles alone ([`Dialect::Article`]). This maps the dialect
//! onto the contract, renders to HTML with pulldown-cmark, and ingests it in
//! migration mode, which reports every step that changed something.
//!
//! [`measure`] checks one document the way a conversion is judged: was any
//! text lost, and does the markdown export read back to the same tree.

use crate::contract::{is_route, normalize_url, url_scheme, Contract};
use crate::ingest::{parse, Mode, Outcome, MAX_INPUT_BYTES};
use crate::model::{new_id, Node, Problem};
use crate::render;
use pulldown_cmark::{
    html::write_html, BrokenLink, CodeBlockKind, CowStr, Event, LinkType, Options, Parser, Tag, TagEnd,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::ops::Range;

/// The extensions each kind takes: the lists the browser's `kindOfLink`
/// holds (`apps/web/src/lib/document/media-kind.ts`), which both page
/// editors draw links by.
const IMAGE_EXT: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico", "avif", "heic", "heif", "tif", "tiff",
];
const AUDIO_EXT: &[&str] = &["mp3", "wav", "ogg", "m4a", "aac", "flac", "opus", "wma"];
const VIDEO_EXT: &[&str] = &["mp4", "webm", "mov", "avi", "mkv", "m4v", "ogv"];
/// What a web address names a document, data or an archive by: never an
/// image, however the site serves it.
const FILE_EXT: &[&str] = &[
    "pdf", "md", "markdown", "txt", "rtf", "csv", "tsv", "json", "xml", "yaml", "yml", "html", "htm",
    "doc", "docx", "odt", "pages", "xls", "xlsx", "ods", "numbers", "ppt", "pptx", "odp", "key", "epub",
    "ics", "vcf", "zip", "gz", "tgz", "tar", "rar", "7z", "dmg", "pkg", "exe", "msi", "apk",
];

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
/// that, a web URL is an image unless its extension names a document, data
/// or an archive ([`FILE_EXT`]): the web serves images from addresses of
/// every shape (`photo-1?w=400`, `svg.latex?x^2`, `image.php?id=3`), and an
/// address that is no image draws as a file card in both editors once it
/// fails to load. Anything else, a Drive file named without an extension
/// included, is a file.
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
    if web && !FILE_EXT.contains(&on_src.as_str()) {
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

/// Where `==highlight==` is in one text run: each highlight's text, as a
/// byte range of `s`, its `==`s the two bytes either side. The first `==`
/// opens one and the next closes it when what is between holds something
/// and neither starts nor ends with a space; the first pair that does not
/// ends the search.
fn highlight_spans(s: &str) -> Vec<Range<usize>> {
    let mut out = vec![];
    let mut at = 0;
    while let Some(start) = s[at..].find("==").map(|i| at + i) {
        let after = start + 2;
        match s[after..].find("==") {
            Some(end) if end > 0 && !s[after..after + end].starts_with(' ') && !s[after..after + end].ends_with(' ') => {
                out.push(after..after + end);
                at = after + end + 2;
            }
            _ => break,
        }
    }
    out
}

/// `==highlight==` inside one text run, as `<mark>`; text without it as is.
fn highlight<'a>(t: CowStr<'a>, out: &mut Vec<Event<'a>>) {
    if !t.contains("==") {
        out.push(Event::Text(t));
        return;
    }
    let spans = highlight_spans(&t);
    if spans.is_empty() {
        out.push(Event::Text(t));
        return;
    }
    let mut at = 0;
    for span in spans {
        out.push(Event::Text(CowStr::from(t[at..span.start - 2].to_string())));
        out.push(Event::InlineHtml(CowStr::from("<mark>")));
        out.push(Event::Text(CowStr::from(t[span.clone()].to_string())));
        out.push(Event::InlineHtml(CowStr::from("</mark>")));
        at = span.end + 2;
    }
    out.push(Event::Text(CowStr::from(t[at..].to_string())));
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

/// Raw HTML in a paste that the page would not read whole, as the text it
/// is: a tag no page reads (`<Login/>`, `Vec<String>`), a comment or a
/// declaration, and a tag whose words ingest would lose
/// ([`crate::ingest::keeps_its_words`]: `<script>` and its content, the
/// words of a `<details>` run into the next). An inline run as text, an
/// HTML block as a paragraph of its lines. HTML whose every tag keeps its
/// words is still markup (`<kbd>Enter</kbd>` is "Enter").
fn literal_tags(events: Events<'_>) -> Events<'_> {
    let markup = |html: &str| {
        !html.contains("<!")
            && !html.contains("<?")
            && tag_names(html)
                .all(|name| is_markup_tag(name) && crate::ingest::keeps_its_words(crate::contract(), name))
    };
    html_as_text(events, &|_, html| markup(html), &markup)
}

/// A paste's lines kept apart as it showed them: a line under a list item
/// that is not indented into it, or under a quote's line without its `>`,
/// starts a paragraph of its own (a blank line put before it) rather than
/// running on in the item or the quote, as CommonMark's lazy continuation
/// reads it (`- milk` then `Call Nick at 5` is a list of one item and a
/// line). Fenced code is left as it is. `None` when no line runs on.
fn lazy_lines_apart(md: &str) -> Option<String> {
    let item = regex::Regex::new(r"^(?:[-*+]|[0-9]{1,9}[.)])(?: +|$)").expect("a list item's sign");
    let mut out = String::with_capacity(md.len());
    let mut changed = false;
    // The open fence's character and length.
    let mut fence: Option<(char, usize)> = None;
    // The column an open list item's lines start at, while its paragraph is open.
    let mut in_item: Option<usize> = None;
    // A quote's paragraph is open.
    let mut in_quote = false;
    for line in md.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        let rest = body.trim_start_matches([' ', '\t']);
        let indent: usize = body[..body.len() - rest.len()]
            .chars()
            .fold(0, |col, c| if c == '\t' { col + 4 - col % 4 } else { col + 1 });
        let fence_run = |c: char| rest.chars().take_while(|&r| r == c).count();
        if let Some((c, n)) = fence {
            if fence_run(c) >= n && rest.trim_start_matches(c).trim().is_empty() {
                fence = None;
            }
            out.push_str(line);
            continue;
        }
        if rest.is_empty() {
            in_item = None;
            in_quote = false;
            out.push_str(line);
            continue;
        }
        if let Some(c) = ['`', '~'].into_iter().find(|&c| fence_run(c) >= 3) {
            fence = Some((c, fence_run(c)));
            in_item = None;
            in_quote = false;
        } else if let Some(m) = item.find(rest) {
            let sign = m.as_str().trim_end().len();
            let spaces = m.len() - sign;
            in_item = Some(indent + sign + if (1..=4).contains(&spaces) { spaces } else { 1 });
            in_quote = false;
        } else if rest.starts_with('>') {
            in_quote = true;
            in_item = None;
        } else if indent <= 3 && rest.starts_with(['#', '|', '<']) {
            in_item = None;
            in_quote = false;
        } else if in_item.is_some_and(|col| indent < col) || in_quote {
            out.push('\n');
            changed = true;
            in_item = None;
            in_quote = false;
        }
        out.push_str(line);
    }
    changed.then_some(out)
}

/// How many cells GFM reads in a table row as written, and whether the row
/// closes with a pipe: its pipes not escaped by a backslash, less one that
/// opens it and one that closes it, plus one, as pulldown-cmark counts a
/// header's (`count_header_cols`). A pipe in inline code splits a cell
/// too: GFM finds the cells before it reads code.
fn row_cells(row: &str) -> (usize, bool) {
    let (mut pipes, mut last, mut escaped) = (0usize, None, false);
    for (i, b) in row.bytes().enumerate() {
        match b {
            b'\\' => {
                escaped = true;
                continue;
            }
            b'|' if !escaped => {
                pipes += 1;
                last = Some(i);
            }
            _ => {}
        }
        escaped = false;
    }
    let opens = row.trim_start().starts_with('|');
    let closes = last.is_some_and(|i| row[i + 1..].trim().is_empty());
    (pipes + 1 - usize::from(opens) - usize::from(closes), closes)
}

/// A paste's tables as wide as their widest row. GFM keeps as many of a
/// row's cells as its header has and drops the rest without a word, and a
/// row written by hand or copied runs past its header often enough (a
/// column the header left out, a pipe inside inline code): the header and
/// its delimiter row get an empty cell for each cell past them, so every
/// cell written is kept. `None` when no row is wider than its header.
fn tables_widened(md: &str) -> Option<String> {
    if !md.contains('|') {
        return None;
    }
    // Where a line's text ends, before the spaces or `\r` after it.
    let text_end = |at: usize| {
        let end = md[at..].find('\n').map_or(md.len(), |i| at + i);
        at + md[at..end].trim_end().len()
    };
    let line = |at: usize| &md[at..text_end(at)];
    // Each header and delimiter row too narrow: where its text ends, and
    // what to write there.
    let mut widen: Vec<(usize, String)> = vec![];
    let mut header: Option<(usize, usize)> = None;
    let mut widest = 0;
    for (event, range) in Parser::new_ext(md, parse_options()).into_offset_iter() {
        match event {
            Event::Start(Tag::TableHead) => {
                let (cells, _) = row_cells(line(range.start));
                header = Some((range.start, cells));
                widest = cells;
            }
            Event::Start(Tag::TableRow) => widest = widest.max(row_cells(line(range.start)).0),
            Event::End(TagEnd::Table) => {
                let Some((start, cells)) = header.take().filter(|&(_, cells)| widest > cells) else {
                    continue;
                };
                let more = widest - cells;
                let head_end = text_end(start);
                let Some(delimiter) = md[head_end..].find('\n').map(|i| head_end + i + 1) else {
                    continue;
                };
                for (end, cell) in [(head_end, " "), (text_end(delimiter), " --- ")] {
                    let row = &md[md[..end].rfind('\n').map_or(0, |i| i + 1)..end];
                    let mut cells = if row_cells(row).1 { String::new() } else { " |".to_string() };
                    for _ in 0..more {
                        cells.push_str(cell);
                        cells.push('|');
                    }
                    widen.push((end, cells));
                }
            }
            _ => {}
        }
    }
    if widen.is_empty() {
        return None;
    }
    let mut out = md.to_string();
    widen.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    for (at, cells) in widen {
        out.insert_str(at, &cells);
    }
    Some(out)
}

/// What the markdown editor of a page drew as markup: `<u>` and the `</u>`
/// after it on the same line (its `<u>(.*?)</u>`), and no other HTML.
const UNDERLINE: (&str, &str) = ("<u>", "</u>");

/// Raw HTML in a page's markdown as its markdown editor showed it, which
/// drew none but underline: an inline run is the text it reads as, an HTML
/// block (a comment among them) a paragraph of its lines, and `<u>` with a
/// `</u>` later on its line is underline.
fn page_tags(events: Events<'_>) -> Events<'_> {
    let mut underline = std::collections::HashSet::new();
    let mut open: Option<usize> = None;
    for (i, (ev, _)) in events.iter().enumerate() {
        match ev {
            Event::InlineHtml(h) if h.as_ref() == UNDERLINE.0 => open = Some(i),
            Event::InlineHtml(h) if h.as_ref() == UNDERLINE.1 => {
                if let Some(at) = open.take() {
                    underline.insert(at);
                    underline.insert(i);
                }
            }
            Event::Text(_)
            | Event::Code(_)
            | Event::InlineHtml(_)
            | Event::FootnoteReference(_)
            | Event::TaskListMarker(_)
            | Event::Start(Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Link { .. } | Tag::Image { .. })
            | Event::End(TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link | TagEnd::Image) => {}
            // A line ends, or a block does.
            _ => open = None,
        }
    }
    html_as_text(events, &|i, _| underline.contains(&i), &|_| false)
}

/// `events` with each raw HTML run that is not markup as the text it is:
/// an inline run (by its index and HTML) as text, an HTML block (by its
/// HTML) as a paragraph of its lines.
fn html_as_text<'a>(
    events: Events<'a>,
    inline_markup: &dyn Fn(usize, &str) -> bool,
    block_markup: &dyn Fn(&str) -> bool,
) -> Events<'a> {
    let markup = block_markup;
    let mut out: Events = Vec::with_capacity(events.len());
    // An HTML block being read: where it starts, and its events.
    let mut block: Option<(Range<usize>, Events)> = None;
    for (i, (ev, range)) in events.into_iter().enumerate() {
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
            (Event::InlineHtml(h), None) if !inline_markup(i, &h) => out.push((Event::Text(h), range)),
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
    /// What the page holds as a node of its own, never as text a
    /// suggestion's marks can cover: an image, a mention, a line break.
    Atom,
    /// The same in markdown read for a writer or an article, where a
    /// proposal around it is still made, its marks on the text either side
    /// of it, and counted ([`Kept::spanned`]).
    Spanned,
    /// Anything else inline: other text, a code span, a space between
    /// lines, inline HTML. A run of them is one.
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
    /// Proposals holding an [`Unit::Atom`].
    atoms: usize,
    /// Proposals made around a [`Unit::Spanned`]: in blocks, accepting or
    /// rejecting one would leave that node behind, or keep it alone.
    spanned: usize,
}

/// CriticMarkup proposals, as the editor wrote them into a page's text:
/// `{--old--}` and `{++new++}` written bare (not escaped) inside one block
/// become proposal marks, and a deletion straight followed by an insertion
/// is one proposal, a change, sharing one id. What they hold is read as
/// markdown, so the escapes the editor wrote into them (`--\}`) read as the
/// text they stand for. A span that crosses blocks, or opens and closes
/// inside different formatting, stays text, and so do comments
/// (`{>>…<<}`) and substitutions (`{~~…~>…~~}`), which a page cannot hold;
/// each is noted. In an owner's page (`page`, [`Dialect::Page`]), so does a
/// proposal holding what the page keeps as a node of its own (an image, a
/// mention, a line break, any line's end, each line kept a line): a
/// suggestion's marks cover text only, so in blocks accepting or rejecting
/// it would leave that node behind or take it alone, where its owner's
/// editor took the whole span. Their number is added to `held`. Elsewhere
/// such a proposal is made all the same, around that node (a footnote, a
/// mention, a line break, an image), and counted in `spanned`: what an
/// article's move holds back for.
///
/// Each proposal is rewritten as six cuts and two tags, so the cuts count
/// against [`MAX_EVENTS`], and markdown with more is refused before they
/// are made.
fn critic_markup<'a>(
    md: &str,
    events: Events<'a>,
    page: bool,
    notes: &mut Vec<Problem>,
    held: &mut usize,
    spanned: &mut usize,
) -> Result<Events<'a>, Problem> {
    let mut cuts = Cuts::new();
    let mut cut_count = 0;
    let mut kept = Kept::default();
    let mut units: Vec<Unit> = vec![];
    // Inside an image (its alt text), a mention's link, a code block or an
    // HTML block: how deep, and whether it is one node in its block.
    let mut skip = (0usize, false);
    // What a node of its own is to a proposal: one it cannot hold, in an
    // owner's page; elsewhere, as before, one more thing inline.
    let node = || if page { Unit::Atom } else { Unit::Spanned };
    for (i, (ev, range)) in events.iter().enumerate() {
        if skip.0 > 0 {
            match ev {
                Event::Start(_) => skip.0 += 1,
                Event::End(_) => skip.0 -= 1,
                _ => {}
            }
            if skip.0 == 0 && skip.1 {
                units.push(node());
            }
            continue;
        }
        match ev {
            Event::Start(Tag::Image { .. }) => skip = (1, true),
            Event::Start(Tag::Link { dest_url, .. }) if page && becomes_mention(dest_url, &events[i + 1..]) => {
                skip = (1, true)
            }
            Event::Start(Tag::Link { dest_url, .. }) if becomes_mention(dest_url, &events[i + 1..]) => {
                units.push(Unit::Spanned);
                units.push(Unit::Open { strike: false })
            }
            Event::HardBreak | Event::SoftBreak if page => units.push(Unit::Atom),
            Event::HardBreak | Event::FootnoteReference(_) => units.push(node()),
            Event::InlineHtml(h) if html_is_a_node(h) => units.push(node()),
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
                    // A footnote marker the parser left as text, which an
                    // article reads as a footnote all the same.
                    if !page && literal && b == b'[' && md.get(range.start + at..).is_some_and(bare_marker_at) {
                        units.push(Unit::Spanned);
                    }
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
    *spanned += kept.spanned;

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
    if kept.atoms > 0 {
        notes.push(Problem::new(
            "markdown",
            count(
                kept.atoms,
                "kept a proposal as text: it holds a mention, an image or a line break, and a suggestion holds only text",
                "kept {n} proposals as text: each holds a mention, an image or a line break, and a suggestion holds only text",
            ),
        ));
    }
    *held += kept.atoms;
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

/// Whether `s` opens with a footnote marker, `[^label]`.
fn bare_marker_at(s: &str) -> bool {
    let Some(rest) = s.strip_prefix("[^") else {
        return false;
    };
    rest.find(']').is_some_and(|end| crate::contract::is_label(&rest[..end]))
}

/// Whether a link to `dest`, whose label runs in `rest` up to its end, is
/// a mention: `[@Label](/kind/id)`, its label plain text ([`events_as`]).
fn becomes_mention(dest: &str, rest: &[(Event<'_>, Range<usize>)]) -> bool {
    let mut label = String::new();
    for (ev, _) in rest {
        match ev {
            Event::Text(t) => label.push_str(t),
            Event::End(TagEnd::Link) => return label.starts_with('@') && is_route(dest),
            _ => return false,
        }
    }
    false
}

/// Whether inline HTML is a node of its own in the page, not formatting: a
/// line break, an image or media, a mention or any other widget.
fn html_is_a_node(html: &str) -> bool {
    let tag = html.trim_start().to_ascii_lowercase();
    ["<br", "<img", "<audio", "<video", "<virtues-"].iter().any(|t| tag.starts_with(t))
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
    // The last node of its own: a span opened before it holds it.
    let mut atom: Option<usize> = None;
    let mut spanned: Option<usize> = None;
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
            Unit::Atom => atom = Some(k),
            Unit::Spanned => spanned = Some(k),
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
                            _ if atom.is_some_and(|a| a > *from) => kept.atoms += 1,
                            _ if *at == (open.len(), open.last().copied()) => {
                                kept.spanned += usize::from(spanned.is_some_and(|a| a > *from));
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

/// Refusing markdown that makes more than [`crate::MAX_NODES`] nodes,
/// found from its parser's events ([`nodes_at_least`]) before its HTML is
/// written and read: a table of short cells reaches the limit long before
/// [`MAX_EVENTS`], and writing and reading it whole first cost half a
/// gigabyte and over a second.
pub fn too_many_parts() -> Problem {
    Problem::new("markdown", crate::model::too_many_nodes())
}

/// Whether a refusal is for more markdown than one conversion holds
/// ([`too_complex`], [`too_much_html`], [`too_many_parts`], or ingest's
/// count of the tree), not for what it says: the answer to it is less
/// markdown at a time.
pub fn past_one_conversion(problem: &Problem) -> bool {
    *problem == too_complex()
        || *problem == too_much_html()
        || problem.message == crate::model::too_many_nodes()
}

/// A count the tree `events` convert to holds at least: never more than
/// [`crate::node_count`] of it, so a refusal past [`crate::MAX_NODES`] on it
/// refuses only what ingest would. Counted are what no conversion drops or
/// merges: a table, each of its body rows, each body cell with the block a
/// cell always holds, and one text run for each textblock (a paragraph, a
/// heading, a code block, a tight list item's line, a cell) whose first
/// event is text with something besides spaces in it. Text after a mark's
/// start is not counted: two runs with the same marks side by side are one.
/// A table's header row is not counted, since an empty one is dropped
/// ([`drop_empty_header_rows`]).
fn nodes_at_least(events: &[Event]) -> usize {
    let mut count = 0usize;
    let mut in_head = false;
    let mut text_starts = false;
    for ev in events {
        match ev {
            Event::Start(Tag::TableHead) => in_head = true,
            Event::End(TagEnd::TableHead) => in_head = false,
            _ if in_head => {}
            Event::Start(Tag::Table(_)) | Event::Start(Tag::TableRow) => count += 1,
            Event::Start(Tag::TableCell) => count += 2,
            Event::Text(t) if text_starts && !t.trim().is_empty() => count += 1,
            _ => {}
        }
        text_starts = matches!(
            ev,
            Event::Start(
                Tag::Paragraph | Tag::Heading { .. } | Tag::CodeBlock(_) | Tag::Item | Tag::TableCell
            )
        ) || matches!(ev, Event::Html(h) if h.starts_with(NUMBERED_ITEM));
    }
    count
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
    /// Our dialect in full: raw HTML is markup, CriticMarkup a proposal. A
    /// list numbered lazily (`1.` `1.` `1.`) counts on, as CommonMark reads
    /// it ([`lazily_numbered`]).
    Written,
    /// Text a person copied. A raw tag that is no HTML element and no tag
    /// of the contract (`<Login/>`, `Vec<String>`, `<placeholder>`) is the
    /// text it reads as, not a tag to unwrap, and so is HTML whose words
    /// the page would lose (a comment, `<script>`, `<details>`), and
    /// CriticMarkup, which a paste cannot make into a suggestion. Each line
    /// stays a line, and one under a list item or a quote that is not
    /// indented into it is a line of its own; a link reference definition
    /// is the line it was, and a link naming its label still goes where it
    /// says; a table is as wide as its widest row ([`tables_widened`]); a
    /// paste opening with `---`, a line and `---` is a rule and the text
    /// under it, never front matter left out: nothing written is lost. A
    /// list numbered lazily counts on, as in [`Dialect::Written`].
    Paste,
    /// A page its owner wrote in the markdown editor, read as that editor
    /// showed it, so moving it to blocks loses nothing they saw. The editor
    /// drew markdown over the text's own lines and no HTML but underline:
    /// raw HTML and comments are the text they read as (`a<b and c>d`,
    /// `Dear <name>,`, `<!-- call the bank -->`), `<u>…</u>` on one line is
    /// underline, and every line of a paragraph stays a line of its own (a
    /// line break where markdown would join the lines with a space). A link
    /// reference definition (`[Monday]: gym`) is the line it was, and a link
    /// that names its label still goes where it says. The editor read no
    /// character reference or escape (`&amp;`, `\*` are what was typed, and
    /// a backslash at a line's end stays before the line's break), no
    /// single `~` as a strike, and no front matter (a page opening with
    /// `---` opens with a rule). CriticMarkup is the owner's suggestions, as
    /// written. What blocks cannot hold of what the editor showed is noted:
    /// an ordered list's own numbers, a run of spaces or a tab, a line's
    /// indent, a table row's cells past its header's, a code block's info
    /// past its language, a NUL.
    Page,
    /// An article's markdown (a wiki article, a day page), as its machine
    /// writers write it: [`Dialect::Written`], and the article syntax the
    /// day page reads. A footnote reference (`[^ev-1]`) is a footnote
    /// carrying the first paragraph of its definition as its note, and the
    /// definitions themselves are not blocks; a fence whose info string is
    /// `figure` and whose lines are the figure's fields is a figure; `⟦…⟧`
    /// in one run of text is a veil. Read for articles only: on any other
    /// page `[^1]` and `⟦` are what was typed. What its readers drew as text
    /// stays text: a raw tag that is no HTML element and no tag of the
    /// contract, HTML whose words the page would lose, as in
    /// [`Dialect::Paste`]; an opening `---`, a line and `---`, which is no
    /// front matter; and a definition no sentence cites under a label the
    /// day page does not lift (`[^note]: …`). What the article syntax could
    /// not read, or read differently from the day page's readers, is noted
    /// and counted ([`ArticleCounts`]).
    Article,
}

/// `md` with each NUL as U+FFFD, noted; `None` when it holds none. The
/// page's editor drew a NUL as a mark of its own, and HTML's parser drops
/// one without a word.
fn nuls_replaced(md: &str, notes: &mut Vec<Problem>) -> Option<String> {
    let n = md.matches('\0').count();
    if n == 0 {
        return None;
    }
    notes.push(Problem::new(
        "markdown",
        match n {
            1 => "wrote a NUL character as U+FFFD: a page holds no NUL".to_string(),
            n => format!("wrote {n} NUL characters as U+FFFD: a page holds no NUL"),
        },
    ));
    Some(md.replace('\0', "\u{FFFD}"))
}

/// A strike opened and closed by one `~` (`~gone~`) as the text it was:
/// the page's editor struck only `~~gone~~`, as GitHub's markdown does.
fn single_tildes<'a>(md: &str, events: Events<'a>) -> Events<'a> {
    let single = |range: &Range<usize>| {
        let src = md.get(range.clone()).unwrap_or("");
        src.starts_with('~') && !src.starts_with("~~")
    };
    events
        .into_iter()
        .map(|(ev, range)| match ev {
            Event::Start(Tag::Strikethrough) if single(&range) => {
                (Event::Text(CowStr::Borrowed("~")), range.start..range.start + 1)
            }
            Event::End(TagEnd::Strikethrough) if single(&range) => {
                (Event::Text(CowStr::Borrowed("~")), range.end - 1..range.end)
            }
            ev => (ev, range),
        })
        .collect()
}

/// Whether `src` is one character reference: `&amp;`, `&#169;`, `&#xA9;`.
fn is_reference(src: &str) -> bool {
    let Some(name) = src.strip_prefix('&').and_then(|s| s.strip_suffix(';')) else {
        return false;
    };
    match name.strip_prefix('#') {
        Some(n) => match n.strip_prefix(['x', 'X']) {
            Some(hex) => !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()),
            None => !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()),
        },
        None => name.bytes().next().is_some_and(|b| b.is_ascii_alphabetic()) && name.bytes().all(|b| b.is_ascii_alphanumeric()),
    }
}

/// A page's text as its editor showed it, which read no character
/// reference and no escape: `&amp;` and `&nbsp;` stay what was typed, and
/// so does a backslash before punctuation (`\*`, a UNC path's `\\server`).
/// Not in code (which reads neither), an image's alt (drawn, never shown as
/// text) or a suggestion (whose escapes the editor wrote, and which reads
/// them, [`critic_markup`]); in a table's cell `\|` is the pipe its table
/// showed, and a definition's escapes (`escaped`, [`definitions_as_text`])
/// read as the text it was. A code block's info past its language, which
/// no block holds, is noted.
fn page_text<'a>(md: &str, events: Events<'a>, escaped: &[Range<usize>], notes: &mut Vec<Problem>) -> Events<'a> {
    let mut out: Events<'a> = Vec::with_capacity(events.len());
    let (mut code, mut image, mut proposal, mut cell) = (0usize, 0usize, 0usize, false);
    for (ev, range) in events {
        match &ev {
            Event::Start(Tag::CodeBlock(kind)) => {
                code += 1;
                if let CodeBlockKind::Fenced(info) = kind {
                    let info = info.trim();
                    if let Some((language, rest)) = info.split_once(char::is_whitespace) {
                        notes.push(Problem::new(
                            "markdown",
                            format!(
                                "left out `{}` after a code block's language `{language}`: a code block holds its language alone",
                                rest.trim()
                            ),
                        ));
                    }
                }
            }
            Event::End(TagEnd::CodeBlock) => code = code.saturating_sub(1),
            Event::Start(Tag::Image { .. }) => image += 1,
            Event::End(TagEnd::Image) => image = image.saturating_sub(1),
            Event::Start(Tag::TableCell) => cell = true,
            Event::End(TagEnd::TableCell) => cell = false,
            Event::InlineHtml(h) if h.starts_with("<virtues-ins") || h.starts_with("<virtues-del") => proposal += 1,
            Event::InlineHtml(h) if h.starts_with("</virtues-ins") || h.starts_with("</virtues-del") => {
                proposal = proposal.saturating_sub(1)
            }
            Event::Text(t) if code == 0 && image == 0 && proposal == 0 => {
                let src = md.get(range.clone()).unwrap_or("");
                if src != t.as_ref() && is_reference(src) {
                    out.push((Event::Text(CowStr::from(src.to_string())), range));
                    continue;
                }
                let typed = src == t.as_ref()
                    && t.starts_with(|c: char| c.is_ascii_punctuation())
                    && escaped_at(md, range.start)
                    && !(cell && t.starts_with('|'))
                    && !escaped.iter().any(|span| span.contains(&range.start));
                if typed {
                    let text = format!("\\{t}");
                    out.push((Event::Text(CowStr::from(text)), range.start - 1..range.end));
                    continue;
                }
            }
            // A backslash at a line's end is a line break to the parser,
            // which drops the backslash; the editor showed it as typed (a
            // path's `C:\Users\`, a shell line's continuation).
            Event::HardBreak if code == 0 && image == 0 && proposal == 0 && typed_break(md, &range) => {
                out.push((Event::Text(CowStr::from("\\")), range.start..range.start + 1));
                out.push((Event::HardBreak, range.start + 1..range.end));
                continue;
            }
            _ => {}
        }
        out.push((ev, range));
    }
    out
}

/// Whether a line break's source is a backslash at a line's end.
fn typed_break(md: &str, range: &Range<usize>) -> bool {
    md.get(range.clone()).is_some_and(|src| src.starts_with('\\'))
}

/// The lines of a page's markdown that end in a backslash the parser reads
/// as a line break, outside an image's description: each one the editor
/// showed with its backslash, as the text it was. Read from the markdown,
/// not from the conversion, so the conversion cannot hide one it drops.
fn backslash_breaks(md: &str) -> Vec<&str> {
    let mut out = vec![];
    let mut image = 0usize;
    for (event, range) in Parser::new_ext(md, parse_options()).into_offset_iter().take(MAX_EVENTS) {
        match event {
            Event::Start(Tag::Image { .. }) => image += 1,
            Event::End(TagEnd::Image) => image = image.saturating_sub(1),
            Event::HardBreak if image == 0 && typed_break(md, &range) => {
                let start = md[..range.start].rfind('\n').map_or(0, |at| at + 1);
                out.push(&md[start..=range.start]);
            }
            _ => {}
        }
    }
    out
}

/// How many of the blocks' line breaks follow a backslash: text ending in
/// `\` straight before a line break.
fn backslash_breaks_kept(nodes: &[Node]) -> usize {
    let mut n = 0;
    for node in nodes {
        node.walk(&mut |b| {
            n += b
                .content
                .windows(2)
                .filter(|pair| {
                    pair[1].kind == "hardBreak" && pair[0].text.as_deref().is_some_and(|t| t.ends_with('\\'))
                })
                .count();
        });
    }
    n
}

// ------------------------------------------------------------ article syntax

/// A footnote's definition as an article's readers read it: the first line
/// of what follows `[^label]:`, never parsed inline (`_` and `·` in a
/// record's id stay as they are), its whitespace collapsed.
struct Definition {
    note: String,
    /// Its text runs past its first line: a lazy line, or a paragraph
    /// indented under it. The readers read the first line alone.
    continued: bool,
}

/// What a definition at `range` of `md` reads as: see [`Definition`].
fn definition_at(md: &str, range: &Range<usize>) -> Definition {
    let src = md.get(range.clone()).unwrap_or("");
    let mut lines = src.lines().filter(|l| !l.trim().is_empty());
    let first = lines.next().unwrap_or("");
    let note = first.split_once("]:").map_or("", |(_, rest)| rest);
    Definition {
        note: note.split_whitespace().collect::<Vec<_>>().join(" "),
        continued: lines.next().is_some(),
    }
}

/// The HTML a footnote is read in as: `<virtues-footnote>`, its note an
/// attribute.
fn footnote_html(label: &str, note: Option<&str>) -> String {
    match note {
        Some(note) => format!(
            "<virtues-footnote label=\"{}\" note=\"{}\"></virtues-footnote>",
            esc(label),
            esc(note)
        ),
        None => format!("<virtues-footnote label=\"{}\"></virtues-footnote>", esc(label)),
    }
}

/// The figure a fence's body is, as `<virtues-figure>`, or why it is not
/// one: every line `key: value`, split at its first `:`, each key (any case)
/// an attribute of the contract's figure and none twice, its `kind` one the
/// contract names and its `src` one a page may hold. Values have their
/// whitespace collapsed, as the writers write them.
fn figure_html(c: &Contract, body: &str) -> Result<String, String> {
    let Some(spec) = c.node("figure") else {
        return Err("the contract has no figure".into());
    };
    let mut fields: Vec<(String, String)> = vec![];
    // Every line, an empty last one too: the browser's export reads it so
    // (`readsAsFigure` in `markdown.ts`).
    for line in body.split('\n') {
        let Some((key, value)) = line.split_once(':') else {
            return Err("a line of it is not `key: value`".into());
        };
        let key = key.trim().to_ascii_lowercase();
        if spec.attrs().get(&key).is_none() {
            return Err(format!("a figure has no `{key}`"));
        }
        if fields.iter().any(|(k, _)| *k == key) {
            return Err(format!("it gives `{key}` twice"));
        }
        fields.push((key, value.split_whitespace().collect::<Vec<_>>().join(" ")));
    }
    let mut html = String::from("<virtues-figure");
    for (key, a) in spec.attrs() {
        let Some((_, value)) = fields.iter().find(|(k, _)| k == key) else {
            if a.required {
                return Err(format!("it gives no `{key}`"));
            }
            continue;
        };
        let typed = match a.ty {
            crate::contract::AttrType::Url => normalize_url(value),
            _ => value.clone(),
        };
        a.check(c, key, &serde_json::Value::String(typed))?;
        html.push_str(&format!(" {}=\"{}\"", a.html.as_deref().unwrap_or(key), esc(value)));
    }
    html.push_str("></virtues-figure>\n");
    Ok(html)
}

/// Whether a figure fence with this body reads as a figure ([`figure_html`]):
/// for the export, which writes a code block in the language `figure` that
/// would read as one with a longer fence, so it stays the code it is.
pub fn reads_as_figure(c: &Contract, body: &str) -> bool {
    figure_html(c, body).is_ok()
}

/// One character of a run of text in [`article_syntax`], and whether it
/// was written as itself: not through an escape or a character reference.
#[derive(Clone, Copy)]
struct Written {
    ch: char,
    bare: bool,
}

/// A piece of a run of text, once its article syntax is read.
enum RunPiece {
    Text(String),
    Html(String),
}

/// A footnote marker written in a run of text, `[^label]`, its brackets and
/// caret bare, starting at `chars[i]`: the label and where the marker ends.
/// pulldown-cmark leaves a marker as text where it reads no definition for
/// it, and the first of two markers written side by side (`.[^ev-1][^ev-2]`)
/// even where it does: the readers read both.
fn marker_at(chars: &[Written], i: usize) -> Option<(String, usize)> {
    let bare = |k: usize, ch: char| chars.get(k).is_some_and(|w| w.bare && w.ch == ch);
    if !bare(i, '[') || !bare(i + 1, '^') {
        return None;
    }
    let mut label = String::new();
    let mut k = i + 2;
    while let Some(w) = chars.get(k) {
        match w.ch {
            ']' if w.bare => return Some((label, k + 1)),
            '[' | ']' | '^' => return None,
            c if c.is_whitespace() || !w.bare || label.len() > crate::contract::MAX_LABEL => return None,
            c => label.push(c),
        }
        k += 1;
    }
    None
}

/// What [`article_syntax`] has read so far.
struct ArticleRead<'n> {
    defs: HashMap<String, String>,
    cited: std::collections::HashSet<String>,
    counts: ArticleCounts,
    notes: &'n mut Vec<Problem>,
    bad_labels: usize,
}

impl ArticleRead<'_> {
    /// The footnote a marker with `label` is, as HTML, or `None` (noted)
    /// when the label is none a footnote takes.
    fn footnote(&mut self, label: &str) -> Option<String> {
        if !crate::contract::is_label(label) {
            self.bad_labels += 1;
            return None;
        }
        self.counts.refs_in += 1;
        self.cited.insert(label.to_string());
        Some(footnote_html(label, self.defs.get(label).map(String::as_str)))
    }
}

/// A run of text, `chars`, split into its footnotes, veils and highlights.
/// `highlights`: each highlight's characters, as `chars` ranges. `None`
/// when the run holds none of the article's syntax and stays as it is.
fn article_run(chars: &[Written], highlights: &[Range<usize>], read: &mut ArticleRead) -> Option<Vec<RunPiece>> {
    // Where the run is cut, and what stands at each cut: a highlight's
    // edges, then each footnote.
    let mut cuts: Vec<(Range<usize>, String)> = vec![];
    for h in highlights {
        cuts.push((h.start - 2..h.start, "<mark>".into()));
        cuts.push((h.end..h.end + 2, "</mark>".into()));
    }
    let mut i = 0;
    let mut found = false;
    while i < chars.len() {
        match marker_at(chars, i) {
            Some((label, end)) if !cuts.iter().any(|(r, _)| r.start < end && i < r.end) => {
                found = true;
                if let Some(html) = read.footnote(&label) {
                    cuts.push((i..end, html));
                }
                i = end;
            }
            _ => i += 1,
        }
    }
    cuts.sort_by_key(|(r, _)| r.start);
    // Between the cuts, the veils: a bare `⟦` and the next bare `⟧`, with no
    // bare bracket between them and something inside. A bracket written as
    // a character reference is the veil's own text, as the export writes it.
    let mut pieces: Vec<RunPiece> = vec![];
    let mut from = 0;
    let text = |a: usize, b: usize| chars[a..b].iter().map(|w| w.ch).collect::<String>();
    let segment = |a: usize, b: usize, pieces: &mut Vec<RunPiece>, found: &mut bool| {
        let mut at = a;
        let mut k = a;
        while k < b {
            if chars[k].ch == '⟦' && chars[k].bare {
                let close = (k + 1..b).find(|&j| chars[j].bare && matches!(chars[j].ch, '⟦' | '⟧'));
                if let Some(j) = close.filter(|&j| chars[j].ch == '⟧' && chars[j].bare && j > k + 1) {
                    *found = true;
                    pieces.push(RunPiece::Text(text(at, k)));
                    pieces.push(RunPiece::Html("<virtues-veil>".into()));
                    pieces.push(RunPiece::Text(text(k + 1, j)));
                    pieces.push(RunPiece::Html("</virtues-veil>".into()));
                    at = j + 1;
                    k = j + 1;
                    continue;
                }
            }
            k += 1;
        }
        pieces.push(RunPiece::Text(text(at, b)));
    };
    for (range, html) in &cuts {
        segment(from, range.start, &mut pieces, &mut found);
        pieces.push(RunPiece::Html(html.clone()));
        from = range.end;
    }
    segment(from, chars.len(), &mut pieces, &mut found);
    found.then_some(pieces)
}

/// A top-level footnote definition in an article, as [`article_syntax`]
/// reads it.
enum DefinitionSpan {
    /// One with a footnote's label: its label, its source, and whether it
    /// runs past its first line.
    Note { label: String, src: String, continued: bool },
    /// One whose label no footnote takes: its source.
    Text(String),
}

/// Whether the day page reads a definition under `label` as a note
/// (`DEF` in `lib/wiki/dayArticle.ts`): `ev-` or `cx-` and digits. It draws
/// any other as the paragraph it is.
pub fn lifted_by_the_day_page(label: &str) -> bool {
    ["ev-", "cx-"].iter().any(|kind| {
        label
            .strip_prefix(kind)
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// A paragraph of `text`'s lines, each trimmed, a line's end between them.
fn paragraph_of_lines<'a>(text: &str, range: &Range<usize>) -> Events<'a> {
    let mut out: Events<'a> = vec![(Event::Start(Tag::Paragraph), range.clone())];
    for (k, line) in text.lines().enumerate() {
        if k > 0 {
            out.push((Event::SoftBreak, range.clone()));
        }
        out.push((Event::Text(CowStr::from(line.trim().to_string())), range.clone()));
    }
    out.push((Event::End(TagEnd::Paragraph), range.clone()));
    out
}

/// An article's syntax ([`Dialect::Article`]) read into the contract's
/// tags: each top-level footnote definition becomes the note of the
/// footnotes citing it and is no block of its own; each reference, and
/// each bare marker in text (`[^ev-9]`, which pulldown-cmark leaves as text
/// when nothing defines it and for the first of two markers side by side),
/// a footnote; a figure fence a figure; and `⟦…⟧` in one run of text a
/// veil. What it reads otherwise than the day page's readers would, or
/// cannot read, is noted, and `counts` says what it read.
fn article_syntax<'a>(md: &str, events: Events<'a>, notes: &mut Vec<Problem>, counts: &mut ArticleCounts) -> Events<'a> {
    let c = crate::contract();
    // The definitions, first: a reference comes before its definition.
    let mut defs: HashMap<String, String> = HashMap::new();
    let mut order: Vec<String> = vec![];
    // Each top-level definition's events, by where they start: the index of
    // its end, and what it is.
    let mut spans: HashMap<usize, (usize, DefinitionSpan)> = HashMap::new();
    let mut depth = 0usize;
    let mut i = 0;
    while i < events.len() {
        let (ev, range) = &events[i];
        match ev {
            Event::Start(Tag::FootnoteDefinition(label)) if depth == 0 => {
                let mut end = i + 1;
                let mut inner = 0usize;
                while end < events.len() {
                    match &events[end].0 {
                        Event::Start(_) => inner += 1,
                        Event::End(TagEnd::FootnoteDefinition) if inner == 0 => break,
                        Event::End(_) => inner = inner.saturating_sub(1),
                        _ => {}
                    }
                    end += 1;
                }
                let def = definition_at(md, range);
                let src = md.get(range.clone()).unwrap_or("").trim_end().to_string();
                if crate::contract::is_label(label) {
                    if defs.insert(label.to_string(), def.note).is_none() {
                        order.push(label.to_string());
                    }
                    let note = DefinitionSpan::Note {
                        label: label.to_string(),
                        src,
                        continued: def.continued,
                    };
                    spans.insert(i, (end, note));
                } else {
                    spans.insert(i, (end, DefinitionSpan::Text(src)));
                }
                i = end + 1;
                continue;
            }
            Event::Start(_) => depth += 1,
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
        i += 1;
    }
    let mut read = ArticleRead {
        defs,
        cited: Default::default(),
        counts: std::mem::take(counts),
        notes,
        bad_labels: 0,
    };

    let mut out: Events<'a> = Vec::with_capacity(events.len());
    // A run of text being gathered: its text events and the line ends
    // between them.
    let mut run: Events<'a> = vec![];
    let (mut code, mut image) = (0usize, 0usize);
    let mut nested_defs = 0usize;
    // Where each definition with a footnote's label stood in `out`, for the
    // ones no sentence cites under a label the day page does not lift,
    // which go back there as the text it drew.
    let mut defined_at: Vec<(usize, &DefinitionSpan, Range<usize>)> = vec![];
    let mut words = String::new();
    let flush = |run: &mut Events<'a>, out: &mut Events<'a>, read: &mut ArticleRead| {
        if run.is_empty() {
            return;
        }
        let mut chars: Vec<Written> = vec![];
        let mut highlights: Vec<Range<usize>> = vec![];
        for (ev, range) in run.iter() {
            let text = match ev {
                Event::Text(t) => t.as_ref(),
                _ => "\n",
            };
            let literal = matches!(ev, Event::Text(_)) && md.get(range.clone()) == Some(text);
            let escaped = literal && escaped_at(md, range.start);
            let base = chars.len();
            // Highlights are read within one event, as `highlight` reads
            // them; the characters counted, not the bytes.
            let at_char = |byte: usize| base + text[..byte].chars().count();
            highlights.extend(highlight_spans(text).into_iter().map(|r| at_char(r.start)..at_char(r.end)));
            for (k, ch) in text.chars().enumerate() {
                chars.push(Written { ch, bare: literal && !(k == 0 && escaped) });
            }
        }
        let start = run[0].1.start;
        let end = run[run.len() - 1].1.end;
        match article_run(&chars, &highlights, read) {
            None => out.append(run),
            // Written as HTML, so nothing after reads its text again: a
            // highlight in it is already read.
            Some(pieces) => {
                run.clear();
                for p in pieces {
                    let html = match p {
                        RunPiece::Text(t) if t.is_empty() => continue,
                        RunPiece::Text(t) => esc(&t),
                        RunPiece::Html(h) => h,
                    };
                    out.push((Event::InlineHtml(CowStr::from(html)), start..end));
                }
            }
        }
    };
    let mut i = 0;
    while i < events.len() {
        if let Some((end, span)) = spans.get(&i) {
            flush(&mut run, &mut out, &mut read);
            match span {
                DefinitionSpan::Text(text) => {
                    read.bad_labels += 1;
                    out.extend(paragraph_of_lines(text, &events[i].1));
                }
                DefinitionSpan::Note { .. } => defined_at.push((out.len(), span, events[i].1.clone())),
            }
            i = end + 1;
            continue;
        }
        let (ev, range) = events[i].clone();
        i += 1;
        match ev {
            Event::Text(ref t) if code == 0 && image == 0 => {
                words.push_str(t);
                run.push((ev, range));
                continue;
            }
            // A line's end inside a paragraph is a space: part of the run.
            Event::SoftBreak if code == 0 && image == 0 && !run.is_empty() => {
                words.push(' ');
                run.push((ev, range));
                continue;
            }
            _ => {}
        }
        flush(&mut run, &mut out, &mut read);
        match ev {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(ref info))) if code == 0 && info.as_ref() == "figure" => {
                read.counts.figures_in += 1;
                // Its body, and where it ends.
                let mut body = String::new();
                let mut end = i;
                while end < events.len() && !matches!(events[end].0, Event::End(TagEnd::CodeBlock)) {
                    if let Event::Text(t) = &events[end].0 {
                        body.push_str(t);
                    }
                    end += 1;
                }
                let fence = md.get(range.clone()).unwrap_or("").trim_start();
                let three = fence.starts_with("```") && !fence.starts_with("````");
                let figure = if three {
                    figure_html(c, body.strip_suffix('\n').unwrap_or(&body))
                } else {
                    Err("its fence is not three backticks".into())
                };
                match figure {
                    Ok(html) => {
                        out.push((Event::Html(CowStr::from(html)), range));
                        i = end + 1;
                    }
                    Err(why) => {
                        read.counts.figures_as_code += 1;
                        read.notes.push(Problem::new(
                            "markdown",
                            format!("kept a figure as a code block: {why}"),
                        ));
                        code += 1;
                        out.push((ev, range));
                    }
                }
            }
            Event::Start(Tag::CodeBlock(_)) => {
                code += 1;
                out.push((ev, range));
            }
            Event::End(TagEnd::CodeBlock) => {
                code = code.saturating_sub(1);
                out.push((ev, range));
            }
            Event::Start(Tag::Image { .. }) => {
                image += 1;
                out.push((ev, range));
            }
            Event::End(TagEnd::Image) => {
                image = image.saturating_sub(1);
                out.push((ev, range));
            }
            // In an image's description, which is its alt text, a marker
            // is the text it was written as.
            Event::FootnoteReference(label) if image > 0 => {
                out.push((Event::Text(CowStr::from(format!("[^{label}]"))), range));
            }
            Event::FootnoteReference(label) => match read.footnote(&label) {
                Some(html) => out.push((Event::InlineHtml(CowStr::from(html)), range)),
                None => out.push((Event::Text(CowStr::from(format!("[^{label}]"))), range)),
            },
            // A definition inside a quote or a list is no definition to the
            // readers, which read one only at a line's start: it is its
            // text, its `[^label]:` opening its first paragraph.
            Event::Start(Tag::FootnoteDefinition(_)) => {
                nested_defs += 1;
                let src = md.get(range.clone()).unwrap_or("");
                let label = src[..src.find("]:").map_or(0, |at| at + 2)].trim_start();
                let label = Event::Text(CowStr::from(format!("{label} ")));
                if let Some((Event::Start(Tag::Paragraph), first)) = events.get(i).cloned() {
                    out.push((Event::Start(Tag::Paragraph), first));
                    i += 1;
                    out.push((label, range));
                } else {
                    out.push((Event::Start(Tag::Paragraph), range.clone()));
                    out.push((label, range.clone()));
                    out.push((Event::End(TagEnd::Paragraph), range));
                }
            }
            Event::End(TagEnd::FootnoteDefinition) => {}
            Event::Start(Tag::Paragraph | Tag::Heading { .. } | Tag::TableCell | Tag::Item)
            | Event::End(TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::TableCell | TagEnd::Item) => {
                read.counts.veils_in += veils_in(&words);
                words.clear();
                out.push((ev, range));
            }
            _ => out.push((ev, range)),
        }
    }
    flush(&mut run, &mut out, &mut read);
    read.counts.veils_in += veils_in(&words);

    let ArticleRead { cited, counts: mut read_counts, notes, bad_labels, .. } = read;
    // A definition no sentence cites under a label the day page does not
    // lift is a paragraph to it, so it stays one; the rest are its notes.
    let kept: HashSet<&str> = order
        .iter()
        .map(String::as_str)
        .filter(|l| !cited.contains(*l) && !lifted_by_the_day_page(l))
        .collect();
    for (at, span, range) in defined_at.iter().rev() {
        let DefinitionSpan::Note { label, src, .. } = span else { continue };
        if kept.contains(label.as_str()) {
            out.splice(*at..*at, paragraph_of_lines(src, range));
        }
    }
    let mut seen = HashSet::new();
    for (_, span, _) in &defined_at {
        let DefinitionSpan::Note { label, continued, .. } = span else { continue };
        if kept.contains(label.as_str()) {
            continue;
        }
        read_counts.notes_continued += usize::from(*continued);
        if !seen.insert(label.as_str()) {
            read_counts.notes_redefined += 1;
        }
    }
    read_counts.notes_in = order.len() - kept.len();
    for label in &order {
        if !cited.contains(label) && !kept.contains(label.as_str()) {
            read_counts.orphan_notes.push(label.clone());
        }
    }
    if !kept.is_empty() {
        notes.push(Problem::new(
            "markdown",
            "kept an uncited definition as its text: the day page reads only an `ev-` or `cx-` label's definition as a note",
        ));
    }
    if !read_counts.orphan_notes.is_empty() {
        notes.push(Problem::new(
            "markdown",
            format!(
                "left out {} no sentence cites: {}",
                match read_counts.orphan_notes.len() {
                    1 => "a source".to_string(),
                    n => format!("{n} sources"),
                },
                read_counts.orphan_notes.iter().map(|l| format!("[^{l}]")).collect::<Vec<_>>().join(", ")
            ),
        ));
    }
    if read_counts.notes_redefined > 0 {
        notes.push(Problem::new(
            "markdown",
            "a footnote is defined twice: its note is the last definition, as the day page reads it",
        ));
    }
    if read_counts.notes_continued > 0 {
        notes.push(Problem::new(
            "markdown",
            "a footnote's definition runs past its first line: the day page reads its first line alone",
        ));
    }
    if bad_labels > 0 {
        notes.push(Problem::new(
            "markdown",
            "kept a footnote as text: its label is not a letter or digit then letters, digits, `_` or `-`",
        ));
    }
    if nested_defs > 0 {
        notes.push(Problem::new(
            "markdown",
            "kept a footnote definition inside a quote or a list as text: the day page reads one only at a line's start",
        ));
    }
    *counts = read_counts;
    out
}

/// How many veils the readers find in a block's text, `⟦` up to the next `⟧`.
fn veils_in(text: &str) -> usize {
    let mut n = 0;
    let mut rest = text;
    while let Some(open) = rest.find('⟦') {
        let after = &rest[open + '⟦'.len_utf8()..];
        match after.find('⟧') {
            Some(close) => {
                n += 1;
                rest = &after[close + '⟧'.len_utf8()..];
            }
            None => break,
        }
    }
    n
}

/// Link reference definitions made the text they were
/// ([`definitions_as_text`]).
struct Definitions {
    /// The markdown with each definition escaped to read as text.
    markdown: String,
    labels: Labels,
    /// Where each definition's escaped text is in `markdown`: those escapes
    /// were put there, and read back to what was typed.
    escaped: Vec<Range<usize>>,
}

/// What each link label in a page's markdown stands for (`[label]: dest
/// "title"`), keyed by [`link_label`]: the first definition of a label, as
/// the parser resolves it.
type Labels = HashMap<String, (String, String)>;

/// A page's or a paste's link reference definitions as the text they were,
/// with what each label stands for, so a link that names one is still a
/// link; `None` when the markdown holds none. The parser takes a definition
/// out of what it renders; the page's editor showed it as the line it is,
/// and so did the text a paste was copied from. So each
/// definition's characters are escaped to be read as text, and a blank line
/// ends it, so what follows starts a block as it did after the definition
/// (a `---` stays a rule, not a heading's underline).
///
/// The parser keeps only a label's first definition: a later one, consumed
/// all the same, is found as a line no block's text covers that starts
/// with `[`.
fn definitions_as_text(md: &str) -> Option<Definitions> {
    let parser = Parser::new_ext(md, parse_options());
    let mut labels = Labels::new();
    let mut spans: Vec<Range<usize>> = vec![];
    for (label, def) in parser.reference_definitions().iter() {
        labels
            .entry(link_label(label))
            .or_insert_with(|| (def.dest.to_string(), def.title.as_deref().unwrap_or("").to_string()));
        spans.push(def.span.clone());
    }
    if spans.is_empty() {
        return None;
    }
    // The bytes some block's text covers. Lists and quotes cover their
    // markers' lines, so they cover nothing here.
    let mut covered = vec![false; md.len()];
    for (event, range) in Parser::new_ext(md, parse_options()).into_offset_iter() {
        let container = matches!(
            event,
            Event::Start(Tag::BlockQuote | Tag::List(_) | Tag::Item)
                | Event::End(TagEnd::BlockQuote | TagEnd::List(_) | TagEnd::Item)
        );
        if !container {
            covered[range].iter_mut().for_each(|c| *c = true);
        }
    }
    // A line's markers: quotes', and a list item's before a `[`.
    let markers = |line: &str| -> usize {
        let mut at = line.len() - line.trim_start_matches([' ', '\t', '>']).len();
        let rest = &line[at..];
        let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        let marker = if digits > 0 && digits <= 9 && rest[digits..].starts_with(['.', ')']) {
            digits + 1
        } else if rest.starts_with(['-', '*', '+']) {
            1
        } else {
            0
        };
        let after = &rest[marker..];
        let gap = after.len() - after.trim_start_matches([' ', '\t']).len();
        if marker > 0 && gap > 0 && after[gap..].starts_with('[') {
            at += marker + gap;
        }
        at
    };
    let mut line_start = 0;
    for line in md.split_inclusive('\n') {
        let at = line_start + markers(line);
        let content = line[at - line_start..].trim_end();
        let uncovered = (at..line_start + line.trim_end().len()).all(|i| !covered[i]);
        if content.starts_with('[') && uncovered && !spans.iter().any(|s| s.contains(&at)) {
            spans.push(at..line_start + line.trim_end().len());
        }
        line_start += line.len();
    }
    spans.sort_by_key(|s| s.start);

    let mut out = String::with_capacity(md.len() + md.len() / 4);
    let mut escaped = vec![];
    let mut from = 0;
    let starts: Vec<usize> = spans.iter().map(|s| s.start).collect();
    for span in &spans {
        if span.start < from {
            continue;
        }
        out.push_str(&md[from..span.start]);
        let mut first = true;
        let escaped_from = out.len();
        for line in md[span.clone()].split_inclusive('\n') {
            // A line after the first goes on past its quote's markers.
            let keep = if first { 0 } else { line.len() - line.trim_start_matches([' ', '\t', '>']).len() };
            first = false;
            out.push_str(&line[..keep]);
            for ch in line[keep..].chars() {
                if ch.is_ascii_punctuation() {
                    out.push('\\');
                }
                out.push(ch);
            }
        }
        escaped.push(escaped_from..out.len());
        from = span.end;
        // The rest of the definition's last line, then a blank line in the
        // blocks it is in when anything follows.
        let end = md[from..].find('\n').map_or(md.len(), |i| from + i + 1);
        out.push_str(&md[from..end]);
        from = end;
        let next = md[from..].split_inclusive('\n').next().unwrap_or("");
        // A definition on the next line goes on the run.
        let defines = starts.iter().any(|&at| at >= from && at < from + next.len());
        if !defines && !next.trim_start_matches([' ', '\t', '>']).trim().is_empty() {
            let line_start = md[..span.start].rfind('\n').map_or(0, |i| i + 1);
            let quotes: String = md[line_start..span.start].chars().filter(|&c| c == '>').collect();
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&quotes);
            out.push('\n');
        }
    }
    out.push_str(&md[from..]);
    Some(Definitions {
        markdown: out,
        labels,
        escaped,
    })
}

/// A link label as CommonMark matches it: any case, runs of white space as
/// one.
fn link_label(label: &str) -> String {
    label.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// The parser's options for our markdown.
fn parse_options() -> Options {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts
}

/// [`parse_options`] for `dialect`: an article's footnotes are read too.
fn parse_options_for(dialect: Dialect) -> Options {
    let mut opts = parse_options();
    if dialect == Dialect::Article {
        opts.insert(Options::ENABLE_FOOTNOTES);
    }
    opts
}

/// How an item carrying its own number opens in the converter's HTML:
/// `<li value="2021">`, written in place of the item's `<li>`.
const NUMBERED_ITEM: &str = "<li value=\"";

/// The number an ordered list's item was written with (`2021.` is 2021),
/// read from its marker in the source. `range` is the item's.
fn item_number(md: &str, range: &Range<usize>) -> Option<u64> {
    let marker = md.get(range.start..)?.trim_start_matches([' ', '\t']);
    let digits = marker.len() - marker.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    marker[..digits].parse().ok()
}

/// Whether each ordered list, in the order the lists open, is numbered
/// lazily: two items or more, each written with the first one's number
/// (`1.` `1.` `1.`). CommonMark counts such a list on from its first number,
/// as the chat shows a model's answer, and so does a writer's markdown or a
/// paste read here: kept as written, `1.` `1.` `1.` showed 1, 1, 1. A page's
/// markdown editor showed each number as typed, so a page
/// ([`Dialect::Page`]) keeps them.
fn lazily_numbered(md: &str, events: &Events) -> Vec<bool> {
    let mut out = vec![];
    // Each open list: its place in `out` when ordered, and its items' numbers.
    let mut open: Vec<(Option<usize>, Vec<Option<u64>>)> = vec![];
    for (ev, range) in events {
        match ev {
            Event::Start(Tag::List(first)) => {
                let slot = first.map(|_| {
                    out.push(false);
                    out.len() - 1
                });
                open.push((slot, vec![]));
            }
            Event::Start(Tag::Item) => {
                if let Some((_, numbers)) = open.last_mut() {
                    numbers.push(item_number(md, range));
                }
            }
            Event::End(TagEnd::List(_)) => {
                if let Some((Some(slot), numbers)) = open.pop() {
                    out[slot] = numbers.len() >= 2 && numbers.iter().all(|n| n.is_some() && *n == numbers[0]);
                }
            }
            _ => {}
        }
    }
    out
}

/// Whether each list item, in order, is a to-do: one whose first block
/// opens with a task marker.
fn task_items(events: &Events) -> Vec<bool> {
    events
        .iter()
        .enumerate()
        .filter(|(_, (e, _))| matches!(e, Event::Start(Tag::Item)))
        .map(|(i, _)| {
            let mut next = events[i + 1..].iter().map(|(e, _)| e);
            match next.next() {
                Some(Event::TaskListMarker(_)) => true,
                Some(Event::Start(Tag::Paragraph)) => matches!(next.next(), Some(Event::TaskListMarker(_))),
                _ => false,
            }
        })
        .collect()
}

/// How many lines a note on collapsed spaces quotes before it counts the rest.
const SPACES_QUOTED: usize = 5;

/// What blocks cannot hold of what a page's editor showed, each said as a
/// note ([`Dialect::Page`]): a run of spaces or a tab, or a line's indent,
/// which a block page shows as one space or none; a table row's cells past
/// its header's, which no row holds. `md` is the source the events' ranges
/// are in.
fn page_changes(md: &str, events: &Events, notes: &mut Vec<Problem>) {
    let clip = |s: &str| -> String {
        let s = s.trim();
        match s.char_indices().nth(60) {
            Some((at, _)) => format!("{}…", &s[..at]),
            None => s.to_string(),
        }
    };
    let line_of = |at: usize| -> (usize, &str) {
        let start = md[..at].rfind('\n').map_or(0, |i| i + 1);
        let end = md[at..].find('\n').map_or(md.len(), |i| at + i);
        (start, &md[start..end])
    };
    let is_ws = |c: char| matches!(c, ' ' | '\t' | '\r' | '\n' | '\u{000c}');
    let has_run = |t: &str| {
        let mut last = false;
        t.chars().any(|c| {
            let ws = is_ws(c);
            let run = (ws && last) || matches!(c, '\t' | '\r' | '\u{000c}');
            last = ws;
            run
        })
    };

    // Blocks around the text: lists, quotes and tables, whose lines start
    // past their own markers.
    let mut depth = 0usize;
    let mut in_code = 0usize;
    let mut at_line_start = false;
    let mut ws_before = false;
    let mut spaced: Vec<usize> = vec![];
    // A table row's last cell's end.
    let mut cell_end: Option<usize> = None;
    for (ev, range) in events {
        match ev {
            Event::Start(Tag::Item) => {
                at_line_start = true;
                ws_before = false;
            }
            Event::Start(Tag::List(_) | Tag::BlockQuote | Tag::Table(_)) => depth += 1,
            Event::End(TagEnd::List(_) | TagEnd::BlockQuote | TagEnd::Table) => depth = depth.saturating_sub(1),
            Event::Start(Tag::CodeBlock(_)) => in_code += 1,
            Event::End(TagEnd::CodeBlock) => in_code = in_code.saturating_sub(1),
            Event::Start(Tag::TableHead | Tag::TableRow) => cell_end = None,
            Event::End(TagEnd::TableCell) => cell_end = Some(range.end),
            Event::End(TagEnd::TableHead | TagEnd::TableRow) => {
                if let Some(end) = cell_end.filter(|&e| e <= range.end) {
                    let rest = md[end..range.end].trim().trim_matches('|').trim();
                    if !rest.is_empty() {
                        notes.push(Problem::new(
                            "markdown",
                            format!(
                                "dropped \"{}\" from a table row: a row holds as many cells as its table's header",
                                clip(rest)
                            ),
                        ));
                    }
                }
            }
            Event::Text(t) | Event::Code(t) if in_code == 0 => {
                let (start, _) = line_of(range.start);
                let before = &md[start..range.start.max(start)];
                let indent = at_line_start && depth == 0 && !before.is_empty() && before.chars().all(|c| c == ' ' || c == '\t');
                let leading = at_line_start && t.starts_with(is_ws);
                let run = has_run(t) || (ws_before && t.starts_with(is_ws));
                if (indent || leading || run) && spaced.last() != Some(&start) {
                    spaced.push(start);
                }
                ws_before = t.ends_with(is_ws);
                at_line_start = false;
            }
            Event::SoftBreak | Event::HardBreak => {
                at_line_start = true;
                ws_before = false;
            }
            Event::Start(Tag::Paragraph | Tag::Heading { .. } | Tag::TableCell) => {
                at_line_start = true;
                ws_before = false;
            }
            _ => {}
        }
    }
    for &start in spaced.iter().take(SPACES_QUOTED) {
        let (_, line) = line_of(start);
        notes.push(Problem::new(
            "markdown",
            format!(
                "spaces collapsed in \"{}\": a block page shows a run of spaces or a tab as one space, and starts a line at its edge",
                clip(line)
            ),
        ));
    }
    if spaced.len() > SPACES_QUOTED {
        notes.push(Problem::new(
            "markdown",
            format!("spaces collapsed in {} more lines", spaced.len() - SPACES_QUOTED),
        ));
    }
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
    to_html_holding(md, dialect).map(|r| (r.html, r.notes))
}

/// What [`to_html_holding`] makes of markdown.
struct Rendering {
    html: String,
    notes: Vec<Problem>,
    /// Pending proposals kept as text for holding what a suggestion cannot.
    held: usize,
    /// The article syntax it read, for [`Dialect::Article`]: the `_in`
    /// counts, and the orphans and definitions it read otherwise.
    article: Option<ArticleCounts>,
}

/// [`to_html_as`], with how many pending proposals it kept as text for
/// holding what a suggestion cannot (a mention, an image, a line break), and
/// what article syntax it read.
fn to_html_holding(md: &str, dialect: Dialect) -> Result<Rendering, Problem> {
    let mut notes = vec![];
    let mut held = 0;
    let mut article = ArticleCounts::default();
    // No HTML holds a NUL: its parser drops one without a word.
    let nul_free = match dialect {
        Dialect::Page => nuls_replaced(md, &mut notes),
        Dialect::Written | Dialect::Paste | Dialect::Article => None,
    };
    let md = nul_free.as_deref().unwrap_or(md);
    let apart = match dialect {
        Dialect::Paste => lazy_lines_apart(md),
        Dialect::Written | Dialect::Page | Dialect::Article => None,
    };
    let md = apart.as_deref().unwrap_or(md);
    let wide = match dialect {
        Dialect::Paste => tables_widened(md),
        Dialect::Written | Dialect::Page | Dialect::Article => None,
    };
    let md = wide.as_deref().unwrap_or(md);
    // A page's definitions are the lines its editor showed, and a paste's
    // the lines of the text it was copied from.
    let defs = match dialect {
        Dialect::Page | Dialect::Paste => definitions_as_text(md),
        Dialect::Written | Dialect::Article => None,
    };
    let (md, labels, escaped) = match &defs {
        Some(d) => (d.markdown.as_str(), Some(&d.labels), d.escaped.as_slice()),
        None => (md, None, &[][..]),
    };
    let out = events_as(md, dialect, labels, escaped, &mut notes, &mut held, &mut article)?;
    if nodes_at_least(&out) > crate::MAX_NODES {
        return Err(too_many_parts());
    }
    let mut html = Capped {
        html: vec![],
        limit: MAX_INPUT_BYTES,
    };
    if write_html(&mut html, out.into_iter()).is_err() {
        return Err(too_much_html());
    }
    let html = String::from_utf8(html.html).map_err(|e| Problem::new("markdown", e.to_string()))?;
    Ok(Rendering {
        html,
        notes,
        held,
        article: (dialect == Dialect::Article).then_some(article),
    })
}

/// The events [`to_html_as`] writes as HTML, the dialect mapped onto
/// contract tags, with a note in `notes` for each dialect feature the
/// contract has no node for; refused past [`MAX_EVENTS`]. `labels`: what
/// each link label stands for, when the markdown's definitions were made
/// its text ([`definitions_as_text`]). `held` counts the pending proposals
/// kept as text for holding what a suggestion cannot ([`critic_markup`]).
fn events_as<'a>(
    md: &'a str,
    dialect: Dialect,
    labels: Option<&Labels>,
    escaped: &[Range<usize>],
    notes: &mut Vec<Problem>,
    held: &mut usize,
    counts: &mut ArticleCounts,
) -> Result<Vec<Event<'a>>, Problem> {
    let opts = parse_options_for(dialect);
    // Read here, not by the parser: its own reading of front matter never
    // returns on some markdown that only looks like it (an indented `---`
    // with a `---` line further on). A page's editor read no front matter:
    // it showed those lines as a rule and the text under it. Nor does a
    // paste: its lines are what the person copied, and a paste says nothing
    // of what it leaves out. Nor does an article: its writers write none,
    // and its readers showed those lines (the day page as its first
    // paragraph, the wiki as a rule and a heading), as an export opening
    // with a rule writes them.
    let front_matter = match dialect {
        Dialect::Page | Dialect::Paste | Dialect::Article => None,
        Dialect::Written => front_matter_end(md),
    };
    let md = match front_matter {
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
    let named = |link: BrokenLink<'a>| -> Option<(CowStr<'a>, CowStr<'a>)> {
        let (dest, title) = labels?.get(&link_label(&link.reference))?;
        Some((CowStr::from(dest.clone()), CowStr::from(title.clone())))
    };
    for event in Parser::new_with_broken_link_callback(md, opts, Some(named)).into_offset_iter() {
        if parsed.len() == MAX_EVENTS {
            return Err(too_complex());
        }
        parsed.push(event);
    }
    // An article's raw tag that is no markup (`<unk>`, `Vec<String>`) is
    // the text its readers drew, as a paste's is; HTML the export writes
    // (`<u>`, `<br>`, an applet) stays markup, so an export reads back.
    let parsed = match dialect {
        Dialect::Written => parsed,
        Dialect::Paste | Dialect::Article => literal_tags(parsed),
        Dialect::Page => single_tildes(md, page_tags(parsed)),
    };
    let events = bare_task_markers(md, parsed);
    let events = alerts(md, events, notes);
    let events = match dialect {
        Dialect::Written | Dialect::Page | Dialect::Article => {
            critic_markup(md, events, dialect == Dialect::Page, notes, held, &mut counts.suggestions_over_nodes)?
        }
        Dialect::Paste => events,
    };
    let events = match dialect {
        Dialect::Article => article_syntax(md, events, notes, counts),
        Dialect::Written | Dialect::Paste | Dialect::Page => events,
    };
    let lines_kept = |events: Events<'a>| -> Events<'a> {
        events
            .into_iter()
            .map(|(e, r)| (if matches!(e, Event::SoftBreak) { Event::HardBreak } else { e }, r))
            .collect()
    };
    // A page's editor showed each line of a paragraph as its own line, and
    // so did the text a paste was copied from.
    let events = match dialect {
        Dialect::Page => {
            let events = page_text(md, events, escaped, notes);
            page_changes(md, &events, notes);
            lines_kept(events)
        }
        Dialect::Paste => lines_kept(events),
        Dialect::Written | Dialect::Article => events,
    };
    // Each open list: for an ordered one, the number its next item takes
    // by the count, whether the item before it was a to-do, and whether its
    // items count on whatever they were written with.
    let mut lists: Vec<Option<(u64, bool, bool)>> = vec![];
    let mut tasks = task_items(&events).into_iter();
    let mut lazy = match dialect {
        Dialect::Page => vec![],
        Dialect::Written | Dialect::Paste | Dialect::Article => lazily_numbered(md, &events),
    }
    .into_iter();
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
                    // A page's image carries no link: the image stays, and
                    // the link's address is said.
                    if buf.iter().any(|e| matches!(e, Event::Start(Tag::Image { .. }))) {
                        notes.push(Problem::new(
                            "markdown",
                            format!("dropped the link to {dest_url} around an image, which a page's image cannot carry"),
                        ));
                        if let Some((_, buf, _)) = buffer.take() {
                            out.extend(buf);
                        }
                        continue;
                    }
                    out.push(Event::InlineHtml(CowStr::from(format!(
                        "<a href=\"{}\">",
                        esc(dest_url)
                    ))));
                    // A link with no text is no link a page can hold: it
                    // keeps its address as its text.
                    if buf.iter().all(|e| matches!(e, Event::Text(t) if t.trim().is_empty())) {
                        notes.push(Problem::new(
                            "markdown",
                            format!("a link with no text to {dest_url} took its address as its text"),
                        ));
                        buf.push(Event::Text(CowStr::from(dest_url.to_string())));
                    }
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
                (Event::End(TagEnd::Image), Tag::Image { dest_url, title, .. }) => {
                    if !title.is_empty() {
                        notes.push(Problem::new(
                            "markdown",
                            format!("dropped the title \"{title}\" of an embed of {dest_url}"),
                        ));
                    }
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
            // A link to a label a page's definition gave, as its text, is
            // one the parser calls unknown: it is the same link.
            Event::Start(
                t @ Tag::Link {
                    link_type:
                        LinkType::Inline
                        | LinkType::Reference
                        | LinkType::Collapsed
                        | LinkType::Shortcut
                        | LinkType::ReferenceUnknown
                        | LinkType::CollapsedUnknown
                        | LinkType::ShortcutUnknown,
                    ..
                },
            ) => {
                buffer = Some((t, vec![], vec![]));
            }
            Event::Start(t @ Tag::Image { .. }) => buffer = Some((t, vec![], vec![])),
            Event::Text(t) if in_code == 0 => highlight(t, &mut out),
            Event::Start(Tag::List(first)) => {
                let counted = first.is_some() && lazy.next().unwrap_or(false);
                lists.push(first.map(|n| (n, false, counted)));
                out.push(Event::Start(Tag::List(first)));
            }
            Event::End(TagEnd::List(ordered)) => {
                lists.pop();
                out.push(Event::End(TagEnd::List(ordered)));
            }
            // An ordered item keeps the number it was written with: where
            // that is not the count's (`2019.`, `2021.`), it is the item's
            // own. So is the number of an item after a to-do, whatever it
            // is: ingest makes the to-dos a list of their own, and numbers
            // the list after them from its first item. A list numbered
            // lazily ([`lazily_numbered`]) counts instead.
            Event::Start(Tag::Item) => {
                let task = tasks.next().unwrap_or(false);
                match lists.last_mut() {
                    Some(Some((next, after_task, counted))) => {
                        let n = if *counted { *next } else { item_number(md, &range).unwrap_or(*next) };
                        let own = !task && (n != *next || *after_task);
                        *next = n.saturating_add(1);
                        *after_task = task;
                        out.push(if own {
                            Event::Html(CowStr::from(format!("{NUMBERED_ITEM}{n}\">")))
                        } else {
                            Event::Start(Tag::Item)
                        });
                    }
                    _ => out.push(Event::Start(Tag::Item)),
                }
            }
            other => out.push(other),
        }
    }
    if out.len() > MAX_EVENTS {
        return Err(too_complex());
    }
    Ok(out)
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

/// Where a line breaks inside a block, in [`html_text`] and [`tree_text`]:
/// no whitespace, so the measure's squeeze keeps it.
const LINE: char = '↵';

/// The words of an article's atom, for [`html_text`] and [`tree_text`] to
/// compare: a footnote as `[^label] note`, a figure as its values in the
/// contract's order. `attr` reads one by its name in the contract and in
/// HTML. Without them, a definition moved into a footnote's note would read
/// as text lost.
fn atom_words(kind: &str, attr: &dyn Fn(&str, &str) -> Option<String>) -> Option<String> {
    let c = crate::contract();
    match kind {
        "footnote" => Some(format!(
            "[^{}] {}",
            attr("label", "label").unwrap_or_default(),
            attr("note", "note").unwrap_or_default()
        )),
        "figure" => {
            let spec = c.node("figure")?;
            let values: Vec<String> = spec
                .attrs()
                .iter()
                .filter_map(|(key, a)| attr(key, a.html.as_deref().unwrap_or(key)))
                .collect();
            Some(values.join(" "))
        }
        _ => None,
    }
}

/// Visible text of the HTML the markdown renders to, a line break as [`LINE`].
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
            scraper::Node::Element(e) if matches!(e.name(), "virtues-footnote" | "virtues-figure") => {
                let kind = if e.name() == "virtues-footnote" { "footnote" } else { "figure" };
                if let Some(words) = atom_words(kind, &|_, html| e.attr(html).map(str::to_string)) {
                    s.push_str(&words);
                    s.push(' ');
                }
            }
            scraper::Node::Element(e) if e.name() == "br" => {
                s.push(LINE);
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
                s.push(LINE);
                s.push(' ');
            } else if let Some(words) =
                atom_words(&n.kind, &|key, _| n.attrs.get(key).and_then(|v| v.as_str()).map(str::to_string))
            {
                s.push_str(&words);
                s.push(' ');
            }
        });
        s.push(' ');
    }
    collapse(&s)
}

/// Every address a link or an embed in `md` names, as the parser reads it
/// in `dialect`, with a link's text: what [`measure_as`] holds the blocks
/// to.
fn addresses_in(md: &str, dialect: Dialect) -> Vec<String> {
    let md = front_matter_end(md).map_or(md, |end| &md[end..]);
    let mut out = vec![];
    for event in Parser::new_ext(md, parse_options_for(dialect)).take(MAX_EVENTS) {
        match event {
            // `<nick@example.com>`: the parser gives the address bare, and
            // its link holds it as `mailto:`.
            Event::Start(Tag::Link {
                link_type: LinkType::Email,
                dest_url,
                ..
            }) => out.push(format!("mailto:{dest_url}")),
            Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) if !dest_url.trim().is_empty() => {
                out.push(dest_url.to_string());
            }
            _ => {}
        }
    }
    out
}

/// Every address the blocks hold: a link's, an embed's, a mention's.
fn addresses_held(nodes: &[Node]) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for n in nodes {
        n.walk(&mut |n| {
            for key in ["src", "to", "href"] {
                if let Some(v) = n.attrs.get(key).and_then(|v| v.as_str()) {
                    out.insert(normalize_url(v));
                }
            }
            for m in &n.marks {
                if let Some(v) = m.attrs.get("href").and_then(|v| v.as_str()) {
                    out.insert(normalize_url(v));
                }
            }
        });
    }
    out
}

/// The raw HTML in a page's markdown that its editor showed as text, a
/// line at a time: everything but the underline it drew ([`page_tags`]).
fn raw_html_lines(md: &str) -> Vec<String> {
    let md = front_matter_end(md).map_or(md, |end| &md[end..]);
    let mut out = vec![];
    let mut in_code = false;
    for event in Parser::new_ext(md, parse_options()).take(MAX_EVENTS) {
        match event {
            Event::Start(Tag::CodeBlock(_)) => in_code = true,
            Event::End(TagEnd::CodeBlock) => in_code = false,
            Event::Html(h) | Event::InlineHtml(h) if !in_code => {
                if h.as_ref() == UNDERLINE.0 || h.as_ref() == UNDERLINE.1 {
                    continue;
                }
                out.extend(h.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string));
            }
            _ => {}
        }
    }
    out
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

/// HTML's own elements, by name. A note's kind names a tag only when it is
/// one of these or one the contract reads: markdown reads `<Nick>` as a tag,
/// so any other name may be the page's own words.
const HTML_TAGS: &[&str] = &[
    "a", "abbr", "address", "area", "article", "aside", "audio", "b", "base", "bdi", "bdo", "big",
    "blockquote", "body", "br", "button", "canvas", "caption", "center", "cite", "code", "col",
    "colgroup", "data", "datalist", "dd", "del", "details", "dfn", "dialog", "div", "dl", "dt", "em",
    "embed", "fieldset", "figcaption", "figure", "font", "footer", "form", "frame", "frameset", "h1",
    "h2", "h3", "h4", "h5", "h6", "head", "header", "hgroup", "hr", "html", "i", "iframe", "img",
    "input", "ins", "kbd", "label", "legend", "li", "link", "main", "map", "mark", "marquee", "math",
    "menu", "meta", "meter", "nav", "noscript", "object", "ol", "optgroup", "option", "output", "p",
    "param", "picture", "pre", "progress", "q", "rp", "rt", "ruby", "s", "samp", "script", "search",
    "section", "select", "slot", "small", "source", "span", "strike", "strong", "style", "sub",
    "summary", "sup", "svg", "table", "tbody", "td", "template", "textarea", "tfoot", "th", "thead",
    "time", "title", "tr", "track", "tt", "u", "ul", "var", "video", "wbr",
];

/// Whether `tag` is a name a note's kind may carry: HTML's own, or one the
/// contract reads.
fn known_tag(tag: &str) -> bool {
    HTML_TAGS.contains(&tag) || !crate::contract().node_rules_for_tag(tag).is_empty()
}

/// Whether `name` is a node or a mark the contract names.
fn contract_name(name: &str) -> bool {
    let c = crate::contract();
    c.node(name).is_some() || c.mark(name).is_some()
}

/// A note's kind, for counting notes across documents and for the server's
/// log: a fixed phrase, naming a tag, a node or a mark only when HTML or the
/// contract does ([`known_tag`], [`contract_name`]). Never the note's own
/// words, which quote the page (`spaces collapsed in "…"`, a link's title or
/// address, a code block's info string): a log line, and a table printed
/// from a report, say what kind of change a page went through, never what
/// the page says. A note of no kind here is `other`.
pub fn category(msg: &str) -> String {
    if msg.contains("; got `") {
        return "attribute dropped".into();
    }
    if msg.starts_with("dropped style") {
        return "style dropped".into();
    }
    if msg.starts_with("dropped `") {
        return "copied block id dropped".into();
    }
    if let Some(rest) = msg.strip_prefix("dropped the ") {
        let kind = if rest.starts_with("title ") {
            if rest.contains(" of an embed of ") {
                "embed title dropped"
            } else {
                "link title dropped"
            }
        } else if rest.starts_with("link to ") {
            "link around an image dropped"
        } else if rest.starts_with("width ") {
            "embed width dropped"
        } else if rest.starts_with("value of `") {
            "attribute value dropped"
        } else {
            match rest.split_once(':') {
                Some((name, _)) if contract_name(name) => return format!("{name} dropped"),
                _ => "formatting dropped",
            }
        };
        return kind.into();
    }
    for verb in ["unwrapped", "dropped", "moved"] {
        if let Some(rest) = msg.strip_prefix(verb).and_then(|r| r.strip_prefix(" <")) {
            let tag = rest.split('>').next().unwrap_or("");
            return if known_tag(tag) {
                format!("{verb} <{tag}>")
            } else {
                format!("{verb} an unknown tag")
            };
        }
    }
    if let Some(rest) = msg.strip_prefix('<') {
        let tag = rest.split('>').next().unwrap_or("");
        let named = if known_tag(tag) { format!("<{tag}>") } else { "an unknown tag".into() };
        if msg.contains("does not carry formatting") {
            return format!("{named} without formatting dropped");
        }
        if msg.contains("is part of a table") {
            return format!("{named} outside a table kept as text");
        }
        if msg.contains("whole document's frame") {
            return format!("{named} document frame dropped");
        }
    }
    if msg.contains("cannot be combined") {
        let marks: Vec<&str> = msg.split('`').skip(1).step_by(2).take(2).collect();
        return match marks.as_slice() {
            [a, b] if contract_name(a) && contract_name(b) => format!("mark clash: `{a}` cannot be combined with `{b}`"),
            _ => "mark clash".into(),
        };
    }
    let kinds = [
        ("more notes left out", "notes left out"),
        ("evened out a table", "table evened out"),
        ("kept a table as it is", "table kept as it is"),
        ("dropped a table with no cells", "empty table dropped"),
        ("dropped a table's empty header row", "empty header row dropped"),
        ("an ordered list's tasks became a task list", "numbered tasks became a task list"),
        ("split a list of tasks and other items", "task list split"),
        ("alert became a callout", "alert became a callout"),
        ("{>>…<<} as text", "comments kept as text"),
        ("{~~…~>…~~} as text", "substitutions kept as text"),
        ("{-- or {++ as text", "unclosed proposals kept as text"),
        ("different formatting", "proposals across formatting kept as text"),
        ("a mention, an image or a line break", "proposals holding more than text kept as text"),
        ("as U+FFFD", "NUL written as U+FFFD"),
        ("after a code block's language", "code block info dropped"),
        ("from a table row", "table cells dropped"),
        ("spaces collapsed in", "spaces collapsed"),
        ("front matter", "front matter left out"),
        ("as a link, not a mention", "link kept, not a mention"),
        ("took its address as its text", "empty link took its address"),
        ("wrote its suggestions as accepted", "suggestions accepted"),
        ("no sentence cites", "orphan footnote left out"),
        ("a footnote is defined twice", "footnote defined twice"),
        ("runs past its first line", "footnote definition continued"),
        ("kept a footnote as text", "footnote label kept as text"),
        ("kept a footnote definition inside", "footnote definition in a quote or list kept as text"),
        ("kept an uncited definition", "uncited definition kept as text"),
        ("kept a figure as a code block", "figure kept as code"),
    ];
    kinds
        .iter()
        .find(|(sign, _)| msg.contains(sign))
        .map_or_else(|| "other".into(), |(_, kind)| (*kind).into())
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

/// [`measure_as`]'s judgment of `nodes`, made from `md` read as `dialect`,
/// whose rendering is `html`: where a character of the text is missing,
/// and the addresses no block holds and no note in `notes` names.
fn judge(md: &str, dialect: Dialect, html: &str, nodes: &[Node], notes: &[Problem]) -> (Option<TextLoss>, Vec<String>) {
    let before = html_text(html);
    let after = tree_text(nodes);
    // Spacing between inline runs is the measure's own noise; a lost
    // character is not, and in a page neither is a line run into the next.
    let lines = dialect == Dialect::Page;
    let squeeze = |s: &str| {
        s.chars()
            .filter(|c| !c.is_whitespace() && (lines || *c != LINE))
            .collect::<String>()
    };
    let flat = |s: &str| s.chars().filter(|c| !c.is_whitespace() && *c != LINE).collect::<String>();
    let html_kept = |line: &String| flat(&after).contains(&flat(line));
    // What the page's editor showed that the blocks lack, read from the
    // markdown itself: raw HTML it showed as text, or a backslash typed at
    // a line's end, which the rendering drops as the conversion would.
    let raw_lost = match dialect {
        Dialect::Page => raw_html_lines(md).into_iter().find(|line| !html_kept(line)).or_else(|| {
            let typed = backslash_breaks(md);
            (backslash_breaks_kept(nodes) < typed.len()).then(|| typed[0].trim().to_string())
        }),
        Dialect::Written | Dialect::Paste | Dialect::Article => None,
    };
    let text_lost = (squeeze(&before) != squeeze(&after) || raw_lost.is_some()).then(|| {
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
        match &raw_lost {
            // What the editor showed, missing: where it went is not in the
            // rendering, so the report shows the line itself.
            Some(line) => TextLoss {
                at_char: common,
                html_text_len: before.chars().count(),
                tree_text_len: after.chars().count(),
                html_around: line.clone(),
                tree_around: around(&after),
            },
            None => TextLoss {
                at_char: common,
                html_text_len: before.chars().count(),
                tree_text_len: after.chars().count(),
                html_around: around(&before),
                tree_around: around(&after),
            },
        }
    });
    let held = addresses_held(nodes);
    let mut addresses_lost: Vec<String> = addresses_in(md, dialect)
        .into_iter()
        .filter(|a| !held.contains(&normalize_url(a)) && !notes.iter().any(|n| n.message.contains(a.as_str())))
        .collect();
    addresses_lost.dedup();
    (text_lost, addresses_lost)
}

/// One document converted and judged.
#[derive(Debug, Serialize)]
pub struct Measured {
    pub outcome: Outcome,
    /// Set when a character of the rendered markdown is missing from the tree.
    pub text_lost: Option<TextLoss>,
    /// Addresses a link or an embed in the markdown names that no block
    /// holds and no note names: a link's or an image's target dropped.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub addresses_lost: Vec<String>,
    /// The first difference between the tree and its markdown export read
    /// back in; `None` when they are the same tree.
    pub export_diff: Option<String>,
    /// Pending proposals kept as their text, since each holds what a
    /// suggestion cannot: a mention, an image, a line break. Its owner could
    /// accept or reject each one, and in the tree can do neither.
    #[serde(skip_serializing_if = "is_zero")]
    pub proposals_as_text: usize,
    /// The article syntax counted on the way in and in the blocks, for
    /// markdown read as an article ([`Dialect::Article`]); `None` otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub article: Option<ArticleCounts>,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// An article's syntax as its markdown holds it (`_in`) and as its blocks
/// do (`_out`). An article moves to blocks only when [`ArticleCounts::holds`]:
/// every reference is a footnote, every definition a footnote's note or a
/// source no sentence cites, every figure a figure or the fence it was, and
/// no definition that the day page's readers would read otherwise.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ArticleCounts {
    /// Footnote references (`[^ev-1]`) in the markdown, bare markers with no
    /// definition among them.
    pub refs_in: usize,
    /// Footnotes in the blocks.
    pub refs_out: usize,
    /// Footnote definitions in the markdown, one per label.
    pub notes_in: usize,
    /// Distinct labels whose footnotes in the blocks carry a note.
    pub notes_out: usize,
    /// The labels of definitions no reference cites, left out of the blocks.
    pub orphan_notes: Vec<String>,
    /// Definitions running past their first line, which the readers read
    /// only the first line of.
    pub notes_continued: usize,
    /// Labels defined more than once.
    pub notes_redefined: usize,
    /// Fences whose info string is `figure`.
    pub figures_in: usize,
    /// Figures in the blocks.
    pub figures_out: usize,
    /// Figure fences kept as code blocks, which the readers draw as before.
    pub figures_as_code: usize,
    /// Veils (`⟦…⟧`) in the markdown's text runs.
    pub veils_in: usize,
    /// Runs of text under the veil mark in the blocks.
    pub veils_out: usize,
    /// Pending suggestions around a footnote, a mention, a line break or an
    /// image: a suggestion's marks cover text alone, so in blocks accepting
    /// or rejecting one would leave that node behind or keep it alone,
    /// where the markdown editor took the whole span.
    #[serde(skip_serializing_if = "is_zero")]
    pub suggestions_over_nodes: usize,
}

impl ArticleCounts {
    /// Count what `nodes`, the blocks the markdown made, hold.
    fn count_out(&mut self, nodes: &[Node]) {
        let mut noted = std::collections::HashSet::new();
        for n in nodes {
            n.walk(&mut |n| match n.kind.as_str() {
                "footnote" => {
                    self.refs_out += 1;
                    if let (Some(label), Some(_)) = (
                        n.attrs.get("label").and_then(|v| v.as_str()),
                        n.attrs.get("note").and_then(|v| v.as_str()),
                    ) {
                        noted.insert(label.to_string());
                    }
                }
                "figure" => self.figures_out += 1,
                _ if n.marks.iter().any(|m| m.kind == "veil") => self.veils_out += 1,
                _ => {}
            });
        }
        self.notes_out = noted.len();
    }

    /// Whether the blocks hold the article's syntax as its readers read it.
    pub fn holds(&self) -> bool {
        self.refs_in == self.refs_out
            && self.notes_in == self.notes_out + self.orphan_notes.len()
            && self.figures_in == self.figures_out + self.figures_as_code
            && self.notes_continued == 0
            && self.notes_redefined == 0
            && self.suggestions_over_nodes == 0
    }
}

/// Convert `md` and check the conversion: no text lost, and the export reads
/// back to the same tree. A refused document is returned with neither check.
pub fn measure(c: &Contract, md: &str) -> Measured {
    measure_as(c, md, Dialect::Written)
}

/// [`measure`], read as `dialect`. Judged beside the HTML the markdown
/// renders to, and, since that rendering is the converter's own, against the
/// markdown itself too: every address a link or an embed names is on a
/// block or in a note; and for a page ([`Dialect::Page`]), every line of a
/// paragraph is a line of the blocks, every run of raw HTML its editor
/// showed as text is text of the blocks, and so is every backslash typed at
/// a line's end.
pub fn measure_as(c: &Contract, md: &str, dialect: Dialect) -> Measured {
    let outcome = from_markdown_as(c, md, dialect);
    if !outcome.errors.is_empty() {
        return Measured {
            outcome,
            text_lost: None,
            addresses_lost: vec![],
            export_diff: None,
            proposals_as_text: 0,
            article: None,
        };
    }
    // The same markdown converted just now, so it renders the same.
    let Ok(Rendering { html, held: proposals_as_text, article, .. }) = to_html_holding(md, dialect) else {
        return Measured {
            outcome,
            text_lost: None,
            addresses_lost: vec![],
            export_diff: None,
            proposals_as_text: 0,
            article: None,
        };
    };
    let (text_lost, addresses_lost) = judge(md, dialect, &html, &outcome.nodes, &outcome.notes);
    let article = article.map(|mut counts| {
        counts.count_out(&outcome.nodes);
        counts
    });
    let exported = render::markdown(c, &outcome.nodes);
    // The export is a writer's markdown, an article's read as an article.
    let back = match dialect {
        Dialect::Article => from_markdown_as(c, &exported, Dialect::Article),
        Dialect::Written | Dialect::Paste | Dialect::Page => from_markdown(c, &exported),
    };
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
        addresses_lost,
        export_diff,
        proposals_as_text,
        article,
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
        // Numbered otherwise than on, the list after the to-dos starts at
        // its first item's own number, and its items keep theirs.
        let o = from_markdown(&c, "1. a\n5. [ ] b\n6. c\n9. d\n10. e\n");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        assert_eq!(kinds(&o), ["orderedList", "taskList", "orderedList"]);
        assert_eq!(numbers(&o.nodes[2]), [(6, None), (9, Some(9)), (10, None)]);
        assert_eq!(o.nodes[2].attrs["start"], json!(6));
    }

    /// An ordered list's items as numbered: (number, its own `value`).
    fn numbers(list: &Node) -> Vec<(i64, Option<i64>)> {
        let start = list.attrs["start"].as_i64().unwrap();
        crate::model::item_numbers(start, &list.content)
            .into_iter()
            .zip(&list.content)
            .map(|(n, item)| (n, item.attrs.get("value").and_then(Value::as_i64)))
            .collect()
    }

    /// An ordered list keeps the number each item was written with: an item
    /// numbered otherwise than one past the item before it carries its own
    /// (`<li value>`), and the export writes every item's, so it reads back
    /// to the same list. A list numbered 2019, 2021, 2024 was numbered on
    /// from 2019, which changed what the page said.
    #[test]
    fn an_ordered_list_keeps_each_items_number() {
        let c = Contract::load();
        for (md, start, want) in [
            // Years.
            (
                "2019. Moved to the coast\n2021. Started the boat\n2024. Sold it\n",
                2019,
                vec![(2019, None), (2021, Some(2021)), (2024, Some(2024))],
            ),
            // A gap, the count going on after it.
            ("1. a\n2. b\n7. c\n8. d\n", 1, vec![(1, None), (2, None), (7, Some(7)), (8, None)]),
            // Descending.
            ("3. c\n2. b\n1. a\n", 3, vec![(3, None), (2, Some(2)), (1, Some(1))]),
            // From zero.
            ("0. zero\n1. one\n2. two\n", 0, vec![(0, None), (1, None), (2, None)]),
            // Leading zeros are the number's.
            ("007. a\n009. b\n", 7, vec![(7, None), (9, Some(9))]),
            // An ordinary list carries no number of its own.
            ("1. one\n2. two\n3. three\n", 1, vec![(1, None), (2, None), (3, None)]),
            // Loose, and in a quote.
            ("> 5. a\n>\n> 7. b\n", 5, vec![(5, None), (7, Some(7))]),
        ] {
            for dialect in [Dialect::Written, Dialect::Paste, Dialect::Page] {
                let m = measure_as(&c, md, dialect);
                assert!(m.outcome.errors.is_empty(), "{md:?}: {:?}", m.outcome.errors);
                let mut list = &m.outcome.nodes[0];
                if list.kind == "blockquote" {
                    list = &list.content[0];
                }
                assert_eq!(list.kind, "orderedList", "{md:?}");
                assert_eq!(list.attrs["start"], json!(start), "{md:?}");
                assert_eq!(numbers(list), want, "{md:?} {dialect:?}");
                assert!(m.outcome.notes.is_empty(), "{md:?} {dialect:?}: {:?}", m.outcome.notes);
                assert_eq!(m.export_diff, None, "{md:?} {dialect:?}");
                assert!(m.text_lost.is_none(), "{md:?}: {:?}", m.text_lost);
            }
        }
        // Nested, each list counts its own items. (A list that opens
        // inside a paragraph's lines starts at 1, so a blank line opens it.)
        let o = from_markdown(&c, "1. a\n\n   4. x\n   6. y\n\n3. b\n");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        assert_eq!(numbers(&o.nodes[0]), [(1, None), (3, Some(3))]);
        let inner = &o.nodes[0].content[0].content[1];
        assert_eq!(numbers(inner), [(4, None), (6, Some(6))]);
        // The HTML writes a value only where an item's number departs from
        // the count, and the export every item's number.
        assert_eq!(
            render::html(&c, &o.nodes, &render::Options { ids: false }),
            "<ol><li><p>a</p><ol start=\"4\"><li><p>x</p></li><li value=\"6\"><p>y</p></li></ol></li>\
             <li value=\"3\"><p>b</p></li></ol>"
        );
        assert_eq!(render::markdown(&c, &o.nodes), "1. a\n\n   4. x\n   6. y\n\n3. b\n");
        let o = from_markdown(&c, "1. one\n2. two\n3. three\n");
        assert!(!render::html(&c, &o.nodes, &render::Options { ids: false }).contains("value"));
    }

    /// A list whose every item is written with the first one's number
    /// (`1.` `1.` `1.`) is numbered lazily: CommonMark counts it on, as the
    /// chat shows a model's answer, so a writer's markdown and a paste count
    /// it too. Kept as written, a pasted README's steps all read 1. A page's
    /// markdown editor showed each number as typed, and a page keeps them;
    /// its export, read back as anyone's markdown, counts on.
    #[test]
    fn a_list_numbered_lazily_counts_on_but_in_a_page() {
        let c = Contract::load();
        for (md, start, counted) in [
            ("1. Install\n1. Configure\n1. Run\n", 1, vec![1, 2, 3]),
            ("3. a\n3. b\n", 3, vec![3, 4]),
            ("0. a\n0. b\n0. c\n", 0, vec![0, 1, 2]),
        ] {
            for dialect in [Dialect::Written, Dialect::Paste] {
                let o = from_markdown_as(&c, md, dialect);
                assert!(o.errors.is_empty(), "{md:?}: {:?}", o.errors);
                assert_eq!(o.nodes[0].attrs["start"], json!(start), "{md:?} {dialect:?}");
                let want: Vec<(i64, Option<i64>)> = counted.iter().map(|&n| (n, None)).collect();
                assert_eq!(numbers(&o.nodes[0]), want, "{md:?} {dialect:?}");
                assert!(o.notes.is_empty(), "{md:?} {dialect:?}: {:?}", o.notes);
            }
            for paste in [true, false] {
                let html = crate::convert_for_editor(md, paste).unwrap().html;
                assert!(!html.contains("value"), "{md:?}: {html}");
            }
            let page = measure_as(&c, md, Dialect::Page);
            let kept: Vec<i64> = numbers(&page.outcome.nodes[0]).into_iter().map(|(n, _)| n).collect();
            assert_eq!(kept, vec![start; counted.len()], "{md:?}");
            assert!(page.export_diff.is_some(), "{md:?}: the export reads back counted");
        }
        // Nested, each list is its own; a list whose numbers differ keeps them.
        let o = from_markdown_as(&c, "1. a\n\n   1. x\n   1. y\n\n1. b\n", Dialect::Paste);
        assert_eq!(numbers(&o.nodes[0]), [(1, None), (2, None)]);
        assert_eq!(numbers(&o.nodes[0].content[0].content[1]), [(1, None), (2, None)]);
        let o = from_markdown_as(&c, "1. a\n1. b\n2. c\n", Dialect::Paste);
        assert_eq!(numbers(&o.nodes[0]), [(1, None), (1, Some(1)), (2, None)]);
        // To-dos among lazily numbered items: the items after them count on.
        let o = from_markdown_as(&c, "1. a\n1. [ ] b\n1. c\n", Dialect::Paste);
        let kinds: Vec<&str> = o.nodes.iter().map(|n| n.kind.as_str()).collect();
        assert_eq!(kinds, ["orderedList", "taskList", "orderedList"]);
        assert_eq!(numbers(&o.nodes[2]), [(3, None)]);
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

    /// A table too large for a page is refused from its parser's events,
    /// before its HTML is written or read: the refusal is the converter's
    /// own (`markdown`), not ingest's count of the tree it would have built.
    #[test]
    fn a_table_past_the_node_limit_is_refused_before_its_html() {
        let c = Contract::load();
        let rows = |n: usize| "| a | b | c | d |\n| - | - | - | - |\n".to_string() + &"| 1 | 2 | 3 | 4 |\n".repeat(n);
        let o = from_markdown_as(&c, &rows(20_000), Dialect::Paste);
        assert_eq!(o.errors, [too_many_parts()]);
        assert!(past_one_conversion(&o.errors[0]));
        // Ingest's own refusal of the same count is one conversion too many as well.
        assert!(past_one_conversion(&Problem::new("doc", crate::model::too_many_nodes())));
        let o = from_markdown_as(&c, &rows(100), Dialect::Paste);
        assert_eq!(o.errors, []);
    }

    /// The count the early refusal reads never passes the tree's: it may
    /// only refuse what ingest would.
    #[test]
    fn the_early_count_never_passes_the_tree() {
        let c = Contract::load();
        for md in [
            "| | |\n| - | - |\n| a | b |\n",
            "| | |\n| - | - |\n",
            "| h | |\n| - | - |\n| | b |\n| `c` | **d** |\n",
            "# Title\n\nWords *and* **more** and *a**b*\n\n- one\n- two\n  - nested\n\n1. x\n2. y\n",
            "- [ ] task\n- [x] done\n\n> quote\n>\n> > deeper\n\n> [!tip]\n> Hint\n",
            "```rust\nfn main() {}\n```\n\n    indented\n\n---\n\n![photo.jpg](/drive/file_1)\n",
            "<!-- a comment -->\n\n<div>raw</div>\n\nText with <span>html</span> and <Login/>\n",
            "Line one  \nline two\n\n[@Nick](/person/person_1) and [link](https://example.com)\n",
            "\n\n   \n\n",
        ] {
            for dialect in [Dialect::Written, Dialect::Paste] {
                let mut notes = vec![];
                let floor = nodes_at_least(&events_as(md, dialect, None, &[], &mut notes, &mut 0, &mut ArticleCounts::default()).unwrap());
                let o = from_markdown_as(&c, md, dialect);
                assert_eq!(o.errors, [], "{md:?}");
                assert!(floor <= crate::node_count(&o.nodes), "{md:?}: {floor} > {}", crate::node_count(&o.nodes));
            }
        }
    }

    fn without_block_ids() -> Outcome {
        let c = Contract::load();
        from_markdown(
            &c,
            "Appended note.\n\n<p data-id=\"pg1a2b3c\">Quoted block</p>\n",
        )
    }

    #[test]
    fn markdown_never_brings_block_ids() {
        let o = without_block_ids();
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        let mut ids = vec![];
        for n in &o.nodes {
            n.walk(&mut |n| ids.push(n.id().map(str::to_string)));
        }
        assert!(ids.iter().all(Option::is_none), "{ids:?}");
        assert!(o.notes.iter().any(|n| n.message.contains("data-id")));
    }

    /// Written into a page that has the id already, through the documented
    /// path, every block's id is the page's only one.
    #[cfg(feature = "yjs")]
    #[test]
    fn markdown_written_into_a_page_takes_ids_the_page_lacks() {
        let o = without_block_ids();
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

    /// The cases the browser's `kindOfLink` reads too (`editor.test.ts`),
    /// so a link turned into an embed or drawn in the editor is the block
    /// this converter makes.
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

    /// An article's pending suggestion stays a suggestion when read as the
    /// article's own, and one around what blocks hold as a node of its own
    /// (a footnote, a mention, a line break) is counted: in blocks its
    /// marks would cover the text either side alone, so accepting or
    /// rejecting it would leave that node behind, and the article's move
    /// holds back for it.
    #[test]
    fn an_articles_suggestion_around_a_node_is_counted() {
        let c = Contract::load();
        let over = |md: &str| {
            let m = measure_as(&c, md, Dialect::Article);
            assert!(m.outcome.errors.is_empty(), "{md}: {:?}", m.outcome.errors);
            m.article.expect("an article's counts").suggestions_over_nodes
        };
        for md in [
            "You swam {--at noon.[^ev-1] Then--} later.\n\n[^ev-1]: Location · 9:30 AM\n",
            "You swam.{--[^ev-1]--} Later.\n\n[^ev-1]: Location · 9:30 AM\n",
            "You swam.{--[^ev-9]--} Later.\n",
            "You swam {--with [@Nick](/person/p_1) at--} noon.\n",
            "You swam {--one\\\ntwo--} noon.\n",
            "You swam {++one  \ntwo++} noon.\n",
        ] {
            assert_eq!(over(md), 1, "{md}");
            assert!(!measure_as(&c, md, Dialect::Article).article.unwrap().holds(), "{md}");
        }
        for md in [
            "I had lunch {--at noon--}{++on Friday++} with her.\n",
            "You swam {--one\ntwo--} noon.\n",
            "You swam {--⟦Nick⟧--} at {++[the river](/place/p_1)++}.\n",
            "You swam.[^ev-1] {--Then--} later.[^ev-1]\n\n[^ev-1]: Location · 9:30 AM\n",
        ] {
            assert_eq!(over(md), 0, "{md}");
            let m = measure_as(&c, md, Dialect::Article);
            assert!(m.article.unwrap().holds(), "{md}");
            let mut kinds = vec![];
            for n in &m.outcome.nodes {
                n.walk(&mut |n| kinds.extend(n.marks.iter().map(|m| m.kind.clone())));
            }
            assert!(kinds.iter().any(|k| k.starts_with("proposed")), "{md}: the suggestion stays one");
        }
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

    /// How the markdown page editor escaped a proposal's text
    /// (`escapeReviewBody`), which pages written before blocks still hold.
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

    /// A note's kind is a fixed phrase. The server logs the kinds of every
    /// page a conversion changes (`api::page_convert`), and the notes quote
    /// the page: its lines, a table cell, a code block's info string, a
    /// link's title and address, a word between angle brackets.
    #[test]
    fn a_notes_kind_never_quotes_the_page() {
        let page = "```rust ledger-zebra\nfn main() {}\n```\n\n\
                    | A | B |\n| --- | --- |\n| 1 | 2 | account 99887766 |\n\n\
                    Door code 2290    is in the drawer.\n\n\
                    Lunch\twith David Okafor at noon.\n\n\
                    See [the plan](https://pier.example/harbour \"Quayside\") and [](https://pier.example/quay).\n\n\
                    Ask <Nick> and <kbd>Ctrl</kbd> about it.\n";
        let quoted = ["ledger", "zebra", "99887766", "2290", "drawer", "Okafor", "Lunch", "pier", "harbour", "Quayside", "quay", "Nick"];
        let c = Contract::load();
        let mut seen = 0;
        for dialect in [Dialect::Page, Dialect::Written] {
            let m = measure_as(&c, page, dialect);
            for note in &m.outcome.notes {
                let kind = category(&note.message);
                seen += 1;
                for word in quoted {
                    assert!(!kind.contains(word), "{kind:?} quotes the page, from {:?}", note.message);
                }
                assert_ne!(kind, "other", "{:?} has no kind", note.message);
            }
        }
        assert!(seen >= 6, "the page makes notes to read");
        assert_eq!(category("spaces collapsed in \"Door code 2290    is in the drawer.\": …"), "spaces collapsed");
        assert_eq!(category("unwrapped <nick>, kept its text"), "unwrapped an unknown tag");
        assert_eq!(category("a note no kind names, quoting 2290"), "other");
    }

    fn page_text(md: &str) -> (Measured, String) {
        let m = measure_as(&Contract::load(), md, Dialect::Page);
        assert!(m.outcome.errors.is_empty(), "{md}: {:?}", m.outcome.errors);
        let text = m.outcome.nodes.iter().map(Node::text_content).collect::<Vec<_>>().join("\n");
        (m, text)
    }

    /// A page's markdown editor drew no HTML but underline, so what its
    /// owner typed between angle brackets, and their comments, are text of
    /// the blocks, character for character: none is read as a tag that
    /// swallows the words after it.
    #[test]
    fn a_pages_raw_html_is_the_text_its_editor_showed() {
        for (md, kept) in [
            ("Check if a<b and c>d holds.\n", "Check if a<b and c>d holds."),
            ("Dear <name>,\n\nThanks for <thing you liked>.\n", "Thanks for <thing you liked>."),
            ("Press <Enter> to submit.\n", "Press <Enter> to submit."),
            ("Returns List<int> or nothing.\n", "Returns List<int> or nothing."),
            ("One\n\n<!-- remember to call the bank -->\n\nTwo\n", "<!-- remember to call the bank -->"),
            ("Before <!-- a note --> after.\n", "Before <!-- a note --> after."),
            ("<!--\nTODO:\n- call the bank\n-->\n", "- call the bank"),
            ("A <span style=\"color: red\">red</span> sail.\n", "A <span style=\"color: red\">red</span> sail."),
        ] {
            let (m, text) = page_text(md);
            assert!(text.contains(kept), "{md:?} became {text:?}");
            assert!(m.text_lost.is_none(), "{md:?}: {:?}", m.text_lost);
            assert!(m.addresses_lost.is_empty(), "{md:?}: {:?}", m.addresses_lost);
        }
        // Underline on its line is the underline the editor drew.
        let (m, text) = page_text("Lunch <u>by the water</u>, and a <u> alone.\n");
        assert_eq!(text, "Lunch by the water, and a <u> alone.");
        let marks: Vec<&str> = m.outcome.nodes[0].content.iter().flat_map(|n| n.marks.iter().map(|m| m.kind.as_str())).collect();
        assert_eq!(marks, ["underline"]);
    }

    /// Each line of a paragraph the editor showed as a line stays one: a
    /// list typed one item per line, an address, a journal's lines.
    #[test]
    fn a_pages_lines_stay_lines() {
        let (m, _) = page_text("Groceries\nMilk\nEggs\nBread\n");
        let p = &m.outcome.nodes[0];
        let kinds: Vec<&str> = p.content.iter().map(|n| n.kind.as_str()).collect();
        assert_eq!(kinds, ["text", "hardBreak", "text", "hardBreak", "text", "hardBreak", "text"]);
        assert!(m.text_lost.is_none(), "{:?}", m.text_lost);
        let (m, _) = page_text("- Nick Example\n  12 Example Street\n");
        assert!(m.outcome.nodes[0].content[0].content[0].content.iter().any(|n| n.kind == "hardBreak"));
        // Markdown written by a writer joins them, as markdown does.
        let m = measure(&Contract::load(), "Groceries\nMilk\n");
        assert!(m.outcome.nodes[0].content.iter().all(|n| n.kind != "hardBreak"));
    }

    /// The gate holds a page to its source, not to the converter's own
    /// rendering: blocks made as markdown is read elsewhere (raw HTML as
    /// markup, a paragraph's lines joined) lose what the page's editor
    /// showed, and the page's judgment says so.
    #[test]
    fn a_pages_judgment_sees_what_the_rendering_would_hide() {
        let c = Contract::load();
        for md in [
            "Check if a<b and c>d holds.\n",
            "Dear <name>,\n",
            "One\n\n<!-- remember to call the bank -->\n\nTwo\n",
            "Groceries\nMilk\nEggs\n",
        ] {
            let page = to_html_as(md, Dialect::Page).unwrap().0;
            let read_as_written = from_markdown(&c, md);
            let (lost, _) = judge(md, Dialect::Page, &page, &read_as_written.nodes, &read_as_written.notes);
            assert!(lost.is_some(), "{md:?}: blocks that lose what the editor showed pass");
            let as_page = from_markdown_as(&c, md, Dialect::Page);
            let (lost, _) = judge(md, Dialect::Page, &page, &as_page.nodes, &as_page.notes);
            assert!(lost.is_none(), "{md:?}: {lost:?}");
        }
        let (lost, _) = judge(
            "Dear <name>,\n",
            Dialect::Page,
            &to_html("Dear <name>,\n").unwrap().0,
            &from_markdown(&c, "Dear <name>,\n").nodes,
            &[],
        );
        let lost = lost.expect("raw HTML missing from the blocks is text lost, whatever the rendering says");
        assert_eq!(lost.html_around, "<name>");
        assert_eq!(raw_html_lines("Dear <name>, a <u>b</u>.\n"), ["<name>"]);
        // A backslash typed at a line's end, which every rendering drops
        // as the line break it reads: the markdown says the blocks lack it.
        let md = "Paths: C:\\Users\\\nD:\\Data\n";
        let (lost, _) = judge(md, Dialect::Page, &to_html(md).unwrap().0, &from_markdown(&c, md).nodes, &[]);
        let lost = lost.expect("a typed backslash missing from the blocks is text lost, whatever the rendering says");
        assert_eq!(lost.html_around, "Paths: C:\\Users\\");
        let as_page = from_markdown_as(&c, md, Dialect::Page);
        assert!(judge(md, Dialect::Page, &to_html_as(md, Dialect::Page).unwrap().0, &as_page.nodes, &[]).0.is_none());
        // An address dropped without a note is lost.
        let md = "[x](https://example.com/x)\n";
        let (_, addresses) = judge(md, Dialect::Written, &to_html(md).unwrap().0, &from_markdown(&c, "x\n").nodes, &[]);
        assert_eq!(addresses, ["https://example.com/x"]);
    }

    /// A link reference definition is the line the page's editor showed,
    /// and a link naming its label still goes where it says. The parser
    /// takes definitions out of what it renders, so a page of them moved
    /// with its lines and their addresses gone, judged against that same
    /// rendering and reported clean.
    #[test]
    fn a_pages_link_definitions_are_the_lines_its_editor_showed() {
        for (md, kept) in [
            (
                "Week plan\n\n[Monday]: gym\n[Tuesday]: rest\n[Source]: https://example.com/plan\n\nNotes after.\n",
                "[Monday]: gym\n[Tuesday]: rest\n[Source]: https://example.com/plan",
            ),
            ("Project\n\n[Status]: done\n", "[Status]: done"),
            ("> [Mood]: good\n", "[Mood]: good"),
            ("[book]: https://example.com/book \"The book\"\n", "[book]: https://example.com/book \"The book\""),
            ("[a]: one\n[A]: two\n\nsee [a]\n", "[a]: one\n[A]: two"),
        ] {
            let (m, _) = page_text(md);
            let lines: Vec<String> = m
                .outcome
                .nodes
                .iter()
                .flat_map(|n| {
                    let mut lines = vec![String::new()];
                    n.walk(&mut |n| match (&n.text, n.kind.as_str()) {
                        (Some(t), _) => lines.last_mut().unwrap().push_str(t),
                        (None, "hardBreak") => lines.push(String::new()),
                        _ => {}
                    });
                    lines
                })
                .collect();
            assert!(lines.join("\n").contains(kept), "{md:?} became {lines:?}");
            assert!(m.text_lost.is_none(), "{md:?}: {:?}", m.text_lost);
            assert!(m.addresses_lost.is_empty(), "{md:?}: {:?}", m.addresses_lost);
        }
        // A link that names a label goes where its definition says, the
        // first of two with one label.
        let (m, text) = page_text("Sources [1]\n\n[1]: https://example.com/a\n\nsee [a]\n\n[a]: one\n[a]: two\n");
        assert!(text.contains("[1]: https://example.com/a"), "{text}");
        let hrefs: Vec<String> = m
            .outcome
            .nodes
            .iter()
            .flat_map(|n| {
                let mut out = vec![];
                n.walk(&mut |n| {
                    out.extend(n.marks.iter().filter_map(|m| m.attrs.get("href")?.as_str().map(str::to_string)))
                });
                out
            })
            .collect();
        assert_eq!(hrefs, ["https://example.com/a", "one"]);
        // What follows a definition starts a block as it did: a rule stays a
        // rule, never a heading's underline; a quote or an item holds its
        // definition's lines.
        let (m, text) = page_text("[a]: /url\n---\nAfter\n");
        assert_eq!(kinds(&m.outcome), ["paragraph", "horizontalRule", "paragraph"]);
        assert!(text.starts_with("[a]: /url"), "{text}");
        let (m, text) = page_text("> [b]:\n> /url\n> \"title\"\n");
        assert_eq!(kinds(&m.outcome), ["blockquote"]);
        assert_eq!(text, "[b]:/url\"title\"");
        let (m, text) = page_text("- [x]: /url\n  text\n- next\n");
        assert_eq!(kinds(&m.outcome), ["bulletList"]);
        assert!(text.contains("[x]: /url") && text.contains("text") && text.contains("next"), "{text}");
        // Markdown a writer sends reads definitions as markdown does.
        let m = measure(&Contract::load(), "Project\n\n[Status]: done\n");
        assert_eq!(m.outcome.nodes.len(), 1);
    }

    /// What a page's blocks cannot hold of what its editor showed is
    /// noted: a run of spaces, a tab or a line's indent, a table row's
    /// cells past its header's. Each changed with no note, and the page
    /// moved as clean. An ordered list's own numbers are held, and say
    /// nothing.
    #[test]
    fn a_page_says_what_its_blocks_cannot_hold() {
        let noted = |md: &str, says: &str| {
            let (m, _) = page_text(md);
            assert!(
                m.outcome.notes.iter().any(|n| n.message.contains(says)),
                "{md:?}: no note says {says:?}: {:?}",
                m.outcome.notes
            );
        };
        noted("Total:     $5\nName    Age\n", "spaces collapsed in \"Total:     $5\"");
        noted("Name\tAge\nNick\t40\n", "spaces collapsed in \"Nick\t40\"");
        noted("Dear Nick,\n   indented line\nend\n", "spaces collapsed in \"indented line\"");
        noted("Run `a   b` here.\n", "spaces collapsed in \"Run `a   b` here.\"");
        noted("| a | b |\n| - | - |\n| 1 | 2 | 3 |\n", "dropped \"3\" from a table row");
        // What blocks hold as written says nothing.
        for md in [
            "1. one\n2. two\n\n- a  \n- b\n",
            "2019. Moved to the coast\n2021. Started the boat\n2024. Sold it\n",
            "1. first\n3. third\n",
            "> 5. a\n> 7. b\n",
            "Lunch with Nick.\nTea after.\n",
            "| a | b |\n| - | - |\n| 1 | 2 |\n| x |\n",
            "```\ncode   kept\n\tas is\n```\n",
        ] {
            let (m, _) = page_text(md);
            assert!(m.outcome.notes.is_empty(), "{md:?}: {:?}", m.outcome.notes);
        }
    }

    /// An email autolink holds its address as `mailto:`; the parser gives
    /// it bare, and a page holding one was refused for an address it kept.
    #[test]
    fn an_email_autolink_keeps_its_address() {
        for dialect in [Dialect::Written, Dialect::Page] {
            let m = measure_as(&Contract::load(), "Write to <nick@example.com> today.\n", dialect);
            assert!(m.addresses_lost.is_empty(), "{:?}", m.addresses_lost);
            assert!(m.text_lost.is_none(), "{:?}", m.text_lost);
        }
        assert_eq!(addresses_in("<nick@example.com>\n", Dialect::Written), ["mailto:nick@example.com"]);
    }

    /// An address a link or an embed names is on a block or in a note:
    /// an image's title, a link with no text, a link around an image.
    #[test]
    fn every_address_is_held_or_noted() {
        let c = Contract::load();
        for dialect in [Dialect::Written, Dialect::Page] {
            let m = measure_as(&c, "![alt](https://example.com/a.png \"Evening\")\n", dialect);
            assert!(m.outcome.notes.iter().any(|n| n.message.contains("Evening")), "{:?}", m.outcome.notes);
            let m = measure_as(&c, "See [](https://example.com/x) here.\n", dialect);
            assert!(m.addresses_lost.is_empty(), "{:?}", m.addresses_lost);
            assert_eq!(m.outcome.nodes[0].text_content(), "See https://example.com/x here.");
            let m = measure_as(&c, "[![badge](https://example.com/b.svg)](https://example.com)\n", dialect);
            assert!(m.addresses_lost.is_empty(), "{:?}", m.addresses_lost);
            assert!(m.outcome.notes.iter().any(|n| n.message.contains("https://example.com around an image")), "{:?}", m.outcome.notes);
        }
        assert_eq!(addresses_in("[a](/x) ![b](/y.png) `[c](/z)`\n", Dialect::Written), ["/x", "/y.png"]);
    }
}

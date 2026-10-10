//! Markdown → document tree: existing pages once, and markdown that writers
//! still send.
//!
//! Our markdown has a dialect on top of GFM: `==highlight==`, `<u>`,
//! `[@Label](/person/id)` references, `![alt|600](url)` image widths, and
//! audio/video/file embeds written as images. This maps the dialect onto the
//! contract, renders to HTML with pulldown-cmark, and ingests it in migration
//! mode, which reports every step that changed something.
//!
//! [`measure`] checks one document the way a conversion is judged: was any
//! text lost, and does the markdown export read back to the same tree.

use crate::contract::{is_route, Contract};
use crate::ingest::{parse, Mode, Outcome, MAX_INPUT_BYTES};
use crate::model::{Node, Problem};
use crate::render;
use pulldown_cmark::{html::push_html, CowStr, Event, LinkType, Options, Parser, Tag, TagEnd};
use serde::Serialize;
use std::ops::Range;

const IMAGE_EXT: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico", "avif", "heic", "heif", "tiff",
];

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn ext_of(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or("");
    path.rsplit('.')
        .next()
        .filter(|e| e.len() <= 5 && !e.contains('/'))
        .unwrap_or("")
        .to_ascii_lowercase()
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
                    let digits = src.len() - src.trim_start_matches(|c: char| c.is_ascii_digit()).len();
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
        let (Some(first), Some(last)) = (events.get(start), end.checked_sub(1).and_then(|e| events.get(e)))
        else {
            continue;
        };
        if end == start {
            continue;
        }
        let ends_block = matches!(
            events.get(end),
            Some((Event::End(TagEnd::Item | TagEnd::Paragraph) | Event::Start(_), _))
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

/// Our markdown → HTML, with the dialect mapped onto contract tags. Returns
/// the HTML and notes for dialect features the contract has no node for.
pub fn to_html(md: &str) -> (String, Vec<Problem>) {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS);
    let mut notes = vec![];
    let mut out: Vec<Event> = vec![];
    let mut in_code = 0;
    let mut in_meta = false;
    // A link or image being collected until its end, to rewrite it whole,
    // with where each of its events is in the source.
    let mut buffer: Option<(Tag, Vec<Event>, Vec<Range<usize>>)> = None;

    let events = bare_task_markers(md, Parser::new_ext(md, opts).into_offset_iter().collect());
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
                    // `![alt|600](…)`: the last `|` starts a width, unless it
                    // was written as `&#124;`, which keeps it in the alt.
                    let (alt, width) = match alt.rsplit_once('|') {
                        Some((a, w))
                            if w.trim().parse::<u32>().is_ok() && last_pipe_is_bare(md, buf, ranges) =>
                        {
                            (a.to_string(), Some(w.trim().to_string()))
                        }
                        _ => (alt, None),
                    };
                    let ext = ext_of(dest_url);
                    let is_image = IMAGE_EXT.contains(&ext.as_str())
                        || dest_url.contains("unsplash")
                        || ext.is_empty();
                    let html = if is_image {
                        let w = width.map(|w| format!(" width=\"{w}\"")).unwrap_or_default();
                        let alt_attr = if alt.is_empty() {
                            String::new()
                        } else {
                            format!(" alt=\"{}\"", esc(&alt))
                        };
                        format!("<img src=\"{}\"{alt_attr}{w}>", esc(dest_url))
                    } else {
                        notes.push(Problem::new(
                            "markdown",
                            format!(
                                "media embed ({ext}) has no widget in the contract; became a link"
                            ),
                        ));
                        format!("<a href=\"{}\">{}</a>", esc(dest_url), esc(&alt))
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
            Event::Start(Tag::MetadataBlock(_)) => {
                in_meta = true;
                notes.push(Problem::new(
                    "markdown",
                    "front matter is not document content; left out",
                ));
            }
            Event::End(TagEnd::MetadataBlock(_)) => in_meta = false,
            _ if in_meta => {}
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
    let mut html = String::new();
    push_html(&mut html, out.into_iter());
    (html, notes)
}

/// Our markdown as a document tree, in migration mode: what the contract
/// cannot hold is unwrapped or dropped and noted, never silently. The nodes
/// carry no block ids, not even one the markdown's HTML wrote; writing them
/// to a document gives them ids ([`crate::write_nodes`]).
pub fn from_markdown(c: &Contract, md: &str) -> Outcome {
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
    let (html, mut notes) = to_html(md);
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
    let (html, _) = to_html(md);
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
    use serde_json::json;

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
            assert!(m.outcome.errors.is_empty(), "{md:?}: {:?}", m.outcome.errors);
            assert_eq!(kinds(&m.outcome), want, "{md:?}");
            assert!(
                m.outcome.notes.iter().any(|n| n.message.contains("split a list")),
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
                o.nodes[0].content.iter().all(|i| !i.text_content().contains('[')),
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
    fn an_image_the_contract_refuses_keeps_its_alt_text() {
        let c = Contract::load();
        for md in [
            "Intro\n\n![chart](data:image/png;base64,iVBORw0KGgo=)\n\nMore text.\n",
            "Intro\n\n![chart](file:///Users/x/shot.png)\n\nMore text.\n",
        ] {
            let o = from_markdown(&c, md);
            assert!(o.errors.is_empty(), "{md:?}: {:?}", o.errors);
            let text: Vec<String> = o.nodes.iter().map(Node::text_content).collect();
            assert_eq!(text, ["Intro", "chart", "More text."], "{md:?}");
            let cats: Vec<String> = o.notes.iter().map(|n| category(&n.message)).collect();
            assert_eq!(cats, ["image dropped"], "{md:?}: {:?}", o.notes);
            assert!(o.notes[0].message.contains("kept its alt text"));
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
        assert!(cats.iter().any(|c| c.contains("media embed")), "{cats:?}");
        assert!(m.text_lost.is_none(), "{:?}", m.text_lost);
    }
}

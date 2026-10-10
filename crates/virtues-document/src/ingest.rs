//! HTML → document tree, checked against the contract.
//!
//! Two modes. `Strict` is the model's write path: anything outside the
//! contract is an error the caller hands back, and nothing is written.
//! `Migrate` converts existing pages once: it unwraps unknown tags and keeps
//! their text, and records a note for every such step so the conversion
//! report says what changed.
//!
//! Whitespace and structural normalization (wrapping loose text in a
//! paragraph, moving a block out of a paragraph, filling an empty list item)
//! follow ProseMirror's DOMParser, because the browser editor parses the same
//! HTML with it and the two must agree.

use crate::contract::{normalize_url, AttrSpec, AttrType, Contract, HtmlRule, Node as Spec};
use crate::model::{depth, too_deep, Mark, Node, Problem, MAX_DEPTH};
use ego_tree::iter::Edge;
use ego_tree::NodeRef;
use scraper::{ElementRef, Html, Node as Dom};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Strict,
    Migrate,
}

#[derive(Debug, Default, Serialize)]
pub struct Outcome {
    pub nodes: Vec<Node>,
    pub notes: Vec<Problem>,
    pub errors: Vec<Problem>,
}

type DomRef<'a> = NodeRef<'a, Dom>;

enum Piece {
    Inline(Node),
    Block(Node),
}

/// Tags whose content is never document text.
const DROPPED: &[&str] = &[
    "script", "style", "head", "title", "noscript", "template", "meta", "link",
];

/// The most HTML or markdown one call reads: far more than a page holds, and
/// a bound on what one parse can cost. With the scan's guards a parse costs
/// in proportion to its input, but the constant is large: at this size,
/// HTML of nothing but empty paragraphs builds close to a gigabyte.
pub const MAX_INPUT_BYTES: usize = 4 << 20;

/// The most elements the source may hold open at once ([`Scan::open_depth`])
/// before it is parsed. Looser than [`MAX_DEPTH`], because the count only
/// estimates the parsed depth; it keeps HTML's parser, whose cost grows with
/// depth, away from input nested thousands deep.
const MAX_OPEN_TAGS: usize = 1000;

/// The most formatting elements HTML's parser may have to build again
/// ([`Scan::rebuilt`]). Well-formed HTML has it build none.
const MAX_REBUILT: usize = 10_000;

/// The most attributes one tag may carry. HTML's parser checks each one
/// against those before it, so the cost of a tag grows with the square of
/// its attributes; a tag the contract knows carries a handful.
const MAX_ATTRS: usize = 100;

/// The most errors, and the most notes, one parse collects. Past it, one
/// more says how many were left out: a list longer than this tells its
/// reader nothing more, and a list as long as the input is a cost of its own.
pub const MAX_PROBLEMS: usize = 200;

/// HTML's formatting elements: an unclosed one stays in the parser's list of
/// active formatting, and the parser builds it again inside every block that
/// follows until it is closed.
const FORMATTING: &[&str] = &[
    "a", "b", "big", "code", "em", "font", "i", "nobr", "s", "small", "strike", "strong", "tt",
    "u",
];

/// Tags whose start or end closes the elements open inside a block, which
/// leaves an unclosed formatting element to be built again in the next one.
const BLOCKS: &[&str] = &[
    "address", "article", "aside", "blockquote", "caption", "center", "colgroup", "dd", "details",
    "dialog", "dir", "div", "dl", "dt", "fieldset", "figcaption", "figure", "footer", "form", "h1",
    "h2", "h3", "h4", "h5", "h6", "header", "hgroup", "hr", "li", "listing", "main", "menu", "nav",
    "ol", "p", "plaintext", "pre", "search", "section", "summary", "table", "tbody", "td", "tfoot",
    "th", "thead", "tr", "ul", "xmp",
];

/// A tag that can close the elements opened inside it: a block, or a custom
/// element such as a widget, which HTML closes like any unknown element.
fn ends_a_block(name: &str) -> bool {
    BLOCKS.contains(&name) || name.contains('-')
}

/// HTML's void elements: the only ones where `<x/>` means `<x>`.
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

fn is_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n' | '\u{000c}')
}

fn is_ws_byte(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b'\x0c')
}

fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for ch in s.chars() {
        if is_ws(ch) {
            if !in_ws {
                out.push(' ');
            }
            in_ws = true;
        } else {
            out.push(ch);
            in_ws = false;
        }
    }
    out
}

/// Tags HTML closes by itself when the next one opens (`<p>a<p>b` is two
/// paragraphs, not one inside the other), so an unclosed one does not nest.
const CLOSE_THEMSELVES: &[&str] = &[
    "p", "li", "dt", "dd", "tr", "td", "th", "thead", "tbody", "tfoot", "colgroup", "caption",
    "option", "optgroup", "rb", "rt", "rtc", "rp", "html", "head", "body",
];

/// Elements whose content HTML's parser may read as text up to their end
/// tag, rather than as tags: RAWTEXT, RCDATA and script data. Whether it
/// does depends on where the element sits, which the scan does not know:
/// in HTML it does, inside `<svg>` or `<math>` these are ordinary elements,
/// and `<noscript>` is either, by the parser's scripting flag.
const RAW_TEXT: &[&str] = &[
    "iframe", "noembed", "noframes", "noscript", "script", "style", "textarea", "title", "xmp",
];

/// What the source says that the parsed tree cannot.
struct Scan {
    /// Non-void tags written self-closed (`<virtues-applet .../>`), each
    /// named once. HTML ignores that slash, so the element stays open and
    /// swallows what follows.
    self_closed: Vec<String>,
    /// The most elements open at once, counting each start tag until its end
    /// tag. HTML's parser costs time in proportion to how deep it is when
    /// each tag arrives, so this is measured before parsing. It can exceed
    /// the parsed depth (a formatting tag left unclosed is closed at the end
    /// of its block); it is a guard on the parser, not the contract's limit.
    open_depth: usize,
    /// How many formatting elements HTML's parser builds again, at most: for
    /// each tag that starts or ends a block, the formatting tags still
    /// unclosed when it comes. `<p><b>x<p>y` makes the second paragraph
    /// bold, a copy of the `<b>`; a hundred unclosed `<b>`s with different
    /// attributes before a thousand paragraphs make a hundred thousand
    /// copies, from a few kilobytes. It counts high for formatting that
    /// wraps whole blocks, which the parser does not copy; that is never
    /// the contract's shape either.
    rebuilt: usize,
    /// The most attributes on one tag.
    most_attrs: usize,
    /// What the source can be read two ways around, when it can: the scan
    /// read a tag, comment or quoted value across a place where HTML's
    /// parser, depending on where the element sits, may instead end the
    /// text of a [`RAW_TEXT`] element (its name) or a CDATA section
    /// (`CDATA`). Past that place the scan no longer knows what the parser
    /// reads as tags, so its counts say nothing.
    two_ways: Option<&'static str>,
}

/// Read the source's tags as HTML's tokenizer does, for the guards on what
/// parsing it costs. Every guard is only as good as the scan's agreement
/// with the parser about which text is a tag: whatever the scan steps over
/// as a comment, a quoted value or an element's text, the guards never see.
/// So comments end where the tokenizer ends them (`<!-->`, `<!--->`, `-->`,
/// `--!>`), `<!…>`, `<?…>` and `</` before anything but a letter are the
/// tokenizer's bogus comments, ending at the next `>`, and end tags have
/// their attributes read too. Attribute values are stepped over the way the
/// tokenizer does, so `<a href=/x/>` is an `href` of `/x/`, not a
/// self-closed link.
///
/// Two things the tokenizer decides by where it is in the tree: whether a
/// [`RAW_TEXT`] element's content is text, and whether `<![CDATA[` runs to
/// `]]>` (inside `<svg>` or `<math>`) or to the first `>`. The scan reads
/// the content as tags and the CDATA section to the first `>`, which shows
/// it every tag either reading has, provided it reaches each place the
/// other reading ends that text between tags, as the parser does. When it
/// does not, it stops and says so ([`Scan::two_ways`]).
fn scan(html: &str) -> Scan {
    let b = html.as_bytes();
    let mut found: Vec<String> = vec![];
    let mut depth = 0usize;
    let mut open_depth = 0usize;
    // Formatting tags opened and not yet closed, by name.
    let mut formatting: Vec<String> = vec![];
    let mut rebuilt = 0usize;
    let mut most_attrs = 0usize;
    // Where another reading of the source ends an element's text or a
    // CDATA section, and what ends there: the scan must be between tags at
    // each.
    let mut ends: BTreeMap<usize, &'static str> = BTreeMap::new();
    let mut armed: Vec<&'static str> = vec![];
    let mut lower: Option<String> = None;
    let mut two_ways = None;
    // The next `>` at or after `from`, and the position past it; the input's
    // end when there is none.
    let past_gt = |from: usize| {
        b[from.min(b.len())..]
            .iter()
            .position(|&c| c == b'>')
            .map_or(b.len(), |e| from + e + 1)
    };
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &b[i..];
        let mut tag = None;
        let j = if rest.starts_with(b"<!--") {
            // From the `<!`'s end, so `<!-->` and `<!--->` close at once.
            let dashes = html[i + 2..].find("-->").map(|e| i + 2 + e + 3);
            let bang = html[i + 4..].find("--!>").map(|e| i + 4 + e + 4);
            match (dashes, bang) {
                (Some(a), Some(c)) => a.min(c),
                (Some(a), None) | (None, Some(a)) => a,
                (None, None) => b.len(),
            }
        } else if rest.starts_with(b"<![CDATA[") {
            let bogus = past_gt(i + 2);
            let section = html[i + 9..].find("]]>").map_or(b.len(), |e| i + 9 + e + 3);
            if section > bogus {
                ends.insert(section, "CDATA");
            }
            bogus
        } else if rest.starts_with(b"<!") || rest.starts_with(b"<?") {
            past_gt(i + 2)
        } else if rest.starts_with(b"</") {
            match b.get(i + 2) {
                Some(c) if c.is_ascii_alphabetic() => {
                    let t = read_tag(html, i + 2);
                    let j = t.end;
                    tag = Some((t, false));
                    j
                }
                _ => past_gt(i + 2),
            }
        } else if b.get(i + 1).is_some_and(u8::is_ascii_alphabetic) {
            let t = read_tag(html, i + 1);
            let j = t.end;
            tag = Some((t, true));
            j
        } else {
            i += 1;
            continue;
        };
        if j > i + 1 {
            if let Some((_, what)) = ends.range(i + 1..j).next() {
                two_ways = Some(*what);
                break;
            }
        }
        i = j;
        let Some((t, start)) = tag else { continue };
        most_attrs = most_attrs.max(t.attrs);
        let name = t.name;
        if !start {
            if !VOID.contains(&name.as_str()) && !CLOSE_THEMSELVES.contains(&name.as_str()) {
                depth = depth.saturating_sub(1);
            }
            if FORMATTING.contains(&name.as_str()) {
                if let Some(k) = formatting.iter().rposition(|f| *f == name) {
                    formatting.remove(k);
                }
            } else if ends_a_block(&name) {
                rebuilt = rebuilt.saturating_add(formatting.len());
            }
            continue;
        }
        let void = VOID.contains(&name.as_str());
        if t.self_closed && !void && !found.contains(&name) {
            found.push(name.clone());
        }
        // A self-closed custom element stays open too: the slash is ignored.
        if !void && !CLOSE_THEMSELVES.contains(&name.as_str()) {
            depth += 1;
            open_depth = open_depth.max(depth);
        }
        if let Some(&raw) = RAW_TEXT.iter().find(|r| **r == name) {
            if !armed.contains(&raw) {
                // Every `</name` after the first such element: wherever the
                // parser reads one as text, its text ends at one of these.
                armed.push(raw);
                let lower = lower.get_or_insert_with(|| html.to_ascii_lowercase());
                let close = format!("</{raw}");
                for (at, _) in lower[i..].match_indices(&close) {
                    let after = lower.as_bytes().get(i + at + close.len());
                    if after.is_some_and(|&c| is_ws_byte(c) || c == b'/' || c == b'>') {
                        ends.insert(i + at, raw);
                    }
                }
            }
        }
        if FORMATTING.contains(&name.as_str()) {
            formatting.push(name);
        } else if ends_a_block(&name) {
            rebuilt = rebuilt.saturating_add(formatting.len());
        }
    }
    Scan {
        self_closed: found,
        open_depth,
        rebuilt,
        most_attrs,
        two_ways,
    }
}

/// A start or end tag as the scan reads it.
struct Tag {
    /// Lowercase.
    name: String,
    attrs: usize,
    /// Written `<x/>`.
    self_closed: bool,
    /// Where the tag ends: past its `>`, or the input's end.
    end: usize,
}

/// The tag whose name starts at `start`: its name, then its attributes,
/// stepped over the way HTML's tokenizer steps over them.
fn read_tag(html: &str, start: usize) -> Tag {
    let b = html.as_bytes();
    let mut j = start;
    while j < b.len() && !is_ws_byte(b[j]) && b[j] != b'/' && b[j] != b'>' {
        j += 1;
    }
    let name = html[start..j].to_ascii_lowercase();
    let mut self_closed = false;
    let mut attrs = 0usize;
    while j < b.len() {
        match b[j] {
            c if is_ws_byte(c) => j += 1,
            b'>' => {
                j += 1;
                break;
            }
            b'/' => {
                j += 1;
                if b.get(j) == Some(&b'>') {
                    self_closed = true;
                    j += 1;
                    break;
                }
            }
            _ => {
                // An attribute name, then an optional value.
                attrs += 1;
                j += 1;
                while j < b.len() && !is_ws_byte(b[j]) && !matches!(b[j], b'/' | b'>' | b'=') {
                    j += 1;
                }
                while j < b.len() && is_ws_byte(b[j]) {
                    j += 1;
                }
                if b.get(j) != Some(&b'=') {
                    continue;
                }
                j += 1;
                while j < b.len() && is_ws_byte(b[j]) {
                    j += 1;
                }
                match b.get(j) {
                    Some(&q) if q == b'"' || q == b'\'' => {
                        j = b[j + 1..]
                            .iter()
                            .position(|&c| c == q)
                            .map(|e| j + 1 + e + 1)
                            .unwrap_or(b.len());
                    }
                    _ => {
                        while j < b.len() && !is_ws_byte(b[j]) && b[j] != b'>' {
                            j += 1;
                        }
                    }
                }
            }
        }
    }
    Tag {
        name,
        attrs,
        self_closed,
        end: j,
    }
}

/// How deep elements nest in the parsed DOM below its root. Text is a leaf
/// and no level, as in [`MAX_DEPTH`].
fn dom_depth(root: ElementRef) -> usize {
    let mut depth = 0usize;
    let mut deepest = 0usize;
    for edge in root.traverse() {
        match edge {
            Edge::Open(n) if n.value().is_element() => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            Edge::Close(n) if n.value().is_element() => depth -= 1,
            _ => {}
        }
    }
    // The root itself is not document content.
    deepest.saturating_sub(1)
}

/// Parse `html` as the whole content of a node of type `parent`.
pub fn parse(c: &Contract, html: &str, parent: &str, mode: Mode) -> Outcome {
    parse_with(c, html, parent, mode, true)
}

/// Parse `html` as a slice that will be spliced into a `parent`'s content.
/// The top level is not filled or checked here: whether the slice fits
/// depends on its neighbours, which the caller checks after splicing.
pub fn parse_slice(c: &Contract, html: &str, parent: &str, mode: Mode) -> Outcome {
    parse_with(c, html, parent, mode, false)
}

fn parse_with(c: &Contract, html: &str, parent: &str, mode: Mode, fill: bool) -> Outcome {
    let mut r = Reader {
        c,
        mode,
        notes: vec![],
        errors: vec![],
        left_out: (0, 0),
    };
    let refused = |r: Reader| r.outcome(vec![], parent);
    if html.len() > MAX_INPUT_BYTES {
        r.error(
            parent,
            format!(
                "the HTML is {} bytes; one write holds at most {} MiB",
                html.len(),
                MAX_INPUT_BYTES >> 20
            ),
        );
        return refused(r);
    }
    let scanned = scan(html);
    if let Some(what) = scanned.two_ways {
        let ends = match what {
            "CDATA" => "the `]]>` that ends a CDATA section inside <svg> or <math>".to_string(),
            name => format!("a </{name}>, where HTML may end a <{name}> element's text"),
        };
        r.error(
            parent,
            format!(
                "the HTML reads two ways: a tag, comment or quoted value in it runs past {ends}"
            ),
        );
        return refused(r);
    }
    if scanned.open_depth > MAX_OPEN_TAGS {
        r.error(
            parent,
            format!(
                "the HTML holds more than {MAX_OPEN_TAGS} elements open at once; \
                 close each tag it opens, and nest at most {MAX_DEPTH} levels deep"
            ),
        );
        return refused(r);
    }
    if scanned.rebuilt > MAX_REBUILT {
        r.error(
            parent,
            "the HTML leaves formatting tags (<b>, <em>, <a>, …) open across blocks, \
             which HTML repeats in every block that follows; close each one where its text ends",
        );
        return refused(r);
    }
    if scanned.most_attrs > MAX_ATTRS {
        r.error(
            parent,
            format!("a tag in the HTML carries more than {MAX_ATTRS} attributes"),
        );
        return refused(r);
    }
    for tag in scanned.self_closed {
        let msg = format!(
            "<{tag}/> cannot self-close in HTML: write <{tag} ...></{tag}>, or what follows it lands inside it"
        );
        match mode {
            Mode::Strict => r.error(parent, msg),
            Mode::Migrate => r.note(parent, msg),
        }
    }
    if !r.errors.is_empty() {
        // The tree HTML builds from this is not the one that was written.
        return refused(r);
    }
    let doc = Html::parse_fragment(html);
    let root = doc.root_element();
    // Every walk below recurses once per level of the DOM, so the DOM's depth
    // is checked before any of them runs: past it they would overflow the
    // thread's stack, which aborts the process. The DOM runs deeper than the
    // tree it makes (a mark is an element, a table has its `tbody`), so this
    // bound is looser, and the tree's own depth is checked once it is built.
    if dom_depth(root) > 2 * MAX_DEPTH {
        r.error(parent, too_deep());
        return refused(r);
    }
    let kids: Vec<DomRef> = root.children().collect();
    let nodes = r.block_children(parent, kids, parent, fill);
    if depth(&nodes) > MAX_DEPTH {
        r.error(parent, too_deep());
        return refused(r);
    }
    r.outcome(nodes, parent)
}

struct Reader<'c> {
    c: &'c Contract,
    mode: Mode,
    notes: Vec<Problem>,
    errors: Vec<Problem>,
    /// Errors and notes past [`MAX_PROBLEMS`], counted, not kept.
    left_out: (usize, usize),
}

impl<'c> Reader<'c> {
    fn error(&mut self, at: &str, message: impl Into<String>) {
        if self.errors.len() < MAX_PROBLEMS {
            self.errors.push(Problem::new(at, message));
        } else {
            self.left_out.0 += 1;
        }
    }

    fn note(&mut self, at: &str, message: impl Into<String>) {
        if self.notes.len() < MAX_PROBLEMS {
            self.notes.push(Problem::new(at, message));
        } else {
            self.left_out.1 += 1;
        }
    }

    /// The parse's outcome, with a last error and note saying how many more
    /// were left out.
    fn outcome(mut self, nodes: Vec<Node>, at: &str) -> Outcome {
        let (errors, notes) = self.left_out;
        if errors > 0 {
            self.errors
                .push(Problem::new(at, format!("{errors} more errors left out")));
        }
        if notes > 0 {
            self.notes
                .push(Problem::new(at, format!("{notes} more notes left out")));
        }
        Outcome {
            nodes,
            notes: self.notes,
            errors: self.errors,
        }
    }

    /// An error on the model's path, a note on migration's.
    fn refuse_or_note(&mut self, at: &str, refused: String, noted: String) {
        match self.mode {
            Mode::Strict => self.error(at, refused),
            Mode::Migrate => self.note(at, noted),
        }
    }

    fn is_inline_tag(&self, tag: &str) -> bool {
        self.c.mark_rule_for_tag(tag).is_some()
            || self
                .c
                .node_rules_for_tag(tag)
                .iter()
                .any(|(n, _)| self.c.is_inline(&n.name))
    }

    fn is_known_tag(&self, tag: &str) -> bool {
        self.is_inline_tag(tag) || !self.c.node_rules_for_tag(tag).is_empty()
    }

    fn transparent_for(&self, parent: &str, tag: &str) -> bool {
        self.c
            .node(parent)
            .map(|n| {
                n.spec
                    .html
                    .iter()
                    .any(|r| r.transparent.iter().any(|t| t == tag))
            })
            .unwrap_or(false)
    }

    /// Expand wrappers the parent reads through, and in migration mode any
    /// tag the contract does not know.
    fn flatten<'a>(&mut self, parent: &str, kids: Vec<DomRef<'a>>, at: &str) -> Vec<DomRef<'a>> {
        let mut out = Vec::new();
        for kid in kids {
            if let Dom::Element(e) = kid.value() {
                let tag = e.name();
                if self.transparent_for(parent, tag) {
                    out.extend(self.flatten(parent, kid.children().collect(), at));
                    continue;
                }
                if DROPPED.contains(&tag) {
                    self.refuse_or_note(
                        at,
                        format!("<{tag}> is not document content"),
                        format!("dropped <{tag}> and its content"),
                    );
                    continue;
                }
                if self.mode == Mode::Migrate
                    && !self.is_known_tag(tag)
                    && !self.is_migration_tag(tag)
                {
                    self.note(at, format!("unwrapped <{tag}>, kept its text"));
                    out.extend(self.flatten(parent, kid.children().collect(), at));
                    continue;
                }
            }
            out.push(kid);
        }
        out
    }

    fn is_migration_tag(&self, tag: &str) -> bool {
        self.mode == Mode::Migrate && tag == "input"
    }

    fn block_children(
        &mut self,
        parent: &str,
        kids: Vec<DomRef>,
        at: &str,
        fill: bool,
    ) -> Vec<Node> {
        let kids = self.flatten(parent, kids, at);
        let mut out = Vec::new();
        let mut run: Vec<DomRef> = Vec::new();
        for kid in kids {
            match kid.value() {
                Dom::Text(t) => {
                    if run.is_empty() && t.chars().all(is_ws) {
                        continue;
                    }
                    run.push(kid);
                }
                Dom::Element(e) => {
                    let tag = e.name();
                    if self.is_inline_tag(tag) {
                        run.push(kid);
                        continue;
                    }
                    self.flush_run(parent, &mut run, &mut out, at);
                    if let Some(text) = self.block_element(kid, &mut out, at) {
                        let text = collapse_ws(&text);
                        let text = text.trim();
                        if !text.is_empty() && self.c.may_contain(parent, "paragraph") {
                            let para = vec![Node::text(text, vec![])];
                            out.push(Node::element("paragraph", Map::new(), para));
                        }
                    }
                }
                _ => {}
            }
        }
        self.flush_run(parent, &mut run, &mut out, at);
        if fill {
            self.fill(parent, &mut out, at);
        }
        out
    }

    /// Loose inline content in a block container goes into a paragraph, as
    /// ProseMirror's parser wraps it.
    fn flush_run(&mut self, parent: &str, run: &mut Vec<DomRef>, out: &mut Vec<Node>, at: &str) {
        if run.is_empty() {
            return;
        }
        let kids = std::mem::take(run);
        let all_ws = kids
            .iter()
            .all(|k| matches!(k.value(), Dom::Text(t) if t.chars().all(is_ws)));
        if all_ws {
            return;
        }
        if !self.c.may_contain(parent, "paragraph") {
            let sample: String = kids
                .iter()
                .filter_map(|k| match k.value() {
                    Dom::Text(t) => Some(t.to_string()),
                    _ => ElementRef::wrap(*k).map(|e| e.text().collect()),
                })
                .collect::<String>()
                .trim()
                .chars()
                .take(40)
                .collect();
            self.error(
                at,
                format!("text \"{sample}\" cannot sit directly inside `{parent}`"),
            );
            return;
        }
        let mut pieces = Vec::new();
        self.inline_content("paragraph", kids, &[], &mut pieces, at);
        self.split_textblock("paragraph", Map::new(), pieces, out, false);
    }

    fn match_rule(
        &self,
        tag: &str,
        e: &scraper::node::Element,
    ) -> Option<(&'c Spec, &'c HtmlRule)> {
        self.c
            .node_rules_for_tag(tag)
            .into_iter()
            .find(|(_, r)| r.matches.iter().all(|(k, v)| e.attr(k) == v.as_str()))
    }

    /// Read one block element into `out`. Returns, on migration's path, the
    /// text that stands in for an element that could not be built (an image
    /// whose source the contract refuses: its alt text), for the caller to
    /// keep where the element was.
    fn block_element(&mut self, el: DomRef, out: &mut Vec<Node>, at: &str) -> Option<String> {
        let Dom::Element(e) = el.value() else { return None };
        let tag = e.name();
        let here = format!("{at} > {tag}");

        if self.mode == Mode::Migrate && tag == "input" {
            // pulldown-cmark's task list marker; read by the list item.
            return None;
        }

        let Some((spec, rule)) = self.match_rule(tag, e) else {
            self.error(
                &here,
                format!("<{tag}> is not part of the document contract"),
            );
            return None;
        };
        let attrs = match self.attrs(spec, rule, el, &here) {
            Ok(attrs) => attrs,
            Err(reasons) => return self.stand_in(el, &spec.name, reasons, &here),
        };
        let name = spec.name.as_str();

        if spec.spec.content.is_none() {
            let has_content = el.children().any(|k| match k.value() {
                Dom::Text(t) => !t.chars().all(is_ws),
                Dom::Element(_) => true,
                _ => false,
            });
            if has_content {
                self.error(
                    &here,
                    format!(
                        "<{tag}> holds no content. A custom element cannot self-close in HTML: \
                         write <{tag} ...></{tag}>, or the text after it lands inside it"
                    ),
                );
                return None;
            }
            out.push(Node::element(name, attrs, vec![]));
            return None;
        }

        if spec.spec.code {
            // A code block holds text. `<pre><code class="language-x">` is the
            // one wrapper read through; any other element would lose its tag.
            let stray = el.descendants().skip(1).find_map(|d| match d.value() {
                Dom::Element(x)
                    if !(x.name() == "code" && d.parent().map(|p| p.id()) == Some(el.id())) =>
                {
                    Some(x.name().to_string())
                }
                _ => None,
            });
            if let Some(inner) = stray {
                self.refuse_or_note(
                    &here,
                    format!(
                        "<{inner}> inside a code block: it holds text only, so write < as &lt;"
                    ),
                    format!("unwrapped <{inner}> inside a code block, kept its text"),
                );
                if self.mode == Mode::Strict {
                    return None;
                }
            }
            let text: String = ElementRef::wrap(el)
                .map(|e| e.text().collect())
                .unwrap_or_default();
            let mut text = text.replace("\r\n", "\n");
            if self.mode == Mode::Migrate && text.ends_with('\n') {
                // Markdown renderers end every fenced block with a newline.
                text.pop();
            }
            let content = if text.is_empty() {
                vec![]
            } else {
                vec![Node::text(&text, vec![])]
            };
            out.push(Node::element(name, attrs, content));
            return None;
        }

        if self.c.is_textblock(name) {
            let mut pieces = Vec::new();
            self.inline_content(name, el.children().collect(), &[], &mut pieces, &here);
            self.split_textblock(name, attrs, pieces, out, true);
            return None;
        }

        let mut content = self.block_children(name, el.children().collect(), &here, true);
        if self.mode == Mode::Migrate && name == "listItem" {
            // `<li><input type=checkbox checked> text` → taskItem
            if let Some(checked) = task_marker(el) {
                let mut a = self.c.default_attrs("taskItem");
                a.insert("checked".into(), Value::Bool(checked));
                out.push(Node::element("taskItem", a, std::mem::take(&mut content)));
                return None;
            }
        }
        if self.mode == Mode::Migrate
            && matches!(name, "bulletList" | "orderedList")
            && content.iter().any(|n| n.kind == "taskItem")
        {
            self.task_runs(name, attrs, content, out, &here);
            return None;
        }
        let mut node = Node::element(name, attrs, content);
        if name == "table" && !self.table_is_a_grid(&mut node, &here) {
            return None;
        }
        out.push(node);
        None
    }

    /// A table must be a grid, or the editor's table plugin rewrites it in
    /// the shared document as soon as someone opens the page
    /// ([`crate::table`]). On the model's path a table that is not is
    /// refused; on migration's it is evened out the way the plugin would,
    /// and noted. Returns whether there is a table to keep.
    fn table_is_a_grid(&mut self, table: &mut Node, at: &str) -> bool {
        // A row or cell the contract refuses is named already.
        let rows_ok = table.content.iter().all(|r| {
            r.kind == "tableRow"
                && r.content
                    .iter()
                    .all(|cell| matches!(cell.kind.as_str(), "tableCell" | "tableHeader"))
        });
        if !rows_ok {
            return true;
        }
        let problems = crate::table::problems(table);
        if problems.is_empty() {
            return true;
        }
        match self.mode {
            Mode::Strict => {
                for p in problems {
                    self.error(at, p);
                }
                true
            }
            Mode::Migrate => match crate::table::even_out(self.c, table) {
                Some(true) => {
                    self.note(at, format!("evened out a table: {}", problems.join("; ")));
                    true
                }
                Some(false) => {
                    self.note(at, format!("kept a table as it is: {}", problems.join("; ")));
                    true
                }
                None => {
                    self.note(at, "dropped a table with no cells");
                    false
                }
            },
        }
    }

    /// A markdown list holding task items, which the contract keeps in a
    /// task list of their own: each run of tasks becomes a task list, each
    /// run of other items a list of the kind it was in, numbered on where it
    /// left off. A bullet list of nothing but tasks is just a task list.
    fn task_runs(
        &mut self,
        name: &str,
        attrs: Map<String, Value>,
        items: Vec<Node>,
        out: &mut Vec<Node>,
        at: &str,
    ) {
        let start = attrs.get("start").and_then(Value::as_i64).unwrap_or(1);
        let mut runs: Vec<(bool, i64, Vec<Node>)> = vec![];
        for (i, item) in items.into_iter().enumerate() {
            let task = item.kind == "taskItem";
            match runs.last_mut() {
                Some((t, _, run)) if *t == task => run.push(item),
                _ => runs.push((task, start + i as i64, vec![item])),
            }
        }
        if name == "orderedList" {
            self.note(at, "an ordered list's tasks became a task list, without numbers");
        }
        if runs.len() > 1 {
            self.note(
                at,
                format!(
                    "split a list of tasks and other items into {} lists",
                    runs.len()
                ),
            );
        }
        for (task, first, run) in runs {
            if task {
                out.push(Node::element("taskList", self.c.default_attrs("taskList"), run));
                continue;
            }
            let mut a = attrs.clone();
            if a.contains_key("start") {
                a.insert("start".into(), Value::from(first));
            }
            out.push(Node::element(name, a, run));
        }
    }

    /// What stands in, on migration's path, for an element that could not be
    /// built: its `alt` text, or the text inside it, with a note saying what
    /// was dropped. Nothing on the model's path, where `reasons` are already
    /// errors.
    fn stand_in(
        &mut self,
        el: DomRef,
        name: &str,
        reasons: Vec<String>,
        at: &str,
    ) -> Option<String> {
        if self.mode == Mode::Strict {
            return None;
        }
        let Dom::Element(e) = el.value() else {
            return None;
        };
        let inner: String = ElementRef::wrap(el)
            .map(|e| e.text().collect())
            .unwrap_or_default();
        let (text, kept) = match e.attr("alt").filter(|a| !a.trim().is_empty()) {
            Some(alt) => (alt.to_string(), "; kept its alt text"),
            None if !inner.trim().is_empty() => (inner, "; kept its text"),
            None => (String::new(), ""),
        };
        self.note(
            at,
            format!("dropped the {name}: {}{kept}", reasons.join("; ")),
        );
        Some(text)
    }

    /// A textblock whose inline content held a block (an image inside a
    /// paragraph) splits around it.
    fn split_textblock(
        &mut self,
        name: &str,
        attrs: Map<String, Value>,
        pieces: Vec<Piece>,
        out: &mut Vec<Node>,
        keep_empty: bool,
    ) {
        let mut seg: Vec<Node> = Vec::new();
        let mut lifted = false;
        let mut emitted = false;
        let flush = |seg: &mut Vec<Node>, out: &mut Vec<Node>, emitted: &mut bool| {
            trim_trailing(seg);
            if !seg.is_empty() {
                out.push(Node::element(name, attrs.clone(), std::mem::take(seg)));
                *emitted = true;
            }
        };
        for p in pieces {
            match p {
                Piece::Inline(n) => seg.push(n),
                Piece::Block(b) => {
                    flush(&mut seg, out, &mut emitted);
                    out.push(b);
                    lifted = true;
                }
            }
        }
        flush(&mut seg, out, &mut emitted);
        if !emitted && !lifted && keep_empty {
            out.push(Node::element(name, attrs, vec![]));
        }
    }

    fn inline_content(
        &mut self,
        block: &str,
        kids: Vec<DomRef>,
        marks: &[Mark],
        pieces: &mut Vec<Piece>,
        at: &str,
    ) {
        for kid in kids {
            match kid.value() {
                Dom::Text(t) => self.add_text(t, marks, pieces),
                Dom::Element(e) => {
                    let tag = e.name();
                    if let Some((mspec, _)) = self.c.mark_rule_for_tag(tag) {
                        let here = format!("{at} > {tag}");
                        let Some(attrs) = self.mark_attrs(mspec, kid, &here) else {
                            self.inline_content(
                                block,
                                kid.children().collect(),
                                marks,
                                pieces,
                                &here,
                            );
                            continue;
                        };
                        let mark = Mark {
                            kind: mspec.name.clone(),
                            attrs,
                        };
                        if !self.c.allows_mark(block, &mark.kind) {
                            self.error(&here, format!("`{block}` cannot carry `{}`", mark.kind));
                            continue;
                        }
                        let mut next = marks.to_vec();
                        if let Some(clash) = next
                            .iter()
                            .find(|m| self.c.marks_exclude(&m.kind, &mark.kind))
                        {
                            if clash.kind != mark.kind {
                                let msg = format!(
                                    "`{}` cannot be combined with `{}`",
                                    mark.kind, clash.kind
                                );
                                let kept = format!("{msg}; kept `{}`", clash.kind);
                                self.refuse_or_note(&here, msg, kept);
                            }
                            // Same mark nested in itself: the inner one is redundant.
                            self.inline_content(
                                block,
                                kid.children().collect(),
                                marks,
                                pieces,
                                &here,
                            );
                            continue;
                        }
                        next.push(mark);
                        next.sort_by_key(|m| self.c.mark_rank(&m.kind));
                        self.inline_content(block, kid.children().collect(), &next, pieces, &here);
                        continue;
                    }
                    let here = format!("{at} > {tag}");
                    if let Some((spec, rule)) = self.match_rule(tag, e) {
                        if self.c.is_inline(&spec.name) {
                            let attrs = match self.attrs(spec, rule, kid, &here) {
                                Ok(attrs) => attrs,
                                Err(reasons) => {
                                    if let Some(text) = self.stand_in(kid, &spec.name, reasons, &here) {
                                        self.add_text(&text, marks, pieces);
                                    }
                                    continue;
                                }
                            };
                            if spec.spec.atom
                                && kid.children().any(
                                    |k| !matches!(k.value(), Dom::Text(t) if t.chars().all(is_ws)),
                                )
                            {
                                self.error(
                                    &here,
                                    format!(
                                        "<{tag}> holds no content. A custom element cannot self-close in HTML: \
                                         write <{tag} ...></{tag}>, or the text after it lands inside it"
                                    ),
                                );
                                continue;
                            }
                            if !marks.is_empty() {
                                self.note(
                                    &here,
                                    format!("<{tag}> does not carry formatting; dropped it"),
                                );
                            }
                            pieces.push(Piece::Inline(Node::element(&spec.name, attrs, vec![])));
                            continue;
                        }
                        // A block inside a textblock: moved out, as ProseMirror does.
                        let mut lifted = Vec::new();
                        if let Some(text) = self.block_element(kid, &mut lifted, at) {
                            self.add_text(&text, marks, pieces);
                        }
                        if !lifted.is_empty() {
                            self.note(&here, format!("moved <{tag}> out of the surrounding text"));
                        }
                        pieces.extend(lifted.into_iter().map(Piece::Block));
                        continue;
                    }
                    if DROPPED.contains(&tag) {
                        self.refuse_or_note(
                            &here,
                            format!("<{tag}> is not document content"),
                            format!("dropped <{tag}> and its content"),
                        );
                        continue;
                    }
                    if self.is_migration_tag(tag) {
                        continue;
                    }
                    match self.mode {
                        Mode::Strict => self.error(
                            &here,
                            format!("<{tag}> is not part of the document contract"),
                        ),
                        Mode::Migrate => {
                            self.note(&here, format!("unwrapped <{tag}>, kept its text"));
                            self.inline_content(block, kid.children().collect(), marks, pieces, at);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// ProseMirror's whitespace rules for non-`pre` text: runs collapse to one
    /// space; a leading space goes when nothing precedes it in the block, a
    /// hard break does, or the previous text already ends in one.
    fn add_text(&mut self, raw: &str, marks: &[Mark], pieces: &mut Vec<Piece>) {
        let mut value = collapse_ws(raw);
        if value.starts_with(' ') {
            let strip = match pieces.last() {
                None | Some(Piece::Block(_)) => true,
                Some(Piece::Inline(n)) => {
                    n.kind == "hardBreak"
                        || n.text.as_deref().map(|t| t.ends_with(' ')).unwrap_or(false)
                }
            };
            if strip {
                value.remove(0);
            }
        }
        if value.is_empty() {
            return;
        }
        if let Some(Piece::Inline(last)) = pieces.last_mut() {
            if last.is_text() && last.marks == marks {
                if let Some(t) = last.text.as_mut() {
                    t.push_str(&value);
                    return;
                }
            }
        }
        pieces.push(Piece::Inline(Node::text(&value, marks.to_vec())));
    }

    fn fill(&mut self, parent: &str, out: &mut Vec<Node>, at: &str) {
        let kinds: Vec<&str> = out.iter().map(|n| n.kind.as_str()).collect();
        if self.c.content_matches(parent, &kinds) {
            return;
        }
        if self.mode == Mode::Migrate
            && matches!(parent, "bulletList" | "orderedList")
            && kinds.iter().all(|k| matches!(*k, "listItem" | "taskItem"))
            && kinds.contains(&"taskItem")
        {
            // A GFM list holding tasks; the caller splits it (`task_runs`).
            return;
        }
        let mut with_para = vec!["paragraph"];
        with_para.extend(kinds.iter().copied());
        if self.c.content_matches(parent, &with_para) {
            out.insert(0, Node::element("paragraph", Map::new(), vec![]));
            return;
        }
        let expr = self
            .c
            .node(parent)
            .and_then(|n| n.spec.content.clone())
            .unwrap_or_default();
        let got = if kinds.is_empty() {
            "nothing".to_string()
        } else {
            kinds.join(", ")
        };
        self.error(at, format!("`{parent}` holds `{expr}`; got {got}"));
    }

    /// The node's attributes, or why the node cannot be built: a required
    /// attribute is missing or refused (an image whose `src` is `data:`).
    /// On the model's path every refused value is an error, the reasons
    /// included. On migration's, a refused value the node can do without is
    /// dropped and noted, and the caller notes the reasons a node is not
    /// built ([`Reader::stand_in`]), so one odd image does not refuse a page.
    fn attrs(
        &mut self,
        spec: &Spec,
        rule: &HtmlRule,
        el: DomRef,
        at: &str,
    ) -> Result<Map<String, Value>, Vec<String>> {
        let Dom::Element(e) = el.value() else {
            return Ok(Map::new());
        };
        let mut attrs = self.c.default_attrs(&spec.name);
        for (k, v) in &rule.set {
            attrs.insert(k.clone(), v.clone());
        }
        // (key, why) for each value the contract refuses.
        let mut refused: Vec<(String, String)> = vec![];
        for (name, value) in e.attrs() {
            if name == self.c.id.html && spec.spec.id {
                // An empty id is no id: the browser reads it the same way.
                // Markdown carries no ids: one in its HTML was copied from
                // somewhere, and may be a block's on this page already.
                match self.mode {
                    _ if value.is_empty() => {}
                    Mode::Strict => {
                        attrs.insert(self.c.id.attr.clone(), Value::String(value.to_string()));
                    }
                    Mode::Migrate => self.note(
                        at,
                        format!("dropped `{name}` on <{}>: a page gives its blocks ids", e.name()),
                    ),
                }
                continue;
            }
            if rule.matches.contains_key(name) {
                continue;
            }
            if name == "style" && self.mode == Mode::Migrate {
                self.migrate_style(spec, e.name(), value, &mut attrs, at);
                continue;
            }
            match spec
                .attrs()
                .iter()
                .find(|(_, a)| a.html.as_deref() == Some(name))
            {
                Some((key, a)) => match typed_value(self.c, a, key, value) {
                    Ok(v) => {
                        attrs.insert(key.clone(), v);
                    }
                    Err(why) => refused.push((key.clone(), why)),
                },
                None => self.unknown_attr(spec.attrs().iter(), e.name(), name, at),
            }
        }
        for (key, a) in spec.attrs() {
            if attrs.get(key).map(Value::is_null).unwrap_or(true) {
                if let Some(prefix) = &a.from_child_class {
                    let class = el
                        .children()
                        .find_map(|k| match k.value() {
                            Dom::Element(c) => Some(
                                c.classes()
                                    .find_map(|cl| cl.strip_prefix(prefix.as_str()))
                                    .map(str::to_string),
                            ),
                            _ => None,
                        })
                        .flatten();
                    if let Some(v) = class {
                        attrs.insert(key.clone(), Value::String(v));
                    }
                }
            }
        }
        // A required attribute missing, or present and refused, is one
        // reason the node cannot be built, not two.
        let mut unbuilt = vec![];
        for (key, a) in spec.attrs() {
            if a.required && attrs.get(key).map(Value::is_null).unwrap_or(true) {
                match refused.iter().position(|(k, _)| k == key) {
                    Some(i) => unbuilt.push(refused.remove(i).1),
                    None => unbuilt.push(format!(
                        "<{}> needs `{}`",
                        e.name(),
                        a.html.as_deref().unwrap_or(key)
                    )),
                }
            }
        }
        for (key, why) in refused {
            self.refuse_or_note(
                at,
                why.clone(),
                format!("dropped the value of `{key}` on <{}>: {why}", e.name()),
            );
        }
        if unbuilt.is_empty() {
            return Ok(attrs);
        }
        if self.mode == Mode::Strict {
            for why in &unbuilt {
                self.error(at, why.clone());
            }
        }
        Err(unbuilt)
    }

    /// Migration reads one inline style: `text-align`, which pulldown-cmark
    /// writes on table cells, onto the attribute rendered as `align`. Every
    /// other declaration is dropped and noted.
    fn migrate_style(
        &mut self,
        spec: &Spec,
        tag: &str,
        style: &str,
        attrs: &mut Map<String, Value>,
        at: &str,
    ) {
        for decl in style.split(';').map(str::trim).filter(|d| !d.is_empty()) {
            if let Some((prop, value)) = decl.split_once(':') {
                let align = spec
                    .attrs()
                    .iter()
                    .find(|(_, a)| a.html.as_deref() == Some("align"));
                if let (true, Some((key, a))) =
                    (prop.trim().eq_ignore_ascii_case("text-align"), align)
                {
                    let v = Value::String(value.trim().to_ascii_lowercase());
                    if a.check(self.c, key, &v).is_ok() {
                        attrs.insert(key.clone(), v);
                        continue;
                    }
                }
            }
            self.note(at, format!("dropped style `{decl}` on <{tag}>"));
        }
    }

    /// The mark's attributes, or `None` when the mark cannot be built: a
    /// required attribute is missing or refused (a link to `javascript:`).
    /// Refused on the model's path; on migration's the mark is dropped, its
    /// text kept, and a note says so, so one odd link does not refuse a page.
    fn mark_attrs(
        &mut self,
        spec: &crate::contract::Mark,
        el: DomRef,
        at: &str,
    ) -> Option<Map<String, Value>> {
        let Dom::Element(e) = el.value() else {
            return Some(Map::new());
        };
        let mut attrs = Map::new();
        for (k, a) in spec.attrs() {
            attrs.insert(k.clone(), a.default_value());
        }
        let mut refused: Vec<String> = vec![];
        for (name, value) in e.attrs() {
            match spec
                .attrs()
                .iter()
                .find(|(_, a)| a.html.as_deref() == Some(name))
            {
                Some((key, a)) => match typed_value(self.c, a, key, value) {
                    Ok(v) => {
                        attrs.insert(key.clone(), v);
                    }
                    Err(message) => refused.push(message),
                },
                None => self.unknown_attr(spec.attrs().iter(), e.name(), name, at),
            }
        }
        for (key, a) in spec.attrs() {
            let missing = attrs.get(key).map(Value::is_null).unwrap_or(true);
            if a.required && missing && refused.is_empty() {
                refused.push(format!(
                    "<{}> needs `{}`",
                    e.name(),
                    a.html.as_deref().unwrap_or(key)
                ));
            }
        }
        if refused.is_empty() {
            return Some(attrs);
        }
        match self.mode {
            Mode::Strict => {
                for message in refused {
                    self.error(at, message);
                }
            }
            Mode::Migrate => self.note(
                at,
                format!(
                    "dropped the {}: {}; kept its text",
                    spec.name,
                    refused.join("; ")
                ),
            ),
        }
        None
    }

    fn unknown_attr<'a>(
        &mut self,
        known: impl Iterator<Item = &'a (String, AttrSpec)>,
        tag: &str,
        name: &str,
        at: &str,
    ) {
        let allowed: Vec<&str> = known.filter_map(|(_, a)| a.html.as_deref()).collect();
        let msg = if allowed.is_empty() {
            format!("<{tag}> takes no attributes; got `{name}`")
        } else {
            format!("<{tag}> takes {}; got `{name}`", allowed.join(", "))
        };
        let dropped = format!("{msg} (dropped)");
        self.refuse_or_note(at, msg, dropped);
    }

}

/// One HTML attribute value as the contract types it, or why it is refused.
/// A URL is kept as a browser reads it ([`normalize_url`]), so what is
/// checked is what a click would follow.
fn typed_value(c: &Contract, a: &AttrSpec, key: &str, raw: &str) -> Result<Value, String> {
    let v = match a.ty {
        AttrType::Int => match raw.trim().parse::<i64>() {
            Ok(n) => Value::from(n),
            Err(_) => return Err(format!("`{key}` must be a whole number; got \"{raw}\"")),
        },
        AttrType::Bool => match raw.trim() {
            "" | "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            other => return Err(format!("`{key}` must be true or false; got \"{other}\"")),
        },
        AttrType::Url => Value::String(normalize_url(raw)),
        AttrType::String | AttrType::Route => Value::String(raw.to_string()),
    };
    a.check(c, key, &v).map(|()| v)
}

fn trim_trailing(seg: &mut Vec<Node>) {
    if let Some(last) = seg.last_mut() {
        if let Some(t) = last.text.as_mut() {
            let trimmed = t.trim_end_matches(is_ws).len();
            if trimmed == 0 {
                seg.pop();
            } else {
                t.truncate(trimmed);
            }
        }
    }
}

/// `<li><input type="checkbox" checked>` as pulldown-cmark writes task items.
fn task_marker(li: DomRef) -> Option<bool> {
    let first = li.children().find(|k| match k.value() {
        Dom::Text(t) => !t.chars().all(is_ws),
        _ => true,
    })?;
    let check = |n: DomRef| match n.value() {
        Dom::Element(e) if e.name() == "input" && e.attr("type") == Some("checkbox") => {
            Some(e.attr("checked").is_some())
        }
        _ => None,
    };
    check(first).or_else(|| {
        first
            .children()
            .find(|k| matches!(k.value(), Dom::Element(_)))
            .and_then(check)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn strict(html: &str) -> Outcome {
        parse(&Contract::load(), html, "doc", Mode::Strict)
    }

    fn kinds(nodes: &[Node]) -> Vec<String> {
        nodes.iter().map(|n| n.kind.clone()).collect()
    }

    #[test]
    fn paragraphs_marks_and_whitespace() {
        let o = strict("<p>  Hello <strong>bold <em>both</em></strong>  world </p>\n<p></p>");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        let p = &o.nodes[0];
        assert_eq!(p.content[0].text.as_deref(), Some("Hello "));
        assert_eq!(p.content[1].text.as_deref(), Some("bold "));
        assert_eq!(p.content[2].marks.len(), 2);
        assert_eq!(p.content[3].text.as_deref(), Some(" world"));
        assert_eq!(o.nodes[1].content.len(), 0);
    }

    #[test]
    fn loose_text_wraps_and_lists_fill() {
        let o = strict("<ul><li>one</li><li><p>two</p><ul><li>nested</li></ul></li><li></li></ul>");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        let list = &o.nodes[0];
        assert_eq!(kinds(&list.content[0].content), ["paragraph"]);
        assert_eq!(kinds(&list.content[1].content), ["paragraph", "bulletList"]);
        assert_eq!(kinds(&list.content[2].content), ["paragraph"]);
    }

    #[test]
    fn tables_read_through_tbody() {
        let o = strict("<table><tr><th>A</th><th>B</th><th>C</th></tr><tr><td>1</td><td colspan=\"2\">2</td></tr></table>");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        let t = &o.nodes[0];
        assert_eq!(kinds(&t.content), ["tableRow", "tableRow"]);
        assert_eq!(t.content[1].content[1].attrs["colspan"], json!(2));
    }

    #[test]
    fn unknown_tags_and_attributes_are_refused() {
        let o = strict("<p>Met on <time datetime=\"2026-10-07\">Tuesday</time></p><div>x</div><p class=\"lead\">y</p>");
        let msgs: Vec<&str> = o.errors.iter().map(|e| e.message.as_str()).collect();
        assert!(msgs.iter().any(|m| m.contains("<time>")), "{msgs:?}");
        assert!(msgs.iter().any(|m| m.contains("<div>")), "{msgs:?}");
        assert!(msgs.iter().any(|m| m.contains("`class`")), "{msgs:?}");
    }

    #[test]
    fn migration_unwraps_and_says_so() {
        let o = parse(
            &Contract::load(),
            "<p>Met on <time>Tuesday</time></p>",
            "doc",
            Mode::Migrate,
        );
        assert!(o.errors.is_empty());
        assert_eq!(o.nodes[0].text_content(), "Met on Tuesday");
        assert_eq!(o.notes.len(), 1);
    }

    #[test]
    fn self_closed_widget_is_caught() {
        let o = strict("<p>See <virtues-mention to=\"/person/1\" label=\"Nick\"/> today.</p>");
        assert!(
            o.errors
                .iter()
                .any(|e| e.message.contains("cannot self-close")),
            "{:?}",
            o.errors
        );
        let ok = strict(
            "<p>See <virtues-mention to=\"/person/1\" label=\"Nick\"></virtues-mention> today.</p>",
        );
        assert!(ok.errors.is_empty(), "{:?}", ok.errors);
        assert_eq!(kinds(&ok.nodes[0].content), ["text", "mention", "text"]);
    }

    #[test]
    fn self_closed_is_refused_even_with_nothing_after_it() {
        // Nothing follows to be swallowed, so the tree looks fine; the next
        // write that appends after it would not.
        let o = strict("<p>Chart:</p><virtues-applet ref=\"sleep-week\"/>");
        assert_eq!(o.errors.len(), 1, "{:?}", o.errors);
        assert!(
            o.errors[0].message.contains("<virtues-applet/>"),
            "{:?}",
            o.errors
        );
        // Void elements may self-close; a slash inside an unquoted value is
        // part of the value; a comment is not markup.
        let ok = strict("<p>a<br/>b <a href=/x/>c</a></p><img src=\"/i.png\" /><!-- <p/> -->");
        assert!(ok.errors.is_empty(), "{:?}", ok.errors);
        assert_eq!(ok.nodes[0].content[3].marks[0].attrs["href"], json!("/x/"));
        assert_eq!(
            scan("<a title='x/>' href=\"/y\"/><p/><P />").self_closed,
            ["a", "p"]
        );
    }

    #[test]
    fn attributes_are_typed_and_checked() {
        let o = strict("<h7>x</h7><ol start=\"three\"><li>a</li></ol><aside data-tone=\"loud\"><p>x</p></aside>");
        assert_eq!(o.errors.len(), 3, "{:?}", o.errors);
        let ok = strict(
            "<ol start=\"3\"><li>a</li></ol><aside data-tone=\"tip\"><p>x</p></aside><h2>y</h2>",
        );
        assert!(ok.errors.is_empty(), "{:?}", ok.errors);
        assert_eq!(ok.nodes[0].attrs["start"], json!(3));
        assert_eq!(ok.nodes[2].attrs["level"], json!(2));
    }

    #[test]
    fn headings_one_to_six() {
        let o = strict("<h1>a</h1><h2>b</h2><h3>c</h3><h4>d</h4><h5>e</h5><h6>f</h6>");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        let levels: Vec<Value> = o.nodes.iter().map(|n| n.attrs["level"].clone()).collect();
        assert_eq!(
            levels,
            [json!(1), json!(2), json!(3), json!(4), json!(5), json!(6)]
        );
        assert!(o.nodes.iter().all(|n| n.kind == "heading"));
    }

    #[test]
    fn table_cells_read_align() {
        let o = strict("<table><tr><th align=\"center\">A</th><th>B</th></tr><tr><td align=\"right\">1</td><td>2</td></tr></table>");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        let rows = &o.nodes[0].content;
        assert_eq!(rows[0].content[0].attrs["align"], json!("center"));
        assert_eq!(rows[0].content[1].attrs["align"], Value::Null);
        assert_eq!(rows[1].content[0].attrs["align"], json!("right"));
        // An alignment outside the enum, and the inline style the model must
        // not reach for, are both refused.
        let bad = strict("<table><tr><td align=\"justify\">x</td><td style=\"text-align: left\">y</td></tr></table>");
        assert_eq!(bad.errors.len(), 2, "{:?}", bad.errors);
    }

    #[test]
    fn migration_maps_text_align_style_onto_align() {
        let c = Contract::load();
        let html = "<table><thead><tr><th style=\"text-align: center\">A</th><th style=\"text-align: right; color: red\">B</th></tr></thead>\
                    <tbody><tr><td style=\"text-align: center\">1</td><td style=\"text-align: justify\">2</td></tr></tbody></table>";
        let o = parse(&c, html, "doc", Mode::Migrate);
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        let rows = &o.nodes[0].content;
        assert_eq!(rows[0].content[0].attrs["align"], json!("center"));
        assert_eq!(rows[0].content[1].attrs["align"], json!("right"));
        assert_eq!(rows[1].content[0].attrs["align"], json!("center"));
        assert_eq!(rows[1].content[1].attrs["align"], Value::Null);
        let notes: Vec<&str> = o.notes.iter().map(|n| n.message.as_str()).collect();
        assert_eq!(notes.len(), 2, "{notes:?}");
        assert!(notes.iter().any(|m| m.contains("color: red")), "{notes:?}");
        assert!(notes.iter().any(|m| m.contains("justify")), "{notes:?}");
    }

    #[test]
    fn image_in_paragraph_moves_out() {
        let o = strict("<p>before <img src=\"/a.png\" alt=\"A\"> after</p>");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        assert_eq!(kinds(&o.nodes), ["paragraph", "image", "paragraph"]);
    }

    #[test]
    fn code_block_keeps_whitespace_and_language() {
        let o = strict("<pre><code class=\"language-rust\">fn main() {\n    x  y\n}</code></pre>");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        assert_eq!(o.nodes[0].attrs["language"], json!("rust"));
        assert_eq!(
            o.nodes[0].content[0].text.as_deref(),
            Some("fn main() {\n    x  y\n}")
        );
    }

    #[test]
    fn markup_inside_a_code_block_is_refused() {
        // `Vec<String>` unescaped: HTML reads `<string>` as an element and
        // the type parameter would vanish from the code.
        let o = strict("<pre><code>let v: Vec<String> = vec![];</code></pre>");
        assert_eq!(o.errors.len(), 1, "{:?}", o.errors);
        assert!(o.errors[0].message.contains("&lt;"), "{:?}", o.errors);
        let ok = strict("<pre><code>let v: Vec&lt;String&gt; = vec![];</code></pre>");
        assert!(ok.errors.is_empty(), "{:?}", ok.errors);
        assert_eq!(ok.nodes[0].text_content(), "let v: Vec<String> = vec![];");
    }

    #[test]
    fn code_coexists_with_other_marks() {
        let o =
            strict("<p><a href=\"/x\"><code>fn</code></a> <strong><code>bold</code></strong></p>");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        assert_eq!(o.nodes[0].content[0].marks.len(), 2);
        assert_eq!(o.nodes[0].content[2].marks.len(), 2);
    }

    #[test]
    fn list_holding_a_list_is_refused() {
        let o = strict("<ul><li>a</li><ul><li>b</li></ul></ul>");
        assert_eq!(o.errors.len(), 1, "{:?}", o.errors);
    }

    #[test]
    fn errors_past_the_limit_are_counted_not_kept() {
        let o = strict(&format!("<p>{}</p>", "<span>x</span>".repeat(MAX_PROBLEMS + 50)));
        assert_eq!(o.errors.len(), MAX_PROBLEMS + 1);
        assert_eq!(
            o.errors.last().map(|e| e.message.as_str()),
            Some("50 more errors left out")
        );
    }

    #[test]
    fn formatting_closed_in_its_block_is_not_counted_as_rebuilt() {
        let s = scan("<p><b>x</b></p><p><a href=\"/x\"><em>y</em></a></p>");
        assert_eq!(s.rebuilt, 0);
        // Left open, it is copied into each block that follows.
        let s = scan("<p><b>x<p>y<p>z");
        assert_eq!(s.rebuilt, 2);
        assert_eq!(scan("<p a b=1 c='2' d=\"3\">").most_attrs, 4);
    }

    /// The scan sees the tags HTML's tokenizer sees: a comment ends where
    /// the tokenizer ends it, and its bogus comments, a doctype and an end
    /// tag's attributes hide nothing past their `>`.
    #[test]
    fn the_scan_reads_tags_where_htmls_tokenizer_does() {
        for (html, depth) in [
            ("<!--><div><div>", 2),
            ("<!---><div><div>", 2),
            ("<!-- a --!><div><div>", 2),
            ("<!-- a -- ><div><div>", 0),
            ("<!-- <div> --><div>", 1),
            ("<!----><div>", 1),
            ("<!DOCTYPE html><div>", 1),
            ("<!x <div title=\"><div>", 1),
            ("<?x <div title=\"><div>", 1),
            ("</# <div title=\"><div>", 1),
            ("</><div>", 1),
            ("<div></div title=\"<!--\"><div>", 1),
        ] {
            let s = scan(html);
            assert_eq!(s.open_depth, depth, "{html}");
            assert_eq!(s.two_ways, None, "{html}");
        }
        assert_eq!(scan("<p></p a b c>").most_attrs, 3);
    }

    /// What one place reads as text and another as tags: the scan reads it
    /// as tags, and says so when that crosses where the text would end.
    #[test]
    fn text_html_reads_two_ways_is_named() {
        for (html, what) in [
            ("<style><!--</style><div>", Some("style")),
            ("<textarea><b title=\"</textarea>\">", Some("textarea")),
            ("<script>if (a<b) { x() }</script><p>x</p>", Some("script")),
            ("<title><!--</TITLE ><div>", Some("title")),
            ("<svg><![CDATA[><!--]]><div>", Some("CDATA")),
            ("<style>p { color: red }</style><!-- note --><div>", None),
            ("<svg><style><div><div></style>", None),
            ("<![CDATA[x]]><div>", None),
            ("<style><div title=\"</styles>\"></style>", None),
        ] {
            assert_eq!(scan(html).two_ways, what, "{html}");
        }
        // Read as tags, the content counts.
        assert_eq!(scan("<svg><style><div><div></style>").open_depth, 4);
        let o = strict("<style><!--</style><p>x</p>");
        assert!(
            o.errors.iter().any(|e| e.message.contains("reads two ways")),
            "{:?}",
            o.errors
        );
    }

    #[test]
    fn an_empty_id_is_no_id() {
        let o = strict("<p data-id=\"\">x</p>");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        assert_eq!(o.nodes[0].id(), None);
    }
}

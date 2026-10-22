//! Document tree → canonical HTML (what the model reads, with block ids) and
//! → markdown (what search, embeddings and the `content` column read). An
//! article's footnotes, figures and veils export as the day page's readers
//! read them: a marker in the text and its note on a line after the last
//! block, a `figure` fence, and `⟦…⟧`.
//!
//! The HTML is byte-identical to the browser editor's `getHTML()` for the
//! same tree: same tags, same attribute order, attributes at their default
//! value left out, marks nested the way ProseMirror's serializer nests them.
//! One exception: a code block whose text starts with a line feed is written
//! with the extra one HTML's parser drops after `<pre>`, which a browser's
//! serializer leaves out.
//! A node or mark the contract does not name renders as nothing (its
//! children, for a node); `validate` is what refuses such a tree.

use crate::contract::Contract;
use crate::model::{Mark, Node, ITEM_NUMBER};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

const VOID: &[&str] = &["img", "br", "hr"];

#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Write `data-id` on blocks. On for the model, off for publishing.
    pub ids: bool,
}

pub fn html(c: &Contract, nodes: &[Node], opts: &Options) -> String {
    let mut out = String::new();
    for n in nodes {
        node_html(c, n, opts, &mut out);
    }
    out
}

fn escape_text(s: &str, out: &mut String) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\u{a0}' => out.push_str("&nbsp;"),
            c => out.push(c),
        }
    }
}

fn escape_attr(s: &str, out: &mut String) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\u{a0}' => out.push_str("&nbsp;"),
            c => out.push(c),
        }
    }
}

fn attr_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

fn open_tag(tag: &str, attrs: &[(String, String)], out: &mut String) {
    out.push('<');
    out.push_str(tag);
    for (k, v) in attrs {
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        escape_attr(v, out);
        out.push('"');
    }
    out.push('>');
}

fn node_html(c: &Contract, n: &Node, opts: &Options, out: &mut String) {
    if n.is_text() {
        escape_text(n.text.as_deref().unwrap_or(""), out);
        return;
    }
    let rule = c
        .node(&n.kind)
        .and_then(|spec| spec.render_rule(&n.attrs).map(|rule| (spec, rule)));
    let Some((spec, rule)) = rule else {
        // `doc` renders as its children.
        for child in &n.content {
            node_html(c, child, opts, out);
        }
        return;
    };
    let mut attrs: Vec<(String, String)> = rule
        .matches
        .iter()
        .map(|(k, v)| (k.clone(), attr_string(v)))
        .collect();
    if spec.spec.id && opts.ids {
        if let Some(id) = n.id() {
            attrs.push((c.id.html.clone(), id.to_string()));
        }
    }
    for (key, a) in spec.attrs() {
        let Some(html_name) = &a.html else { continue };
        let v = n.attrs.get(key).cloned().unwrap_or(Value::Null);
        if v.is_null() || v == a.default_value() {
            continue;
        }
        attrs.push((html_name.clone(), attr_string(&v)));
    }
    open_tag(&rule.tag, &attrs, out);
    if VOID.contains(&rule.tag.as_str()) {
        return;
    }
    if rule.tag == "pre" && n.text_content().starts_with('\n') {
        // HTML's parser drops a line feed straight after `<pre>`, so text
        // that starts with one is written with one more, or it reads back
        // a line short. A browser's serializer does not write it, so here
        // the editor's `getHTML()` differs from this HTML, and this is the
        // one that reads back.
        out.push('\n');
    }
    if let Some(w) = &rule.wrap {
        open_tag(w, &[], out);
    }
    if c.is_textblock(&n.kind) {
        inline_html(c, &n.content, opts, out);
    } else {
        for child in &n.content {
            node_html(c, child, opts, out);
        }
    }
    if let Some(w) = &rule.wrap {
        out.push_str(&format!("</{w}>"));
    }
    out.push_str(&format!("</{}>", rule.tag));
}

fn mark_open(c: &Contract, m: &Mark, out: &mut String) -> Option<String> {
    let spec = c.mark(&m.kind)?;
    let rule = spec.render_rule();
    let mut attrs = vec![];
    for (key, a) in spec.attrs() {
        let Some(html_name) = &a.html else { continue };
        let v = m.attrs.get(key).cloned().unwrap_or(Value::Null);
        if v.is_null() || v == a.default_value() {
            continue;
        }
        attrs.push((html_name.clone(), attr_string(&v)));
    }
    open_tag(&rule.tag, &attrs, out);
    Some(rule.tag.clone())
}

/// ProseMirror's DOMSerializer: keep the longest run of marks the next node
/// shares with what is open, close the rest, open what it adds.
fn inline_html(c: &Contract, nodes: &[Node], opts: &Options, out: &mut String) {
    let mut active: Vec<(Mark, String)> = vec![];
    for n in nodes {
        let marks: Vec<&Mark> = n
            .marks
            .iter()
            .filter(|m| c.mark(&m.kind).is_some())
            .collect();
        let mut keep = 0;
        while keep < active.len().min(marks.len()) && active[keep].0 == *marks[keep] {
            keep += 1;
        }
        while active.len() > keep {
            if let Some((_, tag)) = active.pop() {
                out.push_str(&format!("</{tag}>"));
            }
        }
        for m in &marks[keep..] {
            if let Some(tag) = mark_open(c, m, out) {
                active.push(((*m).clone(), tag));
            }
        }
        node_html(c, n, opts, out);
    }
    while let Some((_, tag)) = active.pop() {
        out.push_str(&format!("</{tag}>"));
    }
}

// ---------------------------------------------------------------- markdown

pub fn markdown(c: &Contract, nodes: &[Node]) -> String {
    page_md(c, nodes, false)
}

/// [`markdown`] with `<!-- {id} -->` on its own line above each top-level
/// block: the model's read of a page too long to read as HTML, which it
/// edits by those ids. Removing those lines leaves [`markdown`]'s text. A
/// block whose id a comment cannot hold as written (a line break, `--`, a
/// `>`) gets no line: the model cannot name it, and a write naming another
/// id is refused, not misplaced.
pub fn markdown_with_ids(c: &Contract, nodes: &[Node]) -> String {
    page_md(c, nodes, true)
}

fn page_md(c: &Contract, nodes: &[Node], ids: bool) -> String {
    let apart = footnotes_as_exported(nodes, false);
    let nodes = apart.as_deref().unwrap_or(nodes);
    let mut w = Md::default();
    let mut alt = false;
    for (i, n) in nodes.iter().enumerate() {
        if i > 0 {
            w.text("\n\n");
        }
        if let Some(id) = n.id().filter(|id| ids && comment_holds(id)) {
            w.text(&format!("<!-- {id} -->\n"));
        }
        alt = continues_list(nodes, i, alt);
        w.alt_list = alt;
        block_md(c, n, false, &mut w);
    }
    let notes = footnote_lines(nodes);
    if !notes.is_empty() {
        w.text("\n\n");
        w.text(&notes.join("\n"));
    }
    w.text("\n");
    w.into_string()
}

/// The definitions of the footnotes in `nodes`, as the export writes them
/// after its last block: one line per label, in the order the labels are
/// first cited, `[^label]: note`. A label's note is the first one its
/// footnotes carry; one carrying none writes no line. The note is written
/// as it is, a line break as a space: the converter reads a definition's
/// first line as text, never as markdown. Last, as the day page's readers
/// find them (`day_picture::sections` stops at the first).
pub fn footnote_lines(nodes: &[Node]) -> Vec<String> {
    let mut labels: Vec<(String, Option<String>)> = vec![];
    for n in nodes {
        n.walk(&mut |n| {
            if n.kind != "footnote" {
                return;
            }
            let Some(label) = n.attrs.get("label").and_then(Value::as_str) else {
                return;
            };
            let note = n.attrs.get("note").and_then(Value::as_str);
            match labels.iter_mut().find(|(l, _)| l == label) {
                Some((_, held)) => {
                    if held.is_none() {
                        *held = note.map(str::to_string);
                    }
                }
                None => labels.push((label.to_string(), note.map(str::to_string))),
            }
        });
    }
    labels
        .into_iter()
        .filter_map(|(label, note)| Some(format!("[^{label}]: {}", note?.replace(['\r', '\n'], " "))))
        .collect()
}

/// `nodes` with each footnote as the export writes it, or `None` when that
/// is each as it stands. A label carries one note in markdown, its one
/// definition: where footnotes of one label carry different notes (a
/// sentence pasted from another day, numbered there), the first note the
/// label carries keeps it, and each other note takes a label of its own
/// (`apart_label`), so every sentence's source is written and reads back
/// beside it. `fill`: a footnote carrying no note takes its label's, as the
/// export reads back, for comparing a page with what its export shows.
pub fn footnotes_as_exported(nodes: &[Node], fill: bool) -> Option<Vec<Node>> {
    let mut first: HashMap<String, Option<String>> = HashMap::new();
    let mut used: HashSet<String> = HashSet::new();
    let mut changes = false;
    for n in nodes {
        n.walk(&mut |n| {
            if n.kind != "footnote" {
                return;
            }
            let Some(label) = n.attrs.get("label").and_then(Value::as_str) else {
                return;
            };
            let note = n.attrs.get("note").and_then(Value::as_str);
            used.insert(label.to_string());
            let held = first.entry(label.to_string()).or_insert(None);
            match (held.as_deref(), note) {
                (None, Some(note)) => *held = Some(note.to_string()),
                (Some(held), Some(note)) => changes |= held != note,
                _ => {}
            }
        });
    }
    if !changes && fill {
        for n in nodes {
            n.walk(&mut |n| {
                let label = n.attrs.get("label").and_then(Value::as_str);
                changes |= n.kind == "footnote"
                    && n.attrs.get("note").and_then(Value::as_str).is_none()
                    && label.is_some_and(|l| first.get(l).is_some_and(Option::is_some));
            });
        }
    }
    if !changes {
        return None;
    }
    let mut given: HashMap<(String, String), Option<String>> = HashMap::new();
    let mut out = nodes.to_vec();
    for n in &mut out {
        n.walk_mut(&mut |n| {
            if n.kind != "footnote" {
                return;
            }
            let Some(label) = n.attrs.get("label").and_then(Value::as_str).map(str::to_string) else {
                return;
            };
            let Some(Some(held)) = first.get(&label) else {
                return;
            };
            match n.attrs.get("note").and_then(Value::as_str).map(str::to_string) {
                Some(note) if note != *held => {
                    let label = given
                        .entry((label.clone(), note))
                        .or_insert_with(|| apart_label(&label, &mut used))
                        .clone();
                    if let Some(label) = label {
                        n.attrs.insert("label".into(), Value::String(label));
                    }
                }
                None if fill => {
                    n.attrs.insert("note".into(), Value::String(held.clone()));
                }
                _ => {}
            }
        });
    }
    Some(out)
}

/// A label for a second note under `label`, one no footnote in `used`
/// carries, which it then holds: the label's prefix and the number after the
/// highest any label of that prefix ends in (`ev-2` beside `ev-6` → `ev-7`),
/// so the day page reads it as a source of its kind; a label ending in no
/// digit takes `-2` on, the first one free. `None` when no such label fits
/// a label's length.
fn apart_label(label: &str, used: &mut HashSet<String>) -> Option<String> {
    let digits = label.len() - label.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let (prefix, from) = if digits == 0 {
        (format!("{label}-"), 2u64)
    } else {
        let prefix = &label[..label.len() - digits];
        let highest = used
            .iter()
            .filter_map(|l| l.strip_prefix(prefix))
            .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
            .filter_map(|n| n.parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        (prefix.to_string(), highest.saturating_add(1))
    };
    let free = (from..from.saturating_add(1_000))
        .map(|n| format!("{prefix}{n}"))
        .find(|l| !used.contains(l))
        .filter(|l| crate::contract::is_label(l))?;
    used.insert(free.clone());
    Some(free)
}

/// Whether the block `nodes[i]` is a list written straight after a list of
/// its own kind, which markdown would read as the same list, and so takes
/// the other marker (`*` for `-`, `)` for `.`) unless the list before it
/// took that one. `before`: whether the block before it did.
fn continues_list(nodes: &[Node], i: usize, before: bool) -> bool {
    let list = |n: &Node| matches!(n.kind.as_str(), "bulletList" | "orderedList" | "taskList");
    i > 0 && list(&nodes[i]) && nodes[i - 1].kind == nodes[i].kind && !before
}

/// Whether `<!-- {id} -->` reads back as a comment holding exactly `id`.
fn comment_holds(id: &str) -> bool {
    !id.contains("--") && !id.contains('>') && !id.chars().any(char::is_control)
}

/// A container's prefix: `> ` for a quote, an item's marker on its first
/// line and its indent on the rest.
struct Prefix {
    first: Rc<str>,
    rest: Rc<str>,
    /// The container has had a line, so its next one takes `rest`.
    used: bool,
    /// Anything was written inside it, a line break included.
    wrote: bool,
}

impl Prefix {
    /// The prefix for the container's next line. A line holding nothing
    /// takes it without its trailing spaces.
    fn next(&self, empty: bool) -> Rc<str> {
        let p = if self.used { &self.rest } else { &self.first };
        match empty.then(|| p.trim_end()) {
            Some(trimmed) if trimmed.len() < p.len() => Rc::from(trimmed),
            _ => p.clone(),
        }
    }
}

/// Where the writing stands, at the end of what was written.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Piece {
    /// Nothing written since the last line feed.
    #[default]
    Fresh,
    /// A line being written: text, or prefixes of containers it ended.
    Partial,
    /// A container ended on its last line, already written out, and the
    /// empty piece after its last line feed was dropped.
    Done,
    /// As `Done`, but the container's last line is a blank one, still held
    /// back: to the container around it, the markdown ends in a line feed,
    /// and if that one ends here too, it drops the blank line as the empty
    /// piece after its last line feed.
    Blank,
}

/// A blank line held back, as it will be written.
struct Held {
    line: String,
    /// The outermost container (its index in [`Md::open`]) whose markdown
    /// holds this line as an empty one: from there inward, the line has
    /// taken only prefixes that are blank on a blank line, such as a list
    /// item's indent.
    blank_from: usize,
}

/// Markdown written into one buffer, line by line. A block inside a quote or
/// a list item is written once, each of its lines taking the prefix of every
/// container around it as the line is finished; building a container's
/// markdown from its children's and prefixing that would copy a page's text
/// once for every level it nests, and hold every copy at once.
///
/// What it writes is what prefixing each container's whole markdown would
/// write: a container's lines are its markdown split at line feeds, with the
/// empty piece after a last line feed dropped; an empty line takes its
/// prefix without trailing spaces; an empty container is its first prefix
/// alone. A blank line can turn out to be such a last piece, of a container
/// it ends: it is held back until what follows it shows it stays.
#[derive(Default)]
struct Md {
    out: String,
    /// The containers around what is being written, outermost first.
    open: Vec<Prefix>,
    piece: Piece,
    /// Prefixes of the containers that ended on the current line, innermost
    /// first: the line is the last of each.
    ended: Vec<Rc<str>>,
    /// The current line's text.
    line: String,
    /// Whether `ended` and `line` together hold anything.
    holds: bool,
    /// Blank lines written and held back, in order, all after `out`.
    held: Vec<Held>,
    /// How many times anything was written: a block wrote nothing when this
    /// did not move.
    writes: u64,
    /// The next list written takes its kind's other marker
    /// ([`continues_list`]); read and cleared as the list opens.
    alt_list: bool,
}

impl Md {
    fn wrote(&mut self) {
        self.writes += 1;
        if let Some(p) = self.open.last_mut() {
            p.wrote = true;
        }
    }

    /// Write out the blank lines held back: a line that is not blank follows
    /// them, so no container drops them.
    fn release(&mut self) {
        for h in self.held.drain(..) {
            self.out.push_str(&h.line);
            self.out.push('\n');
        }
    }

    /// Write `s`, which may hold line feeds.
    fn text(&mut self, s: &str) {
        for (i, piece) in s.split('\n').enumerate() {
            if i > 0 {
                self.newline();
            }
            if piece.is_empty() {
                continue;
            }
            debug_assert!(
                matches!(self.piece, Piece::Fresh | Piece::Partial),
                "text after a container, on its line"
            );
            if self.piece != Piece::Partial {
                self.release();
                self.piece = Piece::Partial;
            }
            self.line.push_str(piece);
            self.holds = true;
            self.wrote();
        }
    }

    /// End the current line.
    fn newline(&mut self) {
        self.wrote();
        if matches!(self.piece, Piece::Done | Piece::Blank) {
            // The line it ends is written, or held, already.
            self.piece = Piece::Fresh;
            return;
        }
        // Text, or a prefix, between the line's text and the next prefix out.
        let mut inner = self.ended.iter().any(|p| !p.is_empty());
        let mut blank_from = self.open.len();
        let mut prefixes: Vec<Rc<str>> = Vec::with_capacity(self.open.len());
        for (j, p) in self.open.iter_mut().enumerate().rev() {
            // Each container reads its lines as `str::lines` does: a
            // carriage return before the line feed is not part of the line.
            if self.line.ends_with('\r') {
                self.line.pop();
            }
            let next = p.next(!inner && self.line.is_empty());
            inner |= !next.is_empty();
            if !inner && self.line.is_empty() {
                blank_from = j;
            }
            prefixes.push(next);
            p.used = true;
        }
        let mut line = String::new();
        for p in prefixes.iter().rev().chain(self.ended.iter().rev()) {
            line.push_str(p);
        }
        line.push_str(&self.line);
        if blank_from < self.open.len() {
            self.held.push(Held { line, blank_from });
        } else {
            self.release();
            self.out.push_str(&line);
            self.out.push('\n');
        }
        self.line.clear();
        self.ended.clear();
        self.holds = false;
        self.piece = Piece::Fresh;
    }

    /// Open a container, at the start of a line.
    fn open(&mut self, first: &str, rest: &str) {
        debug_assert!(self.piece == Piece::Fresh, "a container opens a line");
        self.open.push(Prefix {
            first: Rc::from(first),
            rest: Rc::from(rest),
            used: false,
            wrote: false,
        });
    }

    fn close(&mut self) {
        let Some(p) = self.open.pop() else { return };
        let at = self.open.len();
        match self.piece {
            Piece::Done => {}
            Piece::Partial => {
                let next = p.next(!self.holds);
                if !next.is_empty() {
                    self.holds = true;
                    self.release();
                }
                self.ended.push(next);
            }
            // It wrote nothing: it is its first prefix alone.
            Piece::Fresh if !p.wrote => {
                let next = p.next(true);
                self.piece = Piece::Partial;
                if !next.is_empty() {
                    self.holds = true;
                    self.release();
                    self.ended.push(next);
                    self.wrote();
                }
            }
            // What it wrote ends in a line feed, after which the empty piece
            // is dropped: after its last line, or after a blank line that a
            // container inside it ended on, which goes with it.
            Piece::Fresh | Piece::Blank => {
                if self.piece == Piece::Blank {
                    self.held.pop();
                }
                self.piece = match self.held.last() {
                    Some(h) if at >= h.blank_from => Piece::Blank,
                    _ => Piece::Done,
                };
            }
        }
        if p.wrote {
            if let Some(parent) = self.open.last_mut() {
                parent.wrote = true;
            }
        }
    }

    /// What was written, with no line feed after its last line.
    fn into_string(mut self) -> String {
        while !self.open.is_empty() {
            self.close();
        }
        self.release();
        match self.piece {
            Piece::Partial => {
                for p in self.ended.iter().rev() {
                    self.out.push_str(p);
                }
                self.out.push_str(&self.line);
            }
            Piece::Done | Piece::Blank => {
                self.out.pop();
            }
            Piece::Fresh => {}
        }
        self.out
    }
}

/// One block's markdown alone, as a table cell holds it.
fn block_string(c: &Contract, n: &Node, in_table: bool) -> String {
    let mut w = Md::default();
    block_md(c, n, in_table, &mut w);
    w.into_string()
}

fn children_md(c: &Contract, n: &Node, sep: &str, w: &mut Md) {
    let mut alt = false;
    for (i, ch) in n.content.iter().enumerate() {
        if i > 0 {
            w.text(sep);
        }
        alt = continues_list(&n.content, i, alt);
        w.alt_list = alt;
        block_md(c, ch, false, w);
    }
}

/// Where inline content is written, which decides how a line break and a
/// pipe are written so that the text reads back as written.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Lines {
    /// A paragraph: lines end in a backslash break, and a pipe is escaped so
    /// no run of lines reads as a table.
    Many,
    /// A heading, which markdown ends at the line's end: a break is `<br>`.
    One,
    /// A table cell: a break is `<br>`, and pipes are left to the table,
    /// which escapes every one in the cell.
    Cell,
}

/// `in_table`: the block is in a table cell, which holds one line.
fn block_md(c: &Contract, n: &Node, in_table: bool, w: &mut Md) {
    let a = |k: &str| n.attrs.get(k).cloned().unwrap_or(Value::Null);
    match n.kind.as_str() {
        "blockquote" => {
            w.open("> ", "> ");
            children_md(c, n, "\n\n", w);
            w.close();
        }
        "callout" => {
            let tone = a("tone").as_str().unwrap_or("note").to_uppercase();
            w.text(&format!("> [!{tone}]\n"));
            w.open("> ", "> ");
            children_md(c, n, "\n\n", w);
            w.close();
        }
        "bulletList" | "orderedList" | "taskList" => {
            let alt = std::mem::take(&mut w.alt_list);
            let (bullet, stop) = if alt { ('*', ')') } else { ('-', '.') };
            let loose = n.content.iter().any(|item| item.content.len() > 1);
            // Each item's number, its own where it has one: markdown writes
            // every item's, and reads each back as written. A number below
            // zero, which no marker writes, gives way to the count.
            let mut next = a("start").as_u64().unwrap_or(1);
            for (i, item) in n.content.iter().enumerate() {
                if i > 0 {
                    w.text(if loose { "\n\n" } else { "\n" });
                }
                let number = item.attrs.get(ITEM_NUMBER).and_then(Value::as_u64).unwrap_or(next);
                next = number.saturating_add(1);
                let marker = match n.kind.as_str() {
                    "orderedList" => format!("{number}{stop} "),
                    "taskList" => {
                        let checked = item
                            .attrs
                            .get("checked")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        format!("{bullet} [{}] ", if checked { "x" } else { " " })
                    }
                    _ => format!("{bullet} "),
                };
                let indent = " ".repeat(if n.kind == "taskList" {
                    2
                } else {
                    marker.len()
                });
                w.open(&marker, &indent);
                let mut first_empty = false;
                let mut alt = false;
                for (k, b) in item.content.iter().enumerate() {
                    if k > 0 {
                        // After an empty first block, a blank line would end
                        // the item: markdown lets an item open with at most
                        // one blank line, the marker's own.
                        let after_empty = k == 1 && first_empty;
                        w.text(if loose && !after_empty { "\n\n" } else { "\n" });
                    }
                    let before = w.writes;
                    alt = continues_list(&item.content, k, alt);
                    w.alt_list = alt;
                    block_md(c, b, false, w);
                    if k == 0 {
                        first_empty = w.writes == before;
                    }
                }
                w.close();
            }
        }
        _ => w.text(&leaf_md(c, n, in_table)),
    }
}

/// The markdown of a block that holds no blocks.
fn leaf_md(c: &Contract, n: &Node, in_table: bool) -> String {
    let a = |k: &str| n.attrs.get(k).cloned().unwrap_or(Value::Null);
    let lines = |own: Lines| if in_table { Lines::Cell } else { own };
    match n.kind.as_str() {
        "paragraph" => inline_md(&n.content, lines(Lines::Many)),
        "heading" => {
            let level = a("level").as_u64().unwrap_or(1).clamp(1, 6) as usize;
            let mut text = inline_md(&n.content, lines(Lines::One));
            // A heading's closing `#`s are dropped on the way in.
            if text.ends_with('#') {
                text.insert(text.len() - 1, '\\');
            }
            format!("{} {text}", "#".repeat(level))
        }
        "codeBlock" => {
            // The fence's own newline ends the code; a newline the code ends
            // with is a line of its own, and reading back keeps it.
            let text = n.text_content();
            let lang = info_string(a("language").as_str().unwrap_or(""));
            // A backtick fence's info string cannot hold a backtick; a tilde
            // fence's can.
            let mark = if lang.contains('`') { '~' } else { '`' };
            let longest = text.split(|ch| ch != mark).map(str::len).max().unwrap_or(0);
            // An article reads a fence of exactly three backticks in the
            // language `figure` as a figure: code that would read as one
            // takes a fence of four, and stays code.
            let least = if lang == "figure" && crate::migrate::reads_as_figure(c, &text) { 4 } else { 3 };
            let fence = mark.to_string().repeat((longest + 1).max(least));
            format!("{fence}{lang}\n{text}\n{fence}")
        }
        "horizontalRule" => "---".to_string(),
        "image" => {
            let alt = a("alt").as_str().unwrap_or("").replace(['\r', '\n'], " ");
            let mut alt = escape_md(&alt, Lines::Cell, "", false);
            // The converter reads a last `|` and digits as the width, so a
            // pipe of the alt's own is written as a character reference.
            match a("width").as_i64() {
                Some(w) => alt.push_str(&format!("|{w}")),
                None => alt = alt.replace('|', "&#124;"),
            }
            format!("![{alt}]({})", destination(a("src").as_str().unwrap_or("")))
        }
        "audio" | "video" | "file" => {
            // Written as an image: the converter tells the kinds apart by the
            // name's extension (`migrate::media_kind`), as both page editors
            // do (`kindOfLink`), so a name without one reads back as a file.
            let src = a("src").as_str().unwrap_or("").to_string();
            let name = match a("name").as_str() {
                Some(name) if !name.is_empty() => name.to_string(),
                _ => last_segment(&src).to_string(),
            };
            let name = escape_md(&name.replace(['\r', '\n'], " "), Lines::Cell, "", false);
            // A last `|` and digits would read as a width.
            format!("![{}]({})", name.replace('|', "&#124;"), destination(&src))
        }
        "table" => table_md(c, n),
        "applet" => {
            let mut s = String::new();
            node_html(c, n, &Options { ids: false }, &mut s);
            s
        }
        "figure" => figure_md(c, n),
        other => format!("<!-- {other} -->"),
    }
}

/// A figure as the day page's writers write it: a fence of three backticks
/// whose info string is `figure`, then a `key: value` line for each value
/// it holds, in the contract's order, which is the order every writer uses.
/// An empty value is written as `key: `, and a line break in one as a space.
fn figure_md(c: &Contract, n: &Node) -> String {
    let mut s = String::from("```figure\n");
    if let Some(spec) = c.node("figure") {
        for (key, _) in spec.attrs() {
            if let Some(v) = n.attrs.get(key).and_then(Value::as_str) {
                s.push_str(&format!("{key}: {}\n", v.replace(['\r', '\n'], " ")));
            }
        }
    }
    s.push_str("```");
    s
}

/// The last segment of a URL's path, for a media embed with no name.
fn last_segment(url: &str) -> &str {
    let path = url.split(['?', '#']).next().unwrap_or("");
    path.rsplit('/').find(|s| !s.is_empty()).unwrap_or("")
}

/// Whether a pipe table reads back as `n`. GFM has no merged cells, no
/// header cells outside its first row, and one line per cell, so a table
/// holding any of those, or a cell holding more than one paragraph, is
/// written as its HTML instead. A first row of empty header cells is still
/// dropped on reading, as the stand-in for no header row
/// (`migrate::drop_empty_header_rows`), in either form.
fn pipes_hold(n: &Node) -> bool {
    let span = |cell: &Node, key: &str| cell.attrs.get(key).and_then(Value::as_i64).unwrap_or(1) != 1;
    n.content.iter().enumerate().all(|(i, row)| {
        let headers = row.content.iter().filter(|cell| cell.kind == "tableHeader").count();
        let placed = if i == 0 { headers == 0 || headers == row.content.len() } else { headers == 0 };
        placed
            && row.content.iter().all(|cell| {
                !span(cell, "colspan")
                    && !span(cell, "rowspan")
                    && cell.content.len() == 1
                    && cell.content[0].kind == "paragraph"
            })
    })
}

/// GFM aligns a column, not a cell: a column takes the first alignment any
/// of its cells carries. A table pipes cannot hold is its HTML (`pipes_hold`).
fn table_md(c: &Contract, n: &Node) -> String {
    if !pipes_hold(n) {
        let mut s = String::new();
        node_html(c, n, &Options { ids: false }, &mut s);
        return s;
    }
    let rows: Vec<Vec<String>> = n
        .content
        .iter()
        .map(|row| {
            row.content
                .iter()
                .map(|cell| {
                    cell.content
                        .iter()
                        .map(|b| block_string(c, b, true).replace('\n', " "))
                        .collect::<Vec<_>>()
                        .join("<br>")
                        .replace('|', "\\|")
                })
                .collect()
        })
        .collect();
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    let align: Vec<Option<&str>> = (0..width)
        .map(|col| {
            n.content
                .iter()
                .filter_map(|row| row.content.get(col))
                .find_map(|cell| cell.attrs.get("align").and_then(Value::as_str))
        })
        .collect();
    let line = |cells: &[String]| {
        let mut cells = cells.to_vec();
        cells.resize(width, String::new());
        format!("| {} |", cells.join(" | "))
    };
    let mut lines = vec![];
    let header_row = n
        .content
        .first()
        .map(|r| r.content.iter().all(|c| c.kind == "tableHeader"))
        .unwrap_or(false);
    // GFM has no table without a header row; an empty one stands in, and
    // reading the markdown back drops it again (`migrate::to_html`).
    let (head, body) = if header_row && !rows.is_empty() {
        (rows[0].clone(), &rows[1..])
    } else {
        (vec![String::new(); width], &rows[..])
    };
    lines.push(line(&head));
    let rule: Vec<&str> = align
        .iter()
        .map(|a| match a {
            Some("left") => " :--- ",
            Some("center") => " :---: ",
            Some("right") => " ---: ",
            _ => " --- ",
        })
        .collect();
    lines.push(format!("|{}|", rule.join("|")));
    for r in body {
        lines.push(line(r));
    }
    lines.join("\n")
}

/// Text as markdown that reads back as the same text: every character that
/// could start inline syntax escaped. `==` is the dialect's highlight, so an
/// `=` after another is escaped too; `&` before a letter, digit or `#` would
/// read as an entity. `before` is what the output already ends with, read
/// for its last two characters.
///
/// Proposals are CriticMarkup (`{--…--}`, `{++…++}`), which the converter
/// reads in text, so the `{` of `{--` and `{++` is escaped wherever text
/// holds one. Inside a proposal (`body`), what would end it or open another
/// span is escaped too, as the editor escaped a proposal's text: the `{` of
/// `{>>` and the `}` of `--}`, `++}` and `<<}`.
///
/// A line break inside text is written as a space, which is what markdown
/// reads one inside a paragraph as: written as a break, a blank line would
/// end the block and a line starting with `#` would start a heading.
fn escape_md(s: &str, lines: Lines, before: &str, body: bool) -> String {
    let chars: Vec<char> = s
        .chars()
        .map(|ch| if matches!(ch, '\n' | '\r') { ' ' } else { ch })
        .collect();
    let mut out = String::with_capacity(s.len());
    // The two characters before the current one, the nearer last.
    let mut tail: [Option<char>; 2] = {
        let mut last = before.chars().rev();
        let (b, a) = (last.next(), last.next());
        [a, b]
    };
    for (i, &ch) in chars.iter().enumerate() {
        let next = |k: usize| chars.get(i + k).copied();
        let opens = |c: char| next(1) == Some(c) && next(2) == Some(c);
        let escape = match ch {
            '\\' | '*' | '_' | '`' | '[' | ']' | '<' | '~' => true,
            '|' => lines != Lines::Cell,
            '=' => tail[1] == Some('='),
            '&' => next(1).is_some_and(|n| n.is_ascii_alphanumeric() || n == '#'),
            '{' => opens('-') || opens('+') || (body && opens('>')),
            '}' => body && tail[0] == tail[1] && matches!(tail[1], Some('-' | '+' | '<')),
            _ => false,
        };
        if escape {
            out.push('\\');
        }
        out.push(ch);
        tail = [tail[1], Some(ch)];
    }
    out
}

/// Escape what would make the start of a line block syntax: a heading,
/// quote, list item, thematic break or setext underline. `text` is already
/// [`escape_md`]'d, which covers `*`, `_`, `` ` ``, `~`, `[` and `<`. Up to
/// three spaces may come first and the line still counts; four or more at
/// the start of a block make an indented code block, so the first is written
/// as a character reference.
fn escape_line_start(text: &str, block_start: bool) -> String {
    let spaces = text.len() - text.trim_start_matches(' ').len();
    if spaces >= 4 {
        return if block_start {
            format!("&#32;{}", &text[1..])
        } else {
            text.to_string()
        };
    }
    let rest = &text[spaces..];
    let mut out = String::with_capacity(text.len() + 1);
    out.push_str(&text[..spaces]);
    match rest.chars().next() {
        Some('#' | '>' | '-' | '+' | '=') => {
            out.push('\\');
            out.push_str(rest);
        }
        Some(d) if d.is_ascii_digit() => {
            let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
            match rest[digits..].chars().next() {
                Some('.' | ')') if digits <= 9 => {
                    out.push_str(&rest[..digits]);
                    out.push('\\');
                    out.push_str(&rest[digits..]);
                }
                _ => out.push_str(rest),
            }
        }
        _ => out.push_str(rest),
    }
    out
}

/// A code block's language as a fence's info string that reads back as it:
/// a backslash before punctuation and an `&` that starts an entity are read
/// as an escape and a character reference there too, so both are escaped. A
/// line break would end the fence's line, so it is written as a space.
fn info_string(lang: &str) -> String {
    let mut out = String::with_capacity(lang.len());
    let mut chars = lang.chars().peekable();
    while let Some(ch) = chars.next() {
        let escape = match ch {
            '\\' => chars.peek().is_some_and(char::is_ascii_punctuation),
            '&' => chars
                .peek()
                .is_some_and(|n| n.is_ascii_alphanumeric() || *n == '#'),
            _ => false,
        };
        if escape {
            out.push('\\');
        }
        out.push(if matches!(ch, '\n' | '\r') { ' ' } else { ch });
    }
    out
}

/// A link destination that reads back as `url`. Plain when it can be; in
/// angle brackets when it holds a space, a parenthesis or an angle bracket,
/// which would end or break a plain one. A backslash before punctuation and
/// an `&` that starts an entity are read as an escape and a character
/// reference, so both are escaped. Tabs and line breaks are left out, as a
/// browser leaves them out of a URL and the converter reads it
/// ([`crate::contract::normalize_url`]): a destination cannot hold one.
fn destination(url: &str) -> String {
    let url: String = url
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    let url = url.as_str();
    let pointy = url
        .chars()
        .any(|ch| matches!(ch, ' ' | '(' | ')' | '<' | '>') || ch.is_control());
    let mut out = String::with_capacity(url.len() + 2);
    if pointy {
        out.push('<');
    }
    let mut chars = url.chars().peekable();
    while let Some(ch) = chars.next() {
        let escape = match ch {
            '\\' => chars.peek().is_some_and(char::is_ascii_punctuation) || pointy,
            '<' | '>' => pointy,
            '&' => chars
                .peek()
                .is_some_and(|n| n.is_ascii_alphanumeric() || *n == '#'),
            _ => false,
        };
        if escape {
            out.push('\\');
        }
        out.push(ch);
    }
    if pointy {
        out.push('>');
    }
    out
}

/// Text that ends in `!` just before a `[`: markdown would read the `!`
/// and the link after it as an image. `escape_md` leaves `!` alone, so one
/// at the end of `out` is bare.
fn escape_bang(out: &mut String) {
    if out.ends_with('!') {
        out.insert(out.len() - 1, '\\');
    }
}

fn delim(kind: &str) -> (&'static str, &'static str) {
    match kind {
        "bold" => ("**", "**"),
        "italic" => ("*", "*"),
        "strike" => ("~~", "~~"),
        "highlight" => ("==", "=="),
        "underline" => ("<u>", "</u>"),
        "veil" => (VEIL.0, VEIL.1),
        _ => ("", ""),
    }
}

/// The veil's delimiters. Innermost of all, so a veil inside a link or
/// around bold text reads back as the same marks; written around each run,
/// with no whitespace moved outside them, since no flanking rule reads them.
const VEIL: (&str, &str) = ("⟦", "⟧");

/// A veil's own text with its brackets written as character references: a
/// bare one would end the veil, or open another.
fn veiled(text: &str) -> String {
    text.replace(VEIL.0, "&#10214;").replace(VEIL.1, "&#10215;")
}

/// The marks a proposal is written with, as CriticMarkup.
const PROPOSALS: [(&str, &str, &str); 2] = [
    ("proposedDeletion", "{--", "--}"),
    ("proposedInsertion", "{++", "++}"),
];

fn link_of(n: &Node) -> Option<&Mark> {
    n.marks.iter().find(|m| m.kind == "link")
}

/// The proposal a text node is part of.
fn proposal_of(n: &Node) -> Option<&Mark> {
    if !n.is_text() {
        return None;
    }
    n.marks
        .iter()
        .find(|m| PROPOSALS.iter().any(|(kind, _, _)| m.kind == *kind))
}

/// Where the proposal that `nodes[i]` starts ends. An inline atom between
/// two runs of one proposal is inside it in the markdown (atoms carry no
/// marks), so the proposal reads back as one span, not two.
fn proposal_end(nodes: &[Node], i: usize, p: &Mark) -> usize {
    let mut j = i + 1;
    loop {
        while j < nodes.len() && proposal_of(&nodes[j]) == Some(p) {
            j += 1;
        }
        let mut k = j;
        while k < nodes.len() && !nodes[k].is_text() {
            k += 1;
        }
        if k > j && k < nodes.len() && proposal_of(&nodes[k]) == Some(p) {
            j = k;
            continue;
        }
        return j;
    }
}

/// Inline content → markdown. A proposal is outermost, as its mark is, and
/// written as CriticMarkup around what it holds; inside it, consecutive nodes
/// under one link become one link; inside that, delimiter marks stay open
/// across nodes that share them, and whitespace at a delimiter's edge moves
/// outside it, as CommonMark's flanking rules need.
fn inline_md(nodes: &[Node], lines: Lines) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < nodes.len() {
        let proposal = proposal_of(&nodes[i]);
        let j = match proposal {
            Some(p) => proposal_end(nodes, i, p),
            None => {
                let mut j = i + 1;
                while j < nodes.len() && proposal_of(&nodes[j]).is_none() {
                    j += 1;
                }
                j
            }
        };
        let last = j == nodes.len();
        match proposal.and_then(|p| PROPOSALS.iter().find(|(kind, _, _)| p.kind == *kind)) {
            Some((_, open, close)) => {
                out.push_str(open);
                // The opener stands before the body, so no line starts in it.
                out.push_str(&linked_md(&nodes[i..j], lines, false, last, true));
                out.push_str(close);
            }
            None => {
                let line_start = out.is_empty() || out.ends_with('\n');
                out.push_str(&linked_md(&nodes[i..j], lines, line_start, last, false));
            }
        }
        i = j;
    }
    out
}

/// Consecutive nodes under one link become one link. `line_start`: the
/// output begins a line of the block. `ends_block`: nothing in the block
/// follows these nodes. `body`: the nodes are a proposal's.
fn linked_md(
    nodes: &[Node],
    lines: Lines,
    line_start: bool,
    ends_block: bool,
    body: bool,
) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < nodes.len() {
        let link = link_of(&nodes[i]).cloned();
        let mut j = i + 1;
        while j < nodes.len() && link_of(&nodes[j]).cloned() == link {
            j += 1;
        }
        let last = ends_block && j == nodes.len();
        match link {
            Some(l) => {
                // Inside the brackets nothing starts a line.
                let text = delimited_md(&nodes[i..j], lines, false, last, body);
                let href = l.attrs.get("href").and_then(Value::as_str).unwrap_or("");
                escape_bang(&mut out);
                out.push_str(&format!("[{text}]({})", destination(href)));
            }
            None => {
                let at_line_start = if out.is_empty() {
                    line_start
                } else {
                    out.ends_with('\n')
                };
                out.push_str(&delimited_md(
                    &nodes[i..j],
                    lines,
                    at_line_start,
                    last,
                    body,
                ));
            }
        }
        i = j;
    }
    out
}

/// `line_start`: the output begins a line of the block (its first, when the
/// block's output is still empty). `ends_block`: nothing in the block follows
/// these nodes.
fn delimited_md(
    nodes: &[Node],
    lines: Lines,
    line_start: bool,
    ends_block: bool,
    body: bool,
) -> String {
    // Breaks after the last node that is not one end the block with it.
    let last_not_break = nodes.iter().rposition(|n| n.kind != "hardBreak");
    let mut out = String::new();
    let mut active: Vec<&str> = vec![];
    let block_start = line_start;
    let mut line_start = line_start;
    // Where the last footnote's marker ends in `out`, and whether it began a
    // line: what follows it straight away may need an escape.
    let mut marker: Option<(usize, bool)> = None;
    let close_to = |out: &mut String, active: &mut Vec<&str>, keep: usize| {
        // The veil's brackets keep their whitespace inside; any other
        // delimiter closing with it takes it outside, as flanking needs.
        let moves = active[keep.min(active.len())..].iter().any(|k| *k != "veil");
        let trailing = if moves { out.len() - out.trim_end_matches(' ').len() } else { 0 };
        let spaces = out.split_off(out.len() - trailing);
        while active.len() > keep {
            if let Some(k) = active.pop() {
                out.push_str(delim(k).1);
            }
        }
        out.push_str(&spaces);
    };
    for (idx, n) in nodes.iter().enumerate() {
        let wanted: Vec<&str> = n
            .marks
            .iter()
            .map(|m| m.kind.as_str())
            .filter(|k| !delim(k).0.is_empty())
            .collect();
        let mut keep = 0;
        while keep < active.len().min(wanted.len()) && active[keep] == wanted[keep] {
            keep += 1;
        }
        close_to(&mut out, &mut active, keep);
        match n.kind.as_str() {
            "text" => {
                let t = n.text.as_deref().unwrap_or("");
                let opens = keep < wanted.len();
                // Whitespace at an opening moves outside it, but for the
                // veil's alone, which keeps it.
                let moves = wanted[keep..].iter().any(|k| *k != "veil");
                let lead_len = if moves { t.len() - t.trim_start_matches(' ').len() } else { 0 };
                if opens {
                    out.push_str(&t[..lead_len]);
                }
                for k in &wanted[keep..] {
                    out.push_str(delim(k).0);
                    active.push(k);
                }
                let rest = if opens { &t[lead_len..] } else { t };
                let veil = n.marks.iter().any(|m| m.kind == "veil");
                if n.marks.iter().any(|m| m.kind == "code") {
                    // A code span reads a line break as a space; written as
                    // one, a blank line would end the block.
                    let rest = rest.replace(['\n', '\r'], " ");
                    let rest = rest.as_str();
                    // A run of ticks longer than any inside; pad when the
                    // content itself starts or ends with one.
                    let longest = rest.split(|ch| ch != '`').map(str::len).max().unwrap_or(0);
                    let ticks = "`".repeat(longest + 1);
                    let pad = if rest.starts_with('`') || rest.ends_with('`') {
                        " "
                    } else {
                        ""
                    };
                    out.push_str(&format!("{ticks}{pad}{rest}{pad}{ticks}"));
                } else {
                    let mut escaped = escape_md(rest, lines, &out, body);
                    if veil {
                        escaped = veiled(&escaped);
                    }
                    // Straight after a footnote's marker, `(` would make it a
                    // link's text, and `:` after one that begins a line of a
                    // paragraph would make it a definition.
                    if let Some((_, began)) = marker.filter(|(end, _)| *end == out.len()) {
                        if escaped.starts_with('(') || (began && lines == Lines::Many && escaped.starts_with(':')) {
                            escaped.insert(0, '\\');
                        }
                    }
                    if line_start && !opens && lines == Lines::Many {
                        let first = block_start && out.is_empty();
                        out.push_str(&escape_line_start(&escaped, first));
                    } else {
                        out.push_str(&escaped);
                    }
                }
            }
            "hardBreak" => {
                // A backslash break needs a line after it: at the end of the
                // block it would read back as a backslash.
                let trailing = ends_block && last_not_break.is_none_or(|last| idx > last);
                if lines == Lines::Many && !trailing {
                    out.push_str("\\\n");
                    line_start = true;
                    continue;
                }
                out.push_str("<br>");
            }
            "mention" => {
                let label = n.attrs.get("label").and_then(Value::as_str).unwrap_or("");
                let to = n.attrs.get("to").and_then(Value::as_str).unwrap_or("");
                escape_bang(&mut out);
                out.push_str(&format!(
                    "[@{}]({})",
                    escape_md(label, lines, "", body),
                    destination(to)
                ));
            }
            // `[^label]`, never escaped: a label is letters, digits, `_`
            // and `-`. Its note is written after the page's last block
            // (`footnote_lines`).
            "footnote" => {
                let label = n.attrs.get("label").and_then(Value::as_str).unwrap_or("");
                out.push_str(&format!("[^{label}]"));
                marker = Some((out.len(), line_start));
            }
            _ => {}
        }
        line_start = false;
    }
    close_to(&mut out, &mut active, 0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::{parse, Mode};

    fn tree(html_src: &str) -> Vec<Node> {
        let o = parse(&Contract::load(), html_src, "doc", Mode::Strict);
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        o.nodes
    }

    #[test]
    fn link_is_the_outermost_mark() {
        let c = Contract::load();
        let nodes = tree("<p><strong><a href=\"/x\">a</a></strong></p>");
        assert_eq!(
            html(&c, &nodes, &Options { ids: false }),
            "<p><a href=\"/x\"><strong>a</strong></a></p>"
        );
    }

    #[test]
    fn headings_render_and_export_at_every_level() {
        let c = Contract::load();
        let src = "<h1>a</h1><h2>b</h2><h3>c</h3><h4>d</h4><h5>e</h5><h6>f</h6>";
        let nodes = tree(src);
        assert_eq!(html(&c, &nodes, &Options { ids: false }), src);
        assert_eq!(
            markdown(&c, &nodes),
            "# a\n\n## b\n\n### c\n\n#### d\n\n##### e\n\n###### f\n"
        );
    }

    /// An item's own number is written as HTML's `<li value>`, and the
    /// export writes every item's number, its own or the count's, so the
    /// markdown reads back to the same list. An item holding the number the
    /// count gives it (one the editor added an item before) keeps it: the
    /// page's tree is written as it is.
    #[test]
    fn an_items_own_number_renders_and_exports() {
        let c = Contract::load();
        let src = "<ol start=\"2019\"><li><p>a</p></li><li value=\"2021\"><p>b</p></li><li><p>c</p></li><li value=\"2020\"><p>d</p></li></ol>";
        let nodes = tree(src);
        assert_eq!(html(&c, &nodes, &Options { ids: false }), src);
        assert_eq!(markdown(&c, &nodes), "2019. a\n2021. b\n2022. c\n2020. d\n");
        let mut pinned = tree("<ol><li>a</li><li>b</li></ol>");
        pinned[0].content[1].attrs.insert(ITEM_NUMBER.into(), Value::from(2));
        assert_eq!(
            html(&c, &pinned, &Options { ids: false }),
            "<ol><li><p>a</p></li><li value=\"2\"><p>b</p></li></ol>"
        );
        assert_eq!(markdown(&c, &pinned), "1. a\n2. b\n");
        // A number below zero, which no marker writes, gives way to the
        // count, as a start below zero does.
        let nodes = tree("<ol start=\"-1\"><li>a</li><li value=\"-5\">b</li><li value=\"7\">c</li><li>d</li></ol>");
        assert_eq!(markdown(&c, &nodes), "1. a\n2. b\n7. c\n8. d\n");
        // An item's markdown is indented by its own marker's width.
        let nodes = tree("<ol start=\"9\"><li><p>a</p><p>more</p></li><li value=\"100\"><p>b</p><p>more</p></li></ol>");
        assert_eq!(markdown(&c, &nodes), "9. a\n\n   more\n\n100. b\n\n     more\n");
    }

    #[test]
    fn table_alignment_renders_and_exports() {
        let c = Contract::load();
        let src = "<table><tbody><tr><th align=\"left\"><p>L</p></th><th align=\"center\"><p>C</p></th><th align=\"right\"><p>R</p></th><th><p>N</p></th></tr>\
                   <tr><td align=\"left\"><p>1</p></td><td align=\"center\"><p>2</p></td><td align=\"right\"><p>3</p></td><td><p>4</p></td></tr></tbody></table>";
        let nodes = tree(src);
        assert_eq!(html(&c, &nodes, &Options { ids: false }), src);
        assert_eq!(
            markdown(&c, &nodes),
            "| L | C | R | N |\n| :--- | :---: | ---: | --- |\n| 1 | 2 | 3 | 4 |\n"
        );
    }

    /// A table pipes cannot hold is written as its HTML, without ids, and
    /// every dialect a page's markdown is read in gives the same table back.
    #[test]
    fn a_table_pipes_cannot_hold_exports_as_html_and_reads_back() {
        let c = Contract::load();
        for src in [
            // Merged cells.
            "<table><tbody><tr><th colspan=\"2\"><p>Week</p></th></tr><tr><td rowspan=\"2\"><p>Mon</p></td><td><p>run</p></td></tr><tr><td><p>swim</p></td></tr></tbody></table>",
            // A cell holding a list, and one holding two paragraphs.
            "<table><tbody><tr><th><p>Item</p></th><th><p>Notes</p></th></tr><tr><td><ul><li><p>tent</p></li><li><p>stove</p></li></ul></td><td><p>one</p><p>two</p></td></tr></tbody></table>",
            // A header cell outside the first row.
            "<table><tbody><tr><td><p>a</p></td><td><p>b</p></td></tr><tr><th><p>Total</p></th><td><p>3</p></td></tr></tbody></table>",
        ] {
            let nodes = tree(src);
            let md = markdown(&c, &nodes);
            assert_eq!(md, format!("{src}\n"), "{src}");
            for (dialect, read) in [
                ("written", crate::parse_written_markdown(&md)),
                ("paste", crate::parse_pasted_markdown(&md)),
                ("article", crate::parse_article_markdown(&md)),
            ] {
                assert!(read.errors.is_empty(), "{dialect} {src}: {:?}", read.errors);
                assert_eq!(html(&c, &read.nodes, &Options { ids: false }), src, "{dialect}");
            }
        }
        // A table pipes hold stays a pipe table.
        let nodes = tree("<table><tbody><tr><td><p>a</p></td></tr><tr><td><p>b</p></td></tr></tbody></table>");
        assert_eq!(markdown(&c, &nodes), "|  |\n| --- |\n| a |\n| b |\n");
    }

    #[test]
    fn ids_render_only_when_asked() {
        let c = Contract::load();
        let nodes = tree("<p data-id=\"abc\">x</p>");
        assert_eq!(
            html(&c, &nodes, &Options { ids: true }),
            "<p data-id=\"abc\">x</p>"
        );
        assert_eq!(html(&c, &nodes, &Options { ids: false }), "<p>x</p>");
    }

    #[test]
    fn text_that_looks_like_markdown_is_escaped() {
        let c = Contract::load();
        let nodes = tree(
            "<p># h</p><p>12. n</p><p>- b</p><p>&gt; q</p><p>---</p><p>==hl==</p><p>a | b</p>\
             <p>&amp;amp; x</p><p>a<br># b<br>=</p><p>end<br></p>",
        );
        assert_eq!(
            markdown(&c, &nodes),
            "\\# h\n\n12\\. n\n\n\\- b\n\n\\> q\n\n\\---\n\n\\=\\=hl=\\=\n\n\
             a \\| b\n\n\\&amp; x\n\na\\\n\\# b\\\n\\=\n\nend<br>\n"
        );
        let cell = tree("<table><tr><th>A</th></tr><tr><td><p>a<br>b | c</p></td></tr></table>");
        assert_eq!(markdown(&c, &cell), "| A |\n| --- |\n| a<br>b \\| c |\n");
    }

    /// The export read back by the converter, as canonical HTML.
    fn read_back(md: &str) -> String {
        let c = Contract::load();
        let o = crate::migrate::from_markdown(&c, md);
        assert!(o.errors.is_empty(), "{md:?}: {:?}", o.errors);
        html(&c, &o.nodes, &Options { ids: false })
    }

    #[test]
    fn what_text_holds_stays_text_in_the_export() {
        let c = Contract::load();
        for src in [
            // A `!` before a link or a mention would make it an image.
            "<p>Wow!<a href=\"/page/pg_1\">the plan</a></p>",
            "<p>Hi!<virtues-mention to=\"/person/p_1\" label=\"Nick\"></virtues-mention></p>",
            "<p><strong>Wow!</strong><a href=\"/x\">y</a></p>",
            // An alt holding a pipe and digits is not a width.
            "<img src=\"/a.png\" alt=\"Score 3|5\">",
            "<img src=\"/a.png\" alt=\"a|b\" width=\"600\">",
            // A language a fence's info string cannot hold as written.
            "<pre data-language=\"a`b\">code</pre>",
            "<pre data-language=\"c\\+\\+&amp;x\">code</pre>",
        ] {
            let tree = tree(src);
            let md = markdown(&c, &tree);
            assert_eq!(
                read_back(&md),
                html(&c, &tree, &Options { ids: false }),
                "{src}: {md:?}"
            );
        }
    }

    #[test]
    fn a_line_break_in_text_or_a_value_breaks_out_of_nothing() {
        let c = Contract::load();
        // Trees a Yjs update or code can build; HTML ingest folds the breaks.
        let mut para = tree("<p>a</p>");
        para[0].content[0].text = Some("a\n\n# Not a heading".into());
        let mut code = tree("<p>x <code>y</code></p>");
        code[0].content[1].text = Some("y\n\n# Not a heading".into());
        let mut alt = tree("<img src=\"/a.png\" alt=\"x\">");
        alt[0].attrs.insert("alt".into(), "x\n\n# Injected".into());
        let mut lang = tree("<pre data-language=\"rust\">fn x() {}</pre>");
        lang[0]
            .attrs
            .insert("language".into(), "rust\n# Injected heading".into());
        let mut href = tree("<p><a href=\"/x\">y</a></p>");
        href[0].content[0].marks[0]
            .attrs
            .insert("href".into(), "/a\n\nb".into());
        for nodes in [para, code, alt, lang, href] {
            let md = markdown(&c, &nodes);
            let back = crate::migrate::from_markdown(&c, &md);
            assert!(back.errors.is_empty(), "{md:?}: {:?}", back.errors);
            let kinds: Vec<&str> = back.nodes.iter().map(|n| n.kind.as_str()).collect();
            assert_eq!(kinds, [nodes[0].kind.as_str()], "{md:?}");
        }
    }

    #[test]
    fn a_code_block_starting_with_a_line_feed_keeps_it() {
        let c = Contract::load();
        for (src, text) in [
            ("<pre><code>\nx</code></pre>", "\nx"),
            ("<pre>\n\nx</pre>", "\nx"),
            ("<pre><code>\n\nx</code></pre>", "\n\nx"),
        ] {
            let nodes = tree(src);
            assert_eq!(nodes[0].text_content(), text, "{src}");
            let canon = html(&c, &nodes, &Options { ids: false });
            let again = tree(&canon);
            assert_eq!(again[0].text_content(), text, "{src}: {canon:?}");
            assert_eq!(html(&c, &again, &Options { ids: false }), canon);
        }
    }

    /// A container's lines are its markdown split at line feeds, with an
    /// empty last piece dropped: a blank line that a list item's indent
    /// leaves empty and that ends the container around it is dropped with
    /// it; followed by more, or under a quote's `>`, it stays.
    #[test]
    fn blank_lines_at_a_containers_end_are_dropped_as_markdown_drops_them() {
        let c = Contract::load();
        for (tree, md) in [
            (
                r#"[{"type":"blockquote","content":[{"type":"orderedList","attrs":{"start":11},"content":[{"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"x"}]},{"type":"bulletList","content":[]}]}]}]},{"type":"paragraph","content":[{"type":"text","text":"after"}]}]"#,
                "> 11. x\n\nafter\n",
            ),
            (
                r#"[{"type":"blockquote","content":[{"type":"bulletList","content":[{"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"a"}]},{"type":"paragraph","content":[]}]}]},{"type":"paragraph","content":[{"type":"text","text":"b"}]}]}]"#,
                "> - a\n>\n>\n> b\n",
            ),
            (
                r#"[{"type":"bulletList","content":[{"type":"listItem","content":[{"type":"blockquote","content":[{"type":"paragraph","content":[{"type":"text","text":"q"}]},{"type":"paragraph","content":[]}]}]}]}]"#,
                "- > q\n  >\n",
            ),
        ] {
            let nodes: Vec<Node> = serde_json::from_str(tree).unwrap();
            assert_eq!(markdown(&c, &nodes), md, "{tree}");
        }
    }

    #[test]
    fn media_export_as_images_named_for_their_file() {
        let c = Contract::load();
        for (src, md) in [
            (
                "<audio src=\"/drive/df_3\" data-name=\"Interview.m4a\"></audio>",
                "![Interview.m4a](/drive/df_3)\n",
            ),
            (
                "<video src=\"/api/drive/files/df_2/download\" data-name=\"clip.mp4\"></video>",
                "![clip.mp4](/api/drive/files/df_2/download)\n",
            ),
            (
                "<virtues-file src=\"/api/drive/files/df_1/download\" data-name=\"Q3 [draft] 2|5.pdf\"></virtues-file>",
                "![Q3 \\[draft\\] 2&#124;5.pdf](/api/drive/files/df_1/download)\n",
            ),
            (
                "<virtues-file src=\"https://files.example.com/a/notes.txt?dl=1\"></virtues-file>",
                "![notes.txt](https://files.example.com/a/notes.txt?dl=1)\n",
            ),
        ] {
            let nodes = tree(src);
            assert_eq!(markdown(&c, &nodes), md, "{src}");
            // Named with its extension, it reads back as the same embed.
            assert_eq!(read_back(md).split(' ').next(), src.split(' ').next(), "{md}");
        }
    }

    #[test]
    fn proposals_export_as_critic_markup_outermost() {
        let c = Contract::load();
        for (src, md) in [
            (
                "<p>Lunch <virtues-del proposal=\"p1\">at noon</virtues-del><virtues-ins proposal=\"p1\">on Friday</virtues-ins>.</p>",
                "Lunch {--at noon--}{++on Friday++}.\n",
            ),
            (
                "<p><virtues-ins proposal=\"p2\"><strong>bold</strong> and <a href=\"/page/page_abc\"><em>linked</em></a></virtues-ins></p>",
                "{++**bold** and [*linked*](/page/page_abc)++}\n",
            ),
            // A mention or a break between two runs of one proposal is in it.
            (
                "<p><virtues-ins proposal=\"p3\">with </virtues-ins><virtues-mention to=\"/person/person_1\" label=\"Nick\"></virtues-mention><virtues-ins proposal=\"p3\"> today</virtues-ins></p>",
                "{++with [@Nick](/person/person_1) today++}\n",
            ),
            (
                "<p><virtues-del proposal=\"p4\">one</virtues-del><br><virtues-del proposal=\"p4\">two</virtues-del></p>",
                "{--one\\\ntwo--}\n",
            ),
            // Text that would end the proposal, or open another, is escaped.
            (
                "<p><virtues-del proposal=\"p5\">a --} b ++} c {-- d {&gt;&gt; e</virtues-del></p>",
                "{--a --\\} b ++\\} c \\{-- d \\{>> e--}\n",
            ),
        ] {
            let nodes = tree(src);
            assert_eq!(markdown(&c, &nodes), md, "{src}");
        }
    }

    #[test]
    fn text_that_reads_like_a_proposal_stays_text() {
        let c = Contract::load();
        for src in [
            "<p>Write {--old--} and {++new++} here.</p>",
            "<p>{--</p>",
            "<h2>{++ heading</h2>",
            "<table><tr><th>A</th></tr><tr><td><p>{--cell--}</p></td></tr></table>",
        ] {
            let nodes = tree(src);
            let md = markdown(&c, &nodes);
            assert!(!md.contains("{--") || md.contains("\\{--"), "{md:?}");
            assert_eq!(
                read_back(&md),
                html(&c, &nodes, &Options { ids: false }),
                "{src}: {md:?}"
            );
            assert!(!read_back(&md).contains("virtues-"), "{md:?}");
        }
        assert_eq!(
            markdown(&c, &tree("<p>Write {--old--} and {++new++}.</p>")),
            "Write \\{--old--} and \\{++new++}.\n"
        );
    }

    #[test]
    fn the_export_with_ids_is_the_export_with_a_comment_per_block() {
        let c = Contract::load();
        let nodes = tree(
            "<h2 data-id=\"k3n1x0aa\">Plan</h2><p data-id=\"p0q9z1mm\">Lunch with \
             <virtues-mention to=\"/person/person_1\" label=\"Nick\"></virtues-mention></p>\
             <ul data-id=\"l1\"><li data-id=\"i1\"><p data-id=\"q1\">a</p></li><li data-id=\"i2\"><p data-id=\"q2\">b</p></li></ul>\
             <blockquote data-id=\"b1\"><p data-id=\"q3\">quoted</p></blockquote>\
             <pre data-id=\"c1\">code\n\nblock</pre><p data-id=\"e1\"></p><p data-id=\"x--y\">odd id</p><hr data-id=\"h1\">",
        );
        let with = markdown_with_ids(&c, &nodes);
        assert!(
            with.starts_with("<!-- k3n1x0aa -->\n## Plan\n\n<!-- p0q9z1mm -->\nLunch with [@Nick](/person/person_1)\n\n<!-- l1 -->\n- a\n- b\n"),
            "{with}"
        );
        assert!(!with.contains("x--y"), "{with}");
        let stripped: String = with
            .split_inclusive('\n')
            .filter(|l| !(l.starts_with("<!-- ") && l.ends_with(" -->\n")))
            .collect();
        assert_eq!(stripped, markdown(&c, &nodes));
    }

    /// Every footnote's note is written once, after the last block, in the
    /// order its label is first cited: a heading's, a cell's and a list's
    /// footnotes as a paragraph's. One block alone carries its own. A
    /// second note under one label takes a label of its own, the next of
    /// its kind, so it is written too.
    #[test]
    fn references_write_their_notes_after_the_body_in_order() {
        let c = Contract::load();
        let f = |label: &str, note: &str| format!("<virtues-footnote label=\"{label}\" note=\"{note}\"></virtues-footnote>");
        let nodes = tree(&format!(
            "<h2 data-id=\"h\">Morning{}</h2><p data-id=\"p\">You read.{} You ate.{}{}</p>\
             <table data-id=\"t\"><tr><th><p>When</p></th></tr><tr><td><p>noon{}</p></td></tr></table>\
             <ul data-id=\"l\"><li><p>again{}</p></li></ul>",
            f("cx-1", "9:05 AM"),
            f("ev-1", "Recording · 9:10 AM · data_communication_transcription:tr_1"),
            f("ev-2", "Message · 9:44 AM"),
            f("cx-1", "a second note for one label"),
            f("ev-3", "Chat · 12:00 PM"),
            f("ev-1", "Recording · 9:10 AM · data_communication_transcription:tr_1"),
        ));
        let notes = "[^cx-1]: 9:05 AM\n[^ev-1]: Recording · 9:10 AM · data_communication_transcription:tr_1\n\
                     [^ev-2]: Message · 9:44 AM\n[^cx-2]: a second note for one label\n[^ev-3]: Chat · 12:00 PM\n";
        let md = markdown(&c, &nodes);
        assert_eq!(
            md,
            format!(
                "## Morning[^cx-1]\n\nYou read.[^ev-1] You ate.[^ev-2][^cx-2]\n\n| When |\n| --- |\n| noon[^ev-3] |\n\n- again[^ev-1]\n\n{notes}"
            )
        );
        let with = markdown_with_ids(&c, &nodes);
        assert!(with.ends_with(&format!("- again[^ev-1]\n\n{notes}")), "{with}");
        let stripped: String = with
            .split_inclusive('\n')
            .filter(|l| !(l.starts_with("<!-- ") && l.ends_with(" -->\n")))
            .collect();
        assert_eq!(stripped, md);
        assert_eq!(markdown(&c, &nodes[2..3]), "| When |\n| --- |\n| noon[^ev-3] |\n\n[^ev-3]: Chat · 12:00 PM\n");
        assert_eq!(markdown(&c, &nodes[0..1]), "## Morning[^cx-1]\n\n[^cx-1]: 9:05 AM\n");
    }

    #[test]
    fn a_reference_without_a_note_writes_no_line() {
        let c = Contract::load();
        let nodes = tree(
            "<p>Bare.<virtues-footnote label=\"ev-9\"></virtues-footnote> Noted.\
             <virtues-footnote label=\"ev-1\" note=\"Message · 1:00 PM\"></virtues-footnote></p>",
        );
        assert_eq!(markdown(&c, &nodes), "Bare.[^ev-9] Noted.[^ev-1]\n\n[^ev-1]: Message · 1:00 PM\n");
        let bare = tree("<p>Bare.<virtues-footnote label=\"ev-9\"></virtues-footnote></p>");
        assert_eq!(markdown(&c, &bare), "Bare.[^ev-9]\n");
        // A label first cited without a note takes the first note a later
        // footnote with it carries.
        let later = tree(
            "<p>a<virtues-footnote label=\"x\"></virtues-footnote> b<virtues-footnote label=\"x\" note=\"n\"></virtues-footnote></p>",
        );
        assert_eq!(markdown(&c, &later), "a[^x] b[^x]\n\n[^x]: n\n");
    }

    /// Two sentences cite one label with different sources (one pasted from
    /// another day, numbered there). Markdown holds one definition a label:
    /// written as it stood, the export gave both sentences the first
    /// source, and a writer's markdown read off it wrote that source over
    /// the second's. The second source takes the next label of its kind.
    #[test]
    fn two_notes_under_one_label_each_keep_their_source() {
        let c = Contract::load();
        let f = |label: &str, note: &str| format!("<virtues-footnote label=\"{label}\" note=\"{note}\"></virtues-footnote>");
        let nodes = tree(&format!(
            "<p>You ate with Nick.{}</p><p>You swam at noon.{}{}</p><p>Later.{}</p><p>Again.{}</p>",
            f("ev-2", "Message · 12:10 PM · data_communication_message:m_1"),
            f("ev-2", "Location · 9:30 AM"),
            f("ev-6", "Chat · 1:00 PM"),
            f("ev-2", "Location · 9:30 AM"),
            f("ev-2", "Message · 12:10 PM · data_communication_message:m_1"),
        ));
        let md = markdown(&c, &nodes);
        assert_eq!(
            md,
            "You ate with Nick.[^ev-2]\n\nYou swam at noon.[^ev-7][^ev-6]\n\nLater.[^ev-7]\n\nAgain.[^ev-2]\n\n\
             [^ev-2]: Message · 12:10 PM · data_communication_message:m_1\n[^ev-7]: Location · 9:30 AM\n\
             [^ev-6]: Chat · 1:00 PM\n"
        );
        let back = crate::migrate::from_markdown_as(&c, &md, crate::migrate::Dialect::Article);
        let notes: Vec<(String, String)> = back
            .nodes
            .iter()
            .flat_map(|b| b.content.iter())
            .filter(|n| n.kind == "footnote")
            .map(|n| {
                let a = |k: &str| n.attrs.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
                (a("label"), a("note"))
            })
            .collect();
        assert_eq!(notes[1], ("ev-7".to_string(), "Location · 9:30 AM".to_string()));
        assert_eq!(notes[3].1, "Location · 9:30 AM");
        assert_eq!(markdown(&c, &back.nodes), md, "the export reads back as itself");
        // A label ending in no number takes `-2`, the first one free.
        let other = tree(&format!("<p>a{}</p><p>b{}</p><p>c{}</p>", f("x", "one"), f("x-2", "taken"), f("x", "two")));
        assert_eq!(markdown(&c, &other), "a[^x]\n\nb[^x-2]\n\nc[^x-3]\n\n[^x]: one\n[^x-2]: taken\n[^x-3]: two\n");
        // A footnote carrying no note keeps its label, and reads back with
        // its label's.
        let bare = tree(&format!("<p>a{}</p><p>b<virtues-footnote label=\"ev-1\"></virtues-footnote></p>", f("ev-1", "one")));
        assert_eq!(markdown(&c, &bare), "a[^ev-1]\n\nb[^ev-1]\n\n[^ev-1]: one\n");
    }

    /// Two lists of one kind side by side are one list to markdown: written
    /// with one marker, they read back as one, so a writer's markdown read
    /// off the export merged them. The second takes its kind's other marker.
    #[test]
    fn lists_of_one_kind_side_by_side_stay_apart() {
        let c = Contract::load();
        for (src, md) in [
            ("<ul><li><p>a</p></li></ul><ul><li><p>b</p></li></ul>", "- a\n\n* b\n"),
            ("<ol><li><p>a</p></li></ol><ol start=\"4\"><li><p>b</p></li></ol>", "1. a\n\n4) b\n"),
            (
                "<ul data-type=\"taskList\"><li data-type=\"taskItem\" data-checked=\"true\"><p>a</p></li></ul>\
                 <ul data-type=\"taskList\"><li data-type=\"taskItem\"><p>b</p></li></ul>",
                "- [x] a\n\n* [ ] b\n",
            ),
            (
                "<ul><li><p>a</p></li></ul><ul><li><p>b</p></li></ul><ul><li><p>c</p></li></ul>",
                "- a\n\n* b\n\n- c\n",
            ),
            ("<ul><li><p>a</p></li></ul><p>x</p><ul><li><p>b</p></li></ul>", "- a\n\nx\n\n- b\n"),
            ("<ul><li><p>a</p></li></ul><ol><li><p>b</p></li></ol>", "- a\n\n1. b\n"),
            (
                "<blockquote><ul><li><p>a</p></li></ul><ul><li><p>b</p></li></ul></blockquote>",
                "> - a\n>\n> * b\n",
            ),
            (
                "<ul><li><p>a</p><ul><li><p>b</p></li></ul><ul><li><p>c</p></li></ul></li></ul>",
                "- a\n\n  - b\n\n  * c\n",
            ),
        ] {
            let nodes = tree(src);
            assert_eq!(markdown(&c, &nodes), md, "{src}");
            let back = crate::migrate::from_markdown_as(&c, md, crate::migrate::Dialect::Article);
            assert_eq!(html(&c, &back.nodes, &Options { ids: false }), html(&c, &nodes, &Options { ids: false }), "{md}");
        }
    }

    /// The veil is the innermost delimiter, so inside a link and around bold
    /// it reads back as the same marks. Its brackets keep whitespace inside;
    /// a bracket of its own text is a character reference, and one in text
    /// no veil covers stays bare, as it was written.
    #[test]
    fn veil_is_innermost_and_brackets_stay_bare() {
        let c = Contract::load();
        for (src, md) in [
            ("<p>With <a href=\"/person/p_1\"><virtues-veil>Nick</virtues-veil></a>.</p>", "With [⟦Nick⟧](/person/p_1).\n"),
            ("<p>At <strong><virtues-veil>the clinic</virtues-veil></strong>.</p>", "At **⟦the clinic⟧**.\n"),
            ("<p><em>said <virtues-veil>his name</virtues-veil></em> once</p>", "*said ⟦his name⟧* once\n"),
            ("<p>a<virtues-veil> b </virtues-veil>c</p>", "a⟦ b ⟧c\n"),
            ("<p>A <virtues-veil>x ⟦y⟧</virtues-veil> z</p>", "A ⟦x &#10214;y&#10215;⟧ z\n"),
            ("<p>Typed ⟦as is⟧ here.</p>", "Typed ⟦as is⟧ here.\n"),
            ("<p><mark><virtues-veil>marked</virtues-veil></mark></p>", "==⟦marked⟧==\n"),
        ] {
            let nodes = tree(src);
            assert_eq!(markdown(&c, &nodes), md, "{src}");
            let back = crate::migrate::from_markdown_as(&c, md, crate::migrate::Dialect::Article);
            if !src.contains("Typed") {
                assert_eq!(html(&c, &back.nodes, &Options { ids: false }), src, "{md}");
            }
        }
        // After a footnote, `(` would make a link of its marker, and `:` at
        // a line's start a definition.
        let nodes = tree(
            "<p><virtues-footnote label=\"a\" note=\"n\"></virtues-footnote>: begins (x)<virtues-footnote label=\"a\" note=\"n\"></virtues-footnote>(y)</p>",
        );
        let md = markdown(&c, &nodes);
        assert_eq!(md, "[^a]\\: begins (x)[^a]\\(y)\n\n[^a]: n\n");
        let back = crate::migrate::from_markdown_as(&c, &md, crate::migrate::Dialect::Article);
        assert_eq!(html(&c, &back.nodes, &Options { ids: false }), html(&c, &nodes, &Options { ids: false }));
    }

    /// A figure is a fence of three backticks, its values one per line in
    /// the contract's order; a code block that would read as one takes four.
    #[test]
    fn a_figure_exports_as_its_fence() {
        let c = Contract::load();
        let nodes = tree(
            "<virtues-figure kind=\"picture\" time=\"10:40 AM\" src=\"/api/drive/files/f_1/download\" \
             caption=\"Rain\non the glass\" data-style=\"watercolor\" who=\"\"></virtues-figure>\
             <pre data-language=\"figure\">kind: route</pre><pre data-language=\"figure\">not a figure</pre>",
        );
        assert_eq!(
            markdown(&c, &nodes),
            "```figure\nkind: picture\nwho: \nsrc: /api/drive/files/f_1/download\ncaption: Rain on the glass\n\
             style: watercolor\ntime: 10:40 AM\n```\n\n````figure\nkind: route\n````\n\n```figure\nnot a figure\n```\n"
        );
    }

    #[test]
    fn an_unknown_mark_renders_as_its_text() {
        let c = Contract::load();
        let mut nodes = tree("<p>x</p>");
        nodes[0].content[0].marks.push(Mark {
            kind: "sparkle".into(),
            attrs: Default::default(),
        });
        assert_eq!(html(&c, &nodes, &Options { ids: false }), "<p>x</p>");
    }
}

//! Document tree → canonical HTML (what the model reads, with block ids) and
//! → markdown (what search, embeddings and the `content` column read).
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
use crate::model::{Mark, Node};
use serde_json::Value;
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
    let mut w = Md::default();
    for (i, n) in nodes.iter().enumerate() {
        if i > 0 {
            w.text("\n\n");
        }
        block_md(c, n, false, &mut w);
    }
    w.text("\n");
    w.into_string()
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
    for (i, ch) in n.content.iter().enumerate() {
        if i > 0 {
            w.text(sep);
        }
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
            let loose = n.content.iter().any(|item| item.content.len() > 1);
            let start = a("start").as_u64().unwrap_or(1);
            for (i, item) in n.content.iter().enumerate() {
                if i > 0 {
                    w.text(if loose { "\n\n" } else { "\n" });
                }
                let marker = match n.kind.as_str() {
                    "orderedList" => format!("{}. ", start + i as u64),
                    "taskList" => {
                        let checked = item
                            .attrs
                            .get("checked")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        format!("- [{}] ", if checked { "x" } else { " " })
                    }
                    _ => "- ".to_string(),
                };
                let indent = " ".repeat(if n.kind == "taskList" {
                    2
                } else {
                    marker.len()
                });
                w.open(&marker, &indent);
                let mut first_empty = false;
                for (k, b) in item.content.iter().enumerate() {
                    if k > 0 {
                        // After an empty first block, a blank line would end
                        // the item: markdown lets an item open with at most
                        // one blank line, the marker's own.
                        let after_empty = k == 1 && first_empty;
                        w.text(if loose && !after_empty { "\n\n" } else { "\n" });
                    }
                    let before = w.writes;
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
            let fence = mark.to_string().repeat((longest + 1).max(3));
            format!("{fence}{lang}\n{text}\n{fence}")
        }
        "horizontalRule" => "---".to_string(),
        "image" => {
            let alt = a("alt").as_str().unwrap_or("").replace(['\r', '\n'], " ");
            let mut alt = escape_md(&alt, Lines::Cell, None);
            // The converter reads a last `|` and digits as the width, so a
            // pipe of the alt's own is written as a character reference.
            match a("width").as_i64() {
                Some(w) => alt.push_str(&format!("|{w}")),
                None => alt = alt.replace('|', "&#124;"),
            }
            format!(
                "![{alt}]({})",
                destination(a("src").as_str().unwrap_or(""))
            )
        }
        "table" => table_md(c, n),
        "applet" => {
            let mut s = String::new();
            node_html(c, n, &Options { ids: false }, &mut s);
            s
        }
        other => format!("<!-- {other} -->"),
    }
}

/// GFM aligns a column, not a cell: a column takes the first alignment any
/// of its cells carries.
fn table_md(c: &Contract, n: &Node) -> String {
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
/// `=` after another is escaped too (`prev` is what the output already ends
/// with); `&` before a letter, digit or `#` would read as an entity.
///
/// A line break inside text is written as a space, which is what markdown
/// reads one inside a paragraph as: written as a break, a blank line would
/// end the block and a line starting with `#` would start a heading.
fn escape_md(s: &str, lines: Lines, prev: Option<char>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev = prev;
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        let ch = if matches!(ch, '\n' | '\r') { ' ' } else { ch };
        let escape = match ch {
            '\\' | '*' | '_' | '`' | '[' | ']' | '<' | '~' => true,
            '|' => lines != Lines::Cell,
            '=' => prev == Some('='),
            '&' => chars
                .peek()
                .is_some_and(|n| n.is_ascii_alphanumeric() || *n == '#'),
            _ => false,
        };
        if escape {
            out.push('\\');
        }
        out.push(ch);
        prev = Some(ch);
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
    let url: String = url.chars().filter(|c| !matches!(c, '\t' | '\n' | '\r')).collect();
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
        _ => ("", ""),
    }
}

fn link_of(n: &Node) -> Option<&Mark> {
    n.marks.iter().find(|m| m.kind == "link")
}

/// Inline content → markdown. Consecutive nodes under one link become one
/// link; inside it, delimiter marks stay open across nodes that share them,
/// and whitespace at a delimiter's edge moves outside it, as CommonMark's
/// flanking rules need.
fn inline_md(nodes: &[Node], lines: Lines) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < nodes.len() {
        let link = link_of(&nodes[i]).cloned();
        let mut j = i + 1;
        while j < nodes.len() && link_of(&nodes[j]).cloned() == link {
            j += 1;
        }
        let last = j == nodes.len();
        match link {
            Some(l) => {
                // Inside the brackets nothing starts a line.
                let body = delimited_md(&nodes[i..j], lines, false, last);
                let href = l.attrs.get("href").and_then(Value::as_str).unwrap_or("");
                escape_bang(&mut out);
                out.push_str(&format!("[{body}]({})", destination(href)));
            }
            None => {
                let at_line_start = out.is_empty() || out.ends_with('\n');
                out.push_str(&delimited_md(&nodes[i..j], lines, at_line_start, last));
            }
        }
        i = j;
    }
    out
}

/// `line_start`: the output begins a line of the block (its first, when the
/// block's output is still empty). `ends_block`: nothing in the block follows
/// these nodes.
fn delimited_md(nodes: &[Node], lines: Lines, line_start: bool, ends_block: bool) -> String {
    // Breaks after the last node that is not one end the block with it.
    let last_not_break = nodes.iter().rposition(|n| n.kind != "hardBreak");
    let mut out = String::new();
    let mut active: Vec<&str> = vec![];
    let block_start = line_start;
    let mut line_start = line_start;
    let close_to = |out: &mut String, active: &mut Vec<&str>, keep: usize| {
        let trailing = out.len() - out.trim_end_matches(' ').len();
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
                let lead_len = t.len() - t.trim_start_matches(' ').len();
                let opens = keep < wanted.len();
                if opens {
                    out.push_str(&t[..lead_len]);
                }
                for k in &wanted[keep..] {
                    out.push_str(delim(k).0);
                    active.push(k);
                }
                let rest = if opens { &t[lead_len..] } else { t };
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
                    let escaped = escape_md(rest, lines, out.chars().last());
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
                    escape_md(label, lines, None),
                    destination(to)
                ));
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

//! What a writer has been shown of a page, block by block, and what one read
//! shows: the base a model's next edit is checked against ([`crate::apply_ops`]).
//!
//! A read is a [`Shown`], the parts it put in front of the writer. Its text
//! ([`Shown::text`]) and what it teaches the base ([`Seen::learn`]) are both
//! read off that one value, so a base never holds what the text did not
//! show. Knowledge is per block ([`Known`]): a block's own attributes, text
//! and inline content, with a stub where each block inside it goes, and
//! which of those blocks no read has shown inside it. A list read in part is
//! known as a list with its first items shown, and every block inside it is
//! judged on its own when an edit would take or change it, so no block
//! enters the base because a block around it did, and none taken with the
//! list counts as shown there because a read showed it elsewhere.

use crate::model::Node;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// The type of the stub that stands for a block inside a shell.
const STUB: &str = "#ref";

/// What [`Seen::subtree`] puts where a shell names a block no read showed.
pub const UNSEEN: &str = "#unseen";

/// The name of a tree: 16 lowercase hex of the SHA-256 of its JSON. The
/// JSON is canonical (`serde_json` keeps attributes sorted), so the same
/// tree always hashes the same, and a deleted block changes it where a Yjs
/// state vector would not move.
pub fn tree_hash(nodes: &[Node]) -> String {
    let json = serde_json::to_vec(nodes).expect("a page tree serializes");
    hex::encode(&Sha256::digest(&json)[..8])
}

/// [`tree_hash`] of one block, as a block listed as too long for any read
/// is kept ([`Seen::learn`]).
pub fn node_hash(node: &Node) -> String {
    tree_hash(std::slice::from_ref(node))
}

fn stub(id: &str) -> Node {
    let mut attrs = Map::new();
    attrs.insert("of".into(), Value::String(id.into()));
    Node::element(STUB, attrs, vec![])
}

/// The block a stub stands for.
fn stub_of(n: &Node) -> Option<&str> {
    (n.kind == STUB).then(|| n.attrs.get("of").and_then(Value::as_str)).flatten()
}

/// `node` with each outermost block inside it a stub naming it. Through a
/// table's cells, which carry no id: a row's shell keeps its cells and their
/// attributes, each around the stubs of the blocks in it.
pub fn shell(node: &Node) -> Node {
    Node {
        kind: node.kind.clone(),
        attrs: node.attrs.clone(),
        text: node.text.clone(),
        marks: node.marks.clone(),
        content: node
            .content
            .iter()
            .map(|c| match c.id() {
                Some(id) => stub(id),
                None => shell(c),
            })
            .collect(),
    }
}

/// `node`'s own part: its attributes, text, inline content and a row's
/// cells, without the blocks inside it (or their stubs, in a shell). Two
/// versions of a block whose own parts are equal differ only in the blocks
/// inside them, each of which is judged on its own.
pub fn own(node: &Node) -> Node {
    Node {
        kind: node.kind.clone(),
        attrs: node.attrs.clone(),
        text: node.text.clone(),
        marks: node.marks.clone(),
        content: node
            .content
            .iter()
            .filter(|c| c.id().is_none() && stub_of(c).is_none())
            .map(own)
            .collect(),
    }
}

/// The blocks a shell names, in document order, through a row's cells.
pub fn stubs(shell: &Node) -> Vec<&str> {
    fn walk<'a>(n: &'a Node, out: &mut Vec<&'a str>) {
        for c in &n.content {
            match stub_of(c) {
                Some(id) => out.push(id),
                None => walk(c, out),
            }
        }
    }
    let mut out = vec![];
    walk(shell, &mut out);
    out
}

/// How a block was shown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Form {
    /// As canonical HTML, with ids.
    #[default]
    Html,
    /// As the markdown export, which does not say what a block's HTML is (a
    /// mention reads as a link, a callout as a quote).
    Markdown,
}

fn is_html(form: &Form) -> bool {
    *form == Form::Html
}

/// One block as it was last shown: its [`shell`], how, and the blocks its
/// shell names that no read has shown inside it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Known {
    pub shell: Node,
    #[serde(default, skip_serializing_if = "is_html")]
    pub form: Form,
    /// Blocks the shell names that no read showed in it: an opened read
    /// counts the blocks past its window (`<!-- 74 more -->`) without
    /// showing them. One shown elsewhere is no block shown here, so a batch
    /// taking it with this block is judged as taking a block unread; so is
    /// one a later read showed standing anywhere else, inside another block
    /// or at the page's top level.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub unshown: BTreeSet<String>,
}

impl Known {
    fn new(shell: Node, form: Form) -> Self {
        Known {
            shell,
            form,
            unshown: BTreeSet::new(),
        }
    }

    /// Whether a read showed the block `id` inside this one: its shell
    /// names it, and some read showed it there.
    pub fn shows(&self, id: &str) -> bool {
        stubs(&self.shell).contains(&id) && !self.unshown.contains(id)
    }
}

/// What reads have shown a writer of one page: each block shown, as it was
/// last shown, and each block listed as too long for any read, by the hash
/// it had when listed. A block can be known while the block around it is
/// not, and the other way round. Kept as JSON; a base in any other shape
/// (a tree, as bases were once kept) does not read back as one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seen {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    known: BTreeMap<String, Known>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    opaque: BTreeMap<String, String>,
}

/// One piece of what a read put in front of the writer.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    /// A block shown whole, as it stands, with every block inside it.
    Whole(Node),
    /// A block shown by its own tags around some of the blocks inside it:
    /// `shell` is its [`shell`] as it stands, naming every block inside it,
    /// and `inside` what was shown of them.
    Opened { shell: Node, inside: Vec<Part> },
    /// A table cell of an opened row: its tags around what was shown of the
    /// blocks in it.
    Cell { cell: Node, inside: Vec<Part> },
    /// A block named by a comment, with what to do with it. `opaque` holds
    /// its [`node_hash`] when no read can show it, so it can be taken whole
    /// by an op naming it while it stays as listed.
    Listed { id: String, say: String, opaque: Option<String> },
    /// Blocks passed over between what was shown, `count` of them; `after`,
    /// when given, is where a read on starts.
    More { count: usize, after: Option<String> },
    /// A block asked for that is no longer on the page.
    Gone(String),
}

/// What one read, edit result or refusal showed.
#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    pub form: Form,
    pub parts: Vec<Part>,
    /// The block its top-level parts were shown inside, when the reply says
    /// so: a read on among the blocks inside one (its `inside`).
    pub within: Option<String>,
}

impl Shown {
    pub fn html(parts: Vec<Part>) -> Self {
        Shown {
            form: Form::Html,
            parts,
            within: None,
        }
    }

    pub fn markdown(parts: Vec<Part>) -> Self {
        Shown {
            form: Form::Markdown,
            parts,
            within: None,
        }
    }

    /// The same parts, shown inside the block `id`.
    pub fn inside(mut self, id: Option<&str>) -> Self {
        self.within = id.map(str::to_string);
        self
    }

    /// The read's text. HTML: each part as canonical HTML with ids, a
    /// listed block as `<!-- {id}: {say} -->`. Markdown, top-level parts
    /// only: the export with ids of each run of whole blocks, a listed block
    /// as `<!-- {id} -->` over `<!-- {say} -->`, joined by a blank line.
    pub fn text(&self) -> String {
        match self.form {
            Form::Html => {
                let mut out = String::new();
                for part in &self.parts {
                    part_html(part, &mut out);
                }
                out
            }
            Form::Markdown => {
                let mut pieces: Vec<String> = vec![];
                let mut run: Vec<Node> = vec![];
                let flush = |run: &mut Vec<Node>, pieces: &mut Vec<String>| {
                    if !run.is_empty() {
                        let text = crate::to_markdown_with_ids(run);
                        pieces.push(text.trim_end_matches('\n').to_string());
                        run.clear();
                    }
                };
                for part in &self.parts {
                    match part {
                        Part::Whole(n) => run.push(n.clone()),
                        Part::Listed { id, say, .. } => {
                            flush(&mut run, &mut pieces);
                            pieces.push(format!("<!-- {id} -->\n<!-- {say} -->"));
                        }
                        // A markdown read shows whole top-level blocks.
                        Part::Opened { .. } | Part::Cell { .. } | Part::More { .. } | Part::Gone(_) => {
                            flush(&mut run, &mut pieces)
                        }
                    }
                }
                flush(&mut run, &mut pieces);
                let mut text = pieces.join("\n\n");
                text.push('\n');
                text
            }
        }
    }

    /// Every block id the read showed whole or opened, outermost first.
    pub fn shown_ids(&self) -> Vec<String> {
        let mut out = vec![];
        for part in &self.parts {
            part_ids(part, false, &mut out);
        }
        out
    }
}

/// The blocks `part` shows whole or opened, and those it lists by a comment
/// when `listed`.
fn part_ids(part: &Part, listed: bool, out: &mut Vec<String>) {
    match part {
        Part::Whole(n) => n.walk(&mut |b| out.extend(b.id().map(str::to_string))),
        Part::Opened { shell, inside } => {
            out.extend(shell.id().map(str::to_string));
            for p in inside {
                part_ids(p, listed, out);
            }
        }
        Part::Cell { inside, .. } => {
            for p in inside {
                part_ids(p, listed, out);
            }
        }
        Part::Listed { id, .. } if listed => out.push(id.clone()),
        Part::Listed { .. } | Part::More { .. } | Part::Gone(_) => {}
    }
}

/// `node`'s opening and closing tags, as canonical HTML writes them around
/// what it holds: its own, and a table's `<tbody>` inside it.
pub fn tags(node: &Node) -> (String, String) {
    let mut empty = own(node);
    empty.content.clear();
    let html = crate::to_html(std::slice::from_ref(&empty), true);
    match html.find("</") {
        Some(at) => (html[..at].to_string(), html[at..].to_string()),
        None => (html, String::new()),
    }
}

fn part_html(part: &Part, out: &mut String) {
    match part {
        Part::Whole(n) => out.push_str(&crate::to_html(std::slice::from_ref(n), true)),
        Part::Opened { shell, inside } => {
            let (open, close) = tags(shell);
            out.push_str(&open);
            for p in inside {
                part_html(p, out);
            }
            out.push_str(&close);
        }
        Part::Cell { cell, inside } => {
            let (open, close) = tags(cell);
            out.push_str(&open);
            for p in inside {
                part_html(p, out);
            }
            out.push_str(&close);
        }
        Part::Listed { id, say, .. } => out.push_str(&format!("<!-- {id}: {say} -->")),
        Part::More { count, after } => match after {
            Some(after) => out.push_str(&format!("<!-- {count} more: read on after \"{after}\" -->")),
            None => out.push_str(&format!("<!-- {count} more -->")),
        },
        Part::Gone(_) => {}
    }
}

impl Seen {
    /// A read of the whole page as HTML: every block known as it stands.
    pub fn of_page(tree: &[Node]) -> Self {
        let mut seen = Seen::default();
        for n in tree {
            n.walk(&mut |b| {
                if let Some(id) = b.id() {
                    seen.known.insert(id.to_string(), Known::new(shell(b), Form::Html));
                }
            });
        }
        seen
    }

    /// Take in what `shown` showed of the page `tree`, as it stood when the
    /// text was written. The blocks in `keep` stay as they were known:
    /// blocks an op of a refused batch writes over, which the retry sends
    /// again as written, so it merges with a change made since as the first
    /// try would have.
    pub fn learn(&mut self, shown: &Shown, tree: &[Node], keep: &HashSet<String>) {
        for part in &shown.parts {
            self.learn_part(part, shown.form, keep, shown.within.as_deref());
        }
        self.shown_elsewhere(shown, tree);
    }

    /// A block a read showed or listed stands where it is on the page
    /// `tree` (inside the nearest block around it, or at the page's top
    /// level), and is no block shown in any other: each other block whose
    /// shell names it, from a read before it moved, no longer counts it as
    /// shown there. Where it stands is read off the page, since a read on
    /// at the top level and an `ids` read showing a block alone put it
    /// inside no block in their text. An opened read that counted an item
    /// no read had shown yet, then a read of the item where a person had
    /// moved it (another list, the page's top level), left the first list
    /// holding the item as shown inside it, and a batch took it with that
    /// list once the person moved it back.
    fn shown_elsewhere(&mut self, shown: &Shown, tree: &[Node]) {
        let mut named = vec![];
        for part in &shown.parts {
            part_ids(part, true, &mut named);
        }
        if named.is_empty() {
            return;
        }
        let around = parents(tree);
        let placed: HashMap<&str, Option<&str>> = named
            .iter()
            .filter_map(|id| around.get_key_value(id.as_str()).map(|(id, p)| (*id, *p)))
            .collect();
        for (id, k) in self.known.iter_mut() {
            let elsewhere: Vec<String> = stubs(&k.shell)
                .into_iter()
                .filter(|c| placed.get(*c).is_some_and(|p| *p != Some(id.as_str())))
                .map(str::to_string)
                .collect();
            k.unshown.extend(elsewhere);
        }
    }

    /// Take in `part`, shown inside the block `holder` when the text showed
    /// it there.
    fn learn_part(&mut self, part: &Part, form: Form, keep: &HashSet<String>, holder: Option<&str>) {
        match part {
            Part::Whole(n) => {
                n.walk(&mut |b| {
                    if b.id().is_some() {
                        self.know(shell(b), form, keep, BTreeSet::new());
                    }
                });
                self.shown_in(holder, n.id());
            }
            Part::Opened { shell, inside } => {
                if let Some(id) = shell.id() {
                    // Its tags were shown, as HTML, around some of the
                    // blocks inside it. Of the rest, which it named without
                    // showing, one a read showed elsewhere was not shown
                    // here, unless a read showed it in this block before;
                    // one no read showed is unread, and judged so.
                    let mut shown_here = vec![];
                    shown_inside(inside, &mut shown_here);
                    let shown_before: Vec<&str> = self
                        .known
                        .get(id)
                        .map(|k| stubs(&k.shell).into_iter().filter(|c| !k.unshown.contains(*c)).collect())
                        .unwrap_or_default();
                    let unshown = stubs(shell)
                        .into_iter()
                        .filter(|c| self.known.contains_key(*c) && !shown_here.contains(c) && !shown_before.contains(c))
                        .map(str::to_string)
                        .collect();
                    self.know(shell.clone(), Form::Html, keep, unshown);
                }
                self.shown_in(holder, shell.id());
                for p in inside {
                    self.learn_part(p, form, keep, shell.id());
                }
            }
            Part::Cell { inside, .. } => {
                for p in inside {
                    self.learn_part(p, form, keep, holder);
                }
            }
            // A comment names the block where it is, and shows nothing of it.
            Part::Listed { id, opaque, .. } => {
                if let Some(hash) = opaque.as_ref().filter(|_| !keep.contains(id)) {
                    self.opaque.insert(id.clone(), hash.clone());
                }
            }
            Part::More { .. } => {}
            Part::Gone(id) => self.forget(id),
        }
    }

    /// A read showed `child` inside `holder`.
    fn shown_in(&mut self, holder: Option<&str>, child: Option<&str>) {
        if let (Some(holder), Some(child)) = (holder, child) {
            if let Some(k) = self.known.get_mut(holder) {
                k.unshown.remove(child);
            }
        }
    }

    /// Know a block as `shell` shows it, with the blocks it names that no
    /// read showed in it. Shown as markdown, a block already known as HTML
    /// whose own part is unchanged stays known as HTML: its HTML has been
    /// shown.
    fn know(&mut self, shell: Node, form: Form, keep: &HashSet<String>, unshown: BTreeSet<String>) {
        let Some(id) = shell.id().map(str::to_string) else {
            return;
        };
        if keep.contains(&id) {
            return;
        }
        let form = match self.known.get(&id) {
            Some(k) if form == Form::Markdown && k.form == Form::Html && own(&k.shell) == own(&shell) => Form::Html,
            _ => form,
        };
        self.opaque.remove(&id);
        self.known.insert(id, Known { shell, form, unshown });
    }

    /// Forget a block: it is gone, and the writer was told.
    pub fn forget(&mut self, id: &str) {
        self.known.remove(id);
        self.opaque.remove(id);
    }

    /// After an applied edit, from the page just `before` it to the page
    /// `after` it: a block the edit wrote is known as written, since the
    /// writer knows what it wrote, unless it was merged with a change made
    /// since (`merged`), which a read or the result shows; a block whose
    /// inside alone changed (an item added or deleted) keeps its own part as
    /// known, and its stubs move with the edit when they were the page's;
    /// a block the edit took is forgotten.
    pub fn advance(&mut self, before: &[Node], after: &[Node], merged: &HashSet<String>) {
        let then = index(before);
        let now = index(after);
        for (id, a) in &now {
            match then.get(id) {
                None => self.know_written(a),
                Some(b) if own(b) != own(a) => {
                    if !merged.contains(*id) {
                        self.know_written(a);
                    }
                }
                Some(b) => {
                    let (was, is) = (shell(b), shell(a));
                    if let Some(k) = self.known.get_mut(*id) {
                        if k.shell == was {
                            k.shell = is;
                            let names = stubs(&k.shell).into_iter().map(str::to_string).collect::<BTreeSet<_>>();
                            k.unshown.retain(|c| names.contains(c));
                        } else {
                            // Changed since it was read, by someone else:
                            // its stubs stay as read, but what this edit put
                            // in it is the writer's own, and is in it.
                            let before = stubs(&was);
                            for (at, c) in stubs(&is).into_iter().enumerate() {
                                if !before.contains(&c) {
                                    add_stub(&mut k.shell, c, stubs(&is)[..at].last().copied());
                                }
                            }
                        }
                    }
                }
            }
        }
        for id in then.keys().filter(|id| !now.contains_key(*id)) {
            self.forget(id);
        }
    }

    fn know_written(&mut self, node: &Node) {
        if let Some(id) = node.id() {
            self.opaque.remove(id);
            self.known.insert(id.to_string(), Known::new(shell(node), Form::Html));
        }
    }

    /// Whether a read has shown the block `id`.
    pub fn knows(&self, id: &str) -> bool {
        self.known.contains_key(id)
    }

    /// The block `id` as it was last shown.
    pub fn known(&self, id: &str) -> Option<&Known> {
        self.known.get(id)
    }

    /// The hash the block `id` had when it was listed as too long for any
    /// read.
    pub fn opaque_hash(&self, id: &str) -> Option<&str> {
        self.opaque.get(id).map(String::as_str)
    }

    /// Whether a read has shown `node`'s own part as it stands now.
    pub fn seen_as_it_stands(&self, node: &Node) -> bool {
        node.id()
            .and_then(|id| self.known.get(id))
            .is_some_and(|k| own(&k.shell) == own(node))
    }

    /// The block `id` as reads showed it, with every block inside it a read
    /// showed in place, and [`UNSEEN`] (attribute `of`) where a read showed
    /// none. `None` when no read showed `id`.
    pub fn subtree(&self, id: &str) -> Option<Node> {
        let mut path = vec![];
        self.rebuild(id, &mut path)
    }

    fn rebuild(&self, id: &str, path: &mut Vec<String>) -> Option<Node> {
        // A block inside itself: shells from reads at different times can
        // name each other once a block has moved between them.
        if path.iter().any(|p| p == id) || path.len() > crate::MAX_DEPTH {
            return None;
        }
        let known = self.known.get(id)?;
        path.push(id.to_string());
        let node = self.fill(&known.shell, &known.unshown, path);
        path.pop();
        Some(node)
    }

    /// `shell` with each block it names as reads showed it there: one no
    /// read showed in it is [`UNSEEN`], whatever a read showed of it
    /// elsewhere.
    fn fill(&self, shell: &Node, unshown: &BTreeSet<String>, path: &mut Vec<String>) -> Node {
        Node {
            kind: shell.kind.clone(),
            attrs: shell.attrs.clone(),
            text: shell.text.clone(),
            marks: shell.marks.clone(),
            content: shell
                .content
                .iter()
                .map(|c| match stub_of(c) {
                    Some(inner) => {
                        let shown = (!unshown.contains(inner)).then(|| self.rebuild(inner, path)).flatten();
                        shown.unwrap_or_else(|| {
                            let mut attrs = Map::new();
                            attrs.insert("of".into(), Value::String(inner.into()));
                            Node::element(UNSEEN, attrs, vec![])
                        })
                    }
                    None => self.fill(c, unshown, path),
                })
                .collect(),
        }
    }

    /// Whether a read has shown `node` and every block inside it, each in
    /// the block it is in.
    pub fn complete_over(&self, node: &Node) -> bool {
        let mut complete = true;
        node.walk(&mut |b| {
            if let Some(id) = b.id() {
                complete &= self.known.get(id).is_some_and(|k| {
                    let mut inside = vec![];
                    crate::seen::child_ids(b, &mut inside);
                    inside.iter().all(|c| !k.unshown.contains(*c))
                });
            }
        });
        complete
    }

    /// Whether `self` is a read of `tree` whole, as HTML.
    pub fn is_page(&self, tree: &[Node]) -> bool {
        self.opaque.is_empty() && *self == Seen::of_page(tree)
    }
}

/// Each block of `nodes` by its id, with the nearest block around it that
/// has one, through a row's cells (`None`: the page's top level).
fn parents(nodes: &[Node]) -> HashMap<&str, Option<&str>> {
    fn walk<'a>(nodes: &'a [Node], parent: Option<&'a str>, out: &mut HashMap<&'a str, Option<&'a str>>) {
        for n in nodes {
            match n.id() {
                Some(id) => {
                    out.insert(id, parent);
                    walk(&n.content, Some(id), out);
                }
                None => walk(&n.content, parent, out),
            }
        }
    }
    let mut out = HashMap::new();
    walk(nodes, None, &mut out);
    out
}

/// The ids of the parts `inside` shows in the block around them, through a
/// row's cells: a block whole or opened. A comment standing for one shows
/// nothing of it.
fn shown_inside<'a>(inside: &'a [Part], out: &mut Vec<&'a str>) {
    for p in inside {
        match p {
            Part::Whole(n) => out.extend(n.id()),
            Part::Opened { shell, .. } => out.extend(shell.id()),
            Part::Cell { inside, .. } => shown_inside(inside, out),
            Part::Listed { .. } | Part::More { .. } | Part::Gone(_) => {}
        }
    }
}

/// The ids of the outermost blocks inside `node`, through a row's cells.
pub(crate) fn child_ids<'a>(node: &'a Node, out: &mut Vec<&'a str>) {
    for c in &node.content {
        match c.id() {
            Some(id) => out.push(id),
            None => child_ids(c, out),
        }
    }
}

/// Put a stub for `id` in `shell` after the stub for `after` (first when
/// `after` is none, or names no stub there), at the top of the shell: a
/// block whose blocks sit in cells (a row) is left as it is.
fn add_stub(shell: &mut Node, id: &str, after: Option<&str>) {
    if stubs(shell).contains(&id) || shell.content.iter().any(|c| stub_of(c).is_none()) {
        return;
    }
    let at = after
        .and_then(|a| shell.content.iter().position(|c| stub_of(c) == Some(a)))
        .map_or(0, |i| i + 1);
    shell.content.insert(at, stub(id));
}

/// Every block of `nodes` by its id.
pub(crate) fn index(nodes: &[Node]) -> HashMap<&str, &Node> {
    fn walk<'a>(nodes: &'a [Node], out: &mut HashMap<&'a str, &'a Node>) {
        for n in nodes {
            if let Some(id) = n.id() {
                out.insert(id, n);
            }
            walk(&n.content, out);
        }
    }
    let mut out = HashMap::new();
    walk(nodes, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_html;

    fn tree(html: &str) -> Vec<Node> {
        let o = parse_html(html, "doc");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        let mut nodes = o.nodes;
        crate::ensure_ids(&mut nodes);
        nodes
    }

    const PAGE: &str = "<p data-id=\"a\">One <strong>two</strong></p>\
        <ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">Item one</p></li>\
        <li data-id=\"i2\"><p data-id=\"p2\">Item two</p><ul data-id=\"n\"><li data-id=\"i3\"><p data-id=\"p3\">Deep</p></li></ul></li></ul>\
        <table data-id=\"t\"><tr data-id=\"r\"><td colspan=\"2\"><p data-id=\"c1\">Cell</p></td></tr></table>";

    #[test]
    fn a_shell_names_the_blocks_inside_and_own_drops_them() {
        let t = tree(PAGE);
        let list = shell(&t[1]);
        assert_eq!(stubs(&list), ["i1", "i2"]);
        let row = shell(&t[2].content[0]);
        assert_eq!(row.content[0].kind, "tableCell", "a row keeps its cells");
        assert_eq!(stubs(&row), ["c1"]);
        assert_eq!(own(&row).content[0].content, vec![], "and its cells hold no blocks");
        assert_eq!(own(&shell(&t[0])), own(&t[0]), "a paragraph is all its own");
        // A change inside a block is no change to its own part.
        let mut changed = t[1].clone();
        changed.content.pop();
        assert_eq!(own(&changed), own(&t[1]));
        assert_ne!(shell(&changed), shell(&t[1]));
    }

    #[test]
    fn a_page_read_whole_rebuilds_every_block() {
        let t = tree(PAGE);
        let seen = Seen::of_page(&t);
        assert!(seen.is_page(&t));
        for n in &t {
            assert_eq!(seen.subtree(n.id().unwrap()).as_ref(), Some(n));
            assert!(seen.complete_over(n));
            assert!(seen.seen_as_it_stands(n));
        }
        let json = serde_json::to_string(&seen).unwrap();
        assert_eq!(serde_json::from_str::<Seen>(&json).unwrap(), seen);
    }

    /// An opened list is known by its tags and the items shown; the rest are
    /// named, never known, and rebuild as unseen.
    #[test]
    fn an_opened_block_teaches_its_tags_and_what_was_shown() {
        let t = tree(PAGE);
        let shown = Shown::html(vec![Part::Opened {
            shell: shell(&t[1]),
            inside: vec![Part::Whole(t[1].content[0].clone()), Part::More { count: 1, after: Some("i1".into()) }],
        }]);
        assert_eq!(
            shown.text(),
            "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">Item one</p></li>\
             <!-- 1 more: read on after \"i1\" --></ul>"
        );
        let mut seen = Seen::default();
        seen.learn(&shown, &t, &HashSet::new());
        assert!(seen.knows("l") && seen.knows("i1") && seen.knows("p1"));
        assert!(!seen.knows("i2") && !seen.knows("p3"));
        assert!(!seen.complete_over(&t[1]));
        let rebuilt = seen.subtree("l").unwrap();
        assert_eq!(rebuilt.content[1].kind, UNSEEN);
        assert_eq!(shown.shown_ids(), ["l", "i1", "p1"]);
    }

    /// Markdown does not say what a block's HTML is; a block whose HTML was
    /// shown keeps that while its own part has not changed.
    #[test]
    fn markdown_never_unlearns_html() {
        let t = tree(PAGE);
        let mut seen = Seen::of_page(&t);
        seen.learn(&Shown::markdown(vec![Part::Whole(t[0].clone())]), &t, &HashSet::new());
        assert_eq!(seen.known("a").unwrap().form, Form::Html);
        let mut fresh = Seen::default();
        fresh.learn(&Shown::markdown(vec![Part::Whole(t[0].clone())]), &t, &HashSet::new());
        assert_eq!(fresh.known("a").unwrap().form, Form::Markdown);
    }

    /// The writer knows what it wrote; a merged block waits for a read.
    #[test]
    fn an_edit_moves_the_base_with_what_it_wrote() {
        let before = tree(PAGE);
        let mut seen = Seen::of_page(&before);
        let mut after = before.clone();
        after[0].content = vec![Node::text("Changed", vec![])];
        after[1].content.remove(0);
        let mut added = after[0].clone();
        added.attrs.insert("id".into(), Value::String("new1".into()));
        after.push(added);
        seen.advance(&before, &after, &HashSet::from(["a".to_string()]));
        assert_eq!(seen.subtree("a").as_ref(), Some(&before[0]), "merged: as read");
        assert!(!seen.knows("i1") && !seen.knows("p1"), "taken: forgotten");
        assert_eq!(stubs(&seen.known("l").unwrap().shell), ["i2"]);
        assert!(seen.knows("new1"));

        let mut seen = Seen::of_page(&before);
        seen.advance(&before, &after, &HashSet::new());
        assert_eq!(seen.subtree("a").as_ref(), Some(&after[0]), "written: as written");
    }
}

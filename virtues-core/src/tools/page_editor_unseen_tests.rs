//! The model's edits of partly read blocks, at random.
//!
//! Pages of nested lists, tables, quotes and callouts, with paragraphs too
//! long for any read and code whose HTML runs far longer than its markdown,
//! read at small limits so blocks open in part: whole,
//! by id and on from a block, with the same builders `get_page_content`
//! uses. A person edits between reads, and moves blocks as the editor does,
//! keeping their ids (an item indented or outdented, a block joined to the
//! quote above it or lifted out of it, and moved back by an undo). A model
//! writes only from what reads
//! showed it: a block's HTML as shown (comments and all), or as it would
//! write it from markdown; deletes and rewrites, replaces of a block around
//! another, ids from a read a person has since overtaken.
//!
//! The oracle never asks the base. It keeps, from the text of each read,
//! refusal and edit result, every block whose own part the text shows (its
//! HTML, its opening tags, or its markdown under its id) and every block a
//! comment lists as too long for any read, with the hash it was listed at.
//!
//! - **Learned is shown:** every block the base holds, it holds as some
//!   text showed it, and as HTML only when HTML showed it; and a block it
//!   holds as shown inside another, a text showed there.
//! - **Safety:** every block an applied batch takes off the page, or writes
//!   over, a text showed at the version it had just before the batch (as
//!   HTML, to write over it), or a comment listed it at that version and an
//!   op names it, or it is a text block the batch merged; and one it takes
//!   with the block around it, a text showed inside that block, so a block a
//!   person moved there since (an item indented, a paragraph joined into a
//!   quote) is not taken on the strength of a text that showed it elsewhere.
//! - **Liveness:** with no person editing, doing what a refusal says (the
//!   reads it names, the op it says to add, move or leave out) gets the
//!   batch applied within a few rounds. A refusal that cannot be followed
//!   is the model's own mistake in the batch it wrote (an op the contract
//!   refuses: an item where no list is, a list left empty), or a batch all
//!   of whose ops it says to leave out; anything else, a refusal of a batch
//!   rewritten as a refusal said, or the same refusal twice, fails.

use super::*;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use std::collections::HashMap;
use virtues_document::seen::{own, shell, tags};
use virtues_document::Applied;

/// What one read holds here, as `json_bytes` counts it: small, so lists,
/// quotes and tables open in part and long paragraphs fit no read.
const LIMIT: usize = 900;
/// What a refusal and an edit's result show, in characters of HTML, each
/// run: more than a read holds, so a refusal shows whole every block a read
/// can, and less, as a box's are (`WRITTEN_HTML_CHARS` against a read's
/// 64 KiB), so a refusal often cannot show what it is about and says what
/// to read instead.
const SHOWN: [usize; 2] = [WRITTEN_HTML_CHARS, 300];
/// The longest page a full read shows as HTML; a longer one is markdown.
const PAGE_HTML: usize = 1_600;
const SEEDS: u64 = 60;
const STEPS: usize = 50;
/// The most rounds of doing what a refusal says before the batch lands. A
/// round reads what the refusal names, and at this file's small limits a
/// block nested a few lists deep opens one level a read, so a batch taking a
/// deep structure takes a round a level; what is checked is that every round
/// learns something, as the same refusal twice is not, and this only stops
/// a run that never ends.
const ROUNDS: usize = 100;

fn fits(n: &Node) -> bool {
    json_bytes(&html1(n)) <= LIMIT
}

fn html1(n: &Node) -> String {
    virtues_document::to_html(std::slice::from_ref(n), true)
}

fn fingerprint(n: &Node) -> String {
    virtues_document::node_hash(&own(n))
}

fn page_tree(doc: &yrs::Doc) -> Vec<Node> {
    use yrs::Transact;
    virtues_document::read_doc(&doc.transact())
}

const WORDS: &[&str] = &[
    "coast", "harbour", "lighthouse", "ridge", "taxi", "folder", "tickets", "lunch", "ferry",
    "market", "bags", "seats", "map", "dinner", "train", "museum",
];

fn words(rng: &mut SmallRng, n: usize) -> String {
    (0..n).map(|_| WORDS[rng.random_range(0..WORDS.len())]).collect::<Vec<_>>().join(" ")
}

/// A paragraph's inside: a few words, a mention, words whose HTML is longer
/// than their markdown, or text too long for any read.
fn text(rng: &mut SmallRng) -> String {
    match rng.random_range(0..12) {
        0 => words(rng, 200),
        11 => format!("{} &amp; {} &lt; {}", words(rng, 2), words(rng, 2), words(rng, 2)),
        1 => format!(
            "Lunch with <virtues-mention to=\"/person/person_1\" label=\"Nick\"></virtues-mention> by the {}",
            words(rng, 2)
        ),
        _ => {
            let n = rng.random_range(2..7);
            words(rng, n)
        }
    }
}

fn list(rng: &mut SmallRng, depth: usize) -> String {
    let n = rng.random_range(1..14);
    let items: String = (0..n)
        .map(|_| {
            let nested = if depth < 2 && rng.random_bool(0.15) { list(rng, depth + 1) } else { String::new() };
            format!("<li><p>{}</p>{nested}</li>", text(rng))
        })
        .collect();
    format!("<ul>{items}</ul>")
}

fn blocks(rng: &mut SmallRng, depth: usize) -> String {
    let n = rng.random_range(1..4);
    (0..n).map(|_| block(rng, depth + 1)).collect()
}

/// A code block of up to 40 lines of comparisons, whose HTML (`&lt;`,
/// `&amp;&amp;`) runs far longer than its markdown: one whose markdown fits
/// a read and whose HTML does not is shown whole as markdown and listed as
/// too long by a read with ids.
fn code(rng: &mut SmallRng) -> String {
    let lines: Vec<String> = (0..rng.random_range(1..40))
        .map(|_| format!("if a &lt; b &amp;&amp; c &gt; d {{ {} }}", words(rng, 1)))
        .collect();
    format!("<pre><code>{}</code></pre>", lines.join("\n"))
}

fn block(rng: &mut SmallRng, depth: usize) -> String {
    let pick = if depth > 1 { rng.random_range(0..2) } else { rng.random_range(0..6) };
    match pick {
        0 => format!("<p>{}</p>", text(rng)),
        1 => list(rng, depth),
        2 => format!("<blockquote>{}</blockquote>", blocks(rng, depth)),
        3 => format!("<aside data-tone=\"tip\">{}</aside>", blocks(rng, depth)),
        4 => code(rng),
        _ if depth == 0 => {
            let rows: String = (0..rng.random_range(1..5))
                .map(|_| {
                    let cell = |rng: &mut SmallRng| {
                        if rng.random_bool(0.2) { list(rng, 2) } else { format!("<p>{}</p>", text(rng)) }
                    };
                    format!("<tr><td>{}</td><td>{}</td></tr>", cell(rng), cell(rng))
                })
                .collect();
            format!("<table>{rows}</table>")
        }
        _ => format!("<p>{}</p>", text(rng)),
    }
}

fn new_page(rng: &mut SmallRng) -> yrs::Doc {
    let n = rng.random_range(2..6);
    let html: String = (0..n).map(|_| block(rng, 0)).collect::<String>() + "<p>The end.</p>";
    let o = virtues_document::parse_html(&html, "doc");
    assert!(o.errors.is_empty(), "{:?}", o.errors);
    virtues_document::doc_from_nodes(o.nodes)
}

/// What the texts the model was given showed of each block.
#[derive(Default)]
struct Oracle {
    /// Block id: the fingerprints of its own part a text showed, each with
    /// whether as HTML.
    shown: HashMap<String, HashSet<(String, bool)>>,
    /// Block id: the hashes a comment listed it at, as too long for any read.
    listed: HashMap<String, HashSet<String>>,
    /// Every block id any page held, for the learned-is-shown check.
    ever: HashSet<String>,
    /// Block id: the blocks a text showed it inside (`None`: the page).
    inside: HashMap<String, HashSet<Option<String>>>,
}

impl Oracle {
    fn log(&mut self, n: &Node, html: bool, parents: &HashMap<String, Option<String>>) {
        n.walk(&mut |b| {
            if let Some(id) = b.id() {
                self.shown.entry(id.to_string()).or_default().insert((fingerprint(b), html));
                self.inside.entry(id.to_string()).or_default().insert(parents.get(id).cloned().flatten());
            }
        });
    }

    fn log_own(&mut self, n: &Node, parents: &HashMap<String, Option<String>>) {
        if let Some(id) = n.id() {
            self.shown.entry(id.to_string()).or_default().insert((fingerprint(n), true));
            self.inside.entry(id.to_string()).or_default().insert(parents.get(id).cloned().flatten());
        }
    }

    /// What `text` shows of the page `tree`, as it stood when the text was
    /// written.
    fn read(&mut self, text: &str, form: Form, tree: &[Node]) {
        let whole_only = page_reads::TooLong::WholeOnly.advice();
        let parents = parents(tree);
        let mut blocks = vec![];
        for n in tree {
            n.walk(&mut |b| {
                if b.id().is_some() {
                    blocks.push(b.clone());
                }
            });
        }
        for b in &blocks {
            let id = b.id().unwrap();
            self.ever.insert(id.to_string());
            match form {
                Form::Html => {
                    if text.contains(&html1(b)) {
                        self.log(b, true, &parents);
                    } else if !b.content.is_empty() && b.content.iter().any(|c| c.id().is_some() || !c.is_text()) {
                        let (open, _) = tags(&shell(b));
                        if text.contains(&open) {
                            self.log_own(b, &parents);
                        }
                    }
                    if text.contains(&format!("<!-- {id}: {whole_only}")) {
                        self.listed.entry(id.to_string()).or_default().insert(virtues_document::node_hash(b));
                    }
                }
                Form::Markdown => {
                    if text.contains(&format!("<!-- {id} -->\n<!-- {whole_only}")) {
                        self.listed.entry(id.to_string()).or_default().insert(virtues_document::node_hash(b));
                    }
                }
            }
        }
        if form == Form::Markdown {
            for b in tree {
                let md = virtues_document::to_markdown_with_ids(std::slice::from_ref(b));
                if text.contains(md.trim_end()) || text.contains(after_a_twin(b).trim_end()) {
                    self.log(b, false, &parents);
                }
            }
        }
    }

    /// The blocks an applied batch wrote, as written: the model knows what
    /// it wrote. A block merged with a change made since is not known so.
    fn wrote(&mut self, before: &[Node], after: &[Node], merged: &[String]) {
        let then = index(before);
        let (was_in, now_in) = (parents(before), parents(after));
        for (id, n) in index(after) {
            self.ever.insert(id.clone());
            let changed = then.get(&id).is_none_or(|b| fingerprint(b) != fingerprint(&n));
            let moved = was_in.get(&id) != now_in.get(&id);
            if (changed || moved) && !merged.contains(&id) {
                self.log_own(&n, &now_in);
            }
        }
    }

    fn showed(&self, id: &str, fp: &str, html: bool) -> bool {
        self.shown
            .get(id)
            .is_some_and(|s| s.contains(&(fp.to_string(), true)) || (!html && s.contains(&(fp.to_string(), false))))
    }

    fn listed_at(&self, id: &str, n: &Node) -> bool {
        self.listed.get(id).is_some_and(|l| l.contains(&virtues_document::node_hash(n)))
    }

    /// Every block the base holds, it holds as a text showed it; and each
    /// block of the page `tree` it holds as shown inside the block it is in,
    /// a text showed there. (A shell read before a person moved a block out
    /// of it still names that block: what the base says of where a block
    /// was is no claim about where it is.)
    fn check_learned(&self, seen: &Seen, tree: &[Node]) -> Result<(), String> {
        let now_in = parents(tree);
        for id in &self.ever {
            let Some(k) = seen.known(id) else { continue };
            let fp = fingerprint(&k.shell);
            if !self.showed(id, &fp, k.form == Form::Html) {
                return Err(format!("the base holds `{id}` ({:?}) as no text showed it", k.form));
            }
            for child in virtues_document::seen::stubs(&k.shell) {
                let is_in_it = now_in.get(child).is_some_and(|p| p.as_deref() == Some(id.as_str()));
                let there = self.inside.get(child).is_some_and(|s| s.contains(&Some(id.clone())));
                if is_in_it && seen.knows(child) && k.shows(child) && !there {
                    return Err(format!("the base holds `{child}` as shown inside `{id}`, where no text showed it"));
                }
            }
        }
        Ok(())
    }

    /// What an applied batch took or wrote over was shown at the version it
    /// had, or listed and named, or merged.
    fn check_applied(&self, before: &[Node], after: &[Node], ops: &[Op], applied: &Applied) -> Result<(), String> {
        let now = index(after);
        // Taken by name: deleted, or replaced with nothing. A replace that
        // writes something means its block to stay.
        let named: HashSet<&str> = ops
            .iter()
            .filter_map(|op| match op {
                Op::Delete { id } => Some(id.as_str()),
                Op::Replace { id, html } if html.trim().is_empty() => Some(id.as_str()),
                _ => None,
            })
            .collect();
        let replaced: HashSet<&str> = ops
            .iter()
            .filter_map(|op| match op {
                Op::Replace { id, .. } => Some(id.as_str()),
                _ => None,
            })
            .collect();
        let route = Regex::new(r#"href="([^"]+)""#).unwrap();
        let links_route = ops.iter().filter_map(op_html).any(|html| {
            route.captures_iter(html).any(|c| virtues_document::contract::is_route(&c[1]))
        });
        let was_in = parents(before);
        for (id, c) in index(before) {
            let fp = fingerprint(&c);
            match now.get(&id) {
                None => {
                    let shown = self.showed(&id, &fp, false) && (!links_route || self.showed(&id, &fp, true));
                    let taken_by_name = self.listed_at(&id, &c) && named.contains(id.as_str());
                    if !shown && !taken_by_name {
                        return Err(format!("the batch took `{id}`, which no text showed as it stood"));
                    }
                    let parent = was_in.get(&id).cloned().flatten();
                    let shown_there = self.inside.get(&id).is_some_and(|s| s.contains(&parent));
                    if !named.contains(id.as_str()) && !shown_there {
                        return Err(format!(
                            "the batch took `{id}` with `{}`, and no text showed it inside that block",
                            parent.unwrap_or_default()
                        ));
                    }
                }
                Some(r) if fingerprint(r) != fp => {
                    let shown = self.showed(&id, &fp, true);
                    let merged = applied.merged.contains(&id);
                    let replaced_by_name = self.listed_at(&id, &c) && replaced.contains(id.as_str());
                    if !shown && !merged && !replaced_by_name {
                        return Err(format!("the batch wrote over `{id}`, which no text showed as HTML as it stood"));
                    }
                }
                Some(_) => {}
            }
        }
        Ok(())
    }
}

/// Each block's nearest block around it (`None`: the page).
fn parents(nodes: &[Node]) -> HashMap<String, Option<String>> {
    fn walk(nodes: &[Node], parent: Option<&str>, out: &mut HashMap<String, Option<String>>) {
        for n in nodes {
            match n.id() {
                Some(id) => {
                    out.insert(id.to_string(), parent.map(str::to_string));
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

fn index(nodes: &[Node]) -> HashMap<String, Node> {
    let mut out = HashMap::new();
    for n in nodes {
        n.walk(&mut |b| {
            if let Some(id) = b.id() {
                out.insert(id.to_string(), b.clone());
            }
        });
    }
    out
}

/// A block's markdown with its id as a read writes it straight after a
/// block of its own kind: a list then takes its kind's other marker, since
/// two lists of one kind side by side are one list to markdown.
fn after_a_twin(n: &Node) -> String {
    let id = n.id().unwrap_or_default();
    let mut twin = n.clone();
    twin.attrs.insert("id".into(), serde_json::Value::String(format!("{id}-twin")));
    let both = virtues_document::to_markdown_with_ids(&[twin, n.clone()]);
    let own = format!("<!-- {id} -->");
    both.find(&own).map_or(both.clone(), |at| both[at..].to_string())
}

/// What the model has: the base its last result named, the ids it has met,
/// and the HTML it would write each block back with.
#[derive(Default)]
struct Model {
    seen: Seen,
    names: Vec<String>,
    html_of: HashMap<String, String>,
}

static MENTION: OnceLock<Regex> = OnceLock::new();

/// A block written from its markdown: a mention comes back as a link.
fn from_markdown(n: &Node) -> String {
    let md = virtues_document::to_markdown(std::slice::from_ref(n));
    let o = virtues_document::parse_markdown(&md);
    let html = virtues_document::to_html(&o.nodes, false);
    let mention = MENTION.get_or_init(|| {
        Regex::new(r#"<virtues-mention to="([^"]+)" label="([^"]+)"></virtues-mention>"#).unwrap()
    });
    mention.replace_all(&html, "<a href=\"$1\">@$2</a>").to_string()
}

impl Model {
    fn meet(&mut self, id: &str) {
        if !self.names.iter().any(|n| n == id) {
            self.names.push(id.to_string());
        }
    }

    /// Take in what a text showed, as the model would copy from it.
    fn saw(&mut self, shown: &Shown) {
        for part in &shown.parts {
            self.saw_part(part, shown.form, true);
        }
    }

    fn saw_part(&mut self, part: &Part, form: Form, top: bool) {
        match part {
            Part::Whole(n) => n.walk(&mut |b| {
                if let Some(id) = b.id() {
                    self.meet(id);
                    match form {
                        Form::Html => {
                            self.html_of.insert(id.to_string(), html1(b));
                        }
                        Form::Markdown if top && b.id() == n.id() => {
                            self.html_of.insert(id.to_string(), from_markdown(b));
                        }
                        Form::Markdown => {}
                    }
                }
            }),
            Part::Opened { shell, inside } => {
                if let Some(id) = shell.id() {
                    self.meet(id);
                    self.html_of.insert(id.to_string(), Shown::html(vec![part.clone()]).text());
                }
                for p in inside {
                    self.saw_part(p, form, false);
                }
            }
            Part::Cell { inside, .. } => {
                for p in inside {
                    self.saw_part(p, form, false);
                }
            }
            Part::Listed { id, .. } => self.meet(id),
            Part::More { .. } | Part::Gone(_) => {}
        }
    }

    fn name(&self, rng: &mut SmallRng) -> Option<String> {
        (!self.names.is_empty()).then(|| self.names[rng.random_range(0..self.names.len())].clone())
    }
}

/// A read, as `get_page_content` makes it at this file's limits: its text's
/// parts and where reading on goes.
fn read_page(model: &Model, tree: &[Node], how: &Read) -> (Shown, Seen, Option<String>) {
    match how {
        Read::Full => {
            let html = virtues_document::to_html(tree, true);
            if html.chars().count() <= PAGE_HTML {
                let shown = Shown::html(tree.iter().cloned().map(Part::Whole).collect());
                (shown, Seen::of_page(tree), None)
            } else {
                let w = page_reads::markdown_window(tree, 0, LIMIT, json_bytes, fits);
                let mut seen = Seen::default();
                seen.learn(&w.shown, tree, &HashSet::new());
                (w.shown, seen, w.next)
            }
        }
        Read::Ids(ids) => {
            let read = read_by_id(&model.seen, tree, ids, LIMIT, json_bytes);
            let mut parts = read.parts.clone();
            parts.extend(read.missing.iter().cloned().map(Part::Gone));
            let shown = Shown::html(parts);
            let mut seen = model.seen.clone();
            seen.learn(&shown, tree, &HashSet::new());
            (shown, seen, read.opened.and_then(|o| o.next))
        }
        Read::After(after) => {
            let (shown, next) = match tree.iter().position(|n| n.id() == Some(after.as_str())) {
                Some(at) => {
                    let w = page_reads::markdown_window(tree, at + 1, LIMIT, json_bytes, fits);
                    (w.shown, w.next)
                }
                None => match siblings_of(tree, after) {
                    Some((parent, siblings, at)) => {
                        let w = children_window(siblings, at + 1, LIMIT, LIMIT, json_bytes);
                        (Shown::html(w.parts).inside(parent), w.next)
                    }
                    None => (Shown::html(vec![]), None),
                },
            };
            let mut seen = model.seen.clone();
            seen.learn(&shown, tree, &HashSet::new());
            (shown, seen, next)
        }
    }
}

enum Read {
    Full,
    Ids(Vec<String>),
    After(String),
}

struct World {
    /// What a refusal and an edit's result show (`SHOWN`).
    shown: usize,
    rng: SmallRng,
    doc: yrs::Doc,
    model: Model,
    oracle: Oracle,
    applied: usize,
    refused: usize,
    followed: usize,
    unfollowable: usize,
    /// The page before the person last moved a block out of another, and
    /// that block, to move it back.
    moved_out: Option<(Vec<Node>, String)>,
    /// The block a person moved a block back into, which the model's next
    /// batch writes over or takes half the time.
    came_back: Option<String>,
    trace: Vec<String>,
}

impl World {
    fn fail(&self, why: String) -> ! {
        panic!("{why}\nsteps:\n{}", self.trace.join("\n"));
    }

    /// A read, and on to the end of what it opened.
    fn read(&mut self, how: Read) {
        let mut how = how;
        loop {
            let tree = page_tree(&self.doc);
            self.trace.push(match &how {
                Read::Full => "read".to_string(),
                Read::Ids(ids) => format!("read ids {ids:?}"),
                Read::After(a) => format!("read after {a}"),
            });
            let (shown, seen, next) = read_page(&self.model, &tree, &how);
            self.oracle.read(&shown.text(), shown.form, &tree);
            self.model.saw(&shown);
            self.model.seen = seen;
            if let Err(e) = self.oracle.check_learned(&self.model.seen, &tree) {
                self.fail(e);
            }
            match next {
                Some(next) => how = Read::After(next),
                None => return,
            }
        }
    }

    fn person(&mut self) {
        let tree = page_tree(&self.doc);
        // In page order, so a seed makes the same run every time.
        let mut blocks: Vec<Node> = vec![];
        for n in &tree {
            n.walk(&mut |b| {
                if b.id().is_some() {
                    blocks.push(b.clone());
                }
            });
        }
        if let Some((was, from)) = self.moved_out.take().filter(|_| self.rng.random_bool(0.5)) {
            self.trace.push(format!("person moves it back into {from} (undo)"));
            self.doc = virtues_document::doc_from_nodes(was);
            self.came_back = Some(from);
            return;
        }
        if self.rng.random_bool(0.25) {
            let moved = if self.rng.random_bool(0.5) {
                self.person_moves(tree.clone())
            } else {
                self.person_moves_out(tree.clone())
            };
            if moved {
                return;
            }
        }
        let b = &blocks[self.rng.random_range(0..blocks.len())];
        let id = b.id().unwrap().to_string();
        let new_text = format!("{} typed", words(&mut self.rng, 3));
        let op = match (b.kind.as_str(), self.rng.random_range(0..3)) {
            ("paragraph", 0) => Op::Replace { id, html: format!("<p>{new_text}</p>") },
            ("paragraph", 1) => Op::InsertAfter { id, html: format!("<p>{new_text}</p>") },
            ("listItem", 1) => Op::InsertAfter { id, html: format!("<li><p>{new_text}</p></li>") },
            ("tableRow", _) => return,
            (_, 2) if tree.len() > 2 => Op::Delete { id },
            _ => return,
        };
        self.trace.push(format!("person {op:?}"));
        let _ = virtues_document::apply_ops(&self.doc, None, &[op]);
    }

    /// A block moved, keeping its id, into the block before it, as the
    /// editor moves one: an item indented into the nested list of the item
    /// above (Tab), a block joined to the end of the quote or callout above
    /// (Backspace at its start). Whether there was one to move.
    fn person_moves(&mut self, mut tree: Vec<Node>) -> bool {
        /// Every place a block can move into the one before it: the path to
        /// what holds the two, and the later one's index.
        fn spots(nodes: &[Node], path: &mut Vec<usize>, out: &mut Vec<(Vec<usize>, usize)>) {
            for (i, n) in nodes.iter().enumerate() {
                if i > 0 {
                    let prev = &nodes[i - 1];
                    let into_list = n.kind == "listItem"
                        && prev.content.last().is_some_and(|l| matches!(l.kind.as_str(), "bulletList" | "orderedList"));
                    let into_quote = matches!(prev.kind.as_str(), "blockquote" | "callout") && n.kind != "table";
                    if into_list || into_quote {
                        out.push((path.clone(), i));
                    }
                }
                path.push(i);
                spots(&n.content, path, out);
                path.pop();
            }
        }
        fn holder<'a>(nodes: &'a mut Vec<Node>, path: &[usize]) -> &'a mut Vec<Node> {
            match path.split_first() {
                None => nodes,
                Some((&i, rest)) => holder(&mut nodes[i].content, rest),
            }
        }
        let mut found = vec![];
        spots(&tree, &mut vec![], &mut found);
        if found.is_empty() {
            return false;
        }
        let (path, i) = found.swap_remove(self.rng.random_range(0..found.len()));
        let siblings = holder(&mut tree, &path);
        let moving = siblings.remove(i);
        let prev = &mut siblings[i - 1];
        let into = match prev.content.last_mut() {
            Some(list) if moving.kind == "listItem" => list,
            _ => prev,
        };
        let into_id = into.id().unwrap_or_default().to_string();
        self.trace.push(format!("person moves {} into {into_id}", moving.id().unwrap_or_default()));
        into.content.push(moving);
        self.doc = virtues_document::doc_from_nodes(tree);
        true
    }

    /// A block moved out, keeping its id, past the end of the block it is
    /// the last of, as the editor moves one: an item outdented from a
    /// nested list to follow the item that held it (Shift-Tab on the last
    /// item), a quote's or callout's last block lifted out after it. Half
    /// the time the model then reads on from the block it now follows, so a
    /// read shows it where it went: at the page's top level, as markdown,
    /// when it left a quote there. With [`World::person_moves`] or the
    /// person's undo, a block goes out and comes back: an item counted by a
    /// read of its list, moved out, read where it went and moved back, was
    /// taken with that list as shown inside it. Whether there was one to
    /// move.
    fn person_moves_out(&mut self, mut tree: Vec<Node>) -> bool {
        /// Every block a last child can move out of, by its path: a nested
        /// list in an item, a quote or a callout, holding two or more.
        fn spots(nodes: &[Node], in_item: bool, path: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
            for (i, n) in nodes.iter().enumerate() {
                path.push(i);
                let list = matches!(n.kind.as_str(), "bulletList" | "orderedList");
                let quote = matches!(n.kind.as_str(), "blockquote" | "callout");
                if n.content.len() >= 2 && ((list && in_item) || quote) {
                    out.push(path.clone());
                }
                spots(&n.content, n.kind == "listItem", path, out);
                path.pop();
            }
        }
        fn holder<'a>(nodes: &'a mut Vec<Node>, path: &[usize]) -> &'a mut Vec<Node> {
            match path.split_first() {
                None => nodes,
                Some((&i, rest)) => holder(&mut nodes[i].content, rest),
            }
        }
        let mut found = vec![];
        spots(&tree, false, &mut vec![], &mut found);
        if found.is_empty() {
            return false;
        }
        let path = found.swap_remove(self.rng.random_range(0..found.len()));
        let (&at, around) = path.split_last().expect("a spot's path names it");
        let was = tree.clone();
        let from = holder(&mut tree, around)[at].id().unwrap_or_default().to_string();
        let moving = holder(&mut tree, around)[at].content.pop().expect("it holds two or more");
        self.trace.push(format!("person moves {} out of {from}", moving.id().unwrap_or_default()));
        // The block it now follows.
        let follows = match around.split_last() {
            // An item goes to the list around the item that held its list.
            Some((&item, outer)) if moving.kind == "listItem" => {
                let follows = holder(&mut tree, outer)[item].id().map(str::to_string);
                holder(&mut tree, outer).insert(item + 1, moving);
                follows
            }
            _ => {
                holder(&mut tree, around).insert(at + 1, moving);
                Some(from.clone())
            }
        };
        self.doc = virtues_document::doc_from_nodes(tree);
        self.moved_out = Some((was, from.clone()));
        if let Some(follows) = follows.filter(|f| self.model.names.contains(f) && self.rng.random_bool(0.5)) {
            self.read(Read::After(follows));
        }
        true
    }

    fn batch(&mut self) -> Vec<Op> {
        let mut ops = vec![];
        let came_back = self
            .came_back
            .take()
            .filter(|id| self.model.names.contains(id) && self.rng.random_bool(0.5));
        for i in 0..self.rng.random_range(1..4) {
            let named = if i == 0 { came_back.clone() } else { None };
            let Some(x) = named.or_else(|| self.model.name(&mut self.rng)) else { break };
            let html = self.model.html_of.get(&x).cloned();
            match (self.rng.random_range(0..7), html) {
                (0, _) => ops.push(Op::Delete { id: x }),
                (1, Some(html)) => ops.push(Op::Replace { id: x, html: tweak(&mut self.rng, &html) }),
                (2, Some(html)) => {
                    let html = if self.rng.random_bool(0.5) { strip_ids(&html) } else { html };
                    ops.push(Op::Delete { id: x });
                    ops.push(Op::Append { html });
                }
                (3, Some(html)) if html.contains("<!--") => {
                    let html = if self.rng.random_bool(0.5) { strip_comments(&html) } else { html };
                    ops.push(Op::Replace { id: x, html: tweak(&mut self.rng, &html) });
                }
                (4, Some(html)) => {
                    let new = if html.starts_with("<li") {
                        "<li><p>Model item</p></li>"
                    } else if html.starts_with("<p") || html.starts_with("<ul") || html.starts_with("<blockquote") {
                        "<p>Model line</p>"
                    } else {
                        continue;
                    };
                    ops.push(Op::InsertAfter { id: x, html: new.to_string() });
                }
                (5, _) => ops.push(Op::Append { html: "<p>Model line</p>".into() }),
                _ => {}
            }
        }
        ops
    }

    /// Apply a batch with the model's base, as `edit_page` does; on a
    /// refusal, do what it says, with no person editing, until it lands.
    fn edit(&mut self, mut ops: Vec<Op>) {
        // The last refusal's problems, to catch one said twice.
        let mut last: Option<Vec<String>> = None;
        for round in 0..=ROUNDS {
            let before = page_tree(&self.doc);
            self.trace.push(format!("edit {ops:?}"));
            match virtues_document::apply_ops(&self.doc, Some(&self.model.seen), &ops) {
                Ok(applied) => {
                    let after = page_tree(&self.doc);
                    if let Err(e) = self.oracle.check_applied(&before, &after, &ops, &applied) {
                        self.fail(e);
                    }
                    self.oracle.wrote(&before, &after, &applied.merged);
                    let merged: HashSet<String> = applied.merged.iter().cloned().collect();
                    self.model.seen.advance(&before, &after, &merged);
                    let mut fit = Fit::new(self.shown);
                    for n in in_document_order(&after, &applied.written) {
                        fit.whole(&n);
                    }
                    let shown = Shown::html(fit.parts);
                    self.oracle.read(&shown.text(), Form::Html, &after);
                    self.model.saw(&shown);
                    self.model.seen.learn(&shown, &after, &HashSet::new());
                    if let Err(e) = self.oracle.check_learned(&self.model.seen, &after) {
                        self.fail(e);
                    }
                    self.applied += 1;
                    if round > 0 {
                        self.followed += 1;
                    }
                    return;
                }
                Err(problems) => {
                    if round == 0 {
                        self.refused += 1;
                    }
                    let checked = virtues_document::check_ops(&before, &self.model.seen, &ops);
                    let r = refusal_of(
                        &ops,
                        &self.model.seen,
                        problems,
                        checked.unseen,
                        &checked.conflicts,
                        &before,
                        self.shown,
                        &fits,
                    );
                    self.oracle.read(&r.blocks, Form::Html, &before);
                    self.model.saw(&r.shown);
                    self.model.seen.learn(&r.shown, &before, &r.keep);
                    if let Err(e) = self.oracle.check_learned(&self.model.seen, &before) {
                        self.fail(e);
                    }
                    self.trace.push(format!("refused {:?} unread {:?}", r.problems, r.unread.long));
                    let said: Vec<String> = r.problems.iter().map(|p| p.message.clone()).collect();
                    if last.as_ref() == Some(&said) {
                        self.fail(format!("the same refusal twice running: {said:?}"));
                    }
                    match self.follow(ops, &r) {
                        Ok(next) if !next.is_empty() => ops = next,
                        // Every op left out, as the refusal said.
                        Ok(_) => {
                            self.unfollowable += 1;
                            return;
                        }
                        Err(stuck) if round == 0 && models_own_misfit(&stuck) => {
                            self.unfollowable += 1;
                            return;
                        }
                        Err(stuck) => self.fail(format!(
                            "round {round}: a refusal says nothing to do that would land the batch: {stuck:?}"
                        )),
                    }
                    last = Some(said);
                }
            }
        }
        self.fail(format!("a batch was refused {ROUNDS} times running with nobody else editing"));
    }

    /// The batch as a refusal says to send it again, after the reads it
    /// names; `Err` with the first problem that says to change what no rule
    /// here can.
    fn follow(&mut self, mut ops: Vec<Op>, r: &Refusal) -> Result<Vec<Op>, Problem> {
        let mut reads: Vec<Read> = vec![];
        let mut dropped: Vec<usize> = vec![];
        let mut before: Vec<(usize, Op)> = vec![];
        let mut moved: Vec<(usize, usize)> = vec![];
        let quoted = |message: &str, after: &str| -> Option<String> {
            Some(message.split(after).nth(1)?.split(['"', '`']).next()?.to_string())
        };
        for p in &r.problems {
            let message = p.message.as_str();
            let stuck = || p.clone();
            let op = problem_op(&p.at).map(|(i, _)| i);
            if message.contains("after \"") {
                reads.push(Read::After(quoted(message, "after \"").ok_or_else(stuck)?));
            } else if message.contains("ids [\"") {
                reads.push(Read::Ids(vec![quoted(message, "ids [\"").ok_or_else(stuck)?]));
            } else if let Some(c) = by_name().captures(message) {
                before.push((op.ok_or_else(stuck)?, Op::Delete { id: c[1].to_string() }));
            } else if message.contains("put this op before op ") {
                let n: usize = quoted(message, "put this op before op ")
                    .ok_or_else(stuck)?
                    .trim_end_matches(|c: char| !c.is_ascii_digit())
                    .parse()
                    .map_err(|_| stuck())?;
                moved.push((op.ok_or_else(stuck)?, n - 1));
            } else if message.contains("HTML comment") {
                if let Some(i) = op {
                    ops[i] = match &ops[i] {
                        Op::Replace { id, html } => Op::Replace { id: id.clone(), html: strip_comments(html) },
                        Op::InsertAfter { id, html } => Op::InsertAfter { id: id.clone(), html: strip_comments(html) },
                        Op::InsertBefore { id, html } => Op::InsertBefore { id: id.clone(), html: strip_comments(html) },
                        Op::Append { html } => Op::Append { html: strip_comments(html) },
                        other => other.clone(),
                    };
                }
            } else if message.contains("or append it") {
                let i = op.ok_or_else(stuck)?;
                if let Op::InsertAfter { html, .. } | Op::InsertBefore { html, .. } = &ops[i] {
                    ops[i] = Op::Append { html: html.clone() };
                }
            } else if message.contains("leave it out")
                || message.contains("leave this op out")
                || message.contains("leave one of the two out")
                || message.starts_with("no block has id")
            {
                dropped.push(op.ok_or_else(stuck)?);
            } else if message.contains("is above as it is now")
                || message.contains("read it as it is now")
                || message.contains("read it first")
                || message.contains("read it as HTML")
                || message.contains("still too long for any read")
                || message.starts_with("someone is editing the same words")
            {
            } else {
                return Err(stuck());
            }
        }
        if !r.unread.long.is_empty() || !r.unread.inside.is_empty() {
            reads.push(Read::Ids(r.unread.long.iter().chain(&r.unread.inside).cloned().collect()));
        }
        for read in reads {
            self.read(read);
        }
        let mut out: Vec<Option<Op>> = ops.into_iter().map(Some).collect();
        for (i, to) in moved {
            if i < out.len() && to < i {
                let op = out.remove(i);
                out.insert(to, op);
            }
        }
        for i in dropped {
            if let Some(slot) = out.get_mut(i) {
                *slot = None;
            }
        }
        before.sort_by_key(|(i, _)| std::cmp::Reverse(*i));
        for (i, op) in before {
            out.insert(i.min(out.len()), Some(op));
        }
        Ok(out.into_iter().flatten().collect())
    }
}

/// Whether a refusal says the model's own batch wrote what the contract
/// refuses, an op it should not have written: HTML that does not read (a
/// problem inside an op's HTML, `op 2: doc > table`), an item where no list
/// is, a row where no table is, a block emptied of all it must hold.
fn models_own_misfit(p: &Problem) -> bool {
    let m = p.message.as_str();
    p.at.contains(": ") || m.contains(" would hold ") || m.contains(" cannot hold ") || m.contains("HTML reads it only inside one")
}

/// A refusal's op to add before another: "delete `x` by its id in an op
/// before op 2", or "delete or replace `x` …", followed as a delete.
fn by_name() -> &'static Regex {
    static BY_NAME: OnceLock<Regex> = OnceLock::new();
    BY_NAME.get_or_init(|| Regex::new(r"delete (?:or replace )?`([^`]+)` by its id in an op before op").unwrap())
}

fn tweak(rng: &mut SmallRng, html: &str) -> String {
    if rng.random_bool(0.5) {
        html.replacen("</p>", " edited</p>", 1)
    } else {
        html.to_string()
    }
}

fn strip_ids(html: &str) -> String {
    Regex::new(r#" data-id="[^"]*""#).unwrap().replace_all(html, "").to_string()
}

fn strip_comments(html: &str) -> String {
    Regex::new(r"<!--.*?-->").unwrap().replace_all(html, "").to_string()
}

/// Learned is shown, every batch applied takes or writes over only what
/// was shown, and every refusal followed gets its batch applied: with a
/// refusal that shows more than a read holds, and with one that shows less,
/// as a box's does, where a refusal that could not show a block it was
/// about was followed into the same refusal again, and a block moved into
/// a block read in part was taken with it.
#[test]
fn model_edits_of_partly_read_blocks_take_only_what_reads_showed() {
    for shown in SHOWN {
        edits_with_refusals_showing(shown);
    }
}

fn edits_with_refusals_showing(shown: usize) {
    let (mut applied, mut refused, mut followed, mut unfollowable) = (0, 0, 0, 0);
    for seed in 0..SEEDS {
        let mut rng = SmallRng::seed_from_u64(seed);
        let doc = new_page(&mut rng);
        let mut world = World {
            shown,
            rng,
            doc,
            model: Model::default(),
            oracle: Oracle::default(),
            applied: 0,
            refused: 0,
            followed: 0,
            unfollowable: 0,
            moved_out: None,
            came_back: None,
            trace: vec![format!("seed {seed}, a refusal showing {shown} characters")],
        };
        world.read(Read::Full);
        for _ in 0..STEPS {
            match world.rng.random_range(0..20) {
                0..=1 => world.read(Read::Full),
                2..=4 => {
                    let n = world.rng.random_range(1..4);
                    let ids: Vec<String> = (0..n).filter_map(|_| world.model.name(&mut world.rng)).collect();
                    if !ids.is_empty() {
                        world.read(Read::Ids(ids));
                    }
                }
                5..=6 => {
                    if let Some(after) = world.model.name(&mut world.rng) {
                        world.read(Read::After(after));
                    }
                }
                7..=10 => world.person(),
                _ => {
                    let ops = world.batch();
                    if !ops.is_empty() {
                        world.edit(ops);
                    }
                }
            }
        }
        applied += world.applied;
        refused += world.refused;
        followed += world.followed;
        unfollowable += world.unfollowable;
    }
    eprintln!(
        "showing {shown}: applied {applied}, refused {refused}, followed to applied {followed}, unfollowable {unfollowable}"
    );
    assert!(applied >= 150, "too few batches applied to judge: {applied}");
    assert!(followed >= 40, "too few refusals followed to judge: {followed}");
}

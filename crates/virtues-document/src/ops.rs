//! The model's write path: edits addressed by block id, content as HTML.
//!
//! Every op is parsed and checked against the contract on a copy of the tree
//! first. Only when the whole batch fits is anything written, in the one
//! Yjs transaction the batch was read in. A refusal changes nothing and says
//! why.
//!
//! Replacing a text block with one of the same type is applied as a text
//! diff inside it, not a delete and insert, so a person typing elsewhere in
//! the same paragraph keeps their cursor and their keystrokes. Inline atoms
//! (a mention, a line break) are stepped over when the replacement holds the
//! same ones in the same order. Replacing any other block with one of its
//! type (a list, a table, a quote) patches it the same way, child by child:
//! a model rewrites a whole list to tick one item, and a keystroke a person
//! sent into another item meanwhile still lands.

use crate::contract::Contract;
use crate::ingest::{parse_slice, Mode, MAX_INPUT_BYTES};
use crate::model::{depth, new_id, too_deep, Mark, Node, Problem, MAX_DEPTH};
use crate::seen::Seen;
use crate::ydoc::{self, read_doc, read_text, value_to_any, write_nodes};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use yrs::types::Delta;
use yrs::{
    Any, Doc, In, OffsetKind, Out, ReadTxn, Text, Transact, TransactionMut, Xml, XmlElementRef,
    XmlFragment, XmlFragmentRef, XmlTextPrelim, XmlTextRef,
};

/// The most ops one batch holds. A column is added or taken out of a table
/// by replacing every row in one batch, so a table of more rows than this
/// changes a column only by being replaced whole, as a grid refusal says.
pub const MAX_OPS: usize = 200;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    Replace { id: String, html: String },
    InsertAfter { id: String, html: String },
    InsertBefore { id: String, html: String },
    Append { html: String },
    Delete { id: String },
}

#[derive(Debug, Serialize, Default)]
pub struct Applied {
    pub notes: Vec<Problem>,
    /// Ids of the blocks written, in document order of the ops.
    pub written: Vec<String>,
    /// How each replace landed: "text-diff" (a text block's changed text),
    /// "patch" (any other block, child by child) or "swap".
    pub replaced_as: Vec<String>,
    /// Text blocks a replace merged with a change made since the writer's
    /// read: the writer has not seen them as they now stand.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub merged: Vec<String>,
}

/// One op resolved against the tree: where it lands and what it writes.
struct Plan {
    target: Option<String>,
    kind: PlanKind,
    nodes: Vec<Node>,
}

enum PlanKind {
    Replace,
    InsertAfter,
    InsertBefore,
    Append,
    Delete,
}

fn find_path(nodes: &[Node], id: &str, path: &mut Vec<usize>) -> bool {
    for (i, n) in nodes.iter().enumerate() {
        path.push(i);
        if n.id() == Some(id) || find_path(&n.content, id, path) {
            return true;
        }
        path.pop();
    }
    false
}

/// A parent a block was taken out of, left holding what it cannot
/// ([`plan_batch`]): where it is, as later ops in the batch cannot shift
/// it, and the op that left it so.
struct Emptied {
    at: String,
    /// The nearest block with an id at or around the parent; `None` for the
    /// page itself.
    holder: Option<String>,
    /// The steps from that block down to the parent: a table cell's, which
    /// carries no id.
    steps: Vec<usize>,
}

/// [`Emptied`]'s place for the node at `path` under `root`.
fn anchor_of(root: &Node, path: &[usize]) -> (Option<String>, Vec<usize>) {
    let (mut holder, mut steps) = (None, vec![]);
    let mut n = root;
    for &i in path {
        n = &n.content[i];
        match n.id() {
            Some(id) => {
                holder = Some(id.to_string());
                steps.clear();
            }
            None => steps.push(i),
        }
    }
    (holder, steps)
}

fn parent_mut<'a>(root: &'a mut Node, path: &[usize]) -> &'a mut Node {
    let mut n = root;
    for &i in &path[..path.len() - 1] {
        n = &mut n.content[i];
    }
    n
}

pub(crate) fn collect_ids(nodes: &[Node], ids: &mut HashSet<String>) {
    for n in nodes {
        n.walk(&mut |n| {
            if let Some(id) = n.id() {
                ids.insert(id.to_string());
            }
        });
    }
}

/// Fresh ids for new blocks. A replacement's first block keeps the id of the
/// block it replaces, whatever id the model wrote on it: the edited block is
/// still that block, and a block whose id changed would read as deleted to
/// the next edit naming it. Any other id the model wrote that is already
/// taken is renewed. A replace frees every id in the block it replaces
/// before this runs, so the ids of blocks nested in it that the model wrote
/// back are kept.
pub(crate) fn assign_ids(
    c: &Contract,
    nodes: &mut [Node],
    taken: &mut HashSet<String>,
    keep: Option<&str>,
) {
    let mut first = true;
    for n in nodes.iter_mut() {
        n.walk_mut(&mut |n| {
            let top = std::mem::replace(&mut first, false);
            if !c.node(&n.kind).map(|s| s.spec.id).unwrap_or(false) {
                return;
            }
            let current = n.id().map(str::to_string);
            let id = match (&current, keep) {
                (_, Some(k)) if top && !taken.contains(k) => k.to_string(),
                (Some(id), _) if !taken.contains(id) => id.clone(),
                _ => new_id(),
            };
            taken.insert(id.clone());
            n.attrs.insert(c.id.attr.clone(), Value::String(id));
        });
    }
}

/// The ids a replace's HTML leaves off its nested blocks, from the block it
/// replaces (`old`): each child of `new` without an id takes the id of the
/// child of `old` it pairs with, by type and text as
/// [`crate::diff::pair_blocks`] pairs blocks, or else by its place among the
/// unpaired children of its type between two that pair; then the same one
/// level down. An id the HTML gives another block (`written`) is never
/// taken. Without this every nested block the model wrote without its
/// `data-id` was new, so the replace swapped it whole, and a person's
/// keystroke on its way into it was lost.
///
/// A child lends its id only when `lends` says so, asked with the nearest
/// block around it that has an id: never when its writer's base holds that
/// block and no read showed the child in it, a block its writer cannot
/// mean. Lent anyway, the model's new item took the id of an item a person
/// had moved in, and the replace wrote over that item's words where it
/// should have been refused for taking it.
fn inherit_ids(
    c: &Contract,
    old: &Node,
    new: &mut Node,
    written: &HashSet<String>,
    holder: Option<&str>,
    lends: &dyn Fn(Option<&str>, &str) -> bool,
) {
    if old.kind != new.kind || c.is_textblock(&new.kind) {
        return;
    }
    let holder = old.id().or(holder);
    let free = |n: &Node| n.id().is_none_or(|id| !written.contains(id) && lends(holder, id));
    let paired = crate::diff::pair_blocks(&old.content, &new.content);
    let mut pairs = vec![];
    let (mut i0, mut j0) = (0, 0);
    let ends = std::iter::once((old.content.len(), new.content.len()));
    for (i, j) in paired.into_iter().chain(ends) {
        let mut a = i0;
        for b in j0..j {
            if new.content[b].id().is_some() {
                continue;
            }
            while a < i && (old.content[a].kind != new.content[b].kind || !free(&old.content[a])) {
                a += 1;
            }
            if a < i {
                pairs.push((a, b));
                a += 1;
            }
        }
        if i < old.content.len() {
            pairs.push((i, j));
        }
        (i0, j0) = (i + 1, j + 1);
    }
    for (i, j) in pairs {
        let (was, child) = (&old.content[i], &mut new.content[j]);
        if child.id().is_none() && free(was) {
            if let Some(id) = was.id() {
                child.attrs.insert(c.id.attr.clone(), Value::String(id.to_string()));
            }
        }
        if child.id() == was.id() {
            inherit_ids(c, was, child, written, holder, lends);
        }
    }
}

/// Each footnote label on the page with the first note its footnotes carry.
fn notes_by_label(nodes: &[Node]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for n in nodes {
        n.walk(&mut |n| {
            if n.kind != "footnote" {
                return;
            }
            if let (Some(label), Some(note)) = (
                n.attrs.get("label").and_then(Value::as_str),
                n.attrs.get("note").and_then(Value::as_str),
            ) {
                out.entry(label.to_string()).or_insert_with(|| note.to_string());
            }
        });
    }
    out
}

/// A footnote a writer's HTML writes with a label and no note takes the
/// note of the page's own footnote with that label (`page_notes`), so a
/// model that writes `<virtues-footnote label="ev-1"></virtues-footnote>`
/// back keeps its source. The browser's inline writer does the same
/// (`treeAiSession.ts`).
fn inherit_notes(nodes: &mut [Node], page_notes: &HashMap<String, String>) {
    for n in nodes {
        n.walk_mut(&mut |n| {
            if n.kind != "footnote" || n.attrs.get("note").is_some_and(|v| !v.is_null()) {
                return;
            }
            let note = n.attrs.get("label").and_then(Value::as_str).and_then(|l| page_notes.get(l));
            if let Some(note) = note {
                n.attrs.insert("note".into(), Value::String(note.clone()));
            }
        });
    }
}

/// Give every block that carries an id one, unique in `nodes`: an id
/// already present is kept unless an earlier block has it. A new document's
/// tree goes through this first, so no client has to invent ids for blocks
/// every other client is also looking at; nodes written into a document
/// that holds blocks are kept unique against those too
/// ([`crate::write_nodes`]).
pub fn ensure_ids(c: &Contract, nodes: &mut [Node]) {
    assign_ids(c, nodes, &mut HashSet::new(), None);
}

/// Inline content as units: one per character (with its marks), one per
/// inline atom (a mention), so a diff can step over a widget as one thing.
#[derive(Debug, Clone, PartialEq)]
enum Unit {
    Ch(char, Vec<Mark>),
    Atom(Node),
}

type Flat = Vec<Unit>;

fn flat(nodes: &[Node]) -> Flat {
    let mut v = vec![];
    for n in nodes {
        if !n.is_text() {
            v.push(Unit::Atom(n.clone()));
            continue;
        }
        let mut marks = n.marks.clone();
        marks.sort_by(|a, b| a.kind.cmp(&b.kind));
        for ch in n.text.as_deref().unwrap_or("").chars() {
            v.push(Unit::Ch(ch, marks.clone()));
        }
    }
    v
}

fn unflat(c: &Contract, v: &[Unit]) -> Vec<Node> {
    let mut out: Vec<Node> = vec![];
    for u in v {
        match u {
            Unit::Atom(n) => out.push(n.clone()),
            Unit::Ch(ch, marks) => match out.last_mut() {
                Some(Node {
                    text: Some(text),
                    marks: last_marks,
                    ..
                }) if last_marks == marks => text.push(*ch),
                _ => out.push(Node::text(&ch.to_string(), marks.clone())),
            },
        }
    }
    for n in &mut out {
        n.marks.sort_by_key(|m| c.mark_rank(&m.kind));
    }
    out
}

/// What HTML's whitespace rules read as one space, or as nothing.
fn is_ws(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\r' | '\n' | '\u{000c}')
}

/// `units` as HTML's whitespace rules read them back (`ingest`'s
/// `add_text`): a run of spaces, tabs and line ends is one space, and none
/// is kept at the block's start, after a hard break or at its end. Also,
/// for each unit kept, its index in `units`.
fn collapsed(units: &[Unit]) -> (Flat, Vec<usize>) {
    let mut out: Flat = vec![];
    let mut src = vec![];
    for (i, unit) in units.iter().enumerate() {
        if let Unit::Ch(ch, marks) = unit {
            if is_ws(*ch) {
                let dropped = match out.last() {
                    None | Some(Unit::Ch(' ', _)) => true,
                    Some(Unit::Atom(n)) => n.kind == "hardBreak",
                    Some(Unit::Ch(..)) => false,
                };
                if !dropped {
                    out.push(Unit::Ch(' ', marks.clone()));
                    src.push(i);
                }
                continue;
            }
        }
        out.push(unit.clone());
        src.push(i);
    }
    while matches!(out.last(), Some(Unit::Ch(' ', _))) {
        out.pop();
        src.pop();
    }
    (out, src)
}

/// The most cells [`aligned`] fills to align the stretch two texts differ
/// in: a paragraph's worth, both ways.
const ALIGN_CELLS: usize = 1_000_000;

/// The units `a` and `b` have in common, as pairs of indexes in order: the
/// longest common run of them. Past [`ALIGN_CELLS`], only the shared start
/// and end.
fn aligned(a: &[Unit], b: &[Unit]) -> Vec<(usize, usize)> {
    let mut pre = 0;
    while pre < a.len().min(b.len()) && a[pre] == b[pre] {
        pre += 1;
    }
    let mut suf = 0;
    while suf < (a.len() - pre).min(b.len() - pre) && a[a.len() - 1 - suf] == b[b.len() - 1 - suf] {
        suf += 1;
    }
    let (am, bm) = (&a[pre..a.len() - suf], &b[pre..b.len() - suf]);
    let mut pairs: Vec<(usize, usize)> = (0..pre).map(|k| (k, k)).collect();
    if !am.is_empty() && !bm.is_empty() && am.len().saturating_mul(bm.len()) <= ALIGN_CELLS {
        // lcs[i][j]: the longest common run of am[i..] and bm[j..].
        let w = bm.len() + 1;
        let mut lcs = vec![0u32; (am.len() + 1) * w];
        for i in (0..am.len()).rev() {
            for j in (0..bm.len()).rev() {
                lcs[i * w + j] = if am[i] == bm[j] {
                    lcs[(i + 1) * w + j + 1] + 1
                } else {
                    lcs[(i + 1) * w + j].max(lcs[i * w + j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < am.len() && j < bm.len() {
            if am[i] == bm[j] {
                pairs.push((pre + i, pre + j));
                i += 1;
                j += 1;
            } else if lcs[(i + 1) * w + j] >= lcs[i * w + j + 1] {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
    pairs.extend((0..suf).map(|k| (a.len() - suf + k, b.len() - suf + k)));
    pairs
}

/// The text `model`, a textblock written from a read of `read`, with the
/// spaces, tabs and line ends `read` holds wherever the model's text is
/// `read`'s as HTML reads it back: what the model changed is its own, the
/// rest as typed. None when `read` holds nothing HTML would collapse.
fn respaced(c: &Contract, read: &[Node], model: &[Node]) -> Option<Vec<Node>> {
    let typed = flat(read);
    let (seen, src) = collapsed(&typed);
    if seen.len() == typed.len() {
        return None;
    }
    let written = flat(model);
    let pairs = aligned(&seen, &written);
    // A unit HTML dropped goes with the unit kept before it; one before the
    // first goes with the block's start, one after the last with its end,
    // each kept only while the model kept that edge.
    let last = seen.len().saturating_sub(1);
    let group = |i: usize| src[i]..if i < last { src[i + 1] } else { src[i] + 1 };
    let mut out: Flat = vec![];
    if pairs.first() == Some(&(0, 0)) {
        out.extend_from_slice(&typed[..src[0]]);
    }
    let mut next = 0;
    for &(i, j) in &pairs {
        out.extend_from_slice(&written[next..j]);
        out.extend_from_slice(&typed[group(i)]);
        next = j + 1;
    }
    out.extend_from_slice(&written[next..]);
    if !seen.is_empty() && pairs.last() == Some(&(last, written.len().wrapping_sub(1))) {
        out.extend_from_slice(&typed[src[last] + 1..]);
    }
    Some(unflat(c, &out))
}

/// `model`, a block a writer sent to replace one it read as `read`, with
/// the spaces a person typed kept. A read shows text as the page holds it,
/// two spaces after a full stop and a tab included, but HTML's whitespace
/// rules read the copy sent back with each run of them one space: changing
/// one word would have changed every run in the paragraph, in inline code
/// too, where the spaces are what the code says. A code block's text is
/// read as written, and is left as sent. The blocks inside are paired by
/// id, or, with none (a table's cells), by place.
fn keep_typed_space(c: &Contract, read: &Node, model: &mut Node) {
    if read.kind != model.kind || c.node(&model.kind).is_some_and(|n| n.spec.code) {
        return;
    }
    if c.is_textblock(&model.kind) {
        if let Some(content) = respaced(c, &read.content, &model.content) {
            model.content = content;
        }
        return;
    }
    for (i, child) in model.content.iter_mut().enumerate() {
        let pair = match child.id().map(str::to_string) {
            Some(id) => read.content.iter().find(|r| r.id() == Some(id.as_str())),
            None => read.content.get(i).filter(|r| r.id().is_none()),
        };
        if let Some(read) = pair {
            keep_typed_space(c, read, child);
        }
    }
}

/// Why `parent` cannot hold `kinds`, the children an edit that wrote
/// `written` into it would leave: a kind it holds nowhere, named alone, or
/// else what it would hold, in short. A page or a list holds thousands of
/// blocks, and naming each would bury the one that does not fit past the
/// end of what a tool's result carries.
fn misfit(c: &Contract, parent: &str, kinds: &[&str], written: &[Node]) -> String {
    let expr = c
        .node(parent)
        .and_then(|n| n.spec.content.clone())
        .unwrap_or_default();
    let stray = written
        .iter()
        .map(|n| n.kind.as_str())
        .chain(kinds.iter().copied())
        .find(|kind| !c.may_contain(parent, kind));
    let Some(kind) = stray else {
        return format!(
            "after this edit `{parent}` would hold {}; it holds `{expr}`",
            in_short(kinds)
        );
    };
    let hint = match kind {
        "listItem" | "taskItem" => "; an item goes in a list: insert it after an item of one",
        "tableRow" => "; a row goes in a table: insert it after a row of one",
        "tableCell" | "tableHeader" => "; a cell goes in a row: replace the row it belongs to",
        _ if c.node(parent).is_some_and(|n| n.groups().any(|g| g == "list")) => {
            "; a block goes inside an item, or after the list"
        }
        _ => "",
    };
    format!("`{parent}` cannot hold `{kind}`; it holds `{expr}`{hint}")
}

/// `kinds` as a short list: a run of one kind once, with its count
/// (`paragraph ×40`), and a long list cut to its ends.
fn in_short(kinds: &[&str]) -> String {
    if kinds.is_empty() {
        return "nothing".to_string();
    }
    let mut runs: Vec<(&str, usize)> = vec![];
    for kind in kinds {
        match runs.last_mut() {
            Some((last, n)) if last == kind => *n += 1,
            _ => runs.push((kind, 1)),
        }
    }
    let said = |(kind, n): &(&str, usize)| match n {
        1 => kind.to_string(),
        n => format!("{kind} ×{n}"),
    };
    const SHOWN: usize = 6;
    if runs.len() <= SHOWN {
        return runs.iter().map(said).collect::<Vec<_>>().join(", ");
    }
    let head: Vec<String> = runs[..SHOWN / 2].iter().map(said).collect();
    let tail: Vec<String> = runs[runs.len() - SHOWN / 2..].iter().map(said).collect();
    format!("{}, …, {} ({} blocks)", head.join(", "), tail.join(", "), kinds.len())
}

/// The one changed span between two sequences: (prefix, old end, new end).
fn span(a: &Flat, b: &Flat) -> (usize, usize, usize) {
    let mut pre = 0;
    while pre < a.len().min(b.len()) && a[pre] == b[pre] {
        pre += 1;
    }
    let mut suf = 0;
    while suf < (a.len() - pre).min(b.len() - pre) && a[a.len() - 1 - suf] == b[b.len() - 1 - suf] {
        suf += 1;
    }
    (pre, a.len() - suf, b.len() - suf)
}

/// The block changed between the model's read and now. Three-way merge of
/// one text block: the model's change and the person's change both apply
/// when they touch different stretches of the text, or different
/// attributes; otherwise refuse. An attribute both changed (a heading's
/// level, a code block's language) is the person's, unless both changed it
/// to the same.
fn merge_block(c: &Contract, base: &Node, current: &Node, model: &Node) -> Result<Node, String> {
    let same_shape =
        base.kind == current.kind && current.kind == model.kind && c.is_textblock(&model.kind);
    if !same_shape {
        return Err("the block changed since you read it; read it again".into());
    }
    let mut attrs = current.attrs.clone();
    let keys: std::collections::BTreeSet<&String> =
        base.attrs.keys().chain(current.attrs.keys()).chain(model.attrs.keys()).collect();
    for k in keys {
        let (b, now, m) = (base.attrs.get(k), current.attrs.get(k), model.attrs.get(k));
        if m == b || m == now {
            continue;
        }
        if now != b {
            let say = |v: Option<&Value>| v.map_or_else(|| "nothing".to_string(), Value::to_string);
            return Err(format!(
                "someone changed the block's {k} since you read it, from {} to {}; write the op from the block as it is now",
                say(b),
                say(now)
            ));
        }
        match m {
            Some(v) => attrs.insert(k.clone(), v.clone()),
            None => attrs.remove(k),
        };
    }
    let (b, cur, m) = (
        flat(&base.content),
        flat(&current.content),
        flat(&model.content),
    );
    let (ms, me_base, me_model) = span(&b, &m);
    let (hs, he_base, he_cur) = span(&b, &cur);
    if ms == me_base && ms == me_model {
        return Ok(Node::element(&model.kind, attrs, current.content.clone()));
    }
    if hs == he_base && hs == he_cur {
        return Ok(Node::element(&model.kind, attrs, model.content.clone()));
    }
    // Compare whole words: two edits touching the same word conflict even
    // when their characters do not overlap ("slow" → "sly" vs "slow" → "slower").
    let word = |i: usize| matches!(b.get(i), Some(Unit::Ch(ch, _)) if ch.is_alphanumeric());
    let widen = |mut s: usize, mut e: usize| {
        while s > 0 && word(s - 1) {
            s -= 1;
        }
        while word(e) {
            e += 1;
        }
        (s, e)
    };
    let (m0, m1) = widen(ms, me_base);
    let (h0, h1) = widen(hs, he_base);
    let disjoint = m1 <= h0 || h1 <= m0;
    if !disjoint {
        let now: String = unflat(c, &cur).iter().map(Node::text_content).collect();
        return Err(format!(
            "someone is editing the same words right now; the block reads \"{now}\""
        ));
    }
    // Apply the model's span to the current text, shifted past the person's edit.
    let shift = he_cur as isize - he_base as isize;
    let (from, to) = if ms >= he_base {
        (
            (ms as isize + shift) as usize,
            (me_base as isize + shift) as usize,
        )
    } else {
        (ms, me_base)
    };
    let mut merged = cur.clone();
    merged.splice(from..to, m[ms..me_model].iter().cloned());
    Ok(Node::element(&model.kind, attrs, unflat(c, &merged)))
}

/// The node with this block id, anywhere in the tree.
pub fn find_node<'a>(nodes: &'a [Node], id: &str) -> Option<&'a Node> {
    for n in nodes {
        if n.id() == Some(id) {
            return Some(n);
        }
        if let Some(hit) = find_node(&n.content, id) {
            return Some(hit);
        }
    }
    None
}

fn strip_ids_deep(c: &Contract, n: &Node) -> Node {
    let mut n = n.clone();
    n.walk_mut(&mut |x| {
        x.attrs.remove(&c.id.attr);
    });
    n
}

/// Apply a batch in its own transaction. See [`apply_in`].
pub fn apply(
    c: &Contract,
    doc: &Doc,
    base: Option<&Seen>,
    ops: &[Op],
) -> Result<Applied, Vec<Problem>> {
    let mut txn = doc.transact_mut();
    apply_in(c, &mut txn, base, ops)
}

/// Apply a batch inside the caller's transaction, which makes the read and
/// the write one step: nothing can change the document between them.
///
/// `base`: what reads have shown the writer of the page, when it says. A
/// batch with one takes off the page, or writes over, only what the base
/// has seen as it stands ([`check`]): a text block someone changed since it
/// was shown is merged with the edit or refused, and any other block that
/// changed since, or that no read showed, is refused. A block too long for
/// any read goes only by an op deleting it. Without a base, a replace wins
/// over whatever the block holds now.
pub fn apply_in(
    c: &Contract,
    txn: &mut TransactionMut,
    base: Option<&Seen>,
    ops: &[Op],
) -> Result<Applied, Vec<Problem>> {
    apply_checked(c, txn, base, ops, false)
}

/// [`apply_in`] for ops the server made from a tree of its own (a version
/// put back, as [`crate::diff_ops`] makes them), which no write's size limit
/// bounds: a page grows past any one write, and putting a cleared page back
/// writes all of it. Every other check stands: the contract, the grid, the
/// node count and the depth.
pub fn apply_own_in(c: &Contract, txn: &mut TransactionMut, ops: &[Op]) -> Result<Applied, Vec<Problem>> {
    apply_checked(c, txn, None, ops, true)
}

/// What [`apply_in`] refuses of a batch over the page `tree` with the base
/// `base`, checked on a copy, with nothing written: every problem, and the
/// blocks behind each one about what the base has not seen as it stands,
/// for the writer's next step to show or name.
pub fn check(c: &Contract, tree: &[Node], base: &Seen, ops: &[Op]) -> Checked {
    let planned = plan_batch(c, tree.to_vec(), Some(base), ops, false);
    Checked {
        problems: planned.errors,
        unseen: planned.unseen,
        conflicts: planned.conflicts,
    }
}

/// See [`check`].
#[derive(Debug, Default)]
pub struct Checked {
    pub problems: Vec<Problem>,
    pub unseen: Vec<Unseen>,
    /// The replaces (by op, from 0) refused because their block changed
    /// since the read in a way the edit could not be merged with: over the
    /// block, as an unseen one is, not over the op's HTML.
    pub conflicts: Vec<usize>,
}

fn apply_checked(
    c: &Contract,
    txn: &mut TransactionMut,
    base: Option<&Seen>,
    ops: &[Op],
    own: bool,
) -> Result<Applied, Vec<Problem>> {
    let planned = plan_batch(c, read_doc(c, &*txn), base, ops, own);
    if !planned.errors.is_empty() {
        return Err(planned.errors);
    }
    let mut applied = planned.applied;
    let frag = ydoc::fragment(c, txn);
    for plan in planned.plans {
        if let Some(how) = write_plan(c, txn, &frag, plan) {
            applied.replaced_as.push(how.to_string());
        }
    }
    Ok(applied)
}

/// A batch worked out on a copy of the page: what to write, or why not.
#[derive(Default)]
struct Planned {
    plans: Vec<Plan>,
    errors: Vec<Problem>,
    applied: Applied,
    unseen: Vec<Unseen>,
    conflicts: Vec<usize>,
}

fn plan_batch(c: &Contract, page: Vec<Node>, base: Option<&Seen>, ops: &[Op], own: bool) -> Planned {
    let mut planned = Planned::default();
    let html_bytes: usize = ops
        .iter()
        .map(|op| match op {
            Op::Replace { html, .. }
            | Op::InsertAfter { html, .. }
            | Op::InsertBefore { html, .. }
            | Op::Append { html } => html.len(),
            Op::Delete { .. } => 0,
        })
        .sum();
    if html_bytes > MAX_INPUT_BYTES && !own {
        // Each op's HTML is within the limit on its own; the batch is one
        // call, and one call reads at most the limit.
        planned.errors.push(Problem::new(
            "ops",
            format!(
                "the ops hold {html_bytes} bytes of HTML; one write holds at most {} MiB",
                MAX_INPUT_BYTES >> 20
            ),
        ));
        return planned;
    }
    let before = page;
    let page_notes = notes_by_label(&before);
    let mut root = Node::element(&c.fragment, Default::default(), before.clone());
    let then = crate::seen::index(&before);
    let nodes_before = crate::node_count(&root.content);
    let mut taken = HashSet::new();
    collect_ids(&root.content, &mut taken);
    let errors = &mut planned.errors;
    let applied = &mut planned.applied;
    let plans = &mut planned.plans;
    // The tables the batch edits rows of, by id, and the last op to: each is
    // checked for a grid once the batch is done.
    let mut tables: Vec<(String, String)> = vec![];
    // Which ops reached the copy of the page, and the ids each wrote.
    let mut reached = vec![false; ops.len()];
    let mut wrote: Vec<(usize, HashSet<String>)> = vec![];
    // Each text block a replace merged with a change made since, as merged.
    let mut merged: HashMap<String, Node> = HashMap::new();
    // What the batch writes that a block shown only as markdown could come
    // back as.
    let mut from_markdown = FromMarkdown::default();
    // What a block taken out left its parent unable to hold, judged once the
    // batch is done: a later op may take the parent too (an item's only
    // paragraph deleted, then the item), as a refusal's advice to delete a
    // block in an op before one around it asks.
    let mut emptied: Vec<Emptied> = vec![];

    for (i, op) in ops.iter().enumerate() {
        let at = format!("op {}", i + 1);
        let (target, html, kind) = match op {
            Op::Replace { id, html } => (Some(id), Some(html), PlanKind::Replace),
            Op::InsertAfter { id, html } => (Some(id), Some(html), PlanKind::InsertAfter),
            Op::InsertBefore { id, html } => (Some(id), Some(html), PlanKind::InsertBefore),
            Op::Append { html } => (None, Some(html), PlanKind::Append),
            Op::Delete { id } => (Some(id), None, PlanKind::Delete),
        };
        let path = match target {
            Some(id) => {
                let mut p = vec![];
                if !find_path(&root.content, id, &mut p) {
                    errors.push(Problem::new(&at, format!("no block has id `{id}`")));
                    continue;
                }
                p
            }
            None => vec![root.content.len()],
        };
        let parent_kind = parent_mut(&mut root, &path).kind.clone();
        let mut nodes = vec![];
        if let Some(html) = html {
            let o = if own {
                crate::ingest::parse_own_slice(c, html, &parent_kind)
            } else {
                parse_slice(c, html, &parent_kind, Mode::Strict)
            };
            if !o.errors.is_empty() {
                errors.extend(
                    o.errors
                        .into_iter()
                        .map(|e| Problem::new(format!("{at}: {}", e.at), e.message)),
                );
                continue;
            }
            // Only an empty replace takes its block out: HTML that reads as
            // nothing (a comment, an empty tag) was meant to write something.
            if o.nodes.is_empty() && (!matches!(kind, PlanKind::Replace) || !html.trim().is_empty()) {
                let msg = match kind {
                    PlanKind::Replace => "the html holds no blocks; to take the block out, delete it",
                    _ => "the html holds no blocks",
                };
                errors.push(Problem::new(&at, msg));
                continue;
            }
            applied.notes.extend(
                o.notes
                    .into_iter()
                    .map(|n| Problem::new(format!("{at}: {}", n.at), n.message)),
            );
            nodes = o.nodes;
            if !own {
                inherit_notes(&mut nodes, &page_notes);
            }
            let keep = if matches!(kind, PlanKind::Replace) {
                target.map(String::as_str)
            } else {
                None
            };
            if let (Some(k), [model]) = (keep, nodes.as_mut_slice()) {
                if let Some(old) = find_node(&root.content, k) {
                    let mut written = HashSet::new();
                    collect_ids(std::slice::from_ref(model), &mut written);
                    // A block around it the base holds lends only the
                    // blocks a read showed in it.
                    let lends = |holder: Option<&str>, child: &str| {
                        base.and_then(|s| s.known(holder?)).is_none_or(|k| k.shows(child))
                    };
                    inherit_ids(c, old, model, &written, None, &lends);
                }
                // The block as the writer read it: the base's, or the page's
                // when the batch names none or no read showed it.
                let read = base
                    .and_then(|s| s.subtree(k))
                    .or_else(|| find_node(&root.content, k).cloned());
                if let (Some(read), false) = (read, own) {
                    keep_typed_space(c, &read, model);
                }
            }
            if let Some(old) = keep.and_then(|k| find_node(&root.content, k)) {
                let mut freed = HashSet::new();
                collect_ids(std::slice::from_ref(old), &mut freed);
                for id in &freed {
                    taken.remove(id);
                }
            }
            assign_ids(c, &mut nodes, &mut taken, keep);
        }

        // A replace of a block someone changed since the writer read it: a
        // text block is merged with the change, and any other block shown
        // whole is refused. What else the batch takes or writes over is
        // judged once the batch is done (`effects`).
        if let (Some(seen), Some(id), PlanKind::Replace) = (base, target, &kind) {
            if let Some(now) = then.get(id.as_str()) {
                match concurrent(c, seen, now, &nodes) {
                    Ok(Some(mut m)) => {
                        m.attrs.insert(c.id.attr.clone(), Value::String(id.clone()));
                        applied
                            .notes
                            .push(Problem::new(&at, "merged with a concurrent edit to this block"));
                        merged.insert(id.clone(), m.clone());
                        nodes = vec![m];
                    }
                    Ok(None) => {}
                    Err(Refused::Said(message)) => {
                        errors.push(Problem::new(&at, message));
                        planned.conflicts.push(i);
                        continue;
                    }
                    Err(Refused::Unseen(found)) => {
                        for mut u in found {
                            u.op = i;
                            errors.push(Problem::new(&at, u.message()));
                            planned.unseen.push(u);
                        }
                        continue;
                    }
                }
            }
        }

        let idx = *path.last().expect("a path names at least the index");
        let place = nodes.is_empty().then(|| anchor_of(&root, &path[..path.len() - 1]));
        let parent = parent_mut(&mut root, &path);
        match kind {
            PlanKind::Replace => {
                parent.content.splice(idx..=idx, nodes.clone());
            }
            PlanKind::InsertAfter => {
                parent.content.splice(idx + 1..idx + 1, nodes.clone());
            }
            PlanKind::InsertBefore | PlanKind::Append => {
                parent.content.splice(idx..idx, nodes.clone());
            }
            PlanKind::Delete => {
                parent.content.remove(idx);
            }
        }
        reached[i] = true;
        let mut ids = HashSet::new();
        collect_ids(&nodes, &mut ids);
        wrote.push((i, ids));
        from_markdown.add(&nodes);
        let kinds: Vec<&str> = parent.content.iter().map(|n| n.kind.as_str()).collect();
        if !c.content_matches(&parent.kind, &kinds) {
            if let Some((holder, steps)) = place {
                emptied.push(Emptied { at: at.clone(), holder, steps });
            } else {
                errors.push(Problem::new(&at, misfit(c, &parent.kind, &kinds, &nodes)));
                continue;
            }
        }
        // A row added, replaced or taken out must leave the table a grid,
        // or the editor's table plugin would rewrite it on the next open.
        // Only the batch's end reaches the page, so the grid is checked
        // there: a column is added by replacing every row in one batch, each
        // row on its own being a row too wide.
        if parent.kind == "table" {
            if let Some(id) = parent.id() {
                tables.retain(|(t, _)| t != id);
                tables.push((id.to_string(), at.clone()));
            }
        }
        for n in &nodes {
            if let Some(id) = n.id() {
                applied.written.push(id.to_string());
            }
        }
        plans.push(Plan {
            target: target.cloned(),
            kind,
            nodes,
        });
    }

    for e in &emptied {
        let holder = match &e.holder {
            Some(id) => find_node(&root.content, id),
            None => Some(&root),
        };
        // Gone: a later op took the parent with a block around it.
        let Some(mut parent) = holder else { continue };
        let mut there = true;
        for &i in &e.steps {
            match parent.content.get(i) {
                Some(n) => parent = n,
                None => there = false,
            }
            if !there {
                break;
            }
        }
        if !there {
            continue;
        }
        let kinds: Vec<&str> = parent.content.iter().map(|n| n.kind.as_str()).collect();
        if !c.content_matches(&parent.kind, &kinds) {
            errors.push(Problem::new(&e.at, misfit(c, &parent.kind, &kinds, &[])));
        }
    }

    for (id, at) in &tables {
        let Some(table) = find_node(&root.content, id) else {
            continue;
        };
        let wrong = crate::table::problems(table);
        if !wrong.is_empty() {
            // Every row in one batch is no way for a table of more rows than
            // a batch holds ops.
            let how = if table.content.len() > MAX_OPS {
                format!(
                    "To add or remove a column of a table of more than {MAX_OPS} rows, replace the \
                     table whole: one batch holds {MAX_OPS} ops, too few for a replace of every row"
                )
            } else {
                "To add or remove a column, replace every row in one batch, or replace the table whole"
                    .to_string()
            };
            errors.push(Problem::new(
                at,
                format!("after this edit the table would not be a grid: {}. {how}", wrong.join("; ")),
            ));
        }
    }
    if errors.is_empty() {
        let nodes_after = crate::node_count(&root.content);
        if nodes_after > crate::MAX_NODES && nodes_after > nodes_before {
            errors.push(Problem::new("ops", crate::model::too_many_nodes()));
        }
    }
    if errors.is_empty() && depth(&root.content) > MAX_DEPTH {
        // Each op's HTML is within the limit on its own; placed deep in the
        // page it can still take the page past it.
        errors.push(Problem::new("ops", too_deep()));
    }

    // A merged block a later op wrote over again (a replace of a block
    // around it, written from the writer's copy) holds that op's copy, not
    // the merge, and is judged as written over, as any block is.
    merged.retain(|id, m| find_node(&root.content, id).is_some_and(|n| strip_ids_deep(c, n) == strip_ids_deep(c, m)));
    let merged: HashSet<String> = merged.into_keys().collect();

    // What the batch would take or write over, judged on the page it leaves.
    if let Some(seen) = base {
        let mut after = root.content.clone();
        crate::ydoc::canonicalize(c, &mut after);
        let refused: HashSet<String> = errors.iter().map(|p| p.at.clone()).collect();
        let done = Done {
            ops,
            reached: &reached,
            wrote: &wrote,
            merged: &merged,
            from_markdown: &from_markdown,
        };
        for u in effects(seen, &before, &after, &done) {
            let at = format!("op {}", u.op + 1);
            if refused.contains(&at) {
                continue;
            }
            errors.push(Problem::new(at, u.message()));
            planned.unseen.push(u);
        }
    }
    planned.applied.merged = merged.into_iter().collect();
    planned.applied.merged.sort();
    planned
}

/// Why [`concurrent`] refused a replace.
enum Refused {
    Said(String),
    Unseen(Vec<Unseen>),
}

/// A replace of `now`, a block on the page, by `nodes`, checked against
/// what the writer read of it: a text block changed since is merged with
/// the writer's edit (`Some`), or refused when the two touch the same
/// words; any other block a read showed whole is refused when it changed
/// since, naming the blocks inside it that did, since its writer rewrote
/// it from a copy that lacks the change, and writing that copy back would
/// undo it (a deleted item put back, a reorder undone). A block not read
/// whole is left to [`effects`].
fn concurrent(c: &Contract, seen: &Seen, now: &Node, nodes: &[Node]) -> Result<Option<Node>, Refused> {
    let Some(id) = now.id() else {
        return Ok(None);
    };
    if c.is_textblock(&now.kind) {
        let Some(then) = seen.subtree(id) else {
            return Ok(None);
        };
        let (then, now) = (strip_ids_deep(c, &then), strip_ids_deep(c, now));
        if then == now {
            return Ok(None);
        }
        return match nodes {
            [model] => merge_block(c, &then, &now, &strip_ids_deep(c, model))
                .map(Some)
                .map_err(Refused::Said),
            _ => Err(Refused::Unseen(vec![Unseen::one(id, Why::Changed {
                deleted: vec![],
                added: vec![],
            })])),
        };
    }
    if !seen.complete_over(now) {
        return Ok(None);
    }
    if seen.subtree(id).as_ref() == Some(now) {
        return Ok(None);
    }
    let mut found = vec![];
    now.walk(&mut |b| {
        let Some(bid) = b.id() else { return };
        let Some(known) = seen.known(bid) else { return };
        let shell = crate::seen::shell(b);
        if known.shell != shell && found.len() < MAX_CHANGED_SHOWN {
            let was = crate::seen::stubs(&known.shell);
            let is = crate::seen::stubs(&shell);
            let mut u = Unseen::one(bid, Why::Changed {
                deleted: was.iter().filter(|w| !is.contains(w)).map(|s| s.to_string()).collect(),
                added: is.iter().filter(|s| !was.contains(s)).map(|s| s.to_string()).collect(),
            });
            u.target = bid == id;
            found.push(u);
        }
    });
    if found.is_empty() {
        found.push(Unseen::one(id, Why::Changed {
            deleted: vec![],
            added: vec![],
        }));
    }
    Err(Refused::Unseen(found))
}

/// The most blocks one op is refused over for having changed since they
/// were read: each comes with the refusal to be read again, which holds
/// only so much.
const MAX_CHANGED_SHOWN: usize = 8;

/// A block an op would take off the page, or write over, that the base has
/// not seen as it stands ([`check`]), or a run of such blocks beside each
/// other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unseen {
    /// The op (from 0) the problem is charged to: the last op naming the
    /// block or a block around it.
    pub op: usize,
    /// The block, or the run's blocks in page order.
    pub ids: Vec<String>,
    /// The block around them, when a read has shown it.
    pub inside: Option<String>,
    /// The block before the first of them, beside it.
    pub after: Option<String>,
    /// Whether the op takes them off the page, or writes over them.
    pub removes: bool,
    /// Whether the block is the op's own (`ids` is it alone).
    pub target: bool,
    pub why: Why,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Why {
    /// A read showed it, and it has changed since: `deleted` and `added`
    /// name the blocks inside it gone or new since.
    Changed { deleted: Vec<String>, added: Vec<String> },
    /// Shown only as markdown, which does not say what its HTML is.
    MarkdownOnly,
    /// No read showed it. `added_since`: the block around it, as a read
    /// showed it, did not hold it.
    Unread { added_since: bool },
    /// Listed as too long for any read, and changed since it was listed.
    ListedChanged,
    /// Too long for any read, and taken or written over by an op on a block
    /// around it: only an op of its own takes it.
    NamedInside,
    /// Deleted after the read, and written back by its id.
    Deleted,
}

impl Unseen {
    fn one(id: &str, why: Why) -> Self {
        Unseen {
            op: 0,
            ids: vec![id.to_string()],
            inside: None,
            after: None,
            removes: false,
            target: true,
            why,
        }
    }

    /// The first block.
    pub fn id(&self) -> &str {
        &self.ids[0]
    }

    /// What is wrong, naming no tool: the writer's tool says what to do.
    pub fn message(&self) -> String {
        let id = self.id();
        let count = self.ids.len();
        let verb = if self.removes { "remove" } else { "write over" };
        let inside = self.inside.as_deref().map(|p| format!(" inside `{p}`")).unwrap_or_default();
        match &self.why {
            Why::Changed { deleted, added } => {
                let mut said = format!("the block `{id}` changed since you read it");
                if !deleted.is_empty() {
                    said.push_str(&format!(
                        "; {} inside it {} deleted since",
                        names(deleted),
                        if deleted.len() == 1 { "was" } else { "were" }
                    ));
                }
                if !added.is_empty() {
                    said.push_str(&format!(
                        "; {} {} added to it since",
                        names(added),
                        if added.len() == 1 { "was" } else { "were" }
                    ));
                }
                said
            }
            Why::MarkdownOnly if count == 1 => format!(
                "the block `{id}` was shown only as markdown, which does not say what its HTML is \
                 (a mention reads as a link, audio as an image, a callout as a quote)"
            ),
            Why::MarkdownOnly => format!(
                "this op would {verb} {count} blocks shown only as markdown, from `{id}`{inside}; \
                 markdown does not say what a block's HTML is (a mention reads as a link, audio as \
                 an image, a callout as a quote)"
            ),
            Why::Unread { added_since: false } if self.target => {
                format!("the block `{id}` is not in the read that base names")
            }
            Why::Unread { added_since } => match (count, added_since, self.inside.as_deref()) {
                (1, true, Some(p)) => format!("this op would {verb} `{id}`, added to `{p}` after you read it"),
                (1, ..) => format!("this op would {verb} `{id}`, which no read has shown you"),
                (_, true, Some(_)) => {
                    format!("this op would {verb} {count} blocks added after you read the page, from `{id}`{inside}")
                }
                _ => format!("this op would {verb} {count} blocks that no read has shown you, from `{id}`{inside}"),
            },
            Why::ListedChanged => {
                format!("the block `{id}` is too long for any read, and changed since it was listed")
            }
            Why::NamedInside if self.removes => format!(
                "this op would remove `{id}`, which is too long for any read: only an op deleting it \
                 takes it, and a comment that stands for it writes nothing"
            ),
            Why::NamedInside => format!(
                "this op would write over `{id}`, which is too long for any read: only an op naming \
                 it writes over it, and a comment that stands for it writes nothing"
            ),
            Why::Deleted => format!("this op writes `{id}` back, which was deleted after you read the page"),
        }
    }
}

/// Whether `nodes` hold a link to a ref on the box (`/person/…`): what a
/// mention shown as markdown (`[@Nick](/person/person_1)`) comes back as
/// when written from it.
fn links_to_a_ref(nodes: &[Node]) -> bool {
    let mut found = false;
    for n in nodes {
        n.walk(&mut |n| {
            found |= n.marks.iter().any(|m| {
                m.kind == "link"
                    && m.attrs
                        .get("href")
                        .and_then(Value::as_str)
                        .is_some_and(crate::contract::is_route)
            });
        });
    }
    found
}

/// `` `a`, `b` ``.
fn names(ids: &[String]) -> String {
    ids.iter().map(|id| format!("`{id}`")).collect::<Vec<_>>().join(", ")
}

/// The ids of `node` and every block inside it.
fn block_ids<'a>(node: &'a Node, out: &mut Vec<&'a str>) {
    if let Some(id) = node.id() {
        out.push(id);
    }
    for c in &node.content {
        block_ids(c, out);
    }
}

/// Where a block is on the page before a batch.
struct Place<'a> {
    node: &'a Node,
    /// The nearest block around it.
    parent: Option<&'a str>,
    /// The block before it in what holds it (a block, or a table cell).
    prev: Option<&'a str>,
    /// What holds it, numbered: blocks beside each other share one.
    list: usize,
}

fn places<'a>(
    nodes: &'a [Node],
    parent: Option<&'a str>,
    lists: &mut usize,
    out: &mut HashMap<&'a str, Place<'a>>,
    order: &mut Vec<&'a str>,
) {
    let list = *lists;
    *lists += 1;
    let mut prev = None;
    for n in nodes {
        match n.id() {
            Some(id) => {
                out.insert(id, Place { node: n, parent, prev, list });
                order.push(id);
                places(&n.content, Some(id), lists, out, order);
                prev = Some(id);
            }
            None if !n.is_text() => places(&n.content, parent, lists, out, order),
            None => {}
        }
    }
}

/// What a batch did on its copy of the page, for [`effects`].
struct Done<'a> {
    ops: &'a [Op],
    /// Which ops reached the copy.
    reached: &'a [bool],
    /// The ids each op that reached it wrote.
    wrote: &'a [(usize, HashSet<String>)],
    /// Text blocks a replace merged with a change made since the read, each
    /// holding the merge at the batch's end.
    merged: &'a HashSet<String>,
    from_markdown: &'a FromMarkdown,
}

/// What a batch writes that a block shown only as markdown could come back
/// as, written from that markdown: a link to a ref (`/person/…`, as a
/// mention reads), a file's address in another block (audio, a video and a
/// file read as images), a quote opening with `[!TONE]` (as a callout
/// reads), text holding `[^` (as a footnote's marker reads, its note left
/// behind), a code block in the language `figure` (as a figure reads).
#[derive(Default)]
struct FromMarkdown {
    links_route: bool,
    srcs: HashSet<String>,
    alert_quote: bool,
    marker_text: bool,
    figure_code: bool,
}

impl FromMarkdown {
    fn add(&mut self, nodes: &[Node]) {
        self.links_route |= links_to_a_ref(nodes);
        for n in nodes {
            n.walk(&mut |n| {
                if let Some(src) = n.attrs.get("src").and_then(Value::as_str) {
                    self.srcs.insert(src.to_string());
                }
                if n.kind == "blockquote" && n.text_content().trim_start().starts_with("[!") {
                    self.alert_quote = true;
                }
                if n.text.as_deref().is_some_and(|t| t.contains("[^")) {
                    self.marker_text = true;
                }
                if n.kind == "codeBlock" && n.attrs.get("language").and_then(Value::as_str) == Some("figure") {
                    self.figure_code = true;
                }
            });
        }
    }

    /// Whether `block`, shown only as markdown and taken off the page,
    /// could be what the batch writes back elsewhere from that markdown, as
    /// a block of another kind. An image's markdown says all an image is. A
    /// veil written back as its brackets is: it reads the same everywhere.
    fn could_be(&self, block: &Node) -> bool {
        if self.links_route {
            return true;
        }
        let mut found = false;
        block.walk(&mut |n| {
            found |= n.kind != "image"
                && n.attrs.get("src").and_then(Value::as_str).is_some_and(|src| self.srcs.contains(src));
            found |= self.alert_quote && n.kind == "callout";
            found |= self.marker_text && n.kind == "footnote";
            found |= self.figure_code && n.kind == "figure";
        });
        found
    }
}

/// What a batch takes off the page, or writes over, that the base has not
/// seen as it stands, judged by its effect: every block of the page before
/// it (`before`) against the page it leaves (`after`). A block it takes is
/// one a read showed as it stands, or one listed as too long for any read,
/// unchanged since, that an op deletes by name; taken with a block around
/// it, it is also one a read showed inside that block, or one an op deletes
/// by name, so a block moved there since (an item indented, a paragraph
/// joined into a quote) is not taken on the strength of a read that showed
/// it elsewhere, nor on a read of that block that counted it without
/// showing it. A replace naming a block that writes something means the
/// block to stay, and takes nothing. A block listed as too long for any
/// read, as it stands, goes only by an op deleting it, whatever a markdown
/// read showed of it. A block it changes
/// is one a read showed as it stands as HTML, a text block merged with a
/// change made since, or one listed, unchanged, that a replace names. A
/// block written back by the id of one deleted since the read is refused. The batch's own
/// churn (a block it inserted, then replaced) is not seen: only the page
/// before and after. Each problem is charged to the last op naming the
/// block or a block around it; a block inside one refused for the same
/// reason by the same op is that one's, and blocks beside each other make
/// one problem.
fn effects(seen: &Seen, before: &[Node], after: &[Node], done: &Done) -> Vec<Unseen> {
    use crate::seen::{node_hash, own, shell, stubs};
    let Done {
        ops,
        reached,
        wrote,
        merged,
        from_markdown,
    } = *done;
    let mut at: HashMap<&str, Place> = HashMap::new();
    let mut order = vec![];
    places(before, None, &mut 0, &mut at, &mut order);
    let now = crate::seen::index(after);

    // The op each block of the page is charged to, the blocks an op takes
    // off the page by name (a delete, a replace with nothing) and those it
    // writes over by name. A replace that writes something means its block
    // to stay: it is no word to take that block with a block around it.
    let mut charged: HashMap<&str, usize> = HashMap::new();
    let mut deleted: HashSet<&str> = HashSet::new();
    let mut replaced: HashSet<&str> = HashSet::new();
    for (i, op) in ops.iter().enumerate() {
        let (id, replace) = match op {
            Op::Replace { id, html } => (id, !html.trim().is_empty()),
            Op::Delete { id } => (id, false),
            _ => continue,
        };
        if !reached.get(i).copied().unwrap_or(false) {
            continue;
        }
        if replace {
            replaced.insert(id.as_str());
        } else {
            deleted.insert(id.as_str());
        }
        if let Some(place) = at.get(id.as_str()) {
            let mut inside = vec![];
            block_ids(place.node, &mut inside);
            for bid in inside {
                charged.insert(bid, i);
            }
        }
    }

    #[derive(Clone, PartialEq)]
    struct Verdict {
        op: usize,
        why: Why,
        removes: bool,
    }
    let added_since = |id: &str| {
        at.get(id)
            .and_then(|p| p.parent)
            .and_then(|parent| seen.known(parent))
            .is_some_and(|k| !stubs(&k.shell).contains(&id))
    };
    // Named by the block around it, as read, without a read showing it
    // there (counted in `<!-- 74 more -->`), and shown elsewhere: it came
    // there since that read too, as far as any text showed the writer.
    let moved_in_unshown = |id: &str| {
        seen.knows(id)
            && at
                .get(id)
                .and_then(|p| p.parent)
                .and_then(|parent| seen.known(parent))
                .is_some_and(|k| k.unshown.contains(id))
    };
    let mut verdicts: HashMap<&str, Verdict> = HashMap::new();
    for &id in &order {
        let Some(&op) = charged.get(id) else { continue };
        let was = at[id].node;
        let removes = !now.contains_key(id);
        if !removes && own(now[id]) == own(was) {
            continue;
        }
        let markdown = seen.known(id).is_some_and(|k| k.form == crate::seen::Form::Markdown);
        let listed_as_it_stands = seen.opaque_hash(id).is_some_and(|hash| hash == node_hash(was));
        let by_name = (removes && deleted.contains(id)) || (!removes && replaced.contains(id));
        let why = if removes && !deleted.contains(id) && (added_since(id) || moved_in_unshown(id)) {
            // Taken with the block around it, which no read showed holding
            // it: added there or moved there since, whatever a read showed
            // of it where it was. The model saw it elsewhere, so the op
            // would take it unawares.
            Why::Unread { added_since: true }
        } else if listed_as_it_stands {
            // Listed as too long for any read, as it stands: an op deleting
            // it takes it and one replacing it writes over it, and nothing
            // else does either, whatever a markdown read showed of it, which
            // says nothing an HTML read could.
            if by_name {
                continue;
            }
            Why::NamedInside
        } else if merged.contains(id) && !removes {
            if !markdown {
                continue;
            }
            Why::MarkdownOnly
        } else if seen.seen_as_it_stands(was) {
            // Taking a block off the page needs no word of its HTML, unless
            // the batch writes it back elsewhere from its markdown, as a
            // block of another kind: a mention shown as markdown comes back
            // as a link, audio as an image, a callout as a quote.
            if !markdown || (removes && !from_markdown.could_be(was)) {
                continue;
            }
            Why::MarkdownOnly
        } else if seen.opaque_hash(id).is_some() {
            // Listed, and changed since.
            Why::ListedChanged
        } else if let Some(known) = seen.known(id) {
            let shown = shell(was);
            let (then, is) = (stubs(&known.shell), stubs(&shown));
            Why::Changed {
                deleted: then.iter().filter(|t| !is.contains(t)).map(|s| s.to_string()).collect(),
                added: is.iter().filter(|s| !then.contains(s)).map(|s| s.to_string()).collect(),
            }
        } else {
            Why::Unread {
                added_since: added_since(id),
            }
        };
        verdicts.insert(id, Verdict { op, why, removes });
    }

    // A block no read showed, inside blocks no read showed either, all in
    // what one op writes over: the problem is the outermost of them, which
    // the op took or wrote over unread.
    let unread: Vec<&str> = order
        .iter()
        .copied()
        .filter(|id| verdicts.get(id).is_some_and(|v| matches!(v.why, Why::Unread { .. })))
        .collect();
    for id in unread {
        let op = verdicts[id].op;
        let mut cur = id;
        while let Some(parent) = at[cur].parent {
            if seen.knows(parent) || seen.opaque_hash(parent).is_some() || charged.get(parent) != Some(&op) {
                break;
            }
            verdicts.entry(parent).or_insert(Verdict {
                op,
                why: Why::Unread {
                    added_since: added_since(parent),
                },
                removes: !now.contains_key(parent),
            });
            cur = parent;
        }
    }

    let same = |a: &Why, b: &Why| std::mem::discriminant(a) == std::mem::discriminant(b);
    let mut out: Vec<Unseen> = vec![];
    // The open run of each (op, holder, reason, removes), as an index in out.
    let mut runs: HashMap<(usize, usize, std::mem::Discriminant<Why>, bool), usize> = HashMap::new();
    for &id in &order {
        let Some(v) = verdicts.get(id) else { continue };
        let place = &at[id];
        // Inside a block refused for the same reason by the same op: that
        // block's problem says it.
        if let Some(pv) = place.parent.and_then(|p| verdicts.get(p)) {
            if pv.op == v.op && same(&pv.why, &v.why) {
                continue;
            }
        }
        let target = ops.get(v.op).and_then(|op| match op {
            Op::Replace { id, .. } | Op::Delete { id } => Some(id.as_str()),
            _ => None,
        }) == Some(id);
        let runs_on = matches!(v.why, Why::Unread { .. } | Why::MarkdownOnly);
        let key = (v.op, place.list, std::mem::discriminant(&v.why), v.removes);
        if runs_on {
            if let Some(&k) = runs.get(&key) {
                let run: &mut Unseen = &mut out[k];
                if run.ids.last().map(String::as_str) == place.prev && run.why == v.why {
                    run.ids.push(id.to_string());
                    run.target = false;
                    continue;
                }
            }
        }
        out.push(Unseen {
            op: v.op,
            ids: vec![id.to_string()],
            inside: place.parent.filter(|p| seen.knows(p)).map(str::to_string),
            after: place.prev.map(str::to_string),
            removes: v.removes,
            target,
            why: v.why.clone(),
        });
        if runs_on {
            runs.insert(key, out.len() - 1);
        }
    }

    // One op's blocks unread, or shown only as markdown, are one problem,
    // said from the first in page order: the read that shows the first goes
    // on to the rest, and a refusal naming each one would run as long as
    // the page.
    let mut out = out.into_iter().fold(Vec::<Unseen>::new(), |mut acc, u| {
        if matches!(u.why, Why::Unread { .. } | Why::MarkdownOnly) {
            if let Some(m) = acc.iter_mut().find(|m| m.op == u.op && m.why == u.why && m.removes == u.removes) {
                m.ids.extend(u.ids);
                m.target = false;
                return acc;
            }
        }
        acc.push(u);
        acc
    });

    // A block deleted after the read, written back by its id.
    for (op, ids) in wrote {
        let mut ids: Vec<&String> = ids.iter().filter(|id| !at.contains_key(id.as_str()) && seen.knows(id)).collect();
        ids.sort();
        for id in ids {
            out.push(Unseen {
                op: *op,
                ids: vec![id.clone()],
                inside: None,
                after: None,
                removes: false,
                target: false,
                why: Why::Deleted,
            });
        }
    }
    out
}

#[derive(Clone)]
enum Parent {
    Frag(XmlFragmentRef),
    El(XmlElementRef),
}

macro_rules! with_parent {
    ($p:expr, $x:ident => $body:expr) => {
        match $p {
            Parent::Frag($x) => $body,
            Parent::El($x) => $body,
        }
    };
}

fn find_in<T: ReadTxn, F: XmlFragment>(
    c: &Contract,
    txn: &T,
    parent: &F,
    wrap: &Parent,
    id: &str,
) -> Option<(Parent, u32, XmlElementRef)> {
    for (i, child) in ydoc::children(txn, parent) {
        if let Out::YXmlElement(el) = child {
            if el
                .get_attribute(txn, &c.id.attr)
                .map(|o| ydoc::out_to_value(&o))
                == Some(Value::String(id.into()))
            {
                return Some((wrap.clone(), i, el));
            }
            if let Some(hit) = find_in(c, txn, &el, &Parent::El(el.clone()), id) {
                return Some(hit);
            }
        }
    }
    None
}

fn write_plan(
    c: &Contract,
    txn: &mut TransactionMut,
    frag: &XmlFragmentRef,
    plan: Plan,
) -> Option<&'static str> {
    let located = plan
        .target
        .as_deref()
        .and_then(|id| find_in(c, txn, frag, &Parent::Frag(frag.clone()), id));
    match plan.kind {
        PlanKind::Append => {
            let len = frag.len(txn);
            write_nodes(c, txn, frag, len, &plan.nodes);
            None
        }
        PlanKind::Delete => {
            let (p, i, _) = located?;
            with_parent!(&p, x => x.remove_range(txn, i, 1));
            None
        }
        PlanKind::InsertAfter => {
            let (p, i, _) = located?;
            with_parent!(&p, x => write_nodes(c, txn, x, i + 1, &plan.nodes));
            None
        }
        PlanKind::InsertBefore => {
            let (p, i, _) = located?;
            with_parent!(&p, x => write_nodes(c, txn, x, i, &plan.nodes));
            None
        }
        PlanKind::Replace => {
            let (p, i, el) = located?;
            if let [new] = plan.nodes.as_slice() {
                if c.is_textblock(&new.kind) {
                    if patch_textblock(c, txn, &el, new) {
                        return Some("text-diff");
                    }
                } else if patch_block(c, txn, &el, new) {
                    return Some("patch");
                }
            }
            with_parent!(&p, x => {
                x.remove_range(txn, i, 1);
                write_nodes(c, txn, x, i, &plan.nodes);
            });
            Some("swap")
        }
    }
}

/// A string's length in the unit the document counts offsets in.
fn offset_len(kind: OffsetKind, s: &str) -> u32 {
    match kind {
        OffsetKind::Bytes => s.len() as u32,
        OffsetKind::Utf16 => s.encode_utf16().count() as u32,
    }
}

/// An inline atom as the document holds it: its type and attributes.
fn atom_node<T: ReadTxn>(txn: &T, el: &XmlElementRef) -> Node {
    let attrs = el
        .attributes(txn)
        .map(|(k, v)| (k.to_string(), ydoc::out_to_value(&v)))
        .collect();
    Node::element(el.tag().as_ref(), attrs, vec![])
}

/// The same atom: type and every attribute, an absent one reading as null.
fn same_atom(a: &Node, b: &Node) -> bool {
    let get = |n: &Node, k: &str| n.attrs.get(k).cloned().unwrap_or(Value::Null);
    a.kind == b.kind
        && b.marks.is_empty()
        && a.attrs
            .keys()
            .chain(b.attrs.keys())
            .all(|k| get(a, k) == get(b, k))
}

/// Same-type text block holding inline atoms of the same types in the same
/// order: update attributes in place, the block's and each atom's, and
/// write only the changed middle of each run of text between atoms. The
/// atoms, and every run that did not change, keep their Yjs items, so a
/// caret anywhere else in the block holds; a writer that renumbers its
/// footnotes on every draft changes their labels, not the sentences.
fn patch_textblock(c: &Contract, txn: &mut TransactionMut, el: &XmlElementRef, new: &Node) -> bool {
    if el.tag().as_ref() != new.kind || !c.is_textblock(&new.kind) {
        return false;
    }
    // The block's children as runs of text between atoms, each atom with
    // its index among the children.
    let mut runs: Vec<Vec<XmlTextRef>> = vec![vec![]];
    let mut atoms: Vec<(u32, Node, XmlElementRef)> = vec![];
    for (i, child) in ydoc::children(txn, el) {
        match child {
            Out::YXmlText(t) => runs.last_mut().expect("one run at least").push(t),
            Out::YXmlElement(a) => {
                atoms.push((i, atom_node(txn, &a), a));
                runs.push(vec![]);
            }
            _ => return false,
        }
    }
    let mut new_runs: Vec<Vec<Node>> = vec![vec![]];
    let mut new_atoms: Vec<&Node> = vec![];
    for n in &new.content {
        if n.is_text() {
            new_runs
                .last_mut()
                .expect("one run at least")
                .push(n.clone());
        } else {
            new_atoms.push(n);
            new_runs.push(vec![]);
        }
    }
    if atoms.len() != new_atoms.len()
        || !atoms
            .iter()
            .zip(&new_atoms)
            .all(|((_, a, _), b)| a.kind == b.kind && b.marks.is_empty())
    {
        return false;
    }
    // Each run's text before and after. A run that changed and is held in
    // more than one text node is not patched: the block is swapped whole.
    let mut changed: Vec<(usize, Flat, Flat)> = vec![];
    for (r, (texts, after)) in runs.iter().zip(&new_runs).enumerate() {
        let before: Vec<Node> = texts.iter().flat_map(|t| read_text(txn, t)).collect();
        let (a, b) = (flat(&before), flat(after));
        if a == b {
            continue;
        }
        if texts.len() > 1 {
            return false;
        }
        changed.push((r, a, b));
    }

    set_attrs(txn, el, &new.attrs);
    for ((_, was, atom), now) in atoms.iter().zip(&new_atoms) {
        if !same_atom(was, now) {
            set_attrs(txn, atom, &now.attrs);
        }
    }
    // Last run first: a text node inserted for a run moves the children
    // after it, never those before.
    for (r, a, b) in changed.into_iter().rev() {
        let t = match runs[r].first() {
            Some(t) => t.clone(),
            None => {
                let at = if r == 0 { 0 } else { atoms[r - 1].0 + 1 };
                el.insert(txn, at, XmlTextPrelim::new(""))
            }
        };
        patch_text(txn, &t, &a, &b);
    }
    true
}

/// Write `attrs` onto `el`, each only where it differs; a null removes one.
fn set_attrs(txn: &mut TransactionMut, el: &XmlElementRef, attrs: &serde_json::Map<String, Value>) {
    for (k, v) in attrs {
        let cur = el
            .get_attribute(txn, k)
            .map(|o| ydoc::out_to_value(&o))
            .unwrap_or(Value::Null);
        if &cur != v {
            match v {
                Value::Null => el.remove_attribute(txn, &k.as_str()),
                _ => {
                    el.insert_attribute(txn, k.as_str(), value_to_any(v));
                }
            }
        }
    }
}

/// Replace `el`, a block that holds blocks (a list, an item, a table, a
/// row, a quote) or none (an image), with `new` of its own type, in place:
/// its attributes where they differ, and its children paired with `new`'s
/// as [`crate::diff_ops`] pairs a page's blocks, by id, or, for one without
/// (a table's cell), by type and text or by the first id it holds, so a
/// cell whose words changed is patched, not swapped, and a keystroke on its
/// way into it lands. A paired child is patched the same way, a text block
/// by its text (`patch_textblock`), and swapped alone when it cannot be; a
/// child `new` lacks goes, and one only `new` has is written where it goes.
/// What did not change keeps its Yjs items, so a person's keystroke still
/// on its way into it lands. False, with nothing written, when `el` is not
/// of `new`'s type or holds anything but blocks.
fn patch_block(c: &Contract, txn: &mut TransactionMut, el: &XmlElementRef, new: &Node) -> bool {
    if el.tag().as_ref() != new.kind || c.is_textblock(&new.kind) {
        return false;
    }
    let mut kids: Vec<XmlElementRef> = vec![];
    for (_, child) in ydoc::children(txn, el) {
        match child {
            Out::YXmlElement(k) => kids.push(k),
            _ => return false,
        }
    }
    // Read whole for the pairing, one node per element (a depth past the
    // limit reads as none, and is not patched).
    let current = ydoc::read_children(txn, el, "", 1, &mut vec![]);
    if current.len() != kids.len() {
        return false;
    }
    set_attrs(txn, el, &new.attrs);
    let pairs = crate::diff::pair_blocks(&current, &new.content);
    // From the end, so a removal or an insert never moves a child still to
    // be reached.
    let (mut cur_end, mut new_end) = (kids.len(), new.content.len());
    for &(i, j) in pairs.iter().rev() {
        if cur_end > i + 1 {
            el.remove_range(txn, (i + 1) as u32, (cur_end - i - 1) as u32);
        }
        if new_end > j + 1 {
            write_nodes(c, txn, el, (i + 1) as u32, &new.content[j + 1..new_end]);
        }
        let child = &new.content[j];
        let patched = if c.is_textblock(&child.kind) {
            patch_textblock(c, txn, &kids[i], child)
        } else {
            patch_block(c, txn, &kids[i], child)
        };
        if !patched {
            el.remove_range(txn, i as u32, 1);
            write_nodes(c, txn, el, i as u32, std::slice::from_ref(child));
        }
        (cur_end, new_end) = (i, j);
    }
    if cur_end > 0 {
        el.remove_range(txn, 0, cur_end as u32);
    }
    if new_end > 0 {
        write_nodes(c, txn, el, 0, &new.content[..new_end]);
    }
    true
}

/// Write `b` over `a` in `t`: only the changed middle.
fn patch_text(txn: &mut TransactionMut, t: &XmlTextRef, a: &Flat, b: &Flat) {
    let (pre, a_end, _) = span(a, b);
    if pre == a.len() && pre == b.len() {
        return;
    }
    let suf = a.len() - a_end;
    let kind = txn.doc().offset_kind();
    let s = |v: &[Unit]| {
        v.iter()
            .map(|u| {
                if let Unit::Ch(c, _) = u {
                    *c
                } else {
                    '\u{fffc}'
                }
            })
            .collect::<String>()
    };
    let retain = offset_len(kind, &s(&a[..pre]));
    let delete = offset_len(kind, &s(&a[pre..a.len() - suf]));
    // The deletion is its own call, not a step of the delta that inserts:
    // after deleting, yrs reckons the formatting written after the deleted
    // text as already in force where that text was, so text the same delta
    // inserted there would take marks it was not written with, or lose its
    // own. The delta below finds the place, and what is in force there,
    // again.
    if delete > 0 {
        t.remove_range(txn, retain, delete);
    }
    let mut delta: Vec<Delta<In>> = vec![];
    if retain > 0 {
        delta.push(Delta::Retain(retain, None));
    }
    // Group the inserted middle into runs of equal marks. A run holds text
    // only: the atoms between runs are the block's children.
    let mid: Vec<(char, &Vec<Mark>)> = b[pre..b.len() - suf]
        .iter()
        .filter_map(|u| {
            if let Unit::Ch(c, m) = u {
                Some((*c, m))
            } else {
                None
            }
        })
        .collect();
    let mut i = 0;
    while i < mid.len() {
        let mut j = i;
        while j < mid.len() && mid[j].1 == mid[i].1 {
            j += 1;
        }
        let chunk: String = mid[i..j].iter().map(|(c, _)| *c).collect();
        delta.push(Delta::Inserted(
            In::Any(Any::from(chunk)),
            ydoc::mark_attrs(mid[i].1),
        ));
        i = j;
    }
    if !mid.is_empty() {
        t.apply_delta(txn, delta);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{html, Options};
    use yrs::Options as DocOptions;

    fn seed_into(c: &Contract, doc: Doc, html_src: &str) -> Doc {
        let o = crate::ingest::parse(c, html_src, "doc", Mode::Strict);
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        let mut txn = doc.transact_mut();
        let frag = ydoc::fragment(c, &mut txn);
        write_nodes(c, &mut txn, &frag, 0, &o.nodes);
        drop(txn);
        doc
    }

    fn seed(c: &Contract, html_src: &str) -> Doc {
        seed_into(c, ydoc::new_doc(), html_src)
    }

    fn render(c: &Contract, doc: &Doc) -> String {
        html(c, &read_doc(c, &doc.transact()), &Options { ids: true })
    }

    const EXPENSES: &str = "<table data-id=\"t\"><tr data-id=\"r0\"><th><p data-id=\"h1\">Day</p></th><th><p data-id=\"h2\">Cost</p></th></tr>\
         <tr data-id=\"r1\"><td><p data-id=\"c1\">Oct 2</p></td><td><p data-id=\"c2\">40</p></td></tr></table><p data-id=\"z\">After.</p>";

    /// A table's rows carry ids, so a row is added, replaced or taken out
    /// by its id, as a list item is, and the table stays a grid.
    #[test]
    fn a_row_is_edited_by_its_id() {
        let c = Contract::load();
        let doc = seed(&c, EXPENSES);
        let applied = apply(
            &c,
            &doc,
            None,
            &[Op::InsertAfter {
                id: "r1".into(),
                html: "<tr><td><p>Oct 9</p></td><td><p>Taxi</p></td></tr>".into(),
            }],
        );
        assert!(applied.is_ok(), "{applied:?}");
        let tree = read_doc(&c, &doc.transact());
        let table = &tree[0];
        assert_eq!(table.content.len(), 3);
        assert_eq!(table.content[2].kind, "tableRow");
        assert!(table.content[2].id().is_some(), "a new row gets an id");
        assert_eq!(table.content[2].text_content(), "Oct 9Taxi");

        assert!(apply(&c, &doc, None, &[Op::Delete { id: "r1".into() }]).is_ok());
        assert_eq!(read_doc(&c, &doc.transact())[0].content.len(), 2);

        // A row of the wrong width would leave no grid: refused.
        let refused = apply(
            &c,
            &doc,
            None,
            &[Op::Replace {
                id: "r0".into(),
                html: "<tr><th><p>Day</p></th></tr>".into(),
            }],
        )
        .unwrap_err();
        assert!(refused[0].message.contains("would not be a grid"), "{refused:?}");
    }

    /// A row written where a block goes is a table's part outside its table:
    /// HTML would drop its tags and keep its cells' text as paragraphs. It
    /// is refused, with what to do instead, and nothing is written.
    #[test]
    fn a_row_written_outside_its_table_is_refused() {
        let c = Contract::load();
        for (op, at) in [
            (
                Op::InsertAfter {
                    id: "c1".into(),
                    html: "<tr><td><p>Oct 9</p></td><td><p>Taxi</p></td></tr>".into(),
                },
                "tr",
            ),
            (
                Op::Replace {
                    id: "z".into(),
                    html: "<tr><td><p>a</p></td></tr>".into(),
                },
                "tr",
            ),
            (
                Op::InsertAfter {
                    id: "c1".into(),
                    html: "<caption>x</caption><col><p>y</p>".into(),
                },
                "caption",
            ),
            (
                Op::Append {
                    html: "<html><body><p>z</p></body></html>".into(),
                },
                "html",
            ),
        ] {
            let doc = seed(&c, EXPENSES);
            let before = render(&c, &doc);
            let refused = apply(&c, &doc, None, std::slice::from_ref(&op)).unwrap_err();
            assert!(
                refused.iter().any(|p| p.message.starts_with(&format!("<{at}>"))),
                "{op:?}: {refused:?}"
            );
            assert_eq!(render(&c, &doc), before, "{op:?} wrote");
        }
    }

    #[test]
    fn replace_text_block_is_a_text_diff() {
        let c = Contract::load();
        let doc = seed(
            &c,
            "<p data-id=\"a\">The quick fox</p><p data-id=\"b\">Two</p>",
        );
        let r = apply(
            &c,
            &doc,
            None,
            &[Op::Replace {
                id: "a".into(),
                html: "<p>The quick <strong>brown</strong> fox</p>".into(),
            }],
        )
        .unwrap();
        assert_eq!(r.replaced_as, ["text-diff"]);
        assert_eq!(
            render(&c, &doc),
            "<p data-id=\"a\">The quick <strong>brown</strong> fox</p><p data-id=\"b\">Two</p>"
        );
    }

    /// A writer's copy of a read keeps the spaces a person typed, and only
    /// what it changed is its own. HTML reads two spaces, a tab or a run of
    /// them back as one, so a one-word change rewrote every run in the
    /// paragraph, inline code included, and widened the stretch a person's
    /// edit at the same time had to stay clear of.
    #[test]
    fn a_replace_from_a_read_keeps_the_spaces_typed() {
        let c = Contract::load();
        let typed = |doc: &Doc, html: &str| {
            let mut txn = doc.transact_mut();
            apply_own_in(&c, &mut txn, &[Op::Replace { id: "a".into(), html: html.into() }]).unwrap();
        };
        let text = |doc: &Doc| read_doc(&c, &doc.transact())[0].text_content();
        let shown = |base: &[Node]| html(&c, &base[..1], &Options { ids: true });
        let doc = seed(&c, "<p data-id=\"a\">x</p><p data-id=\"b\">Two</p>");
        typed(&doc, "<p data-id=\"a\">Total:  5 apples.\tThen   the <code>ls  -la</code> line.  </p>");
        assert_eq!(text(&doc), "Total:  5 apples.\tThen   the ls  -la line.  ");

        let base = read_doc(&c, &doc.transact());
        let r = apply(&c, &doc, Some(&Seen::of_page(&base)), &[Op::Replace { id: "a".into(), html: shown(&base).replace("apples", "pears") }])
            .unwrap();
        assert_eq!(r.replaced_as, ["text-diff"]);
        assert_eq!(
            render(&c, &doc),
            "<p data-id=\"a\">Total:  5 pears.\tThen   the <code>ls  -la</code> line.  </p><p data-id=\"b\">Two</p>"
        );

        // The person changes a word while the model rewrites another: both
        // land, the spaces between them as typed.
        let base = read_doc(&c, &doc.transact());
        typed(&doc, "<p data-id=\"a\">Total:  5 pears.\tThan   the <code>ls  -la</code> line.  </p>");
        let r = apply(&c, &doc, Some(&Seen::of_page(&base)), &[Op::Replace { id: "a".into(), html: shown(&base).replace("Total", "Sum") }])
            .unwrap();
        assert!(r.notes.iter().any(|n| n.message.contains("merged")), "{:?}", r.notes);
        assert_eq!(text(&doc), "Sum:  5 pears.\tThan   the ls  -la line.  ");

        // What the model adds is its own, as HTML reads it; the rest stays
        // as typed. With no base, the block as it stands is what it read.
        let r = apply(
            &c,
            &doc,
            None,
            &[Op::Replace { id: "a".into(), html: "<p>Sum:  5 pears.\tThan   the <code>ls  -la</code> last  line.</p>".into() }],
        )
        .unwrap();
        assert_eq!(r.replaced_as, ["text-diff"]);
        assert_eq!(text(&doc), "Sum:  5 pears.\tThan   the ls  -la last line.  ");
    }

    /// A replace that takes a mark off text at its edge leaves the new text
    /// without it: the formatting of the text deleted there must not carry
    /// over what is written in its place.
    #[test]
    fn a_replace_that_takes_a_mark_off_takes_it_off() {
        let c = Contract::load();
        for (before, after) in [
            ("<em>ab</em>", "<em>a</em>b"),
            ("<em>ab</em>c", "<em>a</em>Xc"),
            ("<strong>ab</strong>", "<strong>a</strong>X"),
            ("<a href=\"/x\">ab</a>c", "<a href=\"/x\">a</a>Xc"),
            ("The <em>quick</em> fox", "The <em>quic</em>K fox"),
            (
                "<a href=\"/x\">ab</a>",
                "<a href=\"/x\">a</a><a href=\"/y\">b</a>",
            ),
        ] {
            let doc = seed(&c, &format!("<p data-id=\"a\">{before}</p>"));
            let r = apply(
                &c,
                &doc,
                None,
                &[Op::Replace {
                    id: "a".into(),
                    html: format!("<p>{after}</p>"),
                }],
            )
            .unwrap();
            assert_eq!(r.replaced_as, ["text-diff"], "{before}");
            assert_eq!(
                render(&c, &doc),
                format!("<p data-id=\"a\">{after}</p>"),
                "{before} to {after}"
            );
        }
    }

    /// Every replace of one marked text with another lands as written, in
    /// a few thousand random pairs over three letters and four marks.
    #[test]
    fn a_text_diff_lands_every_mark_as_written() {
        let c = Contract::load();
        let marks: [(&str, &str); 4] = [
            ("<a href=\"/x\">", "</a>"),
            ("<strong>", "</strong>"),
            ("<em>", "</em>"),
            ("<code>", "</code>"),
        ];
        let mut rng = fastrand::Rng::with_seed(7);
        let text = |rng: &mut fastrand::Rng| {
            let mut html = String::new();
            for _ in 0..rng.usize(1..7) {
                let on: Vec<bool> = (0..marks.len()).map(|_| rng.u8(..4) == 0).collect();
                for (m, _) in marks.iter().zip(&on).filter(|(_, on)| **on) {
                    html.push_str(m.0);
                }
                html.push(['a', 'b', 'c'][rng.usize(..3)]);
                for (m, _) in marks.iter().zip(&on).filter(|(_, on)| **on).rev() {
                    html.push_str(m.1);
                }
            }
            format!("<p data-id=\"a\">{html}</p>")
        };
        for _ in 0..2_000 {
            let (before, after) = (text(&mut rng), text(&mut rng));
            let doc = seed(&c, &before);
            apply(
                &c,
                &doc,
                None,
                &[Op::Replace {
                    id: "a".into(),
                    html: after.clone(),
                }],
            )
            .unwrap();
            let want = render(&c, &seed(&c, &after));
            assert_eq!(render(&c, &doc), want, "{before} to {after}");
        }
    }

    #[test]
    fn text_diff_offsets_follow_the_documents_unit() {
        // Astral characters are two UTF-16 units and four bytes: an offset in
        // the wrong unit would put the edit inside the emoji or past it.
        let c = Contract::load();
        let byte_doc = Doc::with_options(DocOptions {
            offset_kind: OffsetKind::Bytes,
            ..Default::default()
        });
        for doc in [
            seed(&c, "<p data-id=\"a\">👍🏽 𝔘𝔫𝔦 fox</p>"),
            seed_into(&c, byte_doc, "<p data-id=\"a\">👍🏽 𝔘𝔫𝔦 fox</p>"),
        ] {
            let r = apply(
                &c,
                &doc,
                None,
                &[Op::Replace {
                    id: "a".into(),
                    html: "<p>👍🏽 𝔘𝔫𝔦 red fox</p>".into(),
                }],
            )
            .unwrap();
            assert_eq!(r.replaced_as, ["text-diff"]);
            assert_eq!(render(&c, &doc), "<p data-id=\"a\">👍🏽 𝔘𝔫𝔦 red fox</p>");
        }
    }

    #[test]
    fn nested_inserts_check_the_parent() {
        let c = Contract::load();
        let doc = seed(
            &c,
            "<ul data-id=\"l\"><li data-id=\"i\"><p data-id=\"p\">one</p></li></ul>",
        );
        let ok = apply(
            &c,
            &doc,
            None,
            &[Op::InsertAfter {
                id: "i".into(),
                html: "<li>two</li>".into(),
            }],
        )
        .unwrap();
        assert_eq!(ok.written.len(), 1);
        let bad = apply(
            &c,
            &doc,
            None,
            &[Op::InsertAfter {
                id: "i".into(),
                html: "<p>loose</p>".into(),
            }],
        )
        .unwrap_err();
        assert!(bad[0].message.contains("bulletList"), "{bad:?}");
        let sublist = apply(
            &c,
            &doc,
            None,
            &[Op::InsertAfter {
                id: "p".into(),
                html: "<ul><li>sub</li></ul>".into(),
            }],
        );
        assert!(sublist.is_ok(), "{sublist:?}");
    }

    #[test]
    fn a_refused_batch_writes_nothing() {
        let c = Contract::load();
        let doc = seed(&c, "<p data-id=\"a\">one</p>");
        let before = render(&c, &doc);
        let err = apply(
            &c,
            &doc,
            None,
            &[
                Op::Append {
                    html: "<p>fine</p>".into(),
                },
                Op::Replace {
                    id: "a".into(),
                    html: "<div>no</div>".into(),
                },
            ],
        )
        .unwrap_err();
        assert_eq!(err.len(), 1);
        assert_eq!(render(&c, &doc), before);
    }

    #[test]
    fn deleting_the_last_block_is_refused() {
        let c = Contract::load();
        let doc = seed(&c, "<p data-id=\"a\">one</p>");
        let refused = apply(&c, &doc, None, &[Op::Delete { id: "a".into() }]).unwrap_err();
        assert_eq!(refused[0].message, "after this edit `doc` would hold nothing; it holds `block+`");
    }

    /// A refusal of what a parent cannot hold names what does not fit, not
    /// every block beside it: on a page of thousands of blocks the list of
    /// them ran past the end of what a tool's result carries, cutting off
    /// the kind that did not fit and the rest of the reply.
    #[test]
    fn a_misfit_is_named_alone_on_a_long_page() {
        let c = Contract::load();
        let mut page = String::from("<ul data-id=\"l\">");
        for i in 0..600 {
            page.push_str(&format!("<li data-id=\"i{i}\"><p>Book {i}</p></li>"));
        }
        page.push_str("</ul>");
        for i in 0..9000 {
            page.push_str(&format!("<p data-id=\"p{i}\">Entry {i}</p>"));
        }
        let doc = seed(&c, &page);
        for (op, said) in [
            (
                Op::Append { html: "<li><p>Entry 9000</p></li>".into() },
                "`doc` cannot hold `listItem`; it holds `block+`; an item goes in a list: insert it after an item of one",
            ),
            (
                Op::InsertAfter { id: "i3".into(), html: "<p>between</p>".into() },
                "`bulletList` cannot hold `paragraph`; it holds `listItem+`; a block goes inside an item, or after the list",
            ),
        ] {
            let refused = apply(&c, &doc, None, std::slice::from_ref(&op)).unwrap_err();
            assert_eq!(refused.len(), 1, "{op:?}");
            assert_eq!(refused[0].message, said, "{op:?}");
        }
        // A sequence the parent cannot hold, each kind one it may: what it
        // would hold, in short.
        let headed = seed(&c, "<blockquote data-id=\"q\"><p data-id=\"a\">one</p></blockquote>");
        let refused = apply(&c, &headed, None, &[Op::Delete { id: "a".into() }]).unwrap_err();
        assert_eq!(refused[0].message, "after this edit `blockquote` would hold nothing; it holds `block+`");
        assert_eq!(
            in_short(&["bulletList", "paragraph", "paragraph", "heading", "paragraph", "table", "paragraph", "paragraph", "listItem"]),
            "bulletList, paragraph ×2, heading, …, table, paragraph ×2, listItem (9 blocks)"
        );
    }

    #[test]
    fn a_heading_level_changes_in_place() {
        let c = Contract::load();
        let doc = seed(&c, "<h2 data-id=\"a\">Plan</h2>");
        let r = apply(
            &c,
            &doc,
            None,
            &[Op::Replace {
                id: "a".into(),
                html: "<h5>Plan</h5>".into(),
            }],
        )
        .unwrap();
        assert_eq!(r.replaced_as, ["text-diff"]);
        assert_eq!(render(&c, &doc), "<h5 data-id=\"a\">Plan</h5>");
    }

    #[test]
    fn stale_write_merges_disjoint_edits_and_refuses_overlaps() {
        let c = Contract::load();
        let doc = seed(&c, "<p data-id=\"a\">The quick fox jumps.</p>");
        let base = read_doc(&c, &doc.transact());
        // A person types at the end after the model read.
        apply(
            &c,
            &doc,
            None,
            &[Op::Replace {
                id: "a".into(),
                html: "<p>The quick fox jumps. Then it naps.</p>".into(),
            }],
        )
        .unwrap();
        // The model, working from its read, changes an earlier word.
        let r = apply(
            &c,
            &doc,
            Some(&Seen::of_page(&base)),
            &[Op::Replace {
                id: "a".into(),
                html: "<p>The slow fox jumps.</p>".into(),
            }],
        )
        .unwrap();
        assert!(
            r.notes.iter().any(|n| n.message.contains("merged")),
            "{:?}",
            r.notes
        );
        assert_eq!(
            render(&c, &doc),
            "<p data-id=\"a\">The slow fox jumps. Then it naps.</p>"
        );
        // Neighbouring words: both land.
        let base2 = read_doc(&c, &doc.transact());
        apply(
            &c,
            &doc,
            None,
            &[Op::Replace {
                id: "a".into(),
                html: "<p>The slow red fox jumps. Then it naps.</p>".into(),
            }],
        )
        .unwrap();
        apply(
            &c,
            &doc,
            Some(&Seen::of_page(&base2)),
            &[Op::Replace {
                id: "a".into(),
                html: "<p>The sly fox jumps. Then it naps.</p>".into(),
            }],
        )
        .unwrap();
        assert_eq!(
            render(&c, &doc),
            "<p data-id=\"a\">The sly red fox jumps. Then it naps.</p>"
        );
        // The same word: refused, nothing written.
        let base3 = read_doc(&c, &doc.transact());
        apply(
            &c,
            &doc,
            None,
            &[Op::Replace {
                id: "a".into(),
                html: "<p>The slyest red fox jumps. Then it naps.</p>".into(),
            }],
        )
        .unwrap();
        let err = apply(
            &c,
            &doc,
            Some(&Seen::of_page(&base3)),
            &[Op::Replace {
                id: "a".into(),
                html: "<p>The quick red fox jumps. Then it naps.</p>".into(),
            }],
        )
        .unwrap_err();
        assert!(err[0].message.contains("same words"), "{err:?}");
        assert_eq!(
            render(&c, &doc),
            "<p data-id=\"a\">The slyest red fox jumps. Then it naps.</p>"
        );
    }

    #[test]
    fn merge_steps_over_an_inline_widget() {
        let c = Contract::load();
        let doc = seed(&c, "<p data-id=\"a\">Lunch with <virtues-mention to=\"/person/1\" label=\"Nick\"></virtues-mention> on Friday.</p>");
        let base = read_doc(&c, &doc.transact());
        apply(&c, &doc, None, &[Op::Replace { id: "a".into(), html: "<p>Lunch with <virtues-mention to=\"/person/1\" label=\"Nick\"></virtues-mention> on Friday. Bring the map.</p>".into() }]).unwrap();
        apply(&c, &doc, Some(&Seen::of_page(&base)), &[Op::Replace { id: "a".into(), html: "<p>Dinner with <virtues-mention to=\"/person/1\" label=\"Nick\"></virtues-mention> on Friday.</p>".into() }]).unwrap();
        assert_eq!(
            render(&c, &doc),
            "<p data-id=\"a\">Dinner with <virtues-mention to=\"/person/1\" label=\"Nick\"></virtues-mention> on Friday. Bring the map.</p>"
        );
    }

    /// The children of block `id`, each with the id of its Yjs item.
    fn items(c: &Contract, doc: &Doc, id: &str) -> Vec<yrs::branch::BranchID> {
        let txn = doc.transact();
        let frag = txn.get_xml_fragment("doc").unwrap();
        let (_, _, el) = find_in(c, &txn, &frag, &Parent::Frag(frag.clone()), id).unwrap();
        ydoc::children(&txn, &el)
            .map(|(_, child)| match child {
                Out::YXmlText(t) => AsRef::<yrs::branch::Branch>::as_ref(&t).id(),
                Out::YXmlElement(e) => AsRef::<yrs::branch::Branch>::as_ref(&e).id(),
                other => panic!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn text_around_an_inline_atom_is_patched_and_a_caret_after_it_holds() {
        use yrs::updates::decoder::Decode;
        use yrs::{Assoc, IndexedSequence, Update};
        let c = Contract::load();
        let mention = "<virtues-mention to=\"/person/person_1\" label=\"Nick\"></virtues-mention>";
        let doc = seed(
            &c,
            &format!("<p data-id=\"a\">Lunch wiht {mention} on Friday.</p>"),
        );
        // A second device, holding a caret after the mention: " on |Friday.".
        let other = ydoc::new_doc();
        let sync = |from: &Doc, to: &Doc| {
            let sv = to.transact().state_vector();
            let update = from.transact().encode_state_as_update_v1(&sv);
            to.transact_mut()
                .apply_update(Update::decode_v1(&update).unwrap())
                .unwrap();
        };
        sync(&doc, &other);
        let caret = {
            let txn = other.transact();
            let frag = txn.get_xml_fragment("doc").unwrap();
            let (_, _, p) = find_in(&c, &txn, &frag, &Parent::Frag(frag.clone()), "a").unwrap();
            let Some(Out::YXmlText(after)) = ydoc::children(&txn, &p).nth(2).map(|(_, o)| o) else {
                panic!("the text after the mention");
            };
            after.sticky_index(&txn, 4, Assoc::After).unwrap()
        };
        let before = items(&c, &doc, "a");

        let r = apply(
            &c,
            &doc,
            None,
            &[Op::Replace {
                id: "a".into(),
                html: format!("<p>Lunch with {mention} on Friday.</p>"),
            }],
        )
        .unwrap();
        assert_eq!(r.replaced_as, ["text-diff"]);
        assert_eq!(
            render(&c, &doc),
            format!("<p data-id=\"a\">Lunch with {mention} on Friday.</p>")
        );
        // Every child keeps its item: the mention, and both runs of text.
        assert_eq!(items(&c, &doc, "a"), before);
        sync(&doc, &other);
        let txn = other.transact();
        let at = caret.get_offset(&txn).expect("the caret's text is there");
        assert_eq!(at.index, 4);
        assert!(!at.branch.is_deleted());
        drop(txn);

        // A typo after the mention, and text between two breaks that had
        // none: patched too.
        let r = apply(
            &c,
            &doc,
            None,
            &[Op::Replace {
                id: "a".into(),
                html: format!("<p>Lunch with {mention} on Fri.</p>"),
            }],
        )
        .unwrap();
        assert_eq!(r.replaced_as, ["text-diff"]);
        let doc = seed(&c, "<p data-id=\"b\">a<br><br>b</p>");
        let r = apply(
            &c,
            &doc,
            None,
            &[Op::Replace {
                id: "b".into(),
                html: "<p>a<br><em>between</em><br>b</p>".into(),
            }],
        )
        .unwrap();
        assert_eq!(r.replaced_as, ["text-diff"]);
        assert_eq!(
            render(&c, &doc),
            "<p data-id=\"b\">a<br><em>between</em><br>b</p>"
        );
    }

    /// An atom added or taken out, or one of another type in its place,
    /// swaps the block; one of the same type whose attributes changed is
    /// written in place (`a_renumbered_footnote_is_patched_in_place`).
    #[test]
    fn a_changed_atom_swaps_the_block() {
        let c = Contract::load();
        for (before, after) in [
            (
                "a <virtues-mention to=\"/person/person_1\" label=\"Nick\"></virtues-mention>",
                "a <virtues-footnote label=\"ev-1\"></virtues-footnote>",
            ),
            ("a<br>b", "a b"),
            ("a b", "a<br>b"),
        ] {
            let doc = seed(&c, &format!("<p data-id=\"a\">{before}</p>"));
            let r = apply(
                &c,
                &doc,
                None,
                &[Op::Replace {
                    id: "a".into(),
                    html: format!("<p>{after}</p>"),
                }],
            )
            .unwrap();
            assert_eq!(r.replaced_as, ["swap"], "{before}");
            assert_eq!(render(&c, &doc), format!("<p data-id=\"a\">{after}</p>"));
        }
    }

    /// A writer renumbers its footnotes on every draft: a sentence whose
    /// footnote's label and note changed keeps the footnote's item, and the
    /// text's around it, so a caret in it holds. A mention is the same.
    #[test]
    fn a_renumbered_footnote_is_patched_in_place() {
        let c = Contract::load();
        let f = |label: &str, note: &str| format!("<virtues-footnote label=\"{label}\" note=\"{note}\"></virtues-footnote>");
        let doc = seed(
            &c,
            &format!("<p data-id=\"a\">You read.{} You ate.{}</p>", f("ev-1", "Recording · 9:05 AM"), f("ev-2", "Chat · 9:30 AM")),
        );
        let before = items(&c, &doc, "a");
        let html = format!("<p>You read.{} You ate.{}</p>", f("ev-3", "Recording · 9:05 AM"), f("ev-4", "Chat · 9:31 AM"));
        let r = apply(&c, &doc, None, &[Op::Replace { id: "a".into(), html: html.clone() }]).unwrap();
        assert_eq!(r.replaced_as, ["text-diff"]);
        assert_eq!(items(&c, &doc, "a"), before);
        assert_eq!(render(&c, &doc), html.replace("<p>", "<p data-id=\"a\">"));

        let doc = seed(&c, "<p data-id=\"m\">a <virtues-mention to=\"/person/person_1\" label=\"Nick\"></virtues-mention></p>");
        let before = items(&c, &doc, "m");
        let html = "<p>a <virtues-mention to=\"/person/person_2\" label=\"David Okafor\"></virtues-mention></p>";
        let r = apply(&c, &doc, None, &[Op::Replace { id: "m".into(), html: html.into() }]).unwrap();
        assert_eq!(r.replaced_as, ["text-diff"]);
        assert_eq!(items(&c, &doc, "m"), before);
        assert_eq!(render(&c, &doc), html.replace("<p>", "<p data-id=\"m\">"));
    }

    /// A model writes a footnote back as a read showed it, its note an
    /// attribute it may leave out: a footnote with a label and no note takes
    /// the note of the page's footnote with that label. A label the page
    /// does not hold stays without one, and a note written is kept.
    #[test]
    fn a_footnote_written_without_its_note_takes_the_pages() {
        let c = Contract::load();
        let doc = seed(
            &c,
            "<p data-id=\"a\">You read.<virtues-footnote label=\"ev-1\" note=\"Recording · 9:05 AM\"></virtues-footnote></p>",
        );
        apply(
            &c,
            &doc,
            None,
            &[
                Op::Replace {
                    id: "a".into(),
                    html: "<p>You read a book.<virtues-footnote label=\"ev-1\"></virtues-footnote></p>".into(),
                },
                Op::Append {
                    html: "<p>Again.<virtues-footnote label=\"ev-1\"></virtues-footnote> New.<virtues-footnote label=\"ev-7\"></virtues-footnote> \
                           Said.<virtues-footnote label=\"ev-1\" note=\"Chat · 9:06 AM\"></virtues-footnote></p>"
                        .into(),
                },
            ],
        )
        .unwrap();
        let tree = read_doc(&c, &doc.transact());
        let notes: Vec<(String, Value)> = tree
            .iter()
            .flat_map(|b| b.content.iter())
            .filter(|n| n.kind == "footnote")
            .map(|n| (n.attrs["label"].as_str().unwrap().to_string(), n.attrs["note"].clone()))
            .collect();
        assert_eq!(
            notes,
            [
                ("ev-1".to_string(), Value::from("Recording · 9:05 AM")),
                ("ev-1".to_string(), Value::from("Recording · 9:05 AM")),
                ("ev-7".to_string(), Value::Null),
                ("ev-1".to_string(), Value::from("Chat · 9:06 AM")),
            ]
        );
    }

    #[test]
    fn a_replace_keeps_the_nested_ids_it_writes_back() {
        let c = Contract::load();
        let doc = seed(
            &c,
            "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">a</p></li>\
             <li data-id=\"i2\"><p data-id=\"p2\">b</p></li></ul><p data-id=\"x\">x</p>",
        );
        let edited = "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">a</p></li>\
                      <li data-id=\"i2\"><p data-id=\"p2\">b, changed</p></li>\
                      <li data-id=\"i3\"><p data-id=\"x\">a copy of x's id</p></li></ul>";
        apply(
            &c,
            &doc,
            None,
            &[Op::Replace {
                id: "l".into(),
                html: edited.into(),
            }],
        )
        .unwrap();
        let tree = read_doc(&c, &doc.transact());
        let list = &tree[0];
        assert_eq!(list.id(), Some("l"));
        let item_ids: Vec<_> = list
            .content
            .iter()
            .map(|i| i.id().unwrap().to_string())
            .collect();
        assert_eq!(item_ids, ["i1", "i2", "i3"]);
        assert_eq!(list.content[0].content[0].id(), Some("p1"));
        assert_eq!(list.content[1].content[0].id(), Some("p2"));
        // An id another block still holds is renewed.
        assert_ne!(list.content[2].content[0].id(), Some("x"));
        assert_eq!(tree[1].id(), Some("x"));
    }

    /// The replaced block keeps its id whatever id the model wrote on the
    /// replacement: an unused one, another block's, or one a delete earlier
    /// in the batch freed.
    #[test]
    fn a_replace_keeps_its_target_id_whatever_the_html_says() {
        let c = Contract::load();
        let page = "<p data-id=\"a\">Coffee.</p><p data-id=\"b\">Tea.</p>";
        let cases: [(&str, Vec<Op>); 3] = [
            (
                "an unused id",
                vec![Op::Replace {
                    id: "a".into(),
                    html: "<p data-id=\"x1y2z3w4\">Black coffee.</p>".into(),
                }],
            ),
            (
                "another block's id",
                vec![Op::Replace {
                    id: "a".into(),
                    html: "<p data-id=\"b\">Black coffee.</p>".into(),
                }],
            ),
            (
                "an id deleted in the same batch",
                vec![
                    Op::Delete { id: "b".into() },
                    Op::Replace {
                        id: "a".into(),
                        html: "<p data-id=\"b\">Black coffee.</p>".into(),
                    },
                ],
            ),
        ];
        for (what, ops) in cases {
            let doc = seed(&c, page);
            let applied = apply(&c, &doc, None, &ops).unwrap();
            assert_eq!(applied.written, ["a"], "{what}");
            let tree = read_doc(&c, &doc.transact());
            assert_eq!(tree[0].id(), Some("a"), "{what}");
            assert_eq!(tree[0].text_content(), "Black coffee.", "{what}");
            // The block named again by its id is still there.
            apply(
                &c,
                &doc,
                None,
                &[Op::Replace {
                    id: "a".into(),
                    html: "<p>Coffee, black.</p>".into(),
                }],
            )
            .unwrap_or_else(|e| panic!("{what}: {e:?}"));
            if tree.len() > 1 {
                assert_eq!(tree[1].id(), Some("b"), "{what}");
            }
        }
    }

    #[test]
    fn ensure_ids_fills_and_dedupes() {
        let c = Contract::load();
        let o = crate::ingest::parse(
            &c,
            "<p data-id=\"x\">a</p><p data-id=\"x\">b</p><ul><li>c</li></ul>",
            "doc",
            Mode::Strict,
        );
        let mut nodes = o.nodes;
        ensure_ids(&c, &mut nodes);
        let mut ids = HashSet::new();
        let mut count = 0;
        for n in &nodes {
            n.walk(&mut |n| {
                if c.node(&n.kind).map(|s| s.spec.id).unwrap_or(false) {
                    count += 1;
                    assert!(ids.insert(n.id().expect("every block has an id").to_string()));
                }
            });
        }
        assert_eq!(count, 5);
        assert_eq!(nodes[0].id(), Some("x"));
        assert_ne!(nodes[1].id(), Some("x"));
    }

    /// A copy of `doc` as another device holds it.
    fn device(doc: &Doc) -> Doc {
        use yrs::updates::decoder::Decode;
        let copy = ydoc::new_doc();
        let state = doc.transact().encode_state_as_update_v1(&yrs::StateVector::default());
        copy.transact_mut()
            .apply_update(yrs::Update::decode_v1(&state).unwrap())
            .unwrap();
        copy
    }

    /// Type `typed` at the end of the text block `path` leads to, child
    /// index by child index from the page, and return the update.
    fn type_at_end(doc: &Doc, path: &[u32], typed: &str) -> Vec<u8> {
        let before = doc.transact().state_vector();
        {
            use yrs::{WriteTxn, XmlOut};
            let mut txn = doc.transact_mut();
            let frag = txn.get_or_insert_xml_fragment("doc");
            let Some(XmlOut::Element(mut el)) = frag.get(&txn, path[0]) else {
                panic!("a block at {path:?}")
            };
            for &i in &path[1..] {
                let Some(XmlOut::Element(next)) = el.get(&txn, i) else {
                    panic!("a block at {path:?}")
                };
                el = next;
            }
            let Some(XmlOut::Text(t)) = el.get(&txn, 0) else {
                panic!("text at {path:?}")
            };
            let end = t.len(&txn);
            t.insert(&mut txn, end, typed);
        }
        let update = doc.transact().encode_diff_v1(&before);
        update
    }

    fn apply_update(doc: &Doc, update: &[u8]) {
        use yrs::updates::decoder::Decode;
        doc.transact_mut()
            .apply_update(yrs::Update::decode_v1(update).unwrap())
            .unwrap();
    }

    /// A model that rewrites an item, a list or a quote often leaves the
    /// nested `data-id`s off: each nested block it wrote takes the id of the
    /// one it is, so what it did not change keeps its Yjs items and a
    /// person's keystroke on its way into one still lands; a ticked to-do
    /// keeps its paragraph's id for the next op.
    #[test]
    fn a_replace_without_nested_ids_keeps_the_blocks_it_did_not_change() {
        let c = Contract::load();
        let todo = "<ul data-type=\"taskList\" data-id=\"l\"><li data-type=\"taskItem\" data-checked=\"false\" data-id=\"i1\"><p data-id=\"p1\">Book seats</p></li>\
                    <li data-type=\"taskItem\" data-checked=\"false\" data-id=\"i2\"><p data-id=\"p2\">Pack bags</p></li></ul>";
        let cases: [(&str, &[u32], &str, &str, &str); 4] = [
            // The item ticked, its paragraph written without its id; the
            // person types into that paragraph.
            (
                "i1",
                &[0, 0, 0],
                " for four",
                "<li data-type=\"taskItem\" data-checked=\"true\"><p>Book seats</p></li>",
                "Book seats for four",
            ),
            // The item's text edited: paired by its place.
            ("i1", &[0, 0, 0], " today", "<li data-type=\"taskItem\" data-checked=\"false\"><p>Book the seats</p></li>", "Book the seats today"),
            // The whole list, no nested ids; the person types into the other item.
            (
                "l",
                &[0, 1, 0],
                " tonight",
                "<ul data-type=\"taskList\"><li data-type=\"taskItem\" data-checked=\"true\"><p>Book seats</p></li>\
                 <li data-type=\"taskItem\" data-checked=\"false\"><p>Pack bags</p></li></ul>",
                "Pack bags tonight",
            ),
            ("l", &[0, 1, 0], "!", "<ul data-type=\"taskList\"><li data-type=\"taskItem\"><p>Book the seats</p></li><li data-type=\"taskItem\"><p>Pack bags</p></li></ul>", "Pack bags!"),
        ];
        for (id, path, typed, html_src, expected) in cases {
            let doc = seed(&c, todo);
            let person = device(&doc);
            let keystroke = type_at_end(&person, path, typed);
            let r = apply(&c, &doc, None, &[Op::Replace { id: id.into(), html: html_src.into() }]).unwrap();
            assert_eq!(r.replaced_as, ["patch"], "{html_src}");
            apply_update(&doc, &keystroke);
            let tree = read_doc(&c, &doc.transact());
            let ids: Vec<_> = tree[0]
                .content
                .iter()
                .flat_map(|i| [i.id().map(str::to_string), i.content[0].id().map(str::to_string)])
                .collect();
            assert_eq!(ids, [Some("i1"), Some("p1"), Some("i2"), Some("p2")].map(|s| s.map(str::to_string)), "{html_src}");
            assert!(tree[0].text_content().contains(expected), "{html_src}: {}", render(&c, &doc));
        }

        // A quote: the paragraph the model left as it was keeps the person's words.
        let quote = "<blockquote data-id=\"q\"><p data-id=\"q1\">Teh log</p><p data-id=\"q2\">Once</p></blockquote>";
        let doc = seed(&c, quote);
        let keystroke = type_at_end(&device(&doc), &[0, 1], " and again");
        apply(
            &c,
            &doc,
            None,
            &[Op::Replace {
                id: "q".into(),
                html: "<blockquote><p>The log</p><p>Once</p></blockquote>".into(),
            }],
        )
        .unwrap();
        apply_update(&doc, &keystroke);
        assert_eq!(
            render(&c, &doc),
            "<blockquote data-id=\"q\"><p data-id=\"q1\">The log</p><p data-id=\"q2\">Once and again</p></blockquote>"
        );

        // An id the HTML gives another block is not taken twice.
        let doc = seed(&c, todo);
        apply(
            &c,
            &doc,
            None,
            &[Op::Replace {
                id: "l".into(),
                html: "<ul data-type=\"taskList\"><li data-type=\"taskItem\"><p>New first</p></li>\
                       <li data-type=\"taskItem\" data-id=\"i1\"><p data-id=\"p1\">Book seats</p></li></ul>"
                    .into(),
            }],
        )
        .unwrap();
        let tree = read_doc(&c, &doc.transact());
        let mut all = HashSet::new();
        collect_ids(&tree, &mut all);
        assert_eq!(all.len(), 5, "{}", render(&c, &doc));
        assert_eq!(tree[0].content[1].id(), Some("i1"));
    }

    /// A replace whose HTML reads as nothing (a comment, as a read shows a
    /// block too long for it) took its block out unasked. Only an empty
    /// replace does.
    #[test]
    fn a_replace_that_reads_as_nothing_is_refused_not_a_delete() {
        let c = Contract::load();
        let doc = seed(&c, "<p data-id=\"a\">Keep me.</p><p data-id=\"b\">Next.</p>");
        for html_src in ["<!-- a -->", "<!-- a -->\n<!-- too long to show in any read; delete it or replace it whole -->", " <!---->\n"] {
            let refused = apply(&c, &doc, None, &[Op::Replace { id: "a".into(), html: html_src.into() }]).unwrap_err();
            assert!(refused[0].message.contains("holds no blocks"), "{html_src}: {refused:?}");
            assert_eq!(render(&c, &doc), "<p data-id=\"a\">Keep me.</p><p data-id=\"b\">Next.</p>");
        }
        apply(&c, &doc, None, &[Op::Replace { id: "a".into(), html: String::new() }]).unwrap();
        assert_eq!(render(&c, &doc), "<p data-id=\"b\">Next.</p>");
    }

    /// Only the batch's end reaches the page, so a column is added or taken
    /// out by replacing every row in one batch: each row alone is the wrong
    /// width, the table at the end a grid. A batch that ends with no grid is
    /// refused, saying how to change a column.
    #[test]
    fn a_batch_that_widens_every_row_adds_a_column() {
        let c = Contract::load();
        let doc = seed(&c, EXPENSES);
        let widen = [
            Op::Replace {
                id: "r0".into(),
                html: "<tr><th><p>Day</p></th><th><p>Cost</p></th><th><p>Who</p></th></tr>".into(),
            },
            Op::Replace {
                id: "r1".into(),
                html: "<tr><td><p>Oct 2</p></td><td><p>40</p></td><td><p>Nick</p></td></tr>".into(),
            },
        ];
        apply(&c, &doc, None, &widen).unwrap();
        let tree = read_doc(&c, &doc.transact());
        assert!(tree[0].content.iter().all(|r| r.content.len() == 3), "{}", render(&c, &doc));
        assert_eq!(tree[0].content[1].id(), Some("r1"));

        let narrow = [
            Op::Replace { id: "r0".into(), html: "<tr><th><p>Day</p></th></tr>".into() },
            Op::Replace { id: "r1".into(), html: "<tr><td><p>Oct 2</p></td></tr>".into() },
        ];
        apply(&c, &doc, None, &narrow).unwrap();
        assert!(read_doc(&c, &doc.transact())[0].content.iter().all(|r| r.content.len() == 1));

        let ragged = apply(
            &c,
            &doc,
            None,
            &[Op::Replace {
                id: "r0".into(),
                html: "<tr><th><p>Day</p></th><th><p>Cost</p></th></tr>".into(),
            }],
        )
        .unwrap_err();
        assert!(ragged[0].message.contains("would not be a grid"), "{ragged:?}");
        assert!(ragged[0].message.contains("replace the table whole"), "{ragged:?}");
        assert_eq!(ragged[0].at, "op 1");
    }

    /// A column added to part of a table of more rows than a batch holds:
    /// the refusal says, in one sentence however long the table, which rows
    /// are short, and points at the one way that works for it, the table
    /// replaced whole, not at every row in one batch, which no batch holds.
    #[test]
    fn a_long_tables_grid_refusal_is_short_and_says_what_works() {
        let c = Contract::load();
        let row = |i: usize, cells: usize| {
            let tds: String = (0..cells).map(|j| format!("<td><p>{i}.{j}</p></td>")).collect();
            format!("<tr data-id=\"r{i}\">{tds}</tr>")
        };
        // More rows than a batch holds ops.
        let height = MAX_OPS + 61;
        let rows: String = (0..height).map(|i| row(i, 5)).collect();
        let doc = seed(&c, &format!("<table data-id=\"t\">{rows}</table>"));
        let widen: Vec<Op> = (0..130)
            .map(|i| Op::Replace { id: format!("r{i}"), html: row(i, 6) })
            .collect();
        let refused = apply(&c, &doc, None, &widen).unwrap_err();
        let message = &refused[0].message;
        assert!(
            message.contains(&format!("rows 131 to {height} each cover 5 of the table's 6 columns")),
            "{message}"
        );
        assert!(message.len() < 400, "{} bytes: {message}", message.len());
        assert!(message.contains("replace the table whole"), "{message}");
        assert!(!message.contains("every row in one batch"), "{message}");
    }

    /// Putting a version back writes the server's own tree: a page cleared
    /// to one line, whose version holds more than one write's 4 MiB of HTML,
    /// is put back whole. A write of that size is still refused.
    #[test]
    fn the_servers_own_tree_is_written_back_whatever_its_size() {
        let c = Contract::load();
        let doc = seed(&c, "<p data-id=\"a\"></p>");
        let html_src: String = (0..40_000).map(|i| format!("<p data-id=\"v{i:07}\">Paragraph {i} of a long page, kept as it was written before the page was cleared, every word of it.</p>")).collect();
        assert!(html_src.len() > MAX_INPUT_BYTES);
        let target = crate::ingest::parse_own_blocks(&c, &html_src).nodes;
        let ops = crate::diff::diff_ops(&c, &read_doc(&c, &doc.transact()), &target);
        assert!(apply(&c, &doc, None, &ops).is_err(), "a write past the limit");
        apply_own_in(&c, &mut doc.transact_mut(), &ops).unwrap();
        let tree = read_doc(&c, &doc.transact());
        assert_eq!(tree.len(), 40_000);
        assert_eq!(tree[39_999].id(), Some("v0039999"));
    }

    /// A model rewrites a whole list or table to change one item or cell.
    /// What it leaves as it was keeps its Yjs items, so a keystroke a person
    /// sent into another item before the rewrite reached them still lands,
    /// as it does when the model replaces the item alone.
    #[test]
    fn a_replace_of_a_list_or_table_keeps_the_children_it_does_not_change() {
        let c = Contract::load();
        let list = "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">One</p></li>\
                    <li data-id=\"i2\"><p data-id=\"p2\">Two</p></li>\
                    <li data-id=\"i3\"><p data-id=\"p3\">Three</p></li></ul>";
        let doc = seed(&c, list);
        let person = device(&doc);
        let keystroke = type_at_end(&person, &[0, 2, 0], " and four");
        let r = apply(
            &c,
            &doc,
            None,
            &[Op::Replace {
                id: "l".into(),
                html: list.replace(">One<", ">One, done<"),
            }],
        )
        .unwrap();
        assert_eq!(r.replaced_as, ["patch"]);
        apply_update(&doc, &keystroke);
        assert_eq!(
            render(&c, &doc),
            "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">One, done</p></li>\
             <li data-id=\"i2\"><p data-id=\"p2\">Two</p></li>\
             <li data-id=\"i3\"><p data-id=\"p3\">Three and four</p></li></ul>"
        );

        // A table: one cell changed, a keystroke into another.
        let doc = seed(&c, EXPENSES);
        let person = device(&doc);
        let keystroke = type_at_end(&person, &[0, 1, 0, 0], ", Friday");
        let r = apply(
            &c,
            &doc,
            None,
            &[Op::Replace {
                id: "t".into(),
                html: "<table data-id=\"t\"><tr data-id=\"r0\"><th><p data-id=\"h1\">Day</p></th><th><p data-id=\"h2\">Cost</p></th></tr>\
                       <tr data-id=\"r1\"><td><p data-id=\"c1\">Oct 2</p></td><td><p data-id=\"c2\">45</p></td></tr></table>"
                    .into(),
            }],
        )
        .unwrap();
        assert_eq!(r.replaced_as, ["patch"]);
        apply_update(&doc, &keystroke);
        let html = render(&c, &doc);
        assert!(html.contains(">Oct 2, Friday<"), "{html}");
        assert!(html.contains(">45<"), "{html}");

        // Items added, dropped and reordered land as written.
        let doc = seed(&c, list);
        apply(
            &c,
            &doc,
            None,
            &[Op::Replace {
                id: "l".into(),
                html: "<ul data-id=\"l\"><li data-id=\"i3\"><p data-id=\"p3\">Three</p></li>\
                       <li><p>New</p></li><li data-id=\"i1\"><p data-id=\"p1\">One</p></li></ul>"
                    .into(),
            }],
        )
        .unwrap();
        let tree = read_doc(&c, &doc.transact());
        let texts: Vec<String> = tree[0].content.iter().map(Node::text_content).collect();
        assert_eq!(texts, ["Three", "New", "One"]);
        assert_eq!(crate::validate::validate(&c, &tree, "doc"), []);
    }
}

/// A batch with a base takes off the page, or writes over, only what reads
/// showed as it stands: each case here lost text a read never showed, in
/// an earlier shape of the check.
#[cfg(test)]
mod unseen {
    use super::*;
    use crate::seen::{shell, Part, Shown};

    fn page(html: &str) -> Doc {
        let c = Contract::load();
        let o = crate::ingest::parse(&c, html, "doc", Mode::Strict);
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        let doc = ydoc::new_doc();
        {
            let mut txn = doc.transact_mut();
            let frag = ydoc::fragment(&c, &mut txn);
            write_nodes(&c, &mut txn, &frag, 0, &o.nodes);
        }
        doc
    }

    fn tree(doc: &Doc) -> Vec<Node> {
        read_doc(&Contract::load(), &doc.transact())
    }

    fn node(doc: &Doc, id: &str) -> Node {
        find_node(&tree(doc), id).unwrap().clone()
    }

    /// What a read of `doc` showing `parts` as HTML teaches `seen`.
    fn learn(seen: &mut Seen, doc: &Doc, parts: Vec<Part>) {
        seen.learn(&Shown::html(parts), &tree(doc), &HashSet::new());
    }

    /// `id` opened, with the blocks inside it in `shown` shown whole.
    fn opened(doc: &Doc, id: &str, shown: &[&str]) -> Part {
        let n = node(doc, id);
        let inside = n
            .content
            .iter()
            .filter(|c| c.id().is_some_and(|cid| shown.contains(&cid)))
            .cloned()
            .map(Part::Whole)
            .collect();
        Part::Opened { shell: shell(&n), inside }
    }

    fn apply_seen(doc: &Doc, seen: &Seen, ops: &[Op]) -> Result<Applied, Vec<Problem>> {
        apply(&Contract::load(), doc, Some(seen), ops)
    }

    fn checked(doc: &Doc, seen: &Seen, ops: &[Op]) -> Vec<Unseen> {
        check(&Contract::load(), &tree(doc), seen, ops).unseen
    }

    fn list(items: usize) -> String {
        let li: String = (0..items).map(|i| format!("<li data-id=\"i{i}\"><p data-id=\"p{i}\">Item {i}</p></li>")).collect();
        format!("<p data-id=\"top\">Top</p><ul data-id=\"l\">{li}</ul><p data-id=\"end\">End</p>")
    }

    fn delete(id: &str) -> Op {
        Op::Delete { id: id.into() }
    }

    fn replace(id: &str, html: &str) -> Op {
        Op::Replace { id: id.into(), html: html.into() }
    }

    /// A list opened by its first items is not replaced or deleted until
    /// reads have shown the rest; the refusal says where reading on starts.
    #[test]
    fn replacing_or_deleting_a_block_read_in_part_is_refused_with_where_to_read_on() {
        let doc = page(&list(6));
        let before = tree(&doc);
        let mut seen = Seen::default();
        learn(&mut seen, &doc, vec![opened(&doc, "l", &["i0", "i1"])]);
        for op in [delete("l"), replace("l", "<ul><li><p>Item 0</p></li><li><p>Item 1</p></li></ul>")] {
            let found = checked(&doc, &seen, std::slice::from_ref(&op));
            assert_eq!(found.len(), 1, "{op:?}: {found:?}");
            let u = &found[0];
            assert_eq!(u.ids, ["i2", "i3", "i4", "i5"]);
            assert_eq!(u.inside.as_deref(), Some("l"));
            assert_eq!(u.after.as_deref(), Some("i1"));
            assert!(u.removes);
            assert_eq!(u.why, Why::Unread { added_since: false });
            assert_eq!(
                u.message(),
                "this op would remove 4 blocks that no read has shown you, from `i2` inside `l`"
            );
            assert!(apply_seen(&doc, &seen, &[op]).is_err());
        }
        assert_eq!(tree(&doc), before);
        // Read on to the end: the delete stands.
        learn(&mut seen, &doc, (2..6).map(|i| Part::Whole(node(&doc, &format!("i{i}")))).collect());
        assert!(apply_seen(&doc, &seen, &[delete("l")]).is_ok());
    }

    /// An item an opened read counted before any read had shown it, moved
    /// out by a person, read where it went, then moved back: no read showed
    /// it inside the list. Counted while unknown, it was no block left
    /// unshown there, so once a read elsewhere taught it, the list held it as
    /// shown inside: a replace of the list wrote the model's new item over
    /// the person's under the ids `inherit_ids` gave it, and a replace or a
    /// delete leaving it out took it.
    #[test]
    fn an_item_counted_unread_then_read_elsewhere_is_not_shown_inside_the_list() {
        const L: &str = "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">one</p></li></ul>";
        const K: &str = "<li data-id=\"k\"><p data-id=\"pk\">kay</p></li>";
        const M: &str = "<ul data-id=\"m\"><li data-id=\"m1\"><p data-id=\"pm\">em</p></li>";
        let counted = page(&format!("{}{K}</ul>{M}</ul>", &L[..L.len() - 5]));
        let mut seen = Seen::default();
        // Read 1 opens the list by its first item and counts `k`.
        learn(&mut seen, &counted, vec![opened(&counted, "l", &["i1"])]);
        // A person moves `k` into the other list, and read 2 shows it there.
        let moved = page(&format!("{L}{M}{K}</ul>"));
        learn(&mut seen, &moved, vec![Part::Whole(node(&moved, "m"))]);
        assert!(seen.knows("k"));
        assert!(!seen.known("l").unwrap().shows("k"), "no read showed `k` inside `l`");
        // The person moves it back (undo).
        let doc = page(&format!("{}{K}</ul>{M}</ul>", &L[..L.len() - 5]));
        let before = tree(&doc);

        let written = "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">one</p></li><li><p>two</p></li></ul>";
        for op in [replace("l", written), replace("l", L), delete("l")] {
            let found = checked(&doc, &seen, std::slice::from_ref(&op));
            assert!(found.iter().any(|u| u.ids.contains(&"k".to_string())), "{op:?}: {found:?}");
            assert!(apply_seen(&doc, &seen, &[op]).is_err());
        }
        assert_eq!(tree(&doc), before, "a refused batch writes nothing");
        assert_eq!(node(&doc, "pk").text_content(), "kay");

        // Read on inside the list, `k` is shown there, and the list goes.
        seen.learn(&Shown::html(vec![Part::Whole(node(&doc, "k"))]).inside(Some("l")), &tree(&doc), &HashSet::new());
        assert!(apply_seen(&doc, &seen, &[delete("l")]).is_ok());
    }

    /// The same, with the block moved out to the page's top level and read
    /// there: by a read on at the top level (markdown) or an `ids` read
    /// showing it alone (HTML). A block a read showed at the top level was
    /// never placed, so the quote still held it as shown inside it: a
    /// delete or a replace leaving it out took the person's paragraph, and a
    /// replace writing a paragraph with no id wrote the model's words under
    /// its id.
    #[test]
    fn a_block_counted_unread_then_read_at_the_top_level_is_not_shown_inside_its_old_block() {
        const Q: &str = "<blockquote data-id=\"q\"><p data-id=\"a\">one</p></blockquote>";
        const K: &str = "<p data-id=\"k\">kay</p>";
        const END: &str = "<p data-id=\"end\">End</p>";
        let inside = format!("{}{K}</blockquote>{END}", &Q[..Q.len() - 13]);
        let out = format!("{Q}{K}{END}");
        type Read = fn(&Doc) -> Shown;
        let reads: [(&str, Read); 2] = [
            ("a read on at the top level", |d| {
                Shown::markdown(vec![Part::Whole(node(d, "k")), Part::Whole(node(d, "end"))])
            }),
            ("an ids read", |d| Shown::html(vec![Part::Whole(node(d, "k"))])),
        ];
        for (how, read) in reads {
            let mut seen = Seen::default();
            // Read 1 opens the quote by its first paragraph and counts `k`.
            let counted = page(&inside);
            learn(&mut seen, &counted, vec![opened(&counted, "q", &["a"])]);
            // A person moves `k` out of the quote, and a read shows it at the
            // page's top level.
            let moved = page(&out);
            seen.learn(&read(&moved), &tree(&moved), &HashSet::new());
            assert!(seen.knows("k"), "{how}");
            assert!(!seen.known("q").unwrap().shows("k"), "{how}: no read showed `k` inside `q`");
            // The person moves it back.
            let doc = page(&inside);
            let before = tree(&doc);

            let id_less = "<blockquote data-id=\"q\"><p data-id=\"a\">one</p><p>model words</p></blockquote>";
            for op in [delete("q"), replace("q", Q), replace("q", id_less)] {
                let found = checked(&doc, &seen, std::slice::from_ref(&op));
                assert!(found.iter().any(|u| u.ids.contains(&"k".to_string())), "{how}, {op:?}: {found:?}");
                assert!(apply_seen(&doc, &seen, &[op]).is_err(), "{how}");
            }
            assert_eq!(tree(&doc), before, "{how}: a refused batch writes nothing");
            assert_eq!(node(&doc, "k").text_content(), "kay");
        }
    }

    /// A delete is judged by what it takes, whatever else the batch writes:
    /// a list read in part, deleted beside a rewrite of it, is refused at the
    /// delete; one read whole, deleted beside an append, stands.
    #[test]
    fn a_delete_beside_any_write_is_judged_by_what_it_removes() {
        let doc = page(&list(4));
        let mut seen = Seen::default();
        learn(&mut seen, &doc, vec![opened(&doc, "l", &["i0"])]);
        let rewrite = Op::InsertAfter { id: "top".into(), html: "<ul><li><p>Item 0</p></li></ul>".into() };
        let refused = apply_seen(&doc, &seen, &[delete("l"), rewrite.clone()]).unwrap_err();
        assert_eq!(refused.len(), 1, "{refused:?}");
        assert_eq!(refused[0].at, "op 1");
        // The same delete in its own batch is refused just the same: the
        // rewrite cannot follow it in the next edit.
        assert!(apply_seen(&doc, &seen, &[delete("l")]).is_err());

        let read_whole = Seen::of_page(&tree(&doc));
        let applied = apply_seen(&doc, &read_whole, &[delete("l"), Op::Append { html: "<p>After</p>".into() }]);
        assert!(applied.is_ok(), "{applied:?}");
    }

    /// A paragraph too long for any read, inside a quote the read opened:
    /// a replace of the quote that leaves it out took it. Only an op naming
    /// it takes it, which the refusal says.
    #[test]
    fn a_replace_that_leaves_out_an_opaque_block_is_refused_and_naming_it_lands() {
        let doc = page("<blockquote data-id=\"q\"><p data-id=\"p1\">Teh log</p><p data-id=\"p2\">Long text</p></blockquote><p data-id=\"z\">After</p>");
        let mut seen = Seen::default();
        let p2 = node(&doc, "p2");
        learn(&mut seen, &doc, vec![Part::Opened {
            shell: shell(&node(&doc, "q")),
            inside: vec![
                Part::Whole(node(&doc, "p1")),
                Part::Listed { id: "p2".into(), say: "too long".into(), opaque: Some(crate::seen::node_hash(&p2)) },
            ],
        }]);
        let fixed = replace("q", "<blockquote><p data-id=\"p1\">The log</p></blockquote>");
        let found = checked(&doc, &seen, std::slice::from_ref(&fixed));
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].id(), &found[0].why), ("p2", &Why::NamedInside));
        // Deleting the quote takes it the same way.
        let found = checked(&doc, &seen, &[delete("q")]);
        assert_eq!((found[0].id(), &found[0].why), ("p2", &Why::NamedInside));
        // Named in an op of its own, earlier: both land.
        let applied = apply_seen(&doc, &seen, &[delete("p2"), fixed]);
        assert!(applied.is_ok(), "{applied:?}");
        assert_eq!(
            crate::to_html(&tree(&doc), true),
            "<blockquote data-id=\"q\"><p data-id=\"p1\">The log</p></blockquote><p data-id=\"z\">After</p>"
        );
    }

    /// A block too long for any read that changed since it was listed is
    /// not taken, even by an op naming it: the listing named another text.
    #[test]
    fn an_opaque_block_changed_since_it_was_listed_is_refused() {
        let doc = page("<p data-id=\"a\">One</p><pre data-id=\"code\"><code>log</code></pre>");
        let mut seen = Seen::default();
        learn(&mut seen, &doc, vec![
            Part::Whole(node(&doc, "a")),
            Part::Listed { id: "code".into(), say: "too long".into(), opaque: Some(crate::seen::node_hash(&node(&doc, "code"))) },
        ]);
        assert!(checked(&doc, &seen, &[replace("code", "<p>cut</p>")]).is_empty());
        apply(&Contract::load(), &doc, None, &[replace("code", "<pre><code>log, more</code></pre>")]).unwrap();
        let found = checked(&doc, &seen, &[replace("code", "<p>cut</p>")]);
        assert_eq!((found[0].id(), &found[0].why), ("code", &Why::ListedChanged));
    }

    /// A person's change to an item of a list read whole refuses a replace
    /// of the list written from the read, and names the item, not the list.
    #[test]
    fn an_item_changed_since_the_read_refuses_the_list_replace_and_names_the_item() {
        let doc = page(&list(3));
        let seen = Seen::of_page(&tree(&doc));
        apply(&Contract::load(), &doc, None, &[replace("p1", "<p>Item 1, packed</p>")]).unwrap();
        let rewrite = replace("l", &crate::to_html(&[seen.subtree("l").unwrap()], true).replace("Item 2", "Item two"));
        let found = checked(&doc, &seen, std::slice::from_ref(&rewrite));
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].id(), "p1");
        assert!(matches!(found[0].why, Why::Changed { .. }));
        assert!(apply_seen(&doc, &seen, &[rewrite]).is_err());
        // An item deleted since: named as deleted, and the list too.
        let doc = page(&list(3));
        let seen = Seen::of_page(&tree(&doc));
        apply(&Contract::load(), &doc, None, &[delete("i1")]).unwrap();
        let found = checked(&doc, &seen, &[replace("l", "<ul><li><p>Item 0</p></li><li><p>Item 1</p></li><li><p>Item 2</p></li></ul>")]);
        assert_eq!(found[0].id(), "l");
        assert_eq!(found[0].why, Why::Changed { deleted: vec!["i1".into()], added: vec![] });
    }

    /// An item a person added to a list the read showed whole is no item of
    /// the read: a delete of the list is refused over it, said to be new.
    #[test]
    fn an_item_added_since_refuses_a_delete_and_is_named() {
        let doc = page(&list(3));
        let seen = Seen::of_page(&tree(&doc));
        apply(&Contract::load(), &doc, None, &[Op::InsertAfter { id: "i0".into(), html: "<li><p>socks</p></li>".into() }]).unwrap();
        let added = tree(&doc)[1].content[1].id().unwrap().to_string();
        let found = checked(&doc, &seen, &[delete("l")]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].ids, std::slice::from_ref(&added));
        assert_eq!(found[0].why, Why::Unread { added_since: true });
        assert_eq!(found[0].after.as_deref(), Some("i0"));
        assert_eq!(found[0].message(), format!("this op would remove `{added}`, added to `l` after you read it"));
    }

    /// The id of a block deleted after the read, written back, is a person's
    /// deletion undone: refused.
    #[test]
    fn writing_back_the_id_of_a_block_deleted_since_is_refused() {
        let doc = page(&list(3));
        let seen = Seen::of_page(&tree(&doc));
        apply(&Contract::load(), &doc, None, &[delete("i1")]).unwrap();
        let found = checked(&doc, &seen, &[Op::InsertAfter { id: "i0".into(), html: "<li data-id=\"i1\"><p data-id=\"p1\">Item 1</p></li>".into() }]);
        assert!(found.iter().any(|u| u.why == Why::Deleted && u.id() == "i1"), "{found:?}");
    }

    /// Markdown does not say what a block's HTML is: a write that changes a
    /// block shown only so is refused, and a reorder that changes none lands.
    #[test]
    fn a_markdown_only_block_is_refused_only_where_the_write_changes_it() {
        let doc = page("<p data-id=\"a\">Lunch with <virtues-mention to=\"/person/person_1\" label=\"Nick\"></virtues-mention></p>\
                        <ul data-id=\"l\"><li data-id=\"i0\"><p data-id=\"p0\">One</p></li><li data-id=\"i1\"><p data-id=\"p1\">Two</p></li></ul>");
        let mut seen = Seen::default();
        seen.learn(&Shown::markdown(tree(&doc).into_iter().map(Part::Whole).collect()), &tree(&doc), &HashSet::new());
        let found = checked(&doc, &seen, &[replace("a", "<p>Lunch with <a href=\"/person/person_1\">@Nick</a></p>")]);
        assert_eq!((found[0].id(), &found[0].why), ("a", &Why::MarkdownOnly));
        let reordered = apply_seen(&doc, &seen, &[replace("l", "<ul><li><p>Two</p></li><li><p>One</p></li></ul>")]);
        assert!(reordered.is_ok(), "{reordered:?}");
        let texts: Vec<String> = tree(&doc)[1].content.iter().map(Node::text_content).collect();
        assert_eq!(texts, ["Two", "One"]);
    }

    /// A block the batch inserted is its own to replace in the same batch.
    #[test]
    fn a_batchs_own_inserts_are_its_own() {
        let doc = page("<p data-id=\"a\">One</p>");
        let seen = Seen::default();
        let applied = apply_seen(&doc, &seen, &[Op::Append { html: "<p data-id=\"n\">New</p>".into() }, replace("n", "<p>Newer</p>")]);
        assert!(applied.is_ok(), "{applied:?}");
    }

    /// A list no read showed, named in a replace's HTML by its id (copied
    /// from a comment that listed it), is no read of it: the replace that
    /// would write over it is refused.
    #[test]
    fn an_id_named_only_in_a_comment_does_not_carry_a_rewrite() {
        let doc = page(&format!("<blockquote data-id=\"q\"><p data-id=\"x\">Intro</p>{}</blockquote>", list(3).replace("<p data-id=\"top\">Top</p>", "").replace("<p data-id=\"end\">End</p>", "")));
        let mut seen = Seen::default();
        learn(&mut seen, &doc, vec![Part::Opened {
            shell: shell(&node(&doc, "q")),
            inside: vec![Part::Whole(node(&doc, "x")), Part::Listed { id: "l".into(), say: "read it by id".into(), opaque: None }],
        }]);
        let found = checked(&doc, &seen, &[replace("q", "<blockquote><p>Intro, edited</p><ul data-id=\"l\"><li><p>Item 0</p></li></ul></blockquote>")]);
        assert!(!found.is_empty());
        assert!(found.iter().all(|u| matches!(u.why, Why::Unread { .. })), "{found:?}");
    }

    /// A table row too long for any read is listed whole; the blocks in its
    /// cells are not, and a replace of the row that drops them is refused
    /// over them, by id, to be read.
    #[test]
    fn a_row_listed_whole_does_not_take_the_blocks_in_its_cells() {
        let doc = page(&format!(
            "<table data-id=\"t\"><tr data-id=\"r\"><td><p data-id=\"c1\">Day</p></td><td>{}</td></tr></table>",
            list(3).replace("<p data-id=\"top\">Top</p>", "").replace("<p data-id=\"end\">End</p>", "")
        ));
        let mut seen = Seen::default();
        learn(&mut seen, &doc, vec![Part::Opened {
            shell: shell(&node(&doc, "t")),
            inside: vec![Part::Listed { id: "r".into(), say: "too long".into(), opaque: Some(crate::seen::node_hash(&node(&doc, "r"))) }],
        }]);
        let found = checked(&doc, &seen, &[replace("r", "<tr><td><p>Day</p></td><td><p>Short</p></td></tr>")]);
        let named: Vec<&str> = found.iter().map(Unseen::id).collect();
        assert!(named.contains(&"l"), "{found:?}");
        assert!(found.iter().all(|u| matches!(u.why, Why::Unread { .. })), "{found:?}");
        // Read, the row's blocks go with it.
        learn(&mut seen, &doc, vec![Part::Whole(node(&doc, "c1")), Part::Whole(node(&doc, "l"))]);
        let applied = apply_seen(&doc, &seen, &[replace("r", "<tr><td><p>Day</p></td><td><p>Short</p></td></tr>")]);
        assert!(applied.is_ok(), "{applied:?}");
    }

    /// An item of a list no read showed, written over by its own replace,
    /// is named itself, not the paragraph inside it.
    #[test]
    fn an_unread_target_is_named_as_the_block_the_op_names() {
        let doc = page(&list(3));
        let mut seen = Seen::default();
        learn(&mut seen, &doc, vec![Part::Whole(node(&doc, "top"))]);
        let found = checked(&doc, &seen, &[replace("i1", "<li><p>Item 1, packed</p></li>")]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].id(), found[0].target), ("i1", true));
        assert_eq!(found[0].message(), "the block `i1` is not in the read that base names");
    }

    /// A callout read whole, with a list inside it read in part: the base
    /// once took the callout from the page whole, so deleting it took the
    /// items no read showed. The delete waits for the rest of the list.
    #[test]
    fn deleting_a_block_around_a_list_read_in_part_waits_for_the_rest_of_it() {
        let items: String = (0..5).map(|i| format!("<li data-id=\"i{i}\"><p data-id=\"p{i}\">Item {i}</p></li>")).collect();
        let doc = page(&format!(
            "<p data-id=\"top\">Top</p><aside data-id=\"c\" data-tone=\"tip\"><p data-id=\"cp\">Packing:</p><ul data-id=\"l\">{items}</ul></aside>"
        ));
        let mut seen = Seen::default();
        learn(&mut seen, &doc, vec![Part::Opened {
            shell: shell(&node(&doc, "c")),
            inside: vec![Part::Whole(node(&doc, "cp")), Part::Listed { id: "l".into(), say: "read it by id".into(), opaque: None }],
        }]);
        learn(&mut seen, &doc, vec![opened(&doc, "l", &["i0", "i1"])]);
        let found = checked(&doc, &seen, &[delete("c")]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].ids, ["i2", "i3", "i4"]);
        assert_eq!((found[0].inside.as_deref(), found[0].after.as_deref()), (Some("l"), Some("i1")));
        learn(&mut seen, &doc, (2..5).map(|i| Part::Whole(node(&doc, &format!("i{i}")))).collect());
        assert!(apply_seen(&doc, &seen, &[delete("c")]).is_ok());
    }

    /// A block too long for any read goes by an op naming it, ahead of any
    /// op on a block around it; after it, the op around it has already
    /// taken it, and is refused for that.
    #[test]
    fn a_block_too_long_for_any_read_goes_only_by_an_op_that_names_it() {
        let doc = page("<blockquote data-id=\"q\"><p data-id=\"p1\">Log</p><p data-id=\"p2\">Long text</p></blockquote><p data-id=\"z\">After</p>");
        let mut seen = Seen::default();
        learn(&mut seen, &doc, vec![Part::Opened {
            shell: shell(&node(&doc, "q")),
            inside: vec![
                Part::Whole(node(&doc, "p1")),
                Part::Listed { id: "p2".into(), say: "too long".into(), opaque: Some(crate::seen::node_hash(&node(&doc, "p2"))) },
            ],
        }]);
        let refused = apply_seen(&doc, &seen, &[delete("q"), delete("p2")]).unwrap_err();
        assert!(refused.iter().any(|p| p.at == "op 1" && p.message.contains("too long for any read")), "{refused:?}");
        assert!(refused.iter().any(|p| p.at == "op 2" && p.message == "no block has id `p2`"), "{refused:?}");
        let applied = apply_seen(&doc, &seen, &[delete("p2"), delete("q")]);
        assert!(applied.is_ok(), "{applied:?}");
        assert_eq!(crate::to_html(&tree(&doc), true), "<p data-id=\"z\">After</p>");
    }

    /// A block shown only as markdown, taken off the page while the batch
    /// writes a link to a ref, is a move written from its markdown: the
    /// mention in it would come back as a link. Taken alone, it goes.
    #[test]
    fn a_block_shown_as_markdown_is_not_moved_from_its_markdown() {
        let doc = page("<p data-id=\"a\">Lunch with <virtues-mention to=\"/person/person_1\" label=\"Nick\"></virtues-mention></p><p data-id=\"b\">Coffee</p>");
        let mut seen = Seen::default();
        seen.learn(&Shown::markdown(tree(&doc).into_iter().map(Part::Whole).collect()), &tree(&doc), &HashSet::new());
        let moved = [delete("a"), Op::InsertAfter { id: "b".into(), html: "<p>Lunch with <a href=\"/person/person_1\">@Nick</a></p>".into() }];
        let found = checked(&doc, &seen, &moved);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].id(), &found[0].why, found[0].removes), ("a", &Why::MarkdownOnly, true));
        assert!(checked(&doc, &seen, &[delete("a")]).is_empty());
        // Read as HTML, the move stands: the writer has seen the mention.
        learn(&mut seen, &doc, vec![Part::Whole(node(&doc, "a"))]);
        assert!(apply_seen(&doc, &seen, &moved).is_ok());
    }

    /// A footnote shown as markdown is its marker, its note a line at the
    /// page's end, and a figure a fence: a block holding one, taken off the
    /// page while the batch writes its marker as text or its fence as code,
    /// is moved from its markdown, and comes back without its source or as
    /// code. A veil written back as its brackets reads the same, and stands.
    #[test]
    fn a_footnoted_paragraph_shown_as_markdown_is_not_taken_by_its_text() {
        let cases = [
            (
                "<p data-id=\"a\">Lunch.<virtues-footnote label=\"ev-1\" note=\"Message · 12:10 PM\"></virtues-footnote></p>",
                "<p>Lunch.[^ev-1]</p>",
            ),
            (
                "<virtues-figure data-id=\"a\" kind=\"quote\" text=\"Save me a seat.\"></virtues-figure>",
                "<pre data-language=\"figure\">kind: quote\ntext: Save me a seat.</pre>",
            ),
        ];
        for (block, written) in cases {
            let doc = page(&format!("<p data-id=\"p0\">Intro.</p>{block}<p data-id=\"b\">End.</p>"));
            let mut seen = Seen::default();
            seen.learn(&Shown::markdown(tree(&doc).into_iter().map(Part::Whole).collect()), &tree(&doc), &HashSet::new());
            let moved = [delete("a"), Op::InsertBefore { id: "p0".into(), html: written.into() }];
            let found = checked(&doc, &seen, &moved);
            assert_eq!(found.len(), 1, "{block}: {found:?}");
            assert_eq!((found[0].id(), &found[0].why, found[0].removes), ("a", &Why::MarkdownOnly, true), "{block}");
            assert!(checked(&doc, &seen, &[delete("a")]).is_empty(), "{block}");
        }
        let doc = page("<p data-id=\"p0\">Intro.</p><p data-id=\"a\">At <virtues-veil>the clinic</virtues-veil>.</p>");
        let mut seen = Seen::default();
        seen.learn(&Shown::markdown(tree(&doc).into_iter().map(Part::Whole).collect()), &tree(&doc), &HashSet::new());
        let moved = [delete("a"), Op::InsertBefore { id: "p0".into(), html: "<p>At ⟦the clinic⟧.</p>".into() }];
        assert!(checked(&doc, &seen, &moved).is_empty());
    }

    /// Audio, a video, a file and a callout shown only as markdown read as
    /// an image and a quote: moved by a delete and an insert written from
    /// that markdown, each came back as the other kind, the player gone.
    #[test]
    fn a_widget_shown_as_markdown_is_not_moved_from_its_markdown() {
        let src = "/drive/file_0123456789abcdef";
        let cases = [
            (format!("<audio data-id=\"a\" src=\"{src}\" data-name=\"memo.m4a\"></audio>"), format!("<img src=\"{src}\" alt=\"memo.m4a\">")),
            (format!("<video data-id=\"a\" src=\"{src}\" data-name=\"walk.mp4\"></video>"), format!("<img src=\"{src}\" alt=\"walk.mp4\">")),
            (
                format!("<virtues-file data-id=\"a\" src=\"{src}\" data-name=\"plan.pdf\"></virtues-file>"),
                format!("<img src=\"{src}\" alt=\"plan.pdf\">"),
            ),
            (
                "<aside data-id=\"a\" data-tone=\"warning\"><p data-id=\"w\">Mind the step.</p></aside>".to_string(),
                "<blockquote><p>[!WARNING]<br>Mind the step.</p></blockquote>".to_string(),
            ),
        ];
        for (block, written) in cases {
            let doc = page(&format!("<p data-id=\"p0\">Intro.</p>{block}<p data-id=\"b\">End.</p>"));
            let mut seen = Seen::default();
            seen.learn(&Shown::markdown(tree(&doc).into_iter().map(Part::Whole).collect()), &tree(&doc), &HashSet::new());
            let moved = [delete("a"), Op::InsertBefore { id: "p0".into(), html: written }];
            let found = checked(&doc, &seen, &moved);
            assert_eq!(found.len(), 1, "{block}: {found:?}");
            assert_eq!((found[0].id(), &found[0].why, found[0].removes), ("a", &Why::MarkdownOnly, true), "{block}");
            assert!(checked(&doc, &seen, &[delete("a")]).is_empty(), "{block}");
        }
        // An image's markdown says all an image is: moved from it, it stands.
        let doc = page(&format!("<p data-id=\"p0\">Intro.</p><img data-id=\"a\" src=\"{src}\" alt=\"harbour.png\"><p data-id=\"b\">End.</p>"));
        let mut seen = Seen::default();
        seen.learn(&Shown::markdown(tree(&doc).into_iter().map(Part::Whole).collect()), &tree(&doc), &HashSet::new());
        let moved = [delete("a"), Op::InsertBefore { id: "p0".into(), html: format!("<img src=\"{src}\" alt=\"harbour.png\">") }];
        assert!(checked(&doc, &seen, &moved).is_empty());
    }

    /// Blocks unread in several places of what one op takes are one
    /// problem, said from the first.
    #[test]
    fn one_ops_unread_blocks_are_one_problem() {
        let lists: String = (0..3)
            .map(|l| {
                let items: String = (0..3).map(|i| format!("<li data-id=\"i{l}{i}\"><p data-id=\"p{l}{i}\">Item {i}</p></li>")).collect();
                format!("<li data-id=\"o{l}\"><p data-id=\"h{l}\">Group {l}</p><ul data-id=\"l{l}\">{items}</ul></li>")
            })
            .collect();
        let doc = page(&format!("<ul data-id=\"outer\">{lists}</ul><p data-id=\"z\">After</p>"));
        let mut seen = Seen::default();
        learn(&mut seen, &doc, vec![Part::Opened {
            shell: shell(&node(&doc, "outer")),
            inside: (0..3)
                .map(|l| Part::Opened {
                    shell: shell(&node(&doc, &format!("o{l}"))),
                    inside: vec![Part::Whole(node(&doc, &format!("h{l}"))), opened(&doc, &format!("l{l}"), &[&format!("i{l}0")])],
                })
                .collect(),
        }]);
        let found = checked(&doc, &seen, &[delete("outer")]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].ids, ["i01", "i02", "i11", "i12", "i21", "i22"]);
        assert_eq!(
            found[0].message(),
            "this op would remove 6 blocks that no read has shown you, from `i01` inside `l0`"
        );
    }

    /// An item a person moved into a block the read showed without it (Tab
    /// on it, which nests it under the item above, keeping its id) goes
    /// with that block only once a read has shown it there: a delete of the
    /// block is refused over it, as a replace of it is, though a read showed
    /// the item where it was.
    #[test]
    fn a_block_moved_into_one_read_without_it_refuses_a_delete_of_that_one() {
        let read = page(
            "<ul data-id=\"l\"><li data-id=\"a\"><p data-id=\"pa\">A</p></li>\
             <li data-id=\"b\"><p data-id=\"pb\">B</p><ul data-id=\"n\"><li data-id=\"n1\"><p data-id=\"pn1\">N1</p></li></ul></li>\
             <li data-id=\"c\"><p data-id=\"pc\">C</p></li></ul>",
        );
        let seen = Seen::of_page(&tree(&read));
        let doc = page(
            "<ul data-id=\"l\"><li data-id=\"a\"><p data-id=\"pa\">A</p></li>\
             <li data-id=\"b\"><p data-id=\"pb\">B</p><ul data-id=\"n\"><li data-id=\"n1\"><p data-id=\"pn1\">N1</p></li>\
             <li data-id=\"c\"><p data-id=\"pc\">C</p></li></ul></li></ul>",
        );
        let found = checked(&doc, &seen, &[delete("b")]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].ids, ["c"]);
        assert_eq!(found[0].why, Why::Unread { added_since: true });
        assert_eq!(found[0].inside.as_deref(), Some("n"));
        assert!(found[0].removes);
        assert_eq!(found[0].message(), "this op would remove `c`, added to `n` after you read it");
        let written = crate::to_html(&[seen.subtree("b").unwrap()], true);
        assert!(!checked(&doc, &seen, &[replace("b", &written)]).is_empty());

        // Named, it goes: the writer means it.
        assert!(checked(&doc, &seen, &[delete("c"), delete("b")]).is_empty());
        // Read where it is now, it goes with the block around it.
        let mut now = seen.clone();
        learn(&mut now, &doc, vec![Part::Whole(node(&doc, "n"))]);
        assert!(apply_seen(&doc, &now, &[delete("b")]).is_ok());
    }

    /// A paragraph joined into the quote above it (Backspace at its start)
    /// is taken with the quote only once a read has shown it there.
    #[test]
    fn a_paragraph_joined_into_a_quote_since_the_read_refuses_a_delete_of_the_quote() {
        let read = page(
            "<blockquote data-id=\"q\"><p data-id=\"x\">Quoted</p></blockquote><p data-id=\"p\">Mine</p><p data-id=\"z\">After</p>",
        );
        let seen = Seen::of_page(&tree(&read));
        let doc = page(
            "<blockquote data-id=\"q\"><p data-id=\"x\">Quoted</p><p data-id=\"p\">Mine</p></blockquote><p data-id=\"z\">After</p>",
        );
        let found = checked(&doc, &seen, &[delete("q")]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].id(), &found[0].why), ("p", &Why::Unread { added_since: true }));
        assert!(apply_seen(&doc, &seen, &[delete("q")]).is_err());
        assert_eq!(tree(&doc).len(), 2, "a refused batch writes nothing");
    }

    /// A replace that names a block means it to stay, and is no word to
    /// take it with a block around it: a paragraph a person joined into a
    /// quote, replaced by name in the batch that deletes the quote, went
    /// with the quote, the person's paragraph and the update both lost;
    /// so did an item indented into the list a delete took. A delete, or a
    /// replace with nothing, takes it by name.
    #[test]
    fn a_replace_naming_a_block_moved_in_does_not_take_it_with_the_block_around_it() {
        let read = page(
            "<blockquote data-id=\"q\"><p data-id=\"x\">Quoted</p></blockquote><p data-id=\"p\">Mine</p><p data-id=\"z\">After</p>",
        );
        let seen = Seen::of_page(&tree(&read));
        let doc = page(
            "<blockquote data-id=\"q\"><p data-id=\"x\">Quoted</p><p data-id=\"p\">Mine</p></blockquote><p data-id=\"z\">After</p>",
        );
        let ops = [replace("p", "<p>Mine, updated</p>"), delete("q")];
        let found = checked(&doc, &seen, &ops);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].id(), &found[0].why, found[0].op), ("p", &Why::Unread { added_since: true }, 1));
        assert!(apply_seen(&doc, &seen, &ops).is_err());
        assert_eq!(node(&doc, "p").text_content(), "Mine");
        assert!(checked(&doc, &seen, &[replace("p", ""), delete("q")]).is_empty());
        assert!(checked(&doc, &seen, &[delete("p"), delete("q")]).is_empty());

        let read = page(
            "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">Pack</p><ul data-id=\"n\"><li data-id=\"n1\"><p data-id=\"q1\">Sub</p></li></ul></li>\
             <li data-id=\"i2\"><p data-id=\"p2\">Book the ferry</p></li></ul>",
        );
        let seen = Seen::of_page(&tree(&read));
        let doc = page(
            "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">Pack</p><ul data-id=\"n\"><li data-id=\"n1\"><p data-id=\"q1\">Sub</p></li>\
             <li data-id=\"i2\"><p data-id=\"p2\">Book the ferry</p></li></ul></li></ul>",
        );
        for ops in [
            [replace("i2", "<li><p>Book the ferry for Friday</p></li>"), delete("n")],
            [replace("p2", "<p>Book the ferry for Friday</p>"), delete("n")],
        ] {
            assert!(!checked(&doc, &seen, &ops).is_empty(), "{ops:?}");
            assert!(apply_seen(&doc, &seen, &ops).is_err(), "{ops:?}");
        }
        assert_eq!(node(&doc, "p2").text_content(), "Book the ferry");
    }

    /// A text block merged with a person's change, then written over again
    /// by a later op of the same batch, a replace of the list around it
    /// from the writer's copy: the merge shielded the copy, which landed
    /// over the person's words. Judged as written over, it is refused.
    #[test]
    fn a_merged_block_a_later_op_writes_over_is_judged_as_written_over() {
        let doc = page(
            "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">Book the ferry seats</p></li>\
             <li data-id=\"i2\"><p data-id=\"p2\">Pack the bags</p></li></ul><p data-id=\"z\">End</p>",
        );
        // `i2` read alone, then the list cut down to the way to `p1`: the
        // list is known with `i2` not shown in it.
        let mut seen = Seen::default();
        learn(&mut seen, &doc, vec![Part::Whole(node(&doc, "i2"))]);
        learn(&mut seen, &doc, vec![opened(&doc, "l", &["i1"])]);
        assert!(!seen.complete_over(&node(&doc, "l")));
        apply(&Contract::load(), &doc, None, &[replace("p1", "<p>Reserve the ferry seats</p>")]).unwrap();

        let copy = "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">Book the ferry tickets</p></li>\
                    <li data-id=\"i2\"><p data-id=\"p2\">Pack the bags</p></li></ul>";
        let ops = [replace("p1", "<p>Book the ferry tickets</p>"), replace("l", copy)];
        let found = checked(&doc, &seen, &ops);
        assert!(found.iter().any(|u| u.id() == "p1" && u.op == 1), "{found:?}");
        assert!(apply_seen(&doc, &seen, &ops).is_err());
        assert_eq!(node(&doc, "p1").text_content(), "Reserve the ferry seats");

        // The fix alone merges with the person's word.
        let applied = apply_seen(&doc, &seen, &ops[..1]).unwrap();
        assert_eq!(applied.merged, ["p1"]);
        assert_eq!(node(&doc, "p1").text_content(), "Reserve the ferry tickets");
    }

    /// A paragraph a person joined into a quote too long for one read is
    /// not taken with the quote on a read that opened it and counted the
    /// paragraph without showing it: the opened read's shell named every
    /// block inside, and a stub stood as a read showing it there, so the
    /// paragraph read at the top level went with the quote unseen.
    #[test]
    fn an_opened_read_that_counts_a_block_moved_in_does_not_show_it_there() {
        let quote: String = (0..8).map(|i| format!("<p data-id=\"q{i}\">Line {i}</p>")).collect();
        let read = page(&format!("<blockquote data-id=\"q\">{quote}</blockquote><p data-id=\"x\">Mine</p><p data-id=\"z\">After</p>"));
        let mut seen = Seen::of_page(&tree(&read));
        let copy = crate::to_html(&[seen.subtree("q").unwrap()], true);
        let doc = page(&format!(
            "<blockquote data-id=\"q\">{quote}<p data-id=\"x\">Mine</p></blockquote><p data-id=\"z\">After</p>"
        ));
        assert!(!checked(&doc, &seen, &[replace("q", &copy)]).is_empty());

        // Opened, its first lines shown and the rest, the joined one among
        // them, counted.
        learn(&mut seen, &doc, vec![opened(&doc, "q", &["q0", "q1", "q2"])]);
        for op in [replace("q", &copy), delete("q")] {
            let found = checked(&doc, &seen, std::slice::from_ref(&op));
            assert!(found.iter().any(|u| u.ids.contains(&"x".to_string())), "{op:?}: {found:?}");
            assert!(apply_seen(&doc, &seen, &[op]).is_err());
        }
        assert!(find_node(&tree(&doc), "x").is_some());

        // Read on inside the quote, it is shown there, and goes with it.
        seen.learn(
            &Shown::html((3..8).map(|i| Part::Whole(node(&doc, &format!("q{i}")))).chain([Part::Whole(node(&doc, "x"))]).collect())
                .inside(Some("q")),
            &tree(&doc),
            &HashSet::new(),
        );
        assert!(apply_seen(&doc, &seen, &[delete("q")]).is_ok());
    }

    /// A block a markdown read showed and an HTML read listed as too long
    /// for any read goes only by an op naming it, never with a block around
    /// it, even with a link to a ref in the batch: what the markdown showed
    /// was judged first, and the block went unnamed, or the refusal asked
    /// for an HTML read that could only list it again.
    #[test]
    fn a_block_shown_as_markdown_and_listed_goes_only_by_name() {
        let doc = page(
            "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">One</p>\
             <pre data-id=\"code\"><code>a &lt; b &amp;&amp; c &gt; d</code></pre></li>\
             <li data-id=\"i2\"><p data-id=\"p2\">Two</p></li></ul><p data-id=\"z\">After</p>",
        );
        let mut seen = Seen::default();
        seen.learn(&Shown::markdown(tree(&doc).into_iter().map(Part::Whole).collect()), &tree(&doc), &HashSet::new());
        let listed = Part::Listed {
            id: "code".into(),
            say: "too long".into(),
            opaque: Some(crate::seen::node_hash(&node(&doc, "code"))),
        };
        learn(&mut seen, &doc, vec![Part::Opened {
            shell: shell(&node(&doc, "l")),
            inside: vec![
                Part::Opened { shell: shell(&node(&doc, "i1")), inside: vec![Part::Whole(node(&doc, "p1")), listed] },
                Part::Whole(node(&doc, "i2")),
            ],
        }]);
        let link = Op::Append { html: "<p><a href=\"/person/person_1\">@Nick</a></p>".into() };
        for ops in [vec![delete("l")], vec![delete("i1")], vec![delete("l"), link.clone()]] {
            let found = checked(&doc, &seen, &ops);
            assert_eq!(found.len(), 1, "{ops:?}: {found:?}");
            assert_eq!((found[0].id(), &found[0].why), ("code", &Why::NamedInside), "{ops:?}");
            assert!(apply_seen(&doc, &seen, &ops).is_err());
        }
        assert!(find_node(&tree(&doc), "code").is_some());
        assert!(apply_seen(&doc, &seen, &[delete("code"), delete("l"), link]).is_ok());
    }

    /// A person's change to a text block's attribute since the read is
    /// merged with the writer's change to its text, and an attribute both
    /// changed is refused: the person's level and language were written
    /// over, said only as merged.
    #[test]
    fn an_attribute_both_changed_since_the_read_is_refused_and_one_alone_is_kept() {
        let c = Contract::load();
        let doc = page("<h2 data-id=\"h\">Plan for the trip</h2><pre data-id=\"k\" data-language=\"python\"><code>x = 1</code></pre>");
        let seen = Seen::of_page(&tree(&doc));
        apply(&c, &doc, None, &[
            replace("h", "<h3>Plan for the trip</h3>"),
            replace("k", "<pre data-language=\"rust\"><code>x = 1</code></pre>"),
        ])
        .unwrap();
        for op in [replace("h", "<h1>Plan for the holiday</h1>"), replace("k", "<pre data-language=\"go\"><code>x = 2</code></pre>")] {
            let refused = apply_seen(&doc, &seen, std::slice::from_ref(&op)).unwrap_err();
            assert!(refused[0].message.starts_with("someone changed the block's "), "{refused:?}");
        }
        assert_eq!(
            crate::to_html(&tree(&doc), true),
            "<h3 data-id=\"h\">Plan for the trip</h3><pre data-id=\"k\" data-language=\"rust\">x = 1</pre>"
        );
        // Only the text changed here, only the level there: both stand.
        apply_seen(&doc, &seen, &[replace("h", "<h2>Plan for the holiday</h2>")]).unwrap();
        assert_eq!(node(&doc, "h").attrs["level"], 3);
        assert_eq!(node(&doc, "h").text_content(), "Plan for the holiday");
    }

    /// A block a markdown read showed whole, and an HTML read then listed
    /// as too long for any read (code, whose HTML is far longer than its
    /// markdown), is replaced by an op naming it while it stands as listed,
    /// as a block only ever listed is: no read could show it as HTML.
    #[test]
    fn a_block_shown_as_markdown_then_listed_is_replaced_by_name() {
        let doc = page("<p data-id=\"a\">One</p><pre data-id=\"code\"><code>a &lt; b &amp;&amp; c &gt; d</code></pre>");
        let mut seen = Seen::default();
        seen.learn(&Shown::markdown(tree(&doc).into_iter().map(Part::Whole).collect()), &tree(&doc), &HashSet::new());
        learn(&mut seen, &doc, vec![Part::Listed {
            id: "code".into(),
            say: "too long".into(),
            opaque: Some(crate::seen::node_hash(&node(&doc, "code"))),
        }]);
        assert_eq!(seen.known("code").map(|k| k.form), Some(crate::seen::Form::Markdown));
        let applied = apply_seen(&doc, &seen, &[replace("code", "<pre><code>a &lt; b</code></pre>")]);
        assert!(applied.is_ok(), "{applied:?}");
        assert_eq!(node(&doc, "code").text_content(), "a < b");
        // A block around it still needs a read of its HTML to write over it.
        let doc = page("<blockquote data-id=\"q\"><pre data-id=\"code\"><code>x &lt; y</code></pre></blockquote>");
        let mut seen = Seen::default();
        seen.learn(&Shown::markdown(tree(&doc).into_iter().map(Part::Whole).collect()), &tree(&doc), &HashSet::new());
        learn(&mut seen, &doc, vec![Part::Listed {
            id: "code".into(),
            say: "too long".into(),
            opaque: Some(crate::seen::node_hash(&node(&doc, "code"))),
        }]);
        assert!(!checked(&doc, &seen, &[replace("q", "<blockquote><pre><code>x &lt; z</code></pre></blockquote>")]).is_empty());
    }

    /// A refusal over a block too long for any read, inside one an op
    /// takes, says to delete it by its id in an op before. When
    /// it is all its parent holds (an item's one paragraph, a cell's one
    /// block), deleting it first leaves the parent empty until the op after
    /// takes the parent too: judged once the batch is done, that lands.
    #[test]
    fn deleting_a_parents_only_block_before_the_parent_lands() {
        let big = |doc: &Doc| Part::Listed {
            id: "big".into(),
            say: "too long".into(),
            opaque: Some(crate::seen::node_hash(&node(doc, "big"))),
        };
        let doc = page(
            "<ul data-id=\"l\"><li data-id=\"i0\"><p data-id=\"p0\">Item 0</p></li>\
             <li data-id=\"i1\"><p data-id=\"big\">Long</p></li></ul><p data-id=\"end\">End</p>",
        );
        let mut seen = Seen::default();
        learn(&mut seen, &doc, vec![Part::Opened {
            shell: shell(&node(&doc, "l")),
            inside: vec![
                Part::Whole(node(&doc, "i0")),
                Part::Opened { shell: shell(&node(&doc, "i1")), inside: vec![big(&doc)] },
            ],
        }]);
        let found = checked(&doc, &seen, &[delete("i1")]);
        assert_eq!((found[0].id(), &found[0].why), ("big", &Why::NamedInside));
        let alone = apply_seen(&doc, &seen, &[delete("big")]).unwrap_err();
        assert!(alone[0].message.contains("`listItem` would hold nothing"), "{alone:?}");
        let applied = apply_seen(&doc, &seen, &[delete("big"), delete("i1")]);
        assert!(applied.is_ok(), "{applied:?}");
        assert!(find_node(&tree(&doc), "i1").is_none());

        let doc = page(
            "<table data-id=\"t\"><tr data-id=\"r0\"><td><p data-id=\"a\">A</p></td><td><p data-id=\"b\">B</p></td></tr>\
             <tr data-id=\"r1\"><td><p data-id=\"c\">C</p></td><td><p data-id=\"big\">Long</p></td></tr></table>",
        );
        let mut seen = Seen::default();
        learn(&mut seen, &doc, vec![Part::Opened {
            shell: shell(&node(&doc, "t")),
            inside: vec![
                Part::Whole(node(&doc, "r0")),
                Part::Opened { shell: shell(&node(&doc, "r1")), inside: vec![] },
            ],
        }]);
        learn(&mut seen, &doc, vec![Part::Whole(node(&doc, "c")), big(&doc)]);
        let applied = apply_seen(&doc, &seen, &[delete("big"), delete("r1")]);
        assert!(applied.is_ok(), "{applied:?}");
        assert_eq!(node(&doc, "t").content.len(), 1);
    }
}

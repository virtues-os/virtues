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
use crate::ydoc::{self, read_doc, read_text, value_to_any, write_nodes};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
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
fn inherit_ids(c: &Contract, old: &Node, new: &mut Node, written: &HashSet<String>) {
    if old.kind != new.kind || c.is_textblock(&new.kind) {
        return;
    }
    let free = |n: &Node| n.id().is_none_or(|id| !written.contains(id));
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
            inherit_ids(c, was, child, written);
        }
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
/// when they touch different stretches of the text; otherwise refuse.
fn merge_block(c: &Contract, base: &Node, current: &Node, model: &Node) -> Result<Node, String> {
    let same_shape =
        base.kind == current.kind && current.kind == model.kind && c.is_textblock(&model.kind);
    if !same_shape {
        return Err("the block changed since you read it; read it again".into());
    }
    let mut attrs = current.attrs.clone();
    for (k, v) in &model.attrs {
        if base.attrs.get(k) != Some(v) {
            attrs.insert(k.clone(), v.clone());
        }
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
    base: Option<&[Node]>,
    ops: &[Op],
) -> Result<Applied, Vec<Problem>> {
    let mut txn = doc.transact_mut();
    apply_in(c, &mut txn, base, ops)
}

/// Apply a batch inside the caller's transaction, which makes the read and
/// the write one step: nothing can change the document between them.
///
/// `base`: the tree the model read (as [`read_doc`] returns it), when it says
/// which version it read. A block that changed since is merged with the
/// model's edit or refused. Without it, a replace wins over whatever the
/// block holds now.
pub fn apply_in(
    c: &Contract,
    txn: &mut TransactionMut,
    base: Option<&[Node]>,
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

fn apply_checked(
    c: &Contract,
    txn: &mut TransactionMut,
    base: Option<&[Node]>,
    ops: &[Op],
    own: bool,
) -> Result<Applied, Vec<Problem>> {
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
        return Err(vec![Problem::new(
            "ops",
            format!(
                "the ops hold {html_bytes} bytes of HTML; one write holds at most {} MiB",
                MAX_INPUT_BYTES >> 20
            ),
        )]);
    }
    let mut root = Node::element(&c.fragment, Default::default(), read_doc(c, &*txn));
    let nodes_before = crate::node_count(&root.content);
    let mut taken = HashSet::new();
    collect_ids(&root.content, &mut taken);
    let mut errors = vec![];
    let mut applied = Applied::default();
    let mut plans = vec![];
    // The tables the batch edits rows of, by id, and the last op to: each is
    // checked for a grid once the batch is done.
    let mut tables: Vec<(String, String)> = vec![];

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
            let keep = if matches!(kind, PlanKind::Replace) {
                target.map(String::as_str)
            } else {
                None
            };
            if let (Some(k), [model]) = (keep, nodes.as_mut_slice()) {
                if let Some(old) = find_node(&root.content, k) {
                    let mut written = HashSet::new();
                    collect_ids(std::slice::from_ref(model), &mut written);
                    inherit_ids(c, old, model, &written);
                }
                // The block as the model read it: the base's, or the page's
                // when the batch names none.
                let read = base.and_then(|b| find_node(b, k)).or_else(|| find_node(&root.content, k));
                if let (Some(read), false) = (read, own) {
                    keep_typed_space(c, read, model);
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

        if let (Some(base), Some(id)) = (base, target) {
            let then = find_node(base, id).map(|n| strip_ids_deep(c, n));
            let now = find_node(&root.content, id).map(|n| strip_ids_deep(c, n));
            if let (Some(then), Some(now)) = (then, now) {
                if then != now {
                    let merged = match (&kind, nodes.as_slice()) {
                        (PlanKind::Replace, [model]) => {
                            merge_block(c, &then, &now, &strip_ids_deep(c, model)).map(Some)
                        }
                        (PlanKind::Replace | PlanKind::Delete, _) => {
                            Err("the block changed since you read it; read it again".into())
                        }
                        _ => Ok(None),
                    };
                    match merged {
                        Ok(Some(mut m)) => {
                            m.attrs.insert(c.id.attr.clone(), Value::String(id.clone()));
                            applied.notes.push(Problem::new(
                                &at,
                                "merged with a concurrent edit to this block",
                            ));
                            nodes = vec![m];
                        }
                        Ok(None) => {}
                        Err(message) => {
                            errors.push(Problem::new(&at, message));
                            continue;
                        }
                    }
                }
            }
        }

        let idx = *path.last().expect("a path names at least the index");
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
        let kinds: Vec<&str> = parent.content.iter().map(|n| n.kind.as_str()).collect();
        if !c.content_matches(&parent.kind, &kinds) {
            errors.push(Problem::new(&at, misfit(c, &parent.kind, &kinds, &nodes)));
            continue;
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

    if !errors.is_empty() {
        return Err(errors);
    }

    let frag = ydoc::fragment(c, txn);
    for plan in plans {
        if let Some(how) = write_plan(c, txn, &frag, plan) {
            applied.replaced_as.push(how.to_string());
        }
    }
    Ok(applied)
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

/// Same-type text block holding the same inline atoms in the same order:
/// update attributes in place and write only the changed middle of each
/// run of text between atoms. The atoms, and every run that did not change,
/// keep their Yjs items, so a caret anywhere else in the block holds.
fn patch_textblock(c: &Contract, txn: &mut TransactionMut, el: &XmlElementRef, new: &Node) -> bool {
    if el.tag().as_ref() != new.kind || !c.is_textblock(&new.kind) {
        return false;
    }
    // The block's children as runs of text between atoms, each atom with
    // its index among the children.
    let mut runs: Vec<Vec<XmlTextRef>> = vec![vec![]];
    let mut atoms: Vec<(u32, Node)> = vec![];
    for (i, child) in ydoc::children(txn, el) {
        match child {
            Out::YXmlText(t) => runs.last_mut().expect("one run at least").push(t),
            Out::YXmlElement(a) => {
                atoms.push((i, atom_node(txn, &a)));
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
            .all(|((_, a), b)| same_atom(a, b))
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
/// as [`crate::diff_ops`] pairs a page's blocks, by id, or by type and text
/// for one without (a table's cells). A paired child is patched the same
/// way, a text block by its text (`patch_textblock`), and swapped alone when
/// it cannot be; a child `new` lacks goes, and one only `new` has is written
/// where it goes. What did not change keeps its Yjs items, so a person's
/// keystroke still on its way into it lands. False, with nothing written,
/// when `el` is not of `new`'s type or holds anything but blocks.
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
        let r = apply(&c, &doc, Some(&base), &[Op::Replace { id: "a".into(), html: shown(&base).replace("apples", "pears") }])
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
        let r = apply(&c, &doc, Some(&base), &[Op::Replace { id: "a".into(), html: shown(&base).replace("Total", "Sum") }])
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
            Some(&base),
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
            Some(&base2),
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
            Some(&base3),
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
        apply(&c, &doc, Some(&base), &[Op::Replace { id: "a".into(), html: "<p>Dinner with <virtues-mention to=\"/person/1\" label=\"Nick\"></virtues-mention> on Friday.</p>".into() }]).unwrap();
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

    #[test]
    fn a_changed_atom_swaps_the_block() {
        let c = Contract::load();
        for (before, after) in [
            (
                "a <virtues-mention to=\"/person/person_1\" label=\"Nick\"></virtues-mention>",
                "a <virtues-mention to=\"/person/person_2\" label=\"Nick\"></virtues-mention>",
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

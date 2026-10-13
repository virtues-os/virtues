//! Trees compared block by block: the ops that turn one page into another
//! ([`diff_ops`]), for a restore that keeps every block it does not change.

use crate::contract::Contract;
use crate::migrate::{self, Dialect};
use crate::model::Node;
use crate::ops::Op;
use crate::render;
use crate::ydoc::canonicalize;
use std::collections::{HashMap, HashSet};
use std::ops::Range;

/// The most cells the pairing below fills for the blocks between a common
/// start and end that cannot be paired by id: a 2,000-block page against
/// another. Past it, those blocks are not paired, and every one of them is
/// written again: the right page, at more cost to the document.
const MAX_PAIRING_CELLS: usize = 4_000_000;

/// Ops that turn the page `current` into `target`, top-level blocks only,
/// for [`crate::apply_ops`] with no base.
///
/// Blocks pair by a longest common subsequence where equal means the same
/// id, or, for a target block without an id (a page converted from
/// markdown), the same type and text. A pair that differs is a `Replace` of
/// the current block by the target's HTML, ids and all, so the target's
/// nested ids are kept where no other block holds them. A current block left
/// unpaired is a `Delete`. A run of unpaired target blocks is one insert:
/// after the paired block before it, before the first paired block when none
/// is before it, or at the end when no block pairs.
///
/// Inserts come first, then replaces, each in target order, then deletes,
/// so the page holds a block at every step. A paired block that does not
/// change is not touched: it keeps its Yjs items, and a caret in it holds.
/// A current block without an id cannot be named by an op and stays.
pub fn diff_ops(c: &Contract, current: &[Node], target: &[Node]) -> Vec<Op> {
    let mut current = current.to_vec();
    let mut target = target.to_vec();
    canonicalize(c, &mut current);
    canonicalize(c, &mut target);
    let pairs = pair_blocks(&current, &target);
    let unchanged = |i: usize, j: usize| {
        let (cur, tgt) = (&current[i], &target[j]);
        if tgt.id().is_some() {
            cur == tgt
        } else {
            without_ids(c, cur) == without_ids(c, tgt)
        }
    };
    ops_for(c, &current, &target, &pairs, &unchanged)
}

/// The ops that turn `current` into `target` once their blocks are paired
/// (`pairs`, in order): a pair `unchanged` says is the same is left alone,
/// any other is a `Replace`, and what is unpaired is inserted or deleted, as
/// [`diff_ops`] says.
fn ops_for(
    c: &Contract,
    current: &[Node],
    target: &[Node],
    pairs: &[(usize, usize)],
    unchanged: &dyn Fn(usize, usize) -> bool,
) -> Vec<Op> {
    let mut inserts = vec![];
    let mut replaces = vec![];
    let html = |nodes: &[Node]| render::html(c, nodes, &render::Options { ids: true });

    // Where each target block pairs, if it does.
    let mut paired_at: Vec<Option<usize>> = vec![None; target.len()];
    for &(i, j) in pairs {
        paired_at[j] = Some(i);
    }
    let first_paired = pairs.iter().find_map(|&(i, _)| current[i].id());
    let mut before: Option<&str> = None;
    let mut j = 0;
    while j < target.len() {
        if let Some(i) = paired_at[j] {
            if let (false, Some(id)) = (unchanged(i, j), current[i].id()) {
                replaces.push(Op::Replace {
                    id: id.to_string(),
                    html: html(std::slice::from_ref(&target[j])),
                });
            }
            before = current[i].id().or(before);
            j += 1;
            continue;
        }
        let run_end = (j..target.len())
            .find(|&k| paired_at[k].is_some())
            .unwrap_or(target.len());
        let blocks = html(&target[j..run_end]);
        inserts.push(match (before, first_paired) {
            (Some(id), _) => Op::InsertAfter {
                id: id.to_string(),
                html: blocks,
            },
            (None, Some(id)) => Op::InsertBefore {
                id: id.to_string(),
                html: blocks,
            },
            (None, None) => Op::Append { html: blocks },
        });
        j = run_end;
    }

    let kept: HashSet<usize> = pairs.iter().map(|&(i, _)| i).collect();
    let deletes = current
        .iter()
        .enumerate()
        .filter(|(i, _)| !kept.contains(i))
        .filter_map(|(_, n)| n.id())
        .map(|id| Op::Delete { id: id.to_string() });

    inserts.into_iter().chain(replaces).chain(deletes).collect()
}

/// How alike two blocks' words must be, as a Dice coefficient over each
/// one's words, for a writer's changed block to be written over the page's
/// in place. A guess, to be measured on the writers' own rewrites.
const SIMILAR: f64 = 0.5;

/// Ops that write `target`, a writer's markdown converted (blocks without
/// ids, read as `dialect`, the suggestions the page shows kept:
/// [`keep_shown_suggestions`]), over the page `current`, for
/// [`crate::apply_own_ops_in`].
///
/// Each block is keyed by its markdown as the page's export shows it
/// (`AsShown`): its footnotes' notes included, so a changed source is a
/// change, and, where that differs from every block of the target, as that
/// markdown reads back, so what markdown cannot carry (a run of spaces, a
/// tab) is no change. A block the export shows as nothing (an empty
/// paragraph) is never paired, and is left where it is unless the write
/// keeps no block of the page at all. Blocks
/// with equal keys pair first, by a longest common subsequence, and a pair
/// is left alone: a block the markdown shows as it stands keeps everything,
/// what markdown cannot show included. Between two such pairs (and before
/// the first and after the last), blocks pair again where they are alike
/// ([`similar`]), and each of those pairs is a `Replace` of the page's
/// block, which `apply` writes in place: a text block by its changed text,
/// its id, its atoms and an open editor's caret kept. A list, table or
/// quote is paired the same way inside ([`revised`]): what the writer did
/// not change in it is the page's as it stands, what it changed a little
/// keeps its id, and what it added is new. What is left is inserted and
/// deleted as in [`diff_ops`].
///
/// A writer may not decide the owner's suggestions: [`suggestions_taken`]
/// names a block holding one that the ops would write over.
pub fn revise_ops(c: &Contract, current: &[Node], target: &[Node], dialect: Dialect) -> Vec<Op> {
    let mut current = current.to_vec();
    let mut target = target.to_vec();
    canonicalize(c, &mut current);
    canonicalize(c, &mut target);
    let target_keys: Vec<String> = target.iter().map(|n| key(c, n)).collect();
    let shown = AsShown::of(c, &current, &target_keys, dialect);
    // A block the export shows as nothing takes no part: it stays.
    let seen: Vec<usize> = (0..current.len()).filter(|&i| shown.keys[i].is_some()).collect();
    let same = |a: usize, j: usize| shown.keys[seen[a]].as_deref() == Some(target_keys[j].as_str());
    let exact: Vec<(usize, usize)> = pair_by(0..seen.len(), 0..target.len(), &same);

    let words: Vec<Words> = current.iter().chain(&target).map(words).collect();
    let (cur_words, tgt_words) = words.split_at(current.len());
    let alike = |a: usize, j: usize| {
        let i = seen[a];
        similar(&current[i], &target[j], &cur_words[i], &tgt_words[j])
    };
    let paired = alike_between(&exact, seen.len(), target.len(), &alike);
    let pairs: Vec<(usize, usize)> = paired.iter().map(|&(a, j, _)| (seen[a], j)).collect();
    let exact: HashSet<(usize, usize)> = paired
        .iter()
        .filter(|&&(_, _, same)| same)
        .map(|&(a, j, _)| (seen[a], j))
        .collect();
    // A changed list, table or quote keeps what the writer did not change
    // in it as it stands.
    for &(i, j) in &pairs {
        if exact.contains(&(i, j)) || c.is_textblock(&current[i].kind) || current[i].content.is_empty() {
            continue;
        }
        let merged = shown
            .read_back(c, i, dialect)
            .and_then(|read| revised(c, &current[i], &read, &target[j]))
            .filter(grids_hold);
        if let Some(merged) = merged {
            target[j] = merged;
        }
    }
    let mut ops = ops_for(c, &current, &target, &pairs, &|i, j| exact.contains(&(i, j)));
    // Kept among the blocks the write keeps or writes in place; a write that
    // keeps none is the whole page anew, and takes them with the rest (a new
    // page's one empty paragraph under its first draft).
    if !pairs.is_empty() {
        let invisible: HashSet<&str> = (0..current.len())
            .filter(|&i| shown.keys[i].is_none())
            .filter_map(|i| current[i].id())
            .collect();
        ops.retain(|op| !matches!(op, Op::Delete { id } if invisible.contains(id.as_str())));
    }
    ops
}

/// The ids of the blocks of `current` holding a suggestion its owner has
/// not decided (text under a proposal mark, at any depth) that `ops`
/// replace or delete. A writer's whole-page write that would is refused:
/// whatever it writes over such a block decides the suggestion, which is
/// its owner's to accept or reject.
pub fn suggestions_taken(current: &[Node], ops: &[Op]) -> Vec<String> {
    ops.iter()
        .filter_map(|op| match op {
            Op::Replace { id, .. } | Op::Delete { id } => Some(id.as_str()),
            _ => None,
        })
        .filter(|id| crate::ops::find_node(current, id).is_some_and(holds_suggestion))
        .map(str::to_string)
        .collect()
}

/// Whether `n` holds a suggestion its owner has not decided: text under a
/// proposal mark, at any depth.
pub fn holds_suggestion(n: &Node) -> bool {
    let mut found = false;
    n.walk(&mut |n| {
        found |= n
            .marks
            .iter()
            .any(|m| crate::contract::OWNER_MARKS.contains(&m.kind.as_str()));
    });
    found
}

/// Accept every suggestion in `target`, a writer's markdown read as
/// `dialect` with its suggestions kept, but those in a block that shows one
/// of `current`'s as it stands (`AsShown`): the page's own suggestions,
/// which a writer's markdown carries back because it is read off the page's
/// export. A suggestion anywhere else is the writer's, and suggestions are
/// the page owner's to make, in the page. Returns whether it accepted any.
pub fn keep_shown_suggestions(c: &Contract, current: &[Node], target: &mut [Node], dialect: Dialect) -> bool {
    if !target.iter().any(holds_suggestion) {
        return false;
    }
    let mut current = current.to_vec();
    canonicalize(c, &mut current);
    let mut canonical = target.to_vec();
    canonicalize(c, &mut canonical);
    let target_keys: Vec<String> = canonical.iter().map(|n| key(c, n)).collect();
    let shown = AsShown::of(c, &current, &target_keys, dialect);
    let own: HashSet<&str> = (0..current.len())
        .filter(|&i| holds_suggestion(&current[i]))
        .filter_map(|i| shown.keys[i].as_deref())
        .collect();
    let mut accepted = false;
    for (n, k) in target.iter_mut().zip(&target_keys) {
        if holds_suggestion(n) && !own.contains(k.as_str()) {
            let mut one = vec![std::mem::replace(n, Node::text("", vec![]))];
            accepted |= crate::accept_proposals(&mut one);
            *n = one.pop().unwrap_or_else(|| Node::element("paragraph", Default::default(), vec![]));
        }
    }
    accepted
}

/// A block's markdown alone: its own footnotes' definitions after it.
fn key(c: &Contract, n: &Node) -> String {
    render::markdown(c, std::slice::from_ref(n))
}

/// The page's blocks as its export shows them, each keyed for a writer's
/// markdown to be compared with: its markdown with the labels and notes the
/// page's export gives its footnotes
/// ([`render::footnotes_as_exported`]), or, where no block of the target
/// has that key, that markdown as it reads back as `dialect`. `None` for a
/// block the export writes as nothing.
struct AsShown {
    keys: Vec<Option<String>>,
    /// The page's blocks with their footnotes as its export writes them.
    blocks: Vec<Node>,
}

impl AsShown {
    fn of(c: &Contract, current: &[Node], target_keys: &[String], dialect: Dialect) -> Self {
        let blocks = render::footnotes_as_exported(current, true).unwrap_or_else(|| current.to_vec());
        let wanted: HashSet<&str> = target_keys.iter().map(String::as_str).collect();
        let keys = blocks
            .iter()
            .map(|n| {
                let k = key(c, n);
                if wanted.contains(k.as_str()) {
                    return Some(k);
                }
                if k.trim().is_empty() {
                    return None;
                }
                let mut back = migrate::from_markdown_as(c, &k, dialect).nodes;
                canonicalize(c, &mut back);
                Some(render::markdown(c, &back))
            })
            .collect();
        AsShown { keys, blocks }
    }

    /// The page's block `i` as its markdown reads back as `dialect`: what a
    /// writer reading the export read of it. `None` when that is not one
    /// block.
    fn read_back(&self, c: &Contract, i: usize, dialect: Dialect) -> Option<Node> {
        let o = migrate::from_markdown_as(c, &key(c, &self.blocks[i]), dialect);
        if !o.errors.is_empty() {
            return None;
        }
        let mut back = o.nodes;
        canonicalize(c, &mut back);
        match <[Node; 1]>::try_from(back) {
            Ok([one]) => Some(one),
            Err(_) => None,
        }
    }
}

/// The pairs [`revise_ops`] writes over a page of `n` blocks for a writer's
/// `m`, as (page index, writer's index, whether they read the same): the
/// `exact` pairs, and between two of them (and before the first and after
/// the last) the blocks [`similar`] says are alike, by a longest common
/// subsequence.
fn alike_between(
    exact: &[(usize, usize)],
    n: usize,
    m: usize,
    alike: &dyn Fn(usize, usize) -> bool,
) -> Vec<(usize, usize, bool)> {
    let mut pairs = vec![];
    let (mut i0, mut j0) = (0, 0);
    for (i, j) in exact.iter().copied().chain(std::iter::once((n, m))) {
        pairs.extend(pair_by(i0..i, j0..j, alike).into_iter().map(|(i, j)| (i, j, false)));
        if i < n {
            pairs.push((i, j, true));
        }
        (i0, j0) = (i + 1, j + 1);
    }
    pairs
}

/// The writer's block `tgt`, a list, a table, a quote or a block in one,
/// that [`revise_ops`] pairs with the page's `cur` as changed, with what
/// the writer did not change in it taken from the page. `shown` is `cur`
/// as its markdown reads back: what the writer read of it.
///
/// Its children pair as the page's blocks do. A child of `tgt` that reads
/// as its child of `shown` reads is the page's child as it stands, with
/// whatever it holds that markdown cannot show (a cell's span, its own
/// alignment, a space inside bold); one that is alike is that child
/// revised the same way, keeping its id, so an open editor's caret and a
/// keystroke on its way into it stay in it; any other is new, with ids of
/// its own, so it takes none from a child the writer dropped. An attribute
/// the writer's markdown shows as `shown` shows it is the page's. Read
/// from the markdown alone, a row the writer never touched lost its cells'
/// spans, and an item inserted beside a reworded one took that one's id
/// and live text.
///
/// `None` where a child the writer changed does not line up with its
/// markdown child for child (a row holding a cell across two columns,
/// which markdown writes as two): the writer's block is written as it
/// reads, whole.
fn revised(c: &Contract, cur: &Node, shown: &Node, tgt: &Node) -> Option<Node> {
    if cur.kind != tgt.kind || shown.kind != cur.kind {
        return None;
    }
    let mut out = tgt.clone();
    let keys: HashSet<&String> = cur.attrs.keys().chain(tgt.attrs.keys()).collect();
    for k in keys {
        if tgt.attrs.get(k) != shown.attrs.get(k) {
            continue;
        }
        match cur.attrs.get(k) {
            Some(v) => out.attrs.insert(k.clone(), v.clone()),
            None => out.attrs.remove(k),
        };
    }
    if c.is_textblock(&cur.kind) || tgt.content.is_empty() {
        return Some(out);
    }
    let (now, read, want) = (&cur.content, &shown.content, &tgt.content);
    if now.len() != read.len() || now.iter().zip(read).any(|(a, b)| a.kind != b.kind) {
        return None;
    }
    let read_bare: Vec<Node> = read.iter().map(|n| without_ids(c, n)).collect();
    let want_bare: Vec<Node> = want.iter().map(|n| without_ids(c, n)).collect();
    let exact = pair_by(0..now.len(), 0..want.len(), &|i, j| read_bare[i] == want_bare[j]);
    let words: Vec<Words> = now.iter().chain(want).map(words).collect();
    let (now_words, want_words) = words.split_at(now.len());
    let alike = |i: usize, j: usize| similar(&now[i], &want[j], &now_words[i], &want_words[j]);
    let mut paired: Vec<Option<(usize, bool)>> = vec![None; want.len()];
    for (i, j, same) in alike_between(&exact, now.len(), want.len(), &alike) {
        paired[j] = Some((i, same));
    }
    out.content = want
        .iter()
        .zip(paired)
        .map(|(w, p)| match p {
            Some((i, true)) => Some(now[i].clone()),
            Some((i, false)) => revised(c, &now[i], &read[i], w),
            None => Some(fresh(c, w)),
        })
        .collect::<Option<Vec<Node>>>()?;
    Some(out)
}

/// `n` with a new id on every node in it that carries one.
fn fresh(c: &Contract, n: &Node) -> Node {
    let mut n = n.clone();
    n.walk_mut(&mut |x| {
        if c.node(&x.kind).is_some_and(|s| s.spec.id) {
            x.attrs
                .insert(c.id.attr.clone(), serde_json::Value::String(crate::model::new_id()));
        }
    });
    n
}

/// Whether every table in `n` is a grid. The page's rows and the writer's
/// together may not make one: a cell down two rows, kept, over the row
/// under it written from the markdown, which gives that row a cell of its
/// own in the column.
fn grids_hold(n: &Node) -> bool {
    let mut ok = true;
    n.walk(&mut |x| ok &= x.kind != "table" || crate::table::problems(x).is_empty());
    ok
}

/// A block's words, lowercased, each with how often it comes: letters and
/// digits, anything else between them, and between two blocks in it (a
/// table's cells, whose texts run together in its text with nothing
/// between).
type Words = (HashMap<String, usize>, usize);

fn words(n: &Node) -> Words {
    fn spaced(n: &Node, out: &mut String) {
        match &n.text {
            Some(t) => out.push_str(t),
            None => {
                out.push(' ');
                for k in &n.content {
                    spaced(k, out);
                }
                out.push(' ');
            }
        }
    }
    let mut out: HashMap<String, usize> = HashMap::new();
    let mut total = 0;
    let mut text = String::new();
    spaced(n, &mut text);
    let text = text.to_lowercase();
    for w in text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
        *out.entry(w.to_string()).or_default() += 1;
        total += 1;
    }
    (out, total)
}

/// Whether a writer's block `b` is the page's block `a` changed, for
/// [`revise_ops`]: of one type, and, for blocks that hold text, words alike
/// by [`SIMILAR`] or more; a figure of one kind, media with one source.
fn similar(a: &Node, b: &Node, aw: &Words, bw: &Words) -> bool {
    if a.kind != b.kind {
        return false;
    }
    let attr = |n: &Node, k: &str| n.attrs.get(k).cloned();
    match a.kind.as_str() {
        "figure" => return attr(a, "kind") == attr(b, "kind"),
        "image" | "audio" | "video" | "file" => return attr(a, "src") == attr(b, "src"),
        "applet" => return attr(a, "ref") == attr(b, "ref"),
        _ => {}
    }
    let ((a_words, a_total), (b_words, b_total)) = (aw, bw);
    if *a_total == 0 || *b_total == 0 {
        return false;
    }
    let shared: usize = a_words
        .iter()
        .map(|(w, n)| (*n).min(b_words.get(w).copied().unwrap_or(0)))
        .sum();
    2.0 * shared as f64 >= SIMILAR * (a_total + b_total) as f64
}

fn without_ids(c: &Contract, n: &Node) -> Node {
    let mut n = n.clone();
    n.walk_mut(&mut |x| {
        x.attrs.remove(&c.id.attr);
    });
    n
}

/// The longest common subsequence of `current` and `target`, as (current
/// index, target index) pairs in order. A target block with an id is equal
/// only to the block with that id; one without, to a block of its type and
/// text, or of its type holding the block with the first id it holds (a
/// table's cell, which has no id of its own, by its paragraph's). A common
/// start and end are paired first; between them, blocks that all carry ids
/// pair by the longest increasing run of their places in `current`, which
/// for unique keys is the same subsequence, and others by [`pair_by`]'s
/// table.
pub(crate) fn pair_blocks(current: &[Node], target: &[Node]) -> Vec<(usize, usize)> {
    // Text and the first id inside, read once, for a target without ids to
    // pair by.
    let by_text = target.iter().any(|t| t.id().is_none());
    let read = |nodes: &[Node]| -> Vec<(String, Option<String>)> {
        match by_text {
            true => nodes.iter().map(|n| (n.text_content(), id_inside(n))).collect(),
            false => vec![],
        }
    };
    let (current_read, target_read) = (read(current), read(target));
    let eq = |i: usize, j: usize| match target[j].id() {
        Some(id) => current[i].id() == Some(id),
        None => {
            let ((cur_text, cur_inside), (tgt_text, tgt_inside)) = (&current_read[i], &target_read[j]);
            current[i].kind == target[j].kind
                && (cur_text == tgt_text || (tgt_inside.is_some() && cur_inside == tgt_inside))
        }
    };
    let (n, m) = (current.len(), target.len());
    let (pre, suf) = common_ends(0..n, 0..m, &eq);
    let (cur, tgt) = (pre..n - suf, pre..m - suf);
    if !target[tgt.clone()].iter().all(|t| t.id().is_some()) {
        return pair_by(0..n, 0..m, &eq);
    }
    let mut pairs: Vec<(usize, usize)> = (0..pre).map(|k| (k, k)).collect();
    let place: HashMap<&str, usize> = cur
        .clone()
        .filter_map(|i| current[i].id().map(|id| (id, i)))
        .collect();
    let found: Vec<(usize, usize)> = tgt
        .filter_map(|j| target[j].id().and_then(|id| place.get(id)).map(|&i| (i, j)))
        .collect();
    pairs.extend(longest_increasing(&found));
    pairs.extend((0..suf).map(|k| (n - suf + k, m - suf + k)));
    pairs
}

/// The first id among the blocks `n` holds, at any depth.
fn id_inside(n: &Node) -> Option<String> {
    n.content
        .iter()
        .find_map(|k| k.id().map(str::to_string).or_else(|| id_inside(k)))
}

/// How many indices of `cur` and `tgt` pair from their starts, and then
/// from their ends, under `eq`.
fn common_ends(cur: Range<usize>, tgt: Range<usize>, eq: &dyn Fn(usize, usize) -> bool) -> (usize, usize) {
    let (n, m) = (cur.len(), tgt.len());
    let mut pre = 0;
    while pre < n && pre < m && eq(cur.start + pre, tgt.start + pre) {
        pre += 1;
    }
    let mut suf = 0;
    while suf < n - pre && suf < m - pre && eq(cur.end - 1 - suf, tgt.end - 1 - suf) {
        suf += 1;
    }
    (pre, suf)
}

/// The longest common subsequence of the index ranges `cur` and `tgt` under
/// `eq`, as (current index, target index) pairs in order: a common start and
/// end first, then the textbook table between them, up to
/// [`MAX_PAIRING_CELLS`]. Past it, only the start and the end pair.
pub(crate) fn pair_by(cur: Range<usize>, tgt: Range<usize>, eq: &dyn Fn(usize, usize) -> bool) -> Vec<(usize, usize)> {
    let (pre, suf) = common_ends(cur.clone(), tgt.clone(), eq);
    let mut pairs: Vec<(usize, usize)> = (0..pre).map(|k| (cur.start + k, tgt.start + k)).collect();
    let (cur_mid, tgt_mid) = (cur.start + pre..cur.end - suf, tgt.start + pre..tgt.end - suf);
    let (w, h) = (cur_mid.len(), tgt_mid.len());
    if w * h <= MAX_PAIRING_CELLS {
        // lengths[a][b]: the longest common subsequence of cur[a..], tgt[b..].
        let mut lengths = vec![0u32; (w + 1) * (h + 1)];
        let at = |a: usize, b: usize| a * (h + 1) + b;
        for a in (0..w).rev() {
            for b in (0..h).rev() {
                lengths[at(a, b)] = if eq(cur_mid.start + a, tgt_mid.start + b) {
                    lengths[at(a + 1, b + 1)] + 1
                } else {
                    lengths[at(a + 1, b)].max(lengths[at(a, b + 1)])
                };
            }
        }
        let (mut a, mut b) = (0, 0);
        while a < w && b < h {
            if eq(cur_mid.start + a, tgt_mid.start + b) {
                pairs.push((cur_mid.start + a, tgt_mid.start + b));
                a += 1;
                b += 1;
            } else if lengths[at(a + 1, b)] >= lengths[at(a, b + 1)] {
                a += 1;
            } else {
                b += 1;
            }
        }
    }
    pairs.extend((0..suf).map(|k| (cur.end - suf + k, tgt.end - suf + k)));
    pairs
}

/// The longest run of `pairs` (in target order) whose current places
/// increase: patience sorting, n log n.
fn longest_increasing(pairs: &[(usize, usize)]) -> Vec<(usize, usize)> {
    // tails[k]: the index in `pairs` ending the best run of length k + 1.
    let mut tails: Vec<usize> = vec![];
    let mut prev: Vec<Option<usize>> = vec![None; pairs.len()];
    for (p, &(i, _)) in pairs.iter().enumerate() {
        let k = tails.partition_point(|&t| pairs[t].0 < i);
        if k > 0 {
            prev[p] = Some(tails[k - 1]);
        }
        if k == tails.len() {
            tails.push(p);
        } else {
            tails[k] = p;
        }
    }
    let mut out = vec![];
    let mut at = tails.last().copied();
    while let Some(p) = at {
        out.push(pairs[p]);
        at = prev[p];
    }
    out.reverse();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::{parse, Mode};
    use crate::render::{html, Options};

    fn tree(src: &str) -> Vec<Node> {
        let o = parse(&Contract::load(), src, "doc", Mode::Strict);
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        o.nodes
    }

    fn show(nodes: &[Node]) -> String {
        html(&Contract::load(), nodes, &Options { ids: true })
    }

    fn doc_of(src: &str) -> yrs::Doc {
        crate::doc_from_nodes(tree(src))
    }

    /// Each top-level block's id with its Yjs item.
    fn items(doc: &yrs::Doc) -> HashMap<String, yrs::branch::BranchID> {
        use yrs::{Out, ReadTxn, Transact, Xml};
        let txn = doc.transact();
        let Some(frag) = txn.get_xml_fragment("doc") else {
            return HashMap::new();
        };
        crate::ydoc::children(&txn, &frag)
            .filter_map(|(_, child)| match child {
                Out::YXmlElement(el) => {
                    let id = el
                        .get_attribute(&txn, "id")
                        .map(|o| crate::ydoc::out_to_value(&o));
                    let branch: &yrs::branch::Branch = el.as_ref();
                    Some((id?.as_str()?.to_string(), branch.id()))
                }
                _ => None,
            })
            .collect()
    }

    fn rank(op: &Op) -> u8 {
        match op {
            Op::InsertAfter { .. } | Op::InsertBefore { .. } | Op::Append { .. } => 0,
            Op::Replace { .. } => 1,
            Op::Delete { .. } => 2,
        }
    }

    #[test]
    fn diff_ops_turn_one_page_into_another_and_leave_the_rest_alone() {
        use yrs::updates::decoder::Decode;
        use yrs::{ReadTxn, Transact, Update};
        let c = Contract::load();
        let p = |id: &str, text: &str| format!("<p data-id=\"{id}\">{text}</p>");
        let abc = format!("{}{}{}", p("a", "A"), p("b", "B"), p("c", "C"));
        // (current, target, whether every target id is kept)
        let cases: Vec<(String, String, bool)> = vec![
            (abc.clone(), abc.clone(), true),
            (abc.clone(), format!("{}{}{}", p("a", "A"), p("b", "B, changed"), p("c", "C")), true),
            (abc.clone(), format!("{abc}{}", p("d", "D")), true),
            (abc.clone(), format!("{}{abc}", p("z", "Z")), true),
            (abc.clone(), format!("{}{}{}{}", p("a", "A"), p("n", "N"), p("b", "B"), p("c", "C")), true),
            (abc.clone(), format!("{}{}", p("a", "A"), p("c", "C")), true),
            (abc.clone(), format!("{}{}", p("b", "B"), p("c", "C")), true),
            (abc.clone(), format!("{}{}", p("a", "A"), p("b", "B")), true),
            (abc.clone(), format!("{}{}{}", p("c", "C"), p("a", "A"), p("b", "B")), false),
            (format!("{}{}", p("a", "A"), p("b", "B")), format!("{}{}", p("b", "B"), p("a", "A")), false),
            (abc.clone(), format!("{}{}", p("x", "X"), p("y", "Y")), true),
            (String::new(), format!("{}{}", p("x", "X"), p("y", "Y")), true),
            (abc.clone(), format!("<h2 data-id=\"b\">B as a heading</h2>{}{}", p("a", "A"), p("c", "C")), false),
            (
                abc.clone(),
                format!("{}<h3 data-id=\"b\">B</h3>{}", p("a", "A"), p("c", "C")),
                true,
            ),
            (
                format!("<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"q1\">one</p></li></ul>{}", p("a", "A")),
                format!(
                    "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"q1\">one</p></li>\
                     <li data-id=\"i2\"><p data-id=\"q2\">two</p></li></ul>{}",
                    p("a", "A")
                ),
                true,
            ),
            (
                format!("<table data-id=\"t\"><tr data-id=\"r1\"><td><p data-id=\"t1\">1</p></td></tr></table>{}", p("a", "A")),
                format!("<table data-id=\"t\"><tr data-id=\"r1\"><td><p data-id=\"t1\">1, changed</p></td></tr></table>{}", p("a", "A")),
                true,
            ),
            (
                abc.clone(),
                format!(
                    "{}{}{}{}{}{}",
                    p("n1", "N1"),
                    p("a", "A"),
                    p("n2", "N2"),
                    p("n3", "N3"),
                    p("c", "C, changed"),
                    p("n4", "N4")
                ),
                true,
            ),
            // Targets without ids: a page converted from markdown.
            (abc.clone(), "<p>A</p><p>B</p><p>C</p>".into(), false),
            (abc.clone(), "<p>A</p><p>B, changed</p><p>C</p><p>D</p>".into(), false),
            (abc.clone(), "<h1>All</h1><p>new</p>".into(), false),
            (String::new(), "<p>From nothing</p>".into(), false),
        ];
        assert!(cases.len() >= 20);
        for (current, target, ids_kept) in cases {
            let doc = if current.is_empty() {
                crate::new_doc()
            } else {
                doc_of(&current)
            };
            let target = tree(&target);
            let now = crate::read_doc(&doc.transact());
            let ops = diff_ops(&c, &now, &target);
            let ranks: Vec<u8> = ops.iter().map(rank).collect();
            assert!(
                ranks.windows(2).all(|w| w[0] <= w[1]),
                "{target:?}: {ops:?}"
            );

            // Blocks the target holds as they are now are not touched, but
            // for one moved, which leaves its place and is written in its new
            // one.
            let deleted: HashSet<&str> = ops
                .iter()
                .filter_map(|o| match o {
                    Op::Delete { id } => Some(id.as_str()),
                    _ => None,
                })
                .collect();
            let equal: Vec<String> = now
                .iter()
                .filter(|n| {
                    target.iter().any(|t| {
                        t == *n || (t.id().is_none() && without_ids(&c, t) == without_ids(&c, n))
                    })
                })
                .filter_map(|n| n.id().map(str::to_string))
                .collect();
            if ids_kept {
                assert!(
                    equal.iter().all(|id| !deleted.contains(id.as_str())),
                    "{ops:?}"
                );
            }
            let unchanged: Vec<&String> = equal
                .iter()
                .filter(|id| !deleted.contains(id.as_str()))
                .collect();
            assert!(target.len() < 3 || !unchanged.is_empty() || now.is_empty() || !ids_kept);
            let before_items = items(&doc);
            let before = doc.transact().state_vector();
            let applied = crate::apply_ops(&doc, None, &ops);
            assert!(applied.is_ok(), "{ops:?}: {applied:?}");
            let update = doc.transact().encode_state_as_update_v1(&before);
            let update = Update::decode_v1(&update).unwrap();
            let after_items = items(&doc);
            for id in unchanged {
                let item = &before_items[id];
                assert_eq!(
                    after_items.get(id),
                    Some(item),
                    "{id} was rewritten: {ops:?}"
                );
                if let yrs::branch::BranchID::Nested(item) = item {
                    assert!(!update.delete_set().contains(item), "{id} deleted: {ops:?}");
                }
            }

            let back = crate::read_doc(&doc.transact());
            assert_eq!(show_plain(&back), show_plain(&target), "{ops:?}");
            if ids_kept {
                assert_eq!(show(&back), show(&target), "{ops:?}");
            }
            // Nothing left to do. A moved block comes back under a new id
            // (its own was still the old place's when it was written), so a
            // page with one differs from the target by that id.
            if ids_kept || target.iter().all(|t| t.id().is_none()) {
                assert_eq!(diff_ops(&c, &back, &target).len(), 0, "{target:?}");
            }
        }
    }

    fn show_plain(nodes: &[Node]) -> String {
        html(&Contract::load(), nodes, &Options { ids: false })
    }

    /// A tree the editor wrote holds its spaces as typed: two between
    /// words, one at a line's end or a block's start, a tab, a line made of
    /// spaces. Putting such a version back wrote it as HTML read by HTML's
    /// whitespace rules, which collapsed them: the page then differed from
    /// the version, and putting it back again wrote it again.
    #[test]
    fn a_version_put_back_keeps_its_spaces() {
        use yrs::Transact;
        let c = Contract::load();
        let block = |kind: &str, id: &str, extra: &[(&str, serde_json::Value)], content: Vec<Node>| {
            let mut attrs = serde_json::Map::new();
            attrs.insert(c.id.attr.clone(), serde_json::Value::String(id.into()));
            for (k, v) in extra {
                attrs.insert(k.to_string(), v.clone());
            }
            Node::element(kind, attrs, content)
        };
        let text = |t: &str| Node::text(t, vec![]);
        let version = vec![
            block("heading", "h", &[("level", 2.into())], vec![text("Plan ")]),
            block(
                "paragraph",
                "a",
                &[],
                vec![
                    text("End.  Next "),
                    Node::element("hardBreak", Default::default(), vec![]),
                    text("  after\ta break"),
                ],
            ),
            block("paragraph", "b", &[], vec![text("   ")]),
            block("paragraph", "c", &[], vec![text("  indented, and gone since ")]),
        ];
        let current = tree("<h2 data-id=\"h\">Plan</h2><p data-id=\"a\">changed</p><p data-id=\"b\">x</p>");
        let doc = crate::doc_from_nodes(current.clone());
        let ops = diff_ops(&c, &current, &version);
        crate::ops::apply_own_in(&c, &mut doc.transact_mut(), &ops).unwrap();
        let back = crate::read_doc(&doc.transact());
        assert_eq!(show(&back), show(&version));
        assert!(diff_ops(&c, &back, &version).is_empty(), "{:?}", diff_ops(&c, &back, &version));
    }

    /// A writer's markdown, converted: blocks without ids.
    fn written(md: &str) -> Vec<Node> {
        let o = crate::parse_article_markdown(md);
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        o.nodes
    }

    /// The writer's diff on `doc`: the ops, and how each replace landed.
    fn revise(doc: &yrs::Doc, md: &str) -> (Vec<Op>, Vec<String>) {
        use yrs::Transact;
        let c = Contract::load();
        let now = crate::read_doc(&doc.transact());
        let draft = crate::parse_draft(&now, md, Dialect::Article);
        assert!(draft.errors.is_empty(), "{:?}", draft.errors);
        let ops = revise_ops(&c, &now, &draft.nodes, Dialect::Article);
        assert_eq!(suggestions_taken(&now, &ops), Vec::<String>::new(), "{ops:?}");
        let applied = crate::ops::apply_own_in(&c, &mut doc.transact_mut(), &ops).unwrap();
        (ops, applied.replaced_as)
    }

    /// A page as a person leaves it, written back by a writer that changed
    /// nothing: its export read as the writer's markdown. Nothing is
    /// written, whatever the page holds that its markdown shows otherwise
    /// or not at all.
    #[test]
    fn a_page_written_back_unchanged_is_left_alone() {
        use yrs::Transact;
        let c = Contract::load();
        let p = |id: &str, inner: &str| format!("<p data-id=\"{id}\">{inner}</p>");
        let list = |kind: &str, id: &str, items: &[&str]| {
            let (open, item) = match kind {
                "ol" => ("<ol".to_string(), "<li>".to_string()),
                "task" => ("<ul data-type=\"taskList\"".to_string(), "<li data-type=\"taskItem\">".to_string()),
                _ => ("<ul".to_string(), "<li>".to_string()),
            };
            let close = if kind == "ol" { "</ol>" } else { "</ul>" };
            let body: String = items.iter().map(|t| format!("{item}<p>{t}</p></li>")).collect();
            format!("{open} data-id=\"{id}\">{body}{close}")
        };
        let f = |label: &str, note: &str| format!("<virtues-footnote label=\"{label}\" note=\"{note}\"></virtues-footnote>");
        let pages = [
            // An empty paragraph, where a person pressed Enter, mid-page and last.
            format!("{}{}{}", p("a", "a"), p("e", ""), p("b", "b")),
            format!("{}{}", p("a", "a"), p("e", "")),
            format!("{}{}{}{}", p("e1", ""), p("a", "a"), p("e2", ""), p("e3", "")),
            // Lists of one kind side by side.
            format!("{}{}", list("ul", "l1", &["a"]), list("ul", "l2", &["b"])),
            format!("{}{}{}", list("ol", "l1", &["a"]), list("ol", "l2", &["b", "c"]), list("ol", "l3", &["d"])),
            format!("{}{}", list("task", "l1", &["a"]), list("task", "l2", &["b"])),
            // Spaces as typed: two between words, a tab, at either end.
            format!("{}{}{}{}{}", p("a", "two  spaces"), p("b", "tab\there"), p("c", "trailing space "), p("d", " leading space"), p("x", "End.  Next sentence.")),
            // One label, two sources.
            format!(
                "{}{}",
                p("a", &format!("You ate with Nick.{}", f("ev-2", "Message · 12:10 PM · data_communication_message:m_1"))),
                p("b", &format!("You swam at noon.{}", f("ev-2", "Location · 9:30 AM")))
            ),
            // A footnote whose note its label's other footnote carries.
            format!(
                "{}{}",
                p("a", &format!("One.{}", f("ev-1", "Message · 9:00 AM"))),
                p("b", "Two.<virtues-footnote label=\"ev-1\"></virtues-footnote>")
            ),
            // The owner's pending suggestion.
            format!(
                "{}{}",
                p("a", "You met Zoe at the shop."),
                p("b", "I had lunch <virtues-del proposal=\"p1\">at noon</virtues-del><virtues-ins proposal=\"p1\">on Friday</virtues-ins> with her.")
            ),
        ];
        for src in pages {
            let doc = doc_of(&src);
            let now = crate::read_doc(&doc.transact());
            let md = crate::to_markdown(&now);
            let draft = crate::parse_draft(&now, &md, Dialect::Article);
            assert!(draft.errors.is_empty(), "{src}: {:?}", draft.errors);
            let ops = revise_ops(&c, &now, &draft.nodes, Dialect::Article);
            assert!(ops.is_empty(), "{src}\n{md:?}\n{ops:?}");
            assert!(!draft.notes.iter().any(|n| n.message.contains("as accepted")), "{src}: {:?}", draft.notes);
            assert_eq!(
                crate::edition_of(&crate::to_markdown(&draft.nodes)),
                crate::edition_of(&md),
                "{src}: the draft reads as the page"
            );
        }
    }

    /// A paragraph added after an empty one, and another page block
    /// changed, leave the empty paragraph where it was: the export shows it
    /// as nothing, so the writer's markdown cannot say it went. A page that
    /// is only that paragraph, written whole, holds the writing alone.
    #[test]
    fn an_empty_paragraph_stays_through_a_writers_change() {
        use yrs::Transact;
        let empty = doc_of("<p data-id=\"e\"></p>");
        let (ops, _) = revise(&empty, "A first draft.\n\nIts second paragraph.\n");
        assert!(ops.iter().any(|o| matches!(o, Op::Delete { id } if id == "e")), "{ops:?}");
        assert_eq!(crate::to_markdown(&crate::read_doc(&empty.transact())), "A first draft.\n\nIts second paragraph.\n");

        let doc = doc_of("<p data-id=\"a\">You read on the porch.</p><p data-id=\"e\"></p><p data-id=\"b\">End.</p>");
        let (ops, _) = revise(&doc, "You read on the porch while it rained.\n\nEnd.\n\nA new line.\n");
        assert!(!ops.iter().any(|o| matches!(o, Op::Delete { id } if id == "e")), "{ops:?}");
        let after = crate::read_doc(&doc.transact());
        let new_id = after[3].id().unwrap();
        assert_eq!(
            show(&after),
            format!(
                "<p data-id=\"a\">You read on the porch while it rained.</p><p data-id=\"e\"></p>\
                 <p data-id=\"b\">End.</p><p data-id=\"{new_id}\">A new line.</p>"
            )
        );
    }

    /// The owner's pending suggestion is theirs to decide. A writer's
    /// markdown that carries it back as it stands leaves its block alone;
    /// one that changes that block is named, so the write is refused; one
    /// that writes a suggestion of its own writes it as accepted.
    #[test]
    fn a_writer_leaves_the_owners_suggestion_to_them() {
        use yrs::Transact;
        let c = Contract::load();
        let src = "<p data-id=\"a\">You met Zoe at the shop.</p>\
                   <p data-id=\"b\">I had lunch <virtues-del proposal=\"p1\">at noon</virtues-del><virtues-ins proposal=\"p1\">on Friday</virtues-ins> with her.</p>";
        let doc = doc_of(src);
        let now = crate::read_doc(&doc.transact());
        let md = crate::to_markdown(&now);
        assert_eq!(md, "You met Zoe at the shop.\n\nI had lunch {--at noon--}{++on Friday++} with her.\n");

        // Carried back, with a paragraph added: the suggestion stays.
        let (ops, _) = revise(&doc, &format!("{md}\nYou walked home.\n"));
        assert!(matches!(&ops[..], [Op::InsertAfter { id, .. }] if id == "b"), "{ops:?}");
        let after = show(&crate::read_doc(&doc.transact()));
        assert!(after.contains("<virtues-del proposal=\"p1\">at noon</virtues-del><virtues-ins proposal=\"p1\">on Friday</virtues-ins>"), "{after}");

        // Changed, accepted or rejected: named, for the write to be refused.
        let now = crate::read_doc(&doc.transact());
        for decided in ["I had lunch on Friday with her.", "I had lunch at noon with her.", "I had lunch {--at noon--}{++on Friday++} with Nick."] {
            let md = format!("You met Zoe at the shop.\n\n{decided}\n\nYou walked home.\n");
            let draft = crate::parse_draft(&now, &md, Dialect::Article);
            let ops = revise_ops(&c, &now, &draft.nodes, Dialect::Article);
            assert_eq!(suggestions_taken(&now, &ops), ["b"], "{decided}: {ops:?}");
        }
        // Dropped with the paragraph: named too.
        let draft = crate::parse_draft(&now, "You met Zoe at the shop.\n\nYou walked home.\n", Dialect::Article);
        let ops = revise_ops(&c, &now, &draft.nodes, Dialect::Article);
        assert_eq!(suggestions_taken(&now, &ops), ["b"], "{ops:?}");

        // A suggestion of the writer's own is written as accepted.
        let draft = crate::parse_draft(&now, &format!("{}\nYou walked {{--home--}}{{++back++}}.\n", crate::to_markdown(&now).replace("You walked home.\n", "")), Dialect::Article);
        assert!(draft.notes.iter().any(|n| n.message.contains("as accepted")), "{:?}", draft.notes);
        assert!(crate::to_markdown(&draft.nodes).contains("You walked back."), "{}", crate::to_markdown(&draft.nodes));
        assert!(crate::to_markdown(&draft.nodes).contains("{--at noon--}{++on Friday++}"));
    }

    /// A block the writer's markdown shows as it stands is left alone, what
    /// markdown cannot show included: a cell's span, a space inside bold.
    /// `diff_ops`, which pairs by text, would write over both.
    #[test]
    fn revise_leaves_an_unchanged_block_untouched_even_where_markdown_cannot_show_it() {
        use yrs::Transact;
        let c = Contract::load();
        let doc = doc_of(
            "<p data-id=\"a\">Lunch with <strong>Nick </strong>today.</p>\
             <table data-id=\"t\"><tr data-id=\"r1\"><th colspan=\"2\"><p>Both</p></th></tr>\
             <tr data-id=\"r2\"><td><p>x</p></td><td><p>y</p></td></tr></table><p data-id=\"b\">End.</p>",
        );
        let now = crate::read_doc(&doc.transact());
        let md = crate::to_markdown(&now);
        let target = written(&md);
        assert_ne!(show_plain(&target), show_plain(&now), "markdown cannot show these");
        assert!(!diff_ops(&c, &now, &target).is_empty());
        let before = items(&doc);
        let (ops, _) = revise(&doc, &md);
        assert!(ops.is_empty(), "{ops:?}");
        assert_eq!(items(&doc), before);
        assert_eq!(show(&crate::read_doc(&doc.transact())), show(&now));
    }

    /// A block the writer changed a little (a word, a heading's level, a
    /// footnote renumbered, a figure's text) is written in place: its id
    /// kept, a text block by its changed text.
    #[test]
    fn a_lightly_changed_paragraph_keeps_its_id() {
        use yrs::Transact;
        let doc = doc_of(
            "<h2 data-id=\"h\">Morning</h2>\
             <p data-id=\"a\">You read on the porch while it rained.<virtues-footnote label=\"ev-1\" note=\"Recording · 9:05 AM\"></virtues-footnote></p>\
             <virtues-figure data-id=\"f\" kind=\"quote\" text=\"Save me a seat.\" who=\"Nick\"></virtues-figure>\
             <p data-id=\"b\">Later you walked the dog.</p>",
        );
        let before = items(&doc);
        let (ops, how) = revise(
            &doc,
            "### Morning\n\nYou read on the porch while it poured.[^ev-2]\n\n```figure\nkind: quote\ntext: Save me a seat by the window.\nwho: Nick\n```\n\nLater you walked the dog.\n\n[^ev-2]: Recording · 9:06 AM\n",
        );
        let replaced: Vec<&str> = ops
            .iter()
            .map(|o| match o {
                Op::Replace { id, .. } => id.as_str(),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(replaced, ["h", "a", "f"]);
        assert_eq!(how, ["text-diff", "text-diff", "patch"]);
        assert_eq!(items(&doc), before, "every block keeps its item");
        assert_eq!(
            show(&crate::read_doc(&doc.transact())),
            "<h3 data-id=\"h\">Morning</h3>\
             <p data-id=\"a\">You read on the porch while it poured.<virtues-footnote label=\"ev-2\" note=\"Recording · 9:06 AM\"></virtues-footnote></p>\
             <virtues-figure data-id=\"f\" kind=\"quote\" text=\"Save me a seat by the window.\" who=\"Nick\"></virtues-figure>\
             <p data-id=\"b\">Later you walked the dog.</p>"
        );
    }

    /// A block whose words are not the page's block's is new: inserted where
    /// it goes, and the page's block deleted. So is one of another type.
    #[test]
    fn an_unrelated_paragraph_is_swapped() {
        let doc = doc_of("<p data-id=\"a\">You read on the porch.</p><p data-id=\"b\">End of the day.</p>");
        let (ops, how) = revise(&doc, "Dinner was at the harbor with Nick.\n\nEnd of the day.\n");
        assert!(how.is_empty(), "{ops:?}");
        assert!(matches!(&ops[..], [Op::InsertBefore { id, .. }, Op::Delete { id: gone }] if id == "b" && gone == "a"), "{ops:?}");
        let doc = doc_of("<p data-id=\"a\">The walk home</p><p data-id=\"b\">End.</p>");
        let (ops, _) = revise(&doc, "## The walk home\n\nEnd.\n");
        assert!(matches!(&ops[..], [Op::InsertBefore { .. }, Op::Delete { id }] if id == "a"), "{ops:?}");
    }

    /// A writer's change to one row of a table leaves every other row as
    /// it stands, what markdown cannot show included: a header cell across
    /// two columns, a cell down two rows, a cell's own alignment where its
    /// column's differs. Read from the markdown, the rows it did not change
    /// took the markdown's defaults: the wide header split in two, the cell
    /// under the tall one moved into its column, every cell of a column
    /// aligned alike. A table of words with no punctuation between its
    /// cells is still the table changed, not a new one.
    #[test]
    fn a_writers_change_to_one_row_leaves_the_other_rows_as_they_stand() {
        use yrs::Transact;
        let cases = [
            (
                "<table data-id=\"t\"><tr data-id=\"r1\"><th colspan=\"2\"><p data-id=\"h\">Both sides</p></th></tr>\
                 <tr data-id=\"r2\"><td><p data-id=\"a2\">Left.</p></td><td><p data-id=\"b2\">Right.</p></td></tr>\
                 <tr data-id=\"r3\"><td><p data-id=\"a3\">More left.</p></td><td><p data-id=\"b3\">More right.</p></td></tr></table>",
                ("More right.", "More right side."),
            ),
            (
                "<table data-id=\"t\"><tr data-id=\"r1\"><th><p data-id=\"h1\">When</p></th><th><p data-id=\"h2\">What</p></th></tr>\
                 <tr data-id=\"r2\"><td rowspan=\"2\"><p data-id=\"a2\">Morning</p></td><td><p data-id=\"b2\">You read on the porch.</p></td></tr>\
                 <tr data-id=\"r3\"><td><p data-id=\"b3\">You had coffee with friends.</p></td></tr>\
                 <tr data-id=\"r4\"><td><p data-id=\"a4\">Evening</p></td><td><p data-id=\"b4\">You finished a long book.</p></td></tr></table>",
                ("a long book", "a very long book"),
            ),
            (
                "<table data-id=\"t\"><tr data-id=\"r1\"><th align=\"center\"><p data-id=\"h1\">Day</p></th><th><p data-id=\"h2\">Weather</p></th></tr>\
                 <tr data-id=\"r2\"><td><p data-id=\"a2\">Monday</p></td><td><p data-id=\"b2\">rain</p></td></tr>\
                 <tr data-id=\"r3\"><td align=\"right\"><p data-id=\"a3\">Tuesday</p></td><td><p data-id=\"b3\">sun</p></td></tr></table>",
                ("rain", "rain again"),
            ),
        ];
        for (src, (from, to)) in cases {
            let doc = doc_of(src);
            let md = crate::to_markdown(&crate::read_doc(&doc.transact()));
            assert_eq!(md.matches(from).count(), 1, "{md}");
            let before = items(&doc);
            let (ops, how) = revise(&doc, &md.replace(from, to));
            assert!(matches!(&ops[..], [Op::Replace { id, .. }] if id == "t"), "{src}: {ops:?}");
            assert_eq!(how, ["patch"], "{src}");
            assert_eq!(items(&doc), before, "{src}");
            assert_eq!(
                show(&crate::read_doc(&doc.transact())),
                show(&tree(&src.replace(from, to))),
                "{src}"
            );
        }
    }

    /// A change markdown cannot line up with the page's rows (the row
    /// under a cell two rows tall, or that cell's own row dropped) writes
    /// the table as the writer's markdown reads, and the table stays a
    /// grid: kept, the tall cell would cover a column the row under it
    /// also fills.
    #[test]
    fn a_table_changed_across_a_tall_cell_is_written_as_it_reads() {
        use yrs::Transact;
        let src = "<table data-id=\"t\"><tr data-id=\"r1\"><th><p>When</p></th><th><p>What</p></th></tr>\
                   <tr data-id=\"r2\"><td rowspan=\"2\"><p>Morning</p></td><td><p>You read on the porch.</p></td></tr>\
                   <tr data-id=\"r3\"><td><p>You had coffee with friends.</p></td></tr>\
                   <tr data-id=\"r4\"><td><p>Evening</p></td><td><p>You finished a long book.</p></td></tr></table>";
        let edits: [&dyn Fn(&str) -> String; 2] = [
            &|md| md.replace("coffee with friends", "tea with friends"),
            &|md| md.lines().filter(|l| !l.contains("porch")).map(|l| format!("{l}\n")).collect(),
        ];
        for edit in edits {
            let doc = doc_of(src);
            let md = edit(&crate::to_markdown(&crate::read_doc(&doc.transact())));
            revise(&doc, &md);
            let after = crate::read_doc(&doc.transact());
            assert_eq!(crate::table::problems(&after[0]), Vec::<String>::new(), "{}", show(&after));
            assert_eq!(crate::to_markdown(&after), md);
        }
    }

    /// A copy of `doc` as another device holds it.
    fn device(doc: &yrs::Doc) -> yrs::Doc {
        use yrs::updates::decoder::Decode;
        use yrs::{ReadTxn, Transact};
        let copy = crate::new_doc();
        let state = doc.transact().encode_state_as_update_v1(&yrs::StateVector::default());
        copy.transact_mut()
            .apply_update(yrs::Update::decode_v1(&state).unwrap())
            .unwrap();
        copy
    }

    /// Type `typed` at the end of the text block `path` leads to, child by
    /// child from the page, and return the update.
    fn type_at_end(doc: &yrs::Doc, path: &[u32], typed: &str) -> Vec<u8> {
        use yrs::{ReadTxn, Text, Transact, WriteTxn, XmlFragment, XmlOut};
        let before = doc.transact().state_vector();
        {
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

    fn apply_update(doc: &yrs::Doc, update: &[u8]) {
        use yrs::updates::decoder::Decode;
        use yrs::Transact;
        doc.transact_mut()
            .apply_update(yrs::Update::decode_v1(update).unwrap())
            .unwrap();
    }

    /// Every id `nodes` hold, in order.
    fn ids_of(nodes: &[Node]) -> Vec<String> {
        let mut out = vec![];
        for n in nodes {
            n.walk(&mut |n| out.extend(n.id().map(str::to_string)));
        }
        out
    }

    /// An item or a row the writer inserts beside one it rewords is new, as
    /// a paragraph inserted beside a reworded one is: it takes no id of the
    /// page's, and the reworded one keeps its id and its text, so a
    /// person's keystroke on its way into it lands in it. Paired by place,
    /// the inserted item took the reworded one's id and live text, and the
    /// keystroke landed in the other sentence.
    #[test]
    fn an_item_inserted_beside_a_reworded_one_is_new() {
        use yrs::Transact;
        let cases = [
            (
                "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">You walked the dog in the park.</p></li>\
                 <li data-id=\"i2\"><p data-id=\"p2\">You cooked dinner for your family.</p></li>\
                 <li data-id=\"i3\"><p data-id=\"p3\">You read before bed.</p></li></ul>",
                &[0u32, 1, 0][..],
                "- You walked the dog in the park.\n- You called your sister about the trip.\n\
                 - You cooked a late dinner for your family.\n- You read before bed.\n",
                "<li data-id=\"i2\"><p data-id=\"p2\">You cooked a late dinner for your family. with Nick</p></li>",
            ),
            (
                "<table data-id=\"t\"><tr data-id=\"r1\"><th><p data-id=\"h1\">When</p></th><th><p data-id=\"h2\">What</p></th></tr>\
                 <tr data-id=\"r2\"><td><p data-id=\"a2\">Morning</p></td><td><p data-id=\"b2\">You cooked dinner for your family.</p></td></tr>\
                 <tr data-id=\"r3\"><td><p data-id=\"a3\">Night</p></td><td><p data-id=\"b3\">You read before bed.</p></td></tr></table>",
                &[0u32, 1, 1, 0][..],
                "| When | What |\n| --- | --- |\n| Afternoon | You called your sister about the trip. |\n\
                 | Morning | You cooked a late dinner for your family. |\n| Night | You read before bed. |\n",
                "<tr data-id=\"r2\"><td><p data-id=\"a2\">Morning</p></td><td><p data-id=\"b2\">You cooked a late dinner for your family. with Nick</p></td></tr>",
            ),
        ];
        for (src, path, md, kept) in cases {
            let doc = doc_of(src);
            let page_ids = ids_of(&crate::read_doc(&doc.transact()));
            let keystroke = type_at_end(&device(&doc), path, " with Nick");
            let (ops, how) = revise(&doc, md);
            assert_eq!(how, ["patch"], "{ops:?}");
            apply_update(&doc, &keystroke);
            let after = crate::read_doc(&doc.transact());
            let html = show(&after);
            assert!(html.contains(kept), "{html}");
            assert_eq!(html.matches("with Nick").count(), 1, "{html}");
            // The inserted one, second in either, holds only ids of its own.
            let inserted = &after[0].content[1];
            assert!(inserted.text_content().contains("You called your sister"), "{html}");
            let new_ids = ids_of(std::slice::from_ref(inserted));
            assert!(!new_ids.is_empty(), "{html}");
            assert!(new_ids.iter().all(|id| !page_ids.contains(id)), "{new_ids:?} in {page_ids:?}: {html}");
            assert_eq!(ids_of(&after).len(), page_ids.len() + new_ids.len(), "{html}");
        }
    }

    /// `diff_ops`, for a version put back and the model, pairs a target
    /// without ids by type and text as it always did: a changed paragraph is
    /// a new one there, and only the writer's diff writes it in place.
    #[test]
    fn diff_ops_pairs_as_before() {
        use yrs::Transact;
        let c = Contract::load();
        let doc = doc_of("<p data-id=\"a\">You read on the porch while it rained.</p><p data-id=\"b\">End.</p>");
        let now = crate::read_doc(&doc.transact());
        let target = written("You read on the porch while it poured.\n\nEnd.\n");
        let ops = diff_ops(&c, &now, &target);
        assert!(matches!(&ops[..], [Op::InsertBefore { id, .. }, Op::Delete { id: gone }] if id == "b" && gone == "a"), "{ops:?}");
        let ops = revise_ops(&c, &now, &target, Dialect::Article);
        assert!(matches!(&ops[..], [Op::Replace { id, .. }] if id == "a"), "{ops:?}");
    }

    #[test]
    fn diff_ops_name_their_anchors() {
        let c = Contract::load();
        let now = tree("<p data-id=\"a\">A</p><p data-id=\"b\">B</p>");
        let ops = diff_ops(&c, &now, &tree("<p data-id=\"z\">Z</p><p data-id=\"a\">A</p><p data-id=\"n\">N</p><p data-id=\"b\">B2</p>"));
        let shown: Vec<String> = ops
            .iter()
            .map(|o| serde_json::to_string(o).unwrap())
            .collect();
        assert_eq!(
            shown,
            [
                r#"{"op":"insert_before","id":"a","html":"<p data-id=\"z\">Z</p>"}"#,
                r#"{"op":"insert_after","id":"a","html":"<p data-id=\"n\">N</p>"}"#,
                r#"{"op":"replace","id":"b","html":"<p data-id=\"b\">B2</p>"}"#,
            ]
        );
        let ops = diff_ops(&c, &now, &tree("<p>X</p><p>Y</p>"));
        let shown: Vec<String> = ops
            .iter()
            .map(|o| serde_json::to_string(o).unwrap())
            .collect();
        assert_eq!(
            shown,
            [
                r#"{"op":"append","html":"<p>X</p><p>Y</p>"}"#,
                r#"{"op":"delete","id":"a"}"#,
                r#"{"op":"delete","id":"b"}"#,
            ]
        );
    }
}

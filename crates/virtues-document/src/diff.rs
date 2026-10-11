//! Trees compared block by block: the ops that turn one page into another
//! ([`diff_ops`]), for a restore that keeps every block it does not change,
//! and what a reader has seen of a page ([`seen_tree`]), the base its next
//! edit is merged against.

use crate::contract::Contract;
use crate::model::Node;
use crate::ops::{find_node, Op};
use crate::render;
use crate::ydoc::canonicalize;
use std::collections::{HashMap, HashSet};

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

    let mut inserts = vec![];
    let mut replaces = vec![];
    let html = |nodes: &[Node]| render::html(c, nodes, &render::Options { ids: true });

    // Where each target block pairs, if it does.
    let mut paired_at: Vec<Option<usize>> = vec![None; target.len()];
    for &(i, j) in &pairs {
        paired_at[j] = Some(i);
    }
    let first_paired = pairs.iter().find_map(|&(i, _)| current[i].id());
    let mut before: Option<&str> = None;
    let mut j = 0;
    while j < target.len() {
        if let Some(i) = paired_at[j] {
            let (cur, tgt) = (&current[i], &target[j]);
            let same = if tgt.id().is_some() {
                cur == tgt
            } else {
                without_ids(c, cur) == without_ids(c, tgt)
            };
            if let (false, Some(id)) = (same, cur.id()) {
                replaces.push(Op::Replace {
                    id: id.to_string(),
                    html: html(std::slice::from_ref(tgt)),
                });
            }
            before = cur.id().or(before);
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
/// text. A common start and end are paired first; between them, blocks that
/// all carry ids pair by the longest increasing run of their places in
/// `current`, which for unique keys is the same subsequence, and others by
/// the textbook table, up to [`MAX_PAIRING_CELLS`].
pub(crate) fn pair_blocks(current: &[Node], target: &[Node]) -> Vec<(usize, usize)> {
    // Text, read once, for a target without ids to pair by.
    let by_text = target.iter().any(|t| t.id().is_none());
    let text = |nodes: &[Node]| -> Vec<String> {
        match by_text {
            true => nodes.iter().map(Node::text_content).collect(),
            false => vec![],
        }
    };
    let (current_text, target_text) = (text(current), text(target));
    let eq = |i: usize, j: usize| match target[j].id() {
        Some(id) => current[i].id() == Some(id),
        None => current[i].kind == target[j].kind && current_text[i] == target_text[j],
    };
    let (n, m) = (current.len(), target.len());
    let mut pre = 0;
    while pre < n && pre < m && eq(pre, pre) {
        pre += 1;
    }
    let mut suf = 0;
    while suf < n - pre && suf < m - pre && eq(n - 1 - suf, m - 1 - suf) {
        suf += 1;
    }
    let (cur, tgt) = (pre..n - suf, pre..m - suf);
    let mut pairs: Vec<(usize, usize)> = (0..pre).map(|k| (k, k)).collect();

    if target[tgt.clone()].iter().all(|t| t.id().is_some()) {
        let place: HashMap<&str, usize> = cur
            .clone()
            .filter_map(|i| current[i].id().map(|id| (id, i)))
            .collect();
        let found: Vec<(usize, usize)> = tgt
            .clone()
            .filter_map(|j| target[j].id().and_then(|id| place.get(id)).map(|&i| (i, j)))
            .collect();
        pairs.extend(longest_increasing(&found));
    } else if cur.len() * tgt.len() <= MAX_PAIRING_CELLS {
        let (w, h) = (cur.len(), tgt.len());
        // lengths[a][b]: the longest common subsequence of cur[a..], tgt[b..].
        let mut lengths = vec![0u32; (w + 1) * (h + 1)];
        let at = |a: usize, b: usize| a * (h + 1) + b;
        for a in (0..w).rev() {
            for b in (0..h).rev() {
                lengths[at(a, b)] = if eq(cur.start + a, tgt.start + b) {
                    lengths[at(a + 1, b + 1)] + 1
                } else {
                    lengths[at(a + 1, b)].max(lengths[at(a, b + 1)])
                };
            }
        }
        let (mut a, mut b) = (0, 0);
        while a < w && b < h {
            if eq(cur.start + a, tgt.start + b) {
                pairs.push((cur.start + a, tgt.start + b));
                a += 1;
                b += 1;
            } else if lengths[at(a + 1, b)] >= lengths[at(a, b + 1)] {
                a += 1;
            } else {
                b += 1;
            }
        }
    }
    pairs.extend((0..suf).map(|k| (n - suf + k, m - suf + k)));
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

/// What a reader has seen of a page: `base`, the page as it was when it
/// read it, with every block whose id is in `fresh` (the blocks it has read
/// again since, or written) taken from `current`, whole. Such a block
/// replaces the base's block with its id; one the base lacks goes after its
/// nearest preceding sibling in `current` that the base has, or first in its
/// parent; one `current` lacks leaves. Nothing else of `current` enters: an
/// edit someone made to any other block since the reader's read stays
/// unseen, so a write over that block is merged against what the reader
/// saw, or refused.
///
/// A block taken from `current` brings the blocks inside it; any of those
/// the base holds elsewhere leaves that place, so no id is in the tree
/// twice. A block whose parent cannot be found in the base (its parent is
/// new, and not in `fresh`) is not placed.
pub fn seen_tree(base: &[Node], current: &[Node], fresh: &HashSet<String>) -> Vec<Node> {
    let mut seen = base.to_vec();
    // Gone from the page: gone from what was seen.
    for id in fresh {
        if find_node(current, id).is_none() {
            remove(&mut seen, id);
        }
    }
    let mut placed: Vec<Placement> = vec![];
    locate(current, &mut vec![], None, fresh, &mut placed);
    for p in placed {
        let Some(node) = node_at(current, &p.at) else {
            continue;
        };
        let node = node.clone();
        // Ids inside the block that the seen tree holds elsewhere leave
        // there.
        let mut inside = HashSet::new();
        crate::ops::collect_ids(std::slice::from_ref(&node), &mut inside);
        let mut held = HashSet::new();
        crate::ops::collect_ids(&seen, &mut held);
        let mut here = HashSet::new();
        if let Some(n) = find_node(&seen, &p.id) {
            crate::ops::collect_ids(std::slice::from_ref(n), &mut here);
        }
        for id in inside
            .iter()
            .filter(|id| held.contains(*id) && !here.contains(*id))
        {
            remove(&mut seen, id);
        }
        if let Some(path) = path_of(&seen, &p.id) {
            if let Some(slot) = node_at_mut(&mut seen, &path) {
                *slot = node;
            }
            continue;
        }
        // New to what was seen: after its nearest preceding sibling there.
        let parent = match &p.anchor {
            Some(anchor) => match path_of(&seen, anchor) {
                Some(mut path) => {
                    path.extend(&p.below);
                    Some(path)
                }
                None => continue,
            },
            None if p.below.is_empty() => None,
            None => Some(p.below.clone()),
        };
        let siblings = match &parent {
            None => &mut seen,
            Some(path) => match node_at_mut(&mut seen, path) {
                Some(n) => &mut n.content,
                None => continue,
            },
        };
        let after = p
            .preceding
            .iter()
            .find_map(|id| siblings.iter().position(|n| n.id() == Some(id.as_str())));
        let at = after.map_or(0, |k| k + 1);
        siblings.insert(at, node);
    }
    seen
}

/// The block a reader is shown so that the block `id` enters what it has
/// seen ([`seen_tree`]): `id` itself when `base` holds it, or holds the
/// nearest block with an id around it; otherwise the outermost block around
/// it that `base` lacks. [`seen_tree`] places a new block only inside one
/// the base has, so an item of a list the base never held enters with its
/// list, or not at all. `None` when `current` has no block `id`.
pub fn shown_for(base: &[Node], current: &[Node], id: &str) -> Option<String> {
    let mut around = vec![];
    if !holders(current, id, &mut around) {
        return None;
    }
    if find_node(base, id).is_some() {
        return Some(id.to_string());
    }
    let mut shown = id.to_string();
    for holder in around.iter().rev() {
        if find_node(base, holder).is_some() {
            break;
        }
        shown = holder.clone();
    }
    Some(shown)
}

/// Whether `nodes` holds the block `id`; when it does, `around` ends up
/// holding the ids of the blocks around it, outermost first.
fn holders(nodes: &[Node], id: &str, around: &mut Vec<String>) -> bool {
    for n in nodes {
        if n.id() == Some(id) {
            return true;
        }
        let own = n.id().map(str::to_string);
        let pushed = own.is_some();
        around.extend(own);
        if holders(&n.content, id, around) {
            return true;
        }
        if pushed {
            around.pop();
        }
    }
    false
}

/// `seen` with each block in `keep` put back as `base` has it, wherever
/// `seen` holds it: a block a reader is about to write over stays as it was
/// read, even inside a block brought up to date around it ([`seen_tree`]),
/// so the write merges with a change made since, as it would have against
/// that read. Ids inside the block as read that `seen` holds elsewhere
/// leave there. A block `base` or `seen` lacks is left as it is.
pub fn keep_as_read(seen: &mut Vec<Node>, base: &[Node], keep: &HashSet<String>) {
    for id in keep {
        let Some(then) = find_node(base, id) else {
            continue;
        };
        let Some(path) = path_of(seen, id) else {
            continue;
        };
        if node_at(seen, &path) == Some(then) {
            continue;
        }
        let mut inside = HashSet::new();
        crate::ops::collect_ids(std::slice::from_ref(then), &mut inside);
        let mut here = HashSet::new();
        if let Some(n) = node_at(seen, &path) {
            crate::ops::collect_ids(std::slice::from_ref(n), &mut here);
        }
        let mut held = HashSet::new();
        crate::ops::collect_ids(seen, &mut held);
        for other in inside
            .iter()
            .filter(|other| held.contains(*other) && !here.contains(*other))
        {
            remove(seen, other);
        }
        if let Some(slot) = path_of(seen, id).and_then(|path| node_at_mut(seen, &path)) {
            *slot = then.clone();
        }
    }
}

/// A fresh block in `current`: where it is, and where to place it in a tree
/// that lacks it.
struct Placement {
    id: String,
    /// Its path in `current`.
    at: Vec<usize>,
    /// The nearest ancestor with an id (none: the page), and the path from
    /// it to the block's parent: a table cell has no id, so a paragraph in
    /// one is found through its table.
    anchor: Option<String>,
    below: Vec<usize>,
    /// The ids of its preceding siblings, nearest first.
    preceding: Vec<String>,
}

/// Every block of `nodes` whose id is in `fresh`, in document order, parents
/// before what they hold.
fn locate(
    nodes: &[Node],
    path: &mut Vec<usize>,
    anchor: Option<(&str, usize)>,
    fresh: &HashSet<String>,
    out: &mut Vec<Placement>,
) {
    for (i, n) in nodes.iter().enumerate() {
        path.push(i);
        if let Some(id) = n.id().filter(|id| fresh.contains(*id)) {
            let (anchor_id, from) = match anchor {
                Some((id, depth)) => (Some(id.to_string()), depth),
                None => (None, 0),
            };
            out.push(Placement {
                id: id.to_string(),
                at: path.clone(),
                anchor: anchor_id,
                below: path[from..path.len() - 1].to_vec(),
                preceding: nodes[..i]
                    .iter()
                    .rev()
                    .filter_map(|s| s.id().map(str::to_string))
                    .collect(),
            });
        }
        let next = match n.id() {
            Some(id) => Some((id, path.len())),
            None => anchor,
        };
        locate(&n.content, path, next, fresh, out);
        path.pop();
    }
}

fn path_of(nodes: &[Node], id: &str) -> Option<Vec<usize>> {
    for (i, n) in nodes.iter().enumerate() {
        if n.id() == Some(id) {
            return Some(vec![i]);
        }
        if let Some(mut rest) = path_of(&n.content, id) {
            rest.insert(0, i);
            return Some(rest);
        }
    }
    None
}

fn node_at<'a>(nodes: &'a [Node], path: &[usize]) -> Option<&'a Node> {
    let (first, rest) = path.split_first()?;
    let n = nodes.get(*first)?;
    if rest.is_empty() {
        Some(n)
    } else {
        node_at(&n.content, rest)
    }
}

fn node_at_mut<'a>(nodes: &'a mut [Node], path: &[usize]) -> Option<&'a mut Node> {
    let (first, rest) = path.split_first()?;
    let n = nodes.get_mut(*first)?;
    if rest.is_empty() {
        Some(n)
    } else {
        node_at_mut(&mut n.content, rest)
    }
}

fn remove(nodes: &mut Vec<Node>, id: &str) {
    let Some(path) = path_of(nodes, id) else {
        return;
    };
    let (last, parent) = path.split_last().expect("a path names a block");
    let siblings = if parent.is_empty() {
        nodes
    } else {
        match node_at_mut(nodes, parent) {
            Some(p) => &mut p.content,
            None => return,
        }
    };
    siblings.remove(*last);
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

    fn fresh(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
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

    #[test]
    fn a_fresh_item_inside_a_list_leaves_the_rest_at_base() {
        let base = tree(
            "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">a</p></li>\
             <li data-id=\"i2\"><p data-id=\"p2\">b</p></li></ul><p data-id=\"x\">x</p>",
        );
        let current = tree(
            "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">a, read again</p></li>\
             <li data-id=\"i2\"><p data-id=\"p2\">b, typed since</p></li></ul><p data-id=\"x\">x, typed since</p>",
        );
        let seen = seen_tree(&base, &current, &fresh(&["i1"]));
        assert_eq!(
            show(&seen),
            "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">a, read again</p></li>\
             <li data-id=\"i2\"><p data-id=\"p2\">b</p></li></ul><p data-id=\"x\">x</p>"
        );
    }

    #[test]
    fn a_fresh_block_gone_from_the_page_leaves() {
        let base = tree("<p data-id=\"a\">a</p><p data-id=\"b\">b</p><p data-id=\"c\">c</p>");
        let current = tree("<p data-id=\"a\">a</p><p data-id=\"c\">c, typed since</p>");
        let seen = seen_tree(&base, &current, &fresh(&["b"]));
        assert_eq!(show(&seen), "<p data-id=\"a\">a</p><p data-id=\"c\">c</p>");
    }

    #[test]
    fn a_fresh_block_new_to_the_base_goes_after_its_preceding_sibling() {
        let base = tree("<p data-id=\"a\">a</p><p data-id=\"c\">c</p>");
        let current = tree(
            "<p data-id=\"new0\">first</p><p data-id=\"a\">a</p><p data-id=\"n1\">one</p>\
             <p data-id=\"n2\">two</p><p data-id=\"c\">c</p>",
        );
        let seen = seen_tree(&base, &current, &fresh(&["n1", "n2", "new0"]));
        assert_eq!(
            show(&seen),
            "<p data-id=\"new0\">first</p><p data-id=\"a\">a</p><p data-id=\"n1\">one</p>\
             <p data-id=\"n2\">two</p><p data-id=\"c\">c</p>"
        );
        // Nested: a new item in a list the base has.
        let base = tree("<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">a</p></li></ul>");
        let current = tree(
            "<ul data-id=\"l\"><li data-id=\"i0\"><p data-id=\"p0\">zero</p></li>\
             <li data-id=\"i1\"><p data-id=\"p1\">a</p></li><li data-id=\"i2\"><p data-id=\"p2\">b</p></li></ul>",
        );
        let seen = seen_tree(&base, &current, &fresh(&["i2", "i0"]));
        assert_eq!(show(&seen), show(&current));
        // In a table cell, which has no id: found through the table.
        let base = tree("<table data-id=\"t\"><tr><td><p data-id=\"c1\">x</p></td></tr></table>");
        let current = tree(
            "<table data-id=\"t\"><tr><td><p data-id=\"c1\">x, typed since</p><p data-id=\"c2\">y</p></td></tr></table>",
        );
        let seen = seen_tree(&base, &current, &fresh(&["c2"]));
        assert_eq!(
            show(&seen),
            "<table data-id=\"t\"><tbody><tr><td><p data-id=\"c1\">x</p><p data-id=\"c2\">y</p></td></tr></tbody></table>"
        );
    }

    #[test]
    fn nothing_outside_fresh_comes_from_the_page() {
        let base = tree("<p data-id=\"a\">a</p><p data-id=\"b\">b</p>");
        let current =
            tree("<p data-id=\"a\">a2</p><p data-id=\"z\">new, unread</p><p data-id=\"b\">b2</p>");
        assert_eq!(show(&seen_tree(&base, &current, &fresh(&[]))), show(&base));
        assert_eq!(
            show(&seen_tree(
                &base,
                &current,
                &fresh(&["b", "gone-everywhere"])
            )),
            "<p data-id=\"a\">a</p><p data-id=\"b\">b2</p>"
        );
        // A whole read: everything fresh is the page.
        let all = fresh(&["a", "b", "z"]);
        assert_eq!(show(&seen_tree(&base, &current, &all)), show(&current));
    }

    /// A list the base never held: its item cannot be placed alone, so the
    /// list is what a reader is shown, and the item enters with it.
    #[test]
    fn an_item_of_a_list_the_base_lacks_is_shown_with_its_list() {
        let base = tree("<p data-id=\"p1\">a</p>");
        let current = tree(
            "<p data-id=\"p1\">a</p><ul data-type=\"taskList\" data-id=\"l1\">\
             <li data-type=\"taskItem\" data-checked=\"false\" data-id=\"i1\"><p data-id=\"q1\">Pack</p></li>\
             <li data-type=\"taskItem\" data-checked=\"false\" data-id=\"i2\"><p data-id=\"q2\">Print</p></li></ul>",
        );
        // Alone, the item has nowhere to go.
        assert_eq!(seen_tree(&base, &current, &fresh(&["i1"])), base);
        assert_eq!(shown_for(&base, &current, "i1").as_deref(), Some("l1"));
        assert_eq!(shown_for(&base, &current, "q2").as_deref(), Some("l1"));
        let seen = seen_tree(&base, &current, &fresh(&["l1"]));
        assert!(find_node(&seen, "i1").is_some());
        assert_eq!(seen, current);

        // A block the base has, or whose list the base has, is shown itself.
        assert_eq!(shown_for(&seen, &current, "i1").as_deref(), Some("i1"));
        assert_eq!(shown_for(&base, &current, "p1").as_deref(), Some("p1"));
        let base = tree("<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">a</p></li></ul>");
        let current = tree(
            "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">a</p></li>\
             <li data-id=\"i2\"><p data-id=\"p2\">b</p></li></ul>",
        );
        assert_eq!(shown_for(&base, &current, "p2").as_deref(), Some("i2"));
        assert_eq!(shown_for(&base, &current, "gone"), None);
    }

    /// A block brought up to date around a block the reader is about to
    /// write over: that block stays as it was read.
    #[test]
    fn a_block_kept_as_read_stays_inside_a_fresh_one() {
        let base =
            tree("<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">Pakc bags</p></li></ul>");
        let current = tree(
            "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">Pakc bags tonight</p></li>\
             <li data-id=\"i2\"><p data-id=\"p2\">Book seats</p></li></ul>",
        );
        let mut seen = seen_tree(&base, &current, &fresh(&["l"]));
        keep_as_read(&mut seen, &base, &fresh(&["p1", "not-read"]));
        assert_eq!(
            show(&seen),
            "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">Pakc bags</p></li>\
             <li data-id=\"i2\"><p data-id=\"p2\">Book seats</p></li></ul>"
        );

        // A block kept as read takes back what it held, from wherever the
        // fresh block put it: no id is in the tree twice.
        let base = tree(
            "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">a</p></li><li data-id=\"i2\"><p data-id=\"p2\">b</p></li></ul>\
             <ul data-id=\"m\"><li data-id=\"i3\"><p data-id=\"p3\">c</p></li></ul>",
        );
        let current = tree(
            "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">a</p></li></ul>\
             <ul data-id=\"m\"><li data-id=\"i3\"><p data-id=\"p3\">c</p></li><li data-id=\"i2\"><p data-id=\"p2\">b</p></li></ul>",
        );
        let mut seen = seen_tree(&base, &current, &fresh(&["m"]));
        assert_eq!(show(&seen), show(&current));
        keep_as_read(&mut seen, &base, &fresh(&["l"]));
        assert_eq!(seen, base);
    }

    #[test]
    fn a_block_moved_into_a_fresh_one_leaves_its_old_place() {
        let base = tree(
            "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">a</p></li></ul>\
             <ul data-id=\"m\"><li data-id=\"i2\"><p data-id=\"p2\">b</p></li><li data-id=\"i3\"><p data-id=\"p3\">c</p></li></ul>",
        );
        let current = tree(
            "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">a</p></li><li data-id=\"i2\"><p data-id=\"p2\">b</p></li></ul>\
             <ul data-id=\"m\"><li data-id=\"i3\"><p data-id=\"p3\">c</p></li></ul>",
        );
        let seen = seen_tree(&base, &current, &fresh(&["l"]));
        assert_eq!(show(&seen), show(&current));
    }
}

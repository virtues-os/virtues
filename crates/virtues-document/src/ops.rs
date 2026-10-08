//! The model's write path: edits addressed by block id, content as HTML.
//!
//! Every op is parsed and checked against the contract on a copy of the tree
//! first. Only when the whole batch fits is anything written, in the one
//! Yjs transaction the batch was read in. A refusal changes nothing and says
//! why.
//!
//! Replacing a text block with one of the same type is applied as a text
//! diff inside it, not a delete and insert, so a person typing elsewhere in
//! the same paragraph keeps their cursor and their keystrokes.

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
    XmlFragment, XmlFragmentRef, XmlTextPrelim,
};

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
    /// How each replace landed: "text-diff" or "swap".
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
/// block it replaces (the edited block is still that block); any other id
/// the model wrote that is already taken is renewed.
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
                (Some(id), Some(k)) if top && id == k => id.clone(),
                (Some(id), _) if !taken.contains(id) => id.clone(),
                (None, Some(k)) if top && !taken.contains(k) => k.to_string(),
                _ => new_id(),
            };
            taken.insert(id.clone());
            n.attrs.insert(c.id.attr.clone(), Value::String(id));
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
    if html_bytes > MAX_INPUT_BYTES {
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
    let mut taken = HashSet::new();
    collect_ids(&root.content, &mut taken);
    let mut errors = vec![];
    let mut applied = Applied::default();
    let mut plans = vec![];

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
            let o = parse_slice(c, html, &parent_kind, Mode::Strict);
            if !o.errors.is_empty() {
                errors.extend(
                    o.errors
                        .into_iter()
                        .map(|e| Problem::new(format!("{at}: {}", e.at), e.message)),
                );
                continue;
            }
            if o.nodes.is_empty() && !matches!(kind, PlanKind::Replace) {
                errors.push(Problem::new(&at, "the html holds no blocks"));
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
            if let Some(k) = keep {
                taken.remove(k);
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
            let expr = c
                .node(&parent.kind)
                .and_then(|n| n.spec.content.clone())
                .unwrap_or_default();
            let got = if kinds.is_empty() {
                "nothing".to_string()
            } else {
                kinds.join(", ")
            };
            errors.push(Problem::new(
                &at,
                format!(
                    "after this edit `{}` would hold {got}; it holds `{expr}`",
                    parent.kind
                ),
            ));
            continue;
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
            if plan.nodes.len() == 1 && patch_textblock(c, txn, &el, &plan.nodes[0]) {
                return Some("text-diff");
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

/// Same-type text block, text-only content on both sides: update attributes
/// in place and write only the changed middle of the text.
fn patch_textblock(c: &Contract, txn: &mut TransactionMut, el: &XmlElementRef, new: &Node) -> bool {
    if el.tag().as_ref() != new.kind || !c.is_textblock(&new.kind) {
        return false;
    }
    if new.content.iter().any(|n| !n.is_text()) {
        return false;
    }
    let children: Vec<Out> = ydoc::children(txn, el).map(|(_, child)| child).collect();
    let text = match children.as_slice() {
        [] => None,
        [Out::YXmlText(t)] => Some(t.clone()),
        _ => return false,
    };

    for (k, v) in &new.attrs {
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

    let old: Vec<Node> = text.as_ref().map(|t| read_text(txn, t)).unwrap_or_default();
    let a = flat(&old);
    let b = flat(&new.content);
    let (pre, a_end, _) = span(&a, &b);
    if pre == a.len() && pre == b.len() {
        return true;
    }
    let suf = a.len() - a_end;
    let t = match text {
        Some(t) => t,
        None => el.insert(txn, 0, XmlTextPrelim::new("")),
    };
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
    // Group the inserted middle into runs of equal marks. (Only text reaches
    // here: a block holding an inline atom is swapped, not patched.)
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
    true
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
            ("<a href=\"/x\">ab</a>", "<a href=\"/x\">a</a><a href=\"/y\">b</a>"),
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
        assert!(apply(&c, &doc, None, &[Op::Delete { id: "a".into() }]).is_err());
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
}

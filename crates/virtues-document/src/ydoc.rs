//! Document tree ↔ Yjs XML, in the shape Tiptap's binding (`@tiptap/y-tiptap`,
//! a fork of y-prosemirror) reads and writes:
//!
//! - a node is an `XmlElement` named after its type, every non-null attribute
//!   set on it, numbers as numbers;
//! - a run of text nodes is one `XmlText`, each mark an attribute on the run
//!   whose key is the mark name and whose value is the mark's attrs;
//! - an inline atom (a mention) is an `XmlElement` between runs.
//!
//! A client that meets an element or mark its schema does not know DELETES it
//! from the shared doc (y-tiptap's `createNodeFromYElement` catch). The
//! contract version in `meta` exists so a stale client can refuse to bind.

use crate::contract::{Contract, MAX_INT};
use crate::model::{too_deep, Mark, Node, Problem, MAX_DEPTH};
use crate::wire::MAX_VALUE_DEPTH;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::Arc;
use yrs::types::array::ArrayIter;
use yrs::types::text::YChange;
use yrs::types::{Attrs, Delta};
use yrs::block::ClientID;
use yrs::{
    Any, Doc, In, Map as YMap, OffsetKind, Options, Out, ReadTxn, Text, TransactionMut, WriteTxn,
    Xml, XmlElementPrelim, XmlFragment, XmlFragmentRef, XmlTextPrelim, XmlTextRef,
};

/// The root map that carries the document's contract version.
pub const META: &str = "meta";
/// The key in [`META`] that holds the version.
pub const META_CONTRACT: &str = "contract";

/// A new, empty page document.
///
/// Offsets count UTF-16 code units (`OffsetKind::Utf16`), because JS Yjs
/// does: an index into a text means the same place in the browser and here.
/// With yrs's default (UTF-8 bytes) an edit after any non-ASCII character
/// would land somewhere else than the browser puts it.
pub fn new_doc() -> Doc {
    Doc::with_options(Options {
        client_id: client_id(),
        offset_kind: OffsetKind::Utf16,
        ..Default::default()
    })
}

/// A client id for a document the server writes into: 32 bits, the space
/// Yjs draws its own ids from (`random.uint32`). yrs's default draws 53
/// bits, and yrs 0.18, which a box runs again after `virtues rollback`,
/// reads the id an item is written under as 32 bits: a wider id comes back
/// as another client's, and an editor that reconnects is sent the page's
/// text a second time. Every page document carries the ids of everything
/// written into it, so every server-side writer takes its id from here.
pub fn client_id() -> ClientID {
    ClientID::new(u64::from(fastrand::u32(..)))
}

/// The fragment that holds the tree, created if missing.
pub fn fragment(c: &Contract, txn: &mut TransactionMut) -> XmlFragmentRef {
    txn.get_or_insert_xml_fragment(c.fragment.as_str())
}

/// Write the contract version into the document's `meta` map.
pub fn stamp_version(c: &Contract, txn: &mut TransactionMut) {
    let meta = txn.get_or_insert_map(META);
    meta.insert(txn, META_CONTRACT, Any::from(c.version as i64));
}

/// The contract version a document was stamped with, if any.
pub fn stamped_version<T: ReadTxn>(txn: &T) -> Option<u32> {
    let meta = txn.get_map(META)?;
    let v = out_to_value(&meta.get(txn, META_CONTRACT)?);
    v.as_u64().and_then(|n| u32::try_from(n).ok())
}

/// A value as Yjs stores it. Arrays and objects nested past
/// [`MAX_VALUE_DEPTH`] are written as null there: the conversion recurses
/// once per level, and no attribute or mark the contract names holds one.
pub fn value_to_any(v: &Value) -> Any {
    value_to_any_at(v, 0)
}

fn value_to_any_at(v: &Value, depth: usize) -> Any {
    match v {
        Value::Null => Any::Null,
        Value::Bool(b) => Any::Bool(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => Any::from(i),
            None => Any::from(n.as_f64().unwrap_or(0.0)),
        },
        Value::String(s) => Any::from(s.as_str()),
        Value::Array(_) | Value::Object(_) if depth >= MAX_VALUE_DEPTH => Any::Null,
        Value::Array(a) => Any::from(
            a.iter()
                .map(|v| value_to_any_at(v, depth + 1))
                .collect::<Vec<_>>(),
        ),
        Value::Object(o) => {
            let m: HashMap<String, Any> = o
                .iter()
                .map(|(k, v)| (k.clone(), value_to_any_at(v, depth + 1)))
                .collect();
            Any::from(m)
        }
    }
}

/// JS writes every number as a double; an integral one reads back as an
/// integer, so `level: 2` from the browser equals `level: 2` from here. Up
/// to [`MAX_INT`] either side of zero, the range in which a double is
/// exactly the integer and in which an `int` attribute stays: a larger
/// double is not one integer but the nearest of several.
/// Arrays and maps nested past [`MAX_VALUE_DEPTH`] read as null, for the
/// reason [`value_to_any`] gives; an update holding one is refused before
/// it is decoded ([`crate::wire::decode_update`]).
pub fn any_to_value(a: &Any) -> Value {
    any_to_value_at(a, 0)
}

fn any_to_value_at(a: &Any, depth: usize) -> Value {
    match a {
        Any::Null | Any::Undefined => Value::Null,
        Any::Bool(b) => Value::Bool(*b),
        Any::Number(n) => match n {
            yrs::Number::Int(i) => Value::from(*i),
            yrs::Number::Float(f) if f.fract() == 0.0 && f.abs() <= MAX_INT as f64 => {
                Value::from(*f as i64)
            }
            yrs::Number::Float(f) => Value::from(*f),
        },
        Any::String(s) => Value::String(s.to_string()),
        Any::Array(_) | Any::Map(_) if depth >= MAX_VALUE_DEPTH => Value::Null,
        Any::Array(xs) => Value::Array(xs.iter().map(|x| any_to_value_at(x, depth + 1)).collect()),
        Any::Map(m) => Value::Object(
            m.iter()
                .map(|(k, v)| (k.clone(), any_to_value_at(v, depth + 1)))
                .collect(),
        ),
        Any::Buffer(_) => Value::Null,
    }
}

pub fn out_to_value(o: &Out) -> Value {
    match o {
        Out::Any(a) => any_to_value(a),
        _ => Value::Null,
    }
}

/// Insert `nodes` into `parent` at `index`.
///
/// yrs finds an index by walking the parent's children from the first, so
/// inserting at a fixed index costs the index, and appending one child after
/// another costs the square of their number (ten seconds for a 20,000-block
/// page). The nodes go in last first, each at `index`: the walk is the same
/// short one every time, and none at all for a new element.
pub fn write_nodes<F: XmlFragment>(
    c: &Contract,
    txn: &mut TransactionMut,
    parent: &F,
    index: u32,
    nodes: &[Node],
) {
    for n in nodes.iter().rev() {
        write_node(c, txn, parent, index, n);
    }
}

fn write_node<F: XmlFragment>(
    c: &Contract,
    txn: &mut TransactionMut,
    parent: &F,
    index: u32,
    n: &Node,
) {
    let el = parent.insert(txn, index, XmlElementPrelim::empty(n.kind.as_str()));
    for (k, v) in &n.attrs {
        if !v.is_null() {
            el.insert_attribute(txn, k.as_str(), value_to_any(v));
        }
    }
    if c.is_textblock(&n.kind) {
        write_inline(txn, &el, &n.content);
    } else {
        write_nodes(c, txn, &el, 0, &n.content);
    }
}

pub(crate) fn mark_attrs(marks: &[Mark]) -> Option<Box<Attrs>> {
    if marks.is_empty() {
        return None;
    }
    let mut attrs = Attrs::new();
    for m in marks {
        let inner: HashMap<String, Any> = m
            .attrs
            .iter()
            .map(|(k, v)| (k.clone(), value_to_any(v)))
            .collect();
        attrs.insert(Arc::from(m.kind.as_str()), Any::from(inner));
    }
    Some(Box::new(attrs))
}

/// Text runs become one `XmlText` each; inline atoms sit between them. They
/// go in last first at one index, for the reason [`write_nodes`] gives.
pub fn write_inline<F: XmlFragment>(txn: &mut TransactionMut, el: &F, content: &[Node]) {
    enum Piece<'a> {
        Run(Vec<&'a Node>),
        Atom(&'a Node),
    }
    let mut pieces: Vec<Piece> = vec![];
    for n in content {
        match (n.is_text(), pieces.last_mut()) {
            (true, Some(Piece::Run(run))) => run.push(n),
            (true, _) => pieces.push(Piece::Run(vec![n])),
            (false, _) => pieces.push(Piece::Atom(n)),
        }
    }
    let at = el.len(txn);
    for piece in pieces.iter().rev() {
        match piece {
            Piece::Run(run) => {
                let t: XmlTextRef = el.insert(txn, at, XmlTextPrelim::new(""));
                let delta: Vec<Delta<In>> = run
                    .iter()
                    .map(|n| {
                        Delta::Inserted(
                            In::Any(Any::from(n.text.as_deref().unwrap_or(""))),
                            mark_attrs(&n.marks),
                        )
                    })
                    .collect();
                t.apply_delta(txn, delta);
            }
            Piece::Atom(n) => {
                let child = el.insert(txn, at, XmlElementPrelim::empty(n.kind.as_str()));
                for (k, v) in &n.attrs {
                    if !v.is_null() {
                        child.insert_attribute(txn, k.as_str(), value_to_any(v));
                    }
                }
            }
        }
    }
}

/// Read the children of a fragment or element as they are stored, with no
/// defaults filled.
pub fn read_fragment<T: ReadTxn, F: XmlFragment>(txn: &T, frag: &F) -> Vec<Node> {
    read_children(txn, frag, "", 1, &mut vec![])
}

/// The live children of a fragment or element, in order, each with its index
/// (the position `XmlFragment::get` and `insert` take).
///
/// One pass along the list. `XmlFragment::get(i)` walks from the first child
/// on every call, so reading a page of N blocks by index costs N². And this
/// yields every child, where `get` and `children()` see only shared types: a
/// string or value a peer pushed straight into the fragment would otherwise
/// pass unseen.
pub fn children<'a, T: ReadTxn, F: XmlFragment>(
    txn: &'a T,
    frag: &F,
) -> impl Iterator<Item = (u32, Out)> + 'a {
    ArrayIter::from_ref(frag.as_ref(), txn)
        .enumerate()
        .map(|(i, out)| (i as u32, out))
}

/// Read, recording what the tree cannot express: a value that is a shared
/// type rather than plain data, plain content where a node belongs, an
/// embed inside text, a nested fragment, a tree past [`MAX_DEPTH`].
///
/// `depth` is the level of `frag`'s children (1 for the page's blocks). An
/// element past [`MAX_DEPTH`] is reported, not read, because reading it
/// would overflow the stack; text is a leaf and is read at any level.
pub(crate) fn read_children<T: ReadTxn, F: XmlFragment>(
    txn: &T,
    frag: &F,
    at: &str,
    depth: usize,
    problems: &mut Vec<Problem>,
) -> Vec<Node> {
    let mut out = vec![];
    // A run of plain values is one problem, not one per character.
    let mut in_plain_run = false;
    for (i, child) in children(txn, frag) {
        let plain = !matches!(
            child,
            Out::YXmlElement(_) | Out::YXmlText(_) | Out::YXmlFragment(_)
        );
        if plain && !in_plain_run {
            let what = match &child {
                Out::Any(Any::String(_)) => "text that is not in a text node",
                Out::Any(_) => "a plain value",
                _ => "a shared type that is not XML",
            };
            problems.push(Problem::new(
                at,
                format!("{what} sits where a node belongs"),
            ));
        }
        in_plain_run = plain;
        match child {
            Out::YXmlElement(_) if depth > MAX_DEPTH => {
                problems.push(Problem::new(at, too_deep()));
            }
            Out::YXmlElement(el) => {
                let tag = el.tag().to_string();
                let here = if at.is_empty() {
                    format!("{tag}[{i}]")
                } else {
                    format!("{at} > {tag}[{i}]")
                };
                let mut attrs = Map::new();
                for (k, v) in el.attributes(txn) {
                    match &v {
                        Out::Any(a) => {
                            attrs.insert(k.to_string(), any_to_value(a));
                        }
                        _ => problems.push(Problem::new(
                            &here,
                            format!("attribute `{k}` holds a shared type, not a value"),
                        )),
                    }
                }
                let content = read_children(txn, &el, &here, depth + 1, problems);
                out.push(Node::element(&tag, attrs, content));
            }
            Out::YXmlText(t) => out.extend(read_text_checked(txn, &t, at, problems)),
            Out::YXmlFragment(_) => problems.push(Problem::new(
                at,
                "a nested fragment is not document content",
            )),
            _ => {}
        }
    }
    out
}

/// One `XmlText` as text nodes, a node per run of equal marks.
pub fn read_text<T: ReadTxn>(txn: &T, t: &XmlTextRef) -> Vec<Node> {
    read_text_checked(txn, t, "", &mut vec![])
}

fn read_text_checked<T: ReadTxn>(
    txn: &T,
    t: &XmlTextRef,
    at: &str,
    problems: &mut Vec<Problem>,
) -> Vec<Node> {
    let mut out = vec![];
    for d in t.diff(txn, YChange::identity) {
        let text = match &d.insert {
            Out::Any(Any::String(s)) => s.to_string(),
            _ => {
                problems.push(Problem::new(at, "text holds an embedded object"));
                continue;
            }
        };
        let mut marks: Vec<Mark> = vec![];
        for (k, v) in d.attributes.iter().flat_map(|a| a.iter()) {
            match any_to_value(v) {
                Value::Object(attrs) => marks.push(Mark {
                    kind: k.to_string(),
                    attrs,
                }),
                other => problems.push(Problem::new(
                    at,
                    format!("mark `{k}` holds {other}, not its attributes"),
                )),
            }
        }
        marks.sort_by(|a, b| a.kind.cmp(&b.kind));
        out.push(Node::text(&text, marks));
    }
    out
}

/// Read the tree back with marks in contract order and every attribute the
/// contract gives a node filled, so it compares equal to what ingest
/// produced.
pub fn read_doc<T: ReadTxn>(c: &Contract, txn: &T) -> Vec<Node> {
    let Some(frag) = txn.get_xml_fragment(c.fragment.as_str()) else {
        return vec![];
    };
    let mut nodes = read_fragment(txn, &frag);
    canonicalize(c, &mut nodes);
    nodes
}

/// [`canonical_order`] over a whole tree.
pub fn canonicalize(c: &Contract, nodes: &mut [Node]) {
    for n in nodes {
        n.walk_mut(&mut |n| canonical_order(c, n));
    }
}

/// Marks sorted by contract rank; a node's and a mark's missing attributes
/// filled with their defaults, as ProseMirror's `node.attrs` holds them.
pub fn canonical_order(c: &Contract, n: &mut Node) {
    n.marks.sort_by_key(|m| c.mark_rank(&m.kind));
    if !n.is_text() {
        for (k, d) in c.default_attrs(&n.kind) {
            n.attrs.entry(k).or_insert(d);
        }
    }
    for m in &mut n.marks {
        if let Some(spec) = c.mark(&m.kind) {
            for (k, a) in spec.attrs() {
                m.attrs
                    .entry(k.clone())
                    .or_insert_with(|| a.default_value());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::{parse, Mode};
    use yrs::Transact;

    fn round_trip(html: &str) {
        let c = Contract::load();
        let o = parse(&c, html, "doc", Mode::Strict);
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        let mut expected = o.nodes.clone();
        canonicalize(&c, &mut expected);

        let doc = new_doc();
        {
            let mut txn = doc.transact_mut();
            let frag = fragment(&c, &mut txn);
            write_nodes(&c, &mut txn, &frag, 0, &o.nodes);
        }
        let back = read_doc(&c, &doc.transact());
        assert_eq!(
            serde_json::to_value(&back).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
    }

    #[test]
    fn tree_round_trips_through_yjs() {
        round_trip(
            "<h2 data-id=\"h1\">Title</h2><p data-id=\"p1\">a <strong>b <em>c</em></strong> \
             <virtues-mention to=\"/person/1\" label=\"Nick\"></virtues-mention> d<br>e \
             <a href=\"/x\"><code>fn</code></a></p>\
             <ul data-type=\"taskList\" data-id=\"t\"><li data-type=\"taskItem\" data-checked=\"true\" data-id=\"i\"><p data-id=\"q\">done</p></li></ul>\
             <table data-id=\"tb\"><tr><th><p data-id=\"x\">A</p></th><th><p data-id=\"z\">B</p></th></tr><tr><td colspan=\"2\"><p data-id=\"y\">1</p></td></tr></table>\
             <virtues-applet data-id=\"ap\" ref=\"sleep-week\" height=\"240\"></virtues-applet>",
        );
    }

    #[test]
    fn deep_headings_and_alignment_round_trip() {
        round_trip(
            "<h4>four</h4><h5>five</h5><h6>six</h6>\
             <table><tr><th align=\"center\">A</th><th align=\"right\">B</th></tr><tr><td align=\"left\">1</td><td>2</td></tr></table>",
        );
    }

    #[test]
    fn nodes_written_mid_list_keep_their_order() {
        let c = Contract::load();
        let o = parse(&c, "<p>a</p><p>d</p>", "doc", Mode::Strict);
        let doc = new_doc();
        {
            let mut txn = doc.transact_mut();
            let frag = fragment(&c, &mut txn);
            write_nodes(&c, &mut txn, &frag, 0, &o.nodes);
            let mid = parse(&c, "<p>b</p><p>c <em>e</em><virtues-mention to=\"/p/1\" label=\"N\"></virtues-mention></p>", "doc", Mode::Strict);
            write_nodes(&c, &mut txn, &frag, 1, &mid.nodes);
        }
        let text: Vec<String> = read_doc(&c, &doc.transact())
            .iter()
            .map(Node::text_content)
            .collect();
        assert_eq!(text, ["a", "b", "c e", "d"]);
        let p = &read_doc(&c, &doc.transact())[2];
        let kinds: Vec<&str> = p.content.iter().map(|n| n.kind.as_str()).collect();
        assert_eq!(kinds, ["text", "text", "mention"]);
    }

    #[test]
    fn the_version_stamp_reads_back() {
        let c = Contract::load();
        let doc = new_doc();
        assert_eq!(stamped_version(&doc.transact()), None);
        stamp_version(&c, &mut doc.transact_mut());
        assert_eq!(stamped_version(&doc.transact()), Some(c.version));
    }

    #[test]
    fn page_docs_count_utf16() {
        assert!(matches!(new_doc().offset_kind(), OffsetKind::Utf16));
    }

    #[test]
    fn page_docs_write_under_a_32_bit_client_id() {
        // yrs's own default lands above `u32::MAX` all but once in two
        // million draws, so a hundred docs would catch it.
        for _ in 0..100 {
            assert!(new_doc().client_id().get() <= u64::from(u32::MAX));
        }
        let c = Contract::load();
        let doc = crate::doc_from_nodes(parse(&c, "<p>x</p>", "doc", Mode::Strict).nodes);
        assert!(doc.client_id().get() <= u64::from(u32::MAX));
    }
}

//! Is this tree inside the contract? The check a whole document passes
//! before the server accepts it, whoever wrote it.
//!
//! Ingest only ever builds trees inside the contract; this is for trees that
//! arrive another way: a Yjs update from a browser, a stored document, a tree
//! built by code. A client running a newer contract can write a node this
//! server does not know, and a client running an older one deletes what it
//! cannot read, so an update that fails here is refused before it is applied
//! or relayed.
//!
//! Not every problem is one a client wrote. Two edits that are each inside
//! the contract on their own device can merge into a document that is not:
//! a laptop and a phone each delete one of a list's two items, and the list
//! holds none. Such a shape is the merge's, the device that sent the second
//! edit cannot send any other, and refusing it would shut that device out of
//! the page. So [`check_update`] refuses only what no merge can make, and the
//! server puts a merged shape right in its own transaction
//! ([`crate::repair`]).

use crate::contract::Contract;
use crate::model::{too_deep, Node, Problem, MAX_DEPTH};
use crate::wire::{decode_update, encode_state};
use crate::ydoc;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use yrs::{ReadTxn, Transact, Update};

/// Whether a problem refuses the update that brings it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    /// Nothing inside the contract makes it, alone or merged: an unknown
    /// node, mark or attribute, a value the contract refuses, a node where
    /// its parent cannot hold it at all, a stray root, a tree too deep.
    Refused,
    /// A shape a merge of edits inside the contract can make: children that
    /// may each sit in their parent, in a number or order its content does
    /// not allow (an emptied list), or a block id used twice (two devices
    /// moving the same block).
    Shape,
}

/// What a check finds, each problem with its class, in document order.
#[derive(Default)]
struct Found {
    problems: Vec<(Class, Problem)>,
    /// Block ids seen so far, to find one used twice.
    ids: HashSet<String>,
}

impl Found {
    fn refuse(&mut self, at: &str, message: impl Into<String>) {
        self.problems.push((Class::Refused, Problem::new(at, message)));
    }

    fn all(self) -> Vec<Problem> {
        self.problems.into_iter().map(|(_, p)| p).collect()
    }
}

/// Problems with `nodes` as the content of a node of type `parent`. Empty
/// means the tree is inside the contract.
pub fn validate(c: &Contract, nodes: &[Node], parent: &str) -> Vec<Problem> {
    let mut found = Found::default();
    children(c, parent, nodes, parent, 1, &mut found);
    found.all()
}

/// Problems with a whole Yjs document: what [`validate`] finds in its tree,
/// plus what the tree cannot express (a root other than the fragment and
/// `meta` holding content, plain content where a node belongs, an embed
/// inside text, an attribute that is a shared type, a tree past
/// [`MAX_DEPTH`]) and a `meta` other than the contract version. Only
/// integrated content is seen; an update yrs holds back for missing
/// dependencies is checked when it integrates.
pub fn validate_doc<T: ReadTxn>(c: &Contract, txn: &T) -> Vec<Problem> {
    found_doc(c, txn).all()
}

fn found_doc<T: ReadTxn>(c: &Contract, txn: &T) -> Found {
    let mut problems = vec![];
    for (name, root) in txn.root_refs() {
        if name == c.fragment {
            continue;
        }
        let holds_text_or_items = root.try_branch().map(|b| b.len() > 0).unwrap_or(false);
        if name == ydoc::META {
            meta(txn, holds_text_or_items, &mut problems);
            continue;
        }
        let holds_entries = txn
            .get_map(name)
            .map(|m| yrs::Map::len(&m, txn) > 0)
            .unwrap_or(false);
        if holds_text_or_items || holds_entries {
            problems.push(Problem::new(
                name,
                format!("the document holds `{name}`, which a page does not have"),
            ));
        }
    }
    let nodes = txn.get_xml_fragment(c.fragment.as_str()).map(|frag| {
        let mut nodes = ydoc::read_children(txn, &frag, &c.fragment, 1, &mut problems);
        ydoc::canonicalize(c, &mut nodes);
        nodes
    });
    // What the tree cannot express, no device wrote inside the contract.
    let mut found = Found {
        problems: problems.into_iter().map(|p| (Class::Refused, p)).collect(),
        ..Found::default()
    };
    if let Some(nodes) = nodes {
        children(c, &c.fragment, &nodes, &c.fragment, 1, &mut found);
    }
    found
}

/// `meta` holds one entry, the contract version as a whole number
/// ([`ydoc::stamp_version`]), and nothing else.
fn meta<T: ReadTxn>(txn: &T, holds_items: bool, problems: &mut Vec<Problem>) {
    if holds_items {
        problems.push(Problem::new(
            ydoc::META,
            "`meta` holds content other than its entries",
        ));
    }
    let Some(m) = txn.get_map(ydoc::META) else {
        return;
    };
    for (key, value) in yrs::Map::iter(&m, txn) {
        if key != ydoc::META_CONTRACT {
            problems.push(Problem::new(
                ydoc::META,
                format!("`meta` has no entry `{key}`"),
            ));
            continue;
        }
        let v = ydoc::out_to_value(&value);
        if v.as_u64().and_then(|n| u32::try_from(n).ok()).is_none() {
            problems.push(Problem::new(
                ydoc::META,
                format!("`meta.{key}` must be a whole number; got {v}"),
            ));
        }
    }
}

/// Everything `meta` holds, to tell whether an update changed it.
fn meta_entries<T: ReadTxn>(txn: &T) -> Value {
    let Some(m) = txn.get_map(ydoc::META) else {
        return Value::Null;
    };
    Value::Object(
        yrs::Map::iter(&m, txn)
            .map(|(k, v)| (k.to_string(), ydoc::out_to_value(&v)))
            .collect(),
    )
}

/// A problem's identity across an edit: its message and where it is, with
/// positions left out, since an insert before a node moves its index.
fn problem_key(p: &Problem) -> (String, String) {
    let mut at = String::with_capacity(p.at.len());
    let mut chars = p.at.chars().peekable();
    while let Some(ch) = chars.next() {
        at.push(ch);
        if ch == '[' {
            while chars.peek().is_some_and(char::is_ascii_digit) {
                chars.next();
            }
        }
    }
    (at, p.message.clone())
}

/// Problems `update` would bring into `doc` that refuse it: what the
/// document holds after the update that it did not hold before, and that no
/// merge of edits inside the contract can make. A shape a merge can make
/// (see the module's documentation) is taken, and the server repairs it
/// ([`crate::repair`]); [`validate_doc`] still names it until then. The
/// check runs on a copy, which costs a copy of the document per call; the
/// document is only read.
///
/// A problem the document already had is not the update's, so a document
/// already outside the contract (a root an older writer left, say) still
/// takes edits that do not make it worse; [`validate_doc`] says whether it
/// is. `meta` is the server's: an update that changes it at all, the
/// contract version included, is refused, because the version decides which
/// clients may bind and a client that lowers it lets in the ones that delete
/// what they cannot read.
///
/// An update that does not decode or apply is an error.
pub fn check_update(c: &Contract, doc: &yrs::Doc, update: &[u8]) -> anyhow::Result<Vec<Problem>> {
    use yrs::updates::decoder::Decode;
    let scratch = ydoc::new_doc();
    {
        let state = encode_state(&doc.transact(), &yrs::StateVector::default());
        let mut txn = scratch.transact_mut();
        txn.apply_update(Update::decode_v1(&state)?)?;
        txn.apply_update(decode_update(update)?)?;
    }
    let after_txn = scratch.transact();
    let before_txn = doc.transact();
    let mut problems = vec![];
    if meta_entries(&before_txn) != meta_entries(&after_txn) {
        problems.push(Problem::new(
            ydoc::META,
            "the update changes `meta`, which only the server writes",
        ));
    }
    let refused = |found: Found| {
        found
            .problems
            .into_iter()
            .filter(|(class, _)| *class == Class::Refused)
            .map(|(_, p)| p)
            .collect::<Vec<_>>()
    };
    let after = refused(found_doc(c, &after_txn));
    if after.is_empty() {
        return Ok(problems);
    }
    // Only now is the document as it stands read: a valid result, the
    // usual case, needs no comparison.
    let mut had: HashMap<(String, String), usize> = HashMap::new();
    for p in refused(found_doc(c, &before_txn)) {
        *had.entry(problem_key(&p)).or_default() += 1;
    }
    for p in after {
        match had.get_mut(&problem_key(&p)) {
            Some(n) if *n > 0 => *n -= 1,
            _ => problems.push(p),
        }
    }
    Ok(problems)
}

/// Whether `update` changes `doc`'s `meta`, and nothing more: for a page
/// still in markdown, whose text has no contract to check but whose `meta`
/// is the server's all the same.
///
/// Such a page has no `meta` at all (the server writes one when it raises
/// the page to a contract), and then the update is read, not applied: an
/// item lands in `meta` only by naming `meta` as its parent, or by coming
/// after an item already in it, whose parent it takes. With no item in
/// `meta`, the first one in has to name it, and the update that carries that
/// item is refused here, whenever it comes and whatever waits on it.
/// Applying the update to an empty document instead would not see an entry
/// written after an edit to the page's text: yrs holds back every later item
/// of a client whose item it cannot place, and in an empty document the text
/// the edit goes into is missing.
///
/// A document that has `meta`, even emptied, is compared on a copy, as
/// [`check_update`] compares it, items the document holds back included.
pub fn changes_meta(doc: &yrs::Doc, update: &[u8]) -> anyhow::Result<bool> {
    let (before, has_meta) = {
        let txn = doc.transact();
        (meta_entries(&txn), txn.get_map(ydoc::META).is_some())
    };
    if !has_meta {
        return Ok(crate::wire::named_roots(update)?
            .iter()
            .any(|root| root == ydoc::META));
    }
    use yrs::updates::decoder::Decode;
    let scratch = ydoc::new_doc();
    let state = encode_state(&doc.transact(), &yrs::StateVector::default());
    {
        let mut txn = scratch.transact_mut();
        txn.apply_update(Update::decode_v1(&state)?)?;
        txn.apply_update(decode_update(update)?)?;
    }
    let after = meta_entries(&scratch.transact());
    Ok(after != before)
}

fn label(n: &Node, i: usize) -> String {
    match n.id() {
        Some(id) => format!("{}#{id}", n.kind),
        None => format!("{}[{i}]", n.kind),
    }
}

/// `depth` is the level of `nodes` (1 for a page's blocks). A node past
/// [`MAX_DEPTH`] is reported and not walked: the walk recurses once per
/// level, and a tree built in code has had no other check. Text is a leaf
/// and is checked at any level.
fn children(
    c: &Contract,
    parent: &str,
    nodes: &[Node],
    at: &str,
    depth: usize,
    found: &mut Found,
) {
    if depth > MAX_DEPTH && nodes.iter().any(|n| !n.is_text()) {
        found.refuse(at, too_deep());
        return;
    }
    let kinds: Vec<&str> = nodes.iter().map(|n| n.kind.as_str()).collect();
    let all_known = nodes
        .iter()
        .all(|n| n.is_text() || c.node(&n.kind).is_some());
    if all_known && !c.content_matches(parent, &kinds) {
        let expr = c
            .node(parent)
            .and_then(|n| n.spec.content.clone())
            .unwrap_or_default();
        let got = if kinds.is_empty() {
            "nothing".to_string()
        } else {
            kinds.join(", ")
        };
        // Every child may sit here, just not in this number or order: a
        // merge can leave that. One that may not sit here at all, no
        // device wrote inside the contract.
        let class = if kinds.iter().all(|k| c.may_contain(parent, k)) {
            Class::Shape
        } else {
            Class::Refused
        };
        found.problems.push((
            class,
            Problem::new(at, format!("`{parent}` holds `{expr}`; got {got}")),
        ));
    }
    let textblock = c.is_textblock(parent);
    for (i, n) in nodes.iter().enumerate() {
        let here = format!("{at} > {}", label(n, i));
        if n.is_text() {
            text(c, parent, n, &here, found);
            continue;
        }
        let Some(spec) = c.node(&n.kind) else {
            found.refuse(
                &here,
                format!("`{}` is not part of the document contract", n.kind),
            );
            continue;
        };
        if n.kind == c.fragment {
            found.refuse(
                &here,
                format!("`{}` is the document itself, not a block in it", n.kind),
            );
            continue;
        }
        if textblock != c.is_inline(&n.kind) {
            // content_matches above names the misplaced kind.
            continue;
        }
        if n.text.is_some() || !n.marks.is_empty() {
            found.refuse(
                &here,
                format!("`{}` is not text and carries no text or marks", n.kind),
            );
        }
        attrs(
            c,
            &n.kind,
            spec.attrs().iter(),
            spec.spec.id,
            &n.attrs,
            &here,
            found,
        );
        if spec.spec.id {
            if let Some(id) = n.id() {
                if !found.ids.insert(id.to_string()) {
                    found.problems.push((
                        Class::Shape,
                        Problem::new(
                            &here,
                            format!("block id `{id}` is already an earlier block's"),
                        ),
                    ));
                }
            }
        }
        if spec.spec.content.is_none() && !n.content.is_empty() {
            found.refuse(&here, format!("`{}` holds no content", n.kind));
            continue;
        }
        if spec.spec.content.is_some() {
            children(c, &n.kind, &n.content, &here, depth + 1, found);
        }
        if n.kind == "table" {
            // A merge can leave a table that is not a grid (one device adds
            // a column while another adds a row); the editor's table plugin
            // evens it out on the next device that opens it.
            for message in crate::table::problems(n) {
                found
                    .problems
                    .push((Class::Shape, Problem::new(&here, message)));
            }
        }
    }
}

fn text(c: &Contract, parent: &str, n: &Node, at: &str, found: &mut Found) {
    if n.text.as_deref().unwrap_or("").is_empty() {
        found.refuse(at, "a text node holds no text");
    }
    if !n.attrs.is_empty() || !n.content.is_empty() {
        found.refuse(at, "a text node carries only text and marks");
    }
    for (i, m) in n.marks.iter().enumerate() {
        let Some(spec) = c.mark(&m.kind) else {
            found.refuse(
                at,
                format!("mark `{}` is not part of the document contract", m.kind),
            );
            continue;
        };
        if !c.allows_mark(parent, &m.kind) {
            found.refuse(at, format!("`{parent}` cannot carry `{}`", m.kind));
        }
        if let Some(other) = n.marks[..i]
            .iter()
            .find(|o| c.marks_exclude(&o.kind, &m.kind))
        {
            found.refuse(
                at,
                format!("`{}` cannot be combined with `{}`", m.kind, other.kind),
            );
        }
        attrs(c, &m.kind, spec.attrs().iter(), false, &m.attrs, at, found);
    }
}

fn attrs<'a>(
    c: &Contract,
    kind: &str,
    specs: impl Iterator<Item = &'a (String, crate::contract::AttrSpec)> + Clone,
    has_id: bool,
    values: &Map<String, Value>,
    at: &str,
    found: &mut Found,
) {
    for (key, value) in values {
        if has_id && *key == c.id.attr {
            if !(value.is_null() || value.is_string()) {
                found.refuse(at, format!("`{key}` must be text; got {value}"));
            }
            continue;
        }
        match specs.clone().find(|(k, _)| k == key) {
            Some((_, a)) => {
                if let Err(message) = a.check(c, key, value) {
                    found.refuse(at, message);
                }
            }
            None => found.refuse(at, format!("`{kind}` has no attribute `{key}`")),
        }
    }
    for (key, a) in specs {
        if a.required && values.get(key).map(Value::is_null).unwrap_or(true) {
            found.refuse(at, format!("`{kind}` needs `{key}`"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::{parse, Mode};
    use crate::model::Mark;
    use serde_json::json;
    use yrs::{Text, Transact, WriteTxn, XmlElementPrelim, XmlFragment};

    fn tree(html: &str) -> Vec<Node> {
        let o = parse(&Contract::load(), html, "doc", Mode::Strict);
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        o.nodes
    }

    #[test]
    fn what_ingest_builds_is_valid() {
        let c = Contract::load();
        let nodes = tree(
            "<h6>t</h6><p>a <a href=\"/x\"><strong>b</strong></a> <virtues-mention to=\"/p/1\" label=\"N\"></virtues-mention></p>\
             <table><tr><th align=\"center\">A</th></tr></table><pre><code class=\"language-rust\">x</code></pre>\
             <virtues-applet ref=\"r\"></virtues-applet>",
        );
        assert_eq!(validate(&c, &nodes, "doc"), []);
    }

    #[test]
    fn trees_outside_the_contract_are_named() {
        let c = Contract::load();
        let mut nodes = tree("<h2>t</h2><p>x</p><table><tr><td>1</td></tr></table>");
        nodes[0].attrs.insert("level".into(), json!(7));
        nodes[1].content[0].marks.push(Mark {
            kind: "sparkle".into(),
            attrs: Map::new(),
        });
        nodes[2].content[0].content[0]
            .attrs
            .insert("align".into(), json!("justify"));
        nodes.push(Node::element("callout", Map::new(), vec![]));
        nodes.push(Node::element("details", Map::new(), vec![]));
        let problems = validate(&c, &nodes, "doc");
        let msgs: Vec<&str> = problems.iter().map(|p| p.message.as_str()).collect();
        assert!(msgs.iter().any(|m| m.contains("`level`")), "{msgs:?}");
        assert!(msgs.iter().any(|m| m.contains("sparkle")), "{msgs:?}");
        assert!(msgs.iter().any(|m| m.contains("justify")), "{msgs:?}");
        assert!(
            msgs.iter().any(|m| m.contains("`callout` holds")),
            "{msgs:?}"
        );
        assert!(
            msgs.iter().any(|m| m.contains("`details` is not part")),
            "{msgs:?}"
        );
    }

    #[test]
    fn marks_obey_the_block_and_each_other() {
        let c = Contract::load();
        let mut nodes = tree("<pre>x</pre><p><strong>y</strong></p>");
        nodes[0].content[0].marks.push(Mark {
            kind: "bold".into(),
            attrs: Map::new(),
        });
        nodes[1].content[0].marks.push(Mark {
            kind: "bold".into(),
            attrs: Map::new(),
        });
        nodes[1].content[0].marks.push(Mark {
            kind: "link".into(),
            attrs: Map::new(),
        });
        let msgs: Vec<String> = validate(&c, &nodes, "doc")
            .into_iter()
            .map(|p| p.message)
            .collect();
        assert!(
            msgs.iter()
                .any(|m| m.contains("`codeBlock` cannot carry `bold`")),
            "{msgs:?}"
        );
        assert!(
            msgs.iter()
                .any(|m| m.contains("`bold` cannot be combined with `bold`")),
            "{msgs:?}"
        );
        assert!(
            msgs.iter().any(|m| m.contains("`link` needs `href`")),
            "{msgs:?}"
        );
    }

    #[test]
    fn a_doc_with_a_stray_root_or_unknown_element_is_refused() {
        let c = Contract::load();
        let doc = crate::doc_from_nodes(tree("<p>x</p>"));
        assert_eq!(validate_doc(&c, &doc.transact()), []);

        // A stale client binding the old Y.Text and typing into it.
        let stale = crate::doc_from_nodes(tree("<p>x</p>"));
        {
            let mut txn = stale.transact_mut();
            let t = txn.get_or_insert_text("content");
            t.insert(&mut txn, 0, "# typed into the void");
        }
        let problems = validate_doc(&c, &stale.transact());
        assert!(
            problems.iter().any(|p| p.message.contains("`content`")),
            "{problems:?}"
        );

        // An element a newer contract would know.
        let newer = crate::doc_from_nodes(tree("<p>x</p>"));
        {
            let mut txn = newer.transact_mut();
            let frag = txn.get_or_insert_xml_fragment("doc");
            frag.push_back(&mut txn, XmlElementPrelim::empty("poll"));
        }
        let problems = validate_doc(&c, &newer.transact());
        assert!(
            problems.iter().any(|p| p.message.contains("`poll`")),
            "{problems:?}"
        );
    }

    #[test]
    fn an_update_is_checked_before_it_is_applied() {
        let c = Contract::load();
        let doc = crate::doc_from_nodes(tree("<p>x</p>"));
        // A peer that synced the document, then wrote a heading level the
        // contract refuses.
        let peer = ydoc::new_doc();
        {
            let state = doc
                .transact()
                .encode_state_as_update_v1(&yrs::StateVector::default());
            use yrs::updates::decoder::Decode;
            peer.transact_mut()
                .apply_update(Update::decode_v1(&state).unwrap())
                .unwrap();
        }
        let before = peer.transact().state_vector();
        {
            let mut txn = peer.transact_mut();
            let frag = txn.get_or_insert_xml_fragment("doc");
            let h = frag.push_back(&mut txn, XmlElementPrelim::empty("heading"));
            yrs::Xml::insert_attribute(&h, &mut txn, "level", yrs::Any::from(9i64));
        }
        let update = peer.transact().encode_diff_v1(&before);
        let problems = check_update(&c, &doc, &update).unwrap();
        assert!(
            problems.iter().any(|p| p.message.contains("`level`")),
            "{problems:?}"
        );
        // The real document is untouched.
        assert_eq!(validate_doc(&c, &doc.transact()), []);
        assert_eq!(ydoc::read_doc(&c, &doc.transact()).len(), 1);
    }

    /// A peer that synced `doc`, and the update it sends after `edit`.
    fn peer_update(doc: &yrs::Doc, edit: impl FnOnce(&mut yrs::TransactionMut)) -> Vec<u8> {
        use yrs::updates::decoder::Decode;
        let peer = ydoc::new_doc();
        let state = doc
            .transact()
            .encode_state_as_update_v1(&yrs::StateVector::default());
        peer.transact_mut()
            .apply_update(Update::decode_v1(&state).unwrap())
            .unwrap();
        let before = peer.transact().state_vector();
        edit(&mut peer.transact_mut());
        let update = peer.transact().encode_diff_v1(&before);
        update
    }

    fn messages(problems: &[Problem]) -> Vec<&str> {
        problems.iter().map(|p| p.message.as_str()).collect()
    }

    #[test]
    fn only_the_server_writes_meta() {
        use yrs::Map as _;
        let c = Contract::load();
        let doc = crate::doc_from_nodes(tree("<p>x</p>"));
        assert_eq!(ydoc::stamped_version(&doc.transact()), Some(c.version));

        let lowered = peer_update(&doc, |txn| {
            let meta = txn.get_or_insert_map(ydoc::META);
            meta.insert(txn, ydoc::META_CONTRACT, yrs::Any::from(0i64));
        });
        let removed = peer_update(&doc, |txn| {
            let meta = txn.get_or_insert_map(ydoc::META);
            meta.remove(txn, ydoc::META_CONTRACT);
        });
        let raised = peer_update(&doc, |txn| {
            let meta = txn.get_or_insert_map(ydoc::META);
            meta.insert(txn, ydoc::META_CONTRACT, yrs::Any::from(99i64));
        });
        let junk = peer_update(&doc, |txn| {
            let meta = txn.get_or_insert_map(ydoc::META);
            meta.insert(txn, "junk", "x");
        });
        for (what, update) in [
            ("lowered", lowered),
            ("removed", removed),
            ("raised", raised),
            ("junk", junk),
        ] {
            let problems = check_update(&c, &doc, &update).unwrap();
            assert!(
                messages(&problems)
                    .iter()
                    .any(|m| m.contains("only the server writes")),
                "{what}: {problems:?}"
            );
        }
        // The same value written again changes nothing, and is not refused.
        let same = peer_update(&doc, |txn| {
            let meta = txn.get_or_insert_map(ydoc::META);
            meta.insert(txn, ydoc::META_CONTRACT, yrs::Any::from(c.version as i64));
        });
        assert_eq!(check_update(&c, &doc, &same).unwrap(), []);
    }

    #[test]
    fn a_markdown_page_takes_text_but_not_meta() {
        use yrs::updates::decoder::Decode;
        use yrs::Map as _;
        let page = ydoc::new_doc();
        {
            let mut txn = page.transact_mut();
            let t = txn.get_or_insert_text("content");
            t.insert(&mut txn, 0, "# notes");
        }
        let typed = peer_update(&page, |txn| {
            let t = txn.get_or_insert_text("content");
            t.insert(txn, 0, "More. ");
        });
        let stamped = peer_update(&page, |txn| {
            let meta = txn.get_or_insert_map(ydoc::META);
            meta.insert(txn, ydoc::META_CONTRACT, yrs::Any::from(1i64));
        });
        assert!(!changes_meta(&page, &typed).unwrap());
        assert!(changes_meta(&page, &stamped).unwrap());

        // Typing into the page's text and stamping it in one transaction:
        // applied to a document without that text, yrs would hold the stamp
        // back with the typing, and the stamp would pass unseen.
        for at in [0, 4] {
            let both = peer_update(&page, |txn| {
                let t = txn.get_or_insert_text("content");
                t.insert(txn, at, "x");
                let meta = txn.get_or_insert_map(ydoc::META);
                meta.insert(txn, ydoc::META_CONTRACT, yrs::Any::from(1i64));
            });
            assert!(changes_meta(&page, &both).unwrap(), "typed at {at}, then stamped");
        }
        // A stamp sent apart from the typing before it, which the page has
        // not seen: it cannot be placed yet, and is refused all the same.
        let peer = ydoc::new_doc();
        {
            let state = page
                .transact()
                .encode_state_as_update_v1(&yrs::StateVector::default());
            let mut txn = peer.transact_mut();
            txn.apply_update(Update::decode_v1(&state).unwrap()).unwrap();
            let t = txn.get_or_insert_text("content");
            t.insert(&mut txn, 2, "unsent ");
        }
        let after_typing = peer.transact().state_vector();
        {
            let mut txn = peer.transact_mut();
            let meta = txn.get_or_insert_map(ydoc::META);
            meta.insert(&mut txn, ydoc::META_CONTRACT, yrs::Any::from(1i64));
        }
        let stamp_alone = peer.transact().encode_diff_v1(&after_typing);
        assert!(changes_meta(&page, &stamp_alone).unwrap());

        // A `meta` whose entries were all removed still has them, and an
        // entry written over a removed one names no parent: compared on a
        // copy.
        let emptied = ydoc::new_doc();
        {
            let mut txn = emptied.transact_mut();
            let t = txn.get_or_insert_text("content");
            t.insert(&mut txn, 0, "# notes");
            let meta = txn.get_or_insert_map(ydoc::META);
            meta.insert(&mut txn, ydoc::META_CONTRACT, yrs::Any::from(1i64));
            meta.remove(&mut txn, ydoc::META_CONTRACT);
        }
        let restamped = peer_update(&emptied, |txn| {
            let meta = txn.get_or_insert_map(ydoc::META);
            meta.insert(txn, ydoc::META_CONTRACT, yrs::Any::from(1i64));
        });
        assert_eq!(crate::wire::named_roots(&restamped).unwrap(), Vec::<String>::new());
        assert!(changes_meta(&emptied, &restamped).unwrap());

        // A page that holds a `meta` (one an older writer left) is compared
        // on a copy: overwriting its entry changes it, typing does not.
        {
            let mut txn = page.transact_mut();
            let meta = txn.get_or_insert_map(ydoc::META);
            meta.insert(&mut txn, "junk", "x");
        }
        let typed = peer_update(&page, |txn| {
            let t = txn.get_or_insert_text("content");
            t.insert(txn, 0, "Again. ");
        });
        let overwritten = peer_update(&page, |txn| {
            let meta = txn.get_or_insert_map(ydoc::META);
            meta.insert(txn, "junk", yrs::Any::from(1i64));
        });
        assert!(!changes_meta(&page, &typed).unwrap());
        assert!(changes_meta(&page, &overwritten).unwrap());
    }

    #[test]
    fn meta_holds_the_version_and_nothing_else() {
        use yrs::Map as _;
        let c = Contract::load();
        let doc = crate::doc_from_nodes(tree("<p>x</p>"));
        {
            let mut txn = doc.transact_mut();
            let meta = txn.get_or_insert_map(ydoc::META);
            meta.insert(&mut txn, ydoc::META_CONTRACT, "999");
            meta.insert(&mut txn, "junk", "x");
        }
        let problems = validate_doc(&c, &doc.transact());
        let msgs = messages(&problems);
        assert!(
            msgs.iter().any(|m| m.contains("must be a whole number")),
            "{msgs:?}"
        );
        assert!(
            msgs.iter().any(|m| m.contains("no entry `junk`")),
            "{msgs:?}"
        );
    }

    #[test]
    fn plain_content_where_a_node_belongs_is_refused() {
        let c = Contract::load();
        let doc = crate::doc_from_nodes(tree("<blockquote><p>x</p></blockquote>"));
        // Text typed straight into the fragment, as a stale binding of the
        // page's root as a Y.Text would.
        let text = peer_update(&doc, |txn| {
            let t = txn.get_or_insert_text(c.fragment.as_str());
            t.insert(txn, 0, "typed as text");
        });
        // A value pushed into the fragment as if it were an array.
        let value = peer_update(&doc, |txn| {
            let a = txn.get_or_insert_array(c.fragment.as_str());
            yrs::Array::push_back(&a, txn, yrs::Any::from(42i64));
        });
        // The same, inside a block.
        let nested = peer_update(&doc, |txn| {
            let frag = txn.get_or_insert_xml_fragment(c.fragment.as_str());
            let Some(yrs::XmlOut::Element(quote)) = frag.get(txn, 0) else {
                panic!("the blockquote")
            };
            let a = yrs::ArrayRef::from(yrs::branch::BranchPtr::from(
                AsRef::<yrs::branch::Branch>::as_ref(&quote),
            ));
            yrs::Array::insert(&a, txn, 0, "loose");
        });
        for (what, update) in [("text", text), ("value", value), ("nested", nested)] {
            let problems = check_update(&c, &doc, &update).unwrap();
            assert!(
                messages(&problems)
                    .iter()
                    .any(|m| m.contains("where a node belongs")),
                "{what}: {problems:?}"
            );
        }
    }

    #[test]
    fn an_update_answers_only_for_what_it_brings() {
        let c = Contract::load();
        let doc = crate::doc_from_nodes(tree("<p>hello</p>"));
        // A document already outside the contract: a root an older writer left.
        {
            let mut txn = doc.transact_mut();
            let t = txn.get_or_insert_text("content");
            t.insert(&mut txn, 0, "# left behind");
        }
        assert!(!validate_doc(&c, &doc.transact()).is_empty());
        let keystroke = peer_update(&doc, |txn| {
            let frag = txn.get_or_insert_xml_fragment(c.fragment.as_str());
            let Some(yrs::XmlOut::Element(p)) = frag.get(txn, 0) else {
                panic!("the paragraph")
            };
            let Some(yrs::XmlOut::Text(t)) = p.get(txn, 0) else {
                panic!("its text")
            };
            t.insert(txn, 5, "!");
        });
        assert_eq!(check_update(&c, &doc, &keystroke).unwrap(), []);

        // An update that does add a problem answers for that one alone,
        // wherever its index puts the old one.
        let worse = peer_update(&doc, |txn| {
            let frag = txn.get_or_insert_xml_fragment(c.fragment.as_str());
            let h = frag.insert(txn, 0, XmlElementPrelim::empty("heading"));
            yrs::Xml::insert_attribute(&h, txn, "level", yrs::Any::from(9i64));
        });
        let problems = check_update(&c, &doc, &worse).unwrap();
        assert_eq!(messages(&problems).len(), 1, "{problems:?}");
        assert!(problems[0].message.contains("`level`"), "{problems:?}");
    }

    /// Delete the `item`th child of the list at the page's `list`th block.
    fn delete_item(txn: &mut yrs::TransactionMut, list: u32, item: u32) {
        let frag = txn.get_or_insert_xml_fragment("doc");
        let Some(yrs::XmlOut::Element(l)) = frag.get(txn, list) else {
            panic!("the list")
        };
        l.remove_range(txn, item, 1);
    }

    #[test]
    fn two_valid_edits_that_merge_into_an_empty_list_are_both_taken() {
        let c = Contract::load();
        let doc = crate::doc_from_nodes(tree(
            r#"<p>Groceries</p><ul data-type="taskList"><li data-type="taskItem"><p>milk</p></li><li data-type="taskItem"><p>eggs</p></li></ul>"#,
        ));
        // A laptop and a phone synced the page, then each deleted one item.
        let laptop = peer_update(&doc, |txn| delete_item(txn, 1, 0));
        let phone = peer_update(&doc, |txn| delete_item(txn, 1, 1));
        assert_eq!(check_update(&c, &doc, &laptop).unwrap(), []);
        use yrs::updates::decoder::Decode;
        doc.transact_mut()
            .apply_update(Update::decode_v1(&laptop).unwrap())
            .unwrap();
        // The merge empties the list; the phone's own edit was valid, and
        // it could only ever send it again.
        assert_eq!(check_update(&c, &doc, &phone).unwrap(), []);
        doc.transact_mut()
            .apply_update(Update::decode_v1(&phone).unwrap())
            .unwrap();
        // The document says so until the server repairs it.
        assert!(messages(&validate_doc(&c, &doc.transact()))
            .iter()
            .any(|m| m.contains("`taskList` holds `taskItem+`; got nothing")));
    }

    #[test]
    fn a_node_where_its_parent_cannot_hold_one_is_still_refused() {
        let c = Contract::load();
        let doc = crate::doc_from_nodes(tree("<p>x</p><ul><li><p>a</p></li></ul>"));
        // A paragraph straight inside a list, and a task item in a bullet
        // list: no merge of valid edits puts either there.
        let paragraph = peer_update(&doc, |txn| {
            let frag = txn.get_or_insert_xml_fragment("doc");
            let Some(yrs::XmlOut::Element(l)) = frag.get(txn, 1) else {
                panic!("the list")
            };
            l.push_back(txn, XmlElementPrelim::empty("paragraph"));
        });
        let task = peer_update(&doc, |txn| {
            let frag = txn.get_or_insert_xml_fragment("doc");
            let Some(yrs::XmlOut::Element(l)) = frag.get(txn, 1) else {
                panic!("the list")
            };
            l.push_back(txn, XmlElementPrelim::empty("taskItem"));
        });
        for (what, update) in [("paragraph", paragraph), ("task", task)] {
            let problems = check_update(&c, &doc, &update).unwrap();
            assert!(
                messages(&problems)
                    .iter()
                    .any(|m| m.contains("`bulletList` holds `listItem+`")),
                "{what}: {problems:?}"
            );
        }
    }

    #[test]
    fn a_block_id_used_twice_is_named_but_not_refused() {
        let c = Contract::load();
        let mut nodes = tree(r#"<p data-id="a1">x</p><p data-id="b2">y</p>"#);
        nodes[1].attrs.insert("id".into(), json!("a1"));
        let problems = validate(&c, &nodes, "doc");
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].message.contains("`a1`"), "{problems:?}");

        // Two devices that each moved the same block keep its id twice; the
        // server renews one (`repair`), and neither device is refused.
        let doc = crate::doc_from_nodes(tree(r#"<p data-id="a1">x</p><p data-id="b2">y</p>"#));
        let copy = peer_update(&doc, |txn| {
            let frag = txn.get_or_insert_xml_fragment("doc");
            let p = frag.push_back(txn, XmlElementPrelim::empty("paragraph"));
            yrs::Xml::insert_attribute(&p, txn, "id", yrs::Any::from("a1"));
        });
        assert_eq!(check_update(&c, &doc, &copy).unwrap(), []);
    }
}

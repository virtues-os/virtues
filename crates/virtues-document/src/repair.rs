//! Putting a merged page document back inside the contract.
//!
//! Two edits, each inside the contract on the device that made it, can merge
//! into a document that is not (see [`crate::validate`]): a laptop and a
//! phone each delete one of a list's two items, and the list holds none.
//! Yjs merges without asking the schema, and no client puts the shared
//! document right: the app's patch to Tiptap's binding draws such a node
//! filled to fit and writes nothing. The server takes such an update and
//! then repairs the shape in its own transaction, which it relays with the
//! update that called for it, as one, so no other client holds the merge:
//!
//! - a node whose children do not fit its content takes an empty paragraph
//!   first, when that makes them fit (an emptied list item, quote or table
//!   cell; an emptied page);
//! - otherwise a node left with no children is removed (an emptied list or
//!   table), and its parent is looked at again;
//! - a block id an earlier block already has is replaced with a new one;
//! - a run of text carrying two marks that exclude each other keeps the one
//!   the contract lists later: words one device suggested inserting inside
//!   a run another device suggested deleting stay a suggested insertion,
//!   and the deletion goes from those words alone.
//!
//! Anything else outside the contract is not a merge's, and is left for
//! [`crate::validate`] to name.

use crate::contract::Contract;
use crate::model::{new_id, MAX_DEPTH};
use crate::ydoc;
use std::collections::HashSet;
use yrs::types::text::YChange;
use yrs::types::Attrs;
use yrs::{Any, OffsetKind, Out, ReadTxn, Text, TransactionMut, Xml, XmlElementPrelim, XmlFragment, XmlTextRef};

/// Repair what a merge left outside the contract, in `txn`. Returns how many
/// changes it made; 0 writes nothing.
pub fn repair(c: &Contract, txn: &mut TransactionMut) -> usize {
    let Some(frag) = txn.get_xml_fragment(c.fragment.as_str()) else {
        return 0;
    };
    let mut changes = 0;
    shape(c, txn, &frag, &c.fragment, true, 1, &mut changes);
    renew_ids(c, txn, &frag, 1, &mut HashSet::new(), &mut changes);
    exclusive_marks(c, txn, &frag, 1, &mut changes);
    changes
}

/// Take, from every run of text under `el` carrying two marks that exclude
/// each other, the one the contract lists first. Of the marks a merge can
/// cross that way, the proposals, that keeps the insertion: the newer
/// suggestion, its words someone added inside another's deletion.
fn exclusive_marks<F: XmlFragment>(c: &Contract, txn: &mut TransactionMut, el: &F, depth: usize, changes: &mut usize) {
    if depth > MAX_DEPTH {
        return;
    }
    let children: Vec<Out> = ydoc::children(&*txn, el).map(|(_, out)| out).collect();
    for child in children {
        match child {
            Out::YXmlElement(child) => exclusive_marks(c, txn, &child, depth + 1, changes),
            Out::YXmlText(text) => exclusive_runs(c, txn, &text, changes),
            _ => {}
        }
    }
}

fn exclusive_runs(c: &Contract, txn: &mut TransactionMut, text: &XmlTextRef, changes: &mut usize) {
    let kind = txn.doc().offset_kind();
    let mut fixes: Vec<(u32, u32, String)> = vec![];
    let mut at = 0u32;
    for d in text.diff(&*txn, YChange::identity) {
        let len = match &d.insert {
            Out::Any(Any::String(s)) => match kind {
                OffsetKind::Bytes => s.len() as u32,
                OffsetKind::Utf16 => s.encode_utf16().count() as u32,
            },
            _ => 1,
        };
        if let Some(attrs) = &d.attributes {
            let mut marks: Vec<&str> = attrs.keys().map(|k| k.as_ref()).collect();
            marks.sort_by_key(|m| c.mark_rank(m));
            for (i, first) in marks.iter().enumerate() {
                if marks[i + 1..].iter().any(|later| later != first && c.marks_exclude(first, later)) {
                    fixes.push((at, len, first.to_string()));
                }
            }
        }
        at += len;
    }
    for (at, len, mark) in fixes {
        let mut off = Attrs::new();
        off.insert(mark.into(), Any::Null);
        text.format(txn, at, len, off);
        *changes += 1;
    }
}

/// Repair the children of `el`, a node of type `kind`, then `el` itself.
/// Returns whether `el` is left empty where it may not be, for its parent
/// to remove. `depth` is the level of `el`'s children, as in [`MAX_DEPTH`].
fn shape<F: XmlFragment>(
    c: &Contract,
    txn: &mut TransactionMut,
    el: &F,
    kind: &str,
    root: bool,
    depth: usize,
    changes: &mut usize,
) -> bool {
    let Some(spec) = c.node(kind) else {
        return false;
    };
    if spec.spec.content.is_none() || c.is_textblock(kind) || depth > MAX_DEPTH {
        return false;
    }
    let children: Vec<(u32, Out)> = ydoc::children(&*txn, el).collect();
    let mut emptied = vec![];
    for (i, child) in children {
        if let Out::YXmlElement(child) = child {
            let tag = child.tag().to_string();
            if shape(c, txn, &child, &tag, false, depth + 1, changes) {
                emptied.push(i);
            }
        }
    }
    for i in emptied.into_iter().rev() {
        el.remove_range(txn, i, 1);
        *changes += 1;
    }

    let mut kinds = vec![];
    for (_, child) in ydoc::children(&*txn, el) {
        match child {
            Out::YXmlElement(child) => kinds.push(child.tag().to_string()),
            // Content that is not a node is no merge's.
            _ => return false,
        }
    }
    let kinds: Vec<&str> = kinds.iter().map(String::as_str).collect();
    if c.content_matches(kind, &kinds) {
        return false;
    }
    let mergeable = kinds
        .iter()
        .all(|k| c.node(k).is_some() && c.may_contain(kind, k));
    if !mergeable {
        return false;
    }
    let mut with_para = vec!["paragraph"];
    with_para.extend(kinds.iter().copied());
    if c.content_matches(kind, &with_para) {
        let p = el.insert(txn, 0, XmlElementPrelim::empty("paragraph"));
        if c.node("paragraph").is_some_and(|p| p.spec.id) {
            p.insert_attribute(txn, c.id.attr.as_str(), Any::from(new_id()));
        }
        *changes += 1;
        return false;
    }
    kinds.is_empty() && !root
}

/// Give a new id to each block whose id an earlier block, in document
/// order, already has.
fn renew_ids<F: XmlFragment>(
    c: &Contract,
    txn: &mut TransactionMut,
    el: &F,
    depth: usize,
    seen: &mut HashSet<String>,
    changes: &mut usize,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let children: Vec<Out> = ydoc::children(&*txn, el).map(|(_, out)| out).collect();
    for child in children {
        let Out::YXmlElement(child) = child else {
            continue;
        };
        let tag = child.tag().to_string();
        if c.node(&tag).is_some_and(|n| n.spec.id) {
            let id = child
                .get_attribute(&*txn, c.id.attr.as_str())
                .map(|v| ydoc::out_to_value(&v));
            if let Some(serde_json::Value::String(id)) = id {
                if !id.is_empty() && !seen.insert(id) {
                    let fresh = loop {
                        let fresh = new_id();
                        if !seen.contains(&fresh) {
                            break fresh;
                        }
                    };
                    child.insert_attribute(txn, c.id.attr.as_str(), Any::from(fresh.as_str()));
                    seen.insert(fresh);
                    *changes += 1;
                }
            }
        }
        if !c.is_textblock(&tag) {
            renew_ids(c, txn, &child, depth + 1, seen, changes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::{parse, Mode};
    use crate::validate::{check_update, validate_doc};
    use crate::wire::decode_update;
    use yrs::{Doc, StateVector, Transact, WriteTxn, XmlOut};

    fn page(html: &str) -> Doc {
        let o = parse(&Contract::load(), html, "doc", Mode::Strict);
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        crate::doc_from_nodes(o.nodes)
    }

    /// A device that synced `doc`.
    fn device(doc: &Doc) -> Doc {
        let peer = ydoc::new_doc();
        let state = doc
            .transact()
            .encode_state_as_update_v1(&StateVector::default());
        peer.transact_mut()
            .apply_update(decode_update(&state).unwrap())
            .unwrap();
        peer
    }

    /// Make `edit` on `device`, and return the update it sends.
    fn edit(device: &Doc, edit: impl FnOnce(&mut TransactionMut)) -> Vec<u8> {
        let before = device.transact().state_vector();
        edit(&mut device.transact_mut());
        let update = device.transact().encode_diff_v1(&before);
        update
    }

    /// Remove the child at `path` (indexes from the page's fragment).
    fn remove_at(txn: &mut TransactionMut, path: &[u32]) {
        let frag = txn.get_or_insert_xml_fragment("doc");
        let (last, parents) = path.split_last().unwrap();
        if parents.is_empty() {
            frag.remove_range(txn, *last, 1);
            return;
        }
        let mut el = match frag.get(txn, parents[0]) {
            Some(XmlOut::Element(e)) => e,
            other => panic!("{other:?}"),
        };
        for &i in &parents[1..] {
            el = match el.get(txn, i) {
                Some(XmlOut::Element(e)) => e,
                other => panic!("{other:?}"),
            };
        }
        el.remove_range(txn, *last, 1);
    }

    fn apply(doc: &Doc, update: &[u8]) {
        doc.transact_mut()
            .apply_update(decode_update(update).unwrap())
            .unwrap();
    }

    fn html(doc: &Doc) -> String {
        let c = Contract::load();
        crate::render::html(
            &c,
            &ydoc::read_doc(&c, &doc.transact()),
            &crate::render::Options { ids: false },
        )
    }

    /// Two devices each make a deletion that is valid on its own; the server
    /// takes both, in either order, and repairs what their merge leaves.
    fn concurrent(src: &str, laptop: &[u32], phone: &[u32], repaired: &str) {
        let c = Contract::load();
        for laptop_first in [true, false] {
            let server = page(src);
            let a = device(&server);
            let b = device(&server);
            let from_a = edit(&a, |txn| remove_at(txn, laptop));
            let from_b = edit(&b, |txn| remove_at(txn, phone));
            assert_eq!(validate_doc(&c, &a.transact()), [], "valid on the laptop");
            assert_eq!(validate_doc(&c, &b.transact()), [], "valid on the phone");
            let (first, second) = if laptop_first {
                (&from_a, &from_b)
            } else {
                (&from_b, &from_a)
            };
            assert_eq!(check_update(&c, &server, first).unwrap(), []);
            apply(&server, first);
            assert_eq!(
                check_update(&c, &server, second).unwrap(),
                [],
                "the second device is not shut out"
            );
            apply(&server, second);
            assert_ne!(validate_doc(&c, &server.transact()), [], "{src}");

            let fix = {
                let mut txn = server.transact_mut();
                assert!(repair(&c, &mut txn) > 0);
                txn.encode_update_v1()
            };
            assert_eq!(validate_doc(&c, &server.transact()), [], "{src}");
            assert_eq!(html(&server), repaired, "{src}");
            // Nothing left to repair.
            assert_eq!(repair(&c, &mut server.transact_mut()), 0);

            // The second device's resync after reconnecting is taken too,
            // and every device ends where the server is.
            for d in [&a, &b] {
                let sv = server.transact().state_vector();
                let resync = d.transact().encode_state_as_update_v1(&sv);
                assert_eq!(check_update(&c, &server, &resync).unwrap(), []);
                apply(
                    d,
                    &server
                        .transact()
                        .encode_state_as_update_v1(&StateVector::default()),
                );
                apply(d, &fix);
                assert_eq!(html(d), repaired);
            }
        }
    }

    #[test]
    fn an_emptied_list_is_removed() {
        concurrent(
            "<p>Groceries</p><ul data-type=\"taskList\"><li data-type=\"taskItem\"><p>milk</p></li>\
             <li data-type=\"taskItem\"><p>eggs</p></li></ul>",
            &[1, 0],
            &[1, 1],
            "<p>Groceries</p>",
        );
        concurrent(
            "<p>a</p><ul><li><p>x</p></li><li><p>y</p></li></ul>",
            &[1, 0],
            &[1, 1],
            "<p>a</p>",
        );
    }

    #[test]
    fn an_emptied_page_takes_a_paragraph() {
        concurrent("<p>one</p><p>two</p>", &[0], &[1], "<p></p>");
    }

    #[test]
    fn an_emptied_table_is_removed_and_the_page_kept() {
        concurrent(
            "<table><tr><td><p>1</p></td></tr><tr><td><p>2</p></td></tr></table>",
            &[0, 0],
            &[0, 1],
            "<p></p>",
        );
    }

    #[test]
    fn an_emptied_quote_or_cell_takes_a_paragraph() {
        concurrent(
            "<blockquote><p>a</p><p>b</p></blockquote>",
            &[0, 0],
            &[0, 1],
            "<blockquote><p></p></blockquote>",
        );
        concurrent(
            "<table><tr><td><p>a</p><p>b</p></td></tr></table>",
            &[0, 0, 0, 0],
            &[0, 0, 0, 1],
            "<table><tbody><tr><td><p></p></td></tr></tbody></table>",
        );
    }

    #[test]
    fn a_list_item_left_without_its_first_paragraph_takes_one() {
        concurrent(
            "<ul><li><p>a</p><p>b</p><ul><li><p>c</p></li></ul></li></ul>",
            &[0, 0, 0],
            &[0, 0, 1],
            "<ul><li><p></p><ul><li><p>c</p></li></ul></li></ul>",
        );
    }

    #[test]
    fn a_block_id_used_twice_is_renewed_on_the_later_block() {
        let c = Contract::load();
        let doc = page("<p data-id=\"aaaa1111\">one</p><p data-id=\"bbbb2222\">two</p>");
        {
            let mut txn = doc.transact_mut();
            let frag = txn.get_or_insert_xml_fragment("doc");
            let Some(XmlOut::Element(p)) = frag.get(&txn, 1) else {
                panic!()
            };
            p.insert_attribute(&mut txn, "id", Any::from("aaaa1111"));
        }
        assert!(validate_doc(&c, &doc.transact())
            .iter()
            .any(|p| p.message.contains("already an earlier block's")));
        assert_eq!(repair(&c, &mut doc.transact_mut()), 1);
        assert_eq!(validate_doc(&c, &doc.transact()), []);
        let tree = ydoc::read_doc(&c, &doc.transact());
        assert_eq!(tree[0].id(), Some("aaaa1111"));
        assert_ne!(tree[1].id(), Some("aaaa1111"));
    }

    /// One device suggests deleting a sentence; another, offline, pastes a
    /// suggested insertion inside it. Each is inside the contract; merged,
    /// the inserted words carry both suggestions. The second is taken, and
    /// the words keep the insertion alone, in either order.
    #[test]
    fn an_insertion_suggested_inside_a_suggested_deletion_keeps_the_insertion() {
        use yrs::{Text, XmlTextRef};
        let c = Contract::load();
        let text_of = |txn: &mut TransactionMut| -> XmlTextRef {
            let frag = txn.get_or_insert_xml_fragment("doc");
            let Some(XmlOut::Element(p)) = frag.get(txn, 0) else { panic!("a paragraph") };
            let Some(XmlOut::Text(t)) = p.get(txn, 0) else { panic!("its text") };
            t
        };
        let proposal = |key: &str, id: &str| {
            let mut a = Attrs::new();
            let mut inner = std::collections::HashMap::new();
            inner.insert("proposal".to_string(), Any::from(id));
            a.insert(key.into(), Any::from(inner));
            a
        };
        for a_first in [true, false] {
            let server = page("<p>hello world</p>");
            let a = device(&server);
            let b = device(&server);
            let from_a = edit(&a, |txn| {
                let t = text_of(txn);
                t.format(txn, 0, 11, proposal("proposedDeletion", "p1"));
                t.insert_with_attributes(txn, 11, "Hi there", proposal("proposedInsertion", "p1"));
            });
            let from_b = edit(&b, |txn| {
                let t = text_of(txn);
                t.insert_with_attributes(txn, 6, "brave ", proposal("proposedInsertion", "p2"));
            });
            assert_eq!(validate_doc(&c, &a.transact()), []);
            assert_eq!(validate_doc(&c, &b.transact()), []);
            let (first, second) = if a_first { (&from_a, &from_b) } else { (&from_b, &from_a) };
            assert_eq!(check_update(&c, &server, first).unwrap(), []);
            apply(&server, first);
            assert_eq!(check_update(&c, &server, second).unwrap(), [], "the second device is not shut out");
            apply(&server, second);
            assert!(validate_doc(&c, &server.transact())
                .iter()
                .any(|p| p.message.contains("cannot be combined")));
            assert!(repair(&c, &mut server.transact_mut()) > 0);
            assert_eq!(validate_doc(&c, &server.transact()), []);
            assert_eq!(
                html(&server),
                "<p><virtues-del proposal=\"p1\">hello </virtues-del><virtues-ins proposal=\"p2\">brave </virtues-ins>\
                 <virtues-del proposal=\"p1\">world</virtues-del><virtues-ins proposal=\"p1\">Hi there</virtues-ins></p>"
            );
            assert_eq!(repair(&c, &mut server.transact_mut()), 0);
        }
    }

    #[test]
    fn what_no_merge_makes_is_left_alone() {
        let c = Contract::load();
        let doc = page("<p>x</p>");
        {
            let mut txn = doc.transact_mut();
            let frag = txn.get_or_insert_xml_fragment("doc");
            let Some(XmlOut::Element(p)) = frag.get(&txn, 0) else {
                panic!()
            };
            // A block inside a paragraph, which no editor writes.
            p.push_back(&mut txn, XmlElementPrelim::empty("blockquote"));
        }
        let before = validate_doc(&c, &doc.transact());
        assert!(!before.is_empty());
        assert_eq!(repair(&c, &mut doc.transact_mut()), 0);
        assert_eq!(validate_doc(&c, &doc.transact()), before);
    }
}

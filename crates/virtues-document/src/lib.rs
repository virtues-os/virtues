//! The page document contract, and the Rust that interprets it.
//!
//! A page is a Yjs XML tree edited in the browser with Tiptap. One file,
//! `contract.json`, names every node, mark and attribute a page may hold and
//! the HTML each reads from and writes to; the browser builds its schema from
//! the same file. This crate reads it and provides what the server does with
//! a page:
//!
//! - **ingest** HTML strictly ([`parse_html`]): anything outside the contract
//!   is refused with a message the model can act on, never silently dropped;
//! - **convert** our markdown dialect ([`parse_markdown`]) in migration mode,
//!   which unwraps what the contract cannot hold and notes each step;
//! - **write and read** the tree in a Yjs document in the shape Tiptap's
//!   binding uses ([`new_doc`], [`write_nodes`], [`read_doc`]);
//! - **render** canonical HTML, with or without block ids ([`to_html`]), and
//!   the markdown export the `content` column holds ([`to_markdown`]);
//! - **edit** by block id ([`apply_ops`]), merging a write against the tree
//!   the model read when it says which one that was;
//! - **validate** a tree or a whole document ([`validate`], [`validate_doc`],
//!   [`check_update`]) so an update outside the contract is refused before
//!   it is applied or relayed, and **repair** ([`repair()`]) the shapes a
//!   merge of valid edits can leave.
//!
//! The sync server stays in virtues-core; nothing here does I/O.
//!
//! # Limits
//!
//! Every walk over a tree recurses once per level, so a tree deeper than
//! [`MAX_DEPTH`] is refused wherever it comes in (HTML, markdown, a Yjs
//! update) before it is walked, and so is a value inside a Yjs update nested
//! past [`MAX_VALUE_DEPTH`] ([`decode_update`]), before yrs decodes it, as
//! is JSON content, which yrs cannot encode again once it holds it. One
//! call reads at most [`MAX_INPUT_BYTES`], and HTML whose parse would cost
//! far more than its size (formatting left open across blocks, a tag with
//! hundreds of attributes) is refused before it is parsed. Reading,
//! checking, editing and exporting a page cost time in proportion to its
//! size; a table's grid is checked up to [`table::MAX_TABLE_CELLS`] cells.
//!
//! # The contract version
//!
//! [`Contract::version`] is stamped into every document ([`stamp_version`])
//! because a client deletes from the shared document what its schema cannot
//! read. The sync server binds a client only at or above a document's
//! version (`virtues-core`, `server/yjs.rs`), and [`check_update`] refuses
//! an update that changes the stamp, which only the server writes. The
//! version is bumped only once documents written under it have shipped;
//! until then the current version is still being defined. After that, adding
//! a node type, a mark or an attribute requires a bump.

pub mod contract;
pub mod ingest;
pub mod migrate;
pub mod model;
pub mod ops;
pub mod render;
pub mod repair;
pub mod table;
pub mod validate;
pub mod wire;
pub mod ydoc;

use std::sync::OnceLock;
use yrs::{Doc, ReadTxn, Transact, TransactionMut, XmlFragment};

pub use contract::{Contract, CONTRACT_JSON};
pub use ingest::{Mode, Outcome, MAX_INPUT_BYTES, MAX_PROBLEMS};
pub use model::{Mark, Node, Problem, MAX_DEPTH};
pub use ops::{Applied, Op};
pub use wire::{decode_update, encode_state, MAX_VALUE_DEPTH};
pub use ydoc::{client_id, new_doc, stamp_version, stamped_version};

/// The compiled-in contract, parsed on first use.
pub fn contract() -> &'static Contract {
    static CONTRACT: OnceLock<Contract> = OnceLock::new();
    CONTRACT.get_or_init(Contract::load)
}

/// HTML as the content of a node of type `parent` (`"doc"` for a whole
/// page), refusing anything outside the contract. The model's write path.
pub fn parse_html(html: &str, parent: &str) -> Outcome {
    ingest::parse(contract(), html, parent, Mode::Strict)
}

/// Our markdown as a page tree, through migration mode. Notes say what
/// changed; the nodes carry no block ids yet ([`write_nodes`] gives them).
pub fn parse_markdown(md: &str) -> Outcome {
    migrate::from_markdown(contract(), md)
}

/// Give every block that carries an id one, unique within `nodes`: for a
/// new document's tree. Nodes going into a document that already holds
/// blocks get theirs from [`write_nodes`].
pub fn ensure_ids(nodes: &mut [Node]) {
    ops::ensure_ids(contract(), nodes);
}

/// Insert `nodes` into `parent` (the page's fragment, or an element in it)
/// at `index`, every block with an id no other block in the document has:
/// one a node carries is kept unless a block already has it. Returns the
/// nodes as written, ids filled.
pub fn write_nodes<F: XmlFragment>(
    txn: &mut TransactionMut,
    parent: &F,
    index: u32,
    nodes: &[Node],
) -> Vec<Node> {
    let c = contract();
    let mut nodes = nodes.to_vec();
    let mut taken = std::collections::HashSet::new();
    ops::collect_ids(&read_doc(&*txn), &mut taken);
    ops::assign_ids(c, &mut nodes, &mut taken, None);
    ydoc::write_nodes(c, txn, parent, index, &nodes);
    nodes
}

/// A new page document holding `nodes`, block ids filled in and the
/// contract version stamped.
pub fn doc_from_nodes(mut nodes: Vec<Node>) -> Doc {
    let c = contract();
    ops::ensure_ids(c, &mut nodes);
    let doc = new_doc();
    {
        let mut txn = doc.transact_mut();
        let frag = ydoc::fragment(c, &mut txn);
        ydoc::write_nodes(c, &mut txn, &frag, 0, &nodes);
        ydoc::stamp_version(c, &mut txn);
    }
    doc
}

/// The page's tree, every attribute filled and marks in contract order.
pub fn read_doc<T: ReadTxn>(txn: &T) -> Vec<Node> {
    ydoc::read_doc(contract(), txn)
}

/// Canonical HTML: byte-identical to the browser editor's `getHTML()`.
/// `ids` writes block ids (`data-id`): on for the model, off for publishing.
pub fn to_html(nodes: &[Node], ids: bool) -> String {
    render::html(contract(), nodes, &render::Options { ids })
}

/// The markdown export: what the `content` column, search and embeddings
/// read. A projection; it does not read back to the identical tree in every
/// case.
pub fn to_markdown(nodes: &[Node]) -> String {
    render::markdown(contract(), nodes)
}

/// Apply block-id edits in one transaction. All or nothing: a refused batch
/// writes nothing and every problem is returned. `base` is the tree the
/// model read, when it says; see [`ops::apply_in`].
pub fn apply_ops(doc: &Doc, base: Option<&[Node]>, ops: &[Op]) -> Result<Applied, Vec<Problem>> {
    ops::apply(contract(), doc, base, ops)
}

/// [`apply_ops`] inside the caller's transaction.
pub fn apply_ops_in(
    txn: &mut TransactionMut,
    base: Option<&[Node]>,
    ops: &[Op],
) -> Result<Applied, Vec<Problem>> {
    ops::apply_in(contract(), txn, base, ops)
}

/// Problems with `nodes` as a whole page. Empty means inside the contract.
pub fn validate(nodes: &[Node]) -> Vec<Problem> {
    let c = contract();
    validate::validate(c, nodes, &c.fragment)
}

/// Problems with a whole page document, including what its tree cannot
/// express (a stray root such as the old `content` text).
pub fn validate_doc<T: ReadTxn>(txn: &T) -> Vec<Problem> {
    validate::validate_doc(contract(), txn)
}

/// Problems `update` would bring into `doc` that refuse it, checked on a
/// copy: what the document would hold that it does not hold now, and that
/// no merge of edits inside the contract can make. See
/// [`validate::check_update`].
pub fn check_update(doc: &Doc, update: &[u8]) -> anyhow::Result<Vec<Problem>> {
    validate::check_update(contract(), doc, update)
}

/// Whether `update` changes `doc`'s `meta`, checked without the rest of the
/// contract: for a page still in markdown. See [`validate::changes_meta`].
pub fn changes_meta(doc: &Doc, update: &[u8]) -> anyhow::Result<bool> {
    validate::changes_meta(doc, update)
}

/// Repair, in `txn`, the shapes a merge of edits inside the contract can
/// leave outside it (an emptied list, a block id used twice). Returns how
/// many changes it made. See [`repair::repair`].
pub fn repair(txn: &mut TransactionMut) -> usize {
    repair::repair(contract(), txn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_public_path_round_trips() {
        let o = parse_html("<h4>Plan</h4><p>One <strong>two</strong></p>", "doc");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        let doc = doc_from_nodes(o.nodes);
        assert_eq!(stamped_version(&doc.transact()), Some(contract().version));
        let tree = read_doc(&doc.transact());
        assert!(tree.iter().all(|n| n.id().is_some()));
        assert_eq!(validate(&tree), []);
        assert_eq!(validate_doc(&doc.transact()), []);
        assert_eq!(
            to_html(&tree, false),
            "<h4>Plan</h4><p>One <strong>two</strong></p>"
        );
        assert_eq!(to_markdown(&tree), "#### Plan\n\nOne **two**\n");

        let id = tree[1].id().unwrap().to_string();
        let applied = apply_ops(
            &doc,
            Some(&tree),
            &[Op::Replace {
                id: id.clone(),
                html: "<p>One <em>three</em></p>".into(),
            }],
        )
        .unwrap();
        assert_eq!(applied.written, [id]);
        assert_eq!(
            to_markdown(&read_doc(&doc.transact())),
            "#### Plan\n\nOne *three*\n"
        );
    }

    #[test]
    fn markdown_converts_through_the_contract() {
        let o = parse_markdown("## Notes\n\n| a | b |\n| :-: | --- |\n| 1 | 2 |\n");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        assert_eq!(o.notes, []);
        assert_eq!(validate(&o.nodes), []);
    }
}

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
//!   which unwraps what the contract cannot hold and notes each step, and an
//!   article's ([`parse_article_markdown`]), whose footnotes, figures and
//!   veils are nodes and a mark of their own;
//! - **write and read** the tree in a Yjs document in the shape Tiptap's
//!   binding uses ([`new_doc`], [`write_nodes`], [`read_doc`]);
//! - **render** canonical HTML, with or without block ids ([`to_html`]), and
//!   the markdown export the `content` column holds ([`to_markdown`]);
//! - **edit** by block id ([`apply_ops`]), checked against what reads have
//!   shown the writer ([`Seen`], built from what each read put in front of
//!   it, a [`Shown`]), and turn one page into another block by block
//!   ([`diff_ops`]), or write a writer's markdown over it, leaving alone
//!   what the markdown shows as it stands ([`revise_ops`]);
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
//! size, which [`MAX_NODES`] bounds: a write that would take a page past it
//! (HTML, markdown, an edit, an editor's update) is refused. A table's grid
//! is checked up to [`table::MAX_TABLE_CELLS`] cells.
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
#[cfg(feature = "yjs")]
pub mod diff;
pub mod ingest;
pub mod migrate;
pub mod model;
#[cfg(feature = "yjs")]
pub mod ops;
pub mod render;
#[cfg(feature = "yjs")]
pub mod repair;
#[cfg(feature = "yjs")]
pub mod seen;
pub mod table;
pub mod validate;
#[cfg(feature = "yjs")]
pub mod wire;
#[cfg(feature = "yjs")]
pub mod ydoc;

use std::sync::OnceLock;
#[cfg(feature = "yjs")]
use yrs::{Doc, ReadTxn, Transact, TransactionMut, XmlFragment};

pub use contract::{Contract, CONTRACT_JSON};
pub use ingest::{Mode, Outcome, MAX_INPUT_BYTES, MAX_PROBLEMS};
pub use migrate::{media_kind, MediaKind};
pub use model::{node_count, Mark, Node, Problem, MAX_DEPTH, MAX_NODES};
#[cfg(feature = "yjs")]
pub use ops::{Applied, Checked, Op, Unseen, Why};
#[cfg(feature = "yjs")]
pub use seen::{node_hash, tree_hash, Form, Known, Part, Seen, Shown};
#[cfg(feature = "yjs")]
pub use wire::{decode_update, encode_state, MAX_VALUE_DEPTH};
#[cfg(feature = "yjs")]
pub use ydoc::{client_id, new_doc, stamp_version, stamp_version_as, stamped_version};

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
/// Suggestions in it stay suggestions: for the page owner's own history.
/// Markdown from anyone else goes through [`parse_written_markdown`].
pub fn parse_markdown(md: &str) -> Outcome {
    migrate::from_markdown(contract(), md)
}

/// A page its owner wrote in the markdown editor, as blocks: read as that
/// editor showed it ([`migrate::Dialect::Page`]), raw HTML as its text and
/// each line of a paragraph a line of its own, so moving the page to
/// blocks, or putting back a version from before, loses nothing its owner
/// saw. Suggestions in it stay suggestions, as [`parse_markdown`] keeps them.
pub fn parse_page_markdown(md: &str) -> Outcome {
    migrate::from_markdown_as(contract(), md, migrate::Dialect::Page)
}

/// Markdown a writer sends into a page (a model, an applet, a PDF's text, a
/// paste) as blocks: [`parse_markdown`], with every suggestion in it written
/// as accepted ([`accept_proposals`]) and a note saying so. Suggestions are
/// the page owner's to make, in the page, whether they come as CriticMarkup
/// or as the contract's own tags written raw.
pub fn parse_written_markdown(md: &str) -> Outcome {
    let mut o = parse_markdown(md);
    if accept_proposals(&mut o.nodes) {
        o.notes.push(Problem::new(
            "markdown",
            "wrote its suggestions as accepted: suggestions are made by the page's owner, in the page",
        ));
    }
    o
}

/// Markdown a person pasted into a page, as blocks: [`parse_written_markdown`]
/// reading it as a paste ([`migrate::Dialect::Paste`]). A raw tag that is no
/// HTML (`<Login/>`, `Vec<String>`) and CriticMarkup stay text, as written:
/// a paste of code or prose loses none of it.
pub fn parse_pasted_markdown(md: &str) -> Outcome {
    let mut o = migrate::from_markdown_as(contract(), md, migrate::Dialect::Paste);
    if accept_proposals(&mut o.nodes) {
        o.notes.push(Problem::new(
            "markdown",
            "wrote its suggestions as accepted: suggestions are made by the page's owner, in the page",
        ));
    }
    o
}

/// An article's markdown from its writer (a narrator, the wiki editor) as
/// blocks, read as an article ([`migrate::Dialect::Article`]): footnotes,
/// figures and veils become the article's own nodes. Its suggestions, if it
/// holds any, are written as accepted, as [`parse_written_markdown`] writes
/// a writer's. A writer's markdown over a whole page goes through
/// [`parse_draft`], which keeps the page's own suggestions.
pub fn parse_article_markdown(md: &str) -> Outcome {
    let mut o = parse_own_article_markdown(md);
    if accept_proposals(&mut o.nodes) {
        o.notes.push(Problem::new(
            "markdown",
            "wrote its suggestions as accepted: suggestions are made by the page's owner, in the page",
        ));
    }
    o
}

/// An article's own markdown as blocks (the page as it stands, a version
/// of it, its edition), read as an article, its suggestions kept as
/// suggestions: they are its owner's, still to accept or reject.
pub fn parse_own_article_markdown(md: &str) -> Outcome {
    migrate::from_markdown_as(contract(), md, migrate::Dialect::Article)
}

/// An article's markdown as its block page would export it: read as the
/// article's own ([`parse_own_article_markdown`], its suggestions kept) and
/// written back ([`to_markdown`]). What a writer records as its edition of
/// an article that is blocks, and what any text of the article is compared
/// as, so two spellings of one page (an escape, the order of its
/// definitions) compare equal, and a sentence under the owner's pending
/// suggestion reads as that suggestion, never as decided. The problems when
/// the markdown makes no page.
pub fn edition_of(md: &str) -> Result<String, Vec<Problem>> {
    let o = parse_own_article_markdown(md);
    if !o.errors.is_empty() {
        return Err(o.errors);
    }
    Ok(to_markdown(&o.nodes))
}

#[cfg(feature = "yjs")]
/// A writer's markdown over the whole page `current` (a narrator's draft,
/// the wiki editor's revision, a page a picture was painted into), as
/// blocks, read as `dialect` ([`migrate::Dialect::Article`] on an article,
/// [`migrate::Dialect::Written`] on any other page). The writer read the
/// page's export, so a block that shows one of the page's own as it stands
/// carries the page's suggestions back as CriticMarkup: those stay
/// suggestions, and [`revise_ops`] leaves the block alone. Every other
/// suggestion is the writer's, and is written as accepted, with a note
/// saying so ([`diff::keep_shown_suggestions`]).
pub fn parse_draft(current: &[Node], md: &str, dialect: migrate::Dialect) -> Outcome {
    let mut o = migrate::from_markdown_as(contract(), md, dialect);
    if o.errors.is_empty() && diff::keep_shown_suggestions(contract(), current, &mut o.nodes, dialect) {
        o.notes.push(Problem::new(
            "markdown",
            "wrote its suggestions as accepted: suggestions are made by the page's owner, in the page",
        ));
    }
    o
}

/// The most markdown the editor converts at once: a paste, a drop, the
/// inline writer's output. The editor converts on the page's own thread
/// (`crates/virtues-document-wasm`), so this bounds how long a paste holds
/// it; more goes in as the text it is.
pub const EDITOR_MAX_BYTES: usize = 256 << 10;

/// How the editor reads markdown it converts ([`convert_for_editor`]). The
/// browser's converter takes it as a number (`crates/virtues-document-wasm`),
/// which is each variant's discriminant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum EditorMode {
    /// A writer's markdown ([`parse_written_markdown`]), on any page.
    Written = 0,
    /// A person's paste ([`parse_pasted_markdown`]).
    Paste = 1,
    /// A writer's markdown into an article ([`parse_article_markdown`]): the
    /// inline writer on an article writes its footnotes, figures and veils.
    Article = 2,
}

impl EditorMode {
    /// The mode a number names, as the browser's converter passes it.
    pub fn from_number(n: u32) -> Option<Self> {
        match n {
            0 => Some(EditorMode::Written),
            1 => Some(EditorMode::Paste),
            2 => Some(EditorMode::Article),
            _ => None,
        }
    }
}

/// `true` is a paste and `false` a writer's markdown, as
/// [`convert_for_editor`] took them before an article had a mode of its own.
impl From<bool> for EditorMode {
    fn from(paste: bool) -> Self {
        if paste {
            EditorMode::Paste
        } else {
            EditorMode::Written
        }
    }
}

/// Markdown as the editor inserts it ([`convert_for_editor`]).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Converted {
    /// Canonical HTML of the blocks, without block ids: the editor gives
    /// each block its own as it inserts them.
    pub html: String,
    /// What the conversion changed on the way in.
    pub notes: Vec<Problem>,
}

/// Why the editor's markdown was not converted ([`convert_for_editor`]).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "refused", rename_all = "kebab-case")]
pub enum Refused {
    /// More than one conversion holds: past [`EDITOR_MAX_BYTES`], or more
    /// parts or nodes than one conversion makes
    /// ([`migrate::past_one_conversion`]). Less at a time converts.
    TooLarge,
    /// What the contract refuses in it, a tree nested too deep say.
    Problems { problems: Vec<Problem> },
}

/// Markdown as the blocks the editor inserts for it, as canonical HTML
/// without block ids. A person's paste ([`EditorMode::Paste`]) is read as
/// one ([`parse_pasted_markdown`]): a raw tag that is no HTML and
/// CriticMarkup stay in its text as written. Otherwise it is a writer's
/// ([`parse_written_markdown`], or [`parse_article_markdown`] into an
/// article): a suggestion in it comes in accepted, since suggestions are
/// made in the page, never brought into it.
///
/// The one converter: the server's writers convert through the same
/// functions, and the browser runs this one, compiled to WebAssembly.
pub fn convert_for_editor(markdown: &str, mode: impl Into<EditorMode>) -> Result<Converted, Refused> {
    if markdown.len() > EDITOR_MAX_BYTES {
        return Err(Refused::TooLarge);
    }
    let o = match mode.into() {
        EditorMode::Paste => parse_pasted_markdown(markdown),
        EditorMode::Written => parse_written_markdown(markdown),
        EditorMode::Article => parse_article_markdown(markdown),
    };
    if o.errors.iter().any(migrate::past_one_conversion) {
        return Err(Refused::TooLarge);
    }
    if !o.errors.is_empty() {
        return Err(Refused::Problems { problems: o.errors });
    }
    Ok(Converted {
        html: to_html(&o.nodes, false),
        notes: o.notes,
    })
}

/// [`convert_for_editor`]'s answer as the editor reads it, JSON:
/// `{"html", "notes"}`, `{"refused": "too-large"}` or
/// `{"refused": "problems", "problems": […]}`. The browser's WebAssembly
/// build and `document-conformance convert` both answer with this, so the
/// two are compared byte for byte.
pub fn convert_for_editor_json(markdown: &str, mode: impl Into<EditorMode>) -> String {
    let answer = match convert_for_editor(markdown, mode) {
        Ok(converted) => serde_json::to_string(&converted),
        Err(refused) => serde_json::to_string(&refused),
    };
    answer.expect("a conversion's answer is plain data")
}

/// Every suggestion in `nodes` decided as accepted: text a suggestion takes
/// out goes, text it puts in stays, unmarked. Returns whether there was any.
pub fn accept_proposals(nodes: &mut Vec<Node>) -> bool {
    let deletes = |n: &Node| n.marks.iter().any(|m| m.kind == "proposedDeletion");
    let before = nodes.len();
    nodes.retain(|n| !deletes(n));
    let mut found = nodes.len() != before;
    for n in nodes.iter_mut() {
        n.walk_mut(&mut |n| {
            let before = n.content.len();
            n.content.retain(|c| !deletes(c));
            found |= n.content.len() != before;
            let before = n.marks.len();
            n.marks.retain(|m| m.kind != "proposedInsertion");
            found |= n.marks.len() != before;
        });
    }
    found
}

#[cfg(feature = "yjs")]
/// Put a document's tree back inside the contract when it holds what the
/// contract refuses ([`validate::refused_in_tree`]): content no update
/// inside the contract makes, which reached it past [`check_update`]. Each
/// top-level block holding any of it is read again, as HTML, in migration
/// mode ([`ingest::parse_own_blocks`]), which unwraps or drops what the
/// contract cannot hold, keeping text, and is written in its place, its ids
/// kept; a value or a fragment where a block belongs goes. Every other block
/// is left as it is, its Yjs items with it, so an edit a device made to it
/// against the saved state still merges. A block is read whatever its size:
/// the tree is the server's own, not a write.
///
/// `Ok(None)` for a tree inside the contract, which it leaves as it is;
/// `Ok(Some(refused))` when it put the tree back; `Err(refused)` when the
/// tree still holds what the contract refuses afterwards, which is not to be
/// served, and the transaction is not to be kept.
pub fn put_back_in_contract(txn: &mut TransactionMut) -> Result<Option<Vec<Problem>>, Vec<Problem>> {
    let c = contract();
    let refused = validate::refused_in_tree(c, &*txn);
    if refused.is_empty() {
        return Ok(None);
    }
    let frag = ydoc::fragment(c, txn);
    let kids: Vec<(u32, yrs::Out)> = ydoc::children(&*txn, &frag).collect();
    // From the end, so a block written in place never moves one still to
    // be reached.
    for (i, child) in kids.into_iter().rev() {
        if !matches!(child, yrs::Out::YXmlElement(_) | yrs::Out::YXmlText(_)) {
            frag.remove_range(txn, i, 1);
            continue;
        }
        let mut read = vec![];
        let nodes = ydoc::read_child(&*txn, i, &child, &c.fragment, 1, &mut read);
        if validate::refused_in_blocks(c, nodes.clone(), read).is_empty() {
            continue;
        }
        let o = ingest::parse_own_blocks(c, &to_html(&nodes, true));
        if !o.errors.is_empty() {
            return Err(o.errors);
        }
        let mut blocks = o.nodes;
        ensure_ids(&mut blocks);
        frag.remove_range(txn, i, 1);
        ydoc::write_nodes(c, txn, &frag, i, &blocks);
    }
    // A page holds a block at least.
    if frag.len(txn) == 0 {
        let paragraph = Node::element("paragraph", Default::default(), vec![]);
        let mut blocks = vec![paragraph];
        ensure_ids(&mut blocks);
        ydoc::write_nodes(c, txn, &frag, 0, &blocks);
    }
    let left = validate::refused_in_tree(c, &*txn);
    if !left.is_empty() {
        return Err(left);
    }
    Ok(Some(refused))
}

#[cfg(feature = "yjs")]
/// Give every block that carries an id one, unique within `nodes`: for a
/// new document's tree. Nodes going into a document that already holds
/// blocks get theirs from [`write_nodes`].
pub fn ensure_ids(nodes: &mut [Node]) {
    ops::ensure_ids(contract(), nodes);
}

#[cfg(feature = "yjs")]
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

#[cfg(feature = "yjs")]
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

#[cfg(feature = "yjs")]
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

/// The export with `<!-- {id} -->` on its own line above each top-level
/// block, joined as [`to_markdown`] joins them: the model's read of a page
/// too long to read as HTML. Removing those lines leaves [`to_markdown`].
pub fn to_markdown_with_ids(nodes: &[Node]) -> String {
    render::markdown_with_ids(contract(), nodes)
}

#[cfg(feature = "yjs")]
/// Ops that turn the page `current` into `target`, top-level blocks only:
/// inserts and replaces first, each in target order, deletes last, so the
/// page holds a block at every step. A block that does not change is not
/// touched. See [`diff::diff_ops`].
pub fn diff_ops(current: &[Node], target: &[Node]) -> Vec<Op> {
    diff::diff_ops(contract(), current, target)
}

#[cfg(feature = "yjs")]
/// Ops that write a writer's markdown, converted as `dialect` reads it
/// ([`parse_draft`]: `target`, blocks without ids), over the page
/// `current`: a block the markdown shows as it stands is left alone,
/// whatever it holds that markdown cannot show, and so is one the export
/// shows as nothing, unless the write keeps no block of the page; a block
/// the writer changed a little is written in place, keeping its id. See [`diff::revise_ops`]; a write is refused when
/// [`suggestions_taken`] names a block.
pub fn revise_ops(current: &[Node], target: &[Node], dialect: migrate::Dialect) -> Vec<Op> {
    diff::revise_ops(contract(), current, target, dialect)
}

#[cfg(feature = "yjs")]
/// The blocks of `current` holding a suggestion its owner has not decided
/// that `ops` would replace or delete, by id: what a writer may not touch.
/// See [`diff::suggestions_taken`].
pub fn suggestions_taken(current: &[Node], ops: &[Op]) -> Vec<String> {
    diff::suggestions_taken(current, ops)
}

/// The tags a writer may use, generated from the contract: one line each for
/// blocks, inline nodes and marks, with match attributes and small enums.
/// Proposals are left out: they are the page owner's to make.
pub fn tag_guide() -> String {
    contract().tag_guide()
}

#[cfg(feature = "yjs")]
/// Apply block-id edits in one transaction. All or nothing: a refused batch
/// writes nothing and every problem is returned. `base` is what reads have
/// shown the writer, when it says; see [`ops::apply_in`].
pub fn apply_ops(doc: &Doc, base: Option<&Seen>, ops: &[Op]) -> Result<Applied, Vec<Problem>> {
    ops::apply(contract(), doc, base, ops)
}

#[cfg(feature = "yjs")]
/// [`apply_ops`] inside the caller's transaction.
pub fn apply_ops_in(
    txn: &mut TransactionMut,
    base: Option<&Seen>,
    ops: &[Op],
) -> Result<Applied, Vec<Problem>> {
    ops::apply_in(contract(), txn, base, ops)
}

#[cfg(feature = "yjs")]
/// What [`apply_ops`] refuses of a batch over the page `tree` with `base`,
/// checked on a copy: the problems, and the blocks behind each one about
/// what the base has not seen as it stands. See [`ops::check`].
pub fn check_ops(tree: &[Node], base: &Seen, ops: &[Op]) -> Checked {
    ops::check(contract(), tree, base, ops)
}

#[cfg(feature = "yjs")]
/// [`apply_ops_in`] for the ops [`diff_ops`] makes from a tree of the
/// server's own (a version put back), which no write's size limit bounds;
/// see [`ops::apply_own_in`].
pub fn apply_own_ops_in(txn: &mut TransactionMut, ops: &[Op]) -> Result<Applied, Vec<Problem>> {
    ops::apply_own_in(contract(), txn, ops)
}

/// Problems with `nodes` as a whole page. Empty means inside the contract.
pub fn validate(nodes: &[Node]) -> Vec<Problem> {
    let c = contract();
    validate::validate(c, nodes, &c.fragment)
}

#[cfg(feature = "yjs")]
/// Problems with a whole page document, including what its tree cannot
/// express (a stray root such as the old `content` text).
pub fn validate_doc<T: ReadTxn>(txn: &T) -> Vec<Problem> {
    validate::validate_doc(contract(), txn)
}

#[cfg(feature = "yjs")]
/// Problems `update` would bring into `doc` that refuse it, checked on a
/// copy: what the document would hold that it does not hold now, and that
/// no merge of edits inside the contract can make. See
/// [`validate::check_update`].
pub fn check_update(doc: &Doc, update: &[u8]) -> anyhow::Result<Vec<Problem>> {
    validate::check_update(contract(), doc, update)
}

#[cfg(feature = "yjs")]
/// [`check_update`] against a document's full state (`encode_state`), which
/// owns nothing of the document: for a check run off the thread that holds
/// it. See [`validate::check_update_on_state`].
pub fn check_update_on_state(state: &[u8], update: &[u8]) -> anyhow::Result<Vec<Problem>> {
    validate::check_update_on_state(contract(), state, update)
}

#[cfg(feature = "yjs")]
/// [`check_update_on_state`] as a page's socket acts on it: an update that
/// only waits on items the page lacks is neither taken nor refused. See
/// [`validate::admit_update_on_state`].
pub fn admit_update_on_state(state: &[u8], update: &[u8]) -> anyhow::Result<validate::Admission> {
    validate::admit_update_on_state(contract(), state, update)
}

#[cfg(feature = "yjs")]
/// Whether `update` changes `doc`'s `meta`, checked without the rest of the
/// contract: for a page still in markdown. See [`validate::changes_meta`].
pub fn changes_meta(doc: &Doc, update: &[u8]) -> anyhow::Result<bool> {
    validate::changes_meta(doc, update)
}

#[cfg(feature = "yjs")]
/// Repair, in `txn`, the shapes a merge of edits inside the contract can
/// leave outside it (an emptied list, a block id used twice). Returns how
/// many changes it made. See [`repair::repair`].
pub fn repair(txn: &mut TransactionMut) -> usize {
    repair::repair(contract(), txn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "yjs")]
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
            Some(&Seen::of_page(&tree)),
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

    /// Markdown writes media inside a paragraph; the converter lifts it out
    /// into a block of its own. That is a change worth a note only when the
    /// paragraph held text as well: a media line on its own was moved out
    /// of nothing.
    #[test]
    fn a_media_line_is_noted_as_moved_only_from_text_around_it() {
        let moved = |o: &Outcome| {
            o.notes
                .iter()
                .filter(|n| n.message.contains("out of the surrounding text"))
                .count()
        };
        let alone = parse_markdown(
            "![Harbour](https://images.example.com/photo-1)\n\n\
             ![voice.m4a](/drive/df_3)\n\n\
             ![](https://example.com/report.pdf)\n",
        );
        assert!(alone.errors.is_empty(), "{:?}", alone.errors);
        let kinds: Vec<&str> = alone.nodes.iter().map(|n| n.kind.as_str()).collect();
        assert_eq!(kinds, ["image", "audio", "file"]);
        assert_eq!(moved(&alone), 0, "{:?}", alone.notes);

        let amid = parse_markdown("Lunch at the harbour ![Harbour](https://images.example.com/photo-1) on Friday.\n");
        assert!(amid.errors.is_empty(), "{:?}", amid.errors);
        let kinds: Vec<&str> = amid.nodes.iter().map(|n| n.kind.as_str()).collect();
        assert_eq!(kinds, ["paragraph", "image", "paragraph"]);
        assert_eq!(moved(&amid), 1, "{:?}", amid.notes);
    }

    /// A paste is text a person copied. A raw tag that is no HTML element
    /// (`<Login/>`, `<Enter>`, generics) and CriticMarkup are text in it,
    /// kept as written; HTML that is markup is still read as markup.
    #[test]
    fn pasted_markdown_keeps_what_reads_as_text() {
        let text = |o: &Outcome| {
            let mut s = String::new();
            for n in &o.nodes {
                n.walk(&mut |n| {
                    if let Some(t) = &n.text {
                        s.push_str(t);
                    } else if n.kind == "hardBreak" {
                        s.push('\n');
                    }
                });
                s.push('|');
            }
            s
        };
        for (md, kept) in [
            ("- if (user == null) return <Login/>;\n", "if (user == null) return <Login/>;|"),
            ("- Press <Enter> to confirm, then check that a == b\n", "Press <Enter> to confirm, then check that a == b|"),
            ("- remember <placeholder> here\n", "remember <placeholder> here|"),
            ("- a Vec<String> == other, and `Vec<T>`\n", "a Vec<String> == other, and Vec<T>|"),
            ("- Draft {--old--} == fine\n", "Draft {--old--} == fine|"),
            ("- Pick an Option<Label>\n", "Pick an Option<Label>|"),
            ("<Login/>\n<Logout/>\n\n- x\n", "<Login/>\n<Logout/>|x|"),
            ("- Press <kbd>Enter</kbd>\n", "Press Enter|"),
        ] {
            let o = parse_pasted_markdown(md);
            assert!(o.errors.is_empty(), "{md}: {:?}", o.errors);
            assert_eq!(text(&o), kept, "{md}");
            let mut held = false;
            for n in &o.nodes {
                n.walk(&mut |n| held |= n.marks.iter().any(|m| contract::OWNER_MARKS.contains(&m.kind.as_str())));
            }
            assert!(!held, "a paste makes no suggestion: {md}");
        }
        // Read as written, the same markdown loses the words a tag stood for.
        assert_eq!(text(&parse_written_markdown("- remember <placeholder> here\n")), "remember here|");
    }

    /// Version 1 shipped (staging.97, mac 1.0.30), and a client deletes
    /// from the shared document what its schema cannot read: the article's
    /// footnote, figure and veil are version 2.
    #[test]
    fn the_contract_is_version_two() {
        assert_eq!(contract().version, 2);
    }

    #[test]
    fn the_tag_guide_names_what_a_writer_may_use() {
        assert_eq!(
            tag_guide(),
            "Blocks: <p> <h1>…<h6> <blockquote> <aside data-tone=\"note|tip|warning\"> <ul> <ol start> <li value> \
             <ul data-type=\"taskList\"> <li data-type=\"taskItem\" data-checked=\"true|false\"> <pre data-language> \
             <hr> <img src alt width> <audio src data-name></audio> <video src data-name></video> \
             <virtues-file src data-name></virtues-file> <table> <tr> <th colspan rowspan align=\"left|center|right\"> \
             <td colspan rowspan align=\"left|center|right\"> <virtues-applet ref height></virtues-applet> \
             <virtues-figure kind=\"quote|thread|route|picture\" text who ref refs src caption data-style time></virtues-figure>\n\
             Inline: <br> <virtues-mention to label></virtues-mention> <virtues-footnote label note></virtues-footnote>\n\
             Marks: <a href> <strong> <em> <u> <s> <mark> <virtues-veil> <code>"
        );
        // Every tag in it is one the model's write path takes.
        let o = parse_html(
            "<p><a href=\"/x\"><strong>a</strong></a><em>b</em><u>c</u><s>d</s><mark>e</mark><code>f</code><br>\
             <virtues-mention to=\"/person/person_1\" label=\"Nick\"></virtues-mention></p>\
             <audio src=\"/drive/df_3\" data-name=\"memo.m4a\"></audio><video src=\"/drive/df_2\"></video>\
             <virtues-file src=\"/drive/df_1\" data-name=\"report.pdf\"></virtues-file>\
             <p>Lunch<virtues-footnote label=\"ev-1\" note=\"Message · 12:10 PM\"></virtues-footnote> at <virtues-veil>the Blue Kettle</virtues-veil></p>\
             <virtues-figure kind=\"picture\" src=\"/drive/df_4\" caption=\"Rain\" data-style=\"watercolor\" time=\"9:00 AM\"></virtues-figure>",
            "doc",
        );
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        assert!(!tag_guide().contains("virtues-ins") && !tag_guide().contains("virtues-del"));
    }

    /// A suggestion is the page owner's to make: markdown a writer sends,
    /// raw tags in any case or CriticMarkup, comes in accepted.
    #[test]
    fn written_markdown_holds_no_suggestion() {
        for (md, accepted) in [
            (
                "Dinner on <virtues-del proposal=\"p1\">Friday</virtues-del><virtues-ins proposal=\"p1\">Saturday</virtues-ins>.\n",
                "Dinner on Saturday.\n",
            ),
            (
                "<VIRTUES-INS proposal=\"x\">Shown on the shared page</VIRTUES-INS> too.\n",
                "Shown on the shared page too.\n",
            ),
            ("- The {--quick--}{++slow++} fox\n", "- The slow fox\n"),
        ] {
            let suggested = parse_markdown(md);
            assert!(suggested.errors.is_empty(), "{:?}", suggested.errors);
            let mut held = false;
            for n in &suggested.nodes {
                n.walk(&mut |n| held |= n.marks.iter().any(|m| contract::OWNER_MARKS.contains(&m.kind.as_str())));
            }
            assert!(held, "the owner's own history keeps its suggestions: {md}");

            let o = parse_written_markdown(md);
            assert!(o.errors.is_empty(), "{:?}", o.errors);
            assert_eq!(to_markdown(&o.nodes), accepted);
            assert!(!to_html(&o.nodes, false).contains("virtues-"), "{md}");
            assert!(o.notes.iter().any(|n| n.message.contains("as accepted")), "{:?}", o.notes);
        }
        let plain = parse_written_markdown("Lunch with Nick.\n");
        assert_eq!(plain.notes, []);
    }

    /// The editor's conversion: canonical HTML without block ids, a paste
    /// read as one, a writer's suggestions accepted, and what it refuses
    /// named as too large (less at a time converts) or by its problems.
    #[test]
    fn the_editor_converts_markdown_to_blocks_without_ids() {
        let converted = convert_for_editor(
            "---\ntitle: Trip\n---\n\n## Plan\n\n- [ ] Book seats\n\nLunch with [@Nick](/person/person_1).\n",
            false,
        )
        .unwrap();
        assert_eq!(
            converted.html,
            "<h2>Plan</h2><ul data-type=\"taskList\"><li data-type=\"taskItem\"><p>Book seats</p></li></ul>\
             <p>Lunch with <virtues-mention to=\"/person/person_1\" label=\"Nick\"></virtues-mention>.</p>"
        );
        assert!(converted.notes.iter().any(|n| n.message.contains("front matter")), "{:?}", converted.notes);
        assert!(!converted.html.contains("data-id"));

        let written = convert_for_editor("The {--quick--}{++slow++} fox.\n", false).unwrap();
        assert_eq!(written.html, "<p>The slow fox.</p>");
        assert!(written.notes.iter().any(|n| n.message.contains("as accepted")));
        let pasted = convert_for_editor("- Draft {--old--} with <Login/>\n", true).unwrap();
        assert_eq!(pasted.html, "<ul><li><p>Draft {--old--} with &lt;Login/&gt;</p></li></ul>");

        // Markdown that only looks like front matter converts, and returns.
        for md in ["Notes:\n\n ---\nLunch with **Nick** on Friday.\n---\n", " ---\nx\n---"] {
            assert!(convert_for_editor(md, true).is_ok(), "{md:?}");
        }
        // A paste opening with `---`, a line and `---` keeps its words, as a
        // rule and a heading: a paste is what the person copied, and the
        // lines left out as front matter vanished from the page unsaid.
        for (md, kept) in [
            ("---\nMeeting notes from Tuesday\nAttendees: Nick, David Okafor\n---\n## Agenda\n- one\n- two", "Attendees: Nick, David Okafor"),
            ("---\ntitle: Trip plan\ntags: [travel]\n---\n\n# Trip\n\n- Book seats", "tags: [travel]"),
        ] {
            let pasted = convert_for_editor(md, true).unwrap();
            assert!(pasted.html.starts_with("<hr>"), "{md:?}: {}", pasted.html);
            assert!(pasted.html.contains(kept), "{md:?}: {}", pasted.html);
            assert!(!pasted.notes.iter().any(|n| n.message.contains("front matter")), "{:?}", pasted.notes);
        }

        let deep = format!("{}Deep.\n", "> ".repeat(150));
        assert!(matches!(convert_for_editor(&deep, true), Err(Refused::Problems { .. })));
        let too_large = "a".repeat(EDITOR_MAX_BYTES + 1);
        assert_eq!(convert_for_editor(&too_large, true), Err(Refused::TooLarge));
        // Under the size limit, but more nodes than a page holds.
        let rows = "| a | b |\n| - | - |\n".to_string() + &"| x | y |\n".repeat(20_000);
        assert!(rows.len() <= EDITOR_MAX_BYTES);
        assert_eq!(convert_for_editor(&rows, true), Err(Refused::TooLarge));
    }

    /// What the browser's converter answers with, as JSON
    /// (`crates/virtues-document-wasm`).
    #[test]
    fn a_refusal_reads_as_its_kind() {
        assert_eq!(serde_json::to_value(Refused::TooLarge).unwrap(), serde_json::json!({ "refused": "too-large" }));
        assert_eq!(
            serde_json::to_value(Refused::Problems { problems: vec![Problem::new("doc", "x")] }).unwrap(),
            serde_json::json!({ "refused": "problems", "problems": [{ "at": "doc", "message": "x" }] })
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

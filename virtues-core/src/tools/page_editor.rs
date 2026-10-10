//! Page editor tool implementation
//!
//! Three tools for pages:
//! - create_page: create a page from markdown
//! - get_page_content: read a page before editing it
//! - edit_page: edit it
//!
//! A page is one of two formats (`pages::PageFormat`), and each is read and
//! edited its own way:
//!
//! - **A block page** (`tree`) is read as canonical HTML with a `data-id` on
//!   every block, or, when long, as its markdown export with each top-level
//!   block's id in a comment above it. Every read records what the model was
//!   shown as a read base (`api::page_reads`) and returns its name. An edit
//!   is a batch of block-id ops naming that base, applied all or none
//!   (`YjsState::edit_tree`): a block someone changed since the read is
//!   merged with the edit or refused, never overwritten unseen. A refused
//!   batch comes back as a result the model acts on, not an error.
//! - **A markdown page** is read as its text and edited by find/replace on
//!   it, through the live Y.Text.
//!
//! Each edit keeps a restore point of the page before it (when no version
//! already holds it) and cuts a version credited to 'ai' after it, so the
//! user can undo it from version history. Typing that lands between the
//! restore point and the edit is versioned as the owner's first, so History
//! credits the chat with its own change alone.

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, OnceLock};
use virtues_document::{Node, Op, Problem};

// Static regexes for stripping CriticMarkup (legacy safety fallback — strips markers if LLM still outputs them)
static ADDITION_RE: OnceLock<Regex> = OnceLock::new();
static DELETION_RE: OnceLock<Regex> = OnceLock::new();

use super::executor::{ToolContext, ToolError, ToolResult};
use crate::api::page_reads;
use crate::api::pages::{self, PageFormat};
use crate::ids;
use crate::server::yjs::{TextWriteError, TreeError, YjsState};

/// The description on the owner's typing when it is versioned just before a
/// chat edit lands on it: an ordinary autosave (`'auto'`), so History reads
/// it as theirs. Not a restore point (`pages::RESTORE_POINT`), and not the
/// label chat edits once put on the page before them (`wiki_articles`'
/// `BEFORE_CHAT_EDIT`), which History reads as the chat's.
const TYPED_BEFORE_CHAT_EDIT: &str = "Auto-saved (before a chat edit)";

/// The longest a block page's canonical HTML is shown whole. A longer page
/// is shown as its markdown export, which carries the same text in about a
/// third of the characters, and the model reads the blocks it edits as HTML
/// by id.
const READ_HTML_CHARS: usize = 24_000;

// The most ops one edit takes, the crate's, whose grid refusal names it. A
// batch is applied in one transaction under the page's lock, which open
// editors wait on.
use virtues_document::ops::MAX_OPS;

/// How much of a long page one read shows, as the bytes its markdown takes
/// in the result: whole top-level blocks only, and below the agent loop's
/// cut of a tool's output (`MAX_TOOL_OUTPUT_BYTES`, 96 KiB) with room for
/// the rest of the result, so the base names only blocks the model saw. The
/// rest of the page is read on with `after`; an `ids` read takes the same
/// limit.
const READ_MARKDOWN_BYTES: usize = 64 * 1024;

/// Whether a read with ids shows `node` whole: its HTML, with ids, fits in
/// [`READ_MARKDOWN_BYTES`] as the result counts it. A block that does not
/// is too long for any read (unless it opens, `page_reads::opens`).
pub(crate) fn fits_one_read(node: &Node) -> bool {
    json_bytes(&virtues_document::to_html(std::slice::from_ref(node), true)) <= READ_MARKDOWN_BYTES
}

/// How much of the written blocks' HTML an edit's result shows, whole blocks
/// only. A block shown here is one the model has seen as it now stands, so
/// it enters the next base; one too long to show does not, and the result
/// names it, to be read by id before it is replaced.
const WRITTEN_HTML_CHARS: usize = 8_000;

/// How much markdown each side of an edit's `find`/`replace` carries: they
/// are for the chat's diff card, not for the model.
const EDIT_TEXT_CHARS: usize = 2_000;

/// What a full read of a block page tells the model about editing it. Here
/// rather than in the tool's definition, which is re-sent on every step.
const HOW_TO_EDIT: &str = "Edit with edit_page: page_id, base, ops. replace swaps the block with \
     that data-id for your html, which keeps its id; insert_after and insert_before add html \
     beside it; delete removes it; append adds html at the end. html is whole blocks; leave \
     data-id off blocks you add. If any op is refused nothing is written, and the reply says why.";

/// Beside a long page's markdown.
const LONG_PAGE_NOTE: &str = "Long page, shown as markdown. To edit a block exactly, read it as \
     html: get_page_content with ids and this base.";

/// An `ids` or `after` read without a base it can name.
const IDS_NEED_BASE: &str = "ids and after need the base from your last read of this page; \
     read it again without them";

const EDITED_BY_BLOCK: &str =
    "This page is edited by block: read it with get_page_content, then send base and ops.";

const EDITED_AS_TEXT: &str = "This page is edited as text: send find and replace.";

const NOTHING_TO_CHANGE: &str = "Nothing to change: send ops with their base for a block page, \
     find and replace for a text page, or a title to rename it.";

const NEEDS_LIVE_DOCUMENT: &str =
    "Editing a block page needs the live document; this one runs only on the server.";

// ============================================================================
// Argument structs for each tool
// ============================================================================

/// Arguments for create_page tool
#[derive(Debug, Deserialize)]
pub struct CreatePageArgs {
    /// Page title (required)
    pub title: String,
    /// Initial content (optional)
    #[serde(default)]
    pub content: Option<String>,
}

/// Arguments for get_page_content tool
#[derive(Debug, Deserialize)]
pub struct GetPageContentArgs {
    /// Page ID (optional - uses context if not provided)
    #[serde(default)]
    pub page_id: Option<String>,
    /// Block ids to read as HTML: a list, or a string holding one (models
    /// stringify nested arrays). Needs `base`.
    #[serde(default)]
    pub ids: Option<Value>,
    /// A long page: the block the read starts after, from the last read's
    /// `more_after`. Needs `base`.
    #[serde(default)]
    pub after: Option<String>,
    /// The base from the read the `ids` refresh or `after` reads on from.
    #[serde(default)]
    pub base: Option<String>,
    /// `"markdown"`: a block page's export, with no ids and no base. The
    /// CLI's plain read; not in the tool's schema.
    #[serde(default)]
    pub view: Option<String>,
}

/// Arguments for edit_page tool
#[derive(Debug, Deserialize)]
pub struct EditPageArgs {
    /// Page ID (optional - uses context if not provided)
    #[serde(default)]
    pub page_id: Option<String>,
    /// New title for the page (optional - only set to rename)
    #[serde(default)]
    pub title: Option<String>,
    /// A markdown page: the text to find (empty for a full replacement).
    #[serde(default)]
    pub find: Option<String>,
    /// A markdown page: the replacement text, markdown (CriticMarkup
    /// markers stripped as a safety fallback).
    #[serde(default)]
    pub replace: Option<String>,
    /// A block page: the base the read returned.
    #[serde(default)]
    pub base: Option<String>,
    /// A block page: the ops, a list or a string holding one.
    #[serde(default)]
    pub ops: Option<Value>,
}

// ============================================================================
// Result structs
// ============================================================================

/// Result for edit_page - contains the edit info for frontend to track/display
#[derive(Debug, Serialize)]
pub struct EditResult {
    /// Unique ID for this edit
    pub edit_id: String,
    /// Page being edited
    pub page_id: String,
    /// The page's format: `markdown` or `tree`.
    pub format: &'static str,
    /// The markdown the edit replaced: the found text, or for a block page
    /// the export of the blocks it replaced and deleted (for the diff card).
    pub find: String,
    /// The markdown the edit wrote: the replacement, or for a block page the
    /// export of the blocks it wrote.
    pub replace: String,
    /// A block page: the ids of the blocks written, in document order, for
    /// the open editor to show.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocks: Option<Vec<String>>,
}

// ============================================================================
// Page Editor Tool
// ============================================================================

/// Page editor tool - handles create, read, and edit operations
#[derive(Clone)]
pub struct PageEditorTool {
    pool: Arc<PgPool>,
    /// Optional YjsState for real-time collaborative editing
    /// When present, edits are applied through Yjs for live sync
    yjs_state: Option<YjsState>,
}

impl PageEditorTool {
    pub fn new(pool: Arc<PgPool>, yjs_state: Option<YjsState>) -> Self {
        Self { pool, yjs_state }
    }

    /// Safety fallback: strip CriticMarkup markers if the LLM still outputs them.
    /// Keeps content inside addition markers, removes deletion markers entirely.
    fn strip_critic_markup(content: &str) -> String {
        // Use static regexes (compiled once on first use)
        let addition_re = ADDITION_RE
            .get_or_init(|| Regex::new(r"\{\+\+([\s\S]*?)\+\+\}").expect("valid addition regex"));
        let deletion_re = DELETION_RE
            .get_or_init(|| Regex::new(r"\{--([\s\S]*?)--\}").expect("valid deletion regex"));

        // Remove addition markers but keep content: {++text++} -> text
        let result = addition_re.replace_all(content, "$1");

        // Remove deletion markers and content: {--text--} -> ""
        let result = deletion_re.replace_all(&result, "");

        result.to_string()
    }

    /// Create a new page from markdown, in the server's default format.
    /// CriticMarkup markers are stripped first, and a block page's
    /// conversion takes the contract's suggestion tags in as accepted
    /// (`parse_written_markdown`): proposals are the owner's to make, in the
    /// page. A block page's conversion notes come back.
    pub async fn create_page(&self, arguments: serde_json::Value) -> Result<ToolResult, ToolError> {
        let args: CreatePageArgs = serde_json::from_value(arguments)
            .map_err(|e| ToolError::InvalidParameters(format!("Invalid arguments: {}", e)))?;

        // Strip any CriticMarkup markers the AI might have included
        let clean_content = args
            .content
            .map(|c| Self::strip_critic_markup(&c))
            .unwrap_or_default();

        let req = pages::CreatePageRequest {
            title: args.title.clone(),
            content: clean_content,
            icon: None,
            icon_color: None,
            cover_url: None,
            tags: None,
            project_id: None,
            format: None,
        };

        let created = pages::create_page_reporting(self.pool.as_ref(), req)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to create page: {}", e)))?;
        Ok(ToolResult::success(created_result(created)))
    }

    /// Read a page: what the model edits from.
    ///
    /// A markdown page returns its text, live from Yjs when it is available.
    /// A block page returns HTML with block ids, or the long form, and keeps
    /// what it showed as the base the next edit names. `ids` with `base`
    /// reads those blocks as HTML and keeps a base that is the earlier read
    /// with just those blocks brought up to date (`seen_tree`). Without Yjs
    /// (a CLI read in its own process) a block page is read from its saved
    /// state, which can trail the live doc by the save debounce; a base
    /// kept from it is still a tree the page held, and the merge handles
    /// what changed since.
    pub async fn get_page_content(
        &self,
        arguments: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let args: GetPageContentArgs = serde_json::from_value(arguments)
            .map_err(|e| ToolError::InvalidParameters(format!("Invalid arguments: {}", e)))?;

        // Get page_id from args or context
        let page_id = args.page_id.clone().or_else(|| context.page_id.clone());

        let page_id = match page_id {
            Some(id) => id,
            None => {
                return Ok(ToolResult::success(serde_json::json!({
                    "needs_binding": true,
                    "message": "No page is currently bound. Please select a page to read.",
                    "hint": "Open a page in split view or use the page picker in chat."
                })));
            }
        };
        let markdown_view = match args.view.as_deref().map(str::trim) {
            None | Some("") => false,
            Some("markdown") => true,
            Some(other) => {
                return Err(ToolError::InvalidParameters(format!(
                    "view takes \"markdown\" or nothing; got \"{other}\""
                )))
            }
        };
        let ids = args.ids.map(string_list).transpose()?.unwrap_or_default();
        let after = args
            .after
            .as_deref()
            .map(str::trim)
            .filter(|a| !a.is_empty())
            .map(str::to_string);
        if !ids.is_empty() && after.is_some() {
            return Err(ToolError::InvalidParameters(
                "send ids or after, not both".into(),
            ));
        }

        // Get page metadata from database (title, etc.)
        let page = pages::get_page(self.pool.as_ref(), &page_id)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to get page: {}", e)))?;

        if page.format == PageFormat::Markdown {
            // Get content from Yjs if available (live state), otherwise use database content
            let content = if let Some(ref yjs_state) = self.yjs_state {
                match yjs_state.read_text(&page_id).await {
                    Ok(yjs_content) => yjs_content,
                    Err(_) => page.content.clone(), // Fallback to database on error
                }
            } else {
                page.content.clone()
            };

            return Ok(ToolResult::success(serde_json::json!({
                "page_id": page.id,
                "title": page.title,
                "format": "markdown",
                "content": content,
                "content_length": content.len(),
            })));
        }

        let pool = self.pool.as_ref();
        let tree = self.current_tree(&page.id).await?;

        if markdown_view {
            return Ok(ToolResult::success(json!({
                "page_id": page.id,
                "title": page.title,
                "format": "tree",
                "markdown": virtues_document::to_markdown(&tree),
            })));
        }

        // An `ids` or `after` read adds to an earlier read: what the model
        // has seen is that read's base with the blocks shown now brought up
        // to date.
        let base_tree = if ids.is_empty() && after.is_none() {
            None
        } else {
            let base = args
                .base
                .as_deref()
                .map(str::trim)
                .filter(|b| !b.is_empty())
                .ok_or_else(|| ToolError::InvalidParameters(IDS_NEED_BASE.into()))?;
            Some(
                self.base_tree(&page.id, base, &tree)
                    .await?
                    .ok_or_else(|| ToolError::InvalidParameters(IDS_NEED_BASE.into()))?,
            )
        };

        if let Some(base) = base_tree.as_ref().filter(|_| !ids.is_empty()) {
            // A block whose list (or quote, or table) the base never held
            // enters it only with that list, so the list is what is shown.
            // When the list is too long to show whole, it is shown cut down
            // to the way to the block (`path_copy`).
            let mut found: Vec<Node> = vec![];
            let mut missing = vec![];
            // The blocks asked for, by the block shown for them, in the order
            // asked: several items of one list the base lacks are one list.
            let mut asked_in: Vec<(String, Vec<String>)> = vec![];
            for id in &ids {
                let Some(shown) = virtues_document::shown_for(&base.tree, &tree, id) else {
                    missing.push(id.clone());
                    continue;
                };
                match asked_in.iter_mut().find(|(s, _)| *s == shown) {
                    Some((_, inside)) => inside.push(id.clone()),
                    None => asked_in.push((shown, vec![id.clone()])),
                }
            }
            // Each list shown cut down, the blocks in it shown in part, and
            // the blocks asked for that it shows.
            let mut cut: Vec<(String, Vec<String>, Vec<String>)> = vec![];
            for (shown, inside) in &asked_in {
                let Some(node) = virtues_document::ops::find_node(&tree, shown) else {
                    continue;
                };
                if !inside.contains(shown) && !fits_one_read(node) {
                    let targets: Vec<&str> = inside.iter().map(String::as_str).collect();
                    if let Some((copy, partial)) = path_copy(node, &targets, None) {
                        cut.push((shown.clone(), partial, inside.clone()));
                        found.push(copy);
                        continue;
                    }
                }
                found.push(node.clone());
            }
            let read = read_by_id(&found, READ_MARKDOWN_BYTES, json_bytes);
            // A block asked for and gone is seen gone, so it leaves the
            // base; one the read had no room for stays as the base had it.
            let fresh: HashSet<String> =
                read.seen.iter().cloned().chain(missing.iter().cloned()).collect();
            let mut seen = virtues_document::seen_tree(&base.tree, &tree, &fresh);
            // A list shown cut down enters the base as it was shown.
            let cut: Vec<(String, Vec<String>, Vec<String>)> =
                cut.into_iter().filter(|(outer, _, _)| read.seen.contains(outer)).collect();
            for (outer, _, _) in &cut {
                if let Some(copy) = found.iter().find(|n| n.id() == Some(outer.as_str())) {
                    replace_node(&mut seen, outer, copy.clone());
                }
            }
            // An opened block enters the base as it was shown: the blocks
            // inside it this read listed, and any an earlier read showed, as
            // that read showed them. Taken from the page, those would bring
            // what no read has shown into the base: a person's change to a
            // block read before, which a write would then land over unseen,
            // or the rest of a list inside read only in part.
            if let Some(opened) = &read.opened {
                let mut keep: Vec<String> = opened.shown.clone();
                let mut as_read: HashSet<String> = HashSet::new();
                if let Some(had) = virtues_document::ops::find_node(&base.tree, &opened.id) {
                    for id in had.content.iter().filter_map(Node::id) {
                        if !opened.shown.iter().any(|s| s == id) {
                            as_read.insert(id.to_string());
                        }
                        keep.push(id.to_string());
                    }
                }
                keep_shown_part(&mut seen, &opened.id, &keep);
                virtues_document::keep_as_read(&mut seen, &base.tree, &as_read);
            }
            // What this read showed as HTML: each block it showed whole,
            // and of one it opened, its own tag and the blocks inside it the
            // read showed, not those an earlier read showed as markdown.
            let mut as_html: HashSet<String> = HashSet::new();
            for id in &read.seen {
                let opened = read.opened.as_ref().filter(|o| o.id == *id);
                let Some(node) = virtues_document::ops::find_node(&seen, id) else {
                    continue;
                };
                match opened {
                    Some(opened) => {
                        as_html.insert(id.clone());
                        let shown = |c: &&Node| c.id().is_some_and(|cid| opened.shown.iter().any(|s| s == cid));
                        for child in node.content.iter().filter(shown) {
                            as_html.extend(ids_in(std::slice::from_ref(child)));
                        }
                    }
                    None => as_html.extend(ids_in(std::slice::from_ref(node))),
                }
            }
            let mut next_base = carry(base, seen, &as_html);
            for (_, partial, _) in &cut {
                for id in partial {
                    shown_in_part(&mut next_base, &tree, id);
                }
            }
            if let Some(opened) = &read.opened {
                shown_in_part(&mut next_base, &tree, &opened.id);
            }
            let kept = keep_base(pool, &page.id, &next_base).await?;
            // A block asked for that the read did not show.
            let shown_ids: HashSet<String> = read
                .seen
                .iter()
                .filter_map(|id| virtues_document::ops::find_node(&next_base.tree, id))
                .flat_map(|n| ids_in(std::slice::from_ref(n)))
                .collect();
            let mut unread = read.unread.clone();
            for id in &ids {
                if !missing.contains(id) && !shown_ids.contains(id) && !unread.contains(id) {
                    unread.push(id.clone());
                }
            }
            let mut out = json!({
                "page_id": page.id,
                "format": "tree",
                "base": kept,
                "html": read.html,
                "missing": missing,
            });
            let mut notes = vec![];
            for (outer, partial, inside) in &cut {
                notes.push(cut_down_note(outer, partial, inside));
            }
            if let Some(opened) = &read.opened {
                notes.push(opened.note());
                if let Some(next) = &opened.next {
                    out["more_after"] = json!(next);
                }
            }
            if !read.whole_only.is_empty() {
                notes.push(whole_only_note(&read.whole_only));
            }
            if !unread.is_empty() {
                notes.push(format!(
                    "{} did not fit in this read; read {} with ids and this base.",
                    block_names(&unread),
                    them(&unread),
                ));
                out["unread"] = json!(unread);
            }
            if !notes.is_empty() {
                out["note"] = json!(notes.join(" "));
            }
            if holds_proposal(&found) {
                out["suggestions"] = json!(SUGGESTIONS_NOTE);
            }
            return Ok(ToolResult::success(out));
        }

        if let (Some(base), Some(after)) = (base_tree.as_ref(), after.as_deref()) {
            let Some(at) = tree.iter().position(|n| n.id() == Some(after)) else {
                // A block inside another one: the read goes on inside it,
                // as HTML, which is how the blocks before it were shown.
                let Some((parent, siblings, at)) = siblings_of(&tree, after) else {
                    return Err(ToolError::InvalidParameters(format!(
                        "the page has no block `{after}` now; read it again without after"
                    )));
                };
                let window = children_window(
                    siblings,
                    at + 1,
                    READ_MARKDOWN_BYTES,
                    READ_MARKDOWN_BYTES,
                    json_bytes,
                );
                let fresh: HashSet<String> = window.seen.iter().cloned().collect();
                let mut seen = virtues_document::seen_tree(&base.tree, &tree, &fresh);
                // A block the base lacks, read on inside: it enters the base
                // with the blocks this read shows, cut down to the way to it.
                let mut partial = vec![];
                if let Some(parent) = parent.filter(|p| virtues_document::ops::find_node(&base.tree, p).is_none()) {
                    let outer = virtues_document::shown_for(&base.tree, &tree, parent);
                    let node = outer.as_deref().and_then(|o| virtues_document::ops::find_node(&tree, o));
                    if let (Some(outer), Some(node)) = (outer.as_deref(), node) {
                        if let Some((copy, cut)) = path_copy(node, &[parent], Some(&fresh)) {
                            seen = virtues_document::seen_tree(
                                &base.tree,
                                &tree,
                                &HashSet::from([outer.to_string()]),
                            );
                            replace_node(&mut seen, outer, copy);
                            partial = cut;
                        }
                    }
                }
                let as_html: HashSet<String> = siblings
                    .iter()
                    .filter(|n| n.id().is_some_and(|id| fresh.contains(id)))
                    .flat_map(|n| ids_in(std::slice::from_ref(n)))
                    .collect();
                let mut next_base = carry(base, seen, &as_html);
                for id in &partial {
                    shown_in_part(&mut next_base, &tree, id);
                }
                let kept = keep_base(pool, &page.id, &next_base).await?;
                let mut out = json!({
                    "page_id": page.id,
                    "format": "tree",
                    "base": kept,
                    "html": window.html,
                });
                if let Some(parent) = parent {
                    out["inside"] = json!(parent);
                }
                let mut notes = vec![];
                if let Some(next) = &window.next {
                    notes.push(read_on_inside(parent.unwrap_or_default(), next));
                    out["more_after"] = json!(next);
                }
                if let Some(note) = listed_only_note(&window) {
                    notes.push(note);
                }
                if !notes.is_empty() {
                    out["note"] = json!(notes.join(" "));
                }
                if holds_proposal(&siblings[at + 1..at + 1 + window.listed.len()]) {
                    out["suggestions"] = json!(SUGGESTIONS_NOTE);
                }
                return Ok(ToolResult::success(out));
            };
            let window = page_reads::markdown_window(
                &tree,
                at + 1,
                READ_MARKDOWN_BYTES,
                json_bytes,
                fits_one_read,
            );
            let fresh: HashSet<String> = window
                .listed
                .iter()
                .filter_map(|n| n.id().map(str::to_string))
                .collect();
            let seen = virtues_document::seen_tree(&base.tree, &tree, &fresh);
            // Shown again as markdown: its HTML not shown, as before.
            let mut next_base = carry(base, seen, &ids_in(&window.listed));
            next_base.markdown_only.extend(window.read_base().markdown_only);
            let kept = keep_base(pool, &page.id, &next_base).await?;
            let mut out = json!({
                "page_id": page.id,
                "format": "tree",
                "base": kept,
                "markdown": window.markdown,
            });
            if let Some(next) = window.next {
                out["note"] = json!(read_on(&next));
                out["more_after"] = json!(next);
            }
            if holds_proposal(&window.listed) {
                out["suggestions"] = json!(SUGGESTIONS_NOTE);
            }
            return Ok(ToolResult::success(out));
        }

        // A full read: the whole page as HTML, or a long page's markdown as
        // far as one read holds. The base is the blocks it lists.
        let html = virtues_document::to_html(&tree, true);
        let mut out = json!({
            "page_id": page.id,
            "title": page.title,
            "format": "tree",
        });
        let (shown, next) = if html.chars().count() <= READ_HTML_CHARS {
            out["html"] = json!(html);
            (page_reads::ReadBase::of(tree), None)
        } else {
            let window = page_reads::markdown_window(
                &tree,
                0,
                READ_MARKDOWN_BYTES,
                json_bytes,
                fits_one_read,
            );
            out["markdown"] = json!(window.markdown);
            out["note"] = json!(match &window.next {
                Some(next) => format!("{LONG_PAGE_NOTE} {}", read_on(next)),
                None => LONG_PAGE_NOTE.to_string(),
            });
            (window.read_base(), window.next)
        };
        out["base"] = json!(keep_base(pool, &page.id, &shown).await?);
        if let Some(next) = next {
            out["more_after"] = json!(next);
        }
        if holds_proposal(&shown.tree) {
            out["suggestions"] = json!(SUGGESTIONS_NOTE);
        }
        out["how_to_edit"] = json!(HOW_TO_EDIT);
        out["tags"] = json!(virtues_document::tag_guide());
        Ok(ToolResult::success(out))
    }

    /// Edit a page: a block page by ops against a base, a markdown page by
    /// find/replace. Either form takes `title` to rename, alone or beside
    /// the edit.
    ///
    /// Editing a page is reversible (versions keep it) and local, so it runs
    /// freely with no permission prompt. Write-permission gating lives in
    /// the executor and applies only to destructive or outbound tools
    /// (run_applet, delete_applet).
    pub async fn edit_page(
        &self,
        arguments: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let args: EditPageArgs = serde_json::from_value(arguments)
            .map_err(|e| ToolError::InvalidParameters(format!("Invalid arguments: {}", e)))?;

        // Get page_id from args or context
        let page_id = args.page_id.clone().or_else(|| context.page_id.clone());

        let page_id = match page_id {
            Some(id) => id,
            None => {
                return Ok(ToolResult::success(serde_json::json!({
                    "needs_binding": true,
                    "message": "No page is currently bound for editing. Please select a page to edit.",
                    "hint": "Open a page in split view or use the page picker in chat."
                })));
            }
        };

        let format = pages::page_format(self.pool.as_ref(), &page_id)
            .await
            .map_err(|e| match e {
                crate::error::Error::NotFound(message) => ToolError::ExecutionFailed(message),
                e => ToolError::ExecutionFailed(format!("Failed to get page: {e}")),
            })?;
        match format {
            PageFormat::Tree => self.edit_block_page(page_id, args).await,
            PageFormat::Markdown => self.edit_text_page(page_id, args).await,
        }
    }

    /// `edit_page` on a markdown page: find/replace on its Y.Text.
    ///
    /// `find` matches the raw markdown. `replace` is markdown, with
    /// CriticMarkup markers stripped as a safety fallback. An empty `find`
    /// replaces the whole document; `find` absent with a `title` renames.
    async fn edit_text_page(
        &self,
        page_id: String,
        args: EditPageArgs,
    ) -> Result<ToolResult, ToolError> {
        if has_ops(&args.ops) {
            return Err(ToolError::InvalidParameters(EDITED_AS_TEXT.into()));
        }
        // A whole-page replacement takes an explicit empty `find`: a model
        // that left `find` out did not mean "everything".
        let (find, replace) = match (args.find, args.replace) {
            (Some(find), Some(replace)) => (find, replace),
            (Some(find), None) if !find.is_empty() => {
                return Err(ToolError::InvalidParameters(
                    "send replace with find: the text to put in its place, or an empty string to delete it."
                        .into(),
                ))
            }
            (None, Some(replace)) if !replace.is_empty() => {
                return Err(ToolError::InvalidParameters(
                    "send find with replace: the text it replaces, or an empty find to replace the whole page."
                        .into(),
                ))
            }
            _ => (String::new(), String::new()),
        };

        // Strip CriticMarkup markers from replace text
        let replace_content = Self::strip_critic_markup(&replace);

        // Skip content edit when both find and replace are empty (title-only change)
        let has_content_edit = !find.is_empty() || !replace_content.is_empty();
        if !has_content_edit && args.title.is_none() {
            return Err(ToolError::InvalidParameters(NOTHING_TO_CHANGE.into()));
        }

        // False when the edit is on the page but its save is still pending.
        let mut saved = true;

        // Apply the edit through Yjs if available, otherwise fall back to database
        // With Y.Text, the document IS markdown — no plain text conversion needed
        if let (true, Some(yjs_state)) = (has_content_edit, self.yjs_state.as_ref()) {
            let pool = self.pool.as_ref();
            // Another machine write to this page waits until this one's
            // versions are cut, so neither reads the other as the owner's typing.
            let _turn = yjs_state.write_turn(&page_id).await;

            // The page as it stands, kept first: a version is the state AFTER
            // the edit it records, so without this the text before a chat edit
            // (a whole-page replacement included) would be in no version, and
            // the edit could not be undone. No copy, no edit.
            let kept = keep_restore_point(pool, yjs_state, &page_id).await?;

            // Apply through Yjs for real-time sync (direct markdown find/replace).
            // An edit whose save failed is applied all the same: it is versioned
            // and reported as made, so nobody makes it a second time.
            let edit = yjs_state
                .apply_text_edit(&page_id, &find, &replace_content)
                .await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
            let written = match edit.after {
                Ok(written) => written,
                Err(TextWriteError::NotSaved { written, error }) => {
                    tracing::warn!(page = %page_id, error = %error, "chat edit applied but not saved yet");
                    saved = false;
                    written
                }
                Err(e) => return Err(ToolError::ExecutionFailed(e.to_string())),
            };

            // Typing that landed after the copy above and before the edit is
            // the owner's, and in no version yet: kept as their own, so
            // History credits the chat's version below with the chat's
            // change alone.
            if !kept.holds(&edit.before) {
                pages::cut_version(
                    pool,
                    &page_id,
                    &edit.before,
                    "auto",
                    Some(TYPED_BEFORE_CHAT_EDIT),
                )
                .await;
            }
            // The chat's own version, credited to it.
            pages::cut_version(pool, &page_id, &written, "ai", Some("Edited from a chat")).await;
        } else if has_content_edit {
            // Fallback: direct database update (no real-time sync)
            tracing::warn!("YjsState not available, falling back to direct database update");

            let page = pages::get_page(self.pool.as_ref(), &page_id)
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!("Failed to get page: {}", e)))?;

            if !find.is_empty() && !page.content.contains(&find) {
                return Err(ToolError::ExecutionFailed(format!(
                    "Text not found in page: '{}'",
                    if find.chars().count() > 50 {
                        format!("{}...", find.chars().take(50).collect::<String>())
                    } else {
                        find.clone()
                    }
                )));
            }

            let new_content = if find.is_empty() {
                replace_content.clone()
            } else {
                page.content.replacen(&find, &replace_content, 1)
            };

            let update_req = pages::UpdatePageRequest {
                title: None,
                content: Some(new_content),
                icon: None,
                icon_color: None,
                cover_url: None,
                tags: None,
            };

            pages::update_page(self.pool.as_ref(), &page_id, update_req)
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!("Failed to update page: {}", e)))?;
        }

        let title = self
            .rename(&page_id, args.title.as_deref(), has_content_edit)
            .await?;

        let edit_id = ids::generate_id("edit", &[&page_id, &chrono::Utc::now().to_rfc3339()]);

        let result = EditResult {
            edit_id,
            page_id: page_id.clone(),
            format: PageFormat::Markdown.as_str(),
            find,
            replace: replace_content,
            blocks: None,
        };

        let mut message = match (&args.title, has_content_edit) {
            (Some(t), true) if title.changed => {
                format!("Title changed to '{}' and content edit applied.", t)
            }
            (Some(_), true) => "The content edit is on the page.".to_string(),
            (Some(t), false) => format!("Title changed to '{}'.", t),
            (None, _) => "Edit applied successfully.".to_string(),
        };
        if !saved {
            message.push_str(
                " The edit is on the page, but the server couldn't save it yet and saves \
                 it again on its own.",
            );
        }
        if let Some(e) = &title.error {
            message.push_str(&format!(" The title did not change: {e}."));
        }
        if !saved || title.error.is_some() {
            message.push_str(" Do not make this edit again.");
        }

        Ok(ToolResult::success(serde_json::json!({
            "edit": result,
            "applied": true,
            "saved": saved,
            "title_changed": title.changed,
            "message": message,
        })))
    }

    /// `edit_page` on a block page: ops against the base the model read,
    /// applied all or none through `YjsState::edit_tree`.
    ///
    /// There is no unchecked whole-page write: replacing everything is a
    /// replace or delete per block, each checked against the base. A replace
    /// or delete without a base the server still holds is refused, since
    /// the server cannot tell whether its block changed since the model saw
    /// it; inserts and appends need none.
    async fn edit_block_page(
        &self,
        page_id: String,
        args: EditPageArgs,
    ) -> Result<ToolResult, ToolError> {
        // An empty find and replace ask for no text edit, so they are not
        // the text form: models send them beside a rename.
        let text_form = args.find.as_deref().is_some_and(|f| !f.is_empty())
            || args.replace.as_deref().is_some_and(|r| !r.is_empty());
        if text_form {
            return Err(ToolError::InvalidParameters(EDITED_BY_BLOCK.into()));
        }
        let ops = parse_ops(args.ops)?;
        if ops.is_empty() {
            return match args.title {
                Some(title) => self.rename_only(&page_id, &title, PageFormat::Tree).await,
                None => Err(ToolError::InvalidParameters(NOTHING_TO_CHANGE.into())),
            };
        }

        let problems = html_problems(&ops);
        if !problems.is_empty() {
            return Ok(refused(problems, None, String::new(), Unread::default(), false));
        }

        let Some(yjs) = self.yjs_state.as_ref() else {
            return Err(ToolError::ExecutionFailed(NEEDS_LIVE_DOCUMENT.into()));
        };
        let pool = self.pool.as_ref();

        // What the model read. A base the server no longer keeps still
        // serves when the page has not changed since: its name is the hash
        // of the page as it stands.
        let current = read_tree(yjs, &page_id).await?;
        let read_base = match args
            .base
            .as_deref()
            .map(str::trim)
            .filter(|b| !b.is_empty())
        {
            Some(base) => self.base_tree(&page_id, base, &current).await?,
            None => None,
        };
        // The tree the ops are checked against and merged with.
        let base_tree = read_base.as_ref().map(|b| tree_for_ops(b, &current, &ops));
        let mut problems = vec![];
        let writes_ref_link = ops.iter().any(|op| op_html(op).is_some_and(holds_ref_link));
        let writes_blocks = ops.iter().any(|op| op_html(op).is_some());
        for (i, op) in ops.iter().enumerate() {
            if let Some(problem) = proposal_problem(op, &current) {
                problems.push(Problem::new(format!("op {}", i + 1), problem));
                continue;
            }
            let Some(id) = checked_target(op) else {
                continue;
            };
            let verb = op_name(op);
            // Markdown does not say what a block's HTML is: a replace of
            // one shown only so would write a mention back as a link. So
            // would a move of it: deleted, and written again elsewhere from
            // its markdown in the same batch, a ref in it as a link.
            let rewrites = match op {
                Op::Replace { .. } => Some("replace"),
                Op::Delete { .. } if writes_ref_link => Some("move"),
                _ => None,
            };
            if let (Some(what), Some(read)) = (rewrites, &read_base) {
                if shown_as_markdown(read, id) && virtues_document::ops::find_node(&current, id).is_some() {
                    problems.push(Problem::new(
                        format!("op {}", i + 1),
                        markdown_only_problem(read, &current, id, what),
                    ));
                    continue;
                }
            }
            match &base_tree {
                None => problems.push(Problem::new(
                    format!("op {}", i + 1),
                    format!(
                        "{verb} needs the base from your latest read of this page; that read is no \
                         longer kept, or none was sent. Read it again."
                    ),
                )),
                // A block the read did not show is one whose changes the
                // server cannot check: a replace would land over them unseen.
                Some(base)
                    if virtues_document::ops::find_node(base, id).is_none()
                        && virtues_document::ops::find_node(&current, id).is_some() =>
                {
                    problems.push(Problem::new(
                        format!("op {}", i + 1),
                        format!(
                            "the block `{id}` is not in the read that base names; read it with \
                             get_page_content, ids and that base, then use the base it returns"
                        ),
                    ))
                }
                // A block a read opened and showed in part: the blocks inside
                // it no read showed would go with it.
                Some(base) => {
                    let in_part = read_base.as_ref().map(|r| &r.whole).unwrap_or(&NONE_IN_PART);
                    let rewrites = match (op, &read_base) {
                        (Op::Delete { id }, Some(read)) if writes_blocks => rewritten_in_part(id, read, &current),
                        _ => None,
                    };
                    if let Some(problem) = partly_read(op, base, &current, in_part).or(rewrites) {
                        problems.push(Problem::new(format!("op {}", i + 1), problem));
                    }
                }
            }
        }
        // A comment in an op's HTML, unless the op is refused already: an
        // opened block sent back as shown, its comment saying what was not
        // shown, is refused for that, which says to read on.
        for (i, op) in ops.iter().enumerate() {
            let at = format!("op {}", i + 1);
            if op_html(op).is_some_and(|html| html.contains("<!--")) && !problems.iter().any(|p| p.at == at) {
                problems.push(Problem::new(at, COMMENT_WRITES_NOTHING));
            }
        }
        if !problems.is_empty() {
            return match (&read_base, &base_tree) {
                (Some(read), Some(base)) => self.refusal(&page_id, &ops, read, base, problems, &current).await,
                _ => Ok(refused(problems, None, String::new(), Unread::default(), false)),
            };
        }

        // Another machine write to this page waits until this one's versions
        // are cut, so neither reads the other as the owner's typing.
        let _turn = yjs.write_turn(&page_id).await;
        let kept = keep_restore_point(pool, yjs, &page_id).await?;
        let edit = match yjs.edit_tree(&page_id, base_tree.as_deref(), &ops).await {
            Ok(edit) => edit,
            Err(TreeError::Refused { problems, tree }) => {
                return match (&read_base, &base_tree) {
                    (Some(read), Some(base)) => self.refusal(&page_id, &ops, read, base, problems, &tree).await,
                    _ => {
                        let problems = explain_missing(problems, &ops, None, &tree);
                        let blocks = target_html(&ops, &tree).0;
                        let names_tags = problems.iter().any(|p| names_a_tag(&p.message));
                        Ok(refused(problems, None, blocks, Unread::default(), names_tags))
                    }
                };
            }
            Err(TreeError::NotTree) => {
                return Err(ToolError::InvalidParameters(EDITED_AS_TEXT.into()))
            }
            Err(e @ TreeError::Load(_)) => return Err(ToolError::ExecutionFailed(e.to_string())),
        };

        // An edit whose save failed is applied all the same: it is versioned
        // and reported as made, so nobody makes it a second time.
        let mut saved = true;
        let after = match edit.after {
            Ok(written) => written,
            Err(TextWriteError::NotSaved { written, error }) => {
                tracing::warn!(page = %page_id, error = %error, "chat block edit applied but not saved yet");
                saved = false;
                written
            }
            Err(e) => return Err(ToolError::ExecutionFailed(e.to_string())),
        };
        let changed = after.tree != edit.before.tree;
        if changed {
            // Typing that landed after the copy above and before the edit
            // is the owner's: kept as theirs, so the chat's version below
            // holds the chat's change alone.
            if !kept.holds(&edit.before) {
                pages::cut_version(
                    pool,
                    &page_id,
                    &edit.before,
                    "auto",
                    Some(TYPED_BEFORE_CHAT_EDIT),
                )
                .await;
            }
            pages::cut_version(pool, &page_id, &after, "ai", Some("Edited from a chat")).await;
        }

        let before_tree = edit.before.tree.clone().unwrap_or_default();
        let after_tree = after.tree.clone().unwrap_or_default();
        let written = in_document_order(&after_tree, &edit.applied.written);
        // What is shown is what enters the base below: a block written
        // into a list the base never held is shown with its list.
        let to_show = match &read_base {
            Some(read) => in_document_order(
                &after_tree,
                &shown_for_each(&read.tree, &after_tree, &edit.applied.written),
            ),
            None => written.clone(),
        };
        let (html, shown, unread) = blocks_html(&to_show, WRITTEN_HTML_CHARS, chars);
        let deleted: Vec<String> = ops
            .iter()
            .filter_map(|op| match op {
                Op::Delete { id } => Some(id.clone()),
                _ => None,
            })
            .collect();
        let removed: Vec<String> = ops
            .iter()
            .filter_map(|op| match op {
                Op::Replace { id, .. } | Op::Delete { id } => Some(id.clone()),
                _ => None,
            })
            .collect();

        // What the model has now seen: its base, with the blocks shown above
        // and the ones it deleted brought up to date. Nothing else of the
        // page enters it, so a person's edit elsewhere since the read is
        // still checked on the next write. The edit is made either way, so
        // a base that could not be kept is reported as none, never as a
        // failure that would invite the edit a second time.
        let base = match &read_base {
            Some(read) => {
                let as_html: HashSet<String> = shown
                    .iter()
                    .filter_map(|id| virtues_document::ops::find_node(&after_tree, id))
                    .flat_map(|n| ids_in(std::slice::from_ref(n)))
                    .collect();
                let fresh: HashSet<String> = shown.into_iter().chain(deleted).collect();
                let seen = virtues_document::seen_tree(&read.tree, &after_tree, &fresh);
                let mut next = carry(read, seen, &as_html);
                // A block shown in part that only this edit changed is
                // still unchanged but for what the model wrote.
                for (id, hash) in next.whole.iter_mut() {
                    let then = virtues_document::ops::find_node(&before_tree, id);
                    let now = virtues_document::ops::find_node(&after_tree, id);
                    if let (Some(then), Some(now)) = (then, now) {
                        if page_reads::node_hash(then) == *hash {
                            *hash = page_reads::node_hash(now);
                        }
                    }
                }
                match keep_base(pool, &page_id, &next).await {
                    Ok(base) => Some(base),
                    Err(e) => {
                        tracing::warn!(page = %page_id, error = %e, "block edit made but its next base was not kept");
                        None
                    }
                }
            }
            None => None,
        };

        let title = self.rename(&page_id, args.title.as_deref(), true).await?;

        let result = EditResult {
            edit_id: ids::generate_id("edit", &[&page_id, &chrono::Utc::now().to_rfc3339()]),
            page_id: page_id.clone(),
            format: PageFormat::Tree.as_str(),
            find: clip(&blocks_markdown(&in_document_order(&before_tree, &removed))),
            replace: clip(&blocks_markdown(&written)),
            blocks: Some(
                written
                    .iter()
                    .filter_map(|n| n.id().map(str::to_string))
                    .collect(),
            ),
        };

        let mut message = match (&args.title, title.changed, changed) {
            (Some(t), true, true) => format!("Title changed to '{t}' and the edit is on the page."),
            (Some(t), true, false) => format!(
                "Title changed to '{t}'. The page already says what the ops write, so nothing else changed."
            ),
            (_, _, true) => "The edit is on the page.".to_string(),
            (_, _, false) => "The page already says what the ops write, so nothing changed.".to_string(),
        };
        if !saved {
            message.push_str(" The server couldn't save it yet and saves it again on its own.");
        }
        if let Some(e) = &title.error {
            message.push_str(&format!(" The title did not change: {e}."));
        }
        if !saved || title.error.is_some() {
            message.push_str(" Do not make this edit again.");
        }
        match &base {
            Some(base) => {
                message.push_str(&format!(" Use base {base} for your next edit."));
                // A block written but not shown is not in that base: its
                // next edit reads it first, or meets its own last write as
                // someone else's.
                if !unread.is_empty() {
                    message.push_str(&format!(
                        " {}; read {} with get_page_content, ids and that base before editing {} again.",
                        too_long_to_show(&unread),
                        them(&unread),
                        them(&unread),
                    ));
                }
            }
            None => message.push_str(" Read the page before replacing or deleting."),
        }

        let mut out = json!({
            "applied": true,
            "saved": saved,
            "title_changed": title.changed,
            "html": html,
            "edit": result,
            "message": message,
        });
        if let Some(base) = base {
            out["base"] = json!(base);
        }
        if !unread.is_empty() {
            out["unread"] = json!(unread);
        }
        if !edit.applied.notes.is_empty() {
            out["notes"] = json!(edit.applied.notes);
        }
        Ok(ToolResult::success(out))
    }

    /// A refused batch, as the model's next step needs it: what was wrong
    /// with each op, the blocks it has to write again from what they hold
    /// now, shown as they are, and a base that has seen those and nothing
    /// else, so a retry needs no read of its own. An op that was not refused
    /// is sent again as it was written, so its block stays in the base as
    /// the model read it, and the retry merges it with a person's change as
    /// the first try would have. Nothing was written.
    ///
    /// `read` is the base the batch named; `base` its tree as the ops were
    /// checked against it ([`tree_for_ops`]). A block the base holds only in
    /// part is neither shown nor said to be read by id: the base keeps what
    /// was read of it, and the problem says where reading on starts.
    async fn refusal(
        &self,
        page_id: &str,
        ops: &[Op],
        read: &page_reads::ReadBase,
        base: &[Node],
        problems: Vec<Problem>,
        tree: &[Node],
    ) -> Result<ToolResult, ToolError> {
        let mut problems = explain_missing(problems, ops, Some(base), tree);
        let partial: HashSet<&str> = ops
            .iter()
            .filter(|op| partly_read(op, base, tree, &read.whole).is_some())
            .filter_map(op_target)
            .collect();
        let rewrite: Vec<String> = to_rewrite(ops, &problems, base, tree)
            .into_iter()
            .filter(|id| !partial.contains(id.as_str()))
            .collect();
        let asked = rewrite;
        let rewrite = shown_for_each(&read.tree, tree, &asked);
        let (blocks, shown, unread) = blocks_html(
            &in_document_order(tree, &rewrite),
            WRITTEN_HTML_CHARS,
            chars,
        );
        // A block shown for blocks inside it the base lacks (a long list,
        // for an item in it) that is too long to show here: the blocks
        // themselves are named, since an ids read of each cuts the list down
        // to the way to it, where one of the list opens it from its start,
        // short of them.
        let mut inside: Vec<String> = vec![];
        let unread: Vec<String> = unread
            .into_iter()
            .filter(|outer| {
                let within: Vec<&String> = asked
                    .iter()
                    .filter(|id| *id != outer && virtues_document::shown_for(&read.tree, tree, id).as_ref() == Some(outer))
                    .collect();
                inside.extend(within.iter().map(|id| id.to_string()));
                within.is_empty()
            })
            .collect();
        // One too long to show that has not changed since the read is in
        // this base as it stands: the retry needs no read of it, unless the
        // read showed it only as markdown, whose HTML no read has shown. A
        // block a read opened says where reading on inside it starts, in its
        // problem ([`markdown_only_problem`]); an ids read would open it
        // from its start again.
        let unread: Vec<String> = unread
            .into_iter()
            .filter(|id| {
                let find = virtues_document::ops::find_node;
                find(base, id) != find(tree, id)
                    || (shown_as_markdown(read, id) && !read.whole.contains_key(id.as_str()))
            })
            .collect();
        // One a read opened that changed since says where reading on inside
        // it starts, in its problem ([`changed_in_part`]): an ids read of it
        // would show it as the base holds it, and the retry would be
        // refused again. The blocks inside it gone from the page are seen
        // gone, so reading on from there shows the change.
        let mut gone_inside: Vec<String> = vec![];
        let unread: Vec<String> = unread
            .into_iter()
            .filter(|id| {
                let Some((said, gone)) = changed_in_part(read, tree, id) else {
                    return true;
                };
                let mut told = false;
                for p in problems.iter_mut().filter(|p| {
                    p.message.starts_with(CHANGED_SINCE_READ)
                        && problem_op(&p.at)
                            .filter(|(_, inside)| !inside)
                            .and_then(|(i, _)| ops.get(i))
                            .and_then(op_target)
                            == Some(id.as_str())
                }) {
                    p.message = said.clone();
                    told = true;
                }
                if told {
                    gone_inside.extend(gone);
                }
                !told
            })
            .collect();
        // A block that is gone is seen gone; one too long to show here
        // stays as the base had it, and the reply names it to be read by
        // id: sent again with this base, it would be refused again.
        let fresh: HashSet<String> = rewrite
            .into_iter()
            .filter(|id| {
                shown.contains(id) || virtues_document::ops::find_node(tree, id).is_none()
            })
            .chain(gone_inside)
            .collect();
        let mut seen = virtues_document::seen_tree(&read.tree, tree, &fresh);
        // A block an op that was not refused writes over stays as read even
        // inside a block brought up to date around it: sent again as
        // written, it merges with a person's change as the first try would
        // have.
        virtues_document::keep_as_read(&mut seen, &read.tree, &kept_as_written(ops, &problems));
        let as_html: HashSet<String> = shown
            .iter()
            .filter_map(|id| virtues_document::ops::find_node(tree, id))
            .flat_map(|n| ids_in(std::slice::from_ref(n)))
            .collect();
        let new_base = keep_base(self.pool.as_ref(), page_id, &carry(read, seen, &as_html)).await?;
        let names_tags = problems.iter().any(|p| names_a_tag(&p.message));
        Ok(refused(
            problems,
            Some(new_base),
            blocks,
            Unread { long: unread, inside },
            names_tags,
        ))
    }

    /// The tree a block page holds now: live from Yjs, or from its saved
    /// state when this process has no Yjs (a CLI read).
    async fn current_tree(&self, page_id: &str) -> Result<Vec<Node>, ToolError> {
        if let Some(yjs) = &self.yjs_state {
            return read_tree(yjs, page_id).await;
        }
        let state: Option<Option<Vec<u8>>> = sqlx::query_scalar(
            "SELECT yjs_state FROM app_pages WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(page_id)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read the page: {e}")))?;
        let state = state
            .ok_or_else(|| ToolError::ExecutionFailed(format!("Page not found: {page_id}")))?
            .ok_or_else(|| {
                ToolError::ExecutionFailed(format!(
                    "the block page {page_id} has no saved document on your server"
                ))
            })?;
        crate::server::yjs::page_text_of_state(&state)
            .map_err(|e| {
                ToolError::ExecutionFailed(format!(
                    "your server couldn't read the page's document: {e}"
                ))
            })?
            .tree
            .ok_or_else(|| {
                ToolError::ExecutionFailed(format!(
                    "the page {page_id} is a block page, but its saved document holds text"
                ))
            })
    }

    /// The tree `base` names for this page: the one kept for it, or the
    /// page as it is now when that hashes to `base`. `None` when neither.
    async fn base_tree(
        &self,
        page_id: &str,
        base: &str,
        current: &[Node],
    ) -> Result<Option<page_reads::ReadBase>, ToolError> {
        let kept = page_reads::read_base(self.pool.as_ref(), page_id, base)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(match kept {
            Some(kept) => Some(kept),
            None if page_reads::tree_hash(current) == base => Some(page_reads::ReadBase::of(current.to_vec())),
            None => None,
        })
    }

    /// Rename only: no content edit, so a title that cannot change is the
    /// call's failure.
    async fn rename_only(
        &self,
        page_id: &str,
        title: &str,
        format: PageFormat,
    ) -> Result<ToolResult, ToolError> {
        self.rename(page_id, Some(title), false).await?;
        let result = EditResult {
            edit_id: ids::generate_id("edit", &[page_id, &chrono::Utc::now().to_rfc3339()]),
            page_id: page_id.to_string(),
            format: format.as_str(),
            find: String::new(),
            replace: String::new(),
            blocks: (format == PageFormat::Tree).then(Vec::new),
        };
        Ok(ToolResult::success(json!({
            "edit": result,
            "applied": true,
            "saved": true,
            "title_changed": true,
            "message": format!("Title changed to '{title}'."),
        })))
    }

    /// Change the title, if one was sent. After a content edit, a title that
    /// could not be changed is reported beside the edit, not as the call's
    /// failure: the edit is on the page, and a failure would invite it a
    /// second time.
    async fn rename(
        &self,
        page_id: &str,
        title: Option<&str>,
        after_an_edit: bool,
    ) -> Result<Renamed, ToolError> {
        let Some(title) = title else {
            return Ok(Renamed {
                changed: false,
                error: None,
            });
        };
        let update_req = pages::UpdatePageRequest {
            title: Some(title.to_string()),
            content: None,
            icon: None,
            icon_color: None,
            cover_url: None,
            tags: None,
        };
        match pages::update_page(self.pool.as_ref(), page_id, update_req).await {
            Ok(_) => Ok(Renamed {
                changed: true,
                error: None,
            }),
            Err(e) if after_an_edit => {
                tracing::warn!(page = %page_id, error = %e, "chat edit applied but the title did not change");
                Ok(Renamed {
                    changed: false,
                    error: Some(e.to_string()),
                })
            }
            Err(e) => Err(ToolError::ExecutionFailed(format!(
                "Failed to update title: {}",
                e
            ))),
        }
    }
}

/// `create_page`'s result: the page's format, and what converting a block
/// page's markdown changed on the way in.
fn created_result(created: pages::CreatedPage) -> Value {
    let page = created.page;
    let message = match page.format {
        PageFormat::Markdown => "Page created.",
        PageFormat::Tree => {
            "Page created. Read it with get_page_content before editing its blocks."
        }
    };
    let mut out = json!({
        "page_id": page.id,
        "title": page.title,
        "format": page.format.as_str(),
        "message": message,
    });
    if !created.notes.is_empty() {
        out["notes"] = json!(created.notes);
    }
    out
}

/// What became of a title sent with an edit.
struct Renamed {
    changed: bool,
    error: Option<String>,
}

impl std::fmt::Debug for PageEditorTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PageEditorTool").finish()
    }
}

/// Keep the page as it stands before a machine changes it. No copy, no
/// edit: the edit could not be undone.
async fn keep_restore_point(
    pool: &PgPool,
    yjs: &YjsState,
    page_id: &str,
) -> Result<pages::RestorePoint, ToolError> {
    pages::cut_restore_point(pool, yjs, page_id)
        .await
        .map_err(|e| {
            ToolError::ExecutionFailed(format!(
            "could not keep a copy of the page before editing it, so the edit was not made: {e}"
        ))
        })
}

async fn read_tree(yjs: &YjsState, page_id: &str) -> Result<Vec<Node>, ToolError> {
    yjs.read_tree(page_id).await.map_err(|e| match e {
        TreeError::NotTree => ToolError::InvalidParameters(EDITED_AS_TEXT.into()),
        e => ToolError::ExecutionFailed(e.to_string()),
    })
}

async fn keep_base(pool: &PgPool, page_id: &str, kept: &page_reads::ReadBase) -> Result<String, ToolError> {
    page_reads::keep_read_base(pool, page_id, kept)
        .await
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
}

/// A refused batch's result. A success to the caller, with `status:
/// "refused"`: the model acts on it in the same turn, the chat draws no
/// failed card, and the CLI exits 1 with its sentence.
/// The blocks a refusal could not show, to read by id: too long to show
/// here (`long`), or inside a block too long to show, which an `ids` read of
/// each cuts down to the way to it (`inside`).
#[derive(Default)]
struct Unread {
    long: Vec<String>,
    inside: Vec<String>,
}

fn refused(
    problems: Vec<Problem>,
    base: Option<String>,
    blocks: String,
    unread: Unread,
    names_tags: bool,
) -> ToolResult {
    let said = [
        (!unread.long.is_empty()).then(|| too_long_to_show(&unread.long)),
        (!unread.inside.is_empty()).then(|| inside_too_long(&unread.inside)),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("; ");
    let unread: Vec<String> = unread.long.into_iter().chain(unread.inside).collect();
    let error = format!(
        "Nothing was written. {}",
        problems
            .iter()
            .map(|p| format!("{}: {}", p.at, p.message))
            .collect::<Vec<_>>()
            .join("; ")
    );
    // A problem with the page or the batch as a whole names no op: sent
    // again as it is, the batch is refused again.
    let whole = |at: &str| problems.iter().any(|p| p.at == at);
    // A problem that says to read on first: the read's base is the one to
    // send again with, never this one, which has not seen what it shows.
    let reads_on = problems.iter().any(|p| p.message.contains("with get_page_content, after"));
    let message = match (&base, blocks.is_empty(), unread.is_empty()) {
        _ if whole("page") => "Your server can't edit this page until it is updated, so don't send \
             this edit again. Tell the person their server needs an update to edit it."
            .to_string(),
        _ if whole("ops") => "The batch as a whole was refused: split it into smaller edits, or \
             nest its blocks less deeply, as the problem says. Sent again as it is, it is refused \
             again."
            .to_string(),
        (Some(base), shown_none, false) => format!(
            "{}{}: read {} with get_page_content, ids and base {base}, then send the whole batch \
             again with the base that read returns, the ops that were not refused as you wrote them.",
            if shown_none {
                ""
            } else {
                "The other blocks the refused ops name are above as they are now. "
            },
            said,
            them(&unread),
        ),
        (Some(base), false, true) => format!(
            "The blocks the refused ops name are above as they are now: write those ops from them. \
             Send the whole batch again with base {base}, the ops that were not refused as you wrote them."
        ),
        (Some(_), true, true) if reads_on => "Read on where the problems say, then change the \
             refused ops from what that read shows and send the whole batch again with the base \
             that read returns, the ops that were not refused as you wrote them."
            .to_string(),
        (Some(base), true, true) => format!(
            "Change the refused ops as the problems say, then send the whole batch again with base \
             {base}, the ops that were not refused as you wrote them."
        ),
        (None, ..) => "Fix the ops and send the whole batch again. A replace or delete needs the \
                      base from a read of the page."
            .to_string(),
    };
    let mut out = json!({
        "applied": false,
        "status": "refused",
        "error": error,
        "refused": problems,
    });
    if let Some(base) = base {
        out["base"] = json!(base);
    }
    if !blocks.is_empty() {
        out["blocks"] = json!(blocks);
    }
    if !unread.is_empty() {
        out["unread"] = json!(unread);
    }
    if names_tags {
        out["tags"] = json!(virtues_document::tag_guide());
    }
    out["message"] = json!(message);
    ToolResult::success(out)
}

/// Whether ops were sent at all: an absent key, `null`, an empty list, and
/// a string that is blank or holds an empty list all mean none, as
/// [`parse_ops`] reads them. Models that fill every field the schema offers
/// send `ops: []` beside find and replace.
fn has_ops(ops: &Option<Value>) -> bool {
    match ops {
        None | Some(Value::Null) => false,
        Some(Value::Array(items)) => !items.is_empty(),
        Some(Value::String(s)) => match serde_json::from_str::<Value>(s) {
            Ok(Value::Array(items)) => !items.is_empty(),
            Ok(Value::Null) => false,
            _ => !s.trim().is_empty(),
        },
        Some(_) => true,
    }
}

/// The ops, from an array or a string holding one (models stringify nested
/// arrays). Each is read on its own so a problem names `op N`; an op name
/// written with hyphens (`insert-after`) is read as the tool spells it.
fn parse_ops(ops: Option<Value>) -> Result<Vec<Op>, ToolError> {
    let shape = "ops must be a JSON array of {op, id, html} objects";
    let value = match ops {
        None | Some(Value::Null) => return Ok(vec![]),
        Some(Value::String(s)) if s.trim().is_empty() => return Ok(vec![]),
        Some(Value::String(s)) => serde_json::from_str::<Value>(&s)
            .map_err(|e| ToolError::InvalidParameters(format!("{shape}: {e}")))?,
        Some(v) => v,
    };
    let Value::Array(items) = value else {
        return Err(ToolError::InvalidParameters(shape.into()));
    };
    if items.len() > MAX_OPS {
        return Err(ToolError::InvalidParameters(format!(
            "{} ops in one edit; one edit takes at most {MAX_OPS}. Split it.",
            items.len()
        )));
    }
    let mut parsed = vec![];
    let mut problems = vec![];
    for (i, mut item) in items.into_iter().enumerate() {
        if let Some(Value::String(name)) = item.get_mut("op") {
            *name = name.trim().replace('-', "_");
        }
        match serde_json::from_value::<Op>(item) {
            Ok(op) => parsed.push(op),
            Err(e) => problems.push(format!("op {}: {e}", i + 1)),
        }
    }
    if !problems.is_empty() {
        return Err(ToolError::InvalidParameters(problems.join("; ")));
    }
    Ok(parsed)
}

/// What is wrong with an op's HTML before it reaches the contract: markdown
/// where HTML belongs, and proposals, which the page's owner makes in the
/// page. A replace's proposals are checked against its block
/// ([`proposal_problem`]), which may hold one.
fn html_problems(ops: &[Op]) -> Vec<Problem> {
    let mut problems = vec![];
    for (i, op) in ops.iter().enumerate() {
        let Some(html) = op_html(op) else { continue };
        let at = format!("op {}", i + 1);
        if !html.trim().is_empty() && !html.trim_start().starts_with('<') {
            problems.push(Problem::new(
                at,
                "`html` must be HTML blocks such as <p>…</p>; this looks like markdown",
            ));
        } else if !matches!(op, Op::Replace { .. }) && writes_proposal(html) {
            problems.push(Problem::new(at, NEW_PROPOSAL));
        }
    }
    problems
}

/// Refusing op HTML that holds a comment. A read shows a block too long for
/// it as a comment naming its id; a comment writes nothing, so one copied
/// into a replace of the block around it deleted that block.
const COMMENT_WRITES_NOTHING: &str = "the html holds an HTML comment (<!-- … -->), which writes \
     nothing: a block a read showed as a comment would go. Leave such a block out of your ops and \
     edit the blocks around it by id; to take it out, delete it by id";

/// Refusing an op that writes proposal tags into new content.
const NEW_PROPOSAL: &str =
    "proposals are made in the page by its owner; write the text as it should read";

/// Beside a read that shows a suggestion: what the two tags are, so the
/// model leaves the block alone rather than deciding it for the owner.
pub(crate) const SUGGESTIONS_NOTE: &str = "<virtues-del> and <virtues-ins> ({--…--} and {++…++} \
     in markdown) mark a suggestion the page's owner has not accepted or rejected yet. A block \
     holding one cannot be replaced: leave it as it is, or ask the owner to accept or reject the \
     suggestion first.";

fn writes_proposal(html: &str) -> bool {
    let lower = html.to_ascii_lowercase();
    lower.contains("<virtues-ins") || lower.contains("<virtues-del")
}

/// Whether any of `nodes` holds a suggestion its owner has not decided:
/// text under a proposal mark, at any depth.
pub(crate) fn holds_proposal(nodes: &[Node]) -> bool {
    let mut found = false;
    for n in nodes {
        n.walk(&mut |n| {
            found |= n
                .marks
                .iter()
                .any(|m| virtues_document::contract::OWNER_MARKS.contains(&m.kind.as_str()));
        });
    }
    found
}

/// Why a replace or a delete cannot be made over its block's suggestion,
/// at any depth inside it: whatever HTML a replace sends decides the
/// suggestion, since an edit cannot write proposal tags and a block
/// rewritten without them keeps neither side as a suggestion, and a delete
/// takes both sides out. `None` for an insert, or a block holding none.
fn proposal_problem(op: &Op, current: &[Node]) -> Option<String> {
    let (id, html) = match op {
        Op::Replace { id, html } => (id, Some(html)),
        Op::Delete { id } => (id, None),
        _ => return None,
    };
    let holds = virtues_document::ops::find_node(current, id)
        .is_some_and(|block| holds_proposal(std::slice::from_ref(block)));
    if holds {
        return Some(format!(
            "the block `{id}` holds a suggestion its owner has not accepted or rejected yet; \
             leave it as it is, or ask them to accept or reject it first"
        ));
    }
    html.is_some_and(|html| writes_proposal(html)).then(|| NEW_PROPOSAL.to_string())
}

fn op_html(op: &Op) -> Option<&str> {
    match op {
        Op::Replace { html, .. }
        | Op::InsertAfter { html, .. }
        | Op::InsertBefore { html, .. }
        | Op::Append { html } => Some(html),
        Op::Delete { .. } => None,
    }
}

/// The block an op names, if it names one.
fn op_target(op: &Op) -> Option<&str> {
    match op {
        Op::Replace { id, .. }
        | Op::InsertAfter { id, .. }
        | Op::InsertBefore { id, .. }
        | Op::Delete { id } => Some(id),
        Op::Append { .. } => None,
    }
}

/// The block an op changes, which the base must have shown: a replace's or
/// a delete's. Inserts and appends change no block that is there.
fn checked_target(op: &Op) -> Option<&str> {
    match op {
        Op::Replace { id, .. } | Op::Delete { id } => Some(id),
        _ => None,
    }
}

fn op_name(op: &Op) -> &'static str {
    match op {
        Op::Replace { .. } => "replace",
        Op::InsertAfter { .. } => "insert_after",
        Op::InsertBefore { .. } => "insert_before",
        Op::Append { .. } => "append",
        Op::Delete { .. } => "delete",
    }
}

/// The op a problem is about: `N - 1` for `op N`, or for `op N: …`, a
/// problem inside its HTML (the second of the pair). `None` for a problem
/// about the batch as a whole.
fn problem_op(at: &str) -> Option<(usize, bool)> {
    let rest = at.strip_prefix("op ")?;
    let (number, inside) = match rest.split_once(':') {
        Some((number, _)) => (number, true),
        None => (rest, false),
    };
    let n: usize = number.trim().parse().ok()?;
    Some((n.checked_sub(1)?, inside))
}

/// The blocks a refused batch's retry has to write from what they hold now,
/// in no order: the blocks of the refused ops, each one only when its op was
/// refused over the block itself (it is gone, the read did not show it, it
/// changed in a way the edit could not merge with, it holds a suggestion)
/// or when it has not changed since the base, which a refresh leaves as it
/// was.
///
/// A block that changed since the base and whose op was refused for its own
/// HTML is left out: the model fixes that HTML from its copy, and the retry
/// merges it with the change as it stands. So is any block an op that was
/// not refused rewrites or deletes: it is sent again as written, from the
/// copy the base holds. Either brought into the base would let the retry
/// land over a person's change the first try would have kept.
fn to_rewrite(ops: &[Op], problems: &[Problem], base: &[Node], tree: &[Node]) -> Vec<String> {
    use virtues_document::ops::find_node;
    let about: Vec<(usize, bool)> = problems.iter().filter_map(|p| problem_op(&p.at)).collect();
    let refused_ops: HashSet<usize> = about.iter().map(|(i, _)| *i).collect();
    let kept_as_written = kept_as_written(ops, problems);
    let unchanged = |id: &str| match (find_node(base, id), find_node(tree, id)) {
        (Some(then), Some(now)) => then == now,
        _ => false,
    };
    let mut out: Vec<String> = vec![];
    for i in refused_ops {
        let Some(id) = ops.get(i).and_then(op_target) else {
            continue;
        };
        let inside_html = about.iter().any(|(j, inside)| *j == i && *inside);
        let gone = find_node(tree, id).is_none();
        let unseen = find_node(base, id).is_none();
        let rewrite = gone || unseen || unchanged(id) || !inside_html;
        if rewrite && !kept_as_written.contains(id) && !out.iter().any(|o| o == id) {
            out.push(id.to_string());
        }
    }
    out
}

/// The blocks a refused batch's retry writes over as the model wrote them:
/// those of the replaces and deletes that were not refused.
fn kept_as_written(ops: &[Op], problems: &[Problem]) -> HashSet<String> {
    let refused_ops: HashSet<usize> = problems
        .iter()
        .filter_map(|p| problem_op(&p.at))
        .map(|(i, _)| i)
        .collect();
    ops.iter()
        .enumerate()
        .filter(|(i, _)| !refused_ops.contains(i))
        .filter_map(|(_, op)| checked_target(op))
        .map(str::to_string)
        .collect()
}

/// What to show of `tree` so that each of `ids` enters a base made from
/// `base` ([`virtues_document::shown_for`]), each once: a block on the page
/// whose container `base` lacks becomes that container. A block gone from
/// the page stays named, to leave the base.
fn shown_for_each(base: &[Node], tree: &[Node], ids: &[String]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for id in ids {
        let shown = virtues_document::shown_for(base, tree, id).unwrap_or_else(|| id.clone());
        if !out.contains(&shown) {
            out.push(shown);
        }
    }
    out
}

/// Each of the crate's "no block has id `x`" said as what happened to `x`,
/// with what to do instead, so a retry is not the same batch again:
///
/// - `x` is still on the page, so an op earlier in the batch took it away
///   (ops run in order): that op is named.
/// - the read had `x` and the page does not, so someone deleted it after the
///   read: a replace or delete leaves it out, an insert anchors elsewhere.
///
/// Any other problem, and a block never on the page, as it is.
fn explain_missing(
    problems: Vec<Problem>,
    ops: &[Op],
    base: Option<&[Node]>,
    tree: &[Node],
) -> Vec<Problem> {
    use virtues_document::ops::find_node;
    problems
        .into_iter()
        .map(|p| {
            let Some(id) = p
                .message
                .strip_prefix("no block has id `")
                .and_then(|rest| rest.strip_suffix('`'))
            else {
                return p;
            };
            let Some((i, op)) = problem_op(&p.at)
                .filter(|(_, inside)| !inside)
                .and_then(|(i, _)| ops.get(i).map(|op| (i, op)))
            else {
                return p;
            };
            let insert = matches!(op, Op::InsertAfter { .. } | Op::InsertBefore { .. });
            let message = if find_node(tree, id).is_some() {
                let Some(j) = ops[..i].iter().rposition(|o| removes(o, id, tree)) else {
                    return p;
                };
                let earlier = j + 1;
                match op {
                    Op::Delete { .. } => {
                        format!("op {earlier} already removes the block `{id}`; leave this op out")
                    }
                    _ if insert => format!(
                        "op {earlier} removes the block `{id}` before this op runs, and ops run in \
                         order; put this op before op {earlier}, or anchor it on another block"
                    ),
                    _ => format!(
                        "op {earlier} removes the block `{id}` before this op runs, and ops run in \
                         order; leave one of the two out"
                    ),
                }
            } else if base.is_some_and(|b| find_node(b, id).is_some()) {
                if insert {
                    format!(
                        "the block `{id}` was deleted after you read the page; anchor this op on \
                         another block, or append it"
                    )
                } else {
                    format!(
                        "the block `{id}` was deleted after you read the page; leave it out \
                         unless you were asked to bring it back"
                    )
                }
            } else {
                return p;
            };
            Problem::new(p.at, message)
        })
        .collect()
}

/// Whether `op` takes the block `id` off the page: deletes it or a block
/// around it, replaces a block around it, or replaces it with nothing.
fn removes(op: &Op, id: &str, tree: &[Node]) -> bool {
    use virtues_document::ops::find_node;
    let holds = |outer: &str| {
        find_node(tree, outer).is_some_and(|n| find_node(&n.content, id).is_some())
    };
    match op {
        Op::Delete { id: target } => target == id || holds(target),
        Op::Replace { id: target, html } => (target == id && html.trim().is_empty()) || holds(target),
        _ => false,
    }
}

/// Whether a refusal is about a tag, an attribute or what a block may hold,
/// so the tag guide belongs beside it.
fn names_a_tag(message: &str) -> bool {
    message.contains('<')
        || message.contains("attribute")
        || message.contains(" takes ")
        || message.contains(" holds `")
}

/// The blocks the ops name, as they are in `tree` now, up to
/// [`WRITTEN_HTML_CHARS`]: the HTML, the ids shown, and the ids of those
/// on the page that were not.
fn target_html(ops: &[Op], tree: &[Node]) -> (String, HashSet<String>, Vec<String>) {
    let targets: Vec<String> = ops
        .iter()
        .filter_map(op_target)
        .map(str::to_string)
        .collect();
    blocks_html(
        &in_document_order(tree, &targets),
        WRITTEN_HTML_CHARS,
        chars,
    )
}

/// `nodes` as HTML with ids, whole blocks only, as many as fit in `limit` as
/// `measure` counts them, a block that does not fit passed over for the
/// ones after it: the HTML, the ids of the blocks shown, and the ids of the
/// blocks not shown, in order.
fn blocks_html(
    nodes: &[Node],
    limit: usize,
    measure: impl Fn(&str) -> usize,
) -> (String, HashSet<String>, Vec<String>) {
    let mut html = String::new();
    let mut length = 0;
    let mut shown = HashSet::new();
    let mut unshown = vec![];
    for node in nodes {
        let one = virtues_document::to_html(std::slice::from_ref(node), true);
        let n = measure(&one);
        if length + n > limit {
            unshown.extend(node.id().map(str::to_string));
            continue;
        }
        length += n;
        html.push_str(&one);
        if let Some(id) = node.id() {
            shown.insert(id.to_string());
        }
    }
    (html, shown, unshown)
}

/// What an `ids` read shows of the blocks it found ([`read_by_id`]).
#[derive(Default)]
struct ByIdRead {
    html: String,
    /// Every block the read has seen, to enter its base as it stands: shown
    /// whole, listed as too long for any read, or opened.
    seen: HashSet<String>,
    /// Blocks there was no room left for; another `ids` read shows them.
    unread: Vec<String>,
    /// Blocks too long for any read that hold no blocks to show instead.
    whole_only: Vec<String>,
    /// A block too long to show whole, shown by the blocks inside it.
    opened: Option<Opened>,
}

/// A block an `ids` read showed by the blocks inside it ([`open_block`]).
struct Opened {
    id: String,
    /// How many blocks it holds.
    holds: usize,
    /// The ids of the blocks inside it the read showed, whole or as too
    /// long for any read: what enters the base.
    shown: Vec<String>,
    /// Of those, the ones too long for any read, shown as a comment.
    whole_only: Vec<String>,
    /// The blocks inside it the read listed by id alone, to read by id.
    listed_only: Vec<String>,
    /// The last block listed, when more follow it: where reading on starts.
    next: Option<String>,
}

impl Opened {
    fn note(&self) -> String {
        let id = &self.id;
        let mut note = format!(
            "`{id}` is too long to show whole, so it is shown with {} of the {} blocks inside it. \
             Edit those by id. Replacing `{id}` itself, or deleting it and writing it again, would \
             lose the blocks inside it not shown, so either waits until reads have shown you all \
             of them.",
            self.shown.len() - self.whole_only.len(),
            self.holds
        );
        if !self.whole_only.is_empty() {
            note.push(' ');
            note.push_str(&format!(
                "{} inside it {} too long for any read, shown as a comment: a replace of `{id}` \
                 that leaves {} out deletes {}.",
                block_names(&self.whole_only),
                if self.whole_only.len() == 1 { "is" } else { "are" },
                them(&self.whole_only),
                them(&self.whole_only),
            ));
        }
        if !self.listed_only.is_empty() {
            note.push(' ');
            note.push_str(&listed_by_id(&self.listed_only));
        }
        if let Some(next) = &self.next {
            note.push(' ');
            note.push_str(&read_on_inside(id, next));
        }
        note
    }
}

/// What to do with blocks a read listed by id alone, inside another.
fn listed_by_id(ids: &[String]) -> String {
    let verb = if ids.len() == 1 { "is" } else { "are" };
    format!(
        "{} inside it {verb} listed by id alone: read {} with get_page_content, ids and this base \
         before editing {}.",
        block_names(ids),
        them(ids),
        them(ids),
    )
}

/// The note for blocks a read on inside a block listed by id alone.
fn listed_only_note(window: &ChildWindow) -> Option<String> {
    let only: Vec<String> = window
        .listed
        .iter()
        .filter(|id| !window.seen.contains(id))
        .cloned()
        .collect();
    (!only.is_empty()).then(|| listed_by_id(&only))
}

/// `found` as an `ids` read shows it, in `limit` as `measure` counts: each
/// block whole while there is room; a block too long for any read listed by
/// id, or, when it holds blocks with ids, opened ([`open_block`]), one per
/// read. Every block the read lists enters its base as it stands, so one
/// too long to show can still be deleted, or replaced whole, and a block
/// asked for again is shown again, opened again, or said to be too long,
/// never passed over twice for the same reason.
fn read_by_id(found: &[Node], limit: usize, measure: impl Fn(&str) -> usize + Copy) -> ByIdRead {
    let mut read = ByIdRead::default();
    let mut room = limit;
    for node in found {
        let Some(id) = node.id().map(str::to_string) else {
            continue;
        };
        let one = virtues_document::to_html(std::slice::from_ref(node), true);
        let n = measure(&one);
        if n <= room {
            room -= n;
            read.html.push_str(&one);
            read.seen.insert(id);
            continue;
        }
        if n <= limit {
            read.unread.push(id);
            continue;
        }
        if !page_reads::opens(node) {
            let listed = format!("<!-- {id}: {} -->", page_reads::too_long_advice(node));
            if measure(&listed) <= room {
                room -= measure(&listed);
                read.html.push_str(&listed);
                read.whole_only.push(id.clone());
                read.seen.insert(id);
            } else {
                read.unread.push(id);
            }
            continue;
        }
        match read.opened.is_none().then(|| open_block(node, room, limit, measure)).flatten() {
            Some((html, opened)) => {
                room = room.saturating_sub(measure(&html));
                read.html.push_str(&html);
                read.seen.insert(id);
                read.opened = Some(opened);
            }
            None => read.unread.push(id),
        }
    }
    read
}

/// The most a comment about the blocks not shown inside an opened block
/// takes, kept free for it.
const MORE_INSIDE: usize = 160;

/// `node`, too long to show whole, as its own tag around the blocks inside
/// it that fit in `room` ([`children_window`]), and a comment where the
/// rest go. `None` when not even its tag and one block fit.
fn open_block(
    node: &Node,
    room: usize,
    limit: usize,
    measure: impl Fn(&str) -> usize + Copy,
) -> Option<(String, Opened)> {
    let id = node.id()?.to_string();
    let mut shell = node.clone();
    shell.content.clear();
    let empty = virtues_document::to_html(std::slice::from_ref(&shell), true);
    // Empty, the block is its opening tags and then its closing ones: its
    // own, and a table's `<tbody>` inside it, around where its blocks go.
    let (open, close) = empty.split_at(empty.find("</")?);
    let inner_room = room.checked_sub(measure(open) + measure(close) + MORE_INSIDE)?;
    let window = children_window(&node.content, 0, inner_room, limit, measure);
    if window.listed.is_empty() {
        return None;
    }
    let mut html = format!("{open}{}", window.html);
    if let Some(next) = &window.next {
        let rest = node.content.len() - window.listed.len();
        html.push_str(&format!("<!-- {rest} more: read on after \"{next}\" -->"));
    }
    html.push_str(close);
    let listed_only = window
        .listed
        .iter()
        .filter(|id| !window.seen.contains(id))
        .cloned()
        .collect();
    Some((
        html,
        Opened {
            id,
            holds: node.content.len(),
            shown: window.seen,
            whole_only: window.whole_only,
            listed_only,
            next: window.next,
        },
    ))
}

/// Blocks shown one after another, as HTML ([`children_window`]).
struct ChildWindow {
    html: String,
    /// The ids of the blocks shown or listed by id, in order.
    listed: Vec<String>,
    /// Of those, the ones too long for any read and not ones to open,
    /// shown as a comment and seen as they stand.
    whole_only: Vec<String>,
    /// Of those, the ones a read of the window has seen: each shown whole,
    /// or too long for any read and not one to open, which is in a base as
    /// it stands. One listed by id because a read with ids shows it, or the
    /// blocks inside it, is not.
    seen: Vec<String>,
    /// The last id listed, when more blocks follow it.
    next: Option<String>,
}

/// `nodes` from `start`, as many as fit in `room`: each whole, or, when it
/// is too long to show in any read of `limit`, or is the first and does not
/// fit, listed by its id with what to do with it, so reading on always
/// moves.
fn children_window(
    nodes: &[Node],
    start: usize,
    room: usize,
    limit: usize,
    measure: impl Fn(&str) -> usize,
) -> ChildWindow {
    let mut window = ChildWindow {
        html: String::new(),
        listed: vec![],
        whole_only: vec![],
        seen: vec![],
        next: None,
    };
    let mut used = 0;
    let mut end = start;
    for node in nodes.iter().skip(start) {
        let Some(id) = node.id() else { break };
        let one = virtues_document::to_html(std::slice::from_ref(node), true);
        let (part, seen, whole_only) = if used + measure(&one) <= room {
            (one, true, false)
        } else if measure(&one) > limit {
            let advice = page_reads::too_long_advice(node);
            let whole_only = !page_reads::opens(node);
            (format!("<!-- {id}: {advice} -->"), whole_only, whole_only)
        } else if window.listed.is_empty() {
            (
                format!("<!-- {id}: too long to show here; read it with get_page_content and ids -->"),
                false,
                false,
            )
        } else {
            break;
        };
        if used + measure(&part) > room {
            break;
        }
        used += measure(&part);
        window.html.push_str(&part);
        window.listed.push(id.to_string());
        if seen {
            window.seen.push(id.to_string());
        }
        if whole_only {
            window.whole_only.push(id.to_string());
        }
        end += 1;
    }
    if end < nodes.len() {
        window.next = window.listed.last().cloned();
    }
    window
}

/// The id of the block around `id` (none at the page's top level, or inside
/// a block that carries none, such as a table cell), the blocks beside it,
/// and its index among them.
fn siblings_of<'a>(nodes: &'a [Node], id: &str) -> Option<(Option<&'a str>, &'a [Node], usize)> {
    fn walk<'a>(
        parent: Option<&'a str>,
        nodes: &'a [Node],
        id: &str,
    ) -> Option<(Option<&'a str>, &'a [Node], usize)> {
        if let Some(at) = nodes.iter().position(|n| n.id() == Some(id)) {
            return Some((parent, nodes, at));
        }
        nodes
            .iter()
            .find_map(|n| walk(n.id(), &n.content, id))
    }
    walk(None, nodes, id)
}

/// Where the blocks inside a block go on: the read that shows the rest.
fn read_on_inside(inside: &str, next: &str) -> String {
    let what = if inside.is_empty() {
        "More blocks follow".to_string()
    } else {
        format!("More blocks inside `{inside}` follow")
    };
    format!("{what}: read on with get_page_content, after \"{next}\" and this base.")
}

/// The blocks too long for any read, which a read lists by id alone.
fn whole_only_note(ids: &[String]) -> String {
    if ids.len() == 1 {
        format!(
            "{} is too long to show in any read. It is in this base as it stands: delete it, or \
             replace it whole.",
            block_names(ids)
        )
    } else {
        format!(
            "{} are too long to show in any read. They are in this base as they stand: delete \
             them, or replace them whole.",
            block_names(ids)
        )
    }
}

/// `id`, opened by a read, as the read showed it in the base `seen`: its
/// own tag around the blocks inside it in `shown`. Reading on inside it
/// (`after`) adds the rest. A replace or a delete of it before then sees
/// what it has not read (`partly_read`), and is refused.
fn keep_shown_part(seen: &mut [Node], id: &str, shown: &[String]) {
    if let Some(node) = find_node_mut(seen, id) {
        node.content
            .retain(|c| c.id().is_some_and(|cid| shown.iter().any(|s| s == cid)));
    }
}

fn find_node_mut<'a>(nodes: &'a mut [Node], id: &str) -> Option<&'a mut Node> {
    for n in nodes {
        if n.id() == Some(id) {
            return Some(n);
        }
        if let Some(found) = find_node_mut(&mut n.content, id) {
            return Some(found);
        }
    }
    None
}

/// Why a replace or a delete of a block the base holds only part of cannot
/// be made: blocks inside it that no read has shown would go with it.
/// `in_part` names the blocks reads showed in part (`ReadBase::whole`): the
/// block itself, or one inside it, as a list read in part inside a callout
/// read whole goes with the callout. A block read whole that has gained a
/// block since is no such block: its change is the merge's to take or
/// refuse, and a refusal shows it as it is now, which no read on inside it
/// would. A delete takes a block inside it shown in part as it stands while
/// nothing in that block has changed since it was shown, as it takes the
/// block itself ([`tree_for_ops`]).
fn partly_read(op: &Op, base: &[Node], current: &[Node], in_part: &BTreeMap<String, String>) -> Option<String> {
    let (id, deletes) = match op {
        Op::Replace { id, .. } => (id, false),
        Op::Delete { id } => (id, true),
        _ => return None,
    };
    unread_in(id, deletes, base, current, in_part)
}

/// A delete of a block shown in part, in a batch that writes blocks too: a
/// rewrite of it, the delete and the write as one edit, which would put
/// back only the blocks inside it a read showed and lose the rest. A delete
/// alone stands while nothing in the block has changed ([`tree_for_ops`]);
/// beside a write it is the replace [`partly_read`] refuses, made in two ops.
/// `read` is the base as read, before [`tree_for_ops`] took the block as it
/// stands.
fn rewritten_in_part(id: &str, read: &page_reads::ReadBase, current: &[Node]) -> Option<String> {
    let problem = unread_in(id, false, &read.tree, current, &read.whole)?;
    Some(format!(
        "this edit deletes `{id}` and writes blocks too, which puts back only what was read of it: {problem}"
    ))
}

/// [`partly_read`] for the block `id`, a delete (`deletes`) taking a block
/// inside it shown in part as it stands while nothing in it has changed.
fn unread_in(id: &str, deletes: bool, base: &[Node], current: &[Node], in_part: &BTreeMap<String, String>) -> Option<String> {
    let then = virtues_document::ops::find_node(base, id)?;
    let mut held = vec![];
    then.walk(&mut |n| {
        if let Some(inner) = n.id().filter(|inner| in_part.contains_key(*inner)) {
            held.push(inner.to_string());
        }
    });
    held.iter().find_map(|inner| {
        let now = virtues_document::ops::find_node(current, inner)?;
        if deletes && inner != id && in_part.get(inner) == Some(&page_reads::node_hash(now)) {
            return None;
        }
        unread_inside(id, inner, base, now)
    })
}

/// [`partly_read`] for the block `inner` (the op's block `id`, or one inside
/// it): how many of the blocks inside it the base has not read, and where
/// reading on starts.
fn unread_inside(id: &str, inner: &str, base: &[Node], now: &Node) -> Option<String> {
    let then = virtues_document::ops::find_node(base, inner)?;
    let read: HashSet<&str> = then.content.iter().filter_map(Node::id).collect();
    let unread = now
        .content
        .iter()
        .filter(|c| c.id().is_some_and(|cid| !read.contains(cid)))
        .count();
    if unread == 0 || now.content.len() <= then.content.len() {
        return None;
    }
    // A read on from just before the first block not read shows it and
    // what follows; from the start of the block, an ids read of it does.
    let first_unread = now
        .content
        .iter()
        .position(|c| c.id().is_some_and(|cid| !read.contains(cid)))?;
    let before = first_unread.checked_sub(1).and_then(|i| now.content[i].id());
    let read_on = match before {
        Some(last) => format!("read the rest of it with get_page_content, after \"{last}\" and this base"),
        None => format!("read it with get_page_content, ids [\"{inner}\"] and this base"),
    };
    let (what, with) = if inner == id {
        (format!("`{id}` was shown only in part"), "it".to_string())
    } else {
        (format!("`{id}` holds `{inner}`, which was shown only in part"), format!("`{id}`"))
    };
    Some(format!(
        "{what}: {unread} of the blocks inside it have not been read, and would go with {with}. \
         Edit the blocks inside it by id, or {read_on}, first"
    ))
}

/// The ids of every block in `nodes`, the blocks inside them included.
fn ids_in(nodes: &[Node]) -> HashSet<String> {
    let mut out = HashSet::new();
    for n in nodes {
        n.walk(&mut |n| {
            if let Some(id) = n.id() {
                out.insert(id.to_string());
            }
        });
    }
    out
}

/// Put `with` in place of the block `id` in `nodes`.
fn replace_node(nodes: &mut [Node], id: &str, with: Node) {
    if let Some(node) = find_node_mut(nodes, id) {
        *node = with;
    }
}

/// `node` cut down to the way to each of the blocks `targets` inside it (or
/// `node` itself): each block with an id on the way keeps only the children
/// that lead on, and blocks without one (a table's cells) are kept whole. A
/// block in `targets` is kept whole, or, with `keep`, with only the
/// children in it. Also the ids of the blocks cut down, outermost first.
/// `None` when `node` holds none of `targets`.
fn path_copy(node: &Node, targets: &[&str], keep: Option<&HashSet<String>>) -> Option<(Node, Vec<String>)> {
    let is_target = |n: &Node| n.id().is_some_and(|id| targets.contains(&id));
    let holds = |n: &Node| {
        is_target(n) || targets.iter().any(|t| virtues_document::ops::find_node(&n.content, t).is_some())
    };
    if is_target(node) {
        let Some(keep) = keep else {
            return Some((node.clone(), vec![]));
        };
        let mut copy = node.clone();
        copy.content.retain(|c| c.id().is_none_or(|id| keep.contains(id)));
        let cut = match node.id() {
            Some(id) if copy.content.len() < node.content.len() => vec![id.to_string()],
            _ => vec![],
        };
        return Some((copy, cut));
    }
    if !holds(node) {
        return None;
    }
    let mut copy = node.clone();
    let mut cut = vec![];
    let mut content = vec![];
    for child in &node.content {
        if holds(child) {
            let (inner, inner_cut) = path_copy(child, targets, keep)?;
            content.push(inner);
            cut.extend(inner_cut);
        } else if child.id().is_none() {
            content.push(child.clone());
        }
    }
    if content.len() < node.content.len() {
        if let Some(id) = node.id() {
            cut.insert(0, id.to_string());
        }
    }
    copy.content = content;
    Some((copy, cut))
}

/// What a read says of `outer` shown cut down to `shown`, the blocks asked
/// for inside it, every one of them in the copy shown.
fn cut_down_note(outer: &str, cut: &[String], shown: &[String]) -> String {
    let inside: Vec<String> = shown.iter().filter(|id| *id != outer).cloned().collect();
    format!(
        "{} {} shown inside `{outer}`, with only the blocks on the way to {}: edit {} by id. \
         Replacing {} waits until reads have shown all of {}.",
        block_names(&inside),
        if inside.len() == 1 { "is" } else { "are" },
        them(&inside),
        them(&inside),
        cut.iter().map(|id| format!("`{id}`")).collect::<Vec<_>>().join(", "),
        if cut.len() == 1 { "it" } else { "them" },
    )
}

/// `base` with `seen` as what has been seen now, the blocks `as_html`
/// names just shown as HTML, whole: none of them is shown only as markdown
/// or in part any more, and what the base says of blocks gone from `seen`
/// goes with them.
fn carry(base: &page_reads::ReadBase, seen: Vec<Node>, as_html: &HashSet<String>) -> page_reads::ReadBase {
    let present = ids_in(&seen);
    page_reads::ReadBase {
        markdown_only: base
            .markdown_only
            .iter()
            .filter(|id| present.contains(*id) && !as_html.contains(*id))
            .cloned()
            .collect(),
        whole: base
            .whole
            .iter()
            .filter(|(id, _)| present.contains(*id) && !as_html.contains(*id))
            .map(|(id, hash)| (id.clone(), hash.clone()))
            .collect(),
        tree: seen,
    }
}

/// Say in `base` that the block `id` was shown in part, as `tree` holds it.
fn shown_in_part(base: &mut page_reads::ReadBase, tree: &[Node], id: &str) {
    if let Some(node) = virtues_document::ops::find_node(tree, id) {
        base.whole.insert(id.to_string(), page_reads::node_hash(node));
    }
}

/// The tree a batch is checked against and merged with: the base's, with a
/// block shown in part that the batch deletes taken as it stands when it
/// still hashes as it did when shown. Nothing in it changed since, so the
/// delete takes nothing a read did not have the chance to show; a replace
/// still waits for every block inside to be read ([`partly_read`]).
fn tree_for_ops(base: &page_reads::ReadBase, current: &[Node], ops: &[Op]) -> Vec<Node> {
    let mut tree = base.tree.clone();
    for op in ops {
        let Op::Delete { id } = op else { continue };
        let (Some(hash), Some(now)) = (base.whole.get(id), virtues_document::ops::find_node(current, id)) else {
            continue;
        };
        if page_reads::node_hash(now) == *hash {
            replace_node(&mut tree, id, now.clone());
        }
    }
    tree
}

/// Whether a replace of the block `id` would write a block shown only as
/// markdown in the reads `base` holds: `id` itself, or a block inside it.
/// A block around it is no matter; the replace does not write it.
fn shown_as_markdown(base: &page_reads::ReadBase, id: &str) -> bool {
    if base.markdown_only.is_empty() {
        return false;
    }
    match virtues_document::ops::find_node(&base.tree, id) {
        Some(node) => ids_in(std::slice::from_ref(node))
            .iter()
            .any(|inner| base.markdown_only.contains(inner)),
        None => base.markdown_only.contains(id),
    }
}

/// Why a replace (or a move) of the block `id` waits: a read showed it, or
/// blocks inside it, only as markdown, which does not say what its HTML is.
/// A block a read opened is read on inside from just before the first block
/// in it still shown only as markdown. Any other block's HTML comes with the
/// refusal, or, too long to come with it, is named there to be read by id.
fn markdown_only_problem(read: &page_reads::ReadBase, current: &[Node], id: &str, what: &str) -> String {
    let opened = read
        .whole
        .contains_key(id)
        .then(|| virtues_document::ops::find_node(current, id))
        .flatten()
        .and_then(|now| {
            let first = now
                .content
                .iter()
                .position(|c| c.id().is_some_and(|cid| shown_as_markdown(read, cid)))?;
            Some(first.checked_sub(1).and_then(|i| now.content[i].id()))
        });
    let then = match opened {
        Some(Some(last)) => format!(
            "read the rest of it as HTML with get_page_content, after \"{last}\" and this base, then \
             write the {what} from its HTML"
        ),
        Some(None) => format!(
            "read it as HTML with get_page_content, ids [\"{id}\"] and this base, then write the \
             {what} from its HTML"
        ),
        None => format!("write the {what} from its HTML"),
    };
    format!(
        "the block `{id}` was shown only as markdown, which does not say what its HTML is (a mention \
         reads as a link, a callout as a quote); {then}"
    )
}

/// How the crate's merge refuses a replace or a delete of a block that
/// changed since the read and could not be merged.
const CHANGED_SINCE_READ: &str = "the block changed since you read it";

/// What a refusal says of the block `id`, one a read opened
/// (`ReadBase::whole`) that changed since: where reading on inside it
/// starts, just before the first block in it the base does not hold as the
/// page has it.
/// An ids read opens such a block from its start again and keeps the blocks
/// past its first window as the earlier read showed them, so it would never
/// show a change further in, and the retry would be refused again. Also the
/// blocks the base holds inside it that are gone from the page, which the
/// refusal's base sees gone, so the read on starts where the page differs.
/// None for any other block, or when its first block changed, which an ids
/// read shows.
fn changed_in_part(read: &page_reads::ReadBase, current: &[Node], id: &str) -> Option<(String, Vec<String>)> {
    if !read.whole.contains_key(id) {
        return None;
    }
    let then = virtues_document::ops::find_node(&read.tree, id)?;
    let now = virtues_document::ops::find_node(current, id)?;
    let gone: Vec<String> = then
        .content
        .iter()
        .filter_map(Node::id)
        .filter(|inner| !now.content.iter().any(|c| c.id() == Some(*inner)))
        .map(str::to_string)
        .collect();
    let kept: Vec<&Node> = then
        .content
        .iter()
        .filter(|c| c.id().is_none_or(|inner| !gone.iter().any(|g| g == inner)))
        .collect();
    let first = (0..now.content.len().max(kept.len()))
        .find(|&i| now.content.get(i) != kept.get(i).copied())?;
    let last = first.checked_sub(1).and_then(|i| now.content[i].id())?;
    Some((
        format!(
            "{CHANGED_SINCE_READ}, further in than a read of it by id shows: read the rest of it \
             with get_page_content, after \"{last}\" and this base, then write the op from what \
             that read shows"
        ),
        gone,
    ))
}

/// No block a read showed in part ([`partly_read`]): a base with no kept read.
static NONE_IN_PART: BTreeMap<String, String> = BTreeMap::new();

/// Whether op HTML holds a link to a ref on the box (`/person/…`): what a
/// mention, shown as markdown (`[@Nick](/person/person_1)`), comes back as
/// when written from it.
fn holds_ref_link(html: &str) -> bool {
    let mut found = false;
    for n in &virtues_document::parse_html(html, "doc").nodes {
        n.walk(&mut |n| {
            found |= n.marks.iter().any(|m| {
                m.kind == "link"
                    && m.attrs
                        .get("href")
                        .and_then(|h| h.as_str())
                        .is_some_and(virtues_document::contract::is_route)
            });
        });
    }
    found
}

fn chars(text: &str) -> usize {
    text.chars().count()
}

/// The bytes `text` takes as a JSON string in a tool's result, which is
/// what the agent loop's cut counts.
fn json_bytes(text: &str) -> usize {
    serde_json::to_string(text).map_or(text.len(), |json| json.len().saturating_sub(2))
}

/// "The block `a`" or "The blocks `a`, `b`".
fn block_names(ids: &[String]) -> String {
    let named = ids
        .iter()
        .map(|id| format!("`{id}`"))
        .collect::<Vec<_>>()
        .join(", ");
    if ids.len() == 1 {
        format!("The block {named}")
    } else {
        format!("The blocks {named}")
    }
}

fn too_long_to_show(ids: &[String]) -> String {
    let verb = if ids.len() == 1 { "is" } else { "are" };
    format!("{} {verb} too long to show here", block_names(ids))
}

fn inside_too_long(ids: &[String]) -> String {
    let verb = if ids.len() == 1 { "is" } else { "are" };
    format!("{} {verb} inside a block too long to show here", block_names(ids))
}

fn them(ids: &[String]) -> &'static str {
    if ids.len() == 1 {
        "it"
    } else {
        "them"
    }
}

/// Where a long page goes on: the read that shows the rest.
fn read_on(next: &str) -> String {
    format!(
        "The page goes on past this: read on with get_page_content, after \"{next}\" and this \
         base. A block not shown is read before it is replaced."
    )
}

/// The blocks of `tree` whose ids are in `ids`, in document order, a block
/// inside another one named not listed twice.
fn in_document_order(tree: &[Node], ids: &[String]) -> Vec<Node> {
    fn walk(nodes: &[Node], ids: &HashSet<&str>, out: &mut Vec<Node>) {
        for n in nodes {
            if n.id().is_some_and(|id| ids.contains(id)) {
                out.push(n.clone());
            } else {
                walk(&n.content, ids, out);
            }
        }
    }
    let ids: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let mut out = vec![];
    walk(tree, &ids, &mut out);
    out
}

/// Blocks as markdown, each as the page's export writes it. A list item
/// is written inside a list of its kind and a row inside a table, which is
/// where markdown has them: neither has markdown of its own, and the chat's
/// diff card draws what this writes.
fn blocks_markdown(nodes: &[Node]) -> String {
    let mut wrapped: Vec<Node> = vec![];
    for n in nodes {
        match n.kind.as_str() {
            "listItem" => wrapped.push(Node::element("bulletList", Default::default(), vec![n.clone()])),
            "taskItem" => wrapped.push(Node::element("taskList", Default::default(), vec![n.clone()])),
            // Rows side by side are one table's: written as its rows, under
            // the empty header GFM asks of a table that has none.
            "tableRow" => match wrapped.last_mut() {
                Some(table) if table.kind == "table" && table.id().is_none() => table.content.push(n.clone()),
                _ => wrapped.push(Node::element("table", Default::default(), vec![n.clone()])),
            },
            _ => wrapped.push(n.clone()),
        }
    }
    if wrapped.is_empty() {
        return String::new();
    }
    virtues_document::to_markdown(&wrapped)
        .trim_end()
        .to_string()
}

/// Cut to [`EDIT_TEXT_CHARS`], with `…` where it was cut.
fn clip(text: &str) -> String {
    if text.chars().count() <= EDIT_TEXT_CHARS {
        return text.to_string();
    }
    let mut out: String = text.chars().take(EDIT_TEXT_CHARS - 1).collect();
    out.push('…');
    out
}

/// A list of strings from an array, a string holding a JSON array, or one
/// bare id.
fn string_list(value: Value) -> Result<Vec<String>, ToolError> {
    let bad = || ToolError::InvalidParameters("ids must be a list of block ids".into());
    let value = match value {
        Value::String(s) if s.trim_start().starts_with('[') => {
            serde_json::from_str::<Value>(&s).map_err(|_| bad())?
        }
        Value::String(s) if s.trim().is_empty() => return Ok(vec![]),
        Value::String(s) => return Ok(vec![s.trim().to_string()]),
        Value::Null => return Ok(vec![]),
        v => v,
    };
    let Value::Array(items) = value else {
        return Err(bad());
    };
    let mut out: Vec<String> = vec![];
    for item in items {
        let Value::String(id) = item else {
            return Err(bad());
        };
        if !out.contains(&id) {
            out.push(id);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::yjs::extract_text_content;

    /// Two chat edits to one page at once take turns: neither reads the
    /// other's change as the owner's typing, so no version credits a chat's
    /// words to the owner.
    #[sqlx::test]
    async fn two_chat_edits_at_once_take_turns(pool: PgPool) {
        let page = pages::create_page(
            &pool,
            pages::CreatePageRequest {
                title: "Notes".into(),
                content: "Coffee.\nTea.\n".into(),
                project_id: None,
                icon: None,
                icon_color: None,
                cover_url: None,
                tags: None,
                format: None,
            },
        )
        .await
        .unwrap();
        let yjs = YjsState::new(pool.clone());
        let tool = PageEditorTool::new(Arc::new(pool.clone()), Some(yjs.clone()));
        let context = ToolContext::default();

        let (a, b) = tokio::join!(
            tool.edit_page(serde_json::json!({ "page_id": page.id, "find": "Coffee.", "replace": "Black coffee." }), &context),
            tool.edit_page(serde_json::json!({ "page_id": page.id, "find": "Tea.", "replace": "Green tea." }), &context),
        );
        a.unwrap();
        b.unwrap();

        let typed: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM app_page_versions WHERE page_id = $1 AND description = $2",
        )
        .bind(&page.id)
        .bind(TYPED_BEFORE_CHAT_EDIT)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(typed, 0, "a chat edit was versioned as the owner's typing");
        assert_eq!(
            yjs.read_text(&page.id).await.unwrap(),
            "Black coffee.\nGreen tea.\n"
        );
    }

    /// A chat edit is undoable and credited: the page before it is kept as a
    /// restore point, and the result is a version labelled 'ai'. When the page
    /// already says what the latest version holds, nothing new is kept first.
    #[sqlx::test]
    async fn a_chat_edit_is_kept_before_and_credited_after(pool: PgPool) {
        let page = pages::create_page(
            &pool,
            pages::CreatePageRequest {
                title: "Notes".into(),
                content: "Your own line.\n".into(),
                project_id: None,
                icon: None,
                icon_color: None,
                cover_url: None,
                tags: None,
                format: None,
            },
        )
        .await
        .unwrap();
        let yjs = YjsState::new(pool.clone());
        let tool = PageEditorTool::new(Arc::new(pool.clone()), Some(yjs.clone()));
        let context = ToolContext::default();

        tool.edit_page(
            serde_json::json!({ "page_id": page.id, "find": "", "replace": "The chat's whole page.\n" }),
            &context,
        )
        .await
        .unwrap();
        tool.edit_page(
            serde_json::json!({ "page_id": page.id, "find": "whole", "replace": "entire" }),
            &context,
        )
        .await
        .unwrap();

        let rows: Vec<(String, Option<String>, Vec<u8>)> = sqlx::query_as(
            "SELECT created_by, description, yjs_snapshot FROM app_page_versions \
             WHERE page_id = $1 ORDER BY version_number",
        )
        .bind(&page.id)
        .fetch_all(&pool)
        .await
        .unwrap();
        let read: Vec<(bool, &str, String)> = rows
            .iter()
            .map(|(by, description, snapshot)| {
                (
                    pages::is_restore_point(by, description.as_deref()),
                    by.as_str(),
                    extract_text_content(snapshot),
                )
            })
            .collect();
        assert_eq!(
            read,
            vec![
                (true, "auto", "Your own line.\n".to_string()),
                (false, "ai", "The chat's whole page.\n".to_string()),
                (false, "ai", "The chat's entire page.\n".to_string()),
            ]
        );
    }

    /// An edit whose save fails is still an edit: the tool reports it as
    /// made (with the save pending), History has its version, and the save
    /// loop lands it later. Reporting a failure would invite the same edit
    /// twice.
    #[sqlx::test]
    async fn a_chat_edit_that_could_not_be_saved_is_still_made_once(pool: PgPool) {
        let page = pages::create_page(
            &pool,
            pages::CreatePageRequest {
                title: "Notes".into(),
                content: "Coffee.\n".into(),
                project_id: None,
                icon: None,
                icon_color: None,
                cover_url: None,
                tags: None,
                format: None,
            },
        )
        .await
        .unwrap();
        let yjs = YjsState::new(pool.clone());
        let tool = PageEditorTool::new(Arc::new(pool.clone()), Some(yjs.clone()));
        for stmt in [
            "CREATE FUNCTION refuse_page_saves() RETURNS trigger LANGUAGE plpgsql \
             AS $$ BEGIN RAISE EXCEPTION 'disk full'; END $$",
            "CREATE TRIGGER refuse_page_saves BEFORE UPDATE ON app_pages \
             FOR EACH ROW WHEN (NEW.yjs_state IS NOT NULL) EXECUTE FUNCTION refuse_page_saves()",
        ] {
            sqlx::query(stmt).execute(&pool).await.unwrap();
        }

        let result = tool
            .edit_page(
                serde_json::json!({ "page_id": page.id, "find": "Coffee.", "replace": "Coffee. Tea." }),
                &ToolContext::default(),
            )
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.data["applied"], true);
        assert_eq!(result.data["saved"], false);
        assert!(result.data["message"]
            .as_str()
            .unwrap()
            .contains("Do not make this edit again"));

        let latest: (String, Vec<u8>) = sqlx::query_as(
            "SELECT created_by, yjs_snapshot FROM app_page_versions \
             WHERE page_id = $1 ORDER BY version_number DESC LIMIT 1",
        )
        .bind(&page.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(latest.0, "ai");
        assert_eq!(extract_text_content(&latest.1), "Coffee. Tea.\n");

        sqlx::query("DROP TRIGGER refuse_page_saves ON app_pages")
            .execute(&pool)
            .await
            .unwrap();
        yjs.flush_pending_saves().await;
        let content: String = sqlx::query_scalar("SELECT content FROM app_pages WHERE id = $1")
            .bind(&page.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(content, "Coffee. Tea.\n");
    }

    async fn notes_page(pool: &PgPool, content: &str) -> String {
        pages::create_page(
            pool,
            pages::CreatePageRequest {
                title: "Notes".into(),
                content: content.into(),
                project_id: None,
                icon: None,
                icon_color: None,
                cover_url: None,
                tags: None,
                format: None,
            },
        )
        .await
        .unwrap()
        .id
    }

    /// The page's versions, oldest first, as (created_by, description, text).
    async fn versions(pool: &PgPool, page_id: &str) -> Vec<(String, Option<String>, String)> {
        let rows: Vec<(String, Option<String>, Vec<u8>)> = sqlx::query_as(
            "SELECT created_by, description, yjs_snapshot FROM app_page_versions \
             WHERE page_id = $1 ORDER BY version_number",
        )
        .bind(page_id)
        .fetch_all(pool)
        .await
        .unwrap();
        rows.into_iter()
            .map(|(by, description, snapshot)| (by, description, extract_text_content(&snapshot)))
            .collect()
    }

    /// A content edit that is on the page is reported as made when the
    /// title beside it cannot be changed: a failure would invite the edit a
    /// second time.
    #[sqlx::test]
    async fn a_title_that_could_not_change_does_not_undo_the_report_of_the_edit(pool: PgPool) {
        let page_id = notes_page(&pool, "Coffee.\n").await;
        let yjs = YjsState::new(pool.clone());
        let tool = PageEditorTool::new(Arc::new(pool.clone()), Some(yjs.clone()));
        for stmt in [
            "CREATE FUNCTION refuse_titles() RETURNS trigger LANGUAGE plpgsql \
             AS $$ BEGIN RAISE EXCEPTION 'lock timeout'; END $$",
            "CREATE TRIGGER refuse_titles BEFORE UPDATE ON app_pages \
             FOR EACH ROW WHEN (NEW.title IS DISTINCT FROM OLD.title) EXECUTE FUNCTION refuse_titles()",
        ] {
            sqlx::query(stmt).execute(&pool).await.unwrap();
        }

        let result = tool
            .edit_page(
                serde_json::json!({
                    "page_id": page_id, "find": "Coffee.", "replace": "Coffee. Tea.", "title": "Drinks",
                }),
                &ToolContext::default(),
            )
            .await
            .expect("the edit is reported, not failed");
        assert!(result.success);
        assert_eq!(result.data["applied"], true);
        assert_eq!(result.data["saved"], true);
        assert_eq!(result.data["title_changed"], false);
        let message = result.data["message"].as_str().unwrap();
        assert!(message.contains("content edit is on the page"), "{message}");
        assert!(message.contains("title did not change"), "{message}");
        assert!(message.contains("Do not make this edit again"), "{message}");

        let (content, title): (String, String) =
            sqlx::query_as("SELECT content, title FROM app_pages WHERE id = $1")
                .bind(&page_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(content, "Coffee. Tea.\n");
        assert_eq!(title, "Notes");

        // Without a content edit, a title that cannot change is the call's
        // failure: nothing was made.
        let refused = tool
            .edit_page(
                serde_json::json!({ "page_id": page_id, "find": "", "replace": "", "title": "Drinks" }),
                &ToolContext::default(),
            )
            .await;
        assert!(refused.is_err());
    }

    /// The owner types after the chat's copy of the page is kept and before
    /// its edit lands. That typing is versioned as theirs, before the chat's
    /// version, so History does not credit it to the chat.
    #[sqlx::test]
    async fn typing_that_lands_before_a_chat_edit_is_the_owners(pool: PgPool) {
        let page_id = notes_page(&pool, "Your own line.\n").await;
        let yjs = YjsState::new(pool.clone());
        let tool = PageEditorTool::new(Arc::new(pool.clone()), Some(yjs.clone()));

        // Hold the chat's copy partway: it has read the page, and its
        // version waits on this lock.
        let mut hold = pool.begin().await.unwrap();
        sqlx::query("LOCK TABLE app_page_versions IN SHARE MODE")
            .execute(&mut *hold)
            .await
            .unwrap();
        let edit = tokio::spawn({
            let (tool, page_id) = (tool.clone(), page_id.clone());
            async move {
                tool.edit_page(
                    serde_json::json!({ "page_id": page_id, "find": "Your own line.", "replace": "Your line, edited." }),
                    &ToolContext::default(),
                )
                .await
            }
        });
        pages::until_a_statement_waits(&pool, "%INSERT INTO app_page_versions%").await;

        yjs.apply_text_diff(&page_id, "Your own line.\n", "Your own line.\nTyped.\n")
            .await
            .unwrap();
        hold.rollback().await.unwrap();
        edit.await.unwrap().expect("the chat edit");

        assert_eq!(
            versions(&pool, &page_id).await,
            vec![
                (
                    "auto".to_string(),
                    Some(pages::RESTORE_POINT.to_string()),
                    "Your own line.\n".to_string()
                ),
                (
                    "auto".to_string(),
                    Some(TYPED_BEFORE_CHAT_EDIT.to_string()),
                    "Your own line.\nTyped.\n".to_string()
                ),
                (
                    "ai".to_string(),
                    Some("Edited from a chat".to_string()),
                    "Your line, edited.\nTyped.\n".to_string()
                ),
            ]
        );
    }
}

/// The three tools on block pages.
#[cfg(test)]
mod block_pages {
    use super::*;
    use crate::server::yjs::extract_text_content;

    /// A block page holding `markdown`, converted the way `create_page`
    /// converts it when the flag is on.
    async fn tree_page(pool: &PgPool, markdown: &str) -> String {
        pages::create_page_as(
            pool,
            pages::CreatePageRequest {
                title: "Trip".into(),
                content: markdown.into(),
                project_id: None,
                icon: None,
                icon_color: None,
                cover_url: None,
                tags: None,
                format: None,
            },
            PageFormat::Tree,
        )
        .await
        .unwrap()
        .page
        .id
    }

    fn tool(pool: &PgPool, yjs: &YjsState) -> PageEditorTool {
        PageEditorTool::new(Arc::new(pool.clone()), Some(yjs.clone()))
    }

    async fn read(tool: &PageEditorTool, args: Value) -> Value {
        let result = tool
            .get_page_content(args, &ToolContext::default())
            .await
            .unwrap();
        assert!(result.success);
        result.data
    }

    async fn edit(tool: &PageEditorTool, args: Value) -> Value {
        let result = tool.edit_page(args, &ToolContext::default()).await.unwrap();
        assert!(result.success, "{:?}", result.error);
        result.data
    }

    fn top_ids(tree: &[Node]) -> Vec<String> {
        tree.iter().map(|n| n.id().unwrap().to_string()).collect()
    }

    fn texts(tree: &[Node]) -> Vec<String> {
        tree.iter().map(Node::text_content).collect()
    }

    /// Someone editing the page in their editor, as the server sees it: a
    /// block rewritten with no base, so it lands as typed.
    async fn person_writes(yjs: &YjsState, page_id: &str, op: Op) {
        yjs.edit_tree(page_id, None, &[op])
            .await
            .unwrap()
            .after
            .unwrap();
    }

    async fn kept_base(pool: &PgPool, page_id: &str, base: &str) -> Option<Vec<Node>> {
        page_reads::read_base(pool, page_id, base).await.unwrap().map(|kept| kept.tree)
    }

    async fn versions(pool: &PgPool, page_id: &str) -> Vec<(String, Option<String>, String)> {
        let rows: Vec<(String, Option<String>, Vec<u8>)> = sqlx::query_as(
            "SELECT created_by, description, yjs_snapshot FROM app_page_versions \
             WHERE page_id = $1 ORDER BY version_number",
        )
        .bind(page_id)
        .fetch_all(pool)
        .await
        .unwrap();
        rows.into_iter()
            .map(|(by, description, snapshot)| (by, description, extract_text_content(&snapshot)))
            .collect()
    }

    #[sqlx::test]
    async fn a_short_block_page_reads_as_html_with_a_kept_base(pool: PgPool) {
        let page_id = tree_page(&pool, "## Plan\n\nLunch with [@Nick](/person/person_1).\n").await;
        let yjs = YjsState::new(pool.clone());
        let data = read(&tool(&pool, &yjs), json!({ "page_id": page_id })).await;

        assert_eq!(data["format"], "tree");
        assert_eq!(data["title"], "Trip");
        let tree = yjs.read_tree(&page_id).await.unwrap();
        assert_eq!(data["html"], virtues_document::to_html(&tree, true));
        assert!(data["html"].as_str().unwrap().contains("data-id="));
        assert!(data["html"].as_str().unwrap().contains(
            "<virtues-mention to=\"/person/person_1\" label=\"Nick\"></virtues-mention>"
        ));
        let base = data["base"].as_str().unwrap();
        assert_eq!(base, page_reads::tree_hash(&tree));
        assert_eq!(kept_base(&pool, &page_id, base).await, Some(tree));
        assert_eq!(data["how_to_edit"], HOW_TO_EDIT);
        assert_eq!(data["tags"], virtues_document::tag_guide());
        assert!(data.get("markdown").is_none());
    }

    #[sqlx::test]
    async fn a_long_block_page_reads_as_markdown_with_its_ids(pool: PgPool) {
        let markdown: String = (0..400)
            .map(|i| format!("Paragraph {i} of a long page about a trip to the coast.\n\n"))
            .collect();
        let page_id = tree_page(&pool, &markdown).await;
        let yjs = YjsState::new(pool.clone());
        let data = read(&tool(&pool, &yjs), json!({ "page_id": page_id })).await;

        let tree = yjs.read_tree(&page_id).await.unwrap();
        assert!(data.get("html").is_none());
        assert_eq!(
            data["markdown"],
            virtues_document::to_markdown_with_ids(&tree)
        );
        let first = tree[0].id().unwrap();
        assert!(data["markdown"]
            .as_str()
            .unwrap()
            .starts_with(&format!("<!-- {first} -->\nParagraph 0")));
        assert_eq!(data["note"], LONG_PAGE_NOTE);
        // Every block was shown, so the whole page is the base, every block
        // in it shown as markdown alone.
        let kept = page_reads::read_base(&pool, &page_id, data["base"].as_str().unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(kept.tree, tree);
        assert_eq!(kept.markdown_only, ids_in(&tree).into_iter().collect());
        assert!(data.get("more_after").is_none());
    }

    /// A page longer than one read holds is read in windows of whole blocks,
    /// each under the agent loop's cut of a result, and each base names only
    /// what was shown: a block past the window is refused until it is read,
    /// and reading on brings it into the base.
    #[sqlx::test]
    async fn a_page_past_one_read_is_read_on_and_its_base_is_what_was_shown(pool: PgPool) {
        let markdown: String = (0..1_600)
            .map(|i| format!("Paragraph {i} of a long page about a trip to the coast.\n\n"))
            .collect();
        let page_id = tree_page(&pool, &markdown).await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let last = tree.last().unwrap().id().unwrap().to_string();

        let first = read(&tool, json!({ "page_id": page_id })).await;
        let shown = first["markdown"].as_str().unwrap();
        assert!(json_bytes(shown) <= READ_MARKDOWN_BYTES);
        assert!(
            serde_json::to_string(&first).unwrap().len()
                < crate::agent::executor::MAX_TOOL_OUTPUT_BYTES
        );
        let k = tree
            .iter()
            .position(|n| !shown.contains(&format!("<!-- {} -->", n.id().unwrap())))
            .unwrap();
        assert!(k > 0 && k < tree.len(), "{k}");
        assert_eq!(shown, virtues_document::to_markdown_with_ids(&tree[..k]));
        let base1 = first["base"].as_str().unwrap().to_string();
        assert_eq!(kept_base(&pool, &page_id, &base1).await.unwrap(), tree[..k]);
        let next = tree[k - 1].id().unwrap();
        assert_eq!(first["more_after"], next);
        assert!(first["note"].as_str().unwrap().contains(&format!(
            "read on with get_page_content, after \"{next}\" and this base"
        )));

        // The owner changes the last block; a replace of it from what the
        // model never read is refused.
        person_writes(
            &yjs,
            &page_id,
            Op::Replace {
                id: last.clone(),
                html: "<p>Budget: $800, confirmed with Nick.</p>".into(),
            },
        )
        .await;
        let replace = |base: &str| {
            json!({ "page_id": page_id, "base": base, "ops": [
                { "op": "replace", "id": last, "html": "<p>Budget: $500.</p>" }
            ] })
        };
        let refused = edit(&tool, replace(&base1)).await;
        assert_eq!(refused["applied"], false);
        assert!(refused["error"]
            .as_str()
            .unwrap()
            .contains("is not in the read that base names"));

        // Reading on reaches the end, each base holding what was read.
        let mut base = base1;
        let mut after = Some(next.to_string());
        while let Some(a) = after {
            let start = tree
                .iter()
                .position(|n| n.id() == Some(a.as_str()))
                .unwrap()
                + 1;
            let more = read(
                &tool,
                json!({ "page_id": page_id, "after": a, "base": base }),
            )
            .await;
            let text = more["markdown"].as_str().unwrap();
            assert!(json_bytes(text) <= READ_MARKDOWN_BYTES);
            assert!(text.starts_with(&format!("<!-- {} -->", tree[start].id().unwrap())));
            base = more["base"].as_str().unwrap().to_string();
            after = more["more_after"].as_str().map(str::to_string);
            if after.is_none() {
                assert!(text.contains(&format!("<!-- {last} -->\nBudget: $800")));
            }
        }
        let now = yjs.read_tree(&page_id).await.unwrap();
        assert_eq!(kept_base(&pool, &page_id, &base).await, Some(now));

        // With a base that has seen the owner's text, as markdown, the
        // replace waits for the block's HTML, which the refusal brings with
        // a base that has seen it; sent again with that base, it lands.
        let refused = edit(&tool, replace(&base)).await;
        assert_eq!(refused["applied"], false, "{refused}");
        assert!(refused["error"].as_str().unwrap().contains("shown only as markdown"), "{refused}");
        assert!(refused["blocks"].as_str().unwrap().contains(&format!("data-id=\"{last}\">Budget: $800")));
        let base = refused["base"].as_str().unwrap().to_string();
        let applied = edit(&tool, replace(&base)).await;
        assert_eq!(applied["applied"], true, "{applied}");

        // Reading on from a block that is gone says to read again.
        let Err(ToolError::InvalidParameters(message)) = tool
            .get_page_content(
                json!({ "page_id": page_id, "after": "gone0000", "base": base }),
                &ToolContext::default(),
            )
            .await
        else {
            panic!("refused");
        };
        assert!(message.contains("read it again without after"), "{message}");
    }

    /// An `ids` read shows whole blocks up to what one read holds, and the
    /// blocks it had no room for stay out of its base, named to be read
    /// again.
    #[sqlx::test]
    async fn an_ids_read_past_one_read_names_what_it_did_not_show(pool: PgPool) {
        let markdown: String = (0..40)
            .map(|i| format!("{}\n\n", format!("Day {i} on the coast road. ").repeat(100)))
            .collect();
        let page_id = tree_page(&pool, &markdown).await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let first = read(&tool, json!({ "page_id": page_id })).await;
        let base = first["base"].as_str().unwrap().to_string();
        let ids = top_ids(&yjs.read_tree(&page_id).await.unwrap());

        let data = read(
            &tool,
            json!({ "page_id": page_id, "ids": ids, "base": base }),
        )
        .await;
        assert!(json_bytes(data["html"].as_str().unwrap()) <= READ_MARKDOWN_BYTES);
        let unread: Vec<String> = serde_json::from_value(data["unread"].clone()).unwrap();
        assert!(!unread.is_empty() && unread.len() < ids.len(), "{unread:?}");
        for id in &unread {
            assert!(!data["html"].as_str().unwrap().contains(id.as_str()));
        }
        assert!(data["note"]
            .as_str()
            .unwrap()
            .contains("did not fit in this read"));
        let more = read(
            &tool,
            json!({ "page_id": page_id, "ids": unread, "base": data["base"] }),
        )
        .await;
        assert!(more.get("unread").is_none());
    }

    /// Reading blocks by id shows them as they are now, and keeps a base
    /// that is the earlier read with only those blocks brought up to date:
    /// an edit someone made elsewhere stays unseen.
    #[sqlx::test]
    async fn reading_blocks_by_id_refreshes_only_those_blocks(pool: PgPool) {
        let page_id = tree_page(&pool, "Coffee.\n\nTea.\n\nJuice.\n").await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let first = read(&tool, json!({ "page_id": page_id })).await;
        let base1 = first["base"].as_str().unwrap().to_string();
        let ids = top_ids(&yjs.read_tree(&page_id).await.unwrap());

        person_writes(
            &yjs,
            &page_id,
            Op::Replace {
                id: ids[1].clone(),
                html: "<p>Green tea.</p>".into(),
            },
        )
        .await;
        person_writes(
            &yjs,
            &page_id,
            Op::Replace {
                id: ids[2].clone(),
                html: "<p>Orange juice.</p>".into(),
            },
        )
        .await;

        let data = read(
            &tool,
            json!({ "page_id": page_id, "ids": [ids[1], "gone0000"], "base": base1 }),
        )
        .await;
        assert_eq!(
            data["html"],
            format!("<p data-id=\"{}\">Green tea.</p>", ids[1])
        );
        assert_eq!(data["missing"], json!(["gone0000"]));
        let base2 = data["base"].as_str().unwrap();
        assert_ne!(base2, base1);
        let seen = kept_base(&pool, &page_id, base2).await.unwrap();
        assert_eq!(texts(&seen), ["Coffee.", "Green tea.", "Juice."]);

        // A string holding the list is read the same.
        let again = read(
            &tool,
            json!({ "page_id": page_id, "ids": format!("[\"{}\"]", ids[1]), "base": base1 }),
        )
        .await;
        assert_eq!(again["base"], base2);
    }

    #[sqlx::test]
    async fn reading_blocks_by_id_needs_a_base(pool: PgPool) {
        let page_id = tree_page(&pool, "Coffee.\n").await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let id = top_ids(&yjs.read_tree(&page_id).await.unwrap())[0].clone();
        for args in [
            json!({ "page_id": page_id, "ids": [id] }),
            json!({ "page_id": page_id, "ids": [id], "base": "0123456789abcdef" }),
        ] {
            let Err(ToolError::InvalidParameters(message)) =
                tool.get_page_content(args, &ToolContext::default()).await
            else {
                panic!("refused");
            };
            assert_eq!(message, IDS_NEED_BASE);
        }
    }

    #[sqlx::test]
    async fn the_markdown_view_reads_the_export_and_keeps_no_base(pool: PgPool) {
        let page_id = tree_page(&pool, "## Plan\n\n- [ ] Book seats\n").await;
        let yjs = YjsState::new(pool.clone());
        let data = read(
            &tool(&pool, &yjs),
            json!({ "page_id": page_id, "view": "markdown" }),
        )
        .await;
        assert_eq!(data["format"], "tree");
        assert_eq!(data["markdown"], "## Plan\n\n- [ ] Book seats\n");
        assert!(data.get("base").is_none());
        let kept: i64 =
            sqlx::query_scalar("SELECT count(*) FROM app_page_read_bases WHERE page_id = $1")
                .bind(&page_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(kept, 0);
    }

    /// The CLI reads in its own process, with no live document: the page's
    /// saved state serves, and its base is a tree the page held.
    #[sqlx::test]
    async fn without_yjs_a_block_page_reads_from_its_saved_state(pool: PgPool) {
        let page_id = tree_page(&pool, "Coffee.\n\nTea.\n").await;
        let offline = PageEditorTool::new(Arc::new(pool.clone()), None);
        let data = read(&offline, json!({ "page_id": page_id })).await;
        let tree = YjsState::new(pool.clone())
            .read_tree(&page_id)
            .await
            .unwrap();
        assert_eq!(data["html"], virtues_document::to_html(&tree, true));
        assert_eq!(data["base"], page_reads::tree_hash(&tree));

        let plain = read(&offline, json!({ "page_id": page_id, "view": "markdown" })).await;
        assert_eq!(plain["markdown"], "Coffee.\n\nTea.\n");
    }

    #[sqlx::test]
    async fn a_markdown_page_reads_as_its_text(pool: PgPool) {
        let page = pages::create_page(
            &pool,
            pages::CreatePageRequest {
                title: "Notes".into(),
                content: "Coffee.\n".into(),
                project_id: None,
                icon: None,
                icon_color: None,
                cover_url: None,
                tags: None,
                format: None,
            },
        )
        .await
        .unwrap();
        let yjs = YjsState::new(pool.clone());
        let data = read(&tool(&pool, &yjs), json!({ "page_id": page.id })).await;
        assert_eq!(data["format"], "markdown");
        assert_eq!(data["content"], "Coffee.\n");
        assert_eq!(data["content_length"], 8);
        assert!(data.get("base").is_none());
    }

    /// Every kind of op lands, all in one batch. The result shows the
    /// written blocks with their ids, a base for the next edit, and the
    /// markdown the chat's diff card draws; History has the page before and
    /// the chat's version after.
    #[sqlx::test]
    async fn a_batch_of_ops_lands_and_is_versioned(pool: PgPool) {
        let page_id = tree_page(
            &pool,
            "## Plan\n\nLunch with [@Nick](/person/person_1).\n\nOld line.\n",
        )
        .await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let base = read(&tool, json!({ "page_id": page_id })).await["base"].clone();
        let ids = top_ids(&yjs.read_tree(&page_id).await.unwrap());

        let data = edit(
            &tool,
            json!({
                "page_id": page_id,
                "base": base,
                "title": "Trip to Lisbon",
                "ops": [
                    { "op": "replace", "id": ids[1], "html": "<p>Lunch with <virtues-mention to=\"/person/person_1\" label=\"Nick\"></virtues-mention> on Friday.</p>" },
                    { "op": "insert_after", "id": ids[1], "html": "<ul data-type=\"taskList\"><li data-type=\"taskItem\" data-checked=\"false\"><p>Book seats</p></li></ul>" },
                    { "op": "delete", "id": ids[2] },
                    { "op": "append", "html": "<hr>" },
                ],
            }),
        )
        .await;

        assert_eq!(data["applied"], true);
        assert_eq!(data["saved"], true);
        assert_eq!(data["title_changed"], true);
        let tree = yjs.read_tree(&page_id).await.unwrap();
        assert_eq!(
            virtues_document::to_markdown(&tree),
            "## Plan\n\nLunch with [@Nick](/person/person_1) on Friday.\n\n- [ ] Book seats\n\n---\n"
        );
        let written: Vec<String> = top_ids(&tree)[1..].to_vec();
        assert_eq!(written[0], ids[1], "the replaced block keeps its id");
        assert_eq!(data["edit"]["blocks"], json!(written));
        assert_eq!(data["edit"]["format"], "tree");
        assert_eq!(
            data["edit"]["find"],
            "Lunch with [@Nick](/person/person_1).\n\nOld line."
        );
        assert_eq!(
            data["edit"]["replace"],
            "Lunch with [@Nick](/person/person_1) on Friday.\n\n- [ ] Book seats\n\n---"
        );
        assert_eq!(data["html"], virtues_document::to_html(&tree[1..], true));

        // The new base has seen what was written and what was deleted.
        let next = data["base"].as_str().unwrap();
        assert!(data["message"]
            .as_str()
            .unwrap()
            .contains(&format!("Use base {next}")));
        assert_eq!(kept_base(&pool, &page_id, next).await.unwrap(), tree);

        let title: String = sqlx::query_scalar("SELECT title FROM app_pages WHERE id = $1")
            .bind(&page_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(title, "Trip to Lisbon");
        assert_eq!(
            versions(&pool, &page_id).await,
            vec![
                (
                    "auto".to_string(),
                    Some(pages::RESTORE_POINT.to_string()),
                    "## Plan\n\nLunch with [@Nick](/person/person_1).\n\nOld line.\n".to_string()
                ),
                (
                    "ai".to_string(),
                    Some("Edited from a chat".to_string()),
                    virtues_document::to_markdown(&tree)
                ),
            ]
        );
    }

    /// A row edited by its id is drawn in the chat's diff card as the
    /// table's row, rows side by side as one table, never as a placeholder.
    #[sqlx::test]
    async fn a_row_edit_draws_its_rows_in_the_diff_card(pool: PgPool) {
        let page_id = tree_page(&pool, "| Day | Cost |\n| --- | ---: |\n| Fri | 40 |\n| Sat | 360 |\n| Sun | 12 |\n").await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let base = read(&tool, json!({ "page_id": page_id })).await["base"].clone();
        let rows: Vec<String> = yjs.read_tree(&page_id).await.unwrap()[0]
            .content
            .iter()
            .map(|r| r.id().unwrap().to_string())
            .collect();

        let data = edit(
            &tool,
            json!({ "page_id": page_id, "base": base, "ops": [
                { "op": "replace", "id": rows[2], "html": "<tr><td><p>Sat</p></td><td align=\"right\"><p>36</p></td></tr>" },
            ] }),
        )
        .await;
        assert_eq!(data["applied"], true, "{data}");
        assert_eq!(data["edit"]["find"], "|  |  |\n| --- | ---: |\n| Sat | 360 |");
        assert_eq!(data["edit"]["replace"], "|  |  |\n| --- | ---: |\n| Sat | 36 |");

        let data = edit(
            &tool,
            json!({ "page_id": page_id, "base": data["base"], "ops": [
                { "op": "delete", "id": rows[2] },
                { "op": "delete", "id": rows[3] },
            ] }),
        )
        .await;
        assert_eq!(data["applied"], true, "{data}");
        assert_eq!(data["edit"]["find"], "|  |  |\n| --- | ---: |\n| Sat | 36 |\n| Sun | 12 |");
        assert_eq!(data["edit"]["replace"], "");
    }

    /// The owner types after the chat's copy of the page is kept and before
    /// its block edit lands. That typing is versioned as theirs, before the
    /// chat's version.
    #[sqlx::test]
    async fn typing_that_lands_before_a_block_edit_is_the_owners(pool: PgPool) {
        let page_id = tree_page(&pool, "Your own line.\n\nAnother.\n").await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let base = read(&tool, json!({ "page_id": page_id })).await["base"].clone();
        let ids = top_ids(&yjs.read_tree(&page_id).await.unwrap());

        let mut hold = pool.begin().await.unwrap();
        sqlx::query("LOCK TABLE app_page_versions IN SHARE MODE")
            .execute(&mut *hold)
            .await
            .unwrap();
        let chat = tokio::spawn({
            let (tool, page_id, id) = (tool.clone(), page_id.clone(), ids[0].clone());
            async move {
                tool.edit_page(
                    json!({ "page_id": page_id, "base": base,
                            "ops": [{ "op": "replace", "id": id, "html": "<p>Your line, edited.</p>" }] }),
                    &ToolContext::default(),
                )
                .await
            }
        });
        pages::until_a_statement_waits(&pool, "%INSERT INTO app_page_versions%").await;
        person_writes(
            &yjs,
            &page_id,
            Op::Replace {
                id: ids[1].clone(),
                html: "<p>Another, typed.</p>".into(),
            },
        )
        .await;
        hold.rollback().await.unwrap();
        let data = chat.await.unwrap().expect("the chat edit").data;
        assert_eq!(data["applied"], true);

        assert_eq!(
            versions(&pool, &page_id).await,
            vec![
                (
                    "auto".to_string(),
                    Some(pages::RESTORE_POINT.to_string()),
                    "Your own line.\n\nAnother.\n".to_string()
                ),
                (
                    "auto".to_string(),
                    Some(TYPED_BEFORE_CHAT_EDIT.to_string()),
                    "Your own line.\n\nAnother, typed.\n".to_string()
                ),
                (
                    "ai".to_string(),
                    Some("Edited from a chat".to_string()),
                    "Your line, edited.\n\nAnother, typed.\n".to_string()
                ),
            ]
        );
    }

    /// A person's change to other words of the block the model rewrites is
    /// merged in, with a note. A change to the same words is refused: the
    /// reply carries the block as it is now and a base that has seen it, and
    /// a retry with that base lands with no read in between.
    #[sqlx::test]
    async fn an_edit_merges_or_is_refused_and_the_retry_needs_no_read(pool: PgPool) {
        let page_id = tree_page(&pool, "Lunch on Friday.\n").await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let base = read(&tool, json!({ "page_id": page_id })).await["base"].clone();
        let id = top_ids(&yjs.read_tree(&page_id).await.unwrap())[0].clone();

        person_writes(
            &yjs,
            &page_id,
            Op::Replace {
                id: id.clone(),
                html: "<p>Long lunch on Friday.</p>".into(),
            },
        )
        .await;
        let data = edit(
            &tool,
            json!({ "page_id": page_id, "base": base,
                    "ops": [{ "op": "replace", "id": id, "html": "<p>Lunch on Saturday.</p>" }] }),
        )
        .await;
        assert_eq!(data["applied"], true);
        assert!(data["notes"][0]["message"]
            .as_str()
            .unwrap()
            .contains("merged with a concurrent edit"));
        assert_eq!(
            texts(&yjs.read_tree(&page_id).await.unwrap()),
            ["Long lunch on Saturday."]
        );
        let base = data["base"].clone();

        // Both change the same word.
        person_writes(
            &yjs,
            &page_id,
            Op::Replace {
                id: id.clone(),
                html: "<p>Long lunch on late Saturday.</p>".into(),
            },
        )
        .await;
        let refused = edit(
            &tool,
            json!({ "page_id": page_id, "base": base,
                    "ops": [{ "op": "replace", "id": id, "html": "<p>Long lunch on Sunday.</p>" }] }),
        )
        .await;
        assert_eq!(refused["applied"], false);
        assert_eq!(refused["status"], "refused");
        assert!(refused["error"]
            .as_str()
            .unwrap()
            .starts_with("Nothing was written. op 1: someone is editing the same words"));
        assert_eq!(
            refused["blocks"],
            format!("<p data-id=\"{id}\">Long lunch on late Saturday.</p>")
        );
        assert_eq!(
            texts(&yjs.read_tree(&page_id).await.unwrap()),
            ["Long lunch on late Saturday."]
        );

        let retry = edit(
            &tool,
            json!({ "page_id": page_id, "base": refused["base"],
                    "ops": [{ "op": "replace", "id": id, "html": "<p>Long lunch on late Sunday.</p>" }] }),
        )
        .await;
        assert_eq!(retry["applied"], true);
        assert_eq!(
            texts(&yjs.read_tree(&page_id).await.unwrap()),
            ["Long lunch on late Sunday."]
        );
    }

    /// A block too long to show in an edit's reply is named, not left out of
    /// the base unsaid: the edit that wrote it says to read it by id, and a
    /// refusal over it says the same, where "send the batch again" with a
    /// base that has not seen it would be refused again, every time.
    #[sqlx::test]
    async fn a_block_too_long_to_show_is_named_and_read_by_id(pool: PgPool) {
        let page_id = tree_page(&pool, "Lunch on Friday.\n\nCoffee.\n").await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let base = read(&tool, json!({ "page_id": page_id })).await["base"].clone();
        let id = top_ids(&yjs.read_tree(&page_id).await.unwrap())[0].clone();
        let rest = "and the coast road after it. ".repeat(300);
        let replace = |base: &Value, html: String| {
            json!({ "page_id": page_id, "base": base,
                    "ops": [{ "op": "replace", "id": id, "html": html }] })
        };

        let wrote = edit(
            &tool,
            replace(&base, format!("<p>Lunch on Friday, {rest}</p>")),
        )
        .await;
        assert_eq!(wrote["applied"], true);
        assert_eq!(wrote["html"], "");
        assert_eq!(wrote["unread"], json!([id]));
        assert!(wrote["message"].as_str().unwrap().ends_with(&format!(
            "The block `{id}` is too long to show here; read it with get_page_content, ids and that \
             base before editing it again."
        )));
        let base = wrote["base"].clone();

        // The owner and the model change the same word.
        person_writes(
            &yjs,
            &page_id,
            Op::Replace {
                id: id.clone(),
                html: format!("<p>Lunch on Sunday, {rest}</p>"),
            },
        )
        .await;
        let refused = edit(
            &tool,
            replace(&base, format!("<p>Lunch on Saturday, {rest}</p>")),
        )
        .await;
        assert_eq!(refused["applied"], false);
        assert!(refused.get("blocks").is_none());
        assert_eq!(refused["unread"], json!([id]));
        let message = refused["message"].as_str().unwrap();
        assert!(
            message.starts_with(&format!(
                "The block `{id}` is too long to show here: read it with get_page_content, ids and \
                 base {}, then send the whole batch again",
                refused["base"].as_str().unwrap()
            )),
            "{message}"
        );

        // Read by id, the block is seen as it stands, and the edit lands.
        let seen = read(
            &tool,
            json!({ "page_id": page_id, "ids": [id], "base": refused["base"] }),
        )
        .await;
        assert!(seen["html"].as_str().unwrap().contains("Lunch on Sunday,"));
        let retry = edit(
            &tool,
            replace(&seen["base"], format!("<p>Lunch on Saturday, {rest}</p>")),
        )
        .await;
        assert_eq!(retry["applied"], true, "{retry}");
        assert!(texts(&yjs.read_tree(&page_id).await.unwrap())[0].starts_with("Lunch on Saturday,"));
    }

    /// A list whose HTML is past what one read holds is opened when read by
    /// id: its own tag around the items that fit, each with its id, and a
    /// way to read on inside it. Each item is then edited by id, however far
    /// down the list it is.
    #[sqlx::test]
    async fn a_list_too_long_for_one_read_is_opened_and_its_items_edited_by_id(pool: PgPool) {
        let page_id = tree_page(&pool, &format!("Packing list:\n\n{}", long_list(1_000))).await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let list = tree[1].id().unwrap().to_string();
        let whole = virtues_document::to_html(&tree[1..2], true);
        assert!(json_bytes(&whole) > READ_MARKDOWN_BYTES, "the list's HTML fits one read");

        let first = read(&tool, json!({ "page_id": page_id })).await;
        assert!(first["markdown"].as_str().unwrap().contains(&format!("<!-- {list} -->")));
        assert!(!first["markdown"].as_str().unwrap().contains("Item 0 "), "the list was shown");

        let opened = read(
            &tool,
            json!({ "page_id": page_id, "ids": [list], "base": first["base"] }),
        )
        .await;
        let html = opened["html"].as_str().unwrap();
        assert!(json_bytes(html) <= READ_MARKDOWN_BYTES);
        assert!(html.starts_with(&format!("<ul data-id=\"{list}\">")), "{}", &html[..80]);
        assert!(html.ends_with("</ul>"));
        assert!(opened.get("unread").is_none(), "{}", opened["note"]);
        let note = opened["note"].as_str().unwrap();
        assert!(note.contains(&format!("`{list}` is too long to show whole")), "{note}");
        let next = opened["more_after"].as_str().unwrap().to_string();
        assert!(html.contains(&format!("data-id=\"{next}\"")));
        assert!(note.contains(&format!("after \"{next}\" and this base")), "{note}");

        // The list itself, sent back as it was shown, would take the items
        // not shown with it: refused.
        let refused = refused_whole(
            &tool,
            &yjs,
            &page_id,
            json!({ "page_id": page_id, "base": opened["base"], "ops": [
                { "op": "replace", "id": list, "html": html.replace("Item 0 for", "Item 0, first, for") }
            ] }),
        )
        .await;
        let message = refused["refused"][0]["message"].as_str().unwrap();
        assert!(message.contains(&format!("`{list}` was shown only in part")), "{message}");
        assert_eq!(yjs.read_tree(&page_id).await.unwrap()[1].content.len(), 1_000);

        // An item the opened list showed.
        let item_ids: Vec<String> = tree[1]
            .content
            .iter()
            .map(|n| n.id().unwrap().to_string())
            .collect();
        let early = item_ids[3].clone();
        assert!(html.contains(&format!("data-id=\"{early}\"")));
        let wrote = edit(
            &tool,
            json!({ "page_id": page_id, "base": opened["base"], "ops": [
                { "op": "replace", "id": early, "html": "<li><p>Item 3, packed</p></li>" },
            ] }),
        )
        .await;
        assert_eq!(wrote["applied"], true, "{wrote}");

        // Reading on inside the list shows the items after, by id, as HTML.
        let on = read(
            &tool,
            json!({ "page_id": page_id, "after": next, "base": wrote["base"] }),
        )
        .await;
        assert_eq!(on["inside"], list);
        let more = on["html"].as_str().unwrap();
        assert!(json_bytes(more) <= READ_MARKDOWN_BYTES);
        let at = item_ids.iter().position(|id| *id == next).unwrap();
        assert!(more.starts_with(&format!("<li data-id=\"{}\">", item_ids[at + 1])), "{}", &more[..80]);
        let late = item_ids[at + 2].clone();
        let wrote = edit(
            &tool,
            json!({ "page_id": page_id, "base": on["base"], "ops": [
                { "op": "delete", "id": late },
            ] }),
        )
        .await;
        assert_eq!(wrote["applied"], true, "{wrote}");
        let now = yjs.read_tree(&page_id).await.unwrap();
        assert_eq!(now[1].content.len(), 999);
        assert_eq!(now[1].content[3].text_content(), "Item 3, packed");
    }

    /// A bulleted list of `n` items, its markdown and its HTML each past
    /// what one read holds.
    fn long_list(n: usize) -> String {
        (0..n)
            .map(|i| format!("- Item {i} for the trip to the coast, packed by the door the night before\n"))
            .collect()
    }

    /// The ids of the blocks directly inside the block `id`.
    fn child_ids(tree: &[Node], id: &str) -> Vec<String> {
        virtues_document::ops::find_node(tree, id)
            .unwrap()
            .content
            .iter()
            .filter_map(|n| n.id().map(str::to_string))
            .collect()
    }

    /// A block inside an opened one that is itself too long for any read
    /// is named by a comment, and only that: it is not in the base, the
    /// note does not count it as shown, and neither it nor the block
    /// around it is replaced over what no read showed.
    #[sqlx::test]
    async fn a_block_inside_an_opened_one_too_long_for_a_read_is_named_not_based(pool: PgPool) {
        let nested: String = (0..900)
            .map(|i| format!("  - Step {i} of the build, checked against the plan and signed off by the team\n"))
            .collect();
        let page_id = tree_page(
            &pool,
            &format!("Plan:\n\n- Phase one: research\n- Phase two: build\n{nested}- Phase three: ship\n"),
        )
        .await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let list = tree[1].id().unwrap().to_string();
        let items = child_ids(&tree, &list);
        assert_eq!(items.len(), 3);
        assert!(!fits_one_read(&tree[1].content[1]), "item two fits one read");

        let first = read(&tool, json!({ "page_id": page_id })).await;
        let opened = read(
            &tool,
            json!({ "page_id": page_id, "ids": [list], "base": first["base"] }),
        )
        .await;
        let html = opened["html"].as_str().unwrap();
        assert!(html.contains(&format!("<!-- {}: too long to show here", items[1])), "{html}");
        let note = opened["note"].as_str().unwrap();
        assert!(note.contains("shown with 2 of the 3 blocks inside it"), "{note}");
        let base = opened["base"].as_str().unwrap();
        let kept = kept_base(&pool, &page_id, base).await.unwrap();
        assert_eq!(child_ids(&kept, &list), [items[0].clone(), items[2].clone()]);

        for op in [
            json!({ "op": "replace", "id": list, "html": html.replace("Phase one", "Phase one, done") }),
            json!({ "op": "replace", "id": items[1], "html": "<li><p>Phase two: build and test</p></li>" }),
        ] {
            refused_whole(&tool, &yjs, &page_id, json!({ "page_id": page_id, "base": base, "ops": [op] })).await;
        }
        assert_eq!(yjs.read_tree(&page_id).await.unwrap(), tree);
    }

    /// A paragraph inside a quote too long for any read is shown as a
    /// comment. The model's replace of the quote, written from that read
    /// with a typo fixed, copied the comment, which wrote nothing: the long
    /// paragraph went, and the edit said it was on the page. A comment in
    /// op HTML is refused, and the read's note says what leaving it out does.
    #[sqlx::test]
    async fn a_block_shown_as_a_comment_is_never_written_over_by_copying_the_comment(pool: PgPool) {
        let long_line = "ridge ".repeat(14_000);
        let page_id = tree_page(&pool, &format!("Notes:\n\n> Teh log from the ridge:\n>\n> {long_line}\n")).await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let quote = tree[1].id().unwrap().to_string();
        let inner = child_ids(&tree, &quote);
        assert_eq!(inner.len(), 2);
        let first = read(&tool, json!({ "page_id": page_id })).await;
        let opened = read(&tool, json!({ "page_id": page_id, "ids": [quote], "base": first["base"] })).await;
        let html = opened["html"].as_str().unwrap();
        assert!(html.contains(&format!("<!-- {}: too long to show in any read", inner[1])), "{html}");
        let note = opened["note"].as_str().unwrap();
        assert!(note.contains("shown with 1 of the 2 blocks inside it"), "{note}");
        assert!(note.contains("a replace of"), "{note}");
        let refused = refused_whole(
            &tool,
            &yjs,
            &page_id,
            json!({ "page_id": page_id, "base": opened["base"], "ops": [
                { "op": "replace", "id": quote, "html": html.replace("Teh log", "The log") }
            ] }),
        )
        .await;
        assert!(refused["error"].as_str().unwrap().contains("HTML comment"), "{refused}");
        assert_eq!(yjs.read_tree(&page_id).await.unwrap(), tree);
    }

    /// A list read whole that a person has added an item to since is no list
    /// read in part. Told it was, with a read on past its last item to
    /// follow, the model read nothing, was told the same again, and never
    /// wrote: the change is the merge's to take or refuse, and a refusal
    /// shows the list as it is now.
    #[sqlx::test]
    async fn a_list_read_whole_that_gained_an_item_is_no_list_read_in_part(pool: PgPool) {
        let page_id = tree_page(&pool, "Packing:\n\n- one\n- two\n- three\n").await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let first = read(&tool, json!({ "page_id": page_id })).await;
        let html = first["html"].as_str().unwrap().to_string();
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let list = tree[1].id().unwrap().to_string();
        let items = child_ids(&tree, &list);
        person_writes(
            &yjs,
            &page_id,
            Op::InsertAfter { id: items[0].clone(), html: "<li><p>socks</p></li>".into() },
        )
        .await;
        let list_html = |html: &str| {
            let start = html.find("<ul").unwrap();
            html[start..html.rfind("</ul>").unwrap() + "</ul>".len()].to_string()
        };
        let mut base = first["base"].as_str().unwrap().to_string();
        let mut written = list_html(&html).replace(">three<", ">three, packed<");
        for round in 0..2 {
            let data = edit(&tool, json!({ "page_id": page_id, "base": base, "ops": [
                { "op": "replace", "id": list, "html": written }
            ] }))
            .await;
            if data["applied"] == true {
                let now = yjs.read_tree(&page_id).await.unwrap();
                assert!(texts(&now)[1].contains("three, packed"), "{now:?}");
                return;
            }
            let error = data["error"].as_str().unwrap();
            assert!(!error.contains("shown only in part"), "round {round}: {data}");
            let blocks = data["blocks"].as_str().unwrap_or_else(|| panic!("round {round}: {data}"));
            assert!(blocks.contains("socks"), "{blocks}");
            base = data["base"].as_str().unwrap().to_string();
            written = list_html(blocks).replace(">three<", ">three, packed<");
        }
        panic!("the replace never landed");
    }

    /// The opened list's refusal points the read on at the first item not
    /// read, wherever a person added it: from just before it, or with an
    /// ids read of the list when it is the first.
    #[test]
    fn a_read_on_starts_before_the_first_block_not_read() {
        let item = |id: &str| {
            let mut n = Node::element("listItem", Default::default(), vec![]);
            n.attrs.insert("id".into(), json!(id));
            n
        };
        let list = |ids: &[&str]| {
            let mut n = Node::element("bulletList", Default::default(), ids.iter().map(|i| item(i)).collect());
            n.attrs.insert("id".into(), json!("l"));
            n
        };
        let in_part = BTreeMap::from([("l".to_string(), String::new())]);
        let op = Op::Delete { id: "l".into() };
        let base = [list(&["a", "b", "c"])];
        let middle = partly_read(&op, &base, &[list(&["a", "x", "b", "c"])], &in_part).unwrap();
        assert!(middle.contains("after \"a\""), "{middle}");
        let top = partly_read(&op, &base, &[list(&["x", "a", "b", "c"])], &in_part).unwrap();
        assert!(top.contains("ids [\"l\"]"), "{top}");
        // Read whole: no read on, the merge decides.
        assert!(partly_read(&op, &base, &[list(&["a", "x", "b", "c"])], &BTreeMap::new()).is_none());
    }

    /// A long page reads as markdown, where a mention reads as a link. A
    /// replace of a block shown so waits for its HTML; so does a move of it,
    /// deleted and written again elsewhere in one batch, which would have
    /// turned the mention into a link.
    #[sqlx::test]
    async fn a_block_shown_only_as_markdown_is_not_moved_from_its_markdown(pool: PgPool) {
        let days: String = (0..400)
            .map(|i| format!("Day {i} on the coast road, walked from the harbour to the lighthouse.\n\n"))
            .collect();
        let page_id = tree_page(&pool, &format!("Intro with [@Nick](/person/person_1) about the trip.\n\n{days}")).await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let first = read(&tool, json!({ "page_id": page_id })).await;
        assert!(first["markdown"].is_string(), "the long page reads as markdown");
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let intro = tree[0].id().unwrap().to_string();
        let last = tree.last().unwrap().id().unwrap().to_string();
        let refused = refused_whole(
            &tool,
            &yjs,
            &page_id,
            json!({ "page_id": page_id, "base": first["base"], "ops": [
                { "op": "delete", "id": intro },
                { "op": "insert_after", "id": last, "html": "<p>Intro with <a href=\"/person/person_1\">@Nick</a> about the trip.</p>" }
            ] }),
        )
        .await;
        assert!(refused["error"].as_str().unwrap().contains("write the move from its HTML"), "{refused}");
        // A delete alone writes nothing back, and stands.
        let deleted = edit(&tool, json!({ "page_id": page_id, "base": first["base"], "ops": [
            { "op": "delete", "id": intro }
        ] }))
        .await;
        assert_eq!(deleted["applied"], true, "{deleted}");
    }

    /// A replace refused because the base holds a list only in part says to
    /// read on inside it, and keeps what was read: following the refusal
    /// moves on, it does not start the list again.
    #[sqlx::test]
    async fn a_list_read_in_part_is_read_on_from_where_the_reading_stopped(pool: PgPool) {
        let page_id = tree_page(&pool, &format!("Packing list:\n\n{}", long_list(2_000))).await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let list = tree[1].id().unwrap().to_string();
        let first = read(&tool, json!({ "page_id": page_id })).await;
        let opened = read(&tool, json!({ "page_id": page_id, "ids": [list], "base": first["base"] })).await;
        let on = read(
            &tool,
            json!({ "page_id": page_id, "after": opened["more_after"], "base": opened["base"] }),
        )
        .await;
        let base = on["base"].as_str().unwrap().to_string();
        let read_so_far = child_ids(&kept_base(&pool, &page_id, &base).await.unwrap(), &list);
        let last = read_so_far.last().unwrap().clone();
        assert_eq!(on["more_after"], last.as_str());

        let refused = refused_whole(
            &tool,
            &yjs,
            &page_id,
            json!({ "page_id": page_id, "base": base, "ops": [
                { "op": "replace", "id": list, "html": "<ul><li><p>Passport</p></li></ul>" }
            ] }),
        )
        .await;
        let message = refused["refused"][0]["message"].as_str().unwrap();
        assert!(message.contains(&format!("after \"{last}\" and this base")), "{message}");
        assert!(refused.get("unread").is_none_or(|u| !u.to_string().contains(&list)), "{refused}");
        assert!(!refused["message"].as_str().unwrap().contains("ids and base"), "{refused}");
        let kept = refused["base"].as_str().unwrap().to_string();
        assert_eq!(child_ids(&kept_base(&pool, &page_id, &kept).await.unwrap(), &list), read_so_far);

        // Reading the list by id again keeps what the base had read of it.
        let again = read(&tool, json!({ "page_id": page_id, "ids": [list], "base": kept })).await;
        let held = child_ids(&kept_base(&pool, &page_id, again["base"].as_str().unwrap()).await.unwrap(), &list);
        assert_eq!(held, read_so_far);
    }

    /// Deleting a list a read showed in part stands while nothing in it
    /// changed since the read: the read had the chance to show it all. A
    /// change to an item no read showed refuses it.
    #[sqlx::test]
    async fn deleting_a_list_shown_in_part_stands_while_nothing_in_it_changed(pool: PgPool) {
        let page_id = tree_page(&pool, &format!("Packing list:\n\n{}\nDone.\n", long_list(1_000))).await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let list = tree[1].id().unwrap().to_string();
        let late = tree[1].content[990].content[0].id().unwrap().to_string();
        let first = read(&tool, json!({ "page_id": page_id })).await;
        let opened = read(&tool, json!({ "page_id": page_id, "ids": [list], "base": first["base"] })).await;
        assert!(!opened["html"].as_str().unwrap().contains(&late));
        let delete = json!({ "page_id": page_id, "base": opened["base"], "ops": [{ "op": "delete", "id": list }] });

        person_writes(&yjs, &page_id, Op::Replace { id: late.clone(), html: "<p>Item 990, the charger</p>".into() })
            .await;
        let refused = refused_whole(&tool, &yjs, &page_id, delete.clone()).await;
        let message = refused["refused"][0]["message"].as_str().unwrap();
        assert!(message.contains(&format!("`{list}` was shown only in part")), "{message}");

        // Read again, the list is deleted whole.
        let opened = read(&tool, json!({ "page_id": page_id, "ids": [list], "base": first["base"] })).await;
        let deleted = edit(
            &tool,
            json!({ "page_id": page_id, "base": opened["base"], "ops": [{ "op": "delete", "id": list }] }),
        )
        .await;
        assert_eq!(deleted["applied"], true, "{deleted}");
        assert_eq!(texts(&yjs.read_tree(&page_id).await.unwrap()), ["Packing list:", "Done."]);
    }

    /// A list read whole across reads, then changed by a person further in
    /// than an ids read of it shows: the refusal of a replace says where
    /// reading on inside it starts. An ids read of it opens it from its
    /// start and keeps the items past its first window as the base had
    /// them, so following a refusal that named it for one was refused again
    /// and again. Read on from where it says, the replace lands.
    #[sqlx::test]
    async fn a_list_changed_further_in_than_an_ids_read_shows_says_where_to_read_on(pool: PgPool) {
        let page_id = tree_page(&pool, &format!("Packing list:\n\n{}\nDone.\n", long_list(1_000))).await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let list = tree[1].id().unwrap().to_string();
        let items = child_ids(&tree, &list);
        let read_through = |mut on: Value| {
            let tool = &tool;
            let page_id = &page_id;
            async move {
                while let Some(after) = on["more_after"].as_str().map(str::to_string) {
                    on = read(tool, json!({ "page_id": page_id, "after": after, "base": on["base"] })).await;
                }
                on["base"].clone()
            }
        };
        let first = read(&tool, json!({ "page_id": page_id })).await;
        let opened = read(&tool, json!({ "page_id": page_id, "ids": [list], "base": first["base"] })).await;
        assert!(opened["more_after"].is_string(), "{opened}");
        let base = read_through(opened).await;

        person_writes(&yjs, &page_id, Op::Replace { id: items[900].clone(), html: "<li><p>Item 900, the charger</p></li>".into() })
            .await;
        let replace = json!({ "op": "replace", "id": list, "html": "<ul><li><p>Passport</p></li></ul>" });
        let refused = refused_whole(
            &tool,
            &yjs,
            &page_id,
            json!({ "page_id": page_id, "base": base, "ops": [replace] }),
        )
        .await;
        let message = refused["refused"][0]["message"].as_str().unwrap();
        assert!(message.contains(&format!("after \"{}\" and this base", items[899])), "{message}");
        assert!(refused.get("unread").is_none(), "{refused}");
        assert!(
            refused["message"].as_str().unwrap().starts_with("Read on where the problems say"),
            "{}",
            refused["message"]
        );

        // Done as it says.
        let on = read(&tool, json!({ "page_id": page_id, "after": items[899], "base": refused["base"] })).await;
        let base = read_through(on).await;
        let wrote = edit(&tool, json!({ "page_id": page_id, "base": base, "ops": [replace] })).await;
        assert_eq!(wrote["applied"], true, "{wrote}");
        assert_eq!(texts(&yjs.read_tree(&page_id).await.unwrap()), ["Packing list:", "Passport", "Done."]);
    }

    /// A list shown in part, deleted and written again in one edit from what
    /// was shown: the rewrite puts back only the items a read showed, and
    /// the rest went with no word to anyone. Refused, as a replace of it is,
    /// until the rest has been read; and the opened read never offers the
    /// delete as the way past a refused replace.
    #[sqlx::test]
    async fn a_list_shown_in_part_is_not_deleted_and_written_again_in_one_edit(pool: PgPool) {
        let page_id = tree_page(&pool, &format!("Packing list:\n\n{}\nDone.\n", long_list(1_000))).await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let list = tree[1].id().unwrap().to_string();
        let done = tree[2].id().unwrap().to_string();
        let first = read(&tool, json!({ "page_id": page_id })).await;
        let opened = read(&tool, json!({ "page_id": page_id, "ids": [list], "base": first["base"] })).await;
        let note = opened["note"].as_str().unwrap_or_default();
        assert!(note.contains("deleting it and writing it again"), "{opened}");
        assert!(!note.contains("deleting it stands"), "{note}");

        let shown = opened["html"].as_str().unwrap().split("<!--").next().unwrap().to_string();
        for written in [
            json!({ "op": "insert_before", "id": done, "html": shown }),
            json!({ "op": "append", "html": "<ul><li><p>Item 0, rewritten</p></li></ul>" }),
        ] {
            let refused = refused_whole(
                &tool,
                &yjs,
                &page_id,
                json!({ "page_id": page_id, "base": opened["base"], "ops": [{ "op": "delete", "id": list }, written] }),
            )
            .await;
            let message = refused["refused"][0]["message"].as_str().unwrap();
            assert!(message.contains(&format!("deletes `{list}` and writes blocks too")), "{message}");
            assert!(message.contains(&format!("`{list}` was shown only in part")), "{message}");
        }
        assert_eq!(child_ids(&yjs.read_tree(&page_id).await.unwrap(), &list).len(), 1_000);
    }

    /// Several items of one long list the base lacks, asked for in one
    /// `ids` read, are shown together in the list cut down to the way to
    /// each: the read showed the first of them alone while saying it showed
    /// every one, and the edit it invited was refused.
    #[sqlx::test]
    async fn several_blocks_deep_in_one_long_list_are_read_together(pool: PgPool) {
        let page_id = tree_page(&pool, &format!("Packing list:\n\n{}", long_list(1_000))).await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let list = tree[1].id().unwrap().to_string();
        let items = child_ids(&tree, &list);
        let first = read(&tool, json!({ "page_id": page_id })).await;
        let asked = [items[5].clone(), items[400].clone(), items[650].clone()];
        let deep = read(&tool, json!({ "page_id": page_id, "ids": asked, "base": first["base"] })).await;
        let html = deep["html"].as_str().unwrap();
        for id in &asked {
            assert!(html.contains(&format!("data-id=\"{id}\"")), "{id} not shown: {html}");
        }
        assert!(!html.contains(&format!("data-id=\"{}\"", items[6])), "{html}");
        assert_eq!(html.matches("<ul").count(), 1, "one list, cut down: {html}");
        assert!(deep.get("unread").is_none(), "{deep}");
        let note = deep["note"].as_str().unwrap();
        assert!(!note.contains("did not fit"), "{note}");
        assert!(note.contains(&format!("shown inside `{list}`")), "{note}");
        for (i, id) in [(400, &asked[1]), (650, &asked[2])] {
            let wrote = edit(
                &tool,
                json!({ "page_id": page_id, "base": deep["base"], "ops": [
                    { "op": "replace", "id": id, "html": format!("<li><p>Item {i}, packed</p></li>") }
                ] }),
            )
            .await;
            assert_eq!(wrote["applied"], true, "{wrote}");
        }
    }

    /// A refused edit of an item in a long list the base lacks names the
    /// item to read, not the list: an ids read of the list opens it from its
    /// start, short of the item, and the edit was refused a second time.
    #[sqlx::test]
    async fn a_refusal_for_an_item_in_a_long_list_names_the_item_to_read(pool: PgPool) {
        let page_id = tree_page(&pool, &format!("Packing list:\n\n{}", long_list(1_000))).await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let list = tree[1].id().unwrap().to_string();
        let item = child_ids(&tree, &list)[600].clone();
        let first = read(&tool, json!({ "page_id": page_id })).await;
        let replace = json!({ "op": "replace", "id": item, "html": "<li><p>Item 600, the charger</p></li>" });
        let refused = refused_whole(
            &tool,
            &yjs,
            &page_id,
            json!({ "page_id": page_id, "base": first["base"], "ops": [replace] }),
        )
        .await;
        assert_eq!(refused["unread"], json!([item]), "{refused}");
        let message = refused["message"].as_str().unwrap();
        assert!(message.contains(&format!("`{item}` is inside a block too long to show here")), "{message}");
        assert!(!message.contains(&format!("`{list}`")), "{message}");

        let shown = read(&tool, json!({ "page_id": page_id, "ids": [item], "base": refused["base"] })).await;
        assert!(shown["html"].as_str().unwrap().contains(&format!("data-id=\"{item}\"")), "{shown}");
        let wrote = edit(&tool, json!({ "page_id": page_id, "base": shown["base"], "ops": [replace] })).await;
        assert_eq!(wrote["applied"], true, "{wrote}");
    }

    /// An `ids` read of a block deep inside a list the base lacks shows
    /// that block, inside its list cut down to the way to it, not the list's
    /// first items; reading on inside a list the base lacks puts what it
    /// shows in the base. Either way the block is then edited by id.
    #[sqlx::test]
    async fn a_block_deep_in_a_long_list_is_read_and_edited_by_id(pool: PgPool) {
        let page_id = tree_page(&pool, &format!("Packing list:\n\n{}", long_list(1_000))).await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let list = tree[1].id().unwrap().to_string();
        let items = child_ids(&tree, &list);
        let first = read(&tool, json!({ "page_id": page_id })).await;

        let deep = read(&tool, json!({ "page_id": page_id, "ids": [items[800]], "base": first["base"] })).await;
        let html = deep["html"].as_str().unwrap();
        assert!(html.contains(&format!("data-id=\"{}\"", items[800])), "{html}");
        assert!(!html.contains(&format!("data-id=\"{}\"", items[0])), "{html}");
        assert!(deep.get("unread").is_none(), "{deep}");
        let wrote = edit(
            &tool,
            json!({ "page_id": page_id, "base": deep["base"], "ops": [
                { "op": "replace", "id": items[800], "html": "<li><p>Item 800, packed</p></li>" }
            ] }),
        )
        .await;
        assert_eq!(wrote["applied"], true, "{wrote}");

        let on = read(&tool, json!({ "page_id": page_id, "after": items[600], "base": first["base"] })).await;
        assert!(on["html"].as_str().unwrap().contains(&format!("data-id=\"{}\"", items[700])));
        assert_ne!(on["base"], first["base"], "reading on put nothing in the base");
        let wrote = edit(
            &tool,
            json!({ "page_id": page_id, "base": on["base"], "ops": [
                { "op": "replace", "id": items[700], "html": "<li><p>Item 700, packed</p></li>" }
            ] }),
        )
        .await;
        assert_eq!(wrote["applied"], true, "{wrote}");
        let now = yjs.read_tree(&page_id).await.unwrap();
        assert_eq!(now[1].content[800].text_content(), "Item 800, packed");
        assert_eq!(now[1].content[700].text_content(), "Item 700, packed");
    }

    /// A table's rows carry ids, so a table too long for one read opens as a
    /// list does: its own tag around the rows that fit, each by id, and a
    /// way to read on. A cell is edited, and a row added or taken out, by
    /// id, however far down the table it is.
    #[sqlx::test]
    async fn a_table_too_long_for_one_read_opens_by_its_rows(pool: PgPool) {
        let rows: String = (0..300)
            .map(|i| format!("| Oct {i} | Taxi to the coast | 40 | Card | Nick |\n"))
            .collect();
        let page_id = tree_page(
            &pool,
            &format!("Expenses:\n\n| Day | What | Cost | Paid by | With |\n| --- | --- | --- | --- | --- |\n{rows}"),
        )
        .await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let table = tree[1].id().unwrap().to_string();
        assert!(tree[1].content.iter().all(|r| r.id().is_some()), "every row has an id");
        assert!(!fits_one_read(&tree[1]), "the table's HTML fits one read");

        let first = read(&tool, json!({ "page_id": page_id })).await;
        let opened = read(
            &tool,
            json!({ "page_id": page_id, "ids": [table], "base": first["base"] }),
        )
        .await;
        let html = opened["html"].as_str().unwrap();
        assert!(html.starts_with(&format!("<table data-id=\"{table}\"><tbody><tr data-id=")), "{}", &html[..120]);
        assert!(html.ends_with("</tbody></table>"), "{}", &html[html.len() - 80..]);
        assert!(!opened["note"].as_str().unwrap().contains("any read"), "{}", opened["note"]);
        let next = opened["more_after"].as_str().unwrap().to_string();

        // A cell of a row the read showed, and a row added after it.
        let row = &tree[1].content[5];
        let cell = row.content[2].content[0].id().unwrap().to_string();
        let wrote = edit(
            &tool,
            json!({ "page_id": page_id, "base": opened["base"], "ops": [
                { "op": "replace", "id": cell, "html": "<p>45</p>" },
                { "op": "insert_after", "id": row.id().unwrap(), "html":
                    "<tr><td><p>Oct 4b</p></td><td><p>Tram</p></td><td><p>3</p></td><td><p>Cash</p></td><td><p>Nick</p></td></tr>" },
            ] }),
        )
        .await;
        assert_eq!(wrote["applied"], true, "{wrote}");
        let now = yjs.read_tree(&page_id).await.unwrap();
        assert_eq!(now[1].content.len(), 302);
        assert_eq!(now[1].content[5].content[2].text_content(), "45");
        assert_eq!(now[1].content[6].content[0].text_content(), "Oct 4b");

        // Reading on inside the table shows the rows after, by id.
        let on = read(&tool, json!({ "page_id": page_id, "after": next, "base": wrote["base"] })).await;
        assert_eq!(on["inside"], table);
        assert!(on["html"].as_str().unwrap().starts_with("<tr data-id="));
    }

    /// A block too long for any read is listed by id, and the read's base
    /// holds it as it stands: it can be deleted, or replaced whole, with no
    /// read that cannot show it. An `ids` read of it says so rather than
    /// sending the model to read it again.
    #[sqlx::test]
    async fn a_block_too_long_for_any_read_is_deleted_or_replaced_whole(pool: PgPool) {
        let log: String = (0..2_500)
            .map(|i| format!("12:{:02}:{:02} GET /api/pages 200 4ms\n", i / 60 % 60, i % 60))
            .collect();
        let items: String = (0..1_600)
            .map(|i| format!("- Item {i} for the trip to the coast, packed by Nick\n"))
            .collect();
        let page_id = tree_page(
            &pool,
            &format!("Before.\n\n```\n{log}```\n\nBetween.\n\n{items}\nAfter.\n"),
        )
        .await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let (code, list) = (tree[1].id().unwrap().to_string(), tree[3].id().unwrap().to_string());
        for block in [&tree[1], &tree[3]] {
            let md = virtues_document::to_markdown_with_ids(std::slice::from_ref(block));
            assert!(json_bytes(&md) > READ_MARKDOWN_BYTES, "the block's markdown fits one read");
        }

        let first = read(&tool, json!({ "page_id": page_id })).await;
        let shown = first["markdown"].as_str().unwrap();
        assert!(shown.contains(&format!(
            "<!-- {code} -->\n<!-- too long to show in any read; delete it or replace it whole -->"
        )), "{shown}");
        assert!(shown.contains(&format!("<!-- {list} -->\n<!-- too long to show here; read it")), "{shown}");

        // Read by id, the code block is said to be too long, and is not
        // left to be read again.
        let by_id = read(
            &tool,
            json!({ "page_id": page_id, "ids": [code], "base": first["base"] }),
        )
        .await;
        assert!(by_id.get("unread").is_none());
        assert!(by_id["note"].as_str().unwrap().contains("too long to show in any read"));

        // The list, whose items a read shows, is not in that base: deleting
        // it unread would take items nobody read with it.
        let unread = refused_whole(
            &tool,
            &yjs,
            &page_id,
            json!({ "page_id": page_id, "base": first["base"], "ops": [{ "op": "delete", "id": list }] }),
        )
        .await;
        assert!(
            unread["refused"][0]["message"].as_str().unwrap().contains("is not in the read that base names"),
            "{unread}"
        );

        // The code block is written over from the full read's base.
        let wrote = edit(
            &tool,
            json!({ "page_id": page_id, "base": first["base"], "ops": [
                { "op": "replace", "id": code, "html": "<p>The server log, cut.</p>" },
            ] }),
        )
        .await;
        assert_eq!(wrote["applied"], true, "{wrote}");
        let now = texts(&yjs.read_tree(&page_id).await.unwrap());
        assert_eq!(now[..3], ["Before.", "The server log, cut.", "Between."]);

        // A refusal over such a block that has not changed asks for no read
        // of it: the base already holds it as it stands.
        let only = tree_page(&pool, &format!("```\n{log}```\n")).await;
        let first = read(&tool, json!({ "page_id": only })).await;
        let block = top_ids(&yjs.read_tree(&only).await.unwrap())[0].clone();
        let refused = refused_whole(
            &tool,
            &yjs,
            &only,
            json!({ "page_id": only, "base": first["base"], "ops": [{ "op": "delete", "id": block }] }),
        )
        .await;
        assert!(refused.get("unread").is_none(), "{refused}");
        assert!(!refused["message"].as_str().unwrap().contains("read"), "{}", refused["message"]);
    }

    /// A refusal says what to change. Ops run in order, so one naming a block
    /// an earlier op removed is told which; an insert anchored on a block a
    /// person deleted is told to anchor elsewhere, not to leave its text out.
    #[sqlx::test]
    async fn a_refusal_says_what_to_change(pool: PgPool) {
        let page_id = tree_page(&pool, "Coffee at nine.\n\nLunch with Nick.\n\nTea.\n").await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let base = read(&tool, json!({ "page_id": page_id })).await["base"].clone();
        let ids = top_ids(&yjs.read_tree(&page_id).await.unwrap());

        let in_order = refused_whole(
            &tool,
            &yjs,
            &page_id,
            json!({ "page_id": page_id, "base": base, "ops": [
                { "op": "delete", "id": ids[0] },
                { "op": "insert_after", "id": ids[0], "html": "<p>Breakfast.</p>" },
            ] }),
        )
        .await;
        assert_eq!(
            in_order["refused"],
            json!([{ "at": "op 2", "message": format!(
                "op 1 removes the block `{}` before this op runs, and ops run in order; put this op \
                 before op 1, or anchor it on another block",
                ids[0]
            ) }])
        );
        assert!(in_order["message"]
            .as_str()
            .unwrap()
            .starts_with("Change the refused ops as the problems say"));
        let swapped = edit(
            &tool,
            json!({ "page_id": page_id, "base": in_order["base"], "ops": [
                { "op": "insert_after", "id": ids[0], "html": "<p>Breakfast.</p>" },
                { "op": "delete", "id": ids[0] },
            ] }),
        )
        .await;
        assert_eq!(swapped["applied"], true, "{swapped}");

        person_writes(&yjs, &page_id, Op::Delete { id: ids[2].clone() }).await;
        let anchored = refused_whole(
            &tool,
            &yjs,
            &page_id,
            json!({ "page_id": page_id, "base": swapped["base"], "ops": [
                { "op": "insert_after", "id": ids[2], "html": "<p>Dinner.</p>" },
            ] }),
        )
        .await;
        assert_eq!(
            anchored["refused"][0]["message"],
            format!(
                "the block `{}` was deleted after you read the page; anchor this op on another \
                 block, or append it",
                ids[2]
            )
        );
    }

    /// A problem with the page or the batch as a whole is not answered with
    /// "send it again": that would be refused the same way.
    #[test]
    fn a_refusal_of_the_page_or_the_batch_says_what_to_change() {
        let page = refused(
            vec![Problem::new(
                "page",
                "a newer version of Virtues wrote this page; update your server to edit it",
            )],
            Some("3e61765aa88cf9b8".into()),
            String::new(),
            Unread::default(),
            false,
        );
        let message = page.data["message"].as_str().unwrap();
        assert!(message.contains("can't edit this page until it is updated"), "{message}");
        assert!(!message.contains("send the whole batch again"), "{message}");

        let ops = refused(
            vec![Problem::new(
                "ops",
                "the ops hold 5000000 bytes of HTML; one write holds at most 4 MiB",
            )],
            Some("3e61765aa88cf9b8".into()),
            String::new(),
            Unread::default(),
            false,
        );
        let message = ops.data["message"].as_str().unwrap();
        assert!(message.contains("split it into smaller edits"), "{message}");
        assert!(!message.contains("with base"), "{message}");
    }

    /// The base an edit returns is what the model has seen, not the page
    /// after its write: a person's word typed into another block before
    /// the first edit is still checked when a second edit, chained on that
    /// base, rewrites that block from the model's stale copy.
    #[sqlx::test]
    async fn a_chained_edit_keeps_a_word_typed_before_the_first(pool: PgPool) {
        let page_id = tree_page(&pool, "Plan the trip.\n\nCoffee at nine.\n").await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let base = read(&tool, json!({ "page_id": page_id })).await["base"].clone();
        let ids = top_ids(&yjs.read_tree(&page_id).await.unwrap());

        person_writes(
            &yjs,
            &page_id,
            Op::Replace {
                id: ids[1].clone(),
                html: "<p>Coffee at nine, black.</p>".into(),
            },
        )
        .await;
        let first = edit(
            &tool,
            json!({ "page_id": page_id, "base": base,
                    "ops": [{ "op": "replace", "id": ids[0], "html": "<p>Plan the trip to Lisbon.</p>" }] }),
        )
        .await;
        let second = edit(
            &tool,
            json!({ "page_id": page_id, "base": first["base"],
                    "ops": [{ "op": "replace", "id": ids[1], "html": "<p>Tea at nine.</p>" }] }),
        )
        .await;
        assert_eq!(second["applied"], true);
        assert_eq!(
            texts(&yjs.read_tree(&page_id).await.unwrap()),
            ["Plan the trip to Lisbon.", "Tea at nine, black."]
        );
    }

    /// The refused result for one batch, and the page untouched by it.
    async fn refused_whole(
        tool: &PageEditorTool,
        yjs: &YjsState,
        page_id: &str,
        args: Value,
    ) -> Value {
        let before = yjs.read_tree(page_id).await.unwrap();
        let data = edit(tool, args).await;
        assert_eq!(data["applied"], false, "{data}");
        assert_eq!(data["status"], "refused");
        assert_eq!(
            yjs.read_tree(page_id).await.unwrap(),
            before,
            "a refused batch wrote"
        );
        data
    }

    #[sqlx::test]
    async fn a_refused_batch_writes_nothing_and_says_why(pool: PgPool) {
        let page_id = tree_page(&pool, "Coffee.\n\nTea.\n").await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let base = read(&tool, json!({ "page_id": page_id })).await["base"].clone();
        let ids = top_ids(&yjs.read_tree(&page_id).await.unwrap());
        let refused = |ops: Value| json!({ "page_id": page_id, "base": base, "ops": ops });

        // Outside the contract: the tag guide comes with the refusal.
        let div = refused_whole(
            &tool,
            &yjs,
            &page_id,
            refused(json!([
                { "op": "append", "html": "<p>Fine.</p>" },
                { "op": "replace", "id": ids[0], "html": "<div>Coffee.</div>" },
            ])),
        )
        .await;
        assert!(div["error"].as_str().unwrap().contains("op 2"), "{div}");
        assert!(div["error"].as_str().unwrap().contains("<div>"), "{div}");
        assert_eq!(div["tags"], virtues_document::tag_guide());
        assert_eq!(
            div["blocks"],
            format!("<p data-id=\"{}\">Coffee.</p>", ids[0])
        );

        // A block where its list cannot hold one: what the list holds is a
        // matter of tags, so the guide comes with that refusal too.
        let tasks = tree_page(&pool, "- [ ] Book seats\n- [ ] Pack\n").await;
        let task_base = read(&tool, json!({ "page_id": tasks })).await["base"].clone();
        let list = yjs.read_tree(&tasks).await.unwrap()[0].clone();
        let item = list.content[0].id().unwrap().to_string();
        let held = refused_whole(
            &tool,
            &yjs,
            &tasks,
            json!({ "page_id": tasks, "base": task_base, "ops": [
                { "op": "insert_after", "id": item, "html": "<li><p>Call Nick</p></li>" },
            ] }),
        )
        .await;
        assert!(held["error"].as_str().unwrap().contains("holds `taskItem+`"), "{held}");
        assert_eq!(held["tags"], virtues_document::tag_guide());

        // An id no block has had.
        let unknown = refused_whole(
            &tool,
            &yjs,
            &page_id,
            refused(json!([
                { "op": "replace", "id": "zz81aa00", "html": "<p>Juice.</p>" },
            ])),
        )
        .await;
        assert_eq!(
            unknown["refused"][0]["message"],
            "no block has id `zz81aa00`"
        );

        // Markdown where HTML belongs, and a proposal.
        let markdown = refused_whole(
            &tool,
            &yjs,
            &page_id,
            refused(json!([
                { "op": "replace", "id": ids[0], "html": "**Coffee**, black." },
                { "op": "append", "html": "<p><virtues-ins proposal=\"1\">Tea.</virtues-ins></p>" },
            ])),
        )
        .await;
        assert_eq!(
            markdown["refused"],
            json!([
                { "at": "op 1", "message": "`html` must be HTML blocks such as <p>…</p>; this looks like markdown" },
                { "at": "op 2", "message": "proposals are made in the page by its owner; write the text as it should read" },
            ])
        );

        // A block someone deleted after the read.
        person_writes(&yjs, &page_id, Op::Delete { id: ids[1].clone() }).await;
        let deleted = refused_whole(
            &tool,
            &yjs,
            &page_id,
            refused(json!([
                { "op": "replace", "id": ids[1], "html": "<p>Green tea.</p>" },
            ])),
        )
        .await;
        assert_eq!(
            deleted["refused"][0]["message"],
            format!(
                "the block `{}` was deleted after you read the page; leave it out unless you were asked to bring it back",
                ids[1]
            )
        );
        // Its base has seen the deletion.
        let seen = kept_base(&pool, &page_id, deleted["base"].as_str().unwrap())
            .await
            .unwrap();
        assert_eq!(texts(&seen), ["Coffee."]);
    }

    #[sqlx::test]
    async fn a_replace_or_delete_needs_a_base_and_an_append_does_not(pool: PgPool) {
        let page_id = tree_page(&pool, "Coffee.\n\nTea.\n").await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let ids = top_ids(&yjs.read_tree(&page_id).await.unwrap());

        let none = refused_whole(
            &tool,
            &yjs,
            &page_id,
            json!({ "page_id": page_id, "ops": [
            { "op": "append", "html": "<p>Juice.</p>" },
            { "op": "delete", "id": ids[1] },
        ] }),
        )
        .await;
        assert_eq!(none["refused"][0]["at"], "op 2");
        assert!(none["refused"][0]["message"]
            .as_str()
            .unwrap()
            .starts_with("delete needs the base"));
        assert!(none.get("base").is_none());

        let appended = edit(
            &tool,
            json!({ "page_id": page_id,
            "ops": [{ "op": "append", "html": "<p>Juice.</p>" }] }),
        )
        .await;
        assert_eq!(appended["applied"], true);
        assert!(appended.get("base").is_none());
        assert!(appended["message"]
            .as_str()
            .unwrap()
            .ends_with("Read the page before replacing or deleting."));
        assert_eq!(
            texts(&yjs.read_tree(&page_id).await.unwrap()),
            ["Coffee.", "Tea.", "Juice."]
        );
    }

    /// A base the server no longer keeps still serves while the page is as
    /// it named; once the page has changed, a replace is refused and an
    /// append still lands.
    #[sqlx::test]
    async fn a_pruned_base_serves_only_an_unchanged_page(pool: PgPool) {
        let page_id = tree_page(&pool, "Coffee.\n\nTea.\n").await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let ids = top_ids(&yjs.read_tree(&page_id).await.unwrap());
        let prune = || async {
            sqlx::query("DELETE FROM app_page_read_bases WHERE page_id = $1")
                .bind(&page_id)
                .execute(&pool)
                .await
                .unwrap();
        };

        let base = read(&tool, json!({ "page_id": page_id })).await["base"].clone();
        prune().await;
        let unchanged = edit(
            &tool,
            json!({ "page_id": page_id, "base": base,
            "ops": [{ "op": "replace", "id": ids[0], "html": "<p>Black coffee.</p>" }] }),
        )
        .await;
        assert_eq!(unchanged["applied"], true);

        let base = read(&tool, json!({ "page_id": page_id })).await["base"].clone();
        prune().await;
        person_writes(
            &yjs,
            &page_id,
            Op::Replace {
                id: ids[1].clone(),
                html: "<p>Green tea.</p>".into(),
            },
        )
        .await;
        let changed = refused_whole(
            &tool,
            &yjs,
            &page_id,
            json!({ "page_id": page_id, "base": base,
            "ops": [{ "op": "replace", "id": ids[0], "html": "<p>Coffee.</p>" }] }),
        )
        .await;
        assert!(changed["refused"][0]["message"]
            .as_str()
            .unwrap()
            .starts_with("replace needs the base"));

        let appended = edit(
            &tool,
            json!({ "page_id": page_id, "base": base,
            "ops": [{ "op": "append", "html": "<p>Juice.</p>" }] }),
        )
        .await;
        assert_eq!(appended["applied"], true);
        assert_eq!(
            texts(&yjs.read_tree(&page_id).await.unwrap()),
            ["Black coffee.", "Green tea.", "Juice."]
        );
    }

    /// A block the model's base never showed (one it wrote, named with an
    /// older base) is not replaced over changes the server cannot check.
    #[sqlx::test]
    async fn a_block_outside_the_base_is_not_replaced_blind(pool: PgPool) {
        let page_id = tree_page(&pool, "Coffee.\n").await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let base = read(&tool, json!({ "page_id": page_id })).await["base"].clone();
        let added = edit(
            &tool,
            json!({ "page_id": page_id, "base": base,
            "ops": [{ "op": "append", "html": "<p>Tea.</p>" }] }),
        )
        .await;
        let new_id = added["edit"]["blocks"][0].as_str().unwrap().to_string();

        let stale = refused_whole(
            &tool,
            &yjs,
            &page_id,
            json!({ "page_id": page_id, "base": base,
            "ops": [{ "op": "replace", "id": new_id, "html": "<p>Green tea.</p>" }] }),
        )
        .await;
        assert!(stale["refused"][0]["message"]
            .as_str()
            .unwrap()
            .contains("is not in the read that base names"));
        // The refusal's own base has seen it, so the retry lands.
        let retry = edit(
            &tool,
            json!({ "page_id": page_id, "base": stale["base"],
            "ops": [{ "op": "replace", "id": new_id, "html": "<p>Green tea.</p>" }] }),
        )
        .await;
        assert_eq!(retry["applied"], true);
    }

    /// A page nested as deep as the contract allows is read and edited by
    /// block like any other: its base reads back for the edit.
    #[sqlx::test]
    async fn a_page_as_deep_as_the_contract_allows_is_edited_by_block(pool: PgPool) {
        let quotes = virtues_document::MAX_DEPTH - 1;
        let page_id = tree_page(&pool, &format!("{}Deep.\n\nCoffee.\n", "> ".repeat(quotes))).await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let tree = yjs.read_tree(&page_id).await.unwrap();
        assert_eq!(virtues_document::model::depth(&tree), virtues_document::MAX_DEPTH);
        let base = read(&tool, json!({ "page_id": page_id })).await["base"].clone();
        let ids = top_ids(&tree);

        let seen = read(&tool, json!({ "page_id": page_id, "ids": [ids[1]], "base": base })).await;
        assert!(seen["html"].as_str().unwrap().contains("Coffee."), "{seen}");
        let edited = edit(
            &tool,
            json!({ "page_id": page_id, "base": seen["base"],
                    "ops": [{ "op": "replace", "id": ids[1], "html": "<p>Tea.</p>" }] }),
        )
        .await;
        assert_eq!(edited["applied"], true, "{edited}");
        assert_eq!(texts(&yjs.read_tree(&page_id).await.unwrap()), ["Deep.", "Tea."]);
    }

    /// A refused batch's base takes in the blocks of the refused ops alone.
    /// An op that would have merged with a person's change is sent again as
    /// written, against its block as the model read it, so the retry merges
    /// it as the first try would have, and the person's word stays.
    #[sqlx::test]
    async fn a_refusal_brings_only_the_refused_blocks_up_to_date(pool: PgPool) {
        let page_id = tree_page(&pool, "The quick brown fox.\n\nLunch on Friday.\n").await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let base = read(&tool, json!({ "page_id": page_id })).await["base"].clone();
        let ids = top_ids(&yjs.read_tree(&page_id).await.unwrap());
        person_writes(
            &yjs,
            &page_id,
            Op::Replace {
                id: ids[0].clone(),
                html: "<p>The quick brown fox jumps.</p>".into(),
            },
        )
        .await;
        person_writes(
            &yjs,
            &page_id,
            Op::Replace {
                id: ids[1].clone(),
                html: "<p>Lunch on Saturday.</p>".into(),
            },
        )
        .await;
        let slow = json!({ "op": "replace", "id": ids[0], "html": "<p>The slow brown fox.</p>" });

        let refused = refused_whole(
            &tool,
            &yjs,
            &page_id,
            json!({ "page_id": page_id, "base": base, "ops": [
                slow, { "op": "replace", "id": ids[1], "html": "<p>Lunch on Sunday.</p>" },
            ] }),
        )
        .await;
        assert_eq!(refused["refused"].as_array().unwrap().len(), 1, "{refused}");
        assert_eq!(refused["refused"][0]["at"], "op 2");
        assert_eq!(
            refused["blocks"],
            format!("<p data-id=\"{}\">Lunch on Saturday.</p>", ids[1])
        );
        let seen = kept_base(&pool, &page_id, refused["base"].as_str().unwrap())
            .await
            .unwrap();
        assert_eq!(texts(&seen), ["The quick brown fox.", "Lunch on Saturday."]);

        // Op 1 as written, op 2 written again from the block shown.
        let retry = edit(
            &tool,
            json!({ "page_id": page_id, "base": refused["base"], "ops": [
                slow, { "op": "replace", "id": ids[1], "html": "<p>Lunch on Sunday.</p>" },
            ] }),
        )
        .await;
        assert_eq!(retry["applied"], true, "{retry}");
        assert_eq!(retry["notes"][0]["at"], "op 1", "{retry}");
        assert_eq!(
            texts(&yjs.read_tree(&page_id).await.unwrap()),
            ["The slow brown fox jumps.", "Lunch on Sunday."]
        );
    }

    /// The refused op names a list item; the op that was not refused
    /// rewrites the paragraph inside it, which a person has typed in since
    /// the read. The item is brought up to date for the retry and the
    /// paragraph stays as read, so the retry merges the person's word in.
    #[sqlx::test]
    async fn a_refusal_keeps_a_block_inside_a_refreshed_one_as_read(pool: PgPool) {
        let page_id = tree_page(&pool, "- Pakc bags for Lisbon\n- Book seats\n").await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let base = read(&tool, json!({ "page_id": page_id })).await["base"].clone();
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let item = tree[0].content[0].id().unwrap().to_string();
        let para = tree[0].content[0].content[0].id().unwrap().to_string();
        person_writes(
            &yjs,
            &page_id,
            Op::Replace {
                id: para.clone(),
                html: "<p>Pakc bags for Lisbon tonight</p>".into(),
            },
        )
        .await;
        let fix = json!({ "op": "replace", "id": para, "html": "<p>Pack bags for Lisbon</p>" });

        let refused = refused_whole(
            &tool,
            &yjs,
            &page_id,
            json!({ "page_id": page_id, "base": base, "ops": [
                fix, { "op": "insert_after", "id": item, "html": "<p>Print tickets</p>" },
            ] }),
        )
        .await;
        assert_eq!(refused["refused"].as_array().unwrap().len(), 1, "{refused}");
        assert_eq!(refused["refused"][0]["at"], "op 2");
        let seen = kept_base(&pool, &page_id, refused["base"].as_str().unwrap())
            .await
            .unwrap();
        let read_para = virtues_document::ops::find_node(&seen, &para).unwrap();
        assert_eq!(read_para.text_content(), "Pakc bags for Lisbon", "the paragraph as read");

        let retry = edit(
            &tool,
            json!({ "page_id": page_id, "base": refused["base"], "ops": [
                fix, { "op": "insert_after", "id": item, "html": "<li><p>Print tickets</p></li>" },
            ] }),
        )
        .await;
        assert_eq!(retry["applied"], true, "{retry}");
        assert_eq!(retry["notes"][0]["at"], "op 1", "{retry}");
        let items: Vec<String> = yjs.read_tree(&page_id).await.unwrap()[0]
            .content
            .iter()
            .map(Node::text_content)
            .collect();
        assert_eq!(items, ["Pack bags for Lisbon tonight", "Print tickets", "Book seats"]);
    }

    /// A list written with no base is in no read the model has. Its items
    /// enter a base with the list: through an `ids` read of one, a refusal
    /// of an edit to one, or a write beside one. Each base that comes back
    /// takes the next edit, so no read or retry goes round in a loop.
    #[sqlx::test]
    async fn a_list_written_with_no_base_enters_a_base_whole(pool: PgPool) {
        let page_id = tree_page(&pool, "Coffee.\n").await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let base = read(&tool, json!({ "page_id": page_id })).await["base"].clone();
        let appended = edit(
            &tool,
            json!({ "page_id": page_id, "ops": [{ "op": "append", "html":
                "<ul data-type=\"taskList\"><li data-type=\"taskItem\" data-checked=\"false\"><p>Pack</p></li>\
                 <li data-type=\"taskItem\" data-checked=\"false\"><p>Print</p></li></ul>" }] }),
        )
        .await;
        assert_eq!(appended["applied"], true, "{appended}");
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let list = tree[1].id().unwrap().to_string();
        let items: Vec<String> = tree[1].content.iter().map(|n| n.id().unwrap().to_string()).collect();
        let tick = |id: &str, text: &str| {
            json!({ "op": "replace", "id": id, "html":
                format!("<li data-type=\"taskItem\" data-checked=\"true\"><p>{text}</p></li>") })
        };

        // Refused with the older base: the reply shows the list, and its
        // base takes the retry.
        let refused = refused_whole(
            &tool,
            &yjs,
            &page_id,
            json!({ "page_id": page_id, "base": base, "ops": [tick(&items[0], "Pack")] }),
        )
        .await;
        assert_ne!(refused["base"], base, "{refused}");
        assert!(refused["blocks"].as_str().unwrap().contains(&format!("data-id=\"{list}\"")), "{refused}");
        let retry = edit(
            &tool,
            json!({ "page_id": page_id, "base": refused["base"], "ops": [tick(&items[0], "Pack")] }),
        )
        .await;
        assert_eq!(retry["applied"], true, "{retry}");

        // An `ids` read of an item with the older base shows its list.
        let seen = read(&tool, json!({ "page_id": page_id, "ids": [items[1]], "base": base })).await;
        assert_ne!(seen["base"], base, "{seen}");
        assert!(seen["html"].as_str().unwrap().contains(&format!("data-id=\"{list}\"")), "{seen}");
        let ticked = edit(
            &tool,
            json!({ "page_id": page_id, "base": seen["base"], "ops": [tick(&items[1], "Print")] }),
        )
        .await;
        assert_eq!(ticked["applied"], true, "{ticked}");

        // A write beside an item, with the older base: its result's base
        // has the list, and the item it wrote takes the next edit.
        let beside = edit(
            &tool,
            json!({ "page_id": page_id, "base": base, "ops": [{ "op": "insert_after", "id": items[1], "html":
                "<li data-type=\"taskItem\" data-checked=\"false\"><p>Pay</p></li>" }] }),
        )
        .await;
        assert_eq!(beside["applied"], true, "{beside}");
        assert_ne!(beside["base"], base, "{beside}");
        let added = beside["edit"]["blocks"][0].as_str().unwrap().to_string();
        let paid = edit(
            &tool,
            json!({ "page_id": page_id, "base": beside["base"], "ops": [tick(&added, "Pay")] }),
        )
        .await;
        assert_eq!(paid["applied"], true, "{paid}");
        let checked: Vec<bool> = yjs.read_tree(&page_id).await.unwrap()[1]
            .content
            .iter()
            .map(|n| n.attrs["checked"] == json!(true))
            .collect();
        assert_eq!(checked, [true, true, true]);
    }

    /// An op refused for its own HTML is fixed from the model's copy of the
    /// block, so a block a person changed since the read stays in the base
    /// as read, and the fixed op merges with the change.
    #[sqlx::test]
    async fn an_op_refused_for_its_html_keeps_its_block_as_read(pool: PgPool) {
        let page_id = tree_page(&pool, "Lunch on Friday.\n").await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let base = read(&tool, json!({ "page_id": page_id })).await["base"].clone();
        let id = top_ids(&yjs.read_tree(&page_id).await.unwrap())[0].clone();
        person_writes(
            &yjs,
            &page_id,
            Op::Replace {
                id: id.clone(),
                html: "<p>Lunch on Saturday.</p>".into(),
            },
        )
        .await;

        let refused = refused_whole(
            &tool,
            &yjs,
            &page_id,
            json!({ "page_id": page_id, "base": base,
                    "ops": [{ "op": "replace", "id": id, "html": "<div>Dinner on Friday.</div>" }] }),
        )
        .await;
        assert!(refused["error"].as_str().unwrap().contains("<div>"), "{refused}");
        assert!(refused.get("blocks").is_none(), "{refused}");

        let retry = edit(
            &tool,
            json!({ "page_id": page_id, "base": refused["base"],
                    "ops": [{ "op": "replace", "id": id, "html": "<p>Dinner on Friday.</p>" }] }),
        )
        .await;
        assert_eq!(retry["applied"], true, "{retry}");
        assert_eq!(
            texts(&yjs.read_tree(&page_id).await.unwrap()),
            ["Dinner on Saturday."]
        );
    }

    /// A suggestion the owner has not decided is theirs to decide. A read
    /// says what its tags are; a replace of its block, which could only
    /// drop the tags, is refused whatever it sends, and the rest of the
    /// page is edited as ever.
    #[sqlx::test]
    async fn a_block_holding_a_suggestion_is_not_replaced(pool: PgPool) {
        let page_id = tree_page(&pool, "The quick fox, Fridy.\n\nCoffee at nine.\n").await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        // The owner suggests a change in their editor.
        let first = top_ids(&yjs.read_tree(&page_id).await.unwrap())[0].clone();
        person_writes(
            &yjs,
            &page_id,
            Op::Replace {
                id: first,
                html: "<p>The <virtues-del proposal=\"p1\">quick</virtues-del>\
                       <virtues-ins proposal=\"p1\">slow</virtues-ins> fox, Fridy.</p>"
                    .into(),
            },
        )
        .await;
        let data = read(&tool, json!({ "page_id": page_id })).await;
        assert!(data["html"].as_str().unwrap().contains("<virtues-del proposal="), "{data}");
        assert_eq!(data["suggestions"], SUGGESTIONS_NOTE);
        let base = data["base"].clone();
        let ids = top_ids(&yjs.read_tree(&page_id).await.unwrap());
        let held = virtues_document::to_html(&yjs.read_tree(&page_id).await.unwrap()[..1], true);

        for html in [
            "<p>The quick slow fox, Friday.</p>".to_string(),
            held.replace("Fridy", "Friday"),
        ] {
            let refused = refused_whole(
                &tool,
                &yjs,
                &page_id,
                json!({ "page_id": page_id, "base": base,
                        "ops": [{ "op": "replace", "id": ids[0], "html": html }] }),
            )
            .await;
            assert_eq!(
                refused["refused"][0]["message"],
                format!(
                    "the block `{}` holds a suggestion its owner has not accepted or rejected yet; \
                     leave it as it is, or ask them to accept or reject it first",
                    ids[0]
                ),
                "{refused}"
            );
        }

        // A delete takes both sides out: it decides the suggestion as surely.
        let refused = refused_whole(
            &tool,
            &yjs,
            &page_id,
            json!({ "page_id": page_id, "base": base, "ops": [{ "op": "delete", "id": ids[0] }] }),
        )
        .await;
        assert!(
            refused["refused"][0]["message"].as_str().unwrap().contains("holds a suggestion"),
            "{refused}"
        );

        let other = edit(
            &tool,
            json!({ "page_id": page_id, "base": base,
                    "ops": [{ "op": "replace", "id": ids[1], "html": "<p>Tea at nine.</p>" }] }),
        )
        .await;
        assert_eq!(other["applied"], true, "{other}");
        let tree = yjs.read_tree(&page_id).await.unwrap();
        assert_eq!(virtues_document::to_html(&tree[..1], true), held);

        // Nor does deleting a quote that holds one, inside it.
        let quoted = tree_page(&pool, "Coffee at nine.\n\n> The quick fox.\n").await;
        let quote = top_ids(&yjs.read_tree(&quoted).await.unwrap())[1].clone();
        let inner = yjs.read_tree(&quoted).await.unwrap()[1].content[0].id().unwrap().to_string();
        person_writes(
            &yjs,
            &quoted,
            Op::Replace {
                id: inner,
                html: "<p>The <virtues-del proposal=\"p2\">quick</virtues-del> fox.</p>".into(),
            },
        )
        .await;
        let base = read(&tool, json!({ "page_id": quoted })).await["base"].clone();
        let refused = refused_whole(
            &tool,
            &yjs,
            &quoted,
            json!({ "page_id": quoted, "base": base, "ops": [{ "op": "delete", "id": quote }] }),
        )
        .await;
        assert!(
            refused["refused"][0]["message"].as_str().unwrap().contains("holds a suggestion"),
            "{refused}"
        );
        assert!(holds_proposal(&yjs.read_tree(&quoted).await.unwrap()));

        // A page without one says nothing about them.
        let plain = tree_page(&pool, "Coffee.\n").await;
        assert!(read(&tool, json!({ "page_id": plain })).await.get("suggestions").is_none());
    }

    #[sqlx::test]
    async fn each_page_takes_its_own_form(pool: PgPool) {
        let tree = tree_page(&pool, "Coffee.\n").await;
        let text = pages::create_page(
            &pool,
            pages::CreatePageRequest {
                title: "Notes".into(),
                content: "Coffee.\n".into(),
                project_id: None,
                icon: None,
                icon_color: None,
                cover_url: None,
                tags: None,
                format: None,
            },
        )
        .await
        .unwrap()
        .id;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let refusal = |args: Value| {
            let tool = tool.clone();
            async move {
                match tool.edit_page(args, &ToolContext::default()).await {
                    Err(ToolError::InvalidParameters(message)) => message,
                    other => panic!("expected InvalidParameters, got {other:?}"),
                }
            }
        };

        assert_eq!(
            refusal(json!({ "page_id": tree, "find": "Coffee.", "replace": "Tea." })).await,
            EDITED_BY_BLOCK
        );
        assert_eq!(
            refusal(json!({ "page_id": text, "ops": [{ "op": "append", "html": "<p>Tea.</p>" }] }))
                .await,
            EDITED_AS_TEXT
        );
        assert_eq!(refusal(json!({ "page_id": tree })).await, NOTHING_TO_CHANGE);
        assert_eq!(refusal(json!({ "page_id": text })).await, NOTHING_TO_CHANGE);
        assert!(refusal(json!({ "page_id": text, "replace": "Tea." }))
            .await
            .starts_with("send find"));
        assert!(
            refusal(json!({ "page_id": tree, "ops": [{ "op": "move", "id": "x" }] }))
                .await
                .starts_with("op 1: unknown variant `move`")
        );

        // Empty ops beside find and replace are no ops: the text form lands.
        for (find, replace, ops) in [
            ("Coffee.", "Tea.", json!([])),
            ("Tea.", "Juice.", json!("[]")),
            ("Juice.", "Water.", json!(" ")),
        ] {
            let data = edit(
                &tool,
                json!({ "page_id": text, "find": find, "replace": replace, "ops": ops }),
            )
            .await;
            assert_eq!(data["applied"], true, "{ops}");
        }
        assert_eq!(yjs.read_text(&text).await.unwrap(), "Water.\n");

        // A title alone renames either kind, with no base.
        let renamed = edit(&tool, json!({ "page_id": tree, "title": "Drinks" })).await;
        assert_eq!(renamed["title_changed"], true);
        assert_eq!(renamed["edit"]["blocks"], json!([]));
        assert_eq!(texts(&yjs.read_tree(&tree).await.unwrap()), ["Coffee."]);
    }

    /// Models stringify nested arrays, and write op names with hyphens.
    #[sqlx::test]
    async fn ops_sent_as_a_string_are_read(pool: PgPool) {
        let page_id = tree_page(&pool, "Coffee.\n").await;
        let yjs = YjsState::new(pool.clone());
        let data = edit(
            &tool(&pool, &yjs),
            json!({ "page_id": page_id, "ops": "[{\"op\": \"append\", \"html\": \"<p>Tea.</p>\"}]" }),
        )
        .await;
        assert_eq!(data["applied"], true);
        let ids = top_ids(&yjs.read_tree(&page_id).await.unwrap());
        let data = edit(
            &tool(&pool, &yjs),
            json!({ "page_id": page_id, "ops": [{ "op": "insert-before", "id": ids[0], "html": "<h2>Drinks</h2>" }] }),
        )
        .await;
        assert_eq!(data["applied"], true);
        assert_eq!(
            texts(&yjs.read_tree(&page_id).await.unwrap()),
            ["Drinks", "Coffee.", "Tea."]
        );
    }

    #[sqlx::test]
    async fn without_yjs_a_block_page_is_not_edited(pool: PgPool) {
        let page_id = tree_page(&pool, "Coffee.\n").await;
        let offline = PageEditorTool::new(Arc::new(pool.clone()), None);
        let Err(ToolError::ExecutionFailed(message)) = offline
            .edit_page(
                json!({ "page_id": page_id, "ops": [{ "op": "append", "html": "<p>Tea.</p>" }] }),
                &ToolContext::default(),
            )
            .await
        else {
            panic!("refused");
        };
        assert_eq!(message, NEEDS_LIVE_DOCUMENT);
        let content: String = sqlx::query_scalar("SELECT content FROM app_pages WHERE id = $1")
            .bind(&page_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(content, "Coffee.\n");
    }

    /// Two chat edits to one block page at once take turns: neither reads
    /// the other's change as the owner's typing, and both land.
    #[sqlx::test]
    async fn two_block_edits_at_once_take_turns(pool: PgPool) {
        let page_id = tree_page(&pool, "Coffee.\n\nTea.\n").await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let base = read(&tool, json!({ "page_id": page_id })).await["base"].clone();
        let ids = top_ids(&yjs.read_tree(&page_id).await.unwrap());

        let context = ToolContext::default();
        let (a, b) = tokio::join!(
            tool.edit_page(
                json!({ "page_id": page_id, "base": base,
                        "ops": [{ "op": "replace", "id": ids[0], "html": "<p>Black coffee.</p>" }] }),
                &context
            ),
            tool.edit_page(
                json!({ "page_id": page_id, "base": base,
                        "ops": [{ "op": "replace", "id": ids[1], "html": "<p>Green tea.</p>" }] }),
                &context
            ),
        );
        assert_eq!(a.unwrap().data["applied"], true);
        assert_eq!(b.unwrap().data["applied"], true);
        let typed: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM app_page_versions WHERE page_id = $1 AND description = $2",
        )
        .bind(&page_id)
        .bind(TYPED_BEFORE_CHAT_EDIT)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(typed, 0, "a chat edit was versioned as the owner's typing");
        assert_eq!(
            texts(&yjs.read_tree(&page_id).await.unwrap()),
            ["Black coffee.", "Green tea."]
        );
    }

    /// A block page's creation says it is one, with what its conversion
    /// changed.
    #[sqlx::test]
    async fn creating_a_block_page_reports_its_format_and_notes(pool: PgPool) {
        let created = pages::create_page_as(
            &pool,
            pages::CreatePageRequest {
                title: "Reading".into(),
                content: "---\nsource: pdf\n---\n\n## Plan\n".into(),
                project_id: None,
                icon: None,
                icon_color: None,
                cover_url: None,
                tags: None,
                format: None,
            },
            PageFormat::Tree,
        )
        .await
        .unwrap();
        let data = created_result(created);
        assert_eq!(data["format"], "tree");
        assert_eq!(
            data["message"],
            "Page created. Read it with get_page_content before editing its blocks."
        );
        assert!(
            data["notes"][0]["message"]
                .as_str()
                .unwrap()
                .contains("front matter"),
            "{data}"
        );

        // The tool itself follows the server's default: markdown, with the
        // flag off as it is in tests.
        let yjs = YjsState::new(pool.clone());
        let data = tool(&pool, &yjs)
            .create_page(json!({ "title": "Notes", "content": "Coffee." }))
            .await
            .unwrap()
            .data;
        assert_eq!(data["format"], "markdown");
        assert_eq!(data["message"], "Page created.");
        assert!(data.get("notes").is_none());
    }

    /// A page with a callout holding a list too long for one read, the
    /// callout's id, its list's, and the list's item texts.
    async fn callout_around_a_long_list(pool: &PgPool, tool: &PageEditorTool, yjs: &YjsState) -> (String, String, String) {
        let page_id = tree_page(pool, "# Trip\n\nIntro.\n").await;
        let items: String = (0..900)
            .map(|i| format!("<li><p>Item {i:03} for the trip to the coast, packed by the door the night before</p></li>"))
            .collect();
        let first = read(tool, json!({ "page_id": page_id })).await;
        let appended = edit(tool, json!({ "page_id": page_id, "base": first["base"], "ops": [
            { "op": "append", "html": format!("<aside data-tone=\"tip\"><p>Packing, by room:</p><ul>{items}</ul></aside>") }
        ] }))
        .await;
        assert_eq!(appended["applied"], true, "{appended}");
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let callout = tree.last().unwrap();
        (
            page_id,
            callout.id().unwrap().to_string(),
            callout.content[1].id().unwrap().to_string(),
        )
    }

    /// A list read in part inside a callout went with the callout: the
    /// replace was refused as changed, the re-read of the callout the
    /// refusal asked for took the whole list from the page into the base,
    /// and the replace then deleted the items no read had shown.
    #[sqlx::test]
    async fn a_list_read_in_part_inside_a_block_is_not_replaced_with_it(pool: PgPool) {
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let (page_id, callout, list) = callout_around_a_long_list(&pool, &tool, &yjs).await;
        let first = read(&tool, json!({ "page_id": page_id })).await;
        let opened = read(&tool, json!({ "page_id": page_id, "ids": [callout], "base": first["base"] })).await;
        let opened_list = read(&tool, json!({ "page_id": page_id, "ids": [list], "base": opened["base"] })).await;
        let seen = child_ids(
            &kept_base(&pool, &page_id, opened_list["base"].as_str().unwrap()).await.unwrap(),
            &list,
        );
        assert!(seen.len() < 900, "{}", seen.len());
        let written: String = (0..seen.len())
            .map(|i| format!("<li><p>Item {i:03} for the trip to the coast, packed by the door the night before</p></li>"))
            .collect();
        let replace = |base: &Value| {
            json!({ "page_id": page_id, "base": base, "ops": [
                { "op": "replace", "id": callout, "html": format!("<aside data-tone=\"warning\"><p>Packing, by room:</p><ul>{written}</ul></aside>") }
            ] })
        };
        let last = seen.last().unwrap();
        let refused = refused_whole(&tool, &yjs, &page_id, replace(&opened_list["base"])).await;
        let message = refused["refused"][0]["message"].as_str().unwrap();
        assert!(message.contains(&format!("holds `{list}`, which was shown only in part")), "{message}");
        assert!(message.contains(&format!("after \"{last}\" and this base")), "{message}");

        // Reading the callout again keeps the list as it was read.
        let again = read(&tool, json!({ "page_id": page_id, "ids": [callout], "base": refused["base"] })).await;
        let held = child_ids(&kept_base(&pool, &page_id, again["base"].as_str().unwrap()).await.unwrap(), &list);
        assert_eq!(held, seen);
        refused_whole(&tool, &yjs, &page_id, replace(&again["base"])).await;
        let now = yjs.read_tree(&page_id).await.unwrap();
        assert_eq!(virtues_document::ops::find_node(&now, &list).unwrap().content.len(), 900);
    }

    /// A list opened again after a person changed an item read earlier:
    /// the base took the item from the page, so the model's edit of it,
    /// written from what it read, landed over the person's word unseen.
    #[sqlx::test]
    async fn opening_a_list_again_keeps_the_items_read_before_as_they_were_read(pool: PgPool) {
        let page_id = tree_page(&pool, &format!("Packing list:\n\n{}\nAfter the list.\n", long_list(900))).await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let list = tree[1].id().unwrap().to_string();
        let p700 = tree[1].content[700].content[0].id().unwrap().to_string();
        let first = read(&tool, json!({ "page_id": page_id })).await;
        let opened = read(&tool, json!({ "page_id": page_id, "ids": [list], "base": first["base"] })).await;
        let on = read(&tool, json!({ "page_id": page_id, "after": opened["more_after"], "base": opened["base"] })).await;
        assert!(on["html"].as_str().unwrap().contains(&format!("data-id=\"{p700}\"")), "item 700 was read");

        person_writes(
            &yjs,
            &page_id,
            Op::Replace {
                id: p700.clone(),
                html: "<p>Item 700 for the trip to the coast, PACKED by the door the night before</p>".into(),
            },
        )
        .await;
        let again = read(&tool, json!({ "page_id": page_id, "ids": [list], "base": on["base"] })).await;
        assert!(!again["html"].as_str().unwrap().contains(&format!("data-id=\"{p700}\"")), "item 700 not shown again");
        let kept = kept_base(&pool, &page_id, again["base"].as_str().unwrap()).await.unwrap();
        assert_eq!(
            virtues_document::ops::find_node(&kept, &p700).map(Node::text_content).as_deref(),
            Some("Item 700 for the trip to the coast, packed by the door the night before"),
            "the base holds the item as it was read"
        );

        let merged = edit(&tool, json!({ "page_id": page_id, "base": again["base"], "ops": [
            { "op": "replace", "id": p700, "html": "<p>Item 700 for the trip to the coast, packed by the door the night prior</p>" }
        ] }))
        .await;
        assert_eq!(merged["applied"], true, "{merged}");
        let now = yjs.read_tree(&page_id).await.unwrap();
        assert_eq!(
            virtues_document::ops::find_node(&now, &p700).map(Node::text_content).as_deref(),
            Some("Item 700 for the trip to the coast, PACKED by the door the night prior")
        );
    }

    /// A block shown only as markdown whose HTML is too long for a
    /// refusal: the refusal said its HTML was above, showed none, and gave
    /// back the same base, so the retry it asked for was the same call.
    #[sqlx::test]
    async fn a_long_block_shown_as_markdown_is_named_to_read_by_id(pool: PgPool) {
        let words: String = (0..1_400).map(|i| format!("point{i} ")).collect();
        let days: String = (0..400)
            .map(|i| format!("Day {i} on the coast road, walked from the harbour to the lighthouse.\n\n"))
            .collect();
        let page_id = tree_page(&pool, &format!("We agreed on {words}and then hung up.\n\n{days}")).await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let notes = tree[0].id().unwrap().to_string();
        assert!(virtues_document::to_html(&tree[..1], true).chars().count() > WRITTEN_HTML_CHARS);
        let first = read(&tool, json!({ "page_id": page_id })).await;
        assert!(first["markdown"].is_string(), "the long page reads as markdown");

        let html = format!("<p>{}</p>", tree[0].text_content().replace("point7 ", "point seven "));
        let replace = |base: &Value| json!({ "page_id": page_id, "base": base, "ops": [
            { "op": "replace", "id": notes, "html": html }
        ] });
        let refused = refused_whole(&tool, &yjs, &page_id, replace(&first["base"])).await;
        assert!(!refused["error"].as_str().unwrap().contains("is above"), "{refused}");
        assert_eq!(refused["unread"], json!([notes]), "{refused}");
        assert!(
            refused["message"].as_str().unwrap().contains("read it with get_page_content, ids and base"),
            "{refused}"
        );
        let by_id = read(&tool, json!({ "page_id": page_id, "ids": [notes], "base": refused["base"] })).await;
        let applied = edit(&tool, replace(&by_id["base"])).await;
        assert_eq!(applied["applied"], true, "{applied}");
    }

    /// A list shown whole as markdown, then opened by id: a replace of it
    /// waits for the HTML of the items the open did not show, and the
    /// refusal says to read on inside it from there, not to open it again.
    #[sqlx::test]
    async fn a_list_shown_as_markdown_then_opened_is_read_on_before_it_is_replaced(pool: PgPool) {
        let items: String = (0..1_000).map(|i| format!("- [ ] Pack thing {i:04}\n")).collect();
        let page_id = tree_page(&pool, &format!("Checklist:\n\n{items}\nAfter.\n")).await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let list = tree[1].id().unwrap().to_string();
        let first = read(&tool, json!({ "page_id": page_id })).await;
        assert!(first["markdown"].as_str().unwrap().contains("Pack thing 0999"), "shown whole as markdown");
        let opened = read(&tool, json!({ "page_id": page_id, "ids": [list], "base": first["base"] })).await;
        let last = opened["more_after"].as_str().unwrap().to_string();
        let html = format!(
            "<ul data-type=\"taskList\">{}</ul>",
            (0..1_000).rev().map(|i| format!("<li data-type=\"taskItem\"><p>Pack thing {i:04}</p></li>")).collect::<String>()
        );
        let replace = |base: &Value| json!({ "page_id": page_id, "base": base, "ops": [
            { "op": "replace", "id": list, "html": html }
        ] });
        let refused = refused_whole(&tool, &yjs, &page_id, replace(&opened["base"])).await;
        let message = refused["refused"][0]["message"].as_str().unwrap();
        assert!(message.contains(&format!("after \"{last}\" and this base")), "{message}");
        assert!(refused.get("unread").is_none(), "{refused}");

        let mut base = refused["base"].clone();
        let mut after = Some(last);
        while let Some(a) = after {
            let on = read(&tool, json!({ "page_id": page_id, "after": a, "base": base })).await;
            base = on["base"].clone();
            after = on["more_after"].as_str().map(str::to_string);
        }
        let applied = edit(&tool, replace(&base)).await;
        assert_eq!(applied["applied"], true, "{applied}");
    }
}

#[cfg(test)]
mod page_edit_eval {
    //! How well the production models edit block pages through these two
    //! tools, measured before block pages become the default.
    //!
    //! Twelve fixed edit tasks, each on a fresh copy of one fixture page,
    //! run through get_page_content and edit_page against the Standard
    //! slot's model and the applet runner's (the Lite slot, its default).
    //! The model is sent what a chat with the page open sends: the chat's
    //! page guidance (`agent::prompt::PAGE_TOOL_PROMPT`) and the open page's
    //! section (`api::chat::open_page_section`), so a change to either is
    //! measured.
    //! Counts refusals, recoveries on retry, tool errors and wrong edits (the
    //! page's HTML afterwards against the expected HTML). It reports and
    //! never fails: a model's taste is not a build break.
    //!
    //! Networked and key-gated, so it is `#[ignore]`d and skips cleanly when
    //! `AI_GATEWAY_API_KEY` is unset in the shell that runs it (the repo's
    //! `.env` does not count). `EVAL_MODELS` (comma-separated ids) runs other
    //! models instead:
    //!
    //!     cargo test -p virtues --lib page_edit_eval -- --ignored --nocapture
    //!
    //! It spends real money, so it holds a hard budget: `EVAL_MAX_USD`
    //! (default 1.50). Prices come from the gateway's own catalog before any
    //! call, a model with no published price is not run, and no call is made
    //! that could take the total past the budget at its worst (the prompt so
    //! far plus `max_tokens` of output). Tasks the budget stops are reported as
    //! not run, never as passed.

    use super::*;
    use virtues_registry::models::ModelSlot;

    const GATEWAY: &str = "https://ai-gateway.vercel.sh/v1/chat/completions";
    const CATALOG: &str = "https://ai-gateway.vercel.sh/v1/models";
    const MAX_TOKENS: u64 = 2048;

    /// Dollars per input and per output token, from the gateway's catalog.
    async fn prices(client: &reqwest::Client, key: &str) -> std::collections::HashMap<String, (f64, f64)> {
        let catalog: Value = match client.get(CATALOG).bearer_auth(key).send().await {
            Ok(r) => r.json().await.unwrap_or(Value::Null),
            Err(_) => Value::Null,
        };
        let num = |v: &Value| v.as_str().and_then(|s| s.parse::<f64>().ok()).or_else(|| v.as_f64());
        catalog["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| {
                let id = m["id"].as_str()?.to_string();
                Some((id, (num(&m["pricing"]["input"])?, num(&m["pricing"]["output"])?)))
            })
            .collect()
    }

    /// Model calls per task. A read, an edit and a retry fit in four.
    const MAX_STEPS: usize = 6;

    const FIXTURE: &str = "## Plan\n\n\
        Lunch with [@Nick](/person/person_1) on Fridy.\n\n\
        Dinner with [@David Okafor](/person/person_2) at eight.\n\n\
        - [ ] Book seats\n- [ ] Pack bags\n\n\
        | Day | Place |\n| --- | --- |\n| Friday | Lisbon |\n| Saturday | Porto |\n\n\
        Send the receipts to nick@example.com.\n";

    const SYSTEM: &str = "You edit the user's page with the tools. The page's id is {page}. \
        Make exactly the change asked for and nothing else, then say done.";

    /// One task: what is asked, and what it does to the fixture's top-level
    /// blocks (as canonical HTML without ids) when done right.
    struct Task {
        ask: &'static str,
        expect: fn(&mut Vec<String>),
    }

    fn tasks() -> Vec<Task> {
        vec![
            Task { ask: "Fix the typo in the lunch line.", expect: |b| b[1] = b[1].replace("Fridy", "Friday") },
            Task { ask: "Rename the Plan heading to Itinerary.", expect: |b| b[0] = "<h2>Itinerary</h2>".into() },
            Task {
                ask: "Add a to-do 'Print tickets' at the end of the checklist.",
                expect: |b| {
                    b[3] = b[3].replace(
                        "</ul>",
                        "<li data-type=\"taskItem\" data-checked=\"false\"><p>Print tickets</p></li></ul>",
                    )
                },
            },
            Task {
                ask: "Check off 'Book seats'.",
                // Canonical HTML leaves `data-checked` off an open item.
                expect: |b| {
                    b[3] = b[3].replacen(
                        "<li data-type=\"taskItem\">",
                        "<li data-type=\"taskItem\" data-checked=\"true\">",
                        1,
                    )
                },
            },
            Task { ask: "Delete the dinner line.", expect: |b| { b.remove(2); } },
            Task {
                ask: "Add a paragraph 'Flights are booked.' right after the heading.",
                expect: |b| b.insert(1, "<p>Flights are booked.</p>".into()),
            },
            Task {
                ask: "At the very end of the page, add a divider and then a line 'Questions for David'.",
                expect: |b| {
                    b.push("<hr>".into());
                    b.push("<p>Questions for David</p>".into());
                },
            },
            Task {
                ask: "Make 'at eight' bold in the dinner line.",
                expect: |b| b[2] = b[2].replace("at eight", "<strong>at eight</strong>"),
            },
            Task { ask: "Change Porto to Coimbra in the table.", expect: |b| b[4] = b[4].replace("Porto", "Coimbra") },
            Task {
                ask: "Replace the receipts line with a bulleted list of two items: 'Receipts' and 'Tickets'.",
                expect: |b| b[5] = "<ul><li><p>Receipts</p></li><li><p>Tickets</p></li></ul>".into(),
            },
            Task {
                ask: "In the lunch line, change 'Lunch' to 'Brunch' and keep the mention of Nick as it is.",
                expect: |b| b[1] = b[1].replacen("Lunch", "Brunch", 1),
            },
            Task {
                ask: "After the table, add a tip callout that says 'Bring an umbrella.'",
                expect: |b| b.insert(5, "<aside data-tone=\"tip\"><p>Bring an umbrella.</p></aside>".into()),
            },
        ]
    }

    /// HTML as the page would hold it: through the contract and back.
    fn canonical(html: &str) -> String {
        let o = virtues_document::parse_html(html, "doc");
        assert!(
            o.errors.is_empty(),
            "the expected HTML is outside the contract: {:?}",
            o.errors
        );
        virtues_document::to_html(&o.nodes, false)
    }

    #[derive(Default)]
    struct Tally {
        wrong: usize,
        refusals: usize,
        recoveries: usize,
        errors: usize,
        steps: usize,
    }

    /// `#[sqlx::test]` written out, so the key is checked before the
    /// scratch database is made: making it loads the repo's `.env` into
    /// this process, which would hand the eval a key the shell never set.
    #[test]
    #[ignore = "network + AI_GATEWAY_API_KEY: spends real money on the live gateway"]
    fn block_page_edits_on_the_production_models() {
        if std::env::var("AI_GATEWAY_API_KEY").is_err() {
            eprintln!("AI_GATEWAY_API_KEY unset — skipping");
            return;
        }
        let mut args = sqlx::testing::TestArgs::new(concat!(
            module_path!(),
            "::block_page_edits_on_the_production_models"
        ));
        args.migrator(&crate::database::EMBEDDED_MIGRATIONS);
        let f: fn(PgPool) -> _ = run;
        sqlx::testing::TestFn::run_test(f, args)
    }

    async fn run(pool: PgPool) {
        let key =
            std::env::var("AI_GATEWAY_API_KEY").expect("checked before the database was made");
        let _ = rustls::crypto::ring::default_provider().install_default();
        let models: Vec<String> = match std::env::var("EVAL_MODELS") {
            Ok(list) => list
                .split(',')
                .map(|m| m.trim().to_string())
                .filter(|m| !m.is_empty())
                .collect(),
            Err(_) => vec![
                crate::api::model_catalog::model_for_slot(ModelSlot::Standard),
                crate::api::model_catalog::model_for_slot(ModelSlot::Lite),
            ],
        };
        let tools = crate::tools::tools_named(&["get_page_content", "edit_page"]);
        let client = reqwest::Client::new();
        let budget: f64 = std::env::var("EVAL_MAX_USD")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1.50);
        let price_list = prices(&client, &key).await;
        let mut spent = 0.0_f64;
        let mut stopped = false;
        let yjs = YjsState::new(pool.clone());
        let tool = PageEditorTool::new(Arc::new(pool.clone()), Some(yjs.clone()));
        let context = ToolContext::default();

        for model in &models {
            let mut tally = Tally::default();
            eprintln!("\n=== {model} ===");
            let Some(&(price_in, price_out)) = price_list.get(model) else {
                eprintln!("  not run: the gateway publishes no price for {model}, so its cost cannot be bounded");
                continue;
            };
            let mut last_prompt: u64 = 0;
            let mut not_run = 0;
            for (n, task) in tasks().iter().enumerate() {
                if stopped {
                    not_run += 1;
                    eprintln!("  task {:>2} NOT RUN (budget) {}", n + 1, task.ask);
                    continue;
                }
                let page_id = pages::create_page_as(
                    &pool,
                    pages::CreatePageRequest {
                        title: "Trip".into(),
                        content: FIXTURE.into(),
                        project_id: None,
                        icon: None,
                        icon_color: None,
                        cover_url: None,
                        tags: None,
                        format: None,
                    },
                    PageFormat::Tree,
                )
                .await
                .expect("the fixture page")
                .page
                .id;
                let start = yjs.read_tree(&page_id).await.expect("the fixture's tree");
                let mut blocks: Vec<String> = start
                    .iter()
                    .map(|b| virtues_document::to_html(std::slice::from_ref(b), false))
                    .collect();
                (task.expect)(&mut blocks);
                let want = canonical(&blocks.concat());

                let open = crate::api::chat::open_page_section(
                    &pool,
                    &yjs,
                    &crate::api::chat::ActivePageContext {
                        page_id: Some(page_id.clone()),
                        page_title: Some("Trip".into()),
                        content: None,
                    },
                )
                .await
                .unwrap_or_default();
                let system = format!(
                    "{}{}{open}",
                    SYSTEM.replace("{page}", &page_id),
                    crate::agent::prompt::PAGE_TOOL_PROMPT
                );
                let mut messages = vec![
                    json!({ "role": "system", "content": system }),
                    json!({ "role": "user", "content": task.ask }),
                ];
                let mut owed_a_retry = false;
                for _ in 0..MAX_STEPS {
                    let worst_prompt = (last_prompt.max(4_000) as f64 * 1.5) as u64;
                    let worst = worst_prompt as f64 * price_in + MAX_TOKENS as f64 * price_out;
                    if spent + worst > budget {
                        stopped = true;
                        eprintln!("  budget: ${spent:.4} spent of ${budget:.2}; stopping before a call that could cost ${worst:.4}");
                        break;
                    }
                    tally.steps += 1;
                    let body = json!({ "model": model, "max_tokens": MAX_TOKENS, "tools": tools, "messages": messages });
                    let resp = match client
                        .post(GATEWAY)
                        .bearer_auth(&key)
                        .json(&body)
                        .send()
                        .await
                    {
                        Ok(resp) => resp,
                        Err(e) => {
                            eprintln!("  task {}: the gateway failed: {e}", n + 1);
                            break;
                        }
                    };
                    let payload: Value = resp.json().await.unwrap_or(Value::Null);
                    let prompt = payload["usage"]["prompt_tokens"].as_u64().unwrap_or(worst_prompt);
                    let completion = payload["usage"]["completion_tokens"].as_u64().unwrap_or(MAX_TOKENS);
                    last_prompt = prompt;
                    let computed = prompt as f64 * price_in + completion as f64 * price_out;
                    // Charge whichever is higher, the gateway's own figure or ours.
                    spent += payload["usage"]["cost"].as_f64().map_or(computed, |c| c.max(computed));
                    let message = payload["choices"][0]["message"].clone();
                    if message.is_null() {
                        eprintln!("  task {}: no reply: {payload}", n + 1);
                        break;
                    }
                    let calls = message["tool_calls"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    messages.push(message);
                    if calls.is_empty() {
                        break;
                    }
                    for call in calls {
                        let name = call["function"]["name"].as_str().unwrap_or_default();
                        let args: Value = serde_json::from_str(
                            call["function"]["arguments"].as_str().unwrap_or("{}"),
                        )
                        .unwrap_or(json!({}));
                        let result = match name {
                            "get_page_content" => tool.get_page_content(args, &context).await,
                            "edit_page" => tool.edit_page(args, &context).await,
                            other => Err(ToolError::UnknownTool(other.to_string())),
                        };
                        let content = match result {
                            Ok(r) if r.data["status"] == "refused" => {
                                tally.refusals += 1;
                                owed_a_retry = true;
                                r.data.to_string()
                            }
                            Ok(r) => {
                                if name == "edit_page" && r.data["applied"] == true && owed_a_retry
                                {
                                    tally.recoveries += 1;
                                    owed_a_retry = false;
                                }
                                r.data.to_string()
                            }
                            Err(e) => {
                                tally.errors += 1;
                                owed_a_retry = true;
                                json!({ "error": e.to_string() }).to_string()
                            }
                        };
                        messages.push(json!({ "role": "tool", "tool_call_id": call["id"], "content": content }));
                    }
                }

                if stopped {
                    not_run += 1;
                    eprintln!("  task {:>2} NOT RUN (budget, stopped mid-task) {}", n + 1, task.ask);
                    continue;
                }
                let got = virtues_document::to_html(
                    &yjs.read_tree(&page_id).await.expect("the page"),
                    false,
                );
                if got == want {
                    eprintln!("  task {:>2} ok    {}", n + 1, task.ask);
                } else {
                    tally.wrong += 1;
                    eprintln!(
                        "  task {:>2} WRONG {}\n      want {want}\n      got  {got}",
                        n + 1,
                        task.ask
                    );
                }
            }
            eprintln!(
                "  {model}: {} of {} wrong, {not_run} not run; {} refusals, {} recovered on retry; {} tool errors; {} model calls",
                tally.wrong,
                tasks().len(),
                tally.refusals,
                tally.recoveries,
                tally.errors,
                tally.steps
            );
        }
        eprintln!("\nspent ${spent:.4} of a ${budget:.2} budget");
    }
}

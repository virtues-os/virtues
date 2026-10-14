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
//!   shown as a read base (`virtues_document::Seen`, kept by
//!   `api::page_reads`) and returns its name. An edit is a batch of
//!   block-id ops naming that base, applied all or none
//!   (`YjsState::edit_tree`): it takes off the page or writes over only
//!   what reads showed the model as it stands, a block someone changed
//!   since the read is merged with the edit or refused, and nothing is
//!   overwritten unseen. A refused batch comes back as a result the model
//!   acts on, not an error.
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
use std::collections::HashSet;
use std::sync::{Arc, OnceLock};
use virtues_document::{Form, Node, Op, Part, Problem, Seen, Shown, Unseen, Why};

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

/// Whether a full read shows the page's canonical `html` whole: short
/// enough in characters ([`READ_HTML_CHARS`]), and in the bytes it takes in
/// the result ([`READ_MARKDOWN_BYTES`], as every read is measured), so the
/// agent loop's cut never clips it and leaves the model a base of blocks it
/// was not shown. A control character takes six bytes in JSON (`\u001b`).
fn shows_whole_as_html(html: &str) -> bool {
    chars(html) <= READ_HTML_CHARS && json_bytes(html) <= READ_MARKDOWN_BYTES
}

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

/// How much of the written blocks' HTML an edit's result shows, and a
/// refusal of the blocks its retry is written from, whole blocks only. A
/// block an edit wrote is in the next base as written whether it is shown
/// or not; one a refusal could not show is named, to be read by id.
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
const IDS_NEED_BASE: &str = "ids and after need the base from your last read of this page in \
     this turn; read it again without them";

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
    /// reads those blocks as HTML, and `after` with `base` reads on; either
    /// keeps a base that is the earlier one with what this read shows
    /// added. A read's text and what its base learns are both read off one
    /// value (`virtues_document::Shown`), so a base never holds what the
    /// text did not show. Without Yjs (a CLI read in its own process) a
    /// block page is read from its saved state, which can trail the live
    /// doc by the save debounce; a base kept from it holds what the page
    /// held, and the merge handles what changed since.
    pub async fn get_page_content(
        &self,
        arguments: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let args: GetPageContentArgs = serde_json::from_value(arguments)
            .map_err(|e| ToolError::InvalidParameters(format!("Invalid arguments: {}", e)))?;
        let turn = context.turn.as_deref();

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
        // has been shown is that read's base and what this read shows.
        let prior = if ids.is_empty() && after.is_none() {
            None
        } else {
            let base = args
                .base
                .as_deref()
                .map(str::trim)
                .filter(|b| !b.is_empty())
                .ok_or_else(|| ToolError::InvalidParameters(IDS_NEED_BASE.into()))?;
            Some(
                self.base_seen(&page.id, base, &tree, turn)
                    .await?
                    .ok_or_else(|| ToolError::InvalidParameters(IDS_NEED_BASE.into()))?,
            )
        };

        if let Some(prior) = prior.as_ref().filter(|_| !ids.is_empty()) {
            let read = read_by_id(prior, &tree, &ids, READ_MARKDOWN_BYTES, json_bytes);
            // A block asked for and gone is seen gone, so it leaves the base.
            let mut parts = read.parts.clone();
            parts.extend(read.missing.iter().cloned().map(Part::Gone));
            let shown = Shown::html(parts);
            let mut next = prior.clone();
            next.learn(&shown, &tree, &HashSet::new());
            let kept = keep_base(pool, &page.id, &next, &tree, turn).await?;
            // A block asked for that the read did not show.
            let shown_ids: HashSet<String> = shown.shown_ids().into_iter().collect();
            let mut unread = read.unread.clone();
            for id in &ids {
                let passed = read.missing.contains(id)
                    || shown_ids.contains(id)
                    || read.whole_only.contains(id)
                    || unread.contains(id);
                if !passed {
                    unread.push(id.clone());
                }
            }
            let mut out = json!({
                "page_id": page.id,
                "format": "tree",
                "base": kept,
                "html": shown.text(),
                "missing": read.missing,
            });
            let mut notes = vec![];
            for (outer, cut, inside) in &read.cut {
                notes.push(cut_down_note(outer, cut, inside));
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
            if holds_proposal(&wholes(&shown.parts)) {
                out["suggestions"] = json!(SUGGESTIONS_NOTE);
            }
            return Ok(ToolResult::success(out));
        }

        if let (Some(prior), Some(after)) = (prior.as_ref(), after.as_deref()) {
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
                // The blocks it shows, inside the block the reply names, and
                // no more: the tags of that block were not shown.
                let shown = Shown::html(window.parts.clone()).inside(parent);
                let mut next = prior.clone();
                next.learn(&shown, &tree, &HashSet::new());
                let kept = keep_base(pool, &page.id, &next, &tree, turn).await?;
                let mut out = json!({
                    "page_id": page.id,
                    "format": "tree",
                    "base": kept,
                    "html": shown.text(),
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
                if holds_proposal(&wholes(&shown.parts)) {
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
            let mut next = prior.clone();
            next.learn(&window.shown, &tree, &HashSet::new());
            let kept = keep_base(pool, &page.id, &next, &tree, turn).await?;
            let mut out = json!({
                "page_id": page.id,
                "format": "tree",
                "base": kept,
                "markdown": window.shown.text(),
            });
            if let Some(next) = window.next {
                out["note"] = json!(read_on(&next));
                out["more_after"] = json!(next);
            }
            if holds_proposal(&wholes(&window.shown.parts)) {
                out["suggestions"] = json!(SUGGESTIONS_NOTE);
            }
            return Ok(ToolResult::success(out));
        }

        // A full read: the whole page as HTML, or a long page's markdown as
        // far as one read holds. Its base is what it shows, from nothing.
        let html = virtues_document::to_html(&tree, true);
        let mut out = json!({
            "page_id": page.id,
            "title": page.title,
            "format": "tree",
        });
        let (seen, next, shown) = if shows_whole_as_html(&html) {
            out["html"] = json!(html);
            (Seen::of_page(&tree), None, tree.clone())
        } else {
            let window = page_reads::markdown_window(
                &tree,
                0,
                READ_MARKDOWN_BYTES,
                json_bytes,
                fits_one_read,
            );
            out["markdown"] = json!(window.shown.text());
            out["note"] = json!(match &window.next {
                Some(next) => format!("{LONG_PAGE_NOTE} {}", read_on(next)),
                None => LONG_PAGE_NOTE.to_string(),
            });
            let mut seen = Seen::default();
            seen.learn(&window.shown, &tree, &HashSet::new());
            let shown = wholes(&window.shown.parts);
            (seen, window.next, shown)
        };
        out["base"] = json!(keep_base(pool, &page.id, &seen, &tree, turn).await?);
        if let Some(next) = next {
            out["more_after"] = json!(next);
        }
        if holds_proposal(&shown) {
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
            PageFormat::Tree => self.edit_block_page(page_id, args, context.turn.as_deref()).await,
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
            "page_id": page_id,
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
    /// the server cannot tell what the model has seen; inserts and appends
    /// need none. With a base, the batch takes off the page or writes over
    /// only what reads have shown the model as it stands
    /// (`virtues_document::check_ops`), checked here first so a refusal
    /// keeps no restore point, and again in the write's own transaction.
    async fn edit_block_page(
        &self,
        page_id: String,
        args: EditPageArgs,
        turn: Option<&str>,
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
        let read = match args
            .base
            .as_deref()
            .map(str::trim)
            .filter(|b| !b.is_empty())
        {
            Some(base) => self.base_seen(&page_id, base, &current, turn).await?,
            None => None,
        };
        let mut problems = vec![];
        for (i, op) in ops.iter().enumerate() {
            let at = format!("op {}", i + 1);
            if let Some(problem) = proposal_problem(op, &current) {
                problems.push(Problem::new(at, problem));
            } else if read.is_none() && checked_target(op).is_some() {
                problems.push(Problem::new(
                    at,
                    format!(
                        "{} needs the base from your latest read of this page in this turn; that \
                         read was in an earlier turn, is no longer kept, or none was sent. Read it \
                         again.",
                        op_name(op)
                    ),
                ));
            }
        }
        // Refused here already: the crate's word on these ops would say
        // nothing the retry acts on.
        let refused_here: HashSet<usize> = problems
            .iter()
            .filter_map(|p| problem_op(&p.at))
            .map(|(i, _)| i)
            .collect();
        let mut unseen = vec![];
        let mut conflicts = vec![];
        if let Some(seen) = &read {
            let checked = virtues_document::check_ops(&current, seen, &ops);
            problems.extend(
                checked
                    .problems
                    .into_iter()
                    .filter(|p| problem_op(&p.at).is_none_or(|(i, _)| !refused_here.contains(&i))),
            );
            unseen = checked
                .unseen
                .into_iter()
                .filter(|u| !refused_here.contains(&u.op))
                .collect();
            conflicts = checked.conflicts.into_iter().filter(|i| !refused_here.contains(i)).collect();
        }
        // A comment in an op's HTML writes nothing: a block a read showed as
        // a comment, copied into a replace of the block around it, would go.
        // The check above refuses that too; this says why in the model's
        // own terms.
        for (i, op) in ops.iter().enumerate() {
            if op_html(op).is_some_and(|html| html.contains("<!--")) && !refused_here.contains(&i) {
                problems.push(Problem::new(format!("op {}", i + 1), COMMENT_WRITES_NOTHING));
            }
        }
        if !problems.is_empty() {
            return match &read {
                Some(seen) => self.refusal(&page_id, &ops, seen, problems, unseen, conflicts, &current, turn).await,
                None => Ok(refused(problems, None, String::new(), Unread::default(), false)),
            };
        }

        // Another machine write to this page waits until this one's versions
        // are cut, so neither reads the other as the owner's typing.
        let _turn = yjs.write_turn(&page_id).await;
        let kept = keep_restore_point(pool, yjs, &page_id).await?;
        let edit = match yjs.edit_tree(&page_id, read.as_ref(), &ops).await {
            Ok(edit) => edit,
            Err(TreeError::Refused { problems, tree }) => {
                return match &read {
                    Some(seen) => {
                        // The page changed between the check above and the
                        // write: what the write found, said as the check says
                        // it.
                        let checked = virtues_document::check_ops(&tree, seen, &ops);
                        self.refusal(&page_id, &ops, seen, problems, checked.unseen, checked.conflicts, &tree, turn)
                            .await
                    }
                    None => {
                        let problems = explain_missing(problems, &ops, None, &tree);
                        let blocks = target_html(&ops, &tree);
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
        let mut fit = Fit::new(WRITTEN_HTML_CHARS);
        let unshown: Vec<String> = written
            .iter()
            .filter(|n| !fit.whole(n))
            .filter_map(|n| n.id().map(str::to_string))
            .collect();
        let shown = Shown::html(fit.parts);
        // A written block too long to show is known as the model wrote it,
        // unless it was merged with a change made since, which only a read
        // shows.
        let merged: HashSet<String> = edit.applied.merged.iter().cloned().collect();
        let (unread, as_written): (Vec<String>, Vec<String>) =
            unshown.into_iter().partition(|id| merged.contains(id));
        let removed: Vec<String> = ops
            .iter()
            .filter_map(|op| match op {
                Op::Replace { id, .. } | Op::Delete { id } => Some(id.clone()),
                _ => None,
            })
            .collect();

        // What the model has now seen: its base, moved on by what it wrote
        // (`Seen::advance`), and the blocks shown above. A person's edit
        // elsewhere since the read is still unseen, and checked on the next
        // write. The edit is made either way, so a base that could not be
        // kept is reported as none, never as a failure that would invite the
        // edit a second time.
        let base = match &read {
            Some(seen) => {
                let mut next = seen.clone();
                next.advance(&before_tree, &after_tree, &merged);
                next.learn(&shown, &after_tree, &HashSet::new());
                match keep_base(pool, &page_id, &next, &after_tree, turn).await {
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
                if !as_written.is_empty() {
                    message.push_str(&format!(
                        " {}; that base holds {} as you wrote {}.",
                        too_long_to_show(&as_written),
                        them(&as_written),
                        them(&as_written),
                    ));
                }
                // Merged with a change made since and not shown: its next
                // edit reads it first, or is written from a copy that lacks
                // the change, which the merge then has to take.
                if !unread.is_empty() {
                    message.push_str(&format!(
                        " {}, merged with a change made since; read {} with get_page_content, ids \
                         and that base before editing {} again.",
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
            "html": shown.text(),
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
    /// with each op and what to do about it, the blocks it has to write
    /// again from what they hold now, shown as they are, and a base that has
    /// seen those, so a retry needs no read of its own where they fit. An
    /// op that was not refused is sent again as it was written, so the
    /// blocks it writes over stay in the base as the model read them, and
    /// the retry merges it with a person's change as the first try would
    /// have. Nothing was written.
    ///
    /// `unseen` is what the batch would have taken or written over that
    /// `read` has not seen as it stands (`virtues_document::check_ops`):
    /// each says what to show, or what to read and where. `conflicts`: the
    /// replaces refused because their block changed in a way they could
    /// not be merged with.
    #[allow(clippy::too_many_arguments)]
    async fn refusal(
        &self,
        page_id: &str,
        ops: &[Op],
        read: &Seen,
        problems: Vec<Problem>,
        unseen: Vec<Unseen>,
        conflicts: Vec<usize>,
        tree: &[Node],
        turn: Option<&str>,
    ) -> Result<ToolResult, ToolError> {
        // The batch is right and waits only on reads of blocks no read has
        // shown: each read and edit after it shows the model more of the
        // page, which the turn's guard does not count as a loop.
        let unread_only = !unseen.is_empty()
            && conflicts.is_empty()
            && unseen.iter().all(|u| matches!(u.why, Why::Unread { .. }));
        let unseen_count = unseen.len();
        let r = refusal_of(ops, read, problems, unseen, &conflicts, tree, WRITTEN_HTML_CHARS, &fits_one_read);
        // Each problem one of those, the refusal's own included.
        let read_first = unread_only && r.problems.len() == unseen_count;
        let mut next = read.clone();
        next.learn(&r.shown, tree, &r.keep);
        let new_base = keep_base(self.pool.as_ref(), page_id, &next, tree, turn).await?;
        let names_tags = r.problems.iter().any(|p| names_a_tag(&p.message));
        let mut result = refused(r.problems, Some(new_base), r.blocks, r.unread, names_tags);
        if read_first {
            result.data["read_first"] = json!(true);
        }
        Ok(result)
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

    /// What `base` names for this page: the base kept for it, or a read of
    /// the page as it is now when that hashes to `base`. `None` when
    /// neither, and in a chat turn for a base an earlier turn made
    /// (`page_reads::serves_turn`).
    async fn base_seen(
        &self,
        page_id: &str,
        base: &str,
        current: &[Node],
        turn: Option<&str>,
    ) -> Result<Option<Seen>, ToolError> {
        if !page_reads::serves_turn(base, turn) {
            return Ok(None);
        }
        let kept = page_reads::read_base(self.pool.as_ref(), page_id, base)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(match kept {
            Some(kept) => Some(kept),
            None if virtues_document::tree_hash(current) == page_reads::base_hash(base) => {
                Some(Seen::of_page(current))
            }
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

/// What a refusal says and shows ([`refusal_of`]).
struct Refusal {
    /// The problems, each said with what to do about it.
    problems: Vec<Problem>,
    /// What the reply shows, and the blocks it says are gone: what the
    /// refusal's base learns, but for `keep`.
    shown: Shown,
    /// The blocks the ops that were not refused write over, and the blocks
    /// inside them: they stay in the base as read.
    keep: HashSet<String>,
    /// The blocks shown, as HTML.
    blocks: String,
    /// The blocks to read by id.
    unread: Unread,
}

/// [`PageEditorTool::refusal`] without the base kept: what the refusal
/// says and shows, in `room` characters of HTML ([`WRITTEN_HTML_CHARS`]).
/// `fits` is whether a read shows a block whole ([`fits_one_read`]): a read
/// holds far more than a refusal shows, so a block a refusal cannot show is
/// one to read.
#[allow(clippy::too_many_arguments)]
fn refusal_of(
    ops: &[Op],
    read: &Seen,
    problems: Vec<Problem>,
    unseen: Vec<Unseen>,
    conflicts: &[usize],
    tree: &[Node],
    room: usize,
    fits: &dyn Fn(&Node) -> bool,
) -> Refusal {
    use virtues_document::ops::find_node;
    let mut problems = explain_missing(problems, ops, Some(read), tree);
    let refused_ops: HashSet<usize> = problems
        .iter()
        .filter_map(|p| problem_op(&p.at))
        .map(|(i, _)| i)
        .collect();
    let unseen: Vec<Unseen> = unseen
        .into_iter()
        .filter(|u| refused_ops.contains(&u.op))
        .collect();
    let with_unseen: HashSet<usize> = unseen.iter().map(|u| u.op).collect();
    let over_block: HashSet<usize> = with_unseen.iter().chain(conflicts).copied().collect();

    // The blocks the refused ops name, each shown as the block around it
    // a read has not shown when that one fits.
    let mut groups: Vec<(String, Vec<(usize, String)>)> = vec![];
    for (i, id) in to_rewrite(ops, &problems, read, tree, &over_block) {
        let shown = context_for(read, tree, &id).unwrap_or_else(|| id.clone());
        match groups.iter_mut().find(|(s, _)| *s == shown) {
            Some((_, asked)) => asked.push((i, id)),
            None => groups.push((shown, vec![(i, id)])),
        }
    }
    let mut fit = Fit::new(room);
    let mut gone: Vec<String> = vec![];
    let mut long: Vec<String> = vec![];
    let mut inside: Vec<String> = vec![];
    let place = page_order(tree);
    groups.sort_by_key(|(shown, _)| place.get(shown.as_str()).copied().unwrap_or(usize::MAX));
    for (shown, asked) in &groups {
        let Some(node) = find_node(tree, shown) else {
            // Gone from the page: seen gone.
            gone.push(shown.clone());
            continue;
        };
        if fit.whole(node) {
            continue;
        }
        // Too long to show here. An op refused over what the base has
        // not seen says what to read in its problem, below.
        if asked.iter().any(|(i, _)| with_unseen.contains(i)) {
            continue;
        }
        for (_, id) in asked {
            let Some(block) = find_node(tree, id) else { continue };
            if id != shown && fit.whole(block) {
                continue;
            }
            if !needs_a_read(read, block) {
                continue;
            }
            let names = if id == shown { &mut long } else { &mut inside };
            if !names.contains(id) {
                names.push(id.clone());
            }
        }
    }

    // What each block the base has not seen asks of the retry.
    let mut advice: Vec<(usize, String, String)> = vec![];
    for u in &unseen {
        let said = u.message();
        let first = u.id();
        let node = find_node(tree, first);
        let above = node.is_some_and(|n| fit.covers(n));
        let tell = match &u.why {
            // Taken with the block around it, which no read showed holding
            // it: that block is shown cut down to the way to it, which shows
            // it there, or read; the block alone would show it nowhere.
            Why::Unread { added_since: true } if u.inside.is_some() => {
                let holder = u.inside.as_deref().unwrap_or_default();
                let ids: Vec<&str> = u.ids.iter().map(String::as_str).collect();
                let around = find_node(tree, holder);
                let shown = around.is_some_and(|h| fit.covers(h))
                    || around.is_some_and(|h| fit.part(cut_down(h, &ids, &mut vec![])));
                if shown {
                    format!(
                        "`{holder}` is above as it is now, with {} inside it: write the op from it",
                        block_names(&u.ids)
                    )
                } else if !read.knows(first) && read.opaque_hash(first).is_none() {
                    // No read has shown it: it is read first, as any block
                    // is, on the way from the block around it when it was
                    // read elsewhere ([`context_for`]).
                    format!("read `{first}` with get_page_content, ids [\"{first}\"] and this base, first")
                } else if around.is_some_and(page_reads::opens)
                    && node.is_some_and(fits)
                    && read.known(holder).is_some_and(|k| virtues_document::seen::stubs(&k.shell).contains(&first))
                {
                    // Its read named it there without showing it: a read on
                    // inside that block shows it there.
                    format!("{}, first", read_on_advice(tree, first, false, fits))
                } else {
                    // Shown elsewhere, or listed: an op deleting it takes
                    // it, and one replacing it writes over it.
                    format!(
                        "{} `{first}` by its id in an op before op {}, or leave it as it is",
                        named_first(u.removes),
                        first_taker(ops, first, tree, u.op) + 1
                    )
                }
            }
            Why::Changed { .. } | Why::MarkdownOnly | Why::Unread { .. } | Why::ListedChanged
                if u.ids.len() == 1 && (above || node.is_some_and(|n| fit.whole(n))) =>
            {
                match &u.why {
                    Why::MarkdownOnly => format!(
                        "its HTML is above as it is now: write the {} from its HTML",
                        if u.removes { "move" } else { "op" }
                    ),
                    _ => "it is above as it is now: write the op from it".to_string(),
                }
            }
            Why::Changed { .. } => {
                if !long.iter().any(|l| l == first) {
                    long.push(first.to_string());
                }
                "read it as it is now".to_string()
            }
            Why::MarkdownOnly | Why::Unread { .. } if u.ids.len() == 1 && u.target => {
                if !long.iter().any(|l| l == first) {
                    long.push(first.to_string());
                }
                match u.why {
                    Why::MarkdownOnly => format!(
                        "read it as HTML, then write the {} from its HTML",
                        if u.removes { "move" } else { "op" }
                    ),
                    _ => "read it first".to_string(),
                }
            }
            Why::MarkdownOnly => format!(
                "{}, then write the op from their HTML",
                read_on_advice(tree, first, true, fits)
            ),
            Why::Unread { .. } => format!("{}, first", read_run_advice(tree, &u.ids, fits)),
            Why::NamedInside => format!(
                "{} `{first}` by its id in an op before op {}, or leave it as it is",
                named_first(u.removes),
                first_taker(ops, first, tree, u.op) + 1
            ),
            Why::ListedChanged => match node {
                // Still too long for any read: listed again, at its hash now.
                Some(n) if !fits(n) && !page_reads::opens(n) => {
                    fit.parts.push(page_reads::listed(n, page_reads::TooLong::WholeOnly));
                    "it is still too long for any read: an op naming it deletes it, or replaces it \
                     whole, with the base this reply returns"
                        .to_string()
                }
                // A read shows it now, or the blocks inside it: what it says
                // now is read before an op takes it.
                _ => {
                    if !long.iter().any(|l| l == first) {
                        long.push(first.to_string());
                    }
                    "read it as it is now".to_string()
                }
            },
            Why::Deleted => "leave it out unless you were asked to bring it back, and then write \
                 it without its data-id"
                .to_string(),
        };
        advice.push((u.op, said, tell));
    }
    for p in problems.iter_mut() {
        let Some((i, false)) = problem_op(&p.at) else { continue };
        if let Some((_, _, tell)) = advice.iter().find(|(op, said, _)| *op == i && *said == p.message) {
            p.message = format!("{}; {tell}", p.message);
        }
    }

    // A block that is gone is seen gone. The blocks the ops that were
    // not refused write over stay as read, the blocks inside them too:
    // sent again as written, those ops merge with a person's change as
    // the first try would have.
    let mut parts = fit.parts;
    let blocks = Shown::html(parts.clone()).text();
    gone.sort();
    gone.dedup();
    parts.extend(gone.into_iter().map(Part::Gone));
    // A block a refused op's problem is about is learned all the same: the
    // retry is refused over it until the base has seen it, and the reply
    // has shown it as it is now.
    let about: HashSet<String> = unseen
        .iter()
        .flat_map(|u| u.ids.iter())
        .filter_map(|id| find_node(tree, id))
        .flat_map(|n| ids_in(std::slice::from_ref(n)))
        .collect();
    // So an op that was not refused, whose block is one of those and
    // changed since the read, would be sent again as written against a base
    // that holds the block as it is now, and land over the change its first
    // try merged with. It is written again from the block shown, as the
    // refused ops are.
    let mut learned = read.clone();
    learned.learn(&Shown::html(parts.clone()), tree, &HashSet::new());
    for (i, op) in ops.iter().enumerate() {
        let Op::Replace { id, html } = op else { continue };
        if refused_ops.contains(&i) || html.trim().is_empty() || !about.contains(id) {
            continue;
        }
        let Some(now) = find_node(tree, id) else { continue };
        if read.seen_as_it_stands(now) || learned.known(id) == read.known(id) {
            continue;
        }
        problems.push(Problem::new(
            format!("op {}", i + 1),
            format!("the block `{id}` changed since you read it; it is above as it is now: write the op from it"),
        ));
    }
    let keep: HashSet<String> = kept_as_written(ops, &problems)
        .iter()
        .filter_map(|id| find_node(tree, id))
        .flat_map(|n| ids_in(std::slice::from_ref(n)))
        .filter(|id| !about.contains(id))
        .collect();
    Refusal {
        problems,
        shown: Shown::html(parts),
        keep,
        blocks,
        unread: Unread { long, inside },
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

async fn keep_base(
    pool: &PgPool,
    page_id: &str,
    seen: &Seen,
    page: &[Node],
    turn: Option<&str>,
) -> Result<String, ToolError> {
    page_reads::keep_read_base(pool, page_id, seen, page, turn)
        .await
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
}

/// The blocks a refusal could not show, to read by id: too long to show
/// here (`long`), or inside a block too long to show, which an `ids` read of
/// each cuts down to the way to it (`inside`).
#[derive(Default)]
struct Unread {
    long: Vec<String>,
    inside: Vec<String>,
}

/// A refused batch's result. A success to the caller, with `status:
/// "refused"`: the model acts on it in the same turn, the chat draws no
/// failed card, and the CLI exits 1 with its sentence.
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
/// into a replace of the block around it would take that block. The base
/// check refuses that on its own; this says why in the model's terms.
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
/// each with the op naming it: the blocks of the refused ops, each one only
/// when its op was refused over the block itself (it is gone, the read did
/// not show it as it stands, `over_block`: what the base has not seen of it
/// or a change the edit could not merge with, it holds a suggestion) or
/// when it has not changed since the read, which a refresh leaves as it
/// was.
///
/// A block that changed since the read and whose op was refused for
/// anything else is left out: a problem with the op's HTML (a tag, a
/// comment, HTML that holds no block or one its place cannot hold, a new
/// suggestion), whether it is charged to the op or to a place inside its
/// HTML, or with the batch's shape (a grid, a block an earlier op took).
/// The model fixes that from its copy, and the retry merges it with the
/// change as it stands. So is any block an op that was not refused
/// rewrites or deletes: it is sent again as written, from the copy the base
/// holds. Either brought into the base would let the retry land over a
/// person's change the first try would have kept.
fn to_rewrite(
    ops: &[Op],
    problems: &[Problem],
    read: &Seen,
    tree: &[Node],
    over_block: &HashSet<usize>,
) -> Vec<(usize, String)> {
    use virtues_document::ops::find_node;
    let mut refused_ops: Vec<usize> = problems.iter().filter_map(|p| problem_op(&p.at)).map(|(i, _)| i).collect();
    refused_ops.sort();
    refused_ops.dedup();
    let kept_as_written = kept_as_written(ops, problems);
    let mut out: Vec<(usize, String)> = vec![];
    for i in refused_ops {
        let Some(id) = ops.get(i).and_then(op_target) else {
            continue;
        };
        let now = find_node(tree, id);
        let unchanged = now.is_some_and(|now| read.subtree(id).as_ref() == Some(now));
        let suggested = now.is_some_and(|now| holds_proposal(std::slice::from_ref(now)));
        let rewrite = now.is_none() || !read.knows(id) || unchanged || over_block.contains(&i) || suggested;
        if rewrite && !kept_as_written.contains(id) && !out.iter().any(|(_, o)| o == id) {
            out.push((i, id.to_string()));
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

/// Each of the crate's "no block has id `x`" said as what happened to `x`,
/// with what to do instead, so a retry is not the same batch again:
///
/// - `x` is still on the page, so an op earlier in the batch took it away
///   (ops run in order): that op is named. A block too long for any read,
///   which only an op deleting it takes, is deleted first.
/// - the read had `x` and the page does not, so someone deleted it after the
///   read: a replace or delete leaves it out, an insert anchors elsewhere.
///
/// Any other problem, and a block never on the page, as it is.
fn explain_missing(
    problems: Vec<Problem>,
    ops: &[Op],
    read: Option<&Seen>,
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
                // Taken with a block around it, which only an op deleting it
                // may do: one too long for any read, or one no read showed
                // in the block around it. An op naming it already takes it.
                // Put first, this op takes it only if it deletes it: a
                // replace means the block to stay.
                let by_name = op_target(&ops[j]) != Some(id) && removes(op, id, tree);
                let opaque = by_name && read.is_some_and(|r| r.opaque_hash(id).is_some());
                let unshown_in = path_to(tree, id)
                    .and_then(|path| path.len().checked_sub(2).map(|at| path[at].clone()))
                    .filter(|holder| {
                        by_name && read.is_some_and(|r| r.knows(id) && r.known(holder).is_some_and(|k| !k.shows(id)))
                    });
                match op {
                    Op::Delete { .. } | Op::Replace { .. } if opaque => format!(
                        "op {earlier} removes the block `{id}` before this op runs, and ops run in \
                         order; `{id}` is too long for any read, and only an op deleting it takes it, \
                         so put this op before op {earlier}"
                    ),
                    Op::Delete { .. } | Op::Replace { .. } if unshown_in.is_some() => format!(
                        "op {earlier} removes the block `{id}` before this op runs, and ops run in \
                         order; no read has shown you `{id}` inside `{}`, so put this op before op {earlier}",
                        unshown_in.unwrap_or_default()
                    ),
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
            } else if read.is_some_and(|r| r.knows(id) || r.opaque_hash(id).is_some()) {
                // An item or a row goes only beside another: appended, it
                // would land where no list or table is.
                let part = match op {
                    Op::InsertAfter { html, .. } | Op::InsertBefore { html, .. } => part_of_a_block(html),
                    _ => None,
                };
                if let Some(part) = part.filter(|_| insert) {
                    format!(
                        "the block `{id}` was deleted after you read the page; anchor this op on \
                         another {part} beside where it was, or leave it out"
                    )
                } else if insert {
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

/// The op a refusal asks for before the one refused, by the block's id: a
/// delete takes a block an op would take with a block around it (a replace
/// means its block to stay), and a delete or a replace lets an op write
/// over it.
fn named_first(removes: bool) -> &'static str {
    if removes {
        "delete"
    } else {
        "delete or replace"
    }
}

/// The first op, up to `last`, that takes the block `id` off the page: an
/// op naming it goes before that one.
fn first_taker(ops: &[Op], id: &str, tree: &[Node], last: usize) -> usize {
    ops.iter()
        .take(last + 1)
        .position(|op| removes(op, id, tree))
        .unwrap_or(last)
}

/// What `html` writes when it is only a part of a block, which goes beside
/// another such part and never where a block goes: `item` for a list item,
/// `row` for a table row.
fn part_of_a_block(html: &str) -> Option<&'static str> {
    let lead = html.trim_start().get(..3)?.to_ascii_lowercase();
    match lead.as_str() {
        "<li" => Some("item"),
        "<tr" => Some("row"),
        _ => None,
    }
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
/// [`WRITTEN_HTML_CHARS`], as HTML.
fn target_html(ops: &[Op], tree: &[Node]) -> String {
    let targets: Vec<String> = ops
        .iter()
        .filter_map(op_target)
        .map(str::to_string)
        .collect();
    let mut fit = Fit::new(WRITTEN_HTML_CHARS);
    for node in in_document_order(tree, &targets) {
        fit.whole(&node);
    }
    Shown::html(fit.parts).text()
}

/// Blocks shown whole in a result, as many as fit in `limit` characters of
/// HTML with ids, a block that does not fit passed over for the ones after
/// it.
struct Fit {
    parts: Vec<Part>,
    used: usize,
    limit: usize,
    /// Every block inside the blocks shown.
    covered: HashSet<String>,
}

impl Fit {
    fn new(limit: usize) -> Self {
        Fit {
            parts: vec![],
            used: 0,
            limit,
            covered: HashSet::new(),
        }
    }

    /// Show `node` whole if it fits, or say it is shown already.
    fn whole(&mut self, node: &Node) -> bool {
        if self.covers(node) {
            return true;
        }
        let n = chars(&virtues_document::to_html(std::slice::from_ref(node), true));
        if self.used + n > self.limit {
            return false;
        }
        self.used += n;
        self.covered.extend(ids_in(std::slice::from_ref(node)));
        self.parts.push(Part::Whole(node.clone()));
        true
    }

    /// Whether `node` is shown, whole or inside a block shown whole.
    fn covers(&self, node: &Node) -> bool {
        node.id().is_some_and(|id| self.covered.contains(id))
    }

    /// Show `part` (a block cut down to the way to some inside it) if it
    /// fits.
    fn part(&mut self, part: Part) -> bool {
        let n = chars(&Shown::html(vec![part.clone()]).text());
        if self.used + n > self.limit {
            return false;
        }
        self.used += n;
        self.covered.extend(ids_in(&wholes(std::slice::from_ref(&part))));
        self.parts.push(part);
        true
    }
}

/// The ids of the blocks from the page's top level down to `id`, `id` last;
/// a table cell, which carries none, is passed through. `None` when the page
/// has no block `id`.
fn path_to(tree: &[Node], id: &str) -> Option<Vec<String>> {
    fn walk(nodes: &[Node], id: &str, path: &mut Vec<String>) -> bool {
        for n in nodes {
            let pushed = n.id().map(|own| path.push(own.to_string())).is_some();
            if n.id() == Some(id) || walk(&n.content, id, path) {
                return true;
            }
            if pushed {
                path.pop();
            }
        }
        false
    }
    let mut path = vec![];
    walk(tree, id, &mut path).then_some(path)
}

/// Each block of the page by its place in document order.
fn page_order(tree: &[Node]) -> std::collections::HashMap<String, usize> {
    let mut out = std::collections::HashMap::new();
    for n in tree {
        n.walk(&mut |b| {
            if let Some(id) = b.id() {
                let at = out.len();
                out.insert(id.to_string(), at);
            }
        });
    }
    out
}

/// The block a read shows for the block `id`: `id` itself when a read has
/// shown it where it is or shown the block around it, or else the
/// outermost block around it no read has shown, so it comes with its
/// context. One the block around it, as read, does not show inside it
/// (added or moved there since, or counted there once a read had shown it
/// elsewhere) comes with that block, read or not, cut down to the way to it
/// when it is long: the read shows it there, and so in no block a read
/// showed it in before. A display choice only: what enters the base is what
/// the read shows. `None` when the page has no block `id`.
fn context_for(seen: &Seen, tree: &[Node], id: &str) -> Option<String> {
    let path = path_to(tree, id)?;
    let around = &path[..path.len() - 1];
    if seen.knows(id) {
        let holder = around.last().and_then(|p| seen.known(p).map(|k| (p, k)));
        return Some(match holder {
            Some((p, k)) if !k.shows(id) => p.clone(),
            _ => id.to_string(),
        });
    }
    Some(match around.iter().rposition(|a| seen.knows(a)) {
        Some(k) if seen.known(&path[k]).is_some_and(|holder| !holder.shows(&path[k + 1])) => path[k].clone(),
        Some(k) => path[k + 1].clone(),
        None => path[0].clone(),
    })
}

/// Whether a block a refusal has no room to show has to be read before its
/// op is written again: unless the base holds it as it stands, all of it as
/// HTML, or it is too long for any read and still as it was listed.
fn needs_a_read(read: &Seen, node: &Node) -> bool {
    let Some(id) = node.id() else { return true };
    if read.opaque_hash(id) == Some(virtues_document::node_hash(node).as_str()) {
        return false;
    }
    let mut as_html = true;
    node.walk(&mut |b| {
        if let Some(bid) = b.id() {
            as_html &= read.known(bid).is_some_and(|k| k.form == Form::Html);
        }
    });
    !(as_html && read.subtree(id).as_ref() == Some(node))
}

/// The read that shows the block `first` (the first of what a refused op
/// would take or write over unseen), and what follows it: inside the
/// deepest block around it too long for one read, on from just before the
/// block there that leads to it. By the leading block's id when it is the
/// first there (an `ids` read of the block around would open it with less
/// room than a read, its tags and the comment after taking some, and could
/// only list it), when it is itself too long for one read (an `ids` read
/// opens it), or when the block around holds it in a table cell, which a
/// read on does not cross.
fn read_on_advice(tree: &[Node], first: &str, as_html: bool, fits: &dyn Fn(&Node) -> bool) -> String {
    use virtues_document::ops::find_node;
    let how = if as_html { "as HTML " } else { "" };
    let by_id = |id: &str| format!("read `{id}` {how}with get_page_content, ids [\"{id}\"] and this base");
    let Some(path) = path_to(tree, first) else {
        return by_id(first);
    };
    let nodes: Vec<&Node> = path.iter().filter_map(|id| find_node(tree, id)).collect();
    if nodes.len() != path.len() {
        return by_id(first);
    }
    let Some(j) = (0..path.len() - 1).rev().find(|&j| !fits(nodes[j])) else {
        // Everything around it fits one read.
        return by_id(&path[0]);
    };
    let (outer, lead) = (nodes[j], &path[j + 1]);
    if !page_reads::opens(outer) || !fits(nodes[j + 1]) {
        return by_id(lead);
    }
    let at = outer.content.iter().position(|c| c.id() == Some(lead.as_str()));
    match at.and_then(|i| i.checked_sub(1)).and_then(|i| outer.content[i].id()) {
        Some(before) => format!(
            "read the rest of `{}` {how}with get_page_content, after \"{before}\" and this base",
            path[j]
        ),
        None => by_id(lead),
    }
}

/// [`read_on_advice`] for a run of blocks no read has shown, `ids` in page
/// order. Where they lie in a block too long for any read that cannot be
/// opened (a table row, whose cells carry no id), one `ids` read names them
/// all: no read on goes inside that block, and a read of the first shows
/// the others only as a count, so naming one at a time took a refusal for
/// each block in the row's cells.
fn read_run_advice(tree: &[Node], ids: &[String], fits: &dyn Fn(&Node) -> bool) -> String {
    use virtues_document::ops::find_node;
    let Some(first) = ids.first() else {
        return String::new();
    };
    let in_unopened = path_to(tree, first).is_some_and(|path| {
        path[..path.len() - 1]
            .iter()
            .filter_map(|id| find_node(tree, id))
            .any(|n| !fits(n) && !page_reads::opens(n))
    });
    if ids.len() < 2 || !in_unopened {
        return read_on_advice(tree, first, false, fits);
    }
    let listed = ids.iter().map(|id| format!("\"{id}\"")).collect::<Vec<_>>().join(", ");
    let named = ids.iter().map(|id| format!("`{id}`")).collect::<Vec<_>>().join(", ");
    format!("read {named} with get_page_content, ids [{listed}] and this base")
}

/// The blocks shown whole in `parts`, at any depth, in order.
fn wholes(parts: &[Part]) -> Vec<Node> {
    let mut out = vec![];
    for part in parts {
        match part {
            Part::Whole(n) => out.push(n.clone()),
            Part::Opened { inside, .. } | Part::Cell { inside, .. } => out.extend(wholes(inside)),
            Part::Listed { .. } | Part::More { .. } | Part::Gone(_) => {}
        }
    }
    out
}

/// What an `ids` read shows of the blocks asked for ([`read_by_id`]).
#[derive(Default)]
struct ByIdRead {
    /// What it shows, as HTML, in the order asked: its text and what its
    /// base learns are both read off these.
    parts: Vec<Part>,
    /// Asked for, and not on the page.
    missing: Vec<String>,
    /// Blocks there was no room left for; another `ids` read shows them.
    unread: Vec<String>,
    /// Blocks too long for any read that hold no blocks to show instead,
    /// listed by a comment.
    whole_only: Vec<String>,
    /// A block too long to show whole, shown by the blocks inside it.
    opened: Option<Opened>,
    /// Each block shown cut down to the way to blocks asked for inside it:
    /// its id, the ids of the blocks cut down, and the blocks asked for.
    cut: Vec<(String, Vec<String>, Vec<String>)>,
}

/// A block an `ids` read showed by the blocks inside it ([`open_block`]).
struct Opened {
    id: String,
    /// How many blocks it holds.
    holds: usize,
    /// How many of them the read showed whole.
    shown: usize,
    /// The blocks inside it too long for any read, shown as a comment.
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
             Edit those by id. Replacing or deleting `{id}` waits until reads have shown you all of \
             the blocks inside it.",
            self.shown, self.holds
        );
        if !self.whole_only.is_empty() {
            let it = them(&self.whole_only);
            note.push(' ');
            note.push_str(&format!(
                "{} inside it {} too long for any read, shown as a comment: a replace of `{id}` that \
                 leaves {it} out, or a delete of `{id}`, is refused while {it} {} there. Delete or \
                 replace {it} by id in an op before it.",
                block_names(&self.whole_only),
                if self.whole_only.len() == 1 { "is" } else { "are" },
                if self.whole_only.len() == 1 { "is" } else { "are" },
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
        .filter(|id| !window.shown.contains(id) && !window.whole_only.contains(id))
        .cloned()
        .collect();
    (!only.is_empty()).then(|| listed_by_id(&only))
}

/// `ids` as an `ids` read shows them, in `limit` as `measure` counts: each
/// block whole while there is room, with the block around it no read has
/// shown when that fits one read ([`context_for`]), or cut down to the way
/// to it when that does not ([`cut_down`]), or alone when not even the way
/// to it fits; a block too long for any read
/// listed by id, or, when it holds blocks with ids, opened
/// ([`open_block`]), one per read. A block asked for again is shown again,
/// opened again, or said to be too long, never passed over twice for the
/// same reason.
fn read_by_id(
    seen: &Seen,
    tree: &[Node],
    ids: &[String],
    limit: usize,
    measure: impl Fn(&str) -> usize + Copy,
) -> ByIdRead {
    use virtues_document::ops::find_node;
    let html = |n: &Node| virtues_document::to_html(std::slice::from_ref(n), true);
    let mut read = ByIdRead::default();
    // The blocks asked for, by the block shown for them, in the order
    // asked: several items of one list no read has shown are one list. A
    // block too long for one read is shown by itself, as it can be.
    let mut groups: Vec<(String, Vec<String>)> = vec![];
    for id in ids {
        let Some(node) = find_node(tree, id) else {
            read.missing.push(id.clone());
            continue;
        };
        let shown = if measure(&html(node)) <= limit {
            context_for(seen, tree, id).unwrap_or_else(|| id.clone())
        } else {
            id.clone()
        };
        match groups.iter_mut().find(|(s, _)| *s == shown) {
            Some((_, asked)) => asked.push(id.clone()),
            None => groups.push((shown, vec![id.clone()])),
        }
    }
    let mut room = limit;
    for (shown, asked) in &groups {
        let Some(node) = find_node(tree, shown) else {
            continue;
        };
        let n = measure(&html(node));
        if !asked.contains(shown) && n > limit {
            // The way to as many of them as fit, in the order asked; the
            // rest are another read's. Each shown alone instead would be no
            // block shown there, inside the block around them.
            let mut fitted: Option<(Part, Vec<String>, usize, usize)> = None;
            for k in 1..=asked.len() {
                let targets: Vec<&str> = asked[..k].iter().map(String::as_str).collect();
                let mut cut = vec![];
                let part = cut_down(node, &targets, &mut cut);
                let said = measure(&Shown::html(vec![part.clone()]).text());
                if said > room {
                    break;
                }
                fitted = Some((part, cut, k, said));
            }
            if let Some((part, cut, k, said)) = fitted {
                room -= said;
                read.parts.push(part);
                read.cut.push((shown.clone(), cut, asked[..k].to_vec()));
                read.unread.extend(asked[k..].iter().cloned());
                continue;
            }
            // The way to even one of them takes more than the read holds (a
            // row too long for any read, its tags and its other cells'
            // comments), and no read would hold it: each is shown alone, as
            // it fits one.
            for id in asked {
                let Some(block) = find_node(tree, id) else { continue };
                let m = measure(&html(block));
                if m <= room {
                    room -= m;
                    read.parts.push(Part::Whole(block.clone()));
                } else {
                    read.unread.push(id.clone());
                }
            }
            continue;
        }
        if n <= room {
            room -= n;
            read.parts.push(Part::Whole(node.clone()));
            continue;
        }
        if n <= limit {
            read.unread.push(shown.clone());
            continue;
        }
        if !page_reads::opens(node) {
            let part = page_reads::listed(node, page_reads::TooLong::WholeOnly);
            let said = measure(&Shown::html(vec![part.clone()]).text());
            if said <= room {
                room -= said;
                read.parts.push(part);
                read.whole_only.push(shown.clone());
            } else {
                read.unread.push(shown.clone());
            }
            continue;
        }
        match read.opened.is_none().then(|| open_block(node, room, limit, measure)).flatten() {
            Some((part, opened)) => {
                room = room.saturating_sub(measure(&Shown::html(vec![part.clone()]).text()));
                read.parts.push(part);
                read.opened = Some(opened);
            }
            None => read.unread.push(shown.clone()),
        }
    }
    read
}

/// `node` cut down to the way to each of `targets` inside it: each block on
/// the way shown by its own tags around only the blocks that lead on, a
/// table cell on the way the same, a cell off it as a comment counting what
/// it holds, and each target whole. Also the ids of the blocks cut down,
/// outermost first, into `cut`.
fn cut_down(node: &Node, targets: &[&str], cut: &mut Vec<String>) -> Part {
    use virtues_document::ops::find_node;
    let is_target = |n: &Node| n.id().is_some_and(|id| targets.contains(&id));
    if is_target(node) {
        return Part::Whole(node.clone());
    }
    let holds = |n: &Node| is_target(n) || targets.iter().any(|t| find_node(&n.content, t).is_some());
    let at = cut.len();
    let mut inside = vec![];
    let mut passed = false;
    for child in &node.content {
        if holds(child) {
            inside.push(cut_down(child, targets, cut));
        } else if child.id().is_none() && !child.is_text() {
            inside.push(Part::Cell {
                cell: child.clone(),
                inside: vec![Part::More {
                    count: child.content.len(),
                    after: None,
                }],
            });
            passed = true;
        } else {
            passed = true;
        }
    }
    match node.id() {
        Some(id) => {
            if passed {
                cut.insert(at, id.to_string());
            }
            Part::Opened {
                shell: virtues_document::seen::shell(node),
                inside,
            }
        }
        None => Part::Cell {
            cell: node.clone(),
            inside,
        },
    }
}

/// The most a comment about the blocks not shown inside an opened block
/// takes, kept free for it.
const MORE_INSIDE: usize = 160;

/// `node`, too long to show whole, as its own tags around the blocks inside
/// it that fit in `room` ([`children_window`]), and a comment where the
/// rest go. `None` when not even its tags and one block fit.
fn open_block(
    node: &Node,
    room: usize,
    limit: usize,
    measure: impl Fn(&str) -> usize + Copy,
) -> Option<(Part, Opened)> {
    let id = node.id()?.to_string();
    let shell = virtues_document::seen::shell(node);
    let (open, close) = virtues_document::seen::tags(&shell);
    if close.is_empty() {
        return None;
    }
    let inner_room = room.checked_sub(measure(&open) + measure(&close) + MORE_INSIDE)?;
    let window = children_window(&node.content, 0, inner_room, limit, measure);
    if window.listed.is_empty() {
        return None;
    }
    let mut inside = window.parts.clone();
    if let Some(next) = &window.next {
        inside.push(Part::More {
            count: node.content.len() - window.listed.len(),
            after: Some(next.clone()),
        });
    }
    let listed_only = window
        .listed
        .iter()
        .filter(|id| !window.shown.contains(id) && !window.whole_only.contains(id))
        .cloned()
        .collect();
    Some((
        Part::Opened { shell, inside },
        Opened {
            id,
            holds: node.content.len(),
            shown: window.shown.len(),
            whole_only: window.whole_only,
            listed_only,
            next: window.next,
        },
    ))
}

/// Blocks shown one after another, as HTML ([`children_window`]).
#[derive(Default)]
struct ChildWindow {
    parts: Vec<Part>,
    /// The ids of the blocks shown or listed by a comment, in order.
    listed: Vec<String>,
    /// Of those, the ones shown whole.
    shown: Vec<String>,
    /// Of those, the ones too long for any read and not ones to open,
    /// listed with their hash.
    whole_only: Vec<String>,
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
    let mut window = ChildWindow::default();
    let mut used = 0;
    let mut end = start;
    for node in nodes.iter().skip(start) {
        let Some(id) = node.id() else { break };
        let one = virtues_document::to_html(std::slice::from_ref(node), true);
        let alone = measure(&one);
        let (part, whole, whole_only) = if used + alone <= room {
            (Part::Whole(node.clone()), true, false)
        } else if alone > limit {
            let kind = page_reads::TooLong::of(node, false);
            (page_reads::listed(node, kind), false, kind == page_reads::TooLong::WholeOnly)
        } else if window.listed.is_empty() {
            (
                Part::Listed {
                    id: id.to_string(),
                    say: "too long to show here; read it with get_page_content and ids".into(),
                    opaque: None,
                },
                false,
                false,
            )
        } else {
            break;
        };
        let n = if whole { alone } else { measure(&Shown::html(vec![part.clone()]).text()) };
        if used + n > room {
            break;
        }
        used += n;
        window.parts.push(part);
        window.listed.push(id.to_string());
        if whole {
            window.shown.push(id.to_string());
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
    let it = them(ids);
    format!(
        "{} {} too long to show in any read. An op naming {it} deletes {it}, or replaces {it} \
         whole; a replace or delete of a block around {it} does not take {it}.",
        block_names(ids),
        if ids.len() == 1 { "is" } else { "are" },
    )
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

/// What a read says of `outer` shown cut down to `shown`, the blocks asked
/// for inside it, every one of them in the copy shown.
fn cut_down_note(outer: &str, cut: &[String], shown: &[String]) -> String {
    let inside: Vec<String> = shown.iter().filter(|id| *id != outer).cloned().collect();
    format!(
        "{} {} shown inside `{outer}`, with only the blocks on the way to {}: edit {} by id. \
         Replacing or deleting {} waits until reads have shown all of {}.",
        block_names(&inside),
        if inside.len() == 1 { "is" } else { "are" },
        them(&inside),
        them(&inside),
        cut.iter().map(|id| format!("`{id}`")).collect::<Vec<_>>().join(", "),
        if cut.len() == 1 { "it" } else { "them" },
    )
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

    /// A block in a cell of a row too long for any read, which fits a read
    /// alone but not with the way to it (the row's tags and its other
    /// cells' comments), is shown alone: left unread, a refusal's advice to
    /// read it by id would come back the same, read after read.
    #[test]
    fn a_block_in_a_long_rows_cell_is_read_alone_when_the_way_to_it_does_not_fit() {
        use virtues_document::ops::find_node;
        let long = "harbour ".repeat(200);
        let items: String = (0..6).map(|i| format!("<li data-id=\"i{i}\"><p data-id=\"p{i}\">Item {i}</p></li>")).collect();
        let html = format!(
            "<table data-id=\"t\"><tr data-id=\"r\"><td><p data-id=\"c\">{long}</p></td>\
             <td><ul data-id=\"l\">{items}</ul></td></tr></table>"
        );
        let parsed = virtues_document::parse_html(&html, "doc");
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let tree = parsed.nodes;
        let node = |id: &str| find_node(&tree, id).unwrap().clone();
        let mut seen = Seen::default();
        seen.learn(
            &Shown::html(vec![Part::Opened {
                shell: virtues_document::seen::shell(&node("t")),
                inside: vec![page_reads::listed(&node("r"), page_reads::TooLong::WholeOnly)],
            }]),
            &tree,
            &HashSet::new(),
        );
        let list = virtues_document::to_html(&[node("l")], true);
        let limit = json_bytes(&list) + 8;
        let read = read_by_id(&seen, &tree, &["l".to_string()], limit, json_bytes);
        assert_eq!(read.unread, Vec::<String>::new());
        assert!(matches!(&read.parts[..], [Part::Whole(n)] if n.id() == Some("l")), "{:?}", read.parts.len());
        seen.learn(&Shown::html(read.parts), &tree, &HashSet::new());
        assert_eq!(seen.subtree("l"), Some(node("l")));
    }

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

    async fn kept_base(pool: &PgPool, page_id: &str, base: &str) -> Option<Seen> {
        page_reads::read_base(pool, page_id, base).await.unwrap()
    }

    /// The text of the block `id` as the base holds it.
    fn known_text(seen: &Seen, id: &str) -> Option<String> {
        seen.subtree(id).map(|n| n.text_content())
    }

    /// The blocks inside the block `id` that the base holds, in page order.
    fn known_children(seen: &Seen, id: &str) -> Vec<String> {
        seen.known(id)
            .map(|k| {
                virtues_document::seen::stubs(&k.shell)
                    .into_iter()
                    .filter(|c| seen.knows(c))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether the base holds every block of `nodes` as it stands.
    fn holds_as_it_stands(seen: &Seen, nodes: &[Node]) -> bool {
        nodes.iter().all(|n| seen.subtree(n.id().unwrap()).as_ref() == Some(n))
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

    /// A chat turn's results replay to later turns clipped (to 32 KiB, then
    /// 2 KiB), while a base would still let a replace write over all a read
    /// showed. So a base serves the turn that made it, the edits it chains
    /// included, and a later turn's replace or delete with it is refused,
    /// writing nothing, until a read in that turn; the CLI, which has no
    /// turns, takes any base.
    #[sqlx::test]
    async fn a_base_serves_only_the_turn_that_made_it(pool: PgPool) {
        let page_id = tree_page(&pool, "Groceries\n\n- Milk\n- Eggs\n- Bread\n").await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let in_turn = |turn: &str| ToolContext { turn: Some(turn.into()), ..Default::default() };
        let first = tool
            .get_page_content(json!({ "page_id": page_id }), &in_turn("turn_1"))
            .await
            .unwrap()
            .data;
        let base = first["base"].as_str().unwrap().to_string();
        let tree = yjs.read_tree(&page_id).await.unwrap();
        assert_eq!(page_reads::base_hash(&base), virtues_document::tree_hash(&tree));
        let list = tree[1].id().unwrap().to_string();
        let replace = |base: &str, html: &str| {
            json!({ "page_id": page_id, "base": base, "ops": [{ "op": "replace", "id": list, "html": html }] })
        };

        // The turn that read it edits with it, and chains on what the edit returns.
        let done = tool
            .edit_page(replace(&base, "<ul><li><p>Milk</p></li><li><p>Eggs</p></li></ul>"), &in_turn("turn_1"))
            .await
            .unwrap();
        assert_eq!(done.data["applied"], true, "{:?}", done.data);
        let chained = done.data["base"].as_str().unwrap().to_string();

        // A later turn holds only part of that read: its replace is refused.
        for base in [&base, &chained] {
            let refused = tool
                .edit_page(replace(base, "<ul><li><p>Milk</p></li></ul>"), &in_turn("turn_2"))
                .await
                .unwrap();
            assert_eq!(refused.data["applied"], false, "{:?}", refused.data);
            assert!(refused.data["error"].as_str().unwrap().contains("in an earlier turn"), "{:?}", refused.data);
        }
        assert_eq!(texts(&yjs.read_tree(&page_id).await.unwrap()), ["Groceries", "MilkEggs"]);
        let ids_read = tool
            .get_page_content(json!({ "page_id": page_id, "ids": [list], "base": chained }), &in_turn("turn_2"))
            .await;
        assert!(ids_read.is_err(), "an ids read adds to a read of its own turn");

        // Read again in that turn, it edits.
        let again = tool
            .get_page_content(json!({ "page_id": page_id }), &in_turn("turn_2"))
            .await
            .unwrap()
            .data;
        let base = again["base"].as_str().unwrap();
        let done = tool
            .edit_page(replace(base, "<ul><li><p>Milk</p></li></ul>"), &in_turn("turn_2"))
            .await
            .unwrap();
        assert_eq!(done.data["applied"], true, "{:?}", done.data);
        // Outside a turn, any base.
        let cli = edit(&tool, replace(done.data["base"].as_str().unwrap(), "<ul><li><p>Oats</p></li></ul>")).await;
        assert_eq!(cli["applied"], true);
        assert!(page_reads::serves_turn(base, None));
        assert!(!page_reads::serves_turn(&virtues_document::tree_hash(&tree), Some("turn_2")));
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
        assert_eq!(base, virtues_document::tree_hash(&tree));
        assert_eq!(kept_base(&pool, &page_id, base).await, Some(Seen::of_page(&tree)));
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
        assert!(holds_as_it_stands(&kept, &tree));
        assert!(ids_in(&tree).iter().all(|id| kept.known(id).is_some_and(|k| k.form == Form::Markdown)));
        assert!(data.get("more_after").is_none());
    }

    /// A page whose HTML is short in characters and long in the bytes its
    /// result takes (a control character is six bytes in JSON) was shown
    /// whole: the agent loop cut the result at 96 KiB, mid-page, after a
    /// base that claimed every block, and a replace written from the part
    /// shown would drop a tail no read showed. It reads as markdown, as far
    /// as one read holds, inside the cut.
    #[sqlx::test]
    async fn a_full_read_fits_the_loops_cut_in_bytes_not_only_characters(pool: PgPool) {
        let code = "\u{1b}\u{7}".repeat(11_950);
        let page_id = tree_page(&pool, &format!("Log:\n\n```\n{code}\n```\n\nEnd.\n")).await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let html = virtues_document::to_html(&tree, true);
        assert!(html.contains(&code), "the code keeps its control characters");
        assert!(chars(&html) <= READ_HTML_CHARS, "{}", chars(&html));
        assert!(json_bytes(&html) > crate::agent::executor::MAX_TOOL_OUTPUT_BYTES);

        let first = read(&tool, json!({ "page_id": page_id })).await;
        assert!(first.get("html").is_none(), "shown whole as HTML past the cut");
        assert!(first["markdown"].is_string(), "{first}");
        assert!(serde_json::to_string(&first).unwrap().len() < crate::agent::executor::MAX_TOOL_OUTPUT_BYTES);
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
        let kept = kept_base(&pool, &page_id, &base1).await.unwrap();
        assert!(holds_as_it_stands(&kept, &tree[..k]));
        assert!(!kept.knows(tree[k].id().unwrap()));
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
        assert!(holds_as_it_stands(&kept_base(&pool, &page_id, &base).await.unwrap(), &now));

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
        let held: Vec<Option<String>> = ids.iter().map(|id| known_text(&seen, id)).collect();
        assert_eq!(held, [Some("Coffee.".into()), Some("Green tea.".into()), Some("Juice.".into())]);

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
        assert_eq!(data["base"], virtues_document::tree_hash(&tree));

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
        assert_eq!(kept_base(&pool, &page_id, next).await.unwrap(), Seen::of_page(&tree));

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

    /// A block too long to show in an edit's reply is named: the base holds
    /// it as the model wrote it, so its next edit needs no read. A refusal
    /// over it once a person has changed it says to read it by id, where
    /// "send the batch again" with a base that has not seen the change would
    /// be refused again, every time.
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
        assert!(wrote.get("unread").is_none(), "{wrote}");
        assert!(wrote["message"].as_str().unwrap().ends_with(&format!(
            "The block `{id}` is too long to show here; that base holds it as you wrote it."
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
        assert!(message.contains("that no read has shown you"), "{message}");
        assert!(message.contains(&format!("inside `{list}`")), "{message}");
        assert!(message.contains(&format!("after \"{next}\" and this base")), "{message}");
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
        assert_eq!(known_children(&kept, &list), [items[0].clone(), items[2].clone()]);

        for op in [
            json!({ "op": "replace", "id": list, "html": html.replace("Phase one", "Phase one, done") }),
            json!({ "op": "replace", "id": items[1], "html": "<li><p>Phase two: build and test</p></li>" }),
        ] {
            refused_whole(&tool, &yjs, &page_id, json!({ "page_id": page_id, "base": base, "ops": [op] })).await;
        }
        assert_eq!(yjs.read_tree(&page_id).await.unwrap(), tree);
    }

    /// A quote holding a paragraph too long for any read, opened by id: its
    /// first paragraph, and a comment for the long one. Its page, the quote,
    /// the long paragraph's id, and the opened read.
    async fn quote_around_a_long_paragraph(pool: &PgPool, tool: &PageEditorTool, yjs: &YjsState) -> (String, String, String, Value) {
        let long_line = "ridge ".repeat(14_000);
        let page_id = tree_page(pool, &format!("Notes:\n\n> Teh log from the ridge:\n>\n> {long_line}\n")).await;
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let quote = tree[1].id().unwrap().to_string();
        let inner = child_ids(&tree, &quote);
        assert_eq!(inner.len(), 2);
        assert!(!fits_one_read(&tree[1].content[1]), "the paragraph fits one read");
        let first = read(tool, json!({ "page_id": page_id })).await;
        let opened = read(tool, json!({ "page_id": page_id, "ids": [quote], "base": first["base"] })).await;
        (page_id, quote, inner[1].clone(), opened)
    }

    /// A paragraph inside a quote too long for any read is shown as a
    /// comment. The model's replace of the quote, written from that read
    /// with a typo fixed, copied the comment, which wrote nothing: the long
    /// paragraph went, and the edit said it was on the page. Refused, as
    /// taking a block no read can show by an op on the block around it, and
    /// as a comment, which writes nothing; the read's note says so first.
    #[sqlx::test]
    async fn a_block_shown_as_a_comment_is_never_written_over_by_copying_the_comment(pool: PgPool) {
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let (page_id, quote, long, opened) = quote_around_a_long_paragraph(&pool, &tool, &yjs).await;
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let html = opened["html"].as_str().unwrap();
        assert!(html.contains(&format!("<!-- {long}: too long to show in any read")), "{html}");
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
        let error = refused["error"].as_str().unwrap();
        assert!(error.contains("HTML comment"), "{refused}");
        assert!(error.contains(&format!("`{long}`, which is too long for any read")), "{refused}");
        assert_eq!(yjs.read_tree(&page_id).await.unwrap(), tree);
    }

    /// The same replace with the comment left out took the long paragraph
    /// just the same: the base held the quote's blocks as read, the long one
    /// "seen" as listed, so nothing in the quote counted as unread. A block
    /// too long for any read goes only by an op deleting it, ahead of any op
    /// on a block around it; the refusal says so, and that batch lands.
    #[sqlx::test]
    async fn a_replace_that_leaves_out_a_block_too_long_for_any_read_is_refused(pool: PgPool) {
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let (page_id, quote, long, opened) = quote_around_a_long_paragraph(&pool, &tool, &yjs).await;
        let base = opened["base"].clone();
        let fixed = json!({ "op": "replace", "id": quote, "html": "<blockquote><p>The log from the ridge:</p></blockquote>" });
        let refused = refused_whole(&tool, &yjs, &page_id, json!({ "page_id": page_id, "base": base, "ops": [fixed] })).await;
        let message = refused["refused"][0]["message"].as_str().unwrap();
        assert!(message.contains(&format!("`{long}`, which is too long for any read")), "{message}");
        assert!(message.contains(&format!("delete `{long}` by its id in an op before op 1")), "{message}");

        // Deleting the quote takes it the same way; after it, the op naming
        // it finds it gone, and is told to go first.
        let delete_quote = json!({ "op": "delete", "id": quote });
        let delete_long = json!({ "op": "delete", "id": long });
        refused_whole(&tool, &yjs, &page_id, json!({ "page_id": page_id, "base": base, "ops": [delete_quote] })).await;
        let late = refused_whole(
            &tool,
            &yjs,
            &page_id,
            json!({ "page_id": page_id, "base": base, "ops": [delete_quote, delete_long] }),
        )
        .await;
        let named = late["refused"].as_array().unwrap().iter().find(|p| p["at"] == "op 2").unwrap();
        assert!(named["message"].as_str().unwrap().contains("put this op before op 1"), "{late}");

        let wrote = edit(&tool, json!({ "page_id": page_id, "base": base, "ops": [delete_long, fixed] })).await;
        assert_eq!(wrote["applied"], true, "{wrote}");
        assert_eq!(texts(&yjs.read_tree(&page_id).await.unwrap()), ["Notes:", "The log from the ridge:"]);
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

    /// A refusal's read on starts just before the first block not read,
    /// inside the deepest block around it too long for one read: from just
    /// before it, or with an ids read of it when it is the first there;
    /// around a block that fits one read, that block whole.
    #[test]
    fn a_read_on_starts_before_the_first_block_not_read() {
        let items: String = (0..1_000)
            .map(|i| format!("<li data-id=\"i{i}\"><p data-id=\"p{i}\">Item {i} for the trip to the coast, packed by the door</p></li>"))
            .collect();
        let o = virtues_document::parse_html(
            &format!("<ul data-id=\"l\">{items}</ul><ul data-id=\"s\"><li data-id=\"s0\"><p data-id=\"t0\">Short</p></li></ul>"),
            "doc",
        );
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        let tree = o.nodes;
        assert!(!fits_one_read(&tree[0]), "the list fits one read");
        assert_eq!(
            read_on_advice(&tree, "i500", false, &fits_one_read),
            "read the rest of `l` with get_page_content, after \"i499\" and this base"
        );
        assert_eq!(
            read_on_advice(&tree, "p500", true, &fits_one_read),
            "read the rest of `l` as HTML with get_page_content, after \"i499\" and this base"
        );
        assert_eq!(read_on_advice(&tree, "i0", false, &fits_one_read), "read `i0` with get_page_content, ids [\"i0\"] and this base");
        assert_eq!(read_on_advice(&tree, "t0", false, &fits_one_read), "read `s` with get_page_content, ids [\"s\"] and this base");
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
        let read_so_far = known_children(&kept_base(&pool, &page_id, &base).await.unwrap(), &list);
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
        assert_eq!(known_children(&kept_base(&pool, &page_id, &kept).await.unwrap(), &list), read_so_far);

        // Reading the list by id again keeps what the base had read of it.
        let again = read(&tool, json!({ "page_id": page_id, "ids": [list], "base": kept })).await;
        let held = known_children(&kept_base(&pool, &page_id, again["base"].as_str().unwrap()).await.unwrap(), &list);
        assert_eq!(held, read_so_far);
    }

    /// A list a read showed in part, deleted while nothing in it had
    /// changed, was let through: the read "had the chance" to show it all.
    /// The rewrite then came in the next edit, from the items shown, and the
    /// rest were gone with no word to anyone. Deleting it waits until reads
    /// have shown all of it, and following the refusal gets there.
    #[sqlx::test]
    async fn a_list_read_in_part_is_not_deleted_then_written_again_in_the_next_edit(pool: PgPool) {
        let page_id = tree_page(&pool, &format!("Packing list:\n\n{}\nDone.\n", long_list(1_000))).await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let list = tree[1].id().unwrap().to_string();
        let first = read(&tool, json!({ "page_id": page_id })).await;
        let opened = read(&tool, json!({ "page_id": page_id, "ids": [list], "base": first["base"] })).await;
        let delete = |base: &Value| json!({ "page_id": page_id, "base": base, "ops": [{ "op": "delete", "id": list }] });
        let refused = refused_whole(&tool, &yjs, &page_id, delete(&opened["base"])).await;
        let message = refused["refused"][0]["message"].as_str().unwrap();
        let next = opened["more_after"].as_str().unwrap();
        assert!(message.contains("that no read has shown you"), "{message}");
        assert!(message.contains(&format!("after \"{next}\" and this base")), "{message}");

        let base = follow(&tool, &page_id, &refused).await;
        let deleted = edit(&tool, delete(&base)).await;
        assert_eq!(deleted["applied"], true, "{deleted}");
        assert_eq!(texts(&yjs.read_tree(&page_id).await.unwrap()), ["Packing list:", "Done."]);
    }

    /// Does what a refusal's problems say to read, reading on to the end of
    /// each read on, and returns the last base: the base the retry names.
    async fn follow(tool: &PageEditorTool, page_id: &str, refused: &Value) -> Value {
        let mut base = refused["base"].clone();
        let mut reads: Vec<Value> = vec![];
        for p in refused["refused"].as_array().unwrap() {
            let message = p["message"].as_str().unwrap();
            if let Some(rest) = message.split("after \"").nth(1) {
                let after = rest.split('"').next().unwrap();
                reads.push(json!({ "after": after }));
            } else if let Some(rest) = message.split("ids [").nth(1) {
                let listed = rest.split(']').next().unwrap();
                let ids: Value = serde_json::from_str(&format!("[{listed}]")).unwrap();
                reads.push(json!({ "ids": ids }));
            }
        }
        if let Some(unread) = refused.get("unread") {
            reads.push(json!({ "ids": unread }));
        }
        for mut args in reads {
            args["page_id"] = json!(page_id);
            args["base"] = base;
            let mut on = read(tool, args).await;
            while let Some(after) = on["more_after"].as_str().map(str::to_string) {
                on = read(tool, json!({ "page_id": page_id, "after": after, "base": on["base"] })).await;
            }
            base = on["base"].clone();
        }
        base
    }

    /// A list read whole across reads, then changed by a person further in
    /// than an ids read of it shows. An ids read of it opens it from its
    /// start and keeps the items past its first window as the base had
    /// them, so following a refusal that named the list was refused again
    /// and again. The refusal shows the item that changed, and its base has
    /// seen it: the retry lands with no read.
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
        assert!(message.contains("changed since you read it"), "{message}");
        assert!(refused["blocks"].as_str().unwrap().contains("Item 900, the charger"), "{refused}");
        assert!(refused.get("unread").is_none(), "{refused}");

        let wrote = edit(&tool, json!({ "page_id": page_id, "base": refused["base"], "ops": [replace] })).await;
        assert_eq!(wrote["applied"], true, "{wrote}");
        assert_eq!(texts(&yjs.read_tree(&page_id).await.unwrap()), ["Packing list:", "Passport", "Done."]);
    }

    /// A list shown in part, deleted and written again in one edit from what
    /// was shown: the rewrite puts back only the items a read showed, and
    /// the rest went with no word to anyone. The delete is refused, judged by
    /// what it takes whatever else the batch writes, until the rest has been
    /// read; and the opened read never offers the delete as the way past a
    /// refused replace.
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
        assert!(note.contains(&format!("Replacing or deleting `{list}` waits")), "{opened}");
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
            assert_eq!(refused["refused"].as_array().unwrap().len(), 1, "{refused}");
            assert_eq!(refused["refused"][0]["at"], "op 1");
            let message = refused["refused"][0]["message"].as_str().unwrap();
            assert!(message.contains("that no read has shown you"), "{message}");
            assert!(message.contains(&format!("inside `{list}`")), "{message}");
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

    /// A refused edit of an item in a long list the base lacks shows the
    /// item, not the list: an ids read of the list opens it from its start,
    /// short of the item, and the edit was refused a second time. The
    /// refusal's base has seen the item, and the retry lands.
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
        assert!(refused.get("unread").is_none(), "{refused}");
        let blocks = refused["blocks"].as_str().unwrap();
        assert!(blocks.starts_with(&format!("<li data-id=\"{item}\">")), "{blocks}");
        assert!(!refused["message"].as_str().unwrap().contains(&format!("`{list}`")), "{refused}");
        let wrote = edit(&tool, json!({ "page_id": page_id, "base": refused["base"], "ops": [replace] })).await;
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
            "<!-- {code} -->\n<!-- too long to show in any read; delete it or replace it whole, by its id -->"
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
        // An item goes beside another item: appended, it would land where
        // no list is.
        let item = refused_whole(
            &tool,
            &yjs,
            &page_id,
            json!({ "page_id": page_id, "base": swapped["base"], "ops": [
                { "op": "insert_after", "id": ids[2], "html": "<li><p>Dinner.</p></li>" },
            ] }),
        )
        .await;
        assert_eq!(
            item["refused"][0]["message"],
            format!(
                "the block `{}` was deleted after you read the page; anchor this op on another \
                 item beside where it was, or leave it out",
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
        assert!(seen.knows(&ids[0]) && !seen.knows(&ids[1]));
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
        assert_eq!(known_text(&seen, &ids[0]).as_deref(), Some("The quick brown fox."));
        assert_eq!(known_text(&seen, &ids[1]).as_deref(), Some("Lunch on Saturday."));

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
        assert_eq!(known_text(&seen, &para).as_deref(), Some("Pakc bags for Lisbon"), "the paragraph as read");

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

    /// An op refused over its own HTML keeps its block as read whatever the
    /// problem is charged to: a comment in it, HTML that holds no block.
    /// Learned as it stands, the block let the retry write over a person's
    /// change the first try would have merged with.
    #[sqlx::test]
    async fn an_op_refused_for_any_html_problem_keeps_its_block_as_read(pool: PgPool) {
        for (markdown, html) in [
            ("Lunch on Friday.\n", "<p>Dinner on Friday.</p><!-- x -->"),
            ("Lunch on Friday.\n", "<!-- x -->"),
            ("- Lunch on Friday.\n", "<p>Dinner on Friday.</p><!-- x -->"),
        ] {
            let page_id = tree_page(&pool, markdown).await;
            let yjs = YjsState::new(pool.clone());
            let tool = tool(&pool, &yjs);
            let base = read(&tool, json!({ "page_id": page_id })).await["base"].clone();
            let mut id = String::new();
            for n in yjs.read_tree(&page_id).await.unwrap() {
                n.walk(&mut |n| {
                    if id.is_empty() && n.kind == "paragraph" {
                        id = n.id().unwrap_or_default().to_string();
                    }
                });
            }
            person_writes(&yjs, &page_id, Op::Replace { id: id.clone(), html: "<p>Lunch on Saturday.</p>".into() }).await;

            let refused = refused_whole(
                &tool,
                &yjs,
                &page_id,
                json!({ "page_id": page_id, "base": base, "ops": [{ "op": "replace", "id": id, "html": html }] }),
            )
            .await;
            assert!(refused.get("blocks").is_none(), "{html}: {refused}");
            let retry = edit(
                &tool,
                json!({ "page_id": page_id, "base": refused["base"],
                        "ops": [{ "op": "replace", "id": id, "html": "<p>Dinner on Friday.</p>" }] }),
            )
            .await;
            assert_eq!(retry["applied"], true, "{html}: {retry}");
            assert_eq!(texts(&yjs.read_tree(&page_id).await.unwrap()), ["Dinner on Saturday."], "{html}");
        }
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
        let seen = known_children(
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
        assert!(message.contains(&format!("that no read has shown you, from `")), "{message}");
        assert!(message.contains(&format!("inside `{list}`")), "{message}");
        assert!(message.contains(&format!("after \"{last}\" and this base")), "{message}");

        // Reading the callout again keeps the list as it was read.
        let again = read(&tool, json!({ "page_id": page_id, "ids": [callout], "base": refused["base"] })).await;
        let held = known_children(&kept_base(&pool, &page_id, again["base"].as_str().unwrap()).await.unwrap(), &list);
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
            known_text(&kept, &p700).as_deref(),
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

    /// A list shown whole as markdown, then opened by id: a replace of it,
    /// written from the markdown, would turn each mention in the items the
    /// open did not show into a link. It waits for their HTML, and the
    /// refusal says to read on inside it from there, not to open it again.
    #[sqlx::test]
    async fn a_list_shown_as_markdown_then_opened_is_read_on_before_it_is_replaced(pool: PgPool) {
        let items: String = (0..1_000).map(|i| format!("- [ ] Pack {i:04} [@Nick](/person/person_1)\n")).collect();
        let page_id = tree_page(&pool, &format!("Checklist:\n\n{items}\nAfter.\n")).await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let list = tree[1].id().unwrap().to_string();
        let first = read(&tool, json!({ "page_id": page_id })).await;
        assert!(first["markdown"].as_str().unwrap().contains("Pack 0999"), "shown whole as markdown");
        let opened = read(&tool, json!({ "page_id": page_id, "ids": [list], "base": first["base"] })).await;
        let last = opened["more_after"].as_str().unwrap().to_string();
        let html = format!(
            "<ul data-type=\"taskList\">{}</ul>",
            (0..1_000)
                .rev()
                .map(|i| format!("<li data-type=\"taskItem\"><p>Pack {i:04} <a href=\"/person/person_1\">@Nick</a></p></li>"))
                .collect::<String>()
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

    /// A table row too long for any read holds blocks in its cells a read
    /// can show. Listed whole by its id, the row went with a replace that
    /// wrote two short cells, and the long list in one of them, which no
    /// read had shown, with it. The blocks in its cells go only once read:
    /// the refusal names what to read by id, and following it the replace
    /// lands.
    #[sqlx::test]
    async fn a_row_too_long_for_any_read_does_not_take_the_blocks_in_its_cells(pool: PgPool) {
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let page_id = tree_page(&pool, "Expenses:\n").await;
        let items: String = (0..1_200)
            .map(|i| format!("<li><p>Receipt {i:04} for the taxi to the coast, kept in the blue folder</p></li>"))
            .collect();
        let appended = edit(&tool, json!({ "page_id": page_id, "ops": [{ "op": "append", "html": format!(
            "<table><tr><td><p>Fri</p></td><td><ul>{items}</ul></td></tr><tr><td><p>Sat</p></td><td><p>Lunch</p></td></tr></table>"
        ) }] }))
        .await;
        assert_eq!(appended["applied"], true, "{appended}");
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let table = tree[1].id().unwrap().to_string();
        let row = tree[1].content[0].id().unwrap().to_string();
        let list = tree[1].content[0].content[1].content[0].id().unwrap().to_string();
        assert!(!fits_one_read(&tree[1].content[0]), "the row fits one read");

        let first = read(&tool, json!({ "page_id": page_id })).await;
        assert!(!first["markdown"].as_str().unwrap().contains("Receipt 0000"), "the table was shown");
        let opened = read(&tool, json!({ "page_id": page_id, "ids": [table], "base": first["base"] })).await;
        let html = opened["html"].as_str().unwrap();
        assert!(html.contains(&format!("<!-- {row}: too long to show in any read")), "{html}");
        let short = json!({ "op": "replace", "id": row, "html": "<tr><td><p>Fri</p></td><td><p>Taxis</p></td></tr>" });
        let mut refused =
            refused_whole(&tool, &yjs, &page_id, json!({ "page_id": page_id, "base": opened["base"], "ops": [short] })).await;
        let message = refused["refused"][0]["message"].as_str().unwrap();
        assert!(message.contains("no read has shown you"), "{message}");
        assert!(message.contains("with get_page_content, ids ["), "{message}");
        for _ in 0..8 {
            let base = follow(&tool, &page_id, &refused).await;
            let data = edit(&tool, json!({ "page_id": page_id, "base": base, "ops": [short] })).await;
            if data["applied"] == true {
                let now = yjs.read_tree(&page_id).await.unwrap();
                assert!(virtues_document::ops::find_node(&now, &list).is_none());
                assert_eq!(now[1].content[0].text_content(), "FriTaxis");
                return;
            }
            assert_eq!(yjs.read_tree(&page_id).await.unwrap()[1], tree[1], "a refused batch wrote");
            refused = data;
        }
        panic!("the replace never landed: {refused}");
    }

    /// A row too long for any read, its cells holding more blocks than one
    /// read shows. Each refusal named one of them, and every one took an
    /// edit refused in a row, so the turn's guard closed edit_page before
    /// the row could go. One read now names them all, and a refusal that
    /// waits only on reads does not count toward the close: deleting the
    /// row by following the refusals lands within the turn.
    #[sqlx::test]
    async fn a_row_with_more_in_its_cells_than_one_read_shows_is_deleted_within_a_turn(pool: PgPool) {
        use crate::agent::executor::ToolExecutionResult;
        use crate::agent::guard::{RepeatGuard, MAX_CONSECUTIVE};
        use crate::agent::stream::ToolCall;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let page_id = tree_page(&pool, "Expenses:\n").await;
        let words = "Receipt for the taxi to the coast, kept in the blue folder. ".repeat(320);
        let cells: String = (0..14).map(|i| format!("<td><p>{i} {words}</p></td>")).collect();
        let short: String = (0..14).map(|i| format!("<td><p>{i}</p></td>")).collect();
        let appended = edit(&tool, json!({ "page_id": page_id, "ops": [{ "op": "append", "html": format!(
            "<table><tr>{cells}</tr><tr>{short}</tr></table>"
        ) }] }))
        .await;
        assert_eq!(appended["applied"], true, "{appended}");
        let tree = yjs.read_tree(&page_id).await.unwrap();
        let table = tree[1].id().unwrap().to_string();
        let row = tree[1].content[0].id().unwrap().to_string();
        let inside: Vec<String> = tree[1].content[0].content.iter().map(|cell| cell.content[0].id().unwrap().to_string()).collect();
        assert!(!fits_one_read(&tree[1].content[0]));
        assert!(fits_one_read(&tree[1].content[0].content[0]));

        let first = read(&tool, json!({ "page_id": page_id })).await;
        let opened = read(&tool, json!({ "page_id": page_id, "ids": [table], "base": first["base"] })).await;
        let delete = |base: &Value| json!({ "page_id": page_id, "base": base, "ops": [{ "op": "delete", "id": row }] });
        let mut args = delete(&opened["base"]);
        let mut guard = RepeatGuard::new();
        for i in 0..40 {
            let call = ToolCall { id: format!("c{i}"), name: "edit_page".into(), arguments: args.clone() };
            let (run, closed) = guard.admit(std::slice::from_ref(&call));
            assert!(closed.is_empty(), "after {i} refusals: {}", closed[0].to_llm_content());
            let result = tool.edit_page(args.clone(), &ToolContext::default()).await.unwrap();
            let data = result.data.clone();
            guard.record(&run, &[ToolExecutionResult { tool_call_id: call.id, tool_name: call.name, result: Ok(result) }]);
            if data["applied"] == true {
                assert!(i > MAX_CONSECUTIVE as usize, "only {i} refusals: the guard was never in the way");
                let now = yjs.read_tree(&page_id).await.unwrap();
                assert!(virtues_document::ops::find_node(&now, &row).is_none());
                return;
            }
            assert_eq!(data["read_first"], true, "{data}");
            if i == 0 {
                // One read names every block in the row's cells.
                let listed = inside.iter().map(|id| format!("\"{id}\"")).collect::<Vec<_>>().join(", ");
                let message = data["refused"][0]["message"].as_str().unwrap();
                assert!(message.contains(&format!("ids [{listed}]")), "{message}");
            }
            args = delete(&follow(&tool, &page_id, &data).await);
        }
        panic!("the delete never landed");
    }

    /// A callout read by id and the long list inside it opened by id: the
    /// base took the callout from the page whole, so deleting it took the
    /// items no read had shown. The delete waits for the rest of the list,
    /// and following the refusal, it stands.
    #[sqlx::test]
    async fn deleting_a_block_around_a_list_read_in_part_waits_for_the_rest_of_it(pool: PgPool) {
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let (page_id, callout, list) = callout_around_a_long_list(&pool, &tool, &yjs).await;
        let first = read(&tool, json!({ "page_id": page_id })).await;
        let opened = read(&tool, json!({ "page_id": page_id, "ids": [callout], "base": first["base"] })).await;
        let opened_list = read(&tool, json!({ "page_id": page_id, "ids": [list], "base": opened["base"] })).await;
        let delete = |base: &Value| json!({ "page_id": page_id, "base": base, "ops": [{ "op": "delete", "id": callout }] });
        let refused = refused_whole(&tool, &yjs, &page_id, delete(&opened_list["base"])).await;
        let message = refused["refused"][0]["message"].as_str().unwrap();
        assert!(message.contains(&format!("inside `{list}`")), "{message}");
        let base = follow(&tool, &page_id, &refused).await;
        let deleted = edit(&tool, delete(&base)).await;
        assert_eq!(deleted["applied"], true, "{deleted}");
        assert!(virtues_document::ops::find_node(&yjs.read_tree(&page_id).await.unwrap(), &list).is_none());
    }

    /// The model knows what it wrote: a block an edit wrote that was too
    /// long to show in the result is edited again with the result's base and
    /// no read.
    #[sqlx::test]
    async fn blocks_an_edit_wrote_but_could_not_show_are_edited_again_with_no_read(pool: PgPool) {
        let page_id = tree_page(&pool, "Lunch on Friday.\n").await;
        let yjs = YjsState::new(pool.clone());
        let tool = tool(&pool, &yjs);
        let base = read(&tool, json!({ "page_id": page_id })).await["base"].clone();
        let id = top_ids(&yjs.read_tree(&page_id).await.unwrap())[0].clone();
        let rest = "and the coast road after it. ".repeat(300);
        let replace = |base: &Value, html: String| {
            json!({ "page_id": page_id, "base": base, "ops": [{ "op": "replace", "id": id, "html": html }] })
        };
        let wrote = edit(&tool, replace(&base, format!("<p>Lunch on Friday, {rest}</p>"))).await;
        assert_eq!(wrote["html"], "", "{wrote}");
        let again = edit(&tool, replace(&wrote["base"], format!("<p>Lunch on Saturday, {rest}</p>"))).await;
        assert_eq!(again["applied"], true, "{again}");
        assert!(texts(&yjs.read_tree(&page_id).await.unwrap())[0].starts_with("Lunch on Saturday,"));
    }
}

#[cfg(test)]
#[path = "page_editor_unseen_tests.rs"]
mod unseen_tests;

/// The model's edits at a box's own limits: a read holds 64 KiB, a refusal
/// shows 8,000 characters. Each case here got past the base, or looped, only
/// where a refusal shows less than a read holds, which the random test at
/// its small limits once never reached.
#[cfg(test)]
mod production_limits {
    use super::*;
    use virtues_document::ops::find_node;

    fn parsed(html: &str) -> Vec<Node> {
        let o = virtues_document::parse_html(html, "doc");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        o.nodes
    }

    fn delete(id: &str) -> Op {
        Op::Delete { id: id.into() }
    }

    /// The refusal `edit_page` gives, or `None` when the batch lands.
    fn refusal(seen: &Seen, tree: &[Node], ops: &[Op]) -> Option<Refusal> {
        let checked = virtues_document::check_ops(tree, seen, ops);
        (!checked.problems.is_empty())
            .then(|| {
                refusal_of(
                    ops,
                    seen,
                    checked.problems,
                    checked.unseen,
                    &checked.conflicts,
                    tree,
                    WRITTEN_HTML_CHARS,
                    &fits_one_read,
                )
            })
    }

    fn said(r: &Refusal) -> String {
        r.problems.iter().map(|p| p.message.as_str()).collect::<Vec<_>>().join(" | ")
    }

    /// An `ids` read of `id`, and on inside it to its end, as
    /// `get_page_content` reads.
    fn read_through(seen: &mut Seen, tree: &[Node], id: &str) {
        let read = read_by_id(seen, tree, &[id.to_string()], READ_MARKDOWN_BYTES, json_bytes);
        let mut next = read.opened.as_ref().and_then(|o| o.next.clone());
        seen.learn(&Shown::html(read.parts), tree, &HashSet::new());
        while let Some(after) = next {
            let (parent, siblings, at) = siblings_of(tree, &after).unwrap();
            let window = children_window(siblings, at + 1, READ_MARKDOWN_BYTES, READ_MARKDOWN_BYTES, json_bytes);
            seen.learn(&Shown::html(window.parts).inside(parent), tree, &HashSet::new());
            next = window.next;
        }
    }

    fn quote_lines(n: usize) -> String {
        (0..n)
            .map(|i| format!("<p data-id=\"q{i}\">Line {i} of the harbour notes, with the ferry times kept</p>"))
            .collect()
    }

    fn list(n: usize, extra: &[(usize, &str)]) -> String {
        let mut items = String::new();
        for i in 0..n {
            items.push_str(&format!("<li data-id=\"i{i}\"><p data-id=\"p{i}\">Item {i} on the list for the coast</p></li>"));
            for (at, id) in extra {
                if *at == i {
                    items.push_str(&format!("<li data-id=\"{id}\"><p data-id=\"{id}p\">Added by hand</p></li>"));
                }
            }
        }
        format!("<ul data-id=\"l\">{items}</ul><p data-id=\"z\">After</p>")
    }

    /// A paragraph a person joined to the end of a quote too long for one
    /// read: the read the refusal names opens the quote without showing
    /// it, and the copy the model had is still refused, the paragraph kept.
    #[test]
    fn a_block_joined_into_a_long_quote_is_not_taken_on_a_read_that_did_not_show_it_there() {
        let lines = quote_lines(1200);
        let before = parsed(&format!("<blockquote data-id=\"q\">{lines}</blockquote><p data-id=\"x\">Mine</p><p data-id=\"z\">After</p>"));
        let mut seen = Seen::of_page(&before);
        let copy = Op::Replace {
            id: "q".into(),
            html: virtues_document::to_html(&[seen.subtree("q").unwrap()], true),
        };
        let now = parsed(&format!("<blockquote data-id=\"q\">{lines}<p data-id=\"x\">Mine</p></blockquote><p data-id=\"z\">After</p>"));
        assert!(!fits_one_read(find_node(&now, "q").unwrap()));
        let r = refusal(&seen, &now, std::slice::from_ref(&copy)).expect("refused");
        assert!(said(&r).contains("`x`"), "{}", said(&r));

        let read = read_by_id(&seen, &now, &["q".to_string()], READ_MARKDOWN_BYTES, json_bytes);
        let shown = Shown::html(read.parts);
        assert!(read.opened.is_some());
        assert!(!shown.text().contains("data-id=\"x\""), "the read does not show it");
        seen.learn(&shown, &now, &HashSet::new());
        for op in [copy, delete("q")] {
            let r = refusal(&seen, &now, std::slice::from_ref(&op)).expect("refused");
            assert!(said(&r).contains("`x`"), "{op:?}: {}", said(&r));
        }

        // Read on inside the quote, it is shown there, and goes with it.
        read_through(&mut seen, &now, "q");
        assert!(refusal(&seen, &now, &[delete("q")]).is_none());
    }

    /// The last paragraph of a quote too long for one read, counted by the
    /// read that opened the quote, moved out to the page's top level by a
    /// person, read there (a read on after the quote, or an `ids` read),
    /// and moved back: no read showed it inside the quote, and a delete of
    /// the quote, or a replace from the opened read, is refused over it.
    /// The top-level read placed it nowhere, so the quote held it as shown.
    #[test]
    fn a_block_read_at_the_top_level_is_not_taken_with_the_quote_it_was_counted_in() {
        let lines = quote_lines(1200);
        let inside = parsed(&format!(
            "<blockquote data-id=\"q\">{lines}<p data-id=\"k\">Kept</p></blockquote><p data-id=\"z\">After</p>"
        ));
        let out = parsed(&format!(
            "<blockquote data-id=\"q\">{lines}</blockquote><p data-id=\"k\">Kept</p><p data-id=\"z\">After</p>"
        ));
        let comments = Regex::new(r"<!--.*?-->").unwrap();
        for how in ["a read on after the quote", "an ids read"] {
            let mut seen = Seen::default();
            let read = read_by_id(&seen, &inside, &["q".to_string()], READ_MARKDOWN_BYTES, json_bytes);
            assert!(read.opened.is_some(), "the quote opens");
            let opened = Shown::html(read.parts);
            assert!(!opened.text().contains("data-id=\"k\""), "the opened read counts `k`");
            seen.learn(&opened, &inside, &HashSet::new());
            // The quote as the model would write it back from that read.
            let copy = Op::Replace {
                id: "q".into(),
                html: comments.replace_all(&opened.text(), "").to_string(),
            };

            let at_top = if how == "an ids read" {
                Shown::html(read_by_id(&seen, &out, &["k".to_string()], READ_MARKDOWN_BYTES, json_bytes).parts)
            } else {
                page_reads::markdown_window(&out, 1, READ_MARKDOWN_BYTES, json_bytes, fits_one_read).shown
            };
            assert!(at_top.text().contains("Kept"), "{how} shows `k`");
            seen.learn(&at_top, &out, &HashSet::new());
            assert!(seen.knows("k"), "{how}");
            assert!(!seen.known("q").unwrap().shows("k"), "{how}: no read showed `k` inside `q`");

            for op in [delete("q"), copy] {
                let r = refusal(&seen, &inside, std::slice::from_ref(&op)).expect("refused");
                assert!(said(&r).contains("`k`"), "{how}, {op:?}: {}", said(&r));
            }
        }
    }

    /// Items a person added to a list read through, and one the model
    /// added after a person's edit: the refusal over an added item shows
    /// it inside the list, so the next try lands, where it showed the item
    /// alone and was given again word for word; and the model's own item is
    /// no item added since its read.
    #[test]
    fn items_added_to_a_list_read_through_are_shown_inside_it_and_the_next_try_lands() {
        let read = parsed(&list(1500, &[]));
        assert!(!fits_one_read(find_node(&read, "l").unwrap()));
        let mut seen = Seen::default();
        read_through(&mut seen, &read, "l");
        assert!(refusal(&seen, &read, &[delete("l")]).is_none());

        for added in [vec![(700, "x")], vec![(300, "x"), (1200, "y")]] {
            let now = parsed(&list(1500, &added));
            let mut base = seen.clone();
            let r = refusal(&base, &now, &[delete("l")]).expect("refused");
            assert!(said(&r).contains("`l` is above as it is now"), "{}", said(&r));
            base.learn(&r.shown, &now, &r.keep);
            assert!(refusal(&base, &now, &[delete("l")]).is_none(), "{added:?}");
        }

        // A person deletes an item; the model adds one with its base.
        let doc = virtues_document::doc_from_nodes(read.clone());
        virtues_document::apply_ops(&doc, None, &[delete("i5")]).unwrap();
        let before = { use yrs::Transact; virtues_document::read_doc(&doc.transact()) };
        let mut base = seen.clone();
        let applied = virtues_document::apply_ops(
            &doc,
            Some(&base),
            &[Op::InsertAfter { id: "i10".into(), html: "<li><p>Mine</p></li>".into() }],
        )
        .unwrap();
        let after = { use yrs::Transact; virtues_document::read_doc(&doc.transact()) };
        base.advance(&before, &after, &applied.merged.iter().cloned().collect());
        assert!(refusal(&base, &after, &[delete("l")]).is_none());
    }

    /// Code a markdown read showed whole, which an `ids` read lists as too
    /// long for any read: a delete of the item around it is refused with
    /// the op to add, never a read as HTML no read can give.
    #[test]
    fn a_listed_block_a_markdown_read_showed_is_named_not_read_again() {
        // Its markdown fits a read; its HTML, every `<` an `&lt;`, does not.
        let code = "x &lt; y &lt; z &lt; w &amp;&amp; v &gt; u\n".repeat(2000);
        let tree = parsed(&format!(
            "<ul data-id=\"l\"><li data-id=\"i1\"><p data-id=\"p1\">One</p><pre data-id=\"code\">{code}</pre></li>\
             <li data-id=\"i2\"><p data-id=\"p2\">Two</p></li></ul><p data-id=\"z\">After</p>"
        ));
        let code_node = find_node(&tree, "code").unwrap();
        assert!(!fits_one_read(code_node));
        let mut seen = Seen::default();
        let window = page_reads::markdown_window(&tree, 0, READ_MARKDOWN_BYTES, json_bytes, fits_one_read);
        seen.learn(&window.shown, &tree, &HashSet::new());
        assert!(seen.knows("code"), "the markdown read shows it whole");
        // The list and its item read as HTML by id, as refusals say: each
        // opens, and the code is listed in the item.
        for id in ["l", "i1"] {
            let read = read_by_id(&seen, &tree, &[id.to_string()], READ_MARKDOWN_BYTES, json_bytes);
            assert!(read.opened.is_some(), "{id}");
            seen.learn(&Shown::html(read.parts), &tree, &HashSet::new());
        }
        assert_eq!(seen.known("i1").map(|k| k.form), Some(Form::Html));
        assert!(seen.opaque_hash("code").is_some());
        let link = Op::Append { html: "<p><a href=\"/person/person_1\">@Nick</a></p>".into() };
        for ops in [vec![delete("i1")], vec![delete("l"), link.clone()]] {
            let r = refusal(&seen, &tree, &ops).expect("refused");
            assert!(said(&r).contains("delete `code` by its id"), "{ops:?}: {}", said(&r));
            assert!(!said(&r).contains("as HTML"), "{}", said(&r));
        }
        assert!(refusal(&seen, &tree, &[delete("code"), delete("l"), link]).is_none());
    }

    /// A paragraph listed as too long for any read, which a person then
    /// rewrote short enough for one: the refusal asks for a read of it, and
    /// an op by its name does not take the new text unread.
    #[test]
    fn a_listed_block_rewritten_to_fit_a_read_is_read_before_it_goes() {
        let big = format!("<p data-id=\"big\">{}</p><p data-id=\"z\">After</p>", "word ".repeat(16_000));
        let tree = parsed(&big);
        let mut seen = Seen::default();
        let read = read_by_id(&seen, &tree, &["big".to_string()], READ_MARKDOWN_BYTES, json_bytes);
        assert_eq!(read.whole_only, ["big"]);
        seen.learn(&Shown::html(read.parts), &tree, &HashSet::new());
        let now = parsed(&format!("<p data-id=\"big\">{}</p><p data-id=\"z\">After</p>", "fresh ".repeat(3_000)));
        assert!(fits_one_read(find_node(&now, "big").unwrap()));
        for op in [delete("big"), Op::Replace { id: "big".into(), html: "<p>cut</p>".into() }] {
            let r = refusal(&seen, &now, std::slice::from_ref(&op)).expect("refused");
            assert!(said(&r).contains("read it as it is now"), "{}", said(&r));
            assert_eq!(r.unread.long, ["big"]);
            let mut base = seen.clone();
            base.learn(&r.shown, &now, &r.keep);
            assert!(refusal(&base, &now, std::slice::from_ref(&op)).is_some(), "no read showed the new text");
            let read = read_by_id(&base, &now, &["big".to_string()], READ_MARKDOWN_BYTES, json_bytes);
            base.learn(&Shown::html(read.parts), &now, &HashSet::new());
            assert!(refusal(&base, &now, &[op]).is_none());
        }
    }

    /// A paragraph a person joined into a quote, replaced by name in the
    /// batch that deletes the quote: a replace means its block to stay, so
    /// the delete is refused over it and the quote is shown with it inside.
    /// Taken as named, the person's paragraph and the model's update to it
    /// both went with the quote.
    #[test]
    fn a_replace_by_name_does_not_take_a_block_moved_in_with_the_block_around_it() {
        let read = parsed(
            "<blockquote data-id=\"q\"><p data-id=\"x\">Quoted line</p></blockquote>\
             <p data-id=\"c\">The person's paragraph</p><p data-id=\"z\">End</p>",
        );
        let seen = Seen::of_page(&read);
        let now = parsed(
            "<blockquote data-id=\"q\"><p data-id=\"x\">Quoted line</p><p data-id=\"c\">The person's paragraph</p></blockquote>\
             <p data-id=\"z\">End</p>",
        );
        let ops = [
            Op::Replace { id: "c".into(), html: "<p>The person's paragraph, updated</p>".into() },
            delete("q"),
        ];
        let r = refusal(&seen, &now, &ops).expect("refused");
        assert!(said(&r).contains("`c`, added to `q` after you read it"), "{}", said(&r));
        assert!(said(&r).contains("`q` is above as it is now"), "{}", said(&r));
        // Shown there, the quote goes with it: the writer has seen it holds it.
        let mut base = seen.clone();
        base.learn(&r.shown, &now, &r.keep);
        assert!(refusal(&base, &now, &ops).is_none());
    }

    /// A refused op on a to-do and a kept op on the paragraph inside it,
    /// which a person changed since the read. The refusal shows the to-do
    /// as it is now, so its base learns the paragraph too: the kept op, sent
    /// again as written as the reply said, landed over the person's words
    /// its first try had merged with. It is refused with the other, to be
    /// written from what is shown.
    #[test]
    fn a_kept_op_on_a_block_a_refusal_shows_changed_is_written_again_from_it() {
        use yrs::Transact;
        let read = parsed(
            "<ul data-type=\"taskList\" data-id=\"l\"><li data-type=\"taskItem\" data-checked=\"false\" data-id=\"i\">\
             <p data-id=\"x\">Book the ferry seats</p></li></ul><p data-id=\"z\">After</p>",
        );
        let seen = Seen::of_page(&read);
        let doc = virtues_document::doc_from_nodes(read);
        let person = Op::Replace { id: "x".into(), html: "<p>Reserve the ferry seats</p>".into() };
        virtues_document::apply_ops(&doc, None, &[person]).unwrap();
        let now = virtues_document::read_doc(&doc.transact());
        let item = |text: &str| Op::Replace {
            id: "i".into(),
            html: format!("<li data-type=\"taskItem\" data-checked=\"true\"><p data-id=\"x\">{text}</p></li>"),
        };
        let fix = |text: &str| Op::Replace { id: "x".into(), html: format!("<p>{text}</p>") };
        let ops = [item("Book the ferry seats"), fix("Book the ferry tickets")];
        // Alone, the paragraph's op merges with the person's word.
        assert!(refusal(&seen, &now, &ops[1..]).is_none());

        let r = refusal(&seen, &now, &ops).expect("refused");
        let at: Vec<&str> = r.problems.iter().map(|p| p.at.as_str()).collect();
        assert_eq!(at, ["op 1", "op 2"], "{}", said(&r));
        assert!(r.problems[1].message.contains("`x` changed since you read it"), "{}", said(&r));
        assert!(r.problems[1].message.contains("is above as it is now"), "{}", said(&r));
        assert!(r.blocks.contains("Reserve the ferry seats"), "{}", r.blocks);
        assert!(!r.keep.contains("x"));

        // Both written again from what was shown: the person's word stays.
        let mut base = seen.clone();
        base.learn(&r.shown, &now, &r.keep);
        let retry = [item("Reserve the ferry seats"), fix("Reserve the ferry tickets")];
        virtues_document::apply_ops(&doc, Some(&base), &retry).unwrap();
        let after = virtues_document::read_doc(&doc.transact());
        assert_eq!(find_node(&after, "x").unwrap().text_content(), "Reserve the ferry tickets");
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

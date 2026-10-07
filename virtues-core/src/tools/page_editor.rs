//! Page editor tool implementation
//!
//! Provides three separate tools for page operations:
//! - create_page: Create a new page with content
//! - get_page_content: Read current page content before editing
//! - edit_page: Apply edits using simple find/replace
//!
//! When YjsState is available, edits go through the Yjs layer for real-time sync.
//!
//! Edits are applied as clean text. Each one keeps a restore point of the page
//! before it (when no version already holds that text) and cuts a version
//! credited to 'ai' after it, so the user can undo it from version history.
//! Typing that lands between the restore point and the edit is versioned as
//! the owner's first, so History credits the chat with its own change alone.

use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::{Arc, OnceLock};

// Static regexes for stripping CriticMarkup (legacy safety fallback — strips markers if LLM still outputs them)
static ADDITION_RE: OnceLock<Regex> = OnceLock::new();
static DELETION_RE: OnceLock<Regex> = OnceLock::new();

use super::executor::{ToolContext, ToolError, ToolResult};
use crate::api::pages;
use crate::ids;
use crate::server::yjs::{TextWriteError, YjsState};

/// The description on the owner's typing when it is versioned just before a
/// chat edit lands on it: an ordinary autosave (`'auto'`), so History reads
/// it as theirs. Not a restore point (`pages::RESTORE_POINT`), and not the
/// label chat edits once put on the page before them (`wiki_articles`'
/// `BEFORE_CHAT_EDIT`), which History reads as the chat's.
const TYPED_BEFORE_CHAT_EDIT: &str = "Auto-saved (before a chat edit)";

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
    /// Text to find in the document (empty string for full replacement)
    pub find: String,
    /// Replacement text (supports markdown; CriticMarkup markers stripped as safety fallback)
    pub replace: String,
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
    /// Original text that was replaced (for diff display and reject/undo)
    pub find: String,
    /// New text that replaced it (clean, no markup)
    pub replace: String,
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
        let addition_re = ADDITION_RE.get_or_init(|| {
            Regex::new(r"\{\+\+([\s\S]*?)\+\+\}").expect("valid addition regex")
        });
        let deletion_re = DELETION_RE.get_or_init(|| {
            Regex::new(r"\{--([\s\S]*?)--\}").expect("valid deletion regex")
        });

        // Remove addition markers but keep content: {++text++} -> text
        let result = addition_re.replace_all(content, "$1");

        // Remove deletion markers and content: {--text--} -> ""
        let result = deletion_re.replace_all(&result, "");

        result.to_string()
    }

    /// Create a new page with optional initial content.
    /// Content supports markdown and is applied directly.
    /// CriticMarkup markers are stripped as a safety fallback.
    pub async fn create_page(
        &self,
        arguments: serde_json::Value,
    ) -> Result<ToolResult, ToolError> {
        let args: CreatePageArgs = serde_json::from_value(arguments)
            .map_err(|e| ToolError::InvalidParameters(format!("Invalid arguments: {}", e)))?;

        // Strip any CriticMarkup markers the AI might have included
        let clean_content = args.content
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
        };

        let page = pages::create_page(self.pool.as_ref(), req)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to create page: {}", e)))?;

        Ok(ToolResult::success(serde_json::json!({
            "page_id": page.id,
            "title": page.title,
            "message": "Page created successfully.",
        })))
    }

    /// Get current content of a page
    ///
    /// Should be called before edit_page to see what text to find.
    /// When YjsState is available, reads from Yjs (live state) instead of database.
    pub async fn get_page_content(
        &self,
        arguments: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let args: GetPageContentArgs = serde_json::from_value(arguments)
            .map_err(|e| ToolError::InvalidParameters(format!("Invalid arguments: {}", e)))?;

        // Get page_id from args or context
        let page_id = args.page_id.or_else(|| context.page_id.clone());

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

        // Get page metadata from database (title, etc.)
        let page = pages::get_page(self.pool.as_ref(), &page_id)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to get page: {}", e)))?;

        // Get content from Yjs if available (live state), otherwise use database content
        let content = if let Some(ref yjs_state) = self.yjs_state {
            match yjs_state.get_page_content(&page_id).await {
                Ok(yjs_content) => yjs_content,
                Err(_) => page.content.clone(), // Fallback to database on error
            }
        } else {
            page.content.clone()
        };

        Ok(ToolResult::success(serde_json::json!({
            "page_id": page.id,
            "title": page.title,
            "content": content,
            "content_length": content.len(),
        })))
    }

    /// Edit a page using find/replace.
    ///
    /// The 'find' text matches against plain text content (formatting stripped).
    /// The 'replace' text supports markdown, which is preserved through the Yjs roundtrip.
    /// CriticMarkup markers in replace are stripped as a safety fallback.
    /// Empty 'find' string means replace entire document.
    ///
    /// Edits are applied immediately via Yjs for real-time sync to connected clients.
    /// A restore point is kept before each edit and an 'ai' version cut after it,
    /// for undo via version history.
    ///
    /// Permission checking: If chat_id is provided in context, checks that the page has
    /// been granted edit permission for this chat. If not, returns permission_needed: true.
    pub async fn edit_page(
        &self,
        arguments: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let args: EditPageArgs = serde_json::from_value(arguments)
            .map_err(|e| ToolError::InvalidParameters(format!("Invalid arguments: {}", e)))?;

        // Get page_id from args or context
        let page_id = args.page_id.or_else(|| context.page_id.clone());

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

        // Editing a page is reversible (Yjs keeps history) and local, so it runs freely — no
        // permission prompt. Write-permission gating lives in the executor and applies only to
        // destructive/outbound tools (run_applet, delete_applet).

        // Strip CriticMarkup markers from replace text
        let replace_content = Self::strip_critic_markup(&args.replace);

        // Skip content edit when both find and replace are empty (title-only change)
        let has_content_edit = !args.find.is_empty() || !replace_content.is_empty();

        // False when the edit is on the page but its save is still pending.
        let mut saved = true;

        // Apply the edit through Yjs if available, otherwise fall back to database
        // With Y.Text, the document IS markdown — no plain text conversion needed
        if has_content_edit && self.yjs_state.is_some() {
            let yjs_state = self.yjs_state.as_ref().unwrap();
            let pool = self.pool.as_ref();
            // Another machine write to this page waits until this one's
            // versions are cut, so neither reads the other as the owner's typing.
            let _turn = yjs_state.write_turn(&page_id).await;

            // The page as it stands, kept first: a version is the state AFTER
            // the edit it records, so without this the text before a chat edit
            // (a whole-page replacement included) would be in no version, and
            // the edit could not be undone. No copy, no edit.
            let kept = pages::cut_restore_point(pool, yjs_state, &page_id)
                .await
                .map_err(|e| {
                    ToolError::ExecutionFailed(format!(
                        "could not keep a copy of the page before editing it, so the edit was not made: {e}"
                    ))
                })?;

            // Apply through Yjs for real-time sync (direct markdown find/replace).
            // An edit whose save failed is applied all the same: it is versioned
            // and reported as made, so nobody makes it a second time.
            let edit = yjs_state
                .apply_text_edit(&page_id, &args.find, &replace_content)
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
            if edit.before.text != kept.text {
                pages::cut_version(pool, &page_id, &edit.before, "auto", Some(TYPED_BEFORE_CHAT_EDIT))
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

            if !args.find.is_empty() && !page.content.contains(&args.find) {
                return Err(ToolError::ExecutionFailed(format!(
                    "Text not found in page: '{}'",
                    if args.find.chars().count() > 50 {
                        format!("{}...", args.find.chars().take(50).collect::<String>())
                    } else {
                        args.find.clone()
                    }
                )));
            }

            let new_content = if args.find.is_empty() {
                replace_content.clone()
            } else {
                page.content.replacen(&args.find, &replace_content, 1)
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

        // Update title if provided. After a content edit, a title that could
        // not be changed is reported beside the edit, not as the call's
        // failure: the edit is on the page, and a failure would invite it a
        // second time.
        let mut title_error = None;
        if let Some(ref new_title) = args.title {
            let update_req = pages::UpdatePageRequest {
                title: Some(new_title.clone()),
                content: None,
                icon: None,
                icon_color: None,
                cover_url: None,
                tags: None,
            };
            match pages::update_page(self.pool.as_ref(), &page_id, update_req).await {
                Ok(_) => {}
                Err(e) if has_content_edit => {
                    tracing::warn!(page = %page_id, error = %e, "chat edit applied but the title did not change");
                    title_error = Some(e.to_string());
                }
                Err(e) => {
                    return Err(ToolError::ExecutionFailed(format!("Failed to update title: {}", e)))
                }
            }
        }
        let title_changed = args.title.is_some() && title_error.is_none();

        let edit_id = ids::generate_id("edit", &[&page_id, &chrono::Utc::now().to_rfc3339()]);

        let result = EditResult {
            edit_id,
            page_id: page_id.clone(),
            find: args.find.clone(),
            replace: replace_content.clone(),
        };

        let mut message = match (&args.title, has_content_edit) {
            (Some(t), true) if title_changed => {
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
        if let Some(e) = &title_error {
            message.push_str(&format!(" The title did not change: {e}."));
        }
        if !saved || title_error.is_some() {
            message.push_str(" Do not make this edit again.");
        }

        Ok(ToolResult::success(serde_json::json!({
            "edit": result,
            "applied": true,
            "saved": saved,
            "title_changed": title_changed,
            "message": message,
        })))
    }
}

impl std::fmt::Debug for PageEditorTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PageEditorTool").finish()
    }
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
        assert_eq!(yjs.read_text(&page.id).await.unwrap(), "Black coffee.\nGreen tea.\n");
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
        assert!(result.data["message"].as_str().unwrap().contains("Do not make this edit again"));

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
                ("auto".to_string(), Some(pages::RESTORE_POINT.to_string()), "Your own line.\n".to_string()),
                (
                    "auto".to_string(),
                    Some(TYPED_BEFORE_CHAT_EDIT.to_string()),
                    "Your own line.\nTyped.\n".to_string()
                ),
                ("ai".to_string(), Some("Edited from a chat".to_string()), "Your line, edited.\nTyped.\n".to_string()),
            ]
        );
    }
}

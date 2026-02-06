//! Page editor tool implementation
//!
//! Provides three separate tools for page operations:
//! - create_page: Create a new page with content
//! - get_page_content: Read current page content before editing
//! - edit_page: Apply edits using simple find/replace
//!
//! When YjsState is available, edits go through the Yjs layer for real-time sync.
//!
//! Edits are applied as clean text. The frontend handles highlighting and
//! accept/reject UI via decorations (no CriticMarkup in the document).

use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::{Arc, OnceLock};

// Static regexes for stripping CriticMarkup (safety net if AI includes markers)
static ADDITION_RE: OnceLock<Regex> = OnceLock::new();
static DELETION_RE: OnceLock<Regex> = OnceLock::new();

use super::executor::{ToolContext, ToolError, ToolResult};
use crate::api::{chat_permissions, pages};
use crate::ids;
use crate::server::yjs::YjsState;

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
    /// Text to find in the document (empty string for full replacement)
    pub find: String,
    /// Replacement text (CriticMarkup markers are stripped if present)
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
    pool: Arc<SqlitePool>,
    /// Optional YjsState for real-time collaborative editing
    /// When present, edits are applied through Yjs for live sync
    yjs_state: Option<YjsState>,
}

impl PageEditorTool {
    pub fn new(pool: Arc<SqlitePool>, yjs_state: Option<YjsState>) -> Self {
        Self { pool, yjs_state }
    }

    /// Strip CriticMarkup markers from content
    /// Keeps the content inside addition markers, removes deletion markers entirely
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

    /// Create a new page with optional initial content
    ///
    /// Content is applied directly - no CriticMarkup needed.
    /// The user does NOT need to accept/reject for new pages.
    /// Any CriticMarkup markers are automatically stripped as a safety net.
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
            cover_url: None,
            tags: None,
            space_id: None,
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

    /// Edit a page using find/replace
    ///
    /// The 'find' text is located in the document and replaced with clean 'replace' text.
    /// Any CriticMarkup markers in the replace text are stripped (safety net).
    /// Empty 'find' string means replace entire document.
    ///
    /// The frontend handles highlighting and accept/reject UI via decorations.
    ///
    /// When YjsState is available, edits go through Yjs for real-time sync to connected clients.
    /// Otherwise, falls back to direct database update (legacy behavior).
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

        // Check edit permission if chat_id is available
        if let Some(ref chat_id) = context.chat_id {
            let has_perm = chat_permissions::has_permission(self.pool.as_ref(), chat_id, &page_id)
                .await
                .unwrap_or(false);

            if !has_perm {
                // Get page title for the permission prompt
                let page_title = pages::get_page(self.pool.as_ref(), &page_id)
                    .await
                    .map(|p| p.title)
                    .unwrap_or_else(|_| "Unknown".to_string());

                return Ok(ToolResult::success(serde_json::json!({
                    "permission_needed": true,
                    "entity_id": page_id,
                    "entity_type": "page",
                    "entity_title": page_title,
                    "message": format!("Permission required to edit \"{}\"", page_title),
                })));
            }
        }

        // Strip any CriticMarkup markers from replace text (clean document)
        // Frontend handles highlighting via decorations, not document markup
        let replace_content = Self::strip_critic_markup(&args.replace);

        // Apply the edit through Yjs if available, otherwise fall back to database
        if let Some(ref yjs_state) = self.yjs_state {
            // Apply through Yjs for real-time sync
            yjs_state.apply_text_edit(&page_id, &args.find, &replace_content)
                .await
                .map_err(|e| ToolError::ExecutionFailed(e))?;
        } else {
            // Fallback: direct database update (no real-time sync)
            tracing::warn!("YjsState not available, falling back to direct database update");

            // Verify page exists and get current content
            let page = pages::get_page(self.pool.as_ref(), &page_id)
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!("Failed to get page: {}", e)))?;

            // Validate find text exists (unless empty for full replacement)
            if !args.find.is_empty() && !page.content.contains(&args.find) {
                return Err(ToolError::ExecutionFailed(format!(
                    "Text not found in page: '{}'",
                    if args.find.len() > 50 {
                        format!("{}...", &args.find[..50])
                    } else {
                        args.find.clone()
                    }
                )));
            }

            // Apply the edit to page content
            let new_content = if args.find.is_empty() {
                replace_content.clone()
            } else {
                page.content.replacen(&args.find, &replace_content, 1)
            };

            // Update the page in the database
            let update_req = pages::UpdatePageRequest {
                title: None,
                content: Some(new_content),
                icon: None,
                cover_url: None,
                tags: None,
            };

            pages::update_page(self.pool.as_ref(), &page_id, update_req)
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!("Failed to update page: {}", e)))?;
        }

        // Generate edit ID for tracking (edit_{hash16})
        let edit_id = ids::generate_id("edit", &[&page_id, &chrono::Utc::now().to_rfc3339()]);

        // Return the edit details for frontend to track and display diff
        let result = EditResult {
            edit_id,
            page_id: page_id.clone(),
            find: args.find,      // Original text (for diff display)
            replace: args.replace, // New text (clean, for reject/undo)
        };

        Ok(ToolResult::success(serde_json::json!({
            "edit": result,
            "applied": true,
            "message": "Edit applied successfully.",
        })))
    }
}

impl std::fmt::Debug for PageEditorTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PageEditorTool").finish()
    }
}

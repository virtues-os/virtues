//! Page editor tool implementation
//!
//! Provides three separate tools for page operations:
//! - create_page: Create a new page with content (no CriticMarkup needed)
//! - get_page_content: Read current page content before editing
//! - edit_page: Apply edits using simple find/replace with CriticMarkup

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;

use super::executor::{ToolContext, ToolError, ToolResult};
use crate::api::pages;

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
    /// Replacement text with CriticMarkup markers
    pub replace: String,
}

// ============================================================================
// Result structs
// ============================================================================

/// Result for edit_page - contains the edit info for frontend to apply
#[derive(Debug, Serialize)]
pub struct EditResult {
    /// Unique ID for this edit
    pub edit_id: String,
    /// Page being edited
    pub page_id: String,
    /// Text to find (for frontend validation)
    pub find: String,
    /// Replacement text with CriticMarkup
    pub replace: String,
}

// ============================================================================
// Page Editor Tool
// ============================================================================

/// Page editor tool - handles create, read, and edit operations
#[derive(Clone)]
pub struct PageEditorTool {
    pool: Arc<SqlitePool>,
}

impl PageEditorTool {
    pub fn new(pool: Arc<SqlitePool>) -> Self {
        Self { pool }
    }

    /// Strip CriticMarkup markers from content
    /// Keeps the content inside addition markers, removes deletion markers entirely
    fn strip_critic_markup(content: &str) -> String {
        use regex::Regex;
        
        // Remove addition markers but keep content: {++text++} -> text
        let addition_re = Regex::new(r"\{\+\+([\s\S]*?)\+\+\}").unwrap();
        let result = addition_re.replace_all(content, "$1");
        
        // Remove deletion markers and content: {--text--} -> ""
        let deletion_re = Regex::new(r"\{--([\s\S]*?)--\}").unwrap();
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

        let page = pages::get_page(self.pool.as_ref(), &page_id)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to get page: {}", e)))?;

        Ok(ToolResult::success(serde_json::json!({
            "page_id": page.id,
            "title": page.title,
            "content": page.content,
            "content_length": page.content.len(),
        })))
    }

    /// Edit a page using find/replace with CriticMarkup
    ///
    /// The 'find' text is located in the document and replaced with 'replace'.
    /// CriticMarkup markers in 'replace' are stripped - the result is clean content.
    /// Empty 'find' string means replace entire document.
    /// 
    /// The edit is applied server-side and the result is returned as a historical record.
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

        // Strip CriticMarkup from replacement text to get clean content
        let clean_replace = Self::strip_critic_markup(&args.replace);

        // Apply the edit to page content
        let new_content = if args.find.is_empty() {
            // Full document replacement
            clean_replace.clone()
        } else {
            // Find and replace
            page.content.replacen(&args.find, &clean_replace, 1)
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

        // Generate edit ID for tracking
        let edit_id = format!(
            "edit_{}",
            uuid::Uuid::new_v4().to_string().replace("-", "")[..12].to_string()
        );

        // Return the edit as a historical record (what was changed)
        let result = EditResult {
            edit_id,
            page_id: page_id.clone(),
            find: args.find,
            replace: args.replace, // Keep original with CriticMarkup for display
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

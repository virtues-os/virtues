//! Tool execution dispatcher
//!
//! The ToolExecutor is responsible for routing tool calls to their implementations
//! and returning structured results.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;

use super::{PageEditorTool, SemanticSearchTool, SqlQueryTool, WebSearchTool};
use crate::server::yjs::YjsState;

/// Context provided to tools during execution
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// Current page ID (for edit_page tool)
    pub page_id: Option<String>,
    /// User ID
    pub user_id: Option<String>,
    /// Space ID
    pub space_id: Option<String>,
    /// Chat ID (for permission checking)
    pub chat_id: Option<String>,
}

impl Default for ToolContext {
    fn default() -> Self {
        Self {
            page_id: None,
            user_id: None,
            space_id: None,
            chat_id: None,
        }
    }
}

/// Result from tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Whether the tool executed successfully
    pub success: bool,
    /// The result data (tool-specific JSON)
    pub data: serde_json::Value,
    /// Optional error message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ToolResult {
    /// Create a successful result
    pub fn success(data: serde_json::Value) -> Self {
        Self {
            success: true,
            data,
            error: None,
        }
    }

    /// Create an error result
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: serde_json::Value::Null,
            error: Some(message.into()),
        }
    }
}

/// Tool execution errors
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Unknown tool: {0}")]
    UnknownTool(String),

    #[error("Invalid parameters: {0}")]
    InvalidParameters(String),

    #[error("Tool execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Tool not enabled: {0}")]
    NotEnabled(String),

    #[error("Missing context: {0}")]
    MissingContext(String),
}

/// Tool executor - routes tool calls to implementations
#[derive(Clone)]
pub struct ToolExecutor {
    _pool: Arc<SqlitePool>,
    tollbooth_url: String,
    _tollbooth_secret: String,
    web_search: WebSearchTool,
    semantic_search: SemanticSearchTool,
    sql_query: SqlQueryTool,
    page_editor: PageEditorTool,
}

impl ToolExecutor {
    /// Create a new tool executor
    pub fn new(pool: SqlitePool, tollbooth_url: String, tollbooth_secret: String) -> Self {
        let pool = Arc::new(pool);
        Self {
            web_search: WebSearchTool::new(tollbooth_url.clone(), tollbooth_secret.clone()),
            semantic_search: SemanticSearchTool::new(pool.clone()),
            sql_query: SqlQueryTool::new(pool.clone()),
            page_editor: PageEditorTool::new(pool.clone(), None),
            _pool: pool,
            tollbooth_url,
            _tollbooth_secret: tollbooth_secret,
        }
    }

    /// Create a new tool executor with YjsState for real-time page editing
    pub fn new_with_yjs(
        pool: SqlitePool,
        tollbooth_url: String,
        tollbooth_secret: String,
        yjs_state: YjsState,
    ) -> Self {
        let pool = Arc::new(pool);
        Self {
            web_search: WebSearchTool::new(tollbooth_url.clone(), tollbooth_secret.clone()),
            semantic_search: SemanticSearchTool::new(pool.clone()),
            sql_query: SqlQueryTool::new(pool.clone()),
            page_editor: PageEditorTool::new(pool.clone(), Some(yjs_state)),
            _pool: pool,
            tollbooth_url,
            _tollbooth_secret: tollbooth_secret,
        }
    }

    /// Create from environment variables
    pub fn from_env(pool: SqlitePool) -> Result<Self, ToolError> {
        let tollbooth_url = std::env::var("TOLLBOOTH_URL")
            .unwrap_or_else(|_| "http://localhost:9002".to_string());
        let tollbooth_secret = std::env::var("TOLLBOOTH_INTERNAL_SECRET")
            .map_err(|_| ToolError::ExecutionFailed("TOLLBOOTH_INTERNAL_SECRET not set".into()))?;

        Ok(Self::new(pool, tollbooth_url, tollbooth_secret))
    }

    /// Execute a tool by name with given arguments
    pub async fn execute(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        tracing::info!(tool = tool_name, "Executing tool");

        match tool_name {
            "think" => {
                // No-op: the thought is captured in the tool call arguments.
                // Return minimal acknowledgment to avoid doubling token cost.
                Ok(ToolResult::success(serde_json::json!({ "acknowledged": true })))
            }
            "web_search" => self.web_search.execute(arguments).await,
            "semantic_search" => self.semantic_search.execute(arguments).await,
            "sql_query" => self.sql_query.execute(arguments).await,
            "code_interpreter" => self.execute_code_interpreter(arguments).await,
            // Page editing tools - all routed to PageEditorTool
            "create_page" => self.page_editor.create_page(arguments).await,
            "get_page_content" => self.page_editor.get_page_content(arguments, context).await,
            "edit_page" => self.page_editor.edit_page(arguments, context).await,
            _ => Err(ToolError::UnknownTool(tool_name.to_string())),
        }
    }

    /// Execute Python code in sandboxed environment
    async fn execute_code_interpreter(
        &self,
        arguments: serde_json::Value,
    ) -> Result<ToolResult, ToolError> {
        let code = arguments
            .get("code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("code is required".into()))?;

        let timeout = arguments
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(60) as u32;

        let request = crate::api::code::ExecuteCodeRequest {
            code: code.to_string(),
            timeout,
        };

        let response = crate::api::code::execute_code(request).await;

        if response.success {
            Ok(ToolResult::success(serde_json::json!({
                "output": response.stdout,
                "stderr": response.stderr,
                "execution_time_ms": response.execution_time_ms,
            })))
        } else {
            // Return the error but still as a "successful" tool call
            // so the LLM can see what went wrong and potentially fix it
            Ok(ToolResult {
                success: false,
                data: serde_json::json!({
                    "output": response.stdout,
                    "stderr": response.stderr,
                    "error": response.error,
                    "execution_time_ms": response.execution_time_ms,
                }),
                error: response.error,
            })
        }
    }

    /// Get the list of available tool names
    pub fn available_tools(&self) -> Vec<&'static str> {
        vec!["think", "web_search", "semantic_search", "sql_query", "code_interpreter", "create_page", "get_page_content", "edit_page"]
    }

    /// Check if a tool is available
    pub fn has_tool(&self, name: &str) -> bool {
        self.available_tools().contains(&name)
    }
}

impl std::fmt::Debug for ToolExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolExecutor")
            .field("tollbooth_url", &self.tollbooth_url)
            .field("available_tools", &self.available_tools())
            .finish()
    }
}

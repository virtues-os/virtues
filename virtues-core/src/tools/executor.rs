//! Tool execution dispatcher
//!
//! The ToolExecutor is responsible for routing tool calls to their implementations
//! and returning structured results.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;

use super::{PageEditorTool, SemanticSearchTool, SqlQueryTool, WebSearchTool};
use crate::server::yjs::YjsState;

/// Lifecycle status of a Deep Research subagent (worker), for the live panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentStatus {
    Thinking,
    Done,
    Failed,
}

impl SubagentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SubagentStatus::Thinking => "thinking",
            SubagentStatus::Done => "done",
            SubagentStatus::Failed => "failed",
        }
    }
}

/// A live update from a Deep Research worker, streamed out to the panel via the
/// `subagent_tx` side-channel on [`ToolContext`].
#[derive(Debug, Clone)]
pub struct SubagentUpdate {
    /// Unique per-dispatch id, so multiple dispatch rounds in one turn don't collide in the panel.
    pub dispatch_id: u64,
    pub id: usize,
    pub title: String,
    pub model: String,
    pub status: SubagentStatus,
    pub tokens: u32,
}

/// Context provided to tools during execution
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// Current page ID (for edit_page tool)
    pub page_id: Option<String>,
    /// User ID
    pub user_id: Option<String>,
    /// Notebook ID
    pub notebook_id: Option<String>,
    /// How the notebook shapes retrieval: Weighted (Open chat) or Exclusive
    /// (Scoped/grounded chat). Meaningless without a notebook_id.
    pub scope_mode: crate::search::ScopeMode,
    /// Chat ID (for permission checking)
    pub chat_id: Option<String>,
    /// Applet ID (set when running as an action — for action memory tool)
    pub applet_id: Option<String>,
    /// Side-channel for streaming Deep Research subagent status to the live panel.
    /// Set by the chat handler; `None` for headless/action runs.
    pub subagent_tx: Option<tokio::sync::mpsc::Sender<SubagentUpdate>>,
    /// Cancellation token for the turn, so long-running tools (Deep Research workers) can be
    /// stopped when the user cancels or disconnects.
    pub cancel_token: Option<tokio_util::sync::CancellationToken>,
    /// Shared per-turn budget of subagent workers, so a Deep Research turn can't fan out without
    /// bound across repeated dispatches. `None` = unbounded (non-chat callers).
    pub worker_budget: Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
}

impl Default for ToolContext {
    fn default() -> Self {
        Self {
            page_id: None,
            user_id: None,
            notebook_id: None,
            scope_mode: crate::search::ScopeMode::default(),
            chat_id: None,
            applet_id: None,
            subagent_tx: None,
            cancel_token: None,
            worker_budget: None,
        }
    }
}

/// Media a tool wants the model to actually look at, rather than describe.
///
/// A tool result is a string, so until now a tool could tell the model that an
/// image existed but never hand it over — the multimodal path ran one way,
/// inward from the browser, and nothing on the server could construct a part.
/// An attachment is that missing direction: the agent loop turns it into the
/// same image content block a pasted screenshot produces, so a file the model
/// found is worth exactly as much as a file the user dropped in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolAttachment {
    /// IANA type — `image/png`, `application/pdf`. Decides how (and whether)
    /// the loop can attach it.
    pub media_type: String,
    /// A `data:` URL. Inline rather than a link because the provider fetches
    /// nothing from this box: it is on someone's desk behind a NAT, and a URL
    /// that only resolves on the LAN would silently arrive empty.
    pub data_url: String,
    /// Shown to the model alongside the media so it can name what it looked at.
    pub filename: String,
}

/// Ceiling on a single attached file, before base64 inflates it by 4/3.
///
/// Sized for what this is actually for — screenshots and photos, which land
/// well under it — rather than for the largest image a drive can hold. An
/// attachment stays in the conversation for every subsequent turn, so the cost
/// of one careless 40MB scan is paid again on every message that follows.
const MAX_ATTACHMENT_BYTES: i64 = 5 * 1024 * 1024;

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
    /// Media for the model to see on the next turn. Empty for nearly every
    /// tool, and skipped when empty so no existing result JSON changes shape.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<ToolAttachment>,
}

impl ToolResult {
    /// Create a successful result
    pub fn success(data: serde_json::Value) -> Self {
        Self {
            success: true,
            data,
            error: None,
            attachments: Vec::new(),
        }
    }

    /// Create an error result
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: serde_json::Value::Null,
            error: Some(message.into()),
            attachments: Vec::new(),
        }
    }

    /// Attach media for the model to look at on the next turn.
    pub fn with_attachments(mut self, attachments: Vec<ToolAttachment>) -> Self {
        self.attachments = attachments;
        self
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
    _pool: Arc<PgPool>,
    web_search: WebSearchTool,
    semantic_search: SemanticSearchTool,
    sql_query: SqlQueryTool,
    page_editor: PageEditorTool,
    yjs_state: Option<YjsState>,
}

impl ToolExecutor {
    /// Create a new tool executor. Tools that reach virtues-api (web_search)
    /// go through `BearerClient`, which sources the URL + bearer itself.
    pub fn new(pool: PgPool) -> Self {
        let pool = Arc::new(pool);
        Self {
            web_search: WebSearchTool::new((*pool).clone()),
            semantic_search: SemanticSearchTool::new(pool.clone()),
            sql_query: SqlQueryTool::new(pool.clone()),
            page_editor: PageEditorTool::new(pool.clone(), None),
            _pool: pool,
            yjs_state: None,
        }
    }

    /// Create a new tool executor with YjsState for real-time page editing
    /// and action dispatch (required by the `run_applet` tool).
    pub fn new_with_yjs(pool: PgPool, yjs_state: YjsState) -> Self {
        let pool = Arc::new(pool);
        Self {
            web_search: WebSearchTool::new((*pool).clone()),
            semantic_search: SemanticSearchTool::new(pool.clone()),
            sql_query: SqlQueryTool::new(pool.clone()),
            page_editor: PageEditorTool::new(pool.clone(), Some(yjs_state.clone())),
            _pool: pool,
            yjs_state: Some(yjs_state),
        }
    }

    /// Create from environment.
    pub fn from_env(pool: PgPool) -> Result<Self, ToolError> {
        Ok(Self::new(pool))
    }

    /// Tools that require an explicit "I allow" from the user before running, because they
    /// destroy something or take a real-world / outbound action. Everything else runs freely
    /// (reversible, local). The free/gated split is the whole permission model.
    const PERMISSION_REQUIRED: &'static [&'static str] =
        &["run_applet", "delete_applet", "run_applet", "delete_applet"];

    /// If `tool_name` is gated and the user hasn't granted it for this chat, return a
    /// `permission_needed` result (the frontend then shows an inline allow/deny prompt and
    /// regenerates on approval). Returns `None` when the tool may run.
    async fn check_tool_permission(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        context: &ToolContext,
    ) -> Result<Option<ToolResult>, ToolError> {
        if !Self::PERMISSION_REQUIRED.contains(&tool_name) {
            return Ok(None);
        }
        // Only interactive chat is gated. Autonomous action runs set `applet_id` (and may carry a
        // linked `chat_id`) but have no user present to approve, so they must run ungated.
        if context.applet_id.is_some() {
            return Ok(None);
        }
        // Headless calls with no chat aren't gated either.
        let Some(chat_id) = context.chat_id.as_deref() else {
            return Ok(None);
        };
        // The gated action tools all identify their target via `id`. If absent, let the tool
        // surface its own validation error.
        let Some(applet_id) = arguments.get("id").and_then(|v| v.as_str()) else {
            return Ok(None);
        };

        let granted =
            crate::api::chat_permissions::has_permission(self._pool.as_ref(), chat_id, applet_id)
                .await
                .unwrap_or(false);
        if granted {
            return Ok(None);
        }

        let title = crate::scheduler::applets::get_applet(self._pool.as_ref(), applet_id)
            .await
            .map(|a| a.name)
            .unwrap_or_else(|_| "this action".to_string());

        let verb = if tool_name.starts_with("delete_") { "delete" } else { "run" };

        Ok(Some(ToolResult::success(serde_json::json!({
            "permission_needed": true,
            "entity_id": applet_id,
            "entity_type": "action",
            "entity_title": title,
            "message": format!("AI wants to {verb} \"{title}\""),
        }))))
    }

    /// Execute a tool by name with given arguments
    pub async fn execute(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        tracing::info!(tool = tool_name, "Executing tool");

        // Tool-based write permissions: destructive/outbound tools require an explicit
        // "I allow" from the user before they run (see PERMISSION_REQUIRED).
        if let Some(prompt) = self.check_tool_permission(tool_name, &arguments, context).await? {
            return Ok(prompt);
        }

        match tool_name {
            "think" => {
                // No-op: the thought is captured in the tool call arguments.
                // Return minimal acknowledgment to avoid doubling token cost.
                Ok(ToolResult::success(serde_json::json!({ "acknowledged": true })))
            }
            "propose_narrative_identity_edit" => {
                self.execute_propose_narrative_identity(arguments).await
            }
            "update_memory" => self.execute_update_memory(arguments).await,
            "set_user_name" => self.execute_set_user_name(arguments).await,
            "set_assistant_name" => self.execute_set_assistant_name(arguments).await,
            "web_search" => self.web_search.execute(arguments).await,
            "semantic_search" => {
                self.semantic_search
                    .execute(arguments, context.notebook_id.as_deref(), context.scope_mode)
                    .await
            }
            "sql_query" => self.sql_query.execute(arguments).await,
            "sql_write" => super::sql_write::execute(&self._pool, arguments).await,
            "read_asset" => self.execute_read_asset(arguments).await,
            "code_interpreter" => self.execute_code_interpreter(arguments).await,
            // Deep Research fan-out: spawn read-only research workers in parallel.
            "dispatch_subagents" => {
                crate::agent::subagent::dispatch(self._pool.clone(), arguments, context).await
            }
            // Page editing tools - all routed to PageEditorTool
            "create_page" => self.page_editor.create_page(arguments).await,
            "get_page_content" => self.page_editor.get_page_content(arguments, context).await,
            "edit_page" => self.page_editor.edit_page(arguments, context).await,
            // Applet setup
            "setup_applet" => super::applet_setup::execute(&self._pool, arguments, context).await,
            // Applet memory (persistent scratchpad for actions across runs)
            "update_applet_memory" => self.execute_update_applet_memory(arguments, context).await,
            // Applet management — list / get / edit / delete / run
            "list_applets" => super::applet_management::list_applets(&self._pool, arguments).await,
            "get_applet" => super::applet_management::get_applet(&self._pool, arguments).await,
            "edit_applet" => super::applet_management::edit_applet(&self._pool, arguments).await,
            "delete_applet" => super::applet_management::delete_applet(&self._pool, arguments).await,
            "run_applet" => {
                let yjs = self.yjs_state.as_ref().ok_or_else(|| {
                    ToolError::ExecutionFailed(
                        "run_applet tool requires YjsState — executor constructed without it".into(),
                    )
                })?;
                super::applet_management::run_applet(&self._pool, yjs, arguments, context).await
            }
            // Dayline event CRUD (used by hourly/EOD actions)
            "dayline_event" => super::dayline_events::execute(&self._pool, arguments, context).await,
            // Project item fetch (for attached project context lens)
            "get_project_item" => self.execute_get_project_item(arguments).await,
            // Text-to-image generation (rendered inline to the user)
            "generate_image" => self.execute_generate_image(arguments).await,
            _ => Err(ToolError::UnknownTool(tool_name.to_string())),
        }
    }

    /// Generate an image from a text prompt via the gateway image model, returned
    /// as a base64 data URL the chat renders inline (and persists/reloads as-is).
    async fn execute_generate_image(
        &self,
        arguments: serde_json::Value,
    ) -> Result<ToolResult, ToolError> {
        let prompt = arguments
            .get("prompt")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::InvalidParameters("prompt is required".into()))?;

        let png = crate::api::image_gen::generate_image_via_gateway(&self._pool, prompt)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Image generation failed: {e}")))?;

        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &png);

        Ok(ToolResult::success(serde_json::json!({
            "url": format!("data:image/png;base64,{b64}"),
            "prompt": prompt,
        })))
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
                attachments: Vec::new(),
            })
        }
    }

    /// Hand a stored file to the model to look at.
    ///
    /// The point of this tool is the attachment, not the JSON: for an image
    /// the data is what answers the question, and a caption written from the
    /// filename would be a guess dressed as a reading. So a file we cannot
    /// attach returns a plain refusal with a reason, never a description.
    async fn execute_read_asset(
        &self,
        arguments: serde_json::Value,
    ) -> Result<ToolResult, ToolError> {
        use base64::Engine;

        let raw = arguments
            .get("file_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if raw.is_empty() {
            return Err(ToolError::InvalidParameters("file_id is required".into()));
        }
        // Ref URLs are how files are named everywhere else in the prompt, so
        // accept one rather than making the model remember which surface it is
        // talking to.
        let file_id = raw.rsplit('/').next().unwrap_or(raw);

        let storage = crate::storage::Storage::file(
            crate::storage::lake::lake_root()
                .to_string_lossy()
                .into_owned(),
        )
        .map_err(|e| ToolError::ExecutionFailed(format!("Storage unavailable: {e}")))?;
        let config = crate::api::DriveConfig::new(std::sync::Arc::new(storage));

        let (file, bytes) =
            match crate::api::drive::download_file(&self._pool, &config, file_id).await {
                Ok(v) => v,
                Err(e) => {
                    return Ok(ToolResult::success(serde_json::json!({
                        "shown": false,
                        "file_id": file_id,
                        "reason": format!("Could not read that file: {e}"),
                    })))
                }
            };

        let mime = file.mime_type.clone().unwrap_or_default();
        if !mime.starts_with("image/") {
            return Ok(ToolResult::success(serde_json::json!({
                "shown": false,
                "file_id": file_id,
                "filename": file.filename,
                "mime_type": mime,
                "reason": "Only images can be looked at directly today. For a document, \
                           its extracted text is what semantic_search indexes.",
            })));
        }

        // Base64 inflates by 4/3, and this rides in the context window of every
        // subsequent turn of the conversation — not just the next one. A cap
        // that refuses loudly beats one that quietly poisons a long chat.
        if bytes.len() as i64 > MAX_ATTACHMENT_BYTES {
            return Ok(ToolResult::success(serde_json::json!({
                "shown": false,
                "file_id": file_id,
                "filename": file.filename,
                "size_bytes": bytes.len(),
                "reason": format!(
                    "That image is {:.1}MB, over the {:.0}MB limit for looking at a file directly.",
                    bytes.len() as f64 / 1_048_576.0,
                    MAX_ATTACHMENT_BYTES as f64 / 1_048_576.0
                ),
            })));
        }

        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let attachment = ToolAttachment {
            media_type: mime.clone(),
            data_url: format!("data:{mime};base64,{encoded}"),
            filename: file.filename.clone(),
        };

        Ok(
            ToolResult::success(serde_json::json!({
                "shown": true,
                "file_id": file_id,
                "filename": file.filename,
                "mime_type": mime,
                "size_bytes": bytes.len(),
                "note": "The image follows this result. Describe what you actually see in it.",
            }))
            .with_attachments(vec![attachment]),
        )
    }

    /// Update AI persistent memory
    /// Leave a note proposing an addition to the narrative identity.
    ///
    /// **Propose, never write.** The narrative identity is in the system prompt
    /// of every conversation, so a model editing it directly would be editing
    /// the lens it is seen through — quietly, and in its own favour if it drifts.
    /// This writes a `wiki_notes` row and nothing else; the user sees Add or
    /// Dismiss, and the document changes only if they choose.
    ///
    /// The note carries `why` as its citation. A machine note must cite (the DB
    /// enforces it), and for a proposal drawn from a conversation the honest
    /// source is the conversation itself — so the reason the model gives IS the
    /// evidence the user judges it on.
    async fn execute_propose_narrative_identity(
        &self,
        arguments: serde_json::Value,
    ) -> Result<ToolResult, ToolError> {
        let text = arguments
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let why = arguments
            .get("why")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();

        if text.is_empty() {
            return Err(ToolError::InvalidParameters(
                "A proposal needs text".to_string(),
            ));
        }

        let body = if why.is_empty() {
            text.to_string()
        } else {
            format!("{text}\n\n— proposed because: {why}")
        };

        sqlx::query(
            "INSERT INTO wiki_notes (subject_type, subject_id, kind, body, author, source_refs) \
             VALUES ('narrative_identity', 'nar_identity_001', 'observation', $1, 'ai', $2)",
        )
        .bind(&body)
        .bind(serde_json::json!([format!("conversation: {why}")]))
        .execute(self._pool.as_ref())
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to save proposal: {e}")))?;

        Ok(ToolResult::success(serde_json::json!({
            "status": "proposed",
            "message": "Left this for them to accept or dismiss on their Narrative Identity page. \
                        It has NOT been added — do not tell them it has."
        })))
    }

    async fn execute_update_memory(
        &self,
        arguments: serde_json::Value,
    ) -> Result<ToolResult, ToolError> {
        let content = arguments
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("content is required".into()))?;

        // Cap at 2000 chars (find a valid UTF-8 boundary)
        let content = if content.len() > 2000 {
            let end = content.char_indices()
                .map(|(i, c)| i + c.len_utf8())
                .take_while(|&i| i <= 2000)
                .last()
                .unwrap_or(0);
            &content[..end]
        } else {
            content
        };

        sqlx::query("UPDATE app_assistant_profile SET memory = $1 WHERE id = '00000000-0000-0000-0000-000000000001'")
            .bind(content)
            .execute(self._pool.as_ref())
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to update memory: {}", e)))?;

        Ok(ToolResult::success(serde_json::json!({
            "saved": true,
            "length": content.len()
        })))
    }

    /// Update an action's persistent memory (markdown scratchpad across runs).
    /// Only works when called from an action context (chat_id must map to an action).
    async fn execute_update_applet_memory(
        &self,
        arguments: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let content = arguments
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("content is required".into()))?;

        let applet_id = context.applet_id.as_deref()
            .ok_or_else(|| ToolError::ExecutionFailed("No action context — this tool can only be used by actions".into()))?;

        // Cap at 8000 chars
        let content = if content.len() > 8000 {
            let end = content.char_indices()
                .map(|(i, c)| i + c.len_utf8())
                .take_while(|&i| i <= 8000)
                .last()
                .unwrap_or(0);
            &content[..end]
        } else {
            content
        };

        crate::scheduler::applets::update_memory(&self._pool, &applet_id, content)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to update action memory: {}", e)))?;

        Ok(ToolResult::success(serde_json::json!({
            "saved": true,
            "length": content.len()
        })))
    }

    /// Set the user's preferred name
    async fn execute_set_user_name(
        &self,
        arguments: serde_json::Value,
    ) -> Result<ToolResult, ToolError> {
        let name = arguments
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("name is required".into()))?;

        let name = name.trim();
        if name.is_empty() || name.len() > 100 {
            return Err(ToolError::InvalidParameters("name must be 1-100 characters".into()));
        }

        sqlx::query("UPDATE app_user_profile SET preferred_name = $1, updated_at = now() WHERE id = '00000000-0000-0000-0000-000000000001'")
            .bind(name)
            .execute(self._pool.as_ref())
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to set user name: {}", e)))?;

        // End onboarding — user name is the last piece, unlock full tools
        let _ = sqlx::query("UPDATE app_user_profile SET onboarding_status = 'active' WHERE onboarding_status = 'onboarding'")
            .execute(self._pool.as_ref())
            .await;

        Ok(ToolResult::success(serde_json::json!({
            "name": name,
            "updated": true
        })))
    }

    /// Set the AI assistant's name
    async fn execute_set_assistant_name(
        &self,
        arguments: serde_json::Value,
    ) -> Result<ToolResult, ToolError> {
        let name = arguments
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("name is required".into()))?;

        let name = name.trim();
        if name.is_empty() || name.len() > 100 {
            return Err(ToolError::InvalidParameters("name must be 1-100 characters".into()));
        }

        sqlx::query("UPDATE app_assistant_profile SET assistant_name = $1, updated_at = now() WHERE id = '00000000-0000-0000-0000-000000000001'")
            .bind(name)
            .execute(self._pool.as_ref())
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to set assistant name: {}", e)))?;

        Ok(ToolResult::success(serde_json::json!({
            "name": name,
            "updated": true
        })))
    }

    /// Fetch the full content of a project-referenced entity (page, chat, person, etc.)
    async fn execute_get_project_item(
        &self,
        arguments: serde_json::Value,
    ) -> Result<ToolResult, ToolError> {
        let item_url = arguments
            .get("item_url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("item_url is required".into()))?;

        let pool = self._pool.as_ref();

        if let Some(page_id) = item_url.strip_prefix("/page/") {
            match crate::api::get_page(pool, page_id).await {
                Ok(page) => Ok(ToolResult::success(serde_json::json!({
                    "type": "page",
                    "id": page.id,
                    "title": page.title,
                    "content": page.content,
                    "tags": page.tags,
                    "updated_at": page.updated_at,
                }))),
                Err(e) => Ok(ToolResult::error(format!("Failed to fetch page: {}", e))),
            }
        } else if let Some(chat_id) = item_url.strip_prefix("/chat/") {
            match crate::api::get_chat(pool, chat_id.to_string()).await {
                Ok(chat_detail) => {
                    let messages: Vec<serde_json::Value> = chat_detail.messages
                        .iter()
                        .rev()
                        .take(20)
                        .rev()
                        .map(|m| serde_json::json!({ "role": m.role, "content": m.content }))
                        .collect();
                    Ok(ToolResult::success(serde_json::json!({
                        "type": "chat",
                        "id": chat_detail.conversation.conversation_id,
                        "title": chat_detail.conversation.title,
                        "recent_messages": messages,
                    })))
                }
                Err(e) => Ok(ToolResult::error(format!("Failed to fetch chat: {}", e))),
            }
        } else if let Some(person_id) = item_url.strip_prefix("/person/") {
            match crate::api::get_person(pool, person_id.to_string()).await {
                Ok(person) => Ok(ToolResult::success(serde_json::json!({
                    "type": "person",
                    "id": person.id,
                    "name": person.canonical_name,
                    "content": person.content,
                }))),
                Err(e) => Ok(ToolResult::error(format!("Failed to fetch person: {}", e))),
            }
        } else if let Some(place_id) = item_url.strip_prefix("/place/") {
            match crate::api::get_wiki_place(pool, place_id.to_string()).await {
                Ok(place) => Ok(ToolResult::success(serde_json::json!({
                    "type": "place",
                    "id": place.id,
                    "name": place.name,
                    "content": place.content,
                    "address": place.address,
                }))),
                Err(e) => Ok(ToolResult::error(format!("Failed to fetch place: {}", e))),
            }
        } else if let Some(org_id) = item_url.strip_prefix("/org/") {
            match crate::api::get_organization(pool, org_id.to_string()).await {
                Ok(org) => Ok(ToolResult::success(serde_json::json!({
                    "type": "organization",
                    "id": org.id,
                    "name": org.canonical_name,
                    "content": org.content,
                }))),
                Err(e) => Ok(ToolResult::error(format!("Failed to fetch organization: {}", e))),
            }
        } else if let Some(notebook_id) = item_url.strip_prefix("/notebook/") {
            match crate::api::notebooks::get_notebook(pool, notebook_id).await {
                Ok(detail) => {
                    let members: Vec<&str> =
                        detail.items.iter().map(|i| i.url.as_str()).collect();
                    Ok(ToolResult::success(serde_json::json!({
                        "type": "notebook",
                        "id": detail.notebook.id,
                        "name": detail.notebook.name,
                        "status": detail.notebook.current_status,
                        "members": members,
                    })))
                }
                Err(e) => Ok(ToolResult::error(format!("Failed to fetch notebook: {}", e))),
            }
        } else if item_url.starts_with("http://") || item_url.starts_with("https://") {
            // External URL — content lives outside Virtues. Return guidance to use web tools.
            Ok(ToolResult::success(serde_json::json!({
                "type": "external_url",
                "url": item_url,
                "note": "This is an external URL. Use the web_search tool or visit the URL directly to fetch its content."
            })))
        } else {
            Ok(ToolResult::error(format!(
                "Unsupported item URL type: {}. Supported: /page/, /chat/, /notebook/, /person/, /place/, /org/, or https://",
                item_url
            )))
        }
    }

    /// Get the list of available tool names
    pub fn available_tools(&self) -> Vec<&'static str> {
        vec![
            "think",
            "update_memory",
            "set_user_name",
            "set_assistant_name",
            "web_search",
            "semantic_search",
            "sql_query",
            "sql_write",
            "code_interpreter",
            "create_page",
            "get_page_content",
            "edit_page",
            "setup_applet",
            "update_applet_memory",
            "list_applets",
            "get_applet",
            "edit_applet",
            "delete_applet",
            "run_applet",
            "dayline_event",
            "get_project_item",
        ]
    }

    /// Check if a tool is available
    pub fn has_tool(&self, name: &str) -> bool {
        self.available_tools().contains(&name)
    }
}

impl std::fmt::Debug for ToolExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolExecutor")
            .field("available_tools", &self.available_tools())
            .finish()
    }
}

/// Live checks for read_asset against a dev box's real drive. Ignored by
/// default — CI has neither the database nor the object store.
///   cargo test -p virtues --lib tools::executor::live_read_asset -- --ignored --nocapture
#[cfg(test)]
mod live_read_asset {
    use super::*;

    async fn executor() -> ToolExecutor {
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://virtues:virtues@localhost:5432/virtues".to_string());
        ToolExecutor::new(PgPool::connect(&url).await.expect("dev database"))
    }

    /// Walks every image in the drive rather than trusting the first one.
    ///
    /// A drive row can outlive its bytes — this dev checkout has a
    /// content-addressed `.media/` row whose blob was never copied here — and
    /// a test that picked one file would report the resulting refusal as a
    /// failure of the tool. The refusal is the tool working. What must be
    /// proven is that a file WITH bytes comes back as something to look at.
    #[tokio::test]
    #[ignore]
    async fn an_unextracted_screenshot_comes_back_as_something_to_look_at() {
        let ex = executor().await;
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM app_drive_files
             WHERE mime_type LIKE 'image/%' AND deleted_at IS NULL AND is_folder = FALSE
             ORDER BY size_bytes ASC",
        )
        .fetch_all(ex._pool.as_ref())
        .await
        .expect("query");
        if ids.is_empty() {
            println!("no images in this drive; nothing to check");
            return;
        }

        let mut refused: Vec<String> = Vec::new();
        for id in &ids {
            // A ref URL, the form the model reads in the notebook block — the
            // bare-id path is the same call with the prefix stripped.
            let out = ex
                .execute_read_asset(serde_json::json!({ "file_id": format!("/drive/{id}") }))
                .await
                .expect("tool ran");

            if out.data["shown"] != true {
                refused.push(format!("{id}: {}", out.data["reason"]));
                continue;
            }

            assert_eq!(out.attachments.len(), 1, "expected one attachment");
            let att = &out.attachments[0];
            assert!(att.media_type.starts_with("image/"), "{}", att.media_type);
            assert!(
                att.data_url
                    .starts_with(&format!("data:{};base64,", att.media_type)),
                "malformed data url prefix"
            );
            // Real bytes, not an empty envelope that would reach the model as
            // a blank image and be described as one.
            let b64 = att.data_url.split_once(";base64,").unwrap().1;
            assert!(b64.len() > 1000, "suspiciously small payload: {}", b64.len());
            println!(
                "attached {} ({}, {} base64 chars)",
                att.filename,
                att.media_type,
                b64.len()
            );
            return;
        }

        panic!(
            "no image in the drive could be shown; every one refused:\n  {}",
            refused.join("\n  ")
        );
    }

    #[tokio::test]
    #[ignore]
    async fn a_missing_file_refuses_with_a_reason_and_never_a_description() {
        let ex = executor().await;
        let out = ex
            .execute_read_asset(serde_json::json!({ "file_id": "file_does_not_exist" }))
            .await
            .expect("tool ran");
        assert!(out.data["shown"] == false);
        assert!(out.data["reason"].is_string(), "a refusal must say why");
        assert!(out.attachments.is_empty());
        println!("refusal: {}", out.data["reason"]);
    }
}

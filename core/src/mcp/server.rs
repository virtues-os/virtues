//! MCP Server implementation for Virtues
//!
//! This module implements the ServerHandler trait from rmcp to expose
//! Virtues's tools to AI assistants via the Model Context Protocol.
//!
//! Tools are defined in virtues-registry and executed via the ToolExecutor.

use rmcp::{
    handler::server::router::tool::ToolRouter,
    model::{
        Annotated, ErrorData as McpError, Implementation, ListResourcesResult,
        PaginatedRequestParam, ProtocolVersion, RawResource, ReadResourceRequestParam,
        ReadResourceResult, Resource, ResourceContents, ServerCapabilities, ServerInfo,
    },
    service::RequestContext,
    tool_handler, tool_router, RoleServer, ServerHandler,
};
use sqlx::SqlitePool;
use std::sync::Arc;

/// Virtues MCP Server
///
/// Exposes Virtues's tools to AI assistants via the Model Context Protocol.
/// 
/// Currently, this is a minimal server. Tool execution is handled by the
/// ToolExecutor in core/src/tools/, which is integrated into the chat API.
/// 
/// MCP clients (like Claude Desktop) can connect to discover available tools
/// and resources. The actual tool execution happens through the chat endpoint.
#[derive(Debug, Clone)]
pub struct VirtuesMcpServer {
    #[allow(dead_code)]
    pool: Arc<SqlitePool>,
    tool_router: ToolRouter<VirtuesMcpServer>,
}

#[tool_router]
impl VirtuesMcpServer {
    /// Create a new Virtues MCP server
    pub fn new(pool: SqlitePool) -> Self {
        let tool_router = Self::tool_router();
        let tool_count = tool_router.list_all().len();
        tracing::info!("MCP Server initialized with {} tools", tool_count);
        Self {
            pool: Arc::new(pool),
            tool_router,
        }
    }

    // Tools will be added here when MCP tool execution is implemented.
    // For now, tools are executed through the chat API's ToolExecutor.
}

#[tool_handler]
impl ServerHandler for VirtuesMcpServer {
    fn get_info(&self) -> ServerInfo {
        tracing::debug!("MCP get_info() called");
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
            server_info: Implementation {
                name: "virtues".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: Some("Virtues Personal Data Warehouse".to_string()),
                website_url: Some("https://github.com/ariata-os/ariata".to_string()),
                icons: None,
            },
            instructions: Some(
                r#"Virtues Personal Data Warehouse MCP Server

This server provides access to your personal data warehouse.

Available Tools:
1. web_search - Search the web using Exa AI
2. sql_query - Query your personal data with SQL (health, location, calendar, etc.)
3. edit_page - AI-assisted page editing with accept/reject

Note: Tools are currently executed through the Virtues chat API.
For full tool functionality, use the Virtues web interface.

Privacy & Data Sensitivity:
- All SQL queries are read-only
- No data leaves your local machine without explicit export
"#
                .to_string(),
            ),
        }
    }

    /// List available resources
    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourcesResult, McpError>> + Send + '_ {
        async move {
            // Define the baseline context resource
            let baseline_resource = RawResource {
                uri: "virtues://context/baseline".to_string(),
                name: "Baseline Context".to_string(),
                title: Some("Baseline Context".to_string()),
                description: Some(
                    "Essential context for every conversation: user identity, current date/time."
                        .to_string(),
                ),
                mime_type: Some("text/markdown".to_string()),
                size: None,
                icons: None,
            };

            let resource: Resource = Annotated::new(baseline_resource, None);

            Ok(ListResourcesResult {
                resources: vec![resource],
                next_cursor: None,
            })
        }
    }

    /// Read a specific resource
    fn read_resource(
        &self,
        request: ReadResourceRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ReadResourceResult, McpError>> + Send + '_ {
        async move {
            match request.uri.as_str() {
                "virtues://context/baseline" => {
                    let now = chrono::Local::now();
                    let formatted_date = now.format("%A, %B %d, %Y, %I:%M %p %Z").to_string();

                    let baseline_text = format!(
                        r#"# Baseline Context

**Current Date/Time**: {}
**Timezone**: {} (UTC{})

## Available Tools

1. **web_search** - Search the web for current information
2. **sql_query** - Query your personal data (health, location, calendar, etc.)
3. **edit_page** - AI-assisted page editing

## Guidelines

When responding:
- Answer conversationally and naturally
- Don't mention technical details like table names or SQL syntax
- Focus on the meaning and insights, not the data retrieval process
"#,
                        formatted_date,
                        now.format("%Z"),
                        now.format("%:z")
                    );

                    Ok(ReadResourceResult {
                        contents: vec![ResourceContents::text(baseline_text, request.uri)],
                    })
                }
                _ => Err(McpError::invalid_params(
                    format!("Unknown resource URI: {}", request.uri),
                    None,
                )),
            }
        }
    }
}

//! MCP Server implementation for Virtues
//!
//! This module implements the ServerHandler trait from rmcp to expose
//! Virtues's data warehouse as an MCP server.

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        Annotated, CallToolResult, Content, ErrorData as McpError, Implementation,
        ListResourcesResult, PaginatedRequestParam, ProtocolVersion, RawResource,
        ReadResourceRequestParam, ReadResourceResult, Resource, ResourceContents,
        ServerCapabilities, ServerInfo,
    },
    service::RequestContext,
    tool, tool_handler, tool_router, RoleServer, ServerHandler,
};
use sqlx::SqlitePool;
use std::sync::Arc;

use crate::mcp::tools;

/// Virtues MCP Server
///
/// Exposes Virtues's personal data warehouse to AI assistants via the Model Context Protocol.
#[derive(Debug, Clone)]
pub struct VirtuesMcpServer {
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
        if tool_count > 0 {
            tracing::debug!(
                "Available MCP tools: {:?}",
                tool_router
                    .list_all()
                    .iter()
                    .map(|t| &t.name)
                    .collect::<Vec<_>>()
            );
        } else {
            tracing::warn!("No MCP tools registered! Check #[tool_router] macro expansion");
        }
        Self {
            pool: Arc::new(pool),
            tool_router,
        }
    }

    /// Unified ontology tool - query factual life data
    #[tool(description = r#"Query the user's life data - what actually happened.

## Operations

1. **query**: Execute read-only SQL (SELECT only)
2. **list_tables**: Discover available tables with column names and row counts
3. **get_schema**: Get detailed schema for a specific table

## When to Use

- "Did I sleep well?" → health_sleep
- "How many steps?" → health_steps
- "Show my heart rate" → health_heart_rate (use 'bpm' column, NOT 'heart_rate')
- "Where was I?" → location_visit
- "What apps did I use?" → activity_app_usage

## Available Domains

**Health** (data_health_*)
- health_sleep: Sleep sessions (duration, quality, stages)
- health_heart_rate: BPM measurements (column: bpm)
- health_hrv: Heart rate variability
- health_steps: Daily step counts
- health_workout: Exercise (type, duration, calories)

**Location** (data_location_*)
- location_visit: Places visited (name, duration, coordinates)
- location_point: Raw GPS points

**Social** (data_social_*)
- social_email: Email messages
- social_message: SMS/iMessage

**Activity** (data_activity_*)
- activity_app_usage: App focus time
- activity_web_browsing: Web history

**Knowledge** (data_knowledge_*)
- knowledge_document: Documents
- knowledge_ai_conversation: Past AI chats

**Financial** (data_financial_*)
- financial_transaction: Bank/card transactions

## Query Tips

- Always include date filters for time-bound queries
- Use aggregations (AVG, SUM, COUNT) for summaries
- Limit results to avoid overwhelming context"#)]
    async fn virtues_query_ontology(
        &self,
        params: Parameters<tools::QueryOntologyRequest>,
    ) -> Result<CallToolResult, McpError> {
        match tools::query_ontology(&self.pool, params.0).await {
            Ok(result) => {
                let json_str = serde_json::to_string_pretty(&result).map_err(|e| {
                    McpError::internal_error(format!("Failed to serialize result: {}", e), None)
                })?;
                Ok(CallToolResult::success(vec![Content::text(json_str)]))
            }
            Err(e) => {
                // Categorize errors appropriately
                if e.contains("forbidden keyword")
                    || e.contains("Only SELECT")
                    || e.contains("exceeds maximum length")
                    || e.contains("Missing")
                    || e.contains("Invalid operation")
                    || e.contains("not found")
                    || e.contains("does not exist")
                {
                    Err(McpError::invalid_params(e, None))
                } else {
                    Err(McpError::internal_error(e, None))
                }
            }
        }
    }

    /// Semantic search across emails, messages, calendar events, AI conversations, and documents
    #[tool(
        description = "Search your personal data using natural language. Uses AI embeddings (pgvector) to find semantically similar content across emails, messages, calendar events, AI conversations, and documents (including Notion pages). Examples: 'emails about project deadline', 'messages about dinner plans', 'meetings with John about budget', 'documents about architecture'. Returns results ranked by relevance with previews and timestamps."
    )]
    async fn virtues_semantic_search(
        &self,
        params: Parameters<tools::SemanticSearchRequest>,
    ) -> Result<CallToolResult, McpError> {
        match tools::semantic_search(&self.pool, params.0).await {
            Ok(result) => {
                let json_str = serde_json::to_string_pretty(&result).map_err(|e| {
                    McpError::internal_error(format!("Failed to serialize result: {}", e), None)
                })?;
                Ok(CallToolResult::success(vec![Content::text(json_str)]))
            }
            Err(e) => {
                // Categorize errors appropriately
                if e.contains("Failed to embed query") || e.contains("Ollama") {
                    Err(McpError::internal_error(
                        format!("Embedding service unavailable: {}. Ensure Ollama is running with the nomic-embed-text model.", e),
                        None,
                    ))
                } else {
                    Err(McpError::internal_error(e, None))
                }
            }
        }
    }
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

This server provides access to your personal data warehouse, which aggregates data from:
- Health data (HealthKit: heart rate, steps, sleep, etc.)
- Location data (GPS points, visits, places)
- Social data (email, calendar events)
- Knowledge data (documents, AI conversations)

Available Tools:
1. virtues_semantic_search - Natural language search across emails, messages, calendar, AI conversations (use for "find emails about X", "messages mentioning Y")
2. virtues_query_ontology - Unified ontology tool with 3 operations: query (execute SQL), list_tables (discover tables), get_schema (get table details)

Best Practices:
- Use virtues_query_ontology with operation='list_tables' to discover what data is available
- Use virtues_query_ontology with operation='get_schema' to understand column names and types before querying
- Queries are read-only and limited to 1000 rows max
- Use aggregate functions (COUNT, AVG, etc.) to summarize large datasets
- Filter by date ranges to narrow results

Privacy & Data Sensitivity:
- All queries run in read-only transactions
- No data leaves your local machine without explicit export
- Sensitive PII can be filtered before results are returned to the model

**Data Sensitivity Notice:**
This server provides access to highly personal data that may include:
- Health metrics: Heart rate, sleep patterns, activity levels (PII: Medical data)
- Location history: Precise GPS coordinates, visited places (PII: Location tracking)
- Social data: Email content, calendar events (PII: Personal communications)
- Knowledge: Documents, notes, AI conversations (PII: Intellectual property)

When crafting queries, consider privacy-conscious approaches:
- Use WHERE clauses to filter sensitive time ranges
- Use aggregate functions (COUNT, AVG) instead of raw data dumps
- Exclude specific columns that contain PII when not needed
- Limit result sets to minimize data exposure

**Example Queries:**

```sql
-- Weekly health summary (aggregated, no raw data)
SELECT DATE(timestamp), AVG(bpm) as avg_heart_rate, COUNT(*) as measurements
FROM data_health_heart_rate
WHERE timestamp > datetime('now', '-7 days')
GROUP BY DATE(timestamp);

-- Step count trends (no location data)
SELECT DATE(timestamp), SUM(step_count) as total_steps
FROM data_health_steps
WHERE timestamp > datetime('now', '-30 days')
GROUP BY DATE(timestamp)
ORDER BY DATE(timestamp);

-- Upcoming calendar (titles only, no attendees/content)
SELECT title, start_time, end_time
FROM data_praxis_calendar
WHERE start_time BETWEEN datetime('now') AND datetime('now', '+7 days')
ORDER BY start_time;
```
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
                title: Some("Prudent Baseline Context".to_string()),
                description: Some(
                    "Essential context for every conversation: user identity, current date/time, and session information."
                        .to_string(),
                ),
                mime_type: Some("text/markdown".to_string()),
                size: None,
                icons: None,
            };

            // Wrap RawResource in Annotated to create Resource
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
                    // Get current date/time with timezone
                    let now = chrono::Local::now();
                    let formatted_date = now.format("%A, %B %d, %Y, %I:%M %p %Z").to_string();

                    // Build baseline context markdown
                    let baseline_text = format!(
                        r#"# Baseline Context

**User**: Adam Jace
**Current Date/Time**: {}
**Timezone**: {} (UTC{})

---

## Available Tools

1. **virtues_query_ontology** - Query your life data (health, location, social, activity, knowledge, financial)
   - Use `operation='list_tables'` to discover available tables
   - Use `operation='get_schema'` to understand column names before querying
   - Use `operation='query'` to execute SQL queries

2. **virtues_semantic_search** - Natural language search across emails, messages, calendar, and documents

---

## User-Facing Principles - CRITICAL

When you find information:
- **Answer conversationally**: Provide natural, helpful responses
- **Hide ALL implementation details**: Never mention query syntax, table names, database structures, or WHERE clauses
- **Keep focus on meaning**: Emphasize what happened, who was involved, and why it matters
- **Avoid observability language**: Don't say "I queried...", "the data shows...", "the table returned...", or explain your query process

### Never Say These Things:
- "Let me query the database..."
- "The data.health_heart_rate table shows..."
- "SELECT * FROM..."
- "Here's what the query returned..."

---

This context is provided at the start of every conversation to ensure natural, contextually-grounded responses without exposing technical implementation.
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

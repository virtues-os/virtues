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
use sqlx::PgPool;
use std::sync::Arc;

use crate::mcp::tools;

/// Virtues MCP Server
///
/// Exposes Virtues's personal data warehouse to AI assistants via the Model Context Protocol.
#[derive(Debug, Clone)]
pub struct VirtuesMcpServer {
    pool: Arc<PgPool>,
    tool_router: ToolRouter<VirtuesMcpServer>,
}

#[tool_router]
impl VirtuesMcpServer {
    /// Create a new Virtues MCP server
    pub fn new(pool: PgPool) -> Self {
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
    #[tool(
        description = r#"Query the user's life data - what actually happened.

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

**Health** (data.health_*)
- health_sleep: Sleep sessions (duration, quality, stages)
- health_heart_rate: BPM measurements (column: bpm)
- health_hrv: Heart rate variability
- health_steps: Daily step counts
- health_workout: Exercise (type, duration, calories)

**Location** (data.location_*)
- location_visit: Places visited (name, duration, coordinates)
- location_point: Raw GPS points

**Social** (data.social_*)
- social_email: Email messages
- social_message: SMS/iMessage

**Activity** (data.activity_*)
- activity_app_usage: App focus time
- activity_web_browsing: Web history

**Knowledge** (data.knowledge_*)
- knowledge_document: Documents
- knowledge_ai_conversation: Past AI chats

**Financial** (data.financial_*)
- financial_transaction: Bank/card transactions

## Query Tips

- Always include date filters for time-bound queries
- Use aggregations (AVG, SUM, COUNT) for summaries
- Limit results to avoid overwhelming context
- For biographical questions ('what happened', 'who did I meet'), use virtues_query_narratives instead"#
    )]
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

    // DISABLED: Exposes test dataset names like "monday-in-rome"
    // /// List all data sources and their status
    // #[tool(description = "List all connected data sources (Google, iOS, Mac, Notion, etc.) and their current status, including number of enabled streams and last sync time.")]
    // async fn virtues_list_sources(
    //     &self,
    //     _params: Parameters<tools::ListSourcesRequest>,
    // ) -> Result<CallToolResult, McpError> {
    //     match tools::list_sources(&self.pool).await {
    //         Ok(result) => {
    //             let json_str = serde_json::to_string_pretty(&result)
    //                 .map_err(|e| McpError::internal_error(
    //                     format!("Failed to serialize result: {}", e),
    //                     None
    //                 ))?;
    //             Ok(CallToolResult::success(vec![Content::text(json_str)]))
    //         }
    //         Err(e) => Err(McpError::internal_error(e, None)),
    //     }
    // }

    /// Execute a read-only SQL query against axiology and praxis tables
    #[tool(
        description = r#"Query the user's character and commitments - who they are and what they're working on.

## When to Use

- Struggles or temptations → query vices ("SELECT * FROM vices WHERE is_active = true")
- Decisions or life direction → query telos ("SELECT * FROM telos WHERE is_active = true")
- "How can I be more X?" → query virtues + temperament
- What are my goals/tasks? → query tasks, initiatives, aspirations
- Progress on a virtue → query BOTH the virtue AND tasks linked to it

## Available Tables

**Axiology (character):**
- telos: Life purpose (singular active)
- virtues: Positive patterns being cultivated
- vices: Negative patterns being resisted
- temperaments: Natural dispositions
- preferences: Affinities with people/places/activities

**Praxis (action):**
- tasks: Short-term (daily/weekly), may have virtue_ids/vice_ids
- initiatives: Medium-term (month/quarter)
- aspirations: Long-term (years)
- calendar: Scheduled events

## Key Insight

Tasks and initiatives have virtue_ids and vice_ids. When someone asks "Am I making progress on patience?", query both:
1. Axiology for the virtue (patience)
2. Tasks WHERE virtue_ids contains that virtue's ID

## Example Queries

- "SELECT title, description FROM virtues WHERE is_active = true"
- "SELECT title, description FROM vices WHERE is_active = true"
- "SELECT * FROM tasks WHERE is_active = true ORDER BY due_date"
- "SELECT * FROM calendar WHERE start_time >= CURRENT_DATE"

Shorthand table names (virtues, tasks, etc.) are auto-rewritten to data.axiology_virtue, data.praxis_task, etc."#
    )]
    async fn virtues_query_axiology(
        &self,
        params: Parameters<tools::QueryAxiologyRequest>,
    ) -> Result<CallToolResult, McpError> {
        match tools::query_axiology(&self.pool, params.0).await {
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
                {
                    Err(McpError::invalid_params(e, None))
                } else {
                    Err(McpError::internal_error(e, None))
                }
            }
        }
    }

    /// Query narrative biography summaries for specific dates, locations, or people
    #[tool(
        description = "USE THIS FIRST for biographical questions! Query your narrative biography and life story. Returns pre-synthesized prose summaries about what happened, who you met, and events on specific dates/locations. **ALWAYS call this BEFORE any other tool** when answering questions like 'who did I meet', 'what happened on [date]', 'what did I do in [location]', or any question about past events, people, or activities. This is 10x faster than querying raw data tables and contains ALL the context you need. Only use other tools if this returns no results."
    )]
    async fn virtues_query_narratives(
        &self,
        params: Parameters<tools::QueryNarrativesRequest>,
    ) -> Result<CallToolResult, McpError> {
        match tools::query_narratives(&self.pool, params.0).await {
            Ok(result) => {
                let json_str = serde_json::to_string_pretty(&result).map_err(|e| {
                    McpError::internal_error(format!("Failed to serialize result: {}", e), None)
                })?;
                Ok(CallToolResult::success(vec![Content::text(json_str)]))
            }
            Err(e) => {
                // Categorize errors appropriately
                if e.contains("Invalid date format") {
                    Err(McpError::invalid_params(e, None))
                } else {
                    Err(McpError::internal_error(e, None))
                }
            }
        }
    }

    /// Manage axiology entities (create, read, update, delete operations)
    #[tool(
        description = "Manage your axiology system - create, read, update, delete tasks, initiatives, aspirations, values, telos, virtues, vices, habits, temperaments, and preferences. Operations: 'create' (new entity), 'read' (get by ID), 'update' (modify existing), 'delete' (soft delete), 'list' (get all active). Entity types: task (daily/weekly), initiative (month/quarter), aspiration (multi-year), value (foundational principle), telos (life purpose), virtue (positive pattern), vice (negative pattern), habit (daily practice), temperament (innate disposition), preference (entity affinity)."
    )]
    async fn virtues_manage_axiology(
        &self,
        params: Parameters<tools::ManageAxiologyRequest>,
    ) -> Result<CallToolResult, McpError> {
        match tools::manage_axiology(&self.pool, params.0).await {
            Ok(result) => {
                let json_str = serde_json::to_string_pretty(&result).map_err(|e| {
                    McpError::internal_error(format!("Failed to serialize result: {}", e), None)
                })?;
                Ok(CallToolResult::success(vec![Content::text(json_str)]))
            }
            Err(e) => {
                // Categorize errors appropriately
                if e.contains("Invalid operation")
                    || e.contains("Invalid entity_type")
                    || e.contains("Missing")
                    || e.contains("not found")
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
- Axiology data (values, telos, goals, virtues, vices, habits, temperaments, preferences)

Available Tools:
1. virtues_query_narratives - Query your narrative biography and life story (use this for "what happened", "who did I meet", biographical questions)
2. virtues_semantic_search - Natural language search across emails, messages, calendar, AI conversations (use for "find emails about X", "messages mentioning Y")
3. virtues_query_ontology - Unified ontology tool with 3 operations: query (execute SQL), list_tables (discover tables), get_schema (get table details)
4. virtues_query_axiology - Execute read-only SQL queries against axiology tables
5. virtues_manage_axiology - Manage your axiology system (create, read, update, delete tasks, initiatives, aspirations, values, etc.)

Axiology System:
The axiology tables store the user's formal axiological framework:
- axiology_value: Foundational principles (Level 0)
- axiology_telos: Ultimate life purpose - singular active (Level 1)
- axiology_goal: Concrete pursuits with goal_type (work/character/experiential/relational) (Level 2)
- axiology_virtue: Positive character patterns to cultivate (Level 3)
- axiology_vice: Negative character patterns to resist (Level 3)
- axiology_temperament: Innate dispositions (neutral, stable) (Level 3)
- axiology_preference: Affinities with entities (Level 4)

Best Practices:
- Use virtues_query_ontology with operation='list_tables' to discover what data is available
- Use virtues_query_ontology with operation='get_schema' to understand column names and types before querying
- Queries are read-only and limited to 1000 rows max
- Use aggregate functions (COUNT, AVG, etc.) to summarize large datasets
- Filter by date ranges to narrow results
- Query axiology tables for context-aware decision support and dialectic navigation

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
- Axiology: Personal values, goals, virtues, preferences (PII: Psychological profile)

When crafting queries, consider privacy-conscious approaches:
- Use WHERE clauses to filter sensitive time ranges
- Use aggregate functions (COUNT, AVG) instead of raw data dumps
- Exclude specific columns that contain PII when not needed
- Limit result sets to minimize data exposure

**Example Queries:**

Privacy-conscious aggregated queries:
```sql
-- Weekly health summary (aggregated, no raw data)
SELECT DATE(timestamp), AVG(value) as avg_heart_rate, COUNT(*) as measurements
FROM data.health_heart_rate
WHERE timestamp > NOW() - INTERVAL '7 days'
GROUP BY DATE(timestamp);

-- Step count trends (no location data)
SELECT DATE(start_time), SUM(value) as total_steps
FROM data.health_steps
WHERE start_time > NOW() - INTERVAL '30 days'
GROUP BY DATE(start_time)
ORDER BY DATE(start_time);

-- Upcoming calendar (titles only, no attendees/content)
SELECT title, start_time, end_time
FROM data.praxis_calendar
WHERE start_time BETWEEN NOW() AND NOW() + INTERVAL '7 days'
ORDER BY start_time;
```

Axiology queries for context-aware assistance:
```sql
-- Active aspirations
SELECT title, description, status, target_timeframe
FROM data.praxis_aspiration
WHERE is_active = true
ORDER BY created_at DESC;

-- Active virtues to cultivate
SELECT title, description
FROM data.axiology_virtue
WHERE is_active = true;
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

## Query Strategy - INTERNAL DECISION LOGIC

### CRITICAL: ALWAYS CHECK NARRATIVES FIRST

**For ANY biographical question** (what happened, who met, what did, where was), your **FIRST and ONLY initial action** must be:

1. Call `virtues_query_narratives` with the date
2. If narratives exist → answer the question from narratives
3. ONLY if narratives are empty → then consider other tools

**DO NOT:**
- ❌ Call `virtues_query_ontology` with operation='list_tables' first
- ❌ Call `virtues_query_ontology` with operation='query' before checking narratives
- ❌ "Explore" the data before checking narratives
- ❌ Check multiple data sources when narratives have the answer


**Why narratives first?**
- Narratives are pre-synthesized prose with ALL context (people, places, events)
- 10x faster than querying raw tables
- More accurate for biographical questions
- Contain the "story" not just raw data points

**When to use other tools:**
- `virtues_query_ontology` operation='list_tables': Only when you need to discover what raw data types exist
- `virtues_query_ontology` operation='query': Only for specific metrics (exact heart rate at 3:42pm) or when narratives don't exist
- `virtues_query_axiology`: For values, goals, habits, virtues, vices queries

---

## User-Facing Principles - CRITICAL

When you find information:
- **Answer conversationally**: Say "Let me check your Monday in Rome..." then provide the answer
- **Hide ALL implementation details**: Never mention query syntax, table names, database structures, or WHERE clauses
- **Keep focus on meaning**: Emphasize what happened, who was involved, and why it matters
- **Avoid observability language**: Don't say "I queried...", "the data shows...", "the table returned...", or explain your query process

### Never Say These Things:
❌ "Let me use the virtues_query_narratives tool..."
❌ "Querying the narrative_chunks table..."
❌ "The data.narrative_chunks table shows..."
❌ "SELECT narrative_text FROM..."
❌ "Here's what the praxis_calendar table returned..."
❌ "The location_point data indicates..."
❌ "Based on ios_microphone_transcription..."


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

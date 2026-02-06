//! Built-in tool registry
//!
//! This module defines BUILT-IN tools that are part of Virtues core.
//! These are executed as native Rust functions via the ToolExecutor.
//!
//! MCP tools (user-connected) are stored in SQLite `app_mcp_tools` table
//! and executed via the MCP protocol.
//!
//! # Tool Types
//!
//! - `builtin` - Native Rust implementation (web_search, sql_query, create_page, get_page_content, edit_page)
//! - `mcp` - MCP protocol (user-connected servers, stored in SQLite)

use serde::{Deserialize, Serialize};

/// Tool type - distinguishes built-in vs MCP tools
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolType {
    /// Built-in tool - native Rust implementation
    Builtin,
    /// MCP tool - executed via MCP protocol
    Mcp,
}

/// Tool category for UI grouping
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolCategory {
    Search,
    Data,
    Edit,
}

/// Built-in tool configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolConfig {
    /// Unique tool identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Short description for UI
    pub description: String,
    /// Detailed description for LLM (helps model decide when to use)
    pub llm_description: String,
    /// JSON Schema for parameters
    pub parameters: serde_json::Value,
    /// Tool type (builtin for registry tools)
    pub tool_type: ToolType,
    /// Category for grouping in UI
    pub category: ToolCategory,
    /// Iconify icon name
    pub icon: String,
    /// Display order in UI
    pub display_order: i32,
}

/// Get default built-in tool configurations
///
/// These are the core tools that ship with Virtues:
/// - web_search: Search the web using Exa AI
/// - sql_query: Read-only SQL queries against user data
/// - code_interpreter: Execute Python code for calculations and analysis
/// - create_page: Create a new page with content
/// - get_page_content: Read current page content
/// - edit_page: Apply edits using find/replace with CriticMarkup
pub fn default_tools() -> Vec<ToolConfig> {
    vec![
        web_search_tool(),
        sql_query_tool(),
        code_interpreter_tool(),
        create_page_tool(),
        get_page_content_tool(),
        edit_page_tool(),
    ]
}

/// Web Search tool (Exa AI)
fn web_search_tool() -> ToolConfig {
    ToolConfig {
        id: "web_search".to_string(),
        name: "Web Search".to_string(),
        description: "Search the web for current information".to_string(),
        llm_description: r#"Search the web for current information using Exa AI.

Use this tool when:
- User asks about recent events, news, or current information
- You need factual information you're uncertain about
- User explicitly asks to search or look something up
- Information might have changed since your training cutoff

Do NOT use when:
- User is asking about their personal data (use sql_query instead)
- The question is purely conversational or opinion-based

Returns: Relevant web pages with titles, URLs, summaries, and text excerpts."#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query - be specific and include relevant context"
                },
                "num_results": {
                    "type": "integer",
                    "description": "Number of results (1-10)",
                    "default": 5,
                    "minimum": 1,
                    "maximum": 10
                },
                "search_type": {
                    "type": "string",
                    "enum": ["auto", "keyword", "neural"],
                    "description": "Search type: 'auto' (recommended), 'keyword' for exact matches, 'neural' for semantic",
                    "default": "auto"
                }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Search,
        icon: "ri:search-line".to_string(),
        display_order: 1,
    }
}

/// SQL Query tool (read-only data access)
fn sql_query_tool() -> ToolConfig {
    ToolConfig {
        id: "sql_query".to_string(),
        name: "Query Data".to_string(),
        description: "Query user's personal data with SQL".to_string(),
        llm_description: r#"Execute read-only SQL queries against the user's personal data.

Operations:
- 'list_tables': Get all tables with row counts
- 'get_schema': Get detailed columns for specific table(s)
- 'query': Execute a SELECT query (read-only, max 200 rows)

================================================================================
DATA TABLES (raw ontology from connected sources)
================================================================================

HEALTH
  data_health_heart_rate     BPM measurements from wearables
  data_health_hrv            Heart rate variability (ms)
  data_health_steps          Step counts
  data_health_sleep          Sleep sessions with duration & quality
  data_health_workout        Exercise sessions (type, duration, calories)

LOCATION  
  data_location_point        Raw GPS coordinates (high volume)
  data_location_visit        Place visits with arrival/departure times

SOCIAL
  data_social_email          Email messages (subject, body, from/to)
  data_social_message        Chat messages (iMessage, SMS, etc.)

CALENDAR
  data_calendar              Events with attendees, location, times

FINANCIAL (amounts stored in cents - divide by 100 for dollars)
  data_financial_account      Bank/credit/investment accounts
  data_financial_transaction  Purchases, transfers, payments
  data_financial_asset        Investment holdings (stocks, crypto)
  data_financial_liability    Loans, mortgages, debt

ACTIVITY
  data_activity_app_usage     Desktop/mobile app usage sessions
  data_activity_web_browsing  Web browsing history

KNOWLEDGE
  data_knowledge_document        Saved documents and notes
  data_knowledge_ai_conversation Past AI chat history

OTHER
  data_speech_transcription   Voice/audio transcriptions
  data_device_battery         Battery level snapshots
  data_environment_pressure   Barometric pressure readings

================================================================================
WIKI TABLES (entity resolution + temporal context)
================================================================================

ENTITIES (resolved nouns in user's life)
  wiki_people       People with names, emails, relationship info
  wiki_places       Places with name, address, coordinates, visit stats
  wiki_orgs         Organizations with type, role, interaction history
  wiki_connections  Relationships between people/places/orgs

TEMPORAL (daily/yearly context)
  wiki_days         Day summaries with autobiography, context vector
  wiki_years        Year summaries with highlights, themes
  wiki_events       Timeline events within a day

REFERENCES
  wiki_citations    Links wiki content to source ontology records

================================================================================
NARRATIVE TABLES (life story structure)
================================================================================
  narrative_telos     User's life purpose/direction
  narrative_acts      Major life periods (multi-year)
  narrative_chapters  Chapters within acts (months/seasons)

================================================================================
QUERY TIPS
================================================================================
- Use 'get_schema' to see columns before writing queries
- Date filter: WHERE timestamp > datetime('now', '-7 days')
- Financial: amount/100.0 for dollars
- JOIN data tables to wiki_* for resolved names
- Always LIMIT results (max 200)"#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["operation"],
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["query", "list_tables", "get_schema"],
                    "description": "Operation to perform"
                },
                "sql": {
                    "type": "string",
                    "description": "SQL query (required for 'query' operation). SELECT only, read-only."
                },
                "tables": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Table name(s) to get schema for (required for 'get_schema' operation)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max rows to return (default 50, max 200)",
                    "default": 50,
                    "maximum": 200
                }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Data,
        icon: "ri:database-2-line".to_string(),
        display_order: 2,
    }
}

/// Code Interpreter tool - execute Python code in a sandbox
fn code_interpreter_tool() -> ToolConfig {
    ToolConfig {
        id: "code_interpreter".to_string(),
        name: "Python".to_string(),
        description: "Execute Python code for calculations and data analysis".to_string(),
        llm_description: r#"Execute Python code in a secure sandboxed environment.

Use this tool when you need to:
- Perform calculations, math, statistics, or numerical analysis
- Process, transform, or analyze data (CSV, JSON, etc.)
- Financial calculations (loans, mortgages, investments, IRR, NPV)
- Generate charts and visualizations
- Work with dates, times, or complex logic

Available packages:
- Python 3.12 standard library (math, statistics, datetime, json, csv, re, decimal, etc.)
- numpy - numerical computing, arrays, linear algebra
- numpy-financial - financial functions: pmt, fv, pv, irr, npv, nper, rate
- pandas - data analysis, DataFrames, CSV/JSON loading
- matplotlib - charts and visualizations (use plt.savefig('/tmp/chart.png'))
- scipy - scientific computing, statistics, optimization
- requests - HTTP client
- python-dateutil - date parsing
- pytz - timezones

The code runs in an isolated sandbox with:
- No filesystem access (except /tmp for temporary files)
- No network access
- 60 second timeout (max 120 seconds)

IMPORTANT: Use print() to output your results. The stdout will be returned to you.

Example - financial calculation (mortgage payment):
{
  "code": "import numpy_financial as npf\nloan = 400000\nrate = 0.065 / 12  # 6.5% annual -> monthly\nmonths = 30 * 12\npayment = npf.pmt(rate, months, -loan)\nprint(f'Monthly payment: ${payment:,.2f}')"
}

Example - data analysis with pandas:
{
  "code": "import pandas as pd\ndata = {'month': ['Jan', 'Feb', 'Mar'], 'sales': [100, 150, 120]}\ndf = pd.DataFrame(data)\nprint(f'Total: ${df.sales.sum()}')\nprint(f'Average: ${df.sales.mean():.2f}')\nprint(f'Best month: {df.loc[df.sales.idxmax(), \"month\"]}')"
}

Example - statistics with numpy:
{
  "code": "import numpy as np\ndata = [23, 45, 67, 32, 89, 54, 38]\nprint(f'Mean: {np.mean(data):.1f}')\nprint(f'Std Dev: {np.std(data):.1f}')\nprint(f'Correlation example: {np.corrcoef([1,2,3,4], [2,4,5,8])[0,1]:.3f}')"
}"#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["code"],
            "properties": {
                "code": {
                    "type": "string",
                    "description": "Python code to execute. Use print() to output results."
                },
                "timeout": {
                    "type": "integer",
                    "description": "Execution timeout in seconds (default: 60, max: 120)",
                    "default": 60,
                    "minimum": 5,
                    "maximum": 120
                }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Data,
        icon: "ri:code-s-slash-line".to_string(),
        display_order: 3,
    }
}

/// Create Page tool - creates a new page with optional initial content
fn create_page_tool() -> ToolConfig {
    ToolConfig {
        id: "create_page".to_string(),
        name: "Create Page".to_string(),
        description: "Create a new page with content".to_string(),
        llm_description: r#"Create a new page with a title and optional initial content.

Use this tool when:
- User asks you to create a new page, document, or note
- User wants to start a new document from scratch
- You need to save information to a new page

IMPORTANT: Write content directly - do NOT use CriticMarkup markers.
- Do NOT use {++ or ++} markers
- Do NOT use {-- or --} markers
- Just write plain text/markdown content
The content goes directly into the page without user review.

Example:
{
  "title": "Meeting Notes - January 29",
  "content": "Meeting Notes content here..."
}"#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["title"],
            "properties": {
                "title": {
                    "type": "string",
                    "description": "Title for the new page"
                },
                "content": {
                    "type": "string",
                    "description": "Initial content for the page (markdown supported). Applied directly without review."
                }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Edit,
        icon: "ri:file-add-line".to_string(),
        display_order: 4,
    }
}

/// Get Page Content tool - reads current content of a page
fn get_page_content_tool() -> ToolConfig {
    ToolConfig {
        id: "get_page_content".to_string(),
        name: "Get Page Content".to_string(),
        description: "Read the current content of a page".to_string(),
        llm_description: r#"Read the current content of a page before editing.

Use this tool when:
- You need to see what's currently in a page before making edits
- User asks about the contents of their document
- You need context about the page to make good edits

ALWAYS call this before using edit_page so you know what text to find.

IMPORTANT - Extracting page_id:
When user mentions a page using entity syntax like [Page Name](entity:page_abc123),
extract the ID from the link: page_abc123 (everything after "entity:").
You MUST pass this page_id parameter when the user references a specific page.

Returns the page title, content, and content length."#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["page_id"],
            "properties": {
                "page_id": {
                    "type": "string",
                    "description": "Page ID to read. Extract from entity links: [Name](entity:page_xxx) -> page_xxx"
                }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Edit,
        icon: "ri:file-text-line".to_string(),
        display_order: 5,
    }
}

/// Edit Page tool - applies edits using simple find/replace
fn edit_page_tool() -> ToolConfig {
    ToolConfig {
        id: "edit_page".to_string(),
        name: "Edit Page".to_string(),
        description: "Edit a page using find/replace".to_string(),
        llm_description: r#"Edit an existing page by finding text and replacing it.

Use this tool when:
- User asks you to modify, update, or change their document
- User says "help me with this", "can you improve", "fix this"
- You need to make changes to existing content

IMPORTANT: Call get_page_content FIRST to see the current document!

IMPORTANT - Extracting page_id:
When user mentions a page using entity syntax like [Page Name](entity:page_abc123),
extract the ID from the link: page_abc123 (everything after "entity:").
You MUST pass this page_id parameter when the user references a specific page.

How it works:
1. Provide 'page_id' - extracted from the entity link
2. Provide 'find' - the exact text to locate in the document
3. Provide 'replace' - the new text you want instead

Changes are automatically highlighted for the user to accept/reject.

Example - changing a word:
{
  "page_id": "page_abc123",
  "find": "The quick brown fox",
  "replace": "The fast brown fox"
}

Example - full document rewrite (find empty string):
{
  "page_id": "page_abc123",
  "find": "",
  "replace": "Entirely new document content here"
}

Tips:
- Use enough context in 'find' to uniquely identify the location
- Keep 'find' as short as possible while still being unique
- For large changes, prefer fewer comprehensive edits over many small ones"#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["page_id", "find", "replace"],
            "properties": {
                "page_id": {
                    "type": "string",
                    "description": "Page ID to edit. Extract from entity links: [Name](entity:page_xxx) -> page_xxx"
                },
                "find": {
                    "type": "string",
                    "description": "Text to find in the document. Use empty string for full document replacement."
                },
                "replace": {
                    "type": "string",
                    "description": "New text to replace the found text with"
                }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Edit,
        icon: "ri:edit-line".to_string(),
        display_order: 6,
    }
}

/// Get default enabled tools configuration (for assistant profile)
pub fn default_enabled_tools() -> serde_json::Value {
    serde_json::json!({
        "web_search": true,
        "sql_query": true,
        "code_interpreter": true,
        "create_page": true,
        "get_page_content": true,
        "edit_page": true
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_tools() {
        let tools = default_tools();
        assert_eq!(tools.len(), 6, "Should have 6 tools");

        // Verify all tools have required fields
        for tool in &tools {
            assert!(!tool.id.is_empty());
            assert!(!tool.name.is_empty());
            assert!(!tool.llm_description.is_empty(), "LLM description is required");
            assert!(tool.parameters.is_object(), "Parameters must be JSON object");
            assert_eq!(tool.tool_type, ToolType::Builtin, "Registry tools should be builtin type");
        }

        // Verify specific tools exist
        let ids: Vec<&str> = tools.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&"web_search"));
        assert!(ids.contains(&"sql_query"));
        assert!(ids.contains(&"code_interpreter"));
        assert!(ids.contains(&"create_page"));
        assert!(ids.contains(&"get_page_content"));
        assert!(ids.contains(&"edit_page"));
    }

    #[test]
    fn test_default_enabled_tools() {
        let enabled = default_enabled_tools();
        assert!(enabled.is_object());
        assert_eq!(enabled.get("web_search"), Some(&serde_json::json!(true)));
        assert_eq!(enabled.get("sql_query"), Some(&serde_json::json!(true)));
        assert_eq!(enabled.get("code_interpreter"), Some(&serde_json::json!(true)));
        assert_eq!(enabled.get("create_page"), Some(&serde_json::json!(true)));
        assert_eq!(enabled.get("get_page_content"), Some(&serde_json::json!(true)));
        assert_eq!(enabled.get("edit_page"), Some(&serde_json::json!(true)));
    }

    #[test]
    fn test_tool_parameters_have_type() {
        for tool in default_tools() {
            assert_eq!(
                tool.parameters.get("type"),
                Some(&serde_json::json!("object")),
                "Tool {} parameters should have type: object",
                tool.id
            );
        }
    }
}

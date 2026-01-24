//! MCP Tool definitions for Virtues
//!
//! This module defines the tools exposed to AI assistants via the MCP protocol.

use crate::mcp::schema;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::{Column, Row, SqlitePool, TypeInfo};

// ============================================================================
// Custom Deserializer: String or Vec<String>
// ============================================================================

/// Deserializer that accepts either a single string or an array of strings.
/// This makes tool parameters more forgiving when AI models pass "calendar" instead of ["calendar"].
mod string_or_vec {
    use serde::{self, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum StringOrVec {
            String(String),
            Vec(Vec<String>),
        }

        let opt = Option::<StringOrVec>::deserialize(deserializer)?;
        Ok(opt.map(|v| match v {
            StringOrVec::String(s) => vec![s],
            StringOrVec::Vec(v) => v,
        }))
    }
}

// ============================================================================
// Shared Utilities
// ============================================================================

/// Convert SQLite rows to JSON array with comprehensive type support
///
/// This function handles NULL values, integers, floats, decimals, booleans,
/// dates, timestamps, JSON, and falls back gracefully for unsupported types.
fn convert_rows_to_json(rows: &[sqlx::sqlite::SqliteRow]) -> Vec<serde_json::Value> {
    let mut json_rows = Vec::new();

    for row in rows {
        let mut obj = serde_json::Map::new();

        for (i, col) in row.columns().iter().enumerate() {
            let col_name = col.name();

            // Try to extract value based on type, with comprehensive type support
            let value: serde_json::Value =
                // Try NULL first - check if the column value is NULL
                if let Ok(opt_val) = row.try_get::<Option<String>, _>(i) {
                    if opt_val.is_none() {
                        serde_json::Value::Null
                    } else {
                        // We know it's not NULL, so unwrap the Option
                        serde_json::Value::String(opt_val.unwrap())
                    }
                }
                // String types (if NULL check didn't work, try direct string)
                else if let Ok(v) = row.try_get::<String, _>(i) {
                    serde_json::Value::String(v)
                }
                // Integer types
                else if let Ok(v) = row.try_get::<i64, _>(i) {
                    serde_json::Value::Number(v.into())
                } else if let Ok(v) = row.try_get::<i32, _>(i) {
                    serde_json::Value::Number(v.into())
                } else if let Ok(v) = row.try_get::<i16, _>(i) {
                    serde_json::Value::Number(v.into())
                }
                // Float types
                else if let Ok(v) = row.try_get::<f64, _>(i) {
                    serde_json::json!(v)
                } else if let Ok(v) = row.try_get::<f32, _>(i) {
                    serde_json::json!(v)
                }
                // Boolean (SQLite stores as INTEGER 0/1)
                else if let Ok(v) = row.try_get::<bool, _>(i) {
                    serde_json::Value::Bool(v)
                }
                // Date/Time types (convert to string)
                else if let Ok(v) = row.try_get::<chrono::NaiveDate, _>(i) {
                    serde_json::Value::String(v.format("%Y-%m-%d").to_string())
                } else if let Ok(v) = row.try_get::<chrono::NaiveDateTime, _>(i) {
                    serde_json::Value::String(v.format("%Y-%m-%d %H:%M:%S").to_string())
                } else if let Ok(v) = row.try_get::<chrono::DateTime<chrono::Utc>, _>(i) {
                    serde_json::Value::String(v.to_rfc3339())
                } else if let Ok(v) = row.try_get::<chrono::DateTime<chrono::FixedOffset>, _>(i) {
                    serde_json::Value::String(v.to_rfc3339())
                } else if let Ok(v) = row.try_get::<chrono::DateTime<chrono::Local>, _>(i) {
                    serde_json::Value::String(v.to_rfc3339())
                }
                // JSONB/JSON
                else if let Ok(v) = row.try_get::<serde_json::Value, _>(i) {
                    v
                }
                // UUID
                else if let Ok(v) = row.try_get::<uuid::Uuid, _>(i) {
                    serde_json::Value::String(v.to_string())
                }
                // Fallback: try to get raw value info for better error message
                else {
                    let type_info = col.type_info().name();
                    tracing::warn!(
                        "Column '{}' with SQLite type '{}' could not be extracted - falling back to placeholder",
                        col_name,
                        type_info
                    );
                    serde_json::Value::String(format!("<unsupported type: {}>", type_info))
                };

            obj.insert(col_name.to_string(), value);
        }

        json_rows.push(serde_json::Value::Object(obj));
    }

    json_rows
}

// ============================================================================
// Query Ontology Tool (Unified)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct QueryOntologyRequest {
    /// Operation to perform: "query" (execute SQL), "list_tables" (discover tables), or "get_schema" (get table schema)
    pub operation: String,

    // Query operation parameters
    /// SQL query to execute (required for "query" operation, SELECT only, read-only)
    #[serde(default)]
    pub query: Option<String>,
    /// Optional limit on number of rows returned (for "query" operation, default: 100, max: 1000)
    #[serde(default)]
    pub limit: Option<u32>,

    // Schema operation parameters
    /// Table name (required for "get_schema" operation)
    #[serde(default)]
    pub table_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct QueryOntologyResponse {
    /// Operation that was performed
    pub operation: String,

    // Query operation fields
    /// Query results as JSON array (for "query" operation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<serde_json::Value>,
    /// Number of rows returned (for "query" operation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_count: Option<usize>,
    /// Whether more results exist beyond the returned rows (for "query" operation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
    /// The limit that was applied to the query (for "query" operation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_applied: Option<u32>,

    // List tables operation fields
    /// List of tables (for "list_tables" operation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tables: Option<Vec<OntologyTableInfo>>,

    // Get schema operation fields
    /// Table schema (for "get_schema" operation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<schema::TableSchema>,
}

/// Unified ontology tool - routes to specific operations
pub async fn query_ontology(
    pool: &SqlitePool,
    request: QueryOntologyRequest,
) -> Result<QueryOntologyResponse, String> {
    match request.operation.as_str() {
        "query" => {
            let query = request
                .query
                .ok_or("Missing 'query' parameter for query operation")?;
            let result = query_ontology_sql(pool, query, request.limit).await?;
            Ok(QueryOntologyResponse {
                operation: "query".to_string(),
                rows: Some(result.rows),
                row_count: Some(result.row_count),
                has_more: Some(result.has_more),
                limit_applied: Some(result.limit_applied),
                tables: None,
                schema: None,
            })
        }
        "list_tables" => {
            let tables = list_ontology_tables_internal(pool).await?;
            Ok(QueryOntologyResponse {
                operation: "list_tables".to_string(),
                rows: None,
                row_count: None,
                has_more: None,
                limit_applied: None,
                tables: Some(tables),
                schema: None,
            })
        }
        "get_schema" => {
            let table_name = request
                .table_name
                .ok_or("Missing 'table_name' parameter for get_schema operation")?;
            let schema = get_table_schema_internal(pool, &table_name).await?;
            Ok(QueryOntologyResponse {
                operation: "get_schema".to_string(),
                rows: None,
                row_count: None,
                has_more: None,
                limit_applied: None,
                tables: None,
                schema: Some(schema),
            })
        }
        _ => Err(format!(
            "Invalid operation '{}'. Must be one of: query, list_tables, get_schema",
            request.operation
        )),
    }
}

// Internal struct for query operation results
struct QueryOntologyInternalResponse {
    rows: serde_json::Value,
    row_count: usize,
    has_more: bool,
    limit_applied: u32,
}

/// Internal: Execute a read-only SQL query against ontology tables
async fn query_ontology_sql(
    pool: &SqlitePool,
    query: String,
    limit: Option<u32>,
) -> Result<QueryOntologyInternalResponse, String> {
    // Security: Validate query length to prevent DOS
    if query.len() > 10000 {
        return Err("Query exceeds maximum length of 10KB".to_string());
    }

    // Security: Validate that query is read-only
    let query_lower = query.trim().to_lowercase();

    if !query_lower.starts_with("select") && !query_lower.starts_with("with") {
        return Err("Only SELECT queries are allowed".to_string());
    }

    // Check for dangerous keywords
    let dangerous_keywords = [
        "insert", "update", "delete", "drop", "create", "alter", "truncate",
    ];
    for keyword in &dangerous_keywords {
        if query_lower.contains(keyword) {
            return Err(format!("Query contains forbidden keyword: {}", keyword));
        }
    }

    // Apply limit (fetch one extra row to detect if more results exist)
    let limit = limit.unwrap_or(100).min(1000);
    let fetch_limit = limit + 1; // Fetch one extra to detect has_more

    let query_with_limit = if query_lower.contains("limit") {
        // User provided their own LIMIT, use as-is
        query.clone()
    } else {
        format!("{} LIMIT {}", query, fetch_limit)
    };

    // Execute query in a read-only transaction
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    // Set transaction to read-only
    sqlx::query("SET TRANSACTION READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

    // Execute the query
    let rows = sqlx::query(&query_with_limit)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| format!("Query execution failed: {}", e))?;

    // Convert rows to JSON using shared utility function
    let json_rows = convert_rows_to_json(&rows);

    // Check if there are more results
    let has_more = json_rows.len() > limit as usize;
    let final_rows: Vec<_> = json_rows.into_iter().take(limit as usize).collect();

    tx.commit().await.map_err(|e| e.to_string())?;

    Ok(QueryOntologyInternalResponse {
        row_count: final_rows.len(),
        rows: serde_json::Value::Array(final_rows),
        has_more,
        limit_applied: limit,
    })
}

// ============================================================================
// List Sources Tool
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListSourcesRequest {
    // Empty struct - this tool takes no parameters
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListSourcesResponse {
    pub sources: Vec<SourceInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SourceInfo {
    pub id: String,
    pub name: String,
    pub source_type: String,
    pub status: String,
    pub enabled_streams: i32,
    pub created_at: crate::types::Timestamp,
    pub last_sync: Option<crate::types::Timestamp>,
}

/// List all data sources and their status
pub async fn list_sources(pool: &SqlitePool) -> Result<ListSourcesResponse, String> {
    let rows = sqlx::query(
        r#"
        SELECT
            s.id::text as id,
            s.name,
            s.source as source_type,
            CASE
                WHEN NOT s.is_active THEN 'inactive'
                WHEN s.pairing_status = 'pending' THEN 'pending'
                WHEN s.pairing_status = 'revoked' THEN 'revoked'
                WHEN s.error_message IS NOT NULL THEN 'error'
                ELSE 'active'
            END as status,
            COUNT(DISTINCT st.stream_name) FILTER (WHERE st.is_enabled = true)::int as enabled_streams,
            s.created_at::text as created_at,
            MAX(j.completed_at)::text as last_sync
        FROM elt_source_connections s
        LEFT JOIN elt_stream_connections st ON st.source_connection_id = s.id
        LEFT JOIN elt_jobs j ON j.source_connection_id = s.id AND j.status = 'succeeded'
        GROUP BY s.id, s.name, s.source, s.is_active, s.pairing_status, s.error_message, s.created_at
        ORDER BY s.created_at DESC
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut sources = Vec::new();
    for row in rows {
        sources.push(SourceInfo {
            id: row.get("id"),
            name: row.get("name"),
            source_type: row.get("source_type"),
            status: row.get("status"),
            enabled_streams: row.get("enabled_streams"),
            created_at: row.get("created_at"),
            last_sync: row.get("last_sync"),
        });
    }

    Ok(ListSourcesResponse { sources })
}

// ============================================================================
// Internal Helper Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OntologyTableInfo {
    pub table_name: String,
    pub columns: Vec<String>,
    pub row_count: Option<i64>,
}

/// Internal: List all available ontology tables with their schemas
async fn list_ontology_tables_internal(
    pool: &SqlitePool,
) -> Result<Vec<OntologyTableInfo>, String> {
    let table_names = schema::list_ontology_tables(pool)
        .await
        .map_err(|e| e.to_string())?;

    let mut tables = Vec::new();

    for table_name in table_names {
        // Get schema
        let table_schema = schema::get_table_schema(pool, &table_name)
            .await
            .map_err(|e| e.to_string())?;

        // Get row count
        let row_count: Option<i64> =
            sqlx::query_scalar(&format!("SELECT COUNT(*) FROM data_{}", table_name))
                .fetch_optional(pool)
                .await
                .ok()
                .flatten();

        tables.push(OntologyTableInfo {
            table_name,
            columns: table_schema
                .columns
                .iter()
                .map(|c| c.name.clone())
                .collect(),
            row_count,
        });
    }

    Ok(tables)
}

/// Internal: Get detailed schema for a specific ontology table
async fn get_table_schema_internal(
    pool: &SqlitePool,
    table_name: &str,
) -> Result<schema::TableSchema, String> {
    schema::get_table_schema(pool, table_name)
        .await
        .map_err(|e| e.to_string())
}

// ============================================================================
// Semantic Search Tool
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SemanticSearchRequest {
    /// Natural language search query (e.g., "emails about project deadline", "messages about dinner plans")
    pub query: String,

    /// Content types to search (optional). If not specified, searches all types.
    /// Options: "email", "message", "calendar", "ai_conversation", "document"
    /// Accepts either a single string ("calendar") or an array (["calendar", "email"])
    #[serde(default, deserialize_with = "string_or_vec::deserialize")]
    pub content_types: Option<Vec<String>>,

    /// Maximum results to return (default: 10, max: 50)
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SemanticSearchResponse {
    /// Search results ordered by relevance
    pub results: Vec<SemanticSearchResult>,

    /// Original query
    pub query: String,

    /// Number of results returned
    pub total_results: usize,

    /// Search execution time in milliseconds
    pub search_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SemanticSearchResult {
    /// Content type (email, message, calendar, ai_conversation)
    pub content_type: String,

    /// Record ID
    pub id: String,

    /// Title (for emails and calendar events)
    pub title: Option<String>,

    /// Content preview (first ~200 characters)
    pub preview: String,

    /// Author/sender name
    pub author: Option<String>,

    /// Timestamp
    pub timestamp: String,

    /// Similarity score (0.0-1.0, higher is more relevant)
    pub similarity: f32,
}

/// Perform semantic search across embedded content (emails, messages, calendar, AI conversations)
pub async fn semantic_search(
    pool: &SqlitePool,
    request: SemanticSearchRequest,
) -> Result<SemanticSearchResponse, String> {
    use crate::api::search;

    // Map to API request
    let api_request = search::SemanticSearchRequest {
        query: request.query.clone(),
        limit: request.limit.unwrap_or(10).min(50),
        content_types: request.content_types,
        min_similarity: 0.3,
    };

    // Execute search
    let result = search::semantic_search(pool, api_request)
        .await
        .map_err(|e| e.to_string())?;

    // Map results to tool response
    let results: Vec<SemanticSearchResult> = result
        .results
        .into_iter()
        .map(|r| SemanticSearchResult {
            content_type: r.content_type,
            id: r.id.to_string(),
            title: r.title,
            preview: r.preview,
            author: r.author,
            timestamp: r.timestamp.to_rfc3339(),
            similarity: r.similarity,
        })
        .collect();

    Ok(SemanticSearchResponse {
        total_results: results.len(),
        results,
        query: result.query,
        search_time_ms: result.search_time_ms,
    })
}

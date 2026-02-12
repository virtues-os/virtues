//! Semantic search tool
//!
//! Wraps the search engine into a tool callable by the agent.

use sqlx::SqlitePool;
use std::sync::Arc;

use super::executor::{ToolError, ToolResult};
use crate::search::SemanticSearchEngine;

/// Semantic search tool executor
#[derive(Clone)]
pub struct SemanticSearchTool {
    engine: Arc<SemanticSearchEngine>,
}

impl SemanticSearchTool {
    pub fn new(pool: Arc<SqlitePool>) -> Self {
        Self {
            engine: Arc::new(SemanticSearchEngine::new(pool)),
        }
    }

    /// Ensure the vec_search table exists (call at startup)
    pub async fn ensure_ready(&self) -> Result<(), ToolError> {
        self.engine
            .ensure_vec_table()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to init vec table: {}", e)))
    }

    pub async fn execute(&self, arguments: serde_json::Value) -> Result<ToolResult, ToolError> {
        let query = arguments
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("query is required".into()))?;

        let domains: Option<Vec<String>> = arguments
            .get("domains")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect());

        let date_after = arguments.get("date_after").and_then(|v| v.as_str());
        let date_before = arguments.get("date_before").and_then(|v| v.as_str());
        let num_results = arguments
            .get("num_results")
            .and_then(|v| v.as_i64());

        let results = self
            .engine
            .search(
                query,
                domains.as_deref(),
                date_after,
                date_before,
                num_results,
            )
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Semantic search failed: {}", e)))?;

        let result_json: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "ontology": r.ontology,
                    "record_id": r.record_id,
                    "score": format!("{:.3}", r.score),
                    "title": r.title,
                    "preview": r.preview,
                    "author": r.author,
                    "timestamp": r.timestamp,
                })
            })
            .collect();

        Ok(ToolResult::success(serde_json::json!({
            "results": result_json,
            "count": results.len(),
            "tip": "Use sql_query with record IDs to get full details for specific results."
        })))
    }
}

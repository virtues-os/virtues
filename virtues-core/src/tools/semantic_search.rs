//! Semantic search tool
//!
//! Wraps the search engine into a tool callable by the agent.

use sqlx::PgPool;
use std::sync::Arc;

use super::executor::{ToolError, ToolResult};
use crate::search::SemanticSearchEngine;

/// Semantic search tool executor
#[derive(Clone)]
pub struct SemanticSearchTool {
    engine: Arc<SemanticSearchEngine>,
}

impl SemanticSearchTool {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self {
            engine: Arc::new(SemanticSearchEngine::new(pool)),
        }
    }

    /// Probe that the pgvector search_vectors table is reachable (call at startup).
    pub async fn ensure_ready(&self) -> Result<(), ToolError> {
        self.engine
            .ensure_vec_table()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to init vec table: {}", e)))
    }

    pub async fn execute(
        &self,
        arguments: serde_json::Value,
        notebook_id: Option<&str>,
    ) -> Result<ToolResult, ToolError> {
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
        // Resolved entity IDs (person/place/org/thing) to scope the search to.
        let entities: Option<Vec<String>> = arguments
            .get("entities")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect());
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
                entities.as_deref(),
                notebook_id,
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
                    // Viewable route for this source, when it resolves to a
                    // life-graph entity. Cite it inline (see the tool-usage
                    // prompt); absent when the record has no navigable source.
                    "ref": r.ref_url,
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

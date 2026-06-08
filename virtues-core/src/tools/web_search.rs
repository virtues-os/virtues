//! Web Search tool implementation (Exa AI)
//!
//! Provides web search capabilities using Exa AI.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use super::executor::{ToolError, ToolResult};
use crate::api::exa;

/// Web search tool arguments (from LLM)
#[derive(Debug, Deserialize)]
pub struct WebSearchArgs {
    /// Search query
    pub query: String,
    /// Number of results (1-10, default 5)
    #[serde(default)]
    pub num_results: Option<u8>,
    /// Search type: auto, keyword, neural
    #[serde(default)]
    pub search_type: Option<String>,
}

/// Web search result for LLM
#[derive(Debug, Serialize)]
pub struct WebSearchResult {
    pub title: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_date: Option<String>,
}

/// Web search tool
#[derive(Clone)]
pub struct WebSearchTool {
    pool: PgPool,
}

impl WebSearchTool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Execute web search
    pub async fn execute(&self, arguments: serde_json::Value) -> Result<ToolResult, ToolError> {
        // Parse arguments
        let args: WebSearchArgs = serde_json::from_value(arguments)
            .map_err(|e| ToolError::InvalidParameters(format!("Invalid arguments: {}", e)))?;

        // Validate query
        if args.query.trim().is_empty() {
            return Err(ToolError::InvalidParameters(
                "Search query cannot be empty".to_string(),
            ));
        }

        // Map search type
        let search_type = match args.search_type.as_deref() {
            Some("keyword") => Some(exa::SearchType::Keyword),
            Some("neural") => Some(exa::SearchType::Neural),
            _ => Some(exa::SearchType::Auto),
        };

        // Build request
        let request = exa::SearchRequest {
            query: args.query,
            num_results: args.num_results,
            search_type,
            category: None,
            include_domains: None,
            exclude_domains: None,
            start_published_date: None,
            end_published_date: None,
        };

        // Execute search via the shared Exa client (bearer-auth + charge).
        let response = exa::search(&self.pool, request)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        // Convert to tool result format
        let results: Vec<WebSearchResult> = response
            .results
            .into_iter()
            .map(|r| WebSearchResult {
                title: r.title,
                url: r.url,
                summary: r.summary,
                text: r.text,
                published_date: r.published_date,
            })
            .collect();

        Ok(ToolResult::success(serde_json::json!({
            "query": response.query,
            "results_count": results.len(),
            "results": results,
        })))
    }

}

impl std::fmt::Debug for WebSearchTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebSearchTool").finish()
    }
}

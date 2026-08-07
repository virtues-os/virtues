//! Web Search tool implementation (Parallel).
//!
//! Answers "find me pages about X". Distinct from `fetch` (`virtues-core/src/fetch`),
//! which answers "read the one page I already have" — different jobs, no shared
//! code.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use super::executor::{ToolError, ToolResult};
use crate::api::parallel;

/// Web search tool arguments (from LLM)
#[derive(Debug, Deserialize)]
pub struct WebSearchArgs {
    /// Search query
    pub query: String,
    /// What the caller is actually trying to learn. Optional, and the reason
    /// this API beats a bare keyword search — it disambiguates a short query.
    #[serde(default)]
    pub objective: Option<String>,
    /// Number of results (1-10, default 5)
    #[serde(default)]
    pub num_results: Option<u8>,
    /// Escalate to a comprehensive multi-step search for hard or thin-result
    /// queries. Higher cost/latency — off by default.
    #[serde(default)]
    pub deep: Option<bool>,
    /// Freshness: max age (hours) of a cached result before re-fetching live.
    /// Use `1` for news/sports/odds/live data; omit for stable information.
    #[serde(default)]
    pub max_age_hours: Option<u32>,
}

/// Web search result for LLM
#[derive(Debug, Serialize)]
pub struct WebSearchResult {
    pub title: String,
    pub url: String,
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
        let args: WebSearchArgs = serde_json::from_value(arguments)
            .map_err(|e| ToolError::InvalidParameters(format!("Invalid arguments: {}", e)))?;

        if args.query.trim().is_empty() {
            return Err(ToolError::InvalidParameters(
                "Search query cannot be empty".to_string(),
            ));
        }

        let request = parallel::SearchRequest {
            objective: args.objective,
            queries: vec![args.query.clone()],
            mode: if args.deep.unwrap_or(false) {
                parallel::Mode::Advanced
            } else {
                parallel::Mode::Basic
            },
            max_results: args.num_results,
            // The tool speaks hours because that is the unit a model reasons
            // in; the API wants seconds.
            max_age_seconds: args.max_age_hours.map(|h| h.saturating_mul(3600)),
        };

        let response = parallel::search(&self.pool, request)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let results: Vec<WebSearchResult> = response
            .results
            .into_iter()
            .map(|r| WebSearchResult {
                // A result with no title is still a usable answer; its host is
                // the most honest stand-in.
                title: r.title.unwrap_or_else(|| {
                    url::Url::parse(&r.url)
                        .ok()
                        .and_then(|u| u.host_str().map(|h| h.to_string()))
                        .unwrap_or_else(|| r.url.clone())
                }),
                url: r.url,
                text: r.text,
                published_date: r.published_date,
            })
            .collect();

        Ok(ToolResult::success(serde_json::json!({
            "query": args.query,
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

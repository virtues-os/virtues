//! Web Search tool implementation (Exa AI)
//!
//! Provides web search capabilities using Exa AI.

use serde::{Deserialize, Serialize};

use super::executor::{ToolError, ToolResult};
use crate::api::exa;
use crate::tollbooth;

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
    tollbooth_url: String,
    tollbooth_secret: String,
}

impl WebSearchTool {
    pub fn new(tollbooth_url: String, tollbooth_secret: String) -> Self {
        Self {
            tollbooth_url,
            tollbooth_secret,
        }
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

        // Execute search via Exa API
        let response = self.execute_search(request).await?;

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

    /// Execute the actual search via Tollbooth/Exa
    async fn execute_search(&self, request: exa::SearchRequest) -> Result<exa::SearchResponse, ToolError> {
        let num_results = request.num_results.unwrap_or(5).clamp(1, 10);
        let search_type = request.search_type.unwrap_or_default();
        let search_type_str = match search_type {
            exa::SearchType::Auto => "auto",
            exa::SearchType::Keyword => "keyword",
            exa::SearchType::Neural => "neural",
        };

        // Build Exa request structure
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ExaSearchRequest {
            query: String,
            num_results: u8,
            #[serde(rename = "type")]
            search_type: String,
            contents: ExaContents,
        }

        #[derive(serde::Serialize)]
        struct ExaContents {
            text: ExaTextOptions,
            summary: bool,
        }

        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ExaTextOptions {
            max_characters: u32,
            include_html_tags: bool,
        }

        let exa_request = ExaSearchRequest {
            query: request.query.clone(),
            num_results,
            search_type: search_type_str.to_string(),
            contents: ExaContents {
                text: ExaTextOptions {
                    max_characters: 1000,
                    include_html_tags: false,
                },
                summary: true,
            },
        };

        // Execute request
        let client = reqwest::Client::new();
        let response = tollbooth::with_system_auth(
            client.post(format!("{}/v1/services/exa/search", self.tollbooth_url)),
            &self.tollbooth_secret,
        )
        .header("Content-Type", "application/json")
        .json(&exa_request)
        .send()
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("Exa API request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ToolError::ExecutionFailed(format!(
                "Exa API error ({}): {}",
                status, error_text
            )));
        }

        // Parse response
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ExaApiResponse {
            results: Vec<ExaResult>,
            #[serde(default)]
            autoprompt_string: Option<String>,
            #[serde(default)]
            request_id: Option<String>,
        }

        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ExaResult {
            title: String,
            url: String,
            #[serde(default)]
            published_date: Option<String>,
            #[serde(default)]
            author: Option<String>,
            #[serde(default)]
            summary: Option<String>,
            #[serde(default)]
            text: Option<String>,
            #[serde(default)]
            score: Option<f64>,
        }

        let exa_response: ExaApiResponse = response
            .json()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to parse Exa response: {}", e)))?;

        let results: Vec<exa::SearchResult> = exa_response
            .results
            .into_iter()
            .map(|r| exa::SearchResult {
                title: r.title,
                url: r.url,
                published_date: r.published_date,
                author: r.author,
                summary: r.summary,
                text: r.text.map(|t| t.chars().take(500).collect()),
                score: r.score,
            })
            .collect();

        Ok(exa::SearchResponse {
            success: true,
            query: request.query,
            results_count: results.len(),
            search_type: search_type_str.to_string(),
            results,
            autoprompt_string: exa_response.autoprompt_string,
            request_id: exa_response.request_id,
        })
    }
}

impl std::fmt::Debug for WebSearchTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebSearchTool")
            .field("tollbooth_url", &self.tollbooth_url)
            .finish()
    }
}

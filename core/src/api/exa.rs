//! Exa AI Web Search API
//!
//! Provides web search capabilities using Exa AI's search API.
//! Exa specializes in AI-optimized web search with neural and keyword search.
//!
//! Requests are proxied through Tollbooth for budget enforcement.
//! @see https://docs.exa.ai for API documentation

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::tollbooth;

/// Get Tollbooth URL from environment (defaults to localhost:9002 for development)
fn get_tollbooth_url() -> String {
    std::env::var("TOLLBOOTH_URL").unwrap_or_else(|_| "http://localhost:9002".to_string())
}

/// Get Tollbooth internal secret from environment
fn get_tollbooth_secret() -> Result<String> {
    std::env::var("TOLLBOOTH_INTERNAL_SECRET").map_err(|_| {
        Error::Configuration("TOLLBOOTH_INTERNAL_SECRET environment variable not set".into())
    })
}

/// Search type for Exa queries
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchType {
    #[default]
    Auto,
    Keyword,
    Neural,
}

/// Category filter for search results
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Company,
    #[serde(rename = "research paper")]
    ResearchPaper,
    News,
    Pdf,
    Github,
    #[serde(rename = "personal site")]
    PersonalSite,
    #[serde(rename = "linkedin profile")]
    LinkedinProfile,
    #[serde(rename = "financial report")]
    FinancialReport,
}

/// Search request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    /// The search query
    pub query: String,

    /// Number of results (1-10, default 5)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_results: Option<u8>,

    /// Search type
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub search_type: Option<SearchType>,

    /// Category filter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<Category>,

    /// Only include results from these domains
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_domains: Option<Vec<String>>,

    /// Exclude results from these domains
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_domains: Option<Vec<String>>,

    /// Filter results published after this date (ISO 8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_published_date: Option<String>,

    /// Filter results published before this date (ISO 8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_published_date: Option<String>,
}

/// A single search result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub published_date: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub score: Option<f64>,
}

/// Search response from Exa
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub success: bool,
    pub query: String,
    pub results_count: usize,
    pub search_type: String,
    pub results: Vec<SearchResult>,
    #[serde(default)]
    pub autoprompt_string: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
}

// Internal Exa API structures
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExaSearchRequest {
    query: String,
    num_results: u8,
    #[serde(rename = "type")]
    search_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    include_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exclude_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_published_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_published_date: Option<String>,
    contents: ExaContents,
}

#[derive(Serialize)]
struct ExaContents {
    text: ExaTextOptions,
    summary: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExaTextOptions {
    max_characters: u32,
    include_html_tags: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExaApiResponse {
    results: Vec<ExaResult>,
    #[serde(default)]
    autoprompt_string: Option<String>,
    #[serde(default)]
    request_id: Option<String>,
}

#[derive(Deserialize)]
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

/// Perform a web search using Exa AI (proxied through Tollbooth)
pub async fn search(request: SearchRequest) -> Result<SearchResponse> {
    let secret = get_tollbooth_secret()?;

    if request.query.trim().is_empty() {
        return Err(Error::InvalidInput("Search query cannot be empty".into()));
    }

    let num_results = request.num_results.unwrap_or(5).clamp(1, 10);
    let search_type = request.search_type.unwrap_or_default();
    let search_type_str = match search_type {
        SearchType::Auto => "auto",
        SearchType::Keyword => "keyword",
        SearchType::Neural => "neural",
    };

    let exa_request = ExaSearchRequest {
        query: request.query.clone(),
        num_results,
        search_type: search_type_str.to_string(),
        category: request.category.map(|c| {
            match c {
                Category::Company => "company",
                Category::ResearchPaper => "research paper",
                Category::News => "news",
                Category::Pdf => "pdf",
                Category::Github => "github",
                Category::PersonalSite => "personal site",
                Category::LinkedinProfile => "linkedin profile",
                Category::FinancialReport => "financial report",
            }
            .to_string()
        }),
        include_domains: request.include_domains,
        exclude_domains: request.exclude_domains,
        start_published_date: request.start_published_date,
        end_published_date: request.end_published_date,
        contents: ExaContents {
            text: ExaTextOptions {
                max_characters: 1000,
                include_html_tags: false,
            },
            summary: true,
        },
    };

    let tollbooth_url = get_tollbooth_url();

    let client = reqwest::Client::new();
    let response = tollbooth::with_system_auth(
        client.post(format!("{}/v1/services/exa/search", tollbooth_url)),
        &secret,
    )
    .header("Content-Type", "application/json")
    .json(&exa_request)
    .send()
    .await
    .map_err(|e| Error::ExternalApi(format!("Tollbooth/Exa API request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(Error::ExternalApi(format!(
            "Tollbooth/Exa API error ({}): {}",
            status, error_text
        )));
    }

    let exa_response: ExaApiResponse = response
        .json()
        .await
        .map_err(|e| Error::ExternalApi(format!("Failed to parse Exa response: {}", e)))?;

    let results: Vec<SearchResult> = exa_response
        .results
        .into_iter()
        .map(|r| SearchResult {
            title: r.title,
            url: r.url,
            published_date: r.published_date,
            author: r.author,
            summary: r.summary,
            text: r.text.map(|t| t.chars().take(500).collect()), // Truncate to 500 chars
            score: r.score,
        })
        .collect();

    Ok(SearchResponse {
        success: true,
        query: request.query,
        results_count: results.len(),
        search_type: search_type_str.to_string(),
        results,
        autoprompt_string: exa_response.autoprompt_string,
        request_id: exa_response.request_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_type_serialization() {
        let auto = SearchType::Auto;
        let json = serde_json::to_string(&auto).unwrap();
        assert_eq!(json, "\"auto\"");
    }

    #[test]
    fn test_search_request_serialization() {
        let request = SearchRequest {
            query: "test query".to_string(),
            num_results: Some(5),
            search_type: Some(SearchType::Neural),
            category: None,
            include_domains: None,
            exclude_domains: None,
            start_published_date: None,
            end_published_date: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("test query"));
        assert!(json.contains("neural"));
    }
}

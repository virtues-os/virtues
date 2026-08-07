//! Parallel Search API — the web-search upstream.
//!
//! Replaces Exa. The motivation is vendor consolidation, not capability: one
//! fewer account, key, and bill. Costs land in the same place (~$0.005 a
//! search either way) and the integration shape is unchanged — a POST proxied
//! through virtues-api for budget enforcement, exactly as Exa was.
//!
//! Worth recording, because it is the thing people assume: Parallel is also
//! offered as a Vercel AI Gateway tool (`gateway.tools.parallelSearch`). That
//! form does NOT fit here. It is an *inference-time* tool the gateway executes
//! during a model call, and core runs its own tool loop (`tools/executor.rs`)
//! and returns its own `ToolResult`. So this is a direct API call, and the
//! "centralized billing through the gateway" pitch does not apply to us.
//!
//! Shape differences from Exa that the caller has to know about:
//!
//! - Parallel takes an `objective` (what you are actually trying to learn)
//!   alongside `search_queries`. That is a real gain — the tool can pass
//!   intent, not just keywords.
//! - Results carry `excerpts`, not a summary and a relevance score. Exa's
//!   `summary`/`score`/`author` have no equivalent and are simply gone rather
//!   than faked.
//! - `type: keyword|neural|auto` has no equivalent either; retrieval strategy
//!   is Parallel's business. `mode` replaces it as a cost/depth dial.
//!
//! @see https://docs.parallel.ai/api-reference/search/search

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::{Error, Result};
use crate::virtues_api::client::BearerClient;

/// How hard to look. The cost/latency dial, and the closest thing to Exa's
/// `deep` escalation.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Cheapest and fastest.
    Turbo,
    #[default]
    Basic,
    /// Multi-step. The escalation tier for hard or thin-result queries.
    Advanced,
}

#[derive(Debug, Clone)]
pub struct SearchRequest {
    /// What the caller is trying to learn, in natural language. Optional, but
    /// the reason to prefer this API — it disambiguates a bare query.
    pub objective: Option<String>,
    /// At least one. Parallel's guidance is 2–3 for a hard question.
    pub queries: Vec<String>,
    pub mode: Mode,
    pub max_results: Option<u8>,
    /// Freshness: refuse a cached page older than this. Carries Exa's
    /// `max_age_hours` across — the one argument that looked at risk in the
    /// swap and turned out to have a home.
    pub max_age_seconds: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
struct WireRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    objective: Option<String>,
    search_queries: Vec<String>,
    mode: Mode,
    #[serde(skip_serializing_if = "Option::is_none")]
    advanced_settings: Option<AdvancedSettings>,
}

#[derive(Debug, Clone, Serialize)]
struct AdvancedSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    max_results: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fetch_policy: Option<FetchPolicy>,
}

#[derive(Debug, Clone, Serialize)]
struct FetchPolicy {
    max_age_seconds: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct WireResponse {
    #[serde(default)]
    results: Vec<WireResult>,
}

#[derive(Debug, Clone, Deserialize)]
struct WireResult {
    url: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    publish_date: Option<String>,
    #[serde(default)]
    excerpts: Vec<String>,
}

/// One result, in the shape the tool layer hands to the model.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub title: Option<String>,
    pub url: String,
    pub published_date: Option<String>,
    /// The passages Parallel judged relevant, joined. This is the payload —
    /// there is no separate summary.
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
}

/// Cap on how much of one result's excerpts we hand the model.
///
/// Parallel already returns excerpts rather than whole pages, so this is a
/// backstop against one verbose result crowding out the others in the context
/// window, not a content decision.
const MAX_TEXT_CHARS: usize = 1_200;

/// Run a search, proxied through virtues-api for budget enforcement.
pub async fn search(pool: &PgPool, request: SearchRequest) -> Result<SearchResponse> {
    let queries: Vec<String> = request
        .queries
        .into_iter()
        .map(|q| q.trim().to_string())
        .filter(|q| !q.is_empty())
        // Parallel caps queries at 5 and 200 chars each; trim rather than let
        // the API reject the whole call.
        .map(|q| q.chars().take(200).collect())
        .take(5)
        .collect();
    if queries.is_empty() {
        return Err(Error::InvalidInput("search needs a query".to_string()));
    }

    let advanced_settings = if request.max_results.is_some() || request.max_age_seconds.is_some() {
        Some(AdvancedSettings {
            max_results: request.max_results,
            fetch_policy: request.max_age_seconds.map(|s| FetchPolicy {
                max_age_seconds: s,
            }),
        })
    } else {
        None
    };

    let body = serde_json::to_value(WireRequest {
        objective: request
            .objective
            .map(|o| o.trim().to_string())
            .filter(|o| !o.is_empty()),
        search_queries: queries,
        mode: request.mode,
        advanced_settings,
    })
    .map_err(|e| Error::ExternalApi(format!("failed to serialize Parallel request: {e}")))?;

    let response = BearerClient::from_env(pool.clone())
        .post_json("/v1/parallel/search", &body)
        .await
        .map_err(|e| Error::ExternalApi(format!("virtues-api/Parallel request failed: {e}")))?;

    if !response.is_success() {
        return Err(Error::ExternalApi(format!(
            "virtues-api/Parallel error ({}): {}",
            response.status, response.body
        )));
    }

    let parsed: WireResponse = serde_json::from_value(response.body)
        .map_err(|e| Error::ExternalApi(format!("failed to parse Parallel response: {e}")))?;

    Ok(SearchResponse {
        results: parsed
            .results
            .into_iter()
            .map(|r| SearchResult {
                title: r.title,
                url: r.url,
                published_date: r.publish_date,
                text: {
                    let joined = r.excerpts.join("\n\n");
                    let trimmed = joined.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.chars().take(MAX_TEXT_CHARS).collect())
                    }
                },
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(req: WireRequest) -> serde_json::Value {
        serde_json::to_value(req).unwrap()
    }

    #[test]
    fn mode_serializes_as_parallel_spells_it() {
        assert_eq!(serde_json::to_value(Mode::Advanced).unwrap(), "advanced");
        assert_eq!(serde_json::to_value(Mode::Basic).unwrap(), "basic");
    }

    #[test]
    fn optional_settings_are_omitted_not_nulled() {
        // A null `advanced_settings` is not the same as an absent one to a
        // strict API, and sending nulls is how you get 422s that look like
        // nothing is wrong.
        let v = wire(WireRequest {
            objective: None,
            search_queries: vec!["stucco".into()],
            mode: Mode::Basic,
            advanced_settings: None,
        });
        assert!(v.get("objective").is_none());
        assert!(v.get("advanced_settings").is_none());
        assert_eq!(v["search_queries"][0], "stucco");
    }

    #[test]
    fn freshness_survives_the_swap() {
        let v = wire(WireRequest {
            objective: Some("today's scores".into()),
            search_queries: vec!["scores".into()],
            mode: Mode::Basic,
            advanced_settings: Some(AdvancedSettings {
                max_results: Some(5),
                fetch_policy: Some(FetchPolicy {
                    max_age_seconds: 3600,
                }),
            }),
        });
        assert_eq!(v["advanced_settings"]["fetch_policy"]["max_age_seconds"], 3600);
        assert_eq!(v["advanced_settings"]["max_results"], 5);
    }
}

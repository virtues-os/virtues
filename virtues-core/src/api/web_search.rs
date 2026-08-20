//! Web search, executed by the Vercel AI Gateway.
//!
//! **No search vendor account.** The gateway runs the search tool itself during
//! a model turn and bills it to the gateway key we already have — which is the
//! entire reason this exists. Exa needed its own account; so did a direct
//! Parallel client. This needs neither.
//!
//! The shape is unusual and worth stating plainly: a search here is a *model
//! call*. We hand the gateway a question and a tool; the model composes its own
//! queries, the gateway runs them, and the results come back as a `tool-result`
//! block. We ignore the model's prose entirely and keep the structured results.
//!
//! That costs more than calling a search API directly — the results pass
//! through a model's context, so a search is roughly a cent rather than half of
//! one. It buys one fewer vendor relationship, which was the ask.
//!
//! Two protocol facts, both load-bearing and both learned the hard way:
//!
//! - Provider-executed tools do NOT exist on `/v1/chat/completions`. It rejects
//!   them with `expected "function"`. They live only on the gateway's own
//!   `/v4/ai/language-model` endpoint, which is what the proxy route speaks.
//! - The wire type is `"provider"`, not `"provider-defined"` — the latter is
//!   the AI SDK's internal name and the gateway refuses it.
//!
//! The search vendor is one string: `gateway.parallel_search` becomes
//! `gateway.exa_search` or `gateway.perplexity_search` and nothing else moves.

use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;

use crate::error::{Error, Result};
use crate::virtues_api::client::BearerClient;
use virtues_registry::models::{default_model_for_slot, ModelSlot};

/// The gateway tool that performs the search.
const SEARCH_TOOL_ID: &str = "gateway.parallel_search";

/// What the search model is told before the query.
///
/// Both sentences were earned by measurement, not caution. Over 16 broad
/// queries the unconstrained prompt produced 5 malformed tool calls
/// (`search_queries` sent as a string, or as nested arrays — the gateway
/// validates and fails that search) and fired anywhere from 1 to **11** searches
/// in a single turn, which at $0.005 each is a 5.5¢ query pretending to be a
/// half-cent one. The same 16 queries with this instruction: zero malformed
/// calls, exactly one search every time.
///
/// One search per call is also the right layering. If the answer is thin, the
/// calling agent can invoke the tool again — that decision belongs to the
/// conversation, not to a sub-call quietly spending money on its own initiative.
const SEARCH_INSTRUCTION: &str = "Perform EXACTLY ONE web search, then stop and \
     answer briefly. When you call the search tool, `search_queries` MUST be an \
     array of plain strings — never a single string, never nested arrays.\n\n";

/// Cap on excerpt text kept per result.
///
/// The gateway already returns excerpts rather than whole pages; this stops one
/// verbose result from crowding the others out of the caller's context.
const MAX_TEXT_CHARS: usize = 1_200;

#[derive(Debug, Clone)]
pub struct SearchRequest {
    /// What the caller wants to know. Passed as the prompt, so the model can
    /// compose sensible queries from it.
    pub query: String,
    /// Extra framing when the query alone is ambiguous.
    pub objective: Option<String>,
    pub max_results: Option<u8>,
    /// Refuse cached pages older than this.
    pub max_age_seconds: Option<u32>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResult {
    pub title: Option<String>,
    pub url: String,
    pub published_date: Option<String>,
    /// The passages the search judged relevant.
    pub text: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
}

// ── The slice of the gateway's response we actually read ────────────────────

#[derive(Debug, Deserialize)]
struct GatewayResponse {
    #[serde(default)]
    content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    /// The only block we care about. Everything else — the model's reasoning,
    /// its prose answer, its tool call — is discarded: the caller wants sources,
    /// not a second opinion.
    #[serde(rename = "tool-result")]
    ToolResult { result: ToolResultBody },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ToolResultBody {
    Ok {
        results: Vec<RawResult>,
    },
    /// The gateway reports tool failures inside the result rather than as an
    /// HTTP error: a failed search returns `{error, message}` and a 200.
    ///
    /// Observed in the wild: a model may issue SEVERAL searches in one turn and
    /// have only some of them fail. So a failure is only fatal when nothing
    /// else succeeded — otherwise the partial results are the answer, and
    /// throwing them away over a sibling's failure would lose a good search.
    Err {
        error: serde_json::Value,
        #[serde(default)]
        message: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
struct RawResult {
    url: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    publish_date: Option<String>,
    #[serde(default)]
    excerpts: Vec<String>,
}

/// Run a web search through the gateway.
pub async fn search(pool: &PgPool, request: SearchRequest) -> Result<SearchResponse> {
    let query = request.query.trim();
    if query.is_empty() {
        return Err(Error::InvalidInput("search needs a query".to_string()));
    }

    let prompt_text = match request.objective.as_deref().map(str::trim) {
        Some(o) if !o.is_empty() => format!("{SEARCH_INSTRUCTION}{o}\n\nSearch the web for: {query}"),
        _ => format!("{SEARCH_INSTRUCTION}Search the web for: {query}"),
    };

    let mut tool_args = json!({});
    if let Some(n) = request.max_results {
        tool_args["maxResults"] = json!(n);
    }
    if let Some(secs) = request.max_age_seconds {
        tool_args["fetchPolicy"] = json!({ "maxAgeSeconds": secs });
    }

    let body = json!({
        // The proxy lifts this into the gateway's header and forwards the rest.
        //
        // Lite, not Chat. The model here only has to compose a query and let
        // the gateway search — its prose is discarded — so paying Chat-slot
        // rates for it is waste: measured, the model side is $0.0006 on Lite
        // against $0.018 on Chat, which is the difference between a search
        // costing half a cent and costing two.
        "model": default_model_for_slot(ModelSlot::Lite),
        "prompt": [{
            "role": "user",
            "content": [{ "type": "text", "text": prompt_text }]
        }],
        "tools": [{
            "type": "provider",
            "id": SEARCH_TOOL_ID,
            "name": "parallel_search",
            "args": tool_args
        }]
    });

    let response = BearerClient::from_env(pool.clone())
        .post_json("/v1/ai/search", &body)
        .await
        .map_err(|e| Error::ExternalApi(format!("virtues-api/search request failed: {e}")))?;

    if !response.is_success() {
        return Err(Error::ExternalApi(format!(
            "virtues-api/search error ({}): {}",
            response.status, response.body
        )));
    }

    let parsed: GatewayResponse = serde_json::from_value(response.body)
        .map_err(|e| Error::ExternalApi(format!("failed to parse search response: {e}")))?;

    parse_results(parsed)
}

fn parse_results(parsed: GatewayResponse) -> Result<SearchResponse> {
    let mut results = Vec::new();
    let mut failure: Option<String> = None;
    let mut saw_tool_result = false;

    for block in parsed.content {
        let ContentBlock::ToolResult { result } = block else {
            continue;
        };
        saw_tool_result = true;
        match result {
            ToolResultBody::Ok { results: raw } => {
                results.extend(raw.into_iter().map(|r| SearchResult {
                    title: r.title,
                    url: r.url,
                    published_date: r.publish_date,
                    text: {
                        let joined = r.excerpts.join("\n\n");
                        let trimmed = joined.trim();
                        (!trimmed.is_empty())
                            .then(|| trimmed.chars().take(MAX_TEXT_CHARS).collect())
                    },
                }));
            }
            ToolResultBody::Err { error, message } => {
                failure.get_or_insert_with(|| message.unwrap_or_else(|| error.to_string()));
            }
        }
    }

    // Only fatal if every search failed. A partial failure still has an answer.
    if results.is_empty() && saw_tool_result {
        if let Some(why) = failure {
            return Err(Error::ExternalApi(format!("search tool failed: {why}")));
        }
    }
    Ok(SearchResponse { results })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocks(v: serde_json::Value) -> GatewayResponse {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn keeps_results_and_discards_the_models_prose() {
        let r = parse_results(blocks(json!({"content": [
            {"type": "reasoning", "text": "I should search."},
            {"type": "text", "text": "Here is what I found..."},
            {"type": "tool-call", "toolName": "parallel_search"},
            {"type": "tool-result", "result": {"search_id": "s1", "results": [
                {"url": "https://a.example", "title": "A", "publish_date": "2026-07-01",
                 "excerpts": ["first", "second"]},
                {"url": "https://b.example", "excerpts": []}
            ]}}
        ]})))
        .unwrap();

        assert_eq!(r.results.len(), 2);
        assert_eq!(r.results[0].title.as_deref(), Some("A"));
        assert_eq!(r.results[0].text.as_deref(), Some("first\n\nsecond"));
        assert_eq!(r.results[0].published_date.as_deref(), Some("2026-07-01"));
        // No excerpts is None, not an empty string — the caller renders the
        // absence rather than a blank line.
        assert_eq!(r.results[1].text, None);
        assert_eq!(r.results[1].title, None);
    }

    #[test]
    fn a_total_tool_failure_is_an_error_not_an_empty_result_set() {
        // The gateway returns 200 with {error, message} inside the tool result
        // when a search fails. Reporting that as "found nothing" would hide a
        // broken integration behind a plausible answer.
        let e = parse_results(blocks(json!({"content": [
            {"type": "tool-result", "result": {"error": "unsupported", "message": "search failed"}}
        ]})))
        .unwrap_err();
        assert!(e.to_string().contains("search failed"), "got: {e}");
    }

    #[test]
    fn a_partial_failure_keeps_the_results_that_worked() {
        // Observed live: a model issues several searches in one turn and only
        // some fail. Discarding the successful ones because a sibling errored
        // would throw away the answer we already have.
        let r = parse_results(blocks(json!({"content": [
            {"type": "tool-result", "result": {"error": "rate_limited", "message": "slow down"}},
            {"type": "tool-result", "result": {"results": [
                {"url": "https://a.example", "title": "A", "excerpts": ["text"]}
            ]}}
        ]})))
        .unwrap();
        assert_eq!(r.results.len(), 1);
        assert_eq!(r.results[0].url, "https://a.example");
    }

    #[test]
    fn a_response_with_no_tool_result_yields_no_results() {
        // The model answered without searching. Honest empty, not an error.
        let r = parse_results(blocks(json!({"content": [
            {"type": "text", "text": "I already know this."}
        ]})))
        .unwrap();
        assert!(r.results.is_empty());
    }

    #[test]
    fn several_searches_in_one_turn_are_all_collected() {
        let r = parse_results(blocks(json!({"content": [
            {"type": "tool-result", "result": {"results": [{"url": "https://a.example", "excerpts": []}]}},
            {"type": "tool-result", "result": {"results": [{"url": "https://b.example", "excerpts": []}]}}
        ]})))
        .unwrap();
        assert_eq!(r.results.len(), 2);
    }
}

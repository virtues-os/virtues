//! Web Search tool implementation (Parallel).
//!
//! Answers "find me pages about X". Distinct from `fetch` (`virtues-core/src/fetch`),
//! which answers "read the one page I already have" — different jobs, no shared
//! code.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use super::executor::{ToolError, ToolResult};
use crate::api::web_search as search_api;

/// Web search tool arguments (from LLM)
#[derive(Debug, Deserialize)]
pub struct WebSearchArgs {
    /// Search query
    pub query: String,
    /// What the caller is actually trying to learn. Optional, and the reason
    /// this API beats a bare keyword search — it disambiguates a short query.
    #[serde(default)]
    pub objective: Option<String>,
    /// Number of results (1-10, default [`DEFAULT_RESULTS`])
    #[serde(default)]
    pub num_results: Option<u8>,
    /// Freshness: max age (hours) of a cached result before re-fetching live.
    /// Use `1` for news/sports/odds/live data; omit for stable information.
    #[serde(default)]
    pub max_age_hours: Option<u32>,
}

/// Results per search when the model does not say. Eight rather than five:
/// a turn that got five thin results ran another query to fill the gap, and
/// every extra step re-sends the whole conversation, which costs more than
/// three more passages in this one.
pub const DEFAULT_RESULTS: u8 = 8;

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

        let request = search_api::SearchRequest {
            objective: args.objective,
            query: args.query.clone(),
            // Sent explicitly, not left to the gateway: the schema tells the
            // model the default, so the default must be the one that runs.
            max_results: Some(args.num_results.unwrap_or(DEFAULT_RESULTS).clamp(1, 10)),
            // The tool speaks hours because that is the unit a model reasons
            // in; the API wants seconds.
            max_age_seconds: args.max_age_hours.map(|h| h.saturating_mul(3600)),
        };

        let response = search_api::search(&self.pool, request)
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

/// Times whole chat turns that search, through the real agent loop, prompt,
/// tools and Standard-slot model, and keeps each answer for reading. Spends
/// real money through virtues-api, so it is ignored:
///
/// ```sh
/// VIRTUES_API_URL=http://localhost:9002 VIRTUES_API_KEY=<dev key> \
///   ROUNDS=2 BENCH_LABEL=before BENCH_OUT=/tmp/before.json \
///   cargo test -p virtues --lib web_search_turn_bench -- --ignored --nocapture
/// ```
#[cfg(test)]
mod turn_bench {
    use std::time::{Duration, Instant};

    use futures::StreamExt;
    use serde_json::{json, Value};
    use sqlx::PgPool;

    use crate::agent::{AgentConfig, AgentEvent, AgentLoop};
    use crate::api::chat_mode::ChatMode;
    use crate::tools::ToolContext;

    /// Four plain lookups and a question with two parts.
    const QUESTIONS: &[&str] = &[
        "are the bengals favored today?",
        "what's the price of gold today?",
        "who won the world series?",
        "what's the weather in chicago this weekend?",
        "how do the iPhone 17 Pro and Pixel 10 Pro cameras compare?",
    ];

    struct Run {
        first_text: Option<Duration>,
        total: Duration,
        searches: usize,
        queries: Vec<String>,
        answer: String,
    }

    async fn turn(pool: &PgPool, model: &str, system: &str, q: &str) -> Run {
        let mode = ChatMode::Chat;
        let limits = mode.limits();
        let agent = AgentLoop::new(pool.clone())
            .with_config(AgentConfig {
                max_steps: limits.max_steps,
                tool_timeout: limits.tool_timeout,
                parallel_tools: true,
                thinking: limits.thinking,
            })
            .with_budget(limits.budget);
        let t0 = Instant::now();
        let messages = vec![
            json!({ "role": "system", "content": system }),
            json!({ "role": "user", "content": q }),
        ];
        let mut stream =
            agent.run(model.to_string(), messages, mode.tools(), ToolContext::default(), None);
        let mut run = Run {
            first_text: None,
            total: Duration::ZERO,
            searches: 0,
            queries: vec![],
            answer: String::new(),
        };
        while let Some(event) = stream.next().await {
            match event {
                AgentEvent::TextDelta { content } => {
                    if run.first_text.is_none() && !content.trim().is_empty() {
                        run.first_text = Some(t0.elapsed());
                    }
                    run.answer.push_str(&content);
                }
                AgentEvent::ToolCallArgsComplete { args, .. } => {
                    if let Some(q) = args.get("query").and_then(|q| q.as_str()) {
                        run.queries.push(q.to_string());
                    }
                }
                AgentEvent::ToolCallResult { result, .. } if result.get("results_count").is_some() => {
                    run.searches += 1;
                }
                _ => {}
            }
        }
        run.total = t0.elapsed();
        run
    }

    fn median(mut v: Vec<f64>) -> f64 {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        if v.is_empty() { f64::NAN } else { v[v.len() / 2] }
    }

    fn mean(v: &[f64]) -> f64 {
        v.iter().sum::<f64>() / v.len() as f64
    }

    #[tokio::test]
    #[ignore]
    async fn web_search_turn_bench() {
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://virtues:virtues@localhost:5432/virtues".to_string());
        let pool = PgPool::connect(&url).await.expect("dev database");
        let model = crate::agent::headless::model_for_slot(&pool, virtues_registry::models::ModelSlot::Standard).await;
        let system = crate::api::chat::build_system_prompt_for_audit(&pool).await;
        let rounds: usize = std::env::var("ROUNDS").ok().and_then(|r| r.parse().ok()).unwrap_or(2);
        let label = std::env::var("BENCH_LABEL").unwrap_or_else(|_| "run".to_string());
        println!("{label}: model {model}, prompt ~{} tokens, {rounds} rounds", system.len() / 4);

        // Warm the prompt cache so the first measured turn does not pay the write.
        turn(&pool, &model, &system, "hello").await;

        let mut runs: Vec<(&str, Run)> = vec![];
        for _ in 0..rounds {
            for q in QUESTIONS {
                let r = turn(&pool, &model, &system, q).await;
                println!(
                    "{label} {q:60} first text {:>5.1}s  total {:>5.1}s  searches {}  {:?}",
                    r.first_text.map_or(f64::NAN, |d| d.as_secs_f64()),
                    r.total.as_secs_f64(),
                    r.searches,
                    r.queries
                );
                runs.push((q, r));
            }
        }

        let searched: Vec<&Run> = runs.iter().map(|(_, r)| r).filter(|r| r.searches > 0).collect();
        let first: Vec<f64> = searched.iter().filter_map(|r| r.first_text).map(|d| d.as_secs_f64()).collect();
        let total: Vec<f64> = searched.iter().map(|r| r.total.as_secs_f64()).collect();
        println!(
            "{label} summary: n={} first text median {:.1}s mean {:.2}s | total median {:.1}s mean {:.2}s | searches/turn {:.2}",
            searched.len(),
            median(first.clone()),
            mean(&first),
            median(total.clone()),
            mean(&total),
            searched.iter().map(|r| r.searches as f64).sum::<f64>() / searched.len() as f64
        );

        if let Ok(path) = std::env::var("BENCH_OUT") {
            let rows: Vec<Value> = runs
                .iter()
                .map(|(q, r)| {
                    json!({
                        "label": label, "question": q, "searches": r.searches, "queries": r.queries,
                        "first_text_s": r.first_text.map(|d| d.as_secs_f64()),
                        "total_s": r.total.as_secs_f64(), "answer": r.answer,
                    })
                })
                .collect();
            std::fs::write(&path, serde_json::to_string_pretty(&rows).unwrap()).expect("write BENCH_OUT");
        }
    }
}

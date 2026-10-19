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
    pool: Arc<PgPool>,
}

impl SemanticSearchTool {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self {
            engine: Arc::new(SemanticSearchEngine::new(pool.clone())),
            pool,
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
        project_id: Option<&str>,
        scope_mode: crate::search::ScopeMode,
    ) -> Result<ToolResult, ToolError> {
        // Prefer `queries` (multi-facet recall); fall back to the single `query`
        // for back-compat. Blank phrasings are dropped; cap at 4.
        let mut queries: Vec<String> = arguments
            .get("queries")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        if queries.is_empty() {
            if let Some(q) = arguments.get("query").and_then(|v| v.as_str()) {
                let q = q.trim();
                if !q.is_empty() {
                    queries.push(q.to_string());
                }
            }
        }
        if queries.is_empty() {
            return Err(ToolError::InvalidParameters(
                "queries (or query) is required".into(),
            ));
        }
        queries.truncate(4);

        // The tool advertises friendly domain names (email, calendar, document…)
        // that do NOT equal the real ontology names (calendar_event,
        // uploaded_document…). Normalize aliases to real ontology names and
        // DROP anything that resolves to nothing — a hallucinated or unknown
        // domain must degrade to "search everything", never zero the results.
        let domains: Option<Vec<String>> = arguments
            .get("domains")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .flat_map(normalize_domain)
                    .collect::<Vec<String>>()
            })
            .filter(|d| !d.is_empty());

        let date_after = arguments.get("date_after").and_then(|v| v.as_str());
        let date_before = arguments.get("date_before").and_then(|v| v.as_str());
        // Resolved entity IDs (person/place/org) to scope the search to.
        let entities: Option<Vec<String>> = arguments
            .get("entities")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect());
        let num_results = arguments
            .get("num_results")
            .and_then(|v| v.as_i64());

        let opts = crate::search::query::SearchOptions {
            ontologies: domains.unwrap_or_default(),
            date_after: date_after.map(str::to_string),
            date_before: date_before.map(str::to_string),
            entities: entities.unwrap_or_default(),
            project_id: project_id.map(str::to_string),
            scope_mode,
            limit: num_results,
        };
        let mut results = self
            .engine
            .search_multi(&queries, &opts)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Semantic search failed: {}", e)))?;

        // What the wiki says about anything the search names, ahead of the
        // results. Not when the agent asked for particular sources, nor in a
        // chat scoped to a project's materials alone.
        let scoped = !opts.ontologies.is_empty()
            || (project_id.is_some() && scope_mode == crate::search::ScopeMode::Exclusive);
        let mut from_wiki = Vec::new();
        if !scoped {
            let parse = |d: Option<&str>| d.and_then(crate::search::query::parse_date_filter);
            let range = crate::search::wiki_first::day_range(
                parse(date_after),
                parse(date_before),
                chrono::Utc::now().date_naive(),
            );
            match crate::search::wiki_first::wiki_first(&self.pool, &queries, &opts.entities, range)
                .await
            {
                Ok(lead) => {
                    // An article already above is not repeated below.
                    results.retain(|r| {
                        !(r.ontology == "wiki_article" && lead.page_ids.contains(&r.record_id))
                    });
                    from_wiki = lead.entries;
                }
                // The search still answers; it just loses its lead.
                Err(e) => tracing::warn!(error = %e, "wiki lead failed; returning the search alone"),
            }
        }

        // Document-chunk hits cite the FILE VIEWER at the right page (with a
        // quote snippet for passage landing), not the raw record route — the
        // researcher-plan trust loop. One batch lookup for all doc hits.
        let doc_chunk_ids: Vec<String> = results
            .iter()
            .filter(|r| r.ontology == "uploaded_document")
            .map(|r| r.record_id.clone())
            .collect();
        let doc_refs = self
            .engine
            .document_ref_info(&doc_chunk_ids)
            .await
            .unwrap_or_default();

        // Annotation hits cite the viewer at the highlight itself.
        let anno_ids: Vec<String> = results
            .iter()
            .filter(|r| r.ontology == "document_marginalia")
            .map(|r| r.record_id.clone())
            .collect();
        let anno_refs = self
            .engine
            .annotation_ref_info(&anno_ids)
            .await
            .unwrap_or_default();

        let result_json: Vec<serde_json::Value> = results
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let ref_route = if let Some((file_id, _filename, page, quote)) =
                    doc_refs.get(&r.record_id)
                {
                    let mut route = format!("/drive/{file_id}");
                    let mut sep = '?';
                    if let Some(p) = page {
                        route.push_str(&format!("{sep}page={p}"));
                        sep = '&';
                    }
                    if !quote.is_empty() {
                        route.push_str(&format!("{sep}q={}", urlencoding::encode(quote)));
                    }
                    route
                } else if let Some((file_id, page)) = anno_refs.get(&r.record_id) {
                    // hl=<annotation_id> lands on the highlight (D2.4 flashes it).
                    let mut route = format!("/drive/{file_id}");
                    let mut sep = '?';
                    if let Some(p) = page {
                        route.push_str(&format!("{sep}page={p}"));
                        sep = '&';
                    }
                    route.push_str(&format!("{sep}hl={}", r.record_id));
                    route
                } else {
                    format!("/record/{}/{}", r.ontology, r.record_id)
                };
                serde_json::json!({
                    "ontology": r.ontology,
                    "record_id": r.record_id,
                    // Rank, not "score": the engine min-max normalizes within
                    // the result set, so the old 0–1 number made the top hit of
                    // a garbage pool look identical to the top hit of a great
                    // one ("1.000") — manufactured confidence the model then
                    // cited. Rank claims only what is true: relative order.
                    "rank": i + 1,
                    "title": r.title,
                    // Transcript previews carry per-chunk speaker numbers on old
                    // rows; flatten them as every other narrator read does.
                    "preview": r.preview.as_deref().map(crate::transcript::flatten_speaker_tags),
                    "author": r.author,
                    "timestamp": r.timestamp,
                    // Viewable route for this exact source — document chunks
                    // open the file viewer at their page; everything else opens
                    // the raw record in the data viewer. Cite it inline (see
                    // the tool-usage prompt).
                    "ref": ref_route,
                })
            })
            .collect();

        let mut body = serde_json::json!({
            "results": result_json,
            "count": results.len(),
            "tip": "Use sql_query with record IDs to get full details for specific results."
        });
        // Keys serialize in sorted order, and this one sorts ahead of
        // `results`, so the model reads the wiki first.
        if !from_wiki.is_empty() {
            body["from_your_wiki"] = from_wiki.into();
        }
        Ok(ToolResult::success(body))
    }
}

/// Map a friendly domain alias to the real ontology name(s) the search filters
/// on (`se.ontology`). Unknown tokens that aren't already a real ontology name
/// map to nothing, so the caller drops them rather than zeroing the search.
fn normalize_domain(raw: &str) -> Vec<String> {
    let d = raw.trim().to_lowercase();
    let mapped: &[&str] = match d.as_str() {
        "document" | "documents" | "doc" | "docs" | "file" | "files" | "pdf" | "paper"
        | "papers" => &["uploaded_document"],
        "highlight" | "highlights" | "annotation" | "annotations" => &["document_marginalia"],
        "message" | "messages" | "email" | "emails" | "sms" | "text" => {
            &["communication_message"]
        }
        "transcription" | "transcript" | "transcripts" | "audio" | "recording" => {
            &["communication_transcription"]
        }
        "calendar" | "event" | "events" | "meeting" | "meetings" => &["calendar_event"],
        "transaction" | "transactions" | "finance" | "financial" | "purchase" => {
            &["financial_transaction"]
        }
        "chat" | "chats" | "conversation" | "conversations" | "ai_conversation" => {
            &["app_chat", "app_chat_message"]
        }
        "page" | "pages" | "note" | "notes" => &["app_page"],
        // Already a real ontology name → pass through; else it maps to nothing.
        other => {
            if virtues_registry::ontologies::get_ontology(other).is_some() {
                return vec![other.to_string()];
            }
            &[]
        }
    };
    mapped.iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::normalize_domain;

    #[test]
    fn friendly_aliases_map_to_real_ontologies() {
        assert_eq!(normalize_domain("document"), vec!["uploaded_document"]);
        assert_eq!(normalize_domain("PDF"), vec!["uploaded_document"]);
        assert_eq!(normalize_domain("highlights"), vec!["document_marginalia"]);
        assert_eq!(normalize_domain("calendar"), vec!["calendar_event"]);
        assert_eq!(
            normalize_domain("chat"),
            vec!["app_chat", "app_chat_message"]
        );
    }

    #[test]
    fn real_ontology_names_pass_through() {
        assert_eq!(normalize_domain("uploaded_document"), vec!["uploaded_document"]);
    }

    #[test]
    fn hallucinated_domains_drop_to_nothing() {
        // The exact failure seen on the box: a made-up ontology name must
        // resolve to nothing (→ caller searches everything, not zero).
        assert!(normalize_domain("data_content_document").is_empty());
        assert!(normalize_domain("nonsense").is_empty());
    }
}

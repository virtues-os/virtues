//! Semantic search query engine.
//!
//! Hybrid retrieval, per the on-device field-report measurements:
//!   1. Dense recall — the query embedding against `search_vectors`
//!      (pgvector `halfvec`, HNSW cosine `<=>`), top-200.
//!   2. Lexical recall — real BM25 (k1=1.5, b=0.75) over `search_bm25_postings`,
//!      document-frequency derived inline, top-200. (Not `ts_rank`, which has no
//!      IDF and dragged hybrid *below* dense-only.)
//!   3. Fusion — z-score normalize both arms over the candidate union and blend
//!      with a query-adaptive weight α: rare-term/entity queries lean lexical,
//!      paraphrase queries stay dense.
//!   4. Dedupe to one chunk per record, then a *conditional* cross-encoder /
//!      ColBERT rerank — only when the fused top-1/top-2 margin is tight.

use anyhow::Result;
use pgvector::Vector;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;

use super::bm25;
use super::embedder::get_embedder;
use super::reranker::get_reranker;

/// A single semantic search result. `score` is always normalized to [0, 1]
/// within the result set (min-max) — after reranking it's the rerank order,
/// otherwise the fused-score order. Only meaningful as a within-query ranking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub ontology: String,
    pub record_id: String,
    pub score: f64,
    pub title: Option<String>,
    pub preview: Option<String>,
    pub author: Option<String>,
    pub timestamp: Option<String>,
    /// The matched chunk's text — what the reranker scores (not the whole
    /// record). `None` only for legacy rows without `content`.
    #[serde(skip)]
    pub content: Option<String>,
}

/// Semantic search engine
pub struct SemanticSearchEngine {
    pool: Arc<PgPool>,
}

/// First-stage candidate pool per arm before fusion. The field-report SQL used
/// 200; the fused top-`recall_limit` feed the reranker.
const CANDIDATE_POOL: i64 = 200;

/// Cap each reranker candidate so a (query, doc) pair fits the rerank model's
/// window (~512 tok for the cross-encoder, 256 for ColBERT). A document's lead
/// carries the relevance signal, so truncating to ~1000 chars (~256 tok) is
/// lossless in effect. Char-based (not byte) slicing keeps it UTF-8 safe.
const MAX_RERANK_CHARS: usize = 1000;

fn truncate_for_rerank(text: &str) -> String {
    if text.chars().count() <= MAX_RERANK_CHARS {
        text.to_string()
    } else {
        text.chars().take(MAX_RERANK_CHARS).collect()
    }
}

/// Conditional-rerank trigger: rerank only when the fused top-1/top-2 score
/// margin is below this (the ranking is ambiguous and reranking can reorder it);
/// skip it when the top result already dominates. The field report calibrated a
/// ~60th-percentile gap offline on SciFact; we can't transplant that constant to
/// a personal corpus, so this defaults conservatively (skip only clear top-1
/// wins) and is tunable via `VIRTUES_RERANK_GAP` pending real-data calibration.
fn rerank_gap_threshold() -> f64 {
    std::env::var("VIRTUES_RERANK_GAP")
        .ok()
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|v| *v >= 0.0)
        .unwrap_or(1.5)
}

/// Min-max rescale candidate scores into [0, 1] in place, preserving order.
fn normalize_scores(candidates: &mut [SearchResult]) {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for c in candidates.iter() {
        lo = lo.min(c.score);
        hi = hi.max(c.score);
    }
    let span = hi - lo;
    for c in candidates.iter_mut() {
        c.score = if span > 0.0 { (c.score - lo) / span } else { 1.0 };
    }
}

impl SemanticSearchEngine {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// Confirms the search_vectors HNSW index is reachable.
    pub async fn ensure_vec_table(&self) -> Result<()> {
        sqlx::query("SELECT 1 FROM search_vectors LIMIT 0")
            .execute(self.pool.as_ref())
            .await?;
        tracing::info!("search_vectors table ready");
        Ok(())
    }

    /// The query-adaptive fusion weight α (lexical share) plus the BM25 corpus
    /// stats (N, avgdl) reused by the main query. α = 0.4·clip((mean top-2 query
    /// IDF − 5)/5): high-IDF (rare/entity) query terms pull α up toward lexical;
    /// common paraphrase terms leave it near 0 (pure dense). df for the query's
    /// terms is fetched here (cheap, indexed) and the IDFs computed in Rust.
    async fn fusion_alpha(&self, terms: &[String]) -> Result<(f64, i64, f64)> {
        if terms.is_empty() {
            return Ok((0.0, 0, 1.0));
        }
        let (n_docs, sum_len): (i64, i64) =
            sqlx::query_as("SELECT n_docs, sum_len FROM search_bm25_stats WHERE singleton")
                .fetch_optional(self.pool.as_ref())
                .await?
                .unwrap_or((0, 0));
        if n_docs == 0 {
            return Ok((0.0, 0, 1.0));
        }
        let avg_len = sum_len as f64 / n_docs as f64;

        let df_rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT term, count(*)::int8 FROM search_bm25_postings \
             WHERE term = ANY($1) GROUP BY term",
        )
        .bind(terms)
        .fetch_all(self.pool.as_ref())
        .await?;
        let df: HashMap<String, i64> = df_rows.into_iter().collect();

        // IDF over the distinct query terms (Lucene/BM25+ variant, matching the
        // scoring SQL). An absent term has df=0 → maximal IDF.
        let mut distinct: Vec<&String> = terms.iter().collect();
        distinct.sort();
        distinct.dedup();
        let mut idfs: Vec<f64> = distinct
            .iter()
            .map(|t| {
                let d = df.get(*t).copied().unwrap_or(0) as f64;
                (((n_docs as f64) - d + 0.5) / (d + 0.5) + 1.0).ln()
            })
            .collect();
        idfs.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        let top2 = &idfs[..idfs.len().min(2)];
        let mean_idf = if top2.is_empty() {
            0.0
        } else {
            top2.iter().sum::<f64>() / top2.len() as f64
        };
        let alpha = 0.4 * ((mean_idf - 5.0) / 5.0).clamp(0.0, 1.0);
        Ok((alpha, n_docs, avg_len))
    }

    /// Search for similar documents by natural language query.
    pub async fn search(
        &self,
        query: &str,
        ontologies: Option<&[String]>,
        date_after: Option<&str>,
        date_before: Option<&str>,
        // Resolved entity IDs (person/place/org/thing). When set, only rows whose
        // source record references one of these entities are returned.
        entities: Option<&[String]>,
        limit: Option<i64>,
    ) -> Result<Vec<SearchResult>> {
        let embedder = get_embedder().await?;
        let query_vec = embedder.embed_query_async(query).await?;
        let query_vector = Vector::from(query_vec);
        let limit = limit.unwrap_or(10).clamp(1, 50);
        let recall_limit = (limit * 2).clamp(10, 20);

        // Lexical arm inputs: BM25 query terms (same tokenizer as ingest) and the
        // adaptive fusion weight (+ corpus stats reused in the scoring SQL).
        let terms = bm25::tokens(query);
        let (alpha, n_docs, avg_len) = self.fusion_alpha(&terms).await?;
        let w_dense = 1.0 - alpha;
        let w_lex = alpha;

        // Shared filters (applied to both arms). $1 = query vector, $2 = query
        // terms; filter placeholders start at $3; the final placeholder is the
        // recall limit.
        let mut filter_sql = String::new();
        let mut next = 3usize;
        if let Some(onts) = ontologies {
            if !onts.is_empty() {
                let ph: Vec<String> = (0..onts.len()).map(|i| format!("${}", next + i)).collect();
                filter_sql.push_str(&format!(" AND se.ontology IN ({})", ph.join(",")));
                next += onts.len();
            }
        }
        if date_after.is_some() {
            filter_sql.push_str(&format!(" AND se.timestamp >= ${next}"));
            next += 1;
        }
        if date_before.is_some() {
            filter_sql.push_str(&format!(" AND se.timestamp <= ${next}"));
            next += 1;
        }
        let entity_filter = entities.map(|e| !e.is_empty()).unwrap_or(false);
        if entity_filter {
            filter_sql.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM wiki_entity_refs er \
                  WHERE er.source_table = se.source_table AND er.source_id = se.record_id \
                  AND er.entity_id = ANY(${next}))",
            ));
            next += 1;
        }
        let lim = next; // recall_limit placeholder

        // Hybrid: dense (halfvec `<=>`) ⊕ BM25 (inline df), unioned, z-fused with
        // a query-adaptive weight, deduped to the best chunk per record.
        // Numeric constants (N, avgdl, k1, b, weights) are Rust-computed and
        // inlined — injection-safe (all f64/i64).
        let sql = format!(
            "WITH prm AS (SELECT $1::halfvec AS qv), \
             dense AS ( \
               SELECT se.id \
               FROM search_vectors vs JOIN search_embeddings se ON se.id = vs.embedding_id, prm \
               WHERE 1=1{f} \
               ORDER BY vs.embedding <=> prm.qv, se.id LIMIT {pool} \
             ), lex AS ( \
               SELECT dt.chunk_id AS id, \
                      sum( ln(({n}::float8 - df.df + 0.5)/(df.df + 0.5) + 1) \
                           * dt.tf * {k1p1} \
                           / (dt.tf + {k1}*({omb} + {b}*COALESCE(se.bm25_len,1)::float8/{avg})) ) AS bs \
               FROM search_bm25_postings dt \
               JOIN (SELECT term, count(*) AS df FROM search_bm25_postings \
                     WHERE term = ANY($2) GROUP BY term) df ON df.term = dt.term \
               JOIN search_embeddings se ON se.id = dt.chunk_id \
               WHERE dt.term = ANY($2){f} \
               GROUP BY dt.chunk_id, se.bm25_len ORDER BY bs DESC LIMIT {pool} \
             ), u AS (SELECT id FROM dense UNION SELECT id FROM lex), \
             sc AS ( \
               SELECT u.id, -(vs.embedding <=> prm.qv) AS ds, COALESCE(l.bs, 0) AS bs \
               FROM u JOIN search_vectors vs ON vs.embedding_id = u.id \
                      LEFT JOIN lex l ON l.id = u.id, prm \
             ), z AS ( \
               SELECT id, \
                 (ds - avg(ds) OVER())/(COALESCE(stddev_samp(ds) OVER(),0) + 1e-9) AS dz, \
                 (bs - avg(bs) OVER())/(COALESCE(stddev_samp(bs) OVER(),0) + 1e-9) AS bz \
               FROM sc \
             ), best AS ( \
               SELECT DISTINCT ON (se.record_id) \
                      se.ontology, se.record_id, se.title, se.preview, se.author, \
                      to_char(se.timestamp AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as ts, \
                      se.content, ({wd}*z.dz + {wl}*z.bz)::float8 AS s \
               FROM z JOIN search_embeddings se ON se.id = z.id \
               ORDER BY se.record_id, s DESC, se.id \
             ) \
             SELECT ontology, record_id, title, preview, author, ts, content, s \
             FROM best ORDER BY s DESC, record_id LIMIT ${lim}",
            f = filter_sql,
            pool = CANDIDATE_POOL,
            n = n_docs,
            avg = avg_len,
            k1 = bm25::K1,
            k1p1 = bm25::K1 + 1.0,
            omb = 1.0 - bm25::B,
            b = bm25::B,
            wd = w_dense,
            wl = w_lex,
            lim = lim,
        );

        let mut db_query = sqlx::query_as::<
            _,
            (
                String,
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>, // content (chunk text)
                f64,             // fused score
            ),
        >(&sql)
        .bind(&query_vector)
        .bind(&terms);

        if let Some(onts) = ontologies {
            for ont in onts {
                db_query = db_query.bind(ont);
            }
        }
        if let Some(da) = date_after {
            db_query = db_query.bind(
                chrono::DateTime::parse_from_rfc3339(da)
                    .map(|d| d.with_timezone(&chrono::Utc))
                    .ok(),
            );
        }
        if let Some(db) = date_before {
            db_query = db_query.bind(
                chrono::DateTime::parse_from_rfc3339(db)
                    .map(|d| d.with_timezone(&chrono::Utc))
                    .ok(),
            );
        }
        if entity_filter {
            db_query = db_query.bind(entities.unwrap().to_vec());
        }
        db_query = db_query.bind(recall_limit);

        let rows = db_query.fetch_all(self.pool.as_ref()).await?;

        let mut candidates: Vec<SearchResult> = rows
            .into_iter()
            .map(|row| SearchResult {
                ontology: row.0,
                record_id: row.1,
                title: row.2,
                preview: row.3,
                author: row.4,
                timestamp: row.5,
                content: row.6,
                score: row.7, // fused score; reranked or normalized below
            })
            .collect();

        // Conditional rerank: only when the fused top-1/top-2 margin is tight.
        let ambiguous = candidates.len() > 1 && {
            let gap = candidates[0].score - candidates[1].score;
            gap < rerank_gap_threshold()
        };
        let reranked = if ambiguous {
            match self.rerank_candidates(query, &mut candidates).await {
                Ok(did) => {
                    if did {
                        let q: String = query.chars().take(60).collect();
                        tracing::debug!("Reranked {} candidates for: {}", candidates.len(), q);
                    }
                    did
                }
                Err(e) => {
                    tracing::warn!("Reranker unavailable, using fused ranking: {}", e);
                    false
                }
            }
        } else {
            false
        };
        let _ = reranked;

        // Normalize to [0, 1] for the caller/LLM (order already set — by rerank
        // if it ran, else by the fused SQL).
        normalize_scores(&mut candidates);
        candidates.truncate(limit as usize);
        Ok(candidates)
    }

    /// Rerank candidates over the matched **chunk** text. Returns `true` if it
    /// reranked, `false` if there were no usable docs. Errors only when the
    /// reranker is unreachable (caller falls back to fused order). Sets each
    /// candidate's `score` to the raw rerank score (cross-encoder logit or
    /// ColBERT MaxSim); the caller min-max normalizes — both are monotonic, so
    /// order is preserved either way.
    async fn rerank_candidates(
        &self,
        query: &str,
        candidates: &mut Vec<SearchResult>,
    ) -> Result<bool> {
        let reranker = get_reranker().await?;

        let mut rerank_indices: Vec<usize> = Vec::new();
        let mut rerank_docs: Vec<String> = Vec::new();
        for (i, c) in candidates.iter().enumerate() {
            let text = c
                .content
                .clone()
                .or_else(|| c.preview.clone())
                .unwrap_or_default();
            if !text.is_empty() {
                rerank_indices.push(i);
                rerank_docs.push(truncate_for_rerank(&text));
            }
        }
        if rerank_docs.is_empty() {
            return Ok(false);
        }

        let scores = reranker.rerank_async(query, &rerank_docs).await?;
        for score in &scores {
            candidates[rerank_indices[score.index]].score = score.score as f64;
        }
        candidates.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(true)
    }
}

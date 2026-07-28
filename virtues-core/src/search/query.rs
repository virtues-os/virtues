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

/// Notebook-scoped retrieval (lean v1): additive bonus, in z-score space, for a
/// candidate chunk that belongs to the active notebook's members. z-scores can be
/// negative, so we ADD a bonus (≈ one std-dev) rather than multiply — a boost, not
/// a hard filter (recall is unchanged; only ranking shifts toward the notebook).
const NOTEBOOK_BOOST: f64 = 1.0;

/// How an active notebook shapes retrieval (user-facing: "Open" vs "Scoped"
/// chat — see researcher-plan decision 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScopeMode {
    /// Open: search everything; notebook members get the additive z-boost.
    #[default]
    Weighted,
    /// Scoped: hard-filter to the notebook's members (grounded chat). An
    /// empty scope returns no results — honest, never silently open.
    Exclusive,
}

/// The scoping applied to a recall pass, resolved and owned so a single set can
/// be reused across many recall calls (e.g. multi-query fan-out) without
/// re-resolving. Empty collections mean "no filter". Notebook membership is
/// pre-resolved into `nb_records`/`nb_entities` by `resolve_notebook_scope`, so
/// `recall_and_fuse` does no I/O of its own.
#[derive(Debug, Clone, Default)]
pub struct SearchFilters {
    pub ontologies: Vec<String>,
    pub date_after: Option<String>,
    pub date_before: Option<String>,
    pub entities: Vec<String>,
    pub nb_records: Vec<String>,
    pub nb_entities: Vec<String>,
    pub scope_mode: ScopeMode,
    /// Record routes (`/record/{ontology}/{record_id}`) to exclude from recall
    /// BEFORE the fused LIMIT. The magnet passes a container's existing members
    /// and own-chat here: they are the nearest neighbours of the container's own
    /// centroid, so filtering them *after* recall would let them consume the
    /// whole budget and starve fresh candidates. Empty means no exclusion.
    pub exclude_urls: Vec<String>,
}

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
pub(crate) fn normalize_scores(candidates: &mut [SearchResult]) {
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

/// Reciprocal Rank Fusion constant. A document's fused weight is
/// `Σ 1/(RRF_K + rank)` across the lists it appears in; the standard `k=60`
/// damps how much any single list's top ranks dominate.
const RRF_K: f64 = 60.0;

/// How many fused candidates survive into the single rerank pass. Wider than one
/// query's recall (multiple variants contribute), but still a shortlist.
const RRF_POOL: usize = 30;

/// Merge several ranked candidate lists into one by Reciprocal Rank Fusion.
///
/// Each per-query list is z-normalized *within its own pool*, so scores are not
/// comparable across queries — RRF merges by **rank**, which is. A record that
/// several variants rank highly rises; the surviving `SearchResult` keeps its
/// first-seen fields (its `content` still feeds the downstream reranker). The
/// returned `score` is the RRF weight, replaced by Stage B's normalization.
fn rrf_merge(lists: Vec<Vec<SearchResult>>, k: f64, pool: usize) -> Vec<SearchResult> {
    let mut acc: HashMap<(String, String), (f64, SearchResult)> = HashMap::new();
    for list in lists {
        for (rank, hit) in list.into_iter().enumerate() {
            let key = (hit.ontology.clone(), hit.record_id.clone());
            let entry = acc.entry(key).or_insert_with(|| (0.0, hit.clone()));
            entry.0 += 1.0 / (k + rank as f64 + 1.0);
        }
    }
    let mut merged: Vec<(f64, SearchResult)> = acc.into_values().collect();
    merged.sort_by(|a, b| b.0.total_cmp(&a.0));
    merged
        .into_iter()
        .take(pool)
        .map(|(w, mut hit)| {
            hit.score = w;
            hit
        })
        .collect()
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
            sqlx::query_as("SELECT n_docs, sum_len FROM search_index_meta WHERE singleton")
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
    #[allow(clippy::too_many_arguments)]
    pub async fn search(
        &self,
        query: &str,
        ontologies: Option<&[String]>,
        date_after: Option<&str>,
        date_before: Option<&str>,
        // Resolved entity IDs (person/place/org/thing). When set, only rows whose
        // source record references one of these entities are returned.
        entities: Option<&[String]>,
        // Active notebook: its members' chunks get an additive ranking boost
        // (Weighted) or become the only searchable set (Exclusive).
        notebook_id: Option<&str>,
        scope_mode: ScopeMode,
        limit: Option<i64>,
    ) -> Result<Vec<SearchResult>> {
        let embedder = get_embedder().await?;
        let query_vec = embedder.embed_query_async(query).await?;
        let query_vector = Vector::from(query_vec);
        let limit = limit.unwrap_or(10).clamp(1, 50);
        let recall_limit = (limit * 2).clamp(10, 20);
        let terms = bm25::tokens(query);

        // Notebook scoping: resolve the active notebook's members into a set of
        // record_ids (page/day/source/chat + document chunks for /drive/file_
        // members) and entity_ids (person/place/org/thing). Weighted = additive
        // ranking bonus; Exclusive = hard filter (grounded chat).
        let (nb_records, nb_entities): (Vec<String>, Vec<String>) = match notebook_id {
            Some(nb) => self.resolve_notebook_scope(nb).await?,
            None => (Vec::new(), Vec::new()),
        };
        let notebook_scoped = !nb_records.is_empty() || !nb_entities.is_empty();
        // Grounded chat over an empty (or fully unindexed) scope: honest zero
        // results — never silently fall open to the whole graph.
        if notebook_id.is_some() && scope_mode == ScopeMode::Exclusive && !notebook_scoped {
            return Ok(Vec::new());
        }

        let filters = SearchFilters {
            ontologies: ontologies.map(<[String]>::to_vec).unwrap_or_default(),
            date_after: date_after.map(str::to_string),
            date_before: date_before.map(str::to_string),
            entities: entities.map(<[String]>::to_vec).unwrap_or_default(),
            nb_records,
            nb_entities,
            scope_mode,
            exclude_urls: Vec::new(),
        };

        let candidates = self
            .recall_and_fuse(&query_vector, &terms, &filters, recall_limit)
            .await?;
        self.rerank_and_finalize(query, candidates, limit as usize)
            .await
    }

    /// Multi-query retrieval (RAG-fusion). Runs recall for several phrasings of
    /// one information need in parallel and fuses them with RRF, then reranks the
    /// merged pool ONCE against the primary query (`queries[0]`). Widens recall on
    /// vague or many-worded questions where a single phrasing misses.
    ///
    /// Cheap on-box: the variants embed in ONE batched sidecar call and their
    /// recalls run concurrently (Postgres parallelizes); only the final rerank
    /// touches the reranker, once. A single-element `queries` delegates to
    /// `search()` unchanged.
    #[allow(clippy::too_many_arguments)]
    pub async fn search_multi(
        &self,
        queries: &[String],
        ontologies: Option<&[String]>,
        date_after: Option<&str>,
        date_before: Option<&str>,
        entities: Option<&[String]>,
        notebook_id: Option<&str>,
        scope_mode: ScopeMode,
        limit: Option<i64>,
    ) -> Result<Vec<SearchResult>> {
        // One phrasing (or none) is just a plain search — no fan-out overhead.
        let non_empty: Vec<&String> = queries.iter().filter(|q| !q.trim().is_empty()).collect();
        if non_empty.len() <= 1 {
            let q = non_empty.first().map(|s| s.as_str()).unwrap_or("");
            return self
                .search(
                    q, ontologies, date_after, date_before, entities, notebook_id, scope_mode,
                    limit,
                )
                .await;
        }
        let queries: Vec<String> = non_empty.into_iter().cloned().collect();

        let limit = limit.unwrap_or(10).clamp(1, 50);
        let recall_limit = (limit * 2).clamp(10, 20); // per-variant

        // Resolve notebook scope ONCE and share it across all variants.
        let (nb_records, nb_entities): (Vec<String>, Vec<String>) = match notebook_id {
            Some(nb) => self.resolve_notebook_scope(nb).await?,
            None => (Vec::new(), Vec::new()),
        };
        let notebook_scoped = !nb_records.is_empty() || !nb_entities.is_empty();
        if notebook_id.is_some() && scope_mode == ScopeMode::Exclusive && !notebook_scoped {
            return Ok(Vec::new());
        }
        let filters = SearchFilters {
            ontologies: ontologies.map(<[String]>::to_vec).unwrap_or_default(),
            date_after: date_after.map(str::to_string),
            date_before: date_before.map(str::to_string),
            entities: entities.map(<[String]>::to_vec).unwrap_or_default(),
            nb_records,
            nb_entities,
            scope_mode,
            exclude_urls: Vec::new(),
        };

        // One batched embed call for every variant.
        let embedder = get_embedder().await?;
        let vecs = embedder.embed_query_batch(&queries).await?;

        // Recall each variant concurrently (shared &filters, immutable &self).
        let recalls = queries.iter().zip(vecs.into_iter()).map(|(q, v)| {
            let qv = Vector::from(v);
            let terms = bm25::tokens(q);
            let filters = &filters;
            async move { self.recall_and_fuse(&qv, &terms, filters, recall_limit).await }
        });
        let lists = futures::future::try_join_all(recalls).await?;

        // Fuse by rank, then one rerank against the user's actual question.
        let merged = rrf_merge(lists, RRF_K, RRF_POOL);
        self.rerank_and_finalize(&queries[0], merged, limit as usize)
            .await
    }

    /// Stage A — recall + z-fusion for one query VECTOR. Returns fused, deduped
    /// candidates ordered by the fused z-score, WITHOUT reranking or [0,1]
    /// normalization (that is Stage B, `rerank_and_finalize`).
    ///
    /// Takes a vector, not text, so a centroid or an event embedding is a
    /// first-class query — the magnet's centroid ANN and multi-query fan-out are
    /// both callers. `terms` are the BM25 tokens of the source query (empty for a
    /// pure-vector query, which degenerates cleanly to dense-only: the lexical
    /// arm matches nothing and `bz` normalizes to 0). Scope resolution is the
    /// caller's job (see `SearchFilters`); this method does no notebook I/O and
    /// does not enforce Exclusive honest-zero — `search()` does that before
    /// calling in.
    pub(crate) async fn recall_and_fuse(
        &self,
        query_vector: &Vector,
        terms: &[String],
        filters: &SearchFilters,
        recall_limit: i64,
    ) -> Result<Vec<SearchResult>> {
        // Adaptive fusion weight + corpus stats reused in the scoring SQL.
        let (alpha, n_docs, avg_len) = self.fusion_alpha(terms).await?;
        let w_dense = 1.0 - alpha;
        let w_lex = alpha;

        let notebook_scoped =
            !filters.nb_records.is_empty() || !filters.nb_entities.is_empty();
        let notebook_boost = notebook_scoped;

        // Shared filters (applied to both arms). $1 = query vector, $2 = query
        // terms; filter placeholders start at $3; the final placeholder is the
        // recall limit.
        let mut filter_sql = String::new();
        let mut next = 3usize;
        if !filters.ontologies.is_empty() {
            let ph: Vec<String> = (0..filters.ontologies.len())
                .map(|i| format!("${}", next + i))
                .collect();
            filter_sql.push_str(&format!(" AND se.ontology IN ({})", ph.join(",")));
            next += filters.ontologies.len();
        }
        if filters.date_after.is_some() {
            filter_sql.push_str(&format!(" AND se.timestamp >= ${next}"));
            next += 1;
        }
        if filters.date_before.is_some() {
            filter_sql.push_str(&format!(" AND se.timestamp <= ${next}"));
            next += 1;
        }
        let entity_filter = !filters.entities.is_empty();
        if entity_filter {
            filter_sql.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM wiki_entity_refs er \
                  WHERE er.source_table = se.source_table AND er.source_id = se.record_id \
                  AND er.entity_id = ANY(${next}))",
            ));
            next += 1;
        }
        // Exclude specific record routes BEFORE the fused LIMIT, so filtered rows
        // don't consume the recall budget (see `SearchFilters::exclude_urls`).
        let exclude_filter = !filters.exclude_urls.is_empty();
        if exclude_filter {
            filter_sql.push_str(&format!(
                " AND ('/record/' || se.ontology || '/' || se.record_id) <> ALL(${next})"
            ));
            next += 1;
        }
        // Notebook boost placeholders ($record_ids, $entity_ids), bound after the
        // filters and before the recall limit.
        let (p_nb_rec, p_nb_ent) = if notebook_boost {
            let a = next;
            let b = next + 1;
            next += 2;
            (a, b)
        } else {
            (0, 0)
        };
        // Exclusive: membership becomes a hard filter on BOTH arms (the clause
        // joins filter_sql, which dense and lex share). Weighted: the additive
        // z-boost as before. Placeholder numbers were allocated above in bind
        // order, so using them inside filter_sql is safe.
        if notebook_boost && filters.scope_mode == ScopeMode::Exclusive {
            filter_sql.push_str(&format!(
                " AND (se.record_id = ANY(${r}) OR EXISTS (SELECT 1 FROM wiki_entity_refs er2 \
                  WHERE er2.source_table = se.source_table AND er2.source_id = se.record_id \
                  AND er2.entity_id = ANY(${e})))",
                r = p_nb_rec,
                e = p_nb_ent,
            ));
        }
        let boost_sql = if notebook_boost && filters.scope_mode == ScopeMode::Weighted {
            format!(
                " + CASE WHEN se.record_id = ANY(${r}) OR EXISTS (SELECT 1 FROM wiki_entity_refs er2 \
                  WHERE er2.source_table = se.source_table AND er2.source_id = se.record_id \
                  AND er2.entity_id = ANY(${e})) THEN {boost} ELSE 0 END",
                r = p_nb_rec,
                e = p_nb_ent,
                boost = NOTEBOOK_BOOST,
            )
        } else {
            String::new()
        };
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
                      se.content, ({wd}*z.dz + {wl}*z.bz{boost})::float8 AS s \
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
            boost = boost_sql,
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
        .bind(query_vector)
        .bind(terms);

        for ont in &filters.ontologies {
            db_query = db_query.bind(ont);
        }
        if let Some(da) = &filters.date_after {
            db_query = db_query.bind(
                chrono::DateTime::parse_from_rfc3339(da)
                    .map(|d| d.with_timezone(&chrono::Utc))
                    .ok(),
            );
        }
        if let Some(db) = &filters.date_before {
            db_query = db_query.bind(
                chrono::DateTime::parse_from_rfc3339(db)
                    .map(|d| d.with_timezone(&chrono::Utc))
                    .ok(),
            );
        }
        if entity_filter {
            db_query = db_query.bind(filters.entities.clone());
        }
        if exclude_filter {
            db_query = db_query.bind(filters.exclude_urls.clone());
        }
        if notebook_boost {
            db_query = db_query.bind(filters.nb_records.clone());
            db_query = db_query.bind(filters.nb_entities.clone());
        }
        db_query = db_query.bind(recall_limit);

        let rows = db_query.fetch_all(self.pool.as_ref()).await?;

        Ok(rows
            .into_iter()
            .map(|row| SearchResult {
                ontology: row.0,
                record_id: row.1,
                title: row.2,
                preview: row.3,
                author: row.4,
                timestamp: row.5,
                content: row.6,
                score: row.7, // fused score; reranked or normalized in Stage B
            })
            .collect())
    }

    /// Stage B — conditional rerank, then min-max normalize to [0,1] and truncate
    /// to `limit`. Reranks only when the fused top-1/top-2 margin is tight (the
    /// ordering is ambiguous and a rerank can reorder it); a dominant top-1 skips
    /// the reranker entirely. `query` is the text the reranker scores against.
    pub(crate) async fn rerank_and_finalize(
        &self,
        query: &str,
        mut candidates: Vec<SearchResult>,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let ambiguous = candidates.len() > 1 && {
            let gap = candidates[0].score - candidates[1].score;
            gap < rerank_gap_threshold()
        };
        if ambiguous {
            match self.rerank_candidates(query, &mut candidates).await {
                Ok(did) => {
                    if did {
                        let q: String = query.chars().take(60).collect();
                        tracing::debug!("Reranked {} candidates for: {}", candidates.len(), q);
                    }
                }
                Err(e) => {
                    tracing::warn!("Reranker unavailable, using fused ranking: {}", e);
                }
            }
        }

        // Normalize to [0, 1] for the caller/LLM (order already set — by rerank
        // if it ran, else by the fused SQL).
        normalize_scores(&mut candidates);
        candidates.truncate(limit);
        Ok(candidates)
    }

    /// Resolve an active notebook's members into the two buckets the search
    /// scope understands: direct record_ids (page/day/source/chat, plus the
    /// document CHUNKS of `/drive/file_` members — the uploaded_document
    /// ontology indexes per-chunk) and entity_ids (person/place/org/thing —
    /// matched via `wiki_entity_refs`). Filters to `role='library'` (= grounds
    /// chat; nav-only 'pin' rows are ignored). External URLs and nested
    /// notebooks aren't indexed and are skipped.
    async fn resolve_notebook_scope(&self, notebook_id: &str) -> Result<(Vec<String>, Vec<String>)> {
        let urls: Vec<String> = sqlx::query_scalar(
            "SELECT url FROM app_notebook_items WHERE notebook_id = $1 AND role = 'library'",
        )
        .bind(notebook_id)
        .fetch_all(self.pool.as_ref())
        .await?;
        let mut records = Vec::new();
        let mut entities = Vec::new();
        let mut file_ids = Vec::new();
        for url in urls {
            if let Some(id) = url.strip_prefix("/page/") {
                records.push(id.to_string());
            } else if let Some(id) = url.strip_prefix("/day/") {
                records.push(id.to_string());
            } else if let Some(id) = url.strip_prefix("/source/") {
                records.push(id.to_string());
            } else if let Some(id) = url.strip_prefix("/chat/") {
                records.push(id.to_string());
            } else if let Some(id) = url.strip_prefix("/person/") {
                entities.push(id.to_string());
            } else if let Some(id) = url.strip_prefix("/place/") {
                entities.push(id.to_string());
            } else if let Some(id) = url.strip_prefix("/org/") {
                entities.push(id.to_string());
            } else if let Some(id) = url.strip_prefix("/thing/") {
                entities.push(id.to_string());
            } else if let Some(id) = url.strip_prefix("/drive/") {
                if id.starts_with("file_") {
                    // Strip any viewer params (?page=N) a stored route carries.
                    file_ids.push(id.split('?').next().unwrap_or(id).to_string());
                }
            }
            // external https://, /notebook/ → not indexed
        }
        if !file_ids.is_empty() {
            let chunk_ids: Vec<String> = sqlx::query_scalar(
                "SELECT id FROM extracted_document_chunks WHERE file_id = ANY($1)",
            )
            .bind(&file_ids)
            .fetch_all(self.pool.as_ref())
            .await?;
            records.extend(chunk_ids);
        }
        Ok((records, entities))
    }

    /// Citation info for document-chunk hits: chunk_id → (file_id, filename,
    /// page_num, quote_head). The semantic_search tool uses this to emit
    /// viewer-resolvable refs (`/drive/{file}?page=N&q=…`) instead of raw
    /// record routes for `uploaded_document` results.
    pub async fn document_ref_info(
        &self,
        chunk_ids: &[String],
    ) -> Result<std::collections::HashMap<String, (String, String, Option<i32>, String)>> {
        if chunk_ids.is_empty() {
            return Ok(Default::default());
        }
        let rows: Vec<(String, String, String, Option<i32>, String)> = sqlx::query_as(
            "SELECT c.id, c.file_id, f.filename, c.page_num, c.quote_head \
             FROM extracted_document_chunks c \
             JOIN app_drive_files f ON f.id = c.file_id \
             WHERE c.id = ANY($1)",
        )
        .bind(chunk_ids)
        .fetch_all(self.pool.as_ref())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, file_id, filename, page, quote)| (id, (file_id, filename, page, quote)))
            .collect())
    }

    /// Citation info for annotation hits: annotation_id → (file_id, page_num).
    /// The semantic_search tool turns these into `/drive/{file}?page=N&hl=<id>`.
    pub async fn annotation_ref_info(
        &self,
        anno_ids: &[String],
    ) -> Result<std::collections::HashMap<String, (String, Option<i32>)>> {
        if anno_ids.is_empty() {
            return Ok(Default::default());
        }
        let rows: Vec<(String, String, Option<i32>)> = sqlx::query_as(
            "SELECT id, file_id, page_num FROM app_annotations WHERE id = ANY($1)",
        )
        .bind(anno_ids)
        .fetch_all(self.pool.as_ref())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, file_id, page)| (id, (file_id, page)))
            .collect())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sr(ontology: &str, record_id: &str) -> SearchResult {
        SearchResult {
            ontology: ontology.to_string(),
            record_id: record_id.to_string(),
            score: 0.0,
            title: None,
            preview: None,
            author: None,
            timestamp: None,
            content: Some(format!("{ontology}/{record_id}")),
        }
    }

    #[test]
    fn rrf_ranks_shared_top_hits_first_and_dedupes() {
        // A is rank-0 in BOTH lists; it must win. B/D are each a single rank-1;
        // C/E each a single rank-2.
        let list1 = vec![sr("m", "A"), sr("m", "B"), sr("m", "C")];
        let list2 = vec![sr("m", "A"), sr("m", "D"), sr("m", "E")];

        let merged = rrf_merge(vec![list1, list2], RRF_K, RRF_POOL);

        // Deduped to 5 distinct records, A first.
        assert_eq!(merged.len(), 5);
        assert_eq!(merged[0].record_id, "A");
        assert_eq!(merged.iter().filter(|r| r.record_id == "A").count(), 1);

        // A's fused weight is strictly greater than any single-list rank-1.
        let a = merged[0].score;
        let b = merged.iter().find(|r| r.record_id == "B").unwrap().score;
        assert!(a > b, "shared top hit must outweigh a single rank-1");
    }

    #[test]
    fn rrf_respects_the_pool_cap() {
        let list = vec![sr("m", "A"), sr("m", "B"), sr("m", "C")];
        let merged = rrf_merge(vec![list], RRF_K, 2);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].record_id, "A");
    }
}

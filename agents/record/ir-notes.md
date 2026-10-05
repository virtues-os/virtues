# IR & the magnet: how retrieval actually works, and where to take it

*Notebooks were renamed to projects on 2026-09-21 (`app_notebooks` → `app_projects`, `/notebook/…` → `/project/…`); the names below are as they were.*

*Working notes — 2026-07-22. A grounded map of the retrieval stack as it exists,
the non-obvious truths a full read exposed, and a ranked set of improvements with
spikes. Written after cutting `wiki_stories` from v1 (see
[stories-plan.md](../archive/stories-plan.md), now shelved) and fixing the notebook magnet
(branch `fix/notebook-magnet-drop-stories`). File references are to
`virtues-core/src/` and `crates/virtues-registry/src/` unless noted.*

---

## 0. TL;DR

- You have a **near-SOTA hybrid retriever** (dense + real BM25, z-score fusion,
  conditional rerank). The problems are **calibration and a few structural
  seams**, not the algorithm.
- The retrieval substrate has **two engines by data type**: semantic IR
  (`search()`) for prose/aboutness, deterministic SQL for structured/time. That
  split is healthy — name it and enforce it.
- The biggest single lever is a small refactor: **split `search()` into a
  vector-capable recall stage + a rerank/finalize stage.** It unlocks the magnet
  collapse, multi-query fan-out, and "days like this" — all as thin callers.

---

## 1. The stack as it really is

### 1.1 Recall → fusion → conditional rerank (`search/query.rs`)

Four stages, one `search()` call:

1. **Dense arm** — pgvector `halfvec` HNSW cosine (`<=>`), top-200
   (`CANDIDATE_POOL = 200`).
2. **Lexical arm** — real **BM25** (`k1=1.5, b=0.75`, `bm25.rs`) over
   `search_bm25_postings`, top-200. Not `ts_rank` — proper IDF, df derived
   inline per query (no stale global df table).
3. **Fusion** — per-arm **z-score normalization + query-adaptive weighted sum**
   (NOT RRF, NOT min-max):

   ```
   dz = (ds − mean(ds)) / (stddev(ds) + 1e-9)     # ds = −cosine_distance
   bz = (bs − mean(bs)) / (stddev(bs) + 1e-9)     # bs = BM25 score
   s  = w_dense·dz + w_lex·bz  (+ notebook boost)
   w_lex = alpha,  w_dense = 1 − alpha
   ```

   Then `DISTINCT ON (record_id)` keeps the highest-scoring chunk per record.

4. **Conditional rerank** — only when the top-1/top-2 fused-score gap is small
   (ambiguous ordering). Reranks, reorders, then min-max normalizes to [0,1].

Final output is clamped small: `recall_limit = (limit*2).clamp(10, 20)` — i.e.
`search()` returns **≤20 rows**.

### 1.2 The two hand-set constants (both guesses)

- **`alpha` (lexical weight), capped at 0.4.** `alpha = 0.4·clip((mean_idf − 5)/5, 0, 1)`,
  where `mean_idf` is the mean IDF of the two rarest query terms. So lexical
  weight is driven by term rarity but **can never exceed 40%** — meaning always
  keeps ≥60%. Magic constants: `0.4`, `5.0`, `5.0`, top-2 terms. Suspect for
  personal data, which is proper-noun-heavy (names, merchants, places) where
  exact-match should arguably weigh more.
- **`VIRTUES_RERANK_GAP = 1.5`** (z-score units) — the "confident enough to skip
  rerank" threshold. Self-documented in the code as a SciFact-derived placeholder
  that "can't transplant to a personal corpus." A guess.

### 1.3 The models (fleet-dependent — this matters)

- **Embedder** (`search/embedder.rs`): no hardcoded model. **Dragon/NPU** =
  `gte-small`, 384-d native, untruncated (`virtues-qnnd`, :18181). Default
  sidecar = EmbeddingGemma-300M, 768→256 Matryoshka. Query vs doc prompts
  supported (default empty). Model-swap protection via a boot-time fingerprint.
- **Reranker** (`search/reranker.rs`): model-agnostic `/v1/rerank` contract.
  **Default sidecar = a cross-encoder** (`gte-reranker-modernbert`, unbounded
  logits). **Dragon = ColBERT MaxSim** (`answerai-colbert-small-v1` via
  `virtues-qnnd`, :18182). *These two have incompatible score scales* — see §2.1.
- On Dragon a **single `virtues-qnnd` process serves both** embed (18181) and
  rerank (18182) on the one Hexagon v68 NPU.

### 1.4 What gets embedded (`crates/virtues-registry/src/ontologies.rs`)

Per-ontology embed text is a **SQL expression string** (`embed_text_sql`) — not
Rust. The indexer interpolates it and computes the text in Postgres per record,
then word-windows it (`WINDOW_WORDS=96 ≈ 128 tokens`, `OVERLAP_WORDS=14`,
`MAX_CHUNK_CHARS=2048`; short records stay one verbatim chunk).

- **Embedded** (prose-bearing): email, message, calendar, transcription,
  documents, `wiki_event`, bookmarks, chats, pages, annotations — **and
  `financial_transaction`** (the lone structured exception).
- **NOT embedded** (`embedding: None`): all health, all location, activity,
  audio, `financial_account`. Correct — structured streams carry no free text.
- `wiki_event` deliberately emits **NULL** embed-text for unknown/hidden events
  ("embedding the word 'Unknown' 84 times teaches the index nothing").

### 1.5 How retrieval is consumed

- **Chat = agentic.** The model calls the `semantic_search` tool (→ `search()`)
  or `sql_query`, steered only by prose tool descriptions. Retrieval is NOT
  pre-stuffed; the model decides per-query. A small deterministic context block
  (identity, last 3 day-autobiographies, active sources) *is* pre-stuffed via
  plain SQL.
- **Notebook-grounded chat** = `ScopeMode::Exclusive` — hard-filters to the
  notebook's members; empty scope returns honest zero, never falls open.
  `ScopeMode::Weighted` instead adds a flat `+1.0` z-boost to members.
- **Day/home layer = 100% deterministic**, time-bounded SQL per ontology
  (`data_health_*`, `data_financial_transaction`, `wiki_events`, …). **No
  embeddings are ever queried here.** Events are embedded *downstream* (novelty,
  magnet) but the day layer never reads those vectors.
- **The magnet** (`magnet.rs`) is a **second, partly-bespoke retrieval path**:
  it calls `search()` for the seed-text arm AND hand-writes its own centroid
  `<=>` ANN + `DISTINCT ON` dedup.

---

## 2. Non-obvious truths the mapping exposed

### 2.1 The reranker score-scale schism is the root disease

The reranker client is model-agnostic, but the fleet runs two rerankers with
**incompatible score scales**: sidecar cross-encoder (unbounded logits;
threshold-able) vs Dragon ColBERT MaxSim (~28.6 baseline, tiny relevance delta).
Nothing abstracts "reranker score → comparable number." Consequences that all
trace to this one fact:

- the magnet's old absolute gate (`raw ≥ 2.0`) was *correct for the cross-encoder*
  and silently broke when Dragon swapped in ColBERT (every candidate scored ~28.6,
  `sigmoid` saturated to 1.000, everything admitted);
- `query.rs`'s rerank-gap (1.5) and its sigmoid also assume a scale.

The magnet bug was never a magnet bug — it was **fleet heterogeneity with no
score-calibration layer.**

### 2.2 The magnet is a duplicate engine, and it has a hidden bug

`magnet.rs` reimplements pgvector ANN + dedup that `search()` already does — two
independently-maintained query strings over the same schema (drift risk; this is
literally how the centroid-dim bug hid for months). And it calls
`search(seed, limit=60)`, but `search()` clamps to `recall_limit ≤ 20`, so the
magnet's hybrid arm is **silently starved to 20 candidates** while asking for 60.

### 2.3 The day layer and semantic IR never touch

Events are embedded but the day layer only reads them by time SQL. "What did I do
like today, last year?" works **only** if the chat model happens to choose
`semantic_search` — there's no deterministic bridge from the temporal spine into
similarity retrieval. A one-directional gap exactly where the meaning-making
payoff should be.

### 2.4 The transaction embed-text degenerates

Designed as `merchant_name || ' ' || category_labels`, but on the box `category`
is empty everywhere, so it collapses to the bare token `"Starbucks "`. Amount,
date, and account are **never** in the vector. So semantic search over
transactions is merchant-token matching at best — noise that dilutes the index.

---

## 3. Ideas, ranked by leverage

Moved to [`../plan/retrieval-plan.md`](../plan/retrieval-plan.md) on
2026-09-29, less what had shipped (the `search()` split, the magnet collapse,
multi-query fan-out).

---

## 4. Multi-query fan-out — detailed sketch (idea ⑦)

### 4.1 What Dragon needs: essentially nothing new (measured 2026-07-22)

| Piece | Measured | Implication |
|---|---|---|
| Embed sidecar (18181) | 3 queries → 3 vectors in **one** call | variants embed in one batched call |
| Rerank sidecar (18182) | 69ms single, **207ms for 5 concurrent** | cheap; and we rerank **once** anyway |
| NPU daemon | one `virtues-qnnd` serves both, partial concurrency | not a bottleneck at these latencies |

**No query-expansion LLM is needed.** Chat is already agentic — the model
generates the query, so it can emit a few facets in the same tool call. Zero new
model round-trips; works whether chat runs local or cloud.

### 4.2 The pipeline

```
embed_batch([q1..qN])                         ← ONE sidecar call
├─ recall_and_fuse(v1) ┐
├─ recall_and_fuse(v2) ┤  parallel (tokio::join); Postgres parallelizes
└─ recall_and_fuse(vN) ┘
→ RRF merge across the N ranked lists          ← cross-query dedup here
→ rerank ONCE on merged top-K (vs primary query q1)
→ normalize → top-k
```

Rerank once, at the end, on the merged pool — variants widen *recall* only; the
primary query is what we answer.

### 4.3 The method

```rust
pub async fn search_multi(&self, queries: &[String], filters: &Filters, limit: usize)
    -> Result<Vec<SearchResult>>
{
    if queries.len() == 1 { return self.search(&queries[0], .., limit).await; }

    let vecs = embedder.embed_query_batch(queries).await?;          // one call

    let lists: Vec<Vec<SearchResult>> = join_all(
        queries.iter().zip(&vecs).map(|(q, v)| {
            let terms = bm25::tokens(q);
            self.recall_and_fuse(v, &terms, filters)                 // Stage A (idea ②)
        })
    ).await.into_iter().collect::<Result<_>>()?;

    let merged = rrf_merge(lists, RRF_K, RRF_POOL);                  // → top ~30
    self.rerank_and_finalize(&queries[0], merged, limit).await      // Stage B
}
```

### 4.4 RRF fusion (rank-based, because scores aren't comparable across queries)

```rust
const RRF_K: f64 = 60.0;     // canonical
const RRF_POOL: usize = 30;  // survive into the single rerank

fn rrf_merge(lists: Vec<Vec<SearchResult>>, k: f64, pool: usize) -> Vec<SearchResult> {
    let mut acc: HashMap<(String,String), (f64, SearchResult)> = HashMap::new(); // (ontology, record_id)
    for list in lists {
        for (rank, hit) in list.iter().enumerate() {
            let e = acc.entry((hit.ontology.clone(), hit.record_id.clone()))
                       .or_insert((0.0, hit.clone()));
            e.0 += 1.0 / (k + rank as f64 + 1.0);
        }
    }
    let mut v: Vec<_> = acc.into_values().collect();
    v.sort_by(|a, b| b.0.total_cmp(&a.0));
    v.into_iter().take(pool).map(|(_, hit)| hit).collect()
}
```

Each per-query list is z-normalized *within its own pool*, so scores across
queries aren't comparable — RRF merges by **rank**, the field-standard fix.

### 4.5 The tool change (only agent-facing edit)

In `crates/virtues-registry/src/tools.rs`, `query: string` → `queries: array`
(`minItems 1, maxItems 4`), described as "one to four phrasings/facets of the same
information need — provide multiple when the question is broad or vague." Keep
accepting a bare `query` string, mapped to `queries:[query]`, for back-compat.
`tools/semantic_search.rs` reads the array and calls `search_multi`.

### 4.6 Decisions pinned

- **Variants:** cap 4; 2–3 is the sweet spot; the model chooses.
- **Rerank:** always rerank the fused pool (one ~70ms call; fusion mixed scales so
  the gap-skip is meaningless here) — quietly retires one uncalibrated constant.
- **Filters/scope:** `Filters` pass straight through Stage A, so Exclusive
  grounding + date/entity filters work unchanged across all variants.
- **Degenerate:** near-duplicate variants just reinforce the same docs (harmless);
  all-empty → honest zero.

---

## 5. Build order, and open calibration questions

Moved to [`../plan/retrieval-plan.md`](../plan/retrieval-plan.md).

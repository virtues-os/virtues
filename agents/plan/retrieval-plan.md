# Retrieval — calibration and cleanup

**Status:** Open. The rest of the retrieval backlog from
[`ir-notes.md`](../record/ir-notes.md) (2026-07-22) is what remains after the
structural work shipped: `search()` is split into `recall_and_fuse` /
`rerank_and_finalize`, the magnet runs on them with no private ANN, multi-query
fan-out (`search_multi`, RRF, `queries[]` on `semantic_search`) is live, the
rerank gap is a relative margin, and the HNSW index is reachable again
(42210362). Day-level "days like this" is `similar_days` in
`api/day_article.rs`, owned by `day-article-plan.md`.
Getting chat to retrieve *through the wiki* is
[`wiki-retrieval-plan.md`](wiki-retrieval-plan.md); the two share one labeled
query set (below).

## 1. Calibrate reranker scores centrally

The fleet runs rerankers with incompatible score scales (sidecar cross-encoder
logits vs the Dragon's ColBERT MaxSim, ~28.6 baseline with a tiny relevance
delta). `search()` is safe because it uses rerank scores as order only and then
min-max normalizes. Anything that needs an **absolute** threshold is not:

- the magnet still admits by `rerank − anchor_baseline ≥ DELTA`, injecting
  known-irrelevant anchor docs (`magnet.rs`) — a pragmatic hack from the spike,
  not best practice;
- the wiki-retrieval relevance cutoff will need the same.

Give the reranker client (`search/reranker.rs`) a per-backend calibration so
every consumer sees one comparable scale. Prefer, in order:

1. **Cross-encoder for admit decisions** (logit → sigmoid → probability).
2. **Length-normalize ColBERT** — MaxSim ≈ query tokens × mean max-sim, so the
   baseline is mostly a token-count offset; divide it out.
3. **Pool-tail baseline** — the bottom of the actual candidate pool as the
   negatives, instead of injected anchors.
4. Score-distribution modeling (Manmatha) — most rigorous, heaviest.

*Spike:* implement 2 or 3; re-run the magnet gate on the Dragon **and** a
sidecar box; confirm one threshold works on both; then retire the anchors.

## 2. Drop transactions from the semantic index

`financial_transaction` embeds `merchant || categories`; amount, date and
account never reach the vector, so semantic search over transactions is
merchant-token matching that dilutes the pool. Set its `embedding` to `None`,
reindex, and A/B a few real queries for pool cleanliness. Transactions stay
answerable through `sql_query`. If category enrichment makes the text useful,
revisit as a synthesized sentence, not a bare token.

## 3. Calibrate the constants on a labeled set

Two values are still guesses: the lexical fusion cap (`fusion_alpha`,
α = 0.4 · clip(IDF), `search/query.rs`) and `VIRTUES_RERANK_GAP` (relative
margin, default 0.4). Build a small labeled query set from real questions on
the box — the same ten that
[`wiki-retrieval-plan.md`](wiki-retrieval-plan.md) step 0 needs — and sweep
both.

Questions it answers:

- Does proper-noun-heavy personal search want α above 0.4?
- What rerank gap fits the corpus, in normalized units after §1?
- Does RRF over 2–3 phrasings beat single-query recall enough to justify the
  fan-out, measured rather than assumed?

## 4. Write the two-engine doctrine down

*Prose/aboutness → `search()`; structured/time → SQL; no third path.* It is the
antidote to retrieval methods multiplying (the magnet's bespoke ANN was the one
violation, now retired). It belongs in `agents/build/` as a short section, so a
new retrieval feature is a thin caller of one of the two engines.

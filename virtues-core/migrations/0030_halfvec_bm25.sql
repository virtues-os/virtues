-- 0030 — Retrieval system rework: halfvec dense + true BM25 lexical.
--
-- Two changes, both from the on-device field-report measurements:
--
--  1. Dense vectors move to `halfvec` (fp16). Half the storage, ~3× faster
--     query, recall within noise of fp32 (0.909 vs 0.906 @300k). The column
--     TYPE + dim + HNSW ops are (re)applied by `database::ensure_embedding_dims`
--     at bringup — it already owns the vector column, sizes it to the active
--     model, and guards a populated index against a dim change. So the vector
--     column itself is NOT touched here.
--
--  2. Lexical retrieval switches from Postgres FTS `ts_rank` to real BM25.
--     `ts_rank` has no IDF (measured 0.30 alone, and it dragged hybrid *below*
--     dense-only); true BM25 scores ~0.66 and is the single biggest quality
--     lever in the data layer. BM25 is computed in SQL over a postings table;
--     document-frequency is derived inline per query (no stale global df table),
--     and N/avgdl come from a tiny running stats row.

-- Postings: one row per (chunk, term) with term frequency. `term` is a plain
-- lowercase [a-z0-9]+ token (NOT a tsvector lexeme) — the tokenization must
-- match query.rs and the indexer exactly for scores to line up.
CREATE TABLE IF NOT EXISTS search_bm25_postings (
    chunk_id TEXT NOT NULL REFERENCES search_embeddings(id) ON DELETE CASCADE,
    term     TEXT NOT NULL,
    tf       INT  NOT NULL,
    PRIMARY KEY (chunk_id, term)
);
-- The hot path: `WHERE term = ANY($query_terms)`, and the inline df count.
CREATE INDEX IF NOT EXISTS idx_search_bm25_postings_term
    ON search_bm25_postings (term);

-- Per-chunk token length, for BM25 length normalization (the `dl.len/avgdl`
-- term). Lives on the chunk row so the fusion query needs no extra join.
ALTER TABLE search_embeddings ADD COLUMN IF NOT EXISTS bm25_len INT;

-- Global BM25 stats — doc count (N) and summed length (for avgdl = sum_len/N)
-- without a per-query scan over the corpus. Single row, maintained by the
-- indexer on insert and reset by `virtues reindex`. (Deletes may leave it
-- slightly high until the next reindex; length-normalization is mild, so the
-- drift is negligible.)
CREATE TABLE IF NOT EXISTS search_bm25_stats (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    n_docs    BIGINT  NOT NULL DEFAULT 0,
    sum_len   BIGINT  NOT NULL DEFAULT 0
);
INSERT INTO search_bm25_stats (singleton) VALUES (TRUE) ON CONFLICT DO NOTHING;

-- NOTE: the ts_rank FTS path (`content_tsv` + its GIN index) is retired by the
-- query rewrite, NOT here — query.rs still SELECTs `content_tsv` until then, so
-- dropping it in this migration would break every search on the transition. The
-- drop moves into the migration that ships alongside the BM25 query.

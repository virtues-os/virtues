-- 0031 — Retire the ts_rank FTS path, now that query.rs scores lexical via BM25
-- (0030 tables) instead of `ts_rank(content_tsv, …)`. Ships together with the
-- BM25 query rewrite, so there's no window where a live search still reads
-- `content_tsv`. Chunk text stays in `content` (tokenized for BM25, read by the
-- reranker); only the generated tsvector + its GIN index — now dead write-cost —
-- are dropped.
DROP INDEX IF EXISTS idx_search_embeddings_tsv;
ALTER TABLE search_embeddings DROP COLUMN IF EXISTS content_tsv;

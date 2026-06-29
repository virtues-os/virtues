-- 0018 — Hybrid retrieval: lexical (Postgres FTS) alongside dense (pgvector).
--
-- Personal data is full of exact tokens dense embeddings smear — proper nouns,
-- project names, "wood glue", dates, IDs. A pure-dense system fumbles "the wood
-- glue text" because that's a rare exact token, not a semantic concept. So we
-- index the chunk text lexically and fuse dense + lexical with Reciprocal Rank
-- Fusion (RRF) in query.rs before reranking.
--
-- `content` holds the indexed chunk text (the same text we embed). `content_tsv`
-- is a generated tsvector over title + content, with a GIN index for FTS.
-- Backfilled by run_embedding_job (it already re-embeds after 0017).

ALTER TABLE search_embeddings ADD COLUMN IF NOT EXISTS content TEXT;

ALTER TABLE search_embeddings
    ADD COLUMN IF NOT EXISTS content_tsv tsvector
    GENERATED ALWAYS AS (
        to_tsvector('english', coalesce(title, '') || ' ' || coalesce(content, ''))
    ) STORED;

CREATE INDEX IF NOT EXISTS idx_search_embeddings_tsv
    ON search_embeddings USING gin (content_tsv);

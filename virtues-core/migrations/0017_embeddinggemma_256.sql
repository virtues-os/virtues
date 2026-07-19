-- 0017 — Migrate the embedding stack to EmbeddingGemma-300M @ 256-dim.
--
-- Was bge-m3 (1024-dim) via the llama-server embed sidecar. Now EmbeddingGemma
-- (768-dim native, Matryoshka-truncated to 256 + re-normalized in embedder.rs)
-- — a 4× smaller vector, so a 4× lighter HNSW index in RAM. Every stored
-- embedding was produced by the old model + a symmetric (no-prompt) encoding,
-- so they are all invalid: wipe and let `run_embedding_job` re-embed from
-- source (embeddings are derived data). search_embedding_progress is reset to
-- force a full backfill.
--
-- Pairs with: embedder.rs (EMBED_DIM=256, Matryoshka truncate, asymmetric
-- query/doc prompts), inference_report.rs (EMBED_GGUF/RERANK_GGUF), and the
-- installer sidecar units (--pooling mean for embed, --pooling rank + the new
-- GGUFs).

DROP INDEX IF EXISTS search_vectors_hnsw;

-- CASCADE truncates search_vectors too (it FK-references search_embeddings).
TRUNCATE search_embeddings CASCADE;
TRUNCATE search_topic_cache;
TRUNCATE search_embedding_progress;

-- wiki_events.embedding is a SEPARATE embedding store (BYTEA, used by dayline
-- novelty + autonomic scoring) that the search tables above don't cover. Its
-- blobs were produced by the old 1024-dim, no-prompt model. cosine_similarity
-- loops min(dims), so mixing them with new 256-dim vectors silently corrupts
-- novelty_z / local_novelty_z / autonomic scores forever (already-scored events
-- are never re-selected). Null the embeddings + every embedding-derived score
-- so each scoring pass recomputes from scratch with the new model.
UPDATE wiki_events SET
    embedding       = NULL,
    novelty_z       = NULL,
    local_novelty_z = NULL,
    lof_raw         = NULL,
    hr_z            = NULL,
    hrv_z           = NULL,
    autonomic_z     = NULL,
    topic_novelty   = NULL,
    entity_novelty  = NULL;

ALTER TABLE search_vectors     ALTER COLUMN embedding TYPE vector(256);
ALTER TABLE search_topic_cache ALTER COLUMN embedding TYPE vector(256);

CREATE INDEX search_vectors_hnsw
    ON search_vectors
    USING hnsw (embedding vector_cosine_ops);

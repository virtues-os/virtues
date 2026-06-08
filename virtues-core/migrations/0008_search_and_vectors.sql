-- 0008 — Search and embeddings.
--
-- Two-tier setup: metadata + scalar provenance live in `search_embeddings`
-- (filterable rows), and the actual vectors live in `search_vectors` with
-- an HNSW cosine index for ANN. This replaces the previous `vec_search`
-- virtual table and gives us a real ANN structure instead of the linear
-- scan we had before.
--
-- `search_topic_cache` stays for the topic→embedding cache used by the
-- novelty scoring path.

CREATE TABLE search_embeddings (
    id            TEXT PRIMARY KEY,
    ontology      TEXT NOT NULL,
    record_id     TEXT NOT NULL,
    text_hash     TEXT NOT NULL,
    model         TEXT NOT NULL,
    chunk_index   INTEGER NOT NULL DEFAULT 0,
    title         TEXT,
    preview       TEXT,
    author        TEXT,
    timestamp     TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(ontology, record_id, chunk_index)
);
CREATE INDEX idx_search_embeddings_ontology  ON search_embeddings(ontology);
CREATE INDEX idx_search_embeddings_timestamp ON search_embeddings(timestamp DESC);
CREATE INDEX idx_search_embeddings_record    ON search_embeddings(ontology, record_id);

CREATE TABLE search_embedding_progress (
    ontology                  TEXT PRIMARY KEY,
    last_processed_id         TEXT,
    last_processed_timestamp  TIMESTAMPTZ,
    total_embedded            BIGINT NOT NULL DEFAULT 0,
    last_run_at               TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 768-dim vectors from nomic-embed-text-v1.5. Cosine distance.
CREATE TABLE search_vectors (
    embedding_id  TEXT PRIMARY KEY REFERENCES search_embeddings(id) ON DELETE CASCADE,
    embedding     vector(768) NOT NULL
);
CREATE INDEX search_vectors_hnsw
    ON search_vectors
    USING hnsw (embedding vector_cosine_ops);

-- Topic→embedding cache (used by novelty scoring; not part of ANN search).
CREATE TABLE search_topic_cache (
    topic       TEXT PRIMARY KEY,
    embedding   vector(768) NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

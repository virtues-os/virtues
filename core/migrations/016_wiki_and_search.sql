-- 016: Wiki Enhancements & Semantic Search
--
-- Adds chaos/order scoring to wiki days and creates the semantic search
-- infrastructure (embedding metadata + indexer progress).
-- Vector data stored in vec0 virtual table (created programmatically at startup).

-- =============================================================================
-- PART 1: Wiki Day Scoring
-- =============================================================================

ALTER TABLE wiki_days ADD COLUMN chaos_score REAL;
ALTER TABLE wiki_days ADD COLUMN entropy_calibration_days INTEGER;

CREATE TABLE IF NOT EXISTS wiki_day_domain_embeddings (
    id TEXT PRIMARY KEY,
    day_date TEXT NOT NULL,
    domain TEXT NOT NULL,
    embedding BLOB NOT NULL,
    text_hash TEXT NOT NULL,
    model TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(day_date, domain)
);

-- =============================================================================
-- PART 2: Semantic Search
-- =============================================================================

CREATE TABLE IF NOT EXISTS search_embeddings (
    id TEXT PRIMARY KEY,
    ontology TEXT NOT NULL,
    record_id TEXT NOT NULL,
    text_hash TEXT NOT NULL,
    model TEXT NOT NULL,
    chunk_index INTEGER NOT NULL DEFAULT 0,
    title TEXT,
    preview TEXT,
    author TEXT,
    timestamp TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(ontology, record_id, chunk_index)
);

CREATE INDEX IF NOT EXISTS idx_search_embeddings_ontology
    ON search_embeddings(ontology);
CREATE INDEX IF NOT EXISTS idx_search_embeddings_timestamp
    ON search_embeddings(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_search_embeddings_record
    ON search_embeddings(ontology, record_id);

CREATE TABLE IF NOT EXISTS search_embedding_progress (
    ontology TEXT PRIMARY KEY,
    last_processed_id TEXT,
    last_processed_timestamp TEXT,
    total_embedded INTEGER NOT NULL DEFAULT 0,
    last_run_at TEXT NOT NULL DEFAULT (datetime('now'))
);

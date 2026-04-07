-- 037: Topic embedding cache for dayline topic novelty scoring
--
-- Stores per-topic embeddings (768-dim nomic-embed) so we don't re-embed
-- the same topic string every scoring run. Builds lazily — new topics
-- are embedded and inserted on first encounter.

CREATE TABLE IF NOT EXISTS dayline_topic_cache (
    topic TEXT PRIMARY KEY,
    embedding BLOB NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

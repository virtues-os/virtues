-- ---------------------------------------------------------------------------
-- The index learns what it is, instead of being told.
--
-- Embedding was hardcoded to one model in a way that made "bring your own"
-- a claim rather than a fact:
--
--   * `validate_native_dim` REJECTED any vector that wasn't 768-d, with the
--     message "check the sidecar's GGUF is EmbeddingGemma-300M". You could not
--     swap the model without editing Rust.
--   * The stored width was the constant 256 regardless of what the model
--     actually emitted.
--   * Query/document prompt prefixes defaulted to EmbeddingGemma's *format* —
--     so a different model silently got a foreign model's prompt glued to every
--     input.
--   * `search_embeddings.model` held the literal 'embeddinggemma', written by
--     the indexer and read by nobody.
--
-- The paradigm is one line: ASK THE MODEL, DON'T TELL IT. Probe the endpoint,
-- take the width it gives, record what produced it. Truncation (Matryoshka)
-- becomes an opt-in storage optimisation (`VIRTUES_EMBED_DIMS`), not a
-- hardcoded fact about one model.
--
-- But the index cannot go asking a sidecar every time Postgres opens: `virtues
-- migrate` and half the CLI must work with no embedder running at all. So the
-- geometry lives HERE, in the database, and the probe's job is to VERIFY it, not
-- to supply it.
--
--   fresh box  → index is empty → probe, record, size the column to it
--   every run  → probe, compare. Same → go. Different → the stored vectors are
--                in another geometry, cosine between them is meaningless, and
--                the only honest answer is `virtues reindex`.
--
-- This SUPERSEDES `search_bm25_stats`, which was already a singleton holding
-- facts about the index as a whole (n_docs, sum_len). Those reset on reindex
-- for exactly the same reason the geometry does — they describe one index — so
-- they belong in one row, not two tables.
-- ---------------------------------------------------------------------------

CREATE TABLE search_index_meta (
    singleton   BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),

    -- The GEOMETRY: what these vectors mean. NULL until the first embed, which
    -- is correct — an empty index has no geometry, and pretending otherwise is
    -- how you end up asserting a model you never ran.
    model       TEXT,
    dim         INTEGER,
    -- Two models can share a width and mean entirely different things by it. The
    -- fingerprint is the only thing that catches a same-width swap.
    fingerprint TEXT,
    built_at    TIMESTAMPTZ,

    -- The CORPUS: BM25's view of the same index (moved from search_bm25_stats).
    n_docs      BIGINT NOT NULL DEFAULT 0,
    sum_len     BIGINT NOT NULL DEFAULT 0
);

-- Carry the corpus stats across, then retire the old table.
INSERT INTO search_index_meta (singleton, n_docs, sum_len)
SELECT TRUE, COALESCE(n_docs, 0), COALESCE(sum_len, 0)
FROM search_bm25_stats
WHERE singleton
ON CONFLICT (singleton) DO NOTHING;

-- Cover the case where the old singleton was missing entirely.
INSERT INTO search_index_meta (singleton) VALUES (TRUE)
ON CONFLICT (singleton) DO NOTHING;

DROP TABLE search_bm25_stats;

-- Whatever is in the index right now was built by the old hardcoded path, whose
-- model column was a literal. We know its width (the vector column says so) but
-- not honestly what produced it, so the geometry stays NULL and the next embed
-- records the truth.

COMMENT ON TABLE search_index_meta IS
    'One row: what this search index IS. The geometry its vectors live in '
    '(model/dim/fingerprint) and the corpus statistics BM25 scores against. '
    'Everything here resets together on `virtues reindex`, because it all '
    'describes one derived artefact.';

COMMENT ON COLUMN search_index_meta.dim IS
    'The stored vector width, learned from the model — never a constant. The '
    'vector column is sized to this at bringup, with no network call.';

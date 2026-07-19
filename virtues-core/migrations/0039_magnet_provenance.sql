-- ---------------------------------------------------------------------------
-- The magnet: provenance on notebook members, and centroids as real vectors.
--
-- 1. PROVENANCE. 0038 gave `wiki_story_members` `added_by` and `similarity`.
--    Notebook members need the same two, for the same reason: the magnet is one
--    primitive serving both, and auto-attached material must be VISIBLY
--    auto-attached and one-click removable. The AI does not silently
--    restructure what a human declared — a thing you dragged in and a thing the
--    machine guessed at cannot look identical.
--
--    `similarity` is kept so the threshold can be re-tuned against what it
--    actually admitted. A threshold you cannot audit is a threshold you cannot
--    fix.
--
-- 2. CENTROIDS BECOME halfvec(256). They were BYTEA, on the `wiki_events`
--    precedent — but that precedent is about vectors we STORE and hand-compare
--    in Rust. A centroid is neither: it is the QUERY vector, handed straight to
--    Postgres to run against `search_vectors` (halfvec(256), HNSW,
--    halfvec_cosine_ops). Keeping it as a blob meant serialising f32s in Rust
--    and casting on every call, to hold a value whose only consumer is a
--    pgvector operator.
--
--    So: one vector type across the vector path. `<=>` compares halfvec to
--    halfvec with no cast, and the dimension is now checked by the database
--    rather than by a comment. Both columns are NULL everywhere (nothing has
--    ever written a centroid), so this converts no data.
-- ---------------------------------------------------------------------------

ALTER TABLE app_notebook_items
    ADD COLUMN added_by   TEXT NOT NULL DEFAULT 'user'
                          CHECK (added_by IN ('user', 'magnet')),
    ADD COLUMN similarity DOUBLE PRECISION;

-- Existing members were all dragged in by hand — which is what the default
-- says, and it is true: nothing has ever written 'magnet'.

CREATE INDEX idx_notebook_items_magnet
    ON app_notebook_items (notebook_id)
    WHERE added_by = 'magnet';

-- Centroids: BYTEA → halfvec(256), the same space and width as the corpus they
-- are matched against. USING NULL is honest — every value is already NULL, and
-- a wrong cast here would be a silently mis-typed vector, which is worse than
-- no vector.
ALTER TABLE app_notebooks
    ALTER COLUMN centroid TYPE halfvec(256) USING NULL;

ALTER TABLE wiki_stories
    ALTER COLUMN centroid TYPE halfvec(256) USING NULL;

COMMENT ON COLUMN app_notebooks.centroid IS
    'The magnet''s query vector: mean of the seed (name + instructions) and the '
    'members'' embeddings. Same space as search_vectors — halfvec(256).';

COMMENT ON COLUMN wiki_stories.centroid IS
    'The magnet''s query vector: mean of the seed (title + thesis) and the '
    'evidence already gathered. Same space as search_vectors — halfvec(256).';

-- Enforce id = {ontology}:{record_id}:{chunk_index} structurally, so the
-- disease 0085 repaired cannot recur.
--
-- The rule existed only as textual copies in each writer (two in the
-- indexer, three in 0085's own SQL), and "enforced by convention at every
-- writer" is how 0082's ontology rename left ids carrying the old name —
-- writers then saw those rows as never-indexed, re-inserted, and collided
-- with the primary key on every cron tick. Two moves end that:
--
-- 1. A BEFORE INSERT OR UPDATE trigger derives id from the triple, always.
--    A writer's supplied id is overwritten with the derived one; a writer
--    that drifts is corrected instead of colliding, and an ontology rename
--    is just `UPDATE search_embeddings SET ontology = 'new' WHERE ...` —
--    the trigger recomputes ids in the same statement.
--
-- 2. The child FKs gain ON UPDATE CASCADE, so when a rename recomputes
--    parent ids, search_vectors and search_bm25_postings follow in the
--    same statement instead of pinning the old ids (their lack of UPDATE
--    action is why 0085 had to drop and re-add them around its rewrite).
--
-- The Rust writers keep computing the same id (one shared fn now), so the
-- trigger is a no-op in the healthy case — it exists for the unhealthy one.

CREATE OR REPLACE FUNCTION search_embeddings_derive_id() RETURNS trigger AS $$
BEGIN
    NEW.id := NEW.ontology || ':' || NEW.record_id || ':' || NEW.chunk_index;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_search_embeddings_derive_id ON search_embeddings;
CREATE TRIGGER trg_search_embeddings_derive_id
    BEFORE INSERT OR UPDATE OF ontology, record_id, chunk_index, id
    ON search_embeddings
    FOR EACH ROW
    EXECUTE FUNCTION search_embeddings_derive_id();

ALTER TABLE search_vectors
    DROP CONSTRAINT search_vectors_embedding_id_fkey,
    ADD CONSTRAINT search_vectors_embedding_id_fkey
        FOREIGN KEY (embedding_id) REFERENCES search_embeddings(id)
        ON DELETE CASCADE ON UPDATE CASCADE;

ALTER TABLE search_bm25_postings
    DROP CONSTRAINT search_bm25_postings_chunk_id_fkey,
    ADD CONSTRAINT search_bm25_postings_chunk_id_fkey
        FOREIGN KEY (chunk_id) REFERENCES search_embeddings(id)
        ON DELETE CASCADE ON UPDATE CASCADE;

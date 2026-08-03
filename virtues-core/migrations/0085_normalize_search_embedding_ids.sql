-- Normalize search_embeddings.id to the one derivable form:
--
--     {ontology}:{record_id}:{chunk_index}
--
-- Why this exists: 0082 renamed ontology 'document_annotation' →
-- 'document_marginalia' on live index rows but left id carrying the old
-- prefix. The id is opaque to every reader, so nothing broke at read time —
-- but any writer still compiled with the old ontology name then found the
-- rows "never indexed" (freshness joins on ontology), re-inserted under the
-- old-style id, and hit search_embeddings_pkey: the id was taken while the
-- (ontology, record_id, chunk_index) arbiter triple was vacant. On dev this
-- had applet_embedding_index failing every cron tick.
--
-- The indexer's empty-record placeholder rows had the same disease from the
-- other direction: written as two-segment {ontology}:{record_id}, no chunk
-- suffix (code fixed alongside this migration). After this, one rule holds
-- for every row, and id = f(triple) is actually true rather than usually
-- true.
--
-- The child tables reference id with ON DELETE CASCADE but no ON UPDATE
-- action, so the parent id cannot change while children point at it. Drop
-- the constraints, rewrite all three tables from one mapping, re-add.

ALTER TABLE search_vectors       DROP CONSTRAINT search_vectors_embedding_id_fkey;
ALTER TABLE search_bm25_postings DROP CONSTRAINT search_bm25_postings_chunk_id_fkey;

WITH m AS (
    SELECT id AS old_id,
           ontology || ':' || record_id || ':' || chunk_index AS new_id
    FROM search_embeddings
    WHERE id <> ontology || ':' || record_id || ':' || chunk_index
)
UPDATE search_vectors sv
   SET embedding_id = m.new_id
  FROM m
 WHERE sv.embedding_id = m.old_id;

WITH m AS (
    SELECT id AS old_id,
           ontology || ':' || record_id || ':' || chunk_index AS new_id
    FROM search_embeddings
    WHERE id <> ontology || ':' || record_id || ':' || chunk_index
)
UPDATE search_bm25_postings sp
   SET chunk_id = m.new_id
  FROM m
 WHERE sp.chunk_id = m.old_id;

-- The triple is UNIQUE, and the new id is a pure function of the triple, so
-- this cannot collide: at most one row can want any given new id.
UPDATE search_embeddings
   SET id = ontology || ':' || record_id || ':' || chunk_index
 WHERE id <> ontology || ':' || record_id || ':' || chunk_index;

ALTER TABLE search_vectors
    ADD CONSTRAINT search_vectors_embedding_id_fkey
    FOREIGN KEY (embedding_id) REFERENCES search_embeddings(id) ON DELETE CASCADE;
ALTER TABLE search_bm25_postings
    ADD CONSTRAINT search_bm25_postings_chunk_id_fkey
    FOREIGN KEY (chunk_id) REFERENCES search_embeddings(id) ON DELETE CASCADE;

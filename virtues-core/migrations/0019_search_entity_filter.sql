-- 0019 — Entity-aware retrieval: filter search by resolved person/place/org.
--
-- The ER pipeline already populates wiki_entity_refs(entity_id, source_table,
-- source_id). To let a query like "what's Jake been up to" become a metadata
-- filter + semantic rank (far more reliable than dense similarity on a name),
-- query.rs joins search rows to wiki_entity_refs. That join needs the source
-- table on each search row (ontology name ≠ table name, and record_ids aren't
-- globally unique across tables), so we denormalize it here. Backfilled by
-- run_embedding_job (re-embeds after 0017).

ALTER TABLE search_embeddings ADD COLUMN IF NOT EXISTS source_table TEXT;

-- Supports the EXISTS join in query.rs (er.source_table, er.source_id).
CREATE INDEX IF NOT EXISTS idx_search_embeddings_source_table
    ON search_embeddings(source_table, record_id);

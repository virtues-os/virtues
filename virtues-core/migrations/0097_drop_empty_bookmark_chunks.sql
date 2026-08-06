-- ---------------------------------------------------------------------------
-- Remove bookmark chunks that have no text, and correct the corpus stats.
--
-- The `content_bookmark` descriptor now carries an `embed_where` excluding rows
-- with nothing to embed — a saved URL before enrichment has no title, no
-- description, no note and no extraction record, so its embed text is a couple
-- of newlines.
--
-- That guard stops NEW empty documents. It does not remove the ones already
-- indexed, and it cannot: the indexer only deletes stale chunks for records it
-- re-selects (`search/indexer.rs`), and a row outside `embed_where` is never
-- selected again. So its chunks would sit in `search_embeddings` forever —
-- and, worse, keep counting toward `search_index_meta.n_docs` / `sum_len`,
-- which is the BM25 corpus statistic every other query is scored against.
-- Leaving them would preserve exactly the harm the guard was added to prevent.
--
-- Deleting from `search_embeddings` cascades to `search_vectors` and
-- `search_bm25_postings`. The meta counters are decremented by what is actually
-- removed rather than recomputed, matching how the indexer accounts for its own
-- deletes.
--
-- NOTE for whoever adds the next `embed_where`: this class of orphan is general,
-- not specific to bookmarks. Narrowing any ontology's scope strands the rows
-- that fall out. A reconcile pass in the indexer would retire this pattern.
-- ---------------------------------------------------------------------------

WITH doomed AS (
    SELECT se.id, se.bm25_len
      FROM search_embeddings se
      JOIN data_content_bookmark t ON t.id = se.record_id
     WHERE se.ontology = 'content_bookmark'
       AND btrim(
             COALESCE(t.title, '')
             || COALESCE(t.description, '')
             || COALESCE(t.note, '')
             || COALESCE(t.extraction_text, '')
           ) = ''
),
removed AS (
    DELETE FROM search_embeddings
     WHERE id IN (SELECT id FROM doomed)
    RETURNING bm25_len
)
UPDATE search_index_meta
   SET n_docs  = GREATEST(n_docs  - (SELECT COUNT(*) FROM removed), 0),
       sum_len = GREATEST(sum_len - COALESCE((SELECT SUM(bm25_len) FROM removed), 0), 0)
 WHERE singleton;

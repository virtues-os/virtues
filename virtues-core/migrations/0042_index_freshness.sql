-- ---------------------------------------------------------------------------
-- The index could never go stale, because it could never be refreshed.
--
-- `search_embeddings.text_hash` has been computed and written on every chunk
-- since the beginning, and NOTHING HAS EVER READ IT. The indexer's backlog is:
--
--     LEFT JOIN search_embeddings se ON se.record_id = t.id
--     WHERE se.id IS NULL
--
-- One embedding exists ⇒ the record is "done" ⇒ it is never looked at again.
-- So:
--
--   * You edit a page in Virtues and search keeps answering with the version you
--     first wrote. Forever. That is a live bug today.
--   * A conversation that gains a message never re-embeds — which makes indexing
--     chats-as-documents impossible, since a chat's text is its messages and a
--     chat GROWS.
--
-- `text_hash` cannot fix this on its own: it hashes a CHUNK, and the question is
-- whether the DOCUMENT changed. So: `doc_hash`, the same value on every chunk of
-- a record, computed by Postgres (`md5(embed_text)`) so that the writer and the
-- freshness check can never disagree about what the text was.
--
-- `model` gets the same treatment. It is currently the literal 'embeddinggemma'
-- baked into an INSERT, read by nobody. The only real guard is dimensional, so a
-- BYO embedder of the SAME width silently corrupts the vector space — every new
-- vector lands in a different geometry than its neighbours and cosine between
-- them means nothing. Record what the embedder actually reports.
-- ---------------------------------------------------------------------------

ALTER TABLE search_embeddings
    ADD COLUMN doc_hash TEXT;

-- The freshness probe: "is this record's current text still what we indexed?"
-- reads chunk 0 only, so it must be cheap.
CREATE INDEX idx_search_embeddings_freshness
    ON search_embeddings (ontology, record_id)
    INCLUDE (doc_hash)
    WHERE chunk_index = 0;

-- Existing rows have no doc_hash, so they read as stale and re-embed once. That
-- is correct: they were indexed under the old unit (a chat TURN was a document),
-- and that unit is exactly what we are abandoning.

COMMENT ON COLUMN search_embeddings.doc_hash IS
    'md5 of the record''s full embed_text, identical on every chunk of the record. '
    'The staleness check — NULL or differing means re-embed. Computed in SQL so it '
    'cannot drift from the text it describes.';

COMMENT ON COLUMN search_embeddings.text_hash IS
    'sha256 (truncated) of THIS chunk. Diagnostic only — for staleness use doc_hash.';

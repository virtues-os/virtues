-- 0049_search_vectors_next
--
-- Search keeps working while the box moves to another embedding model.
--
-- A model change used to wipe the index and re-embed it in place
-- (`virtues reindex`), so search answered nothing until the rebuild finished:
-- hours on a CPU box. Chunks and their BM25 postings don't depend on the model,
-- only the vectors do. So the new model's vectors are built here, beside the
-- live ones, while search keeps using the old model; when every chunk has one,
-- `search::next_index` swaps this table with `search_vectors` in a single
-- transaction.
--
-- `embedding` is an untyped halfvec: the next model's width is not known until
-- its server answers. It is typed to that width when a build starts, and the
-- HNSW index is built just before the swap.

CREATE TABLE public.search_vectors_next (
    embedding_id text NOT NULL,
    embedding public.halfvec NOT NULL
)
WITH (autovacuum_vacuum_scale_factor='0.05');

ALTER TABLE ONLY public.search_vectors_next
    ADD CONSTRAINT search_vectors_next_pkey PRIMARY KEY (embedding_id);

ALTER TABLE ONLY public.search_vectors_next
    ADD CONSTRAINT search_vectors_next_embedding_id_fkey FOREIGN KEY (embedding_id)
    REFERENCES public.search_embeddings(id) ON UPDATE CASCADE ON DELETE CASCADE;

-- The model and width the next table is being built for; NULL when no model
-- change is under way.
ALTER TABLE public.search_index_meta
    ADD COLUMN next_model text,
    ADD COLUMN next_dim integer;

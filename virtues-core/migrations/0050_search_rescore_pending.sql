-- 0050_search_rescore_pending
--
-- A search model change finishes in two steps: the table swap (one short
-- transaction, search::next_index), then work that can't share its lock:
-- forgetting the old model's event embeddings, rescoring every day's events,
-- resizing project centroids. That second step can take minutes and can be cut
-- off (the indexer run hits its time limit, an update restarts the server). A
-- step cut off and never retried leaves every past day's event scores empty
-- for good. So the swap sets this flag in its own transaction, and it is
-- cleared only once that work is done; until then, every indexer run resumes it.

ALTER TABLE public.search_index_meta
    ADD COLUMN rescore_pending boolean NOT NULL DEFAULT false;

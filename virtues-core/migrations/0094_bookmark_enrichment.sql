-- ---------------------------------------------------------------------------
-- Bookmark enrichment queue and extraction record (docs/bookmarks-plan.md §3).
--
-- Ingest and enrichment are deliberately decoupled. A save writes a cheap
-- normalized row and returns; enrichment is a separate budgeted sweep that
-- fetches the page, composes an extraction record, and lets the row re-embed.
-- The reason is the bulk-import cliff: a browser bookmark file is thousands of
-- rows on day one, and enriching inline would turn one sync into a surprise
-- bill and an unbounded run.
--
-- `extraction` is DERIVED DATA and is treated as disposable — the originals
-- live at their URLs and in the lake, so the whole layer is re-runnable when
-- models improve. `enrichment_model` records what produced the current record
-- so a re-run is an explicit decision rather than a silent overwrite.
--
-- Note what is NOT here: nothing in this migration is user-authored. `note`
-- (0073) stays the only column a person writes, and no enrichment pass may
-- touch it. Machine text and user text stay segregated so the retrieval boost
-- on the user's own words remains a boost on the user's own words.
-- ---------------------------------------------------------------------------

ALTER TABLE data_content_bookmark
    ADD COLUMN enrichment_status       TEXT NOT NULL DEFAULT 'pending',
    ADD COLUMN enriched_at             TIMESTAMPTZ,
    ADD COLUMN enrichment_model        TEXT,
    ADD COLUMN enrichment_attempts     INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN enrichment_last_attempt TIMESTAMPTZ,
    ADD COLUMN extraction              JSONB;

-- 'pending'   — waiting for the sweep (the default, so existing rows enqueue)
-- 'enriching' — claimed by a run; a stale claim is recovered by age, never by
--               a lock, because the applet is a subprocess that can be killed
-- 'done'      — has an extraction record
-- 'failed'    — gave up after the attempt cap
-- 'skipped'   — deliberately not worth enriching (a scheme we do not fetch, a
--               content type this path does not read); distinct from 'failed'
--               so it is neither retried nor reported as an error
ALTER TABLE data_content_bookmark
    ADD CONSTRAINT data_content_bookmark_enrichment_status_check
    CHECK (enrichment_status IN ('pending', 'enriching', 'done', 'failed', 'skipped'));

-- The drain is newest-first: a bookmark saved today is worth more than one
-- imported from a 2014 folder, and on a big backfill the user should watch the
-- top of their list fill in. Partial, because 'done' rows are the overwhelming
-- majority at rest and are never queried through this path.
CREATE INDEX idx_content_bookmark_enrichment_queue
    ON data_content_bookmark (timestamp DESC)
    WHERE enrichment_status IN ('pending', 'enriching');

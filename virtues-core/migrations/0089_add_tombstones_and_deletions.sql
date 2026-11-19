-- Delete and tombstone: removing a record the owner does not want on their
-- server, so that it stays removed.
--
-- A `data_*` row carries `source_stream_id`, the id its source gave it, and
-- every sync upserts on that id. A row that was only deleted comes back the
-- next time the source sends it: a calendar resync, a bank update, a phone
-- retrying its queue, a rescan of the Mac's messages. So removal is two
-- writes in one transaction (`api::deletions`): a tombstone naming the
-- record, then the delete. The trigger below refuses any insert that matches
-- a tombstone, which covers every sync, shipped or authored, and any future
-- job that rebuilds tables from the raw files.
--
-- A tombstone has no content. It is the table and two ids.

-- One deletion the owner was asked to approve, or made in the table viewer.
-- The chat card reads it while it waits, and the receipt afterwards.
CREATE TABLE app_deletions (
    id          text PRIMARY KEY,

    -- The chat that proposed it. NULL for one made in the table viewer.
    -- Deleting the chat deletes this row; the tombstones stay.
    chat_id     text REFERENCES app_chats(id) ON DELETE CASCADE,

    -- What is being deleted, as the card says it: "Your messages with David
    -- Okafor". Written by the assistant, read by the owner.
    summary     text NOT NULL,

    -- Which rows: [{"table": "data_…", "where": "<SQL predicate>"}] or
    -- [{"table": "data_…", "ids": ["…"]}]. A predicate is resolved by the
    -- read-only reader login, when the card previews it and again when the
    -- owner allows it.
    targets     jsonb NOT NULL,

    status      text NOT NULL DEFAULT 'proposed'
                CHECK (status IN ('proposed', 'running', 'done', 'failed', 'cancelled')),

    -- What was removed, by place, and what was not reached. Counts and
    -- dates, never the content.
    receipt     jsonb NOT NULL DEFAULT '{}',

    -- Why it failed, when it did.
    error       text,

    created_at  timestamptz NOT NULL DEFAULT now(),
    -- When the owner allowed it.
    approved_at timestamptz,
    finished_at timestamptz
);

COMMENT ON TABLE app_deletions IS
    'A deletion of ingested records the owner was asked to approve: which rows, where it stands, and the receipt. Holds no record content.';

CREATE INDEX idx_app_deletions_chat ON app_deletions (chat_id) WHERE chat_id IS NOT NULL;

-- A record the owner deleted, which no sync may bring back.
--
-- `source_table` and `source_id` name the row the way `wiki_refs` and
-- `search_embeddings` do. `source_stream_id` is the id the source gave it,
-- which is what a sync sends again.
CREATE TABLE app_tombstones (
    source_table     text NOT NULL,
    source_stream_id text NOT NULL,
    source_id        text NOT NULL,

    -- The deletion that wrote it. NULL once that row is gone.
    deletion_id      text REFERENCES app_deletions(id) ON DELETE SET NULL,

    created_at       timestamptz NOT NULL DEFAULT now(),

    PRIMARY KEY (source_table, source_stream_id)
);

COMMENT ON TABLE app_tombstones IS
    'Records the owner deleted. A BEFORE INSERT trigger on each data_* table refuses a row that matches one, so a sync cannot bring it back. Removing a tombstone lets the next sync collect the record again.';

-- The trigger's second probe, and the lookup for lifting one row's tombstone.
CREATE INDEX idx_app_tombstones_row ON app_tombstones (source_table, source_id);

-- Refuse a row whose record has a tombstone. Returning NULL from a BEFORE
-- INSERT row trigger skips the row before `ON CONFLICT` is considered, so
-- `DO NOTHING` and `DO UPDATE` upserts are both covered, and the statement
-- carries on with its other rows.
--
-- SECURITY DEFINER with a fixed search_path: ingest must never fail because
-- the inserting role cannot read `app_tombstones`.
CREATE FUNCTION app_refuse_tombstoned() RETURNS trigger
    LANGUAGE plpgsql
    SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM public.app_tombstones
        WHERE source_table = TG_TABLE_NAME AND source_stream_id = NEW.source_stream_id
    ) OR EXISTS (
        SELECT 1 FROM public.app_tombstones
        WHERE source_table = TG_TABLE_NAME AND source_id = NEW.id
    ) THEN
        RETURN NULL;
    END IF;
    RETURN NEW;
END
$$;

-- Every data table that has both ids. `data_audio_session` has no
-- `source_stream_id`: it is rebuilt from recordings and follows them.
--
-- A data table added later needs this trigger too. The test
-- `every_data_table_refuses_tombstoned_rows` (api/deletions.rs) fails until
-- it has one.
DO $$
DECLARE
    t text;
BEGIN
    FOR t IN
        SELECT c.relname
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = 'public'
          AND c.relkind = 'r'
          AND c.relname LIKE 'data\_%'
          AND EXISTS (SELECT 1 FROM pg_attribute a
                      WHERE a.attrelid = c.oid AND a.attname = 'source_stream_id' AND NOT a.attisdropped)
          AND EXISTS (SELECT 1 FROM pg_attribute a
                      WHERE a.attrelid = c.oid AND a.attname = 'id' AND NOT a.attisdropped)
        ORDER BY c.relname
    LOOP
        EXECUTE format(
            'CREATE TRIGGER refuse_tombstoned BEFORE INSERT ON public.%I
             FOR EACH ROW EXECUTE FUNCTION public.app_refuse_tombstoned()',
            t
        );
    END LOOP;
END
$$;

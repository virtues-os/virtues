-- Drop the orphaned prose-ER pause triggers.
--
-- 0061 dropped `er_extraction_log`. The `er_prose_pause` triggers that write to
-- it were never created by any migration — they were applied by hand to a
-- running box on 2026-07-17 to pause prose extraction (the stamp string
-- 'paused-manual-2026-07-17' is baked into the function body). 0061 therefore
-- had no way to know they existed, and dropping the table under them turned
-- every stamped INSERT into:
--
--   ERROR: relation "er_extraction_log" does not exist
--
-- On the box that carried the hotfix that meant `data_communication_email`
-- (Gmail), `data_communication_message` (iMessages), `data_content_document`
-- (document ingest) and `app_chats` (starting a new chat) all began rejecting
-- every insert the moment 0061 ran — a data-ingest and chat outage created by
-- an upgrade, with the cause on the far side of a schema change from the
-- symptom.
--
-- Discovered by a `google_gmail_sync` run that finally got far enough to touch
-- the database, having been masked until then by an unrelated TLS panic.
--
-- Deliberately keyed on the FUNCTION rather than a fixed table list: the
-- triggers were hand-applied, so a given box may carry them on tables this
-- migration's author never saw. Idempotent — a box that never had the hotfix
-- drops nothing.
DO $$
DECLARE
    t record;
BEGIN
    FOR t IN
        SELECT c.relname AS table_name
        FROM pg_trigger tg
        JOIN pg_class c ON c.oid = tg.tgrelid
        JOIN pg_proc p ON p.oid = tg.tgfoid
        WHERE p.proname = 'er_prose_pause_stamp'
          AND NOT tg.tgisinternal
    LOOP
        EXECUTE format('DROP TRIGGER IF EXISTS er_prose_pause ON %I', t.table_name);
    END LOOP;
END $$;

DROP FUNCTION IF EXISTS er_prose_pause_stamp();

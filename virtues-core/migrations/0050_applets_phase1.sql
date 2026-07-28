-- 0050: Applets phase 1 (additive).
--
-- The applet contract (docs/applets-overhaul-plan.md): owner gains 'ai'
-- (chat-authored), lifecycle collapses into one nullable `until` field
-- (NULL = forever | 'once' = archive after first success | SQL bool =
-- archive when true, evaluated post-success), and `supervise` replaces the
-- service arm of the runtime taxonomy (command + supervise = service).
--
-- Deliberately additive: `runtime` and `dir` are still read by the runner,
-- scheduler, and UI. They get DROPped at the end of phase 1 once every
-- branch derives from fields instead.

ALTER TABLE app_actions DROP CONSTRAINT app_actions_owner_check;
ALTER TABLE app_actions ADD CONSTRAINT app_actions_owner_check
    CHECK (owner IN ('system', 'user', 'ai'));

ALTER TABLE app_actions ADD COLUMN until       TEXT;
ALTER TABLE app_actions ADD COLUMN archived_at TIMESTAMPTZ;
ALTER TABLE app_actions ADD COLUMN supervise   BOOLEAN NOT NULL DEFAULT FALSE;

-- Backfill: today's service-runtime rows are exactly the supervised ones.
UPDATE app_actions SET supervise = TRUE WHERE runtime = 'service';

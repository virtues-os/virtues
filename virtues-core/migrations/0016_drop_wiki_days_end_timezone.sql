-- Drop the vestigial wiki_days.end_timezone column.
--
-- The timezone model uses a single per-day zone ("the timezone you woke up in"),
-- stored on wiki_days.start_timezone. end_timezone was never written or read.
-- See docs/timezone-model.md.

ALTER TABLE wiki_days DROP COLUMN IF EXISTS end_timezone;

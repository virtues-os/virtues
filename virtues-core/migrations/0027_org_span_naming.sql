-- `wiki_orgs.start_date`/`end_date` — the last span in the schema still using
-- the old names.
--
-- The 2026-08-17/18 rename established one vocabulary for time: `occurred_at`
-- for an instant, `started_at`/`ended_at` for a span, `created_at`/`updated_at`
-- for when WE wrote the row. Twenty-one columns moved. The schema had grown
-- SEVEN names for "when this happened" and a configuration field to paper over
-- the confusion.
--
-- This pair was missed, and it stayed missed through migration 0023, which
-- renamed exactly these two names on `wiki_stories` for exactly this reason.
-- So the schema has carried one table speaking the old dialect ever since,
-- which is how a convention starts reading as a preference: every span in
-- `wiki_events`, `wiki_chapters` and `wiki_stories` says `started_at`, and a
-- reader who meets `wiki_orgs` concludes both spellings are fine.
--
-- **This breaks rollback for the release that carries it**, exactly as a drop
-- does: the previous binary selects `start_date` and will not find it. That is
-- affordable here only because migrations 0024 and 0025 are in the same
-- unreleased batch and have already spent that budget — a rename like this one
-- must NOT be slipped into a release whose rollback still works.

ALTER TABLE wiki_orgs RENAME COLUMN start_date TO started_at;
ALTER TABLE wiki_orgs RENAME COLUMN end_date   TO ended_at;

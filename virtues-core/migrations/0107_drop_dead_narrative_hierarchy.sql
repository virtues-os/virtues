-- ---------------------------------------------------------------------------
-- Drop the narrative hierarchy that was never built: telos → acts → chapters,
-- and the year summaries beside them.
--
-- Four tables, zero rows, and — the part that matters — ZERO WRITERS. Nothing
-- in the tree has ever inserted or updated any of them. What existed was the
-- read half: `api/wiki.rs` served them, `server/mod.rs` routed to them, the
-- frontend had types, converters and a live render branch, and
-- `tools/sql_query.rs` advertised all four to the SQL agent WITH JOIN HINTS.
--
-- So the box could be asked "what act of my life am I in" and would run a real
-- query against a permanently empty table and answer nothing, forever, with the
-- confidence of a system that believes it looked. That is worse than not having
-- the feature: an empty answer from a table that is supposed to hold something
-- is indistinguishable from an empty life.
--
-- This is the same failure class as `wiki_rules` — schema and read path landing
-- while the writer never did — and it is the largest instance of it. An audit
-- on 2026-08-17 found roughly twenty-five columns and five tables in that state.
-- The lesson being recorded here, because a migration outlives a plan document:
-- A COLUMN OR TABLE SHOULD NOT LAND UNTIL ITS WRITER LANDS. Designing in SQL
-- feels like progress and produces a schema that lies about what the product
-- does.
--
-- `wiki_stories` is deliberately KEPT despite being equally empty. It is the one
-- of the five with a live intent behind it, and `wiki_articles.subject_type`
-- still admits 'story'.
--
-- No data is lost, because there has never been any. Every statement is guarded
-- so a box that somehow lacks one of these converges silently rather than
-- aborting mid-upgrade.
-- ---------------------------------------------------------------------------

-- The FK columns first: `wiki_days` points at acts and chapters, and dropping
-- the parents while a child column still references them would fail.
ALTER TABLE IF EXISTS wiki_days DROP COLUMN IF EXISTS act_id;
ALTER TABLE IF EXISTS wiki_days DROP COLUMN IF EXISTS chapter_id;

-- Then the hierarchy, leaf to root: chapters → acts → telos.
DROP TABLE IF EXISTS wiki_chapters;
DROP TABLE IF EXISTS wiki_acts;
DROP TABLE IF EXISTS wiki_telos;

-- Unrelated to the hierarchy, same condition: read path, no writer.
DROP TABLE IF EXISTS wiki_years;

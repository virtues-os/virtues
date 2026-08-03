-- 0083 — Days become articles.
--
-- `wiki_days.autobiography` is the ONLY prose in the wiki with real data — 13
-- of 42 days on a real box — so this is the only part of the consolidation that
-- has to MOVE something rather than simply appear. Everything else (article and
-- content on people/places/orgs, narrative identity) measured empty.
--
-- Pure SQL is sufficient, which is not obvious. A page carries a Yjs document,
-- and constructing one in a migration would be absurd — but `get_or_create`
-- seeds `Y.Text` from the `content` column whenever `yjs_state` is NULL, so
-- inserting rows with content and no CRDT produces a correct document the first
-- time anyone opens it. No application code, no backfill job.
--
-- Three constraints make it safe:
--
--   1. The id is DERIVED from the day, not generated. `create_page` hashes a
--      timestamp, which SQL cannot reproduce, so a re-run would mint a second
--      page for the same day. Hashing the day id instead makes this migration
--      replayable — which matters because `_sqlx_migrations` rides inside the
--      pg_dump, so a backup restored across this boundary replays it.
--
--   2. `app_pages.date` is left NULL. The page ontology's day source filters on
--      that column with `use_date_filter: true`, so setting it would make each
--      day's article appear INSIDE that day as "you wrote a page today" — the
--      exact provenance failure the ontology split exists to prevent, arriving
--      through a different door. The day linkage lives on
--      `wiki_articles.subject_id` and nowhere else.
--
--   3. Nothing is UPDATED. The Yjs doc cache is in-memory, so rewriting
--      `app_pages.content` under a live editor would be clobbered by the next
--      debounced save. Inserting new rows cannot collide with a cache that has
--      never seen them.
--
-- `autobiography` is NOT dropped here. `day_summary_eod` still writes it and
-- seven readers still select it; the drop trails by a release, once the code
-- that stopped reading it is the version `virtues rollback` would land on.

INSERT INTO app_pages (id, title, content, kind)
SELECT
    'page_' || md5('day-article:' || d.id),
    to_char(d.date, 'FMDD Month YYYY'),
    d.autobiography,
    'article'
FROM wiki_days d
WHERE d.autobiography IS NOT NULL
  AND trim(d.autobiography) <> ''
ON CONFLICT (id) DO NOTHING;

INSERT INTO wiki_articles (id, subject_type, subject_id, page_id, source_ref_count, last_written_at)
SELECT
    'article_' || md5('day:' || d.id),
    'day',
    d.id,
    'page_' || md5('day-article:' || d.id),
    0,
    d.updated_at
FROM wiki_days d
WHERE d.autobiography IS NOT NULL
  AND trim(d.autobiography) <> ''
  -- The page insert above may have been skipped by ON CONFLICT on a replay;
  -- only claim a page that actually exists.
  AND EXISTS (SELECT 1 FROM app_pages p WHERE p.id = 'page_' || md5('day-article:' || d.id))
ON CONFLICT (subject_type, subject_id) DO NOTHING;

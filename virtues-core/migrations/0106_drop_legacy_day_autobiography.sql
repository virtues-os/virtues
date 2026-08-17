-- ---------------------------------------------------------------------------
-- 0106 — Finish 0083: a day's prose lives on its article page, and only there.
--
-- 0083 moved day prose onto article pages. 0087 added `wiki_day_prose`, which
-- reads the page first and falls back to `wiki_days.autobiography` "until its
-- drop". `narrate_day` has passed `autobiography: None` ever since. Everything
-- except the column itself was migrated; this is the drop that header promised.
--
-- WHY IT COULD NOT JUST SIT THERE. A half-finished migration is not inert. The
-- day page's inline editor still saved through `updateDay({ autobiography })`,
-- into the legacy column — while the view preferred the page. Every narrated day
-- has an article (`save_day_article` creates one, and 0083 backfilled the rest),
-- so the page always won and the user's typed prose was invisible the moment
-- they saved it. `last_edited_by: "user"` still fired, claiming the day and
-- stopping narration. They lost the text AND the narrator. The same shape as the
-- event-label deletion fixed alongside this: the human writes, the machine wins,
-- silently. Two homes for one thing is not untidiness, it is a place for edits
-- to fall.
--
-- The inline editor is deleted rather than repointed at the page. An article
-- page may carry a live Yjs document, and a plain `content` write under one is
-- discarded — the hazard `save_day_article` already guards with
-- `WHERE yjs_state IS NULL`. The page editor is CRDT-aware; a contenteditable in
-- a read view was never going to be. One editor, and it is that one.
--
-- NOTHING IS LOST. Every non-empty `autobiography` was copied to a page by 0083,
-- and the guard below refuses to drop if any row still holds prose the page
-- does not — so a checkout that somehow wrote the column after 0083 aborts here
-- loudly rather than deleting a day's writing.
-- ---------------------------------------------------------------------------

-- Refuse to run if any day's legacy prose is NOT already on its page.
DO $$
DECLARE stranded INT;
BEGIN
    SELECT count(*) INTO stranded
    FROM wiki_days d
    LEFT JOIN wiki_articles a ON a.subject_type = 'day' AND a.subject_id = d.id
    LEFT JOIN app_pages p ON p.id = a.page_id
    WHERE NULLIF(trim(d.autobiography), '') IS NOT NULL
      AND NULLIF(trim(p.content), '') IS DISTINCT FROM NULLIF(trim(d.autobiography), '');

    IF stranded > 0 THEN
        RAISE EXCEPTION
            'refusing to drop wiki_days.autobiography: % day(s) hold prose that is not on their article page. '
            'Copy it across before re-running (see 0083).', stranded;
    END IF;
END $$;

-- The view loses its fallback arm; the page is now the only source.
DROP VIEW IF EXISTS wiki_day_prose;
CREATE VIEW wiki_day_prose AS
SELECT
    d.id   AS day_id,
    d.date AS date,
    NULLIF(trim(p.content), '') AS prose
FROM wiki_days d
LEFT JOIN wiki_articles a ON a.subject_type = 'day' AND a.subject_id = d.id
LEFT JOIN app_pages p ON p.id = a.page_id;

COMMENT ON VIEW wiki_day_prose IS
    'A day''s prose, from its article page. The legacy wiki_days.autobiography '
    'fallback was dropped in 0106 — the page is the only home.';

ALTER TABLE wiki_days DROP COLUMN IF EXISTS autobiography;

-- `autobiography_sections` never had a writer: `narrate_day` has always passed
-- None, no other code path sets it, and the shape it describes (per-section
-- authorship) is what article pages and `wiki_notes` do properly. It goes with
-- the column it belonged to rather than being left as a field nothing fills.
ALTER TABLE wiki_days DROP COLUMN IF EXISTS autobiography_sections;

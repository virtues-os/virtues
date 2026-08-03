-- 0087 — One definition of "the day's prose".
--
-- 0083 moved the existing autobiographies into day article pages, but every
-- reader (day API, entity dossiers, NI dossier, chat context, CLI) still
-- selected `wiki_days.autobiography` — so the nightly writer kept feeding the
-- deprecated column and the article pages went stale from day one. Rather
-- than teach seven readers the same three-table join (and fix them one at a
-- time when it changes), the join is written once, here.
--
-- `prose` prefers the article page and falls back to the legacy column, so
-- days narrated before 0083 keep working until the autobiography drop lands.
-- The drop migration must recreate this view without the fallback — a view
-- holds a dependency on the column, so `DROP COLUMN` will refuse until then.
-- That refusal is a feature: the view makes the "did every reader move?"
-- question mechanical.

CREATE VIEW wiki_day_prose AS
SELECT
    d.id   AS day_id,
    d.date AS date,
    COALESCE(
        NULLIF(trim(p.content), ''),
        NULLIF(trim(d.autobiography), '')
    ) AS prose
FROM wiki_days d
LEFT JOIN wiki_articles a ON a.subject_type = 'day' AND a.subject_id = d.id
LEFT JOIN app_pages p ON p.id = a.page_id;

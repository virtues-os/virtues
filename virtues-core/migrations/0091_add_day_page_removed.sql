-- 0091_add_day_page_removed
--
-- A person can remove a day's written page. The day keeps its records, and
-- the day writer must not write the page again unless the person asks.
--
-- `wiki_days.page_removed_at` is when they removed it. It stays set after the
-- trash purges the page 30 days later, which is what stops the writer then:
-- with the article row gone, nothing else says the page was ever removed.
-- Putting the page back or asking for it to be written again clears it.
ALTER TABLE wiki_days
    ADD COLUMN page_removed_at timestamp with time zone;

-- A day's prose is the text of its article page. A page in the trash has no
-- text to show: without the `deleted_at` test a removed page kept reading as
-- the day's account in the day view, in search and in later days' pages.
CREATE OR REPLACE VIEW wiki_day_prose AS
 SELECT d.id AS day_id,
    d.date,
    NULLIF(TRIM(BOTH FROM p.content), ''::text) AS prose
   FROM ((wiki_days d
     LEFT JOIN wiki_articles a ON (((a.subject_type = 'day'::text) AND (a.subject_id = d.id))))
     LEFT JOIN app_pages p ON (((p.id = a.page_id) AND (p.deleted_at IS NULL))));

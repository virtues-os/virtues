-- A story becomes a subject the person names.
--
-- Not a span (corrected 2026-09-15). The examples are "Piano & Composition",
-- "Books Written", "My relationship with animals & pets", "How I learned to
-- pray" — none of which is a stretch of time. A story MAY have dates and
-- usually will not, which is what separates it from a chapter (an era, always
-- dated, always contiguous with its neighbours) and from a year.
--
-- This is also the rung that proves the editor has to be agentic. A story has
-- nothing beneath it: no day list, no ref set, no partition. The only way to
-- write "Piano & Composition" is to search the record for it.
--
-- `wiki_stories` has had zero readers and zero writers since the day it was
-- created, so this reshapes rather than migrates: there is nothing in it on
-- any box, and no old binary reads the columns being renamed.

-- The span columns take the names the schema uses everywhere else. A span is
-- `started_at`/`ended_at` (see the column-naming rules); `start_date`/`end_date`
-- was a third spelling of a thing already spelled twice.
ALTER TABLE wiki_stories RENAME COLUMN start_date TO started_at;
ALTER TABLE wiki_stories RENAME COLUMN end_date TO ended_at;

-- Dates are optional AND vague, exactly as a chapter's are. "Sometime in the
-- nineties" is a real answer about when you learned to pray, and forcing a
-- full date would record a lie. Same idiom as wiki_chapters, so the two read
-- the same way wherever they are shown together.
ALTER TABLE wiki_stories ADD COLUMN started_precision text
    CONSTRAINT wiki_stories_started_precision_check
    CHECK (started_precision IS NULL OR started_precision IN ('year', 'month', 'day'));
ALTER TABLE wiki_stories ADD COLUMN ended_precision text
    CONSTRAINT wiki_stories_ended_precision_check
    CHECK (ended_precision IS NULL OR ended_precision IN ('year', 'month', 'day'));

-- A precision without a date says nothing; a date without one is the common
-- case. Only the first is refused. (wiki_people uses this shape for death.)
ALTER TABLE wiki_stories ADD CONSTRAINT wiki_stories_started_shape_check
    CHECK (started_precision IS NULL OR started_at IS NOT NULL);
ALTER TABLE wiki_stories ADD CONSTRAINT wiki_stories_ended_shape_check
    CHECK (ended_precision IS NULL OR ended_at IS NOT NULL);

-- Theirs, verbatim — the one sentence about what this story is. Same column,
-- same meaning and same rules as `wiki_chapters.summary` and
-- `wiki_years.summary`: placed by the editor, never paraphrased.
ALTER TABLE wiki_stories ADD COLUMN summary text;

-- A title is the whole of a story until someone writes more, so it may not be
-- blank. (The table has no rows anywhere, so this cannot fail on upgrade.)
ALTER TABLE wiki_stories ADD CONSTRAINT wiki_stories_title_check
    CHECK (title <> '');

-- `content`, `subtitle`, `themes`, `metadata`, `cover_image` and `sort_order`
-- are not dropped here. The prose lives in the article's page like every other
-- subject's, and the rest never had a writer — but a dropped column stops the
-- previous binary booting, which takes `virtues upgrade`'s rollback with it.
-- Every drop waits for one terminal migration after boxes have rolled.

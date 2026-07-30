-- 0075 — Stories: a themed article that spans time.
--
-- A story is what you'd title "the story of my wedding", "the story of how I
-- started the company", "the story of my sobriety". It gathers a stretch of
-- life around a THEME rather than a date range, and it is explicitly NOT the
-- act/chapter spine: acts are ordered and contiguous (Telos → Acts → Chapters
-- → Days, each with a start and an end that tile the timeline), whereas
-- stories overlap freely, skip years, and answer "what was that about" rather
-- than "when was that".
--
-- Not to be confused with the `wiki_stories` that migration 0060 dropped.
-- That was the significance/claim model — `wiki_story_members` with evidence
-- and corrigibility columns, a machine trying to decide what mattered. This
-- one is the opposite: a page a person writes on purpose. Same name, because
-- it is the right word and the old meaning is gone.
--
-- Deliberately minimal for now: a generic article, hand-authored, with no
-- pipeline that creates or maintains one. Membership (which days, entities or
-- records belong to a story) is a later question and gets its own table when
-- there is something that needs it — adding an empty join table now would
-- only pre-commit the answer.

CREATE TABLE wiki_stories (
    id           TEXT PRIMARY KEY,
    title        TEXT NOT NULL,
    subtitle     TEXT,
    -- The article itself. Markdown, same as the rest of the wiki's prose.
    content      TEXT,
    -- Optional and soft: a story may be "the year I was 19" or may be
    -- undateable. Never used for ordering — `sort_order` is the spine.
    start_date   DATE,
    end_date     DATE,
    sort_order   INTEGER NOT NULL DEFAULT 0,
    themes       JSONB NOT NULL DEFAULT '[]'::jsonb,
    metadata     JSONB NOT NULL DEFAULT '{}'::jsonb,
    cover_image  TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- The index list is ordered by hand, then by recency for anything unsorted.
CREATE INDEX idx_wiki_stories_order ON wiki_stories (sort_order, created_at DESC);

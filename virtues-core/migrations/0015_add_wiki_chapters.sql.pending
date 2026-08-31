-- Chapters — the person's own partition of their life into named eras.
--
-- The ONE structured thing the narrative interview produces. Everything else
-- it gathers is testimony, arranged later into prose; this is a coordinate
-- system. Its job is that every timestamp the box holds folds into exactly one
-- named era, so "which chapter was that in?" is a question with an answer.
--
-- SHAPED AFTER wiki_events, deliberately. A day is already partitioned by the
-- nightly pass into a gapless, non-overlapping sequence, and it keeps itself
-- gapless by MATERIALISING the holes (kind='unknown') rather than by allowing
-- them. A life is the same contract at a different scale, so it gets the same
-- treatment rather than a second idiom.
--
-- WHERE IT INVERTS: an event is inferred from evidence and then corrected by
-- the person (hence auto_label beside user_label). A chapter is AUTHORED and
-- never inferred — there is only their label. No boundary may be derived from
-- a detected move, a job change in a mailbox, or a cluster of locations. That
-- doctrine is why narrative_identity_gen was deleted, and why the earlier
-- machine-set wiki_days.chapter_id was dropped in the 2026-08-18 squash.
--
-- NOT STORED, BECAUSE DERIVED: a day's chapter. It is a range lookup
-- (started_at <= date ORDER BY started_at DESC LIMIT 1), always correct, and
-- it survives the person moving a boundary. A stored foreign key would need a
-- backfill on every edit, and a missed backfill is invisible drift — which is
-- exactly how the old chapter_id column came to serialize a permanent None
-- for months.
CREATE TABLE wiki_chapters (
    id   text PRIMARY KEY,
    kind text NOT NULL DEFAULT 'chapter'
        CONSTRAINT wiki_chapters_kind_check CHECK (kind IN ('chapter', 'unknown')),

    -- Theirs, never ours. NULL only for an 'unknown' stretch — the years
    -- someone declines to name are still part of the shape of a life, and a
    -- hole in the partition would break every fold that crosses it.
    title text
        CONSTRAINT wiki_chapters_title_check
        CHECK ((kind = 'unknown') OR (title IS NOT NULL AND title <> '')),

    -- A span, both ends stored (as wiki_events does). ended_at NULL is the
    -- one deviation from events and it is the honest one: a day is over when
    -- the nightly pass runs, and a life is not.
    started_at date NOT NULL,
    ended_at   date,
    is_current boolean GENERATED ALWAYS AS (ended_at IS NULL) STORED NOT NULL,

    -- "Till about 2015" is the normal answer, not a degraded one. Forcing a
    -- real date would record a lie; dropping the date would lose the ordering.
    -- A coarse edge is drawn soft, so the uncertainty is visible.
    started_precision text NOT NULL DEFAULT 'year'
        CONSTRAINT wiki_chapters_started_precision_check
        CHECK (started_precision IN ('year', 'month', 'day')),
    ended_precision text
        CONSTRAINT wiki_chapters_ended_precision_check
        CHECK (ended_precision IS NULL OR ended_precision IN ('year', 'month', 'day')),

    -- What ended it, in their words. The highest-information field here: the
    -- interview asks for the changepoint precisely because it says more about
    -- a life than the era's name does.
    changepoint text,
    -- Their own sentence about the era, verbatim. Arranging is the drafter's
    -- job; this table never paraphrases.
    summary text,

    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,

    CONSTRAINT wiki_chapters_span_check CHECK (ended_at IS NULL OR ended_at > started_at),

    -- No overlap, enforced by the database. Contiguity stays the writer's job
    -- (as it is for events) because no constraint can express "and no gaps",
    -- but two eras claiming the same week is a corruption the schema can
    -- refuse outright.
    CONSTRAINT wiki_chapters_no_overlap EXCLUDE USING gist (
        daterange(started_at, COALESCE(ended_at, 'infinity'::date), '[)') WITH &&
    )
);

CREATE INDEX idx_wiki_chapters_started ON wiki_chapters (started_at);

CREATE TRIGGER set_updated_at BEFORE UPDATE ON wiki_chapters
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

-- A chapter is an ENTITY, not a label: it gets a real page, with prose,
-- history and marginalia, so "the Wisconsin years" is somewhere you can open
-- and write in rather than a band on a drawing. ('chapter' is already a valid
-- ref prefix in parse_entity_id, so refs and citations resolve the moment
-- rows exist.)
ALTER TABLE wiki_articles DROP CONSTRAINT wiki_articles_subject_type_check;
ALTER TABLE wiki_articles ADD CONSTRAINT wiki_articles_subject_type_check
    CHECK (subject_type = ANY (ARRAY[
        'person'::text, 'place'::text, 'organization'::text, 'day'::text,
        'story'::text, 'narrative_identity'::text, 'chapter'::text
    ]));

ALTER TABLE wiki_notes DROP CONSTRAINT wiki_notes_subject_type_check;
ALTER TABLE wiki_notes ADD CONSTRAINT wiki_notes_subject_type_check
    CHECK (subject_type = ANY (ARRAY[
        'event'::text, 'day'::text, 'story'::text, 'person'::text,
        'place'::text, 'organization'::text, 'chat'::text, 'page'::text,
        'narrative_identity'::text, 'chapter'::text
    ]));

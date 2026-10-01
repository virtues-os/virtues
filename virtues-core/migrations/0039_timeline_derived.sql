-- 0039_timeline_derived
--
-- The Timeline's derived layer: stays, drives, signal gaps, nights and the
-- moments inside them, rebuilt from the raw record by
-- `maintenance::timeline_builder` on the fast clock.
--
-- Every row here is derived and is replaced whole on each rebuild, in one
-- transaction; nothing else writes these tables, so they can be emptied and
-- rebuilt at any time.

-- A place the Timeline found by stopping there: a raw-GPS stop, or stops
-- within 80 m of each other, centred on them. Its link to the wiki's own place
-- row (the one whose radius holds its centre) is where a name comes from.
-- `stop_count` counts its stops; each stop also times one stay.
CREATE TABLE IF NOT EXISTS wiki_timeline_places (
    id                 text PRIMARY KEY,
    latitude           double precision NOT NULL,
    longitude          double precision NOT NULL,
    stop_count         integer NOT NULL,
    dwell_minutes      integer NOT NULL,
    overnight_minutes  integer NOT NULL,
    is_home            boolean NOT NULL DEFAULT false,
    is_work            boolean NOT NULL DEFAULT false,
    place_id           text REFERENCES wiki_places(id) ON DELETE SET NULL,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now()
);

-- A stretch of the day, in the day's four kinds. `stay`, `transit` and
-- `unknown` (a signal gap) tile the located record without overlapping;
-- `sleep` rows are the nights, and overlap the stay they were slept in.
CREATE TABLE IF NOT EXISTS wiki_timeline_spans (
    id                 text PRIMARY KEY,
    kind               text NOT NULL CHECK (kind IN ('stay', 'transit', 'sleep', 'unknown')),
    started_at         timestamptz NOT NULL,
    ended_at           timestamptz NOT NULL,
    timeline_place_id  text REFERENCES wiki_timeline_places(id) ON DELETE CASCADE,
    metadata           jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    CHECK (ended_at >= started_at)
);
CREATE INDEX IF NOT EXISTS idx_wiki_timeline_spans_time ON wiki_timeline_spans (started_at, ended_at);

-- What happened inside the stretches: a conversation (transcription windows
-- joined within one stay) or a walk inside a stay.
CREATE TABLE IF NOT EXISTS wiki_timeline_moments (
    id                 text PRIMARY KEY,
    kind               text NOT NULL CHECK (kind IN ('conversation', 'walk')),
    started_at         timestamptz NOT NULL,
    ended_at           timestamptz NOT NULL,
    title              text,
    metadata           jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    CHECK (ended_at >= started_at)
);
CREATE INDEX IF NOT EXISTS idx_wiki_timeline_moments_time ON wiki_timeline_moments (started_at, ended_at);

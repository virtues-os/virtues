-- Calorie Tracker, version 1.
--
-- Numbered and append-only: a later change ships as 0002 with an ALTER, never
-- as an edit to this file. CREATE TABLE IF NOT EXISTS on a table that already
-- exists does nothing, so rewriting this one would apply cleanly and change
-- nothing at all.

CREATE SCHEMA IF NOT EXISTS applet_calorie_tracker;

CREATE TABLE IF NOT EXISTS applet_calorie_tracker.entries (
    id          TEXT PRIMARY KEY,

    -- When they ate it, which is not when they told you. Defaults to now
    -- because most logging is immediate, but a person can say "at lunch".
    eaten_at    TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- What they actually sent, verbatim. This is the provenance of the row:
    -- the only way to check the parse later, and the only field in the table
    -- that is certainly true.
    said        TEXT NOT NULL,

    -- What it was parsed into. One row per item, so "eggs and toast" is two.
    item        TEXT NOT NULL,
    quantity    TEXT,

    -- NULLABLE, and that is the point. Unknown is not zero. A row that admits
    -- it does not know the calories is honest and still useful; a row with an
    -- invented number quietly corrupts every total computed after it. The same
    -- lesson as a cost column where 0 meant "we never looked".
    kcal        INTEGER,
    protein_g   INTEGER,

    -- Whose number is it. 'stated' means the user gave it; 'estimated' means a
    -- model guessed. A tracker that mixes the two into one clean total is
    -- telling you something it does not know, so the face reports the split
    -- rather than hiding it.
    confidence  TEXT NOT NULL DEFAULT 'estimated'
                CHECK (confidence IN ('stated', 'estimated')),

    logged_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- The face asks one question — what did I eat, most recently first — and the
-- corrections path asks the same one to find the last row.
CREATE INDEX IF NOT EXISTS entries_eaten_at
    ON applet_calorie_tracker.entries (eaten_at DESC);

-- Daily Readings, version 1.
--
-- Reference data, not the user's life, so it lives in the applet's own schema
-- rather than in a `data_*` ontology table. Cached rather than fetched per
-- use: the lectionary is deterministic, a month is knowable in advance, and
-- one request a month is both less exposure to someone else's HTML changing
-- and less of a daily record of religious practice sitting with a third party.

CREATE SCHEMA IF NOT EXISTS applet_daily_readings;

CREATE TABLE IF NOT EXISTS applet_daily_readings.readings (
    -- The liturgical day this belongs to, in the box's local calendar. One row
    -- per day per reading slot.
    day          DATE NOT NULL,

    -- "Reading 1", "Responsorial Psalm", "Alleluia", "Gospel", and on Sundays
    -- "Reading 2". Kept as the source labels them rather than normalised into
    -- an enum: the lectionary has more shapes than an enum would survive, and
    -- a label we do not recognise should pass through rather than be dropped.
    slot         TEXT NOT NULL,
    slot_order   INTEGER NOT NULL,

    -- "Matthew 16:24-28". A citation is a fact and cannot be wrong for
    -- copyright reasons; it is also what makes a row useful when the body is
    -- missing.
    citation     TEXT,

    -- The reading itself. NULLABLE on purpose: a fetch that got the citation
    -- but not the text is a partial success worth keeping, and an empty string
    -- would read as "this reading is blank" rather than "we did not get it".
    body         TEXT,

    -- Lectionary number, when the source gives one. Useful for checking a day
    -- against a printed missal.
    lectionary   TEXT,

    fetched_at   TIMESTAMPTZ NOT NULL DEFAULT now(),

    PRIMARY KEY (day, slot)
);

-- The only question anyone asks: what are today's readings, in order.
CREATE INDEX IF NOT EXISTS readings_day ON applet_daily_readings.readings (day, slot_order);

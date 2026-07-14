-- ---------------------------------------------------------------------------
-- Honest app usage + presence (docs/mac-presence-plan.md)
--
-- data_activity_app_usage was close to an INVERSION of the truth. The collector
-- emits events only on change (focus at 12:00, unfocus at 12:40), and the box
-- sessionized within a single 5-minute upload batch. So a real 40-minute session
-- put its focus event in one batch (start == end → dropped by the <1s filter) and
-- its unfocus in another 40 minutes later (also dropped): a deep-work session
-- recorded NOTHING. Meanwhile backlog batches — collector restarts, upload
-- backoff, sleep/wake — delivered hours of events at once, and consecutive
-- same-app runs got merged into fabricated spans.
--
-- Measured before this migration: 326 of 429 recorded hours came from sessions
-- LONGER than the upload interval, which steady-state collection cannot produce.
-- 76% of the hours were artifacts. The box's most-used "app" was the lock screen.
--
-- The fix is structural, not a filter:
--
--   1. Sessions are opened and closed STATEFULLY, against this table, so a
--      session can span any number of batches. `is_open` is that state.
--   2. Absence is recorded rather than inferred. data_activity_presence holds
--      active | watching | idle | locked | asleep. `loginwindow` stops arriving as
--      an app you used and becomes what it always was: locked time. Kept in full —
--      it is the only signal that you were away.
--   3. `device_id` scopes a session. Sessions are held open across batches, so
--      two Macs would otherwise close each other's.
--
-- Pre-launch, so the old rows are simply deleted rather than migrated: they can't
-- be repaired (the raw focus events behind them were aggregated away at ingest and
-- no longer exist) and 76% of them are fiction.
-- ---------------------------------------------------------------------------

-- 1. Sessions gain state ------------------------------------------------------

ALTER TABLE data_activity_app_usage
    ADD COLUMN device_id  TEXT,
    -- An open session's end_time is provisional: it holds the last moment we know
    -- the app was still focused (its last heartbeat), and moves forward until the
    -- session actually closes. end_time is NOT NULL, so "open" needs its own flag.
    ADD COLUMN is_open    BOOLEAN NOT NULL DEFAULT false,
    -- How the session ended — switch | quit | idle | watch | lock | sleep | stale.
    -- Makes it possible to ask "how often do I get interrupted" and to spot a
    -- sessionizer that is mis-closing.
    ADD COLUMN closed_by  TEXT;

-- The sessionizer's hot path: "the open session for this device + app".
CREATE INDEX idx_activity_app_usage_open
    ON data_activity_app_usage (device_id, app_bundle_id)
    WHERE is_open;

-- 2. Presence: where the human was --------------------------------------------

CREATE TABLE data_activity_presence (
    id                   TEXT PRIMARY KEY,
    source_connection_id TEXT,

    device_id            TEXT,
    state                TEXT NOT NULL CHECK (state IN ('active', 'watching', 'idle', 'locked', 'asleep')),

    started_at           TIMESTAMPTZ NOT NULL,
    ended_at             TIMESTAMPTZ NOT NULL,
    is_open              BOOLEAN NOT NULL DEFAULT false,

    source_stream_id     TEXT NOT NULL UNIQUE,
    source_table         TEXT NOT NULL,
    source_provider      TEXT NOT NULL,
    deleted_at_source    TIMESTAMPTZ,
    is_archived          BOOLEAN NOT NULL DEFAULT false,
    metadata             JSONB NOT NULL DEFAULT '{}',
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_activity_presence_start ON data_activity_presence (started_at DESC);
CREATE INDEX idx_activity_presence_open
    ON data_activity_presence (device_id)
    WHERE is_open;

-- 3. Drop the fiction ---------------------------------------------------------

DELETE FROM data_activity_app_usage WHERE source_provider = 'mac';

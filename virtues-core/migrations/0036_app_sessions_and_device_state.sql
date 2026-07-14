-- ---------------------------------------------------------------------------
-- Honest app usage: attended sessions + device state (docs/mac-presence-plan.md)
--
-- data_activity_app_usage was close to an INVERSION of the truth. The collector
-- emits events only on change (focus at 12:00, unfocus at 12:40) and the box
-- sessionized WITHIN a single 5-minute upload batch — so a real 40-minute session
-- put its focus in one batch (start == end → dropped as noise) and its unfocus in
-- a batch 40 minutes later (also dropped). A deep-work session recorded NOTHING.
-- Meanwhile backlog batches (collector restarts, upload backoff, sleep/wake)
-- delivered hours of events at once and the consecutive-run merge fabricated
-- enormous spans from them.
--
-- Measured: 326 of 429 recorded hours came from sessions LONGER than the upload
-- interval, which steady-state collection is structurally incapable of producing.
-- 76% of the hours were fiction, and the box's most-used "app" was the lock
-- screen at 211 hours — which was the user asleep.
--
-- TWO TABLES, because these are two different things:
--
--   data_activity_app_session   things you DID.  Attended, focused app time.
--                               Never has a NULL app. Sits beside web_browsing
--                               and listening. Sum it and you get app time — and
--                               ONLY app time.
--
--   data_activity_device_state  what the MACHINE observed. Not a claim about you.
--                               Tiles the timeline, so a gap means "the collector
--                               wasn't running" — unknown, and honestly so.
--
-- The state is `suspended`, NOT "asleep". A Mac cannot observe human sleep; it can
-- only observe that its lid closed. Human sleep already has a table
-- (data_health_sleep) fed by a device that can actually measure it. Had the Mac
-- been allowed to emit "asleep", closing the lid at lunch would have told the
-- narrative engine you took a nap. Every human fact — were you there, were you
-- working, were you sleeping — is an INFERENCE to be made downstream by fusing
-- devices, with the evidence still present to overrule it. A column that has
-- already declared "asleep" can never be taken back.
--
-- Pre-launch, so the fabricated rows are deleted rather than migrated: they cannot
-- be repaired (the raw focus events behind them were aggregated away at ingest and
-- no longer exist).
-- ---------------------------------------------------------------------------

-- 1. app_usage → app_session: attended time in an app --------------------------

ALTER TABLE data_activity_app_usage RENAME TO data_activity_app_session;
ALTER INDEX idx_activity_app_usage_start RENAME TO idx_activity_app_session_start;

ALTER TABLE data_activity_app_session
    ADD COLUMN device_id TEXT,
    -- active = you were typing/clicking. watching = no input, but the focused app
    -- is holding the display awake — a video, a call. Watching IS attention; it
    -- just isn't typing, and a naive idle check would have deleted it.
    ADD COLUMN attention TEXT NOT NULL DEFAULT 'active'
        CHECK (attention IN ('active', 'watching')),
    -- An open session's end_time is PROVISIONAL: it holds the last moment we know
    -- the app was still focused (its last heartbeat) and walks forward until the
    -- session closes. end_time is NOT NULL, so "open" needs its own flag.
    ADD COLUMN is_open BOOLEAN NOT NULL DEFAULT false,
    -- switch | quit | idle | lock | suspend | stale. Makes "how often am I
    -- interrupted" answerable, and makes a mis-closing sessionizer visible.
    ADD COLUMN closed_by TEXT;

-- The sessionizer's hot path: "the open session for this device".
CREATE INDEX idx_activity_app_session_open
    ON data_activity_app_session (device_id, app_bundle_id)
    WHERE is_open;

-- 2. Device state: what the machine saw ---------------------------------------

CREATE TABLE data_activity_device_state (
    id                   TEXT PRIMARY KEY,
    source_connection_id TEXT,

    device_id            TEXT,
    state                TEXT NOT NULL
        CHECK (state IN ('active', 'watching', 'idle', 'locked', 'suspended')),

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

CREATE INDEX idx_activity_device_state_start ON data_activity_device_state (started_at DESC);
CREATE INDEX idx_activity_device_state_open
    ON data_activity_device_state (device_id)
    WHERE is_open;

-- 3. Drop the fiction ---------------------------------------------------------

DELETE FROM data_activity_app_session WHERE source_provider = 'mac';

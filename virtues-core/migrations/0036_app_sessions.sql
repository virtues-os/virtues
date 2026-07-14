-- ---------------------------------------------------------------------------
-- Honest app usage: attended sessions (docs/mac-presence-plan.md)
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
-- 76% of the hours were fiction, and the box's most-used "app" was the lock screen
-- at 211 hours — which was the user asleep.
--
-- ONE table, and it holds only what it says: attended time in an app.
--
--   * A session is opened by focus, kept alive by heartbeats, and CLOSED by the
--     device event that ended it — a switch, a lock, going idle, the lid closing.
--     `closed_by` records which, so the gap AFTER a session already explains
--     itself ("closed_by = lock" → you locked the screen and came back later).
--     There is no separate presence table: the reason you stopped lives on the
--     session, and the raw device events are archived in the lake if we ever want
--     a full attention timeline. Derive it then; don't model it now.
--
--   * `attention` distinguishes typing from watching. A 40-minute video produces
--     no input, and a naive idle check would call it "away" and delete it — so an
--     app holding the display awake (scoped to the FOCUSED app's pid, because
--     builds and screen-sharing hold that assertion too) counts as attention.
--
--   * `loginwindow` is NOT an app. It is the machine saying nobody is there.
--     Recording it as usage is how the lock screen became the most-used
--     application on this box.
--
-- Pre-launch, so the fabricated rows are deleted rather than migrated: they cannot
-- be repaired (the raw focus events behind them were aggregated away at ingest and
-- no longer exist).
-- ---------------------------------------------------------------------------

ALTER TABLE data_activity_app_usage RENAME TO data_activity_app_session;
ALTER INDEX idx_activity_app_usage_start RENAME TO idx_activity_app_session_start;

ALTER TABLE data_activity_app_session
    -- Sessions are held OPEN across batches, so the sessionizer must know whose
    -- machine this is: two Macs would otherwise close each other's sessions.
    ADD COLUMN device_id TEXT,

    -- active = typing/clicking. watching = no input, but the focused app is
    -- holding the display awake (a video, a call). Both are usage.
    ADD COLUMN attention TEXT NOT NULL DEFAULT 'active'
        CHECK (attention IN ('active', 'watching')),

    -- An open session's end_time is PROVISIONAL: it holds the last moment we know
    -- the app was still focused (its last heartbeat) and walks forward until the
    -- session closes. end_time is NOT NULL, so "open" needs its own flag.
    ADD COLUMN is_open BOOLEAN NOT NULL DEFAULT false,

    -- switch | quit | idle | lock | suspend | stale.
    --
    -- This is what makes a second table unnecessary: it explains the gap that
    -- follows. `stale` in particular means the collector DIED mid-session (a
    -- crash, a power cut, an update swapping the binary) — so a gap can be told
    -- apart from "you walked away", which was the one honesty property worth
    -- protecting.
    --
    -- `suspend` means the MACHINE slept. It says nothing about whether you did —
    -- human sleep is data_health_sleep, from a watch that can actually observe it.
    ADD COLUMN closed_by TEXT;

-- The sessionizer's hot path: "the open session for this device".
CREATE INDEX idx_activity_app_session_open
    ON data_activity_app_session (device_id, app_bundle_id)
    WHERE is_open;

DELETE FROM data_activity_app_session WHERE source_provider = 'mac';

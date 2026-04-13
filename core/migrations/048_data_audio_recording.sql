-- Migration 048: data_audio_recording ontology + transcription_resolution cron action
--
-- Decouples audio receipt from transcription. The push action `ios_microphone`
-- writes audio bytes to disk and inserts a row into `data_audio_recording`,
-- returning success in <1s so the iOS request doesn't time out. A separate
-- DEVICE-AGNOSTIC cron action `transcription_resolution` LEFT JOINs recordings
-- against `data_communication_transcription`, calls Gemini for the missing
-- ones, and writes the transcripts.
--
-- The resolver does not care which device produced the recording — iOS today,
-- Mac/web/imports later. Anything that writes to `data_audio_recording` gets
-- transcribed by the same action.
--
-- This mirrors the existing pattern of upstream-raw → downstream-derived
-- ontology pairs (e.g. data_location_point → data_location_place). The
-- recording is a real ontology object — it happened in the user's day —
-- regardless of whether transcription succeeded.
--
-- Fully idempotent: CREATE TABLE IF NOT EXISTS, INSERT OR IGNORE.

CREATE TABLE IF NOT EXISTS data_audio_recording (
    id TEXT PRIMARY KEY,
    -- iOS chunk id; UNIQUE so iOS retries are no-ops
    source_stream_id TEXT NOT NULL UNIQUE,

    -- Timing
    started_at TEXT NOT NULL,
    ended_at TEXT,
    duration_seconds REAL,

    -- Storage location. For local dev: relative path like
    -- "data/lake/ios_microphone/abc123.m4a". For production (future): s3:// URL.
    -- The transcribe action reads the scheme to dispatch.
    audio_url TEXT NOT NULL,
    audio_format TEXT NOT NULL DEFAULT 'm4a',

    -- Signal characteristics from the device
    is_silent INTEGER NOT NULL DEFAULT 0,
    average_db_level REAL,

    -- Standard ontology provenance fields
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata TEXT DEFAULT '{}',

    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_data_audio_recording_started
    ON data_audio_recording(started_at DESC);

CREATE INDEX IF NOT EXISTS idx_data_audio_recording_drain
    ON data_audio_recording(is_silent, created_at);

CREATE TRIGGER IF NOT EXISTS data_audio_recording_set_updated_at
    AFTER UPDATE ON data_audio_recording
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_audio_recording SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- ─────────────────────────────────────────────────────────────────────────────
-- Seed the resolution action row.
-- ─────────────────────────────────────────────────────────────────────────────
-- credential_id is NULL — this is a global ontology→ontology resolver, not
-- tied to any specific device or credential. The runner's run_cron_action
-- helper looks up by function_name AND credential_id IS NULL.

INSERT OR IGNORE INTO app_actions (
    id, action_type, owner, name, enabled, config,
    function_name, credential_id, created_at, updated_at
)
VALUES (
    'action_transcription_resolution',
    'system', 'system', 'Transcription Resolution', 1, '{}',
    'transcription_resolution', NULL,
    datetime('now'), datetime('now')
);

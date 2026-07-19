-- ---------------------------------------------------------------------------
-- Audio sessions: the derived rollup above the 5-minute transcription chunks.
--
-- Ambient audio is captured and transcribed in ~5-minute recorder slices —
-- hundreds a day, each titled in isolation. That granularity is a recorder
-- artifact, not a unit of life. `sessionize::audio` rolls the chunks up into
-- coherent CONTEXT sessions (a conversation, a drive, ~10h of sleep-with-a-fan
-- as one block) by changepoint detection over loudness + speaker count.
--
-- This is the "visit" of audio — the same two-layer shape as location:
--
--   data_location_point  (raw)  →  data_location_visit    (session)
--   data_audio_recording (raw)  →  [transcription chunks] →  data_audio_session
--
-- The chunks stay put in `data_communication_transcription` as fine-grained
-- facts (they anchor citations — "you said X at 2:47"). This table is the
-- coarse unit the day pipeline reads: one row per context, so the detective
-- fuses ~24 sessions a day instead of drowning in 271 chunks.
--
-- It is MECHANICAL output. No title, no generated summary — `content` is the
-- chunk summaries stitched verbatim. All labelling ("quiet, no speakers, 10h,
-- home, overnight" → "Sleeping") is the detective's job, where the full context
-- lives. See docs/event-timeline.md.
-- ---------------------------------------------------------------------------

CREATE TABLE data_audio_session (
    id            TEXT PRIMARY KEY,
    start_time    TIMESTAMPTZ NOT NULL,
    end_time      TIMESTAMPTZ NOT NULL,

    -- Modal social context over the session, the one classification the acoustic
    -- signal supports on its own: 0 silent, 1 solo, 2 dyad, 3 group.
    speaker_mode  SMALLINT NOT NULL,
    -- Mean loudness — a weak environment hint for the labeller downstream.
    avg_db        DOUBLE PRECISION,
    chunk_count   INTEGER NOT NULL,

    -- The chunk summaries, stitched. The detective's content clue and, later, the
    -- search document. Empty when nothing was said (a silent session).
    content       TEXT,

    -- Ontology plumbing, matching the data_* convention.
    source_table    TEXT NOT NULL DEFAULT 'data_audio_session',
    source_provider TEXT NOT NULL DEFAULT 'ios',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_audio_session_time ON data_audio_session (start_time);

COMMENT ON TABLE data_audio_session IS
    'Derived audio context sessions (changepoint rollup of transcription chunks). '
    'The coarse unit the day pipeline reads; chunks remain in '
    'data_communication_transcription as the fine-grained citation layer.';

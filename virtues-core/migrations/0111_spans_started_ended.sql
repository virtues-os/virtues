-- Spans finish the job 0110 started: `started_at` / `ended_at`.
--
-- 0110 gave every instant one name (`occurred_at`) and deliberately left the
-- seven `start_time`/`end_time` pairs alone, on the grounds that they were
-- already internally consistent — style rather than confusion. That was true,
-- and it was still the wrong place to stop, for a reason visible in the code
-- rather than the schema.
--
-- `OntologyDescriptor.timestamp_sql` exists ONLY because the "when" column had
-- several names. After 0110 it was down to four expressions from five, and the
-- remainder were exactly these: `t.start_time`, beside `t.created_at` and
-- `t.updated_at`. So the field survived — a piece of configuration whose entire
-- purpose is to spell one idea differently per table — and every new ontology
-- still has to be told which spelling its table uses.
--
-- Three tables already used the right names before this: `app_applet_runs`,
-- `data_audio_recording`, and `data_location_visit` (converted in 0110). So the
-- convention was not invented here; it was already the majority and the
-- exceptions were the older tables.
--
-- The rule, complete now:
--
--   * `occurred_at`             — an instant. When the thing happened.
--   * `started_at` / `ended_at` — a span. When it began and when it finished.
--   * `created_at` / `updated_at` — when WE wrote the row. Never the event.
--
-- Struct fields and JSON keys are NOT renamed with the columns. The frontend
-- reads `start_time`/`end_time` from API responses in forty places, and a
-- serialized field is a contract the compiler cannot check — so the column
-- moves and the wire format stays put. That asymmetry is deliberate: the
-- database is ours to name, the API is something other code depends on.

ALTER TABLE data_activity_app_session        RENAME COLUMN start_time TO started_at;
ALTER TABLE data_activity_app_session        RENAME COLUMN end_time   TO ended_at;
ALTER TABLE data_audio_session               RENAME COLUMN start_time TO started_at;
ALTER TABLE data_audio_session               RENAME COLUMN end_time   TO ended_at;
ALTER TABLE data_calendar_event              RENAME COLUMN start_time TO started_at;
ALTER TABLE data_calendar_event              RENAME COLUMN end_time   TO ended_at;
ALTER TABLE data_communication_transcription RENAME COLUMN start_time TO started_at;
ALTER TABLE data_communication_transcription RENAME COLUMN end_time   TO ended_at;
ALTER TABLE data_health_sleep                RENAME COLUMN start_time TO started_at;
ALTER TABLE data_health_sleep                RENAME COLUMN end_time   TO ended_at;
ALTER TABLE data_health_workout              RENAME COLUMN start_time TO started_at;
ALTER TABLE data_health_workout              RENAME COLUMN end_time   TO ended_at;
ALTER TABLE wiki_events                      RENAME COLUMN start_time TO started_at;
ALTER TABLE wiki_events                      RENAME COLUMN end_time   TO ended_at;

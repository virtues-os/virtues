-- 015: Ontology Restructure
--
-- Restructures ontology domains to model universal human concepts:
--   social → communication (email, message, transcription)
--   knowledge → content (document, conversation, bookmark)
--   speech → absorbed into communication
--   calendar → calendar_event (fix naming pattern)
--   device/environment → removed (battery + barometer telemetry noise)
--
-- Also creates content_bookmark ontology.

-- =============================================================================
-- PART 1: Domain Renames
-- =============================================================================

ALTER TABLE data_social_email RENAME TO data_communication_email;
ALTER TABLE data_social_message RENAME TO data_communication_message;
ALTER TABLE data_knowledge_document RENAME TO data_content_document;
ALTER TABLE data_knowledge_ai_conversation RENAME TO data_content_conversation;
ALTER TABLE data_speech_transcription RENAME TO data_communication_transcription;
ALTER TABLE data_calendar RENAME TO data_calendar_event;

-- =============================================================================
-- PART 2: Pipeline Removals (battery + barometer)
-- =============================================================================

DROP TABLE IF EXISTS data_device_battery;
DROP TABLE IF EXISTS data_environment_pressure;

DELETE FROM elt_stream_checkpoints WHERE stream_name IN ('battery', 'barometer');
DELETE FROM elt_stream_connections WHERE stream_name IN ('battery', 'barometer');

-- =============================================================================
-- PART 3: Transcription Enrichment
-- =============================================================================

ALTER TABLE data_communication_transcription ADD COLUMN title TEXT;
ALTER TABLE data_communication_transcription ADD COLUMN summary TEXT;
ALTER TABLE data_communication_transcription ADD COLUMN confidence REAL;
ALTER TABLE data_communication_transcription ADD COLUMN tags TEXT DEFAULT '[]';
ALTER TABLE data_communication_transcription ADD COLUMN entities TEXT DEFAULT '{}';

CREATE INDEX IF NOT EXISTS idx_communication_transcription_end
    ON data_communication_transcription(end_time DESC);

-- =============================================================================
-- PART 4: New Ontology — content_bookmark
-- =============================================================================

CREATE TABLE IF NOT EXISTS data_content_bookmark (
    id TEXT PRIMARY KEY,
    source_connection_id TEXT REFERENCES elt_source_connections(id),

    url TEXT NOT NULL,
    title TEXT,
    description TEXT,

    source_platform TEXT,
    bookmark_type TEXT,
    content_type TEXT,

    author TEXT,
    tags TEXT,
    thumbnail_url TEXT,

    timestamp TEXT NOT NULL,

    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,

    deleted_at_source TEXT,
    is_archived INTEGER DEFAULT 0,
    metadata TEXT DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_content_bookmark_ts
    ON data_content_bookmark(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_content_bookmark_platform
    ON data_content_bookmark(source_platform);
CREATE INDEX IF NOT EXISTS idx_content_bookmark_type
    ON data_content_bookmark(bookmark_type);

CREATE TRIGGER IF NOT EXISTS data_content_bookmark_set_updated_at
    AFTER UPDATE ON data_content_bookmark
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_content_bookmark SET updated_at = datetime('now') WHERE id = NEW.id;
END;

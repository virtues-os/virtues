-- Foundation: Pipeline infrastructure for SQLite
-- Note: No schemas, extensions, or PL/pgSQL in SQLite

--------------------------------------------------------------------------------
-- SOURCE CONNECTIONS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_source_connections (
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    name TEXT NOT NULL UNIQUE,

    -- Auth
    access_token TEXT,
    refresh_token TEXT,
    token_expires_at TEXT,
    auth_type TEXT NOT NULL DEFAULT 'oauth2' CHECK (auth_type IN ('oauth2', 'device', 'api_key', 'none', 'plaid')),

    -- Device pairing
    device_id TEXT,
    device_info TEXT,  -- JSON
    device_token TEXT,
    pairing_status TEXT CHECK (pairing_status IS NULL OR pairing_status IN ('pending', 'active', 'revoked')),
    last_seen_at TEXT,

    -- State
    is_active INTEGER DEFAULT 1,
    is_internal INTEGER DEFAULT 0,
    error_message TEXT,
    error_at TEXT,

    -- Config
    metadata TEXT,  -- JSON
    sync_strategy TEXT DEFAULT 'ongoing' CHECK (sync_strategy IS NULL OR sync_strategy IN ('migration', 'ongoing', 'hybrid')),

    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_source_connections_source_device
    ON data_source_connections(source, device_id) WHERE device_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_source_connections_device_token
    ON data_source_connections(device_token) WHERE device_token IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS data_source_connections_set_updated_at
    AFTER UPDATE ON data_source_connections
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_source_connections SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- STREAM CONNECTIONS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_stream_connections (
    id TEXT PRIMARY KEY,
    source_connection_id TEXT NOT NULL REFERENCES data_source_connections(id) ON DELETE CASCADE,
    stream_name TEXT NOT NULL,
    table_name TEXT NOT NULL,
    is_enabled INTEGER NOT NULL DEFAULT 1,
    cron_schedule TEXT,
    config TEXT NOT NULL DEFAULT '{}',  -- JSON
    last_sync_token TEXT,
    last_sync_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(source_connection_id, stream_name)
);

CREATE TRIGGER IF NOT EXISTS data_stream_connections_set_updated_at
    AFTER UPDATE ON data_stream_connections
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_stream_connections SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- JOBS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_jobs (
    id TEXT PRIMARY KEY,
    job_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'running', 'succeeded', 'failed', 'cancelled')),
    source_connection_id TEXT REFERENCES data_source_connections(id) ON DELETE CASCADE,
    stream_name TEXT,
    sync_mode TEXT,
    transform_id TEXT,
    transform_strategy TEXT,
    parent_job_id TEXT REFERENCES data_jobs(id) ON DELETE CASCADE,
    transform_stage TEXT,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT,
    records_processed INTEGER NOT NULL DEFAULT 0,
    error_message TEXT,
    error_class TEXT,
    metadata TEXT NOT NULL DEFAULT '{}',  -- JSON
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_jobs_pending
    ON data_jobs(created_at ASC) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_jobs_parent
    ON data_jobs(parent_job_id) WHERE parent_job_id IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS data_jobs_set_updated_at
    AFTER UPDATE ON data_jobs
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_jobs SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- STREAM OBJECTS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_stream_objects (
    id TEXT PRIMARY KEY,
    source_connection_id TEXT NOT NULL REFERENCES data_source_connections(id) ON DELETE CASCADE,
    stream_name TEXT NOT NULL,
    s3_key TEXT NOT NULL UNIQUE,
    record_count INTEGER NOT NULL CHECK (record_count > 0),
    size_bytes INTEGER NOT NULL CHECK (size_bytes > 0),
    min_timestamp TEXT,
    max_timestamp TEXT,
    archive_job_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (min_timestamp IS NULL OR max_timestamp IS NULL OR min_timestamp <= max_timestamp)
);

CREATE INDEX IF NOT EXISTS idx_stream_objects_source_stream
    ON data_stream_objects(source_connection_id, stream_name);
CREATE INDEX IF NOT EXISTS idx_stream_objects_timestamp_range
    ON data_stream_objects(source_connection_id, stream_name, min_timestamp, max_timestamp);
CREATE INDEX IF NOT EXISTS idx_stream_objects_archive_job
    ON data_stream_objects(archive_job_id) WHERE archive_job_id IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS data_stream_objects_set_updated_at
    AFTER UPDATE ON data_stream_objects
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_stream_objects SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- STREAM TRANSFORM CHECKPOINTS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_stream_transform_checkpoints (
    id TEXT PRIMARY KEY,
    source_connection_id TEXT NOT NULL REFERENCES data_source_connections(id) ON DELETE CASCADE,
    stream_name TEXT NOT NULL,
    transform_name TEXT NOT NULL,
    last_processed_s3_key TEXT,
    last_processed_timestamp TEXT,
    last_processed_object_id TEXT REFERENCES data_stream_objects(id) ON DELETE SET NULL,
    records_processed INTEGER NOT NULL DEFAULT 0,
    objects_processed INTEGER NOT NULL DEFAULT 0,
    last_run_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(source_connection_id, stream_name, transform_name)
);

CREATE INDEX IF NOT EXISTS idx_transform_checkpoints_lookup
    ON data_stream_transform_checkpoints(source_connection_id, stream_name, transform_name);

CREATE TRIGGER IF NOT EXISTS data_stream_transform_checkpoints_set_updated_at
    AFTER UPDATE ON data_stream_transform_checkpoints
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_stream_transform_checkpoints SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- STREAM CHECKPOINTS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_stream_checkpoints (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL,
    stream_name TEXT NOT NULL,
    checkpoint_key TEXT NOT NULL,
    last_processed_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(source_id, stream_name, checkpoint_key)
);

CREATE INDEX IF NOT EXISTS idx_stream_checkpoints_lookup
    ON data_stream_checkpoints(source_id, stream_name, checkpoint_key);

CREATE TRIGGER IF NOT EXISTS data_stream_checkpoints_set_updated_at
    AFTER UPDATE ON data_stream_checkpoints
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_stream_checkpoints SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- ARCHIVE JOBS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_archive_jobs (
    id TEXT PRIMARY KEY,
    sync_job_id TEXT REFERENCES data_jobs(id) ON DELETE CASCADE,
    source_connection_id TEXT NOT NULL REFERENCES data_source_connections(id) ON DELETE CASCADE,
    stream_name TEXT NOT NULL,
    s3_key TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'in_progress', 'completed', 'failed')),
    error_message TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    record_count INTEGER NOT NULL DEFAULT 0,
    size_bytes INTEGER NOT NULL DEFAULT 0,
    min_timestamp TEXT,
    max_timestamp TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    started_at TEXT,
    completed_at TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    CONSTRAINT valid_status_timestamps CHECK (
        (status = 'in_progress' AND started_at IS NOT NULL) OR
        (status = 'completed' AND started_at IS NOT NULL AND completed_at IS NOT NULL) OR
        (status IN ('pending', 'failed'))
    )
);

CREATE INDEX IF NOT EXISTS idx_archive_jobs_pending
    ON data_archive_jobs(status, created_at) WHERE status IN ('pending', 'failed');
CREATE INDEX IF NOT EXISTS idx_archive_jobs_source
    ON data_archive_jobs(source_connection_id, stream_name, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_archive_jobs_sync_job
    ON data_archive_jobs(sync_job_id) WHERE sync_job_id IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS data_archive_jobs_set_updated_at
    AFTER UPDATE ON data_archive_jobs
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_archive_jobs SET updated_at = datetime('now') WHERE id = NEW.id;
END;

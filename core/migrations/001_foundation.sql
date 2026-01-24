-- Foundation: Pipeline infrastructure for SQLite
-- Note: No schemas, extensions, or PL/pgSQL in SQLite

--------------------------------------------------------------------------------
-- SOURCE CONNECTIONS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS elt_source_connections (
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

CREATE UNIQUE INDEX IF NOT EXISTS idx_elt_source_connections_source_device
    ON elt_source_connections(source, device_id) WHERE device_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_elt_source_connections_device_token
    ON elt_source_connections(device_token) WHERE device_token IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS elt_source_connections_set_updated_at
    AFTER UPDATE ON elt_source_connections
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE elt_source_connections SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- STREAM CONNECTIONS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS elt_stream_connections (
    id TEXT PRIMARY KEY,
    source_connection_id TEXT NOT NULL REFERENCES elt_source_connections(id) ON DELETE CASCADE,
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

CREATE TRIGGER IF NOT EXISTS elt_stream_connections_set_updated_at
    AFTER UPDATE ON elt_stream_connections
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE elt_stream_connections SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- JOBS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS elt_jobs (
    id TEXT PRIMARY KEY,
    job_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'running', 'succeeded', 'failed', 'cancelled')),
    source_connection_id TEXT REFERENCES elt_source_connections(id) ON DELETE CASCADE,
    stream_name TEXT,
    sync_mode TEXT,
    transform_id TEXT,
    transform_strategy TEXT,
    parent_job_id TEXT REFERENCES elt_jobs(id) ON DELETE CASCADE,
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

CREATE INDEX IF NOT EXISTS idx_elt_jobs_pending
    ON elt_jobs(created_at ASC) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_elt_jobs_parent
    ON elt_jobs(parent_job_id) WHERE parent_job_id IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS elt_jobs_set_updated_at
    AFTER UPDATE ON elt_jobs
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE elt_jobs SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- STREAM OBJECTS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS elt_stream_objects (
    id TEXT PRIMARY KEY,
    source_connection_id TEXT NOT NULL REFERENCES elt_source_connections(id) ON DELETE CASCADE,
    stream_name TEXT NOT NULL,
    storage_key TEXT NOT NULL UNIQUE,
    record_count INTEGER NOT NULL CHECK (record_count > 0),
    size_bytes INTEGER NOT NULL CHECK (size_bytes > 0),
    min_timestamp TEXT,
    max_timestamp TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (min_timestamp IS NULL OR max_timestamp IS NULL OR min_timestamp <= max_timestamp)
);

CREATE INDEX IF NOT EXISTS idx_elt_stream_objects_source_stream
    ON elt_stream_objects(source_connection_id, stream_name);
CREATE INDEX IF NOT EXISTS idx_elt_stream_objects_timestamp_range
    ON elt_stream_objects(source_connection_id, stream_name, min_timestamp, max_timestamp);

CREATE TRIGGER IF NOT EXISTS elt_stream_objects_set_updated_at
    AFTER UPDATE ON elt_stream_objects
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE elt_stream_objects SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- TRANSFORM CHECKPOINTS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS elt_transform_checkpoints (
    id TEXT PRIMARY KEY,
    source_connection_id TEXT NOT NULL REFERENCES elt_source_connections(id) ON DELETE CASCADE,
    stream_name TEXT NOT NULL,
    transform_name TEXT NOT NULL,
    last_processed_storage_key TEXT,
    last_processed_timestamp TEXT,
    last_processed_object_id TEXT REFERENCES elt_stream_objects(id) ON DELETE SET NULL,
    records_processed INTEGER NOT NULL DEFAULT 0,
    objects_processed INTEGER NOT NULL DEFAULT 0,
    last_run_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(source_connection_id, stream_name, transform_name)
);

CREATE INDEX IF NOT EXISTS idx_elt_transform_checkpoints_lookup
    ON elt_transform_checkpoints(source_connection_id, stream_name, transform_name);

CREATE TRIGGER IF NOT EXISTS elt_transform_checkpoints_set_updated_at
    AFTER UPDATE ON elt_transform_checkpoints
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE elt_transform_checkpoints SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- STREAM CHECKPOINTS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS elt_stream_checkpoints (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL,
    stream_name TEXT NOT NULL,
    checkpoint_key TEXT NOT NULL,
    last_processed_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(source_id, stream_name, checkpoint_key)
);

CREATE INDEX IF NOT EXISTS idx_elt_stream_checkpoints_lookup
    ON elt_stream_checkpoints(source_id, stream_name, checkpoint_key);

CREATE TRIGGER IF NOT EXISTS elt_stream_checkpoints_set_updated_at
    AFTER UPDATE ON elt_stream_checkpoints
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE elt_stream_checkpoints SET updated_at = datetime('now') WHERE id = NEW.id;
END;


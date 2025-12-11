-- Foundation: Schemas, extensions, pipeline infrastructure
-- Consolidates: 001, 002, 008, 012, 015 (source_connections parts)

--------------------------------------------------------------------------------
-- SCHEMAS & EXTENSIONS
--------------------------------------------------------------------------------

CREATE SCHEMA IF NOT EXISTS data;
CREATE SCHEMA IF NOT EXISTS app;

-- Note: search_path is set at database level via init-schemas.sh
-- Do not use SET search_path in migrations - it breaks SQLx metadata operations

CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS postgis;
CREATE EXTENSION IF NOT EXISTS vector;

--------------------------------------------------------------------------------
-- UTILITY FUNCTIONS
--------------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

--------------------------------------------------------------------------------
-- SOURCE CONNECTIONS (external data sources)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.source_connections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source TEXT NOT NULL,
    name TEXT NOT NULL UNIQUE,
    access_token TEXT,
    refresh_token TEXT,
    token_expires_at TIMESTAMPTZ,
    auth_type TEXT NOT NULL DEFAULT 'oauth2',
    device_id TEXT,
    device_info JSONB,
    device_token TEXT,
    pairing_code TEXT,
    pairing_status TEXT,
    code_expires_at TIMESTAMPTZ,
    last_seen_at TIMESTAMPTZ,
    is_active BOOLEAN DEFAULT true,
    is_internal BOOLEAN DEFAULT false,
    error_message TEXT,
    error_at TIMESTAMPTZ,
    -- From 012: metadata for Plaid and other sources
    metadata JSONB,
    -- From 015: sync strategy for data import behavior
    sync_strategy TEXT DEFAULT 'ongoing',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Add columns if they don't exist (for idempotency)
ALTER TABLE data.source_connections ADD COLUMN IF NOT EXISTS metadata JSONB;
ALTER TABLE data.source_connections ADD COLUMN IF NOT EXISTS sync_strategy TEXT DEFAULT 'ongoing';

-- Constraints (idempotent using DO blocks)
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'source_connections_auth_type_check') THEN
        ALTER TABLE data.source_connections
        ADD CONSTRAINT source_connections_auth_type_check
        CHECK (auth_type IN ('oauth2', 'device', 'api_key', 'none', 'plaid'));
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'source_connections_pairing_status_check') THEN
        ALTER TABLE data.source_connections
        ADD CONSTRAINT source_connections_pairing_status_check
        CHECK (pairing_status IS NULL OR pairing_status IN ('pending', 'active', 'revoked'));
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'source_connections_sync_strategy_check') THEN
        ALTER TABLE data.source_connections
        ADD CONSTRAINT source_connections_sync_strategy_check
        CHECK (sync_strategy IS NULL OR sync_strategy IN ('migration', 'ongoing', 'hybrid'));
    END IF;
END $$;

-- Comments
COMMENT ON COLUMN data.source_connections.metadata IS 'Source-specific metadata (e.g., Plaid item_id, access_token, institution info)';
COMMENT ON COLUMN data.source_connections.sync_strategy IS 'Sync strategy: migration (one-time import), ongoing (continuous sync), or hybrid (both)';

-- Indexes
CREATE UNIQUE INDEX IF NOT EXISTS idx_source_connections_source_device ON data.source_connections(source, device_id)
  WHERE device_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_source_connections_device_token ON data.source_connections(device_token)
  WHERE device_token IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_source_connections_pairing_code ON data.source_connections(pairing_code)
  WHERE pairing_code IS NOT NULL AND pairing_status = 'pending';

-- Trigger
DROP TRIGGER IF EXISTS source_connections_updated_at ON data.source_connections;
CREATE TRIGGER source_connections_updated_at
    BEFORE UPDATE ON data.source_connections
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- STREAM CONNECTIONS (per-stream sync configuration)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.stream_connections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_connection_id UUID NOT NULL REFERENCES data.source_connections(id) ON DELETE CASCADE,
    stream_name TEXT NOT NULL,
    table_name TEXT NOT NULL,
    is_enabled BOOLEAN NOT NULL DEFAULT true,
    -- 6-field cron schedule (sec min hour day month dow). Defaults applied by seeding logic from registry.
    cron_schedule TEXT,
    config JSONB NOT NULL DEFAULT '{}',
    last_sync_token TEXT,
    last_sync_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(source_connection_id, stream_name)
);

DROP TRIGGER IF EXISTS stream_connections_updated_at ON data.stream_connections;
CREATE TRIGGER stream_connections_updated_at
    BEFORE UPDATE ON data.stream_connections
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- JOBS (async job tracking)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    source_connection_id UUID REFERENCES data.source_connections(id) ON DELETE CASCADE,
    stream_name TEXT,
    sync_mode TEXT,
    transform_id UUID,
    transform_strategy TEXT,
    parent_job_id UUID REFERENCES data.jobs(id) ON DELETE CASCADE,
    transform_stage TEXT,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    records_processed BIGINT NOT NULL DEFAULT 0,
    error_message TEXT,
    error_class TEXT,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'jobs_status_check') THEN
        ALTER TABLE data.jobs
        ADD CONSTRAINT jobs_status_check
        CHECK (status IN ('pending', 'running', 'succeeded', 'failed', 'cancelled'));
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_jobs_pending ON data.jobs(created_at ASC)
    WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_jobs_parent ON data.jobs(parent_job_id)
    WHERE parent_job_id IS NOT NULL;

DROP TRIGGER IF EXISTS jobs_updated_at ON data.jobs;
CREATE TRIGGER jobs_updated_at
    BEFORE UPDATE ON data.jobs
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- STREAM OBJECTS (S3 object metadata)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.stream_objects (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_connection_id UUID NOT NULL REFERENCES data.source_connections(id) ON DELETE CASCADE,
    stream_name TEXT NOT NULL,
    s3_key TEXT NOT NULL UNIQUE,
    record_count INTEGER NOT NULL CHECK (record_count > 0),
    size_bytes BIGINT NOT NULL CHECK (size_bytes > 0),
    min_timestamp TIMESTAMPTZ,
    max_timestamp TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (min_timestamp IS NULL OR max_timestamp IS NULL OR min_timestamp <= max_timestamp)
);

CREATE INDEX IF NOT EXISTS idx_stream_objects_source_stream ON data.stream_objects(source_connection_id, stream_name);
CREATE INDEX IF NOT EXISTS idx_stream_objects_timestamp_range ON data.stream_objects(source_connection_id, stream_name, min_timestamp, max_timestamp);
CREATE INDEX IF NOT EXISTS idx_stream_objects_created_at ON data.stream_objects(created_at);

--------------------------------------------------------------------------------
-- STREAM TRANSFORM CHECKPOINTS (transform progress tracking)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.stream_transform_checkpoints (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_connection_id UUID NOT NULL REFERENCES data.source_connections(id) ON DELETE CASCADE,
    stream_name TEXT NOT NULL,
    transform_name TEXT NOT NULL,
    last_processed_s3_key TEXT,
    last_processed_timestamp TIMESTAMPTZ,
    last_processed_object_id UUID REFERENCES data.stream_objects(id) ON DELETE SET NULL,
    records_processed BIGINT NOT NULL DEFAULT 0,
    objects_processed BIGINT NOT NULL DEFAULT 0,
    last_run_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(source_connection_id, stream_name, transform_name)
);

CREATE INDEX IF NOT EXISTS idx_transform_checkpoints_lookup ON data.stream_transform_checkpoints(source_connection_id, stream_name, transform_name);
CREATE INDEX IF NOT EXISTS idx_transform_checkpoints_last_run ON data.stream_transform_checkpoints(last_run_at);

--------------------------------------------------------------------------------
-- STREAM CHECKPOINTS (in-memory transform checkpoints)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.stream_checkpoints (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_id UUID NOT NULL,
    stream_name TEXT NOT NULL,
    checkpoint_key TEXT NOT NULL,
    last_processed_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(source_id, stream_name, checkpoint_key)
);

CREATE INDEX IF NOT EXISTS idx_stream_checkpoints_lookup ON data.stream_checkpoints(source_id, stream_name, checkpoint_key);

--------------------------------------------------------------------------------
-- ARCHIVE JOBS (S3 archival status)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.archive_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sync_job_id UUID REFERENCES data.jobs(id) ON DELETE CASCADE,
    source_connection_id UUID NOT NULL REFERENCES data.source_connections(id) ON DELETE CASCADE,
    stream_name TEXT NOT NULL,
    s3_key TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'in_progress', 'completed', 'failed')),
    error_message TEXT,
    retry_count INT NOT NULL DEFAULT 0,
    max_retries INT NOT NULL DEFAULT 3,
    record_count INT NOT NULL DEFAULT 0,
    size_bytes BIGINT NOT NULL DEFAULT 0,
    min_timestamp TIMESTAMPTZ,
    max_timestamp TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    CONSTRAINT valid_status_timestamps CHECK (
        (status = 'in_progress' AND started_at IS NOT NULL) OR
        (status = 'completed' AND started_at IS NOT NULL AND completed_at IS NOT NULL) OR
        (status IN ('pending', 'failed'))
    )
);

CREATE INDEX IF NOT EXISTS idx_archive_jobs_pending ON data.archive_jobs(status, created_at)
    WHERE status IN ('pending', 'failed');
CREATE INDEX IF NOT EXISTS idx_archive_jobs_source ON data.archive_jobs(source_connection_id, stream_name, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_archive_jobs_sync_job ON data.archive_jobs(sync_job_id)
    WHERE sync_job_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_archive_jobs_stale ON data.archive_jobs(created_at)
    WHERE status IN ('pending', 'in_progress');

-- Add archive_job_id FK to stream_objects
ALTER TABLE data.stream_objects
ADD COLUMN IF NOT EXISTS archive_job_id UUID REFERENCES data.archive_jobs(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_stream_objects_archive_job ON data.stream_objects(archive_job_id)
    WHERE archive_job_id IS NOT NULL;

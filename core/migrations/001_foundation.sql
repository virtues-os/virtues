-- Foundation: ELT Pipeline, Drive Storage, App Config, Auth, Usage (SQLite)
-- Consolidated from migrations 001, 006, 008, 009, 011, 014, 017

--------------------------------------------------------------------------------
-- ELT: SOURCE CONNECTIONS
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

    -- Tiering
    tier TEXT NOT NULL DEFAULT 'free',
    connection_policy TEXT NOT NULL DEFAULT 'multi_instance',

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
-- ELT: STREAM CONNECTIONS
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

    -- Watermarking
    earliest_record_at TEXT,
    latest_record_at TEXT,
    sync_status TEXT NOT NULL DEFAULT 'pending' CHECK (sync_status IN ('pending', 'initial', 'incremental', 'backfilling', 'failed')),

    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(source_connection_id, stream_name)
);

CREATE INDEX IF NOT EXISTS idx_elt_stream_connections_watermarks
    ON elt_stream_connections(source_connection_id, stream_name, earliest_record_at, latest_record_at);

CREATE TRIGGER IF NOT EXISTS elt_stream_connections_set_updated_at
    AFTER UPDATE ON elt_stream_connections
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE elt_stream_connections SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- ELT: JOBS
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
-- ELT: STREAM OBJECTS
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
-- ELT: TRANSFORM CHECKPOINTS
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
-- ELT: STREAM CHECKPOINTS
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

--------------------------------------------------------------------------------
-- DRIVE: FILES
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS drive_files (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    filename TEXT NOT NULL,
    mime_type TEXT,
    size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
    parent_id TEXT REFERENCES drive_files(id) ON DELETE CASCADE,
    is_folder INTEGER NOT NULL DEFAULT 0,
    sha256_hash TEXT,
    deleted_at TEXT DEFAULT NULL,  -- Soft delete
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_drive_files_path ON drive_files(path);
CREATE INDEX IF NOT EXISTS idx_drive_files_parent ON drive_files(parent_id) WHERE parent_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_drive_files_folder ON drive_files(parent_id, is_folder);
CREATE INDEX IF NOT EXISTS idx_drive_files_deleted_at ON drive_files(deleted_at);

CREATE TRIGGER IF NOT EXISTS drive_files_set_updated_at
    AFTER UPDATE ON drive_files
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE drive_files SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- DRIVE: USAGE (singleton)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS drive_usage (
    id TEXT PRIMARY KEY DEFAULT '00000000-0000-0000-0000-000000000001',
    drive_bytes INTEGER NOT NULL DEFAULT 0 CHECK (drive_bytes >= 0),
    data_lake_bytes INTEGER NOT NULL DEFAULT 0 CHECK (data_lake_bytes >= 0),
    total_bytes INTEGER NOT NULL DEFAULT 0 CHECK (total_bytes >= 0),
    file_count INTEGER NOT NULL DEFAULT 0 CHECK (file_count >= 0),
    folder_count INTEGER NOT NULL DEFAULT 0 CHECK (folder_count >= 0),
    quota_bytes INTEGER NOT NULL DEFAULT 107374182400,  -- 100 GB
    warning_80_sent INTEGER NOT NULL DEFAULT 0,
    warning_90_sent INTEGER NOT NULL DEFAULT 0,
    warning_100_sent INTEGER NOT NULL DEFAULT 0,
    last_scan_at TEXT,
    last_scan_bytes INTEGER,
    trash_bytes INTEGER DEFAULT 0,
    trash_count INTEGER DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    CONSTRAINT drive_usage_singleton CHECK (id = '00000000-0000-0000-0000-000000000001')
);

CREATE TRIGGER IF NOT EXISTS drive_usage_set_updated_at
    AFTER UPDATE ON drive_usage
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE drive_usage SET updated_at = datetime('now') WHERE id = NEW.id;
END;

INSERT OR IGNORE INTO drive_usage (id) VALUES ('00000000-0000-0000-0000-000000000001');

--------------------------------------------------------------------------------
-- APP: USER PROFILE (singleton)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app_user_profile (
    id TEXT PRIMARY KEY DEFAULT '00000000-0000-0000-0000-000000000001',
    full_name TEXT,
    preferred_name TEXT,
    birth_date TEXT,
    height_cm REAL,
    weight_kg REAL,
    ethnicity TEXT,
    occupation TEXT,
    employer TEXT,
    onboarding_status TEXT NOT NULL DEFAULT 'welcome' CHECK (onboarding_status IN ('welcome', 'profile', 'places', 'tools', 'complete')),
    home_place_id TEXT,  -- References wiki_places after 002_entities
    theme TEXT DEFAULT 'light',
    update_check_hour INTEGER DEFAULT 8 CHECK (update_check_hour >= 0 AND update_check_hour <= 23),
    crux TEXT,
    technology_vision TEXT,
    pain_point_primary TEXT,
    pain_point_secondary TEXT,
    excited_features TEXT,  -- JSON array
    owner_email TEXT,
    server_status TEXT NOT NULL DEFAULT 'provisioning' CHECK (server_status IN ('provisioning', 'migrating', 'ready')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    CONSTRAINT user_profile_singleton CHECK (id = '00000000-0000-0000-0000-000000000001')
);

CREATE INDEX IF NOT EXISTS idx_app_user_profile_owner_email ON app_user_profile(owner_email);
CREATE INDEX IF NOT EXISTS idx_app_user_profile_server_status ON app_user_profile(server_status);

CREATE TRIGGER IF NOT EXISTS app_user_profile_set_updated_at
    AFTER UPDATE ON app_user_profile
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_user_profile SET updated_at = datetime('now') WHERE id = NEW.id;
END;

INSERT OR IGNORE INTO app_user_profile (id) VALUES ('00000000-0000-0000-0000-000000000001');

--------------------------------------------------------------------------------
-- APP: ASSISTANT PROFILE (singleton)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app_assistant_profile (
    id TEXT PRIMARY KEY DEFAULT '00000000-0000-0000-0000-000000000001',
    assistant_name TEXT DEFAULT 'Ari',
    default_agent_id TEXT DEFAULT 'agent',
    default_model_id TEXT DEFAULT 'anthropic/claude-sonnet-4-20250514',
    background_model_id TEXT DEFAULT 'cerebras/llama-3.3-70b',
    enabled_tools TEXT DEFAULT '{"web_search": true, "virtues_query_ontology": true, "virtues_semantic_search": true}',  -- JSON
    ui_preferences TEXT DEFAULT '{"contextIndicator": {"alwaysVisible": false, "showThreshold": 70}}',  -- JSON
    embedding_model_id TEXT DEFAULT 'nomic-embed-text',
    ollama_endpoint TEXT DEFAULT 'http://localhost:11434',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    CONSTRAINT assistant_profile_singleton CHECK (id = '00000000-0000-0000-0000-000000000001')
);

CREATE TRIGGER IF NOT EXISTS app_assistant_profile_set_updated_at
    AFTER UPDATE ON app_assistant_profile
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_assistant_profile SET updated_at = datetime('now') WHERE id = NEW.id;
END;

INSERT OR IGNORE INTO app_assistant_profile (id) VALUES ('00000000-0000-0000-0000-000000000001');

--------------------------------------------------------------------------------
-- APP: MCP TOOLS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app_mcp_tools (
    id TEXT PRIMARY KEY,
    server_name TEXT NOT NULL,
    server_url TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    description TEXT,
    input_schema TEXT,  -- JSON
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(server_name, tool_name)
);

CREATE INDEX IF NOT EXISTS idx_mcp_tools_server ON app_mcp_tools(server_name);
CREATE INDEX IF NOT EXISTS idx_mcp_tools_enabled ON app_mcp_tools(id) WHERE enabled = 1;

CREATE TRIGGER IF NOT EXISTS app_mcp_tools_set_updated_at
    AFTER UPDATE ON app_mcp_tools
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_mcp_tools SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- AUTH: USER
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app_auth_user (
    id TEXT PRIMARY KEY,
    email TEXT UNIQUE NOT NULL,
    email_verified TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TRIGGER IF NOT EXISTS app_auth_user_set_updated_at
    AFTER UPDATE ON app_auth_user
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_auth_user SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- AUTH: SESSION
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app_auth_session (
    id TEXT PRIMARY KEY,
    session_token TEXT UNIQUE NOT NULL,
    user_id TEXT NOT NULL REFERENCES app_auth_user(id) ON DELETE CASCADE,
    expires TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_auth_session_user ON app_auth_session(user_id);
CREATE INDEX IF NOT EXISTS idx_auth_session_expires ON app_auth_session(expires);

--------------------------------------------------------------------------------
-- AUTH: VERIFICATION TOKEN
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app_auth_verification_token (
    identifier TEXT NOT NULL,
    token TEXT NOT NULL,
    expires TEXT NOT NULL,
    PRIMARY KEY (identifier, token)
);

CREATE INDEX IF NOT EXISTS idx_auth_verification_expires ON app_auth_verification_token(expires);

--------------------------------------------------------------------------------
-- USAGE: API
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app_api_usage (
    id TEXT PRIMARY KEY,
    endpoint TEXT NOT NULL,
    day_bucket TEXT NOT NULL,
    request_count INTEGER NOT NULL DEFAULT 0 CHECK (request_count >= 0),
    token_count INTEGER NOT NULL DEFAULT 0 CHECK (token_count >= 0),
    input_tokens INTEGER NOT NULL DEFAULT 0 CHECK (input_tokens >= 0),
    output_tokens INTEGER NOT NULL DEFAULT 0 CHECK (output_tokens >= 0),
    estimated_cost_usd REAL NOT NULL DEFAULT 0 CHECK (estimated_cost_usd >= 0),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(endpoint, day_bucket)
);

CREATE INDEX IF NOT EXISTS idx_api_usage_day ON app_api_usage(day_bucket DESC);
CREATE INDEX IF NOT EXISTS idx_api_usage_endpoint_day ON app_api_usage(endpoint, day_bucket);

CREATE TRIGGER IF NOT EXISTS app_api_usage_set_updated_at
    AFTER UPDATE ON app_api_usage
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_api_usage SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- USAGE: LLM
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app_llm_usage (
    id TEXT PRIMARY KEY,
    month TEXT UNIQUE NOT NULL,
    tokens_used INTEGER DEFAULT 0,
    cost_cents INTEGER DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_llm_usage_month ON app_llm_usage(month);

CREATE TRIGGER IF NOT EXISTS app_llm_usage_set_updated_at
    AFTER UPDATE ON app_llm_usage
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_llm_usage SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- USAGE: LLM REQUESTS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app_llm_requests (
    id TEXT PRIMARY KEY,
    model TEXT NOT NULL,
    input_tokens INTEGER,
    output_tokens INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_llm_requests_created ON app_llm_requests(created_at DESC);

--------------------------------------------------------------------------------
-- USAGE: LIMITS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app_usage_limits (
    service TEXT PRIMARY KEY,
    monthly_limit INTEGER NOT NULL,
    unit TEXT NOT NULL DEFAULT 'requests',
    limit_type TEXT NOT NULL DEFAULT 'hard' CHECK (limit_type IN ('hard', 'soft')),
    enabled INTEGER DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_usage_limits_enabled ON app_usage_limits(service) WHERE enabled = 1;

CREATE TRIGGER IF NOT EXISTS app_usage_limits_set_updated_at
    AFTER UPDATE ON app_usage_limits
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_usage_limits SET updated_at = datetime('now') WHERE service = NEW.service;
END;

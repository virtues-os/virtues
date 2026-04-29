-- Migration 032: Unified Scheduler
--
-- Replaces 5 fragmented scheduling systems with 2 tables:
-- - scheduled_tasks: what runs and when (pure config, no state)
-- - task_runs: what happened (all execution history)
--
-- Drops: elt_jobs table, agent columns from app_chats, cron_schedule from elt_stream_connections

------------------------------------------------------------
-- Step 1: Create new tables
------------------------------------------------------------

CREATE TABLE scheduled_tasks (
    id TEXT PRIMARY KEY,
    task_type TEXT NOT NULL CHECK (task_type IN ('sync', 'agent', 'function')),
    name TEXT NOT NULL,
    cron_schedule TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    config TEXT NOT NULL DEFAULT '{}',
    activation_code TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_scheduled_tasks_enabled ON scheduled_tasks(task_type)
    WHERE enabled = 1;

CREATE TABLE task_runs (
    id TEXT PRIMARY KEY,
    task_id TEXT REFERENCES scheduled_tasks(id),
    status TEXT NOT NULL DEFAULT 'running'
        CHECK (status IN ('running', 'success', 'error', 'cancelled')),
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT,
    records_processed INTEGER NOT NULL DEFAULT 0,
    error TEXT,
    trigger TEXT NOT NULL DEFAULT 'cron',
    parent_run_id TEXT REFERENCES task_runs(id),
    transform_stage TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_task_runs_task ON task_runs(task_id, created_at DESC);
CREATE INDEX idx_task_runs_status ON task_runs(status) WHERE status = 'running';
CREATE INDEX idx_task_runs_parent ON task_runs(parent_run_id) WHERE parent_run_id IS NOT NULL;

------------------------------------------------------------
-- Step 2: Migrate existing data
------------------------------------------------------------

-- Sync tasks from elt_stream_connections
INSERT INTO scheduled_tasks (id, task_type, name, cron_schedule, enabled, config, created_at)
SELECT
    'task_sync_' || st.id,
    'sync',
    sc.name || ' / ' || st.stream_name,
    st.cron_schedule,
    CASE WHEN st.is_enabled = 1 AND sc.is_active = 1 THEN 1 ELSE 0 END,
    json_object('source_connection_id', st.source_connection_id, 'stream_name', st.stream_name),
    st.created_at
FROM elt_stream_connections st
JOIN elt_source_connections sc ON st.source_connection_id = sc.id
WHERE st.cron_schedule IS NOT NULL
  AND sc.source NOT IN ('mac', 'ios');

-- Agent tasks from app_chats
INSERT INTO scheduled_tasks (id, task_type, name, cron_schedule, enabled, config, activation_code, created_at)
SELECT
    'task_agent_' || c.id,
    'agent',
    c.title,
    c.agent_trigger,
    CASE WHEN c.agent_state IN ('scheduled', 'listening') THEN 1 ELSE 0 END,
    json_object('chat_id', c.id, 'trigger_token', c.agent_trigger_token),
    c.agent_activation,
    c.created_at
FROM app_chats c
WHERE c.agent_state IS NOT NULL;

-- Internal function tasks (seeded)
INSERT INTO scheduled_tasks (id, task_type, name, cron_schedule, enabled, config) VALUES
    ('task_fn_daily_summary',   'function', 'Daily Summary',      '0 0 * * * *',    1, '{"function_name":"daily_summary"}'),
    ('task_fn_embedding_index', 'function', 'Embedding Indexer',  '0 */15 * * * *',  1, '{"function_name":"embedding_index"}'),
    ('task_fn_trash_purge',     'function', 'Drive Trash Purge',  '0 0 3 * * *',     1, '{"function_name":"trash_purge"}');

-- Migrate elt_jobs → task_runs (historical run data)
-- task_id is nullable: sync jobs get linked, transform jobs get NULL (they have parent_run_id)
INSERT INTO task_runs (id, task_id, status, started_at, completed_at, records_processed, error, trigger, parent_run_id, transform_stage, created_at)
SELECT
    j.id,
    CASE WHEN j.job_type = 'sync' THEN 'task_sync_' || st.id ELSE NULL END,
    CASE j.status
        WHEN 'succeeded' THEN 'success'
        WHEN 'failed' THEN 'error'
        WHEN 'cancelled' THEN 'cancelled'
        ELSE 'running'
    END,
    j.started_at,
    j.completed_at,
    j.records_processed,
    j.error_message,
    'cron',
    j.parent_job_id,
    j.transform_stage,
    j.created_at
FROM elt_jobs j
LEFT JOIN elt_stream_connections st
    ON j.source_connection_id = st.source_connection_id
    AND j.stream_name = st.stream_name;

------------------------------------------------------------
-- Step 3: Drop old tables and columns
------------------------------------------------------------

-- Drop elt_jobs (fully replaced by task_runs)
DROP TABLE IF EXISTS elt_jobs;

-- Recreate app_chats without agent scheduling columns
-- Keep: id, title, message_count, trace, conversation_summary, summary_up_to_index,
--       summary_version, last_compacted_at, icon, agent_instruction, created_at, updated_at
-- Drop: agent_state, agent_trigger, agent_activation, agent_last_run_at, agent_trigger_token
CREATE TABLE app_chats_new (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    message_count INTEGER DEFAULT 0,
    trace TEXT,
    conversation_summary TEXT,
    summary_up_to_index INTEGER DEFAULT 0,
    summary_version INTEGER DEFAULT 0,
    last_compacted_at TEXT,
    icon TEXT,
    agent_instruction TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO app_chats_new
SELECT id, title, message_count, trace, conversation_summary,
    summary_up_to_index, summary_version, last_compacted_at, icon, agent_instruction,
    created_at, updated_at
FROM app_chats;

DROP TABLE app_chats;
ALTER TABLE app_chats_new RENAME TO app_chats;

CREATE INDEX idx_chats_updated ON app_chats(updated_at DESC);

-- Recreate the updated_at trigger
CREATE TRIGGER app_chats_set_updated_at
    AFTER UPDATE ON app_chats
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_chats SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- Drop cron_schedule from elt_stream_connections
ALTER TABLE elt_stream_connections DROP COLUMN cron_schedule;

-- Drop stale indexes
DROP INDEX IF EXISTS idx_app_chats_agent_state;

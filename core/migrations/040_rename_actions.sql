-- 040: Rename scheduled_tasks/task_runs to app_actions/app_action_runs
--
-- Universalizes the scheduler vocabulary: everything scheduled is now an "action".
-- Actions have a subtype (action_type) indicating what runs:
--   - 'sync'   = data pipeline (fetch → transform → write), no LLM
--   - 'agent'  = LLM agent loop with chat, instruction, optional activation gate
--   - 'system' = hardcoded Rust job (embedding indexer, trash purge)
--
-- Also:
--   - Renames task_type column → action_type
--   - Removes dead 'function' rows from migration 032 (never read)
--   - Seeds system actions for embedding + trash_purge (previously hardcoded)

-- ─────────────────────────────────────────────────────────────────────────────
-- 1. RECREATE app_actions from scheduled_tasks
-- ─────────────────────────────────────────────────────────────────────────────

CREATE TABLE app_actions (
    id TEXT PRIMARY KEY,
    action_type TEXT NOT NULL CHECK (action_type IN ('sync', 'agent', 'system')),
    name TEXT NOT NULL,
    cron_schedule TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    config TEXT NOT NULL DEFAULT '{}',
    activation_code TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Migrate existing rows, mapping old values:
--   'function' → 'system' (repurposed for hardcoded jobs)
--   'action'   → 'agent'  (the LLM subtype)
--   'sync'     → 'sync'   (unchanged)
-- Skip the 3 dead function rows from migration 032 (never executed).
INSERT INTO app_actions (id, action_type, name, cron_schedule, enabled, config, activation_code, created_at, updated_at)
SELECT
    id,
    CASE task_type
        WHEN 'action' THEN 'agent'
        WHEN 'function' THEN 'system'
        ELSE task_type
    END,
    name, cron_schedule, enabled, config, activation_code, created_at, updated_at
FROM scheduled_tasks
WHERE id NOT IN ('task_fn_daily_summary', 'task_fn_embedding_index', 'task_fn_trash_purge');

CREATE INDEX idx_app_actions_enabled ON app_actions(action_type) WHERE enabled = 1;

-- ─────────────────────────────────────────────────────────────────────────────
-- 2. RECREATE app_action_runs from task_runs
-- ─────────────────────────────────────────────────────────────────────────────

CREATE TABLE app_action_runs (
    id TEXT PRIMARY KEY,
    action_id TEXT REFERENCES app_actions(id),
    status TEXT NOT NULL DEFAULT 'running'
        CHECK (status IN ('running', 'success', 'error', 'cancelled', 'skipped')),
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT,
    records_processed INTEGER NOT NULL DEFAULT 0,
    error TEXT,
    trigger TEXT NOT NULL DEFAULT 'cron',
    parent_run_id TEXT REFERENCES app_action_runs(id),
    transform_stage TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Migrate existing runs, but only for actions that still exist in app_actions
INSERT INTO app_action_runs (id, action_id, status, started_at, completed_at, records_processed, error, trigger, parent_run_id, transform_stage, created_at)
SELECT id, task_id, status, started_at, completed_at, records_processed, error, trigger, parent_run_id, transform_stage, created_at
FROM task_runs
WHERE task_id IS NULL OR task_id IN (SELECT id FROM app_actions);

CREATE INDEX idx_app_action_runs_action ON app_action_runs(action_id, created_at DESC);
CREATE INDEX idx_app_action_runs_status ON app_action_runs(status) WHERE status = 'running';
CREATE INDEX idx_app_action_runs_parent ON app_action_runs(parent_run_id) WHERE parent_run_id IS NOT NULL;

-- ─────────────────────────────────────────────────────────────────────────────
-- 3. DROP old tables
-- ─────────────────────────────────────────────────────────────────────────────

DROP TABLE task_runs;
DROP TABLE scheduled_tasks;

-- ─────────────────────────────────────────────────────────────────────────────
-- 4. SEED system actions (embedding indexer, trash purge)
-- ─────────────────────────────────────────────────────────────────────────────
-- These were previously scheduled via hardcoded Scheduler::schedule_*_job() calls.
-- Now they flow through app_actions like everything else. The scheduler reads
-- action_type='system' rows and dispatches to the right hardcoded function via
-- config.function_name.

INSERT INTO app_actions (id, action_type, name, cron_schedule, enabled, config)
VALUES
    ('action_system_embedding_index', 'system', 'Embedding Indexer',
     '0 */15 * * * *', 1, '{"function_name":"embedding_index"}'),
    ('action_system_trash_purge', 'system', 'Drive Trash Purge',
     '0 0 3 * * *', 1, '{"function_name":"trash_purge"}');

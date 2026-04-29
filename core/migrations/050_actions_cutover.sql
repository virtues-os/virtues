-- Migration 050: actions architecture cutover
--
-- Hard cutover from the legacy elt_* sources paradigm to the unified
-- app_actions paradigm. Pre-launch, no users, no rollback.
--
-- This migration:
--   1. Rebuilds app_actions: drops action_type + activation_code, renames
--      instruction → agent, adds triggers/condition columns.
--   2. Rebuilds app_action_runs: adds CHECK on trigger column enum.
--   3. Backfills triggers from the old action_type + maps legacy triggers.
--   4. Deletes orphaned legacy sync rows (non-iOS sources being removed).
--   5. Deletes stale system/dayline rows that templates.toml reseeds.
--   6. Drops elt_source_connections, elt_stream_connections, elt_stream_objects.

-- Defer FK checks to COMMIT time. Required because this migration rebuilds
-- a parent table (app_actions) that is referenced by app_action_runs via
-- FK. With foreign_keys=ON (set on every connection in database/mod.rs),
-- dropping app_actions before the child is rebuilt would otherwise error
-- with FK constraint failed. Deferring moves the check to COMMIT, by which
-- point both tables have been rebuilt and references are consistent.
-- The pragma is transaction-scoped and auto-resets on COMMIT.
PRAGMA defer_foreign_keys = 1;

-- ─────────────────────────────────────────────────────────────────────────────
-- 1. Rebuild app_actions
-- ─────────────────────────────────────────────────────────────────────────────
--
-- Old schema:
--   id, action_type CHECK(sync|agent|system), name, cron_schedule, enabled,
--   config, activation_code, instruction, owner CHECK, concurrency_mode CHECK,
--   memory, function_name, credential_id, created_at, updated_at
--
-- New schema:
--   id, name, owner CHECK, agent, cron_schedule, enabled, config, condition,
--   triggers, concurrency_mode CHECK, memory, function_name, credential_id,
--   created_at, updated_at

CREATE TABLE app_actions_new (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    owner TEXT NOT NULL DEFAULT 'user'
        CHECK (owner IN ('system', 'user')),
    agent TEXT,                                    -- was `instruction`
    cron_schedule TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    config TEXT NOT NULL DEFAULT '{}',
    condition TEXT,                                -- SQL expression; null = always run
    triggers TEXT NOT NULL DEFAULT '["cron"]',    -- JSON array of trigger names
    concurrency_mode TEXT NOT NULL DEFAULT 'single'
        CHECK (concurrency_mode IN ('single', 'skip', 'parallel')),
    memory TEXT,
    function_name TEXT,
    credential_id TEXT REFERENCES action_credentials(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Backfill. Map old action_type → triggers:
--   'sync'   → '["webhook"]'  (push-driven data ingest; all iOS actions)
--   'agent'  → '["cron","manual"]' (LLM agents on schedule, also user-runnable)
--   'system' → '["cron"]'    (scheduler-only housekeeping)
-- Rename column: instruction → agent
-- Drop: activation_code (Python gate replaced by SQL condition; null for now)

INSERT INTO app_actions_new (
    id, name, owner, agent, cron_schedule, enabled, config,
    condition, triggers, concurrency_mode, memory,
    function_name, credential_id, created_at, updated_at
)
SELECT
    id,
    name,
    owner,
    instruction,
    cron_schedule,
    enabled,
    config,
    NULL,                                          -- condition starts null; templates.toml seeds new values
    CASE action_type
        WHEN 'sync'   THEN '["webhook"]'
        WHEN 'agent'  THEN '["cron","manual"]'
        WHEN 'system' THEN '["cron"]'
        ELSE '["cron"]'
    END,
    concurrency_mode,
    memory,
    function_name,
    credential_id,
    created_at,
    updated_at
FROM app_actions;

-- Drop orphan legacy sync rows: any row that pointed at an elt source connection
-- (non-iOS) is about to lose its target. iOS rows from migration 047 have
-- function_name set, so the WHERE clause skips them. Also clear their run
-- history so the deferred FK check at COMMIT doesn't trip on orphaned children.
DELETE FROM app_action_runs
WHERE action_id IN (
    SELECT id FROM app_actions_new
    WHERE function_name IS NULL
      AND config LIKE '%source_connection_id%'
);
DELETE FROM app_actions_new
WHERE function_name IS NULL
  AND config LIKE '%source_connection_id%';

-- Delete stale system/dayline rows — templates.toml will reseed fresh rows
-- with the new ids and columns. Run history for these ids is also purged
-- to keep the FK graph consistent for the deferred check at COMMIT.
DELETE FROM app_action_runs WHERE action_id IN (
    'action_agent_dayline_hourly',
    'action_agent_dayline_eod',
    'action_system_embedding_index',
    'action_system_trash_purge',
    'action_system_dayline_illustration'
);
DELETE FROM app_actions_new WHERE id IN (
    'action_agent_dayline_hourly',
    'action_agent_dayline_eod',
    'action_system_embedding_index',
    'action_system_trash_purge',
    'action_system_dayline_illustration'
);

-- Swap
DROP TABLE app_actions;
ALTER TABLE app_actions_new RENAME TO app_actions;

-- Recreate indexes
CREATE INDEX idx_app_actions_enabled ON app_actions(enabled);
CREATE INDEX idx_app_actions_function_name ON app_actions(function_name) WHERE function_name IS NOT NULL;
CREATE INDEX idx_app_actions_credential_id ON app_actions(credential_id) WHERE credential_id IS NOT NULL;

-- Recreate updated_at trigger
CREATE TRIGGER app_actions_set_updated_at
    AFTER UPDATE ON app_actions
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_actions SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- ─────────────────────────────────────────────────────────────────────────────
-- 2. Rebuild app_action_runs to add CHECK constraint on trigger
-- ─────────────────────────────────────────────────────────────────────────────
--
-- Old: trigger TEXT NOT NULL DEFAULT 'cron'       (no CHECK)
-- New: trigger TEXT NOT NULL DEFAULT 'cron' CHECK (trigger IN ('cron','manual','tool','api','webhook'))

CREATE TABLE app_action_runs_new (
    id TEXT PRIMARY KEY,
    action_id TEXT REFERENCES app_actions(id),
    status TEXT NOT NULL DEFAULT 'running'
        CHECK (status IN ('running', 'success', 'error', 'cancelled', 'skipped')),
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT,
    records_processed INTEGER NOT NULL DEFAULT 0,
    error TEXT,
    trigger TEXT NOT NULL DEFAULT 'cron'
        CHECK (trigger IN ('cron', 'manual', 'tool', 'api', 'webhook')),
    parent_run_id TEXT REFERENCES app_action_runs(id),
    transform_stage TEXT,
    result_summary TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Backfill: 'push' → 'webhook' (legacy naming)
INSERT INTO app_action_runs_new (
    id, action_id, status, started_at, completed_at, records_processed,
    error, trigger, parent_run_id, transform_stage, result_summary, created_at
)
SELECT
    id, action_id, status, started_at, completed_at, records_processed,
    error,
    CASE trigger WHEN 'push' THEN 'webhook' ELSE trigger END,
    parent_run_id, transform_stage, result_summary, created_at
FROM app_action_runs;

DROP TABLE app_action_runs;
ALTER TABLE app_action_runs_new RENAME TO app_action_runs;

CREATE INDEX idx_app_action_runs_action ON app_action_runs(action_id, created_at DESC);
CREATE INDEX idx_app_action_runs_status ON app_action_runs(status) WHERE status = 'running';
CREATE INDEX idx_app_action_runs_parent ON app_action_runs(parent_run_id) WHERE parent_run_id IS NOT NULL;

-- ─────────────────────────────────────────────────────────────────────────────
-- 3. Drop legacy elt_* stream tables; stub out elt_source_connections
-- ─────────────────────────────────────────────────────────────────────────────
-- All non-iOS sources (Google, Plaid, Strava, Notion, GitHub, Spotify) are
-- being removed from core. iOS credentials already live in action_credentials
-- (migration 047).
--
-- `elt_source_connections` is retained as an EMPTY STUB TABLE because a large
-- number of `data_*` ontology tables hold `source_connection_id` columns with
-- FK references to it (see migration 003). Dropping the table would leave the
-- FKs dangling, which SQLite tolerates at runtime (PRAGMA foreign_keys is off
-- during schema migrations) but the sqlx offline-query compiler does not — it
-- reads the schema directly and errors out when a referenced table is missing.
--
-- The stub table has no rows and no writes land in it. A future migration can
-- rebuild every data_* table to drop the FK column entirely.

DROP TABLE IF EXISTS elt_stream_objects;
DROP TABLE IF EXISTS elt_stream_connections;
DROP TABLE IF EXISTS elt_source_connections;

CREATE TABLE elt_source_connections (
    id TEXT PRIMARY KEY
);

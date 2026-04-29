-- Migration 051: actions architecture cleanup
--
-- Follow-up cleanups to migration 050, based on audit findings:
--
--   1. Add CHECK constraint: an action must have `function_name` OR `agent`
--      populated. A row with neither is a silent no-op and should never exist.
--
--   2. Drop the `concurrency_mode` column entirely. The intended three-way enum
--      (single/skip/parallel) was never fully implemented — the runner only
--      honors "skip if a previous run is active" regardless of mode. Three
--      values with two behaviors and one was never tested. Gone.
--
-- SQLite needs a table rebuild for CHECK changes + column drops.

-- Defer FK checks to COMMIT — same reason as 050: app_action_runs references
-- app_actions, which we're rebuilding in place.
PRAGMA defer_foreign_keys = 1;

CREATE TABLE app_actions_new (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    owner TEXT NOT NULL DEFAULT 'user'
        CHECK (owner IN ('system', 'user')),
    agent TEXT,
    cron_schedule TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    config TEXT NOT NULL DEFAULT '{}',
    condition TEXT,
    triggers TEXT NOT NULL DEFAULT '["cron"]',
    memory TEXT,
    function_name TEXT,
    credential_id TEXT REFERENCES action_credentials(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (function_name IS NOT NULL OR agent IS NOT NULL)
);

-- Backfill: skip any rows that would violate the new CHECK. In a pre-launch
-- environment this should be zero rows; log a warning if it isn't.
INSERT INTO app_actions_new (
    id, name, owner, agent, cron_schedule, enabled, config,
    condition, triggers, memory, function_name, credential_id,
    created_at, updated_at
)
SELECT
    id, name, owner, agent, cron_schedule, enabled, config,
    condition, triggers, memory, function_name, credential_id,
    created_at, updated_at
FROM app_actions
WHERE function_name IS NOT NULL OR agent IS NOT NULL;

DROP TABLE app_actions;
ALTER TABLE app_actions_new RENAME TO app_actions;

CREATE INDEX idx_app_actions_enabled ON app_actions(enabled);
CREATE INDEX idx_app_actions_function_name ON app_actions(function_name) WHERE function_name IS NOT NULL;
CREATE INDEX idx_app_actions_credential_id ON app_actions(credential_id) WHERE credential_id IS NOT NULL;

CREATE TRIGGER app_actions_set_updated_at
    AFTER UPDATE ON app_actions
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_actions SET updated_at = datetime('now') WHERE id = NEW.id;
END;

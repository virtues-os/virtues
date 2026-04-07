-- 035: Dayline Events + Actions Rename
--
-- 1. Recreate wiki_events: drop W6H columns, add Dayline columns
-- 2. Recreate wiki_days: drop W6H columns, add body battery columns
-- 3. Rename "persistent agents" to "actions" (column + task_type)
-- 4. Drop wiki_day_embeddings (W6H dimension embeddings, no longer used)

-- =============================================================================
-- PART 1: Recreate wiki_events (drop W6H, add Dayline)
-- =============================================================================

-- Backup
CREATE TABLE wiki_events_backup AS SELECT * FROM wiki_events;

-- Drop original + triggers + indexes
DROP TRIGGER IF EXISTS wiki_events_set_updated_at;
DROP INDEX IF EXISTS idx_wiki_events_day;
DROP INDEX IF EXISTS idx_wiki_events_time;
DROP TABLE wiki_events;

-- Recreate with new schema
CREATE TABLE wiki_events (
    id TEXT PRIMARY KEY,
    day_id TEXT NOT NULL REFERENCES wiki_days(id) ON DELETE CASCADE,
    -- Time range
    start_time TEXT NOT NULL,
    end_time TEXT NOT NULL,
    -- Auto-generated labels
    auto_label TEXT,
    auto_location TEXT,
    -- User overrides
    user_label TEXT,
    user_location TEXT,
    user_notes TEXT,
    -- Source tracking
    source_ontologies TEXT DEFAULT '[]',  -- JSON array of ontology record IDs
    -- Flags
    is_unknown INTEGER DEFAULT 0,
    is_transit INTEGER DEFAULT 0,
    is_user_added INTEGER DEFAULT 0,
    is_user_edited INTEGER DEFAULT 0,
    -- Dayline: Novelty
    embedding BLOB,                      -- 768-dim f32 nomic-embed (reused from W6H era)
    novelty_z REAL,                      -- z-scored novelty vs 12-week baseline
    -- Dayline: Event structure
    topics TEXT DEFAULT '[]',            -- JSON array of activity contexts
    event_summary TEXT,                  -- 1-3 sentence factual summary (embedded for novelty)
    agent_action TEXT,                   -- NEW/CONTINUE/REVISE/NO_DATA
    -- Dayline: Classification
    is_sleep INTEGER DEFAULT 0,
    user_hidden INTEGER DEFAULT 0,       -- soft delete
    user_created INTEGER DEFAULT 0,      -- user-created, never modified by recompute
    -- Audit
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_wiki_events_day ON wiki_events(day_id);
CREATE INDEX IF NOT EXISTS idx_wiki_events_time ON wiki_events(start_time, end_time);

CREATE TRIGGER IF NOT EXISTS wiki_events_set_updated_at
    AFTER UPDATE ON wiki_events
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE wiki_events SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- Restore data (map old columns to new, drop w6h_activation/entropy/w6h_entropy)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, user_label, user_location, user_notes,
    source_ontologies, is_unknown, is_transit, is_user_added, is_user_edited,
    embedding,
    created_at, updated_at
)
SELECT
    id, day_id, start_time, end_time,
    auto_label, auto_location, user_label, user_location, user_notes,
    source_ontologies, is_unknown, is_transit, is_user_added, is_user_edited,
    embedding,
    created_at, updated_at
FROM wiki_events_backup;

DROP TABLE wiki_events_backup;

-- =============================================================================
-- PART 2: Recreate wiki_days (drop W6H, add body battery)
-- =============================================================================

-- Backup
CREATE TABLE wiki_days_backup AS SELECT * FROM wiki_days;

-- Drop original + triggers + indexes
DROP TRIGGER IF EXISTS wiki_days_set_updated_at;
DROP INDEX IF EXISTS idx_wiki_days_date;
DROP INDEX IF EXISTS idx_wiki_days_act;
DROP INDEX IF EXISTS idx_wiki_days_chapter;
DROP TABLE wiki_days;

-- Recreate with new schema
CREATE TABLE wiki_days (
    id TEXT PRIMARY KEY,
    date TEXT NOT NULL UNIQUE,
    start_timezone TEXT,
    end_timezone TEXT,
    autobiography TEXT,
    autobiography_sections TEXT,  -- JSON
    last_edited_by TEXT DEFAULT 'ai' CHECK (last_edited_by IN ('ai', 'human')),
    act_id TEXT REFERENCES wiki_acts(id),
    chapter_id TEXT REFERENCES wiki_chapters(id),
    -- Body battery (V2 — NULL until implemented)
    morning_baseline REAL,       -- 0-1 sigmoid of overnight recovery z-scores
    battery_curve TEXT,           -- JSON array of hourly battery values
    -- Wiki fields
    cover_image TEXT,
    snapshot TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_wiki_days_date ON wiki_days(date DESC);
CREATE INDEX IF NOT EXISTS idx_wiki_days_act ON wiki_days(act_id) WHERE act_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_wiki_days_chapter ON wiki_days(chapter_id) WHERE chapter_id IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS wiki_days_set_updated_at
    AFTER UPDATE ON wiki_days
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE wiki_days SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- Restore data (drop context_vector, chaos_score, entropy_calibration_days)
INSERT INTO wiki_days (
    id, date, start_timezone, end_timezone,
    autobiography, autobiography_sections, last_edited_by,
    act_id, chapter_id,
    cover_image, snapshot,
    created_at, updated_at
)
SELECT
    id, date, start_timezone, end_timezone,
    autobiography, autobiography_sections, last_edited_by,
    act_id, chapter_id,
    cover_image, snapshot,
    created_at, updated_at
FROM wiki_days_backup;

DROP TABLE wiki_days_backup;

-- =============================================================================
-- PART 3: Rename "persistent agents" to "actions"
-- =============================================================================

-- Rename column on app_chats
ALTER TABLE app_chats RENAME COLUMN agent_instruction TO action_instruction;

-- Recreate scheduled_tasks with corrected CHECK constraint ('agent' → 'action')
CREATE TABLE scheduled_tasks_backup AS SELECT * FROM scheduled_tasks;
DROP INDEX IF EXISTS idx_scheduled_tasks_enabled;
DROP TABLE scheduled_tasks;

CREATE TABLE scheduled_tasks (
    id TEXT PRIMARY KEY,
    task_type TEXT NOT NULL CHECK (task_type IN ('sync', 'action', 'function')),
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

INSERT INTO scheduled_tasks (id, task_type, name, cron_schedule, enabled, config, activation_code, created_at, updated_at)
SELECT id,
    CASE WHEN task_type = 'agent' THEN 'action' ELSE task_type END,
    name, cron_schedule, enabled, config, activation_code, created_at, updated_at
FROM scheduled_tasks_backup;

DROP TABLE scheduled_tasks_backup;

-- Recreate task_runs with corrected CHECK (add 'skipped' status)
CREATE TABLE task_runs_backup AS SELECT * FROM task_runs;
DROP INDEX IF EXISTS idx_task_runs_task;
DROP INDEX IF EXISTS idx_task_runs_status;
DROP INDEX IF EXISTS idx_task_runs_parent;
DROP TABLE task_runs;

CREATE TABLE task_runs (
    id TEXT PRIMARY KEY,
    task_id TEXT REFERENCES scheduled_tasks(id),
    status TEXT NOT NULL DEFAULT 'running'
        CHECK (status IN ('running', 'success', 'error', 'cancelled', 'skipped')),
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

INSERT INTO task_runs (id, task_id, status, started_at, completed_at, records_processed, error, trigger, parent_run_id, transform_stage, created_at)
SELECT id, task_id, status, started_at, completed_at, records_processed, error, trigger, parent_run_id, transform_stage, created_at
FROM task_runs_backup;

DROP TABLE task_runs_backup;

-- =============================================================================
-- PART 4: Drop W6H dimension embeddings table (no longer used)
-- =============================================================================

DROP TABLE IF EXISTS wiki_day_embeddings;

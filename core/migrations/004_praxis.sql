-- Praxis: Task, Initiative, Aspiration, Calendar (SQLite)

--------------------------------------------------------------------------------
-- PRAXIS: TASK
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_praxis_task (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT,
    tags TEXT DEFAULT '[]',  -- JSON array
    thing_id TEXT REFERENCES data_entities_thing(id),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'completed', 'archived')),
    progress_percent INTEGER,
    start_date TEXT,
    target_date TEXT,
    completed_date TEXT,

    -- Habit tracking
    recurrence_rule TEXT,
    is_habit INTEGER DEFAULT 0,
    current_streak INTEGER DEFAULT 0,
    best_streak INTEGER DEFAULT 0,
    last_completed_date TEXT,

    -- Hierarchy
    initiative_id TEXT,
    parent_task_id TEXT,

    -- External source
    source_provider TEXT DEFAULT 'internal',
    external_id TEXT,
    external_url TEXT,

    -- Context
    purpose TEXT,
    context_energy TEXT,
    context_location TEXT,
    estimated_minutes INTEGER,
    actual_minutes INTEGER,

    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_praxis_task_status
    ON data_praxis_task(status) WHERE status = 'active';
CREATE INDEX IF NOT EXISTS idx_praxis_task_habit
    ON data_praxis_task(id) WHERE is_habit = 1;
CREATE INDEX IF NOT EXISTS idx_praxis_task_thing
    ON data_praxis_task(thing_id) WHERE thing_id IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS data_praxis_task_set_updated_at
    AFTER UPDATE ON data_praxis_task
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_praxis_task SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- PRAXIS: INITIATIVE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_praxis_initiative (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT,
    tags TEXT DEFAULT '[]',  -- JSON array
    thing_id TEXT REFERENCES data_entities_thing(id),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'paused', 'completed', 'archived')),
    progress_percent INTEGER,
    start_date TEXT,
    target_date TEXT,
    completed_date TEXT,

    -- Hierarchy
    parent_initiative_id TEXT,

    -- External source
    source_provider TEXT DEFAULT 'internal',
    external_id TEXT,
    external_url TEXT,

    -- Context
    purpose TEXT,
    is_commitment INTEGER DEFAULT 0,
    success_metrics TEXT,  -- JSON
    current_metrics TEXT,  -- JSON

    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_praxis_initiative_status
    ON data_praxis_initiative(status) WHERE status = 'active';
CREATE INDEX IF NOT EXISTS idx_praxis_initiative_thing
    ON data_praxis_initiative(thing_id) WHERE thing_id IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS data_praxis_initiative_set_updated_at
    AFTER UPDATE ON data_praxis_initiative
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_praxis_initiative SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- PRAXIS: ASPIRATION
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_praxis_aspiration (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT,
    tags TEXT DEFAULT '[]',  -- JSON array
    thing_id TEXT REFERENCES data_entities_thing(id),
    status TEXT NOT NULL DEFAULT 'dreaming' CHECK (status IN ('dreaming', 'activated', 'achieved', 'archived')),
    target_timeframe TEXT,
    achieved_date TEXT,

    -- Activation
    activated_date TEXT,
    activated_as_initiative_id TEXT,

    -- External source
    source_provider TEXT DEFAULT 'internal',
    external_id TEXT,
    external_url TEXT,

    -- Context
    purpose TEXT,
    target_date TEXT,

    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_praxis_aspiration_status
    ON data_praxis_aspiration(status) WHERE status IN ('dreaming', 'activated');
CREATE INDEX IF NOT EXISTS idx_praxis_aspiration_thing
    ON data_praxis_aspiration(thing_id) WHERE thing_id IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS data_praxis_aspiration_set_updated_at
    AFTER UPDATE ON data_praxis_aspiration
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_praxis_aspiration SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- PRAXIS: CALENDAR
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_praxis_calendar (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT,
    event_type TEXT,

    -- People
    organizer_identifier TEXT,
    attendee_identifiers TEXT DEFAULT '[]',  -- JSON array
    organizer_person_id TEXT REFERENCES data_entities_person(id),
    attendee_person_ids TEXT DEFAULT '[]',  -- JSON array

    -- Context
    thing_id TEXT REFERENCES data_entities_thing(id),
    place_id TEXT REFERENCES data_entities_place(id),
    location_name TEXT,

    -- Conference
    conference_url TEXT,
    conference_platform TEXT,

    -- Time
    start_time TEXT NOT NULL,
    end_time TEXT NOT NULL,
    is_all_day INTEGER DEFAULT 0,
    timezone TEXT,
    recurrence_rule TEXT,

    -- Links
    task_id TEXT,
    initiative_id TEXT,

    -- Time blocking
    block_type TEXT,
    is_sacred INTEGER DEFAULT 0,

    -- External source
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    external_id TEXT,
    external_url TEXT,

    metadata TEXT DEFAULT '{}',  -- JSON
    -- Vector embedding deferred
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_praxis_calendar_start
    ON data_praxis_calendar(start_time DESC);
CREATE INDEX IF NOT EXISTS idx_praxis_calendar_thing
    ON data_praxis_calendar(thing_id) WHERE thing_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_praxis_calendar_place
    ON data_praxis_calendar(place_id) WHERE place_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_praxis_calendar_block_type
    ON data_praxis_calendar(block_type) WHERE block_type IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS data_praxis_calendar_set_updated_at
    AFTER UPDATE ON data_praxis_calendar
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_praxis_calendar SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- PRAXIS: PRUDENT CONTEXT SNAPSHOT
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_prudent_context_snapshot (
    id TEXT PRIMARY KEY,
    computed_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT NOT NULL,
    context_data TEXT NOT NULL,  -- JSON
    llm_model TEXT,
    token_count INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_prudent_context_expires
    ON data_prudent_context_snapshot(expires_at DESC, computed_at DESC);

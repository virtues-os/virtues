-- Narrative: Telos, Acts, Chapters, Day, User Profile (SQLite)

--------------------------------------------------------------------------------
-- TELOS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_telos (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT,
    is_active INTEGER DEFAULT 1,
    -- Vector embedding deferred
    -- Wiki fields
    slug TEXT UNIQUE,
    content TEXT,
    cover_image TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_telos_single_active
    ON data_telos(is_active) WHERE is_active = 1;
CREATE UNIQUE INDEX IF NOT EXISTS idx_telos_slug
    ON data_telos(slug) WHERE slug IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS data_telos_set_updated_at
    AFTER UPDATE ON data_telos
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_telos SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- NARRATIVE: ACT
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_narrative_act (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    subtitle TEXT,
    description TEXT,
    start_date TEXT NOT NULL,
    end_date TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    telos_id TEXT REFERENCES data_telos(id),
    themes TEXT,  -- JSON array
    metadata TEXT DEFAULT '{}',  -- JSON
    -- Wiki fields
    slug TEXT UNIQUE,
    content TEXT,
    cover_image TEXT,
    location TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_narrative_act_dates
    ON data_narrative_act(start_date, end_date);
CREATE INDEX IF NOT EXISTS idx_narrative_act_order
    ON data_narrative_act(sort_order);
CREATE INDEX IF NOT EXISTS idx_narrative_act_telos
    ON data_narrative_act(telos_id) WHERE telos_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_narrative_act_slug
    ON data_narrative_act(slug) WHERE slug IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS data_narrative_act_set_updated_at
    AFTER UPDATE ON data_narrative_act
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_narrative_act SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- NARRATIVE: CHAPTER
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_narrative_chapter (
    id TEXT PRIMARY KEY,
    act_id TEXT REFERENCES data_narrative_act(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    subtitle TEXT,
    description TEXT,
    start_date TEXT NOT NULL,
    end_date TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    themes TEXT,  -- JSON array
    metadata TEXT DEFAULT '{}',  -- JSON
    -- Wiki fields
    slug TEXT UNIQUE,
    content TEXT,
    cover_image TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_narrative_chapter_act
    ON data_narrative_chapter(act_id);
CREATE INDEX IF NOT EXISTS idx_narrative_chapter_dates
    ON data_narrative_chapter(start_date, end_date);
CREATE INDEX IF NOT EXISTS idx_narrative_chapter_order
    ON data_narrative_chapter(act_id, sort_order);
CREATE UNIQUE INDEX IF NOT EXISTS idx_narrative_chapter_slug
    ON data_narrative_chapter(slug) WHERE slug IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS data_narrative_chapter_set_updated_at
    AFTER UPDATE ON data_narrative_chapter
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_narrative_chapter SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- TEMPORAL: DAY
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_temporal_day (
    id TEXT PRIMARY KEY,
    date TEXT NOT NULL UNIQUE,
    start_timezone TEXT,  -- IANA timezone at 00:00
    end_timezone TEXT,    -- IANA timezone at 24:00
    autobiography TEXT,
    autobiography_sections TEXT,  -- JSON
    last_edited_by TEXT DEFAULT 'ai' CHECK (last_edited_by IN ('ai', 'human')),
    context_vector TEXT,  -- JSON
    act_id TEXT REFERENCES data_narrative_act(id),
    chapter_id TEXT REFERENCES data_narrative_chapter(id),
    -- Wiki fields (date serves as slug)
    cover_image TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_temporal_day_date
    ON data_temporal_day(date DESC);
CREATE INDEX IF NOT EXISTS idx_temporal_day_act
    ON data_temporal_day(act_id) WHERE act_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_temporal_day_chapter
    ON data_temporal_day(chapter_id) WHERE chapter_id IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS data_temporal_day_set_updated_at
    AFTER UPDATE ON data_temporal_day
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_temporal_day SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- USER PROFILE (singleton)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_user_profile (
    id TEXT PRIMARY KEY DEFAULT '00000000-0000-0000-0000-000000000001',
    full_name TEXT,
    preferred_name TEXT,
    birth_date TEXT,
    height_cm REAL,
    weight_kg REAL,
    ethnicity TEXT,
    occupation TEXT,
    employer TEXT,

    -- Onboarding: single status field
    onboarding_status TEXT NOT NULL DEFAULT 'welcome' CHECK (onboarding_status IN ('welcome', 'profile', 'places', 'tools', 'complete')),

    -- Home
    home_place_id TEXT REFERENCES data_entities_place(id),

    -- Preferences
    theme TEXT DEFAULT 'light',
    update_check_hour INTEGER DEFAULT 8 CHECK (update_check_hour >= 0 AND update_check_hour <= 23),

    -- Discovery context
    crux TEXT,
    technology_vision TEXT,
    pain_point_primary TEXT,
    pain_point_secondary TEXT,
    excited_features TEXT,  -- JSON array

    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),

    CONSTRAINT user_profile_singleton CHECK (id = '00000000-0000-0000-0000-000000000001')
);

CREATE TRIGGER IF NOT EXISTS data_user_profile_set_updated_at
    AFTER UPDATE ON data_user_profile
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_user_profile SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- Insert singleton row
INSERT OR IGNORE INTO data_user_profile (id)
VALUES ('00000000-0000-0000-0000-000000000001');

--------------------------------------------------------------------------------
-- CITATION (links markdown [1] references to ontology data)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_citation (
    id TEXT PRIMARY KEY,
    -- Source: which wiki page contains this citation
    source_type TEXT NOT NULL CHECK (source_type IN (
        'person', 'place', 'organization', 'thing',
        'telos', 'act', 'chapter', 'day'
    )),
    source_id TEXT NOT NULL,
    -- Target: which ontology row this citation references
    target_table TEXT NOT NULL,
    target_id TEXT NOT NULL,
    -- Citation display
    citation_index INTEGER NOT NULL,
    label TEXT,
    preview TEXT,
    is_hidden INTEGER DEFAULT 0,
    added_by TEXT DEFAULT 'ai' CHECK (added_by IN ('ai', 'human')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    -- Each page has unique citation indices
    UNIQUE(source_type, source_id, citation_index)
);

CREATE INDEX IF NOT EXISTS idx_citation_source
    ON data_citation(source_type, source_id);
CREATE INDEX IF NOT EXISTS idx_citation_target
    ON data_citation(target_table, target_id);

CREATE TRIGGER IF NOT EXISTS data_citation_set_updated_at
    AFTER UPDATE ON data_citation
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_citation SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- TEMPORAL EVENT (day timeline events)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_temporal_event (
    id TEXT PRIMARY KEY,
    day_id TEXT NOT NULL REFERENCES data_temporal_day(id) ON DELETE CASCADE,
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
    source_ontologies TEXT DEFAULT '[]',  -- JSON array
    -- Flags
    is_unknown INTEGER DEFAULT 0,
    is_transit INTEGER DEFAULT 0,
    is_user_added INTEGER DEFAULT 0,
    is_user_edited INTEGER DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_temporal_event_day
    ON data_temporal_event(day_id);
CREATE INDEX IF NOT EXISTS idx_temporal_event_time
    ON data_temporal_event(start_time, end_time);

CREATE TRIGGER IF NOT EXISTS data_temporal_event_set_updated_at
    AFTER UPDATE ON data_temporal_event
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_temporal_event SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- Entities: Wiki, Narrative, Temporal (SQLite)
-- Consolidated from migrations 002, 005

--------------------------------------------------------------------------------
-- WIKI: PEOPLE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS wiki_people (
    id TEXT PRIMARY KEY,
    canonical_name TEXT NOT NULL,
    emails TEXT DEFAULT '[]',  -- JSON array
    phones TEXT DEFAULT '[]',  -- JSON array
    relationship_category TEXT,
    nickname TEXT,
    notes TEXT,
    first_interaction TEXT,
    last_interaction TEXT,
    interaction_count INTEGER DEFAULT 0,
    metadata TEXT DEFAULT '{}',  -- JSON
    -- Wiki fields
    slug TEXT UNIQUE,
    content TEXT,
    picture TEXT,
    cover_image TEXT,
    -- vCard fields
    birthday TEXT,
    instagram TEXT,
    facebook TEXT,
    linkedin TEXT,
    x TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_wiki_people_name ON wiki_people(canonical_name);
CREATE UNIQUE INDEX IF NOT EXISTS idx_wiki_people_slug ON wiki_people(slug) WHERE slug IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS wiki_people_set_updated_at
    AFTER UPDATE ON wiki_people
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE wiki_people SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- WIKI: PLACES
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS wiki_places (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    category TEXT,
    address TEXT,
    latitude REAL,
    longitude REAL,
    geo_boundary TEXT,  -- GeoJSON polygon
    radius_m REAL,
    visit_count INTEGER DEFAULT 0,
    first_visit TEXT,
    last_visit TEXT,
    metadata TEXT DEFAULT '{}',  -- JSON
    -- Wiki fields
    slug TEXT UNIQUE,
    content TEXT,
    cover_image TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_wiki_places_name ON wiki_places(name);
CREATE INDEX IF NOT EXISTS idx_wiki_places_location ON wiki_places(latitude, longitude);
CREATE UNIQUE INDEX IF NOT EXISTS idx_wiki_places_slug ON wiki_places(slug) WHERE slug IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS wiki_places_set_updated_at
    AFTER UPDATE ON wiki_places
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE wiki_places SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- WIKI: ORGS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS wiki_orgs (
    id TEXT PRIMARY KEY,
    canonical_name TEXT NOT NULL,
    organization_type TEXT,
    primary_place_id TEXT REFERENCES wiki_places(id),
    relationship_type TEXT,
    role_title TEXT,
    start_date TEXT,
    end_date TEXT,
    interaction_count INTEGER DEFAULT 0,
    first_interaction TEXT,
    last_interaction TEXT,
    metadata TEXT DEFAULT '{}',  -- JSON
    -- Wiki fields
    slug TEXT UNIQUE,
    content TEXT,
    cover_image TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_wiki_orgs_name ON wiki_orgs(canonical_name);
CREATE INDEX IF NOT EXISTS idx_wiki_orgs_type ON wiki_orgs(organization_type) WHERE organization_type IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_wiki_orgs_slug ON wiki_orgs(slug) WHERE slug IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS wiki_orgs_set_updated_at
    AFTER UPDATE ON wiki_orgs
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE wiki_orgs SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- WIKI: CONNECTIONS (edges between wiki entries)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS wiki_connections (
    id TEXT PRIMARY KEY,
    source_type TEXT NOT NULL CHECK (source_type IN ('person', 'place', 'organization')),
    source_id TEXT NOT NULL,
    target_type TEXT NOT NULL CHECK (target_type IN ('person', 'place', 'organization')),
    target_id TEXT NOT NULL,
    relationship TEXT NOT NULL,
    strength REAL,
    provenance TEXT,
    evidence TEXT,  -- JSON
    first_seen TEXT,
    last_seen TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(source_type, source_id, target_type, target_id, relationship)
);

CREATE INDEX IF NOT EXISTS idx_wiki_connections_source ON wiki_connections(source_type, source_id);
CREATE INDEX IF NOT EXISTS idx_wiki_connections_target ON wiki_connections(target_type, target_id);
CREATE INDEX IF NOT EXISTS idx_wiki_connections_relationship ON wiki_connections(relationship);

CREATE TRIGGER IF NOT EXISTS wiki_connections_set_updated_at
    AFTER UPDATE ON wiki_connections
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE wiki_connections SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- NARRATIVE: TELOS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS narrative_telos (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT,
    is_active INTEGER DEFAULT 1,
    -- Wiki fields
    slug TEXT UNIQUE,
    content TEXT,
    cover_image TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_narrative_telos_single_active ON narrative_telos(is_active) WHERE is_active = 1;
CREATE UNIQUE INDEX IF NOT EXISTS idx_narrative_telos_slug ON narrative_telos(slug) WHERE slug IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS narrative_telos_set_updated_at
    AFTER UPDATE ON narrative_telos
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE narrative_telos SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- NARRATIVE: ACTS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS narrative_acts (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    subtitle TEXT,
    description TEXT,
    start_date TEXT NOT NULL,
    end_date TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    telos_id TEXT REFERENCES narrative_telos(id),
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

CREATE INDEX IF NOT EXISTS idx_narrative_acts_dates ON narrative_acts(start_date, end_date);
CREATE INDEX IF NOT EXISTS idx_narrative_acts_order ON narrative_acts(sort_order);
CREATE INDEX IF NOT EXISTS idx_narrative_acts_telos ON narrative_acts(telos_id) WHERE telos_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_narrative_acts_slug ON narrative_acts(slug) WHERE slug IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS narrative_acts_set_updated_at
    AFTER UPDATE ON narrative_acts
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE narrative_acts SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- NARRATIVE: CHAPTERS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS narrative_chapters (
    id TEXT PRIMARY KEY,
    act_id TEXT REFERENCES narrative_acts(id) ON DELETE CASCADE,
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

CREATE INDEX IF NOT EXISTS idx_narrative_chapters_act ON narrative_chapters(act_id);
CREATE INDEX IF NOT EXISTS idx_narrative_chapters_dates ON narrative_chapters(start_date, end_date);
CREATE INDEX IF NOT EXISTS idx_narrative_chapters_order ON narrative_chapters(act_id, sort_order);
CREATE UNIQUE INDEX IF NOT EXISTS idx_narrative_chapters_slug ON narrative_chapters(slug) WHERE slug IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS narrative_chapters_set_updated_at
    AFTER UPDATE ON narrative_chapters
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE narrative_chapters SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- WIKI: DAYS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS wiki_days (
    id TEXT PRIMARY KEY,
    date TEXT NOT NULL UNIQUE,
    start_timezone TEXT,  -- IANA timezone at 00:00
    end_timezone TEXT,    -- IANA timezone at 24:00
    autobiography TEXT,
    autobiography_sections TEXT,  -- JSON
    last_edited_by TEXT DEFAULT 'ai' CHECK (last_edited_by IN ('ai', 'human')),
    context_vector TEXT,  -- JSON
    act_id TEXT REFERENCES narrative_acts(id),
    chapter_id TEXT REFERENCES narrative_chapters(id),
    -- Wiki fields (date serves as slug)
    cover_image TEXT,
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

--------------------------------------------------------------------------------
-- WIKI: YEARS (NEW - for /year/{year} routes)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS wiki_years (
    id TEXT PRIMARY KEY,
    year INTEGER NOT NULL UNIQUE,
    summary TEXT,
    highlights TEXT,  -- JSON array
    themes TEXT,      -- JSON array
    -- Wiki fields
    slug TEXT UNIQUE,
    content TEXT,
    cover_image TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_wiki_years_year ON wiki_years(year DESC);
CREATE UNIQUE INDEX IF NOT EXISTS idx_wiki_years_slug ON wiki_years(slug) WHERE slug IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS wiki_years_set_updated_at
    AFTER UPDATE ON wiki_years
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE wiki_years SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- WIKI: CITATIONS (links markdown [1] references to ontology data)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS wiki_citations (
    id TEXT PRIMARY KEY,
    -- Source: which wiki page contains this citation
    source_type TEXT NOT NULL CHECK (source_type IN (
        'person', 'place', 'organization',
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

CREATE INDEX IF NOT EXISTS idx_wiki_citations_source ON wiki_citations(source_type, source_id);
CREATE INDEX IF NOT EXISTS idx_wiki_citations_target ON wiki_citations(target_table, target_id);

CREATE TRIGGER IF NOT EXISTS wiki_citations_set_updated_at
    AFTER UPDATE ON wiki_citations
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE wiki_citations SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- WIKI: EVENTS (day timeline events)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS wiki_events (
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
    source_ontologies TEXT DEFAULT '[]',  -- JSON array
    -- Flags
    is_unknown INTEGER DEFAULT 0,
    is_transit INTEGER DEFAULT 0,
    is_user_added INTEGER DEFAULT 0,
    is_user_edited INTEGER DEFAULT 0,
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

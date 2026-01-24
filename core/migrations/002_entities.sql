-- Wiki: People, Places, Organizations, Things, Connections (SQLite)

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

CREATE INDEX IF NOT EXISTS idx_wiki_people_name
    ON wiki_people(canonical_name);
CREATE UNIQUE INDEX IF NOT EXISTS idx_wiki_people_slug
    ON wiki_people(slug) WHERE slug IS NOT NULL;

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
    -- Geospatial: replaced PostGIS geography with lat/lng columns
    latitude REAL,
    longitude REAL,
    -- For polygon boundaries, store as GeoJSON TEXT
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

CREATE INDEX IF NOT EXISTS idx_wiki_places_name
    ON wiki_places(name);
CREATE INDEX IF NOT EXISTS idx_wiki_places_location
    ON wiki_places(latitude, longitude);
CREATE UNIQUE INDEX IF NOT EXISTS idx_wiki_places_slug
    ON wiki_places(slug) WHERE slug IS NOT NULL;

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

CREATE INDEX IF NOT EXISTS idx_wiki_orgs_name
    ON wiki_orgs(canonical_name);
CREATE INDEX IF NOT EXISTS idx_wiki_orgs_type
    ON wiki_orgs(organization_type) WHERE organization_type IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_wiki_orgs_slug
    ON wiki_orgs(slug) WHERE slug IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS wiki_orgs_set_updated_at
    AFTER UPDATE ON wiki_orgs
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE wiki_orgs SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- WIKI: THINGS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS wiki_things (
    id TEXT PRIMARY KEY,
    canonical_name TEXT NOT NULL,
    thing_type TEXT,
    description TEXT,
    first_mentioned TEXT,
    last_mentioned TEXT,
    mention_count INTEGER DEFAULT 0,
    metadata TEXT DEFAULT '{}',  -- JSON
    -- Wiki fields
    slug TEXT UNIQUE,
    content TEXT,
    cover_image TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_wiki_things_name
    ON wiki_things(canonical_name);
CREATE INDEX IF NOT EXISTS idx_wiki_things_type
    ON wiki_things(thing_type) WHERE thing_type IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_wiki_things_slug
    ON wiki_things(slug) WHERE slug IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS wiki_things_set_updated_at
    AFTER UPDATE ON wiki_things
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE wiki_things SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- WIKI: CONNECTIONS (edges between wiki entries)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS wiki_connections (
    id TEXT PRIMARY KEY,
    source_type TEXT NOT NULL CHECK (source_type IN ('person', 'place', 'organization', 'thing')),
    source_id TEXT NOT NULL,
    target_type TEXT NOT NULL CHECK (target_type IN ('person', 'place', 'organization', 'thing')),
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

CREATE INDEX IF NOT EXISTS idx_wiki_connections_source
    ON wiki_connections(source_type, source_id);
CREATE INDEX IF NOT EXISTS idx_wiki_connections_target
    ON wiki_connections(target_type, target_id);
CREATE INDEX IF NOT EXISTS idx_wiki_connections_relationship
    ON wiki_connections(relationship);

CREATE TRIGGER IF NOT EXISTS wiki_connections_set_updated_at
    AFTER UPDATE ON wiki_connections
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE wiki_connections SET updated_at = datetime('now') WHERE id = NEW.id;
END;

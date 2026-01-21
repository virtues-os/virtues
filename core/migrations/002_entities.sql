-- Entities: Person, Place, Organization, Thing, Edges (SQLite)

--------------------------------------------------------------------------------
-- PERSON
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_entities_person (
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

CREATE INDEX IF NOT EXISTS idx_entities_person_name
    ON data_entities_person(canonical_name);
CREATE UNIQUE INDEX IF NOT EXISTS idx_entities_person_slug
    ON data_entities_person(slug) WHERE slug IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS data_entities_person_set_updated_at
    AFTER UPDATE ON data_entities_person
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_entities_person SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- PLACE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_entities_place (
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

CREATE INDEX IF NOT EXISTS idx_entities_place_name
    ON data_entities_place(name);
CREATE INDEX IF NOT EXISTS idx_entities_place_location
    ON data_entities_place(latitude, longitude);
CREATE UNIQUE INDEX IF NOT EXISTS idx_entities_place_slug
    ON data_entities_place(slug) WHERE slug IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS data_entities_place_set_updated_at
    AFTER UPDATE ON data_entities_place
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_entities_place SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- ORGANIZATION
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_entities_organization (
    id TEXT PRIMARY KEY,
    canonical_name TEXT NOT NULL,
    organization_type TEXT,
    primary_place_id TEXT REFERENCES data_entities_place(id),
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

CREATE INDEX IF NOT EXISTS idx_entities_organization_name
    ON data_entities_organization(canonical_name);
CREATE INDEX IF NOT EXISTS idx_entities_organization_type
    ON data_entities_organization(organization_type) WHERE organization_type IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_entities_organization_slug
    ON data_entities_organization(slug) WHERE slug IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS data_entities_organization_set_updated_at
    AFTER UPDATE ON data_entities_organization
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_entities_organization SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- THING
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_entities_thing (
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

CREATE INDEX IF NOT EXISTS idx_entities_thing_name
    ON data_entities_thing(canonical_name);
CREATE INDEX IF NOT EXISTS idx_entities_thing_type
    ON data_entities_thing(thing_type) WHERE thing_type IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_entities_thing_slug
    ON data_entities_thing(slug) WHERE slug IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS data_entities_thing_set_updated_at
    AFTER UPDATE ON data_entities_thing
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_entities_thing SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- ENTITY EDGES
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_entities_edges (
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

CREATE INDEX IF NOT EXISTS idx_entity_edges_source
    ON data_entities_edges(source_type, source_id);
CREATE INDEX IF NOT EXISTS idx_entity_edges_target
    ON data_entities_edges(target_type, target_id);
CREATE INDEX IF NOT EXISTS idx_entity_edges_relationship
    ON data_entities_edges(relationship);

CREATE TRIGGER IF NOT EXISTS data_entities_edges_set_updated_at
    AFTER UPDATE ON data_entities_edges
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_entities_edges SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- Entities: Person, Place, Organization, Thing, Edges

--------------------------------------------------------------------------------
-- PERSON
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.entities_person (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    canonical_name TEXT NOT NULL,
    emails TEXT[] DEFAULT '{}',
    phones TEXT[] DEFAULT '{}',
    relationship_category TEXT,
    nickname TEXT,
    notes TEXT,
    first_interaction TIMESTAMPTZ,
    last_interaction TIMESTAMPTZ,
    interaction_count INTEGER DEFAULT 0,
    metadata JSONB DEFAULT '{}',
    -- Wiki fields
    slug TEXT UNIQUE,
    content TEXT,
    picture TEXT,
    cover_image TEXT,
    -- vCard fields
    birthday DATE,
    instagram TEXT,
    facebook TEXT,
    linkedin TEXT,
    x TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_entities_person_name
    ON data.entities_person(canonical_name);
CREATE INDEX IF NOT EXISTS idx_entities_person_emails
    ON data.entities_person USING GIN(emails);
CREATE UNIQUE INDEX IF NOT EXISTS idx_entities_person_slug
    ON data.entities_person(slug) WHERE slug IS NOT NULL;

DROP TRIGGER IF EXISTS data_entities_person_set_updated_at ON data.entities_person;
CREATE TRIGGER data_entities_person_set_updated_at
    BEFORE UPDATE ON data.entities_person
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- PLACE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.entities_place (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    category TEXT,
    address TEXT,
    geo_center geography(POINT, 4326),
    geo_boundary geography(POLYGON, 4326),
    radius_m DOUBLE PRECISION,
    visit_count INTEGER DEFAULT 0,
    first_visit TIMESTAMPTZ,
    last_visit TIMESTAMPTZ,
    metadata JSONB DEFAULT '{}',
    -- Wiki fields
    slug TEXT UNIQUE,
    content TEXT,
    cover_image TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_entities_place_name
    ON data.entities_place(name);
CREATE INDEX IF NOT EXISTS idx_entities_place_geo_center
    ON data.entities_place USING GIST(geo_center);
CREATE INDEX IF NOT EXISTS idx_entities_place_geo_boundary
    ON data.entities_place USING GIST(geo_boundary);
CREATE UNIQUE INDEX IF NOT EXISTS idx_entities_place_slug
    ON data.entities_place(slug) WHERE slug IS NOT NULL;

DROP TRIGGER IF EXISTS data_entities_place_set_updated_at ON data.entities_place;
CREATE TRIGGER data_entities_place_set_updated_at
    BEFORE UPDATE ON data.entities_place
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- ORGANIZATION
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.entities_organization (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    canonical_name TEXT NOT NULL,
    organization_type TEXT,
    primary_place_id UUID REFERENCES data.entities_place(id),
    relationship_type TEXT,
    role_title TEXT,
    start_date DATE,
    end_date DATE,
    interaction_count INTEGER DEFAULT 0,
    first_interaction TIMESTAMPTZ,
    last_interaction TIMESTAMPTZ,
    metadata JSONB DEFAULT '{}',
    -- Wiki fields
    slug TEXT UNIQUE,
    content TEXT,
    cover_image TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_entities_organization_name
    ON data.entities_organization(canonical_name);
CREATE INDEX IF NOT EXISTS idx_entities_organization_type
    ON data.entities_organization(organization_type) WHERE organization_type IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_entities_organization_slug
    ON data.entities_organization(slug) WHERE slug IS NOT NULL;

DROP TRIGGER IF EXISTS data_entities_organization_set_updated_at ON data.entities_organization;
CREATE TRIGGER data_entities_organization_set_updated_at
    BEFORE UPDATE ON data.entities_organization
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- THING
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.entities_thing (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    canonical_name TEXT NOT NULL,
    thing_type TEXT,
    description TEXT,
    first_mentioned TIMESTAMPTZ,
    last_mentioned TIMESTAMPTZ,
    mention_count INTEGER DEFAULT 0,
    metadata JSONB DEFAULT '{}',
    -- Wiki fields
    slug TEXT UNIQUE,
    content TEXT,
    cover_image TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_entities_thing_name
    ON data.entities_thing(canonical_name);
CREATE INDEX IF NOT EXISTS idx_entities_thing_type
    ON data.entities_thing(thing_type) WHERE thing_type IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_entities_thing_slug
    ON data.entities_thing(slug) WHERE slug IS NOT NULL;

DROP TRIGGER IF EXISTS data_entities_thing_set_updated_at ON data.entities_thing;
CREATE TRIGGER data_entities_thing_set_updated_at
    BEFORE UPDATE ON data.entities_thing
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- ENTITY EDGES
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.entity_edges (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_type TEXT NOT NULL CHECK (source_type IN ('person', 'place', 'organization', 'thing')),
    source_id UUID NOT NULL,
    target_type TEXT NOT NULL CHECK (target_type IN ('person', 'place', 'organization', 'thing')),
    target_id UUID NOT NULL,
    relationship TEXT NOT NULL,
    strength FLOAT,
    provenance TEXT,
    evidence JSONB,
    first_seen TIMESTAMPTZ,
    last_seen TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(source_type, source_id, target_type, target_id, relationship)
);

CREATE INDEX IF NOT EXISTS idx_entity_edges_source
    ON data.entity_edges(source_type, source_id);
CREATE INDEX IF NOT EXISTS idx_entity_edges_target
    ON data.entity_edges(target_type, target_id);
CREATE INDEX IF NOT EXISTS idx_entity_edges_relationship
    ON data.entity_edges(relationship);

DROP TRIGGER IF EXISTS data_entity_edges_set_updated_at ON data.entity_edges;
CREATE TRIGGER data_entity_edges_set_updated_at
    BEFORE UPDATE ON data.entity_edges
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

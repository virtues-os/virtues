-- 0006 — ELT plumbing and wiki / narrative.
--
-- The ELT side is intentionally minimal — connection bookkeeping has migrated
-- into `credentials` (see 0004). What remains is the per-source identity stub
-- referenced by every data_* table, plus the per-stream sync checkpoint.
--
-- The wiki side is the user-facing knowledge graph: telos → acts → chapters
-- → years → days → events, plus first-class entities (people, places, orgs,
-- things) and the join table that ties any entity to any source record.

-- ---------------------------------------------------------------------------
-- ELT plumbing
-- ---------------------------------------------------------------------------
CREATE TABLE elt_source_connections (
    id TEXT PRIMARY KEY
);

CREATE TABLE elt_stream_checkpoints (
    id                  TEXT PRIMARY KEY,
    source_id           TEXT NOT NULL,
    stream_name         TEXT NOT NULL,
    checkpoint_key      TEXT NOT NULL,
    last_processed_at   TIMESTAMPTZ NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(source_id, stream_name, checkpoint_key)
);
CREATE INDEX idx_elt_stream_checkpoints_lookup
    ON elt_stream_checkpoints(source_id, stream_name, checkpoint_key);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON elt_stream_checkpoints
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

-- ---------------------------------------------------------------------------
-- Narrative spine: telos → acts → chapters → years
-- ---------------------------------------------------------------------------
CREATE TABLE wiki_telos (
    id           TEXT PRIMARY KEY,
    title        TEXT NOT NULL,
    description  TEXT,
    is_active    BOOLEAN NOT NULL DEFAULT TRUE,
    content      TEXT,
    cover_image  TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
-- Enforce a single active telos at a time.
CREATE UNIQUE INDEX idx_narrative_telos_single_active ON wiki_telos(is_active) WHERE is_active;
CREATE TRIGGER set_updated_at BEFORE UPDATE ON wiki_telos
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE wiki_acts (
    id           TEXT PRIMARY KEY,
    title        TEXT NOT NULL,
    subtitle     TEXT,
    description  TEXT,
    start_date   DATE NOT NULL,
    end_date     DATE,
    sort_order   INTEGER NOT NULL DEFAULT 0,
    telos_id     TEXT REFERENCES wiki_telos(id),
    themes       JSONB NOT NULL DEFAULT '[]'::jsonb,
    metadata     JSONB NOT NULL DEFAULT '{}'::jsonb,
    content      TEXT,
    cover_image  TEXT,
    location     TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_narrative_acts_dates ON wiki_acts(start_date, end_date);
CREATE INDEX idx_narrative_acts_order ON wiki_acts(sort_order);
CREATE INDEX idx_narrative_acts_telos ON wiki_acts(telos_id) WHERE telos_id IS NOT NULL;
CREATE TRIGGER set_updated_at BEFORE UPDATE ON wiki_acts
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE wiki_chapters (
    id           TEXT PRIMARY KEY,
    act_id       TEXT REFERENCES wiki_acts(id) ON DELETE CASCADE,
    title        TEXT NOT NULL,
    subtitle     TEXT,
    description  TEXT,
    start_date   DATE NOT NULL,
    end_date     DATE,
    sort_order   INTEGER NOT NULL DEFAULT 0,
    themes       JSONB NOT NULL DEFAULT '[]'::jsonb,
    metadata     JSONB NOT NULL DEFAULT '{}'::jsonb,
    content      TEXT,
    cover_image  TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_narrative_chapters_act    ON wiki_chapters(act_id);
CREATE INDEX idx_narrative_chapters_dates  ON wiki_chapters(start_date, end_date);
CREATE INDEX idx_narrative_chapters_order  ON wiki_chapters(act_id, sort_order);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON wiki_chapters
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE wiki_years (
    id           TEXT PRIMARY KEY,
    year         INTEGER NOT NULL UNIQUE,
    title        TEXT,
    description  TEXT,
    content      TEXT,
    cover_image  TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_wiki_years_year ON wiki_years(year DESC);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON wiki_years
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

-- ---------------------------------------------------------------------------
-- Entities: people, places, organizations, things
-- ---------------------------------------------------------------------------
CREATE TABLE wiki_people (
    id                     TEXT PRIMARY KEY,
    canonical_name         TEXT NOT NULL,
    emails                 JSONB NOT NULL DEFAULT '[]'::jsonb,
    phones                 JSONB NOT NULL DEFAULT '[]'::jsonb,
    relationship_category  TEXT,
    nickname               TEXT,
    notes                  TEXT,
    first_interaction      TIMESTAMPTZ,
    last_interaction       TIMESTAMPTZ,
    interaction_count      BIGINT NOT NULL DEFAULT 0,
    metadata               JSONB NOT NULL DEFAULT '{}'::jsonb,
    content                TEXT,
    picture                TEXT,
    cover_image            TEXT,
    birthday               DATE,
    instagram              TEXT,
    facebook               TEXT,
    linkedin               TEXT,
    x                      TEXT,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_wiki_people_name ON wiki_people(canonical_name);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON wiki_people
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE wiki_places (
    id               TEXT PRIMARY KEY,
    name             TEXT NOT NULL,
    category         TEXT,
    address          TEXT,
    latitude         DOUBLE PRECISION,
    longitude        DOUBLE PRECISION,
    radius_m         DOUBLE PRECISION NOT NULL DEFAULT 100.0,
    google_place_id  TEXT,
    visit_count      BIGINT NOT NULL DEFAULT 0,
    first_visit      TIMESTAMPTZ,
    last_visit       TIMESTAMPTZ,
    metadata         JSONB NOT NULL DEFAULT '{}'::jsonb,
    content          TEXT,
    cover_image      TEXT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_wiki_places_name     ON wiki_places(name);
CREATE INDEX idx_wiki_places_location ON wiki_places(latitude, longitude);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON wiki_places
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE wiki_orgs (
    id                  TEXT PRIMARY KEY,
    canonical_name      TEXT NOT NULL,
    organization_type   TEXT,
    relationship_type   TEXT,
    role_title          TEXT,
    start_date          DATE,
    end_date            DATE,
    interaction_count   BIGINT NOT NULL DEFAULT 0,
    first_interaction   TIMESTAMPTZ,
    last_interaction    TIMESTAMPTZ,
    metadata            JSONB NOT NULL DEFAULT '{}'::jsonb,
    content             TEXT,
    cover_image         TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_wiki_orgs_name ON wiki_orgs(canonical_name);
CREATE INDEX idx_wiki_orgs_type ON wiki_orgs(organization_type) WHERE organization_type IS NOT NULL;
CREATE TRIGGER set_updated_at BEFORE UPDATE ON wiki_orgs
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE wiki_things (
    id                        TEXT PRIMARY KEY,
    name                      TEXT NOT NULL,
    category                  TEXT,
    description               TEXT,
    icon                      TEXT,
    current_status            TEXT,
    current_status_at         TIMESTAMPTZ,
    current_status_edited_by  TEXT NOT NULL DEFAULT 'ai'
                                  CHECK (current_status_edited_by IN ('ai', 'human')),
    metadata                  JSONB NOT NULL DEFAULT '{}'::jsonb,
    content                   TEXT,
    cover_image               TEXT,
    created_at                TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at                TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_wiki_things_name     ON wiki_things(name);
CREATE INDEX idx_wiki_things_category ON wiki_things(category) WHERE category IS NOT NULL;
CREATE TRIGGER set_updated_at BEFORE UPDATE ON wiki_things
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE wiki_thing_pins (
    id           TEXT PRIMARY KEY,
    thing_id     TEXT NOT NULL REFERENCES wiki_things(id) ON DELETE CASCADE,
    url          TEXT NOT NULL,
    name         TEXT,
    description  TEXT,
    sort_order   INTEGER NOT NULL DEFAULT 0,
    added_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(thing_id, url)
);
CREATE INDEX idx_wiki_thing_pins_thing ON wiki_thing_pins(thing_id, sort_order);
CREATE INDEX idx_wiki_thing_pins_url   ON wiki_thing_pins(url);

-- ---------------------------------------------------------------------------
-- Days and events (the dayline)
-- ---------------------------------------------------------------------------
CREATE TABLE wiki_days (
    id                      TEXT PRIMARY KEY,
    date                    DATE NOT NULL UNIQUE,
    start_timezone          TEXT,
    end_timezone            TEXT,
    autobiography           TEXT,
    autobiography_sections  JSONB,
    last_edited_by          TEXT NOT NULL DEFAULT 'ai'
                                CHECK (last_edited_by IN ('ai', 'human')),
    act_id                  TEXT REFERENCES wiki_acts(id),
    chapter_id              TEXT REFERENCES wiki_chapters(id),
    morning_baseline        DOUBLE PRECISION,
    battery_curve           JSONB,
    cover_image             TEXT,
    snapshot                JSONB,
    epigraph                TEXT,
    illustration            BYTEA,
    data_quality            JSONB,
    readiness_score         INTEGER,
    readiness_details       JSONB,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_wiki_days_date    ON wiki_days(date DESC);
CREATE INDEX idx_wiki_days_act     ON wiki_days(act_id)     WHERE act_id     IS NOT NULL;
CREATE INDEX idx_wiki_days_chapter ON wiki_days(chapter_id) WHERE chapter_id IS NOT NULL;
CREATE TRIGGER set_updated_at BEFORE UPDATE ON wiki_days
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE wiki_events (
    id                 TEXT PRIMARY KEY,
    day_id             TEXT NOT NULL REFERENCES wiki_days(id) ON DELETE CASCADE,
    start_time         TIMESTAMPTZ NOT NULL,
    end_time           TIMESTAMPTZ NOT NULL,
    auto_label         TEXT,
    auto_location      TEXT,
    user_label         TEXT,
    user_location      TEXT,
    user_notes         TEXT,
    source_ontologies  JSONB NOT NULL DEFAULT '[]'::jsonb,
    is_unknown         BOOLEAN NOT NULL DEFAULT FALSE,
    is_transit         BOOLEAN NOT NULL DEFAULT FALSE,
    is_user_added      BOOLEAN NOT NULL DEFAULT FALSE,
    is_user_edited     BOOLEAN NOT NULL DEFAULT FALSE,
    is_sleep           BOOLEAN NOT NULL DEFAULT FALSE,
    user_hidden        BOOLEAN NOT NULL DEFAULT FALSE,
    user_created       BOOLEAN NOT NULL DEFAULT FALSE,
    embedding          BYTEA,
    novelty_z          DOUBLE PRECISION,
    topics             JSONB NOT NULL DEFAULT '[]'::jsonb,
    entities           JSONB NOT NULL DEFAULT '[]'::jsonb,
    topic_novelty      JSONB,
    entity_novelty     JSONB,
    event_summary      TEXT,
    agent_action       TEXT,
    avg_hr             DOUBLE PRECISION,
    hr_z               DOUBLE PRECISION,
    hrv_z              DOUBLE PRECISION,
    autonomic_z        DOUBLE PRECISION,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_wiki_events_day  ON wiki_events(day_id);
CREATE INDEX idx_wiki_events_time ON wiki_events(start_time, end_time);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON wiki_events
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

-- ---------------------------------------------------------------------------
-- Entity references (any entity ↔ any source record join)
-- ---------------------------------------------------------------------------
CREATE TABLE wiki_entity_refs (
    id            TEXT PRIMARY KEY,
    entity_type   TEXT NOT NULL CHECK (entity_type IN ('person', 'place', 'organization', 'thing')),
    entity_id     TEXT NOT NULL,
    source_table  TEXT NOT NULL,
    source_id     TEXT NOT NULL,
    role          TEXT,
    confidence    DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    resolved_by   TEXT NOT NULL DEFAULT 'system',
    timestamp     TIMESTAMPTZ,
    metadata      JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_entity_refs_entity      ON wiki_entity_refs(entity_id, timestamp DESC);
CREATE INDEX idx_entity_refs_source      ON wiki_entity_refs(source_table, source_id);
CREATE INDEX idx_entity_refs_type        ON wiki_entity_refs(entity_type, timestamp DESC);
CREATE INDEX idx_entity_refs_source_type ON wiki_entity_refs(source_table, source_id, entity_type);
-- NULLS NOT DISTINCT (pg 15+) — treats
-- (entity_id, source_table, source_id, role) as unique even when role IS NULL.
CREATE UNIQUE INDEX idx_entity_refs_unique
    ON wiki_entity_refs(entity_id, source_table, source_id, role) NULLS NOT DISTINCT;

-- ---------------------------------------------------------------------------
-- Narrative identity (singleton-ish — one "active" row at a time)
-- ---------------------------------------------------------------------------
CREATE TABLE wiki_narrative_identity (
    id          TEXT PRIMARY KEY DEFAULT 'nar_identity_001',
    content     TEXT NOT NULL DEFAULT '',
    active      BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON wiki_narrative_identity
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

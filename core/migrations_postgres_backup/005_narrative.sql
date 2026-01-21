-- Narrative: Telos, Acts, Chapters, Day, User Profile

--------------------------------------------------------------------------------
-- TELOS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.telos (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    description TEXT,
    is_active BOOLEAN DEFAULT true,
    embedding vector(768),
    embedded_at TIMESTAMPTZ,
    -- Wiki fields
    slug TEXT UNIQUE,
    content TEXT,
    cover_image TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_telos_single_active
    ON data.telos(is_active) WHERE is_active = true;
CREATE INDEX IF NOT EXISTS idx_telos_embedding
    ON data.telos USING hnsw (embedding vector_cosine_ops);
CREATE UNIQUE INDEX IF NOT EXISTS idx_telos_slug
    ON data.telos(slug) WHERE slug IS NOT NULL;

DROP TRIGGER IF EXISTS data_telos_set_updated_at ON data.telos;
CREATE TRIGGER data_telos_set_updated_at
    BEFORE UPDATE ON data.telos
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- NARRATIVE: ACT
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.narrative_act (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    subtitle TEXT,
    description TEXT,
    start_date DATE NOT NULL,
    end_date DATE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    telos_id UUID REFERENCES data.telos(id),
    themes TEXT[],
    metadata JSONB DEFAULT '{}',
    -- Wiki fields
    slug TEXT UNIQUE,
    content TEXT,
    cover_image TEXT,
    location TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_narrative_act_dates
    ON data.narrative_act(start_date, end_date);
CREATE INDEX IF NOT EXISTS idx_narrative_act_order
    ON data.narrative_act(sort_order);
CREATE INDEX IF NOT EXISTS idx_narrative_act_telos
    ON data.narrative_act(telos_id) WHERE telos_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_narrative_act_slug
    ON data.narrative_act(slug) WHERE slug IS NOT NULL;

DROP TRIGGER IF EXISTS data_narrative_act_set_updated_at ON data.narrative_act;
CREATE TRIGGER data_narrative_act_set_updated_at
    BEFORE UPDATE ON data.narrative_act
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- NARRATIVE: CHAPTER
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.narrative_chapter (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    act_id UUID REFERENCES data.narrative_act(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    subtitle TEXT,
    description TEXT,
    start_date DATE NOT NULL,
    end_date DATE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    themes TEXT[],
    metadata JSONB DEFAULT '{}',
    -- Wiki fields
    slug TEXT UNIQUE,
    content TEXT,
    cover_image TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_narrative_chapter_act
    ON data.narrative_chapter(act_id);
CREATE INDEX IF NOT EXISTS idx_narrative_chapter_dates
    ON data.narrative_chapter(start_date, end_date);
CREATE INDEX IF NOT EXISTS idx_narrative_chapter_order
    ON data.narrative_chapter(act_id, sort_order);
CREATE UNIQUE INDEX IF NOT EXISTS idx_narrative_chapter_slug
    ON data.narrative_chapter(slug) WHERE slug IS NOT NULL;

DROP TRIGGER IF EXISTS data_narrative_chapter_set_updated_at ON data.narrative_chapter;
CREATE TRIGGER data_narrative_chapter_set_updated_at
    BEFORE UPDATE ON data.narrative_chapter
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- TEMPORAL: DAY
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.temporal_day (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    date DATE NOT NULL UNIQUE,
    start_timezone TEXT,  -- IANA timezone at 00:00 (populated by generation process)
    end_timezone TEXT,    -- IANA timezone at 24:00 (null if same as start or day not ended)
    autobiography TEXT,
    autobiography_sections JSONB,
    last_edited_by TEXT DEFAULT 'ai' CHECK (last_edited_by IN ('ai', 'human')),
    context_vector JSONB,
    act_id UUID REFERENCES data.narrative_act(id),
    chapter_id UUID REFERENCES data.narrative_chapter(id),
    -- Wiki fields (date serves as slug)
    cover_image TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_temporal_day_date
    ON data.temporal_day(date DESC);
CREATE INDEX IF NOT EXISTS idx_temporal_day_act
    ON data.temporal_day(act_id) WHERE act_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_temporal_day_chapter
    ON data.temporal_day(chapter_id) WHERE chapter_id IS NOT NULL;

DROP TRIGGER IF EXISTS data_temporal_day_set_updated_at ON data.temporal_day;
CREATE TRIGGER data_temporal_day_set_updated_at
    BEFORE UPDATE ON data.temporal_day
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- USER PROFILE (singleton)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.user_profile (
    id UUID PRIMARY KEY DEFAULT '00000000-0000-0000-0000-000000000001'::uuid,
    full_name TEXT,
    preferred_name TEXT,
    birth_date DATE,
    height_cm NUMERIC(5,2),
    weight_kg NUMERIC(5,2),
    ethnicity TEXT,
    occupation TEXT,
    employer TEXT,

    -- Onboarding: single status field
    onboarding_status TEXT NOT NULL DEFAULT 'welcome' CHECK (onboarding_status IN ('welcome', 'profile', 'places', 'tools', 'complete')),

    -- Home
    home_place_id UUID REFERENCES data.entities_place(id),

    -- Preferences
    theme TEXT DEFAULT 'light',
    update_check_hour INTEGER DEFAULT 8 CHECK (update_check_hour >= 0 AND update_check_hour <= 23),

    -- Discovery context
    crux TEXT,
    technology_vision TEXT,
    pain_point_primary TEXT,
    pain_point_secondary TEXT,
    excited_features TEXT[],

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT user_profile_singleton CHECK (id = '00000000-0000-0000-0000-000000000001'::uuid)
);

DROP TRIGGER IF EXISTS data_user_profile_set_updated_at ON data.user_profile;
CREATE TRIGGER data_user_profile_set_updated_at
    BEFORE UPDATE ON data.user_profile
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- Insert singleton row
INSERT INTO data.user_profile (id)
VALUES ('00000000-0000-0000-0000-000000000001'::uuid)
ON CONFLICT (id) DO NOTHING;

--------------------------------------------------------------------------------
-- CITATION (links markdown [1] references to ontology data)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.citation (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Source: which wiki page contains this citation
    source_type TEXT NOT NULL CHECK (source_type IN (
        'person', 'place', 'organization', 'thing',
        'telos', 'act', 'chapter', 'day'
    )),
    source_id UUID NOT NULL,
    -- Target: which ontology row this citation references
    target_table TEXT NOT NULL,
    target_id UUID NOT NULL,
    -- Citation display
    citation_index INTEGER NOT NULL, -- [1], [2], etc.
    label TEXT, -- Optional human-readable label
    preview TEXT, -- Preview text for tooltip
    is_hidden BOOLEAN DEFAULT false, -- Hidden citations (data exists but not shown)
    added_by TEXT DEFAULT 'ai' CHECK (added_by IN ('ai', 'human')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Each page has unique citation indices
    UNIQUE(source_type, source_id, citation_index)
);

CREATE INDEX IF NOT EXISTS idx_citation_source
    ON data.citation(source_type, source_id);
CREATE INDEX IF NOT EXISTS idx_citation_target
    ON data.citation(target_table, target_id);

DROP TRIGGER IF EXISTS data_citation_set_updated_at ON data.citation;
CREATE TRIGGER data_citation_set_updated_at
    BEFORE UPDATE ON data.citation
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- TEMPORAL EVENT (day timeline events)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.temporal_event (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    day_id UUID NOT NULL REFERENCES data.temporal_day(id) ON DELETE CASCADE,
    -- Time range
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
    -- Auto-generated labels (from ontology data)
    auto_label TEXT, -- "Work", "Transit", "Sleep", "Unknown"
    auto_location TEXT, -- From location_visit place name
    -- User overrides (preserved on regeneration)
    user_label TEXT, -- "Architecture review with team"
    user_location TEXT, -- Override auto-detected place
    user_notes TEXT, -- Brief annotation
    -- Source tracking (which ontology rows generated this)
    source_ontologies JSONB DEFAULT '[]', -- [{table: "location_visit", id: "..."}, ...]
    -- Flags
    is_unknown BOOLEAN DEFAULT false, -- Gap in timeline, couldn't determine activity
    is_transit BOOLEAN DEFAULT false, -- Travel between places
    is_user_added BOOLEAN DEFAULT false, -- Manually created by user
    is_user_edited BOOLEAN DEFAULT false, -- Auto-event but user modified
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_temporal_event_day
    ON data.temporal_event(day_id);
CREATE INDEX IF NOT EXISTS idx_temporal_event_time
    ON data.temporal_event(start_time, end_time);

DROP TRIGGER IF EXISTS data_temporal_event_set_updated_at ON data.temporal_event;
CREATE TRIGGER data_temporal_event_set_updated_at
    BEFORE UPDATE ON data.temporal_event
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

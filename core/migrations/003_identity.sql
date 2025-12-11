-- Identity: Axiology, praxis, and user profile
-- Consolidates: 004, 011, 013, 014, 015 (aspiration parts), 016, 017, 018, 020, 023

--------------------------------------------------------------------------------
-- AXIOLOGY: TELOS (purpose/ultimate aim)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.axiology_telos (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    description TEXT,
    is_active BOOLEAN DEFAULT true,
    topic_id UUID REFERENCES data.entities_topic(id),
    -- Embedding for semantic search (from 014)
    embedding vector(768),
    embedded_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Add embedding columns if they don't exist
ALTER TABLE data.axiology_telos ADD COLUMN IF NOT EXISTS embedding vector(768);
ALTER TABLE data.axiology_telos ADD COLUMN IF NOT EXISTS embedded_at TIMESTAMPTZ;

CREATE UNIQUE INDEX IF NOT EXISTS idx_axiology_telos_single_active
    ON data.axiology_telos(is_active)
    WHERE is_active = true;

CREATE INDEX IF NOT EXISTS idx_axiology_telos_embedding
    ON data.axiology_telos USING hnsw (embedding vector_cosine_ops);

--------------------------------------------------------------------------------
-- AXIOLOGY: VIRTUE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.axiology_virtue (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    description TEXT,
    topic_id UUID REFERENCES data.entities_topic(id),
    is_active BOOLEAN DEFAULT true,
    embedding vector(768),
    embedded_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE data.axiology_virtue ADD COLUMN IF NOT EXISTS embedding vector(768);
ALTER TABLE data.axiology_virtue ADD COLUMN IF NOT EXISTS embedded_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_axiology_virtue_embedding
    ON data.axiology_virtue USING hnsw (embedding vector_cosine_ops);

--------------------------------------------------------------------------------
-- AXIOLOGY: VICE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.axiology_vice (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    description TEXT,
    topic_id UUID REFERENCES data.entities_topic(id),
    is_active BOOLEAN DEFAULT true,
    embedding vector(768),
    embedded_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE data.axiology_vice ADD COLUMN IF NOT EXISTS embedding vector(768);
ALTER TABLE data.axiology_vice ADD COLUMN IF NOT EXISTS embedded_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_axiology_vice_embedding
    ON data.axiology_vice USING hnsw (embedding vector_cosine_ops);

--------------------------------------------------------------------------------
-- AXIOLOGY: TEMPERAMENT
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.axiology_temperament (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    description TEXT,
    temperament_type TEXT,
    topic_id UUID REFERENCES data.entities_topic(id),
    is_active BOOLEAN DEFAULT true,
    embedding vector(768),
    embedded_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE data.axiology_temperament ADD COLUMN IF NOT EXISTS embedding vector(768);
ALTER TABLE data.axiology_temperament ADD COLUMN IF NOT EXISTS embedded_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_axiology_temperament_embedding
    ON data.axiology_temperament USING hnsw (embedding vector_cosine_ops);

--------------------------------------------------------------------------------
-- AXIOLOGY: PREFERENCE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.axiology_preference (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    description TEXT,
    preference_domain TEXT,
    -- Note: valence column removed per 014 cleanup (valence implicit in table choice)
    person_id UUID REFERENCES data.entities_person(id),
    place_id UUID REFERENCES data.entities_place(id),
    topic_id UUID REFERENCES data.entities_topic(id),
    is_active BOOLEAN DEFAULT true,
    embedding vector(768),
    embedded_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE data.axiology_preference ADD COLUMN IF NOT EXISTS embedding vector(768);
ALTER TABLE data.axiology_preference ADD COLUMN IF NOT EXISTS embedded_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_axiology_preference_embedding
    ON data.axiology_preference USING hnsw (embedding vector_cosine_ops);

--------------------------------------------------------------------------------
-- PRAXIS: TASK (including habits as recurring tasks)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.praxis_task (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    description TEXT,
    tags TEXT[] DEFAULT '{}',
    topic_id UUID REFERENCES data.entities_topic(id),
    status TEXT DEFAULT 'active',
    progress_percent INTEGER,
    start_date TIMESTAMPTZ,
    target_date TIMESTAMPTZ,
    completed_date TIMESTAMPTZ,

    -- Habit fields (for recurring tasks that are habits)
    recurrence_rule TEXT,              -- 'daily', 'weekly', 'monthly', or RRULE
    is_habit BOOLEAN DEFAULT FALSE,    -- marks this as a habit
    current_streak INTEGER DEFAULT 0,  -- habit streak tracking
    best_streak INTEGER DEFAULT 0,     -- best ever streak
    last_completed_date DATE,          -- for streak calculation

    -- Hierarchical relations
    initiative_id UUID,                -- References praxis_initiative (FK added below)
    parent_task_id UUID,               -- References praxis_task (self-referential)

    -- Source integration
    source_provider TEXT DEFAULT 'internal',  -- 'internal', 'todoist', 'notion', etc.
    external_id TEXT,                  -- ID in external system
    external_url TEXT,                 -- Deep link to external system

    -- Axiological links
    virtue_ids UUID[],                 -- Links to axiology_virtue
    vice_ids UUID[],                   -- Links to axiology_vice
    value_ids UUID[],                  -- Links to axiology_value (legacy)
    purpose TEXT,                      -- Why am I doing this?

    -- Context
    context_energy TEXT,               -- 'deep_focus', 'shallow', 'creative'
    context_location TEXT,             -- '@home', '@office', '@anywhere'
    estimated_minutes INTEGER,
    actual_minutes INTEGER,

    is_active BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_praxis_task_tags ON data.praxis_task USING GIN (tags);
CREATE INDEX IF NOT EXISTS idx_praxis_task_status ON data.praxis_task(status) WHERE is_active = true;
CREATE INDEX IF NOT EXISTS idx_praxis_task_habits ON data.praxis_task(is_habit) WHERE is_habit = true;
CREATE INDEX IF NOT EXISTS idx_praxis_task_recurrence ON data.praxis_task(recurrence_rule) WHERE recurrence_rule IS NOT NULL;

--------------------------------------------------------------------------------
-- PRAXIS: INITIATIVE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.praxis_initiative (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    description TEXT,
    tags TEXT[] DEFAULT '{}',
    topic_id UUID REFERENCES data.entities_topic(id),
    status TEXT DEFAULT 'active',
    progress_percent INTEGER,
    start_date TIMESTAMPTZ,
    target_date TIMESTAMPTZ,
    completed_date TIMESTAMPTZ,

    -- Hierarchical relations
    parent_initiative_id UUID,         -- References praxis_initiative (self-referential)

    -- Source integration
    source_provider TEXT DEFAULT 'internal',  -- 'internal', 'asana', 'notion', etc.
    external_id TEXT,
    external_url TEXT,

    -- Axiological links (initiatives often have explicit purpose)
    virtue_ids UUID[],
    vice_ids UUID[],
    value_ids UUID[],
    purpose TEXT,

    -- For commitment-level initiatives (overcoming addiction, etc.)
    is_commitment BOOLEAN DEFAULT FALSE,
    success_metrics JSONB,             -- {"days_clean": 60}
    current_metrics JSONB,             -- {"days_clean": 12}

    is_active BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_praxis_initiative_tags ON data.praxis_initiative USING GIN (tags);
CREATE INDEX IF NOT EXISTS idx_praxis_initiative_status ON data.praxis_initiative(status) WHERE is_active = true;

--------------------------------------------------------------------------------
-- PRAXIS: ASPIRATION
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.praxis_aspiration (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    description TEXT,
    tags TEXT[] DEFAULT '{}',
    topic_id UUID REFERENCES data.entities_topic(id),
    status TEXT DEFAULT 'dreaming',
    target_timeframe TEXT,              -- 'this_year', 'next_5_years', 'lifetime', 'undefined'
    achieved_date TIMESTAMPTZ,

    -- When activated
    activated_date DATE,                -- When moved from someday to active
    activated_as_initiative_id UUID,    -- References praxis_initiative (FK added below)

    -- Axiological links (aspirations are usually values-aligned)
    virtue_ids UUID[],
    value_ids UUID[],
    purpose TEXT,

    -- From 015: Additional aspiration fields
    target_date TIMESTAMPTZ,            -- Specific target date (vs relative target_timeframe)
    source_provider TEXT DEFAULT 'internal',  -- 'internal', 'todoist', 'notion', etc.
    external_id TEXT,
    external_url TEXT,

    is_active BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Add columns from 015 if they don't exist
ALTER TABLE data.praxis_aspiration ADD COLUMN IF NOT EXISTS target_date TIMESTAMPTZ;
ALTER TABLE data.praxis_aspiration ADD COLUMN IF NOT EXISTS source_provider TEXT DEFAULT 'internal';
ALTER TABLE data.praxis_aspiration ADD COLUMN IF NOT EXISTS external_id TEXT;
ALTER TABLE data.praxis_aspiration ADD COLUMN IF NOT EXISTS external_url TEXT;

CREATE INDEX IF NOT EXISTS idx_praxis_aspiration_tags ON data.praxis_aspiration USING GIN (tags);
CREATE INDEX IF NOT EXISTS idx_praxis_aspiration_status ON data.praxis_aspiration(status) WHERE is_active = true;

-- Comments from 015
COMMENT ON COLUMN data.praxis_aspiration.target_date IS 'Specific target date for aspiration (vs relative target_timeframe)';
COMMENT ON COLUMN data.praxis_aspiration.source_provider IS 'Source of aspiration: internal (created in app), or external provider name';

--------------------------------------------------------------------------------
-- FOREIGN KEY CONSTRAINTS (after all tables created)
--------------------------------------------------------------------------------

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_praxis_task_initiative'
        AND conrelid = 'data.praxis_task'::regclass
    ) THEN
        ALTER TABLE data.praxis_task ADD CONSTRAINT fk_praxis_task_initiative
            FOREIGN KEY (initiative_id) REFERENCES data.praxis_initiative(id) ON DELETE SET NULL;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_praxis_task_parent'
        AND conrelid = 'data.praxis_task'::regclass
    ) THEN
        ALTER TABLE data.praxis_task ADD CONSTRAINT fk_praxis_task_parent
            FOREIGN KEY (parent_task_id) REFERENCES data.praxis_task(id) ON DELETE CASCADE;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_praxis_initiative_parent'
        AND conrelid = 'data.praxis_initiative'::regclass
    ) THEN
        ALTER TABLE data.praxis_initiative ADD CONSTRAINT fk_praxis_initiative_parent
            FOREIGN KEY (parent_initiative_id) REFERENCES data.praxis_initiative(id) ON DELETE CASCADE;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_praxis_aspiration_initiative'
        AND conrelid = 'data.praxis_aspiration'::regclass
    ) THEN
        ALTER TABLE data.praxis_aspiration ADD CONSTRAINT fk_praxis_aspiration_initiative
            FOREIGN KEY (activated_as_initiative_id) REFERENCES data.praxis_initiative(id) ON DELETE SET NULL;
    END IF;
END $$;

-- FK constraints for praxis_calendar (table defined in 002_ontology.sql)
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_praxis_calendar_task'
        AND conrelid = 'data.praxis_calendar'::regclass
    ) THEN
        ALTER TABLE data.praxis_calendar ADD CONSTRAINT fk_praxis_calendar_task
            FOREIGN KEY (task_id) REFERENCES data.praxis_task(id) ON DELETE SET NULL;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_praxis_calendar_initiative'
        AND conrelid = 'data.praxis_calendar'::regclass
    ) THEN
        ALTER TABLE data.praxis_calendar ADD CONSTRAINT fk_praxis_calendar_initiative
            FOREIGN KEY (initiative_id) REFERENCES data.praxis_initiative(id) ON DELETE SET NULL;
    END IF;
END $$;

--------------------------------------------------------------------------------
-- PRUDENT CONTEXT SNAPSHOT
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.prudent_context_snapshot (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    computed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    context_data JSONB NOT NULL,
    llm_model TEXT,
    token_count INTEGER,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_prudent_context_expires ON data.prudent_context_snapshot(expires_at DESC, computed_at DESC);

CREATE OR REPLACE FUNCTION cleanup_expired_context()
RETURNS void AS $$
BEGIN
    DELETE FROM data.prudent_context_snapshot
    WHERE expires_at < NOW() - INTERVAL '7 days';
END;
$$ LANGUAGE plpgsql;

--------------------------------------------------------------------------------
-- USER PROFILE (singleton)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.user_profile (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    full_name TEXT,
    preferred_name TEXT,
    birth_date DATE,
    height_cm NUMERIC(5,2),
    weight_kg NUMERIC(5,2),
    ethnicity TEXT,
    occupation TEXT,
    employer TEXT,
    is_onboarding BOOLEAN NOT NULL DEFAULT true,
    -- From 011: Theme preference
    theme TEXT DEFAULT 'light',
    -- From 013: Onboarding step
    onboarding_step INTEGER DEFAULT 0,
    -- From 016: Axiology completion
    axiology_complete BOOLEAN DEFAULT FALSE,
    -- From 017: Home place reference
    home_place_id UUID REFERENCES data.entities_place(id),
    -- From 018: Granular onboarding flags
    onboarding_profile_complete BOOLEAN DEFAULT false,
    onboarding_places_complete BOOLEAN DEFAULT false,
    onboarding_tools_complete BOOLEAN DEFAULT false,
    -- From 020: Crux (shared ethos statement)
    crux TEXT,
    -- From 023: Update schedule
    update_check_hour INTEGER DEFAULT 8,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT user_profile_singleton CHECK (id = '00000000-0000-0000-0000-000000000001'::uuid)
);

-- Add columns if they don't exist (for idempotency)
ALTER TABLE data.user_profile ADD COLUMN IF NOT EXISTS theme TEXT DEFAULT 'light';
ALTER TABLE data.user_profile ADD COLUMN IF NOT EXISTS onboarding_step INTEGER DEFAULT 0;
ALTER TABLE data.user_profile ADD COLUMN IF NOT EXISTS axiology_complete BOOLEAN DEFAULT FALSE;
ALTER TABLE data.user_profile ADD COLUMN IF NOT EXISTS home_place_id UUID REFERENCES data.entities_place(id);
ALTER TABLE data.user_profile ADD COLUMN IF NOT EXISTS onboarding_profile_complete BOOLEAN DEFAULT false;
ALTER TABLE data.user_profile ADD COLUMN IF NOT EXISTS onboarding_places_complete BOOLEAN DEFAULT false;
ALTER TABLE data.user_profile ADD COLUMN IF NOT EXISTS onboarding_tools_complete BOOLEAN DEFAULT false;
ALTER TABLE data.user_profile ADD COLUMN IF NOT EXISTS crux TEXT;
ALTER TABLE data.user_profile ADD COLUMN IF NOT EXISTS update_check_hour INTEGER DEFAULT 8;
-- From technology onboarding step
ALTER TABLE data.user_profile ADD COLUMN IF NOT EXISTS technology_vision TEXT;
ALTER TABLE data.user_profile ADD COLUMN IF NOT EXISTS pain_point_primary TEXT;
ALTER TABLE data.user_profile ADD COLUMN IF NOT EXISTS pain_point_secondary TEXT;
ALTER TABLE data.user_profile ADD COLUMN IF NOT EXISTS excited_features TEXT[];

-- Constraints (idempotent)
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'valid_theme') THEN
        ALTER TABLE data.user_profile
        ADD CONSTRAINT valid_theme CHECK (theme IN ('warm', 'light', 'dark', 'night'));
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'user_profile_update_check_hour_range') THEN
        ALTER TABLE data.user_profile ADD CONSTRAINT user_profile_update_check_hour_range
            CHECK (update_check_hour >= 0 AND update_check_hour <= 23);
    END IF;
END $$;

-- Comments
COMMENT ON COLUMN data.user_profile.onboarding_step IS 'Current step in onboarding wizard (0-5). NULL or 0 means not started, cleared on completion.';
COMMENT ON COLUMN data.user_profile.axiology_complete IS 'Whether the user has completed axiology discovery through chat conversation with the onboarding agent';
COMMENT ON COLUMN data.user_profile.onboarding_profile_complete IS 'Whether the user has completed profile setup (name required)';
COMMENT ON COLUMN data.user_profile.onboarding_places_complete IS 'Whether the user has added places or skipped this step';
COMMENT ON COLUMN data.user_profile.onboarding_tools_complete IS 'Whether the user has connected tools or skipped this step';
COMMENT ON COLUMN data.user_profile.crux IS 'Shared ethos statement from onboarding - user''s vision and goals for Personal AI';
COMMENT ON COLUMN data.user_profile.update_check_hour IS 'Hour (0-23 UTC) when the system checks for updates. Default 8 (3 AM Central). User can configure via settings.';
COMMENT ON COLUMN data.user_profile.technology_vision IS 'User''s vision for how AI/technology should augment human life';
COMMENT ON COLUMN data.user_profile.pain_point_primary IS 'Primary pain point that brought user to Virtues (e.g., chaos, direction, self_knowledge)';
COMMENT ON COLUMN data.user_profile.pain_point_secondary IS 'Optional secondary pain point';
COMMENT ON COLUMN data.user_profile.excited_features IS 'Array of feature keys user is most excited about (e.g., autobiography, praxis, axiology)';

-- Indexes
CREATE INDEX IF NOT EXISTS idx_user_profile_preferred_name ON data.user_profile(preferred_name);
CREATE INDEX IF NOT EXISTS idx_user_profile_full_name ON data.user_profile(full_name);
CREATE INDEX IF NOT EXISTS idx_user_profile_home_place ON data.user_profile(home_place_id) WHERE home_place_id IS NOT NULL;

-- Insert singleton row
INSERT INTO data.user_profile (id)
VALUES ('00000000-0000-0000-0000-000000000001'::uuid)
ON CONFLICT (id) DO NOTHING;

-- Trigger
DROP TRIGGER IF EXISTS user_profile_updated_at ON data.user_profile;
CREATE TRIGGER user_profile_updated_at
    BEFORE UPDATE ON data.user_profile
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

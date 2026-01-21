-- Praxis: Task, Initiative, Aspiration, Calendar

--------------------------------------------------------------------------------
-- PRAXIS: TASK
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.praxis_task (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    description TEXT,
    tags TEXT[] DEFAULT '{}',
    thing_id UUID REFERENCES data.entities_thing(id),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'completed', 'archived')),
    progress_percent INTEGER,
    start_date TIMESTAMPTZ,
    target_date TIMESTAMPTZ,
    completed_date TIMESTAMPTZ,

    -- Habit tracking
    recurrence_rule TEXT,
    is_habit BOOLEAN DEFAULT FALSE,
    current_streak INTEGER DEFAULT 0,
    best_streak INTEGER DEFAULT 0,
    last_completed_date DATE,

    -- Hierarchy
    initiative_id UUID,
    parent_task_id UUID,

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

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_praxis_task_status
    ON data.praxis_task(status) WHERE status = 'active';
CREATE INDEX IF NOT EXISTS idx_praxis_task_tags
    ON data.praxis_task USING GIN(tags);
CREATE INDEX IF NOT EXISTS idx_praxis_task_habit
    ON data.praxis_task(id) WHERE is_habit = true;
CREATE INDEX IF NOT EXISTS idx_praxis_task_thing
    ON data.praxis_task(thing_id) WHERE thing_id IS NOT NULL;

DROP TRIGGER IF EXISTS data_praxis_task_set_updated_at ON data.praxis_task;
CREATE TRIGGER data_praxis_task_set_updated_at
    BEFORE UPDATE ON data.praxis_task
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- PRAXIS: INITIATIVE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.praxis_initiative (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    description TEXT,
    tags TEXT[] DEFAULT '{}',
    thing_id UUID REFERENCES data.entities_thing(id),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'paused', 'completed', 'archived')),
    progress_percent INTEGER,
    start_date TIMESTAMPTZ,
    target_date TIMESTAMPTZ,
    completed_date TIMESTAMPTZ,

    -- Hierarchy
    parent_initiative_id UUID,

    -- External source
    source_provider TEXT DEFAULT 'internal',
    external_id TEXT,
    external_url TEXT,

    -- Context
    purpose TEXT,
    is_commitment BOOLEAN DEFAULT FALSE,
    success_metrics JSONB,
    current_metrics JSONB,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_praxis_initiative_status
    ON data.praxis_initiative(status) WHERE status = 'active';
CREATE INDEX IF NOT EXISTS idx_praxis_initiative_tags
    ON data.praxis_initiative USING GIN(tags);
CREATE INDEX IF NOT EXISTS idx_praxis_initiative_thing
    ON data.praxis_initiative(thing_id) WHERE thing_id IS NOT NULL;

DROP TRIGGER IF EXISTS data_praxis_initiative_set_updated_at ON data.praxis_initiative;
CREATE TRIGGER data_praxis_initiative_set_updated_at
    BEFORE UPDATE ON data.praxis_initiative
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- PRAXIS: ASPIRATION
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.praxis_aspiration (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    description TEXT,
    tags TEXT[] DEFAULT '{}',
    thing_id UUID REFERENCES data.entities_thing(id),
    status TEXT NOT NULL DEFAULT 'dreaming' CHECK (status IN ('dreaming', 'activated', 'achieved', 'archived')),
    target_timeframe TEXT,
    achieved_date TIMESTAMPTZ,

    -- Activation
    activated_date DATE,
    activated_as_initiative_id UUID,

    -- External source
    source_provider TEXT DEFAULT 'internal',
    external_id TEXT,
    external_url TEXT,

    -- Context
    purpose TEXT,
    target_date TIMESTAMPTZ,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_praxis_aspiration_status
    ON data.praxis_aspiration(status) WHERE status IN ('dreaming', 'activated');
CREATE INDEX IF NOT EXISTS idx_praxis_aspiration_tags
    ON data.praxis_aspiration USING GIN(tags);
CREATE INDEX IF NOT EXISTS idx_praxis_aspiration_thing
    ON data.praxis_aspiration(thing_id) WHERE thing_id IS NOT NULL;

DROP TRIGGER IF EXISTS data_praxis_aspiration_set_updated_at ON data.praxis_aspiration;
CREATE TRIGGER data_praxis_aspiration_set_updated_at
    BEFORE UPDATE ON data.praxis_aspiration
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- PRAXIS: CALENDAR
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.praxis_calendar (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    description TEXT,
    event_type TEXT,

    -- People
    organizer_identifier TEXT,
    attendee_identifiers TEXT[] DEFAULT '{}',
    organizer_person_id UUID REFERENCES data.entities_person(id),
    attendee_person_ids UUID[] DEFAULT '{}',

    -- Context
    thing_id UUID REFERENCES data.entities_thing(id),
    place_id UUID REFERENCES data.entities_place(id),
    location_name TEXT,

    -- Conference
    conference_url TEXT,
    conference_platform TEXT,

    -- Time
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
    is_all_day BOOLEAN DEFAULT false,
    timezone TEXT,
    recurrence_rule TEXT,

    -- Links
    task_id UUID,
    initiative_id UUID,

    -- Time blocking
    block_type TEXT,
    is_sacred BOOLEAN DEFAULT FALSE,

    -- External source
    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    external_id TEXT,
    external_url TEXT,

    metadata JSONB DEFAULT '{}',
    embedding vector(768),
    embedded_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT praxis_calendar_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_praxis_calendar_start
    ON data.praxis_calendar(start_time DESC);
CREATE INDEX IF NOT EXISTS idx_praxis_calendar_thing
    ON data.praxis_calendar(thing_id) WHERE thing_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_praxis_calendar_place
    ON data.praxis_calendar(place_id) WHERE place_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_praxis_calendar_block_type
    ON data.praxis_calendar(block_type) WHERE block_type IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_praxis_calendar_embedding
    ON data.praxis_calendar USING hnsw (embedding vector_cosine_ops) WITH (m = 16, ef_construction = 64);

DROP TRIGGER IF EXISTS data_praxis_calendar_set_updated_at ON data.praxis_calendar;
CREATE TRIGGER data_praxis_calendar_set_updated_at
    BEFORE UPDATE ON data.praxis_calendar
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- PRAXIS: PRUDENT CONTEXT SNAPSHOT
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.prudent_context_snapshot (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    computed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    context_data JSONB NOT NULL,
    llm_model TEXT,
    token_count INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_prudent_context_expires
    ON data.prudent_context_snapshot(expires_at DESC, computed_at DESC);

--------------------------------------------------------------------------------
-- FOREIGN KEY CONSTRAINTS
--------------------------------------------------------------------------------

ALTER TABLE data.praxis_task
    ADD CONSTRAINT fk_praxis_task_initiative
    FOREIGN KEY (initiative_id) REFERENCES data.praxis_initiative(id) ON DELETE SET NULL;

ALTER TABLE data.praxis_task
    ADD CONSTRAINT fk_praxis_task_parent
    FOREIGN KEY (parent_task_id) REFERENCES data.praxis_task(id) ON DELETE CASCADE;

ALTER TABLE data.praxis_initiative
    ADD CONSTRAINT fk_praxis_initiative_parent
    FOREIGN KEY (parent_initiative_id) REFERENCES data.praxis_initiative(id) ON DELETE CASCADE;

ALTER TABLE data.praxis_aspiration
    ADD CONSTRAINT fk_praxis_aspiration_initiative
    FOREIGN KEY (activated_as_initiative_id) REFERENCES data.praxis_initiative(id) ON DELETE SET NULL;

ALTER TABLE data.praxis_calendar
    ADD CONSTRAINT fk_praxis_calendar_task
    FOREIGN KEY (task_id) REFERENCES data.praxis_task(id) ON DELETE SET NULL;

ALTER TABLE data.praxis_calendar
    ADD CONSTRAINT fk_praxis_calendar_initiative
    FOREIGN KEY (initiative_id) REFERENCES data.praxis_initiative(id) ON DELETE SET NULL;

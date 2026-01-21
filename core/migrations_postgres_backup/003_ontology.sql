-- Ontology: Health, Location, Social, Activity, Knowledge, Speech, Financial

--------------------------------------------------------------------------------
-- HEALTH: HEART RATE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.health_heart_rate (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bpm INTEGER NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT health_heart_rate_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_health_heart_rate_timestamp
    ON data.health_heart_rate(timestamp DESC);

DROP TRIGGER IF EXISTS data_health_heart_rate_set_updated_at ON data.health_heart_rate;
CREATE TRIGGER data_health_heart_rate_set_updated_at
    BEFORE UPDATE ON data.health_heart_rate
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- HEALTH: HRV
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.health_hrv (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    hrv_ms FLOAT NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT health_hrv_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_health_hrv_timestamp
    ON data.health_hrv(timestamp DESC);

DROP TRIGGER IF EXISTS data_health_hrv_set_updated_at ON data.health_hrv;
CREATE TRIGGER data_health_hrv_set_updated_at
    BEFORE UPDATE ON data.health_hrv
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- HEALTH: STEPS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.health_steps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    step_count INTEGER NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT health_steps_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_health_steps_timestamp
    ON data.health_steps(timestamp DESC);

DROP TRIGGER IF EXISTS data_health_steps_set_updated_at ON data.health_steps;
CREATE TRIGGER data_health_steps_set_updated_at
    BEFORE UPDATE ON data.health_steps
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- HEALTH: SLEEP
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.health_sleep (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sleep_stages JSONB,
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
    duration_minutes INTEGER,
    sleep_quality_score FLOAT,
    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT health_sleep_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_health_sleep_start
    ON data.health_sleep(start_time DESC);

DROP TRIGGER IF EXISTS data_health_sleep_set_updated_at ON data.health_sleep;
CREATE TRIGGER data_health_sleep_set_updated_at
    BEFORE UPDATE ON data.health_sleep
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- HEALTH: WORKOUT
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.health_workout (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workout_type TEXT NOT NULL,
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
    duration_minutes INTEGER,
    calories_burned INTEGER,
    distance_km FLOAT,
    avg_heart_rate INTEGER,
    max_heart_rate INTEGER,
    place_id UUID REFERENCES data.entities_place(id),
    route_geometry geography(LINESTRING, 4326),
    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT health_workout_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_health_workout_start
    ON data.health_workout(start_time DESC);
CREATE INDEX IF NOT EXISTS idx_health_workout_type
    ON data.health_workout(workout_type);
CREATE INDEX IF NOT EXISTS idx_health_workout_route
    ON data.health_workout USING GIST(route_geometry) WHERE route_geometry IS NOT NULL;

DROP TRIGGER IF EXISTS data_health_workout_set_updated_at ON data.health_workout;
CREATE TRIGGER data_health_workout_set_updated_at
    BEFORE UPDATE ON data.health_workout
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- LOCATION: POINT
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.location_point (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    latitude DOUBLE PRECISION NOT NULL,
    longitude DOUBLE PRECISION NOT NULL,
    altitude DOUBLE PRECISION,
    horizontal_accuracy DOUBLE PRECISION,
    vertical_accuracy DOUBLE PRECISION,
    timestamp TIMESTAMPTZ NOT NULL,
    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT location_point_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_location_point_timestamp
    ON data.location_point(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_location_point_coords
    ON data.location_point(latitude, longitude);

DROP TRIGGER IF EXISTS data_location_point_set_updated_at ON data.location_point;
CREATE TRIGGER data_location_point_set_updated_at
    BEFORE UPDATE ON data.location_point
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- LOCATION: VISIT
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.location_visit (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    place_id UUID REFERENCES data.entities_place(id),
    place_name TEXT,
    latitude DOUBLE PRECISION NOT NULL,
    longitude DOUBLE PRECISION NOT NULL,
    arrival_time TIMESTAMPTZ NOT NULL,
    departure_time TIMESTAMPTZ,
    duration_minutes INTEGER,
    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT location_visit_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_location_visit_arrival
    ON data.location_visit(arrival_time DESC);
CREATE INDEX IF NOT EXISTS idx_location_visit_place
    ON data.location_visit(place_id) WHERE place_id IS NOT NULL;

DROP TRIGGER IF EXISTS data_location_visit_set_updated_at ON data.location_visit;
CREATE TRIGGER data_location_visit_set_updated_at
    BEFORE UPDATE ON data.location_visit
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- SOCIAL: EMAIL
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.social_email (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    message_id TEXT NOT NULL,
    thread_id TEXT,
    subject TEXT,
    body TEXT,
    body_preview TEXT,
    from_email TEXT NOT NULL,
    from_name TEXT,
    from_person_id UUID REFERENCES data.entities_person(id),
    to_emails TEXT[] DEFAULT '{}',
    to_names TEXT[] DEFAULT '{}',
    to_person_ids UUID[] DEFAULT '{}',
    cc_emails TEXT[] DEFAULT '{}',
    cc_person_ids UUID[] DEFAULT '{}',
    bcc_emails TEXT[] DEFAULT '{}',
    bcc_person_ids UUID[] DEFAULT '{}',
    direction TEXT NOT NULL CHECK (direction IN ('sent', 'received')),
    is_read BOOLEAN DEFAULT false,
    is_starred BOOLEAN DEFAULT false,
    has_attachments BOOLEAN DEFAULT false,
    labels TEXT[] DEFAULT '{}',
    timestamp TIMESTAMPTZ NOT NULL,
    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata JSONB DEFAULT '{}',
    embedding vector(768),
    embedded_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT social_email_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_social_email_timestamp
    ON data.social_email(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_social_email_thread
    ON data.social_email(thread_id) WHERE thread_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_social_email_from_person
    ON data.social_email(from_person_id) WHERE from_person_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_social_email_embedding
    ON data.social_email USING hnsw (embedding vector_cosine_ops) WITH (m = 16, ef_construction = 64);

DROP TRIGGER IF EXISTS data_social_email_set_updated_at ON data.social_email;
CREATE TRIGGER data_social_email_set_updated_at
    BEFORE UPDATE ON data.social_email
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- SOCIAL: MESSAGE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.social_message (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    message_id TEXT NOT NULL,
    conversation_id TEXT,
    platform TEXT NOT NULL,
    content TEXT,
    from_identifier TEXT NOT NULL,
    from_name TEXT,
    from_person_id UUID REFERENCES data.entities_person(id),
    to_identifiers TEXT[] DEFAULT '{}',
    to_person_ids UUID[] DEFAULT '{}',
    is_read BOOLEAN DEFAULT false,
    is_group_message BOOLEAN DEFAULT false,
    reply_to_message_id TEXT,
    has_attachments BOOLEAN DEFAULT false,
    timestamp TIMESTAMPTZ NOT NULL,
    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata JSONB DEFAULT '{}',
    embedding vector(768),
    embedded_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT social_message_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_social_message_timestamp
    ON data.social_message(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_social_message_conversation
    ON data.social_message(conversation_id) WHERE conversation_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_social_message_platform
    ON data.social_message(platform);
CREATE INDEX IF NOT EXISTS idx_social_message_embedding
    ON data.social_message USING hnsw (embedding vector_cosine_ops) WITH (m = 16, ef_construction = 64);

DROP TRIGGER IF EXISTS data_social_message_set_updated_at ON data.social_message;
CREATE TRIGGER data_social_message_set_updated_at
    BEFORE UPDATE ON data.social_message
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- ACTIVITY: APP USAGE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.activity_app_usage (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    app_name TEXT NOT NULL,
    app_bundle_id TEXT,
    app_category TEXT,
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
    window_title TEXT,
    document_path TEXT,
    url TEXT,
    thing_id UUID REFERENCES data.entities_thing(id),
    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT activity_app_usage_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_activity_app_usage_start
    ON data.activity_app_usage(start_time DESC);
CREATE INDEX IF NOT EXISTS idx_activity_app_usage_app
    ON data.activity_app_usage(app_name);
CREATE INDEX IF NOT EXISTS idx_activity_app_usage_thing
    ON data.activity_app_usage(thing_id) WHERE thing_id IS NOT NULL;

DROP TRIGGER IF EXISTS data_activity_app_usage_set_updated_at ON data.activity_app_usage;
CREATE TRIGGER data_activity_app_usage_set_updated_at
    BEFORE UPDATE ON data.activity_app_usage
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- ACTIVITY: WEB BROWSING
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.activity_web_browsing (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    url TEXT NOT NULL,
    domain TEXT NOT NULL,
    page_title TEXT,
    visit_duration_seconds INTEGER,
    scroll_depth_percent FLOAT,
    timestamp TIMESTAMPTZ NOT NULL,
    thing_id UUID REFERENCES data.entities_thing(id),
    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT activity_web_browsing_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_activity_web_browsing_timestamp
    ON data.activity_web_browsing(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_activity_web_browsing_domain
    ON data.activity_web_browsing(domain);
CREATE INDEX IF NOT EXISTS idx_activity_web_browsing_thing
    ON data.activity_web_browsing(thing_id) WHERE thing_id IS NOT NULL;

DROP TRIGGER IF EXISTS data_activity_web_browsing_set_updated_at ON data.activity_web_browsing;
CREATE TRIGGER data_activity_web_browsing_set_updated_at
    BEFORE UPDATE ON data.activity_web_browsing
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- KNOWLEDGE: DOCUMENT
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.knowledge_document (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT,
    content TEXT,
    content_summary TEXT,
    document_type TEXT,
    external_id TEXT,
    external_url TEXT,
    thing_id UUID REFERENCES data.entities_thing(id),
    tags TEXT[] DEFAULT '{}',
    is_authored BOOLEAN DEFAULT false,
    created_time TIMESTAMPTZ,
    last_modified_time TIMESTAMPTZ,
    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata JSONB DEFAULT '{}',
    embedding vector(768),
    embedded_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT knowledge_document_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_knowledge_document_title
    ON data.knowledge_document(title);
CREATE INDEX IF NOT EXISTS idx_knowledge_document_thing
    ON data.knowledge_document(thing_id) WHERE thing_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_knowledge_document_tags
    ON data.knowledge_document USING GIN(tags);
CREATE INDEX IF NOT EXISTS idx_knowledge_document_search
    ON data.knowledge_document USING GIN(to_tsvector('english', coalesce(title, '') || ' ' || coalesce(content, '')));
CREATE INDEX IF NOT EXISTS idx_knowledge_document_embedding
    ON data.knowledge_document USING hnsw (embedding vector_cosine_ops) WITH (m = 16, ef_construction = 64);

DROP TRIGGER IF EXISTS data_knowledge_document_set_updated_at ON data.knowledge_document;
CREATE TRIGGER data_knowledge_document_set_updated_at
    BEFORE UPDATE ON data.knowledge_document
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- KNOWLEDGE: AI CONVERSATION
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.knowledge_ai_conversation (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    conversation_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system')),
    content TEXT NOT NULL,
    model TEXT,
    provider TEXT NOT NULL,
    thing_id UUID REFERENCES data.entities_thing(id),
    tags TEXT[] DEFAULT '{}',
    timestamp TIMESTAMPTZ NOT NULL,
    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL DEFAULT 'stream_virtues_ai_chat',
    source_provider TEXT NOT NULL DEFAULT 'virtues',
    metadata JSONB DEFAULT '{}',
    embedding vector(768),
    embedded_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT knowledge_ai_conversation_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_knowledge_ai_conversation_conversation
    ON data.knowledge_ai_conversation(conversation_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_knowledge_ai_conversation_timestamp
    ON data.knowledge_ai_conversation(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_knowledge_ai_conversation_thing
    ON data.knowledge_ai_conversation(thing_id) WHERE thing_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_knowledge_ai_conversation_embedding
    ON data.knowledge_ai_conversation USING hnsw (embedding vector_cosine_ops) WITH (m = 16, ef_construction = 64);

DROP TRIGGER IF EXISTS data_knowledge_ai_conversation_set_updated_at ON data.knowledge_ai_conversation;
CREATE TRIGGER data_knowledge_ai_conversation_set_updated_at
    BEFORE UPDATE ON data.knowledge_ai_conversation
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- SPEECH: TRANSCRIPTION
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.speech_transcription (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    audio_url TEXT,
    text TEXT NOT NULL,
    language TEXT,
    duration_seconds FLOAT,
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ,
    speaker_count INTEGER,
    speaker_segments JSONB,
    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT speech_transcription_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_speech_transcription_start
    ON data.speech_transcription(start_time DESC);
CREATE INDEX IF NOT EXISTS idx_speech_transcription_search
    ON data.speech_transcription USING GIN(to_tsvector('english', text));

DROP TRIGGER IF EXISTS data_speech_transcription_set_updated_at ON data.speech_transcription;
CREATE TRIGGER data_speech_transcription_set_updated_at
    BEFORE UPDATE ON data.speech_transcription
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- FINANCIAL: ACCOUNT
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.financial_account (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_name TEXT NOT NULL,
    account_type TEXT NOT NULL,
    institution_name TEXT,
    institution_id TEXT,
    mask TEXT,
    currency TEXT DEFAULT 'USD',
    current_balance NUMERIC(15, 2),
    available_balance NUMERIC(15, 2),
    credit_limit NUMERIC(15, 2),
    is_active BOOLEAN DEFAULT true,
    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT financial_account_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_financial_account_type
    ON data.financial_account(account_type);
CREATE INDEX IF NOT EXISTS idx_financial_account_active
    ON data.financial_account(id) WHERE is_active = true;

DROP TRIGGER IF EXISTS data_financial_account_set_updated_at ON data.financial_account;
CREATE TRIGGER data_financial_account_set_updated_at
    BEFORE UPDATE ON data.financial_account
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- FINANCIAL: TRANSACTION
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.financial_transaction (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES data.financial_account(id) ON DELETE CASCADE,
    transaction_id TEXT NOT NULL,
    amount NUMERIC(15, 2) NOT NULL,
    currency TEXT DEFAULT 'USD',
    merchant_name TEXT,
    merchant_category TEXT,
    description TEXT,
    category TEXT[] DEFAULT '{}',
    is_pending BOOLEAN DEFAULT false,
    transaction_type TEXT,
    payment_channel TEXT,
    place_id UUID REFERENCES data.entities_place(id),
    timestamp TIMESTAMPTZ NOT NULL,
    authorized_timestamp TIMESTAMPTZ,
    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata JSONB DEFAULT '{}',
    embedding vector(768),
    embedded_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT financial_transaction_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_financial_transaction_timestamp
    ON data.financial_transaction(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_financial_transaction_account
    ON data.financial_transaction(account_id);
CREATE INDEX IF NOT EXISTS idx_financial_transaction_merchant
    ON data.financial_transaction(merchant_name) WHERE merchant_name IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_financial_transaction_category
    ON data.financial_transaction USING GIN(category);
CREATE INDEX IF NOT EXISTS idx_financial_transaction_embedding
    ON data.financial_transaction USING hnsw (embedding vector_cosine_ops) WITH (m = 16, ef_construction = 64);

DROP TRIGGER IF EXISTS data_financial_transaction_set_updated_at ON data.financial_transaction;
CREATE TRIGGER data_financial_transaction_set_updated_at
    BEFORE UPDATE ON data.financial_transaction
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- FINANCIAL: ASSET
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.financial_asset (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES data.financial_account(id) ON DELETE CASCADE,
    asset_type TEXT NOT NULL,
    symbol TEXT,
    name TEXT,
    quantity NUMERIC(20, 8),
    cost_basis NUMERIC(15, 2),
    current_value NUMERIC(15, 2),
    currency TEXT DEFAULT 'USD',
    timestamp TIMESTAMPTZ NOT NULL,
    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT financial_asset_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_financial_asset_account
    ON data.financial_asset(account_id);
CREATE INDEX IF NOT EXISTS idx_financial_asset_symbol
    ON data.financial_asset(symbol) WHERE symbol IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_financial_asset_timestamp
    ON data.financial_asset(timestamp DESC);

DROP TRIGGER IF EXISTS data_financial_asset_set_updated_at ON data.financial_asset;
CREATE TRIGGER data_financial_asset_set_updated_at
    BEFORE UPDATE ON data.financial_asset
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- FINANCIAL: LIABILITY
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.financial_liability (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES data.financial_account(id) ON DELETE CASCADE,
    liability_type TEXT NOT NULL,
    principal NUMERIC(15, 2),
    interest_rate NUMERIC(6, 4),
    minimum_payment NUMERIC(15, 2),
    next_payment_due_date DATE,
    origination_date DATE,
    maturity_date DATE,
    currency TEXT DEFAULT 'USD',
    timestamp TIMESTAMPTZ NOT NULL,
    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT financial_liability_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_financial_liability_account
    ON data.financial_liability(account_id);
CREATE INDEX IF NOT EXISTS idx_financial_liability_timestamp
    ON data.financial_liability(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_financial_liability_next_payment
    ON data.financial_liability(next_payment_due_date) WHERE next_payment_due_date IS NOT NULL;

DROP TRIGGER IF EXISTS data_financial_liability_set_updated_at ON data.financial_liability;
CREATE TRIGGER data_financial_liability_set_updated_at
    BEFORE UPDATE ON data.financial_liability
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- EMBEDDING JOBS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.embedding_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    target_table TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'running', 'completed', 'failed')),
    records_processed INTEGER DEFAULT 0,
    records_total INTEGER,
    error_message TEXT,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_embedding_jobs_status
    ON data.embedding_jobs(status, created_at DESC);

DROP TRIGGER IF EXISTS data_embedding_jobs_set_updated_at ON data.embedding_jobs;
CREATE TRIGGER data_embedding_jobs_set_updated_at
    BEFORE UPDATE ON data.embedding_jobs
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

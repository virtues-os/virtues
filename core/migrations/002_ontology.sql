-- Ontology: All entity and activity data tables
-- Consolidates: 003, 010

--------------------------------------------------------------------------------
-- ENTITIES: PERSON
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.entities_person (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    canonical_name TEXT NOT NULL,
    email_addresses TEXT[] DEFAULT '{}',
    phone_numbers TEXT[] DEFAULT '{}',
    display_names TEXT[] DEFAULT '{}',

    relationship_category TEXT,

    first_interaction TIMESTAMPTZ,
    last_interaction TIMESTAMPTZ,

    metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_entities_person_name ON data.entities_person(canonical_name);
CREATE INDEX IF NOT EXISTS idx_entities_person_emails ON data.entities_person USING GIN(email_addresses);
CREATE INDEX IF NOT EXISTS idx_entities_person_phones ON data.entities_person USING GIN(phone_numbers);

DROP TRIGGER IF EXISTS entities_person_updated_at ON data.entities_person;
CREATE TRIGGER entities_person_updated_at
    BEFORE UPDATE ON data.entities_person
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- ENTITIES: PLACE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.entities_place (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    canonical_name TEXT NOT NULL,
    category TEXT,

    geo_center GEOGRAPHY(POINT),
    bounding_box GEOGRAPHY(POLYGON),
    cluster_radius_meters FLOAT,

    visit_count INTEGER DEFAULT 0,
    total_time_minutes INTEGER DEFAULT 0,

    metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_entities_place_name ON data.entities_place(canonical_name);
CREATE INDEX IF NOT EXISTS idx_entities_place_geo ON data.entities_place USING GIST(geo_center);

DROP TRIGGER IF EXISTS entities_place_updated_at ON data.entities_place;
CREATE TRIGGER entities_place_updated_at
    BEFORE UPDATE ON data.entities_place
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- ENTITIES: TOPIC
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.entities_topic (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    name TEXT NOT NULL,
    category TEXT,
    keywords TEXT[] DEFAULT '{}',

    first_mentioned TIMESTAMPTZ,
    last_mentioned TIMESTAMPTZ,
    mention_count INTEGER DEFAULT 0,

    sources JSONB DEFAULT '{}',
    metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_entities_topic_name ON data.entities_topic(name);

DROP TRIGGER IF EXISTS entities_topic_updated_at ON data.entities_topic;
CREATE TRIGGER entities_topic_updated_at
    BEFORE UPDATE ON data.entities_topic
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- HEALTH: HEART RATE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.health_heart_rate (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    bpm INTEGER NOT NULL,
    measurement_context TEXT,

    timestamp TIMESTAMPTZ NOT NULL,

    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,

    metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT health_heart_rate_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_health_heart_rate_timestamp ON data.health_heart_rate(timestamp DESC);

--------------------------------------------------------------------------------
-- HEALTH: HRV
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.health_hrv (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    hrv_ms FLOAT NOT NULL,
    measurement_type TEXT,

    timestamp TIMESTAMPTZ NOT NULL,

    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,

    metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT health_hrv_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_health_hrv_timestamp ON data.health_hrv(timestamp DESC);

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

CREATE INDEX IF NOT EXISTS idx_health_steps_timestamp ON data.health_steps(timestamp DESC);

--------------------------------------------------------------------------------
-- HEALTH: SLEEP
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.health_sleep (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    sleep_stages JSONB,
    total_duration_minutes INTEGER NOT NULL,
    sleep_quality_score FLOAT,

    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,

    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,

    metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT health_sleep_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_health_sleep_start_time ON data.health_sleep(start_time DESC);

--------------------------------------------------------------------------------
-- HEALTH: WORKOUT
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.health_workout (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    activity_type TEXT NOT NULL,
    intensity TEXT,

    calories_burned INTEGER,
    average_heart_rate INTEGER,
    max_heart_rate INTEGER,
    distance_meters FLOAT,

    place_id UUID REFERENCES data.entities_place(id),

    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,

    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,

    metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT health_workout_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_health_workout_start_time ON data.health_workout(start_time DESC);
CREATE INDEX IF NOT EXISTS idx_health_workout_place ON data.health_workout(place_id);

--------------------------------------------------------------------------------
-- LOCATION: POINT
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.location_point (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    coordinates GEOGRAPHY(POINT) NOT NULL,
    latitude FLOAT NOT NULL,
    longitude FLOAT NOT NULL,
    altitude_meters FLOAT,

    accuracy_meters FLOAT,
    speed_meters_per_second FLOAT,
    course_degrees FLOAT,

    timestamp TIMESTAMPTZ NOT NULL,

    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,

    metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT location_point_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_location_point_coords ON data.location_point USING GIST(coordinates);
CREATE INDEX IF NOT EXISTS idx_location_point_timestamp ON data.location_point(timestamp DESC);

--------------------------------------------------------------------------------
-- LOCATION: VISIT
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.location_visit (
    id UUID PRIMARY KEY,

    place_id UUID REFERENCES data.entities_place(id),

    centroid_coordinates GEOGRAPHY(POINT) NOT NULL,
    latitude FLOAT NOT NULL,
    longitude FLOAT NOT NULL,

    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,

    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL DEFAULT 'location_point',
    source_provider TEXT NOT NULL DEFAULT 'ios',

    metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_location_visit_centroid ON data.location_visit USING GIST(centroid_coordinates);
CREATE INDEX IF NOT EXISTS idx_location_visit_start_time ON data.location_visit(start_time DESC);
CREATE INDEX IF NOT EXISTS idx_location_visit_end_time ON data.location_visit(end_time DESC);
CREATE INDEX IF NOT EXISTS idx_location_visit_place ON data.location_visit(place_id) WHERE place_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_location_visit_source ON data.location_visit(source_stream_id);

DROP TRIGGER IF EXISTS location_visit_updated_at ON data.location_visit;
CREATE TRIGGER location_visit_updated_at
    BEFORE UPDATE ON data.location_visit
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- SOCIAL: EMAIL
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.social_email (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    message_id TEXT NOT NULL,
    thread_id TEXT,
    subject TEXT,
    snippet TEXT,

    body_plain TEXT,
    body_html TEXT,

    timestamp TIMESTAMPTZ NOT NULL,

    from_address TEXT,
    from_name TEXT,

    to_addresses TEXT[] DEFAULT '{}',
    to_names TEXT[] DEFAULT '{}',
    cc_addresses TEXT[] DEFAULT '{}',
    cc_names TEXT[] DEFAULT '{}',
    bcc_addresses TEXT[] DEFAULT '{}',

    from_person_id UUID REFERENCES data.entities_person(id),
    to_person_ids UUID[] DEFAULT '{}',
    cc_person_ids UUID[] DEFAULT '{}',
    bcc_person_ids UUID[] DEFAULT '{}',

    direction TEXT NOT NULL,
    labels TEXT[] DEFAULT '{}',
    is_read BOOLEAN DEFAULT false,
    is_starred BOOLEAN DEFAULT false,

    has_attachments BOOLEAN DEFAULT false,
    attachment_count INTEGER DEFAULT 0,

    thread_position INTEGER,
    thread_message_count INTEGER,

    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'social_email_direction_check') THEN
        ALTER TABLE data.social_email
        ADD CONSTRAINT social_email_direction_check CHECK (direction IN ('sent', 'received'));
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'social_email_unique_source') THEN
        ALTER TABLE data.social_email
        ADD CONSTRAINT social_email_unique_source UNIQUE (source_table, message_id);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'social_email_unique_stream_id') THEN
        ALTER TABLE data.social_email
        ADD CONSTRAINT social_email_unique_stream_id UNIQUE (source_stream_id);
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_social_email_from_person ON data.social_email(from_person_id);
CREATE INDEX IF NOT EXISTS idx_social_email_timestamp ON data.social_email(timestamp DESC);

DROP TRIGGER IF EXISTS social_email_updated_at ON data.social_email;
CREATE TRIGGER social_email_updated_at
    BEFORE UPDATE ON data.social_email
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- SOCIAL: MESSAGE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.social_message (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    message_id TEXT NOT NULL,
    thread_id TEXT,
    channel TEXT NOT NULL,

    body TEXT,

    timestamp TIMESTAMPTZ NOT NULL,

    from_identifier TEXT,
    from_name TEXT,

    to_identifiers TEXT[] DEFAULT '{}',
    to_names TEXT[] DEFAULT '{}',

    from_person_id UUID REFERENCES data.entities_person(id),
    to_person_ids UUID[] DEFAULT '{}',

    direction TEXT NOT NULL CHECK (direction IN ('sent', 'received')),
    is_read BOOLEAN DEFAULT false,
    is_group_message BOOLEAN DEFAULT false,
    group_name TEXT,

    has_attachments BOOLEAN DEFAULT false,
    attachment_count INTEGER DEFAULT 0,
    attachment_types TEXT[] DEFAULT '{}',

    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,

    metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT social_message_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_social_message_from_person ON data.social_message(from_person_id);
CREATE INDEX IF NOT EXISTS idx_social_message_timestamp ON data.social_message(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_social_message_channel ON data.social_message(channel);
CREATE INDEX IF NOT EXISTS idx_social_message_thread ON data.social_message(thread_id);

DROP TRIGGER IF EXISTS social_message_updated_at ON data.social_message;
CREATE TRIGGER social_message_updated_at
    BEFORE UPDATE ON data.social_message
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- PRAXIS: CALENDAR
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.praxis_calendar (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    title TEXT,
    description TEXT,
    calendar_name TEXT,
    event_type TEXT,

    organizer_identifier TEXT,
    attendee_identifiers TEXT[] DEFAULT '{}',

    organizer_person_id UUID REFERENCES data.entities_person(id),
    attendee_person_ids UUID[] DEFAULT '{}',

    topic_id UUID REFERENCES data.entities_topic(id),
    topic_keywords TEXT[] DEFAULT '{}',

    location_name TEXT,
    place_id UUID REFERENCES data.entities_place(id),

    conference_url TEXT,
    conference_platform TEXT,

    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
    is_all_day BOOLEAN DEFAULT false,

    status TEXT,
    response_status TEXT,

    -- Time blocking fields
    block_type TEXT,                    -- NULL for normal events, 'deep_work', 'routine', 'buffer', 'sacred'
    is_sacred BOOLEAN DEFAULT FALSE,    -- For unmovable blocks

    -- Recurrence
    recurrence_rule TEXT,                -- RRULE if recurring

    -- Links to praxis items (FKs added in 003_identity.sql)
    task_id UUID,                       -- What task this time block is for
    initiative_id UUID,                 -- What initiative this time block is for

    -- Axiological links (optional)
    purpose TEXT,                       -- Why this matters
    value_ids UUID[],                   -- Optional explicit values

    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,

    metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT praxis_calendar_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_praxis_calendar_topic ON data.praxis_calendar(topic_id);
CREATE INDEX IF NOT EXISTS idx_praxis_calendar_place ON data.praxis_calendar(place_id);
CREATE INDEX IF NOT EXISTS idx_praxis_calendar_start_time ON data.praxis_calendar(start_time DESC);
CREATE INDEX IF NOT EXISTS idx_praxis_calendar_block_type ON data.praxis_calendar(block_type) WHERE block_type IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_praxis_calendar_recurrence ON data.praxis_calendar(recurrence_rule) WHERE recurrence_rule IS NOT NULL;

DROP TRIGGER IF EXISTS praxis_calendar_updated_at ON data.praxis_calendar;
CREATE TRIGGER praxis_calendar_updated_at
    BEFORE UPDATE ON data.praxis_calendar
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

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

    topic_id UUID REFERENCES data.entities_topic(id),
    topic_keywords TEXT[] DEFAULT '{}',

    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,

    metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT activity_app_usage_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_activity_app_usage_app_name ON data.activity_app_usage(app_name);
CREATE INDEX IF NOT EXISTS idx_activity_app_usage_app_category ON data.activity_app_usage(app_category);
CREATE INDEX IF NOT EXISTS idx_activity_app_usage_start_time ON data.activity_app_usage(start_time DESC);
CREATE INDEX IF NOT EXISTS idx_activity_app_usage_topic ON data.activity_app_usage(topic_id);

DROP TRIGGER IF EXISTS activity_app_usage_updated_at ON data.activity_app_usage;
CREATE TRIGGER activity_app_usage_updated_at
    BEFORE UPDATE ON data.activity_app_usage
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

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

    topic_id UUID REFERENCES data.entities_topic(id),
    topic_keywords TEXT[] DEFAULT '{}',

    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,

    metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT activity_web_browsing_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_activity_web_browsing_domain ON data.activity_web_browsing(domain);
CREATE INDEX IF NOT EXISTS idx_activity_web_browsing_timestamp ON data.activity_web_browsing(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_activity_web_browsing_topic ON data.activity_web_browsing(topic_id);
CREATE INDEX IF NOT EXISTS idx_activity_web_browsing_url ON data.activity_web_browsing USING hash(url);

DROP TRIGGER IF EXISTS activity_web_browsing_updated_at ON data.activity_web_browsing;
CREATE TRIGGER activity_web_browsing_updated_at
    BEFORE UPDATE ON data.activity_web_browsing
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

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

    topic_id UUID REFERENCES data.entities_topic(id),
    tags TEXT[] DEFAULT '{}',

    is_authored BOOLEAN DEFAULT false,

    created_time TIMESTAMPTZ,
    last_modified_time TIMESTAMPTZ,

    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,

    metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT knowledge_document_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_knowledge_document_title ON data.knowledge_document(title);
CREATE INDEX IF NOT EXISTS idx_knowledge_document_topic ON data.knowledge_document(topic_id);
CREATE INDEX IF NOT EXISTS idx_knowledge_document_tags ON data.knowledge_document USING GIN(tags);
CREATE INDEX IF NOT EXISTS idx_knowledge_document_source ON data.knowledge_document(source_stream_id);
CREATE INDEX IF NOT EXISTS idx_knowledge_document_search ON data.knowledge_document USING GIN(to_tsvector('english', coalesce(title, '') || ' ' || coalesce(content, '')));

DROP TRIGGER IF EXISTS knowledge_document_updated_at ON data.knowledge_document;
CREATE TRIGGER knowledge_document_updated_at
    BEFORE UPDATE ON data.knowledge_document
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

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

    topic_id UUID REFERENCES data.entities_topic(id),
    tags TEXT[] DEFAULT '{}',

    timestamp TIMESTAMPTZ NOT NULL,

    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL DEFAULT 'stream_virtues_ai_chat',
    source_provider TEXT NOT NULL DEFAULT 'virtues',

    metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT knowledge_ai_conversation_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_knowledge_ai_conversation_conversation
    ON data.knowledge_ai_conversation(conversation_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_knowledge_ai_conversation_timestamp
    ON data.knowledge_ai_conversation(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_knowledge_ai_conversation_topic
    ON data.knowledge_ai_conversation(topic_id) WHERE topic_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_knowledge_ai_conversation_provider
    ON data.knowledge_ai_conversation(provider, timestamp DESC);

DROP TRIGGER IF EXISTS knowledge_ai_conversation_updated_at ON data.knowledge_ai_conversation;
CREATE TRIGGER knowledge_ai_conversation_updated_at
    BEFORE UPDATE ON data.knowledge_ai_conversation
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- SPEECH: TRANSCRIPTION
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.speech_transcription (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    audio_file_path TEXT,
    audio_duration_seconds INTEGER,

    transcript_text TEXT NOT NULL,
    language TEXT,
    confidence_score FLOAT,

    speaker_count INTEGER,
    speaker_labels JSONB,

    recorded_at TIMESTAMPTZ NOT NULL,

    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,

    metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT speech_transcription_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_speech_transcription_source ON data.speech_transcription(source_stream_id);
CREATE INDEX IF NOT EXISTS idx_speech_transcription_recorded_at ON data.speech_transcription(recorded_at DESC);
CREATE INDEX IF NOT EXISTS idx_speech_transcription_search ON data.speech_transcription USING GIN(to_tsvector('english', transcript_text));

DROP TRIGGER IF EXISTS speech_transcription_updated_at ON data.speech_transcription;
CREATE TRIGGER speech_transcription_updated_at
    BEFORE UPDATE ON data.speech_transcription
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- FINANCIAL: ACCOUNT (from 010)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.financial_account (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- External identifiers
    account_id_external TEXT NOT NULL,              -- Provider's account ID (e.g., Plaid account_id)
    persistent_account_id TEXT,                     -- Plaid's persistent_account_id (stable across items)

    -- Account details
    account_name TEXT NOT NULL,
    official_name TEXT,                             -- Bank's official name for the account
    account_type TEXT NOT NULL,                     -- depository, credit, loan, investment, brokerage, other
    account_subtype TEXT,                           -- checking, savings, credit_card, mortgage, etc.
    mask TEXT,                                      -- Last 4 digits

    -- Balances (updated on each sync)
    current_balance NUMERIC(28,10),
    available_balance NUMERIC(28,10),
    credit_limit NUMERIC(28,10),
    currency_code TEXT DEFAULT 'USD',

    -- Institution info
    institution_id TEXT,
    institution_name TEXT,

    -- Status
    is_active BOOLEAN DEFAULT true,

    -- Standard ontology fields
    timestamp TIMESTAMPTZ NOT NULL,                 -- When this account was linked/last updated

    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL DEFAULT 'stream_plaid_accounts',
    source_provider TEXT NOT NULL DEFAULT 'plaid',

    metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT financial_account_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_financial_account_external_id
    ON data.financial_account(account_id_external);
CREATE INDEX IF NOT EXISTS idx_financial_account_type
    ON data.financial_account(account_type);
CREATE INDEX IF NOT EXISTS idx_financial_account_institution
    ON data.financial_account(institution_id);
CREATE INDEX IF NOT EXISTS idx_financial_account_timestamp
    ON data.financial_account(timestamp DESC);

DROP TRIGGER IF EXISTS financial_account_updated_at ON data.financial_account;
CREATE TRIGGER financial_account_updated_at
    BEFORE UPDATE ON data.financial_account
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- FINANCIAL: TRANSACTION (from 010)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.financial_transaction (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- External identifiers
    transaction_id_external TEXT NOT NULL,          -- Provider's transaction ID (e.g., Plaid transaction_id)

    -- Account reference (resolved after account sync)
    account_id UUID REFERENCES data.financial_account(id),
    account_id_external TEXT NOT NULL,              -- For matching before account_id is resolved

    -- Amount (positive = money in, negative = money out)
    amount NUMERIC(28,10) NOT NULL,
    currency_code TEXT DEFAULT 'USD',

    -- Dates
    transaction_date DATE NOT NULL,                 -- Date transaction occurred
    authorized_date DATE,                           -- When authorized (may differ from transaction_date)
    posted_date DATE,                               -- When posted to account

    -- Description
    name TEXT NOT NULL,                             -- Original transaction description
    merchant_name TEXT,                             -- Cleaned merchant name (if available)

    -- Categorization
    category TEXT,                                  -- Normalized category (e.g., 'dining', 'groceries')
    category_detailed TEXT,                         -- Provider's detailed category
    personal_finance_category TEXT,                 -- Plaid's personal finance category

    -- Transaction type
    transaction_type TEXT,                          -- digital, place, special, unresolved
    payment_channel TEXT,                           -- online, in store, other

    -- Status
    is_pending BOOLEAN DEFAULT false,

    -- Location (optional, for in-store transactions)
    location_address TEXT,
    location_city TEXT,
    location_region TEXT,
    location_postal_code TEXT,
    location_country TEXT,
    location_lat FLOAT,
    location_lon FLOAT,

    -- Merchant info
    merchant_entity_id TEXT,                        -- Plaid merchant entity ID

    -- Standard ontology fields
    timestamp TIMESTAMPTZ NOT NULL,                 -- transaction_date as timestamp

    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL DEFAULT 'stream_plaid_transactions',
    source_provider TEXT NOT NULL DEFAULT 'plaid',

    metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT financial_transaction_unique_source UNIQUE (source_stream_id)
);

-- Primary indexes for queries
CREATE INDEX IF NOT EXISTS idx_financial_transaction_date
    ON data.financial_transaction(transaction_date DESC);
CREATE INDEX IF NOT EXISTS idx_financial_transaction_account_date
    ON data.financial_transaction(account_id, transaction_date DESC);
CREATE INDEX IF NOT EXISTS idx_financial_transaction_category
    ON data.financial_transaction(category);
CREATE INDEX IF NOT EXISTS idx_financial_transaction_merchant
    ON data.financial_transaction(merchant_name) WHERE merchant_name IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_financial_transaction_pending
    ON data.financial_transaction(is_pending) WHERE is_pending = true;
CREATE INDEX IF NOT EXISTS idx_financial_transaction_external_id
    ON data.financial_transaction(transaction_id_external);
CREATE INDEX IF NOT EXISTS idx_financial_transaction_account_external
    ON data.financial_transaction(account_id_external);
CREATE INDEX IF NOT EXISTS idx_financial_transaction_timestamp
    ON data.financial_transaction(timestamp DESC);

-- Full-text search on transaction names
CREATE INDEX IF NOT EXISTS idx_financial_transaction_search
    ON data.financial_transaction USING GIN(to_tsvector('english', coalesce(name, '') || ' ' || coalesce(merchant_name, '')));

DROP TRIGGER IF EXISTS financial_transaction_updated_at ON data.financial_transaction;
CREATE TRIGGER financial_transaction_updated_at
    BEFORE UPDATE ON data.financial_transaction
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- FINANCIAL: ASSET (Investment Holdings)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.financial_asset (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Account reference
    account_id UUID REFERENCES data.financial_account(id),
    account_id_external TEXT NOT NULL,

    -- Security identifiers
    security_id_external TEXT NOT NULL,
    ticker_symbol TEXT,
    cusip TEXT,
    isin TEXT,
    security_name TEXT NOT NULL,
    security_type TEXT,  -- equity, etf, mutual_fund, bond, option, cryptocurrency

    -- Holdings
    quantity NUMERIC(28,10) NOT NULL,
    cost_basis NUMERIC(28,10),
    institution_value NUMERIC(28,10),
    close_price NUMERIC(28,10),
    currency_code TEXT DEFAULT 'USD',

    -- Timing
    as_of_date DATE NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,

    -- Standard ontology fields
    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL DEFAULT 'stream_plaid_investments',
    source_provider TEXT NOT NULL DEFAULT 'plaid',
    metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT financial_asset_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_financial_asset_account
    ON data.financial_asset(account_id);
CREATE INDEX IF NOT EXISTS idx_financial_asset_account_external
    ON data.financial_asset(account_id_external);
CREATE INDEX IF NOT EXISTS idx_financial_asset_security
    ON data.financial_asset(security_id_external);
CREATE INDEX IF NOT EXISTS idx_financial_asset_ticker
    ON data.financial_asset(ticker_symbol) WHERE ticker_symbol IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_financial_asset_timestamp
    ON data.financial_asset(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_financial_asset_type
    ON data.financial_asset(security_type);

DROP TRIGGER IF EXISTS financial_asset_updated_at ON data.financial_asset;
CREATE TRIGGER financial_asset_updated_at
    BEFORE UPDATE ON data.financial_asset
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- FINANCIAL: LIABILITY (Credit Cards, Mortgages, Loans)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.financial_liability (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Account reference
    account_id UUID REFERENCES data.financial_account(id),
    account_id_external TEXT NOT NULL,

    -- Liability type
    liability_type TEXT NOT NULL,  -- credit_card, mortgage, student_loan

    -- Interest/APR
    apr_percentage NUMERIC(10,4),
    apr_type TEXT,  -- variable, fixed
    interest_rate_percentage NUMERIC(10,4),

    -- Payment info
    minimum_payment NUMERIC(28,10),
    last_payment_amount NUMERIC(28,10),
    last_payment_date DATE,
    next_payment_due_date DATE,
    next_payment_amount NUMERIC(28,10),

    -- Loan specifics
    original_loan_amount NUMERIC(28,10),
    outstanding_balance NUMERIC(28,10),
    loan_term_months INTEGER,
    origination_date DATE,
    maturity_date DATE,

    -- Mortgage specifics
    property_address TEXT,
    property_city TEXT,
    property_region TEXT,
    property_postal_code TEXT,
    escrow_balance NUMERIC(28,10),

    -- Standard ontology fields
    timestamp TIMESTAMPTZ NOT NULL,
    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL DEFAULT 'stream_plaid_liabilities',
    source_provider TEXT NOT NULL DEFAULT 'plaid',
    metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT financial_liability_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_financial_liability_account
    ON data.financial_liability(account_id);
CREATE INDEX IF NOT EXISTS idx_financial_liability_account_external
    ON data.financial_liability(account_id_external);
CREATE INDEX IF NOT EXISTS idx_financial_liability_type
    ON data.financial_liability(liability_type);
CREATE INDEX IF NOT EXISTS idx_financial_liability_timestamp
    ON data.financial_liability(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_financial_liability_next_payment
    ON data.financial_liability(next_payment_due_date) WHERE next_payment_due_date IS NOT NULL;

DROP TRIGGER IF EXISTS financial_liability_updated_at ON data.financial_liability;
CREATE TRIGGER financial_liability_updated_at
    BEFORE UPDATE ON data.financial_liability
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

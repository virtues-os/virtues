-- Ontology: Health, Location, Social, Activity, Knowledge, Speech, Financial (SQLite)

--------------------------------------------------------------------------------
-- HEALTH: HEART RATE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_health_heart_rate (
    id TEXT PRIMARY KEY,
    bpm INTEGER NOT NULL,
    timestamp TEXT NOT NULL,
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata TEXT DEFAULT '{}',  -- JSON
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_health_heart_rate_timestamp
    ON data_health_heart_rate(timestamp DESC);

CREATE TRIGGER IF NOT EXISTS data_health_heart_rate_set_updated_at
    AFTER UPDATE ON data_health_heart_rate
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_health_heart_rate SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- HEALTH: HRV
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_health_hrv (
    id TEXT PRIMARY KEY,
    hrv_ms REAL NOT NULL,
    timestamp TEXT NOT NULL,
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata TEXT DEFAULT '{}',  -- JSON
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_health_hrv_timestamp
    ON data_health_hrv(timestamp DESC);

CREATE TRIGGER IF NOT EXISTS data_health_hrv_set_updated_at
    AFTER UPDATE ON data_health_hrv
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_health_hrv SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- HEALTH: STEPS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_health_steps (
    id TEXT PRIMARY KEY,
    step_count INTEGER NOT NULL,
    timestamp TEXT NOT NULL,
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata TEXT DEFAULT '{}',  -- JSON
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_health_steps_timestamp
    ON data_health_steps(timestamp DESC);

CREATE TRIGGER IF NOT EXISTS data_health_steps_set_updated_at
    AFTER UPDATE ON data_health_steps
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_health_steps SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- HEALTH: SLEEP
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_health_sleep (
    id TEXT PRIMARY KEY,
    sleep_stages TEXT,  -- JSON
    start_time TEXT NOT NULL,
    end_time TEXT NOT NULL,
    duration_minutes INTEGER,
    sleep_quality_score REAL,
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata TEXT DEFAULT '{}',  -- JSON
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_health_sleep_start
    ON data_health_sleep(start_time DESC);

CREATE TRIGGER IF NOT EXISTS data_health_sleep_set_updated_at
    AFTER UPDATE ON data_health_sleep
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_health_sleep SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- HEALTH: WORKOUT
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_health_workout (
    id TEXT PRIMARY KEY,
    workout_type TEXT NOT NULL,
    start_time TEXT NOT NULL,
    end_time TEXT NOT NULL,
    duration_minutes INTEGER,
    calories_burned INTEGER,
    distance_km REAL,
    avg_heart_rate INTEGER,
    max_heart_rate INTEGER,
    place_id TEXT REFERENCES data_entities_place(id),
    route_geometry TEXT,  -- GeoJSON LineString
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata TEXT DEFAULT '{}',  -- JSON
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_health_workout_start
    ON data_health_workout(start_time DESC);
CREATE INDEX IF NOT EXISTS idx_health_workout_type
    ON data_health_workout(workout_type);

CREATE TRIGGER IF NOT EXISTS data_health_workout_set_updated_at
    AFTER UPDATE ON data_health_workout
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_health_workout SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- LOCATION: POINT
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_location_point (
    id TEXT PRIMARY KEY,
    latitude REAL NOT NULL,
    longitude REAL NOT NULL,
    altitude REAL,
    horizontal_accuracy REAL,
    vertical_accuracy REAL,
    timestamp TEXT NOT NULL,
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata TEXT DEFAULT '{}',  -- JSON
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_location_point_timestamp
    ON data_location_point(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_location_point_coords
    ON data_location_point(latitude, longitude);

CREATE TRIGGER IF NOT EXISTS data_location_point_set_updated_at
    AFTER UPDATE ON data_location_point
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_location_point SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- LOCATION: VISIT
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_location_visit (
    id TEXT PRIMARY KEY,
    place_id TEXT REFERENCES data_entities_place(id),
    place_name TEXT,
    latitude REAL NOT NULL,
    longitude REAL NOT NULL,
    arrival_time TEXT NOT NULL,
    departure_time TEXT,
    duration_minutes INTEGER,
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata TEXT DEFAULT '{}',  -- JSON
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_location_visit_arrival
    ON data_location_visit(arrival_time DESC);
CREATE INDEX IF NOT EXISTS idx_location_visit_place
    ON data_location_visit(place_id) WHERE place_id IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS data_location_visit_set_updated_at
    AFTER UPDATE ON data_location_visit
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_location_visit SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- SOCIAL: EMAIL
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_social_email (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL,
    thread_id TEXT,
    subject TEXT,
    body TEXT,
    body_preview TEXT,
    from_email TEXT NOT NULL,
    from_name TEXT,
    from_person_id TEXT REFERENCES data_entities_person(id),
    to_emails TEXT DEFAULT '[]',  -- JSON array
    to_names TEXT DEFAULT '[]',  -- JSON array
    to_person_ids TEXT DEFAULT '[]',  -- JSON array
    cc_emails TEXT DEFAULT '[]',  -- JSON array
    cc_person_ids TEXT DEFAULT '[]',  -- JSON array
    bcc_emails TEXT DEFAULT '[]',  -- JSON array
    bcc_person_ids TEXT DEFAULT '[]',  -- JSON array
    direction TEXT NOT NULL CHECK (direction IN ('sent', 'received')),
    is_read INTEGER DEFAULT 0,
    is_starred INTEGER DEFAULT 0,
    has_attachments INTEGER DEFAULT 0,
    labels TEXT DEFAULT '[]',  -- JSON array
    timestamp TEXT NOT NULL,
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata TEXT DEFAULT '{}',  -- JSON
    -- Vector embedding deferred (removed for SQLite migration)
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_social_email_timestamp
    ON data_social_email(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_social_email_thread
    ON data_social_email(thread_id) WHERE thread_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_social_email_from_person
    ON data_social_email(from_person_id) WHERE from_person_id IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS data_social_email_set_updated_at
    AFTER UPDATE ON data_social_email
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_social_email SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- SOCIAL: MESSAGE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_social_message (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL,
    conversation_id TEXT,
    platform TEXT NOT NULL,
    content TEXT,
    from_identifier TEXT NOT NULL,
    from_name TEXT,
    from_person_id TEXT REFERENCES data_entities_person(id),
    to_identifiers TEXT DEFAULT '[]',  -- JSON array
    to_person_ids TEXT DEFAULT '[]',  -- JSON array
    is_read INTEGER DEFAULT 0,
    is_group_message INTEGER DEFAULT 0,
    reply_to_message_id TEXT,
    has_attachments INTEGER DEFAULT 0,
    timestamp TEXT NOT NULL,
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata TEXT DEFAULT '{}',  -- JSON
    -- Vector embedding deferred
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_social_message_timestamp
    ON data_social_message(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_social_message_conversation
    ON data_social_message(conversation_id) WHERE conversation_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_social_message_platform
    ON data_social_message(platform);

CREATE TRIGGER IF NOT EXISTS data_social_message_set_updated_at
    AFTER UPDATE ON data_social_message
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_social_message SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- ACTIVITY: APP USAGE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_activity_app_usage (
    id TEXT PRIMARY KEY,
    app_name TEXT NOT NULL,
    app_bundle_id TEXT,
    app_category TEXT,
    start_time TEXT NOT NULL,
    end_time TEXT NOT NULL,
    window_title TEXT,
    document_path TEXT,
    url TEXT,
    thing_id TEXT REFERENCES data_entities_thing(id),
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata TEXT DEFAULT '{}',  -- JSON
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_activity_app_usage_start
    ON data_activity_app_usage(start_time DESC);
CREATE INDEX IF NOT EXISTS idx_activity_app_usage_app
    ON data_activity_app_usage(app_name);
CREATE INDEX IF NOT EXISTS idx_activity_app_usage_thing
    ON data_activity_app_usage(thing_id) WHERE thing_id IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS data_activity_app_usage_set_updated_at
    AFTER UPDATE ON data_activity_app_usage
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_activity_app_usage SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- ACTIVITY: WEB BROWSING
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_activity_web_browsing (
    id TEXT PRIMARY KEY,
    url TEXT NOT NULL,
    domain TEXT NOT NULL,
    page_title TEXT,
    visit_duration_seconds INTEGER,
    scroll_depth_percent REAL,
    timestamp TEXT NOT NULL,
    thing_id TEXT REFERENCES data_entities_thing(id),
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata TEXT DEFAULT '{}',  -- JSON
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_activity_web_browsing_timestamp
    ON data_activity_web_browsing(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_activity_web_browsing_domain
    ON data_activity_web_browsing(domain);
CREATE INDEX IF NOT EXISTS idx_activity_web_browsing_thing
    ON data_activity_web_browsing(thing_id) WHERE thing_id IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS data_activity_web_browsing_set_updated_at
    AFTER UPDATE ON data_activity_web_browsing
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_activity_web_browsing SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- KNOWLEDGE: DOCUMENT
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_knowledge_document (
    id TEXT PRIMARY KEY,
    title TEXT,
    content TEXT,
    content_summary TEXT,
    document_type TEXT,
    external_id TEXT,
    external_url TEXT,
    thing_id TEXT REFERENCES data_entities_thing(id),
    tags TEXT DEFAULT '[]',  -- JSON array
    is_authored INTEGER DEFAULT 0,
    created_time TEXT,
    last_modified_time TEXT,
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata TEXT DEFAULT '{}',  -- JSON
    -- Vector embedding deferred
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_knowledge_document_title
    ON data_knowledge_document(title);
CREATE INDEX IF NOT EXISTS idx_knowledge_document_thing
    ON data_knowledge_document(thing_id) WHERE thing_id IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS data_knowledge_document_set_updated_at
    AFTER UPDATE ON data_knowledge_document
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_knowledge_document SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- KNOWLEDGE: AI CONVERSATION
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_knowledge_ai_conversation (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system')),
    content TEXT NOT NULL,
    model TEXT,
    provider TEXT NOT NULL,
    thing_id TEXT REFERENCES data_entities_thing(id),
    tags TEXT DEFAULT '[]',  -- JSON array
    timestamp TEXT NOT NULL,
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL DEFAULT 'stream_virtues_ai_chat',
    source_provider TEXT NOT NULL DEFAULT 'virtues',
    metadata TEXT DEFAULT '{}',  -- JSON
    -- Vector embedding deferred
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_knowledge_ai_conversation_conversation
    ON data_knowledge_ai_conversation(conversation_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_knowledge_ai_conversation_timestamp
    ON data_knowledge_ai_conversation(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_knowledge_ai_conversation_thing
    ON data_knowledge_ai_conversation(thing_id) WHERE thing_id IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS data_knowledge_ai_conversation_set_updated_at
    AFTER UPDATE ON data_knowledge_ai_conversation
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_knowledge_ai_conversation SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- SPEECH: TRANSCRIPTION
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_speech_transcription (
    id TEXT PRIMARY KEY,
    audio_url TEXT,
    text TEXT NOT NULL,
    language TEXT,
    duration_seconds REAL,
    start_time TEXT NOT NULL,
    end_time TEXT,
    speaker_count INTEGER,
    speaker_segments TEXT,  -- JSON
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata TEXT DEFAULT '{}',  -- JSON
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_speech_transcription_start
    ON data_speech_transcription(start_time DESC);

CREATE TRIGGER IF NOT EXISTS data_speech_transcription_set_updated_at
    AFTER UPDATE ON data_speech_transcription
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_speech_transcription SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- FINANCIAL: ACCOUNT
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_financial_account (
    id TEXT PRIMARY KEY,
    account_name TEXT NOT NULL,
    account_type TEXT NOT NULL,
    institution_name TEXT,
    institution_id TEXT,
    mask TEXT,
    currency TEXT DEFAULT 'USD',
    current_balance REAL,
    available_balance REAL,
    credit_limit REAL,
    is_active INTEGER DEFAULT 1,
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata TEXT DEFAULT '{}',  -- JSON
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_financial_account_type
    ON data_financial_account(account_type);
CREATE INDEX IF NOT EXISTS idx_financial_account_active
    ON data_financial_account(id) WHERE is_active = 1;

CREATE TRIGGER IF NOT EXISTS data_financial_account_set_updated_at
    AFTER UPDATE ON data_financial_account
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_financial_account SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- FINANCIAL: TRANSACTION
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_financial_transaction (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES data_financial_account(id) ON DELETE CASCADE,
    transaction_id TEXT NOT NULL,
    amount REAL NOT NULL,
    currency TEXT DEFAULT 'USD',
    merchant_name TEXT,
    merchant_category TEXT,
    description TEXT,
    category TEXT DEFAULT '[]',  -- JSON array
    is_pending INTEGER DEFAULT 0,
    transaction_type TEXT,
    payment_channel TEXT,
    place_id TEXT REFERENCES data_entities_place(id),
    timestamp TEXT NOT NULL,
    authorized_timestamp TEXT,
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata TEXT DEFAULT '{}',  -- JSON
    -- Vector embedding deferred
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_financial_transaction_timestamp
    ON data_financial_transaction(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_financial_transaction_account
    ON data_financial_transaction(account_id);
CREATE INDEX IF NOT EXISTS idx_financial_transaction_merchant
    ON data_financial_transaction(merchant_name) WHERE merchant_name IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS data_financial_transaction_set_updated_at
    AFTER UPDATE ON data_financial_transaction
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_financial_transaction SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- FINANCIAL: ASSET
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_financial_asset (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES data_financial_account(id) ON DELETE CASCADE,
    asset_type TEXT NOT NULL,
    symbol TEXT,
    name TEXT,
    quantity REAL,
    cost_basis REAL,
    current_value REAL,
    currency TEXT DEFAULT 'USD',
    timestamp TEXT NOT NULL,
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata TEXT DEFAULT '{}',  -- JSON
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_financial_asset_account
    ON data_financial_asset(account_id);
CREATE INDEX IF NOT EXISTS idx_financial_asset_symbol
    ON data_financial_asset(symbol) WHERE symbol IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_financial_asset_timestamp
    ON data_financial_asset(timestamp DESC);

CREATE TRIGGER IF NOT EXISTS data_financial_asset_set_updated_at
    AFTER UPDATE ON data_financial_asset
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_financial_asset SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- FINANCIAL: LIABILITY
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_financial_liability (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES data_financial_account(id) ON DELETE CASCADE,
    liability_type TEXT NOT NULL,
    principal REAL,
    interest_rate REAL,
    minimum_payment REAL,
    next_payment_due_date TEXT,
    origination_date TEXT,
    maturity_date TEXT,
    currency TEXT DEFAULT 'USD',
    timestamp TEXT NOT NULL,
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata TEXT DEFAULT '{}',  -- JSON
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_financial_liability_account
    ON data_financial_liability(account_id);
CREATE INDEX IF NOT EXISTS idx_financial_liability_timestamp
    ON data_financial_liability(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_financial_liability_next_payment
    ON data_financial_liability(next_payment_due_date) WHERE next_payment_due_date IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS data_financial_liability_set_updated_at
    AFTER UPDATE ON data_financial_liability
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_financial_liability SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- EMBEDDING JOBS (deferred - keep table for future use)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data_embedding_jobs (
    id TEXT PRIMARY KEY,
    target_table TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'running', 'completed', 'failed')),
    records_processed INTEGER DEFAULT 0,
    records_total INTEGER,
    error_message TEXT,
    started_at TEXT,
    completed_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_embedding_jobs_status
    ON data_embedding_jobs(status, created_at DESC);

CREATE TRIGGER IF NOT EXISTS data_embedding_jobs_set_updated_at
    AFTER UPDATE ON data_embedding_jobs
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_embedding_jobs SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- 0007 — Data ingest (the raw timeseries layer).
--
-- Every table below shares the standard provenance suffix:
--   source_connection_id  → elt_source_connections.id
--   source_stream_id      UNIQUE        (idempotent re-ingest)
--   source_table          provenance label
--   source_provider       provenance label
--   deleted_at_source     soft-delete from origin
--   is_archived           soft-archive locally
--   metadata              JSONB
--   created_at / updated_at (with trigger)

-- ---------------------------------------------------------------------------
-- HEALTH
-- ---------------------------------------------------------------------------
CREATE TABLE data_health_heart_rate (
    id                     TEXT PRIMARY KEY,
    source_connection_id   TEXT REFERENCES elt_source_connections(id),
    bpm                    INTEGER NOT NULL,
    timestamp              TIMESTAMPTZ NOT NULL,
    source_stream_id       TEXT NOT NULL UNIQUE,
    source_table           TEXT NOT NULL,
    source_provider        TEXT NOT NULL,
    deleted_at_source      TIMESTAMPTZ,
    is_archived            BOOLEAN NOT NULL DEFAULT FALSE,
    metadata               JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_health_heart_rate_timestamp ON data_health_heart_rate(timestamp DESC);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON data_health_heart_rate
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE data_health_hrv (
    id                    TEXT PRIMARY KEY,
    source_connection_id  TEXT REFERENCES elt_source_connections(id),
    hrv_ms                DOUBLE PRECISION NOT NULL,
    timestamp             TIMESTAMPTZ NOT NULL,
    source_stream_id      TEXT NOT NULL UNIQUE,
    source_table          TEXT NOT NULL,
    source_provider       TEXT NOT NULL,
    deleted_at_source     TIMESTAMPTZ,
    is_archived           BOOLEAN NOT NULL DEFAULT FALSE,
    metadata              JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_health_hrv_timestamp ON data_health_hrv(timestamp DESC);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON data_health_hrv
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE data_health_steps (
    id                    TEXT PRIMARY KEY,
    source_connection_id  TEXT REFERENCES elt_source_connections(id),
    step_count            INTEGER NOT NULL,
    timestamp             TIMESTAMPTZ NOT NULL,
    source_stream_id      TEXT NOT NULL UNIQUE,
    source_table          TEXT NOT NULL,
    source_provider       TEXT NOT NULL,
    deleted_at_source     TIMESTAMPTZ,
    is_archived           BOOLEAN NOT NULL DEFAULT FALSE,
    metadata              JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_health_steps_timestamp ON data_health_steps(timestamp DESC);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON data_health_steps
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE data_health_sleep (
    id                     TEXT PRIMARY KEY,
    source_connection_id   TEXT REFERENCES elt_source_connections(id),
    sleep_stages           JSONB,
    start_time             TIMESTAMPTZ NOT NULL,
    end_time               TIMESTAMPTZ NOT NULL,
    duration_minutes       INTEGER,
    sleep_quality_score    DOUBLE PRECISION,
    source_stream_id       TEXT NOT NULL UNIQUE,
    source_table           TEXT NOT NULL,
    source_provider        TEXT NOT NULL,
    deleted_at_source      TIMESTAMPTZ,
    is_archived            BOOLEAN NOT NULL DEFAULT FALSE,
    metadata               JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_health_sleep_start ON data_health_sleep(start_time DESC);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON data_health_sleep
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE data_health_workout (
    id                    TEXT PRIMARY KEY,
    source_connection_id  TEXT REFERENCES elt_source_connections(id),
    workout_type          TEXT NOT NULL,
    start_time            TIMESTAMPTZ NOT NULL,
    end_time              TIMESTAMPTZ NOT NULL,
    duration_minutes      INTEGER,
    calories_burned       INTEGER,
    distance_km           DOUBLE PRECISION,
    avg_heart_rate        INTEGER,
    max_heart_rate        INTEGER,
    route_geometry        JSONB,
    source_stream_id      TEXT NOT NULL UNIQUE,
    source_table          TEXT NOT NULL,
    source_provider       TEXT NOT NULL,
    deleted_at_source     TIMESTAMPTZ,
    is_archived           BOOLEAN NOT NULL DEFAULT FALSE,
    metadata              JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_health_workout_start ON data_health_workout(start_time DESC);
CREATE INDEX idx_health_workout_type  ON data_health_workout(workout_type);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON data_health_workout
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE data_health_active_energy (
    id                TEXT PRIMARY KEY,
    kcal              DOUBLE PRECISION NOT NULL,
    timestamp         TIMESTAMPTZ NOT NULL,
    source_stream_id  TEXT NOT NULL UNIQUE,
    source_table      TEXT NOT NULL,
    source_provider   TEXT NOT NULL,
    metadata          JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_data_health_active_energy_ts ON data_health_active_energy(timestamp DESC);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON data_health_active_energy
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE data_health_distance (
    id                TEXT PRIMARY KEY,
    meters            DOUBLE PRECISION NOT NULL,
    timestamp         TIMESTAMPTZ NOT NULL,
    source_stream_id  TEXT NOT NULL UNIQUE,
    source_table      TEXT NOT NULL,
    source_provider   TEXT NOT NULL,
    metadata          JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_data_health_distance_ts ON data_health_distance(timestamp DESC);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON data_health_distance
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

-- ---------------------------------------------------------------------------
-- LOCATION
-- ---------------------------------------------------------------------------
CREATE TABLE data_location_point (
    id                    TEXT PRIMARY KEY,
    source_connection_id  TEXT REFERENCES elt_source_connections(id),
    latitude              DOUBLE PRECISION NOT NULL,
    longitude             DOUBLE PRECISION NOT NULL,
    altitude              DOUBLE PRECISION,
    horizontal_accuracy   DOUBLE PRECISION,
    vertical_accuracy     DOUBLE PRECISION,
    timestamp             TIMESTAMPTZ NOT NULL,
    source_stream_id      TEXT NOT NULL UNIQUE,
    source_table          TEXT NOT NULL,
    source_provider       TEXT NOT NULL,
    deleted_at_source     TIMESTAMPTZ,
    is_archived           BOOLEAN NOT NULL DEFAULT FALSE,
    metadata              JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_location_point_timestamp ON data_location_point(timestamp DESC);
CREATE INDEX idx_location_point_coords    ON data_location_point(latitude, longitude);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON data_location_point
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE data_location_visit (
    id                    TEXT PRIMARY KEY,
    source_connection_id  TEXT REFERENCES elt_source_connections(id),
    place_name            TEXT,
    latitude              DOUBLE PRECISION NOT NULL,
    longitude             DOUBLE PRECISION NOT NULL,
    arrival_time          TIMESTAMPTZ NOT NULL,
    departure_time        TIMESTAMPTZ,
    duration_minutes      INTEGER,
    source_stream_id      TEXT NOT NULL UNIQUE,
    source_table          TEXT NOT NULL,
    source_provider       TEXT NOT NULL,
    deleted_at_source     TIMESTAMPTZ,
    is_archived           BOOLEAN NOT NULL DEFAULT FALSE,
    metadata              JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_location_visit_arrival ON data_location_visit(arrival_time DESC);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON data_location_visit
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

-- ---------------------------------------------------------------------------
-- ACTIVITY
-- ---------------------------------------------------------------------------
CREATE TABLE data_activity_app_usage (
    id                    TEXT PRIMARY KEY,
    source_connection_id  TEXT REFERENCES elt_source_connections(id),
    app_name              TEXT NOT NULL,
    app_bundle_id         TEXT,
    app_category          TEXT,
    start_time            TIMESTAMPTZ NOT NULL,
    end_time              TIMESTAMPTZ NOT NULL,
    window_title          TEXT,
    document_path         TEXT,
    url                   TEXT,
    source_stream_id      TEXT NOT NULL UNIQUE,
    source_table          TEXT NOT NULL,
    source_provider       TEXT NOT NULL,
    deleted_at_source     TIMESTAMPTZ,
    is_archived           BOOLEAN NOT NULL DEFAULT FALSE,
    metadata              JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_activity_app_usage_start ON data_activity_app_usage(start_time DESC);
CREATE INDEX idx_activity_app_usage_app   ON data_activity_app_usage(app_name);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON data_activity_app_usage
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE data_activity_web_browsing (
    id                       TEXT PRIMARY KEY,
    source_connection_id     TEXT REFERENCES elt_source_connections(id),
    url                      TEXT NOT NULL,
    domain                   TEXT NOT NULL,
    page_title               TEXT,
    visit_duration_seconds   INTEGER,
    scroll_depth_percent     DOUBLE PRECISION,
    timestamp                TIMESTAMPTZ NOT NULL,
    source_stream_id         TEXT NOT NULL UNIQUE,
    source_table             TEXT NOT NULL,
    source_provider          TEXT NOT NULL,
    deleted_at_source        TIMESTAMPTZ,
    is_archived              BOOLEAN NOT NULL DEFAULT FALSE,
    metadata                 JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at               TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_activity_web_browsing_timestamp ON data_activity_web_browsing(timestamp DESC);
CREATE INDEX idx_activity_web_browsing_domain    ON data_activity_web_browsing(domain);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON data_activity_web_browsing
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE data_activity_listening (
    id                    TEXT PRIMARY KEY,
    source_connection_id  TEXT REFERENCES elt_source_connections(id),
    track_name            TEXT NOT NULL,
    artist_name           TEXT,
    album_name            TEXT,
    duration_ms           INTEGER,
    played_at             TIMESTAMPTZ NOT NULL,
    spotify_track_id      TEXT,
    spotify_uri           TEXT,
    context_type          TEXT,
    context_name          TEXT,
    context_uri           TEXT,
    source_stream_id      TEXT NOT NULL UNIQUE,
    source_table          TEXT NOT NULL,
    source_provider       TEXT NOT NULL,
    deleted_at_source     TIMESTAMPTZ,
    is_archived           BOOLEAN NOT NULL DEFAULT FALSE,
    metadata              JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_activity_listening_played_at ON data_activity_listening(played_at);
CREATE INDEX idx_activity_listening_source    ON data_activity_listening(source_connection_id);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON data_activity_listening
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

-- ---------------------------------------------------------------------------
-- AUDIO
-- ---------------------------------------------------------------------------
CREATE TABLE data_audio_recording (
    id                TEXT PRIMARY KEY,
    source_stream_id  TEXT NOT NULL UNIQUE,
    started_at        TIMESTAMPTZ NOT NULL,
    ended_at          TIMESTAMPTZ,
    duration_seconds  DOUBLE PRECISION,
    audio_url         TEXT NOT NULL,
    audio_format      TEXT NOT NULL DEFAULT 'm4a',
    is_silent         BOOLEAN NOT NULL DEFAULT FALSE,
    average_db_level  DOUBLE PRECISION,
    source_table      TEXT NOT NULL,
    source_provider   TEXT NOT NULL,
    metadata          JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_data_audio_recording_started ON data_audio_recording(started_at DESC);
CREATE INDEX idx_data_audio_recording_drain   ON data_audio_recording(is_silent, created_at);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON data_audio_recording
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

-- ---------------------------------------------------------------------------
-- CALENDAR
-- ---------------------------------------------------------------------------
CREATE TABLE data_calendar_event (
    id                    TEXT PRIMARY KEY,
    source_connection_id  TEXT REFERENCES elt_source_connections(id),
    title                 TEXT NOT NULL,
    description           TEXT,
    calendar_name         TEXT,
    event_type            TEXT,
    status                TEXT,
    response_status       TEXT,
    organizer_identifier  TEXT,
    attendee_identifiers  JSONB NOT NULL DEFAULT '[]'::jsonb,
    location_name         TEXT,
    conference_url        TEXT,
    conference_platform   TEXT,
    start_time            TIMESTAMPTZ NOT NULL,
    end_time              TIMESTAMPTZ NOT NULL,
    is_all_day            BOOLEAN NOT NULL DEFAULT FALSE,
    timezone              TEXT,
    recurrence_rule       TEXT,
    block_type            TEXT,
    is_sacred             BOOLEAN NOT NULL DEFAULT FALSE,
    source_stream_id      TEXT NOT NULL UNIQUE,
    source_table          TEXT NOT NULL,
    source_provider       TEXT NOT NULL,
    external_id           TEXT,
    external_url          TEXT,
    deleted_at_source     TIMESTAMPTZ,
    is_archived           BOOLEAN NOT NULL DEFAULT FALSE,
    metadata              JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_calendar_start      ON data_calendar_event(start_time DESC);
CREATE INDEX idx_calendar_block_type ON data_calendar_event(block_type) WHERE block_type IS NOT NULL;
CREATE TRIGGER set_updated_at BEFORE UPDATE ON data_calendar_event
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

-- ---------------------------------------------------------------------------
-- COMMUNICATION
-- ---------------------------------------------------------------------------
CREATE TABLE data_communication_email (
    id                    TEXT PRIMARY KEY,
    source_connection_id  TEXT REFERENCES elt_source_connections(id),
    message_id            TEXT NOT NULL,
    thread_id             TEXT,
    subject               TEXT,
    body                  TEXT,
    body_preview          TEXT,
    from_email            TEXT NOT NULL,
    from_name             TEXT,
    to_emails             JSONB NOT NULL DEFAULT '[]'::jsonb,
    to_names              JSONB NOT NULL DEFAULT '[]'::jsonb,
    cc_emails             JSONB NOT NULL DEFAULT '[]'::jsonb,
    bcc_emails            JSONB NOT NULL DEFAULT '[]'::jsonb,
    direction             TEXT NOT NULL CHECK (direction IN ('sent', 'received')),
    is_read               BOOLEAN NOT NULL DEFAULT FALSE,
    is_starred            BOOLEAN NOT NULL DEFAULT FALSE,
    has_attachments       BOOLEAN NOT NULL DEFAULT FALSE,
    labels                JSONB NOT NULL DEFAULT '[]'::jsonb,
    timestamp             TIMESTAMPTZ NOT NULL,
    source_stream_id      TEXT NOT NULL UNIQUE,
    source_table          TEXT NOT NULL,
    source_provider       TEXT NOT NULL,
    deleted_at_source     TIMESTAMPTZ,
    is_archived           BOOLEAN NOT NULL DEFAULT FALSE,
    metadata              JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_social_email_timestamp ON data_communication_email(timestamp DESC);
CREATE INDEX idx_social_email_thread    ON data_communication_email(thread_id) WHERE thread_id IS NOT NULL;
CREATE TRIGGER set_updated_at BEFORE UPDATE ON data_communication_email
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE data_communication_message (
    id                       TEXT PRIMARY KEY,
    source_connection_id     TEXT REFERENCES elt_source_connections(id),
    message_id               TEXT NOT NULL,
    thread_id                TEXT,
    channel                  TEXT NOT NULL,
    body                     TEXT,
    from_identifier          TEXT NOT NULL,
    from_name                TEXT,
    to_identifiers           JSONB NOT NULL DEFAULT '[]'::jsonb,
    is_read                  BOOLEAN NOT NULL DEFAULT FALSE,
    is_group_message         BOOLEAN NOT NULL DEFAULT FALSE,
    reply_to_message_id      TEXT,
    has_attachments          BOOLEAN NOT NULL DEFAULT FALSE,
    timestamp                TIMESTAMPTZ NOT NULL,
    source_stream_id         TEXT NOT NULL UNIQUE,
    source_table             TEXT NOT NULL,
    source_provider          TEXT NOT NULL,
    deleted_at_source        TIMESTAMPTZ,
    is_archived              BOOLEAN NOT NULL DEFAULT FALSE,
    metadata                 JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at               TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_social_message_timestamp ON data_communication_message(timestamp DESC);
CREATE INDEX idx_social_message_thread    ON data_communication_message(thread_id) WHERE thread_id IS NOT NULL;
CREATE INDEX idx_social_message_channel   ON data_communication_message(channel);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON data_communication_message
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE data_communication_transcription (
    id                    TEXT PRIMARY KEY,
    source_connection_id  TEXT REFERENCES elt_source_connections(id),
    audio_url             TEXT,
    text                  TEXT NOT NULL,
    language              TEXT,
    duration_seconds      DOUBLE PRECISION,
    start_time            TIMESTAMPTZ NOT NULL,
    end_time              TIMESTAMPTZ,
    speaker_count         INTEGER,
    speaker_segments      JSONB,
    title                 TEXT,
    summary               TEXT,
    confidence            DOUBLE PRECISION,
    tags                  JSONB NOT NULL DEFAULT '[]'::jsonb,
    entities              JSONB NOT NULL DEFAULT '{}'::jsonb,
    source_stream_id      TEXT NOT NULL UNIQUE,
    source_table          TEXT NOT NULL,
    source_provider       TEXT NOT NULL,
    deleted_at_source     TIMESTAMPTZ,
    is_archived           BOOLEAN NOT NULL DEFAULT FALSE,
    metadata              JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_speech_transcription_start       ON data_communication_transcription(start_time DESC);
CREATE INDEX idx_communication_transcription_end  ON data_communication_transcription(end_time DESC);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON data_communication_transcription
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

-- ---------------------------------------------------------------------------
-- CONTENT
-- ---------------------------------------------------------------------------
CREATE TABLE data_content_document (
    id                    TEXT PRIMARY KEY,
    source_connection_id  TEXT REFERENCES elt_source_connections(id),
    title                 TEXT,
    content               TEXT,
    content_summary       TEXT,
    document_type         TEXT,
    external_id           TEXT,
    external_url          TEXT,
    tags                  JSONB NOT NULL DEFAULT '[]'::jsonb,
    is_authored           BOOLEAN NOT NULL DEFAULT FALSE,
    created_time          TIMESTAMPTZ,
    last_modified_time    TIMESTAMPTZ,
    source_stream_id      TEXT NOT NULL UNIQUE,
    source_table          TEXT NOT NULL,
    source_provider       TEXT NOT NULL,
    deleted_at_source     TIMESTAMPTZ,
    is_archived           BOOLEAN NOT NULL DEFAULT FALSE,
    metadata              JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_knowledge_document_title ON data_content_document(title);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON data_content_document
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE data_content_conversation (
    id                    TEXT PRIMARY KEY,
    source_connection_id  TEXT REFERENCES elt_source_connections(id),
    conversation_id       TEXT NOT NULL,
    message_id            TEXT NOT NULL,
    role                  TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system')),
    content               TEXT NOT NULL,
    model                 TEXT,
    provider              TEXT NOT NULL,
    tags                  JSONB NOT NULL DEFAULT '[]'::jsonb,
    timestamp             TIMESTAMPTZ NOT NULL,
    source_stream_id      TEXT NOT NULL UNIQUE,
    source_table          TEXT NOT NULL DEFAULT 'stream_virtues_ai_chat',
    source_provider       TEXT NOT NULL DEFAULT 'virtues',
    deleted_at_source     TIMESTAMPTZ,
    is_archived           BOOLEAN NOT NULL DEFAULT FALSE,
    metadata              JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_knowledge_ai_conversation_conversation ON data_content_conversation(conversation_id, timestamp);
CREATE INDEX idx_knowledge_ai_conversation_timestamp    ON data_content_conversation(timestamp DESC);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON data_content_conversation
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE data_content_bookmark (
    id                    TEXT PRIMARY KEY,
    source_connection_id  TEXT REFERENCES elt_source_connections(id),
    url                   TEXT NOT NULL,
    title                 TEXT,
    description           TEXT,
    source_platform       TEXT,
    bookmark_type         TEXT,
    content_type          TEXT,
    author                TEXT,
    tags                  JSONB,
    thumbnail_url         TEXT,
    timestamp             TIMESTAMPTZ NOT NULL,
    source_stream_id      TEXT NOT NULL UNIQUE,
    source_table          TEXT NOT NULL,
    source_provider       TEXT NOT NULL,
    deleted_at_source     TIMESTAMPTZ,
    is_archived           BOOLEAN NOT NULL DEFAULT FALSE,
    metadata              JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_content_bookmark_ts        ON data_content_bookmark(timestamp DESC);
CREATE INDEX idx_content_bookmark_platform  ON data_content_bookmark(source_platform);
CREATE INDEX idx_content_bookmark_type      ON data_content_bookmark(bookmark_type);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON data_content_bookmark
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

-- ---------------------------------------------------------------------------
-- FINANCIAL
-- ---------------------------------------------------------------------------
CREATE TABLE data_financial_account (
    id                    TEXT PRIMARY KEY,
    source_connection_id  TEXT REFERENCES elt_source_connections(id),
    account_name          TEXT NOT NULL,
    account_type          TEXT NOT NULL,
    institution_name      TEXT,
    institution_id        TEXT,
    mask                  TEXT,
    currency              TEXT NOT NULL DEFAULT 'USD',
    current_balance       BIGINT,   -- cents
    available_balance     BIGINT,   -- cents
    credit_limit          BIGINT,   -- cents
    is_active             BOOLEAN NOT NULL DEFAULT TRUE,
    source_stream_id      TEXT NOT NULL UNIQUE,
    source_table          TEXT NOT NULL,
    source_provider       TEXT NOT NULL,
    deleted_at_source     TIMESTAMPTZ,
    is_archived           BOOLEAN NOT NULL DEFAULT FALSE,
    metadata              JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_financial_account_type   ON data_financial_account(account_type);
CREATE INDEX idx_financial_account_active ON data_financial_account(id) WHERE is_active;
CREATE TRIGGER set_updated_at BEFORE UPDATE ON data_financial_account
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE data_financial_asset (
    id                    TEXT PRIMARY KEY,
    source_connection_id  TEXT REFERENCES elt_source_connections(id),
    account_id            TEXT NOT NULL REFERENCES data_financial_account(id) ON DELETE CASCADE,
    asset_type            TEXT NOT NULL,
    symbol                TEXT,
    name                  TEXT,
    quantity              DOUBLE PRECISION,
    cost_basis            BIGINT,    -- cents
    current_value         BIGINT,    -- cents
    currency              TEXT NOT NULL DEFAULT 'USD',
    timestamp             TIMESTAMPTZ NOT NULL,
    source_stream_id      TEXT NOT NULL UNIQUE,
    source_table          TEXT NOT NULL,
    source_provider       TEXT NOT NULL,
    deleted_at_source     TIMESTAMPTZ,
    is_archived           BOOLEAN NOT NULL DEFAULT FALSE,
    metadata              JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_financial_asset_account   ON data_financial_asset(account_id);
CREATE INDEX idx_financial_asset_symbol    ON data_financial_asset(symbol) WHERE symbol IS NOT NULL;
CREATE INDEX idx_financial_asset_timestamp ON data_financial_asset(timestamp DESC);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON data_financial_asset
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE data_financial_liability (
    id                     TEXT PRIMARY KEY,
    source_connection_id   TEXT REFERENCES elt_source_connections(id),
    account_id             TEXT NOT NULL REFERENCES data_financial_account(id) ON DELETE CASCADE,
    liability_type         TEXT NOT NULL,
    principal              BIGINT,     -- cents
    interest_rate          DOUBLE PRECISION,
    minimum_payment        BIGINT,     -- cents
    next_payment_due_date  DATE,
    origination_date       DATE,
    maturity_date          DATE,
    currency               TEXT NOT NULL DEFAULT 'USD',
    timestamp              TIMESTAMPTZ NOT NULL,
    source_stream_id       TEXT NOT NULL UNIQUE,
    source_table           TEXT NOT NULL,
    source_provider        TEXT NOT NULL,
    deleted_at_source      TIMESTAMPTZ,
    is_archived            BOOLEAN NOT NULL DEFAULT FALSE,
    metadata               JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_financial_liability_account      ON data_financial_liability(account_id);
CREATE INDEX idx_financial_liability_timestamp    ON data_financial_liability(timestamp DESC);
CREATE INDEX idx_financial_liability_next_payment ON data_financial_liability(next_payment_due_date) WHERE next_payment_due_date IS NOT NULL;
CREATE TRIGGER set_updated_at BEFORE UPDATE ON data_financial_liability
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE data_financial_transaction (
    id                       TEXT PRIMARY KEY,
    source_connection_id     TEXT REFERENCES elt_source_connections(id),
    account_id               TEXT NOT NULL REFERENCES data_financial_account(id) ON DELETE CASCADE,
    transaction_id           TEXT NOT NULL,
    amount                   BIGINT NOT NULL,    -- cents
    currency                 TEXT NOT NULL DEFAULT 'USD',
    merchant_name            TEXT,
    merchant_category        TEXT,
    description              TEXT,
    category                 JSONB NOT NULL DEFAULT '[]'::jsonb,
    is_pending               BOOLEAN NOT NULL DEFAULT FALSE,
    transaction_type         TEXT,
    payment_channel          TEXT,
    timestamp                TIMESTAMPTZ NOT NULL,
    authorized_timestamp     TIMESTAMPTZ,
    source_stream_id         TEXT NOT NULL UNIQUE,
    source_table             TEXT NOT NULL,
    source_provider          TEXT NOT NULL,
    deleted_at_source        TIMESTAMPTZ,
    is_archived              BOOLEAN NOT NULL DEFAULT FALSE,
    metadata                 JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at               TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_financial_transaction_timestamp ON data_financial_transaction(timestamp DESC);
CREATE INDEX idx_financial_transaction_account   ON data_financial_transaction(account_id);
CREATE INDEX idx_financial_transaction_merchant  ON data_financial_transaction(merchant_name) WHERE merchant_name IS NOT NULL;
CREATE TRIGGER set_updated_at BEFORE UPDATE ON data_financial_transaction
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

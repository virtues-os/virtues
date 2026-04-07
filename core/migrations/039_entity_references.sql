-- 039: Entity references junction table (wiki_entity_refs)
--
-- Replaces scattered FK columns (organizer_person_id, place_id, from_person_id, etc.)
-- on ontology tables with a single junction table. Enables unified "show me everything
-- about Maya" queries across all data sources.
--
-- Naming: sits under wiki_* prefix because entity references are part of the
-- personal knowledge graph — they link resolved wiki entities (people, places,
-- orgs, things) to the data records that mention them. "entity" in the name
-- signals specificity: these are entity-to-data refs, not any wiki-page-to-data ref.
--
-- Also drops dead tables: wiki_connections (never used), wiki_citations (frontend never calls).

-- ─────────────────────────────────────────────────────────────────────────────
-- 1. CREATE wiki_entity_refs
-- ─────────────────────────────────────────────────────────────────────────────

CREATE TABLE wiki_entity_refs (
    id TEXT PRIMARY KEY,
    entity_type TEXT NOT NULL CHECK (entity_type IN ('person', 'place', 'organization', 'thing')),
    entity_id TEXT NOT NULL,
    source_table TEXT NOT NULL,
    source_id TEXT NOT NULL,
    role TEXT,
    confidence REAL NOT NULL DEFAULT 1.0,
    resolved_by TEXT DEFAULT 'system',
    timestamp TEXT,
    metadata TEXT DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_entity_refs_entity ON wiki_entity_refs(entity_id, timestamp DESC);
CREATE INDEX idx_entity_refs_source ON wiki_entity_refs(source_table, source_id);
CREATE INDEX idx_entity_refs_type ON wiki_entity_refs(entity_type, timestamp DESC);
CREATE UNIQUE INDEX idx_entity_refs_unique ON wiki_entity_refs(entity_id, source_table, source_id, role);
CREATE INDEX idx_entity_refs_source_type ON wiki_entity_refs(source_table, source_id, entity_type);

-- ─────────────────────────────────────────────────────────────────────────────
-- 2. RECREATE ontology tables without FK columns
-- ─────────────────────────────────────────────────────────────────────────────
-- SQLite can't reliably DROP COLUMN, so we recreate each table.
-- No data migration needed — all FK columns are NULL.

-- 2a. data_calendar_event (remove: organizer_person_id, attendee_person_ids, place_id)
CREATE TABLE data_calendar_event_new (
    id TEXT PRIMARY KEY,
    source_connection_id TEXT REFERENCES elt_source_connections(id),
    title TEXT NOT NULL,
    description TEXT,
    calendar_name TEXT,
    event_type TEXT,
    status TEXT,
    response_status TEXT,
    organizer_identifier TEXT,
    attendee_identifiers TEXT DEFAULT '[]',
    location_name TEXT,
    conference_url TEXT,
    conference_platform TEXT,
    start_time TEXT NOT NULL,
    end_time TEXT NOT NULL,
    is_all_day INTEGER DEFAULT 0,
    timezone TEXT,
    recurrence_rule TEXT,
    block_type TEXT,
    is_sacred INTEGER DEFAULT 0,
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    external_id TEXT,
    external_url TEXT,
    deleted_at_source TEXT,
    is_archived INTEGER DEFAULT 0,
    metadata TEXT DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO data_calendar_event_new SELECT
    id, source_connection_id, title, description, calendar_name, event_type,
    status, response_status, organizer_identifier, attendee_identifiers,
    location_name, conference_url, conference_platform,
    start_time, end_time, is_all_day, timezone, recurrence_rule,
    block_type, is_sacred, source_stream_id, source_table, source_provider,
    external_id, external_url, deleted_at_source, is_archived,
    metadata, created_at, updated_at
FROM data_calendar_event;

DROP TABLE data_calendar_event;
ALTER TABLE data_calendar_event_new RENAME TO data_calendar_event;

CREATE INDEX idx_calendar_start ON data_calendar_event(start_time DESC);
CREATE INDEX idx_calendar_block_type ON data_calendar_event(block_type) WHERE block_type IS NOT NULL;
CREATE TRIGGER data_calendar_set_updated_at
    AFTER UPDATE ON data_calendar_event
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_calendar_event SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- 2b. data_communication_message (remove: from_person_id, to_person_ids)
CREATE TABLE data_communication_message_new (
    id TEXT PRIMARY KEY,
    source_connection_id TEXT REFERENCES elt_source_connections(id),
    message_id TEXT NOT NULL,
    thread_id TEXT,
    channel TEXT NOT NULL,
    body TEXT,
    from_identifier TEXT NOT NULL,
    from_name TEXT,
    to_identifiers TEXT DEFAULT '[]',
    is_read INTEGER DEFAULT 0,
    is_group_message INTEGER DEFAULT 0,
    reply_to_message_id TEXT,
    has_attachments INTEGER DEFAULT 0,
    timestamp TEXT NOT NULL,
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    deleted_at_source TEXT,
    is_archived INTEGER DEFAULT 0,
    metadata TEXT DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO data_communication_message_new SELECT
    id, source_connection_id, message_id, thread_id, channel, body,
    from_identifier, from_name, to_identifiers,
    is_read, is_group_message, reply_to_message_id, has_attachments,
    timestamp, source_stream_id, source_table, source_provider,
    deleted_at_source, is_archived, metadata, created_at, updated_at
FROM data_communication_message;

DROP TABLE data_communication_message;
ALTER TABLE data_communication_message_new RENAME TO data_communication_message;

CREATE INDEX idx_social_message_timestamp ON data_communication_message(timestamp DESC);
CREATE INDEX idx_social_message_thread ON data_communication_message(thread_id) WHERE thread_id IS NOT NULL;
CREATE INDEX idx_social_message_channel ON data_communication_message(channel);
CREATE TRIGGER data_social_message_set_updated_at
    AFTER UPDATE ON data_communication_message
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_communication_message SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- 2c. data_communication_email (remove: from_person_id, to_person_ids, cc_person_ids, bcc_person_ids)
CREATE TABLE data_communication_email_new (
    id TEXT PRIMARY KEY,
    source_connection_id TEXT REFERENCES elt_source_connections(id),
    message_id TEXT NOT NULL,
    thread_id TEXT,
    subject TEXT,
    body TEXT,
    body_preview TEXT,
    from_email TEXT NOT NULL,
    from_name TEXT,
    to_emails TEXT DEFAULT '[]',
    to_names TEXT DEFAULT '[]',
    cc_emails TEXT DEFAULT '[]',
    bcc_emails TEXT DEFAULT '[]',
    direction TEXT NOT NULL CHECK (direction IN ('sent', 'received')),
    is_read INTEGER DEFAULT 0,
    is_starred INTEGER DEFAULT 0,
    has_attachments INTEGER DEFAULT 0,
    labels TEXT DEFAULT '[]',
    timestamp TEXT NOT NULL,
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    deleted_at_source TEXT,
    is_archived INTEGER DEFAULT 0,
    metadata TEXT DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO data_communication_email_new SELECT
    id, source_connection_id, message_id, thread_id, subject, body, body_preview,
    from_email, from_name, to_emails, to_names, cc_emails, bcc_emails,
    direction, is_read, is_starred, has_attachments, labels,
    timestamp, source_stream_id, source_table, source_provider,
    deleted_at_source, is_archived, metadata, created_at, updated_at
FROM data_communication_email;

DROP TABLE data_communication_email;
ALTER TABLE data_communication_email_new RENAME TO data_communication_email;

CREATE INDEX idx_social_email_timestamp ON data_communication_email(timestamp DESC);
CREATE INDEX idx_social_email_thread ON data_communication_email(thread_id) WHERE thread_id IS NOT NULL;
CREATE TRIGGER data_social_email_set_updated_at
    AFTER UPDATE ON data_communication_email
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_communication_email SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- 2d. data_health_workout (remove: place_id)
CREATE TABLE data_health_workout_new (
    id TEXT PRIMARY KEY,
    source_connection_id TEXT REFERENCES elt_source_connections(id),
    workout_type TEXT NOT NULL,
    start_time TEXT NOT NULL,
    end_time TEXT NOT NULL,
    duration_minutes INTEGER,
    calories_burned INTEGER,
    distance_km REAL,
    avg_heart_rate INTEGER,
    max_heart_rate INTEGER,
    route_geometry TEXT,
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    deleted_at_source TEXT,
    is_archived INTEGER DEFAULT 0,
    metadata TEXT DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO data_health_workout_new SELECT
    id, source_connection_id, workout_type, start_time, end_time,
    duration_minutes, calories_burned, distance_km, avg_heart_rate, max_heart_rate,
    route_geometry, source_stream_id, source_table, source_provider,
    deleted_at_source, is_archived, metadata, created_at, updated_at
FROM data_health_workout;

DROP TABLE data_health_workout;
ALTER TABLE data_health_workout_new RENAME TO data_health_workout;

CREATE INDEX idx_health_workout_start ON data_health_workout(start_time DESC);
CREATE INDEX idx_health_workout_type ON data_health_workout(workout_type);
CREATE TRIGGER data_health_workout_set_updated_at
    AFTER UPDATE ON data_health_workout
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_health_workout SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- 2e. data_location_visit (remove: place_id)
CREATE TABLE data_location_visit_new (
    id TEXT PRIMARY KEY,
    source_connection_id TEXT REFERENCES elt_source_connections(id),
    place_name TEXT,
    latitude REAL NOT NULL,
    longitude REAL NOT NULL,
    arrival_time TEXT NOT NULL,
    departure_time TEXT,
    duration_minutes INTEGER,
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    deleted_at_source TEXT,
    is_archived INTEGER DEFAULT 0,
    metadata TEXT DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO data_location_visit_new SELECT
    id, source_connection_id, place_name, latitude, longitude,
    arrival_time, departure_time, duration_minutes,
    source_stream_id, source_table, source_provider,
    deleted_at_source, is_archived, metadata, created_at, updated_at
FROM data_location_visit;

DROP TABLE data_location_visit;
ALTER TABLE data_location_visit_new RENAME TO data_location_visit;

CREATE INDEX idx_location_visit_arrival ON data_location_visit(arrival_time DESC);
CREATE TRIGGER data_location_visit_set_updated_at
    AFTER UPDATE ON data_location_visit
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_location_visit SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- 2f. data_financial_transaction (remove: place_id)
CREATE TABLE data_financial_transaction_new (
    id TEXT PRIMARY KEY,
    source_connection_id TEXT REFERENCES elt_source_connections(id),
    account_id TEXT NOT NULL REFERENCES data_financial_account(id) ON DELETE CASCADE,
    transaction_id TEXT NOT NULL,
    amount INTEGER NOT NULL,
    currency TEXT DEFAULT 'USD',
    merchant_name TEXT,
    merchant_category TEXT,
    description TEXT,
    category TEXT DEFAULT '[]',
    is_pending INTEGER DEFAULT 0,
    transaction_type TEXT,
    payment_channel TEXT,
    timestamp TEXT NOT NULL,
    authorized_timestamp TEXT,
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    deleted_at_source TEXT,
    is_archived INTEGER DEFAULT 0,
    metadata TEXT DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO data_financial_transaction_new SELECT
    id, source_connection_id, account_id, transaction_id, amount, currency,
    merchant_name, merchant_category, description, category,
    is_pending, transaction_type, payment_channel,
    timestamp, authorized_timestamp,
    source_stream_id, source_table, source_provider,
    deleted_at_source, is_archived, metadata, created_at, updated_at
FROM data_financial_transaction;

DROP TABLE data_financial_transaction;
ALTER TABLE data_financial_transaction_new RENAME TO data_financial_transaction;

CREATE INDEX idx_financial_transaction_timestamp ON data_financial_transaction(timestamp DESC);
CREATE INDEX idx_financial_transaction_account ON data_financial_transaction(account_id);
CREATE INDEX idx_financial_transaction_merchant ON data_financial_transaction(merchant_name) WHERE merchant_name IS NOT NULL;
CREATE TRIGGER data_financial_transaction_set_updated_at
    AFTER UPDATE ON data_financial_transaction
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_financial_transaction SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- 2g. wiki_orgs (remove: primary_place_id)
CREATE TABLE wiki_orgs_new (
    id TEXT PRIMARY KEY,
    canonical_name TEXT NOT NULL,
    organization_type TEXT,
    relationship_type TEXT,
    role_title TEXT,
    start_date TEXT,
    end_date TEXT,
    interaction_count INTEGER DEFAULT 0,
    first_interaction TEXT,
    last_interaction TEXT,
    metadata TEXT DEFAULT '{}',
    content TEXT,
    cover_image TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO wiki_orgs_new SELECT
    id, canonical_name, organization_type,
    relationship_type, role_title, start_date, end_date,
    interaction_count, first_interaction, last_interaction,
    metadata, content, cover_image, created_at, updated_at
FROM wiki_orgs;

DROP TABLE wiki_orgs;
ALTER TABLE wiki_orgs_new RENAME TO wiki_orgs;

CREATE INDEX idx_wiki_orgs_name ON wiki_orgs(canonical_name);
CREATE INDEX idx_wiki_orgs_type ON wiki_orgs(organization_type) WHERE organization_type IS NOT NULL;
CREATE TRIGGER wiki_orgs_set_updated_at
    AFTER UPDATE ON wiki_orgs
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE wiki_orgs SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- ─────────────────────────────────────────────────────────────────────────────
-- 3. DROP dead tables
-- ─────────────────────────────────────────────────────────────────────────────

DROP TABLE IF EXISTS wiki_connections;
DROP TABLE IF EXISTS wiki_citations;

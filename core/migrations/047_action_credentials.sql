-- Migration 047: action_credentials + app_actions function_name/credential_id
--
-- Phase 1 of the actions architecture migration. This is purely additive — no
-- existing tables are dropped, no existing rows are altered (except adding new
-- nullable columns to app_actions). Both the old `elt_source_connections`
-- table and the new `action_credentials` table coexist during the rollout.
--
-- The new actions/ crate binaries (ios_healthkit, ios_location, etc.) will be
-- spawned by a new code path in the /ingest endpoint that looks up actions by
-- `function_name = '{source}_{stream}'` and `credential_id`.

-- ─────────────────────────────────────────────────────────────────────────────
-- 1. action_credentials table
-- ─────────────────────────────────────────────────────────────────────────────
-- Mirrors elt_source_connections but renamed to align with the actions model.
-- Holds OAuth tokens, API keys, and device pairing info.

CREATE TABLE IF NOT EXISTS action_credentials (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,            -- 'ios', 'google', 'plaid', 'strava', etc.
    name TEXT NOT NULL,
    auth_type TEXT NOT NULL CHECK (auth_type IN ('oauth2', 'device', 'api_key', 'none', 'plaid')),

    -- OAuth tokens (encrypted at rest)
    access_token TEXT,
    refresh_token TEXT,
    token_expires_at TEXT,

    -- Device pairing
    device_id TEXT,
    device_token TEXT,
    device_info TEXT,                  -- JSON
    last_seen_at TEXT,

    -- State
    is_active INTEGER DEFAULT 1,
    error_message TEXT,
    metadata TEXT DEFAULT '{}',        -- JSON

    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_action_credentials_provider ON action_credentials(provider);
CREATE INDEX IF NOT EXISTS idx_action_credentials_device_id ON action_credentials(device_id) WHERE device_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_action_credentials_device_token ON action_credentials(device_token) WHERE device_token IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS action_credentials_set_updated_at
    AFTER UPDATE ON action_credentials
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE action_credentials SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- ─────────────────────────────────────────────────────────────────────────────
-- 2. Migrate existing iOS device credentials
-- ─────────────────────────────────────────────────────────────────────────────
-- Copy iOS device pairings from elt_source_connections to action_credentials.
-- Uses the same id so existing references stay valid. Only iOS for this phase —
-- other providers (Google, Plaid, Strava, etc.) still use elt_source_connections
-- via the old code path until they're migrated.

INSERT OR IGNORE INTO action_credentials (
    id, provider, name, auth_type,
    access_token, refresh_token, token_expires_at,
    device_id, device_token, device_info, last_seen_at,
    is_active, error_message, metadata,
    created_at, updated_at
)
SELECT
    id, source, name, auth_type,
    access_token, refresh_token, token_expires_at,
    device_id, device_token, device_info, last_seen_at,
    is_active, error_message, COALESCE(metadata, '{}'),
    created_at, updated_at
FROM elt_source_connections
WHERE source = 'ios' AND auth_type = 'device' AND is_active = 1;

-- ─────────────────────────────────────────────────────────────────────────────
-- 3. Add function_name and credential_id to app_actions
-- ─────────────────────────────────────────────────────────────────────────────
-- Additive only. Existing actions keep working via the old action_type/config
-- path. New actions populate function_name and credential_id.

ALTER TABLE app_actions ADD COLUMN function_name TEXT;
ALTER TABLE app_actions ADD COLUMN credential_id TEXT REFERENCES action_credentials(id);

CREATE INDEX IF NOT EXISTS idx_app_actions_function_name ON app_actions(function_name) WHERE function_name IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_app_actions_credential_id ON app_actions(credential_id) WHERE credential_id IS NOT NULL;

-- ─────────────────────────────────────────────────────────────────────────────
-- 4. Seed iOS action rows for each migrated credential
-- ─────────────────────────────────────────────────────────────────────────────
-- For each iOS credential, create six action rows (one per stream) so the
-- /ingest endpoint can find them by function_name + credential_id.

INSERT OR IGNORE INTO app_actions (
    id, action_type, owner, name, enabled, config, function_name, credential_id, created_at, updated_at
)
SELECT
    'action_ios_healthkit_' || ac.id,
    'sync', 'system', 'iOS HealthKit', 1,
    '{}',
    'ios_healthkit', ac.id,
    datetime('now'), datetime('now')
FROM action_credentials ac WHERE ac.provider = 'ios';

INSERT OR IGNORE INTO app_actions (
    id, action_type, owner, name, enabled, config, function_name, credential_id, created_at, updated_at
)
SELECT
    'action_ios_location_' || ac.id,
    'sync', 'system', 'iOS Location', 1,
    '{}',
    'ios_location', ac.id,
    datetime('now'), datetime('now')
FROM action_credentials ac WHERE ac.provider = 'ios';

INSERT OR IGNORE INTO app_actions (
    id, action_type, owner, name, enabled, config, function_name, credential_id, created_at, updated_at
)
SELECT
    'action_ios_microphone_' || ac.id,
    'sync', 'system', 'iOS Microphone', 1,
    '{}',
    'ios_microphone', ac.id,
    datetime('now'), datetime('now')
FROM action_credentials ac WHERE ac.provider = 'ios';

INSERT OR IGNORE INTO app_actions (
    id, action_type, owner, name, enabled, config, function_name, credential_id, created_at, updated_at
)
SELECT
    'action_ios_contacts_' || ac.id,
    'sync', 'system', 'iOS Contacts', 1,
    '{}',
    'ios_contacts', ac.id,
    datetime('now'), datetime('now')
FROM action_credentials ac WHERE ac.provider = 'ios';

INSERT OR IGNORE INTO app_actions (
    id, action_type, owner, name, enabled, config, function_name, credential_id, created_at, updated_at
)
SELECT
    'action_ios_eventkit_' || ac.id,
    'sync', 'system', 'iOS EventKit', 1,
    '{}',
    'ios_eventkit', ac.id,
    datetime('now'), datetime('now')
FROM action_credentials ac WHERE ac.provider = 'ios';

INSERT OR IGNORE INTO app_actions (
    id, action_type, owner, name, enabled, config, function_name, credential_id, created_at, updated_at
)
SELECT
    'action_ios_financekit_' || ac.id,
    'sync', 'system', 'iOS FinanceKit', 1,
    '{}',
    'ios_financekit', ac.id,
    datetime('now'), datetime('now')
FROM action_credentials ac WHERE ac.provider = 'ios';

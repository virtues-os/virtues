-- Migration 055: credentials Vault + oauth_state + retarget app_actions FK
--
-- This is the foundation of the credentials/connectors charter (CREDENTIALS.md).
-- One unified Vault for all secrets, replacing the typed-column action_credentials
-- table. Per-auth-type secret shapes live inside an encrypted JSON column whose
-- shape is declared by the connector manifest (no schema column per provider).
--
-- This migration is ADDITIVE in the sense that the legacy `action_credentials`
-- table is left in place — to be dropped in migration 056 after a burn-in
-- window. The post-migration Rust hook (core/src/credentials/migrate.rs)
-- re-encrypts existing iOS device tokens into the new shape and computes
-- HMAC lookup hashes; that step needs the master encryption key and so cannot
-- run in pure SQL.
--
-- The FK on `app_actions.credential_id` is retargeted from `action_credentials`
-- to `credentials` here, by table-rebuild. Existing iOS credential ids are
-- preserved across the copy, so existing FK references stay valid.
--
-- Defer FK checks until COMMIT. Required because we rebuild app_actions, which
-- is referenced by app_action_runs.action_id via FK. Same pattern as
-- migration 050's app_actions rebuild.
PRAGMA defer_foreign_keys = 1;

-- ─────────────────────────────────────────────────────────────────────────────
-- 1. credentials — the Vault
-- ─────────────────────────────────────────────────────────────────────────────

CREATE TABLE credentials (
    id                  TEXT PRIMARY KEY,

    -- Identity
    source_id           TEXT NOT NULL,                          -- 'ios', 'google', 'plaid', 'mcp:github', 'user:bank_chase', ...
    name                TEXT NOT NULL,                          -- user-facing label

    -- Status state machine (replaces is_active + implicit pending/revoked semantics)
    status              TEXT NOT NULL
                            CHECK (status IN ('pending', 'active', 'revoked', 'reauth_required', 'error')),
    status_reason       TEXT,                                   -- 'user_revoked', 'token_expired', 'item_login_required', ...

    -- Encrypted secret payload. Shape declared by the connector manifest:
    --   self_issued_bearer  → {"token": "..."}
    --   oauth2_code         → {"access_token": "...", "refresh_token": "...", "expires_at": "..."}
    --   redirect_chain      → connector-specific (e.g. {"access_token": "...", "item_id": "..."} for Plaid)
    --   api_key             → {"token": "..."}
    secrets_ciphertext  TEXT NOT NULL,

    -- HMAC of the plaintext bearer for O(1) webhook lookup. Only set for
    -- connectors with auth.kind = 'self_issued_bearer' (iOS, Mac).
    secret_lookup_hash  TEXT,

    -- OAuth scope tracking; nullable for non-oauth2 connectors.
    scopes              TEXT,                                   -- JSON array

    -- Refresh scheduling. The credential_refresh cron scans
    -- WHERE next_refresh_at < now() AND status = 'active'.
    expires_at          TEXT,
    next_refresh_at     TEXT,

    -- Plaintext non-secret context. Shape declared by the connector manifest.
    -- Examples: {"device_id": "...", "device_model": "...", "os_version": "..."}
    --           {"email": "adam@example.com", "name": "Adam"}
    --           {"item_id": "..."}
    metadata            TEXT NOT NULL DEFAULT '{}',

    last_seen_at        TEXT,
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_credentials_source ON credentials(source_id);
CREATE INDEX idx_credentials_status    ON credentials(status);
CREATE INDEX idx_credentials_next_refresh ON credentials(next_refresh_at)
    WHERE next_refresh_at IS NOT NULL AND status = 'active';
CREATE UNIQUE INDEX idx_credentials_lookup ON credentials(secret_lookup_hash)
    WHERE secret_lookup_hash IS NOT NULL;

CREATE TRIGGER credentials_set_updated_at
    AFTER UPDATE ON credentials
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE credentials SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- ─────────────────────────────────────────────────────────────────────────────
-- (no oauth_state table — CSRF state is a self-contained signed token)
-- ─────────────────────────────────────────────────────────────────────────────
-- (No oauth_state table.) OAuth CSRF state is a self-contained signed token:
-- HMAC-SHA256 over { source_id, existing_credential_id?, expires_at, nonce },
-- keyed by a pepper derived from VIRTUES_ENCRYPTION_KEY with the domain
-- separator "oauth.state.v1". The /oauth/callback handler verifies the
-- signature and decodes the payload — no DB row, no in-flight state to
-- garbage-collect. See crates/virtues-crypto (sign_oauth_state / verify_oauth_state).

-- ─────────────────────────────────────────────────────────────────────────────
-- 2. Copy iOS rows from action_credentials into credentials
-- ─────────────────────────────────────────────────────────────────────────────
-- Placeholder secrets_ciphertext is rewritten by the post-migration Rust hook
-- (which has the encryption key in memory). The hook also computes
-- secret_lookup_hash from the decrypted plaintext and merges device_info
-- into metadata.
--
-- We copy rows in all states (pending, active, revoked) so the FK on
-- app_actions.credential_id remains valid for every historical reference.

INSERT INTO credentials (
    id, source_id, name, status, status_reason,
    secrets_ciphertext, secret_lookup_hash,
    scopes, expires_at, next_refresh_at,
    metadata, last_seen_at, created_at, updated_at
)
SELECT
    id,
    provider,                                    -- legacy provider column → source_id 'ios'
    name,
    CASE
        WHEN is_active = 1 AND device_id IS NOT NULL THEN 'active'
        WHEN is_active = 0 AND device_id IS NOT NULL THEN 'revoked'
        ELSE 'pending'
    END,
    error_message,
    '__PENDING_REENCRYPT__',                      -- hook fills this
    NULL,                                         -- hook computes from decrypted token
    NULL,                                         -- iOS has no scopes
    NULL,                                         -- iOS doesn't expire
    NULL,                                         -- iOS doesn't refresh
    '{}',                                         -- hook merges device_info + metadata
    last_seen_at,
    created_at,
    updated_at
FROM action_credentials
WHERE auth_type = 'device';

-- ─────────────────────────────────────────────────────────────────────────────
-- 3. Rebuild app_actions to retarget credential_id FK at credentials(id)
-- ─────────────────────────────────────────────────────────────────────────────
-- SQLite enforces FKs (PRAGMA foreign_keys = ON in connect_options), so
-- changing the FK target requires a table rebuild. The PRAGMA defer_foreign_keys
-- at the top of this migration moves the check to COMMIT, by which point the
-- new app_actions table exists with the correct FK target.
--
-- Defensive cleanup: null out any app_actions.credential_id that doesn't
-- resolve in the new credentials table. Such rows shouldn't exist with FKs
-- enforced, but a hand-edited dev DB or partial earlier migration could
-- leave dangling refs that the deferred FK check would surface at COMMIT.

UPDATE app_actions
   SET credential_id = NULL
 WHERE credential_id IS NOT NULL
   AND credential_id NOT IN (SELECT id FROM credentials);

CREATE TABLE app_actions_new (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    owner           TEXT NOT NULL DEFAULT 'user'
                        CHECK (owner IN ('system', 'user')),
    agent           TEXT,
    cron_schedule   TEXT,
    enabled         INTEGER NOT NULL DEFAULT 1,
    config          TEXT NOT NULL DEFAULT '{}',
    condition       TEXT,
    triggers        TEXT NOT NULL DEFAULT '["cron"]',
    memory          TEXT,
    function_name   TEXT,
    credential_id   TEXT REFERENCES credentials(id),    -- retargeted from action_credentials
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (function_name IS NOT NULL OR agent IS NOT NULL)
);

INSERT INTO app_actions_new (
    id, name, owner, agent, cron_schedule, enabled, config,
    condition, triggers, memory, function_name, credential_id,
    created_at, updated_at
)
SELECT
    id, name, owner, agent, cron_schedule, enabled, config,
    condition, triggers, memory, function_name, credential_id,
    created_at, updated_at
FROM app_actions;

DROP TABLE app_actions;
ALTER TABLE app_actions_new RENAME TO app_actions;

CREATE INDEX idx_app_actions_enabled       ON app_actions(enabled);
CREATE INDEX idx_app_actions_function_name ON app_actions(function_name) WHERE function_name IS NOT NULL;
CREATE INDEX idx_app_actions_credential_id ON app_actions(credential_id) WHERE credential_id IS NOT NULL;

CREATE TRIGGER app_actions_set_updated_at
    AFTER UPDATE ON app_actions
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_actions SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- ─────────────────────────────────────────────────────────────────────────────
-- 4. Add credential_id column to app_mcp_servers
-- ─────────────────────────────────────────────────────────────────────────────
-- Populated by a later PR that migrates plaintext auth_token values into the
-- Vault as api_key credentials. The auth_token column stays for now and is
-- dropped in migration 056 once that migration has run.

ALTER TABLE app_mcp_servers ADD COLUMN credential_id TEXT REFERENCES credentials(id);
CREATE INDEX idx_app_mcp_servers_credential_id ON app_mcp_servers(credential_id) WHERE credential_id IS NOT NULL;

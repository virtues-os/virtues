-- 0004 — Credentials, actions, MCP.
--
-- `credentials` is the unified store for everything secret the box holds:
--   - OAuth tokens for source connections (Google, Notion, Plaid, …) —
--     these have `device_id = NULL` because OAuth providers aren't "devices".
--   - Self-issued bearer tokens for paired apps/sensors (iOS, Mac, ESP32) —
--     these have `device_id` set, linking back to the canonical `app_device`
--     row that owns this credential.
--
-- Revocation flows from `app_device.revoked_at` outward: when a device is
-- revoked, its credential row gets `status = 'revoked'` and the `wg0` peer
-- (if any) is evicted in the same transaction by `virtues-wg`. For OAuth
-- credentials (no device), revocation is a direct status update.

-- ---------------------------------------------------------------------------
-- Unified credentials (encrypted-at-rest)
-- ---------------------------------------------------------------------------
CREATE TABLE credentials (
    id                  TEXT PRIMARY KEY,
    source_id           TEXT NOT NULL,
    name                TEXT NOT NULL,
    device_id           TEXT REFERENCES app_device(id) ON DELETE CASCADE,         -- NULL for OAuth; set for self-issued-bearer
    status              TEXT NOT NULL
                            CHECK (status IN ('pending', 'active', 'revoked', 'reauth_required', 'error')),
    status_reason       TEXT,
    secrets_ciphertext  TEXT NOT NULL,
    secret_lookup_hash  TEXT,
    scopes              JSONB,
    expires_at          TIMESTAMPTZ,
    next_refresh_at     TIMESTAMPTZ,
    metadata            JSONB NOT NULL DEFAULT '{}'::jsonb,
    last_seen_at        TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_credentials_source       ON credentials(source_id);
CREATE INDEX idx_credentials_status       ON credentials(status);
CREATE INDEX idx_credentials_device       ON credentials(device_id) WHERE device_id IS NOT NULL;
CREATE INDEX idx_credentials_next_refresh ON credentials(next_refresh_at)
    WHERE next_refresh_at IS NOT NULL AND status = 'active';
CREATE UNIQUE INDEX idx_credentials_lookup ON credentials(secret_lookup_hash)
    WHERE secret_lookup_hash IS NOT NULL;
CREATE TRIGGER set_updated_at BEFORE UPDATE ON credentials
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

-- ---------------------------------------------------------------------------
-- Actions and runs
-- ---------------------------------------------------------------------------
CREATE TABLE app_actions (
    id             TEXT PRIMARY KEY,
    name           TEXT NOT NULL,
    owner          TEXT NOT NULL DEFAULT 'user' CHECK (owner IN ('system', 'user')),
    agent          TEXT,
    cron_schedule  TEXT,
    enabled        BOOLEAN NOT NULL DEFAULT TRUE,
    config         JSONB   NOT NULL DEFAULT '{}'::jsonb,
    condition      TEXT,
    triggers       JSONB   NOT NULL DEFAULT '["cron"]'::jsonb,
    memory         JSONB,
    credential_id  TEXT REFERENCES credentials(id),
    runtime        TEXT NOT NULL DEFAULT 'function'
                       CHECK (runtime IN ('function', 'service', 'view')),
    command        TEXT,
    dir            TEXT NOT NULL DEFAULT '',
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (runtime = 'view' OR command IS NOT NULL OR agent IS NOT NULL)
);
CREATE INDEX idx_app_actions_enabled       ON app_actions(enabled);
CREATE INDEX idx_app_actions_credential_id ON app_actions(credential_id) WHERE credential_id IS NOT NULL;
CREATE TRIGGER set_updated_at BEFORE UPDATE ON app_actions
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE app_action_runs (
    id                  TEXT PRIMARY KEY,
    -- ON DELETE SET NULL — runs are an audit trail. If the underlying
    -- action is deleted, keep the run history but null out the FK.
    action_id           TEXT REFERENCES app_actions(id) ON DELETE SET NULL,
    status              TEXT NOT NULL DEFAULT 'running'
                            CHECK (status IN ('running', 'success', 'error', 'cancelled', 'skipped')),
    started_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at        TIMESTAMPTZ,
    records_processed   BIGINT NOT NULL DEFAULT 0,
    error               TEXT,
    trigger             TEXT NOT NULL DEFAULT 'cron'
                            CHECK (trigger IN ('cron', 'manual', 'tool', 'api', 'webhook')),
    parent_run_id       TEXT REFERENCES app_action_runs(id),
    transform_stage     TEXT,
    result_summary      TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_app_action_runs_action ON app_action_runs(action_id, created_at DESC);
CREATE INDEX idx_app_action_runs_status ON app_action_runs(status) WHERE status = 'running';
CREATE INDEX idx_app_action_runs_parent ON app_action_runs(parent_run_id) WHERE parent_run_id IS NOT NULL;

-- ---------------------------------------------------------------------------
-- MCP servers and tools
-- ---------------------------------------------------------------------------
CREATE TABLE app_mcp_servers (
    id                 TEXT PRIMARY KEY,
    name               TEXT NOT NULL,
    url                TEXT NOT NULL,
    description        TEXT,
    auth_token         TEXT,
    enabled            BOOLEAN NOT NULL DEFAULT TRUE,
    status             TEXT NOT NULL DEFAULT 'disconnected'
                           CHECK (status IN ('disconnected', 'connecting', 'connected', 'error')),
    last_error         TEXT,
    last_connected_at  TIMESTAMPTZ,
    -- ON DELETE SET NULL — if the credential is revoked, leave the server
    -- row but null its credential link so the UI can show "needs reconnect".
    credential_id      TEXT REFERENCES credentials(id) ON DELETE SET NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_app_mcp_servers_credential_id ON app_mcp_servers(credential_id) WHERE credential_id IS NOT NULL;
CREATE TRIGGER set_updated_at BEFORE UPDATE ON app_mcp_servers
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE app_mcp_tools (
    id            TEXT PRIMARY KEY,
    server_id     TEXT NOT NULL REFERENCES app_mcp_servers(id) ON DELETE CASCADE,
    server_name   TEXT NOT NULL,
    tool_name     TEXT NOT NULL,
    description   TEXT,
    input_schema  JSONB,
    enabled       BOOLEAN NOT NULL DEFAULT TRUE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_app_mcp_tools_server ON app_mcp_tools(server_id);

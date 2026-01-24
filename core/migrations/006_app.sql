-- App: Assistant config, Chat, Auth, Models, Agents, Tools, Metering (SQLite)

--------------------------------------------------------------------------------
-- ASSISTANT PROFILE (singleton)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app_assistant_profile (
    id TEXT PRIMARY KEY DEFAULT '00000000-0000-0000-0000-000000000001',
    assistant_name TEXT DEFAULT 'Ari',
    default_agent_id TEXT DEFAULT 'agent',
    default_model_id TEXT DEFAULT 'anthropic/claude-sonnet-4-20250514',
    background_model_id TEXT DEFAULT 'cerebras/llama-3.3-70b',  -- For cheap tasks: titles, summaries
    enabled_tools TEXT DEFAULT '{"web_search": true, "virtues_query_ontology": true, "virtues_semantic_search": true}',  -- JSON
    ui_preferences TEXT DEFAULT '{"contextIndicator": {"alwaysVisible": false, "showThreshold": 70}}',  -- JSON
    embedding_model_id TEXT DEFAULT 'nomic-embed-text',
    ollama_endpoint TEXT DEFAULT 'http://localhost:11434',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    CONSTRAINT assistant_profile_singleton CHECK (id = '00000000-0000-0000-0000-000000000001')
);

CREATE TRIGGER IF NOT EXISTS app_assistant_profile_set_updated_at
    AFTER UPDATE ON app_assistant_profile
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_assistant_profile SET updated_at = datetime('now') WHERE id = NEW.id;
END;

INSERT OR IGNORE INTO app_assistant_profile (id)
VALUES ('00000000-0000-0000-0000-000000000001');

--------------------------------------------------------------------------------
-- CHAT SESSIONS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app_chat_sessions (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    message_count INTEGER NOT NULL DEFAULT 0,
    trace TEXT,  -- JSON
    workspace_id TEXT,  -- Which workspace this chat belongs to (for view filtering)
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_chat_sessions_updated
    ON app_chat_sessions(updated_at DESC);

CREATE TRIGGER IF NOT EXISTS app_chat_sessions_set_updated_at
    AFTER UPDATE ON app_chat_sessions
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_chat_sessions SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- MODELS: REMOVED - Models are now read from virtues-registry crate
-- See: packages/virtues-registry/src/models.rs
--------------------------------------------------------------------------------

--------------------------------------------------------------------------------
-- AGENTS: REMOVED - Agents are now read from virtues-registry crate
-- See: packages/virtues-registry/src/agents.rs
--------------------------------------------------------------------------------

--------------------------------------------------------------------------------
-- MCP TOOLS
-- Only stores MCP tools discovered from connected servers.
-- Built-in tools (web_search, query_ontology, semantic_search) are in registry.
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app_mcp_tools (
    id TEXT PRIMARY KEY,
    server_name TEXT NOT NULL,              -- MCP server name (e.g., "my-github-mcp")
    server_url TEXT NOT NULL,               -- MCP server URL
    tool_name TEXT NOT NULL,                -- Tool name from MCP server
    description TEXT,
    input_schema TEXT,                      -- JSON schema for tool parameters
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(server_name, tool_name)
);

CREATE INDEX IF NOT EXISTS idx_mcp_tools_server
    ON app_mcp_tools(server_name);
CREATE INDEX IF NOT EXISTS idx_mcp_tools_enabled
    ON app_mcp_tools(id) WHERE enabled = 1;

CREATE TRIGGER IF NOT EXISTS app_mcp_tools_set_updated_at
    AFTER UPDATE ON app_mcp_tools
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_mcp_tools SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- AUTH: USER
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app_auth_user (
    id TEXT PRIMARY KEY,
    email TEXT UNIQUE NOT NULL,
    email_verified TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TRIGGER IF NOT EXISTS app_auth_user_set_updated_at
    AFTER UPDATE ON app_auth_user
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_auth_user SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- AUTH: SESSION
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app_auth_session (
    id TEXT PRIMARY KEY,
    session_token TEXT UNIQUE NOT NULL,
    user_id TEXT NOT NULL REFERENCES app_auth_user(id) ON DELETE CASCADE,
    expires TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_auth_session_user
    ON app_auth_session(user_id);
CREATE INDEX IF NOT EXISTS idx_auth_session_expires
    ON app_auth_session(expires);

--------------------------------------------------------------------------------
-- AUTH: VERIFICATION TOKEN
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app_auth_verification_token (
    identifier TEXT NOT NULL,
    token TEXT NOT NULL,
    expires TEXT NOT NULL,
    PRIMARY KEY (identifier, token)
);

CREATE INDEX IF NOT EXISTS idx_auth_verification_expires
    ON app_auth_verification_token(expires);

--------------------------------------------------------------------------------
-- API USAGE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app_api_usage (
    id TEXT PRIMARY KEY,
    endpoint TEXT NOT NULL,
    day_bucket TEXT NOT NULL,
    request_count INTEGER NOT NULL DEFAULT 0 CHECK (request_count >= 0),
    token_count INTEGER NOT NULL DEFAULT 0 CHECK (token_count >= 0),
    input_tokens INTEGER NOT NULL DEFAULT 0 CHECK (input_tokens >= 0),
    output_tokens INTEGER NOT NULL DEFAULT 0 CHECK (output_tokens >= 0),
    estimated_cost_usd REAL NOT NULL DEFAULT 0 CHECK (estimated_cost_usd >= 0),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(endpoint, day_bucket)
);

CREATE INDEX IF NOT EXISTS idx_api_usage_day
    ON app_api_usage(day_bucket DESC);
CREATE INDEX IF NOT EXISTS idx_api_usage_endpoint_day
    ON app_api_usage(endpoint, day_bucket);

CREATE TRIGGER IF NOT EXISTS app_api_usage_set_updated_at
    AFTER UPDATE ON app_api_usage
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_api_usage SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- LLM USAGE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app_llm_usage (
    id TEXT PRIMARY KEY,
    month TEXT UNIQUE NOT NULL,
    tokens_used INTEGER DEFAULT 0,
    cost_cents INTEGER DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_llm_usage_month
    ON app_llm_usage(month);

CREATE TRIGGER IF NOT EXISTS app_llm_usage_set_updated_at
    AFTER UPDATE ON app_llm_usage
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_llm_usage SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- LLM REQUESTS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app_llm_requests (
    id TEXT PRIMARY KEY,
    model TEXT NOT NULL,
    input_tokens INTEGER,
    output_tokens INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_llm_requests_created
    ON app_llm_requests(created_at DESC);

--------------------------------------------------------------------------------
-- USAGE LIMITS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app_usage_limits (
    service TEXT PRIMARY KEY,
    monthly_limit INTEGER NOT NULL,
    unit TEXT NOT NULL DEFAULT 'requests',
    limit_type TEXT NOT NULL DEFAULT 'hard' CHECK (limit_type IN ('hard', 'soft')),
    enabled INTEGER DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_usage_limits_enabled
    ON app_usage_limits(service) WHERE enabled = 1;

CREATE TRIGGER IF NOT EXISTS app_usage_limits_set_updated_at
    AFTER UPDATE ON app_usage_limits
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_usage_limits SET updated_at = datetime('now') WHERE service = NEW.service;
END;

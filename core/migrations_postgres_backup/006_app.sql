-- App: Assistant config, Chat, Auth, Models, Agents, Tools, Metering

--------------------------------------------------------------------------------
-- ASSISTANT PROFILE (singleton)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app.assistant_profile (
    id UUID PRIMARY KEY DEFAULT '00000000-0000-0000-0000-000000000001'::uuid,
    assistant_name TEXT DEFAULT 'Ari',
    default_agent_id TEXT DEFAULT 'auto',
    default_model_id TEXT DEFAULT 'google/gemini-3-flash',
    enabled_tools JSONB DEFAULT '{"queryLocationMap": true}',
    ui_preferences JSONB DEFAULT '{}',
    embedding_model_id TEXT DEFAULT 'nomic-embed-text',
    ollama_endpoint TEXT DEFAULT 'http://localhost:11434',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT assistant_profile_singleton CHECK (id = '00000000-0000-0000-0000-000000000001'::uuid)
);

DROP TRIGGER IF EXISTS app_assistant_profile_set_updated_at ON app.assistant_profile;
CREATE TRIGGER app_assistant_profile_set_updated_at
    BEFORE UPDATE ON app.assistant_profile
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

INSERT INTO app.assistant_profile (id)
VALUES ('00000000-0000-0000-0000-000000000001'::uuid)
ON CONFLICT (id) DO NOTHING;

--------------------------------------------------------------------------------
-- CHAT SESSIONS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app.chat_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    messages JSONB NOT NULL DEFAULT '[]'::jsonb,
    message_count INTEGER NOT NULL DEFAULT 0,
    trace JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_chat_sessions_updated
    ON app.chat_sessions(updated_at DESC);

DROP TRIGGER IF EXISTS app_chat_sessions_set_updated_at ON app.chat_sessions;
CREATE TRIGGER app_chat_sessions_set_updated_at
    BEFORE UPDATE ON app.chat_sessions
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- MODELS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app.models (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID,
    model_id TEXT NOT NULL,
    display_name TEXT NOT NULL,
    provider TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,
    sort_order INTEGER NOT NULL DEFAULT 0,
    context_window INTEGER,
    max_output_tokens INTEGER,
    supports_tools BOOLEAN DEFAULT true,
    is_default BOOLEAN DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_models_user_model
    ON app.models(user_id, model_id) WHERE user_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_models_system_model
    ON app.models(model_id) WHERE user_id IS NULL;
CREATE INDEX IF NOT EXISTS idx_models_enabled
    ON app.models(id) WHERE enabled = true;

DROP TRIGGER IF EXISTS app_models_set_updated_at ON app.models;
CREATE TRIGGER app_models_set_updated_at
    BEFORE UPDATE ON app.models
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- AGENTS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app.agents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID,
    agent_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    color TEXT,
    icon TEXT,
    enabled BOOLEAN NOT NULL DEFAULT true,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_agents_user_agent
    ON app.agents(user_id, agent_id) WHERE user_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_agents_system_agent
    ON app.agents(agent_id) WHERE user_id IS NULL;
CREATE INDEX IF NOT EXISTS idx_agents_enabled
    ON app.agents(id) WHERE enabled = true;

DROP TRIGGER IF EXISTS app_agents_set_updated_at ON app.agents;
CREATE TRIGGER app_agents_set_updated_at
    BEFORE UPDATE ON app.agents
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- TOOLS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app.tools (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    tool_type TEXT NOT NULL CHECK (tool_type IN ('mcp', 'virtues')),
    category TEXT,
    icon TEXT,
    default_params JSONB,
    display_order INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_tools_category
    ON app.tools(category) WHERE category IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_tools_type
    ON app.tools(tool_type);

DROP TRIGGER IF EXISTS app_tools_set_updated_at ON app.tools;
CREATE TRIGGER app_tools_set_updated_at
    BEFORE UPDATE ON app.tools
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- AUTH: USER
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app.auth_user (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT UNIQUE NOT NULL,
    email_verified TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

DROP TRIGGER IF EXISTS app_auth_user_set_updated_at ON app.auth_user;
CREATE TRIGGER app_auth_user_set_updated_at
    BEFORE UPDATE ON app.auth_user
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- AUTH: SESSION
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app.auth_session (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_token TEXT UNIQUE NOT NULL,
    user_id UUID NOT NULL REFERENCES app.auth_user(id) ON DELETE CASCADE,
    expires TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_auth_session_user
    ON app.auth_session(user_id);
CREATE INDEX IF NOT EXISTS idx_auth_session_expires
    ON app.auth_session(expires);

--------------------------------------------------------------------------------
-- AUTH: VERIFICATION TOKEN
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app.auth_verification_token (
    identifier TEXT NOT NULL,
    token TEXT NOT NULL,
    expires TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (identifier, token)
);

CREATE INDEX IF NOT EXISTS idx_auth_verification_expires
    ON app.auth_verification_token(expires);

--------------------------------------------------------------------------------
-- API USAGE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app.api_usage (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    endpoint TEXT NOT NULL,
    day_bucket DATE NOT NULL,
    request_count INTEGER NOT NULL DEFAULT 0 CHECK (request_count >= 0),
    token_count INTEGER NOT NULL DEFAULT 0 CHECK (token_count >= 0),
    input_tokens INTEGER NOT NULL DEFAULT 0 CHECK (input_tokens >= 0),
    output_tokens INTEGER NOT NULL DEFAULT 0 CHECK (output_tokens >= 0),
    estimated_cost_usd DECIMAL(10, 4) NOT NULL DEFAULT 0 CHECK (estimated_cost_usd >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(endpoint, day_bucket)
);

CREATE INDEX IF NOT EXISTS idx_api_usage_day
    ON app.api_usage(day_bucket DESC);
CREATE INDEX IF NOT EXISTS idx_api_usage_endpoint_day
    ON app.api_usage(endpoint, day_bucket);

DROP TRIGGER IF EXISTS app_api_usage_set_updated_at ON app.api_usage;
CREATE TRIGGER app_api_usage_set_updated_at
    BEFORE UPDATE ON app.api_usage
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- LLM USAGE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app.llm_usage (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    month DATE UNIQUE NOT NULL,
    tokens_used BIGINT DEFAULT 0,
    cost_cents INTEGER DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_llm_usage_month
    ON app.llm_usage(month);

DROP TRIGGER IF EXISTS app_llm_usage_set_updated_at ON app.llm_usage;
CREATE TRIGGER app_llm_usage_set_updated_at
    BEFORE UPDATE ON app.llm_usage
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

--------------------------------------------------------------------------------
-- LLM REQUESTS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app.llm_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    model TEXT NOT NULL,
    input_tokens INTEGER,
    output_tokens INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_llm_requests_created
    ON app.llm_requests(created_at DESC);

--------------------------------------------------------------------------------
-- USAGE LIMITS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app.usage_limits (
    service TEXT PRIMARY KEY,
    monthly_limit BIGINT NOT NULL,
    unit TEXT NOT NULL DEFAULT 'requests',
    limit_type TEXT NOT NULL DEFAULT 'hard' CHECK (limit_type IN ('hard', 'soft')),
    enabled BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_usage_limits_enabled
    ON app.usage_limits(service) WHERE enabled = TRUE;

DROP TRIGGER IF EXISTS app_usage_limits_set_updated_at ON app.usage_limits;
CREATE TRIGGER app_usage_limits_set_updated_at
    BEFORE UPDATE ON app.usage_limits
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

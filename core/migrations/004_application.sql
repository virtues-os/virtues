-- Application: App config, chat sessions, auth, LLM metering
-- Consolidates: 005, 019, 021, 022

--------------------------------------------------------------------------------
-- ASSISTANT PROFILE (singleton)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app.assistant_profile (
    id UUID PRIMARY KEY DEFAULT '00000000-0000-0000-0000-000000000001'::uuid,
    assistant_name TEXT DEFAULT 'Ari',
    default_agent_id TEXT DEFAULT 'auto',
    default_model_id TEXT DEFAULT 'google/gemini-3-pro-preview',
    enabled_tools JSONB DEFAULT '{"queryLocationMap": true}',
    ui_preferences JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    CONSTRAINT assistant_profile_singleton CHECK (id = '00000000-0000-0000-0000-000000000001'::uuid)
);

INSERT INTO app.assistant_profile (id)
VALUES ('00000000-0000-0000-0000-000000000001'::uuid)
ON CONFLICT (id) DO NOTHING;

CREATE OR REPLACE FUNCTION update_assistant_profile_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS assistant_profile_updated_at ON app.assistant_profile;
CREATE TRIGGER assistant_profile_updated_at
    BEFORE UPDATE ON app.assistant_profile
    FOR EACH ROW
    EXECUTE FUNCTION update_assistant_profile_updated_at();

--------------------------------------------------------------------------------
-- CHAT SESSIONS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app.chat_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    messages JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    message_count INTEGER NOT NULL DEFAULT 0,
    -- From 019: Execution trace metadata
    trace JSONB
);

-- Add trace column if it doesn't exist (from 019)
ALTER TABLE app.chat_sessions ADD COLUMN IF NOT EXISTS trace JSONB;

COMMENT ON COLUMN app.chat_sessions.trace IS 'Execution trace metadata per exchange: system prompts, routing decisions, model input, thinking blocks, token usage';

CREATE INDEX IF NOT EXISTS idx_chat_sessions_updated_at
    ON app.chat_sessions(updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_chat_sessions_message_count
    ON app.chat_sessions(message_count);

--------------------------------------------------------------------------------
-- MODELS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app.models (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES data.entities_person(id) ON DELETE CASCADE,
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

CREATE UNIQUE INDEX IF NOT EXISTS idx_models_user_model ON app.models(user_id, model_id) WHERE user_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_models_system_model ON app.models(model_id) WHERE user_id IS NULL;
CREATE INDEX IF NOT EXISTS idx_models_user_id ON app.models(user_id) WHERE user_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_models_enabled ON app.models(enabled) WHERE enabled = true;

--------------------------------------------------------------------------------
-- AGENTS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app.agents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES data.entities_person(id) ON DELETE CASCADE,
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

CREATE UNIQUE INDEX IF NOT EXISTS idx_agents_user_agent ON app.agents(user_id, agent_id) WHERE user_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_agents_system_agent ON app.agents(agent_id) WHERE user_id IS NULL;
CREATE INDEX IF NOT EXISTS idx_agents_user_id ON app.agents(user_id) WHERE user_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_agents_enabled ON app.agents(enabled) WHERE enabled = true;

--------------------------------------------------------------------------------
-- TOOLS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app.tools (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    tool_type TEXT NOT NULL,
    category TEXT,
    icon TEXT,
    default_params JSONB,
    display_order INTEGER,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Constraint (idempotent)
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'tools_tool_type_check') THEN
        ALTER TABLE app.tools
        ADD CONSTRAINT tools_tool_type_check CHECK (tool_type IN ('mcp', 'virtues'));
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_tools_category ON app.tools(category);
CREATE INDEX IF NOT EXISTS idx_tools_type ON app.tools(tool_type);

--------------------------------------------------------------------------------
-- API USAGE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app.api_usage (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    endpoint TEXT NOT NULL,
    day_bucket DATE NOT NULL,
    request_count INTEGER NOT NULL DEFAULT 0,
    token_count INTEGER NOT NULL DEFAULT 0,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    estimated_cost_usd DECIMAL(10, 4) NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Constraints (idempotent)
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'unique_endpoint_day') THEN
        ALTER TABLE app.api_usage ADD CONSTRAINT unique_endpoint_day UNIQUE(endpoint, day_bucket);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'positive_request_count') THEN
        ALTER TABLE app.api_usage ADD CONSTRAINT positive_request_count CHECK (request_count >= 0);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'positive_token_count') THEN
        ALTER TABLE app.api_usage ADD CONSTRAINT positive_token_count CHECK (token_count >= 0);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'positive_input_tokens') THEN
        ALTER TABLE app.api_usage ADD CONSTRAINT positive_input_tokens CHECK (input_tokens >= 0);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'positive_output_tokens') THEN
        ALTER TABLE app.api_usage ADD CONSTRAINT positive_output_tokens CHECK (output_tokens >= 0);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'positive_cost') THEN
        ALTER TABLE app.api_usage ADD CONSTRAINT positive_cost CHECK (estimated_cost_usd >= 0);
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_usage_day ON app.api_usage(day_bucket DESC);
CREATE INDEX IF NOT EXISTS idx_usage_endpoint ON app.api_usage(endpoint);
CREATE INDEX IF NOT EXISTS idx_usage_endpoint_day ON app.api_usage(endpoint, day_bucket);

--------------------------------------------------------------------------------
-- AUTH: USER (from 021)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app.auth_user (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT UNIQUE NOT NULL,
    email_verified TIMESTAMPTZ
);

--------------------------------------------------------------------------------
-- AUTH: SESSION (from 021)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app.auth_session (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_token TEXT UNIQUE NOT NULL,
    user_id UUID NOT NULL REFERENCES app.auth_user(id) ON DELETE CASCADE,
    expires TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_auth_session_token ON app.auth_session(session_token);
CREATE INDEX IF NOT EXISTS idx_auth_session_user_id ON app.auth_session(user_id);
CREATE INDEX IF NOT EXISTS idx_auth_session_expires ON app.auth_session(expires);

--------------------------------------------------------------------------------
-- AUTH: VERIFICATION TOKEN (from 021)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app.auth_verification_token (
    identifier TEXT NOT NULL,
    token TEXT NOT NULL,
    expires TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (identifier, token)
);

CREATE INDEX IF NOT EXISTS idx_auth_verification_token_expires ON app.auth_verification_token(expires);

--------------------------------------------------------------------------------
-- LLM USAGE (from 022)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app.llm_usage (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    month DATE UNIQUE NOT NULL,
    tokens_used BIGINT DEFAULT 0,
    cost_cents INTEGER DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_llm_usage_month ON app.llm_usage(month);

--------------------------------------------------------------------------------
-- LLM REQUESTS (from 022)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app.llm_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    model TEXT NOT NULL,
    input_tokens INTEGER,
    output_tokens INTEGER
);

CREATE INDEX IF NOT EXISTS idx_llm_requests_created_at ON app.llm_requests(created_at);

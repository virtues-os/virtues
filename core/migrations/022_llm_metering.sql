-- LLM usage metering tables for multi-tenant billing
-- Tracks token usage per month and individual requests

-- Monthly usage aggregates for billing
CREATE TABLE app.llm_usage (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    month DATE UNIQUE NOT NULL,
    tokens_used BIGINT DEFAULT 0,
    cost_cents INTEGER DEFAULT 0
);

-- Individual request log for debugging and detailed analytics
CREATE TABLE app.llm_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    model TEXT NOT NULL,
    input_tokens INTEGER,
    output_tokens INTEGER
);

-- Index for efficient monthly lookups
CREATE INDEX idx_llm_usage_month ON app.llm_usage(month);

-- Index for request history queries
CREATE INDEX idx_llm_requests_created_at ON app.llm_requests(created_at);

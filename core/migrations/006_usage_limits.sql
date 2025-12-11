-- Usage Limits: Configurable monthly limits per external service
-- Services: ai_gateway, assemblyai, google_places, exa
--
-- NOTE: Limits are populated by Rust at server startup via init_limits_from_tier()
-- based on the TIER env var. This migration only creates the schema.

--------------------------------------------------------------------------------
-- USAGE LIMITS TABLE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app.usage_limits (
    service TEXT PRIMARY KEY,
    monthly_limit BIGINT NOT NULL,
    unit TEXT NOT NULL DEFAULT 'requests',
    limit_type TEXT NOT NULL DEFAULT 'hard' CHECK (limit_type IN ('hard', 'soft')),
    enabled BOOLEAN DEFAULT TRUE,
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Index for quick lookups on limits
CREATE INDEX IF NOT EXISTS idx_usage_limits_enabled
    ON app.usage_limits(enabled) WHERE enabled = TRUE;

-- Index for efficient monthly usage aggregation queries
-- Used by get_monthly_usage() to SUM usage across daily buckets
CREATE INDEX IF NOT EXISTS idx_api_usage_endpoint_day
    ON app.api_usage(endpoint, day_bucket);

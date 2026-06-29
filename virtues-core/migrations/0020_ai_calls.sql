-- 0020 — Box-local per-call AI cost log.
--
-- Virtues collects NO central telemetry: the cloud wallet (virtues-api ledger)
-- is the authoritative money truth, but it has no per-call breakdown the user
-- can see. This table is the box-local mirror — one row per paid AI call, with
-- the AUTHORITATIVE `usage.cost` the gateway returns on every response (we used
-- to discard it). It powers the Usage tab's "where did my money go" breakdown
-- and the Telemetry tab's AI-call log.
--
-- METADATA ONLY. Never store prompt or response content here — just feature,
-- model, token counts, and cost. No egress; this lives only on the user's box.
CREATE TABLE app_ai_calls (
    id                TEXT PRIMARY KEY,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Coarse feature bucket: chat | transcription | search | embedding | agent.
    feature           TEXT,
    model             TEXT,
    prompt_tokens     BIGINT NOT NULL DEFAULT 0,
    completion_tokens BIGINT NOT NULL DEFAULT 0,
    reasoning_tokens  BIGINT NOT NULL DEFAULT 0,
    -- Authoritative micros-USD from the gateway `usage.cost` (NOT re-estimated).
    cost_micros       BIGINT NOT NULL DEFAULT 0,
    status            TEXT NOT NULL DEFAULT 'ok',
    chat_id           TEXT,   -- optional link to app_chats
    action_run_id     TEXT    -- optional link to app_action_runs
);

CREATE INDEX idx_app_ai_calls_created_at ON app_ai_calls (created_at);
CREATE INDEX idx_app_ai_calls_feature    ON app_ai_calls (feature, created_at);

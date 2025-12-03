-- Add trace column to chat_sessions for full agent execution observability
-- Stores execution metadata per exchange: system prompts, routing, context, thinking, usage

ALTER TABLE app.chat_sessions
ADD COLUMN IF NOT EXISTS trace JSONB;

-- Add comment for documentation
COMMENT ON COLUMN app.chat_sessions.trace IS 'Execution trace metadata per exchange: system prompts, routing decisions, model input, thinking blocks, token usage';

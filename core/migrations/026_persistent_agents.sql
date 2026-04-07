-- Persistent Agents: turn chat threads into autonomous agents
--
-- All columns nullable. agent_state non-null = agent.
-- States: scheduled, listening, working, paused, complete

ALTER TABLE app_chats ADD COLUMN agent_state TEXT;
ALTER TABLE app_chats ADD COLUMN agent_instruction TEXT;
ALTER TABLE app_chats ADD COLUMN agent_trigger TEXT;
ALTER TABLE app_chats ADD COLUMN agent_activation TEXT;
ALTER TABLE app_chats ADD COLUMN agent_last_run_at TEXT;
ALTER TABLE app_chats ADD COLUMN agent_trigger_token TEXT;

CREATE INDEX IF NOT EXISTS idx_app_chats_agent_state
    ON app_chats(agent_state) WHERE agent_state IS NOT NULL;

-- Action Refactor: decouple actions from chats
--
-- Moves instruction from app_chats.action_instruction to app_actions.instruction.
-- Adds owner (system/user), concurrency_mode, and memory fields.
-- System actions no longer need phantom chats.
--
-- NOTE: app_chats.action_instruction is left as a dead column rather than
-- dropped, to avoid FK cascade risk (app_chat_messages, app_chat_usage).
-- Remove in future schema consolidation.

-- 1. Add new columns to app_actions
ALTER TABLE app_actions ADD COLUMN instruction TEXT;
ALTER TABLE app_actions ADD COLUMN owner TEXT NOT NULL DEFAULT 'user'
    CHECK (owner IN ('system', 'user'));
ALTER TABLE app_actions ADD COLUMN concurrency_mode TEXT NOT NULL DEFAULT 'single'
    CHECK (concurrency_mode IN ('single', 'skip', 'parallel'));
ALTER TABLE app_actions ADD COLUMN memory TEXT;

-- 2. Add result_summary to action runs
ALTER TABLE app_action_runs ADD COLUMN result_summary TEXT;

-- 3. Migrate instruction data from chats to actions
UPDATE app_actions
SET instruction = (
    SELECT action_instruction FROM app_chats
    WHERE app_chats.id = json_extract(app_actions.config, '$.chat_id')
)
WHERE action_type = 'agent'
  AND json_extract(config, '$.chat_id') IS NOT NULL;

-- 4. Mark system actions
UPDATE app_actions SET owner = 'system' WHERE id LIKE 'action_system_%';
UPDATE app_actions SET owner = 'system' WHERE id LIKE 'action_agent_dayline_%';

-- 5. Clear chat_id from system action configs (they no longer need chats)
UPDATE app_actions SET config = '{}' WHERE owner = 'system' AND action_type = 'agent';

-- 6. Delete phantom system chats and their messages
DELETE FROM app_chat_messages WHERE chat_id IN ('chat_dayline_hourly', 'chat_dayline_eod');
DELETE FROM app_chats WHERE id IN ('chat_dayline_hourly', 'chat_dayline_eod');

-- NOTE: app_chats.action_instruction is now a dead column. Do NOT use it.
-- Instruction source of truth is app_actions.instruction.

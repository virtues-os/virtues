-- Migration: Expand model preferences to 4 slots
-- Slots: chat, lite, reasoning, coding

-- Add new model slot columns
ALTER TABLE app_assistant_profile ADD COLUMN chat_model_id TEXT;
ALTER TABLE app_assistant_profile ADD COLUMN lite_model_id TEXT;
ALTER TABLE app_assistant_profile ADD COLUMN reasoning_model_id TEXT;
ALTER TABLE app_assistant_profile ADD COLUMN coding_model_id TEXT;

-- Migrate existing data:
-- default_model_id -> chat_model_id
-- background_model_id -> lite_model_id
UPDATE app_assistant_profile
SET chat_model_id = default_model_id,
    lite_model_id = background_model_id
WHERE chat_model_id IS NULL;

-- Set new defaults for new columns
UPDATE app_assistant_profile
SET chat_model_id = 'google/gemini-3-flash'
WHERE chat_model_id IS NULL;

UPDATE app_assistant_profile
SET lite_model_id = 'zai/glm-4.7-flashx'
WHERE lite_model_id IS NULL;

UPDATE app_assistant_profile
SET reasoning_model_id = 'google/gemini-3-pro-preview'
WHERE reasoning_model_id IS NULL;

UPDATE app_assistant_profile
SET coding_model_id = 'anthropic/claude-opus-4.5'
WHERE coding_model_id IS NULL;

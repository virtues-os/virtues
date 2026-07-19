-- app_assistant_profile seeded default_model_id and background_model_id with
-- hardcoded model ids (0003) that have since been retired from the gateway
-- ('anthropic/claude-sonnet-4-20250514', 'cerebras/llama-3.3-70b'). Those stale
-- values override the code's registry-based fallbacks (default_model_for_slot),
-- silently 500ing every background LLM call — most visibly title generation, so
-- chats keep their raw first-message title in the tab and Spaces list.
--
-- Drop the hardcoded column defaults and clear the stale rows so resolution
-- falls through to the registry, which is the single source of truth for model
-- defaults. Slots the user explicitly picked (any other value) are left alone.
ALTER TABLE app_assistant_profile ALTER COLUMN default_model_id DROP DEFAULT;
ALTER TABLE app_assistant_profile ALTER COLUMN background_model_id DROP DEFAULT;

UPDATE app_assistant_profile
   SET default_model_id = NULL
 WHERE default_model_id = 'anthropic/claude-sonnet-4-20250514';

UPDATE app_assistant_profile
   SET background_model_id = NULL
 WHERE background_model_id = 'cerebras/llama-3.3-70b';

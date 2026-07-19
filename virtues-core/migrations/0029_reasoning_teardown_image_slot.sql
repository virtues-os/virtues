-- Model-slot changes:
--
--  1. Reasoning slot torn down. Nothing selects it anymore — deep-research
--     subagents now run on the Chat model — so drop its per-profile override.
--  2. Image promoted to a first-class configurable slot like the others: a
--     per-box override column, defaulting (when NULL) to the registry Image
--     default via get_image_model().
ALTER TABLE app_assistant_profile DROP COLUMN IF EXISTS reasoning_model_id;
ALTER TABLE app_assistant_profile ADD COLUMN IF NOT EXISTS image_model_id TEXT;

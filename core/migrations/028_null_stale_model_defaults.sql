-- background_model_id is a dead column (replaced by lite_model_id in migration 010).
-- Rust code no longer reads it. NULL it out unconditionally.
-- Also NULL lite_model_id so Rust falls through to default_model_for_slot() in virtues-registry.

UPDATE app_assistant_profile SET background_model_id = NULL;
UPDATE app_assistant_profile SET lite_model_id = NULL;

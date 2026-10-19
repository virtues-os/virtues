-- 0042_standard_and_deep_model_slots
--
-- The model slots became three tiers: Lite, Standard, Deep. Standard is the
-- slot that was called Chat; Deep is new; Coding is gone (nothing ever asked
-- for it).
--
-- New columns rather than renames: a released binary reads this row with
-- `SELECT *` into a struct that names `chat_model_id` and `coding_model_id`,
-- so a rollback must still find them. They are read by nothing from this
-- release on. A pin made after this runs is not seen by a rolled-back binary,
-- which is the cheaper failure: it answers on the recommended model.
ALTER TABLE app_assistant_profile
    ADD COLUMN IF NOT EXISTS standard_model_id text,
    ADD COLUMN IF NOT EXISTS deep_model_id text;

UPDATE app_assistant_profile
SET standard_model_id = chat_model_id
WHERE standard_model_id IS NULL
  AND chat_model_id IS NOT NULL
  AND btrim(chat_model_id) <> '';

-- Backfill the assistant's default name to "Ari" for existing installs.
--
-- The column already declares DEFAULT 'Ari' (0003_app_shell.sql), so new installs
-- are fine. But profiles seeded before that default existed (or with an explicit
-- NULL) fall back to "Ari" only at read time. Set them here so the stored value
-- matches. NULL-only: a user who has renamed their assistant is never clobbered.

UPDATE app_assistant_profile
SET assistant_name = 'Ari'
WHERE assistant_name IS NULL;

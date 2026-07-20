-- 0051: Applets phase 1 (subtractive) — the runtime taxonomy is gone.
--
-- What a row is derives from which fields are set: supervise ⇒ service;
-- no command and no agent ⇒ face-only (old 'view'); otherwise function.
-- `dir` was only ever the manifest folder name, derivable from the id and
-- carried in the in-memory catalog; nothing reads the column anymore.
--
-- Dropping the columns also drops the table CHECK constraints that
-- reference them (runtime enum check; the runtime='view'-or-executable
-- check). No replacement: a face-only applet legitimately has neither
-- command nor agent, so "some field must be set" is no longer a row-level
-- invariant — reconcile --check validates shape at the definition layer.

ALTER TABLE app_actions DROP COLUMN runtime;
ALTER TABLE app_actions DROP COLUMN dir;

-- 0098 — one name for the schedule.
--
-- The manifest key is `schedule`, canonicalized when `default_cron` was
-- retired. The column was `cron_schedule`, and so were the API field, the TS
-- type, the PATCH body and the edit tool. So an author wrote `schedule`, and
-- everything downstream called the same value something else.
--
-- The seed-versus-live distinction that split is often justified by IS real —
-- the manifest declares, the user edits, reconcile does not clobber — but the
-- naming did not encode it. `default_enabled` / `enabled` says which is which
-- in the name; `schedule` / `cron_schedule` differed by an unrelated word and
-- explained nothing. Field ownership is documented in AUTHORING.md, where it
-- belongs; it does not need a second vocabulary.
--
-- Dropping "cron" rather than adding it: the field holds a cron expression
-- today, and a person setting when an applet runs is setting its schedule.

ALTER TABLE app_applets RENAME COLUMN cron_schedule TO schedule;

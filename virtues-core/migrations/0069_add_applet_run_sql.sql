-- 0069_add_applet_run_sql
--
-- The SQL a SKILL.md applet runs before its prompt (`run: sql …` or a .sql
-- file under scripts/). Read-only, on the reader login. Its rows go to the
-- prompt; no rows records a skipped run and calls no model.
ALTER TABLE app_applets ADD COLUMN run_sql text;

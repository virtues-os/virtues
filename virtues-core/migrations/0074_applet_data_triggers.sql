-- 0074_applet_data_triggers
--
-- An applet can start on rows in a data table (agents/plan/applet-shape.md,
-- slice 11): `new data_x`, `30 minutes before data_x`, or
-- `15 minutes after data_x ends`.
--
-- `watch` is that trigger as the file says it, and reconcile rewrites it from
-- the file like the rest of the row: {"table", "kind" ("new", "before" or
-- "after_end"), "minutes", "label"}.
--
-- `watched_through` is the box's: the time up to which the table's rows have
-- been handed to a run. A row whose moment is after it is still waiting. The
-- moment is `created_at` for `new`, the start minus the minutes for `before`,
-- and the end plus the minutes for `after_end`. Reconcile clears it when the
-- trigger changes, and the next poll starts it at the current time, so rows
-- that were already there never start a run.
--
-- Runs a data trigger starts are recorded with trigger 'data'.

ALTER TABLE app_applets ADD COLUMN watch jsonb;
ALTER TABLE app_applets ADD COLUMN watched_through timestamptz;

ALTER TABLE app_applet_runs DROP CONSTRAINT app_applet_runs_trigger_check;
ALTER TABLE app_applet_runs
    ADD CONSTRAINT app_applet_runs_trigger_check
    CHECK (trigger IN ('cron', 'manual', 'tool', 'api', 'webhook', 'message', 'data'));

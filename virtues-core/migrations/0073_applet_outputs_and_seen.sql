-- 0073_applet_outputs_and_seen
--
-- What each applet run made, beyond pages, and when the owner last looked at
-- an applet (agents/plan/applet-shape.md, slice 10).
--
-- `rows` records the rows a run wrote into one of its tables (`ref_id` is the
-- table), and `task` a task it added. `item_count` is how many: rows written,
-- or 1 for a page or a task. The runner adds to it as a run writes.
--
-- `seen_at` is when the owner last opened the applet's page. The Inbox lists
-- an unattended run that made something after it.

ALTER TABLE app_applet_run_outputs DROP CONSTRAINT app_applet_run_outputs_kind_check;
ALTER TABLE app_applet_run_outputs
    ADD CONSTRAINT app_applet_run_outputs_kind_check CHECK (kind IN ('page', 'rows', 'task'));
ALTER TABLE app_applet_run_outputs ADD COLUMN item_count integer NOT NULL DEFAULT 1;

ALTER TABLE app_applets ADD COLUMN seen_at timestamptz;

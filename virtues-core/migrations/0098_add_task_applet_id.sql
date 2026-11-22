-- 0098_add_task_applet_id
--
-- A task of the assistant's that repeats is an applet with a prompt and a
-- trigger, and the applet runner keeps running it. One row in app_tasks
-- stands for the whole of it, so it shows in the task list beside your own
-- tasks. `applet_id` names the applet that row stands for. It is null on
-- every other task.
--
-- One applet has one such row. Deleting the applet deletes the row, because
-- the task no longer exists.
--
-- The request worker must never run one of these rows as a one-time request.
-- It claims rows with a `next_run_at`, and these keep theirs null; the claim
-- also checks `applet_id IS NULL`.

ALTER TABLE app_tasks
    ADD COLUMN applet_id text REFERENCES app_applets (id) ON DELETE CASCADE;

CREATE UNIQUE INDEX app_tasks_applet_id_key ON app_tasks (applet_id) WHERE applet_id IS NOT NULL;

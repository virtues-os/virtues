-- 0059_task_requests
--
-- Requests: tasks assigned to the assistant that it works in the background
-- (agents/plan/tasks-plan.md). A run ends at `ready`, with its answer in
-- `result`, and the owner closes the request by accepting it. `ready` is not
-- closed, so `closed_at` stays null for it.
--
-- `chat_id` is the request's own work chat, made by its first run and kept
-- for every later one. `next_run_at` is when the worker may pick the
-- request up; null means it never runs on its own. Every row that exists
-- before this migration keeps a null `next_run_at`, so an upgrade does not
-- start a model run for every assistant task already on the box.

ALTER TABLE app_tasks DROP CONSTRAINT app_tasks_status_check;
ALTER TABLE app_tasks ADD CONSTRAINT app_tasks_status_check
    CHECK (status IN ('open', 'active', 'ready', 'done', 'dismissed'));

ALTER TABLE app_tasks
    ADD COLUMN result      text,
    ADD COLUMN result_url  text,
    ADD COLUMN chat_id     text REFERENCES app_chats(id) ON DELETE SET NULL,
    ADD COLUMN next_run_at timestamptz,
    ADD COLUMN attempts    integer NOT NULL DEFAULT 0 CHECK (attempts >= 0);

-- The worker's one question: which request is due to run next.
CREATE INDEX idx_app_tasks_queued ON app_tasks (next_run_at)
    WHERE assignee = 'assistant' AND status = 'open' AND next_run_at IS NOT NULL;

-- A request's cost is the sum of the AI calls made in its work chat, so
-- every call records the chat it was made for: chat turns and headless runs
-- both write it.
ALTER TABLE app_ai_calls ADD COLUMN chat_id text;
CREATE INDEX idx_app_ai_calls_chat ON app_ai_calls (chat_id, created_at)
    WHERE chat_id IS NOT NULL;

-- 0057_add_tasks
--
-- Tasks: one to-do list the owner and the assistant share
-- (agents/plan/tasks-plan.md). A task is a title and a few facts. Who does
-- it is `assignee`, who created it is `origin`, and the four statuses are
-- the whole lifecycle: a task leaves the list as `done` or `dismissed`,
-- never by deletion.
--
-- `source_url` uses the app's URL convention (`/page/page_…`, `/chat/chat_…`)
-- so it opens anywhere a ref opens. `due_on` is set whenever `due_at` is,
-- so grouping by day never has to read a timestamp.

CREATE TABLE app_tasks (
    id          text PRIMARY KEY,
    title       text NOT NULL CHECK (btrim(title) <> ''),
    notes       text,
    assignee    text NOT NULL DEFAULT 'owner'
                CHECK (assignee IN ('owner', 'assistant')),
    status      text NOT NULL DEFAULT 'open'
                CHECK (status IN ('open', 'active', 'done', 'dismissed')),
    due_on      date,
    due_at      timestamptz,
    project_id  text REFERENCES app_projects(id) ON DELETE SET NULL,
    origin      text NOT NULL
                CHECK (origin IN ('owner', 'chat', 'assistant')),
    source_url  text,
    sort_order  double precision NOT NULL DEFAULT 0,
    closed_at   timestamptz,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CHECK (due_at IS NULL OR due_on IS NOT NULL),
    CHECK ((status IN ('done', 'dismissed')) = (closed_at IS NOT NULL))
);

-- The list's one hot question: what is still open.
CREATE INDEX idx_app_tasks_open ON app_tasks (due_on, sort_order)
    WHERE status IN ('open', 'active');
CREATE INDEX idx_app_tasks_project ON app_tasks (project_id)
    WHERE project_id IS NOT NULL;

-- What happened to a task, signed by whoever did it. Append-only: the task
-- detail reads it as history, so a row is never edited or removed.
CREATE TABLE app_task_events (
    id          bigserial PRIMARY KEY,
    task_id     text NOT NULL REFERENCES app_tasks(id) ON DELETE CASCADE,
    author      text NOT NULL CHECK (author IN ('owner', 'assistant')),
    kind        text NOT NULL CHECK (kind IN ('created', 'edited', 'status', 'note')),
    body        jsonb NOT NULL DEFAULT '{}'::jsonb,
    chat_id     text,
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_app_task_events_task ON app_task_events (task_id, created_at);

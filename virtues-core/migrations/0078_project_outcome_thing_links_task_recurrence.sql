-- 0078_project_outcome_thing_links_task_recurrence
--
-- Three things a person can now do by hand: say how a project ended, put a
-- project or a task under a thing, and let a task recur.
--
-- A release from before this one closes a project by setting `archived_at`
-- alone, which the outcome CHECK below refuses. After `virtues rollback` to
-- one of those releases, Archive on a project fails until the box upgrades
-- again. Reading projects and tasks is unaffected.

-- A project ends when the person finishes it or drops it. `archived_at`
-- already says when a project was closed, and `outcome` says which of the two
-- it was. A project closed before this migration is marked finished, which is
-- what closing meant until now (0030: "finished, kept, out of the working
-- view"). The person can change it to dropped on the project's page.
ALTER TABLE app_projects
    ADD COLUMN outcome text
        CONSTRAINT app_projects_outcome_check CHECK (outcome IN ('finished', 'dropped')),
    ADD COLUMN thing_id text REFERENCES wiki_things(id) ON DELETE SET NULL;

UPDATE app_projects SET outcome = 'finished' WHERE archived_at IS NOT NULL;

-- An ended project has an outcome, and an open one has none.
ALTER TABLE app_projects ADD CONSTRAINT app_projects_outcome_shape_check
    CHECK ((outcome IS NULL) = (archived_at IS NULL));

CREATE INDEX idx_app_projects_thing ON app_projects (thing_id)
    WHERE thing_id IS NOT NULL;

-- A task sits under a project, under a thing, or under nothing. It has one
-- parent at most: a task under a project is already under that project's
-- thing.
--
-- A task recurs every `recur_every` `recur_unit`s, counted from the day the
-- person finishes it: closing one as done opens the next, which names the one
-- it followed in `recurred_from_id`. So one occurrence is open at a time and
-- a missed one never piles up.
ALTER TABLE app_tasks
    ADD COLUMN thing_id text REFERENCES wiki_things(id) ON DELETE SET NULL,
    ADD COLUMN recurred_from_id text REFERENCES app_tasks(id) ON DELETE SET NULL,
    ADD COLUMN recur_every integer
        CONSTRAINT app_tasks_recur_every_check CHECK (recur_every > 0),
    ADD COLUMN recur_unit text
        CONSTRAINT app_tasks_recur_unit_check
        CHECK (recur_unit IN ('day', 'week', 'month', 'year')),
    ADD CONSTRAINT app_tasks_one_parent_check
        CHECK (project_id IS NULL OR thing_id IS NULL),
    -- Both or neither. Written as an equality because a CHECK passes on NULL,
    -- so "every > 0 AND unit IN (...)" alone would accept a count with no unit.
    ADD CONSTRAINT app_tasks_recur_pair_check
        CHECK ((recur_every IS NULL) = (recur_unit IS NULL));

CREATE INDEX idx_app_tasks_thing ON app_tasks (thing_id)
    WHERE thing_id IS NOT NULL;

-- Reopening a finished task looks for the occurrence that followed it.
CREATE INDEX idx_app_tasks_recurred_from ON app_tasks (recurred_from_id)
    WHERE recurred_from_id IS NOT NULL;

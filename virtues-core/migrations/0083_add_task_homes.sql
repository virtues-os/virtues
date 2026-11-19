-- 0083_add_task_homes
--
-- Every task gets one home: the thread it moves forward or, when no thread
-- fits, the person, place, organization or thing it concerns. A task can
-- also be about other threads and subjects, and it shows on each of them.
--
-- A home is one column per kind of parent, each a foreign key, so deleting a
-- subject clears the home instead of leaving it pointing at nothing. The
-- "also about" links use the same shape for the same reason: a link row
-- names exactly one target, and deleting that target deletes the link.
--
-- A thread that is open is active or in the backlog. `is_backlog` says which;
-- an ended thread keeps the value it had.
--
-- Rolling back to an older release: that release ignores the new columns
-- and the links table, so tasks homed on a person, place or org show as
-- having no home there, and backlog threads show as active.

ALTER TABLE app_tasks
    ADD COLUMN person_id text REFERENCES wiki_people(id) ON DELETE SET NULL,
    ADD COLUMN place_id text REFERENCES wiki_places(id) ON DELETE SET NULL,
    ADD COLUMN org_id text REFERENCES wiki_orgs(id) ON DELETE SET NULL;

ALTER TABLE app_tasks DROP CONSTRAINT app_tasks_one_parent_check;
ALTER TABLE app_tasks ADD CONSTRAINT app_tasks_one_home_check
    CHECK (num_nonnulls(thread_id, thing_id, person_id, place_id, org_id) <= 1);

CREATE INDEX idx_app_tasks_person ON app_tasks (person_id) WHERE person_id IS NOT NULL;
CREATE INDEX idx_app_tasks_place ON app_tasks (place_id) WHERE place_id IS NOT NULL;
CREATE INDEX idx_app_tasks_org ON app_tasks (org_id) WHERE org_id IS NOT NULL;

CREATE TABLE app_task_links (
    id bigserial PRIMARY KEY,
    task_id text NOT NULL REFERENCES app_tasks(id) ON DELETE CASCADE,
    thread_id text REFERENCES app_threads(id) ON DELETE CASCADE,
    person_id text REFERENCES wiki_people(id) ON DELETE CASCADE,
    place_id text REFERENCES wiki_places(id) ON DELETE CASCADE,
    org_id text REFERENCES wiki_orgs(id) ON DELETE CASCADE,
    thing_id text REFERENCES wiki_things(id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT app_task_links_one_target_check
        CHECK (num_nonnulls(thread_id, person_id, place_id, org_id, thing_id) = 1),
    CONSTRAINT app_task_links_thread_key UNIQUE (task_id, thread_id),
    CONSTRAINT app_task_links_person_key UNIQUE (task_id, person_id),
    CONSTRAINT app_task_links_place_key UNIQUE (task_id, place_id),
    CONSTRAINT app_task_links_org_key UNIQUE (task_id, org_id),
    CONSTRAINT app_task_links_thing_key UNIQUE (task_id, thing_id)
);

CREATE INDEX idx_app_task_links_thread ON app_task_links (thread_id) WHERE thread_id IS NOT NULL;
CREATE INDEX idx_app_task_links_person ON app_task_links (person_id) WHERE person_id IS NOT NULL;
CREATE INDEX idx_app_task_links_place ON app_task_links (place_id) WHERE place_id IS NOT NULL;
CREATE INDEX idx_app_task_links_org ON app_task_links (org_id) WHERE org_id IS NOT NULL;
CREATE INDEX idx_app_task_links_thing ON app_task_links (thing_id) WHERE thing_id IS NOT NULL;

ALTER TABLE app_threads ADD COLUMN is_backlog boolean NOT NULL DEFAULT false;

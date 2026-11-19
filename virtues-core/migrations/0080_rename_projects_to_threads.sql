-- 0080_rename_projects_to_threads
--
-- A project is now called a thread: something happening in the person's life
-- that they are following, unfolding and not settled. Some finish, some never
-- do. A thread holds tasks, records, pages, chats and files, and points to the
-- subjects it involves (people, places, organizations, things).
--
-- Only names change. Rows, ids (`nb_…`, `space_…`) and the item urls stay as
-- they are, so links, pins and citations keep resolving.
--
--   app_projects       -> app_threads
--   app_project_items  -> app_thread_items, and its project_id -> thread_id
--   app_chats.project_id, app_tasks.project_id -> thread_id
--
-- Constraint and index names follow, including NOT NULL constraints and the
-- ones still named after notebooks from before 0029.
--
-- A release from before this one queries `app_projects`. After `virtues
-- rollback` to one of those releases, its projects, tasks and project chats
-- fail until the box upgrades again.

ALTER TABLE app_projects RENAME TO app_threads;
ALTER TABLE app_project_items RENAME TO app_thread_items;
ALTER TABLE app_thread_items RENAME COLUMN project_id TO thread_id;
ALTER TABLE app_chats RENAME COLUMN project_id TO thread_id;
ALTER TABLE app_tasks RENAME COLUMN project_id TO thread_id;

-- Postgres 18 keeps a catalog row for a named NOT NULL and older servers do
-- not, so names are read from the catalog. Renaming a primary key or unique
-- constraint renames its index.
DO $$
DECLARE
    c record;
    new_name text;
BEGIN
    FOR c IN
        SELECT conrelid::regclass AS tbl, conname FROM pg_constraint
         WHERE conrelid IN ('app_threads'::regclass, 'app_thread_items'::regclass,
                            'app_chats'::regclass, 'app_tasks'::regclass)
           AND (conname LIKE '%project%' OR conname LIKE '%notebook%')
    LOOP
        new_name := replace(replace(replace(replace(replace(c.conname,
            'app_notebook_items', 'app_thread_items'),
            'app_notebooks', 'app_threads'),
            'app_project_items', 'app_thread_items'),
            'app_projects', 'app_threads'),
            'project_id', 'thread_id');
        EXECUTE format('ALTER TABLE %s RENAME CONSTRAINT %I TO %I', c.tbl, c.conname, new_name);
    END LOOP;
END $$;

DO $$
DECLARE
    i record;
BEGIN
    FOR i IN
        SELECT indexname FROM pg_indexes
         WHERE schemaname = current_schema()
           AND tablename IN ('app_threads', 'app_thread_items', 'app_chats', 'app_tasks')
           AND indexname LIKE '%project%'
    LOOP
        EXECUTE format('ALTER INDEX %I RENAME TO %I', i.indexname,
            replace(replace(replace(i.indexname,
                'app_projects', 'app_threads'),
                'project_items', 'thread_items'),
                'project', 'thread'));
    END LOOP;
END $$;

-- 0029_rename_notebooks_to_projects
--
-- "Notebook" becomes "Project". The product concept is unchanged — a manual
-- collection the user returns to, which grounds the chats that live in it —
-- but "project" is the clearer word for users, so every place a user or a
-- model reads the word now says project, and the schema follows: two tables,
-- two columns, and every index and constraint that carried the old word, so
-- `\d app_projects` reads cleanly. The Rust side (`api/projects.rs`) and the
-- ref-URL route (`/project/{id}`) rename with it.
--
-- Ids are NOT rewritten: a project id is still `nb_…`. Every existing id,
-- ref URL, pin and member row carries that prefix, and rewriting ids across
-- foreign keys and stored JSON on live boxes is not worth a cosmetic letter.
--
-- Two data rewrites follow the renames, explained inline.

-- Tables.
ALTER TABLE app_notebooks RENAME TO app_projects;
ALTER TABLE app_notebook_items RENAME TO app_project_items;

-- Columns.
ALTER TABLE app_project_items RENAME COLUMN notebook_id TO project_id;
ALTER TABLE app_chats RENAME COLUMN notebook_id TO project_id;

-- Constraints. The `_space_id_` names are older still (notebooks were once
-- "spaces"); they collapse to the current word in one step.
ALTER TABLE app_projects RENAME CONSTRAINT app_notebooks_pkey TO app_projects_pkey;
ALTER TABLE app_project_items RENAME CONSTRAINT app_notebook_items_pkey TO app_project_items_pkey;
ALTER TABLE app_project_items RENAME CONSTRAINT app_notebook_items_space_id_url_key TO app_project_items_project_id_url_key;
ALTER TABLE app_project_items RENAME CONSTRAINT app_notebook_items_space_id_fkey TO app_project_items_project_id_fkey;
-- The named NOT NULL exists as a catalog row only on Postgres 18+ (boxes
-- install 18; the 0001 dump is from 18). On an older server the name in
-- `CONSTRAINT … NOT NULL` was accepted and discarded, so guard the rename
-- rather than fail the whole migration over a label.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'app_notebook_items_space_id_not_null'
          AND conrelid = 'app_project_items'::regclass
    ) THEN
        ALTER TABLE app_project_items RENAME CONSTRAINT app_notebook_items_space_id_not_null TO app_project_items_project_id_not_null;
    END IF;
END $$;
ALTER TABLE app_project_items RENAME CONSTRAINT app_notebook_items_added_by_check TO app_project_items_added_by_check;
ALTER TABLE app_project_items RENAME CONSTRAINT app_notebook_items_role_check TO app_project_items_role_check;
ALTER TABLE app_chats RENAME CONSTRAINT app_chats_space_id_fkey TO app_chats_project_id_fkey;

-- Indexes. (Renaming a unique constraint above renamed its backing index.)
ALTER INDEX idx_app_notebooks_dirty RENAME TO idx_app_projects_dirty;
ALTER INDEX idx_app_notebooks_name RENAME TO idx_app_projects_name;
ALTER INDEX idx_app_notebooks_sort RENAME TO idx_app_projects_sort;
ALTER INDEX idx_chats_notebook RENAME TO idx_chats_project;
ALTER INDEX idx_notebook_items_magnet RENAME TO idx_project_items_magnet;
ALTER INDEX idx_notebook_items_notebook RENAME TO idx_project_items_project;
ALTER INDEX idx_notebook_items_url RENAME TO idx_project_items_url;

-- The identity sequence still wore the "space" name from two renames ago.
ALTER SEQUENCE app_space_items_id_seq RENAME TO app_project_items_id_seq;

-- The `set_updated_at` trigger on app_projects does not carry the word and
-- follows its table; nothing to rename.

-- Data rewrite 1: the UI route moves from `/notebook/{id}` to `/project/{id}`.
-- Stored ref URLs in the two tables that hold them as rows are rewritten so a
-- pinned project and a project filed inside another still open. Wiki refs and
-- chat-message JSON are left alone: the ref parser (`refs::split_ref`) accepts
-- `/notebook/` as a legacy spelling of the same kind.
UPDATE app_pins SET url = replace(url, '/notebook/', '/project/') WHERE url LIKE '/notebook/%';
UPDATE app_project_items SET url = replace(url, '/notebook/', '/project/') WHERE url LIKE '/notebook/%';

-- Data rewrite 2: a project cannot be a member of a project — you cannot
-- folder a folder. A nested notebook used to be a nav-only 'pin' edge; the
-- API now refuses the add with a 400, and the rows that already exist go
-- with it (they were never retrieval scope, so nothing a chat could see
-- changes).
DELETE FROM app_project_items WHERE url LIKE '/project/%';

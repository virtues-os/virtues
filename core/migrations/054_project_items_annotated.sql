-- Migration 054: Simplify project items to annotated bookmarks
--
-- Project items are now {name, url, description} — self-contained cards,
-- not cached pointers. The `description` field is the user's annotation of
-- WHY this item is in the project, which is the primary salience signal
-- for the AI. The `icon` column is dropped (derived from URL prefix in UI).
-- The `label` column is renamed to `name`.

-- SQLite doesn't support RENAME COLUMN in older versions, and we need to
-- drop `icon` + add `description`, so rebuild the table.

CREATE TABLE app_project_items_new (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES app_projects(id) ON DELETE CASCADE,
    url TEXT NOT NULL,
    name TEXT,                                    -- human-readable label
    description TEXT,                             -- user's annotation: why this item matters
    sort_order INTEGER NOT NULL DEFAULT 0,
    added_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(project_id, url)
);

INSERT INTO app_project_items_new (id, project_id, url, name, description, sort_order, added_at)
SELECT id, project_id, url, label, NULL, sort_order, added_at
FROM app_project_items;

DROP TABLE app_project_items;
ALTER TABLE app_project_items_new RENAME TO app_project_items;

CREATE INDEX idx_app_project_items_project ON app_project_items(project_id, sort_order);
CREATE INDEX idx_app_project_items_url ON app_project_items(url);

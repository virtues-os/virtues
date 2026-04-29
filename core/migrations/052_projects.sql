-- Migration 052: Projects
--
-- Projects are curated, reusable sets of references (pages, chats, people,
-- places, files) that a user can apply as a context lens in chat. A project
-- is NOT a workspace — it has no default chat thread, no scoped filtering,
-- no sub-pages. It is a named, icon'd table of URLs. Users @-mention a
-- project in chat to inline its members as salience hints for the agent.
--
-- Design notes:
--   - `app_projects`: the project itself (name, icon, optional description).
--   - `app_project_items`: URL references. The `url` column mirrors the
--     existing `app_space_items.url` convention so a project can hold
--     anything URL-addressable for free.
--   - `UNIQUE(project_id, url)` prevents duplicate adds.
--   - `idx_app_project_items_url` supports a future "which projects contain
--     this entity?" reverse lookup cheaply.

CREATE TABLE IF NOT EXISTS app_projects (
    id TEXT PRIMARY KEY,                          -- prj_<ulid>
    name TEXT NOT NULL,
    icon TEXT,                                    -- iconify id or emoji
    description TEXT,                             -- optional goal / salience hint
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_app_projects_sort ON app_projects(sort_order, updated_at DESC);

CREATE TRIGGER IF NOT EXISTS app_projects_set_updated_at
    AFTER UPDATE ON app_projects
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_projects SET updated_at = datetime('now') WHERE id = NEW.id;
END;

CREATE TABLE IF NOT EXISTS app_project_items (
    id TEXT PRIMARY KEY,                          -- prji_<ulid>
    project_id TEXT NOT NULL REFERENCES app_projects(id) ON DELETE CASCADE,
    url TEXT NOT NULL,                            -- '/page/page_xxx', '/chat/chat_xxx', '/person/p_xxx', ...
    label TEXT,                                   -- cached label at add time
    icon TEXT,                                    -- cached icon at add time
    sort_order INTEGER NOT NULL DEFAULT 0,
    added_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(project_id, url)
);

CREATE INDEX IF NOT EXISTS idx_app_project_items_project ON app_project_items(project_id, sort_order);
CREATE INDEX IF NOT EXISTS idx_app_project_items_url ON app_project_items(url);

-- 014: Schema Cleanup + Sidebar & App Enhancements
--
-- Part A: Drops dead tables, renames spaces/views/space_items to app_* prefix.
-- Part B: Refreshes system sidebar views with icons and updated query configs,
-- adds chat icons, space vectorization toggle, and profile timezone.

--------------------------------------------------------------------------------
-- PART A: Schema cleanup & table renames
--------------------------------------------------------------------------------

-- Drop dead tables (never queried from Rust)
DROP TABLE IF EXISTS wiki_connections;
DROP TABLE IF EXISTS data_embedding_jobs;

-- Drop old triggers (will recreate with new names)
DROP TRIGGER IF EXISTS spaces_set_updated_at;
DROP TRIGGER IF EXISTS views_set_updated_at;

-- Rename tables (SQLite automatically updates FK references)
ALTER TABLE spaces RENAME TO app_spaces;
ALTER TABLE views RENAME TO app_views;
ALTER TABLE space_items RENAME TO app_space_items;

-- Recreate triggers with new names
CREATE TRIGGER IF NOT EXISTS app_spaces_set_updated_at
    AFTER UPDATE ON app_spaces
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_spaces SET updated_at = datetime('now') WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS app_views_set_updated_at
    AFTER UPDATE ON app_views
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_views SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- PART B STEP 1: Schema additions
--------------------------------------------------------------------------------

ALTER TABLE chats ADD COLUMN icon TEXT;
ALTER TABLE app_spaces ADD COLUMN vectorize BOOLEAN DEFAULT FALSE;
ALTER TABLE app_user_profile ADD COLUMN timezone TEXT;

--------------------------------------------------------------------------------
-- PART B STEP 2: Replace old system views
-- Old IDs (view_sys_*) → new IDs (view_sys_sec_*) with icons + updated configs
--------------------------------------------------------------------------------

DELETE FROM app_space_items WHERE view_id IN (
    'view_sys_chats', 'view_sys_pages', 'view_sys_wiki',
    'view_sys_data', 'view_sys_developer'
);

DELETE FROM app_views WHERE id IN (
    'view_sys_chats', 'view_sys_pages', 'view_sys_wiki',
    'view_sys_data', 'view_sys_developer'
);

INSERT OR IGNORE INTO app_views (id, space_id, parent_view_id, name, icon, sort_order, view_type, query_config, is_system) VALUES
    ('view_sys_sec_chats',     'space_system', NULL, 'Chats',     'ri:chat-1-line',        100, 'smart',  '{"has_add":"chat","more_route":"/chat-history","namespace":"chat","limit":8}', TRUE),
    ('view_sys_sec_pages',     'space_system', NULL, 'Pages',     'ri:file-text-line',     200, 'smart',  '{"has_add":"page","more_route":"/page","namespace":"page","limit":8}', TRUE),
    ('view_sys_sec_wiki',      'space_system', NULL, 'Wiki',      'ri:book-open-line',     300, 'manual', NULL, TRUE),
    ('view_sys_sec_data',      'space_system', NULL, 'Data',      'ri:database-2-line',    400, 'manual', NULL, TRUE),
    ('view_sys_sec_developer', 'space_system', NULL, 'Developer', 'ri:code-s-slash-line',  500, 'manual', NULL, TRUE);

--------------------------------------------------------------------------------
-- PART B STEP 3: Seed space_items for manual folders
--------------------------------------------------------------------------------

INSERT OR IGNORE INTO app_space_items (view_id, url, sort_order) VALUES
    -- Wiki
    ('view_sys_sec_wiki', '/day', 0),
    ('view_sys_sec_wiki', '/person', 1),
    ('view_sys_sec_wiki', '/place', 2),
    ('view_sys_sec_wiki', '/org', 3),
    -- Data
    ('view_sys_sec_data', '/sources', 0),
    ('view_sys_sec_data', '/drive', 1),
    -- Developer
    ('view_sys_sec_developer', '/virtues/sql', 0),
    ('view_sys_sec_developer', '/virtues/terminal', 1),
    ('view_sys_sec_developer', '/virtues/sitemap', 2),
    ('view_sys_sec_developer', '/virtues/lake', 3),
    ('view_sys_sec_developer', '/virtues/jobs', 4);

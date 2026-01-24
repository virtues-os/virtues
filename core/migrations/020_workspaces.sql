-- Workspaces and Explorer Nodes: Unified hierarchy system
-- Replaces the old folders table with a universal node-based organization
--
-- Key concepts:
-- - Workspaces are swipeable contexts (like Arc browser spaces)
-- - explorer_nodes is the single source of truth for ALL organization
-- - node_type: 'folder' (manual box), 'view' (smart filter), 'shortcut' (link to entity)
-- - Entity type is derived from entity_id prefix (page_, chat_, person_, etc.)

--------------------------------------------------------------------------------
-- WORKSPACES
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS workspaces (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    icon TEXT,
    is_system BOOLEAN DEFAULT FALSE,      -- TRUE for "Virtues" (undeletable)
    is_locked BOOLEAN DEFAULT FALSE,      -- TRUE = nodes cannot be edited by user
    sort_order INTEGER DEFAULT 0,         -- Order in switcher (0 = leftmost)
    
    -- Theming
    accent_color TEXT,                    -- e.g., "#7C3AED"
    theme_mode TEXT,                      -- 'light', 'dark', 'system', or NULL (inherit)
    
    -- State (JSON blobs)
    active_tab_state_json TEXT,           -- Open tabs, splits, active tab
    expanded_nodes_json TEXT,             -- Which folders are expanded
    
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TRIGGER IF NOT EXISTS workspaces_set_updated_at
    AFTER UPDATE ON workspaces
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE workspaces SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- EXPLORER NODES (Unified Hierarchy)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS explorer_nodes (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    parent_id TEXT REFERENCES explorer_nodes(id) ON DELETE CASCADE,
    sort_order INTEGER DEFAULT 0,         -- Gap-based: 1000, 2000, 3000
    
    -- Node classification
    node_type TEXT NOT NULL,              -- 'folder', 'view', 'shortcut'
    name TEXT,                            -- Display name (for folders/views)
    icon TEXT,                            -- Custom icon override
    
    -- For shortcuts: the entity this points to
    -- Type is derived from prefix: page_, chat_, person_, place_, org_, thing_, file_, etc.
    entity_id TEXT,
    
    -- For views: auto-populate configuration
    -- Example: {"type": "pages"} or {"type": "wiki", "subtype": "places"}
    view_config_json TEXT,
    
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_explorer_nodes_workspace
    ON explorer_nodes(workspace_id);

CREATE INDEX IF NOT EXISTS idx_explorer_nodes_tree
    ON explorer_nodes(workspace_id, parent_id, sort_order);

CREATE INDEX IF NOT EXISTS idx_explorer_nodes_entity
    ON explorer_nodes(entity_id);

CREATE TRIGGER IF NOT EXISTS explorer_nodes_set_updated_at
    AFTER UPDATE ON explorer_nodes
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE explorer_nodes SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- SEED DEFAULT WORKSPACES
--------------------------------------------------------------------------------

-- System workspace: "Virtues" - the global inventory (locked, undeletable)
INSERT OR IGNORE INTO workspaces (id, name, icon, is_system, is_locked, sort_order) VALUES
    ('ws_system', 'Virtues', 'ri:compass-3-line', TRUE, TRUE, 0);

-- Default user workspace: "Home" - empty starter workspace
INSERT OR IGNORE INTO workspaces (id, name, icon, is_system, is_locked, sort_order) VALUES
    ('ws_home', 'Home', 'ri:home-4-line', FALSE, FALSE, 1);

--------------------------------------------------------------------------------
-- SEED SYSTEM WORKSPACE TREE
-- The System workspace uses the same explorer_nodes table, just seeded with views
--------------------------------------------------------------------------------

-- Top-level views in System workspace
INSERT OR IGNORE INTO explorer_nodes (id, workspace_id, sort_order, node_type, name, icon, view_config_json) VALUES
    ('node_sys_chats', 'ws_system', 1000, 'view', 'Chats', 'ri:chat-1-line', '{"type":"chats","workspace_scoped":false}'),
    ('node_sys_pages', 'ws_system', 2000, 'view', 'Pages', 'ri:file-text-line', '{"type":"pages","workspace_scoped":false}'),
    ('node_sys_wiki', 'ws_system', 3000, 'folder', 'Wiki', 'ri:book-2-line', NULL),
    ('node_sys_drive', 'ws_system', 4000, 'view', 'Drive', 'ri:folder-line', '{"type":"drive"}'),
    ('node_sys_sources', 'ws_system', 5000, 'view', 'Sources', 'ri:database-2-line', '{"type":"sources"}'),
    ('node_sys_data', 'ws_system', 6000, 'folder', 'Data', 'ri:database-2-line', NULL),
    ('node_sys_dev', 'ws_system', 7000, 'folder', 'Developer', 'ri:code-s-slash-line', NULL);

-- Wiki sub-views (nested under node_sys_wiki)
INSERT OR IGNORE INTO explorer_nodes (id, workspace_id, parent_id, sort_order, node_type, name, icon, view_config_json) VALUES
    ('node_sys_wiki_people', 'ws_system', 'node_sys_wiki', 1000, 'view', 'People', 'ri:user-line', '{"type":"wiki","subtype":"people"}'),
    ('node_sys_wiki_places', 'ws_system', 'node_sys_wiki', 2000, 'view', 'Places', 'ri:map-pin-line', '{"type":"wiki","subtype":"places"}'),
    ('node_sys_wiki_orgs', 'ws_system', 'node_sys_wiki', 3000, 'view', 'Organizations', 'ri:building-line', '{"type":"wiki","subtype":"orgs"}'),
    ('node_sys_wiki_things', 'ws_system', 'node_sys_wiki', 4000, 'view', 'Things', 'ri:box-3-line', '{"type":"wiki","subtype":"things"}'),
    ('node_sys_wiki_days', 'ws_system', 'node_sys_wiki', 5000, 'view', 'Days', 'ri:calendar-line', '{"type":"wiki","subtype":"days"}');

-- Data shortcuts (route links)
INSERT OR IGNORE INTO explorer_nodes (id, workspace_id, parent_id, sort_order, node_type, name, icon, entity_id) VALUES
    ('node_sys_data_activity', 'ws_system', 'node_sys_data', 1000, 'shortcut', 'Activity', 'ri:history-line', 'route:/data/jobs'),
    ('node_sys_data_usage', 'ws_system', 'node_sys_data', 2000, 'shortcut', 'Usage', 'ri:dashboard-2-line', 'route:/usage');

-- Developer shortcuts (route links)
INSERT OR IGNORE INTO explorer_nodes (id, workspace_id, parent_id, sort_order, node_type, name, icon, entity_id) VALUES
    ('node_sys_dev_sql', 'ws_system', 'node_sys_dev', 1000, 'shortcut', 'SQL Viewer', 'ri:database-2-line', 'route:/developer/sql-viewer'),
    ('node_sys_dev_terminal', 'ws_system', 'node_sys_dev', 2000, 'shortcut', 'Terminal', 'ri:terminal-box-line', 'route:/developer/terminal');

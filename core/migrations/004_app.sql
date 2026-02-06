-- App: Chats, Pages, Spaces, Namespaces, Views (SQLite)
-- Consolidated from migrations 007, 010, 012, 018, 020, 021, 022
--
-- Key principle: Spaces are just collections of URLs organized into folders.
-- Entities (chats, pages) do NOT have space_id - they're referenced via space_items.

--------------------------------------------------------------------------------
-- CHATS (renamed from app_chat_sessions)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS chats (
    id TEXT PRIMARY KEY,  -- e.g., 'chat_abc123'
    title TEXT NOT NULL,
    message_count INTEGER NOT NULL DEFAULT 0,
    trace TEXT,  -- JSON

    -- Context management
    conversation_summary TEXT,
    summary_up_to_index INTEGER DEFAULT 0,
    summary_version INTEGER DEFAULT 0,
    last_compacted_at TEXT,

    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_chats_updated ON chats(updated_at DESC);

CREATE TRIGGER IF NOT EXISTS chats_set_updated_at
    AFTER UPDATE ON chats
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE chats SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- CHAT MESSAGES (renamed from app_chat_messages)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS chat_messages (
    id TEXT PRIMARY KEY,
    chat_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system', 'checkpoint')),
    content TEXT NOT NULL,

    -- Model/provider info (for assistant messages)
    model TEXT,
    provider TEXT,
    agent_id TEXT,

    -- Extended content
    reasoning TEXT,
    tool_calls TEXT,  -- JSON array
    intent TEXT,      -- JSON
    subject TEXT,

    -- Ordering within chat
    sequence_num INTEGER NOT NULL,

    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(chat_id, sequence_num)
);

CREATE INDEX IF NOT EXISTS idx_chat_messages_chat ON chat_messages(chat_id, sequence_num);
CREATE INDEX IF NOT EXISTS idx_chat_messages_role ON chat_messages(chat_id, role);
CREATE INDEX IF NOT EXISTS idx_chat_messages_created ON chat_messages(created_at DESC);

--------------------------------------------------------------------------------
-- CHAT USAGE (renamed from app_session_usage)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS chat_usage (
    id TEXT PRIMARY KEY,
    chat_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
    model TEXT NOT NULL,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    reasoning_tokens INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens INTEGER NOT NULL DEFAULT 0,
    cache_write_tokens INTEGER NOT NULL DEFAULT 0,
    estimated_cost_usd REAL NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(chat_id, model)
);

CREATE INDEX IF NOT EXISTS idx_chat_usage_chat ON chat_usage(chat_id);

CREATE TRIGGER IF NOT EXISTS chat_usage_set_updated_at
    AFTER UPDATE ON chat_usage
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE chat_usage SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- PAGES (NO space_id - referenced via space_items)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS pages (
    id TEXT PRIMARY KEY,  -- e.g., 'page_a1b2c3d4e5f6g7h8'
    title TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    icon TEXT,          -- Emoji or icon identifier
    cover_url TEXT,     -- Drive path or external URL for cover image
    tags TEXT,          -- JSON array
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_pages_updated ON pages(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_pages_title ON pages(title);

CREATE TRIGGER IF NOT EXISTS pages_set_updated_at
    AFTER UPDATE ON pages
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE pages SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- SPACES (renamed from workspaces)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS spaces (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    icon TEXT,
    is_system BOOLEAN DEFAULT FALSE,      -- TRUE for "Virtues" (undeletable)
    sort_order INTEGER DEFAULT 0,         -- Order in switcher (0 = leftmost)

    -- Theming
    theme_id TEXT NOT NULL DEFAULT 'tatooine',
    accent_color TEXT,

    -- Tab state
    active_tab_state_json TEXT,

    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TRIGGER IF NOT EXISTS spaces_set_updated_at
    AFTER UPDATE ON spaces
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE spaces SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- NAMESPACES: URL Routing Registry
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS namespaces (
    name TEXT PRIMARY KEY,                -- 'person', 'drive', 'virtues'
    backend TEXT NOT NULL,                -- 'sqlite', 'filesystem', 's3', 'none'
    backend_config TEXT,                  -- JSON
    is_entity BOOLEAN DEFAULT FALSE,      -- TRUE = expects {name}_{id} pattern
    is_system BOOLEAN DEFAULT FALSE,      -- TRUE = cannot be deleted/renamed
    icon TEXT,
    label TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

--------------------------------------------------------------------------------
-- VIEWS: Manual (Playlist) or Smart (Query) Collections
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS views (
    id TEXT PRIMARY KEY,
    space_id TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    parent_view_id TEXT REFERENCES views(id) ON DELETE CASCADE,  -- Shallow nesting (depth=1)
    name TEXT NOT NULL,
    icon TEXT,
    sort_order INTEGER DEFAULT 0,

    -- View type
    view_type TEXT NOT NULL CHECK (view_type IN ('manual', 'smart')),

    -- For smart views: auto-populate query config
    query_config TEXT,  -- JSON

    -- System views cannot be edited/deleted
    is_system BOOLEAN DEFAULT FALSE,

    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_views_space ON views(space_id, sort_order);
CREATE INDEX IF NOT EXISTS idx_views_parent ON views(parent_view_id);

CREATE TRIGGER IF NOT EXISTS views_set_updated_at
    AFTER UPDATE ON views
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE views SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- SPACE_ITEMS: URL references in spaces/views (renamed from view_items)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS space_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    view_id TEXT REFERENCES views(id) ON DELETE CASCADE,
    space_id TEXT REFERENCES spaces(id) ON DELETE CASCADE,
    url TEXT NOT NULL,  -- URL-native: "/chat/chat_abc" or "https://arxiv.org"
    sort_order INTEGER DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    -- Either view_id or space_id must be set, but not both
    CHECK (
        (view_id IS NOT NULL AND space_id IS NULL) OR
        (view_id IS NULL AND space_id IS NOT NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_space_items_view ON space_items(view_id, sort_order);
CREATE INDEX IF NOT EXISTS idx_space_items_space ON space_items(space_id, sort_order);
CREATE INDEX IF NOT EXISTS idx_space_items_url ON space_items(url);
CREATE UNIQUE INDEX IF NOT EXISTS idx_space_items_view_url ON space_items(view_id, url) WHERE view_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_space_items_space_url ON space_items(space_id, url) WHERE space_id IS NOT NULL;

--------------------------------------------------------------------------------
-- SEED: NAMESPACES
--------------------------------------------------------------------------------

-- Entity namespaces (SQLite-backed)
INSERT INTO namespaces (name, backend, backend_config, is_entity, is_system, icon, label) VALUES
    ('chat', 'sqlite', '{"table":"chats"}', TRUE, TRUE, 'ri:chat-1-line', 'Chats'),
    ('page', 'sqlite', '{"table":"pages"}', TRUE, TRUE, 'ri:file-text-line', 'Pages'),
    ('person', 'sqlite', '{"table":"wiki_people"}', TRUE, TRUE, 'ri:user-line', 'People'),
    ('place', 'sqlite', '{"table":"wiki_places"}', TRUE, TRUE, 'ri:map-pin-line', 'Places'),
    ('org', 'sqlite', '{"table":"wiki_orgs"}', TRUE, TRUE, 'ri:building-line', 'Organizations'),
    ('day', 'sqlite', '{"table":"wiki_days"}', TRUE, TRUE, 'ri:calendar-line', 'Days'),
    ('year', 'sqlite', '{"table":"wiki_years"}', TRUE, TRUE, 'ri:calendar-line', 'Years'),
    ('source', 'sqlite', '{"table":"elt_source_connections"}', TRUE, TRUE, 'ri:database-2-line', 'Sources');

-- Storage namespaces (mounted backends)
INSERT INTO namespaces (name, backend, backend_config, is_entity, is_system, icon, label) VALUES
    ('drive', 'filesystem', '{"mount":"/mnt/user/drive"}', FALSE, TRUE, 'ri:hard-drive-2-line', 'Drive'),
    ('lake', 's3', '{"bucket":"user-lake","prefix":"data/"}', FALSE, TRUE, 'ri:cloud-line', 'Data Lake');

-- App namespace (frontend-rendered)
INSERT INTO namespaces (name, backend, backend_config, is_entity, is_system, icon, label) VALUES
    ('virtues', 'none', NULL, FALSE, TRUE, 'ri:compass-3-line', 'Virtues');

--------------------------------------------------------------------------------
-- SEED: SYSTEM SPACE + VIEWS
--------------------------------------------------------------------------------

-- System space: "Virtues"
INSERT OR IGNORE INTO spaces (id, name, is_system, theme_id, sort_order)
VALUES ('space_system', 'Virtues', TRUE, 'pemberley', 0);

-- Default user space: "Home"
INSERT OR IGNORE INTO spaces (id, name, is_system, theme_id, sort_order)
VALUES ('space_home', 'Home', FALSE, 'pemberley', 1);

-- System views (smart views show recent items, manual views have explicit items)
INSERT INTO views (id, space_id, name, icon, sort_order, view_type, query_config, is_system) VALUES
    ('view_sys_chats', 'space_system', 'Chats', NULL, 1000, 'smart', '{"namespace":"chat","limit":5,"static_prefix":["/chat"]}', TRUE),
    ('view_sys_pages', 'space_system', 'Pages', NULL, 1500, 'smart', '{"namespace":"page","limit":5,"static_prefix":["/page"]}', TRUE),
    ('view_sys_wiki', 'space_system', 'Wiki', NULL, 2000, 'manual', NULL, TRUE),
    ('view_sys_data', 'space_system', 'Data', NULL, 3000, 'manual', NULL, TRUE),
    ('view_sys_developer', 'space_system', 'Developer', NULL, 4000, 'manual', NULL, TRUE);

-- System view items (URL-native!)
INSERT INTO space_items (view_id, url, sort_order) VALUES
    -- Wiki folder
    ('view_sys_wiki', '/wiki', 0),
    ('view_sys_wiki', '/day', 1),
    ('view_sys_wiki', '/person', 2),
    ('view_sys_wiki', '/place', 3),
    ('view_sys_wiki', '/org', 4),
    -- Data folder
    ('view_sys_data', '/source', 0),
    ('view_sys_data', '/drive', 1),
    -- Developer folder
    ('view_sys_developer', '/virtues/sql', 0),
    ('view_sys_developer', '/virtues/terminal', 1),
    ('view_sys_developer', '/virtues/sitemap', 2);

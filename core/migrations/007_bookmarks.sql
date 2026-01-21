-- Bookmarks: User-saved references to pages and wiki entities (SQLite)
-- Supports two bookmark types:
--   - 'tab': A saved route/page (e.g., /wiki/people, /data/sources)
--   - 'entity': A saved wiki entity (Person, Place, Organization, Thing)

CREATE TABLE IF NOT EXISTS app_bookmarks (
    id TEXT PRIMARY KEY,

    -- What type of bookmark this is
    bookmark_type TEXT NOT NULL CHECK (bookmark_type IN ('tab', 'entity')),

    -- For tab bookmarks: stores the route and tab metadata
    route TEXT,
    tab_type TEXT,
    label TEXT NOT NULL,
    icon TEXT,

    -- For entity bookmarks: reference to the entity
    entity_type TEXT CHECK (entity_type IS NULL OR entity_type IN ('person', 'place', 'organization', 'thing')),
    entity_id TEXT,
    entity_slug TEXT,

    -- Ordering
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),

    -- Constraints to ensure proper data for each type
    CONSTRAINT bookmark_tab_fields CHECK (
        (bookmark_type = 'tab' AND route IS NOT NULL AND tab_type IS NOT NULL) OR
        bookmark_type != 'tab'
    ),
    CONSTRAINT bookmark_entity_fields CHECK (
        (bookmark_type = 'entity' AND entity_type IS NOT NULL AND entity_id IS NOT NULL AND entity_slug IS NOT NULL) OR
        bookmark_type != 'entity'
    )
);

-- Index for listing bookmarks (sorted by sort_order, then created_at)
CREATE INDEX IF NOT EXISTS idx_bookmarks_order
    ON app_bookmarks(sort_order ASC, created_at DESC);

-- Index for checking if a route is bookmarked
CREATE INDEX IF NOT EXISTS idx_bookmarks_route
    ON app_bookmarks(route)
    WHERE bookmark_type = 'tab';

-- Index for checking if an entity is bookmarked
CREATE INDEX IF NOT EXISTS idx_bookmarks_entity
    ON app_bookmarks(entity_type, entity_id)
    WHERE bookmark_type = 'entity';

-- Updated at trigger
CREATE TRIGGER IF NOT EXISTS app_bookmarks_set_updated_at
    AFTER UPDATE ON app_bookmarks
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_bookmarks SET updated_at = datetime('now') WHERE id = NEW.id;
END;

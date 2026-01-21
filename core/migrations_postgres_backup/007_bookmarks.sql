-- Bookmarks: User-saved references to pages and wiki entities
-- Supports two bookmark types:
--   - 'tab': A saved route/page (e.g., /wiki/people, /data/sources)
--   - 'entity': A saved wiki entity (Person, Place, Organization, Thing)

CREATE TABLE IF NOT EXISTS app.bookmarks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- What type of bookmark this is
    bookmark_type TEXT NOT NULL CHECK (bookmark_type IN ('tab', 'entity')),

    -- For tab bookmarks: stores the route and tab metadata
    route TEXT,
    tab_type TEXT,
    label TEXT NOT NULL,
    icon TEXT,

    -- For entity bookmarks: reference to the entity
    entity_type TEXT CHECK (entity_type IS NULL OR entity_type IN ('person', 'place', 'organization', 'thing')),
    entity_id UUID,
    entity_slug TEXT,

    -- Ordering
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

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
    ON app.bookmarks(sort_order ASC, created_at DESC);

-- Index for checking if a route is bookmarked
CREATE INDEX IF NOT EXISTS idx_bookmarks_route
    ON app.bookmarks(route)
    WHERE bookmark_type = 'tab';

-- Index for checking if an entity is bookmarked
CREATE INDEX IF NOT EXISTS idx_bookmarks_entity
    ON app.bookmarks(entity_type, entity_id)
    WHERE bookmark_type = 'entity';

-- Updated at trigger
DROP TRIGGER IF EXISTS app_bookmarks_set_updated_at ON app.bookmarks;
CREATE TRIGGER app_bookmarks_set_updated_at
    BEFORE UPDATE ON app.bookmarks
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- Wiki Things: catchall entity type for dogs, projects, thoughts, etc.

--------------------------------------------------------------------------------
-- WIKI: THINGS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS wiki_things (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    category TEXT,          -- freeform: "pet", "project", "concept", etc.
    description TEXT,       -- short description / subtitle
    metadata TEXT DEFAULT '{}',  -- JSON
    -- Wiki fields
    content TEXT,
    cover_image TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_wiki_things_name ON wiki_things(name);
CREATE INDEX IF NOT EXISTS idx_wiki_things_category ON wiki_things(category) WHERE category IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS wiki_things_set_updated_at
    AFTER UPDATE ON wiki_things
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE wiki_things SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- wiki_connections and wiki_citations recreate with 'thing' type was here,
-- but both tables are dropped in migration 039 (entity_references).
-- Skip recreation to avoid errors when wiki_connections doesn't exist.

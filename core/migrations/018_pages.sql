-- Pages: User-authored knowledge documents with entity linking
-- Standalone table (not wiki_*) - these are first-class documents
--
-- Pages use the format: ((Display Name))[[prefix_hash]] for entity links
-- The prefix tells us which table to query (person_, place_, org_, file_, page_)

--------------------------------------------------------------------------------
-- PAGES TABLE
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS pages (
    id TEXT PRIMARY KEY,  -- e.g., 'page_a1b2c3d4e5f6g7h8'
    title TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    workspace_id TEXT,  -- Which workspace this page belongs to (for view filtering)
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_pages_updated
    ON pages(updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_pages_title
    ON pages(title);

CREATE TRIGGER IF NOT EXISTS pages_set_updated_at
    AFTER UPDATE ON pages
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE pages SET updated_at = datetime('now') WHERE id = NEW.id;
END;

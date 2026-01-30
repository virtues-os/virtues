-- Yjs state column (source of truth for real-time editing)
-- The content column remains for search/materialized text
ALTER TABLE pages ADD COLUMN yjs_state BLOB;

-- Version history for snapshots
CREATE TABLE IF NOT EXISTS page_versions (
    id TEXT PRIMARY KEY,
    page_id TEXT NOT NULL,
    version_number INTEGER NOT NULL,
    yjs_snapshot BLOB,
    content_preview TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_by TEXT DEFAULT 'user',
    description TEXT,
    FOREIGN KEY (page_id) REFERENCES pages(id) ON DELETE CASCADE,
    UNIQUE(page_id, version_number)
);

CREATE INDEX idx_page_versions_page ON page_versions(page_id, version_number DESC);

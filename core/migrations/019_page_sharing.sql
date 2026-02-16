-- Page sharing: public share tokens for read-only page access
CREATE TABLE IF NOT EXISTS app_page_shares (
    id TEXT PRIMARY KEY,
    page_id TEXT NOT NULL UNIQUE,
    token TEXT UNIQUE NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY(page_id) REFERENCES app_pages(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_page_shares_token ON app_page_shares(token);

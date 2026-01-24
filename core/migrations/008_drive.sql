-- Drive: User file storage and quota tracking (SQLite)
-- Personal cloud storage for user-uploaded files (like Google Drive)
-- 
-- Storage is unified under /home/user/:
--   /home/user/drive/      - User uploads (full CRUD)
--   /home/user/data-lake/  - ELT archives (system-managed, read-only)
--
-- Both count against the same quota allocation per tenant.

--------------------------------------------------------------------------------
-- DRIVE FILES
--------------------------------------------------------------------------------

-- Tracks all files in the user's drive
CREATE TABLE IF NOT EXISTS drive_files (
    id TEXT PRIMARY KEY,

    -- Path within drive (relative to /home/user/drive)
    -- e.g., "documents/report.pdf" or "photos/vacation/beach.jpg"
    path TEXT NOT NULL UNIQUE,

    -- File metadata
    filename TEXT NOT NULL,
    mime_type TEXT,
    size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),

    -- Parent folder (null for root items)
    parent_id TEXT REFERENCES drive_files(id) ON DELETE CASCADE,

    -- Is this a folder?
    is_folder INTEGER NOT NULL DEFAULT 0,

    -- Checksum for integrity verification
    sha256_hash TEXT,

    -- Timestamps
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_drive_files_path
    ON drive_files(path);
CREATE INDEX IF NOT EXISTS idx_drive_files_parent
    ON drive_files(parent_id) WHERE parent_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_drive_files_folder
    ON drive_files(parent_id, is_folder);

CREATE TRIGGER IF NOT EXISTS drive_files_set_updated_at
    AFTER UPDATE ON drive_files
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE drive_files SET updated_at = datetime('now') WHERE id = NEW.id;
END;

--------------------------------------------------------------------------------
-- DRIVE USAGE (singleton)
--------------------------------------------------------------------------------

-- Tracks unified storage usage for quota enforcement
-- Updated on file operations, periodically reconciled with filesystem
CREATE TABLE IF NOT EXISTS drive_usage (
    id TEXT PRIMARY KEY DEFAULT '00000000-0000-0000-0000-000000000001',

    -- Drive usage: user-uploaded files in /home/user/drive/
    drive_bytes INTEGER NOT NULL DEFAULT 0 CHECK (drive_bytes >= 0),

    -- Data lake usage: ELT archives in /home/user/data-lake/
    -- Computed from sum of elt_stream_objects.size_bytes
    data_lake_bytes INTEGER NOT NULL DEFAULT 0 CHECK (data_lake_bytes >= 0),

    -- Legacy column for backwards compatibility (equals drive_bytes)
    total_bytes INTEGER NOT NULL DEFAULT 0 CHECK (total_bytes >= 0),

    -- File counts (drive only)
    file_count INTEGER NOT NULL DEFAULT 0 CHECK (file_count >= 0),
    folder_count INTEGER NOT NULL DEFAULT 0 CHECK (folder_count >= 0),

    -- Quota (set from TIER environment variable on startup)
    -- Default: 100 GB (free tier)
    quota_bytes INTEGER NOT NULL DEFAULT 107374182400,

    -- Warning thresholds reached (to avoid spamming)
    warning_80_sent INTEGER NOT NULL DEFAULT 0,
    warning_90_sent INTEGER NOT NULL DEFAULT 0,
    warning_100_sent INTEGER NOT NULL DEFAULT 0,

    -- Last filesystem scan for reconciliation
    last_scan_at TEXT,
    last_scan_bytes INTEGER,

    -- Audit
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),

    -- Ensure only one row exists
    CONSTRAINT drive_usage_singleton CHECK (id = '00000000-0000-0000-0000-000000000001')
);

CREATE TRIGGER IF NOT EXISTS drive_usage_set_updated_at
    AFTER UPDATE ON drive_usage
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE drive_usage SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- Initialize singleton row with default quota (100GB free tier)
INSERT OR IGNORE INTO drive_usage (id) VALUES ('00000000-0000-0000-0000-000000000001');

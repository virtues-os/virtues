-- Migration: Add soft delete support to drive_files
-- Files are marked as deleted but kept on disk for 30 days before permanent purge

-- Add deleted_at column for soft delete
ALTER TABLE drive_files ADD COLUMN deleted_at TEXT DEFAULT NULL;

-- Index for efficient trash queries
CREATE INDEX idx_drive_files_deleted_at ON drive_files(deleted_at);

-- Add trash_bytes to usage tracking (deleted files still count against quota)
ALTER TABLE drive_usage ADD COLUMN trash_bytes INTEGER DEFAULT 0;
ALTER TABLE drive_usage ADD COLUMN trash_count INTEGER DEFAULT 0;

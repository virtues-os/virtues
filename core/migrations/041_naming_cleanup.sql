-- 041: Naming convention cleanup
--
-- Two renames to align with the prefix convention:
--   dayline_topic_cache → search_topic_cache (embedding cache → search layer)
--   drive_files → app_drive_files            (app-level user content)
--   drive_usage → app_drive_usage            (app-level user content)

-- ─────────────────────────────────────────────────────────────────────────────
-- 1. RENAME dayline_topic_cache → search_topic_cache
-- ─────────────────────────────────────────────────────────────────────────────
-- Simple rename, no FK dependencies.

ALTER TABLE dayline_topic_cache RENAME TO search_topic_cache;

-- ─────────────────────────────────────────────────────────────────────────────
-- 2. RENAME drive_files → app_drive_files, drive_usage → app_drive_usage
-- ─────────────────────────────────────────────────────────────────────────────
-- drive_files has a self-referential FK (parent_id → drive_files), indexes,
-- and a trigger. SQLite's RENAME TABLE automatically updates self-refs and
-- index ownership, but the trigger body text is updated too since SQLite 3.25.

ALTER TABLE drive_files RENAME TO app_drive_files;
ALTER TABLE drive_usage RENAME TO app_drive_usage;

-- Rename indexes to match new table name
DROP INDEX IF EXISTS idx_drive_files_path;
DROP INDEX IF EXISTS idx_drive_files_parent;
DROP INDEX IF EXISTS idx_drive_files_folder;
DROP INDEX IF EXISTS idx_drive_files_deleted_at;

CREATE INDEX idx_app_drive_files_path ON app_drive_files(path);
CREATE INDEX idx_app_drive_files_parent ON app_drive_files(parent_id) WHERE parent_id IS NOT NULL;
CREATE INDEX idx_app_drive_files_folder ON app_drive_files(parent_id, is_folder);
CREATE INDEX idx_app_drive_files_deleted_at ON app_drive_files(deleted_at);

-- Drop old triggers and recreate with new names
DROP TRIGGER IF EXISTS drive_files_set_updated_at;
DROP TRIGGER IF EXISTS drive_usage_set_updated_at;

CREATE TRIGGER app_drive_files_set_updated_at
    AFTER UPDATE ON app_drive_files
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_drive_files SET updated_at = datetime('now') WHERE id = NEW.id;
END;

CREATE TRIGGER app_drive_usage_set_updated_at
    AFTER UPDATE ON app_drive_usage
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_drive_usage SET updated_at = datetime('now') WHERE id = NEW.id;
END;

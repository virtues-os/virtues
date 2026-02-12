-- 017: Schema Cleanup — Phase 2
--
-- Drops dead tables and renames remaining tables for prefix consistency:
--   chats/pages/namespaces → app_* prefix
--   narrative_* → wiki_* prefix (these are wiki entities)

-- =============================================================================
-- PART 1: Drop dead tables
-- =============================================================================

DROP TABLE IF EXISTS elt_transform_checkpoints;
DROP TABLE IF EXISTS app_llm_usage;
DROP TABLE IF EXISTS app_llm_requests;

-- =============================================================================
-- PART 2: Rename chat tables → app_chat_*
-- =============================================================================

-- Drop old triggers first
DROP TRIGGER IF EXISTS chats_set_updated_at;
DROP TRIGGER IF EXISTS chat_messages_set_updated_at;

ALTER TABLE chats RENAME TO app_chats;
ALTER TABLE chat_messages RENAME TO app_chat_messages;
ALTER TABLE chat_usage RENAME TO app_chat_usage;
ALTER TABLE chat_edit_permissions RENAME TO app_chat_edit_permissions;

-- Recreate triggers with new names
CREATE TRIGGER IF NOT EXISTS app_chats_set_updated_at
    AFTER UPDATE ON app_chats
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_chats SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- Note: chat_messages has no updated_at column, so no trigger needed

-- =============================================================================
-- PART 3: Rename pages tables → app_pages_*
-- =============================================================================

DROP TRIGGER IF EXISTS pages_set_updated_at;

ALTER TABLE pages RENAME TO app_pages;
ALTER TABLE page_versions RENAME TO app_page_versions;

CREATE TRIGGER IF NOT EXISTS app_pages_set_updated_at
    AFTER UPDATE ON app_pages
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_pages SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- =============================================================================
-- PART 4: Rename namespaces → app_namespaces
-- =============================================================================

ALTER TABLE namespaces RENAME TO app_namespaces;

-- =============================================================================
-- PART 5: Rename narrative_* → wiki_*
-- =============================================================================

DROP TRIGGER IF EXISTS narrative_telos_set_updated_at;
DROP TRIGGER IF EXISTS narrative_acts_set_updated_at;
DROP TRIGGER IF EXISTS narrative_chapters_set_updated_at;

ALTER TABLE narrative_telos RENAME TO wiki_telos;
ALTER TABLE narrative_acts RENAME TO wiki_acts;
ALTER TABLE narrative_chapters RENAME TO wiki_chapters;

CREATE TRIGGER IF NOT EXISTS wiki_telos_set_updated_at
    AFTER UPDATE ON wiki_telos
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE wiki_telos SET updated_at = datetime('now') WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS wiki_acts_set_updated_at
    AFTER UPDATE ON wiki_acts
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE wiki_acts SET updated_at = datetime('now') WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS wiki_chapters_set_updated_at
    AFTER UPDATE ON wiki_chapters
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE wiki_chapters SET updated_at = datetime('now') WHERE id = NEW.id;
END;

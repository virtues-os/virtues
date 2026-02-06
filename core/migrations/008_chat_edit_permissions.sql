-- Chat Edit Permissions: Tracks which entities AI is allowed to edit per chat
-- Permission is granted inline in chat when AI requests to edit something
-- Cleared when chat is deleted (via CASCADE)

--------------------------------------------------------------------------------
-- CHAT_EDIT_PERMISSIONS
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS chat_edit_permissions (
    id TEXT PRIMARY KEY,
    chat_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
    entity_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,  -- 'page', 'person', 'place', 'org', etc.
    entity_title TEXT,
    granted_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(chat_id, entity_id)
);

CREATE INDEX IF NOT EXISTS idx_chat_edit_permissions_chat ON chat_edit_permissions(chat_id);
CREATE INDEX IF NOT EXISTS idx_chat_edit_permissions_entity ON chat_edit_permissions(entity_id);

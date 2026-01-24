-- Migration: 011_chat_messages
-- Purpose: Normalize chat messages from JSON blob to dedicated table
-- This fixes race conditions, enables proper indexing, and provides stable message IDs

--------------------------------------------------------------------------------
-- CHAT MESSAGES (normalized from app_chat_sessions.messages JSON blob)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS app_chat_messages (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES app_chat_sessions(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system')),
    content TEXT NOT NULL,
    
    -- Model/provider info (for assistant messages)
    model TEXT,
    provider TEXT,
    agent_id TEXT,
    
    -- Extended content
    reasoning TEXT,
    tool_calls TEXT,  -- JSON array of tool calls, null if none
    intent TEXT,      -- JSON intent metadata, null if none
    subject TEXT,
    
    -- Ordering within session
    sequence_num INTEGER NOT NULL,
    
    -- Timestamp
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    
    -- Ensure unique ordering per session
    UNIQUE(session_id, sequence_num)
);

-- Index for fetching messages by session (primary access pattern)
CREATE INDEX IF NOT EXISTS idx_chat_messages_session 
    ON app_chat_messages(session_id, sequence_num);

-- Index for filtering by role within a session (useful for compaction)
CREATE INDEX IF NOT EXISTS idx_chat_messages_role 
    ON app_chat_messages(session_id, role);

-- Index for finding recent messages across all sessions
CREATE INDEX IF NOT EXISTS idx_chat_messages_created 
    ON app_chat_messages(created_at DESC);

--------------------------------------------------------------------------------
-- DATA MIGRATION: Move existing messages from JSON blob to normalized table
-- Note: This runs once on upgrade and handles existing data
--------------------------------------------------------------------------------

-- SQLite doesn't have procedural loops, so data migration will be handled
-- by the application on startup. The migration script just creates the table.
-- See sessions.rs::migrate_json_messages() for the Rust implementation.

--------------------------------------------------------------------------------
-- DEPRECATION NOTICE
--------------------------------------------------------------------------------
-- The app_chat_sessions.messages column is now deprecated.
-- It will be kept for rollback safety but should not be written to.
-- A future migration will DROP this column after confirming data integrity.

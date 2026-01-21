-- Migration: 010_context_management
-- Purpose: Add session token usage tracking and conversation summary support for context management

-- Per-session token usage tracking
-- Tracks cumulative token usage per session, grouped by model
CREATE TABLE IF NOT EXISTS app_session_usage (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    model TEXT NOT NULL,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    reasoning_tokens INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens INTEGER NOT NULL DEFAULT 0,
    cache_write_tokens INTEGER NOT NULL DEFAULT 0,
    estimated_cost_usd REAL NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(session_id, model),
    FOREIGN KEY (session_id) REFERENCES app_chat_sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_session_usage_session ON app_session_usage(session_id);

-- Add conversation summary fields to existing sessions table
-- These support hierarchical summarization for context compaction
ALTER TABLE app_chat_sessions ADD COLUMN conversation_summary TEXT;
ALTER TABLE app_chat_sessions ADD COLUMN summary_up_to_index INTEGER DEFAULT 0;
ALTER TABLE app_chat_sessions ADD COLUMN summary_version INTEGER DEFAULT 0;
ALTER TABLE app_chat_sessions ADD COLUMN last_compacted_at TEXT;

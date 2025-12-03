-- Migration: Rename ariata to virtues
-- This migration updates all references from "ariata" to "virtues"

-- Update the tool_type CHECK constraint in app.tools
-- First, drop the existing constraint and add new one
ALTER TABLE app.tools DROP CONSTRAINT IF EXISTS tools_tool_type_check;
ALTER TABLE app.tools ADD CONSTRAINT tools_tool_type_check
    CHECK (tool_type IN ('mcp', 'virtues'));

-- Update existing rows that have tool_type = 'ariata' to 'virtues'
UPDATE app.tools SET tool_type = 'virtues' WHERE tool_type = 'ariata';

-- Update default values in knowledge_ai_conversation
-- Note: ALTER COLUMN ... SET DEFAULT doesn't affect existing rows
ALTER TABLE data.knowledge_ai_conversation
    ALTER COLUMN source_table SET DEFAULT 'stream_virtues_ai_chat';
ALTER TABLE data.knowledge_ai_conversation
    ALTER COLUMN source_provider SET DEFAULT 'virtues';

-- Update existing rows with ariata references
UPDATE data.knowledge_ai_conversation
    SET source_table = 'stream_virtues_ai_chat'
    WHERE source_table = 'stream_ariata_ai_chat';
UPDATE data.knowledge_ai_conversation
    SET source_provider = 'virtues'
    WHERE source_provider = 'ariata';

-- Rename the stream table if it exists
-- (The stream table is created dynamically, so we use DO block for conditional rename)
DO $$
BEGIN
    IF EXISTS (SELECT FROM information_schema.tables
               WHERE table_schema = 'data'
               AND table_name = 'stream_ariata_ai_chat') THEN
        ALTER TABLE data.stream_ariata_ai_chat RENAME TO stream_virtues_ai_chat;
    END IF;
END $$;

-- Note: transform_checkpoints table was removed in codebase refactor
-- No checkpoint migration needed for fresh installs

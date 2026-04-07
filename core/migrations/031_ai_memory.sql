-- Add persistent AI memory column to assistant profile
-- The AI reads this every conversation and writes to it via update_memory tool.
-- Like Claude Code's MEMORY.md — global, plain text, AI-written, user-visible.

ALTER TABLE app_assistant_profile ADD COLUMN memory TEXT DEFAULT NULL;

-- Rename agent modes: agent→standard, research→deep-search, chat→conversation
-- Note: agent_mode is set per-request (not persisted in DB), so no data migration needed.
-- This migration is a no-op marker for the rename.
SELECT 1;

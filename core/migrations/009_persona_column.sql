-- Add persona column to assistant profile for AI personality customization
-- Archetypes: standard (default), concierge, analyst, coach
-- No CHECK constraint - users can create custom personas stored in `personas` JSON column

ALTER TABLE app_assistant_profile ADD COLUMN persona TEXT DEFAULT 'standard';

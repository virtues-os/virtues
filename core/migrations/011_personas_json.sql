-- Add personas JSON column to store persona definitions
-- Structure: { "items": [...], "hidden": [...] }
--
-- The existing `persona` column (from 009) stores the SELECTED persona ID.
-- This new `personas` column stores the full list of persona definitions.
--
-- On first API access, system personas are seeded from virtues-registry
-- and stored here. Users can then customize or add their own.

ALTER TABLE app_assistant_profile ADD COLUMN personas TEXT DEFAULT NULL;
